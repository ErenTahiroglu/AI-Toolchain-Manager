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
//! * **the recorded key is the key used**: the public key kept in the store must equal the one the backend reports at every use.
//!
//! The real Windows backend lives in `os_hello.rs`; it is compiled on Windows and has NEVER been run to create or use a key.
#![allow(dead_code)] // no command reaches this module in Phase 3D-1

use crate::{
    approval_store::{ApprovalStore, EnrollmentPermit, KeyRecord, StoreError},
    device_trust::{KeyProtection, KeyStore, Presence, TrustError},
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
    pub assurance: Assurance,
    /// SHA-256 of the attestation material the OS returned, if any (kept as evidence; not a verification).
    pub attestation_digest: Option<String>,
}

pub fn key_id_of(public_key: &[u8]) -> String {
    Sha256::digest(public_key).iter().map(|b| format!("{b:02x}")).collect()
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
    fn existing_key(&self, name: &str) -> Result<Option<KeyInfo>, BackendError>;
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
                assurance: info.assurance.as_str().to_string(),
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

/// The approval key store for `device_trust`, built on a recorded key. It signs ONLY with user presence.
pub struct OsKeyStore<'a> {
    backend: &'a dyn OsCredentialBackend,
    verifier: &'a dyn PublicKeyVerifier,
    key_name: String,
    key_id: String,
    public_key: Vec<u8>,
    assurance: Assurance,
}

impl<'a> OsKeyStore<'a> {
    /// Opens the recorded key. Refuses when approval security is not enabled, no key is recorded, the backend is unavailable, or the
    /// backend's key is not the recorded one (key replaced).
    pub fn open(
        store: &ApprovalStore,
        backend: &'a dyn OsCredentialBackend,
        verifier: &'a dyn PublicKeyVerifier,
    ) -> Result<OsKeyStore<'a>, TrustError> {
        let s = store.security().map_err(|_| TrustError::KeyUnavailable)?;
        let rec = match (s.enabled, s.key) {
            (true, Some(k)) => k,
            _ => return Err(TrustError::KeyUnavailable),
        };
        if !backend.available() {
            return Err(TrustError::KeyUnavailable);
        }
        let public_key = unhex(&rec.public_key_hex).ok_or(TrustError::KeyUnavailable)?;
        let assurance = Assurance::parse(&rec.assurance).ok_or(TrustError::KeyUnavailable)?;
        if key_id_of(&public_key) != rec.key_id {
            return Err(TrustError::KeyMismatch);
        }
        match backend.existing_key(&rec.key_name) {
            Ok(Some(info)) if info.public_key == public_key => {}
            Ok(Some(_)) => return Err(TrustError::KeyMismatch),
            _ => return Err(TrustError::KeyUnavailable),
        }
        Ok(OsKeyStore { backend, verifier, key_name: rec.key_name, key_id: rec.key_id, public_key, assurance })
    }
}

impl KeyStore for OsKeyStore<'_> {
    fn key_id(&self) -> String {
        self.key_id.clone()
    }

    fn protection(&self) -> KeyProtection {
        self.assurance.protection()
    }

    /// There is no signing without the person: this key never signs silently.
    fn sign(&self, _message: &[u8]) -> Result<Vec<u8>, TrustError> {
        Err(TrustError::PresenceUnavailable)
    }

    fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        self.verifier.verify(&self.public_key, message, signature)
    }

    fn sign_present(&self, message: &[u8]) -> Result<(Vec<u8>, Presence), TrustError> {
        match self.backend.sign_with_presence(&self.key_name, message) {
            Ok(PresenceSignature { signature, user_verification: UserVerification::Verified }) => Ok((signature, Presence::Confirmed)),
            Ok(PresenceSignature { user_verification: UserVerification::Declined, .. }) | Err(BackendError::Declined) => {
                Ok((Vec::new(), Presence::Declined))
            }
            Ok(_) => Ok((Vec::new(), Presence::Unavailable)),
            Err(BackendError::KeyMissing) | Err(BackendError::Unavailable) | Err(BackendError::Unsupported) => {
                Err(TrustError::KeyUnavailable)
            }
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
            }
        }

        pub fn set_verification(&self, v: UserVerification) {
            *self.verification.lock().unwrap() = v;
        }

        /// A key created behind the store's back (to test that it is never adopted).
        pub fn plant_unrecorded_key(&self, name: &str) {
            let secret = uuid::Uuid::new_v4().as_bytes().to_vec();
            let public = format!("fake-public-{name}").into_bytes();
            let info = KeyInfo { key_id: key_id_of(&public), public_key: public, assurance: self.claims, attestation_digest: None };
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
            Ok(self.keys.lock().unwrap().get(name).map(|(_, i)| i.clone()))
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
            let info = KeyInfo {
                key_id: key_id_of(&public),
                public_key: public,
                assurance: self.claims,
                attestation_digest: matches!(self.claims, Assurance::OsReportedHardware).then(|| "a".repeat(64)),
            };
            g.insert(name.to_string(), (secret, info.clone()));
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
