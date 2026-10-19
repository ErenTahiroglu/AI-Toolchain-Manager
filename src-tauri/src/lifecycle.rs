//! Phase 3A: the SYNTHETIC transactional lifecycle engine.
//!
//! This module proves that install / update / uninstall / rollback / crash recovery can be done honestly and safely — on
//! isolated, synthetic test fixtures only. It is deliberately incapable of anything else:
//!
//! * it works inside one [`SandboxRoot`]: a freshly created directory named `atm-synthetic-<uuid>` that carries a marker file.
//!   Every path is built from validated single components below that root; no caller can name a destination;
//! * it accepts only a [`ValidatedRecipe`] that is marked `synthetic` with a `synthetic-` tool id, together with a
//!   [`VerifiedApproval`] bound to that recipe, that computer, that platform and that action, still inside its lifetime;
//! * the "artifact" is a byte buffer handed in by the caller. Nothing is downloaded, no program is started, no shell, no
//!   package manager, no PATH change, no agent configuration, no user project;
//! * it is **not** wired to any command, button or IPC function. The only callers are tests and the headless self-check.
//!
//! Layout inside a sandbox: `staging/<txn>/`, `versions/<tool>/<version>/artifact.bin`, `state/<tool>.json` (the active-version
//! pointer, switched by an atomic rename), `journal/<txn>.json` (write-ahead journal), `receipts/<txn>.json`,
//! `snapshots/<txn>/`, `locks/<tool>.lock` and `data/<tool>/` (the tool's own user data, which no operation ever touches).

#![allow(dead_code)] // the synthetic lifecycle is exercised by tests and the self-check only; no command reaches it

use crate::{
    approval::{Action, VerifiedApproval},
    audit::{canonical_json, sha256_hex},
    discovery::reference::{self, ArtifactFormat, MatchResult, ObservedArtifact, ReferenceSet},
    recipe::ValidatedRecipe,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

pub const SANDBOX_PREFIX: &str = "atm-synthetic-";
const MARKER: &str = ".atm-synthetic-root";
const MARKER_MAGIC: &str = "atm-synthetic-root-v1";
const SUBDIRS: &[&str] = &["staging", "versions", "state", "journal", "receipts", "snapshots", "locks", "data"];

// ---------------------------------------------------------------------------------------------------------------------
// Errors, fault points
// ---------------------------------------------------------------------------------------------------------------------

/// Where a test may inject a failure or a crash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Point {
    AfterLock,
    /// Snapshot pointer file: intent journalled; exclusive create not yet done.
    SnapshotBeforeCreate,
    /// ... file exists, its identity is not yet in the journal (the irreducible window).
    SnapshotAfterCreate,
    /// ... half of the data is written.
    SnapshotDuringWrite,
    /// ... data is on disk (fsync done), the final record is not.
    SnapshotAfterSync,
    /// ... the final record is durable.
    SnapshotAfterRecord,
    AfterSnapshot,
    /// New pointer file (written next to the active one, then renamed over it) and the receipt file: the same discipline.
    PointerBeforeCreate,
    PointerAfterCreate,
    PointerAfterSync,
    ReceiptAfterCreate,
    ReceiptAfterSync,
    /// Staged artifact: the same five steps.
    StageBeforeCreate,
    StageAfterCreate,
    StageAfterSync,
    DuringStageWrite,
    AfterStageWrite,
    AfterVerify,
    BeforeVersionDirCreate,
    AfterVersionDirCreate,
    BeforePointerWrite,
    AfterPointerSwitch,
    BeforeReceiptWrite,
    AfterReceiptWrite,
    BeforeJournalCommit,
    UninstallAfterPointerMoved,
    UninstallAfterVersionsMoved,
    RollbackPointerRestore,
    RollbackRemoveVersionDir,
    RollbackRemoveStaging,
    RollbackRestoreRemoved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// The operation fails with an error; the engine then rolls back.
    Fail,
    /// The process "dies" here: the engine returns immediately and cleans up nothing.
    Crash,
}

#[derive(Debug)]
pub enum TxnError {
    /// A precondition failed before anything was changed.
    Refused(&'static str),
    /// Another transaction holds the tool.
    Busy,
    /// An unfinished transaction exists; recover first.
    Pending(String),
    /// The operation failed; what rollback did is stated honestly.
    Failed { code: &'static str, rollback: RollbackReport },
    /// A simulated crash at this point: nothing was cleaned up.
    Interrupted(Point),
}

// ---------------------------------------------------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotUndone {
    pub what: String,
    pub why: String,
}

/// What rollback did. It lists what it restored, what it could NOT undo and why, and what lies outside its reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RollbackReport {
    /// `not_needed` | `restored` | `partial` | `failed`
    pub status: String,
    pub undone: Vec<String>,
    pub not_undone: Vec<NotUndone>,
    /// Things rollback never covers by design (for example the tool's own user data).
    pub not_covered: Vec<String>,
}

impl RollbackReport {
    pub(crate) fn not_needed() -> RollbackReport {
        RollbackReport { status: "not_needed".to_string(), undone: vec![], not_undone: vec![], not_covered: vec![] }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub schema_version: u32,
    pub txn_id: String,
    pub action: Action,
    pub tool_id: String,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub artifact_sha256: Option<String>,
    pub recipe_digest: String,
    pub approval_id: String,
    pub platform_target: String,
    pub computer_id: String,
    /// True for a synthetic recipe (and always in the path-based engine); false only for a real recipe that the build was explicitly enabled to apply.
    pub synthetic: bool,
    pub completed_at: String,
    /// Always `committed`.
    pub state: String,
    /// Things deliberately left alone (the tool's user data).
    pub kept: Vec<String>,
    pub rollback: RollbackReport,
    pub receipt_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxnState {
    Started,
    Snapshotted,
    Staged,
    Verified,
    Activated,
    Committed,
    RolledBack,
    RollbackFailed,
}

impl TxnState {
    pub(crate) fn terminal(self) -> bool {
        matches!(self, TxnState::Committed | TxnState::RolledBack)
    }
}

/// The three durable stages of a file the transaction creates. Intent is journalled BEFORE the file exists, the observed identity
/// right after the exclusive create and BEFORE any data is written, the finalized content identity after the data is on disk.
/// A file that exists where only an intent is recorded is an AMBIGUOUS ORPHAN: the crash happened between `create_new` and the
/// durable identity record (an irreducible window), so ownership cannot be proven and the file is never deleted on a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Intended,
    Created,
    Final,
}

fn final_phase() -> Phase {
    Phase::Final
}

/// A file this transaction created (or, for an uninstall, verified and moved), as recorded in the journal BEFORE it is needed.
/// Cleanup deletes a file only if it matches its record: still the same file (device/volume + file index), a single link, and --
/// when the content is known -- the same size and SHA-256. A different file under an expected name is foreign, whatever it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnedFile {
    pub role: String,
    /// How far the creation got, as far as the journal knows (see [`Phase`]).
    #[serde(default = "final_phase")]
    pub phase: Phase,
    /// Every sandbox-relative place the file may legitimately be at (a rename in flight is written ahead).
    pub rels: Vec<String>,
    /// `None` while the content is still being written (a partial file can only be recognised by identity).
    pub sha256: Option<String>,
    pub size: Option<u64>,
    pub identity: Option<[u64; 2]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Journal {
    pub schema_version: u32,
    pub txn_id: String,
    pub action: Action,
    pub tool_id: String,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub artifact_sha256: Option<String>,
    pub recipe_digest: String,
    pub approval_id: String,
    pub platform_target: String,
    pub computer_id: String,
    pub state: TxnState,
    /// Write-ahead intents: recorded BEFORE the side effect they describe.
    pub intents: Vec<String>,
    pub started_at: String,
    pub rollback: Option<RollbackReport>,
    /// The manifest of files this transaction owns (see [`OwnedFile`]).
    #[serde(default)]
    pub owned: Vec<OwnedFile>,
    pub journal_digest: String,
}

fn journal_digest(j: &Journal) -> String {
    let mut blank = j.clone();
    blank.journal_digest = String::new();
    sha256_hex(&canonical_json(&serde_json::to_value(&blank).unwrap_or(serde_json::Value::Null)))
}

pub(crate) fn receipt_digest(r: &Receipt) -> String {
    let mut blank = r.clone();
    blank.receipt_digest = String::new();
    sha256_hex(&canonical_json(&serde_json::to_value(&blank).unwrap_or(serde_json::Value::Null)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub receipt: Receipt,
    /// True when the desired state already held and nothing was changed (idempotent repeat).
    pub already_applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recovered {
    /// An unfinished transaction was undone.
    RolledBack(String),
    /// The work was complete except for the final bookkeeping; it was finished.
    RolledForward(String),
    /// Rollback could not undo everything; a person has to look. The lock stays.
    NeedsAttention(String, RollbackReport),
    /// A journal that fails its own digest or cannot be read. Nothing was changed.
    CorruptJournal(String),
    /// A lock without any journal (a crash right after locking): removed, nothing had been changed.
    OrphanLockRemoved(String),
    /// The transaction is finished, but something this module did not create is still in its staging or snapshot folder.
    /// It was left untouched (nothing is deleted on a guess); the reason is a short description without names.
    LeftoversRetained(String, String),
}

// ---------------------------------------------------------------------------------------------------------------------
// The sandbox
// ---------------------------------------------------------------------------------------------------------------------

fn is_link(p: &Path) -> bool {
    let Ok(m) = fs::symlink_metadata(p) else { return false };
    if m.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT: junctions and other redirections
        if m.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

/// A single path component: ASCII letters, digits and `. _ - +`, starting with a letter or digit, no `..`, not a Windows
/// reserved device name.
pub fn safe_component(c: &str) -> bool {
    let b = c.as_bytes();
    if b.is_empty() || b.len() > 64 || !b[0].is_ascii_alphanumeric() || c.contains("..") || c.ends_with('.') {
        return false;
    }
    if !b.iter().all(|x| x.is_ascii_alphanumeric() || matches!(x, b'.' | b'_' | b'-' | b'+')) {
        return false;
    }
    let stem = c.split('.').next().unwrap_or("").to_ascii_uppercase();
    !matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

#[derive(Debug, Clone)]
pub struct SandboxRoot {
    path: PathBuf,
    computer_id: String,
}

impl SandboxRoot {
    /// Creates a NEW sandbox directly below `base` (an application-owned or temporary directory that already exists and is not a
    /// link). It never adopts an existing directory.
    pub fn create(base: &Path, computer_id: &str) -> Result<SandboxRoot, &'static str> {
        if !safe_component(computer_id) {
            return Err("bad_computer_id");
        }
        if !base.is_dir() || is_link(base) {
            return Err("base_not_a_plain_directory");
        }
        let root_id = uuid::Uuid::new_v4().simple().to_string();
        let path = base.join(format!("{SANDBOX_PREFIX}{root_id}"));
        fs::create_dir(&path).map_err(|_| "cannot_create_sandbox")?;
        let marker = serde_json::json!({ "magic": MARKER_MAGIC, "computerId": computer_id, "rootId": root_id }).to_string();
        fs::write(path.join(MARKER), marker).map_err(|_| "cannot_write_marker")?;
        for d in SUBDIRS {
            fs::create_dir(path.join(d)).map_err(|_| "cannot_create_layout")?;
        }
        Ok(SandboxRoot { path, computer_id: computer_id.to_string() })
    }

    /// Re-opens a sandbox this module created (after a "restart"). Anything without the marker, with a different name pattern or
    /// that is a link is refused.
    pub fn open(path: &Path) -> Result<SandboxRoot, &'static str> {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with(SANDBOX_PREFIX) || !safe_component(name) {
            return Err("not_a_sandbox_name");
        }
        if !path.is_dir() || is_link(path) {
            return Err("not_a_plain_directory");
        }
        let marker: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path.join(MARKER)).map_err(|_| "no_marker")?).map_err(|_| "bad_marker")?;
        if marker.get("magic").and_then(|v| v.as_str()) != Some(MARKER_MAGIC) {
            return Err("bad_marker");
        }
        let computer_id = marker.get("computerId").and_then(|v| v.as_str()).filter(|c| safe_component(c)).ok_or("bad_marker")?.to_string();
        // The marker names the folder it was created for: a marker copied into another folder does not carry over.
        if marker.get("rootId").and_then(|v| v.as_str()) != Some(&name[SANDBOX_PREFIX.len()..]) {
            return Err("marker_does_not_belong_to_this_folder");
        }
        // A sandbox has a closed top level: the marker and the fixed folders, nothing else. A populated user folder that merely
        // received a marker file and the right folder names is therefore not adopted.
        let mut seen_top = 0;
        for e in fs::read_dir(path).map_err(|_| "unreadable")?.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n != MARKER && !SUBDIRS.contains(&n.as_str()) {
                return Err("unexpected_top_level_entry");
            }
            seen_top += 1;
        }
        if seen_top != SUBDIRS.len() + 1 {
            return Err("layout_damaged_or_redirected");
        }
        let root = SandboxRoot { path: path.to_path_buf(), computer_id };
        for d in SUBDIRS {
            let p = root.path.join(d);
            if !p.is_dir() || is_link(&p) {
                return Err("layout_damaged_or_redirected");
            }
        }
        Ok(root)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn computer_id(&self) -> &str {
        &self.computer_id
    }

    /// `root/<top>/<components…>`: every component validated, nothing on the way may be a link.
    fn join(&self, top: &str, comps: &[&str]) -> Result<PathBuf, TxnError> {
        if !SUBDIRS.contains(&top) || comps.iter().any(|c| !safe_component(c)) {
            return Err(TxnError::Refused("unsafe_path_component"));
        }
        let mut p = self.path.join(top);
        if is_link(&self.path) || is_link(&p) {
            return Err(TxnError::Refused("sandbox_redirected"));
        }
        for c in comps {
            p.push(c);
            if is_link(&p) {
                return Err(TxnError::Refused("sandbox_redirected"));
            }
        }
        debug_assert!(p.starts_with(&self.path));
        Ok(p)
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Durable file helpers
// ---------------------------------------------------------------------------------------------------------------------

fn sync_dir(p: &Path) {
    #[cfg(unix)]
    {
        if let Ok(d) = fs::File::open(p) {
            let _ = d.sync_all();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = p;
    }
}

/// Write to a temporary sibling, flush to disk, then rename over the target: readers see the old or the new content, never half.
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    let _ = fs::remove_file(&tmp);
    let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    if let Some(parent) = path.parent() {
        sync_dir(parent);
    }
    Ok(())
}

fn io<E>(_: E) -> TxnError {
    TxnError::Refused("io_error")
}

// ---------------------------------------------------------------------------------------------------------------------
// Conservative removal: only what this module created, in the shape it created it
// ---------------------------------------------------------------------------------------------------------------------

/// A removal that tolerates the short delays Windows shows while a scanner or indexer still holds a handle on something that
/// was just closed. It retries only the same single, non-recursive operation; it never widens it.
fn retry_io<T>(mut op: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut last = None;
    for attempt in 0..8 {
        match op() {
            Ok(v) => return Ok(v),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(e),
            Err(e) => last = Some(e),
        }
        if attempt < 7 {
            std::thread::sleep(std::time::Duration::from_millis(25 * (attempt + 1) as u64));
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("removal failed")))
}

/// Test builds only: why an entry was classified "other" (raw operating-system error of the metadata query), so an intermittent failure of
/// a lifecycle test can show what the engine actually saw at that moment. No effect on production builds.
#[cfg(test)]
pub(crate) static OTHER_LOG: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// What one directory entry is right now. `None` means the entry NO LONGER EXISTS when it is inspected: on Windows a file that was just
/// deleted can still be listed for a moment (observed as a renamed temporary name such as `ARTIFACT.BIN.PART.tmp` whose metadata query
/// answers "not found", Phase 3D-1 closure investigation) while another program still holds a handle. Something that is not there is
/// neither unexpected content to protect nor a reason to refuse the cleanup; every OTHER failure to inspect an entry stays "other",
/// i.e. unknown and never deleted.
fn entry_state(p: &Path) -> Option<&'static str> {
    if is_link(p) {
        return Some("link");
    }
    // A file whose deletion is still pending answers a metadata query with "access denied" (reproduced in a test) or "not found" while it
    // is still listed. Those states end within milliseconds, so the same read-only query is repeated a few times (about 0.3 s in total)
    // before an entry is given up as unknown; an entry that stays unreadable is still "other" and is never deleted.
    let mut result = fs::symlink_metadata(p);
    for attempt in 0..5u32 {
        match &result {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                std::thread::sleep(std::time::Duration::from_millis(10 << attempt));
                result = fs::symlink_metadata(p);
            }
            _ => break,
        }
    }
    match result {
        Ok(m) if m.is_file() => Some("file"),
        Ok(m) if m.is_dir() => Some("folder"),
        other => {
            #[cfg(test)]
            if let Ok(mut log) = OTHER_LOG.lock() {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                log.push(match &other {
                    Ok(_) => format!("{name}: metadata ok but neither file nor folder"),
                    Err(e) => format!("{name}: metadata ERROR raw_os_error={:?} kind={:?}", e.raw_os_error(), e.kind()),
                });
            }
            match other {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                _ => Some("other"),
            }
        }
    }
}

fn entry_kind(p: &Path) -> &'static str {
    entry_state(p).unwrap_or("other")
}

/// After a removal reported success: wait (bounded, about 1 s) until the name is really gone. Windows can keep listing a deleted file
/// while another program holds a handle on it; "removed" should mean the name no longer exists when the removal is reported.
fn wait_until_gone(p: &Path) -> bool {
    for attempt in 0..8u32 {
        if matches!(fs::symlink_metadata(p), Err(e) if e.kind() == std::io::ErrorKind::NotFound) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(15 * (attempt as u64 + 1)));
    }
    matches!(fs::symlink_metadata(p), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
}

fn list_entries(dir: &Path) -> Result<Vec<(String, &'static str)>, String> {
    let mut v: Vec<(String, &'static str)> = fs::read_dir(dir)
        .map_err(|e| format!("cannot list the folder: {e}"))?
        .flatten()
        .filter_map(|e| entry_state(&e.path()).map(|kind| (e.file_name().to_string_lossy().into_owned(), kind)))
        .collect();
    v.sort();
    Ok(v)
}

fn unexpected_description(unexpected: &[&(String, &'static str)]) -> String {
    let count = |k: &str| unexpected.iter().filter(|(_, kind)| *kind == k).count();
    format!(
        "{} unexpected entr{} ({} file(s), {} folder(s), {} link(s), {} other) were left untouched; nothing was deleted",
        unexpected.len(),
        if unexpected.len() == 1 { "y" } else { "ies" },
        count("file"),
        count("folder"),
        count("link"),
        count("other")
    )
}

/// Device/volume and file index of an OPEN file, and its number of hard links. This is the file's identity: a different file
/// with the same name has a different one. (Windows: `GetFileInformationByHandle`; Unix: `st_dev`, `st_ino`, `st_nlink`.)
#[cfg(unix)]
fn identity_and_links(f: &fs::File) -> Option<([u64; 2], u64)> {
    use std::os::unix::fs::MetadataExt;
    let m = f.metadata().ok()?;
    Some(([m.dev(), m.ino()], m.nlink()))
}

#[cfg(windows)]
fn identity_and_links(f: &fs::File) -> Option<([u64; 2], u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
    // SAFETY: `info` is plain data that the call fills in; the handle belongs to the open `File` borrowed for the whole call.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(f.as_raw_handle() as _, &mut info) };
    if ok == 0 {
        return None;
    }
    Some((
        [u64::from(info.dwVolumeSerialNumber), (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow)],
        u64::from(info.nNumberOfLinks),
    ))
}

fn sha256_of_open_file(f: &mut fs::File, limit: u64) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = f.read(&mut buf).map_err(|e| format!("cannot read a recorded file to check it: {e}"))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > limit {
            return Err("a recorded file is larger than any file this module writes".to_string());
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn rel_of(root: &Path, p: &Path) -> Option<String> {
    let rel = p.strip_prefix(root).ok()?;
    Some(rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"))
}

#[derive(Debug)]
enum Verify {
    /// No record says this transaction created a file here.
    Unrecorded,
    /// A record exists, but what is there now is not that file.
    Changed(String),
    /// Only an intent is recorded and something exists at that place: it may be this transaction's file (crash between create and
    /// record) or a foreign one that made the create fail. Not provable, therefore never deleted.
    Ambiguous,
}

const VERIFY_LIMIT: u64 = 1 << 30;

/// Does `path` hold exactly the file the journal says this transaction created? Same identity (never just the same name), one
/// hard link, a regular file (not a link), and -- if the content was recorded -- the same size and SHA-256 read through the very
/// handle whose identity was checked.
fn verify_owned(root: &Path, path: &Path, owned: &[OwnedFile]) -> Result<(), Verify> {
    let rel = rel_of(root, path).ok_or(Verify::Unrecorded)?;
    let entry = owned.iter().find(|e| e.rels.contains(&rel)).ok_or(Verify::Unrecorded)?;
    let changed = |why: &str| Verify::Changed(why.to_string());
    if entry_kind(path) != "file" {
        return Err(changed("not a plain file any more"));
    }
    if entry.phase == Phase::Intended {
        return Err(Verify::Ambiguous);
    }
    let mut f = fs::File::open(path).map_err(|_| changed("cannot be opened"))?;
    let (identity, links) = identity_and_links(&f).ok_or_else(|| changed("identity cannot be read"))?;
    if links != 1 {
        return Err(changed("has other hard links"));
    }
    if entry.identity != Some(identity) {
        return Err(changed("is a different file than the one this transaction created"));
    }
    if entry.phase == Phase::Created {
        return Ok(()); // identity and link count are all that is known while the data is still being written
    }
    if let Some(size) = entry.size {
        if f.metadata().map(|m| m.len()).ok() != Some(size) {
            return Err(changed("has another size"));
        }
    }
    if let Some(sha) = &entry.sha256 {
        if sha256_of_open_file(&mut f, VERIFY_LIMIT).map_err(Verify::Changed)? != *sha {
            return Err(changed("has other content"));
        }
    }
    Ok(())
}

/// Why nothing was deleted, without any name.
fn refusal(unexpected: &[&(String, &'static str)], changed: usize, ambiguous: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    if ambiguous > 0 {
        parts.push(format!(
            "{ambiguous} ambiguous orphan file(s): a crash left a file where this transaction only recorded the intent to create one, so it cannot be proven to be this transaction's; it was left untouched and nothing was deleted"
        ));
    }
    if !unexpected.is_empty() {
        parts.push(unexpected_description(unexpected));
    }
    if changed > 0 {
        parts.push(format!(
            "{changed} recorded file(s) are no longer the file this transaction wrote (identity, links or content differ); they were left untouched and nothing was deleted"
        ));
    }
    parts.join("; ")
}

/// Checks every entry of `dir` against the journal's manifest; returns the files to delete, or why nothing may be deleted.
fn owned_files_of(root: &Path, dir: &Path, owned: &[OwnedFile]) -> Result<Vec<PathBuf>, String> {
    let entries = list_entries(dir)?;
    let mut unexpected: Vec<&(String, &'static str)> = Vec::new();
    let mut changed = 0usize;
    let mut ambiguous = 0usize;
    let mut files = Vec::new();
    for e in &entries {
        if e.1 != "file" {
            unexpected.push(e);
            continue;
        }
        let path = dir.join(&e.0);
        match verify_owned(root, &path, owned) {
            Ok(()) => files.push(path),
            Err(Verify::Unrecorded) => unexpected.push(e),
            Err(Verify::Changed(_)) => changed += 1,
            Err(Verify::Ambiguous) => ambiguous += 1,
        }
    }
    if !unexpected.is_empty() || changed > 0 || ambiguous > 0 {
        return Err(refusal(&unexpected, changed, ambiguous));
    }
    Ok(files)
}

/// Removes the files in `dir` that the journal records as created by this transaction -- each verified by identity (and content
/// where known) -- and then `dir` itself WITHOUT recursion. All or nothing: if anything else is present (an unrecorded file, a
/// folder, a link of any kind) or a recorded file was replaced, nothing is deleted. Links are never followed. A window remains
/// between the check and the unlink; the removal is `remove_file` of the verified name (a link that replaced it would be removed
/// itself, never its target) and the final `remove_dir` fails if anything else appeared.
fn remove_verified_dir(root: &Path, dir: &Path, owned: &[OwnedFile]) -> Result<(), String> {
    let files = owned_files_of(root, dir, owned)?;
    for f in &files {
        retry_io(|| fs::remove_file(f)).map_err(|e| format!("cannot remove a file this module created: {e}"))?;
    }
    retry_io(|| fs::remove_dir(dir)).map_err(|e| format!("the folder is not empty or cannot be removed: {e}"))
}

/// Removes a transaction snapshot (`pointer.json`, and `versions/<version>/artifact.bin` after an uninstall). The whole structure
/// and every file are validated against the manifest first; any foreign entry, replaced file or hard-linked file anywhere keeps
/// everything in place.
fn remove_snapshot(root: &Path, dir: &Path, owned: &[OwnedFile]) -> Result<(), String> {
    let top = list_entries(dir)?;
    let foreign: Vec<&(String, &'static str)> =
        top.iter().filter(|(n, k)| !((n == "pointer.json" && *k == "file") || (n == "versions" && *k == "folder"))).collect();
    if !foreign.is_empty() {
        return Err(unexpected_description(&foreign));
    }
    let versions = dir.join("versions");
    let mut version_dirs: Vec<PathBuf> = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut unexpected: Vec<(String, &'static str)> = Vec::new();
    let mut changed = 0usize;
    let mut ambiguous = 0usize;
    let mut check = |path: PathBuf, entry: (String, &'static str)| match verify_owned(root, &path, owned) {
        Ok(()) => files.push(path),
        Err(Verify::Unrecorded) => unexpected.push(entry),
        Err(Verify::Changed(_)) => changed += 1,
        Err(Verify::Ambiguous) => ambiguous += 1,
    };
    if top.iter().any(|(n, _)| n == "pointer.json") {
        check(dir.join("pointer.json"), ("pointer.json".to_string(), "file"));
    }
    if top.iter().any(|(n, _)| n == "versions") {
        for (n, k) in list_entries(&versions)? {
            let vd = versions.join(&n);
            if k != "folder" || !safe_component(&n) {
                return Err(unexpected_description(&[&(n, k)]));
            }
            for (inner, ik) in list_entries(&vd)? {
                if inner == "artifact.bin" && ik == "file" {
                    check(vd.join("artifact.bin"), (inner, ik));
                } else {
                    return Err(unexpected_description(&[&(inner, ik)]));
                }
            }
            version_dirs.push(vd);
        }
    }
    if !unexpected.is_empty() || changed > 0 || ambiguous > 0 {
        let refs: Vec<&(String, &'static str)> = unexpected.iter().collect();
        return Err(refusal(&refs, changed, ambiguous));
    }
    for f in &files {
        retry_io(|| fs::remove_file(f)).map_err(|e| format!("cannot remove a file this module created: {e}"))?;
    }
    for vd in &version_dirs {
        retry_io(|| fs::remove_dir(vd)).map_err(|e| format!("cannot remove a version folder of the snapshot: {e}"))?;
    }
    if top.iter().any(|(n, _)| n == "versions") {
        retry_io(|| fs::remove_dir(&versions)).map_err(|e| format!("cannot remove the versions folder: {e}"))?;
    }
    retry_io(|| fs::remove_dir(dir)).map_err(|e| format!("the snapshot folder is not empty or cannot be removed: {e}"))
}

// ---------------------------------------------------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------------------------------------------------

pub type FaultFn<'a> = &'a (dyn Fn(Point) -> Option<Fault> + Sync);
pub type ClockFn<'a> = &'a (dyn Fn() -> DateTime<Utc> + Sync);

/// What a call is authorized by. The engine re-checks all of it itself.
pub struct Authority<'a> {
    pub recipe: &'a ValidatedRecipe,
    pub approval: &'a VerifiedApproval,
}

pub struct Engine<'a> {
    pub root: &'a SandboxRoot,
    pub refs: &'a ReferenceSet,
    /// The platform this (synthetic) machine is.
    pub host_platform: &'a str,
    pub clock: ClockFn<'a>,
    pub fault: Option<FaultFn<'a>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Pointer {
    pub(crate) tool: String,
    pub(crate) version: String,
    pub(crate) artifact_sha256: String,
    pub(crate) txn: String,
}

/// One REAL recipe that a build has been explicitly enabled to apply (Phase 3D-3): the exact tool, version, platform, reviewed recipe digest and
/// artifact digest. A release build's list is EMPTY, so no real recipe can be applied; enabling one is a reviewed change to that list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PilotRecipe {
    pub tool_id: String,
    pub version: String,
    pub platform_target: String,
    pub recipe_digest: String,
    pub artifact_sha256: String,
}

impl PilotRecipe {
    pub(crate) fn allows(&self, recipe: &ValidatedRecipe) -> bool {
        let r = recipe.recipe();
        !r.synthetic
            && self.tool_id == r.tool_id
            && self.version == r.version
            && self.platform_target == r.target.platform_target
            && self.recipe_digest == recipe.digest()
            && self.artifact_sha256 == r.artifact.sha256
    }
}

/// The synthetic-recipe and approval checks shared by the synthetic engine and the isolated test-mode adapter.
pub(crate) fn check_authority(
    a: &Authority,
    action: Action,
    computer_id: &str,
    host_platform: &str,
    now: DateTime<Utc>,
) -> Result<(), TxnError> {
    check_authority_gated(a, action, computer_id, host_platform, now, &[])
}

/// The same, with the list of real recipes this build may apply. Everything else is unchanged: a recipe that is neither synthetic nor listed is
/// refused with the same code as before.
pub(crate) fn check_authority_gated(
    a: &Authority,
    action: Action,
    computer_id: &str,
    host_platform: &str,
    now: DateTime<Utc>,
    gate: &[PilotRecipe],
) -> Result<(), TxnError> {
    let r = a.recipe.recipe();
    let ap = a.approval.approval();
    let synthetic_ok = r.synthetic && r.tool_id.starts_with("synthetic-");
    if !synthetic_ok && !gate.iter().any(|g| g.allows(a.recipe)) {
        return Err(TxnError::Refused("not_a_synthetic_recipe"));
    }
    if ap.action != action
        || ap.recipe_digest != a.recipe.digest()
        || ap.tool_id != r.tool_id
        || ap.version != r.version
        || ap.computer_id != computer_id
        || ap.platform_target != host_platform
        || r.target.platform_target != host_platform
    {
        return Err(TxnError::Refused("approval_does_not_match"));
    }
    if now >= a.approval.expires_at() {
        return Err(TxnError::Refused("approval_expired"));
    }
    Ok(())
}

/// The bytes must be exactly what the recipe names AND what the authenticated reference set vouches for on this platform.
pub(crate) fn check_artifact(refs: &ReferenceSet, host_platform: &str, a: &Authority, bytes: &[u8]) -> Result<String, TxnError> {
    check_artifact_for(refs, host_platform, a.recipe, bytes)
}

/// The same judgement without an approval: what a PREVIEW may check before anyone has confirmed anything.
pub(crate) fn check_artifact_for(
    refs: &ReferenceSet,
    host_platform: &str,
    recipe: &ValidatedRecipe,
    bytes: &[u8],
) -> Result<String, TxnError> {
    let r = recipe.recipe();
    let sha = sha256_hex_bytes(bytes);
    if bytes.len() as u64 != r.artifact.size_bytes || sha != r.artifact.sha256 {
        return Err(TxnError::Refused("artifact_does_not_match_recipe"));
    }
    let format = reference::sniff_reader(&bytes[..bytes.len().min(4096)], bytes.len() as u64, &mut Cursor::new(bytes));
    if format == ArtifactFormat::Unrecognised || format.as_str() != r.artifact.format {
        return Err(TxnError::Refused("artifact_format_mismatch"));
    }
    let observed = ObservedArtifact { entry_id: &r.tool_id, platform_target: host_platform, format, sha256: &sha };
    let v = reference::evaluate(&observed, refs);
    if v.result != MatchResult::MatchesAuthenticated || v.version.as_deref() != Some(r.version.as_str()) {
        return Err(TxnError::Refused("artifact_not_authenticated_for_this_platform"));
    }
    Ok(sha)
}

impl<'a> Engine<'a> {
    fn hit(&self, p: Point) -> Result<(), TxnError> {
        match self.fault.and_then(|f| f(p)) {
            Some(Fault::Crash) => Err(TxnError::Interrupted(p)),
            Some(Fault::Fail) => Err(TxnError::Refused("injected_failure")),
            None => Ok(()),
        }
    }

    fn rollback_hit(&self, p: Point) -> Result<(), String> {
        match self.fault.and_then(|f| f(p)) {
            Some(_) => Err("a simulated failure occurred while undoing this step".to_string()),
            None => Ok(()),
        }
    }

    fn now(&self) -> String {
        (self.clock)().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    // ---- authority and preconditions -------------------------------------------------------------------------------

    fn authorize(&self, a: &Authority, action: Action) -> Result<(), TxnError> {
        check_authority(a, action, &self.root.computer_id, self.host_platform, (self.clock)())
    }

    fn check_identity(&self, a: &Authority, bytes: &[u8]) -> Result<String, TxnError> {
        check_artifact(self.refs, self.host_platform, a, bytes)
    }

    fn read_pointer(&self, tool: &str) -> Result<Option<Pointer>, TxnError> {
        let p = self.root.join("state", &[&format!("{tool}.json")])?;
        match fs::read_to_string(&p) {
            Ok(text) => serde_json::from_str(&text).map(Some).map_err(|_| TxnError::Refused("pointer_unreadable")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(TxnError::Refused("pointer_unreadable")),
        }
    }

    /// Refuses early (before the installed state is even consulted) when another transaction holds the tool or left something
    /// unfinished for it: until that is resolved the state on disk is not trustworthy enough to call "already done".
    fn ensure_free(&self, tool: &str) -> Result<(), TxnError> {
        if self.root.join("locks", &[&format!("{tool}.lock")])?.exists() {
            return Err(TxnError::Busy);
        }
        match self.pending_for(tool)? {
            Some(pending) => Err(TxnError::Pending(pending)),
            None => Ok(()),
        }
    }

    fn pending_for(&self, tool: &str) -> Result<Option<String>, TxnError> {
        let dir = self.root.join("journal", &[])?;
        for e in fs::read_dir(dir).map_err(io)?.flatten() {
            let Ok(text) = fs::read_to_string(e.path()) else { continue };
            match serde_json::from_str::<Journal>(&text) {
                Ok(j) if j.tool_id == tool && !j.state.terminal() => return Ok(Some(j.txn_id)),
                Err(_) if e.path().extension().and_then(|x| x.to_str()) == Some("json") => {
                    // an unreadable journal could belong to any tool: do not proceed past it
                    return Ok(Some("unreadable-journal".to_string()));
                }
                _ => {}
            }
        }
        Ok(None)
    }

    // ---- locks and journals -----------------------------------------------------------------------------------------

    fn lock(&self, tool: &str, txn: &str) -> Result<(), TxnError> {
        let p = self.root.join("locks", &[&format!("{tool}.lock")])?;
        match fs::OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(mut f) => {
                f.write_all(txn.as_bytes()).map_err(io)?;
                let _ = f.sync_all();
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(TxnError::Busy),
            Err(_) => Err(TxnError::Refused("lock_unavailable")),
        }
    }

    fn unlock(&self, tool: &str, txn: &str) {
        if let Ok(p) = self.root.join("locks", &[&format!("{tool}.lock")]) {
            if fs::read_to_string(&p).map(|t| t == txn).unwrap_or(false) {
                let _ = fs::remove_file(p);
            }
        }
    }

    fn save(&self, j: &mut Journal) -> Result<(), TxnError> {
        j.journal_digest = journal_digest(j);
        let p = self.root.join("journal", &[&format!("{}.json", j.txn_id)])?;
        atomic_write(&p, serde_json::to_string(j).map_err(io)?.as_bytes()).map_err(io)
    }

    fn new_journal(&self, a: &Authority, action: Action, from: Option<String>, to: Option<String>, sha: Option<String>) -> Journal {
        let r = a.recipe.recipe();
        Journal {
            schema_version: 1,
            txn_id: format!("txn-{}", uuid::Uuid::new_v4().simple()),
            action,
            tool_id: r.tool_id.clone(),
            from_version: from,
            to_version: to,
            artifact_sha256: sha,
            recipe_digest: a.recipe.digest().to_string(),
            approval_id: a.approval.approval().approval_id.clone(),
            platform_target: self.host_platform.to_string(),
            computer_id: self.root.computer_id.clone(),
            state: TxnState::Started,
            intents: Vec::new(),
            started_at: self.now(),
            rollback: None,
            owned: Vec::new(),
            journal_digest: String::new(),
        }
    }

    fn write_receipt(&self, j: &Journal, kept: Vec<String>) -> Result<Receipt, TxnError> {
        let mut r = Receipt {
            schema_version: 1,
            txn_id: j.txn_id.clone(),
            action: j.action,
            tool_id: j.tool_id.clone(),
            from_version: j.from_version.clone(),
            to_version: j.to_version.clone(),
            artifact_sha256: j.artifact_sha256.clone(),
            recipe_digest: j.recipe_digest.clone(),
            approval_id: j.approval_id.clone(),
            platform_target: j.platform_target.clone(),
            computer_id: j.computer_id.clone(),
            synthetic: true,
            completed_at: self.now(),
            state: "committed".to_string(),
            kept,
            rollback: RollbackReport::not_needed(),
            receipt_digest: String::new(),
        };
        r.receipt_digest = receipt_digest(&r);
        let p = self.root.join("receipts", &[&format!("{}.json", j.txn_id)])?;
        atomic_write(&p, serde_json::to_string(&r).map_err(io)?.as_bytes()).map_err(io)?;
        Ok(r)
    }

    /// The receipt of the transaction that last made `tool` what it is, if its content still matches its digest.
    pub fn read_receipt(&self, txn: &str) -> Option<Receipt> {
        let p = self.root.join("receipts", &[&format!("{txn}.json")]).ok()?;
        let r: Receipt = serde_json::from_str(&fs::read_to_string(p).ok()?).ok()?;
        (receipt_digest(&r) == r.receipt_digest).then_some(r)
    }

    /// Which version is active for `tool` (None = not installed).
    pub fn active_version(&self, tool: &str) -> Result<Option<String>, TxnError> {
        Ok(self.read_pointer(tool)?.map(|p| p.version))
    }

    // ---- install / update ---------------------------------------------------------------------------------------------

    pub fn install(&self, a: &Authority, artifact: &[u8]) -> Result<Outcome, TxnError> {
        self.apply(a, Action::Install, artifact)
    }

    pub fn update(&self, a: &Authority, artifact: &[u8]) -> Result<Outcome, TxnError> {
        self.apply(a, Action::Update, artifact)
    }

    fn apply(&self, a: &Authority, action: Action, artifact: &[u8]) -> Result<Outcome, TxnError> {
        self.authorize(a, action)?;
        let r = a.recipe.recipe();
        let (tool, version) = (r.tool_id.as_str(), r.version.as_str());
        if !safe_component(tool) || !safe_component(version) {
            return Err(TxnError::Refused("unsafe_path_component"));
        }
        let sha = self.check_identity(a, artifact)?;
        self.ensure_free(tool)?;
        let current = self.read_pointer(tool)?;
        // ---- idempotency and the update precondition
        match (action, &current) {
            (Action::Install, Some(p)) if p.version == version && p.artifact_sha256 == sha => {
                return self.existing(&p.txn).map(|receipt| Outcome { receipt, already_applied: true });
            }
            (Action::Install, Some(_)) => return Err(TxnError::Refused("already_installed_use_update")),
            (Action::Update, Some(p)) if p.version == version && p.artifact_sha256 == sha => {
                return self.existing(&p.txn).map(|receipt| Outcome { receipt, already_applied: true });
            }
            (Action::Update, Some(p)) if Some(p.version.as_str()) != a.approval.approval().from_version.as_deref() => {
                return Err(TxnError::Refused("installed_version_differs_from_the_approved_update_source"));
            }
            (Action::Update, None) => return Err(TxnError::Refused("not_installed")),
            _ => {}
        }
        let mut j = self.new_journal(a, action, current.as_ref().map(|p| p.version.clone()), Some(version.to_string()), Some(sha.clone()));
        // the lock comes first: whoever holds it owns the question "is something unfinished for this tool?"
        self.lock(tool, &j.txn_id)?;
        match self.pending_for(tool) {
            Ok(None) => {}
            Ok(Some(pending)) => {
                self.unlock(tool, &j.txn_id);
                return Err(TxnError::Pending(pending));
            }
            Err(e) => {
                self.unlock(tool, &j.txn_id);
                return Err(e);
            }
        }
        if let Err(e) = self.save(&mut j) {
            self.unlock(tool, &j.txn_id);
            return Err(e);
        }
        let result = self.run_apply(a, &mut j, artifact, current.is_some());
        self.finish(j, result)
    }

    fn existing(&self, txn: &str) -> Result<Receipt, TxnError> {
        self.read_receipt(txn).ok_or(TxnError::Refused("installed_state_has_no_valid_receipt"))
    }

    /// Step 1 of creating a file the transaction will own: the INTENT (role and every place it may be at) is made durable before the
    /// file exists.
    fn intend(&self, j: &mut Journal, role: &str, rels: Vec<String>) -> Result<(), TxnError> {
        j.owned.push(OwnedFile { role: role.to_string(), phase: Phase::Intended, rels, sha256: None, size: None, identity: None });
        self.save(j)
    }

    fn mark(
        &self,
        j: &mut Journal,
        role: &str,
        phase: Phase,
        identity: Option<[u64; 2]>,
        content: Option<(String, u64)>,
    ) -> Result<(), TxnError> {
        if let Some(e) = j.owned.iter_mut().find(|e| e.role == role) {
            e.phase = phase;
            if identity.is_some() {
                e.identity = identity;
            }
            if let Some((sha, size)) = content {
                e.sha256 = Some(sha);
                e.size = Some(size);
            }
        }
        self.save(j)
    }

    /// Step 2: exclusive create (it fails if anything is there), the identity is read from the OPEN HANDLE and made durable before
    /// any data is written. `after_create` is the injection point inside the irreducible window between the create and the record:
    /// a simulated crash there leaves an ambiguous orphan (nothing is recorded); a plain failure still records what the engine
    /// holds in memory, so the in-process rollback can prove ownership.
    fn create_owned(&self, j: &mut Journal, role: &str, path: &Path, after_create: Point) -> Result<fs::File, TxnError> {
        let file = fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(io)?;
        let identity = identity_and_links(&file).ok_or(TxnError::Refused("identity_unavailable"))?.0;
        if let Err(e) = self.hit(after_create) {
            if matches!(e, TxnError::Refused(_)) {
                let _ = self.mark(j, role, Phase::Created, Some(identity), None);
            }
            return Err(e);
        }
        self.mark(j, role, Phase::Created, Some(identity), None)?;
        Ok(file)
    }

    fn run_apply(&self, a: &Authority, j: &mut Journal, artifact: &[u8], had_pointer: bool) -> Result<Receipt, TxnError> {
        let tool = j.tool_id.clone();
        let version = a.recipe.recipe().version.clone();
        self.hit(Point::AfterLock)?;
        // 1. snapshot of the managed state this transaction may change
        let snap = self.root.join("snapshots", &[&j.txn_id])?;
        j.intents.push("snapshot_dir".to_string());
        self.save(j)?;
        fs::create_dir(&snap).map_err(io)?;
        if had_pointer {
            let src = self.root.join("state", &[&format!("{tool}.json")])?;
            let bytes = fs::read(&src).map_err(io)?;
            // intent (durable) -> exclusive create -> identity from the handle (durable) -> data -> fsync -> final record (durable)
            self.intend(j, "snapshot_pointer", vec![format!("snapshots/{}/pointer.json", j.txn_id)])?;
            self.hit(Point::SnapshotBeforeCreate)?;
            let mut f = self.create_owned(j, "snapshot_pointer", &snap.join("pointer.json"), Point::SnapshotAfterCreate)?;
            if let Some(fault) = self.fault.and_then(|h| h(Point::SnapshotDuringWrite)) {
                f.write_all(&bytes[..bytes.len() / 2]).map_err(io)?;
                return Err(if fault == Fault::Crash {
                    TxnError::Interrupted(Point::SnapshotDuringWrite)
                } else {
                    TxnError::Refused("injected_failure")
                });
            }
            f.write_all(&bytes).map_err(io)?;
            f.sync_all().map_err(io)?;
            drop(f);
            self.hit(Point::SnapshotAfterSync)?;
            self.mark(j, "snapshot_pointer", Phase::Final, None, Some((sha256_hex_bytes(&bytes), bytes.len() as u64)))?;
            self.hit(Point::SnapshotAfterRecord)?;
        }
        j.state = TxnState::Snapshotted;
        self.save(j)?;
        self.hit(Point::AfterSnapshot)?;
        // 2. stage (in a directory of this transaction only)
        j.intents.push("stage_dir".to_string());
        self.save(j)?;
        let stage = self.root.join("staging", &[&j.txn_id])?;
        fs::create_dir(&stage).map_err(io)?;
        let part = stage.join("artifact.bin.part");
        // Same discipline as the snapshot: the intent is durable before the file exists, the identity read from the exclusive
        // create's handle is durable before any data is written. The same file may later be at any of the listed places
        // (renames preserve identity).
        let version_artifact = format!("versions/{tool}/{version}/artifact.bin");
        self.intend(
            j,
            "staged_artifact",
            vec![format!("staging/{}/artifact.bin.part", j.txn_id), format!("staging/{}/artifact.bin", j.txn_id), version_artifact],
        )?;
        self.hit(Point::StageBeforeCreate)?;
        let mut file = self.create_owned(j, "staged_artifact", &part, Point::StageAfterCreate)?;
        match self.fault.and_then(|f| f(Point::DuringStageWrite)) {
            Some(f) => {
                file.write_all(&artifact[..artifact.len() / 2]).map_err(io)?;
                return Err(if f == Fault::Crash {
                    TxnError::Interrupted(Point::DuringStageWrite)
                } else {
                    TxnError::Refused("injected_failure")
                });
            }
            None => {
                file.write_all(artifact).map_err(io)?;
                file.sync_all().map_err(io)?;
            }
        }
        drop(file);
        self.hit(Point::StageAfterSync)?;
        // the content is complete: from now on it is recognised by digest as well (durable before the rename)
        self.mark(j, "staged_artifact", Phase::Final, None, Some((a.recipe.recipe().artifact.sha256.clone(), artifact.len() as u64)))?;
        fs::rename(&part, stage.join("artifact.bin")).map_err(io)?;
        j.state = TxnState::Staged;
        self.save(j)?;
        self.hit(Point::AfterStageWrite)?;
        // 3. verify what was staged (the bytes on disk, not the buffer)
        let staged = fs::read(stage.join("artifact.bin")).map_err(io)?;
        if sha256_hex_bytes(&staged) != a.recipe.recipe().artifact.sha256 {
            return Err(TxnError::Refused("staged_content_does_not_match"));
        }
        if verify_owned(self.root.path(), &stage.join("artifact.bin"), &j.owned).is_err() {
            return Err(TxnError::Refused("staged_file_is_not_the_file_this_transaction_wrote"));
        }
        j.state = TxnState::Verified;
        self.save(j)?;
        self.hit(Point::AfterVerify)?;
        // freshness again, right before the first change to the managed state
        if (self.clock)() >= a.approval.expires_at() {
            return Err(TxnError::Refused("approval_expired"));
        }
        // 4. activate: version folder first, then one atomic pointer switch
        let vdir = self.root.join("versions", &[&tool, &version])?;
        if vdir.exists() {
            return Err(TxnError::Refused("version_folder_already_exists"));
        }
        self.hit(Point::BeforeVersionDirCreate)?;
        j.intents.push(format!("version_dir:{version}"));
        self.save(j)?;
        fs::create_dir_all(&vdir).map_err(io)?;
        self.hit(Point::AfterVersionDirCreate)?;
        // the file about to be activated must still be the one that was staged and verified
        if verify_owned(self.root.path(), &stage.join("artifact.bin"), &j.owned).is_err() {
            return Err(TxnError::Refused("staged_file_is_not_the_file_this_transaction_wrote"));
        }
        fs::rename(stage.join("artifact.bin"), vdir.join("artifact.bin")).map_err(io)?;
        self.hit(Point::BeforePointerWrite)?;
        j.intents.push("pointer_switch".to_string());
        self.save(j)?;
        let pointer = Pointer {
            tool: tool.clone(),
            version: version.clone(),
            artifact_sha256: a.recipe.recipe().artifact.sha256.clone(),
            txn: j.txn_id.clone(),
        };
        let ppath = self.root.join("state", &[&format!("{tool}.json")])?;
        atomic_write(&ppath, serde_json::to_string(&pointer).map_err(io)?.as_bytes()).map_err(io)?;
        j.state = TxnState::Activated;
        self.save(j)?;
        self.hit(Point::AfterPointerSwitch)?;
        // 5. durable receipt, then the journal says committed
        self.hit(Point::BeforeReceiptWrite)?;
        let receipt = self.write_receipt(j, self.kept_for(&tool))?;
        self.hit(Point::AfterReceiptWrite)?;
        self.hit(Point::BeforeJournalCommit)?;
        j.state = TxnState::Committed;
        self.save(j)?;
        Ok(receipt)
    }

    fn kept_for(&self, tool: &str) -> Vec<String> {
        match self.root.join("data", &[tool]) {
            Ok(p) if p.exists() => vec!["user_data".to_string()],
            _ => vec![],
        }
    }

    /// Common tail of every transaction: success cleans up and unlocks; a failure rolls back and says what that achieved; a
    /// simulated crash leaves everything exactly as it is.
    fn finish(&self, mut j: Journal, result: Result<Receipt, TxnError>) -> Result<Outcome, TxnError> {
        match result {
            Ok(receipt) => {
                let _ = self.cleanup_after_commit(&j);
                self.unlock(&j.tool_id, &j.txn_id);
                Ok(Outcome { receipt, already_applied: false })
            }
            Err(TxnError::Interrupted(p)) => Err(TxnError::Interrupted(p)),
            Err(e) => {
                let code = match e {
                    TxnError::Refused(c) => c,
                    TxnError::Failed { code, .. } => code,
                    _ => "failed",
                };
                let report = self.roll_back(&mut j);
                if report.status == "restored" || report.status == "not_needed" {
                    let _ = self.cleanup_after_commit(&j);
                    self.unlock(&j.tool_id, &j.txn_id);
                }
                Err(TxnError::Failed { code, rollback: report })
            }
        }
    }

    /// Removes what a finished transaction leaves behind — staging and snapshot — but only in the shape this module creates.
    /// Anything foreign keeps the folder where it is. Returns why something was retained.
    fn cleanup_after_commit(&self, j: &Journal) -> Option<String> {
        let mut why: Option<String> = None;
        if let Ok(p) = self.root.join("staging", &[&j.txn_id]) {
            if p.exists() {
                if let Err(e) = remove_verified_dir(self.root.path(), &p, &j.owned) {
                    why = Some(format!("staging: {e}"));
                }
            }
        }
        if let Ok(p) = self.root.join("snapshots", &[&j.txn_id]) {
            if p.exists() {
                if let Err(e) = remove_snapshot(self.root.path(), &p, &j.owned) {
                    why.get_or_insert(format!("snapshot: {e}"));
                }
            }
        }
        why
    }

    // ---- uninstall ------------------------------------------------------------------------------------------------------

    pub fn uninstall(&self, a: &Authority) -> Result<Outcome, TxnError> {
        self.authorize(a, Action::Uninstall)?;
        let tool = a.recipe.recipe().tool_id.clone();
        if !safe_component(&tool) {
            return Err(TxnError::Refused("unsafe_path_component"));
        }
        self.ensure_free(&tool)?;
        let current = self.read_pointer(&tool)?;
        let Some(cur) = current else {
            // idempotent: nothing installed, nothing to do; the receipt of the removal (if there is one) is not required
            return Err(TxnError::Refused("not_installed"));
        };
        let mut j = self.new_journal(a, Action::Uninstall, Some(cur.version.clone()), None, Some(cur.artifact_sha256.clone()));
        self.lock(&tool, &j.txn_id)?;
        // Before anything is moved: everything that would be removed must be recognised -- the active pointer, and every version
        // folder must hold exactly one artifact whose digest and identity match a committed receipt.
        match self.uninstall_manifest(&tool, &j.txn_id) {
            Ok(owned) => j.owned = owned,
            Err(e) => {
                self.unlock(&tool, &j.txn_id);
                return Err(e);
            }
        }
        match self.pending_for(&tool) {
            Ok(None) => {}
            Ok(Some(pending)) => {
                self.unlock(&tool, &j.txn_id);
                return Err(TxnError::Pending(pending));
            }
            Err(e) => {
                self.unlock(&tool, &j.txn_id);
                return Err(e);
            }
        }
        if let Err(e) = self.save(&mut j) {
            self.unlock(&tool, &j.txn_id);
            return Err(e);
        }
        let result = self.run_uninstall(&mut j, &tool);
        self.finish(j, result)
    }

    fn uninstall_manifest(&self, tool: &str, txn: &str) -> Result<Vec<OwnedFile>, TxnError> {
        let mut owned = Vec::new();
        let ppath = self.root.join("state", &[&format!("{tool}.json")])?;
        let mut pf = fs::File::open(&ppath).map_err(io)?;
        let (pid, plinks) = identity_and_links(&pf).ok_or(TxnError::Refused("identity_unavailable"))?;
        if plinks != 1 || entry_kind(&ppath) != "file" {
            return Err(TxnError::Refused("unrecognised_installed_content"));
        }
        let psize = pf.metadata().map_err(io)?.len();
        let psha = sha256_of_open_file(&mut pf, VERIFY_LIMIT).map_err(|_| TxnError::Refused("unrecognised_installed_content"))?;
        owned.push(OwnedFile {
            role: "snapshot_pointer".to_string(),
            phase: Phase::Final,
            rels: vec![format!("state/{tool}.json"), format!("snapshots/{txn}/pointer.json")],
            sha256: Some(psha),
            size: Some(psize),
            identity: Some(pid),
        });
        let vroot = self.root.join("versions", &[tool])?;
        if fs::symlink_metadata(&vroot).is_ok() {
            if entry_kind(&vroot) != "folder" {
                return Err(TxnError::Refused("unrecognised_installed_content"));
            }
            for (version, kind) in list_entries(&vroot).map_err(|_| TxnError::Refused("unrecognised_installed_content"))? {
                if kind != "folder" || !safe_component(&version) {
                    return Err(TxnError::Refused("unrecognised_installed_content"));
                }
                let vdir = vroot.join(&version);
                if list_entries(&vdir).map_err(|_| TxnError::Refused("unrecognised_installed_content"))?
                    != vec![("artifact.bin".to_string(), "file")]
                {
                    return Err(TxnError::Refused("unrecognised_installed_content"));
                }
                let art = vdir.join("artifact.bin");
                let mut f = fs::File::open(&art).map_err(io)?;
                let (id, links) = identity_and_links(&f).ok_or(TxnError::Refused("identity_unavailable"))?;
                let sha = sha256_of_open_file(&mut f, VERIFY_LIMIT).map_err(|_| TxnError::Refused("unrecognised_installed_content"))?;
                // the content must be exactly what a committed install/update of this version wrote
                if links != 1 || !self.receipt_vouches(tool, &version, &sha) {
                    return Err(TxnError::Refused("installed_content_has_no_matching_receipt"));
                }
                owned.push(OwnedFile {
                    role: "removed_version_artifact".to_string(),
                    phase: Phase::Final,
                    rels: vec![
                        format!("versions/{tool}/{version}/artifact.bin"),
                        format!("snapshots/{txn}/versions/{version}/artifact.bin"),
                    ],
                    sha256: Some(sha),
                    size: Some(f.metadata().map_err(io)?.len()),
                    identity: Some(id),
                });
            }
        }
        Ok(owned)
    }

    /// Is there a valid committed install/update receipt for exactly this tool, version and artifact digest?
    fn receipt_vouches(&self, tool: &str, version: &str, sha: &str) -> bool {
        let Ok(dir) = self.root.join("receipts", &[]) else { return false };
        let Ok(read) = fs::read_dir(dir) else { return false };
        read.flatten().any(|e| {
            fs::read_to_string(e.path())
                .ok()
                .and_then(|t| serde_json::from_str::<Receipt>(&t).ok())
                .map(|r| {
                    receipt_digest(&r) == r.receipt_digest
                        && r.tool_id == tool
                        && matches!(r.action, Action::Install | Action::Update)
                        && r.to_version.as_deref() == Some(version)
                        && r.artifact_sha256.as_deref() == Some(sha)
                })
                .unwrap_or(false)
        })
    }

    fn run_uninstall(&self, j: &mut Journal, tool: &str) -> Result<Receipt, TxnError> {
        self.hit(Point::AfterLock)?;
        let snap = self.root.join("snapshots", &[&j.txn_id])?;
        j.intents.push("snapshot_dir".to_string());
        self.save(j)?;
        fs::create_dir(&snap).map_err(io)?;
        j.state = TxnState::Snapshotted;
        self.save(j)?;
        self.hit(Point::AfterSnapshot)?;
        // the pointer is MOVED into the snapshot (the removal), the versions folder likewise: both can be put back
        j.intents.push("pointer_removed".to_string());
        self.save(j)?;
        let ppath = self.root.join("state", &[&format!("{tool}.json")])?;
        fs::rename(&ppath, snap.join("pointer.json")).map_err(io)?;
        self.hit(Point::UninstallAfterPointerMoved)?;
        j.intents.push("versions_moved".to_string());
        self.save(j)?;
        let vroot = self.root.join("versions", &[tool])?;
        if vroot.exists() {
            fs::rename(&vroot, snap.join("versions")).map_err(io)?;
        }
        self.hit(Point::UninstallAfterVersionsMoved)?;
        j.state = TxnState::Activated;
        self.save(j)?;
        self.hit(Point::BeforeReceiptWrite)?;
        let receipt = self.write_receipt(j, self.kept_for(tool))?;
        self.hit(Point::AfterReceiptWrite)?;
        self.hit(Point::BeforeJournalCommit)?;
        j.state = TxnState::Committed;
        self.save(j)?;
        Ok(receipt)
    }

    // ---- rollback ------------------------------------------------------------------------------------------------------

    /// Undoes, in reverse order, exactly what the journal's intents say was started, and states the result truthfully. A step that
    /// cannot be undone (or whose result is not what it should be) is reported, never silently skipped.
    fn roll_back(&self, j: &mut Journal) -> RollbackReport {
        let mut undone: Vec<String> = Vec::new();
        let mut not_undone: Vec<NotUndone> = Vec::new();
        let tool = j.tool_id.clone();
        // a receipt of a transaction that did not commit would be a false statement: it goes first
        if let Ok(rp) = self.root.join("receipts", &[&format!("{}.json", j.txn_id)]) {
            if rp.exists() {
                match fs::remove_file(&rp) {
                    Ok(()) if !wait_until_gone(&rp) => not_undone.push(NotUndone {
                        what: "uncommitted receipt".to_string(),
                        why: "it was removed, but its name is still present after a second".to_string(),
                    }),
                    Ok(()) => undone.push("uncommitted receipt removed".to_string()),
                    Err(e) => not_undone.push(NotUndone { what: "uncommitted receipt".to_string(), why: format!("cannot remove it: {e}") }),
                }
            }
        }
        let intents: Vec<String> = j.intents.iter().rev().cloned().collect();
        // while the active pointer could not be restored it may still name the new version: that folder must then stay
        let mut pointer_unrestored = false;
        for intent in intents {
            match intent.as_str() {
                "pointer_switch" => match self.undo_pointer(j, &tool) {
                    Ok(()) => undone.push("active-version pointer restored".to_string()),
                    Err(why) => {
                        pointer_unrestored = true;
                        not_undone.push(NotUndone { what: "active-version pointer".to_string(), why });
                    }
                },
                "pointer_removed" => match self.undo_removed_pointer(j, &tool) {
                    Ok(()) => undone.push("active-version pointer put back".to_string()),
                    Err(why) => not_undone.push(NotUndone { what: "removed active-version pointer".to_string(), why }),
                },
                "versions_moved" => match self.undo_moved_versions(j, &tool) {
                    Ok(()) => undone.push("version folders put back".to_string()),
                    Err(why) => not_undone.push(NotUndone { what: "removed version folders".to_string(), why }),
                },
                "stage_dir" => match self.undo_stage(j) {
                    Ok(()) => undone.push("staging folder removed".to_string()),
                    Err(why) => not_undone.push(NotUndone { what: "staging folder".to_string(), why }),
                },
                // the snapshot is what a retry of an earlier, failed undo would restore from: it goes only when nothing else failed
                "snapshot_dir" if !not_undone.is_empty() => {}
                "snapshot_dir" => match self.undo_snapshot(j) {
                    Ok(()) => undone.push("snapshot folder removed".to_string()),
                    Err(why) => not_undone.push(NotUndone { what: "snapshot folder".to_string(), why }),
                },
                other if other.starts_with("version_dir:") => {
                    let version = &other["version_dir:".len()..];
                    if pointer_unrestored && j.to_version.as_deref() == Some(version) {
                        not_undone.push(NotUndone {
                            what: format!("version folder {version}"),
                            why: "the active-version pointer could not be restored and may still name this version; the folder was left in place"
                                .to_string(),
                        });
                        continue;
                    }
                    match self.undo_version_dir(j, &tool, version) {
                        Ok(true) => undone.push(format!("version folder {version} removed")),
                        Ok(false) => {}
                        Err(why) => not_undone.push(NotUndone { what: format!("version folder {version}"), why }),
                    }
                }
                _ => not_undone
                    .push(NotUndone { what: "unknown journal entry".to_string(), why: "not understood; left untouched".to_string() }),
            }
        }
        let mut not_covered = Vec::new();
        if self.root.join("data", &[&tool]).map(|p| p.exists()).unwrap_or(false) {
            not_covered.push("tool user data (data folder) is never part of a rollback".to_string());
        }
        let status = if not_undone.is_empty() {
            if undone.is_empty() {
                "not_needed"
            } else {
                "restored"
            }
        } else if undone.is_empty() {
            "failed"
        } else {
            "partial"
        };
        let report = RollbackReport { status: status.to_string(), undone, not_undone, not_covered };
        j.state = if report.not_undone.is_empty() { TxnState::RolledBack } else { TxnState::RollbackFailed };
        j.rollback = Some(report.clone());
        let _ = self.save(j);
        report
    }

    fn undo_pointer(&self, j: &Journal, tool: &str) -> Result<(), String> {
        self.rollback_hit(Point::RollbackPointerRestore)?;
        let ppath = self.root.join("state", &[&format!("{tool}.json")]).map_err(|_| "path refused".to_string())?;
        let snap = self.root.join("snapshots", &[&j.txn_id]).map_err(|_| "path refused".to_string())?.join("pointer.json");
        if snap.exists() {
            // the pointer that is about to become the active one must be the very file this transaction saved
            verify_owned(self.root.path(), &snap, &j.owned)
                .map_err(|_| "the saved pointer is not the file this transaction wrote; it was not restored".to_string())?;
            let bytes = fs::read(&snap).map_err(|_| "snapshot unreadable".to_string())?;
            atomic_write(&ppath, &bytes).map_err(|e| format!("cannot restore the pointer: {e}"))?;
        } else if ppath.exists() {
            fs::remove_file(&ppath).map_err(|e| format!("cannot remove the pointer: {e}"))?;
        }
        Ok(())
    }

    fn undo_removed_pointer(&self, j: &Journal, tool: &str) -> Result<(), String> {
        self.rollback_hit(Point::RollbackRestoreRemoved)?;
        let ppath = self.root.join("state", &[&format!("{tool}.json")]).map_err(|_| "path refused".to_string())?;
        let snap = self.root.join("snapshots", &[&j.txn_id]).map_err(|_| "path refused".to_string())?.join("pointer.json");
        if snap.exists() {
            verify_owned(self.root.path(), &snap, &j.owned)
                .map_err(|_| "the saved pointer is not the file this transaction wrote; it was not put back".to_string())?;
            fs::rename(&snap, &ppath).map_err(|e| format!("cannot put the pointer back: {e}"))?;
        }
        Ok(())
    }

    fn undo_moved_versions(&self, j: &Journal, tool: &str) -> Result<(), String> {
        self.rollback_hit(Point::RollbackRestoreRemoved)?;
        let vroot = self.root.join("versions", &[tool]).map_err(|_| "path refused".to_string())?;
        let snap = self.root.join("snapshots", &[&j.txn_id]).map_err(|_| "path refused".to_string())?.join("versions");
        if snap.exists() {
            fs::rename(&snap, &vroot).map_err(|e| format!("cannot put the version folders back: {e}"))?;
        }
        Ok(())
    }

    /// Last step of a rollback (the snapshot is needed by the earlier steps): the snapshot folder goes only if everything in it is
    /// manifest-verified; an ambiguous orphan, a substituted or foreign file keeps it, and the rollback says so.
    fn undo_snapshot(&self, j: &Journal) -> Result<(), String> {
        let snap = self.root.join("snapshots", &[&j.txn_id]).map_err(|_| "path refused".to_string())?;
        if fs::symlink_metadata(&snap).is_err() {
            return Ok(());
        }
        if entry_kind(&snap) != "folder" {
            return Err("the snapshot path is not a plain folder; it was left untouched".to_string());
        }
        remove_snapshot(self.root.path(), &snap, &j.owned)
    }

    fn undo_stage(&self, j: &Journal) -> Result<(), String> {
        self.rollback_hit(Point::RollbackRemoveStaging)?;
        let stage = self.root.join("staging", &[&j.txn_id]).map_err(|_| "path refused".to_string())?;
        if stage.exists() {
            if entry_kind(&stage) != "folder" {
                return Err("the staging path is not a plain folder; it was left untouched".to_string());
            }
            remove_verified_dir(self.root.path(), &stage, &j.owned)?;
        }
        Ok(())
    }

    /// Removes the version folder this transaction created — and ONLY if its whole content is exactly what the transaction
    /// wrote: nothing, or the single regular file `artifact.bin` whose SHA-256 equals the transaction's. A matching primary
    /// artifact is not proof that its siblings are ours: any other file, folder, link or junction keeps everything in place.
    /// The removal is the file, then the (then empty) folder, never a recursive delete. Returns whether a folder was removed.
    fn undo_version_dir(&self, j: &Journal, tool: &str, version: &str) -> Result<bool, String> {
        self.rollback_hit(Point::RollbackRemoveVersionDir)?;
        let vdir = self
            .root
            .join("versions", &[tool, version])
            .map_err(|_| "the version path is a link, redirected or unsafe; it was left untouched".to_string())?;
        if fs::symlink_metadata(&vdir).is_err() {
            return Ok(false);
        }
        if entry_kind(&vdir) != "folder" {
            return Err("the version path is not a plain folder (link or other); it was left untouched".to_string());
        }
        let entries = list_entries(&vdir)?;
        match entries.as_slice() {
            [] => {
                fs::remove_dir(&vdir).map_err(|e| format!("cannot remove the empty folder: {e}"))?;
            }
            [(name, "file")] if name == "artifact.bin" => {
                // identity, links and digest against the journal's manifest, then the file and the (empty) folder
                remove_verified_dir(self.root.path(), &vdir, &j.owned)?;
            }
            other => {
                let unexpected: Vec<&(String, &'static str)> =
                    other.iter().filter(|(n, k)| !(n == "artifact.bin" && *k == "file")).collect();
                let detail = if unexpected.is_empty() { "unrecognised content".to_string() } else { unexpected_description(&unexpected) };
                return Err(format!("the version folder holds more than this transaction wrote: {detail}"));
            }
        }
        // the tool's version root goes too, but only when nothing else (an older version) is in it
        if let Some(parent) = vdir.parent() {
            let _ = fs::remove_dir(parent);
        }
        Ok(true)
    }

    // ---- recovery -------------------------------------------------------------------------------------------------------

    /// To be run at start-up, before any new transaction: finishes or undoes whatever an interrupted run left behind.
    pub fn recover(&self) -> Result<Vec<Recovered>, TxnError> {
        let mut out = Vec::new();
        let jdir = self.root.join("journal", &[])?;
        let mut seen_txns: Vec<String> = Vec::new();
        let mut entries: Vec<_> = fs::read_dir(&jdir).map_err(io)?.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".json") {
                // a leftover temporary file from an interrupted atomic write
                if name.ends_with(".tmp") {
                    let _ = fs::remove_file(e.path());
                }
                continue;
            }
            let txn = name.trim_end_matches(".json").to_string();
            let parsed = fs::read_to_string(e.path())
                .ok()
                .and_then(|t| serde_json::from_str::<Journal>(&t).ok())
                .filter(|j| journal_digest(j) == j.journal_digest);
            let Some(mut j) = parsed else {
                out.push(Recovered::CorruptJournal(txn));
                continue;
            };
            seen_txns.push(j.txn_id.clone());
            match j.state {
                TxnState::Committed | TxnState::RolledBack => {
                    if let Some(why) = self.cleanup_after_commit(&j) {
                        out.push(Recovered::LeftoversRetained(j.txn_id.clone(), why));
                    }
                    self.unlock(&j.tool_id, &j.txn_id);
                }
                TxnState::RollbackFailed => {
                    // try again; the previous failure may have been transient
                    let report = self.roll_back(&mut j);
                    if report.not_undone.is_empty() {
                        let leftovers = self.cleanup_after_commit(&j);
                        self.unlock(&j.tool_id, &j.txn_id);
                        out.push(Recovered::RolledBack(j.txn_id.clone()));
                        if let Some(why) = leftovers {
                            out.push(Recovered::LeftoversRetained(j.txn_id.clone(), why));
                        }
                    } else {
                        out.push(Recovered::NeedsAttention(j.txn_id.clone(), report));
                    }
                }
                _ => {
                    if self.complete_except_bookkeeping(&j) {
                        j.state = TxnState::Committed;
                        let _ = self.save(&mut j);
                        let leftovers = self.cleanup_after_commit(&j);
                        self.unlock(&j.tool_id, &j.txn_id);
                        out.push(Recovered::RolledForward(j.txn_id.clone()));
                        if let Some(why) = leftovers {
                            out.push(Recovered::LeftoversRetained(j.txn_id.clone(), why));
                        }
                    } else {
                        let report = self.roll_back(&mut j);
                        if report.not_undone.is_empty() {
                            let leftovers = self.cleanup_after_commit(&j);
                            self.unlock(&j.tool_id, &j.txn_id);
                            out.push(Recovered::RolledBack(j.txn_id.clone()));
                            if let Some(why) = leftovers {
                                out.push(Recovered::LeftoversRetained(j.txn_id.clone(), why));
                            }
                        } else {
                            out.push(Recovered::NeedsAttention(j.txn_id.clone(), report));
                        }
                    }
                }
            }
        }
        // a lock whose transaction never got a journal: nothing was changed, so it is safe to drop
        let ldir = self.root.join("locks", &[])?;
        for e in fs::read_dir(&ldir).map_err(io)?.flatten() {
            let Ok(txn) = fs::read_to_string(e.path()) else { continue };
            if !seen_txns.contains(&txn) && !out.iter().any(|r| matches!(r, Recovered::CorruptJournal(_))) {
                let _ = fs::remove_file(e.path());
                out.push(Recovered::OrphanLockRemoved(txn));
            }
        }
        Ok(out)
    }

    /// The receipt is on disk and the state it describes is in place: only the journal's final word is missing.
    fn complete_except_bookkeeping(&self, j: &Journal) -> bool {
        let Some(receipt) = self.read_receipt(&j.txn_id) else { return false };
        if receipt.txn_id != j.txn_id || receipt.recipe_digest != j.recipe_digest {
            return false;
        }
        match (j.action, self.read_pointer(&j.tool_id)) {
            (Action::Install | Action::Update, Ok(Some(p))) => Some(p.version.as_str()) == j.to_version.as_deref() && p.txn == j.txn_id,
            (Action::Uninstall, Ok(None)) => true,
            _ => false,
        }
    }
}

pub(crate) fn sha256_hex_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
