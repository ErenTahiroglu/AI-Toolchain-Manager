//! Phase 3D-1 tests of the durable approval ledger, the append-only audit journal and the approval-security setting.
//! Temporary files only; no key, no prompt, no network.

use super::*;
use crate::device_trust::{ApprovalLedger, AuditRecord, AuditSink, LedgerStatus, TrustError};
use std::{path::PathBuf, sync::Arc};

fn file() -> (tempfile::TempDir, PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join(FILE_NAME);
    (d, p)
}

fn rec(event: &'static str, outcome: &'static str, id: &str) -> AuditRecord {
    AuditRecord {
        event,
        outcome,
        approval_id: Some(id.to_string()),
        tool_id: "synthetic-tool".into(),
        version: "1.0.0".into(),
        action: "install",
        code: None,
    }
}

fn issue(s: &ApprovalStore, id: &str) -> u64 {
    let c = s.next_counter().unwrap();
    s.record_issue(id, c).unwrap();
    c
}

fn raw(p: &PathBuf) -> Connection {
    Connection::open(p).unwrap()
}

#[test]
fn single_use_survives_a_restart_and_a_reopen_cannot_resurrect_an_approval() {
    let (_d, p) = file();
    {
        let s = ApprovalStore::open(&p).unwrap();
        issue(&s, "a-1");
        issue(&s, "a-2");
        issue(&s, "a-3");
        assert_eq!(s.status("a-1").unwrap(), Some(LedgerStatus::Issued));
        s.consume("a-1", "txn-1").unwrap();
        s.revoke("a-2").unwrap();
    } // the process "ends"
    let s = ApprovalStore::open(&p).unwrap();
    assert_eq!(s.status("a-1").unwrap(), Some(LedgerStatus::Consumed { txn_id: "txn-1".into() }));
    assert_eq!(s.status("a-2").unwrap(), Some(LedgerStatus::Revoked));
    assert_eq!(s.status("a-3").unwrap(), Some(LedgerStatus::Issued));
    assert_eq!(s.consume("a-1", "txn-2"), Err(TrustError::Replayed));
    assert_eq!(s.consume("a-2", "txn-2"), Err(TrustError::Revoked));
    assert_eq!(s.consume("nope", "txn-2"), Err(TrustError::UnknownApproval));
    // revoking something already used is a harmless no-op and never reopens it
    assert_eq!(s.revoke("a-1"), Ok(()));
    assert_eq!(s.status("a-1").unwrap(), Some(LedgerStatus::Consumed { txn_id: "txn-1".into() }));
    assert_eq!(s.revoke("nope"), Err(TrustError::UnknownApproval));
    // an approval id cannot be issued twice, and the counter keeps counting across reopens
    assert_eq!(s.record_issue("a-1", 99), Err(TrustError::Replayed));
    assert_eq!(s.next_counter().unwrap(), 4);
    drop(s);
    assert_eq!(ApprovalStore::open(&p).unwrap().next_counter().unwrap(), 5);
}

#[test]
fn of_many_simultaneous_attempts_on_separate_connections_exactly_one_consumes_the_approval() {
    let (_d, p) = file();
    issue(&ApprovalStore::open(&p).unwrap(), "race-1");
    let p = Arc::new(p);
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let p = p.clone();
            std::thread::spawn(move || ApprovalStore::open(&p).unwrap().consume("race-1", &format!("txn-{i}")))
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1, "{results:?}");
    assert!(results.iter().filter(|r| r.is_err()).all(|r| *r == Err(TrustError::Replayed)), "{results:?}");
    let winner = ApprovalStore::open(&p).unwrap().status("race-1").unwrap();
    assert!(matches!(winner, Some(LedgerStatus::Consumed { .. })));
}

#[test]
fn consumed_and_revoked_are_terminal_and_nothing_is_ever_deleted_in_the_database_itself() {
    let (_d, p) = file();
    let s = ApprovalStore::open(&p).unwrap();
    issue(&s, "t-1");
    s.consume("t-1", "txn").unwrap();
    issue(&s, "t-2");
    s.revoke("t-2").unwrap();
    s.record(rec("approval_use", "authorized", "t-1")).unwrap();
    let c = raw(&p);
    for id in ["t-1", "t-2"] {
        assert!(c.execute("UPDATE approvals SET status = 'issued' WHERE approval_id = ?1", [id]).is_err(), "{id} cannot go back to issued");
        assert!(c.execute("UPDATE approvals SET txn_id = 'other' WHERE approval_id = ?1", [id]).is_err());
    }
    assert!(c.execute("DELETE FROM approvals", []).is_err());
    assert!(c.execute("UPDATE audit SET outcome = 'x'", []).is_err());
    assert!(c.execute("DELETE FROM audit", []).is_err());
    assert_eq!(s.status("t-1").unwrap(), Some(LedgerStatus::Consumed { txn_id: "txn".into() }));
}

#[test]
fn the_audit_journal_is_a_hash_chain_and_any_tampering_is_detected() {
    let (_d, p) = file();
    let s = ApprovalStore::open(&p).unwrap();
    assert_eq!(s.verify_audit_chain(), Ok(0));
    for i in 0..5 {
        s.record(rec("approval_issue", "issued", &format!("c-{i}"))).unwrap();
    }
    assert_eq!(s.verify_audit_chain(), Ok(5));
    assert_eq!(s.audit_events().unwrap().len(), 5);
    // the triggers stop ordinary edits; a file owner who drops them is DETECTED by the chain, not prevented
    let c = raw(&p);
    c.execute_batch("DROP TRIGGER audit_no_update; DROP TRIGGER audit_no_delete;").unwrap();
    c.execute("UPDATE audit SET outcome = 'refused' WHERE seq = 2", []).unwrap();
    assert_eq!(s.verify_audit_chain(), Err(StoreError::AuditChainBroken), "an altered row");
    c.execute("UPDATE audit SET outcome = 'issued' WHERE seq = 2", []).unwrap();
    assert_eq!(s.verify_audit_chain(), Ok(5), "restoring the original row restores the chain");
    c.execute("DELETE FROM audit WHERE seq = 3", []).unwrap();
    assert_eq!(s.verify_audit_chain(), Err(StoreError::AuditChainBroken), "a removed row");
}

#[test]
fn concurrent_writers_never_fork_the_audit_chain() {
    let (_d, p) = file();
    ApprovalStore::open(&p).unwrap();
    let p = Arc::new(p);
    let handles: Vec<_> = (0..6)
        .map(|t| {
            let p = p.clone();
            std::thread::spawn(move || {
                let s = ApprovalStore::open(&p).unwrap();
                for i in 0..10 {
                    s.record(rec("approval_use", "authorized", &format!("w-{t}-{i}"))).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(ApprovalStore::open(&p).unwrap().verify_audit_chain(), Ok(60));
}

#[test]
fn authorization_is_denied_and_nothing_is_consumed_when_the_durable_audit_cannot_be_written() {
    use crate::{
        approval::Action,
        device_trust::{self, fakes::*, Expected, Presence, TrustPolicy},
        synthetic::testkit::*,
    };
    let (_d, p) = file();
    let s = ApprovalStore::open(&p).unwrap();
    let f = fx("synthetic-tool", "1.0.0", b"payload one");
    let keys = FakeKeyStore::new("k");
    let a = make_approval(&f, Action::Install, None, "synthetic-computer-1", t0());
    let effects = f.valid.permissions_digest();
    let artifact = f.valid.recipe().artifact.sha256.clone();
    let d = device_trust::issue(&a, &artifact, &effects, &keys, &FakePresence(Presence::Confirmed), &s, &s, TrustPolicy::test_doubles())
        .unwrap();
    // the audit table becomes unusable (disk full, corruption, permission, a deleted table: all look the same to the writer)
    raw(&p).execute_batch("ALTER TABLE audit RENAME TO audit_gone;").unwrap();
    let r = f.valid.recipe();
    let want = Expected {
        action: Action::Install,
        tool_id: &r.tool_id,
        version: &r.version,
        platform_target: &r.target.platform_target,
        computer_id: "synthetic-computer-1",
        recipe_digest: f.valid.digest(),
        artifact_sha256: &artifact,
        effects_digest: &effects,
        scope: &r.target.scope,
        project_alias: None,
    };
    let denied =
        device_trust::authorize(&d, &want, "txn-1", t0() + chrono::Duration::seconds(5), &keys, &s, &s, TrustPolicy::test_doubles());
    assert_eq!(denied.unwrap_err(), TrustError::AuditUnavailable);
    assert_eq!(s.status(&d.approval.approval_id).unwrap(), Some(LedgerStatus::Issued), "the approval was NOT consumed by a denied attempt");
    // once the audit works again the same approval can still be used exactly once
    raw(&p).execute_batch("ALTER TABLE audit_gone RENAME TO audit;").unwrap();
    assert!(device_trust::authorize(&d, &want, "txn-1", t0() + chrono::Duration::seconds(6), &keys, &s, &s, TrustPolicy::test_doubles())
        .is_ok());
    assert_eq!(
        device_trust::authorize(&d, &want, "txn-2", t0() + chrono::Duration::seconds(7), &keys, &s, &s, TrustPolicy::test_doubles())
            .unwrap_err(),
        TrustError::Replayed
    );
}

#[test]
fn a_ledger_that_cannot_be_written_denies_instead_of_pretending() {
    let (_d, p) = file();
    let s = ApprovalStore::open(&p).unwrap();
    raw(&p)
        .execute_batch("DROP TRIGGER approvals_terminal; DROP TRIGGER approvals_no_delete; ALTER TABLE approvals RENAME TO approvals_gone;")
        .unwrap();
    assert_eq!(s.record_issue("x", 1), Err(TrustError::LedgerUnavailable));
    assert_eq!(s.status("x"), Err(TrustError::LedgerUnavailable));
    assert_eq!(s.consume("x", "t"), Err(TrustError::LedgerUnavailable));
}

#[test]
fn approval_security_starts_disabled_and_enabling_it_is_an_explicit_audited_act_for_this_computer() {
    let (_d, p) = file();
    let s = ApprovalStore::open(&p).unwrap();
    let st = s.security().unwrap();
    assert!(!st.enabled && st.key.is_none() && st.computer_id.is_none());
    assert_eq!(s.enrollment_permit("pc-1").unwrap_err(), StoreError::NotEnabled, "no permit without enabling");
    // the wrong phrase (or a generic confirmation) enables nothing
    for typed in ["", "yes", "ENABLE-OS-APPROVAL", "enable-os-approval pc-1", "ENABLE-OS-APPROVAL pc-2", "ENABLE-OS-APPROVAL pc-1 "] {
        assert_eq!(s.enable_approval_security("pc-1", typed), Err(StoreError::PhraseMismatch), "{typed:?}");
    }
    assert!(!s.security().unwrap().enabled);
    s.enable_approval_security("pc-1", &enable_phrase("pc-1")).unwrap();
    s.enable_approval_security("pc-1", &enable_phrase("pc-1")).unwrap(); // idempotent
    let st = s.security().unwrap();
    assert!(st.enabled && st.computer_id.as_deref() == Some("pc-1") && st.enabled_at.is_some());
    assert_eq!(s.enable_approval_security("pc-2", &enable_phrase("pc-2")), Err(StoreError::OtherComputer));
    assert_eq!(s.enrollment_permit("pc-2").unwrap_err(), StoreError::OtherComputer);
    assert!(s.enrollment_permit("pc-1").is_ok());
    let events = s.audit_events().unwrap();
    assert_eq!(events.iter().filter(|e| e.0 == "approval_security_enable" && e.1 == "refused").count(), 6, "every refusal is audited");
    assert_eq!(events.iter().filter(|e| e.0 == "approval_security_enable" && e.1 == "enabled").count(), 1);
    assert_eq!(s.verify_audit_chain(), Ok(events.len()));
}

#[test]
fn a_key_is_recorded_once_and_only_its_public_part_is_kept() {
    let (_d, p) = file();
    let s = ApprovalStore::open(&p).unwrap();
    s.enable_approval_security("pc-1", &enable_phrase("pc-1")).unwrap();
    let permit = s.enrollment_permit("pc-1").unwrap();
    let key = KeyRecord {
        key_name: "atm-approval-pc-1".into(),
        key_id: "ab".repeat(32),
        public_key_hex: "cd".repeat(8),
        assurance: "os_reported_hardware".into(),
        attestation_digest: Some("ef".repeat(32)),
    };
    s.record_key(&permit, &key).unwrap();
    assert_eq!(s.record_key(&permit, &key), Err(StoreError::KeyAlreadyRecorded));
    assert_eq!(s.enrollment_permit("pc-1").unwrap_err(), StoreError::KeyAlreadyRecorded);
    drop(s);
    let s = ApprovalStore::open(&p).unwrap();
    let st = s.security().unwrap();
    assert_eq!(st.key, Some(key));
    // the file has no column that could hold a private key or a credential
    let cols: Vec<String> = raw(&p)
        .prepare("PRAGMA table_info(approval_security)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(|c| c.unwrap())
        .collect();
    assert!(
        cols.iter().all(|c| !c.contains("private") && !c.contains("secret") && !c.contains("pin") && !c.contains("password")),
        "{cols:?}"
    );
}

#[test]
fn an_unknown_schema_version_is_refused_and_not_modified() {
    let (_d, p) = file();
    ApprovalStore::open(&p).unwrap();
    raw(&p).execute_batch("PRAGMA user_version = 99;").unwrap();
    assert!(matches!(ApprovalStore::open(&p), Err(StoreError::UnsupportedSchema)));
    let v: i64 = raw(&p).query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 99);
}

#[test]
fn the_store_is_reachable_from_no_command_and_the_application_never_creates_one() {
    for (name, text) in [
        ("commands.rs", include_str!("commands.rs")),
        ("state.rs", include_str!("state.rs")),
        ("settings.rs", include_str!("settings.rs")),
        ("lib.rs", include_str!("lib.rs")),
        ("main.rs", include_str!("main.rs")),
        ("adapter.rs", include_str!("adapter.rs")),
    ] {
        for needle in [
            "ApprovalStore",
            "approval_store::",
            "EnrollmentPermit",
            "enable_approval_security",
            "WindowsHelloBackend",
            "os_approval::provision",
        ] {
            assert!(!text.contains(needle), "{name} must not reference {needle}");
        }
    }
}

// =====================================================================================================================================
// RT-3D1-02: issuance + audit and consumption + audit are each ONE durable transaction
// =====================================================================================================================================

use crate::{
    approval::{Action, Approval},
    device_trust::{
        self,
        fakes::{FakeKeyStore, FakePresence, VecAudit},
        DeviceApproval, Expected, Presence, TrustPolicy,
    },
    synthetic::testkit::*,
};

const COMPUTER: &str = "synthetic-computer-1";

struct Owned {
    tool: String,
    version: String,
    platform: String,
    recipe_digest: String,
    artifact: String,
    effects: String,
    scope: String,
}

impl Owned {
    fn of(f: &Fx) -> Owned {
        let r = f.valid.recipe();
        Owned {
            tool: r.tool_id.clone(),
            version: r.version.clone(),
            platform: r.target.platform_target.clone(),
            recipe_digest: f.valid.digest().to_string(),
            artifact: r.artifact.sha256.clone(),
            effects: f.valid.permissions_digest(),
            scope: r.target.scope.clone(),
        }
    }

    fn expected(&self) -> Expected<'_> {
        Expected {
            action: Action::Install,
            tool_id: &self.tool,
            version: &self.version,
            platform_target: &self.platform,
            computer_id: COMPUTER,
            recipe_digest: &self.recipe_digest,
            artifact_sha256: &self.artifact,
            effects_digest: &self.effects,
            scope: &self.scope,
            project_alias: None,
        }
    }
}

/// A ledger that behaves exactly like the store but makes every caller wait, after it has READ the approval as `issued`, until all callers
/// have read it: the deterministic form of the race ("two concurrent calls can both read `Issued`").
struct BarrierLedger<'a> {
    inner: &'a ApprovalStore,
    barrier: &'a std::sync::Barrier,
}

impl ApprovalLedger for BarrierLedger<'_> {
    fn next_counter(&self) -> Result<u64, TrustError> {
        self.inner.next_counter()
    }
    fn record_issue(&self, id: &str, counter: u64) -> Result<(), TrustError> {
        self.inner.record_issue(id, counter)
    }
    fn status(&self, id: &str) -> Result<Option<LedgerStatus>, TrustError> {
        let st = self.inner.status(id);
        self.barrier.wait();
        st
    }
    fn consume(&self, id: &str, txn: &str) -> Result<(), TrustError> {
        self.inner.consume(id, txn)
    }
    fn revoke(&self, id: &str) -> Result<(), TrustError> {
        self.inner.revoke(id)
    }
    fn atomic_with_audit(&self) -> bool {
        self.inner.atomic_with_audit()
    }
    fn record_issue_audited(&self, id: &str, counter: u64, audit: &dyn AuditSink, record: AuditRecord) -> Result<(), TrustError> {
        self.inner.record_issue_audited(id, counter, audit, record)
    }
    fn consume_audited(&self, id: &str, txn: &str, audit: &dyn AuditSink, record: AuditRecord) -> Result<(), TrustError> {
        self.inner.consume_audited(id, txn, audit, record)
    }
}

struct Rig {
    _d: tempfile::TempDir,
    dir: PathBuf,
    path: PathBuf,
    f: Fx,
    keys: FakeKeyStore,
}

fn rig() -> Rig {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().to_path_buf();
    std::fs::write(dir.join("canary.txt"), b"untouched").unwrap();
    let path = dir.join(FILE_NAME);
    Rig { _d: d, dir, path, f: fx("synthetic-tool", "1.0.0", b"payload one"), keys: FakeKeyStore::new("k") }
}

impl Rig {
    fn store(&self) -> ApprovalStore {
        ApprovalStore::open(&self.path).unwrap()
    }

    fn approval(&self) -> Approval {
        make_approval(&self.f, Action::Install, None, COMPUTER, t0())
    }

    fn issue(&self, s: &ApprovalStore) -> Result<DeviceApproval, TrustError> {
        let o = Owned::of(&self.f);
        device_trust::issue(
            &self.approval(),
            &o.artifact,
            &o.effects,
            &self.keys,
            &FakePresence(Presence::Confirmed),
            s,
            s,
            TrustPolicy::test_doubles(),
        )
    }

    fn authorize_with(&self, s: &ApprovalStore, d: &DeviceApproval, want: &Expected, txn: &str) -> Result<(), TrustError> {
        device_trust::authorize(d, want, txn, t0() + chrono::Duration::seconds(30), &self.keys, s, s, TrustPolicy::test_doubles())
            .map(|_| ())
    }

    fn authorize(&self, s: &ApprovalStore, d: &DeviceApproval, txn: &str) -> Result<(), TrustError> {
        let o = Owned::of(&self.f);
        self.authorize_with(s, d, &o.expected(), txn)
    }

    fn rows(&self) -> Vec<(String, String)> {
        let c = raw(&self.path);
        let mut st = c.prepare("SELECT approval_id, status FROM approvals ORDER BY counter").unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|r| r.unwrap()).collect()
    }

    fn count(&self, event: &str, outcome: &str) -> usize {
        self.store().audit_events().unwrap().iter().filter(|e| e.0 == event && e.1 == outcome).count()
    }

    fn unrelated_is_untouched(&self) {
        assert_eq!(std::fs::read(self.dir.join("canary.txt")).unwrap(), b"untouched");
        let mut names: Vec<String> =
            std::fs::read_dir(&self.dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.retain(|n| !n.starts_with(FILE_NAME));
        assert_eq!(names, vec!["canary.txt".to_string()]);
    }
}

#[test]
fn eight_simultaneous_authorizations_from_separate_connections_commit_exactly_one_success_and_exactly_one_success_row() {
    let r = rig();
    let d = r.issue(&r.store()).unwrap();
    let o = Owned::of(&r.f);
    let path = r.path.clone();
    let barrier = std::sync::Barrier::new(8);
    let results: Vec<Result<(), TrustError>> = std::thread::scope(|sc| {
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let (path, d, o, keys, barrier) = (&path, &d, &o, &r.keys, &barrier);
                sc.spawn(move || {
                    let s = ApprovalStore::open(path).unwrap();
                    // every thread has READ the approval as issued before any of them consumes it
                    let ledger = BarrierLedger { inner: &s, barrier };
                    device_trust::authorize(
                        d,
                        &o.expected(),
                        &format!("txn-{i}"),
                        t0() + chrono::Duration::seconds(30),
                        keys,
                        &ledger,
                        &s,
                        TrustPolicy::test_doubles(),
                    )
                    .map(|_| ())
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(results.iter().filter(|x| x.is_ok()).count(), 1, "{results:?}");
    assert!(results.iter().filter(|x| x.is_err()).all(|x| *x == Err(TrustError::Replayed)), "{results:?}");
    let s = r.store();
    assert_eq!(r.rows(), vec![(d.approval.approval_id.clone(), "consumed".to_string())]);
    assert_eq!(r.count("approval_use", "authorized"), 1, "exactly one success row, for the one that won");
    assert_eq!(r.count("approval_use", "refused"), 7, "every loser has an honest refusal row, never a success row");
    assert_eq!(s.verify_audit_chain(), Ok(s.audit_events().unwrap().len()));
    assert_eq!(s.verify_consistency(), Ok(()));
    r.unrelated_is_untouched();
}

#[test]
fn an_audit_failure_during_issuance_leaves_no_approval_behind_and_the_failure_is_recorded_honestly() {
    let r = rig();
    let s = r.store();
    let before = (r.rows(), r.count("approval_issue", "issued"));
    raw(&r.path)
        .execute_batch("CREATE TRIGGER audit_down BEFORE INSERT ON audit WHEN NEW.event = 'approval_issue' AND NEW.outcome = 'issued' BEGIN SELECT RAISE(ABORT, 'journal down'); END;")
        .unwrap();
    assert_eq!(r.issue(&s).unwrap_err(), TrustError::AuditUnavailable);
    assert_eq!(r.rows(), before.0, "no orphaned 'issued' approval");
    assert_eq!(r.count("approval_issue", "issued"), 0);
    assert_eq!(r.count("approval_issue", "refused"), 1, "the refusal itself is journalled");
    raw(&r.path).execute_batch("DROP TRIGGER audit_down;").unwrap();
    let d = r.issue(&s).unwrap();
    assert_eq!(r.rows(), vec![(d.approval.approval_id.clone(), "issued".to_string())]);
    assert_eq!(s.verify_consistency(), Ok(()));
    assert_eq!(s.verify_audit_chain(), Ok(s.audit_events().unwrap().len()));
}

#[test]
fn an_audit_failure_during_consumption_prevents_it_and_never_leaves_a_success_row() {
    let r = rig();
    let s = r.store();
    let d = r.issue(&s).unwrap();
    raw(&r.path)
        .execute_batch("CREATE TRIGGER audit_down BEFORE INSERT ON audit WHEN NEW.event = 'approval_use' AND NEW.outcome = 'authorized' BEGIN SELECT RAISE(ABORT, 'journal down'); END;")
        .unwrap();
    assert_eq!(r.authorize(&s, &d, "txn-1").unwrap_err(), TrustError::AuditUnavailable);
    assert_eq!(r.rows(), vec![(d.approval.approval_id.clone(), "issued".to_string())], "NOT consumed");
    assert_eq!(r.count("approval_use", "authorized"), 0);
    assert_eq!(r.count("approval_use", "refused"), 1);
    raw(&r.path).execute_batch("DROP TRIGGER audit_down;").unwrap();
    assert!(r.authorize(&s, &d, "txn-2").is_ok(), "once the journal works the same approval is still usable, exactly once");
    assert_eq!(r.authorize(&s, &d, "txn-3").unwrap_err(), TrustError::Replayed);
    assert_eq!(r.count("approval_use", "authorized"), 1);
    assert_eq!(s.verify_consistency(), Ok(()));
}

#[test]
fn a_failing_commit_changes_nothing_for_issuance_and_for_consumption() {
    let r = rig();
    let s = r.store();
    // issuance: the commit fails
    s.fault.store(1, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(r.issue(&s).unwrap_err(), TrustError::LedgerUnavailable);
    assert!(r.rows().is_empty());
    assert_eq!(r.count("approval_issue", "issued"), 0);
    let d = r.issue(&s).unwrap();
    // consumption: the commit fails
    s.fault.store(1, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(r.authorize(&s, &d, "txn-1").unwrap_err(), TrustError::LedgerUnavailable);
    assert_eq!(r.rows(), vec![(d.approval.approval_id.clone(), "issued".to_string())]);
    assert_eq!(r.count("approval_use", "authorized"), 0, "the success row was rolled back with the state change");
    assert!(r.authorize(&s, &d, "txn-2").is_ok());
    assert_eq!(s.verify_consistency(), Ok(()));
    assert_eq!(s.verify_audit_chain(), Ok(s.audit_events().unwrap().len()));
}

/// Run only by [`a_process_that_dies_inside_the_consumption_transaction_…`] as a child process: it starts the consumption and aborts before the commit.
#[test]
#[ignore]
fn child_abort_during_consume() {
    let (Some(path), Some(id)) = (std::env::var_os("ATM_STORE_TEST_PATH"), std::env::var("ATM_STORE_TEST_APPROVAL").ok()) else {
        return;
    };
    let s = ApprovalStore::open(std::path::Path::new(&path)).unwrap();
    s.fault.store(2, std::sync::atomic::Ordering::SeqCst);
    let record = AuditRecord {
        event: "approval_use",
        outcome: "authorized",
        approval_id: Some(id.clone()),
        tool_id: "synthetic-tool".into(),
        version: "1.0.0".into(),
        action: "install",
        code: None,
    };
    let _ = s.consume_audited(&id, "txn-child", &s, record);
}

#[test]
fn a_process_that_dies_inside_the_consumption_transaction_leaves_the_approval_issued_and_the_journal_whole_after_a_restart() {
    let r = rig();
    let s = r.store();
    let d = r.issue(&s).unwrap();
    let id = d.approval.approval_id.clone();
    drop(s);
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "approval_store::tests::child_abort_during_consume", "--include-ignored", "--nocapture", "--test-threads=1"])
        .env("ATM_STORE_TEST_PATH", &r.path)
        .env("ATM_STORE_TEST_APPROVAL", &id)
        .output()
        .unwrap();
    assert!(!out.status.success(), "the child must have died inside the transaction");
    // restart: the interrupted transaction left nothing
    let s = r.store();
    assert_eq!(r.rows(), vec![(id.clone(), "issued".to_string())]);
    assert_eq!(r.count("approval_use", "authorized"), 0);
    assert_eq!(s.verify_audit_chain(), Ok(s.audit_events().unwrap().len()));
    assert_eq!(s.verify_consistency(), Ok(()));
    assert!(r.authorize(&s, &d, "txn-after-restart").is_ok(), "and the approval is intact and usable exactly once");
    assert_eq!(r.authorize(&s, &d, "txn-again").unwrap_err(), TrustError::Replayed);
    r.unrelated_is_untouched();
}

#[test]
fn replayed_and_revoked_approvals_never_produce_a_success_row_and_stay_that_way_after_a_restart() {
    let r = rig();
    let s = r.store();
    let (a, b) = (r.issue(&s).unwrap(), r.issue(&s).unwrap());
    assert!(r.authorize(&s, &a, "txn-1").is_ok());
    s.revoke(&b.approval.approval_id).unwrap();
    for round in 0..2 {
        // round 1 is a "restart": a new store object over the same file
        let again = r.store();
        assert_eq!(r.authorize(&again, &a, "txn-replay").unwrap_err(), TrustError::Replayed);
        assert_eq!(r.authorize(&again, &b, "txn-revoked").unwrap_err(), TrustError::Revoked);
        assert_eq!(r.count("approval_use", "authorized"), 1, "round {round}");
        assert_eq!(r.count("approval_revoke", "revoked"), 1);
        assert_eq!(again.verify_consistency(), Ok(()));
    }
    // revoking something used or already revoked is a no-op that writes nothing more
    s.revoke(&a.approval.approval_id).unwrap();
    s.revoke(&b.approval.approval_id).unwrap();
    assert_eq!(r.count("approval_revoke", "revoked"), 1);
}

#[test]
fn a_forged_audit_row_or_an_altered_approval_state_is_detected_and_never_honoured() {
    let r = rig();
    let s = r.store();
    let d = r.issue(&s).unwrap();
    // (1) a forged row with an invalid chain: the chain check stops every transaction
    raw(&r.path).execute("INSERT INTO audit (ts, event, outcome, approval_id, tool_id, version, action, code, prev_hash, hash) VALUES ('2027-01-01T00:00:00Z','approval_use','authorized',?1,'t','1','install',NULL,'forged-prev','forged-hash')", [&d.approval.approval_id]).unwrap();
    assert_eq!(s.verify_audit_chain(), Err(StoreError::AuditChainBroken));
    assert_eq!(r.authorize(&s, &d, "txn-1").unwrap_err(), TrustError::AuditUnavailable);
    assert_eq!(r.rows(), vec![(d.approval.approval_id.clone(), "issued".to_string())]);
    // (2) a forged row with a VALID chain (the chain has no secret) is a journal that contradicts the ledger: refused as inconsistent
    let r = rig();
    let s = r.store();
    let d = r.issue(&s).unwrap();
    s.record(AuditRecord {
        event: "approval_use",
        outcome: "authorized",
        approval_id: Some(d.approval.approval_id.clone()),
        tool_id: "synthetic-tool".into(),
        version: "1.0.0".into(),
        action: "install",
        code: None,
    })
    .unwrap();
    assert_eq!(s.verify_consistency(), Err(StoreError::Inconsistent));
    assert_eq!(r.authorize(&s, &d, "txn-1").unwrap_err(), TrustError::LedgerInconsistent);
    assert_eq!(
        r.rows(),
        vec![(d.approval.approval_id.clone(), "issued".to_string())],
        "nothing consumed on the strength of a contradicting journal"
    );
    assert_eq!(r.count("approval_use", "authorized"), 1, "no second success row");
    // (3) an altered approval state: a consumed approval is reset to issued (the file owner drops the trigger)
    let r = rig();
    let s = r.store();
    let d = r.issue(&s).unwrap();
    assert!(r.authorize(&s, &d, "txn-1").is_ok());
    raw(&r.path).execute_batch("DROP TRIGGER approvals_terminal; UPDATE approvals SET status = 'issued', txn_id = NULL;").unwrap();
    assert_eq!(s.verify_consistency(), Err(StoreError::Inconsistent));
    assert_eq!(
        r.authorize(&s, &d, "txn-replay").unwrap_err(),
        TrustError::LedgerInconsistent,
        "the journal remembers what the ledger forgot"
    );
    assert_eq!(r.count("approval_use", "authorized"), 1);
    // (4) a revoked approval reset to issued
    let r = rig();
    let s = r.store();
    let d = r.issue(&s).unwrap();
    s.revoke(&d.approval.approval_id).unwrap();
    raw(&r.path).execute_batch("DROP TRIGGER approvals_terminal; UPDATE approvals SET status = 'issued';").unwrap();
    assert_eq!(r.authorize(&s, &d, "txn-x").unwrap_err(), TrustError::LedgerInconsistent);
    assert_eq!(r.count("approval_use", "authorized"), 0);
}

#[test]
fn a_change_of_computer_recipe_artifact_or_effects_is_refused_and_changes_nothing() {
    let r = rig();
    let s = r.store();
    let other = r.issue(&s).unwrap(); // an unrelated approval that must stay untouched
    let d = r.issue(&s).unwrap();
    let o = Owned::of(&r.f);
    let rows_before = r.rows();
    let events_before = r.count("approval_use", "authorized");
    type Mutation = Box<dyn Fn(&mut Expected)>;
    let variants: Vec<(&str, Mutation)> = vec![
        ("computer", Box::new(|w| w.computer_id = "another-computer")),
        ("recipe", Box::new(|w| w.recipe_digest = "f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0")),
        ("artifact", Box::new(|w| w.artifact_sha256 = "e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1")),
        ("effects", Box::new(|w| w.effects_digest = "d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2")),
        ("version", Box::new(|w| w.version = "9.9.9")),
        ("tool", Box::new(|w| w.tool_id = "synthetic-other")),
    ];
    for (name, mutate) in variants {
        let mut want = o.expected();
        mutate(&mut want);
        assert_eq!(r.authorize_with(&s, &d, &want, "txn-x").unwrap_err(), TrustError::BindingMismatch, "{name}");
    }
    assert_eq!(r.rows(), rows_before, "no approval changed state");
    assert_eq!(r.count("approval_use", "authorized"), events_before);
    assert_eq!(r.count("approval_use", "refused"), 6);
    assert_eq!(s.status(&other.approval.approval_id).unwrap(), Some(LedgerStatus::Issued));
    assert!(r.authorize(&s, &d, "txn-ok").is_ok(), "the exact operation still works");
    r.unrelated_is_untouched();
}

#[test]
fn the_atomic_path_refuses_a_journal_that_is_not_the_ledgers_own_and_a_policy_that_needs_atomicity_refuses_a_non_atomic_ledger() {
    let r = rig();
    let s = r.store();
    let o = Owned::of(&r.f);
    let other = VecAudit::default();
    let e = device_trust::issue(
        &r.approval(),
        &o.artifact,
        &o.effects,
        &r.keys,
        &FakePresence(Presence::Confirmed),
        &s,
        &other,
        TrustPolicy::test_doubles(),
    )
    .unwrap_err();
    assert_eq!(e, TrustError::AuditUnavailable);
    assert!(r.rows().is_empty());
    let d = r.issue(&s).unwrap();
    let e = device_trust::authorize(
        &d,
        &o.expected(),
        "t",
        t0() + chrono::Duration::seconds(30),
        &r.keys,
        &s,
        &other,
        TrustPolicy::test_doubles(),
    )
    .unwrap_err();
    assert_eq!(e, TrustError::AuditUnavailable);
    assert_eq!(r.rows(), vec![(d.approval.approval_id.clone(), "issued".to_string())]);
    assert!(s.atomic_with_audit());
    assert!(!device_trust::fakes::InMemoryLedger::default().atomic_with_audit());
}
