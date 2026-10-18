//! Phase 3C tests of the approval/key-store/audit interfaces. Fakes only: no real key, no real prompt, nothing persisted.

use super::{fakes::*, *};
use crate::synthetic::testkit::*;
use chrono::Duration;

const COMPUTER: &str = "synthetic-computer-1";

struct Bed {
    f: Fx,
    keys: FakeKeyStore,
    ledger: InMemoryLedger,
    audit: VecAudit,
}

fn bed() -> Bed {
    Bed {
        f: fx("synthetic-tool", "1.0.0", b"payload one"),
        keys: FakeKeyStore::new("fake-key-1"),
        ledger: InMemoryLedger::default(),
        audit: VecAudit::default(),
    }
}

impl Bed {
    fn approval(&self) -> Approval {
        make_approval(&self.f, Action::Install, None, COMPUTER, t0())
    }

    fn effects(&self) -> String {
        self.f.valid.permissions_digest()
    }

    fn artifact(&self) -> String {
        self.f.valid.recipe().artifact.sha256.clone()
    }

    fn issue(&self, a: &Approval) -> Result<DeviceApproval, TrustError> {
        issue(
            a,
            &self.artifact(),
            &self.effects(),
            &self.keys,
            &FakePresence(Presence::Confirmed),
            &self.ledger,
            &self.audit,
            TrustPolicy::test_doubles(),
        )
    }

    fn want<'a>(&'a self, effects: &'a str, artifact: &'a str) -> Expected<'a> {
        let r = self.f.valid.recipe();
        Expected {
            action: Action::Install,
            tool_id: &r.tool_id,
            version: &r.version,
            platform_target: &r.target.platform_target,
            computer_id: COMPUTER,
            recipe_digest: self.f.valid.digest(),
            artifact_sha256: artifact,
            effects_digest: effects,
            scope: &r.target.scope,
            project_alias: None,
        }
    }

    fn authorize(&self, d: &DeviceApproval, txn: &str, now: DateTime<Utc>) -> Result<DeviceAuthorization, TrustError> {
        let (effects, artifact) = (self.effects(), self.artifact());
        authorize(d, &self.want(&effects, &artifact), txn, now, &self.keys, &self.ledger, &self.audit, TrustPolicy::test_doubles())
    }

    fn events(&self) -> Vec<(&'static str, &'static str, Option<&'static str>)> {
        self.audit.events.lock().unwrap().iter().map(|e| (e.event, e.outcome, e.code)).collect()
    }

    /// Re-signs a tampered copy with the genuine key (an attacker who has the key but not the ledger).
    fn resign(&self, d: &mut DeviceApproval) {
        d.signature = hex(&self.keys.sign(&payload(d)).unwrap());
    }
}

#[test]
fn a_user_present_signed_bound_approval_authorizes_exactly_one_transaction() {
    let b = bed();
    let d = b.issue(&b.approval()).unwrap();
    assert!(d.presence_confirmed && d.counter == 1 && d.key_id == "fake-key-1");
    let auth = b.authorize(&d, "txn-1", t0() + Duration::seconds(60)).unwrap();
    assert_eq!(
        (auth.approval_id(), auth.txn_id(), auth.key_protection()),
        (d.approval.approval_id.as_str(), "txn-1", KeyProtection::SoftwareFake)
    );
    assert_eq!(b.ledger.status(&d.approval.approval_id).unwrap(), Some(LedgerStatus::Consumed { txn_id: "txn-1".into() }));
    assert_eq!(b.events(), vec![("approval_issue", "issued", None), ("approval_use", "authorized", None)]);
}

#[test]
fn the_production_policy_refuses_every_fake_so_nothing_built_from_fakes_can_authorize_production() {
    let b = bed();
    let a = b.approval();
    let r =
        issue(&a, &b.artifact(), &b.effects(), &b.keys, &FakePresence(Presence::Confirmed), &b.ledger, &b.audit, TrustPolicy::production());
    assert_eq!(r.err(), Some(TrustError::KeyNotOsProtected));
    // a genuinely valid test-double approval is refused by the production policy at use time as well
    let d = b.issue(&a).unwrap();
    let (effects, artifact) = (b.effects(), b.artifact());
    let r = authorize(&d, &b.want(&effects, &artifact), "txn", t0(), &b.keys, &b.ledger, &b.audit, TrustPolicy::production());
    assert_eq!(r.err(), Some(TrustError::KeyNotOsProtected));
    assert_eq!(b.ledger.status(&d.approval.approval_id).unwrap(), Some(LedgerStatus::Issued), "refused before anything was consumed");
    // and no OS-protected key store exists in the code base yet: the only implementation is the fake
    let production = include_str!("device_trust.rs").split("#[cfg(test)]").next().unwrap();
    assert_eq!(production.matches("impl KeyStore for").count(), 1);
    assert!(production.contains("impl KeyStore for FakeKeyStore"));
    assert_eq!(TrustPolicy::production(), TrustPolicy::production());
    assert_ne!(TrustPolicy::production(), TrustPolicy::test_doubles());
}

#[test]
fn replay_expiry_and_revocation_are_refused() {
    let b = bed();
    let d = b.issue(&b.approval()).unwrap();
    // expiry (the approval lives 600 seconds)
    assert_eq!(b.authorize(&d, "txn-late", t0() + Duration::seconds(600)).err(), Some(TrustError::Expired));
    assert_eq!(b.authorize(&d, "txn-late", t0() + Duration::days(1)).err(), Some(TrustError::Expired));
    // first use works, second use of the same approval is a replay, even by another transaction
    b.authorize(&d, "txn-1", t0() + Duration::seconds(1)).unwrap();
    assert_eq!(b.authorize(&d, "txn-1", t0() + Duration::seconds(2)).err(), Some(TrustError::Replayed));
    assert_eq!(b.authorize(&d, "txn-2", t0() + Duration::seconds(2)).err(), Some(TrustError::Replayed));
    // revocation before use
    let d2 = b.issue(&b.approval()).unwrap();
    assert_eq!(d2.counter, 2, "counters only grow");
    b.ledger.revoke(&d2.approval.approval_id).unwrap();
    assert_eq!(b.authorize(&d2, "txn-3", t0() + Duration::seconds(1)).err(), Some(TrustError::Revoked));
    // revoking after consumption changes nothing (the transaction already holds it)
    b.ledger.revoke(&d.approval.approval_id).unwrap();
    assert_eq!(b.ledger.status(&d.approval.approval_id).unwrap(), Some(LedgerStatus::Consumed { txn_id: "txn-1".into() }));
    // every refusal is audited with its code
    let refused: Vec<_> = b.events().into_iter().filter(|e| e.1 == "refused").map(|e| e.2.unwrap()).collect();
    assert_eq!(refused, vec!["approval_expired", "approval_expired", "approval_replayed", "approval_replayed", "approval_revoked"]);
}

#[test]
fn an_unkeyed_digest_a_forged_field_a_foreign_key_or_an_unissued_approval_is_not_authentication() {
    let b = bed();
    let a = b.approval();
    let genuine = b.issue(&a).unwrap();
    let now = t0() + Duration::seconds(1);

    // an unkeyed SHA-256 over the same payload (what Phase 3A has) is not a signature
    let mut sha_only = genuine.clone();
    sha_only.signature = hex(&Sha256::digest(payload(&genuine)));
    assert_eq!(b.authorize(&sha_only, "t", now).err(), Some(TrustError::SignatureInvalid));
    // garbage and empty signatures
    for sig in ["", "zz", "00", &"0".repeat(64)] {
        let mut d = genuine.clone();
        d.signature = sig.to_string();
        assert_eq!(b.authorize(&d, "t", now).err(), Some(TrustError::SignatureInvalid), "{sig:?}");
    }
    // every signed field: a change without re-signing is refused
    for mutate in [
        |d: &mut DeviceApproval| d.artifact_sha256 = "0".repeat(64),
        |d: &mut DeviceApproval| d.effects_digest = "0".repeat(64),
        |d: &mut DeviceApproval| d.counter += 1,
        |d: &mut DeviceApproval| d.approval.version = "9.9.9".into(),
        |d: &mut DeviceApproval| d.approval.expires_at = "2099-01-01T00:00:00Z".into(),
        |d: &mut DeviceApproval| d.approval.computer_id = "other".into(),
    ] {
        let mut d = genuine.clone();
        mutate(&mut d);
        assert_eq!(b.authorize(&d, "t", now).err(), Some(TrustError::SignatureInvalid));
    }
    // another key store (same id or not)
    let other = FakeKeyStore::new("fake-key-1");
    let mut d = genuine.clone();
    d.signature = hex(&other.sign(&payload(&d)).unwrap());
    assert_eq!(b.authorize(&d, "t", now).err(), Some(TrustError::SignatureInvalid));
    let mut d = genuine.clone();
    d.key_id = "someone-else".into();
    assert_eq!(b.authorize(&d, "t", now).err(), Some(TrustError::KeyMismatch));
    // signed with the genuine key but never issued by the ledger: unknown
    let mut forged = genuine.clone();
    forged.approval.approval_id = "00000000-0000-4000-8000-000000000000".into();
    b.resign(&mut forged);
    assert_eq!(b.authorize(&forged, "t", now).err(), Some(TrustError::UnknownApproval));
    // none of that consumed the genuine approval
    assert!(b.authorize(&genuine, "t", now).is_ok());
}

#[test]
fn user_presence_is_required_and_part_of_the_signed_payload() {
    let b = bed();
    let a = b.approval();
    for (p, e) in [(Presence::Declined, TrustError::NotUserPresent), (Presence::Unavailable, TrustError::PresenceUnavailable)] {
        let r = issue(&a, &b.artifact(), &b.effects(), &b.keys, &FakePresence(p), &b.ledger, &b.audit, TrustPolicy::test_doubles());
        assert_eq!(r.err(), Some(e));
    }
    assert_eq!(b.ledger.status(&a.approval_id).unwrap(), None, "nothing was recorded");
    let mut d = b.issue(&a).unwrap();
    d.presence_confirmed = false;
    assert_eq!(b.authorize(&d, "t", t0()).err(), Some(TrustError::SignatureInvalid), "flipping the flag breaks the signature");
    b.resign(&mut d);
    assert_eq!(b.authorize(&d, "t", t0()).err(), Some(TrustError::NotUserPresent), "even a re-signed approval without presence is refused");
}

#[test]
fn an_approval_for_another_operation_is_refused_and_not_consumed() {
    let b = bed();
    let d = b.issue(&b.approval()).unwrap();
    let now = t0() + Duration::seconds(1);
    let (effects, artifact) = (b.effects(), b.artifact());
    let zeros = "0".repeat(64);
    let base = b.want(&effects, &artifact);
    let cases: Vec<(&str, Expected)> = vec![
        ("action", Expected { action: Action::Update, ..base.clone() }),
        ("tool", Expected { tool_id: "synthetic-other", ..base.clone() }),
        ("version", Expected { version: "2.0.0", ..base.clone() }),
        ("platform", Expected { platform_target: other_platform(), ..base.clone() }),
        ("computer", Expected { computer_id: "synthetic-computer-2", ..base.clone() }),
        ("recipe", Expected { recipe_digest: &zeros, ..base.clone() }),
        ("artifact", Expected { artifact_sha256: &zeros, ..base.clone() }),
        ("effects", Expected { effects_digest: &zeros, ..base.clone() }),
        ("scope", Expected { scope: "project", ..base.clone() }),
        ("project", Expected { project_alias: Some("proje-a"), ..base.clone() }),
    ];
    for (what, want) in cases {
        let r = authorize(&d, &want, "t", now, &b.keys, &b.ledger, &b.audit, TrustPolicy::test_doubles());
        assert_eq!(r.err(), Some(TrustError::BindingMismatch), "{what}");
    }
    assert_eq!(b.ledger.status(&d.approval.approval_id).unwrap(), Some(LedgerStatus::Issued), "no refusal consumed the approval");
    assert!(b.authorize(&d, "t", now).is_ok());
}

#[test]
fn if_the_audit_trail_the_ledger_or_the_key_is_unavailable_nothing_is_authorized() {
    // audit unavailable at issue
    let mut b = bed();
    b.audit.broken = true;
    assert_eq!(b.issue(&b.approval()).err(), Some(TrustError::AuditUnavailable));
    // audit unavailable at use: the approval is NOT consumed
    let mut b = bed();
    let d = b.issue(&b.approval()).unwrap();
    b.audit.broken = true;
    assert_eq!(b.authorize(&d, "t", t0()).err(), Some(TrustError::AuditUnavailable));
    assert_eq!(b.ledger.status(&d.approval.approval_id).unwrap(), Some(LedgerStatus::Issued));
    b.audit.broken = false;
    assert!(b.authorize(&d, "t", t0()).is_ok());
    // ledger unavailable
    let mut b = bed();
    let d = b.issue(&b.approval()).unwrap();
    b.ledger.broken = true;
    assert_eq!(b.authorize(&d, "t", t0()).err(), Some(TrustError::LedgerUnavailable));
    let mut b2 = bed();
    b2.ledger.broken = true;
    assert_eq!(b2.issue(&b2.approval()).err(), Some(TrustError::LedgerUnavailable));
    // key unavailable
    let mut b = bed();
    b.keys.available = false;
    assert_eq!(b.issue(&b.approval()).err(), Some(TrustError::KeyUnavailable));
}

#[test]
fn audit_records_carry_ids_and_codes_only() {
    let b = bed();
    let d = b.issue(&b.approval()).unwrap();
    b.authorize(&d, "txn", t0()).unwrap();
    let text = format!("{:?}", b.audit.events.lock().unwrap());
    for needle in ["C:\\", "/Users/", "/home/", "ghp_", "Bearer ", "signature", "secret"] {
        assert!(!text.contains(needle), "{needle}");
    }
}

#[test]
fn the_interfaces_have_no_file_system_process_or_network_capability() {
    let production = include_str!("device_trust.rs").split("#[cfg(test)]").next().unwrap();
    for needle in ["std::fs", "std::process", "std::net", "Command::new", "reqwest", "set_var", "File::"] {
        assert!(!production.contains(needle), "{needle}");
    }
}
