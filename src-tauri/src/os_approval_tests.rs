//! Phase 3D-1 tests of the OS-approval layer with FAKE credential backends only: no Windows Hello enrollment, no key, no PIN or
//! biometric prompt, no real credential. Windows and macOS run the same behavioural tests.

use super::{fakes::FakeOsBackend, *};
use crate::{
    approval::{Action, Approval},
    approval_store::{enable_phrase, ApprovalStore, FILE_NAME},
    device_trust::{self, ApprovalLedger, DeviceApproval, Expected, LedgerStatus, PresenceSource, TrustPolicy},
    synthetic::testkit::*,
};
use chrono::Duration;
use std::{path::PathBuf, sync::atomic::Ordering};

const COMPUTER: &str = "synthetic-computer-1";
const KEY: &str = "atm-approval-test";

struct Bed {
    _d: tempfile::TempDir,
    path: PathBuf,
    f: Fx,
    backend: FakeOsBackend,
}

fn bed(claims: Assurance) -> Bed {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join(FILE_NAME);
    Bed { _d: d, path, f: fx("synthetic-tool", "1.0.0", b"payload one"), backend: FakeOsBackend::new(claims) }
}

impl Bed {
    fn store(&self) -> ApprovalStore {
        ApprovalStore::open(&self.path).unwrap()
    }

    fn enabled_store(&self) -> ApprovalStore {
        let s = self.store();
        s.enable_approval_security(COMPUTER, &enable_phrase(COMPUTER)).unwrap();
        s
    }

    /// Enabled + provisioned.
    fn ready(&self) -> ApprovalStore {
        let s = self.enabled_store();
        provision(&s, &self.backend, COMPUTER, KEY).unwrap();
        s
    }

    fn approvals_recorded(&self) -> i64 {
        rusqlite::Connection::open(&self.path).unwrap().query_row("SELECT COUNT(*) FROM approvals", [], |r| r.get(0)).unwrap()
    }

    fn approval(&self) -> Approval {
        make_approval(&self.f, Action::Install, None, COMPUTER, t0())
    }

    fn artifact(&self) -> String {
        self.f.valid.recipe().artifact.sha256.clone()
    }

    fn effects(&self) -> String {
        self.f.valid.permissions_digest()
    }

    fn issue(&self, s: &ApprovalStore, policy: TrustPolicy) -> Result<DeviceApproval, TrustError> {
        let keys = OsKeyStore::open(s, &self.backend, &self.backend)?;
        device_trust::issue_with(&self.approval(), &self.artifact(), &self.effects(), &keys, PresenceSource::SignedByOs, s, s, policy)
    }

    fn authorize(
        &self,
        s: &ApprovalStore,
        d: &DeviceApproval,
        txn: &str,
        policy: TrustPolicy,
    ) -> Result<device_trust::DeviceAuthorization, TrustError> {
        let keys = OsKeyStore::open(s, &self.backend, &self.backend)?;
        let (artifact, effects) = (self.artifact(), self.effects());
        let r = self.f.valid.recipe();
        let want = Expected {
            action: Action::Install,
            tool_id: &r.tool_id,
            version: &r.version,
            platform_target: &r.target.platform_target,
            computer_id: COMPUTER,
            recipe_digest: self.f.valid.digest(),
            artifact_sha256: &artifact,
            effects_digest: &effects,
            scope: &r.target.scope,
            project_alias: None,
        };
        device_trust::authorize(d, &want, txn, t0() + Duration::seconds(30), &keys, s, s, policy)
    }
}

// ---- no silent key creation ----------------------------------------------------------------------------------------------------

#[test]
fn no_key_is_ever_created_before_the_person_explicitly_enabled_approval_security_for_this_computer() {
    let b = bed(Assurance::Fake);
    let s = b.store();
    assert_eq!(provision(&s, &b.backend, COMPUTER, KEY).unwrap_err(), ProvisionError::Store(StoreError::NotEnabled));
    // a wrong phrase does not enable it either
    assert!(s.enable_approval_security(COMPUTER, "yes").is_err());
    assert_eq!(provision(&s, &b.backend, COMPUTER, KEY).unwrap_err(), ProvisionError::Store(StoreError::NotEnabled));
    assert_eq!((b.backend.creations.load(Ordering::SeqCst), b.backend.prompts.load(Ordering::SeqCst)), (0, 0));
    // enabled for ANOTHER computer: still nothing
    s.enable_approval_security("other-pc", &enable_phrase("other-pc")).unwrap();
    assert_eq!(provision(&s, &b.backend, COMPUTER, KEY).unwrap_err(), ProvisionError::Store(StoreError::OtherComputer));
    assert_eq!(b.backend.creations.load(Ordering::SeqCst), 0);
}

#[test]
fn provisioning_creates_exactly_one_key_records_only_its_public_part_and_never_runs_twice() {
    let b = bed(Assurance::OsReportedHardware);
    let s = b.enabled_store();
    let info = provision(&s, &b.backend, COMPUTER, KEY).unwrap();
    assert_eq!(b.backend.creations.load(Ordering::SeqCst), 1);
    assert_eq!(info.assurance, Assurance::OsReportedHardware);
    let st = s.security().unwrap();
    let rec = st.key.unwrap();
    assert_eq!((rec.key_id.as_str(), rec.assurance.as_str()), (info.key_id.as_str(), "os_reported_hardware"));
    assert!(rec.attestation_digest.is_some(), "the attestation digest is kept as evidence, not as a verification");
    assert_eq!(provision(&s, &b.backend, COMPUTER, KEY).unwrap_err(), ProvisionError::Store(StoreError::KeyAlreadyRecorded));
    assert_eq!(b.backend.creations.load(Ordering::SeqCst), 1);
}

#[test]
fn a_key_that_exists_in_the_os_but_not_in_the_store_is_reported_and_never_adopted() {
    let b = bed(Assurance::Fake);
    let s = b.enabled_store();
    b.backend.plant_unrecorded_key(KEY);
    assert_eq!(provision(&s, &b.backend, COMPUTER, KEY).unwrap_err(), ProvisionError::UnrecordedKeyExists);
    assert!(s.security().unwrap().key.is_none());
    assert_eq!(b.backend.creations.load(Ordering::SeqCst), 0);
}

#[test]
fn declining_the_creation_prompt_or_an_unavailable_backend_creates_and_records_nothing() {
    let b = bed(Assurance::Fake);
    let s = b.enabled_store();
    b.backend.set_verification(UserVerification::Declined);
    assert_eq!(provision(&s, &b.backend, COMPUTER, KEY).unwrap_err(), ProvisionError::Backend(BackendError::Declined));
    assert_eq!(b.backend.creations.load(Ordering::SeqCst), 0);
    assert!(s.security().unwrap().key.is_none());
    let mut off = FakeOsBackend::new(Assurance::Fake);
    off.is_available = false;
    assert_eq!(provision(&s, &off, COMPUTER, KEY).unwrap_err(), ProvisionError::BackendUnavailable);
    assert_eq!(off.prompts.load(Ordering::SeqCst), 0);
}

// ---- user presence comes from the OS operation, never from a flag or a click ---------------------------------------------------

#[test]
fn an_os_key_never_signs_without_the_person_and_cannot_be_opened_without_a_recorded_key() {
    let b = bed(Assurance::Fake);
    let s = b.enabled_store();
    assert!(OsKeyStore::open(&s, &b.backend, &b.backend).is_err(), "enabled but no key recorded");
    let s = b.ready();
    let keys = OsKeyStore::open(&s, &b.backend, &b.backend).unwrap();
    assert_eq!(keys.sign(b"anything"), Err(TrustError::PresenceUnavailable), "there is no silent signing path");
    assert_eq!(b.backend.prompts.load(Ordering::SeqCst), 1, "only the creation prompted; sign() did not sign or prompt");
}

#[test]
fn a_full_issue_and_use_cycle_proves_presence_by_the_signing_operation_and_is_single_use_across_restarts() {
    let b = bed(Assurance::Fake);
    let s = b.ready();
    let d = b.issue(&s, TrustPolicy::test_doubles()).unwrap();
    assert!(d.presence_confirmed);
    assert_eq!(b.backend.prompts.load(Ordering::SeqCst), 2, "one prompt for the key, one for THIS signature");
    assert!(b.authorize(&s, &d, "txn-1", TrustPolicy::test_doubles()).is_ok());
    assert_eq!(b.authorize(&s, &d, "txn-2", TrustPolicy::test_doubles()).unwrap_err(), TrustError::Replayed);
    // the process restarts: a new store, a new key-store object, the same file
    drop(s);
    let s2 = b.store();
    assert_eq!(b.authorize(&s2, &d, "txn-3", TrustPolicy::test_doubles()).unwrap_err(), TrustError::Replayed, "no replay after a restart");
    assert_eq!(s2.status(&d.approval.approval_id).unwrap(), Some(LedgerStatus::Consumed { txn_id: "txn-1".into() }));
    let events: Vec<(String, String)> = s2.audit_events().unwrap().into_iter().map(|e| (e.0, e.1)).collect();
    assert!(events.contains(&("approval_issue".into(), "issued".into())) && events.contains(&("approval_use".into(), "authorized".into())));
    assert_eq!(events.iter().filter(|e| e.0 == "approval_use" && e.1 == "refused").count(), 2);
    assert_eq!(s2.verify_audit_chain(), Ok(events.len()));
}

#[test]
fn a_declined_or_unavailable_verification_yields_no_approval_and_leaves_the_ledger_empty() {
    let b = bed(Assurance::Fake);
    let s = b.ready();
    b.backend.set_verification(UserVerification::Declined);
    assert_eq!(b.issue(&s, TrustPolicy::test_doubles()).unwrap_err(), TrustError::NotUserPresent);
    b.backend.set_verification(UserVerification::Unavailable);
    assert_eq!(b.issue(&s, TrustPolicy::test_doubles()).unwrap_err(), TrustError::PresenceUnavailable);
    assert_eq!(b.approvals_recorded(), 0, "nothing was recorded as issued");
    let refused = s.audit_events().unwrap().iter().filter(|e| e.0 == "approval_issue" && e.1 == "refused").count();
    assert_eq!(refused, 2, "both refusals are audited");
    b.backend.set_verification(UserVerification::Verified);
    assert!(b.issue(&s, TrustPolicy::test_doubles()).is_ok(), "and a present person still works afterwards");
}

#[test]
fn a_replaced_os_key_is_detected_before_any_use() {
    let b = bed(Assurance::Fake);
    let s = b.ready();
    let d = b.issue(&s, TrustPolicy::test_doubles()).unwrap();
    b.backend.swap_key(KEY);
    assert!(matches!(OsKeyStore::open(&s, &b.backend, &b.backend), Err(TrustError::KeyMismatch)));
    assert!(b.authorize(&s, &d, "txn-1", TrustPolicy::test_doubles()).is_err());
    assert_eq!(s.status(&d.approval.approval_id).unwrap(), Some(LedgerStatus::Issued), "nothing consumed");
}

#[test]
fn a_signature_that_is_not_from_the_key_is_refused_and_so_is_every_cheap_substitute() {
    let b = bed(Assurance::Fake);
    let s = b.ready();
    let d = b.issue(&s, TrustPolicy::test_doubles()).unwrap();
    use sha2::{Digest, Sha256};
    let digest_only: String = Sha256::digest(serde_json::to_vec(&d.approval).unwrap()).iter().map(|x| format!("{x:02x}")).collect();
    let mut cases = Vec::new();
    for sig in [digest_only, "a".repeat(64), String::new(), "zz".into(), "00".repeat(32)] {
        let mut forged = d.clone();
        forged.signature = sig;
        cases.push(forged);
    }
    let mut no_presence = d.clone();
    no_presence.presence_confirmed = false;
    cases.push(no_presence);
    let mut other_effects = d.clone();
    other_effects.effects_digest = "f".repeat(64);
    cases.push(other_effects);
    for (i, forged) in cases.iter().enumerate() {
        assert!(b.authorize(&s, forged, &format!("txn-{i}"), TrustPolicy::test_doubles()).is_err(), "case {i}");
    }
    assert_eq!(s.status(&d.approval.approval_id).unwrap(), Some(LedgerStatus::Issued), "no forgery consumed the real approval");
    assert!(b.authorize(&s, &d, "txn-real", TrustPolicy::test_doubles()).is_ok());
}

// ---- hardware claims are verified, not assumed -------------------------------------------------------------------------------------

#[test]
fn the_production_policy_accepts_only_a_verified_hardware_key_and_nothing_real_can_be_one_yet() {
    for (claim, expect_prod, expect_pilot, expect_test) in [
        (Assurance::Fake, false, false, true),
        (Assurance::OsSoftware, false, false, false),
        (Assurance::OsReportedHardware, false, true, false),
        (Assurance::HardwareVerified, true, true, true),
    ] {
        let b = bed(claim);
        let s = b.ready();
        let attempt = |policy| b.issue(&s, policy).is_ok();
        // each attempt uses a fresh approval id? the approval is the same object, so use separate beds per policy
        assert_eq!(attempt(TrustPolicy::production()), expect_prod, "{claim:?} production");
        let b2 = bed(claim);
        let s2 = b2.ready();
        assert_eq!(b2.issue(&s2, TrustPolicy::pilot_candidate_os_reported_hardware()).is_ok(), expect_pilot, "{claim:?} pilot candidate");
        let b3 = bed(claim);
        let s3 = b3.ready();
        assert_eq!(b3.issue(&s3, TrustPolicy::test_doubles()).is_ok(), expect_test, "{claim:?} test doubles");
    }
    // the only assurance a REAL backend can produce today is OsSoftware or OsReportedHardware, never HardwareVerified
    let text = include_str!("os_hello.rs");
    assert!(!text.contains("Assurance::HardwareVerified"), "no backend may claim a verification it does not perform");
    assert!(!text.contains("Assurance::Fake"));
}

#[test]
fn a_refused_key_protection_is_audited_and_nothing_is_issued_or_consumed() {
    let b = bed(Assurance::OsReportedHardware);
    let s = b.ready();
    assert_eq!(b.issue(&s, TrustPolicy::production()).unwrap_err(), TrustError::KeyNotOsProtected);
    assert_eq!(b.approvals_recorded(), 0);
    let ev = s.audit_events().unwrap();
    assert!(ev.iter().any(|e| e.0 == "approval_issue" && e.1 == "refused" && e.3.as_deref() == Some("approval_key_not_os_protected")));
    // an approval issued under the pilot candidate policy is still refused when USED under the production policy
    let d = b.issue(&s, TrustPolicy::pilot_candidate_os_reported_hardware()).unwrap();
    assert_eq!(b.authorize(&s, &d, "txn-1", TrustPolicy::production()).unwrap_err(), TrustError::KeyNotOsProtected);
    assert_eq!(s.status(&d.approval.approval_id).unwrap(), Some(LedgerStatus::Issued));
}

// ---- nothing in the application reaches any of this -------------------------------------------------------------------------------

#[test]
fn the_production_adapter_and_the_application_never_reach_the_real_backend_or_these_modules() {
    for (name, text) in [
        ("commands.rs", include_str!("commands.rs")),
        ("adapter.rs", include_str!("adapter.rs")),
        ("lifecycle.rs", include_str!("lifecycle.rs")),
        ("state.rs", include_str!("state.rs")),
        ("main.rs", include_str!("main.rs")),
    ] {
        for needle in ["os_approval", "os_hello::backend", "WindowsHelloBackend", "CngRsaSha256Verifier", "provision("] {
            assert!(!text.contains(needle), "{name} must not reference {needle}");
        }
    }
    // the only caller of the (read-only) probe is the headless self-check
    let selfcheck = include_str!("selfcheck.rs");
    assert!(selfcheck.contains("os_hello::availability_probe") && !selfcheck.contains("backend"));
    assert!(device_trust::TrustPolicy::production() != device_trust::TrustPolicy::pilot_candidate_os_reported_hardware());
}

// ---- enrollment is reversible, explicitly, and never half-done ------------------------------------------------------------------------------

#[test]
fn switching_approval_security_off_deletes_the_os_key_first_clears_the_record_and_keeps_the_history() {
    use crate::approval_store::disable_phrase;
    let b = bed(Assurance::Fake);
    let s = b.ready();
    let d = b.issue(&s, TrustPolicy::test_doubles()).unwrap();
    b.authorize(&s, &d, "txn-1", TrustPolicy::test_doubles()).unwrap();
    // a wrong phrase changes nothing in the OS or in the store, and is audited
    let enable = enable_phrase(COMPUTER);
    for typed in ["", "yes", "DISABLE-OS-APPROVAL", enable.as_str(), "DISABLE-OS-APPROVAL other-pc"] {
        assert!(deprovision(&s, &b.backend, COMPUTER, typed).is_err(), "{typed:?}");
    }
    assert!(b.backend.has_key(KEY) && s.security().unwrap().key.is_some() && b.backend.deletions.load(Ordering::SeqCst) == 0);
    // the exact phrase: key deleted, record cleared, approval security off
    deprovision(&s, &b.backend, COMPUTER, &disable_phrase(COMPUTER)).unwrap();
    assert!(!b.backend.has_key(KEY));
    let st = s.security().unwrap();
    assert!(!st.enabled && st.key.is_none() && st.computer_id.is_none());
    // no key store can be opened any more, and a new key needs a NEW explicit enabling
    assert!(OsKeyStore::open(&s, &b.backend, &b.backend).is_err());
    assert_eq!(provision(&s, &b.backend, COMPUTER, KEY).unwrap_err(), ProvisionError::Store(StoreError::NotEnabled));
    // history is kept: the consumed approval is still consumed, the audit chain still verifies and records both switches
    assert_eq!(s.status(&d.approval.approval_id).unwrap(), Some(LedgerStatus::Consumed { txn_id: "txn-1".into() }));
    let ev = s.audit_events().unwrap();
    assert!(ev.iter().any(|e| e.0 == "approval_security_disable" && e.1 == "disabled"));
    assert_eq!(ev.iter().filter(|e| e.0 == "approval_security_disable" && e.1 == "refused").count(), 5);
    assert_eq!(s.verify_audit_chain(), Ok(ev.len()));
    // not enabled any more: a second call is refused and touches nothing
    assert_eq!(
        deprovision(&s, &b.backend, COMPUTER, &disable_phrase(COMPUTER)).unwrap_err(),
        ProvisionError::Store(StoreError::NotEnabled)
    );
    assert_eq!(b.backend.deletions.load(Ordering::SeqCst), 1);
}

#[test]
fn if_the_os_key_cannot_be_deleted_the_record_is_not_cleared_so_it_keeps_describing_the_key_that_exists() {
    use crate::approval_store::disable_phrase;
    let b = bed(Assurance::Fake);
    let s = b.ready();
    b.backend.delete_fails.store(true, Ordering::SeqCst);
    assert_eq!(
        deprovision(&s, &b.backend, COMPUTER, &disable_phrase(COMPUTER)).unwrap_err(),
        ProvisionError::Backend(BackendError::Failed)
    );
    assert!(b.backend.has_key(KEY));
    let st = s.security().unwrap();
    assert!(st.enabled && st.key.is_some(), "still enabled, record intact");
    assert!(OsKeyStore::open(&s, &b.backend, &b.backend).is_ok(), "the key store still works");
    // and a retry after the problem is gone succeeds
    b.backend.delete_fails.store(false, Ordering::SeqCst);
    assert!(deprovision(&s, &b.backend, COMPUTER, &disable_phrase(COMPUTER)).is_ok());
}

#[test]
fn deprovisioning_something_never_enabled_touches_nothing() {
    use crate::approval_store::disable_phrase;
    let b = bed(Assurance::Fake);
    let s = b.store();
    b.backend.plant_unrecorded_key(KEY);
    assert_eq!(
        deprovision(&s, &b.backend, COMPUTER, &disable_phrase(COMPUTER)).unwrap_err(),
        ProvisionError::Store(StoreError::NotEnabled)
    );
    assert!(b.backend.has_key(KEY), "a key the store does not know is never deleted by this path");
    assert_eq!(b.backend.deletions.load(Ordering::SeqCst), 0);
}
