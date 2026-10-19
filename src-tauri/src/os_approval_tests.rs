//! Phase 3D-1 tests of the OS-approval layer with FAKE credential backends only: no Windows Hello enrollment, no key, no PIN or
//! biometric prompt, no real credential. Windows and macOS run the same behavioural tests.

use super::{fakes::FakeOsBackend, *};
use crate::{
    approval::{Action, Approval},
    approval_store::{enable_phrase, ApprovalStore, FILE_NAME},
    device_trust::{self, ApprovalLedger, DeviceApproval, Expected, KeyProtection, LedgerStatus, PresenceSource, TrustPolicy},
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
        let keys = OsKeyStore::open(s, &self.backend, &self.backend, COMPUTER)?;
        device_trust::issue_with(&self.approval(), &self.artifact(), &self.effects(), &keys, PresenceSource::SignedByOs, s, s, policy)
    }

    fn authorize(
        &self,
        s: &ApprovalStore,
        d: &DeviceApproval,
        txn: &str,
        policy: TrustPolicy,
    ) -> Result<device_trust::DeviceAuthorization, TrustError> {
        let keys = OsKeyStore::open(s, &self.backend, &self.backend, COMPUTER)?;
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
    assert!(OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER).is_err(), "enabled but no key recorded");
    let s = b.ready();
    let keys = OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER).unwrap();
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
    assert!(matches!(OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER), Err(TrustError::KeyMismatch)));
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
    // (claim, does the double offer an attestation verifier?, production, pilot candidate, test doubles)
    for (claim, verifier, expect_prod, expect_pilot, expect_test) in [
        (Assurance::Fake, false, false, false, true),
        (Assurance::OsSoftware, false, false, false, false),
        (Assurance::OsReportedHardware, false, false, true, false),
        (Assurance::HardwareVerified, false, false, true, false), // claimed, never verified: established only as "reported"
        (Assurance::HardwareVerified, true, true, true, true),    // only a double with a verifier gets there
    ] {
        for (policy, expect, label) in [
            (TrustPolicy::production(), expect_prod, "production"),
            (TrustPolicy::pilot_candidate_os_reported_hardware(), expect_pilot, "pilot candidate"),
            (TrustPolicy::test_doubles(), expect_test, "test doubles"),
        ] {
            let b = bed(claim);
            if verifier {
                b.backend.offer_verifier();
            }
            let s = b.ready();
            assert_eq!(b.issue(&s, policy).is_ok(), expect, "{claim:?} verifier={verifier} {label}");
        }
    }
    // the only assurance a REAL backend can produce today is OsSoftware or OsReportedHardware, never HardwareVerified or Fake
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
    assert!(OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER).is_err());
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
    assert!(OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER).is_ok(), "the key store still works");
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

// ---- RT-3D1-01: authority never comes from the database ---------------------------------------------------------------------------------

fn db(b: &Bed) -> rusqlite::Connection {
    rusqlite::Connection::open(&b.path).unwrap()
}

fn set(b: &Bed, column: &str, value: Option<&str>) {
    db(b).execute(&format!("UPDATE approval_security SET {column} = ?1 WHERE id = 1"), [value]).unwrap();
}

fn refused_codes(s: &ApprovalStore) -> Vec<String> {
    s.audit_events().unwrap().into_iter().filter(|e| e.0 == "approval_key_open" && e.1 == "refused").filter_map(|e| e.3).collect()
}

#[test]
fn an_inflated_assurance_label_in_the_database_never_upgrades_a_key_whatever_it_is_inflated_to() {
    for (claim, label) in [
        (Assurance::OsSoftware, "hardware_verified"),
        (Assurance::OsSoftware, "os_reported_hardware"),
        (Assurance::OsReportedHardware, "hardware_verified"),
        (Assurance::HardwareVerified, "hardware_verified"), // claimed verified, but no verifier exists: established is only "reported"
        (Assurance::Fake, "os_software"),
    ] {
        let b = bed(claim);
        let s = b.ready();
        set(&b, "assurance", Some(label));
        // same key, same public key, only the label differs: the live credential does not support it
        let r = OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER);
        assert!(matches!(r, Err(TrustError::AssuranceRejected)), "{claim:?} -> {label}: {:?}", r.err());
        assert_eq!(refused_codes(&s), vec!["approval_assurance_exceeds_established"], "{claim:?} -> {label}: the attempt is audited");
        // ... and no policy, however permissive, can then issue anything
        for policy in [TrustPolicy::production(), TrustPolicy::pilot_candidate_os_reported_hardware(), TrustPolicy::test_doubles()] {
            assert!(b.issue(&s, policy).is_err(), "{claim:?} -> {label}");
        }
        assert_eq!(b.approvals_recorded(), 0);
        // a restart (new store object over the same file, new key store) changes nothing
        drop(s);
        let s2 = b.store();
        assert!(
            matches!(OsKeyStore::open(&s2, &b.backend, &b.backend, COMPUTER), Err(TrustError::AssuranceRejected)),
            "{claim:?} after restart"
        );
    }
}

#[test]
fn a_lowered_label_can_only_lower_authority() {
    let b = bed(Assurance::OsReportedHardware);
    let s = b.ready();
    assert!(
        b.issue(&s, TrustPolicy::pilot_candidate_os_reported_hardware()).is_ok(),
        "the established strength is accepted by the pilot candidate"
    );
    set(&b, "assurance", Some("os_software"));
    let keys = OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER).unwrap();
    assert_eq!(keys.protection(), KeyProtection::OsSoftware, "the weaker recorded label wins");
    assert_eq!(b.issue(&s, TrustPolicy::pilot_candidate_os_reported_hardware()).unwrap_err(), TrustError::KeyNotOsProtected);
    // an unreadable label is a refusal, not a default
    set(&b, "assurance", Some("super_secure"));
    assert!(OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER).is_err());
}

#[test]
fn edited_missing_swapped_or_changed_attestation_evidence_is_refused_and_audited() {
    use super::fakes::EvidenceMode;
    // the recorded digest edited in the file, same key and same label
    let b = bed(Assurance::OsReportedHardware);
    let s = b.ready();
    assert!(s.security().unwrap().key.unwrap().attestation_digest.is_some(), "evidence was recorded at enrollment");
    set(&b, "attestation_digest", Some(&"b".repeat(64)));
    assert!(matches!(OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER), Err(TrustError::AssuranceRejected)));
    assert_eq!(refused_codes(&s), vec!["approval_attestation_evidence_changed"]);
    drop(s);
    assert!(
        matches!(OsKeyStore::open(&b.store(), &b.backend, &b.backend, COMPUTER), Err(TrustError::AssuranceRejected)),
        "after a restart too"
    );
    // the LIVE evidence disappears, is bound to another key, or is a different (well-formed) evidence
    for (mode, code) in [
        (EvidenceMode::Missing, "approval_assurance_exceeds_established"),
        (EvidenceMode::OtherKey, "approval_assurance_exceeds_established"),
        (EvidenceMode::Changed, "approval_attestation_evidence_changed"),
    ] {
        let b = bed(Assurance::OsReportedHardware);
        let s = b.ready();
        assert!(OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER).is_ok());
        b.backend.set_evidence(mode);
        assert!(matches!(OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER), Err(TrustError::AssuranceRejected)), "{mode:?}");
        assert_eq!(refused_codes(&s), vec![code], "{mode:?}");
        assert_eq!(
            b.issue(&s, TrustPolicy::pilot_candidate_os_reported_hardware()).unwrap_err(),
            TrustError::AssuranceRejected,
            "{mode:?}"
        );
        assert_eq!(b.approvals_recorded(), 0);
    }
    // evidence that is simply absent from the record cannot RAISE anything: the live evidence must still support the label
    let b = bed(Assurance::OsSoftware);
    let s = b.ready();
    set(&b, "attestation_digest", Some(&"c".repeat(64)));
    set(&b, "assurance", Some("os_reported_hardware"));
    assert!(
        matches!(OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER), Err(TrustError::AssuranceRejected)),
        "invented evidence plus an invented label"
    );
}

#[test]
fn a_claim_of_verified_hardware_is_worth_nothing_without_an_attestation_verifier_and_no_real_backend_has_one() {
    // claimed verified, no verifier: what is established (and recorded at enrollment) is only "reported"
    let b = bed(Assurance::HardwareVerified);
    let s = b.ready();
    assert_eq!(s.security().unwrap().key.unwrap().assurance, "os_reported_hardware");
    assert_eq!(b.issue(&s, TrustPolicy::production()).unwrap_err(), TrustError::KeyNotOsProtected);
    assert!(
        b.issue(&s, TrustPolicy::pilot_candidate_os_reported_hardware()).is_ok(),
        "the explicit pilot candidate would take it; production never"
    );
    // a claim of verified hardware WITHOUT bound evidence is software
    let b = bed(Assurance::HardwareVerified);
    b.backend.set_evidence(super::fakes::EvidenceMode::Missing);
    let s = b.ready();
    assert_eq!(s.security().unwrap().key.unwrap().assurance, "os_software");
    // only a double that offers a verifier can make a key production-acceptable (the real backend offers none: asserted on its source)
    let b = bed(Assurance::HardwareVerified);
    b.backend.offer_verifier();
    let s = b.ready();
    assert_eq!(s.security().unwrap().key.unwrap().assurance, "hardware_verified");
    assert!(b.issue(&s, TrustPolicy::production()).is_ok());
    let hello = include_str!("os_hello.rs");
    assert!(
        !hello.contains("fn attestation_verifier")
            && !hello.contains("impl AttestationVerifier")
            && !hello.contains("Assurance::HardwareVerified")
    );
    // and in non-test code only the doubles implement the verifier trait
    let approval = include_str!("os_approval.rs");
    let production_part = approval.split("pub mod fakes").next().unwrap();
    assert!(!production_part.contains("impl AttestationVerifier for"), "no production type may vouch for hardware");
}

#[test]
fn another_computer_a_recorded_key_swapped_in_the_file_or_an_edit_between_open_and_use_never_works() {
    let b = bed(Assurance::OsReportedHardware);
    let s = b.ready();
    // the record names another computer
    set(&b, "computer_id", Some("another-computer"));
    assert!(matches!(OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER), Err(TrustError::KeyUnavailable)));
    set(&b, "computer_id", Some(COMPUTER));
    // an edit AFTER the key store was opened is caught by the next use: the weakest protection is reported and nothing is issued
    let keys = OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER).unwrap();
    assert_eq!(keys.protection(), KeyProtection::OsReportedHardware);
    set(&b, "assurance", Some("hardware_verified"));
    assert_eq!(keys.protection(), KeyProtection::OsSoftware, "re-evaluated on every call");
    let r = device_trust::issue_with(
        &b.approval(),
        &b.artifact(),
        &b.effects(),
        &keys,
        PresenceSource::SignedByOs,
        &s,
        &s,
        TrustPolicy::pilot_candidate_os_reported_hardware(),
    );
    assert_eq!(r.unwrap_err(), TrustError::KeyNotOsProtected);
    assert_eq!(b.approvals_recorded(), 0);
    // the public key edited to another value in the file: the key no longer matches the live credential
    set(&b, "assurance", Some("os_reported_hardware"));
    set(&b, "public_key_hex", Some("00ff"));
    assert!(OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER).is_err());
}

#[test]
fn a_policy_that_needs_an_atomic_ledger_refuses_a_ledger_that_is_not_one() {
    use crate::device_trust::fakes::{InMemoryLedger, VecAudit};
    let b = bed(Assurance::OsReportedHardware);
    let s = b.ready();
    let keys = OsKeyStore::open(&s, &b.backend, &b.backend, COMPUTER).unwrap();
    let (ledger, audit) = (InMemoryLedger::default(), VecAudit::default());
    let r = device_trust::issue_with(
        &b.approval(),
        &b.artifact(),
        &b.effects(),
        &keys,
        PresenceSource::SignedByOs,
        &ledger,
        &audit,
        TrustPolicy::pilot_candidate_os_reported_hardware(),
    );
    assert_eq!(r.unwrap_err(), TrustError::LedgerUnavailable);
    assert_eq!(b.backend.prompts.load(Ordering::SeqCst), 1, "nothing was signed: the person was not asked");
    // the durable store with a DIFFERENT audit sink is refused too: the journal row must be written in the ledger's own transaction
    let other = VecAudit::default();
    let r = device_trust::issue_with(
        &b.approval(),
        &b.artifact(),
        &b.effects(),
        &keys,
        PresenceSource::SignedByOs,
        &s,
        &other,
        TrustPolicy::pilot_candidate_os_reported_hardware(),
    );
    assert_eq!(r.unwrap_err(), TrustError::AuditUnavailable);
    assert_eq!(b.approvals_recorded(), 0);
}
