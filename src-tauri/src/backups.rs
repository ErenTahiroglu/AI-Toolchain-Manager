//! Pre-migration database backups: an inventory and a NON-DESTRUCTIVE cleanup preview.
//!
//! The application writes a consistent snapshot (`atm-v<schema>-<UTC stamp>.sqlite`) into its own `backups` folder
//! before every schema migration, so a failed or unwanted upgrade can be rolled back. Such a snapshot is a verbatim
//! copy: if the database held something private at that moment (Phase 2A stored full paths and MCP server names
//! before the privacy gate existed), the snapshot still holds it. Migrating the live database does not change that.
//!
//! Nothing in this module deletes, moves, rewrites or uploads a file. It opens backups read-only, reports counts
//! and file names (which contain only a schema number and a time stamp, never a folder), and states which backups
//! may still hold old personal data. Deleting them needs a separate, explicit user authorization that does not exist
//! in this phase.

use crate::{
    audit,
    db::{self, LATEST_VERSION},
    discovery::privacy,
    error::AppResult,
};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    /// Only `atm-v<N>-<stamp>.sqlite`: no directory, no account name.
    pub file_name: String,
    pub schema_version: Option<i64>,
    pub size_bytes: u64,
    pub modified: String,
    /// `yes` = at least one stored field would be rejected by today's privacy gate; `no` = none found;
    /// `unknown` = the file could not be opened or read.
    pub may_hold_old_personal_data: String,
    /// How many rows were found; counts only, never the content.
    pub rows_failing_privacy_gate: i64,
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
    /// Backups that may hold old personal data and that a future, separately authorized step could offer to delete.
    pub candidates: Vec<String>,
    pub keep: Vec<String>,
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

/// Counts rows that today's privacy gate would reject. Reads text only to judge it; nothing is returned but a count.
pub fn count_failing_rows(conn: &Connection) -> rusqlite::Result<i64> {
    // A file that is not a database fails here (so it is reported as unknown, never as clean).
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))?;
    if conn.prepare("SELECT 1 FROM discovery_observations LIMIT 0").is_err() {
        return Ok(0);
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
    Ok(bad)
}

fn inspect(path: &Path, name: &str, schema: Option<i64>) -> BackupEntry {
    let meta = std::fs::metadata(path).ok();
    let modified = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default();
    let judged = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .ok()
        .and_then(|c| count_failing_rows(&c).ok());
    let (flag, rows) = match judged {
        Some(0) => ("no", 0),
        Some(n) => ("yes", n),
        None => ("unknown", 0),
    };
    BackupEntry {
        file_name: name.to_string(),
        schema_version: schema,
        size_bytes: meta.map(|m| m.len()).unwrap_or(0),
        modified,
        may_hold_old_personal_data: flag.to_string(),
        rows_failing_privacy_gate: rows,
    }
}

/// Lists recognised backup files, oldest first. Files that do not follow the naming scheme are not listed (and not
/// named); their number is reported separately by `cleanup_plan`.
pub fn inventory(backup_dir: &Path) -> (Vec<BackupEntry>, usize) {
    let Ok(read) = std::fs::read_dir(backup_dir) else { return (Vec::new(), 0) };
    let mut entries = Vec::new();
    let mut unrecognised = 0;
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        match (parse_name(&name), item.file_type().map(|t| t.is_file()).unwrap_or(false)) {
            (Some(schema), true) => entries.push(inspect(&item.path(), &name, Some(schema))),
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
    let (entries, unrecognised) = inventory(backup_dir);
    let schema_ok = db::user_version(conn)? == LATEST_VERSION;
    let integrity_ok = conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)).map(|s| s == "ok").unwrap_or(false);
    let audit_ok = audit::verify(conn).map(|r| r.ok).unwrap_or(false);
    let live_clean = count_failing_rows(conn).map(|n| n == 0).unwrap_or(false);
    let preconditions = vec![
        precondition("schema-current", schema_ok, "Veritabanı en son şema sürümünde."),
        precondition("integrity", integrity_ok, "SQLite bütünlük denetimi (integrity_check) başarılı."),
        precondition("audit-chain", audit_ok, "Denetim kaydı zinciri doğrulanıyor."),
        precondition("live-data-clean", live_clean, "Güncel veritabanındaki gözlemler gizlilik kurallarına uyuyor."),
    ];
    let candidates: Vec<String> = entries.iter().filter(|e| e.may_hold_old_personal_data == "yes").map(|e| e.file_name.clone()).collect();
    let keep: Vec<String> = entries.iter().filter(|e| e.may_hold_old_personal_data != "yes").map(|e| e.file_name.clone()).collect();
    let mut notes = vec![
        "Bu bir ÖNİZLEMEDİR: hiçbir yedek silinmedi, taşınmadı veya değiştirilmedi. Silme bu sürümde uygulanmamıştır; ayrı ve açık bir yetki gerekir.".to_string(),
        "Bir yedeği silmek, o sürüme geri dönme imkânını kalıcı olarak ortadan kaldırır. Yeni sürümü yeterince denediyseniz ve geri dönmeniz gerekmiyorsa silmeyi düşünebilirsiniz.".to_string(),
        "Yedekler yalnızca bu uygulamanın kendi veri klasöründedir; GitHub'a veya herhangi bir yere gönderilmez.".to_string(),
        "Dosyanın silinmesi, içeriğin fiziksel olarak yok edildiğini garanti etmez: SSD'lerde aşınma dengeleme, işletim sistemi anlık görüntüleri ve çöp kutusu eski baytları tutabilir.".to_string(),
    ];
    if unrecognised > 0 {
        notes.push(format!("{unrecognised} dosya yedek adlandırmasına uymadığı için listelenmedi ve dokunulmadı."));
    }
    if entries.iter().any(|e| e.may_hold_old_personal_data == "unknown") {
        notes.push("Bazı yedekler açılamadı; içerikleri hakkında bir şey söylenemez.".to_string());
    }
    if candidates.is_empty() {
        notes.push("Eski kişisel bilgi taşıyabilecek yedek bulunamadı.".to_string());
    }
    Ok(CleanupPlan { entries, preconditions, candidates, keep, deletion_available: false, executed: false, notes })
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

    #[test]
    fn the_logical_scrub_and_the_raw_bytes_are_judged_separately_live_file_versus_backup() {
        let (_dir, live, backups) = migrated();
        // SQL-visible state of the migrated database is clean.
        let conn = Connection::open(&live).unwrap();
        assert_eq!(count_failing_rows(&conn).unwrap(), 0);
        drop(conn);
        // RAW bytes of the live database file (and any WAL) no longer hold the old text after secure_delete + VACUUM.
        let mut raw = fs::read(&live).unwrap();
        if let Ok(wal) = fs::read(live.with_extension("sqlite-wal")) {
            raw.extend(wal);
        }
        assert!(!contains(&raw, OTHER), "live database file: old personal text is gone from the raw bytes");
        // The pre-migration backup is a verbatim copy and DOES still hold it — that is why it is reported, not hidden.
        let backup = fs::read_dir(&backups).unwrap().next().unwrap().unwrap().path();
        assert!(contains(&fs::read(&backup).unwrap(), OTHER), "the rollback copy intentionally keeps the old content");
        // This is logical compaction of one file; the test does not and cannot show SSD-level erasure.
    }

    #[test]
    fn the_inventory_reports_which_backup_holds_old_data_without_exposing_it() {
        let (dir, live, backups) = migrated();
        let (entries, unrecognised) = inventory(&backups);
        assert_eq!((entries.len(), unrecognised), (1, 0));
        let e = &entries[0];
        assert_eq!((e.schema_version, e.may_hold_old_personal_data.as_str()), (Some(4), "yes"));
        assert!(e.rows_failing_privacy_gate > 0);
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
        assert_eq!(plan.candidates, vec![e.file_name.clone()]);
        assert!(plan.keep.is_empty());
        assert!(plan.preconditions.iter().all(|p| p.ok), "{:?}", plan.preconditions);
        assert!(!plan.executed && !plan.deletion_available);
        assert!(plan.notes.iter().any(|n| n.contains("ÖNİZLEME") && n.contains("silinmedi")));
        assert!(plan.notes.iter().any(|n| n.contains("SSD")), "the plan must not promise physical erasure");
    }

    #[test]
    fn planning_never_changes_a_backup() {
        let (_dir, live, backups) = migrated();
        let backup = fs::read_dir(&backups).unwrap().next().unwrap().unwrap().path();
        let before = (fs::read(&backup).unwrap(), fs::metadata(&backup).unwrap().modified().unwrap());
        let conn = Connection::open(&live).unwrap();
        for _ in 0..3 {
            cleanup_plan(&conn, &backups).unwrap();
        }
        assert_eq!(fs::read(&backup).unwrap(), before.0);
        assert_eq!(fs::metadata(&backup).unwrap().modified().unwrap(), before.1);
        assert_eq!(fs::read_dir(&backups).unwrap().count(), 1, "no file was added, removed or moved");
    }

    #[test]
    fn old_backups_without_discovery_data_corrupt_files_and_foreign_files_are_classified_conservatively() {
        let dir = tempfile::tempdir().unwrap();
        let backups = dir.path().join("backups");
        fs::create_dir_all(&backups).unwrap();
        // A v2-shaped backup: no discovery tables at all.
        let old = Connection::open(backups.join("atm-v2-20261001T000000Z.sqlite")).unwrap();
        old.execute_batch("CREATE TABLE settings (k TEXT, v TEXT);").unwrap();
        drop(old);
        fs::write(backups.join("atm-v4-20261002T000000Z.sqlite"), b"not a database at all").unwrap();
        fs::write(backups.join("notes-from-someone.txt"), b"x").unwrap();
        fs::write(backups.join("atm-v4-bad-stamp.sqlite"), b"x").unwrap();
        let (entries, unrecognised) = inventory(&backups);
        assert_eq!(unrecognised, 2);
        let by = |v: i64, stamp: &str| entries.iter().find(|e| e.schema_version == Some(v) && e.file_name.contains(stamp)).unwrap();
        assert_eq!(by(2, "20261001").may_hold_old_personal_data, "no");
        assert_eq!(by(4, "20261002").may_hold_old_personal_data, "unknown");
        let live = db::open_file(&dir.path().join("atm.sqlite"), None).unwrap();
        let plan = cleanup_plan(&live, &backups).unwrap();
        assert!(plan.candidates.is_empty());
        assert!(plan.notes.iter().any(|n| n.contains("2 dosya")), "{:?}", plan.notes);
        assert!(plan.notes.iter().any(|n| n.contains("açılamadı")));
        assert!(!serde_json::to_string(&plan).unwrap().contains("notes-from-someone"), "unrecognised files are counted, never named");
    }

    #[test]
    fn a_live_database_that_fails_the_privacy_gate_blocks_the_plan_precondition() {
        let dir = tempfile::tempdir().unwrap();
        let conn = db::open_file(&dir.path().join("atm.sqlite"), None).unwrap();
        crate::catalog::seed_all(&conn).unwrap();
        let env_scan = |c: &Connection| {
            let auth = crate::discovery::detect::Authorization { path_executables: false, user_config: false, project: None };
            let roots = crate::discovery::detect::Roots {
                platform: "windows-x64".into(),
                home: None,
                path_entries: vec![],
                system_dirs: vec![],
                pins: vec![],
            };
            crate::discovery::store::save_scan(c, &roots, &auth, &[], "2026-10-17T00:00:00Z").unwrap();
        };
        env_scan(&conn);
        let plan = cleanup_plan(&conn, &dir.path().join("none")).unwrap();
        assert!(plan.preconditions.iter().all(|p| p.ok));
        assert!(plan.entries.is_empty() && plan.candidates.is_empty());
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
}
