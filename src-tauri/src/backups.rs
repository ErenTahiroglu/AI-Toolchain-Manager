//! Pre-migration database backups: an inventory and a NON-DESTRUCTIVE cleanup preview.
//!
//! The application writes a consistent snapshot (`atm-v<schema>-<UTC stamp>.sqlite`) into its own `backups` folder
//! before every schema migration, so a failed or unwanted upgrade can be rolled back. Such a snapshot is a verbatim
//! copy: whatever the database held at that moment, it still holds.
//!
//! # What the scan can and cannot say
//!
//! The scan is a set of **pattern checks over bounded work**, reported layer by layer with an explicit outcome:
//!
//! | layer | what it covers |
//! | --- | --- |
//! | `observation_rows` | the privacy-gate fields of `discovery_observations` |
//! | `all_text` | every TEXT value of every table (private-path and secret/credential/e-mail shapes) |
//! | `raw_main` | the raw bytes of the backup file, streamed (can reveal text left in free pages) |
//! | `raw_wal` | the raw bytes of the `-wal` sidecar, if one exists |
//!
//! Each layer ends `completed`, `partial` (a limit was reached: cell budget, size, time), `unreadable` (could not be
//! read, was refused, or changed while being read), `unsupported` (e.g. image too large to load) or `not_present`.
//! A count of zero means *"no violation was detected in what was inspected"*; it is called
//! `none_detected_in_examined_content` ONLY when every layer completed. Any other outcome without a finding is
//! `partially_scanned_no_findings` or `not_assessable`. A finding in a partly scanned backup is still reported.
//! Backups written before schema 5 are `potentially_sensitive` whatever the scan finds.
//!
//! # Confinement
//!
//! Every backup file and its `-wal` sidecar is opened through the same confined, no-follow layer as the discovery
//! engine (`discovery::fsx`): symlinks, junctions/reparse points (also in parent folders), network and unrecognised
//! file systems are refused, and everything is read from the ONE handle that was opened (the SQLite image is loaded
//! from that handle, not from the path). A refused or unreadable sidecar makes the whole backup `not_assessable`.
//! Limitation: between listing a folder and opening a name inside it, a file can still be replaced; the replacement is
//! what gets opened (never a link target), and the change of size/time during the scan is detected and reported.
//!
//! Nothing in this module deletes, moves, rewrites or uploads a file; backups are only read. Deleting one needs a
//! separate, explicit user authorization that does not exist in this phase.

use crate::{
    audit,
    db::{self, LATEST_VERSION},
    discovery::{
        fsx::{self, FinalKind, ReadFail, Skip, Walk},
        privacy,
    },
    error::AppResult,
    redact,
};
use regex::Regex;
use rusqlite::{types::ValueRef, Connection};
use serde::Serialize;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::OnceLock,
    time::{Duration, Instant},
};

/// What was looked at (shown to the user verbatim).
pub const EXAMINED: &[&str] = &[
    "discovery_observations tablosunun gizlilik kapısı alanları: evidence, limitations_json, version, version_source, location, project_alias",
    "Tüm tablolardaki tüm metin değerleri: özel klasör yolu, gizli anahtar/parola/e-posta biçimleri (örüntü taraması; hücre sayısı sınırlıdır)",
    "Dosyanın ham baytları (ve varsa -wal yan dosyası): özel klasör yolu biçimleri (örüntü taraması; boyut ve süre sınırlıdır); kullanılmayan sayfalardaki kalıntıları yakalayabilir",
];

/// What was NOT looked at (shown to the user verbatim).
pub const NOT_EXAMINED: &[&str] = &[
    "Hiçbir örüntüye uymayan kişisel metin (ad, not, serbest metin) taranamaz",
    "Yedek dışındaki kopyalar: dosya sistemi anlık görüntüleri, gölge kopyalar, çöp kutusu, bulut eşitleme",
    "Başka kodlamalardaki (örn. UTF-16) veya sıkıştırılmış içerik",
    "Sınırlar aşıldığında dosyanın geri kalanı (bu durumda sonuç \"kısmi\" olarak işaretlenir)",
    "Bu bir örüntü taramasıdır; eksiksizlik iddia etmez",
];

/// Work limits of one backup scan. Reaching any of them makes the affected layer `partial`/`unsupported`, never "complete".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanLimits {
    pub max_text_cells: u64,
    /// Raw streaming scan of the main file and of the WAL (each).
    pub max_raw_bytes: u64,
    /// Largest database image loaded into memory for the SQL-visible layers.
    pub max_sqlite_image_bytes: u64,
    pub timeout_seconds: u64,
}

impl Default for ScanLimits {
    fn default() -> Self {
        ScanLimits {
            max_text_cells: 500_000,
            max_raw_bytes: 256 * 1024 * 1024,
            max_sqlite_image_bytes: 128 * 1024 * 1024,
            timeout_seconds: 30,
        }
    }
}

const RAW_CHUNK: usize = 1024 * 1024;
const RAW_OVERLAP: usize = 512;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LayerOutcome {
    Completed,
    Partial,
    Unreadable,
    Unsupported,
    NotPresent,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LayerReport {
    pub layer: String,
    pub outcome: LayerOutcome,
    /// A fixed code (never content): `cell_limit`, `size_limit`, `time_limit`, `read_error`, `redirected`, ...
    pub reason: String,
}

fn layer(name: &str, outcome: LayerOutcome, reason: &str) -> LayerReport {
    LayerReport { layer: name.to_string(), outcome, reason: reason.to_string() }
}

/// `violations_observed` | `potentially_sensitive` | `none_detected_in_examined_content`
/// | `partially_scanned_no_findings` | `not_assessable`
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
    /// `complete` | `partial` | `incomplete`: whether every layer finished.
    pub coverage: String,
    pub layers: Vec<LayerReport>,
    /// `discovery_observations` rows failing the privacy gate; None = the table is absent or was not read.
    pub observation_rows_failing_gate: Option<i64>,
    /// Text values in ANY table (including discovery) with a private-path or secret shape; counts only.
    pub other_text_findings: i64,
    /// Names of tables (schema identifiers, not content) in which such values were found.
    pub tables_with_findings: Vec<String>,
    /// Private-path-shaped byte sequences in the raw file(s); can include text no row refers to any more (free pages).
    pub raw_path_like_matches: i64,
    /// Raw layers (main file / WAL) in which a credential, e-mail or private-path shape was seen (a count of layers, never content).
    pub raw_sensitive_shape_layers: i64,
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
    pub limits: ScanLimits,
    /// `ok` | `missing` | `refused_redirect` | `refused_non_local` | `unreadable`
    pub directory_status: String,
    /// Backups a person should REVIEW before deciding anything (violations observed, or written before schema 5).
    pub review_candidates: Vec<String>,
    /// Every layer completed and nothing was detected. This is NOT "personal-data-free" and NOT "safe to delete".
    pub no_violation_detected: Vec<String>,
    /// Nothing detected, but at least one layer was partial or unsupported.
    pub partially_scanned: Vec<String>,
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

/// Result of the all-text layer: findings so far and whether the layer finished.
struct TextScan {
    findings: i64,
    tables: Vec<String>,
    outcome: LayerOutcome,
    reason: &'static str,
}

/// Pattern-checks every TEXT value of every table, within the cell budget and the deadline. Reaching either limit while
/// there is still something left to inspect makes the layer `partial` — never silently complete.
fn scan_all_text(conn: &Connection, max_cells: u64, deadline: Instant) -> rusqlite::Result<TextScan> {
    let tables: Vec<String> = {
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let mut out = TextScan { findings: 0, tables: Vec::new(), outcome: LayerOutcome::Completed, reason: "all_cells_inspected" };
    let mut inspected = 0u64;
    for table in tables {
        // Schema identifiers come from sqlite_master; quote them so odd names cannot alter the statement.
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))?;
        let n = stmt.column_count();
        let mut rows = stmt.query([])?;
        let mut hit = false;
        while let Some(row) = rows.next()? {
            for i in 0..n {
                if inspected >= max_cells {
                    out.outcome = LayerOutcome::Partial;
                    out.reason = "cell_limit";
                    if hit {
                        out.tables.push(table);
                    }
                    return Ok(out);
                }
                if inspected.is_multiple_of(1024) && Instant::now() >= deadline {
                    out.outcome = LayerOutcome::Partial;
                    out.reason = "time_limit";
                    if hit {
                        out.tables.push(table);
                    }
                    return Ok(out);
                }
                inspected += 1;
                if let ValueRef::Text(bytes) = row.get_ref(i)? {
                    if text_value_is_sensitive(&String::from_utf8_lossy(bytes)) {
                        out.findings += 1;
                        hit = true;
                    }
                }
            }
        }
        if hit {
            out.tables.push(table);
        }
    }
    Ok(out)
}

/// Streams a handle through the path-shape pattern in bounded chunks (with an overlap so a match split across two
/// chunks is still seen). Returns (matches, outcome, reason). Nothing larger than one chunk is held in memory.
#[cfg(test)]
fn raw_scan(file: &mut fs::File, max_bytes: u64, deadline: Instant) -> (i64, LayerOutcome, &'static str) {
    let (m, _, o, r) = raw_scan_shapes(file, max_bytes, deadline);
    (m, o, r)
}

/// Same stream, additionally reporting whether a credential / e-mail / private-path shape was seen anywhere in the
/// raw bytes (a flag, never content; free pages and an unreplayed WAL can hold text no SQL row refers to).
fn raw_scan_shapes(file: &mut fs::File, max_bytes: u64, deadline: Instant) -> (i64, bool, LayerOutcome, &'static str) {
    let mut matches = 0i64;
    let mut sensitive = false;
    let mut carry: Vec<u8> = Vec::new();
    let mut consumed = 0u64;
    let mut buf = vec![0u8; RAW_CHUNK];
    loop {
        if Instant::now() >= deadline {
            return (matches, sensitive, LayerOutcome::Partial, "time_limit");
        }
        let want = (max_bytes.saturating_sub(consumed)).min(RAW_CHUNK as u64) as usize;
        if want == 0 {
            // The cap was reached: complete only if the file really ends here.
            let mut probe = [0u8; 1];
            return match file.read(&mut probe) {
                Ok(0) => (matches, sensitive, LayerOutcome::Completed, "all_bytes_inspected"),
                Ok(_) => (matches, sensitive, LayerOutcome::Partial, "size_limit"),
                Err(_) => (matches, sensitive, LayerOutcome::Unreadable, "read_error"),
            };
        }
        let n = match file.read(&mut buf[..want]) {
            Ok(0) => return (matches, sensitive, LayerOutcome::Completed, "all_bytes_inspected"),
            Ok(n) => n,
            Err(_) => return (matches, sensitive, LayerOutcome::Unreadable, "read_error"),
        };
        consumed += n as u64;
        let boundary = String::from_utf8_lossy(&carry).len();
        let mut window = std::mem::take(&mut carry);
        window.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&window);
        sensitive = sensitive || redact::has_sensitive_shape(&text);
        matches += raw_path_regex().find_iter(&text).filter(|m| m.end() > boundary).count() as i64;
        let keep = window.len().min(RAW_OVERLAP);
        carry = window[window.len() - keep..].to_vec();
    }
}

fn skip_reason(skip: Skip) -> &'static str {
    match skip {
        Skip::Link => "redirected",
        Skip::Network | Skip::UnknownFilesystem => "non_local",
        Skip::NotRegularFile => "not_regular_file",
        Skip::AccessDenied => "access_denied",
        _ => "read_error",
    }
}

fn fail_reason(f: &ReadFail) -> &'static str {
    match f {
        ReadFail::NotFound => "not_found",
        ReadFail::Skipped(s) => skip_reason(*s),
    }
}

/// Judges whether the logical (SQL) layers can claim to have seen the WAL's content. Reads only the 32-byte header from
/// the already-open handle. A WAL with frames after a valid header holds transactions that may be committed but are not
/// in the main file image: `Partial / wal_not_replayed`. A non-empty WAL whose header cannot be recognised (truncated,
/// foreign bytes) cannot be ruled out either: `Partial / wal_header_unrecognised`. Only an empty WAL or a bare valid
/// header (no frames) is `Completed`.
fn wal_logical_state(wal: &mut fs::File, size: u64) -> (LayerOutcome, &'static str) {
    if size == 0 {
        return (LayerOutcome::Completed, "wal_empty");
    }
    let mut head = [0u8; 32];
    if size < 32 || wal.seek(SeekFrom::Start(0)).is_err() || wal.read_exact(&mut head).is_err() {
        return (LayerOutcome::Partial, "wal_header_unrecognised");
    }
    let magic = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
    if magic != 0x377f_0682 && magic != 0x377f_0683 {
        return (LayerOutcome::Partial, "wal_header_unrecognised");
    }
    if size == 32 {
        (LayerOutcome::Completed, "wal_no_frames")
    } else {
        (LayerOutcome::Partial, "wal_not_replayed")
    }
}

/// A WAL-mode main file declares journal mode "WAL" in header bytes 18/19, which an in-memory image cannot honour. The
/// bytes are rewritten to "rollback journal" (1) IN MEMORY while the image is loaded; the file itself is never touched.
/// The WAL is not applied either way (see `wal_logical_state`).
struct RollbackHeader<R> {
    inner: R,
    pos: u64,
}

impl<R: Read> Read for RollbackHeader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        for off in [18u64, 19u64] {
            if off >= self.pos && off < self.pos + n as u64 && buf[(off - self.pos) as usize] == 2 {
                buf[(off - self.pos) as usize] = 1;
            }
        }
        self.pos += n as u64;
        Ok(n)
    }
}

fn stamp(f: &fs::File) -> Option<(u64, Option<std::time::SystemTime>)> {
    f.metadata().ok().map(|m| (m.len(), m.modified().ok()))
}

/// Inspects ONE backup. All reads go through one confined handle per file.
fn inspect(dir: &Path, name: &str, schema: Option<i64>, limits: ScanLimits) -> BackupEntry {
    let deadline = Instant::now() + Duration::from_secs(limits.timeout_seconds);
    let mut entry = BackupEntry {
        file_name: name.to_string(),
        schema_version: schema,
        size_bytes: 0,
        modified: String::new(),
        privacy_status: "not_assessable".to_string(),
        reason: "unreadable".to_string(),
        coverage: "incomplete".to_string(),
        layers: Vec::new(),
        observation_rows_failing_gate: None,
        other_text_findings: 0,
        tables_with_findings: Vec::new(),
        raw_path_like_matches: 0,
        raw_sensitive_shape_layers: 0,
        has_wal_sidecar: false,
    };
    let mut file = match fsx::open_regular(dir, &[name]) {
        Ok(f) => f,
        Err(e) => {
            let why = fail_reason(&e);
            for l in ["observation_rows", "all_text", "raw_main"] {
                entry.layers.push(layer(l, LayerOutcome::Unreadable, why));
            }
            entry.layers.push(layer("raw_wal", LayerOutcome::Unreadable, "main_file_refused"));
            entry.layers.push(layer("wal_logical_sql", LayerOutcome::Unreadable, "main_file_refused"));
            entry.reason = why.to_string();
            return entry;
        }
    };
    let before = stamp(&file);
    let size = before.map(|b| b.0).unwrap_or(0);
    entry.size_bytes = size;
    entry.modified = before
        .and_then(|b| b.1)
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default();

    // Layer: raw main file (streamed).
    let (raw_main, raw_main_sensitive, outcome, reason) = raw_scan_shapes(&mut file, limits.max_raw_bytes, deadline);
    entry.raw_path_like_matches += raw_main;
    entry.raw_sensitive_shape_layers += raw_main_sensitive as i64;
    entry.layers.push(layer("raw_main", outcome, reason));

    // Layers: SQL-visible content, from an image read through the SAME handle.
    let mut rows_result: Option<Option<i64>> = None;
    if size > limits.max_sqlite_image_bytes {
        entry.layers.push(layer("observation_rows", LayerOutcome::Unsupported, "size_limit"));
        entry.layers.push(layer("all_text", LayerOutcome::Unsupported, "size_limit"));
    } else {
        let loaded = (|| -> Result<Connection, ()> {
            file.seek(SeekFrom::Start(0)).map_err(|_| ())?;
            let mut conn = Connection::open_in_memory().map_err(|_| ())?;
            conn.deserialize_read_exact(rusqlite::MAIN_DB, RollbackHeader { inner: &file, pos: 0 }, size as usize, true).map_err(|_| ())?;
            Ok(conn)
        })();
        match loaded {
            Err(()) => {
                entry.layers.push(layer("observation_rows", LayerOutcome::Unreadable, "not_a_readable_database"));
                entry.layers.push(layer("all_text", LayerOutcome::Unreadable, "not_a_readable_database"));
            }
            Ok(conn) => {
                match count_failing_rows(&conn) {
                    Ok(Some(n)) => {
                        rows_result = Some(Some(n));
                        entry.layers.push(layer("observation_rows", LayerOutcome::Completed, "all_rows_inspected"));
                    }
                    Ok(None) => {
                        rows_result = Some(None);
                        entry.layers.push(layer("observation_rows", LayerOutcome::NotPresent, "table_absent"));
                    }
                    Err(_) => entry.layers.push(layer("observation_rows", LayerOutcome::Unreadable, "not_a_readable_database")),
                }
                match scan_all_text(&conn, limits.max_text_cells, deadline) {
                    Ok(t) => {
                        entry.other_text_findings = t.findings;
                        entry.tables_with_findings = t.tables;
                        entry.layers.push(layer("all_text", t.outcome, t.reason));
                    }
                    Err(_) => entry.layers.push(layer("all_text", LayerOutcome::Unreadable, "not_a_readable_database")),
                }
            }
        }
    }
    entry.observation_rows_failing_gate = rows_result.flatten();

    // Layers: the WAL sidecar, confined exactly like the main file. Two different questions are answered separately:
    //  * `raw_wal`: a bounded pattern scan over the WAL's raw bytes;
    //  * `wal_logical_sql`: could the SQL layers above have seen what the WAL holds? They read the MAIN FILE image only,
    //    so committed transactions that sit only in the WAL are NOT replayed (no checkpoint, no VFS, nothing is written).
    let wal_name = format!("{name}-wal");
    let mut wal_stamp: Option<((u64, Option<std::time::SystemTime>), fs::File)> = None;
    match fsx::open_regular(dir, &[wal_name.as_str()]) {
        Err(ReadFail::NotFound) => {
            entry.layers.push(layer("raw_wal", LayerOutcome::NotPresent, "no_sidecar"));
            entry.layers.push(layer("wal_logical_sql", LayerOutcome::NotPresent, "no_sidecar"));
        }
        Err(e) => {
            entry.layers.push(layer("raw_wal", LayerOutcome::Unreadable, fail_reason(&e)));
            entry.layers.push(layer("wal_logical_sql", LayerOutcome::Unreadable, fail_reason(&e)));
        }
        Ok(mut wal) => {
            entry.has_wal_sidecar = true;
            let wal_before = stamp(&wal);
            let (outcome, why) = wal_logical_state(&mut wal, wal_before.map(|b| b.0).unwrap_or(0));
            let _ = wal.seek(SeekFrom::Start(0));
            let (m, sensitive, o, r) = raw_scan_shapes(&mut wal, limits.max_raw_bytes, deadline);
            entry.raw_path_like_matches += m;
            entry.raw_sensitive_shape_layers += sensitive as i64;
            entry.layers.push(layer("raw_wal", o, r));
            entry.layers.push(layer("wal_logical_sql", outcome, why));
            if let Some(b) = wal_before {
                wal_stamp = Some((b, wal));
            }
        }
    }
    if let Some((before_wal, wal)) = wal_stamp {
        if stamp(&wal) != Some(before_wal) {
            for l in entry.layers.iter_mut().filter(|l| l.layer == "raw_wal" || l.layer == "wal_logical_sql") {
                l.outcome = LayerOutcome::Unreadable;
                l.reason = "changed_while_scanning".to_string();
            }
        }
    }

    // The file must not have changed under us.
    if stamp(&file) != before {
        for l in entry.layers.iter_mut().filter(|l| l.layer != "raw_wal" && l.layer != "wal_logical_sql") {
            l.outcome = LayerOutcome::Unreadable;
            l.reason = "changed_while_scanning".to_string();
        }
    }

    classify(&mut entry);
    entry
}

/// Turns layer outcomes and findings into the status. Precedence: a finding is always reported; then anything that could
/// not be read; then the legacy rule; then incompleteness; only a fully completed, finding-free scan is "none detected".
fn classify(e: &mut BackupEntry) {
    let all = |f: fn(LayerOutcome) -> bool| e.layers.iter().filter(|l| l.outcome != LayerOutcome::NotPresent).any(|l| f(l.outcome));
    let any_unreadable = all(|o| o == LayerOutcome::Unreadable);
    let any_partial = all(|o| matches!(o, LayerOutcome::Partial | LayerOutcome::Unsupported));
    let main_unreadable = e.layers.iter().any(|l| l.layer == "raw_main" && l.outcome == LayerOutcome::Unreadable);
    let observed = e.observation_rows_failing_gate.unwrap_or(0) > 0
        || e.other_text_findings > 0
        || e.raw_path_like_matches > 0
        || e.raw_sensitive_shape_layers > 0;
    let legacy = e.schema_version.map(|v| v < 5).unwrap_or(true);
    e.coverage = if any_unreadable {
        "incomplete"
    } else if any_partial {
        "partial"
    } else {
        "complete"
    }
    .to_string();
    let (status, reason) = if observed {
        ("violations_observed", "findings_in_examined_content")
    } else if main_unreadable || any_unreadable {
        ("not_assessable", "a_layer_could_not_be_read_or_was_refused")
    } else if legacy {
        ("potentially_sensitive", "written_before_privacy_rules_schema_below_5")
    } else if any_partial {
        ("partially_scanned_no_findings", "a_work_limit_was_reached_nothing_found_in_the_inspected_part")
    } else {
        ("none_detected_in_examined_content", "no_pattern_found_coverage_is_partial_by_nature")
    };
    e.privacy_status = status.to_string();
    e.reason = reason.to_string();
}

/// Folder state before any listing: the folder must itself pass the confined walk.
fn directory_state(dir: &Path) -> &'static str {
    match fsx::walk(dir, &[], FinalKind::Dir, false) {
        Walk::Found { .. } => "ok",
        Walk::NotFound => "missing",
        Walk::Skipped(Skip::Link) => "refused_redirect",
        Walk::Skipped(Skip::Network | Skip::UnknownFilesystem) => "refused_non_local",
        Walk::Skipped(_) => "unreadable",
    }
}

/// Lists recognised backup files, oldest first, how many other files were left alone, and the folder's state.
pub fn inventory_with(backup_dir: &Path, limits: ScanLimits) -> (Vec<BackupEntry>, usize, &'static str) {
    let state = directory_state(backup_dir);
    if state != "ok" {
        return (Vec::new(), 0, state);
    }
    let Ok(read) = fs::read_dir(backup_dir) else { return (Vec::new(), 0, "unreadable") };
    let mut entries = Vec::new();
    let mut unrecognised = 0;
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        match parse_name(&name) {
            // Recognised by name: `inspect` decides (a link, folder or odd file becomes `not_assessable`).
            Some(schema) => entries.push(inspect(backup_dir, &name, Some(schema), limits)),
            // SQLite sidecars of a recognised backup are not "foreign" files.
            None if name.ends_with("-wal") || name.ends_with("-shm") || name.ends_with("-journal") => {}
            None => unrecognised += 1,
        }
    }
    entries.sort_by(|a, b| a.modified.cmp(&b.modified).then(a.file_name.cmp(&b.file_name)));
    (entries, unrecognised, state)
}

#[cfg(test)]
pub fn inventory(backup_dir: &Path) -> (Vec<BackupEntry>, usize) {
    let (e, u, _) = inventory_with(backup_dir, ScanLimits::default());
    (e, u)
}

fn precondition(id: &str, ok: bool, text: &str) -> Precondition {
    Precondition { id: id.to_string(), ok, text: text.to_string() }
}

/// A read-only plan. It never touches a backup file.
pub fn cleanup_plan(conn: &Connection, backup_dir: &Path) -> AppResult<CleanupPlan> {
    cleanup_plan_with(conn, backup_dir, ScanLimits::default())
}

pub fn cleanup_plan_with(conn: &Connection, backup_dir: &Path, limits: ScanLimits) -> AppResult<CleanupPlan> {
    let (entries, unrecognised_files, directory_status) = inventory_with(backup_dir, limits);
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
    let partially_scanned = names(&["partially_scanned_no_findings"]);
    let not_assessable = names(&["not_assessable"]);
    let mut notes = vec![
        "Bu bir ÖNİZLEMEDİR: hiçbir yedek silinmedi, taşınmadı veya değiştirilmedi. Silme bu sürümde uygulanmamıştır; ayrı ve açık bir yetki gerekir.".to_string(),
        "\"İhlal bulunmadı\", yedeğin kişisel veri içermediği veya silinmesinin güvenli olduğu anlamına GELMEZ: yalnızca yukarıda sayılan alanlarda ve örüntülerde bir şey bulunamadı.".to_string(),
        "Şema 5'ten eski yedekler, gizlilik kuralları yokken yazıldığı için tarama sonucundan bağımsız olarak \"eski kişisel bilgi taşıyabilir\" sayılır.".to_string(),
        "Bir yedeği silmek, o sürüme geri dönme imkânını kalıcı olarak ortadan kaldırır. Geri dönmeniz gerekmeyecekse silmeyi düşünebilirsiniz.".to_string(),
        "Yedekler yalnızca bu uygulamanın kendi veri klasöründedir; GitHub'a veya herhangi bir yere gönderilmez.".to_string(),
        "Dosyanın silinmesi, içeriğin fiziksel olarak yok edildiğini garanti etmez: SSD'lerde aşınma dengeleme, işletim sistemi anlık görüntüleri ve çöp kutusu eski baytları tutabilir.".to_string(),
    ];
    if directory_status != "ok" && directory_status != "missing" {
        notes.push("Yedek klasörünün kendisi sembolik bağlantı/junction, ağ yolu veya okunamaz olduğu için hiçbir yedeğe dokunulmadı (BİLİNMİYOR).".to_string());
    }
    if unrecognised_files > 0 {
        notes.push(format!(
            "{unrecognised_files} dosya yedek adlandırmasına uymadığı için açılmadı ve incelenmedi (BİLİNMİYOR); adı da gösterilmez."
        ));
    }
    if !not_assessable.is_empty() {
        notes.push("Açılamayan, bozuk, bağlantı/yönlendirme olan veya taranırken değişen yedeklerin içeriği hakkında bir şey söylenemez (BİLİNMİYOR).".to_string());
    }
    if !partially_scanned.is_empty() || entries.iter().any(|e| e.coverage == "partial") {
        notes.push("Bazı yedekler iş sınırları (hücre sayısı, boyut veya süre) nedeniyle yalnızca KISMEN tarandı; bulunmayan bir şey, taranmayan bölümde olabilir.".to_string());
    }
    if entries.iter().any(|e| e.has_wal_sidecar) {
        notes.push("Bazı yedeklerin -wal yan dosyası var; ham örüntü taraması onu da (aynı güvenli açma kuralıyla) kapsadı.".to_string());
    }
    if entries.iter().any(|e| e.layers.iter().any(|l| l.layer == "wal_logical_sql" && l.outcome == LayerOutcome::Partial)) {
        notes.push("Bazı yedeklerin -wal dosyasında ana dosyaya henüz işlenmemiş işlemler olabilir. Bu işlemler SQL satırları olarak İNCELENMEDİ (yedeğe hiçbir şey yazılmaz; kontrol noktası veya yeniden oynatma yapılmaz); yalnızca ham baytları örüntü olarak tarandı. Bu yedekler \"kısmen tarandı\" sayılır.".to_string());
    }
    if review_candidates.is_empty()
        && not_assessable.is_empty()
        && partially_scanned.is_empty()
        && unrecognised_files == 0
        && directory_status == "ok"
    {
        notes.push("İncelenen içerikte eski kişisel bilgi belirtisi bulunmadı (kapsam sınırlıdır).".to_string());
    }
    Ok(CleanupPlan {
        entries,
        preconditions,
        examined: EXAMINED.iter().map(|s| s.to_string()).collect(),
        not_examined: NOT_EXAMINED.iter().map(|s| s.to_string()).collect(),
        limits,
        directory_status: directory_status.to_string(),
        review_candidates,
        no_violation_detected,
        partially_scanned,
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
    use std::{cell::RefCell, rc::Rc};

    const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/phase2a-v4-synthetic.sqlite.fixture");
    const OTHER: &[u8] = b"OtherUser";

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    /// A canonical temp root (macOS temp folders sit below the `/var` symlink, which the confined layer rightly refuses).
    fn root() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        (dir, canonical)
    }

    /// Opens a copy of the synthetic v4 database through the real start-up path (which writes the backup and migrates).
    fn migrated() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let (dir, base) = root();
        let live = base.join("atm.sqlite");
        fs::copy(FIXTURE, &live).unwrap();
        let backups = base.join("backups");
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

    const CLEAN5: &str = "CREATE TABLE notes (id INTEGER PRIMARY KEY, v TEXT); INSERT INTO notes (v) VALUES ('plain words');";

    fn entry_with(dir: &Path, part: &str, limits: ScanLimits) -> BackupEntry {
        inventory_with(dir, limits).0.into_iter().find(|e| e.file_name.contains(part)).expect("entry")
    }

    fn entry_of(dir: &Path, part: &str) -> BackupEntry {
        entry_with(dir, part, ScanLimits::default())
    }

    fn layer_of<'a>(e: &'a BackupEntry, name: &str) -> &'a LayerReport {
        e.layers.iter().find(|l| l.layer == name).unwrap_or_else(|| panic!("no layer {name} in {:?}", e.layers))
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

    // ---- coverage-honest classification (Issue #7) -----------------------------------------------------------------

    #[test]
    fn an_old_schema_backup_without_a_discovery_table_but_with_private_settings_is_never_called_clean() {
        let (_d, base) = root();
        let backups = base.join("backups");
        backup(
            &backups,
            2,
            "20261001T000000Z",
            r"CREATE TABLE settings (k TEXT, v TEXT); INSERT INTO settings VALUES ('note', 'my work is in C:\Clients\AcmeCorp\secret-project');",
        );
        let e = entry_of(&backups, "20261001");
        assert_eq!(e.privacy_status, "violations_observed");
        assert_eq!(e.observation_rows_failing_gate, None);
        assert_eq!(layer_of(&e, "observation_rows").outcome, LayerOutcome::NotPresent);
        assert_eq!(e.tables_with_findings, vec!["settings".to_string()]);
        let live = live_db(&base);
        let plan = cleanup_plan(&live, &backups).unwrap();
        assert_eq!(plan.review_candidates, vec![e.file_name.clone()]);
        assert!(plan.no_violation_detected.is_empty());
    }

    #[test]
    fn an_old_schema_backup_with_nothing_detectable_is_potentially_sensitive_not_clean() {
        let (_d, base) = root();
        let backups = base.join("backups");
        backup(&backups, 2, "20261001T000000Z", "CREATE TABLE settings (k TEXT, v TEXT); INSERT INTO settings VALUES ('theme', 'dark');");
        backup(&backups, 4, "20261002T000000Z", "CREATE TABLE misc (x TEXT); INSERT INTO misc VALUES ('plain words');");
        for part in ["20261001", "20261002"] {
            let e = entry_of(&backups, part);
            assert_eq!(e.privacy_status, "potentially_sensitive", "{part}");
            assert_eq!(e.coverage, "complete", "the scan finished; the label is about the schema, not about coverage");
        }
        let live = live_db(&base);
        let plan = cleanup_plan(&live, &backups).unwrap();
        assert_eq!(plan.review_candidates.len(), 2);
        assert!(plan.no_violation_detected.is_empty());
    }

    #[test]
    fn clean_discovery_rows_do_not_hide_sensitive_text_in_another_table() {
        let (_d, base) = root();
        let backups = base.join("backups");
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
        assert_eq!(e.observation_rows_failing_gate, Some(0));
        assert_eq!(e.privacy_status, "violations_observed");
        assert_eq!(e.tables_with_findings, vec!["diagnostics".to_string()]);
    }

    #[test]
    fn sensitive_text_in_free_pages_is_caught_by_the_raw_scan_although_no_row_holds_it() {
        let (_d, base) = root();
        let backups = base.join("backups");
        fs::create_dir_all(&backups).unwrap();
        let path = backups.join("atm-v5-20261004T000000Z.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE notes (id INTEGER PRIMARY KEY, v TEXT);").unwrap();
        conn.execute("INSERT INTO notes (v) VALUES (?1)", [r"C:\Users\OtherUser\Clients\AcmeCorp\plans.txt".repeat(20)]).unwrap();
        conn.execute_batch("DELETE FROM notes;").unwrap();
        drop(conn);
        let e = entry_of(&backups, "20261004");
        assert_eq!(e.other_text_findings, 0);
        assert!(e.raw_path_like_matches > 0);
        assert_eq!(e.privacy_status, "violations_observed");
    }

    #[test]
    fn unreadable_malformed_and_foreign_files_are_unknown_never_clean() {
        let (_d, base) = root();
        let backups = base.join("backups");
        fs::create_dir_all(&backups).unwrap();
        fs::write(backups.join("atm-v5-20261005T000000Z.sqlite"), b"not a database at all").unwrap();
        fs::write(backups.join("atm-v5-20261006T000000Z.sqlite"), b"SQLite format 3\0 but truncated").unwrap();
        fs::write(backups.join("notes-from-someone.txt"), b"x").unwrap();
        fs::write(backups.join("atm-v4-bad-stamp.sqlite"), b"x").unwrap();
        for part in ["20261005", "20261006"] {
            let e = entry_of(&backups, part);
            assert_eq!(e.privacy_status, "not_assessable", "{part}");
            assert_eq!(layer_of(&e, "all_text").outcome, LayerOutcome::Unreadable);
            assert_eq!(e.coverage, "incomplete");
        }
        let live = live_db(&base);
        let plan = cleanup_plan(&live, &backups).unwrap();
        assert_eq!(plan.unrecognised_files, 2);
        assert_eq!(plan.not_assessable.len(), 2);
        let json = serde_json::to_string(&plan).unwrap();
        assert!(!json.contains("notes-from-someone") && !json.contains("bad-stamp"));
        assert!(plan.notes.iter().any(|n| n.contains("BİLİNMİYOR")));
    }

    #[test]
    fn a_legitimate_current_schema_backup_reports_no_violation_detected_only_when_every_layer_completed() {
        let (_d, base) = root();
        let backups = base.join("backups");
        let src = base.join("src.sqlite");
        {
            let conn = db::open_file(&src, None).unwrap();
            crate::catalog::seed_all(&conn).unwrap();
            fs::create_dir_all(&backups).unwrap();
            conn.execute("VACUUM INTO ?1", [backups.join("atm-v5-20261007T000000Z.sqlite").to_string_lossy().as_ref()]).unwrap();
        }
        let e = entry_of(&backups, "20261007");
        assert_eq!(e.privacy_status, "none_detected_in_examined_content", "{e:?}");
        assert_eq!(e.coverage, "complete");
        assert!(
            ["observation_rows", "all_text", "raw_main"].iter().all(|n| layer_of(&e, n).outcome == LayerOutcome::Completed),
            "{:?}",
            e.layers
        );
        assert_eq!(layer_of(&e, "raw_wal").outcome, LayerOutcome::NotPresent);
        let live = live_db(&base);
        let plan = cleanup_plan(&live, &backups).unwrap();
        assert_eq!(plan.no_violation_detected, vec![e.file_name.clone()]);
        assert!(plan.examined.len() >= 3 && plan.not_examined.len() >= 3);
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
        assert!(plan.notes.iter().any(|n| n.contains("SSD")));
    }

    #[test]
    fn repeated_previews_never_modify_any_backup_or_leave_new_files() {
        let (_dir, live, backups) = migrated();
        backup(&backups, 2, "20261001T000000Z", "CREATE TABLE settings (k TEXT, v TEXT);");
        fs::write(backups.join("atm-v5-20261005T000000Z.sqlite"), b"garbage").unwrap();
        fs::write(backups.join("readme.txt"), b"x").unwrap();
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
        let first = cleanup_plan(&conn, &backups).unwrap();
        for _ in 0..2 {
            assert_eq!(cleanup_plan(&conn, &backups).unwrap(), first, "identical inputs give an identical plan");
        }
        assert_eq!(snapshot(&backups), before, "no byte, no timestamp, no file name changed, and no sidecar file appeared");
    }

    #[test]
    fn a_live_database_that_fails_the_privacy_gate_blocks_the_plan_precondition() {
        let (_d, base) = root();
        let conn = live_db(&base);
        let auth =
            crate::discovery::detect::Authorization { path_executables: false, user_config: false, project: None, ..Default::default() };
        let roots = crate::discovery::detect::Roots {
            platform: "windows-x64".into(),
            home: None,
            path_entries: vec![],
            system_dirs: vec![],
            pins: vec![],
            references: Default::default(),
        };
        let leaky = crate::discovery::types::Observation::new(
            "gitleaks",
            "windows-x64",
            "local-cli",
            crate::discovery::types::Method::PathExecutable,
        );
        crate::discovery::store::save_scan(&conn, &roots, &auth, &[leaky], "2026-10-17T00:00:00Z").unwrap();
        let plan = cleanup_plan(&conn, &base.join("none")).unwrap();
        assert!(plan.preconditions.iter().all(|p| p.ok));
        assert_eq!(plan.directory_status, "missing");
        conn.execute("UPDATE discovery_observations SET evidence = 'at C:\\Users\\X\\y'", []).unwrap();
        let plan = cleanup_plan(&conn, &base.join("none")).unwrap();
        assert!(!plan.preconditions.iter().find(|p| p.id == "live-data-clean").unwrap().ok);
    }

    // ---- RT-2B-R3: a partial scan is never a complete scan ----------------------------------------------------------

    /// A schema-5 backup with `n` single-text-cell rows and (optionally) a secret-shaped value as the LAST row. The secret
    /// has no path shape, so only the all-text layer (not the raw layer) can recognise it.
    fn many_cells(dir: &Path, stamp: &str, n: u32, secret_last: bool) {
        fs::create_dir_all(dir).unwrap();
        let mut conn = Connection::open(dir.join(format!("atm-v5-{stamp}.sqlite"))).unwrap();
        conn.execute_batch("CREATE TABLE cells (id INTEGER PRIMARY KEY, v TEXT);").unwrap();
        let tx = conn.transaction().unwrap();
        {
            let mut stmt = tx.prepare("INSERT INTO cells (v) VALUES (?1)").unwrap();
            for i in 0..n {
                stmt.execute([format!("w{i}")]).unwrap();
            }
            if secret_last {
                stmt.execute(["token=ghp_SYNTHETICSYNTHETICSYNTHETICSYNTH12"]).unwrap();
            }
        }
        tx.commit().unwrap();
    }

    #[test]
    fn more_than_the_default_500000_text_cells_is_partial_and_a_secret_beyond_the_budget_is_not_claimed_absent() {
        let (_d, base) = root();
        let backups = base.join("backups");
        many_cells(&backups, "20261101T000000Z", 500_000, true); // 500_001 rows: the secret is cell number 1000002 counting ids
                                                                 // The raw pattern layer would see the secret bytes; its budget is kept tiny so that THIS test isolates the cell budget.
        let e = entry_with(&backups, "20261101", ScanLimits { max_raw_bytes: 64, ..ScanLimits::default() });
        let text = layer_of(&e, "all_text");
        assert_eq!((text.outcome, text.reason.as_str()), (LayerOutcome::Partial, "cell_limit"), "{:?}", e.layers);
        assert_eq!(e.other_text_findings, 0, "the secret lies beyond the inspection budget");
        assert_eq!(e.coverage, "partial");
        assert_eq!(e.privacy_status, "partially_scanned_no_findings", "never none_detected after a partial scan");
        let live = live_db(&base);
        let plan = cleanup_plan_with(&live, &backups, ScanLimits { max_raw_bytes: 64, ..ScanLimits::default() }).unwrap();
        assert!(plan.no_violation_detected.is_empty() && plan.partially_scanned == vec![e.file_name.clone()]);
        assert!(plan.notes.iter().any(|n| n.contains("KISMEN")));
    }

    #[test]
    fn partial_scans_with_and_without_findings_and_complete_scans_are_three_different_results() {
        let (_d, base) = root();
        let backups = base.join("backups");
        let tiny = ScanLimits { max_text_cells: 20, ..ScanLimits::default() };
        many_cells(&backups, "20261102T000000Z", 5, false); // complete under the tiny budget
        many_cells(&backups, "20261103T000000Z", 100, false); // partial, nothing found
        many_cells(&backups, "20261104T000000Z", 100, true); // partial, the secret is beyond the budget
                                                             // a hit BEFORE the cut-off, in a table that is only partly scanned
        let early = backup(&backups, 5, "20261105T000000Z", "CREATE TABLE a (v TEXT); INSERT INTO a VALUES ('token=ghp_SYNTHETICSYNTHETICSYNTHETICSYNTH12'); CREATE TABLE cells (id INTEGER PRIMARY KEY, v TEXT);");
        {
            let conn = Connection::open(&early).unwrap();
            for i in 0..100 {
                conn.execute("INSERT INTO cells (v) VALUES (?1)", [format!("w{i}")]).unwrap();
            }
        }
        let complete = entry_with(&backups, "20261102", tiny);
        assert_eq!(
            (complete.coverage.as_str(), complete.privacy_status.as_str()),
            ("complete", "none_detected_in_examined_content"),
            "{:?}",
            complete.layers
        );
        let partial = entry_with(&backups, "20261103", tiny);
        assert_eq!((partial.coverage.as_str(), partial.privacy_status.as_str()), ("partial", "partially_scanned_no_findings"));
        // (the raw layer would see the secret bytes; its budget is tiny here so that the cell budget is what is being tested)
        let beyond = entry_with(&backups, "20261104", ScanLimits { max_raw_bytes: 64, ..tiny });
        assert_eq!((beyond.other_text_findings, beyond.privacy_status.as_str()), (0, "partially_scanned_no_findings"));
        let seen_by_raw = entry_with(&backups, "20261104", tiny);
        assert_eq!(
            (seen_by_raw.other_text_findings, seen_by_raw.raw_sensitive_shape_layers, seen_by_raw.privacy_status.as_str()),
            (0, 1, "violations_observed"),
            "with the default raw budget the bounded raw scan reports the credential-shaped bytes"
        );
        let hit = entry_with(&backups, "20261105", tiny);
        assert_eq!(hit.coverage, "partial");
        assert_eq!(
            (hit.other_text_findings, hit.privacy_status.as_str()),
            (1, "violations_observed"),
            "a finding is reported even though the scan stopped early"
        );
        assert_eq!(hit.tables_with_findings, vec!["a".to_string()]);
    }

    #[test]
    fn raw_and_sqlite_size_limits_and_the_time_limit_make_their_layers_partial_or_unsupported() {
        let (_d, base) = root();
        let backups = base.join("backups");
        many_cells(&backups, "20261106T000000Z", 3000, false);
        let size = fs::metadata(backups.join("atm-v5-20261106T000000Z.sqlite")).unwrap().len();
        assert!(size > 8192);
        // main file above the raw cap, WAL sidecar above it too
        fs::write(backups.join("atm-v5-20261106T000000Z.sqlite-wal"), vec![b'x'; 9000]).unwrap();
        let raw_capped = entry_with(&backups, "20261106", ScanLimits { max_raw_bytes: 4096, ..ScanLimits::default() });
        assert_eq!(
            (layer_of(&raw_capped, "raw_main").outcome, layer_of(&raw_capped, "raw_main").reason.as_str()),
            (LayerOutcome::Partial, "size_limit")
        );
        assert_eq!(
            (layer_of(&raw_capped, "raw_wal").outcome, layer_of(&raw_capped, "raw_wal").reason.as_str()),
            (LayerOutcome::Partial, "size_limit")
        );
        assert!(raw_capped.has_wal_sidecar);
        assert_eq!(raw_capped.privacy_status, "partially_scanned_no_findings");
        // image too large to load for the SQL-visible layers
        let image_capped = entry_with(&backups, "20261106", ScanLimits { max_sqlite_image_bytes: 1024, ..ScanLimits::default() });
        assert_eq!(layer_of(&image_capped, "all_text").outcome, LayerOutcome::Unsupported);
        assert_eq!(layer_of(&image_capped, "observation_rows").outcome, LayerOutcome::Unsupported);
        assert_eq!(image_capped.privacy_status, "partially_scanned_no_findings");
        // a deadline that has already passed
        let late = entry_with(&backups, "20261106", ScanLimits { timeout_seconds: 0, ..ScanLimits::default() });
        assert!(late.layers.iter().any(|l| l.outcome == LayerOutcome::Partial && l.reason == "time_limit"), "{:?}", late.layers);
        assert_ne!(late.privacy_status, "none_detected_in_examined_content");
        // a file that fits exactly into the cap is complete (the cap is not a false alarm)
        let exact = entry_with(&backups, "20261106", ScanLimits { max_raw_bytes: size, ..ScanLimits::default() });
        assert_eq!(layer_of(&exact, "raw_main").outcome, LayerOutcome::Completed);
        // the defaults
        let d = ScanLimits::default();
        assert_eq!((d.max_text_cells, d.max_raw_bytes, d.max_sqlite_image_bytes), (500_000, 256 * 1024 * 1024, 128 * 1024 * 1024));
    }

    #[test]
    fn a_path_shape_split_across_two_streaming_chunks_is_still_found_and_not_double_counted() {
        let (_d, base) = root();
        let backups = base.join("backups");
        fs::create_dir_all(&backups).unwrap();
        // 1 MiB chunk: put the pattern across the boundary.
        let mut bytes = vec![b'.'; RAW_CHUNK - 3];
        bytes.extend_from_slice(br"C:\Users\Split\file");
        bytes.extend(vec![b'.'; 4096]);
        let p = backups.join("atm-v5-20261107T000000Z.sqlite");
        fs::write(&p, &bytes).unwrap();
        let mut f = fs::File::open(&p).unwrap();
        let (n, outcome, _) = raw_scan(&mut f, u64::MAX, Instant::now() + Duration::from_secs(30));
        assert_eq!((n, outcome), (1, LayerOutcome::Completed));
    }

    #[test]
    fn a_file_that_changes_while_it_is_scanned_is_reported_as_unreadable() {
        let (_d, base) = root();
        let backups = base.join("backups");
        backup(&backups, 5, "20261108T000000Z", CLEAN5);
        let target = backups.join("atm-v5-20261108T000000Z.sqlite");
        // An (empty) sidecar must exist so that the confined open of it is reached and the hook fires.
        fs::write(backups.join("atm-v5-20261108T000000Z.sqlite-wal"), b"").unwrap();
        let t = target.clone();
        let opens = Rc::new(RefCell::new(0));
        let o = opens.clone();
        // The hook fires before every confined open: the 2nd one is the WAL sidecar, which comes AFTER the main file was scanned.
        fsx::hook::set(move |stage| {
            if stage == "before-open" {
                *o.borrow_mut() += 1;
                if *o.borrow() == 2 {
                    use std::io::Write;
                    fs::OpenOptions::new().append(true).open(&t).unwrap().write_all(&[0u8; 64]).unwrap();
                }
            }
        });
        let e = entry_of(&backups, "20261108");
        fsx::hook::clear();
        assert_eq!(layer_of(&e, "raw_main").reason, "changed_while_scanning", "{:?}", e.layers);
        assert_eq!(e.coverage, "incomplete");
        assert_eq!(e.privacy_status, "not_assessable");
    }

    // ---- RT-2B-R4: confinement --------------------------------------------------------------------------------------

    /// Outside the backup folder: a database whose content would be flagged if it were ever read.
    fn outside_sentinel(base: &Path) -> std::path::PathBuf {
        let p = base.join("outside-sentinel.sqlite");
        let conn = Connection::open(&p).unwrap();
        conn.execute_batch(r"CREATE TABLE t (v TEXT); INSERT INTO t VALUES ('C:\Users\OtherUser\Clients\AcmeCorp\plans.txt token=ghp_SYNTHETICSYNTHETICSYNTHETICSYNTH12');").unwrap();
        p
    }

    fn file_link(link: &Path, target: &Path) -> bool {
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target, link).is_ok()
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
    }

    fn dir_link(link: &Path, target: &Path) {
        #[cfg(windows)]
        {
            let out = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(link).arg(target).output().unwrap();
            assert!(out.status.success(), "could not create a junction for the test");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    fn remove_dir_link(link: &Path) {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("cmd").args(["/C", "rmdir"]).arg(link).output();
        }
        #[cfg(unix)]
        let _ = fs::remove_file(link);
    }

    fn assert_nothing_from_outside(e: &BackupEntry) {
        assert_eq!(e.privacy_status, "not_assessable", "{e:?}");
        assert_eq!(
            (e.raw_path_like_matches, e.other_text_findings, e.observation_rows_failing_gate),
            (0, 0, None),
            "the outside sentinel was read: {e:?}"
        );
        assert!(e.tables_with_findings.is_empty());
    }

    #[test]
    fn a_symlinked_backup_file_is_refused_and_the_outside_target_is_never_read() {
        let (_d, base) = root();
        let backups = base.join("backups");
        fs::create_dir_all(&backups).unwrap();
        let outside = outside_sentinel(&base);
        if !file_link(&backups.join("atm-v5-20261201T000000Z.sqlite"), &outside) {
            eprintln!("file symlinks need a privilege that is not available here; case not exercised");
            return;
        }
        let e = entry_of(&backups, "20261201");
        assert_nothing_from_outside(&e);
        assert_eq!(layer_of(&e, "raw_main").reason, "redirected");
    }

    #[test]
    fn a_symlinked_wal_sidecar_makes_the_backup_not_assessable_and_is_never_followed() {
        let (_d, base) = root();
        let backups = base.join("backups");
        backup(&backups, 5, "20261202T000000Z", CLEAN5);
        let clean = entry_of(&backups, "20261202");
        assert_eq!(clean.privacy_status, "none_detected_in_examined_content", "control: the backup itself is clean");
        let outside = base.join("outside-wal.bin");
        fs::write(&outside, r"C:\Users\OtherUser\Clients\AcmeCorp\plans.txt".repeat(10)).unwrap();
        if !file_link(&backups.join("atm-v5-20261202T000000Z.sqlite-wal"), &outside) {
            eprintln!("file symlinks need a privilege that is not available here; case not exercised");
            return;
        }
        let e = entry_of(&backups, "20261202");
        assert_eq!(e.privacy_status, "not_assessable");
        assert_eq!((layer_of(&e, "raw_wal").outcome, layer_of(&e, "raw_wal").reason.as_str()), (LayerOutcome::Unreadable, "redirected"));
        assert_eq!(e.raw_path_like_matches, 0, "nothing from the link target was counted");
    }

    #[test]
    fn a_junction_or_directory_link_in_the_place_of_a_backup_or_its_wal_is_refused_without_needing_file_symlink_privileges() {
        let (_d, base) = root();
        let backups = base.join("backups");
        backup(&backups, 5, "20261206T000000Z", CLEAN5);
        let outside_dir = base.join("outside-dir");
        fs::create_dir_all(&outside_dir).unwrap();
        fs::write(outside_dir.join("leak.txt"), r"C:\Users\OtherUser\Clients\AcmeCorp").unwrap();
        let wal = backups.join("atm-v5-20261206T000000Z.sqlite-wal");
        dir_link(&wal, &outside_dir);
        let main_link = backups.join("atm-v5-20261207T000000Z.sqlite");
        dir_link(&main_link, &outside_dir);
        let e = entry_of(&backups, "20261206");
        assert_eq!(e.privacy_status, "not_assessable", "{:?}", e.layers);
        assert_eq!((layer_of(&e, "raw_wal").outcome, layer_of(&e, "raw_wal").reason.as_str()), (LayerOutcome::Unreadable, "redirected"));
        assert_eq!(e.raw_path_like_matches, 0);
        let m = entry_of(&backups, "20261207");
        assert_nothing_from_outside(&m);
        remove_dir_link(&wal);
        remove_dir_link(&main_link);
    }

    #[test]
    fn a_junction_or_symlink_as_the_backup_folder_or_as_any_parent_of_it_is_refused() {
        let (_d, base) = root();
        let real = base.join("real");
        let outside_backups = real.join("backups");
        backup(&outside_backups, 5, "20261203T000000Z", CLEAN5);
        // the folder itself
        let link = base.join("backups-link");
        dir_link(&link, &outside_backups);
        let (entries, _, state) = inventory_with(&link, ScanLimits::default());
        assert_eq!((state, entries.len()), ("refused_redirect", 0));
        // a parent of the folder
        let parent_link = base.join("parent-link");
        dir_link(&parent_link, &real);
        let (entries, _, state) = inventory_with(&parent_link.join("backups"), ScanLimits::default());
        assert_eq!((state, entries.len()), ("refused_redirect", 0));
        let live = live_db(&base);
        let plan = cleanup_plan(&live, &parent_link.join("backups")).unwrap();
        assert!(plan.entries.is_empty() && plan.notes.iter().any(|n| n.contains("klasörünün kendisi")));
        // control: the real path works
        assert_eq!(inventory_with(&outside_backups, ScanLimits::default()).2, "ok");
        remove_dir_link(&link);
        remove_dir_link(&parent_link);
    }

    #[test]
    fn a_directory_or_special_entry_named_like_a_backup_is_not_assessable_not_opened() {
        let (_d, base) = root();
        let backups = base.join("backups");
        fs::create_dir_all(backups.join("atm-v5-20261204T000000Z.sqlite")).unwrap();
        let e = entry_of(&backups, "20261204");
        assert_eq!(e.privacy_status, "not_assessable");
    }

    #[test]
    fn a_backup_swapped_for_a_link_between_listing_and_opening_is_refused() {
        let (_d, base) = root();
        let backups = base.join("backups");
        backup(&backups, 5, "20261205T000000Z", CLEAN5);
        let outside = outside_sentinel(&base);
        let probe = base.join("probe-link");
        if !file_link(&probe, &outside) {
            eprintln!("file symlinks need a privilege that is not available here; case not exercised");
            return;
        }
        let _ = fs::remove_file(&probe);
        let target = backups.join("atm-v5-20261205T000000Z.sqlite");
        let (t, o) = (target.clone(), outside.clone());
        // After the folder was listed and just before the confined open of the main file, the name becomes a link.
        fsx::hook::set(move |stage| {
            if stage == "before-open" && t.exists() && fs::symlink_metadata(&t).map(|m| !m.file_type().is_symlink()).unwrap_or(false) {
                fs::remove_file(&t).unwrap();
                assert!(file_link(&t, &o));
            }
        });
        let e = entry_of(&backups, "20261205");
        fsx::hook::clear();
        assert_nothing_from_outside(&e);
    }

    // ---- RT-2C-01: transactions that live only in the WAL ----------------------------------------------------------------

    const WAL_SECRET: &str = "token=ghp_SYNTHETICSYNTHETICSYNTHETICSYNTH12";

    /// A real WAL-mode database whose last transaction is committed ONLY in the `-wal` file (auto-checkpoint off, the
    /// writer connection stays open while the two files are copied). Returns the backup's base name.
    fn wal_only_backup(backups: &Path, stamp: &str, committed_value: &str) -> String {
        fs::create_dir_all(backups).unwrap();
        let (_scratch, base) = root();
        let src = base.join("src.sqlite");
        let conn = Connection::open(&src).unwrap();
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(())).unwrap();
        conn.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        conn.execute_batch("CREATE TABLE notes (id INTEGER PRIMARY KEY, v TEXT); INSERT INTO notes (v) VALUES ('plain words');").unwrap();
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())).unwrap();
        conn.execute("INSERT INTO notes (v) VALUES (?1)", [committed_value]).unwrap();
        let name = format!("atm-v5-{stamp}.sqlite");
        fs::copy(&src, backups.join(&name)).unwrap();
        fs::copy(base.join("src.sqlite-wal"), backups.join(format!("{name}-wal"))).unwrap();
        drop(conn);
        // the fixture really is what the tests claim: the value is in the WAL and NOT in the main file
        assert!(!contains(&fs::read(backups.join(&name)).unwrap(), committed_value.as_bytes()), "the main file must not hold the value");
        assert!(
            contains(&fs::read(backups.join(format!("{name}-wal"))).unwrap(), committed_value.as_bytes()),
            "the WAL must hold the value"
        );
        name
    }

    #[test]
    fn a_credential_committed_only_to_the_wal_is_never_reported_as_a_complete_or_clean_logical_inspection() {
        let (_d, base) = root();
        let backups = base.join("backups");
        wal_only_backup(&backups, "20270101T000000Z", &format!("note {WAL_SECRET}"));
        let e = entry_of(&backups, "20270101");
        // the logical (SQL) layers read the main file image and cannot see the WAL-only row ...
        assert_eq!((e.other_text_findings, e.tables_with_findings.clone()), (0, Vec::<String>::new()), "{e:?}");
        // ... and the entry says so instead of claiming complete coverage
        let wal_sql = layer_of(&e, "wal_logical_sql");
        assert_eq!((wal_sql.outcome, wal_sql.reason.as_str()), (LayerOutcome::Partial, "wal_not_replayed"), "{:?}", e.layers);
        assert_eq!(layer_of(&e, "raw_wal").outcome, LayerOutcome::Completed, "the raw pattern scan is a different layer");
        assert_eq!(e.coverage, "partial");
        assert_ne!(e.privacy_status, "none_detected_in_examined_content");
        // the bounded raw WAL scan still finds the credential-shaped bytes, so it is a reported finding
        assert_eq!(e.raw_sensitive_shape_layers, 1);
        assert_eq!(e.privacy_status, "violations_observed");
        assert!(e.has_wal_sidecar);
    }

    #[test]
    fn a_clean_wal_with_frames_is_partially_scanned_never_none_detected() {
        let (_d, base) = root();
        let backups = base.join("backups");
        wal_only_backup(&backups, "20270102T000000Z", "more plain words");
        let e = entry_of(&backups, "20270102");
        assert_eq!(e.privacy_status, "partially_scanned_no_findings", "{:?}", e.layers);
        assert_eq!(e.coverage, "partial");
        assert_eq!(layer_of(&e, "wal_logical_sql").reason, "wal_not_replayed");
        let live = live_db(&base);
        let plan = cleanup_plan(&live, &backups).unwrap();
        assert!(plan.no_violation_detected.is_empty());
        assert_eq!(plan.partially_scanned, vec![e.file_name.clone()]);
        assert!(plan.notes.iter().any(|n| n.contains("SQL satırları olarak İNCELENMEDİ")), "{:?}", plan.notes);
    }

    #[test]
    fn an_absent_or_empty_or_header_only_wal_does_not_lower_the_logical_coverage() {
        let (_d, base) = root();
        let backups = base.join("backups");
        backup(&backups, 5, "20270103T000000Z", CLEAN5);
        let none = entry_of(&backups, "20270103");
        assert_eq!(
            (layer_of(&none, "wal_logical_sql").outcome, layer_of(&none, "wal_logical_sql").reason.as_str()),
            (LayerOutcome::NotPresent, "no_sidecar")
        );
        assert_eq!(none.privacy_status, "none_detected_in_examined_content");
        assert_eq!(none.coverage, "complete");
        // an empty sidecar
        let empty = backups.join("atm-v5-20270103T000000Z.sqlite-wal");
        fs::write(&empty, b"").unwrap();
        let e = entry_of(&backups, "20270103");
        assert_eq!(
            (layer_of(&e, "wal_logical_sql").outcome, layer_of(&e, "wal_logical_sql").reason.as_str()),
            (LayerOutcome::Completed, "wal_empty")
        );
        assert_eq!(e.privacy_status, "none_detected_in_examined_content");
        // a bare, valid 32-byte header without any frame
        let mut head = vec![0u8; 32];
        head[..4].copy_from_slice(&0x377f_0682u32.to_be_bytes());
        head[4..8].copy_from_slice(&3_007_000u32.to_be_bytes());
        fs::write(&empty, &head).unwrap();
        let e = entry_of(&backups, "20270103");
        assert_eq!(layer_of(&e, "wal_logical_sql").reason, "wal_no_frames");
        assert_eq!(e.privacy_status, "none_detected_in_examined_content");
    }

    #[test]
    fn a_truncated_or_foreign_wal_is_never_trusted_to_be_empty() {
        let (_d, base) = root();
        let backups = base.join("backups");
        let name = wal_only_backup(&backups, "20270104T000000Z", "more plain words");
        let wal = backups.join(format!("{name}-wal"));
        let full = fs::read(&wal).unwrap();
        assert!(full.len() > 40);
        // cut inside the header
        fs::write(&wal, &full[..20]).unwrap();
        let cut_header = entry_of(&backups, "20270104");
        assert_eq!(layer_of(&cut_header, "wal_logical_sql").reason, "wal_header_unrecognised");
        assert_eq!(cut_header.privacy_status, "partially_scanned_no_findings");
        // cut inside the first frame (valid header, partial frame)
        fs::write(&wal, &full[..40]).unwrap();
        let cut_frame = entry_of(&backups, "20270104");
        assert_eq!(layer_of(&cut_frame, "wal_logical_sql").reason, "wal_not_replayed");
        assert_eq!(cut_frame.privacy_status, "partially_scanned_no_findings");
        // foreign bytes
        fs::write(&wal, vec![b'x'; 400]).unwrap();
        let foreign = entry_of(&backups, "20270104");
        assert_eq!(layer_of(&foreign, "wal_logical_sql").reason, "wal_header_unrecognised");
        assert_ne!(foreign.privacy_status, "none_detected_in_examined_content");
        // a truncated WAL that still contains the credential bytes is still found by the raw pattern scan
        let (_d2, base2) = root();
        let backups2 = base2.join("backups");
        let name2 = wal_only_backup(&backups2, "20270105T000000Z", &format!("note {WAL_SECRET}"));
        let wal2 = backups2.join(format!("{name2}-wal"));
        let full2 = fs::read(&wal2).unwrap();
        fs::write(&wal2, &full2[..full2.len() - 100]).unwrap();
        let partial_frame = entry_of(&backups2, "20270105");
        assert_ne!(partial_frame.privacy_status, "none_detected_in_examined_content");
    }

    #[test]
    fn an_unreadable_wal_makes_the_backup_not_assessable_in_both_wal_layers() {
        let (_d, base) = root();
        let backups = base.join("backups");
        backup(&backups, 5, "20270106T000000Z", CLEAN5);
        // a directory where the sidecar should be: cannot be opened as a regular file
        fs::create_dir_all(backups.join("atm-v5-20270106T000000Z.sqlite-wal")).unwrap();
        let e = entry_of(&backups, "20270106");
        for l in ["raw_wal", "wal_logical_sql"] {
            assert_eq!(layer_of(&e, l).outcome, LayerOutcome::Unreadable, "{l}: {:?}", e.layers);
        }
        assert_eq!(e.privacy_status, "not_assessable");
        assert_eq!(e.coverage, "incomplete");
    }

    #[test]
    fn inspecting_a_wal_backup_never_writes_checkpoints_or_creates_files() {
        let (_d, base) = root();
        let backups = base.join("backups");
        let name = wal_only_backup(&backups, "20270108T000000Z", &format!("note {WAL_SECRET}"));
        let snapshot = |dir: &Path| {
            let mut v: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
                .unwrap()
                .flatten()
                .map(|f| (f.file_name().to_string_lossy().into_owned(), fs::read(f.path()).unwrap()))
                .collect();
            v.sort();
            v
        };
        let before = snapshot(&backups);
        let e = entry_of(&backups, "20270108");
        let _ = cleanup_plan(&live_db(&base), &backups).unwrap();
        assert_eq!(snapshot(&backups), before, "no backup byte changed, nothing (-shm, -journal, checkpoint) was created: {name}");
        assert_eq!(e.privacy_status, "violations_observed");
    }
}
