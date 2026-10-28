//! CONFIG-02B (Issue #39): the FILE-SYSTEM half of the one-file MCP configuration transaction PROTOTYPE.
//!
//! Compiled for TESTS ONLY (`#[cfg(test)] mod mcp_file;` inside `managed_fs.rs`), and a child of `managed_fs` on purpose: it re-uses that
//! module's handle-relative, no-follow primitives (`imp::DirImp`: `NtCreateFile`/`openat`, exclusive create, `FILE_RENAME_INFORMATION_EX`
//! / `renameatx_np`) instead of inventing a second file-system layer, and it needs one thing `Dir` deliberately refuses -- the exact name
//! `.mcp.json` (`valid_name` rejects a leading dot). That name is a COMPILE-TIME CONSTANT here; no function in this module takes a file
//! name, a relative path or a path inside the root. The only path ever resolved by text is the root itself, and only by tests.
//!
//! Nothing here is reachable from an IPC command, a shipping feature, a GUI control or a user-selected path; the tests create every
//! directory themselves under the test runner's temporary area. `REAL_CONFIG_WRITE_NO_GO` and `REAL_INSTALLATION_NO_GO` stand.
//!
//! What the primitives give (and `GUARANTEES` below states per platform, asserted by tests):
//! * the root is opened by walking its chain with no link followed; every later step is relative to that handle;
//! * the target is opened without following links, must be a plain file with exactly one hard link and no case-variant sibling;
//! * the staged file is created exclusively and is verified through its own handle; creation of an absent target is an atomic no-clobber
//!   rename (the OS refuses if ANYTHING exists), replacement of a present target requires the exact identity that was inspected;
//! * the original is copied to a SEPARATE private folder before the swap, and only the file this transaction wrote can be rolled back,
//!   and only while it is still byte-for-byte what was written.
//!
//! What they do NOT give: there is no exclusive lock held on the target, so an editor's in-place write inside the microseconds between the
//! last digest check and the swap cannot be prevented (Windows: also not detected; macOS: detected afterwards because the displaced
//! original is kept and compared). There is no persisted recovery record: a crash leaves, at worst, a stray staging file (reported by
//! `inspect`, never deleted because its ownership cannot be proven) and possibly a backup copy. File mode / ACL / extended attributes of
//! the old file are not carried over. Those are the design conditions a real phase must still close; see `docs/config-02b/`.
#![allow(dead_code)]

use super::{hook, imp, verify_expect, Dir, Expect, FileH, FsError, Identity, Kind, MAX_LIST_ENTRIES};
use crate::config_txn::{
    authorize, prepare, sha256_hex, Approval, Failure, Inspection, Ledger, Plan, Platform, Presence, Refusal, Request, State,
    MAX_FILE_BYTES, TARGET_FILE,
};
use std::path::Path;

pub const STAGE_PREFIX: &str = "mcpjson-stage-";

/// What each platform's primitives actually guarantee for this operation. The tests assert these values against behaviour; the documents
/// quote them. `false` is an honest "no".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guarantees {
    /// Creating an absent target is one atomic no-clobber rename decided by the operating system.
    pub atomic_no_clobber_create: bool,
    /// Replacing a present target verifies the destination's identity through an open handle immediately before the rename.
    pub replace_checks_identity_just_before: bool,
    /// The displaced original stays on disk after the swap, so a concurrent in-place edit can be detected and preserved.
    pub displaced_original_retained: bool,
    /// An in-place edit made inside the replace window is detected.
    pub detects_edit_in_replace_window: bool,
    /// An exclusive lock is held on the target between the last check and the swap.
    pub holds_exclusive_lock: bool,
    /// The old file's mode / ACL / extended attributes are carried over to the new file.
    pub preserves_permissions: bool,
    /// A recovery record survives a crash.
    pub persistent_recovery_record: bool,
}

pub const GUARANTEES: Guarantees = Guarantees {
    atomic_no_clobber_create: true,
    replace_checks_identity_just_before: true,
    displaced_original_retained: cfg!(target_os = "macos"),
    detects_edit_in_replace_window: cfg!(target_os = "macos"),
    holds_exclusive_lock: false,
    preserves_permissions: false,
    persistent_recovery_record: false,
};

fn fs_refusal(e: FsError) -> Refusal {
    match e {
        FsError::Unsupported => Refusal::WriteAdapterBlocked,
        FsError::Link => Refusal::TargetIsLink,
        FsError::NotFile => Refusal::TargetNotRegularFile,
        FsError::Exists => Refusal::TargetAppeared,
        FsError::TooLarge => Refusal::FileTooLarge,
        FsError::Changed => Refusal::TargetChanged,
        other => Refusal::Fs(other.code()),
    }
}

thread_local! {
    static FAIL_AT: std::cell::RefCell<Option<&'static str>> = const { std::cell::RefCell::new(None) };
}

/// Test seam: make the named step fail as if the operating system had refused it.
pub(crate) fn fail_at(step: Option<&'static str>) {
    FAIL_AT.with(|f| *f.borrow_mut() = step);
}

fn step(name: &'static str) -> Result<(), Refusal> {
    hook::fire(name);
    if FAIL_AT.with(|f| *f.borrow() == Some(name)) {
        return Err(Refusal::Injected);
    }
    Ok(())
}

/// What a successful commit did. Digests and identities only: never a path, never file content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub plan_digest: String,
    pub new_identity: Identity,
    pub backup_name: Option<String>,
    /// macOS keeps the displaced original under the staging name; it is removed only after a verified comparison with what was inspected.
    /// `true` here means it could not be removed and is still there (reported by `inspect` as a stray staging file).
    pub displaced_kept: bool,
    pub dir_synced: bool,
    pub trace: Vec<&'static str>,
}

/// One project root held open by handle. Tests are the only callers; the path argument of `open` is a test directory.
pub struct ProjectRoot {
    dir: Dir,
}

impl ProjectRoot {
    pub fn open(path: &Path) -> Result<ProjectRoot, Refusal> {
        Ok(ProjectRoot { dir: Dir::open_anchor(path).map_err(fs_refusal)? })
    }

    pub fn identity(&self) -> Identity {
        self.dir.identity()
    }

    /// Reads the target through the root handle: no link followed, one hard link, no case-variant sibling, bounded size.
    pub fn inspect(&self) -> Result<Inspection, Refusal> {
        let names = self.dir.imp.list(MAX_LIST_ENTRIES).map_err(fs_refusal)?;
        if names.iter().any(|n| n != TARGET_FILE && n.eq_ignore_ascii_case(TARGET_FILE)) {
            return Err(Refusal::CaseAliasPresent); // the name must be exactly the constant: an alias could be another file
        }
        let stray = names.iter().filter(|n| n.starts_with(STAGE_PREFIX)).count();
        let Some(stat) = self.dir.imp.stat_name(TARGET_FILE).map_err(fs_refusal)? else {
            return Ok(Inspection::new(Presence::Absent, None, stray));
        };
        match stat.kind {
            Kind::Link => return Err(Refusal::TargetIsLink),
            Kind::File => {}
            _ => return Err(Refusal::TargetNotRegularFile),
        }
        if stat.links != 1 {
            return Err(Refusal::TargetHardLinked);
        }
        let mut f = FileH { file: self.dir.imp.open_file(TARGET_FILE).map_err(fs_refusal)? };
        let opened = f.stat().map_err(fs_refusal)?;
        if opened.kind != Kind::File || opened.identity != stat.identity || opened.links != 1 {
            return Err(Refusal::TargetChanged);
        }
        let bytes = f.read_bounded(MAX_FILE_BYTES).map_err(fs_refusal)?;
        let presence = Presence::Present { identity: opened.identity, size: bytes.len() as u64, sha256: sha256_hex(&bytes) };
        Ok(Inspection::new(presence, Some(bytes), stray))
    }

    /// Inspect + build the plan. Nothing is written.
    pub fn prepare(&self, req: &Request, host: Option<Platform>) -> Result<Plan, Refusal> {
        prepare(req, host, &self.inspect()?)
    }

    fn matches_expected(&self, plan: &Plan) -> Result<(), Refusal> {
        let now = self.inspect()?;
        match (plan.expected(), &now.presence) {
            (Presence::Absent, Presence::Absent) => Ok(()),
            (Presence::Absent, Presence::Present { .. }) => Err(Refusal::TargetAppeared),
            (Presence::Present { .. }, Presence::Absent) => Err(Refusal::TargetVanished),
            (a, b) if a == b => Ok(()),
            _ => Err(Refusal::TargetChanged),
        }
    }

    /// The write. Every precondition is checked before the first side effect; the approval is consumed when the checks pass, so a commit
    /// that fails later needs a NEW plan and a NEW approval. `backup` is a separate private folder (the application-data stand-in); it is
    /// never inside the project.
    pub fn commit(
        &self,
        plan: &Plan,
        project_alias: &str,
        approval: Option<&Approval>,
        ledger: &mut Ledger,
        backup: &Dir,
        now: u64,
    ) -> Result<Receipt, Failure> {
        let mut trace: Vec<&'static str> = vec!["prepared"];
        let fail = |refusal: Refusal, reached: State| Failure { refusal, reached };
        // ---- checks without side effects -------------------------------------------------------------------------------------------
        if project_alias != plan.alias() {
            return Err(fail(Refusal::WrongProject, State::Prepared));
        }
        if Platform::host() != Some(plan.platform()) {
            return Err(fail(Refusal::PlanPlatformMismatch, State::Prepared));
        }
        if self.identity() != plan.root() {
            return Err(fail(Refusal::RootIdentityChanged, State::Prepared));
        }
        if backup.identity() == self.identity() {
            return Err(fail(Refusal::BackupIsRoot, State::Prepared));
        }
        if matches!(plan.expected(), Presence::Present { .. }) && !backup.is_owner_only() {
            return Err(fail(Refusal::BackupNotPrivate, State::Prepared));
        }
        authorize(plan, approval, ledger, now).map_err(|r| fail(r, State::Prepared))?;
        trace.push("authorized");
        self.matches_expected(plan).map_err(|r| fail(r, State::Authorized))?;
        trace.push("target-matches-expected");

        // ---- side effects ------------------------------------------------------------------------------------------------------------
        let mut backup_name = None;
        let mut reached = State::Authorized;
        if let Presence::Present { sha256, .. } = plan.expected() {
            let original = match self.inspect() {
                Ok(i) => i,
                Err(r) => return Err(fail(r, reached)),
            };
            let bytes = original.original_bytes().ok_or_else(|| fail(Refusal::TargetVanished, reached))?;
            if sha256_hex(bytes) != *sha256 {
                return Err(fail(Refusal::TargetChanged, reached));
            }
            let name = format!("backup-{}.json", &plan.digest()[..16]);
            step("txn:before-backup").map_err(|r| fail(r, reached))?;
            let mut f = backup
                .create_file(&name)
                .map_err(|e| fail(if e == FsError::Exists { Refusal::BackupExists } else { fs_refusal(e) }, reached))?;
            let written = f.write_all(bytes).and_then(|_| f.sync_all()).and_then(|_| f.sha256_bounded(MAX_FILE_BYTES));
            match written {
                Ok((sum, _)) if sum == *sha256 => {}
                Ok(_) => return Err(fail(Refusal::BackupMismatch, reached)), // a partial backup file stays: it is not ours to guess about
                Err(e) => return Err(fail(fs_refusal(e), reached)),
            }
            backup_name = Some(name);
            reached = State::BackedUp;
            trace.push("backed-up");
        }

        let stage_name = format!("{STAGE_PREFIX}{}.tmp", &uuid::Uuid::new_v4().simple().to_string()[..16]);
        let mut stage = self.dir.create_file(&stage_name).map_err(|e| fail(fs_refusal(e), reached))?;
        let stage_stat = stage.stat().map_err(|e| fail(fs_refusal(e), reached));
        let stage_identity = match stage_stat {
            Ok(s) => s.identity,
            Err(f) => {
                drop(stage);
                self.discard_stage(&stage_name, None); // identity unknown: nothing is deleted, `inspect` will report it
                return Err(f);
            }
        };
        // before the swap an abort removes the staging file this call made (if it is still exactly that) and reports how far it got
        let abort = |this: &ProjectRoot, r: Refusal, reached: State| {
            this.discard_stage(&stage_name, Some(stage_identity));
            fail(r, reached)
        };
        if let Err(r) = step("txn:after-stage-created") {
            drop(stage); // an open handle would keep the file from being removed (Windows)
            return Err(abort(self, r, reached));
        }
        let written = stage.write_all(plan.new_bytes()).and_then(|_| stage.sync_all()).and_then(|_| stage.sha256_bounded(MAX_FILE_BYTES));
        drop(stage);
        match written {
            Ok((sum, len)) if sum == plan.new_sha256() && len == plan.new_bytes().len() as u64 => {}
            Ok(_) => return Err(abort(self, Refusal::StageMismatch, reached)),
            Err(e) => return Err(abort(self, fs_refusal(e), reached)),
        }
        reached = State::Staged;
        trace.push("staged");
        if let Err(r) = step("txn:after-stage-written") {
            return Err(abort(self, r, reached));
        }

        // the last look, immediately before the swap
        if let Err(r) = step("txn:before-final-recheck").and_then(|_| self.matches_expected(plan)) {
            return Err(abort(self, r, reached));
        }
        trace.push("final-recheck");
        if let Err(r) = step("txn:before-swap") {
            return Err(abort(self, r, reached));
        }
        if hook::unsupported() {
            return Err(abort(self, Refusal::WriteAdapterBlocked, reached)); // no fallback to an ordinary rename, ever
        }
        let mut displaced_kept = false;
        match plan.expected() {
            Presence::Absent => {
                if let Err(e) =
                    self.dir.imp.rename_file(&stage_name, &self.dir.imp, TARGET_FILE, imp::Mode::NoReplace, Some(stage_identity))
                {
                    return Err(abort(self, fs_refusal(e), reached));
                }
            }
            Presence::Present { identity, sha256, .. } => {
                match self.dir.imp.rename_file(
                    &stage_name,
                    &self.dir.imp,
                    TARGET_FILE,
                    imp::Mode::Replace { dest: *identity },
                    Some(stage_identity),
                ) {
                    Err(e) => return Err(abort(self, fs_refusal(e), reached)),
                    Ok(false) => {}
                    Ok(true) => {
                        // macOS: the displaced original sits under the staging name. Compare it with what was inspected; delete it only
                        // if it is exactly that, otherwise keep it (it holds somebody's concurrent edit) and say so.
                        displaced_kept = true;
                        let same = self.dir.read_file(&stage_name, MAX_FILE_BYTES).map(|b| sha256_hex(&b) == *sha256).unwrap_or(false);
                        if same
                            && self
                                .dir
                                .remove_file_verified(
                                    &stage_name,
                                    &Expect { identity: Some(*identity), size: None, sha256: Some(sha256.clone()) },
                                )
                                .is_ok()
                        {
                            displaced_kept = false;
                        } else if !same {
                            // somebody wrote the file inside the replace window: their version is the displaced file. Nothing is
                            // deleted and nothing is swapped back automatically; the new file is in place, the old edit is kept.
                            return Err(fail(Refusal::TargetChanged, State::NeedsAttention));
                        }
                    }
                }
            }
        }
        trace.push("swapped");
        if let Err(r) = step("txn:after-swap") {
            return Err(fail(r, State::NeedsAttention));
        }

        // ---- verify what is now at the target ------------------------------------------------------------------------------------------
        let verified = self.inspect().ok().and_then(|i| match i.presence {
            Presence::Present { identity, sha256, .. } if identity == stage_identity && sha256 == plan.new_sha256() => Some(identity),
            _ => None,
        });
        let Some(new_identity) = verified else { return Err(fail(Refusal::VerifyFailed, State::NeedsAttention)) };
        let dir_synced = self.dir.sync();
        trace.push("verified");
        Ok(Receipt { plan_digest: plan.digest().to_string(), new_identity, backup_name, displaced_kept, dir_synced, trace })
    }

    /// Removes a staging file this call created, only if it is still the very object that was created (its identity, read from the handle
    /// the exclusive creation returned, and exactly one link -- checked through the handle used for the deletion). Content is not
    /// compared: the file may be empty or half written when this runs. Without a known identity nothing is deleted.
    fn discard_stage(&self, name: &str, identity: Option<Identity>) {
        let Some(id) = identity else { return };
        // a failure here means the file is not provably ours any more: it stays, and `inspect` reports it as a stray staging file
        let _ = self.dir.remove_file_verified(name, &Expect { identity: Some(id), size: None, sha256: None });
    }

    /// Undoes a commit, and ONLY while the target is still exactly the file that commit wrote. A later edit by anybody is never overwritten.
    /// With a recorded backup the previous bytes are restored through a staged file and an identity-checked replace; without one (the
    /// target was created by this commit) the file this commit created is removed by a verified deletion.
    pub fn rollback(&self, plan: &Plan, receipt: &Receipt, backup: &Dir) -> Result<(), Refusal> {
        if receipt.plan_digest != plan.digest() {
            return Err(Refusal::ApprovalMismatch);
        }
        let now = self.inspect()?;
        let (identity, sha) = match &now.presence {
            Presence::Present { identity, sha256, .. } => (*identity, sha256.clone()),
            Presence::Absent => return Err(Refusal::RollbackNotNeeded),
        };
        if identity != receipt.new_identity || sha != plan.new_sha256() {
            return Err(Refusal::RollbackRefusedLaterEdit);
        }
        match (plan.expected(), &receipt.backup_name) {
            (Presence::Absent, _) => self
                .dir
                .imp
                .remove_file_verified(TARGET_FILE, &|file, stat| {
                    let mut dup = file.try_clone().map_err(|e| imp::map_io(&e))?;
                    verify_expect(&mut dup, stat, &Expect { identity: Some(identity), size: None, sha256: Some(sha.clone()) })
                })
                .map_err(fs_refusal),
            (Presence::Present { sha256: original, .. }, Some(name)) => {
                let bytes = backup.read_file(name, MAX_FILE_BYTES).map_err(fs_refusal)?;
                if sha256_hex(&bytes) != *original {
                    return Err(Refusal::BackupMismatch);
                }
                let stage_name = format!("{STAGE_PREFIX}{}.tmp", &uuid::Uuid::new_v4().simple().to_string()[..16]);
                let mut f = self.dir.create_file(&stage_name).map_err(fs_refusal)?;
                let sid = f.stat().map_err(fs_refusal)?.identity;
                let written = f.write_all(&bytes).and_then(|_| f.sync_all());
                drop(f);
                if let Err(e) = written {
                    self.discard_stage(&stage_name, Some(sid));
                    return Err(fs_refusal(e));
                }
                match self.dir.imp.rename_file(&stage_name, &self.dir.imp, TARGET_FILE, imp::Mode::Replace { dest: identity }, Some(sid)) {
                    Err(e) => {
                        self.discard_stage(&stage_name, Some(sid));
                        Err(fs_refusal(e))
                    }
                    Ok(remains) => {
                        if remains {
                            let _ = self.dir.remove_file_verified(
                                &stage_name,
                                &Expect { identity: Some(identity), size: None, sha256: Some(sha.clone()) },
                            );
                        }
                        Ok(())
                    }
                }
            }
            (Presence::Present { .. }, None) => Err(Refusal::BackupMismatch),
        }
    }

    /// Names of the entries in the root (test visibility only).
    pub fn entry_names(&self) -> Vec<String> {
        let mut v = self.dir.imp.list(MAX_LIST_ENTRIES).unwrap_or_default();
        v.sort();
        v
    }
}

#[cfg(test)]
#[path = "managed_fs_mcp_tests.rs"]
mod tests;
