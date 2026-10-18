//! Phase 3C: the one-artifact adapter, TEST MODE ONLY.
//!
//! It installs, updates and uninstalls ONE manager-owned executable artifact per tool, transactionally, on top of the
//! handle-relative file-system layer (`managed_fs`, `managed_root`) -- it does not reuse the path-based synthetic engine
//! (`lifecycle.rs`), only its data types (journal entries, receipts, reports) and its two pure policy checks.
//!
//! * **Production is closed.** [`Adapter::production`] refuses every request (`production_adapter_not_enabled`): the production recipe
//!   list and the authenticated reference set are empty, no OS-protected key store exists, and `TrustPolicy::production()` refuses
//!   fakes. The only way to build a working adapter is `Adapter::test_mode`, which exists in test builds only.
//! * **Synthetic artifacts only.** Recipes must be synthetic (`synthetic-*` tools); real catalog entries never pass the trust gate.
//! * **Authorization.** Three independent things are required: a synthetic Phase 3A approval (binding), a device approval (signed by
//!   a key store, user-present, single-use, bound to the exact artifact and effects), and the synthetic recipe itself. The device
//!   approval is consumed after the lock is taken and before the first change to managed state.
//! * **Journal-before-effect with identity.** Every file the transaction creates goes through intent (durable) -> exclusive create ->
//!   identity from the open handle (durable) -> data -> fsync -> final record (durable). Cleanup deletes only files that match their
//!   manifest record (identity, single link, content), through `managed_fs`; anything else is kept and reported.
//! * **Never touched:** the tool's own user data (`data/`), anything outside the managed root, the program search path, system
//!   settings, agent configuration. No process is started and no network is used.
//!
//! What this is not: an installer. Nothing calls it, nothing downloads, and it cannot run on a real tool.
#![allow(dead_code)] // test mode only; no command reaches this module

use crate::{
    approval::Action,
    audit::{canonical_json, sha256_hex},
    device_trust::{self, ApprovalLedger, AuditSink, DeviceApproval, Expected, KeyStore, TrustPolicy},
    discovery::reference::ReferenceSet,
    lifecycle::{
        check_artifact, check_authority, receipt_digest, sha256_hex_bytes, Authority, ClockFn, Fault, FaultFn, NotUndone, Outcome,
        OwnedFile, Phase, Point, Pointer, Receipt, Recovered, RollbackReport, TxnError, TxnState,
    },
    managed_fs::{Dir, Expect, FileH, FsError, Identity, Kind},
    managed_root::ManagedRoot,
};
use serde::{Deserialize, Serialize};

type R<T> = Result<T, TxnError>;

const MAX_JSON: u64 = 1024 * 1024;
const MAX_ARTIFACT: u64 = 256 * 1024 * 1024;

fn fs_err(e: FsError) -> TxnError {
    TxnError::Refused(match e {
        FsError::NotFound => "fs_not_found",
        FsError::Exists => "fs_exists",
        FsError::Link => "fs_link_refused",
        FsError::NotDir => "fs_not_a_directory",
        FsError::NotFile => "fs_not_a_regular_file",
        FsError::CrossDevice => "fs_other_volume",
        FsError::Network => "fs_network_filesystem",
        FsError::Denied => "fs_access_denied",
        FsError::Locked => "fs_locked_by_another_process",
        FsError::TooLarge => "fs_too_large",
        FsError::InvalidName => "fs_invalid_name",
        FsError::NotEmpty => "fs_not_empty",
        FsError::Changed => "fs_changed",
        FsError::Unsupported => "fs_unsupported",
        FsError::Io => "fs_io",
    })
}

fn pair(i: Identity) -> [u64; 2] {
    [i.volume, i.object]
}

fn unpair(p: [u64; 2]) -> Identity {
    Identity { volume: p[0], object: p[1] }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Production,
    TestSynthetic,
}

/// The collaborators that decide whether an approval is real.
pub struct Trust<'a> {
    pub keys: &'a dyn KeyStore,
    pub ledger: &'a dyn ApprovalLedger,
    pub audit: &'a dyn AuditSink,
    pub policy: TrustPolicy,
}

pub struct Adapter<'a> {
    root: &'a ManagedRoot,
    refs: &'a ReferenceSet,
    host_platform: &'a str,
    clock: ClockFn<'a>,
    fault: Option<FaultFn<'a>>,
    trust: Trust<'a>,
    mode: Mode,
}

/// The transaction record (journal). Reuses the lifecycle data types, with its own on-disk shape and digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Txn {
    schema_version: u32,
    txn_id: String,
    action: Action,
    tool_id: String,
    from_version: Option<String>,
    to_version: Option<String>,
    artifact_sha256: Option<String>,
    recipe_digest: String,
    approval_id: String,
    device_approval_id: Option<String>,
    platform_target: String,
    computer_id: String,
    state: TxnState,
    intents: Vec<String>,
    started_at: String,
    rollback: Option<RollbackReport>,
    owned: Vec<OwnedFile>,
    journal_digest: String,
}

fn txn_digest(t: &Txn) -> String {
    let mut blank = t.clone();
    blank.journal_digest = String::new();
    match serde_json::to_value(&blank) {
        Ok(v) => sha256_hex(&canonical_json(&v)),
        Err(_) => String::new(),
    }
}

struct Dirs {
    journal: Dir,
    locks: Dir,
    receipts: Dir,
    snapshots: Dir,
    staging: Dir,
    state: Dir,
    versions: Dir,
}

/// What a preflight found (it changes nothing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preflight {
    pub tool_id: String,
    pub version: String,
    pub action: Action,
    pub artifact_sha256: Option<String>,
    pub installed_version: Option<String>,
    pub already_applied: bool,
}

enum Class {
    Ours(Expect),
    Changed,
    Ambiguous,
    Unrecorded,
}

impl<'a> Adapter<'a> {
    /// The production adapter. It refuses every request: nothing real can pass the trust gate yet.
    pub fn production(
        root: &'a ManagedRoot,
        refs: &'a ReferenceSet,
        host_platform: &'a str,
        clock: ClockFn<'a>,
        trust: Trust<'a>,
    ) -> Adapter<'a> {
        Adapter { root, refs, host_platform, clock, fault: None, trust, mode: Mode::Production }
    }

    /// The only adapter that can act, and it exists in test builds only.
    #[cfg(test)]
    pub fn test_mode(
        root: &'a ManagedRoot,
        refs: &'a ReferenceSet,
        host_platform: &'a str,
        clock: ClockFn<'a>,
        fault: Option<FaultFn<'a>>,
        trust: Trust<'a>,
    ) -> Adapter<'a> {
        Adapter { root, refs, host_platform, clock, fault, trust, mode: Mode::TestSynthetic }
    }

    fn gate(&self) -> R<()> {
        match self.mode {
            Mode::Production => Err(TxnError::Refused("production_adapter_not_enabled")),
            Mode::TestSynthetic => Ok(()),
        }
    }

    fn hit(&self, p: Point) -> R<()> {
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

    fn dirs(&self) -> R<Dirs> {
        let r = self.root.dir();
        let open = |n: &str| r.child(n).map_err(fs_err);
        Ok(Dirs {
            journal: open("journal")?,
            locks: open("locks")?,
            receipts: open("receipts")?,
            snapshots: open("snapshots")?,
            staging: open("staging")?,
            state: open("state")?,
            versions: open("versions")?,
        })
    }

    // ---- journal and manifest -------------------------------------------------------------------------------------------------

    fn new_txn(&self, a: &Authority, action: Action, from: Option<String>, to: Option<String>, sha: Option<String>) -> Txn {
        let r = a.recipe.recipe();
        Txn {
            schema_version: 1,
            txn_id: format!("txn-{}", uuid::Uuid::new_v4().simple()),
            action,
            tool_id: r.tool_id.clone(),
            from_version: from,
            to_version: to,
            artifact_sha256: sha,
            recipe_digest: a.recipe.digest().to_string(),
            approval_id: a.approval.approval().approval_id.clone(),
            device_approval_id: None,
            platform_target: self.host_platform.to_string(),
            computer_id: self.root.computer_id().to_string(),
            state: TxnState::Started,
            intents: Vec::new(),
            started_at: self.now(),
            rollback: None,
            owned: Vec::new(),
            journal_digest: String::new(),
        }
    }

    /// Durable journal write: a fresh temporary file (exclusive create, unique name), fsync, atomic replace. A crash in between leaves
    /// an unrecognised temporary file, which recovery reports and never deletes.
    fn save(&self, d: &Dirs, t: &mut Txn) -> R<()> {
        t.journal_digest = txn_digest(t);
        let bytes = serde_json::to_vec(t).map_err(|_| TxnError::Refused("journal_unwritable"))?;
        let tmp = format!("{}.{}.tmp", t.txn_id, &uuid::Uuid::new_v4().simple().to_string()[..8]);
        let mut f = d.journal.create_file(&tmp).map_err(fs_err)?;
        f.write_all(&bytes).map_err(fs_err)?;
        f.sync_all().map_err(fs_err)?;
        drop(f);
        d.journal.rename_file(&tmp, &d.journal, &format!("{}.json", t.txn_id), true, None).map_err(fs_err)?;
        d.journal.sync();
        Ok(())
    }

    fn read_txn(d: &Dirs, name: &str) -> Option<Txn> {
        let bytes = d.journal.read_file(name, MAX_JSON).ok()?;
        let t: Txn = serde_json::from_slice(&bytes).ok()?;
        (txn_digest(&t) == t.journal_digest).then_some(t)
    }

    /// Step 1 of creating a file the transaction will own: the INTENT is durable before the file exists.
    fn intend(&self, d: &Dirs, t: &mut Txn, role: &str, rels: Vec<String>) -> R<()> {
        t.owned.push(OwnedFile { role: role.to_string(), phase: Phase::Intended, rels, sha256: None, size: None, identity: None });
        self.save(d, t)
    }

    fn mark(&self, d: &Dirs, t: &mut Txn, role: &str, phase: Phase, identity: Option<Identity>, content: Option<(String, u64)>) -> R<()> {
        if let Some(e) = t.owned.iter_mut().find(|e| e.role == role) {
            e.phase = phase;
            if let Some(i) = identity {
                e.identity = Some(pair(i));
            }
            if let Some((sha, size)) = content {
                e.sha256 = Some(sha);
                e.size = Some(size);
            }
        }
        self.save(d, t)
    }

    /// Step 2: exclusive create in `dir`; the identity comes from the created handle and is durable before any data is written.
    /// `after_create` is the injection point inside the irreducible window between the create and the record.
    fn create_owned(&self, d: &Dirs, t: &mut Txn, dir: &Dir, role: &str, name: &str, after_create: Point) -> R<FileH> {
        let file = dir.create_file(name).map_err(fs_err)?;
        let identity = file.stat().map_err(fs_err)?.identity;
        if let Err(e) = self.hit(after_create) {
            if matches!(e, TxnError::Refused(_)) {
                let _ = self.mark(d, t, role, Phase::Created, Some(identity), None);
            }
            return Err(e);
        }
        self.mark(d, t, role, Phase::Created, Some(identity), None)?;
        Ok(file)
    }

    /// Creates, fills, fsyncs and finalizes one owned file in one go (small files: pointer, receipt, lock).
    #[allow(clippy::too_many_arguments)]
    fn write_owned(
        &self,
        d: &Dirs,
        t: &mut Txn,
        dir: &Dir,
        role: &str,
        rels: Vec<String>,
        name: &str,
        bytes: &[u8],
        points: (Point, Point),
    ) -> R<Identity> {
        self.intend(d, t, role, rels)?;
        let mut f = self.create_owned(d, t, dir, role, name, points.0)?;
        f.write_all(bytes).map_err(fs_err)?;
        f.sync_all().map_err(fs_err)?;
        let id = f.stat().map_err(fs_err)?.identity;
        drop(f);
        self.hit(points.1)?;
        self.mark(d, t, role, Phase::Final, None, Some((sha256_hex_bytes(bytes), bytes.len() as u64)))?;
        Ok(id)
    }

    fn entry<'t>(t: &'t Txn, role: &str) -> Option<&'t OwnedFile> {
        t.owned.iter().find(|e| e.role == role)
    }

    fn identity_of(t: &Txn, role: &str) -> Option<Identity> {
        Self::entry(t, role).and_then(|e| e.identity).map(unpair)
    }

    /// What may be done with the file `name` in `dir` given the manifest entry (found by role or by place).
    fn classify(dir: &Dir, name: &str, entry: Option<&OwnedFile>) -> Class {
        let Some(e) = entry else { return Class::Unrecorded };
        match dir.stat(name) {
            Ok(Some(s)) if s.kind == Kind::File => match e.phase {
                Phase::Intended => Class::Ambiguous,
                Phase::Created => match e.identity.map(unpair) {
                    Some(i) if i == s.identity && s.links == 1 => Class::Ours(Expect { identity: Some(i), size: None, sha256: None }),
                    _ => Class::Changed,
                },
                Phase::Final => {
                    let want = Expect { identity: e.identity.map(unpair), size: e.size, sha256: e.sha256.clone() };
                    match dir.check_file(name, &want) {
                        Ok(_) => Class::Ours(want),
                        Err(_) => Class::Changed,
                    }
                }
            },
            _ => Class::Changed,
        }
    }

    fn by_rel<'t>(t: &'t Txn, rel: &str) -> Option<&'t OwnedFile> {
        t.owned.iter().find(|e| e.rels.iter().any(|r| r == rel))
    }

    fn refusal(unexpected: usize, changed: usize, ambiguous: usize) -> String {
        let mut parts: Vec<String> = Vec::new();
        if ambiguous > 0 {
            parts.push(format!(
                "{ambiguous} ambiguous orphan file(s): a crash left a file where this transaction only recorded the intent to create one, so it cannot be proven to be this transaction's; it was left untouched and nothing was deleted"
            ));
        }
        if unexpected > 0 {
            parts.push(format!(
                "{unexpected} unexpected entr{} that this transaction did not create; nothing was deleted",
                if unexpected == 1 { "y" } else { "ies" }
            ));
        }
        if changed > 0 {
            parts.push(format!(
                "{changed} recorded file(s) are no longer the file this transaction wrote (identity, links or content differ); they were left untouched and nothing was deleted"
            ));
        }
        parts.join("; ")
    }

    /// Removes a folder of this transaction (`parent/folder`) after validating EVERY entry against the manifest: all or nothing, files
    /// through verified deletion, then the emptied folder (never recursive).
    fn remove_owned_folder(parent: &Dir, folder: &str, rel_prefix: &str, t: &Txn) -> Result<(), String> {
        let child = match parent.child(folder) {
            Ok(c) => c,
            Err(FsError::NotFound) => return Ok(()),
            Err(e) => return Err(format!("the folder cannot be opened safely ({}); it was left untouched", e.code())),
        };
        let entries = child.list().map_err(|e| format!("the folder cannot be listed ({})", e.code()))?;
        let (mut unexpected, mut changed, mut ambiguous) = (0, 0, 0);
        let mut plan: Vec<(String, Expect)> = Vec::new();
        for (name, kind) in entries {
            if kind != Kind::File {
                unexpected += 1;
                continue;
            }
            match Self::classify(&child, &name, Self::by_rel(t, &format!("{rel_prefix}/{name}"))) {
                Class::Ours(e) => plan.push((name, e)),
                Class::Changed => changed += 1,
                Class::Ambiguous => ambiguous += 1,
                Class::Unrecorded => unexpected += 1,
            }
        }
        if unexpected + changed + ambiguous > 0 {
            return Err(Self::refusal(unexpected, changed, ambiguous));
        }
        for (name, expect) in plan {
            child
                .remove_file_verified(&name, &expect)
                .map_err(|e| format!("cannot remove a file this transaction created ({})", e.code()))?;
        }
        drop(child);
        parent.remove_dir(folder).map_err(|e| format!("the folder is not empty or cannot be removed ({})", e.code()))
    }

    /// The snapshot folder: `pointer.json` and `versions/<v>/artifact.bin`, nothing else.
    fn remove_snapshot(d: &Dirs, t: &Txn) -> Result<(), String> {
        let txn = &t.txn_id;
        let snap = match d.snapshots.child(txn) {
            Ok(c) => c,
            Err(FsError::NotFound) => return Ok(()),
            Err(e) => return Err(format!("the snapshot cannot be opened safely ({}); it was left untouched", e.code())),
        };
        let top = snap.list().map_err(|e| format!("the snapshot cannot be listed ({})", e.code()))?;
        let (mut unexpected, mut changed, mut ambiguous) = (0, 0, 0);
        let mut plan: Vec<(Option<String>, String, Expect)> = Vec::new(); // (version folder, file, expect)
        let mut version_folders: Vec<String> = Vec::new();
        for (name, kind) in &top {
            match (name.as_str(), kind) {
                ("pointer.json", Kind::File) => {
                    match Self::classify(&snap, name, Self::by_rel(t, &format!("snapshots/{txn}/pointer.json"))) {
                        Class::Ours(e) => plan.push((None, name.clone(), e)),
                        Class::Changed => changed += 1,
                        Class::Ambiguous => ambiguous += 1,
                        Class::Unrecorded => unexpected += 1,
                    }
                }
                ("versions", Kind::Dir) => {
                    let vs = snap.child("versions").map_err(|e| format!("the snapshot cannot be opened safely ({})", e.code()))?;
                    for (v, vk) in vs.list().map_err(|e| format!("the snapshot cannot be listed ({})", e.code()))? {
                        if vk != Kind::Dir {
                            unexpected += 1;
                            continue;
                        }
                        let vd = vs.child(&v).map_err(|e| format!("the snapshot cannot be opened safely ({})", e.code()))?;
                        for (f, fk) in vd.list().map_err(|e| format!("the snapshot cannot be listed ({})", e.code()))? {
                            if fk == Kind::File && f == "artifact.bin" {
                                match Self::classify(&vd, &f, Self::by_rel(t, &format!("snapshots/{txn}/versions/{v}/artifact.bin"))) {
                                    Class::Ours(e) => plan.push((Some(v.clone()), f, e)),
                                    Class::Changed => changed += 1,
                                    Class::Ambiguous => ambiguous += 1,
                                    Class::Unrecorded => unexpected += 1,
                                }
                            } else {
                                unexpected += 1;
                            }
                        }
                        version_folders.push(v);
                    }
                }
                _ => unexpected += 1,
            }
        }
        if unexpected + changed + ambiguous > 0 {
            return Err(Self::refusal(unexpected, changed, ambiguous));
        }
        for (v, f, e) in &plan {
            match v {
                None => snap.remove_file_verified(f, e),
                Some(v) => snap.child("versions").and_then(|vs| vs.child(v)).and_then(|vd| vd.remove_file_verified(f, e)),
            }
            .map_err(|e| format!("cannot remove a file this transaction created ({})", e.code()))?;
        }
        if !version_folders.is_empty() {
            let vs = snap.child("versions").map_err(|e| e.code().to_string())?;
            for v in &version_folders {
                vs.remove_dir(v).map_err(|e| format!("cannot remove a snapshot folder ({})", e.code()))?;
            }
            drop(vs);
            snap.remove_dir("versions").map_err(|e| format!("cannot remove a snapshot folder ({})", e.code()))?;
        }
        drop(snap);
        d.snapshots.remove_dir(txn).map_err(|e| format!("the snapshot folder is not empty or cannot be removed ({})", e.code()))
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Transactions
// ---------------------------------------------------------------------------------------------------------------------------------

fn lock_name(tool: &str) -> String {
    format!("{tool}.lock")
}

fn pointer_name(tool: &str) -> String {
    format!("{tool}.json")
}

impl<'a> Adapter<'a> {
    // ---- preflight (offline, changes nothing) -------------------------------------------------------------------------------

    fn read_pointer(d: &Dirs, tool: &str) -> R<Option<Pointer>> {
        match d.state.read_file(&pointer_name(tool), 4096) {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|_| TxnError::Refused("pointer_unreadable")),
            Err(FsError::NotFound) => Ok(None),
            Err(_) => Err(TxnError::Refused("pointer_unreadable")),
        }
    }

    fn pending_for(d: &Dirs, tool: &str, me: &str) -> R<Option<String>> {
        for (name, kind) in d.journal.list().map_err(fs_err)? {
            if kind != Kind::File || !name.ends_with(".json") {
                continue;
            }
            match Self::read_txn(d, &name) {
                Some(t) if t.tool_id == tool && t.txn_id != me && !t.state.terminal() => return Ok(Some(t.txn_id)),
                Some(_) => {}
                // an unreadable journal could belong to any tool: do not proceed past it
                None => return Ok(Some("unreadable-journal".to_string())),
            }
        }
        Ok(None)
    }

    fn ensure_free(&self, d: &Dirs, tool: &str) -> R<()> {
        if d.locks.stat(&lock_name(tool)).map_err(fs_err)?.is_some() {
            return Err(TxnError::Busy);
        }
        match Self::pending_for(d, tool, "")? {
            Some(p) => Err(TxnError::Pending(p)),
            None => Ok(()),
        }
    }

    fn read_receipt_in(d: &Dirs, txn: &str) -> Option<Receipt> {
        let r: Receipt = serde_json::from_slice(&d.receipts.read_file(&format!("{txn}.json"), MAX_JSON).ok()?).ok()?;
        (receipt_digest(&r) == r.receipt_digest).then_some(r)
    }

    /// Everything that can be decided without changing anything. Returns (artifact digest, installed pointer, existing receipt when the
    /// requested state already holds).
    fn preflight_apply(&self, d: &Dirs, a: &Authority, action: Action, artifact: &[u8]) -> R<(String, Option<Pointer>, Option<Receipt>)> {
        check_authority(a, action, self.root.computer_id(), self.host_platform, (self.clock)())?;
        let r = a.recipe.recipe();
        if !crate::managed_fs::valid_name(&r.tool_id) || !crate::managed_fs::valid_name(&r.version) {
            return Err(TxnError::Refused("unsafe_path_component"));
        }
        let sha = check_artifact(self.refs, self.host_platform, a, artifact)?;
        self.ensure_free(d, &r.tool_id)?;
        let current = Self::read_pointer(d, &r.tool_id)?;
        let existing = |p: &Pointer| Self::read_receipt_in(d, &p.txn).ok_or(TxnError::Refused("installed_state_has_no_valid_receipt"));
        match (action, &current) {
            (Action::Install | Action::Update, Some(p)) if p.version == r.version && p.artifact_sha256 == sha => {
                return Ok((sha, current.clone(), Some(existing(p)?)));
            }
            (Action::Install, Some(_)) => return Err(TxnError::Refused("already_installed_use_update")),
            (Action::Update, Some(p)) if Some(p.version.as_str()) != a.approval.approval().from_version.as_deref() => {
                return Err(TxnError::Refused("installed_version_differs_from_the_approved_update_source"));
            }
            (Action::Update, None) => return Err(TxnError::Refused("not_installed")),
            _ => {}
        }
        Ok((sha, current, None))
    }

    /// Offline preflight: authority, names, artifact identity against the recipe and the authenticated references, free tool, state.
    pub fn preflight(&self, a: &Authority, action: Action, artifact: Option<&[u8]>) -> R<Preflight> {
        self.gate()?;
        let d = self.dirs()?;
        let r = a.recipe.recipe();
        match (action, artifact) {
            (Action::Install | Action::Update, Some(bytes)) => {
                let (sha, current, existing) = self.preflight_apply(&d, a, action, bytes)?;
                Ok(Preflight {
                    tool_id: r.tool_id.clone(),
                    version: r.version.clone(),
                    action,
                    artifact_sha256: Some(sha),
                    installed_version: current.map(|p| p.version),
                    already_applied: existing.is_some(),
                })
            }
            (Action::Uninstall, None) => {
                check_authority(a, action, self.root.computer_id(), self.host_platform, (self.clock)())?;
                self.ensure_free(&d, &r.tool_id)?;
                let cur = Self::read_pointer(&d, &r.tool_id)?;
                Ok(Preflight {
                    tool_id: r.tool_id.clone(),
                    version: r.version.clone(),
                    action,
                    artifact_sha256: None,
                    installed_version: cur.as_ref().map(|p| p.version.clone()),
                    already_applied: cur.is_none(),
                })
            }
            _ => Err(TxnError::Refused("artifact_presence_does_not_match_the_action")),
        }
    }

    // ---- lock, authorization, abort -------------------------------------------------------------------------------------------

    fn take_lock(&self, d: &Dirs, t: &mut Txn) -> R<()> {
        let name = lock_name(&t.tool_id);
        self.intend(d, t, "lock", vec![format!("locks/{name}")])?;
        match d.locks.create_file(&name) {
            Ok(mut f) => {
                let id = f.stat().map_err(fs_err)?.identity;
                self.mark(d, t, "lock", Phase::Created, Some(id), None)?;
                f.write_all(t.txn_id.as_bytes()).map_err(fs_err)?;
                f.sync_all().map_err(fs_err)?;
                drop(f);
                self.mark(d, t, "lock", Phase::Final, None, Some((sha256_hex_bytes(t.txn_id.as_bytes()), t.txn_id.len() as u64)))
            }
            Err(e) => {
                // the lock is not ours: forget the intent, close the journal, leave whatever is there alone
                t.owned.retain(|x| x.role != "lock");
                t.state = TxnState::RolledBack;
                let _ = self.save(d, t);
                Err(if e == FsError::Exists { TxnError::Busy } else { fs_err(e) })
            }
        }
    }

    fn release_lock(d: &Dirs, t: &Txn) -> Result<(), String> {
        let name = lock_name(&t.tool_id);
        let Some(entry) = Self::entry(t, "lock") else { return Ok(()) };
        match d.locks.stat(&name) {
            Ok(None) => Ok(()),
            Ok(Some(_)) => match Self::classify(&d.locks, &name, Some(entry)) {
                Class::Ours(e) => d.locks.remove_file_verified(&name, &e).map_err(|e| format!("cannot release the lock ({})", e.code())),
                Class::Ambiguous => Err("the lock file cannot be proven to be this transaction's (crash before its identity was recorded); it was left in place".to_string()),
                _ => Err("the lock file is no longer the one this transaction created; it was left in place".to_string()),
            },
            Err(e) => Err(format!("the lock cannot be examined ({})", e.code())),
        }
    }

    /// For a transaction that is already over: removes its lock if it is provably still its own; a lock that belongs to another
    /// (later, possibly unfinished) transaction is simply not ours, and an unprovable one is reported.
    fn release_finished_lock(d: &Dirs, t: &Txn) -> Result<(), String> {
        let name = lock_name(&t.tool_id);
        let Some(entry) = Self::entry(t, "lock") else { return Ok(()) };
        match d.locks.stat(&name) {
            Ok(Some(_)) => match Self::classify(&d.locks, &name, Some(entry)) {
                Class::Ours(e) => d.locks.remove_file_verified(&name, &e).map_err(|e| format!("cannot release the lock ({})", e.code())),
                Class::Ambiguous => Err("the lock file cannot be proven to be this transaction's (crash before its identity was recorded); it was left in place".to_string()),
                Class::Changed | Class::Unrecorded => Ok(()),
            },
            Ok(None) => Ok(()),
            Err(e) => Err(format!("the lock cannot be examined ({})", e.code())),
        }
    }

    /// The transaction ends before any change to managed state: release the lock and close the journal.
    fn abort(&self, d: &Dirs, t: &mut Txn) {
        t.state = if Self::release_lock(d, t).is_ok() { TxnState::RolledBack } else { TxnState::RollbackFailed };
        let _ = self.save(d, t);
    }

    fn device_authorize(&self, d: &Dirs, t: &mut Txn, a: &Authority, dev: &DeviceApproval) -> R<()> {
        let r = a.recipe.recipe();
        let ap = a.approval.approval();
        let effects = a.recipe.permissions_digest();
        let want = Expected {
            action: t.action,
            tool_id: &r.tool_id,
            version: &r.version,
            platform_target: self.host_platform,
            computer_id: self.root.computer_id(),
            recipe_digest: a.recipe.digest(),
            artifact_sha256: &r.artifact.sha256,
            effects_digest: &effects,
            scope: &r.target.scope,
            project_alias: ap.project_alias.as_deref(),
        };
        let outcome = if dev.approval.approval_id != ap.approval_id {
            Err(device_trust::TrustError::BindingMismatch)
        } else {
            device_trust::authorize(
                dev,
                &want,
                &t.txn_id,
                (self.clock)(),
                self.trust.keys,
                self.trust.ledger,
                self.trust.audit,
                self.trust.policy,
            )
        };
        match outcome {
            Ok(auth) => {
                t.device_approval_id = Some(auth.approval_id().to_string());
                self.save(d, t)
            }
            Err(e) => {
                self.abort(d, t);
                Err(TxnError::Refused(e.code()))
            }
        }
    }

    // ---- install / update ---------------------------------------------------------------------------------------------------

    pub fn install(&self, a: &Authority, dev: &DeviceApproval, artifact: &[u8]) -> R<Outcome> {
        self.apply(a, dev, Action::Install, artifact)
    }

    pub fn update(&self, a: &Authority, dev: &DeviceApproval, artifact: &[u8]) -> R<Outcome> {
        self.apply(a, dev, Action::Update, artifact)
    }

    fn apply(&self, a: &Authority, dev: &DeviceApproval, action: Action, artifact: &[u8]) -> R<Outcome> {
        self.gate()?;
        let d = self.dirs()?;
        let (sha, current, existing) = self.preflight_apply(&d, a, action, artifact)?;
        if let Some(receipt) = existing {
            return Ok(Outcome { receipt, already_applied: true });
        }
        let r = a.recipe.recipe();
        let mut t = self.new_txn(a, action, current.as_ref().map(|p| p.version.clone()), Some(r.version.clone()), Some(sha));
        self.take_lock(&d, &mut t)?;
        match Self::pending_for(&d, &t.tool_id, &t.txn_id) {
            Ok(None) => {}
            Ok(Some(p)) => {
                self.abort(&d, &mut t);
                return Err(TxnError::Pending(p));
            }
            Err(e) => {
                self.abort(&d, &mut t);
                return Err(e);
            }
        }
        // the first thing that consumes anything: the single-use device approval, after the lock and before any change
        self.device_authorize(&d, &mut t, a, dev)?;
        let result = self.run_apply(&d, a, &mut t, artifact, current.is_some());
        self.finish(&d, t, result)
    }

    fn run_apply(&self, d: &Dirs, a: &Authority, t: &mut Txn, artifact: &[u8], had_pointer: bool) -> R<Receipt> {
        let tool = t.tool_id.clone();
        let version = a.recipe.recipe().version.clone();
        let txn = t.txn_id.clone();
        self.hit(Point::AfterLock)?;
        // 1. snapshot of the pointer this transaction may replace
        t.intents.push("snapshot_dir".to_string());
        self.save(d, t)?;
        let snap = d.snapshots.create_child(&txn).map_err(fs_err)?;
        if had_pointer {
            let pname = pointer_name(&tool);
            let mut pf = d.state.open_file(&pname).map_err(fs_err)?;
            let st = pf.stat().map_err(fs_err)?;
            let bytes = pf.read_bounded(4096).map_err(fs_err)?;
            drop(pf);
            // what the active pointer is right now (not created by this transaction; recorded so a rollback can recognise it)
            t.owned.push(OwnedFile {
                role: "previous_pointer".to_string(),
                phase: Phase::Final,
                rels: vec![format!("state/{pname}#previous")],
                sha256: Some(sha256_hex_bytes(&bytes)),
                size: Some(bytes.len() as u64),
                identity: Some(pair(st.identity)),
            });
            self.intend(d, t, "snapshot_pointer", vec![format!("snapshots/{txn}/pointer.json")])?;
            self.hit(Point::SnapshotBeforeCreate)?;
            let mut f = self.create_owned(d, t, &snap, "snapshot_pointer", "pointer.json", Point::SnapshotAfterCreate)?;
            if let Some(fault) = self.fault.and_then(|h| h(Point::SnapshotDuringWrite)) {
                f.write_all(&bytes[..bytes.len() / 2]).map_err(fs_err)?;
                return Err(if fault == Fault::Crash {
                    TxnError::Interrupted(Point::SnapshotDuringWrite)
                } else {
                    TxnError::Refused("injected_failure")
                });
            }
            f.write_all(&bytes).map_err(fs_err)?;
            f.sync_all().map_err(fs_err)?;
            drop(f);
            self.hit(Point::SnapshotAfterSync)?;
            self.mark(d, t, "snapshot_pointer", Phase::Final, None, Some((sha256_hex_bytes(&bytes), bytes.len() as u64)))?;
            self.hit(Point::SnapshotAfterRecord)?;
        }
        t.state = TxnState::Snapshotted;
        self.save(d, t)?;
        self.hit(Point::AfterSnapshot)?;
        // 2. stage, in a folder of this transaction only
        t.intents.push("stage_dir".to_string());
        self.save(d, t)?;
        let stage = d.staging.create_child(&txn).map_err(fs_err)?;
        self.intend(
            d,
            t,
            "staged_artifact",
            vec![
                format!("staging/{txn}/artifact.bin.part"),
                format!("staging/{txn}/artifact.bin"),
                format!("versions/{tool}/{version}/artifact.bin"),
            ],
        )?;
        self.hit(Point::StageBeforeCreate)?;
        let mut file = self.create_owned(d, t, &stage, "staged_artifact", "artifact.bin.part", Point::StageAfterCreate)?;
        let staged_id = file.stat().map_err(fs_err)?.identity;
        if let Some(fault) = self.fault.and_then(|h| h(Point::DuringStageWrite)) {
            file.write_all(&artifact[..artifact.len() / 2]).map_err(fs_err)?;
            return Err(if fault == Fault::Crash {
                TxnError::Interrupted(Point::DuringStageWrite)
            } else {
                TxnError::Refused("injected_failure")
            });
        }
        file.write_all(artifact).map_err(fs_err)?;
        file.sync_all().map_err(fs_err)?;
        drop(file);
        self.hit(Point::StageAfterSync)?;
        let recipe_sha = a.recipe.recipe().artifact.sha256.clone();
        self.mark(d, t, "staged_artifact", Phase::Final, None, Some((recipe_sha.clone(), artifact.len() as u64)))?;
        stage.rename_file("artifact.bin.part", &stage, "artifact.bin", false, Some(staged_id)).map_err(fs_err)?;
        t.state = TxnState::Staged;
        self.save(d, t)?;
        self.hit(Point::AfterStageWrite)?;
        // 3. verify what was staged: the bytes on disk, read through a handle, and that it is still the file this transaction wrote
        let (staged_sha, _) = stage.open_file("artifact.bin").and_then(|mut f| f.sha256_bounded(MAX_ARTIFACT)).map_err(fs_err)?;
        if staged_sha != recipe_sha {
            return Err(TxnError::Refused("staged_content_does_not_match"));
        }
        let staged_ours = |t: &Txn| matches!(Self::classify(&stage, "artifact.bin", Self::entry(t, "staged_artifact")), Class::Ours(_));
        if !staged_ours(t) {
            return Err(TxnError::Refused("staged_file_is_not_the_file_this_transaction_wrote"));
        }
        t.state = TxnState::Verified;
        self.save(d, t)?;
        self.hit(Point::AfterVerify)?;
        if (self.clock)() >= a.approval.expires_at() {
            return Err(TxnError::Refused("approval_expired"));
        }
        // 4. activate: version folder first, then one atomic pointer switch
        let vtool = match d.versions.child(&tool) {
            Ok(v) => v,
            Err(FsError::NotFound) => {
                t.intents.push("version_tool_dir".to_string());
                self.save(d, t)?;
                d.versions.create_child(&tool).map_err(fs_err)?
            }
            Err(e) => return Err(fs_err(e)),
        };
        if vtool.stat(&version).map_err(fs_err)?.is_some() {
            return Err(TxnError::Refused("version_folder_already_exists"));
        }
        self.hit(Point::BeforeVersionDirCreate)?;
        t.intents.push(format!("version_dir:{version}"));
        self.save(d, t)?;
        let vdir = vtool.create_child(&version).map_err(fs_err)?;
        self.hit(Point::AfterVersionDirCreate)?;
        if !staged_ours(t) {
            return Err(TxnError::Refused("staged_file_is_not_the_file_this_transaction_wrote"));
        }
        stage.rename_file("artifact.bin", &vdir, "artifact.bin", false, Some(staged_id)).map_err(fs_err)?;
        drop(vdir);
        drop(vtool);
        self.hit(Point::BeforePointerWrite)?;
        t.intents.push("pointer_switch".to_string());
        self.save(d, t)?;
        let pname = pointer_name(&tool);
        let pointer = Pointer { tool: tool.clone(), version: version.clone(), artifact_sha256: recipe_sha, txn: txn.clone() };
        let pbytes = serde_json::to_vec(&pointer).map_err(|_| TxnError::Refused("pointer_unwritable"))?;
        self.hit(Point::PointerBeforeCreate)?;
        let new_id = self.write_owned(
            d,
            t,
            &d.state,
            "new_pointer",
            vec![format!("state/{pname}.new"), format!("state/{pname}")],
            &format!("{pname}.new"),
            &pbytes,
            (Point::PointerAfterCreate, Point::PointerAfterSync),
        )?;
        d.state.rename_file(&format!("{pname}.new"), &d.state, &pname, true, Some(new_id)).map_err(fs_err)?;
        d.state.sync();
        t.state = TxnState::Activated;
        self.save(d, t)?;
        self.hit(Point::AfterPointerSwitch)?;
        // 5. durable receipt, then the journal says committed
        self.hit(Point::BeforeReceiptWrite)?;
        let receipt = self.write_receipt(d, t)?;
        self.hit(Point::AfterReceiptWrite)?;
        self.hit(Point::BeforeJournalCommit)?;
        t.state = TxnState::Committed;
        self.save(d, t)?;
        Ok(receipt)
    }

    fn kept(&self, tool: &str) -> Vec<String> {
        match self.root.dir().child("data").and_then(|dd| dd.stat(tool)) {
            Ok(Some(_)) => vec!["user_data".to_string()],
            _ => vec![],
        }
    }

    fn write_receipt(&self, d: &Dirs, t: &mut Txn) -> R<Receipt> {
        let mut r = Receipt {
            schema_version: 1,
            txn_id: t.txn_id.clone(),
            action: t.action,
            tool_id: t.tool_id.clone(),
            from_version: t.from_version.clone(),
            to_version: t.to_version.clone(),
            artifact_sha256: t.artifact_sha256.clone(),
            recipe_digest: t.recipe_digest.clone(),
            approval_id: t.approval_id.clone(),
            platform_target: t.platform_target.clone(),
            computer_id: t.computer_id.clone(),
            synthetic: true,
            completed_at: self.now(),
            state: "committed".to_string(),
            kept: self.kept(&t.tool_id),
            rollback: RollbackReport::not_needed(),
            receipt_digest: String::new(),
        };
        r.receipt_digest = receipt_digest(&r);
        let bytes = serde_json::to_vec(&r).map_err(|_| TxnError::Refused("receipt_unwritable"))?;
        let name = format!("{}.json", t.txn_id);
        self.write_owned(
            d,
            t,
            &d.receipts,
            "receipt",
            vec![format!("receipts/{name}")],
            &name,
            &bytes,
            (Point::ReceiptAfterCreate, Point::ReceiptAfterSync),
        )?;
        Ok(r)
    }

    // ---- uninstall --------------------------------------------------------------------------------------------------------

    fn receipt_vouches(d: &Dirs, tool: &str, version: &str, sha: &str) -> bool {
        let Ok(list) = d.receipts.list() else { return false };
        list.into_iter().filter(|(n, k)| *k == Kind::File && n.ends_with(".json")).any(|(n, _)| {
            d.receipts
                .read_file(&n, MAX_JSON)
                .ok()
                .and_then(|b| serde_json::from_slice::<Receipt>(&b).ok())
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

    /// Everything an uninstall would move, recognised BEFORE anything moves: the active pointer, and every version folder must hold
    /// exactly one single-link `artifact.bin` whose digest a committed receipt vouches for.
    fn uninstall_manifest(&self, d: &Dirs, t: &Txn) -> R<Vec<OwnedFile>> {
        let (tool, txn) = (&t.tool_id, &t.txn_id);
        let refused = |c: &'static str| TxnError::Refused(c);
        let mut out = Vec::new();
        let pname = pointer_name(tool);
        let mut pf = d.state.open_file(&pname).map_err(|_| refused("unrecognised_installed_content"))?;
        let ps = pf.stat().map_err(fs_err)?;
        if ps.links != 1 {
            return Err(refused("unrecognised_installed_content"));
        }
        let (psha, psize) = pf.sha256_bounded(4096).map_err(|_| refused("unrecognised_installed_content"))?;
        out.push(OwnedFile {
            role: "removed_pointer".to_string(),
            phase: Phase::Final,
            rels: vec![format!("state/{pname}"), format!("snapshots/{txn}/pointer.json")],
            sha256: Some(psha),
            size: Some(psize),
            identity: Some(pair(ps.identity)),
        });
        drop(pf);
        match d.versions.child(tool) {
            Ok(vtool) => {
                for (v, kind) in vtool.list().map_err(fs_err)? {
                    if kind != Kind::Dir || !crate::managed_fs::valid_name(&v) {
                        return Err(refused("unrecognised_installed_content"));
                    }
                    let vd = vtool.child(&v).map_err(fs_err)?;
                    if vd.list().map_err(fs_err)? != vec![("artifact.bin".to_string(), Kind::File)] {
                        return Err(refused("unrecognised_installed_content"));
                    }
                    let mut f = vd.open_file("artifact.bin").map_err(fs_err)?;
                    let st = f.stat().map_err(fs_err)?;
                    let (sha, size) = f.sha256_bounded(MAX_ARTIFACT).map_err(|_| refused("unrecognised_installed_content"))?;
                    if st.links != 1 || !Self::receipt_vouches(d, tool, &v, &sha) {
                        return Err(refused("installed_content_has_no_matching_receipt"));
                    }
                    out.push(OwnedFile {
                        role: format!("removed_version:{v}"),
                        phase: Phase::Final,
                        rels: vec![format!("versions/{tool}/{v}/artifact.bin"), format!("snapshots/{txn}/versions/{v}/artifact.bin")],
                        sha256: Some(sha),
                        size: Some(size),
                        identity: Some(pair(st.identity)),
                    });
                }
            }
            Err(FsError::NotFound) => {}
            Err(e) => return Err(fs_err(e)),
        }
        Ok(out)
    }

    pub fn uninstall(&self, a: &Authority, dev: &DeviceApproval) -> R<Outcome> {
        self.gate()?;
        let d = self.dirs()?;
        check_authority(a, Action::Uninstall, self.root.computer_id(), self.host_platform, (self.clock)())?;
        let r = a.recipe.recipe();
        if !crate::managed_fs::valid_name(&r.tool_id) {
            return Err(TxnError::Refused("unsafe_path_component"));
        }
        self.ensure_free(&d, &r.tool_id)?;
        let Some(cur) = Self::read_pointer(&d, &r.tool_id)? else { return Err(TxnError::Refused("not_installed")) };
        if cur.version != r.version || cur.artifact_sha256 != r.artifact.sha256 {
            return Err(TxnError::Refused("installed_version_differs_from_the_approved_uninstall"));
        }
        let mut t = self.new_txn(a, Action::Uninstall, Some(cur.version.clone()), None, Some(cur.artifact_sha256.clone()));
        self.take_lock(&d, &mut t)?;
        match Self::pending_for(&d, &t.tool_id, &t.txn_id) {
            Ok(None) => {}
            Ok(Some(p)) => {
                self.abort(&d, &mut t);
                return Err(TxnError::Pending(p));
            }
            Err(e) => {
                self.abort(&d, &mut t);
                return Err(e);
            }
        }
        self.device_authorize(&d, &mut t, a, dev)?;
        // before anything moves: every installed item must be recognised
        match self.uninstall_manifest(&d, &t) {
            Ok(items) => t.owned.extend(items),
            Err(e) => {
                self.abort(&d, &mut t);
                return Err(e);
            }
        }
        if let Err(e) = self.save(&d, &mut t) {
            self.abort(&d, &mut t);
            return Err(e);
        }
        let result = self.run_uninstall(&d, &mut t);
        self.finish(&d, t, result)
    }

    fn run_uninstall(&self, d: &Dirs, t: &mut Txn) -> R<Receipt> {
        let (tool, txn) = (t.tool_id.clone(), t.txn_id.clone());
        self.hit(Point::AfterLock)?;
        t.intents.push("snapshot_dir".to_string());
        self.save(d, t)?;
        let snap = d.snapshots.create_child(&txn).map_err(fs_err)?;
        t.state = TxnState::Snapshotted;
        self.save(d, t)?;
        self.hit(Point::AfterSnapshot)?;
        // the pointer is MOVED into the snapshot (the removal); the version artifacts likewise: both can be put back
        t.intents.push("pointer_removed".to_string());
        self.save(d, t)?;
        let pname = pointer_name(&tool);
        let pid = Self::identity_of(t, "removed_pointer");
        d.state.rename_file(&pname, &snap, "pointer.json", false, pid).map_err(fs_err)?;
        self.hit(Point::UninstallAfterPointerMoved)?;
        t.intents.push("versions_moved".to_string());
        self.save(d, t)?;
        let versions: Vec<(String, Option<Identity>)> = t
            .owned
            .iter()
            .filter_map(|e| e.role.strip_prefix("removed_version:").map(|v| (v.to_string(), e.identity.map(unpair))))
            .collect();
        if !versions.is_empty() {
            let svs = snap.create_child("versions").map_err(fs_err)?;
            let vtool = d.versions.child(&tool).map_err(fs_err)?;
            for (v, id) in &versions {
                let sv = svs.create_child(v).map_err(fs_err)?;
                let vd = vtool.child(v).map_err(fs_err)?;
                vd.rename_file("artifact.bin", &sv, "artifact.bin", false, *id).map_err(fs_err)?;
                drop(vd);
                vtool.remove_dir(v).map_err(fs_err)?;
            }
            drop(vtool);
            d.versions.remove_dir(&tool).map_err(fs_err)?;
        }
        self.hit(Point::UninstallAfterVersionsMoved)?;
        t.state = TxnState::Activated;
        self.save(d, t)?;
        self.hit(Point::BeforeReceiptWrite)?;
        let receipt = self.write_receipt(d, t)?;
        self.hit(Point::AfterReceiptWrite)?;
        self.hit(Point::BeforeJournalCommit)?;
        t.state = TxnState::Committed;
        self.save(d, t)?;
        Ok(receipt)
    }

    // ---- common tail, cleanup, rollback -----------------------------------------------------------------------------------------

    fn finish(&self, d: &Dirs, mut t: Txn, result: R<Receipt>) -> R<Outcome> {
        match result {
            Ok(receipt) => {
                let _ = self.cleanup_after_commit(d, &t);
                let _ = Self::release_lock(d, &t);
                Ok(Outcome { receipt, already_applied: false })
            }
            Err(TxnError::Interrupted(p)) => Err(TxnError::Interrupted(p)),
            Err(e) => {
                let code = match e {
                    TxnError::Refused(c) => c,
                    TxnError::Failed { code, .. } => code,
                    _ => "failed",
                };
                let report = self.roll_back(d, &mut t);
                if report.status == "restored" || report.status == "not_needed" {
                    let _ = self.cleanup_after_commit(d, &t);
                    let _ = Self::release_lock(d, &t);
                }
                Err(TxnError::Failed { code, rollback: report })
            }
        }
    }

    /// Removes what a finished transaction leaves behind, but only what the manifest proves to be its own. Returns why something stayed.
    fn cleanup_after_commit(&self, d: &Dirs, t: &Txn) -> Option<String> {
        let mut why: Option<String> = None;
        if let Err(e) = Self::remove_owned_folder(&d.staging, &t.txn_id, &format!("staging/{}", t.txn_id), t) {
            why = Some(format!("staging: {e}"));
        }
        if let Err(e) = Self::remove_snapshot(d, t) {
            why.get_or_insert(format!("snapshot: {e}"));
        }
        why
    }

    fn roll_back(&self, d: &Dirs, t: &mut Txn) -> RollbackReport {
        let mut undone: Vec<String> = Vec::new();
        let mut not_undone: Vec<NotUndone> = Vec::new();
        // a receipt of a transaction that did not commit would be a false statement: it goes first
        if let Some(entry) = Self::entry(t, "receipt") {
            let name = format!("{}.json", t.txn_id);
            if matches!(d.receipts.stat(&name), Ok(Some(_))) {
                match Self::classify(&d.receipts, &name, Some(entry)) {
                    Class::Ours(e) => match d.receipts.remove_file_verified(&name, &e) {
                        Ok(()) => undone.push("uncommitted receipt removed".to_string()),
                        Err(e) => not_undone
                            .push(NotUndone { what: "uncommitted receipt".to_string(), why: format!("cannot remove it ({})", e.code()) }),
                    },
                    Class::Ambiguous => not_undone.push(NotUndone { what: "uncommitted receipt".to_string(), why: Self::refusal(0, 0, 1) }),
                    _ => not_undone.push(NotUndone {
                        what: "uncommitted receipt".to_string(),
                        why: "the receipt file is no longer the file this transaction wrote; it was left in place".to_string(),
                    }),
                }
            }
        }
        let intents: Vec<String> = t.intents.iter().rev().cloned().collect();
        let mut pointer_unrestored = false;
        for intent in intents {
            match intent.as_str() {
                "pointer_switch" => match self.undo_pointer(d, t) {
                    Ok(()) => undone.push("active-version pointer restored".to_string()),
                    Err(why) => {
                        pointer_unrestored = true;
                        not_undone.push(NotUndone { what: "active-version pointer".to_string(), why });
                    }
                },
                "versions_moved" => match self.undo_moved_versions(d, t) {
                    Ok(()) => undone.push("version folders put back".to_string()),
                    Err(why) => not_undone.push(NotUndone { what: "removed version folders".to_string(), why }),
                },
                "pointer_removed" => match self.undo_removed_pointer(d, t) {
                    Ok(()) => undone.push("active-version pointer put back".to_string()),
                    Err(why) => not_undone.push(NotUndone { what: "removed active-version pointer".to_string(), why }),
                },
                "stage_dir" => match self
                    .rollback_hit(Point::RollbackRemoveStaging)
                    .and_then(|()| Self::remove_owned_folder(&d.staging, &t.txn_id, &format!("staging/{}", t.txn_id), t))
                {
                    Ok(()) => undone.push("staging folder removed".to_string()),
                    Err(why) => not_undone.push(NotUndone { what: "staging folder".to_string(), why }),
                },
                "version_tool_dir" => match self.undo_version_tool_dir(d, t) {
                    Ok(true) => undone.push("version root folder removed".to_string()),
                    Ok(false) => {}
                    Err(why) => not_undone.push(NotUndone { what: "version root folder".to_string(), why }),
                },
                other if other.starts_with("version_dir:") => {
                    let version = &other["version_dir:".len()..];
                    if pointer_unrestored && t.to_version.as_deref() == Some(version) {
                        not_undone.push(NotUndone {
                            what: format!("version folder {version}"),
                            why: "the active-version pointer could not be restored and may still name this version; the folder was left in place".to_string(),
                        });
                        continue;
                    }
                    match self.undo_version_dir(d, t, version) {
                        Ok(true) => undone.push(format!("version folder {version} removed")),
                        Ok(false) => {}
                        Err(why) => not_undone.push(NotUndone { what: format!("version folder {version}"), why }),
                    }
                }
                // the snapshot is what a retry of an earlier, failed undo would restore from: it goes only when nothing else failed
                "snapshot_dir" if !not_undone.is_empty() => {}
                "snapshot_dir" => match Self::remove_snapshot(d, t) {
                    Ok(()) => undone.push("snapshot folder removed".to_string()),
                    Err(why) => not_undone.push(NotUndone { what: "snapshot folder".to_string(), why }),
                },
                _ => not_undone
                    .push(NotUndone { what: "unknown journal entry".to_string(), why: "not understood; left untouched".to_string() }),
            }
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
        let mut not_covered = Vec::new();
        if !self.kept(&t.tool_id).is_empty() {
            not_covered.push("tool user data (data folder) is never part of a rollback".to_string());
        }
        let report = RollbackReport { status: status.to_string(), undone, not_undone, not_covered };
        t.state = if report.not_undone.is_empty() { TxnState::RolledBack } else { TxnState::RollbackFailed };
        t.rollback = Some(report.clone());
        let _ = self.save(d, t);
        report
    }

    fn undo_pointer(&self, d: &Dirs, t: &Txn) -> Result<(), String> {
        self.rollback_hit(Point::RollbackPointerRestore)?;
        let pname = pointer_name(&t.tool_id);
        let newname = format!("{pname}.new");
        let code = |e: FsError| format!("({})", e.code());
        // a new pointer that was written but never switched in
        if d.state.stat(&newname).map_err(code)?.is_some() {
            match Self::classify(&d.state, &newname, Self::entry(t, "new_pointer")) {
                Class::Ours(e) => d.state.remove_file_verified(&newname, &e).map_err(|e| format!("cannot remove the unused new pointer {}", code(e)))?,
                Class::Ambiguous => return Err("a pointer file exists whose creation was never recorded (crash); it cannot be proven to be this transaction's and was left in place".to_string()),
                _ => return Err("the new pointer file is no longer the file this transaction wrote; it was left in place".to_string()),
            }
        }
        let current = d.state.stat(&pname).map_err(code)?;
        match Self::entry(t, "snapshot_pointer") {
            Some(saved) => {
                let previous = Self::entry(t, "previous_pointer");
                if let (Some(s), Some(p)) = (&current, previous) {
                    if p.identity.map(unpair) == Some(s.identity) {
                        return Ok(()); // the original pointer was never replaced
                    }
                }
                if let (Some(s), Some(sv)) = (&current, saved.identity) {
                    if unpair(sv) == s.identity {
                        return Ok(()); // the saved copy is already the active pointer (an earlier rollback attempt got this far)
                    }
                }
                // replace the active pointer only if it is the one this transaction wrote
                if current.is_some() && !matches!(Self::classify(&d.state, &pname, Self::entry(t, "new_pointer")), Class::Ours(_)) {
                    return Err("the active pointer is neither the original, the saved copy nor the one this transaction wrote; it was left untouched".to_string());
                }
                let snap = d.snapshots.child(&t.txn_id).map_err(|e| format!("the saved pointer cannot be reached {}", code(e)))?;
                if !matches!(Self::classify(&snap, "pointer.json", Some(saved)), Class::Ours(_)) {
                    return Err("the saved pointer is not the file this transaction wrote; it was not restored".to_string());
                }
                snap.rename_file("pointer.json", &d.state, &pname, true, saved.identity.map(unpair))
                    .map_err(|e| format!("cannot restore the pointer {}", code(e)))
            }
            None => match current {
                None => Ok(()),
                Some(_) => match Self::classify(&d.state, &pname, Self::entry(t, "new_pointer")) {
                    Class::Ours(e) => {
                        d.state.remove_file_verified(&pname, &e).map_err(|e| format!("cannot remove the pointer {}", code(e)))
                    }
                    _ => Err("the active pointer is no longer the file this transaction wrote; it was left untouched".to_string()),
                },
            },
        }
    }

    fn undo_removed_pointer(&self, d: &Dirs, t: &Txn) -> Result<(), String> {
        self.rollback_hit(Point::RollbackRestoreRemoved)?;
        let pname = pointer_name(&t.tool_id);
        let entry = Self::entry(t, "removed_pointer");
        let id = entry.and_then(|e| e.identity).map(unpair);
        if let Ok(Some(s)) = d.state.stat(&pname) {
            return if Some(s.identity) == id {
                Ok(())
            } else {
                Err("a different pointer file is in place; the removed one was not put back".to_string())
            };
        }
        let snap = match d.snapshots.child(&t.txn_id) {
            Ok(s) => s,
            Err(FsError::NotFound) => return Ok(()), // the removal never started
            Err(e) => return Err(format!("the snapshot cannot be reached ({})", e.code())),
        };
        if snap.stat("pointer.json").map_err(|e| e.code().to_string())?.is_none() {
            return Ok(());
        }
        if !matches!(Self::classify(&snap, "pointer.json", entry), Class::Ours(_)) {
            return Err("the saved pointer is not the file this transaction moved; it was not put back".to_string());
        }
        snap.rename_file("pointer.json", &d.state, &pname, false, id).map_err(|e| format!("cannot put the pointer back ({})", e.code()))
    }

    fn undo_moved_versions(&self, d: &Dirs, t: &Txn) -> Result<(), String> {
        self.rollback_hit(Point::RollbackRestoreRemoved)?;
        let tool = &t.tool_id;
        let snap = match d.snapshots.child(&t.txn_id) {
            Ok(s) => s,
            Err(FsError::NotFound) => return Ok(()),
            Err(e) => return Err(format!("the snapshot cannot be reached ({})", e.code())),
        };
        let svs = match snap.child("versions") {
            Ok(s) => Some(s),
            Err(FsError::NotFound) => None,
            Err(e) => return Err(format!("the snapshot cannot be reached ({})", e.code())),
        };
        for e in t.owned.iter().filter(|e| e.role.starts_with("removed_version:")) {
            let v = &e.role["removed_version:".len()..];
            let id = e.identity.map(unpair);
            let saved = svs.as_ref().and_then(|s| s.child(v).ok());
            let Some(sv) = saved.filter(|sv| matches!(sv.stat("artifact.bin"), Ok(Some(_)))) else { continue };
            if !matches!(Self::classify(&sv, "artifact.bin", Some(e)), Class::Ours(_)) {
                return Err("a saved version artifact is not the file this transaction moved; it was not put back".to_string());
            }
            let vtool = match d.versions.child(tool) {
                Ok(x) => x,
                Err(FsError::NotFound) => d.versions.create_child(tool).map_err(|e| e.code().to_string())?,
                Err(e) => return Err(e.code().to_string()),
            };
            let vd = match vtool.child(v) {
                Ok(x) => x,
                Err(FsError::NotFound) => vtool.create_child(v).map_err(|e| e.code().to_string())?,
                Err(e) => return Err(e.code().to_string()),
            };
            sv.rename_file("artifact.bin", &vd, "artifact.bin", false, id)
                .map_err(|e| format!("cannot put a version back ({})", e.code()))?;
        }
        Ok(())
    }

    /// Removes the version folder this transaction created -- and ONLY if its whole content is nothing, or exactly the single file the
    /// manifest records. Returns whether a folder was removed.
    fn undo_version_dir(&self, d: &Dirs, t: &Txn, version: &str) -> Result<bool, String> {
        self.rollback_hit(Point::RollbackRemoveVersionDir)?;
        let tool = &t.tool_id;
        let vtool = match d.versions.child(tool) {
            Ok(x) => x,
            Err(FsError::NotFound) => return Ok(false),
            Err(e) => return Err(format!("the version path is a link, redirected or unsafe ({}); it was left untouched", e.code())),
        };
        let vd = match vtool.child(version) {
            Ok(x) => x,
            Err(FsError::NotFound) => return Ok(false),
            Err(e) => return Err(format!("the version path is a link, redirected or unsafe ({}); it was left untouched", e.code())),
        };
        let entries = vd.list().map_err(|e| e.code().to_string())?;
        match entries.as_slice() {
            [] => {}
            [(name, Kind::File)] if name == "artifact.bin" => {
                match Self::classify(&vd, "artifact.bin", Self::by_rel(t, &format!("versions/{tool}/{version}/artifact.bin"))) {
                    Class::Ours(e) => {
                        vd.remove_file_verified("artifact.bin", &e).map_err(|e| format!("cannot remove the artifact ({})", e.code()))?
                    }
                    Class::Ambiguous => return Err(Self::refusal(0, 0, 1)),
                    Class::Changed | Class::Unrecorded => return Err(Self::refusal(0, 1, 0)),
                }
            }
            other => {
                let unexpected = other.iter().filter(|(n, k)| !(n == "artifact.bin" && *k == Kind::File)).count();
                return Err(format!(
                    "the version folder holds more than this transaction wrote: {}",
                    Self::refusal(unexpected.max(1), 0, 0)
                ));
            }
        }
        drop(vd);
        vtool.remove_dir(version).map_err(|e| format!("cannot remove the version folder ({})", e.code()))?;
        Ok(true)
    }

    /// The tool's version root, created by this transaction: it goes only while it is empty (an older version keeps it).
    fn undo_version_tool_dir(&self, d: &Dirs, t: &Txn) -> Result<bool, String> {
        let vtool = match d.versions.child(&t.tool_id) {
            Ok(x) => x,
            Err(FsError::NotFound) => return Ok(false),
            Err(e) => return Err(format!("the version root is a link, redirected or unsafe ({}); it was left untouched", e.code())),
        };
        if !vtool.list().map_err(|e| e.code().to_string())?.is_empty() {
            return Ok(false);
        }
        drop(vtool);
        d.versions.remove_dir(&t.tool_id).map_err(|e| format!("cannot remove the version root ({})", e.code()))?;
        Ok(true)
    }

    // ---- recovery and queries ---------------------------------------------------------------------------------------------------

    fn complete_except_bookkeeping(&self, d: &Dirs, t: &Txn) -> bool {
        if t.state != TxnState::Activated {
            return false;
        }
        if Self::read_receipt_in(d, &t.txn_id).is_none() {
            return false;
        }
        match (t.action, Self::read_pointer(d, &t.tool_id)) {
            (Action::Uninstall, Ok(None)) => true,
            (Action::Install | Action::Update, Ok(Some(p))) => p.txn == t.txn_id,
            _ => false,
        }
    }

    /// To be run at start-up, before any new transaction: finishes or undoes whatever an interrupted run left behind. Anything it
    /// cannot prove to be its own is kept and reported on every start.
    pub fn recover(&self) -> R<Vec<Recovered>> {
        self.gate()?;
        let d = self.dirs()?;
        let mut out = Vec::new();
        let mut known_locks: Vec<String> = Vec::new();
        let mut temp_files = 0usize;
        for (name, kind) in d.journal.list().map_err(fs_err)? {
            if kind != Kind::File {
                continue;
            }
            if name.ends_with(".tmp") {
                temp_files += 1;
                continue;
            }
            if !name.ends_with(".json") {
                continue;
            }
            let Some(mut t) = Self::read_txn(&d, &name) else {
                out.push(Recovered::CorruptJournal(name.trim_end_matches(".json").to_string()));
                continue;
            };
            known_locks.push(lock_name(&t.tool_id));
            let leftovers = |me: &Self, t: &Txn, out: &mut Vec<Recovered>| {
                if let Some(why) = me.cleanup_after_commit(&d, t) {
                    out.push(Recovered::LeftoversRetained(t.txn_id.clone(), why));
                }
            };
            match t.state {
                TxnState::Committed | TxnState::RolledBack => {
                    leftovers(self, &t, &mut out);
                    if let Err(why) = Self::release_finished_lock(&d, &t) {
                        out.push(Recovered::LeftoversRetained(t.txn_id.clone(), format!("lock: {why}")));
                    }
                }
                TxnState::RollbackFailed => {
                    let report = self.roll_back(&d, &mut t);
                    if report.not_undone.is_empty() {
                        out.push(Recovered::RolledBack(t.txn_id.clone()));
                        leftovers(self, &t, &mut out);
                        let _ = Self::release_lock(&d, &t);
                    } else {
                        out.push(Recovered::NeedsAttention(t.txn_id.clone(), report));
                    }
                }
                _ => {
                    if self.complete_except_bookkeeping(&d, &t) {
                        t.state = TxnState::Committed;
                        let _ = self.save(&d, &mut t);
                        out.push(Recovered::RolledForward(t.txn_id.clone()));
                        leftovers(self, &t, &mut out);
                        let _ = Self::release_lock(&d, &t);
                    } else {
                        let report = self.roll_back(&d, &mut t);
                        if report.not_undone.is_empty() {
                            out.push(Recovered::RolledBack(t.txn_id.clone()));
                            leftovers(self, &t, &mut out);
                            let _ = Self::release_lock(&d, &t);
                        } else {
                            out.push(Recovered::NeedsAttention(t.txn_id.clone(), report));
                        }
                    }
                }
            }
        }
        // anything in the lock folder that no journal accounts for is kept and reported (never deleted on a guess)
        let strays = d.locks.list().map_err(fs_err)?.into_iter().filter(|(n, _)| !known_locks.contains(n)).count();
        if strays > 0 {
            out.push(Recovered::LeftoversRetained(
                "locks".to_string(),
                format!("{strays} lock entr{} that no journal accounts for; kept", if strays == 1 { "y" } else { "ies" }),
            ));
        }
        if temp_files > 0 {
            out.push(Recovered::LeftoversRetained(
                "journal".to_string(),
                format!("{temp_files} unrecognised temporary file(s) from an interrupted journal write; kept (not provably this module's)"),
            ));
        }
        Ok(out)
    }

    /// Which version is active for `tool` (None = not installed).
    pub fn active_version(&self, tool: &str) -> R<Option<String>> {
        self.gate()?;
        let d = self.dirs()?;
        Ok(Self::read_pointer(&d, tool)?.map(|p| p.version))
    }

    pub fn read_receipt(&self, txn: &str) -> Option<Receipt> {
        let d = self.dirs().ok()?;
        Self::read_receipt_in(&d, txn)
    }
}

#[cfg(test)]
#[path = "adapter_tests.rs"]
mod tests;
