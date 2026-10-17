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
    AfterSnapshot,
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
    fn not_needed() -> RollbackReport {
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
    /// Always true: the engine cannot produce anything else.
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
    fn terminal(self) -> bool {
        matches!(self, TxnState::Committed | TxnState::RolledBack)
    }
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
    pub journal_digest: String,
}

fn journal_digest(j: &Journal) -> String {
    let mut blank = j.clone();
    blank.journal_digest = String::new();
    sha256_hex(&canonical_json(&serde_json::to_value(&blank).unwrap_or(serde_json::Value::Null)))
}

fn receipt_digest(r: &Receipt) -> String {
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
        let path = base.join(format!("{SANDBOX_PREFIX}{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir(&path).map_err(|_| "cannot_create_sandbox")?;
        let marker = serde_json::json!({ "magic": MARKER_MAGIC, "computerId": computer_id }).to_string();
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pointer {
    tool: String,
    version: String,
    artifact_sha256: String,
    txn: String,
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
        let r = a.recipe.recipe();
        let ap = a.approval.approval();
        if !r.synthetic || !r.tool_id.starts_with("synthetic-") {
            return Err(TxnError::Refused("not_a_synthetic_recipe"));
        }
        if ap.action != action
            || ap.recipe_digest != a.recipe.digest()
            || ap.tool_id != r.tool_id
            || ap.version != r.version
            || ap.computer_id != self.root.computer_id
            || ap.platform_target != self.host_platform
            || r.target.platform_target != self.host_platform
        {
            return Err(TxnError::Refused("approval_does_not_match"));
        }
        if (self.clock)() >= a.approval.expires_at() {
            return Err(TxnError::Refused("approval_expired"));
        }
        Ok(())
    }

    fn check_identity(&self, a: &Authority, bytes: &[u8]) -> Result<String, TxnError> {
        let r = a.recipe.recipe();
        let sha = sha256_hex_bytes(bytes);
        if bytes.len() as u64 != r.artifact.size_bytes || sha != r.artifact.sha256 {
            return Err(TxnError::Refused("artifact_does_not_match_recipe"));
        }
        let format = reference::sniff_reader(&bytes[..bytes.len().min(4096)], bytes.len() as u64, &mut Cursor::new(bytes));
        if format == ArtifactFormat::Unrecognised || format.as_str() != r.artifact.format {
            return Err(TxnError::Refused("artifact_format_mismatch"));
        }
        let observed = ObservedArtifact { entry_id: &r.tool_id, platform_target: self.host_platform, format, sha256: &sha };
        let v = reference::evaluate(&observed, self.refs);
        if v.result != MatchResult::MatchesAuthenticated || v.version.as_deref() != Some(r.version.as_str()) {
            return Err(TxnError::Refused("artifact_not_authenticated_for_this_platform"));
        }
        Ok(sha)
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

    fn run_apply(&self, a: &Authority, j: &mut Journal, artifact: &[u8], had_pointer: bool) -> Result<Receipt, TxnError> {
        let tool = j.tool_id.clone();
        let version = a.recipe.recipe().version.clone();
        self.hit(Point::AfterLock)?;
        // 1. snapshot of the managed state this transaction may change
        let snap = self.root.join("snapshots", &[&j.txn_id])?;
        fs::create_dir(&snap).map_err(io)?;
        if had_pointer {
            let src = self.root.join("state", &[&format!("{tool}.json")])?;
            fs::copy(&src, snap.join("pointer.json")).map_err(io)?;
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
        match self.fault.and_then(|f| f(Point::DuringStageWrite)) {
            Some(f) => {
                fs::write(&part, &artifact[..artifact.len() / 2]).map_err(io)?;
                return Err(if f == Fault::Crash {
                    TxnError::Interrupted(Point::DuringStageWrite)
                } else {
                    TxnError::Refused("injected_failure")
                });
            }
            None => {
                let mut f = fs::File::create(&part).map_err(io)?;
                f.write_all(artifact).map_err(io)?;
                f.sync_all().map_err(io)?;
            }
        }
        fs::rename(&part, stage.join("artifact.bin")).map_err(io)?;
        j.state = TxnState::Staged;
        self.save(j)?;
        self.hit(Point::AfterStageWrite)?;
        // 3. verify what was staged (the bytes on disk, not the buffer)
        let staged = fs::read(stage.join("artifact.bin")).map_err(io)?;
        if sha256_hex_bytes(&staged) != a.recipe.recipe().artifact.sha256 {
            return Err(TxnError::Refused("staged_content_does_not_match"));
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
                self.cleanup_after_commit(&j);
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
                    self.cleanup_after_commit(&j);
                    self.unlock(&j.tool_id, &j.txn_id);
                }
                Err(TxnError::Failed { code, rollback: report })
            }
        }
    }

    fn cleanup_after_commit(&self, j: &Journal) {
        if let Ok(p) = self.root.join("staging", &[&j.txn_id]) {
            let _ = fs::remove_dir_all(p);
        }
        if let Ok(p) = self.root.join("snapshots", &[&j.txn_id]) {
            let _ = fs::remove_dir_all(p);
        }
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

    fn run_uninstall(&self, j: &mut Journal, tool: &str) -> Result<Receipt, TxnError> {
        self.hit(Point::AfterLock)?;
        let snap = self.root.join("snapshots", &[&j.txn_id])?;
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
                    Ok(()) => undone.push("uncommitted receipt removed".to_string()),
                    Err(e) => not_undone.push(NotUndone { what: "uncommitted receipt".to_string(), why: format!("cannot remove it: {e}") }),
                }
            }
        }
        let intents: Vec<String> = j.intents.iter().rev().cloned().collect();
        for intent in intents {
            match intent.as_str() {
                "pointer_switch" => match self.undo_pointer(j, &tool) {
                    Ok(()) => undone.push("active-version pointer restored".to_string()),
                    Err(why) => not_undone.push(NotUndone { what: "active-version pointer".to_string(), why }),
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
                other if other.starts_with("version_dir:") => {
                    let version = &other["version_dir:".len()..];
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

    fn undo_stage(&self, j: &Journal) -> Result<(), String> {
        self.rollback_hit(Point::RollbackRemoveStaging)?;
        let stage = self.root.join("staging", &[&j.txn_id]).map_err(|_| "path refused".to_string())?;
        if stage.exists() {
            fs::remove_dir_all(&stage).map_err(|e| format!("cannot remove the staging folder: {e}"))?;
        }
        Ok(())
    }

    /// Removes the version folder this transaction created — and only if it still holds exactly the artifact this transaction
    /// wrote. Returns whether a folder was removed.
    fn undo_version_dir(&self, j: &Journal, tool: &str, version: &str) -> Result<bool, String> {
        self.rollback_hit(Point::RollbackRemoveVersionDir)?;
        let vdir = self.root.join("versions", &[tool, version]).map_err(|_| "path refused".to_string())?;
        if !vdir.exists() {
            return Ok(false);
        }
        let art = vdir.join("artifact.bin");
        let ours = fs::read(&art).map(|b| Some(sha256_hex_bytes(&b)) == j.artifact_sha256).unwrap_or(false)
            || fs::read_dir(&vdir).map(|mut d| d.next().is_none()).unwrap_or(false);
        if !ours {
            return Err("the folder no longer holds what this transaction wrote; it was left untouched".to_string());
        }
        fs::remove_dir_all(&vdir).map_err(|e| format!("cannot remove the folder: {e}"))?;
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
                TxnState::Committed => {
                    self.cleanup_after_commit(&j);
                    self.unlock(&j.tool_id, &j.txn_id);
                }
                TxnState::RolledBack => {
                    self.cleanup_after_commit(&j);
                    self.unlock(&j.tool_id, &j.txn_id);
                }
                TxnState::RollbackFailed => {
                    // try again; the previous failure may have been transient
                    let report = self.roll_back(&mut j);
                    if report.not_undone.is_empty() {
                        self.cleanup_after_commit(&j);
                        self.unlock(&j.tool_id, &j.txn_id);
                        out.push(Recovered::RolledBack(j.txn_id.clone()));
                    } else {
                        out.push(Recovered::NeedsAttention(j.txn_id.clone(), report));
                    }
                }
                _ => {
                    if self.complete_except_bookkeeping(&j) {
                        j.state = TxnState::Committed;
                        let _ = self.save(&mut j);
                        self.cleanup_after_commit(&j);
                        self.unlock(&j.tool_id, &j.txn_id);
                        out.push(Recovered::RolledForward(j.txn_id.clone()));
                    } else {
                        let report = self.roll_back(&mut j);
                        if report.not_undone.is_empty() {
                            self.cleanup_after_commit(&j);
                            self.unlock(&j.tool_id, &j.txn_id);
                            out.push(Recovered::RolledBack(j.txn_id.clone()));
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

fn sha256_hex_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
