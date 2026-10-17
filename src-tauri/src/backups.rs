//! Pre-migration database backups: an inventory and a NON-DESTRUCTIVE cleanup preview.
//!
//! The application writes a consistent snapshot (`atm-v<schema>-<UTC stamp>.sqlite`) into its own `backups` folder
//! before every schema migration, so a failed or unwanted upgrade can be rolled back. Such a snapshot is a verbatim
//! copy: whatever the database held at that moment, it still holds.
//!
//! # What the scan can and cannot say
//!
//! The scan is a set of **pattern checks**, reported with its coverage. A count of zero means *"no violation was
//! detected in the examined content"* — never *"this backup holds no personal data"*. Examined (see `EXAMINED`):
//! the privacy-gate fields of `discovery_observations`; every TEXT value of every table, for private-path and
//! secret/credential/e-mail shapes; and the raw bytes of the file (and its `-wal` sidecar) for private-path shapes,
//! which is what can reveal text left in free pages. Not examined (see `NOT_EXAMINED`): personal text that matches no
//! pattern (names, notes), other copies (file-system snapshots, shadow copies, trash, cloud sync) and content in other
//! encodings. Backups written before schema 5 are treated as **potentially sensitive** whatever the scan finds,
//! because they were written before any privacy rule existed. Unreadable, malformed or unrecognised files are
//! UNKNOWN, never "clean".
//!
//! Nothing in this module deletes, moves, rewrites or uploads a file; backups are opened read-only. Deleting one needs
//! a separate, explicit user authorization that does not exist in this phase.

use crate::{
    audit,
    db::{self, LATEST_VERSION},
    discovery::privacy,
    error::AppResult,
    redact,
};
use regex::Regex;
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use serde::Serialize;
use std::{path::Path, sync::OnceLock};

/// What was looked at (shown to the user verbatim).
pub const EXAMINED: &[&str] = &[
    "discovery_observations tablosunun gizlilik kapısı alanları: evidence, limitations_json, version, version_source, location, project_alias",
    "Tüm tablolardaki tüm metin değerleri: özel klasör yolu, gizli anahtar/parola/e-posta biçimleri (örüntü taraması)",
    "Dosyanın ham baytları (ve varsa -wal yan dosyası): özel klasör yolu biçimleri (örüntü taraması); kullanılmayan sayfalardaki kalıntıları yakalayabilir",
];

/// What was NOT looked at (shown to the user verbatim).
pub const NOT_EXAMINED: &[&str] = &[
    "Hiçbir örüntüye uymayan kişisel metin (ad, not, serbest metin) taranamaz",
    "Yedek dışındaki kopyalar: dosya sistemi anlık görüntüleri, gölge kopyalar, çöp kutusu, bulut eşitleme",
    "Başka kodlamalardaki (örn. UTF-16) veya sıkıştırılmış içerik",
    "Bu bir örüntü taramasıdır; eksiksizlik iddia etmez",
];

/// `violations_observed` | `potentially_sensitive` | `none_detected_in_examined_content` | `not_assessable`
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    /// Only `atm-v<N>-<stamp>.sqlite`: no directory, no account name.
    pub file_name: String,
    pub schema_version: Option<i64>,
    pub size_bytes: u64,
    pub modified: String,
    pub privacy_status: String,
    /// Why that status (a fixed code the UI explains).
    pub reason: String,
    /// `discovery_observations` rows failing the privacy gate; None = the table is absent or unreadable.
    pub observation_rows_failing_gate: Option<i64>,
    /// Text values in ANY table (including discovery) with a private-path or secret shape; counts only.
    pub other_text_findings: i64,
    /// Names of tables (schema identifiers, not content) in which such values were found.
    pub tables_with_findings: Vec<String>,
    /// Private-path-shaped byte sequences in the raw file; can include text no row refers to any more (free pages).
    pub raw_path_like_matches: i64,
    pub has_wal_sidecar: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Precondition {
    pub id: String,
    pub ok: bool,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPlan {
    pub entries: Vec<BackupEntry>,
    pub preconditions: Vec<Precondition>,
    pub examined: Vec<String>,
    pub not_examined: Vec<String>,
    /// Backups a person should REVIEW before deciding anything (violations observed, or written before schema 5).
    pub review_candidates: Vec<String>,
    /// No violation detected in the examined content. This is NOT "personal-data-free" and NOT "safe to delete".
    pub no_violation_detected: Vec<String>,
    pub not_assessable: Vec<String>,
    /// Files in the folder that do not follow the backup naming scheme: never opened, never named.
    pub unrecognised_files: usize,
    /// Always false in this phase: the preview is the whole feature.
    pub deletion_available: bool,
    pub executed: bool,
    pub notes: Vec<String>,
}

fn parse_name(name: &str) -> Option<i64> {
    let rest = name.strip_prefix("atm-v")?.strip_suffix(".sqlite")?;
    let (version, stamp) = rest.split_once('-')?;
    let ok_stamp = stamp.len() == 16
        && stamp.ends_with('Z')
        && stamp.as_bytes()[8] == b'T'
        && stamp[..8].bytes().all(|b| b.is_ascii_digit())
        && stamp[9..15].bytes().all(|b| b.is_ascii_digit());
    if !ok_stamp {
        return None;
    }
    version.parse::<i64>().ok().filter(|v| (1..=1000).contains(v))
}

/// Private-path shapes in raw bytes: a drive path, a UNC path, a macOS/Linux home path.
fn raw_path_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)(?:\b[a-z]:[\\/]|\\\\[a-z0-9._$-]+\\|/(?:users|home)/[a-z0-9._-])").expect("valid pattern"))
}

fn text_value_is_sensitive(text: &str) -> bool {
    redact::has_sensitive_shape(text) || raw_path_regex().is_match(text)
}

/// Counts `discovery_observations` rows the privacy gate would reject. `Ok(None)` = the table does not exist.
pub fn count_failing_rows(conn: &Connection) -> rusqlite::Result<Option<i64>> {
    // A file that is not a database fails here (so it is reported as unknown, never as clean).
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))?;
    if conn.prepare("SELECT 1 FROM discovery_observations LIMIT 0").is_err() {
        return Ok(None);
    }
    let mut stmt =
        conn.prepare("SELECT evidence, limitations_json, version, version_source, location, project_alias FROM discovery_observations")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
        ))
    })?;
    let mut bad = 0;
    for row in rows {
        let (evidence, limitations, version, source, location, alias) = row?;
        let limitation_list: Vec<String> = serde_json::from_str(&limitations).unwrap_or_else(|_| vec!["\u{1}".to_string()]);
        if !privacy::text_is_clean(&evidence)
            || limitation_list.iter().any(|l| !privacy::text_is_clean(l))
            || version.as_deref().map(|v| !privacy::is_documented_version(v)).unwrap_or(false)
            || source.as_deref().map(|s| !privacy::VERSION_SOURCES.contains(&s)).unwrap_or(false)
            || location.as_deref().map(|l| !privacy::is_safe_location(l)).unwrap_or(false)
            || alias.as_deref().map(|a| !privacy::is_safe_project_alias(a)).unwrap_or(false)
        {
            bad += 1;
        }
    }
    Ok(Some(bad))
}

/// Pattern-checks every TEXT value of every table. Returns (findings, names of tables with findings).
fn scan_all_text(conn: &Connection) -> rusqlite::Result<(i64, Vec<String>)> {
    let tables: Vec<String> = {
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let (mut total, mut names, mut budget) = (0i64, Vec::new(), 500_000u32);
    for table in tables {
        // Schema identifiers come from sqlite_master; quote them so odd names cannot alter the statement.
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))?;
        let n = stmt.column_count();
        let mut rows = stmt.query([])?;
        let mut hit = false;
        while let Some(row) = rows.next()? {
            for i in 0..n {
                if budget == 0 {
                    return Ok((total, names));
                }
                budget -= 1;
                if let ValueRef::Text(bytes) = row.get_ref(i)? {
                    if text_value_is_sensitive(&String::from_utf8_lossy(bytes)) {
                        total += 1;
                        hit = true;
                    }
                }
            }
        }
        if hit {
            names.push(table);
        }
    }
    Ok((total, names))
}

fn raw_matches(path: &Path) -> (i64, bool) {
    let mut count = 0i64;
    let mut wal = false;
    for (p, is_wal) in [
        (path.to_path_buf(), false),
        (path.with_file_name(format!("{}-wal", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default())), true),
    ] {
        let Ok(meta) = std::fs::metadata(&p) else { continue };
        if is_wal {
            wal = true;
        }
        if meta.len() > 256 * 1024 * 1024 {
            continue; // too large to scan here; the plan stays conservative because the entry is then not "clean"
        }
        if let Ok(bytes) = std::fs::read(&p) {
            count += raw_path_regex().find_iter(&String::from_utf8_lossy(&bytes)).count() as i64;
        }
    }
    (count, wal)
}

fn inspect(path: &Path, name: &str, schema: Option<i64>) -> BackupEntry {
    let meta = std::fs::metadata(path).ok();
    let modified = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let mut entry = BackupEntry {
        file_name: name.to_string(),
        schema_version: schema,
        size_bytes: size,
        modified,
        privacy_status: "not_assessable".to_string(),
        reason: "unreadable".to_string(),
        observation_rows_failing_gate: None,
        other_text_findings: 0,
        tables_with_findings: Vec::new(),
        raw_path_like_matches: 0,
        has_wal_sidecar: false,
    };
    let (raw, wal) = raw_matches(path);
    entry.raw_path_like_matches = raw;
    entry.has_wal_sidecar = wal;
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok();
    let judged = conn.as_ref().and_then(|c| count_failing_rows(c).ok().and_then(|rows| scan_all_text(c).ok().map(|t| (rows, t))));
    let Some((rows, (findings, tables))) = judged else {
        // Could not be read as a database. Raw matches (if any) are still reported, but the file is never called clean.
        entry.reason = if raw > 0 { "unreadable_with_raw_findings".into() } else { "unreadable".into() };
        return entry;
    };
    entry.observation_rows_failing_gate = rows;
    entry.other_text_findings = findings;
    entry.tables_with_findings = tables;
    let observed = rows.unwrap_or(0) > 0 || findings > 0 || raw > 0;
    let legacy = schema.map(|v| v < 5).unwrap_or(true);
    let (status, reason) = match (observed, legacy) {
        (true, _) => ("violations_observed", "findings_in_examined_content"),
        (false, true) => ("potentially_sensitive", "written_before_privacy_rules_schema_below_5"),
        (false, false) => ("none_detected_in_examined_content", "no_pattern_found_coverage_is_partial"),
    };
    entry.privacy_status = status.to_string();
    entry.reason = reason.to_string();
    entry
}

/// Lists recognised backup files, oldest first, and how many other files were left alone.
pub fn inventory(backup_dir: &Path) -> (Vec<BackupEntry>, usize) {
    let Ok(read) = std::fs::read_dir(backup_dir) else { return (Vec::new(), 0) };
    let mut entries = Vec::new();
    let mut unrecognised = 0;
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        let is_file = item.file_type().map(|t| t.is_file()).unwrap_or(false);
        match (parse_name(&name), is_file) {
            (Some(schema), true) => entries.push(inspect(&item.path(), &name, Some(schema))),
            // SQLite sidecars of a recognised backup are not "foreign" files.
            _ if name.ends_with("-wal") || name.ends_with("-shm") || name.ends_with("-journal") => {}
            _ => unrecognised += 1,
        }
    }
    entries.sort_by(|a, b| a.modified.cmp(&b.modified).then(a.file_name.cmp(&b.file_name)));
    (entries, unrecognised)
}

fn precondition(id: &str, ok: bool, text: &str) -> Precondition {
    Precondition { id: id.to_string(), ok, text: text.to_string() }
}

/// A read-only plan. It never touches a backup file.
pub fn cleanup_plan(conn: &Connection, backup_dir: &Path) -> AppResult<CleanupPlan> {
    let (entries, unrecognised_files) = inventory(backup_dir);
    let schema_ok = db::user_version(conn)? == LATEST_VERSION;
    let integrity_ok = conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)).map(|s| s == "ok").unwrap_or(false);
    let audit_ok = audit::verify(conn).map(|r| r.ok).unwrap_or(false);
    let live_clean = count_failing_rows(conn).map(|n| n.unwrap_or(0) == 0).unwrap_or(false);
    let preconditions = vec![
        precondition("schema-current", schema_ok, "Veritabanı en son şema sürümünde."),
        precondition("integrity", integrity_ok, "SQLite bütünlük denetimi (integrity_check) başarılı."),
        precondition("audit-chain", audit_ok, "Denetim kaydı zinciri doğrulanıyor."),
        precondition("live-data-clean", live_clean, "Güncel veritabanındaki gözlemler gizlilik kurallarına uyuyor."),
    ];
    let names = |status: &[&str]| {
        entries.iter().filter(|e| status.contains(&e.privacy_status.as_str())).map(|e| e.file_name.clone()).collect::<Vec<_>>()
    };
    let review_candidates = names(&["violations_observed", "potentially_sensitive"]);
    let no_violation_detected = names(&["none_detected_in_examined_content"]);
    let not_assessable = names(&["not_assessable"]);
    let mut notes = vec![
        "Bu bir ÖNİZLEMEDİR: hiçbir yedek silinmedi, taşınmadı veya değiştirilmedi. Silme bu sürümde uygulanmamıştır; ayrı ve açık bir yetki gerekir.".to_string(),
        "\"İhlal bulunmadı\", yedeğin kişisel veri içermediği veya silinmesinin güvenli olduğu anlamına GELMEZ: yalnızca yukarıda sayılan alanlarda ve örüntülerde bir şey bulunamadı.".to_string(),
        "Şema 5'ten eski yedekler, gizlilik kuralları yokken yazıldığı için tarama sonucundan bağımsız olarak \"eski kişisel bilgi taşıyabilir\" sayılır.".to_string(),
        "Bir yedeği silmek, o sürüme geri dönme imkânını kalıcı olarak ortadan kaldırır. Geri dönmeniz gerekmeyecekse silmeyi düşünebilirsiniz.".to_string(),
        "Yedekler yalnızca bu uygulamanın kendi veri klasöründedir; GitHub'a veya herhangi bir yere gönderilmez.".to_string(),
        "Dosyanın silinmesi, içeriğin fiziksel olarak yok edildiğini garanti etmez: SSD'lerde aşınma dengeleme, işletim sistemi anlık görüntüleri ve çöp kutusu eski baytları tutabilir.".to_string(),
    ];
    if unrecognised_files > 0 {
        notes.push(format!(
            "{unrecognised_files} dosya yedek adlandırmasına uymadığı için açılmadı ve incelenmedi (BİLİNMİYOR); adı da gösterilmez."
        ));
    }
    if !not_assessable.is_empty() {
        notes.push("Açılamayan veya bozuk yedeklerin içeriği hakkında bir şey söylenemez (BİLİNMİYOR).".to_string());
    }
    if entries.iter().any(|e| e.has_wal_sidecar) {
        notes.push("Bazı yedeklerin -wal yan dosyası var; ham tarama onu da kapsadı.".to_string());
    }
    if review_candidates.is_empty() && not_assessable.is_empty() && unrecognised_files == 0 {
        notes.push("İncelenen içerikte eski kişisel bilgi belirtisi bulunmadı (kapsam sınırlıdır).".to_string());
    }
    Ok(CleanupPlan {
        entries,
        preconditions,
        examined: EXAMINED.iter().map(|s| s.to_string()).collect(),
        not_examined: NOT_EXAMINED.iter().map(|s| s.to_string()).collect(),
        review_candidates,
        no_violation_detected,
        not_assessable,
        unrecognised_files,
        deletion_available: false,
        executed: false,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/phase2a-v4-synthetic.sqlite.fixture");
    const OTHER: &[u8] = b"OtherUser";

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    /// Opens a copy of the synthetic v4 database through the real start-up path (which writes the backup and migrates).
    fn migrated() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("atm.sqlite");
        fs::copy(FIXTURE, &live).unwrap();
        let backups = dir.path().join("backups");
        drop(db::open_file(&live, Some(&backups)).unwrap());
        (dir, live, backups)
    }

    /// A synthetic backup file named like a real one, with caller-supplied SQL.
    fn backup(dir: &Path, schema: i64, stamp: &str, sql: &str) -> std::path::PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("atm-v{schema}-{stamp}.sqlite"));
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(sql).unwrap();
        path
    }

    fn entry_of(dir: &Path, name_part: &str) -> BackupEntry {
        inventory(dir).0.into_iter().find(|e| e.file_name.contains(name_part)).expect("entry")
    }

    fn live_db(dir: &Path) -> Connection {
        let conn = db::open_file(&dir.join("live.sqlite"), None).unwrap();
        crate::catalog::seed_all(&conn).unwrap();
        conn
    }

    // ---- the logical scrub and the raw bytes, judged separately --------------------------------------------------

    #[test]
    fn the_logical_scrub_and_the_raw_bytes_are_judged_separately_live_file_versus_backup() {
        let (_dir, live, backups) = migrated();
        let conn = Connection::open(&live).unwrap();
        assert_eq!(count_failing_rows(&conn).unwrap(), Some(0));
        drop(conn);
        let mut raw = fs::read(&live).unwrap();
        if let Ok(wal) = fs::read(live.with_extension("sqlite-wal")) {
            raw.extend(wal);
        }
        assert!(!contains(&raw, OTHER), "live database file: old personal text is gone from the raw bytes");
        let backup = fs::read_dir(&backups).unwrap().next().unwrap().unwrap().path();
        assert!(contains(&fs::read(&backup).unwrap(), OTHER), "the rollback copy intentionally keeps the old content");
    }

    #[test]
    fn the_compaction_mechanism_itself_removes_deleted_text_from_the_raw_file_and_the_control_shows_why_it_is_needed() {
        let secret = "SYNTHETIC-PRIVATE-FOLDER-NAME-0123456789".repeat(8);
        let run = |compact_it: bool| -> bool {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("t.sqlite");
            let conn = Connection::open(&path).unwrap();
            conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(())).unwrap();
            if compact_it {
                conn.pragma_update(None, "secure_delete", true).unwrap();
            }
            conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);").unwrap();
            conn.execute("INSERT INTO t (v) VALUES (?1)", [&secret]).unwrap();
            conn.execute_batch("DELETE FROM t;").unwrap();
            if compact_it {
                db::compact(&conn);
            } else {
                conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())).unwrap();
            }
            drop(conn);
            let mut raw = fs::read(&path).unwrap();
            if let Ok(wal) = fs::read(path.with_extension("sqlite-wal")) {
                raw.extend(wal);
            }
            contains(&raw, b"SYNTHETIC-PRIVATE-FOLDER-NAME")
        };
        assert!(run(false), "control: a plain DELETE leaves the old text in the file's free pages");
        assert!(!run(true), "secure_delete + checkpoint + VACUUM removes it from this file");
    }

    // ---- RT-2B-02: coverage-honest classification ------------------------------------------------------------------

    #[test]
    fn an_old_schema_backup_without_a_discovery_table_but_with_private_settings_is_never_called_clean() {
        let dir = tempfile::tempdir().unwrap();
        let backups = dir.path().join("backups");
        // schema 2, NO discovery table, private synthetic text in `settings`
        backup(
            &backups,
            2,
            "20261001T000000Z",
            r"CREATE TABLE settings (k TEXT, v TEXT); INSERT INTO settings VALUES ('note', 'my work is in C:\Clients\AcmeCorp\secret-project');",
        );
        let e = entry_of(&backups, "20261001");
        assert_eq!(e.privacy_status, "violations_observed");
        assert_eq!(e.observation_rows_failing_gate, None, "the discovery table does not exist: that is 'not examined', not 'zero'");
        assert_eq!(e.tables_with_findings, vec!["settings".to_string()]);
        assert!(e.other_text_findings >= 1);
        let live = live_db(dir.path());
        let plan = cleanup_plan(&live, &backups).unwrap();
        assert_eq!(plan.review_candidates, vec![e.file_name.clone()]);
        assert!(plan.no_violation_detected.is_empty());
    }

    #[test]
    fn an_old_schema_backup_with_nothing_detectable_is_potentially_sensitive_not_clean() {
        let dir = tempfile::tempdir().unwrap();
        let backups = dir.path().join("backups");
        backup(&backups, 2, "20261001T000000Z", "CREATE TABLE settings (k TEXT, v TEXT); INSERT INTO settings VALUES ('theme', 'dark');");
        backup(&backups, 4, "20261002T000000Z", "CREATE TABLE misc (x TEXT); INSERT INTO misc VALUES ('plain words');");
        for part in ["20261001", "20261002"] {
            let e = entry_of(&backups, part);
            assert_eq!(e.privacy_status, "potentially_sensitive", "{part}");
            assert_eq!(e.reason, "written_before_privacy_rules_schema_below_5");
            assert_eq!((e.other_text_findings, e.raw_path_like_matches), (0, 0));
        }
        let live = live_db(dir.path());
        let plan = cleanup_plan(&live, &backups).unwrap();
        assert_eq!(plan.review_candidates.len(), 2);
        assert!(plan.no_violation_detected.is_empty(), "no old-schema backup is ever listed as having no violation");
    }

    #[test]
    fn clean_discovery_rows_do_not_hide_sensitive_text_in_another_table() {
        let dir = tempfile::tempdir().unwrap();
        let backups = dir.path().join("backups");
        // schema 5 (written under the privacy rules), a clean discovery table, a secret in `diagnostics`
        backup(
            &backups,
            5,
            "20261003T000000Z",
            "CREATE TABLE discovery_observations (evidence TEXT, limitations_json TEXT, version TEXT, version_source TEXT, location TEXT, project_alias TEXT);
             INSERT INTO discovery_observations VALUES ('Dosya bulundu.', '[]', NULL, NULL, '~/.local/bin/claude.exe', NULL);
             CREATE TABLE diagnostics (detail TEXT);
             INSERT INTO diagnostics VALUES ('failed with token=ghp_SYNTHETICSYNTHETICSYNTHETICSYNTH12');",
        );
        let e = entry_of(&backups, "20261003");
        assert_eq!(e.observation_rows_failing_gate, Some(0), "the examined discovery fields are clean");
        assert_eq!(e.privacy_status, "violations_observed", "a clean discovery table must not outweigh a finding elsewhere");
        assert_eq!(e.tables_with_findings, vec!["diagnostics".to_string()]);
    }

    #[test]
    fn sensitive_text_in_free_pages_is_caught_by_the_raw_scan_although_no_row_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        let backups = dir.path().join("backups");
        fs::create_dir_all(&backups).unwrap();
        let path = backups.join("atm-v5-20261004T000000Z.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE notes (id INTEGER PRIMARY KEY, v TEXT);").unwrap();
        conn.execute("INSERT INTO notes (v) VALUES (?1)", [r"C:\Users\OtherUser\Clients\AcmeCorp\plans.txt".repeat(20)]).unwrap();
        conn.execute_batch("DELETE FROM notes;").unwrap();
        drop(conn);
        let e = entry_of(&backups, "20261004");
        assert_eq!(e.other_text_findings, 0, "no active row holds it any more");
        assert!(e.raw_path_like_matches > 0, "but the bytes are still in the file's free pages");
        assert_eq!(e.privacy_status, "violations_observed");
    }

    #[test]
    fn unreadable_malformed_and_foreign_files_are_unknown_never_clean() {
        let dir = tempfile::tempdir().unwrap();
        let backups = dir.path().join("backups");
        fs::create_dir_all(&backups).unwrap();
        fs::write(backups.join("atm-v5-20261005T000000Z.sqlite"), b"not a database at all").unwrap();
        fs::write(backups.join("atm-v5-20261006T000000Z.sqlite"), b"SQLite format 3\0 but truncated").unwrap();
        fs::write(backups.join("notes-from-someone.txt"), b"x").unwrap();
        fs::write(backups.join("atm-v4-bad-stamp.sqlite"), b"x").unwrap();
        for part in ["20261005", "20261006"] {
            let e = entry_of(&backups, part);
            assert_eq!(e.privacy_status, "not_assessable", "{part}");
            assert_eq!(e.observation_rows_failing_gate, None);
        }
        let live = live_db(dir.path());
        let plan = cleanup_plan(&live, &backups).unwrap();
        assert_eq!(plan.unrecognised_files, 2);
        assert_eq!(plan.not_assessable.len(), 2);
        assert!(plan.review_candidates.is_empty() && plan.no_violation_detected.is_empty());
        let json = serde_json::to_string(&plan).unwrap();
        assert!(!json.contains("notes-from-someone") && !json.contains("bad-stamp"), "foreign files are counted, never named");
        assert!(plan.notes.iter().any(|n| n.contains("BİLİNMİYOR")));
    }

    #[test]
    fn a_legitimate_current_schema_backup_reports_no_violation_detected_with_its_coverage_and_no_safety_claim() {
        let dir = tempfile::tempdir().unwrap();
        let backups = dir.path().join("backups");
        // A real, current-schema database copied the way the application does it.
        let src = dir.path().join("src.sqlite");
        {
            let conn = db::open_file(&src, None).unwrap();
            crate::catalog::seed_all(&conn).unwrap();
            fs::create_dir_all(&backups).unwrap();
            conn.execute("VACUUM INTO ?1", [backups.join("atm-v5-20261007T000000Z.sqlite").to_string_lossy().as_ref()]).unwrap();
        }
        let e = entry_of(&backups, "20261007");
        assert_eq!(e.privacy_status, "none_detected_in_examined_content", "{e:?}");
        assert_eq!(e.observation_rows_failing_gate, Some(0));
        let live = live_db(dir.path());
        let plan = cleanup_plan(&live, &backups).unwrap();
        assert_eq!(plan.no_violation_detected, vec![e.file_name.clone()]);
        assert!(plan.review_candidates.is_empty());
        // The preview states what was and was not looked at, and refuses to equate "nothing found" with "safe".
        assert!(plan.examined.len() >= 3 && plan.not_examined.len() >= 3);
        assert!(plan.not_examined.iter().any(|n| n.contains("anlık görüntüler")));
        assert!(plan.notes.iter().any(|n| n.contains("GELMEZ") && n.contains("güvenli")));
        let json = serde_json::to_string(&plan).unwrap().to_lowercase();
        for banned in ["\"safe\"", "personal-data-free", "kişisel veri içermiyor", "no personal data"] {
            assert!(!json.contains(banned), "{banned}");
        }
    }

    #[test]
    fn the_migration_backup_of_the_legacy_database_is_flagged_with_observed_findings_and_counts_only() {
        let (dir, live, backups) = migrated();
        let e = inventory(&backups).0.remove(0);
        assert_eq!((e.schema_version, e.privacy_status.as_str()), (Some(4), "violations_observed"));
        assert!(e.observation_rows_failing_gate.unwrap() > 0 && e.raw_path_like_matches > 0);
        assert!(e.file_name.starts_with("atm-v4-") && e.file_name.ends_with(".sqlite"));
        let conn = Connection::open(&live).unwrap();
        let plan = cleanup_plan(&conn, &backups).unwrap();
        let json = serde_json::to_string(&plan).unwrap();
        let dir_text = dir.path().to_string_lossy().to_lowercase();
        let sep = std::path::MAIN_SEPARATOR_STR;
        let lowered = json.to_lowercase();
        assert!(
            !json.contains("OtherUser")
                && !lowered.contains(&dir_text)
                && !lowered.contains(&dir_text.replace(sep, &format!("{sep}{sep}")))
        );
        assert_eq!(plan.review_candidates, vec![e.file_name.clone()]);
        assert!(plan.preconditions.iter().all(|p| p.ok), "{:?}", plan.preconditions);
        assert!(!plan.executed && !plan.deletion_available);
        assert!(plan.notes.iter().any(|n| n.contains("ÖNİZLEME") && n.contains("silinmedi")));
        assert!(plan.notes.iter().any(|n| n.contains("SSD")), "the plan must not promise physical erasure");
    }

    #[test]
    fn repeated_previews_never_modify_any_backup_or_leave_new_files() {
        let (_dir, live, backups) = migrated();
        extra_backup_set(&backups);
        let snapshot = |d: &Path| -> Vec<(String, Vec<u8>, std::time::SystemTime)> {
            let mut v: Vec<_> = fs::read_dir(d)
                .unwrap()
                .map(|e| {
                    let p = e.unwrap().path();
                    (
                        p.file_name().unwrap().to_string_lossy().into_owned(),
                        fs::read(&p).unwrap(),
                        fs::metadata(&p).unwrap().modified().unwrap(),
                    )
                })
                .collect();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        };
        let before = snapshot(&backups);
        let conn = Connection::open(&live).unwrap();
        for _ in 0..3 {
            cleanup_plan(&conn, &backups).unwrap();
        }
        assert_eq!(snapshot(&backups), before, "no byte, no timestamp, no file name changed, and no sidecar file appeared");
    }

    fn extra_backup_set(backups: &Path) {
        backup(backups, 2, "20261001T000000Z", "CREATE TABLE settings (k TEXT, v TEXT);");
        fs::write(backups.join("atm-v5-20261005T000000Z.sqlite"), b"garbage").unwrap();
        fs::write(backups.join("readme.txt"), b"x").unwrap();
    }

    #[test]
    fn a_live_database_that_fails_the_privacy_gate_blocks_the_plan_precondition() {
        let dir = tempfile::tempdir().unwrap();
        let conn = live_db(dir.path());
        let auth = crate::discovery::detect::Authorization { path_executables: false, user_config: false, project: None };
        let roots = crate::discovery::detect::Roots {
            platform: "windows-x64".into(),
            home: None,
            path_entries: vec![],
            system_dirs: vec![],
            pins: vec![],
        };
        let leaky = crate::discovery::types::Observation::new(
            "gitleaks",
            "windows-x64",
            "local-cli",
            crate::discovery::types::Method::PathExecutable,
        );
        crate::discovery::store::save_scan(&conn, &roots, &auth, &[leaky], "2026-10-17T00:00:00Z").unwrap();
        let plan = cleanup_plan(&conn, &dir.path().join("none")).unwrap();
        assert!(plan.preconditions.iter().all(|p| p.ok));
        conn.execute("UPDATE discovery_observations SET evidence = 'at C:\\Users\\X\\y'", []).unwrap();
        let plan = cleanup_plan(&conn, &dir.path().join("none")).unwrap();
        assert!(!plan.preconditions.iter().find(|p| p.id == "live-data-clean").unwrap().ok);
        assert!(plan.entries.is_empty() && plan.review_candidates.is_empty());
    }
}
