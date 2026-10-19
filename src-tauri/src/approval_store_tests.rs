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
