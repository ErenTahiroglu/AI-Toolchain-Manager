//! Phase 3C: narrow interfaces for device-bound, user-present, single-use approvals, and the audit trail around them.
//!
//! There is NO real implementation: no OS key store, no user-presence prompt, no approval button. The traits exist so the
//! isolated test-mode adapter can be built and tested against fakes, and so that the production policy can state, in code, what a
//! real approval will have to be:
//!
//! * **signed by a key held by the operating system** for this user and device (`KeyProtection::OsProtected`). The only key store
//!   implemented here is a fake that declares itself `SoftwareFake`; [`TrustPolicy::production`] refuses it, so nothing built from
//!   fakes can authorize the production adapter, and no OS key store exists yet, so nothing can;
//! * **given with user presence** (`presence` is part of the signed payload);
//! * **bound** to computer, recipe digest, tool, exact version, platform, scope, project, the exact artifact digest and the effects
//!   (permissions) digest, on top of the Phase 3A approval binding;
//! * **expiring**, **revocable** and **single-use**: [`authorize`] consumes the approval in the ledger atomically for one
//!   transaction id; a second use of the same approval is a replay and is refused;
//! * **audited** before it takes effect: if the audit record cannot be written, nothing is authorized.
//!
//! An unkeyed SHA-256 (the Phase 3A `binding`) and a typed confirmation string are tamper evidence, never authentication.
//!
//! Phase 3D-1 adds the vocabulary for REAL key stores without enabling one: `KeyProtection` now separates an OS key whose hardware
//! protection was VERIFIED (`OsProtected`, nothing implements this yet), one the OS merely REPORTS as hardware-backed
//! (`OsReportedHardware`), and a software-protected OS key (`OsSoftware`). `TrustPolicy::production()` accepts only the first.
//! A key store may prove user presence by the signing operation itself (`KeyStore::sign_present`, as Windows Hello does); a plain
//! button click or a flag is never presence.
#![allow(dead_code)] // test-mode adapter only; no command reaches this module

use crate::{
    approval::{Action, Approval},
    audit::canonical_json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyProtection {
    /// A key the operating system protects (CNG/Windows Hello, Keychain/Secure Enclave) whose HARDWARE protection this program VERIFIED
    /// (attestation chain checked). No implementation exists yet, so nothing real can be this.
    OsProtected,
    /// An OS key credential for which the OS REPORTS hardware (TPM) attestation, not validated by this program. Refused by
    /// [`TrustPolicy::production`]; only a policy the user and the Red Team accept explicitly could take it.
    OsReportedHardware,
    /// An OS key credential without hardware attestation (software-protected). Never accepted.
    OsSoftware,
    /// A test double. Never acceptable to the production policy.
    SoftwareFake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustPolicy {
    accept_fakes: bool,
    accept_os_reported_hardware: bool,
    /// Only a ledger that writes the journal row in the SAME durable transaction as the state change may back this policy.
    require_atomic_book: bool,
}

impl TrustPolicy {
    /// What production will require: an OS-protected key. Fakes are refused.
    pub const fn production() -> TrustPolicy {
        TrustPolicy { accept_fakes: false, accept_os_reported_hardware: false, require_atomic_book: true }
    }

    /// For the isolated test-mode adapter only.
    pub const fn test_doubles() -> TrustPolicy {
        TrustPolicy { accept_fakes: true, accept_os_reported_hardware: false, require_atomic_book: false }
    }

    /// PROPOSAL for the first Windows pilot, NOT used anywhere: additionally accepts an OS key credential the OS reports as hardware
    /// backed (Windows Hello with TPM attestation reported). It still refuses fakes and software keys. Whether this is acceptable
    /// instead of a verified attestation is a decision for the user and the Red Team (docs/phase-3d1/WINDOWS-HELLO-APPROVAL.md).
    pub const fn pilot_candidate_os_reported_hardware() -> TrustPolicy {
        TrustPolicy { accept_fakes: false, accept_os_reported_hardware: true, require_atomic_book: true }
    }

    fn accepts_book(self, ledger: &dyn ApprovalLedger) -> bool {
        !self.require_atomic_book || ledger.atomic_with_audit()
    }

    fn accepts(self, p: KeyProtection) -> bool {
        match p {
            KeyProtection::OsProtected => true,
            KeyProtection::OsReportedHardware => self.accept_os_reported_hardware,
            KeyProtection::OsSoftware => false,
            KeyProtection::SoftwareFake => self.accept_fakes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustError {
    KeyNotOsProtected,
    KeyUnavailable,
    SignatureInvalid,
    KeyMismatch,
    NotUserPresent,
    PresenceUnavailable,
    Expired,
    Revoked,
    Replayed,
    UnknownApproval,
    BindingMismatch,
    AuditUnavailable,
    LedgerUnavailable,
    /// The ledger and the audit journal disagree about an approval (e.g. a consumed approval reset to issued).
    LedgerInconsistent,
    /// The recorded assurance is stronger than the live credential establishes, or its attestation evidence changed.
    AssuranceRejected,
}

impl TrustError {
    pub fn code(self) -> &'static str {
        match self {
            TrustError::KeyNotOsProtected => "approval_key_not_os_protected",
            TrustError::KeyUnavailable => "approval_key_unavailable",
            TrustError::SignatureInvalid => "approval_signature_invalid",
            TrustError::KeyMismatch => "approval_key_mismatch",
            TrustError::NotUserPresent => "approval_without_user_presence",
            TrustError::PresenceUnavailable => "user_presence_unavailable",
            TrustError::Expired => "approval_expired",
            TrustError::Revoked => "approval_revoked",
            TrustError::Replayed => "approval_replayed",
            TrustError::UnknownApproval => "approval_unknown",
            TrustError::BindingMismatch => "approval_binding_mismatch",
            TrustError::AuditUnavailable => "audit_unavailable",
            TrustError::LedgerUnavailable => "approval_ledger_unavailable",
            TrustError::LedgerInconsistent => "approval_ledger_inconsistent",
            TrustError::AssuranceRejected => "approval_assurance_rejected",
        }
    }
}

// ---- the interfaces ---------------------------------------------------------------------------------------------------------------

pub trait KeyStore {
    fn key_id(&self) -> String;
    fn protection(&self) -> KeyProtection;
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, TrustError>;
    fn verify(&self, message: &[u8], signature: &[u8]) -> bool;
    /// Signing that ITSELF demonstrates user presence: the operating system verifies the person (PIN, biometric) as part of the signing
    /// operation, and no signature exists unless that succeeded. The default says this key store cannot do that. A key store that
    /// can (Windows Hello) returns `Presence::Confirmed` only when the OS reported a successful user verification for THIS operation.
    fn sign_present(&self, _message: &[u8]) -> Result<(Vec<u8>, Presence), TrustError> {
        Err(TrustError::PresenceUnavailable)
    }
}

/// What the person is shown when asked to confirm (no paths, no secrets).
#[derive(Debug, Clone)]
pub struct PresencePrompt {
    pub action: Action,
    pub tool_id: String,
    pub version: String,
    pub platform_target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Confirmed,
    Declined,
    Unavailable,
}

pub trait PresenceGate {
    fn confirm(&self, prompt: &PresencePrompt) -> Presence;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerStatus {
    Issued,
    Consumed { txn_id: String },
    Revoked,
}

pub trait ApprovalLedger {
    fn next_counter(&self) -> Result<u64, TrustError>;
    fn record_issue(&self, approval_id: &str, counter: u64) -> Result<(), TrustError>;
    fn status(&self, approval_id: &str) -> Result<Option<LedgerStatus>, TrustError>;
    /// Atomically `Issued` -> `Consumed` for exactly one transaction; anything else is refused.
    fn consume(&self, approval_id: &str, txn_id: &str) -> Result<(), TrustError>;
    fn revoke(&self, approval_id: &str) -> Result<(), TrustError>;

    /// `true` only for a DURABLE ledger whose `*_audited` operations write the journal row in the same transaction as the state change.
    fn atomic_with_audit(&self) -> bool {
        false
    }

    /// Records the approval as issued AND writes `record` to `audit`. The default (non-durable doubles) writes the journal row first and
    /// records the approval only if that succeeded. A durable ledger overrides this with ONE transaction.
    fn record_issue_audited(&self, approval_id: &str, counter: u64, audit: &dyn AuditSink, record: AuditRecord) -> Result<(), TrustError> {
        audit.record(record)?;
        self.record_issue(approval_id, counter)
    }

    /// Consumes the approval for `txn_id` AND writes the success `record`. The default (non-durable doubles) checks the approval, writes the
    /// journal row, then consumes. A durable ledger overrides this with ONE transaction in which the success row exists only if the
    /// consumption itself succeeded.
    fn consume_audited(&self, approval_id: &str, txn_id: &str, audit: &dyn AuditSink, record: AuditRecord) -> Result<(), TrustError> {
        match self.status(approval_id)? {
            None => return Err(TrustError::UnknownApproval),
            Some(LedgerStatus::Revoked) => return Err(TrustError::Revoked),
            Some(LedgerStatus::Consumed { .. }) => return Err(TrustError::Replayed),
            Some(LedgerStatus::Issued) => {}
        }
        audit.record(record)?;
        self.consume(approval_id, txn_id)
    }
}

/// An audit event: closed vocabulary, ids only. Never a path, a credential or free text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub event: &'static str,
    pub outcome: &'static str,
    pub approval_id: Option<String>,
    pub tool_id: String,
    pub version: String,
    pub action: &'static str,
    pub code: Option<&'static str>,
}

pub trait AuditSink {
    fn record(&self, record: AuditRecord) -> Result<(), TrustError>;
}

// ---- the signed approval ----------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceApproval {
    /// The Phase 3A approval with its content binding (recipe, plan, tool, version, platform, scope, project, computer, expiry).
    pub approval: Approval,
    /// The exact artifact the approval is for.
    pub artifact_sha256: String,
    /// Digest of the effects (permissions) the recipe declares.
    pub effects_digest: String,
    pub key_id: String,
    pub counter: u64,
    pub presence_confirmed: bool,
    pub signature: String,
}

fn payload(d: &DeviceApproval) -> Vec<u8> {
    let v = serde_json::json!({
        "approval": d.approval,
        "artifactSha256": d.artifact_sha256,
        "effectsDigest": d.effects_digest,
        "keyId": d.key_id,
        "counter": d.counter,
        "presence": d.presence_confirmed,
    });
    canonical_json(&v).into_bytes()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect()
}

/// Where user presence comes from when an approval is issued.
#[derive(Clone, Copy)]
pub enum PresenceSource<'a> {
    /// A separate gate asked first (test doubles; never an OS proof by itself).
    Separate(&'a dyn PresenceGate),
    /// The signing operation proves it ([`KeyStore::sign_present`]): no prompt of ours, no flag, no button.
    SignedByOs,
}

#[allow(clippy::too_many_arguments)] // the collaborators are separate interfaces on purpose
/// Issues a device approval from an already-bound Phase 3A approval: the person must be present, the key must sign, the ledger must
/// record it, and the audit record must be written. Any failure issues nothing.
pub fn issue(
    approval: &Approval,
    artifact_sha256: &str,
    effects_digest: &str,
    keys: &dyn KeyStore,
    presence: &dyn PresenceGate,
    ledger: &dyn ApprovalLedger,
    audit: &dyn AuditSink,
    policy: TrustPolicy,
) -> Result<DeviceApproval, TrustError> {
    issue_with(approval, artifact_sha256, effects_digest, keys, PresenceSource::Separate(presence), ledger, audit, policy)
}

#[allow(clippy::too_many_arguments)] // the collaborators are separate interfaces on purpose
/// [`issue`] with an explicit presence source. With [`PresenceSource::SignedByOs`] presence is whatever the operating system's own
/// signing operation proved; the signature is also verified against the key before anything is recorded.
pub fn issue_with(
    approval: &Approval,
    artifact_sha256: &str,
    effects_digest: &str,
    keys: &dyn KeyStore,
    presence: PresenceSource,
    ledger: &dyn ApprovalLedger,
    audit: &dyn AuditSink,
    policy: TrustPolicy,
) -> Result<DeviceApproval, TrustError> {
    let rec = |outcome: &'static str, code: Option<&'static str>| AuditRecord {
        event: "approval_issue",
        outcome,
        approval_id: Some(approval.approval_id.clone()),
        tool_id: approval.tool_id.clone(),
        version: approval.version.clone(),
        action: approval.action.as_str(),
        code,
    };
    let fail = |e: TrustError| {
        let _ = audit.record(rec("refused", Some(e.code())));
        e
    };
    if !policy.accepts(keys.protection()) {
        return Err(fail(TrustError::KeyNotOsProtected));
    }
    if !policy.accepts_book(ledger) {
        return Err(fail(TrustError::LedgerUnavailable)); // this policy needs a durable, atomic ledger-and-journal
    }
    let prompt = PresencePrompt {
        action: approval.action,
        tool_id: approval.tool_id.clone(),
        version: approval.version.clone(),
        platform_target: approval.platform_target.clone(),
    };
    if let PresenceSource::Separate(gate) = presence {
        match gate.confirm(&prompt) {
            Presence::Confirmed => {}
            Presence::Declined => return Err(fail(TrustError::NotUserPresent)),
            Presence::Unavailable => return Err(fail(TrustError::PresenceUnavailable)),
        }
    }
    let counter = ledger.next_counter().map_err(fail)?;
    let mut d = DeviceApproval {
        approval: approval.clone(),
        artifact_sha256: artifact_sha256.to_string(),
        effects_digest: effects_digest.to_string(),
        key_id: keys.key_id(),
        counter,
        presence_confirmed: true,
        signature: String::new(),
    };
    let signature = match presence {
        PresenceSource::Separate(_) => keys.sign(&payload(&d)).map_err(fail)?,
        PresenceSource::SignedByOs => {
            // The claimed presence is part of the signed payload, but it counts only because a signature exists: the OS signs only
            // after it verified the person, and a decline or an unavailable verifier yields no signature at all.
            let (sig, p) = keys.sign_present(&payload(&d)).map_err(fail)?;
            match p {
                Presence::Confirmed => sig,
                Presence::Declined => return Err(fail(TrustError::NotUserPresent)),
                Presence::Unavailable => return Err(fail(TrustError::PresenceUnavailable)),
            }
        }
    };
    if !keys.verify(&payload(&d), &signature) {
        return Err(fail(TrustError::SignatureInvalid)); // our own check: never record an approval its own key does not verify
    }
    d.signature = hex(&signature);
    // approval row + its `issued` journal row: one durable transaction on a durable ledger; if it fails, nothing is issued
    ledger.record_issue_audited(&approval.approval_id, counter, audit, rec("issued", None)).map_err(fail)?;
    Ok(d)
}

/// What the operation actually is, as the adapter sees it; the approval has to match all of it.
#[derive(Debug, Clone)]
pub struct Expected<'a> {
    pub action: Action,
    pub tool_id: &'a str,
    pub version: &'a str,
    pub platform_target: &'a str,
    pub computer_id: &'a str,
    pub recipe_digest: &'a str,
    pub artifact_sha256: &'a str,
    pub effects_digest: &'a str,
    pub scope: &'a str,
    pub project_alias: Option<&'a str>,
}

/// Proof that ONE device approval was verified and consumed for ONE transaction. Only [`authorize`] makes one.
#[derive(Debug)]
pub struct DeviceAuthorization {
    approval_id: String,
    txn_id: String,
    key_protection: KeyProtection,
}

impl DeviceAuthorization {
    pub fn approval_id(&self) -> &str {
        &self.approval_id
    }

    pub fn txn_id(&self) -> &str {
        &self.txn_id
    }

    pub fn key_protection(&self) -> KeyProtection {
        self.key_protection
    }
}

#[allow(clippy::too_many_arguments)] // the collaborators are separate interfaces on purpose
/// Verifies the device approval against the operation and CONSUMES it for `txn_id`. The audit record is written first; if it cannot
/// be, nothing is consumed or authorized.
pub fn authorize(
    d: &DeviceApproval,
    want: &Expected,
    txn_id: &str,
    now: DateTime<Utc>,
    keys: &dyn KeyStore,
    ledger: &dyn ApprovalLedger,
    audit: &dyn AuditSink,
    policy: TrustPolicy,
) -> Result<DeviceAuthorization, TrustError> {
    let rec = |event: &'static str, outcome: &'static str, code: Option<&'static str>| AuditRecord {
        event,
        outcome,
        approval_id: Some(d.approval.approval_id.clone()),
        tool_id: want.tool_id.to_string(),
        version: want.version.to_string(),
        action: want.action.as_str(),
        code,
    };
    let refuse = |e: TrustError| {
        let _ = audit.record(rec("approval_use", "refused", Some(e.code())));
        e
    };
    if !policy.accepts(keys.protection()) {
        return Err(refuse(TrustError::KeyNotOsProtected));
    }
    if !policy.accepts_book(ledger) {
        return Err(refuse(TrustError::LedgerUnavailable)); // this policy needs a durable, atomic ledger-and-journal
    }
    // authenticity: the signature over every field, by the key this device holds
    if d.key_id != keys.key_id() {
        return Err(refuse(TrustError::KeyMismatch));
    }
    let sig = unhex(&d.signature).ok_or_else(|| refuse(TrustError::SignatureInvalid))?;
    if !keys.verify(&payload(d), &sig) {
        return Err(refuse(TrustError::SignatureInvalid));
    }
    if !d.presence_confirmed {
        return Err(refuse(TrustError::NotUserPresent));
    }
    // binding: the approval is for exactly this operation
    let a = &d.approval;
    let same = a.action == want.action
        && a.tool_id == want.tool_id
        && a.version == want.version
        && a.platform_target == want.platform_target
        && a.computer_id == want.computer_id
        && a.recipe_digest == want.recipe_digest
        && a.scope == want.scope
        && a.project_alias.as_deref() == want.project_alias
        && d.artifact_sha256 == want.artifact_sha256
        && d.effects_digest == want.effects_digest;
    if !same {
        return Err(refuse(TrustError::BindingMismatch));
    }
    let expires = DateTime::parse_from_rfc3339(&a.expires_at).map(|t| t.with_timezone(&Utc)).map_err(|_| refuse(TrustError::Expired))?;
    if now >= expires {
        return Err(refuse(TrustError::Expired));
    }
    // ledger state before the audit, so a refusal says why; the consumption itself is the last, atomic step
    match ledger.status(&a.approval_id).map_err(refuse)? {
        None => return Err(refuse(TrustError::UnknownApproval)),
        Some(LedgerStatus::Revoked) => return Err(refuse(TrustError::Revoked)),
        Some(LedgerStatus::Consumed { .. }) => return Err(refuse(TrustError::Replayed)),
        Some(LedgerStatus::Issued) => {}
    }
    // single-use consumption + its `authorized` journal row: the success row exists only if THIS call consumed the approval
    ledger.consume_audited(&a.approval_id, txn_id, audit, rec("approval_use", "authorized", None)).map_err(refuse)?;
    Ok(DeviceAuthorization { approval_id: a.approval_id.clone(), txn_id: txn_id.to_string(), key_protection: keys.protection() })
}

// ---- fakes (tests and the isolated test-mode adapter only) --------------------------------------------------------------------------

pub mod fakes {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};

    fn hmac_sha256(key: &[u8], msg: &[u8]) -> Vec<u8> {
        let mut k = [0u8; 64];
        if key.len() > 64 {
            k[..32].copy_from_slice(&Sha256::digest(key));
        } else {
            k[..key.len()].copy_from_slice(key);
        }
        let inner: Vec<u8> = k.iter().map(|b| b ^ 0x36).chain(msg.iter().copied()).collect();
        let inner_hash = Sha256::digest(&inner);
        let outer: Vec<u8> = k.iter().map(|b| b ^ 0x5c).chain(inner_hash.iter().copied()).collect();
        Sha256::digest(&outer).to_vec()
    }

    /// A keyed MAC held in memory. It declares itself `SoftwareFake`.
    pub struct FakeKeyStore {
        id: String,
        secret: Vec<u8>,
        pub available: bool,
    }

    impl FakeKeyStore {
        pub fn new(id: &str) -> FakeKeyStore {
            let secret = uuid::Uuid::new_v4().as_bytes().iter().chain(uuid::Uuid::new_v4().as_bytes().iter()).copied().collect();
            FakeKeyStore { id: id.to_string(), secret, available: true }
        }
    }

    impl KeyStore for FakeKeyStore {
        fn key_id(&self) -> String {
            self.id.clone()
        }
        fn protection(&self) -> KeyProtection {
            KeyProtection::SoftwareFake
        }
        fn sign(&self, message: &[u8]) -> Result<Vec<u8>, TrustError> {
            if !self.available {
                return Err(TrustError::KeyUnavailable);
            }
            Ok(hmac_sha256(&self.secret, message))
        }
        fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
            self.available && hmac_sha256(&self.secret, message) == signature
        }
    }

    pub struct FakePresence(pub Presence);

    impl PresenceGate for FakePresence {
        fn confirm(&self, _prompt: &PresencePrompt) -> Presence {
            self.0
        }
    }

    #[derive(Default)]
    pub struct InMemoryLedger {
        inner: Mutex<(u64, HashMap<String, LedgerStatus>)>,
        pub broken: bool,
    }

    impl ApprovalLedger for InMemoryLedger {
        fn next_counter(&self) -> Result<u64, TrustError> {
            if self.broken {
                return Err(TrustError::LedgerUnavailable);
            }
            let mut g = self.inner.lock().map_err(|_| TrustError::LedgerUnavailable)?;
            g.0 += 1;
            Ok(g.0)
        }
        fn record_issue(&self, approval_id: &str, _counter: u64) -> Result<(), TrustError> {
            let mut g = self.inner.lock().map_err(|_| TrustError::LedgerUnavailable)?;
            g.1.insert(approval_id.to_string(), LedgerStatus::Issued);
            Ok(())
        }
        fn record_issue_audited(
            &self,
            approval_id: &str,
            _counter: u64,
            audit: &dyn AuditSink,
            record: AuditRecord,
        ) -> Result<(), TrustError> {
            // under the ledger's own lock: the journal row first, the approval only if it was written
            let mut g = self.inner.lock().map_err(|_| TrustError::LedgerUnavailable)?;
            audit.record(record)?;
            g.1.insert(approval_id.to_string(), LedgerStatus::Issued);
            Ok(())
        }
        fn consume_audited(&self, approval_id: &str, txn_id: &str, audit: &dyn AuditSink, record: AuditRecord) -> Result<(), TrustError> {
            if self.broken {
                return Err(TrustError::LedgerUnavailable);
            }
            let mut g = self.inner.lock().map_err(|_| TrustError::LedgerUnavailable)?;
            match g.1.get(approval_id) {
                Some(LedgerStatus::Issued) => {}
                Some(LedgerStatus::Consumed { .. }) => return Err(TrustError::Replayed),
                Some(LedgerStatus::Revoked) => return Err(TrustError::Revoked),
                None => return Err(TrustError::UnknownApproval),
            }
            audit.record(record)?; // nothing is consumed if the journal cannot be written; no success row for a refused consumption
            g.1.insert(approval_id.to_string(), LedgerStatus::Consumed { txn_id: txn_id.to_string() });
            Ok(())
        }
        fn status(&self, approval_id: &str) -> Result<Option<LedgerStatus>, TrustError> {
            if self.broken {
                return Err(TrustError::LedgerUnavailable);
            }
            Ok(self.inner.lock().map_err(|_| TrustError::LedgerUnavailable)?.1.get(approval_id).cloned())
        }
        fn consume(&self, approval_id: &str, txn_id: &str) -> Result<(), TrustError> {
            let mut g = self.inner.lock().map_err(|_| TrustError::LedgerUnavailable)?;
            match g.1.get(approval_id) {
                Some(LedgerStatus::Issued) => {
                    g.1.insert(approval_id.to_string(), LedgerStatus::Consumed { txn_id: txn_id.to_string() });
                    Ok(())
                }
                Some(LedgerStatus::Consumed { .. }) => Err(TrustError::Replayed),
                Some(LedgerStatus::Revoked) => Err(TrustError::Revoked),
                None => Err(TrustError::UnknownApproval),
            }
        }
        fn revoke(&self, approval_id: &str) -> Result<(), TrustError> {
            let mut g = self.inner.lock().map_err(|_| TrustError::LedgerUnavailable)?;
            match g.1.get(approval_id) {
                Some(LedgerStatus::Consumed { .. }) => Ok(()), // too late to matter; the transaction already has it
                Some(_) => {
                    g.1.insert(approval_id.to_string(), LedgerStatus::Revoked);
                    Ok(())
                }
                None => Err(TrustError::UnknownApproval),
            }
        }
    }

    #[derive(Default)]
    pub struct VecAudit {
        pub events: Mutex<Vec<AuditRecord>>,
        pub broken: bool,
    }

    impl AuditSink for VecAudit {
        fn record(&self, record: AuditRecord) -> Result<(), TrustError> {
            if self.broken {
                return Err(TrustError::AuditUnavailable);
            }
            self.events.lock().map_err(|_| TrustError::AuditUnavailable)?.push(record);
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "device_trust_tests.rs"]
mod tests;
