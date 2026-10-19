//! Phase 3D-1: the operating-system approval key, behind an interface, with fakes. NO real backend is reachable from the application.
//!
//! What a genuine approval needs (see docs/phase-3d1/WINDOWS-HELLO-APPROVAL.md):
//! * a **device-bound key** created by the OS credential service (Windows Hello `KeyCredentialManager`), never exportable;
//! * **user presence proven by the OS operation itself**: [`OsKeyStore::sign_present`] returns `Presence::Confirmed` only when the
//!   backend reports that the OS verified the person (PIN/biometric) during THAT signing operation; there is no unsigned path
//!   ([`OsKeyStore::sign`] always refuses), no button, no flag;
//! * **hardware claims verified, not assumed**: the key is `KeyProtection::OsProtected` only when this program verified the attestation
//!   (nothing implements that yet). What the OS merely *reports* is `OsReportedHardware`, software-protected is `OsSoftware`, and
//!   [`crate::device_trust::TrustPolicy::production`] accepts none of the three until a verified attestation exists;
//! * **no silent key creation**: [`provision`] needs an [`EnrollmentPermit`], which exists only after the person explicitly enabled
//!   approval security for this computer (`approval_store`), and a key that exists but is not recorded is never adopted;
//! * **the recorded key is the key used**: the public key kept in the store must equal the one the backend reports at every use;
//! * **authority never comes from the database** (RT-3D1-01): the assurance label in `approval-ledger.sqlite` is a mutable, unauthenticated
//!   note. The effective assurance is re-established from the LIVE credential by [`established_assurance`] on open and before every
//!   signature; a recorded label stronger than what is established now is REJECTED (and audited), a weaker one only lowers authority,
//!   attestation evidence must be bound to the exact key and unchanged, and `HardwareVerified` can only come from an
//!   [`AttestationVerifier`], which no real backend has. No database edit can therefore raise a key's authority.
//!
//! The real Windows backend lives in `os_hello.rs`; it is compiled on Windows and has NEVER been run to create or use a key.
#![allow(dead_code)] // no command reaches this module in Phase 3D-1

use crate::{
    approval_store::{ApprovalStore, EnrollmentPermit, KeyRecord, StoreError},
    device_trust::{AuditRecord, AuditSink, KeyProtection, KeyStore, Presence, TrustError},
};
use sha2::{Digest, Sha256};

/// How well the key's protection is established. Ordered from weakest claim to strongest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assurance {
    /// A test double.
    Fake,
    /// The OS key credential is software-protected (no attestation).
    OsSoftware,
    /// The OS reports TPM attestation for the credential; this program has not validated it.
    OsReportedHardware,
    /// This program validated the attestation chain. Nothing produces this yet.
    HardwareVerified,
}

impl Assurance {
    pub fn as_str(self) -> &'static str {
        match self {
            Assurance::Fake => "fake",
            Assurance::OsSoftware => "os_software",
            Assurance::OsReportedHardware => "os_reported_hardware",
            Assurance::HardwareVerified => "hardware_verified",
        }
    }

    pub fn parse(s: &str) -> Option<Assurance> {
        [Assurance::Fake, Assurance::OsSoftware, Assurance::OsReportedHardware, Assurance::HardwareVerified]
            .into_iter()
            .find(|a| a.as_str() == s)
    }

    /// Strength for comparisons only (a fake is below everything real).
    pub fn rank(self) -> u8 {
        match self {
            Assurance::Fake => 0,
            Assurance::OsSoftware => 1,
            Assurance::OsReportedHardware => 2,
            Assurance::HardwareVerified => 3,
        }
    }

    pub fn protection(self) -> KeyProtection {
        match self {
            Assurance::Fake => KeyProtection::SoftwareFake,
            Assurance::OsSoftware => KeyProtection::OsSoftware,
            Assurance::OsReportedHardware => KeyProtection::OsReportedHardware,
            Assurance::HardwareVerified => KeyProtection::OsProtected,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    /// SHA-256 of the public key, hex.
    pub key_id: String,
    pub public_key: Vec<u8>,
    /// What the backend CLAIMS. It is never used as it stands: [`established_assurance`] decides what is established.
    pub assurance: Assurance,
    /// SHA-256 of the attestation material (certificate chain) the OS returned, if any.
    pub attestation_material_digest: Option<String>,
    /// [`bind_attestation`] of the key id and the material digest: evidence tied to exactly this public key.
    pub attestation_digest: Option<String>,
}

pub fn key_id_of(public_key: &[u8]) -> String {
    Sha256::digest(public_key).iter().map(|b| format!("{b:02x}")).collect()
}

/// Binds attestation material to ONE public key: the same material presented for another key does not reproduce this digest.
pub fn bind_attestation(key_id: &str, material_digest: &str) -> String {
    Sha256::digest(format!("atm-attestation-v1|{key_id}|{material_digest}").as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Validates attestation evidence (the TPM attestation chain) for a public key. NO implementation exists in this repository, and the real
/// Windows backend has none: so nothing real can ever be `HardwareVerified`. (Only a test double implements it.)
pub trait AttestationVerifier {
    fn verify(&self, public_key: &[u8], attestation_material_digest: &str) -> bool;
}

/// What is ESTABLISHED about a credential now, from the backend's current answer and nothing else (never from the database):
/// no evidence, evidence not bound to this exact key, or an unverifiable claim all mean "unverified", i.e. software. A claim of
/// verified hardware needs an [`AttestationVerifier`] that approves; without one the most that can be established is "reported".
pub fn established_assurance(backend: &dyn OsCredentialBackend, info: &KeyInfo) -> Assurance {
    let evidence_ok = info.key_id == key_id_of(&info.public_key)
        && match (&info.attestation_material_digest, &info.attestation_digest) {
            (Some(material), Some(bound)) => bind_attestation(&info.key_id, material) == *bound,
            _ => false,
        };
    match info.assurance {
        Assurance::Fake => Assurance::Fake,
        Assurance::OsSoftware => Assurance::OsSoftware,
        Assurance::OsReportedHardware if evidence_ok => Assurance::OsReportedHardware,
        Assurance::OsReportedHardware => Assurance::OsSoftware,
        Assurance::HardwareVerified if !evidence_ok => Assurance::OsSoftware,
        Assurance::HardwareVerified => {
            let material = info.attestation_material_digest.as_deref().unwrap_or("");
            if backend.attestation_verifier().map(|v| v.verify(&info.public_key, material)).unwrap_or(false) {
                Assurance::HardwareVerified
            } else {
                Assurance::OsReportedHardware
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendError {
    Unsupported,
    Unavailable,
    KeyExists,
    KeyMissing,
    Declined,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserVerification {
    /// The OS reported a successful user verification (PIN/biometric) for this operation.
    Verified,
    /// The person declined or cancelled.
    Declined,
    /// No verifier, or an error: nothing was proven.
    Unavailable,
}

pub struct PresenceSignature {
    pub signature: Vec<u8>,
    pub user_verification: UserVerification,
}

/// The OS credential service. `existing_key` and `availability` never prompt; `create_key` and `sign_with_presence` do (that is the point).
pub trait OsCredentialBackend {
    fn available(&self) -> bool;
    /// The credential's identity (cheap, never prompts, never contacts anything; carries NO attestation evidence).
    fn existing_key(&self, name: &str) -> Result<Option<KeyInfo>, BackendError>;
    /// Like `existing_key` but also (re)obtains the attestation evidence the OS offers; may be slow and may involve the OS vendor's
    /// services. The default has no extra evidence.
    fn attest(&self, name: &str) -> Result<Option<KeyInfo>, BackendError> {
        self.existing_key(name)
    }
    /// The verifier that may upgrade a claim to `HardwareVerified`. No real backend has one.
    fn attestation_verifier(&self) -> Option<&dyn AttestationVerifier> {
        None
    }
    fn create_key(&self, name: &str, permit: &EnrollmentPermit) -> Result<KeyInfo, BackendError>;
    fn sign_with_presence(&self, name: &str, message: &[u8]) -> Result<PresenceSignature, BackendError>;
    /// Deletes the key under `name` (reversibility of enrollment). `Ok(())` also when it did not exist.
    fn delete_key(&self, name: &str) -> Result<(), BackendError>;
}

/// Verifies a signature with a PUBLIC key alone (no prompt, no key store).
pub trait PublicKeyVerifier {
    fn verify(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionError {
    Store(StoreError),
    Backend(BackendError),
    /// The backend already holds a key under this name that the store does not know. It is never adopted silently.
    UnrecordedKeyExists,
    BackendUnavailable,
}

/// Creates the approval key, once, after the person enabled approval security. Order: permit (explicit enable) -> backend available ->
/// no unrecorded key -> create (the OS prompts) -> record. A failure after creation leaves an UNRECORDED key, which the next call
/// reports and refuses to adopt.
pub fn provision(
    store: &ApprovalStore,
    backend: &dyn OsCredentialBackend,
    computer_id: &str,
    key_name: &str,
) -> Result<KeyInfo, ProvisionError> {
    let permit = store.enrollment_permit(computer_id).map_err(ProvisionError::Store)?;
    if !backend.available() {
        return Err(ProvisionError::BackendUnavailable);
    }
    if backend.existing_key(key_name).map_err(ProvisionError::Backend)?.is_some() {
        return Err(ProvisionError::UnrecordedKeyExists);
    }
    let info = backend.create_key(key_name, &permit).map_err(ProvisionError::Backend)?;
    store
        .record_key(
            &permit,
            &KeyRecord {
                key_name: key_name.to_string(),
                key_id: info.key_id.clone(),
                public_key_hex: info.public_key.iter().map(|b| format!("{b:02x}")).collect(),
                // the label recorded is what is ESTABLISHED now, never the backend's bare claim
                assurance: established_assurance(backend, &info).as_str().to_string(),
                attestation_digest: info.attestation_digest.clone(),
            },
        )
        .map_err(ProvisionError::Store)?;
    Ok(info)
}

/// Switches approval security off again: needs the exact phrase, deletes the OS key FIRST (if that fails nothing is cleared and the
/// record still describes the key that exists), then clears the record. Ledger and audit history are kept.
pub fn deprovision(
    store: &ApprovalStore,
    backend: &dyn OsCredentialBackend,
    computer_id: &str,
    typed_phrase: &str,
) -> Result<(), ProvisionError> {
    use crate::approval_store::disable_phrase;
    let s = store.security().map_err(ProvisionError::Store)?;
    if typed_phrase != disable_phrase(computer_id) {
        // the store audits and refuses a wrong phrase; nothing in the OS is touched
        return store.disable_approval_security(computer_id, typed_phrase).map_err(ProvisionError::Store);
    }
    if !s.enabled || s.computer_id.as_deref() != Some(computer_id) {
        return store.disable_approval_security(computer_id, typed_phrase).map_err(ProvisionError::Store);
    }
    if let Some(k) = &s.key {
        backend.delete_key(&k.key_name).map_err(ProvisionError::Backend)?;
    }
    store.disable_approval_security(computer_id, typed_phrase).map_err(ProvisionError::Store)
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect()
}

/// The approval key store for `device_trust`, built on a recorded key. It signs ONLY with user presence, and its effective assurance
/// is RE-ESTABLISHED from the live credential: see the module documentation (RT-3D1-01).
pub struct OsKeyStore<'a> {
    store: &'a ApprovalStore,
    backend: &'a dyn OsCredentialBackend,
    verifier: &'a dyn PublicKeyVerifier,
    computer_id: String,
    key_name: String,
    key_id: String,
    public_key: Vec<u8>,
    /// What was established at the last full check (never read from the database).
    established: std::cell::Cell<Assurance>,
}

impl<'a> OsKeyStore<'a> {
    /// Opens the recorded key for `computer_id`. Refuses (and audits a refusal for the assurance cases) when approval security is not
    /// enabled for this computer, no key is recorded, the backend is unavailable, the backend's key is not the recorded one, the
    /// recorded assurance is STRONGER than what the live credential establishes, or the attestation evidence changed.
    pub fn open(
        store: &'a ApprovalStore,
        backend: &'a dyn OsCredentialBackend,
        verifier: &'a dyn PublicKeyVerifier,
        computer_id: &str,
    ) -> Result<OsKeyStore<'a>, TrustError> {
        let s = store.security().map_err(|_| TrustError::KeyUnavailable)?;
        let rec = match (s.enabled, s.computer_id.as_deref() == Some(computer_id), s.key) {
            (true, true, Some(k)) => k,
            _ => return Err(TrustError::KeyUnavailable),
        };
        if !backend.available() {
            return Err(TrustError::KeyUnavailable);
        }
        let public_key = unhex(&rec.public_key_hex).ok_or(TrustError::KeyUnavailable)?;
        if key_id_of(&public_key) != rec.key_id {
            return Err(TrustError::KeyMismatch);
        }
        let k = OsKeyStore {
            store,
            backend,
            verifier,
            computer_id: computer_id.to_string(),
            key_name: rec.key_name,
            key_id: rec.key_id,
            public_key,
            established: std::cell::Cell::new(Assurance::Fake),
        };
        k.evaluate(true)?;
        Ok(k)
    }

    fn refuse_assurance(&self, why: &'static str) -> TrustError {
        // audited like every refusal: a database edit that tried to raise authority leaves a trace
        let _ = self.store.record(AuditRecord {
            event: "approval_key_open",
            outcome: "refused",
            approval_id: None,
            tool_id: "-".to_string(),
            version: "-".to_string(),
            action: "approval-security",
            code: Some(why),
        });
        TrustError::AssuranceRejected
    }

    /// The effective assurance of this key NOW. `full` re-obtains the attestation evidence from the OS; the cheap form re-reads the
    /// database and the credential's identity and keeps the last established assurance. The result is never stronger than either the
    /// recorded label or the established assurance.
    fn evaluate(&self, full: bool) -> Result<Assurance, TrustError> {
        // 1. the setting exactly as it is in the (mutable) file right now
        let s = self.store.security().map_err(|_| TrustError::KeyUnavailable)?;
        let rec = match (s.enabled, s.computer_id.as_deref() == Some(self.computer_id.as_str()), s.key) {
            (true, true, Some(k)) => k,
            _ => return Err(TrustError::KeyUnavailable),
        };
        if rec.key_id != self.key_id
            || rec.key_name != self.key_name
            || unhex(&rec.public_key_hex).as_deref() != Some(self.public_key.as_slice())
        {
            return Err(TrustError::KeyMismatch);
        }
        let recorded = Assurance::parse(&rec.assurance).ok_or(TrustError::KeyUnavailable)?;
        // 2. the credential exactly as the operating system presents it right now
        let live = if full { self.backend.attest(&self.key_name) } else { self.backend.existing_key(&self.key_name) };
        let info = match live {
            Ok(Some(i)) => i,
            _ => return Err(TrustError::KeyUnavailable),
        };
        if info.public_key != self.public_key || info.key_id != self.key_id {
            return Err(TrustError::KeyMismatch);
        }
        let established = if full {
            let e = established_assurance(self.backend, &info);
            self.established.set(e);
            e
        } else {
            self.established.get()
        };
        // 3. a label stronger than what is established is a forgery or a downgrade of the live credential: refuse, never upgrade
        if recorded.rank() > established.rank() {
            return Err(self.refuse_assurance("approval_assurance_exceeds_established"));
        }
        // 4. attestation evidence must be the evidence that was recorded for this key
        if full {
            if let Some(recorded_digest) = &rec.attestation_digest {
                if info.attestation_digest.as_deref() != Some(recorded_digest.as_str()) {
                    return Err(self.refuse_assurance("approval_attestation_evidence_changed"));
                }
            }
        }
        Ok(recorded)
    }
}

impl KeyStore for OsKeyStore<'_> {
    fn key_id(&self) -> String {
        self.key_id.clone()
    }

    /// Re-evaluated on every call (database and credential identity re-read); any problem yields the weakest, always-refused protection.
    fn protection(&self) -> KeyProtection {
        match self.evaluate(false) {
            Ok(a) => a.protection(),
            Err(_) => KeyProtection::OsSoftware,
        }
    }

    /// There is no signing without the person: this key never signs silently.
    fn sign(&self, _message: &[u8]) -> Result<Vec<u8>, TrustError> {
        Err(TrustError::PresenceUnavailable)
    }

    fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        self.verifier.verify(&self.public_key, message, signature)
    }

    fn sign_present(&self, message: &[u8]) -> Result<(Vec<u8>, Presence), TrustError> {
        // immediately before the person is asked: the live credential and its evidence are checked in full
        self.evaluate(true)?;
        match self.backend.sign_with_presence(&self.key_name, message) {
            Ok(PresenceSignature { signature, user_verification: UserVerification::Verified }) => Ok((signature, Presence::Confirmed)),
            Ok(PresenceSignature { user_verification: UserVerification::Declined, .. }) | Err(BackendError::Declined) => {
                Ok((Vec::new(), Presence::Declined))
            }
            Ok(_) => Ok((Vec::new(), Presence::Unavailable)),
            Err(_) => Err(TrustError::KeyUnavailable),
        }
    }
}

// ---- fakes (tests and the synthetic rehearsal only) -------------------------------------------------------------------------------------

pub mod fakes {
    use super::*;
    use std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex,
        },
    };

    fn hmac_sha256(key: &[u8], msg: &[u8]) -> Vec<u8> {
        let mut k = [0u8; 64];
        k[..key.len().min(64)].copy_from_slice(&key[..key.len().min(64)]);
        let inner: Vec<u8> = k.iter().map(|b| b ^ 0x36).chain(msg.iter().copied()).collect();
        let ih = Sha256::digest(&inner);
        let outer: Vec<u8> = k.iter().map(|b| b ^ 0x5c).chain(ih.iter().copied()).collect();
        Sha256::digest(&outer).to_vec()
    }

    /// A fake OS credential service. It can CLAIM any assurance so the policy can be tested, and counts every key creation and prompt.
    pub struct FakeOsBackend {
        pub claims: Assurance,
        pub is_available: bool,
        /// What the fake "person" does at the next prompt.
        pub verification: Mutex<UserVerification>,
        keys: Mutex<HashMap<String, (Vec<u8>, KeyInfo)>>,
        pub creations: AtomicUsize,
        pub prompts: AtomicUsize,
        pub deletions: AtomicUsize,
        pub delete_fails: std::sync::atomic::AtomicBool,
        /// How the LIVE attestation evidence presents itself at the next full check.
        pub evidence: Mutex<EvidenceMode>,
        /// Whether this double offers an attestation verifier (the only way a claim can become `HardwareVerified`).
        pub has_verifier: std::sync::atomic::AtomicBool,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum EvidenceMode {
        Normal,
        /// The OS offers no evidence now.
        Missing,
        /// Evidence bound to a DIFFERENT key (swapped between credentials).
        OtherKey,
        /// Well-formed evidence for this key, but not the evidence that was recorded.
        Changed,
    }

    impl FakeOsBackend {
        pub fn new(claims: Assurance) -> FakeOsBackend {
            FakeOsBackend {
                claims,
                is_available: true,
                verification: Mutex::new(UserVerification::Verified),
                keys: Mutex::new(HashMap::new()),
                creations: AtomicUsize::new(0),
                prompts: AtomicUsize::new(0),
                deletions: AtomicUsize::new(0),
                delete_fails: std::sync::atomic::AtomicBool::new(false),
                evidence: Mutex::new(EvidenceMode::Normal),
                has_verifier: std::sync::atomic::AtomicBool::new(false),
            }
        }

        pub fn set_verification(&self, v: UserVerification) {
            *self.verification.lock().unwrap() = v;
        }

        /// A key created behind the store's back (to test that it is never adopted).
        pub fn plant_unrecorded_key(&self, name: &str) {
            let secret = uuid::Uuid::new_v4().as_bytes().to_vec();
            let public = format!("fake-public-{name}").into_bytes();
            let info = KeyInfo {
                key_id: key_id_of(&public),
                public_key: public,
                assurance: self.claims,
                attestation_material_digest: None,
                attestation_digest: None,
            };
            self.keys.lock().unwrap().insert(name.to_string(), (secret, info));
        }

        /// Replaces the key under `name` by a different one (key swap).
        pub fn swap_key(&self, name: &str) {
            self.plant_unrecorded_key(&format!("{name}-other"));
            let mut g = self.keys.lock().unwrap();
            let other = g.remove(&format!("{name}-other")).unwrap();
            g.insert(name.to_string(), other);
        }
    }

    impl OsCredentialBackend for FakeOsBackend {
        fn available(&self) -> bool {
            self.is_available
        }

        fn existing_key(&self, name: &str) -> Result<Option<KeyInfo>, BackendError> {
            // cheap identity only: no evidence
            Ok(self.keys.lock().unwrap().get(name).map(|(_, i)| KeyInfo {
                attestation_material_digest: None,
                attestation_digest: None,
                ..i.clone()
            }))
        }

        fn attest(&self, name: &str) -> Result<Option<KeyInfo>, BackendError> {
            let g = self.keys.lock().unwrap();
            Ok(g.get(name).map(|(_, i)| self.with_evidence(name, i)))
        }

        fn attestation_verifier(&self) -> Option<&dyn AttestationVerifier> {
            if self.has_verifier.load(Ordering::SeqCst) {
                Some(self)
            } else {
                None
            }
        }

        fn create_key(&self, name: &str, _permit: &EnrollmentPermit) -> Result<KeyInfo, BackendError> {
            let mut g = self.keys.lock().unwrap();
            if g.contains_key(name) {
                return Err(BackendError::KeyExists);
            }
            self.prompts.fetch_add(1, Ordering::SeqCst);
            if *self.verification.lock().unwrap() != UserVerification::Verified {
                return Err(BackendError::Declined);
            }
            self.creations.fetch_add(1, Ordering::SeqCst);
            let secret = uuid::Uuid::new_v4().as_bytes().to_vec();
            let public = format!("fake-public-{name}").into_bytes();
            let base = KeyInfo {
                key_id: key_id_of(&public),
                public_key: public,
                assurance: self.claims,
                attestation_material_digest: None,
                attestation_digest: None,
            };
            let info = self.with_evidence(name, &base);
            g.insert(name.to_string(), (secret, base));
            Ok(info)
        }

        fn sign_with_presence(&self, name: &str, message: &[u8]) -> Result<PresenceSignature, BackendError> {
            let g = self.keys.lock().unwrap();
            let (secret, _) = g.get(name).ok_or(BackendError::KeyMissing)?;
            self.prompts.fetch_add(1, Ordering::SeqCst);
            let v = *self.verification.lock().unwrap();
            // a decline or an unavailable verifier yields NO signature, exactly like the real OS operation
            let signature = if v == UserVerification::Verified { hmac_sha256(secret, message) } else { Vec::new() };
            Ok(PresenceSignature { signature, user_verification: v })
        }

        fn delete_key(&self, name: &str) -> Result<(), BackendError> {
            if self.delete_fails.load(Ordering::SeqCst) {
                return Err(BackendError::Failed);
            }
            self.deletions.fetch_add(1, Ordering::SeqCst);
            self.keys.lock().unwrap().remove(name);
            Ok(())
        }
    }

    impl FakeOsBackend {
        pub fn has_key(&self, name: &str) -> bool {
            self.keys.lock().unwrap().contains_key(name)
        }

        pub fn set_evidence(&self, mode: EvidenceMode) {
            *self.evidence.lock().unwrap() = mode;
        }

        pub fn offer_verifier(&self) {
            self.has_verifier.store(true, Ordering::SeqCst);
        }

        /// The key's info as the live credential service presents it now (evidence only for the stronger claims).
        fn with_evidence(&self, name: &str, base: &KeyInfo) -> KeyInfo {
            let mode = *self.evidence.lock().unwrap();
            let wants_evidence = matches!(base.assurance, Assurance::OsReportedHardware | Assurance::HardwareVerified);
            if !wants_evidence || mode == EvidenceMode::Missing {
                return KeyInfo { attestation_material_digest: None, attestation_digest: None, ..base.clone() };
            }
            let chain = if mode == EvidenceMode::Changed { format!("fake-chain-changed|{name}") } else { format!("fake-chain|{name}") };
            let material: String = Sha256::digest(chain.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
            let bound_to = if mode == EvidenceMode::OtherKey { key_id_of(b"another key") } else { base.key_id.clone() };
            KeyInfo {
                attestation_material_digest: Some(material.clone()),
                attestation_digest: Some(bind_attestation(&bound_to, &material)),
                ..base.clone()
            }
        }
    }

    impl AttestationVerifier for FakeOsBackend {
        fn verify(&self, _public_key: &[u8], _material: &str) -> bool {
            self.has_verifier.load(Ordering::SeqCst)
        }
    }

    impl PublicKeyVerifier for FakeOsBackend {
        fn verify(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> bool {
            self.keys
                .lock()
                .unwrap()
                .values()
                .any(|(secret, info)| info.public_key == public_key && hmac_sha256(secret, message) == signature)
        }
    }
}

#[cfg(test)]
#[path = "os_approval_tests.rs"]
mod tests;
