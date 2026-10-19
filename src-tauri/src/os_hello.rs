//! Windows Hello availability probe (Phase 3D-1). READ-ONLY.
//!
//! It asks the operating system two questions and nothing else:
//! * `KeyCredentialManager::IsSupportedAsync` — can this user/device create Windows Hello key credentials at all?
//! * `UserConsentVerifier::CheckAvailabilityAsync` — is a user-presence verifier (PIN/biometric) configured and available?
//!
//! It does NOT create, open, delete or sign with any key, shows NO PIN/biometric prompt, enrolls nothing and changes no credential or
//! setting. It also reports whether the process runs without package identity (an ordinary unsigned desktop program), which is the
//! situation of this application. Result values are closed codes; no path, account name or credential detail is returned.
use serde_json::{json, Value};

/// The probe result for `--self-check` (`windowsHelloProbe`).
pub fn availability_probe() -> Value {
    #[cfg(windows)]
    {
        windows_impl::probe()
    }
    #[cfg(not(windows))]
    {
        json!({ "state": "not_applicable_platform", "why": "Windows Hello exists on Windows only; macOS would use the Secure Enclave through a signed application (not implemented)" })
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use windows::Security::Credentials::{
        KeyCredentialManager,
        UI::{UserConsentVerifier, UserConsentVerifierAvailability},
    };

    fn availability_name(a: UserConsentVerifierAvailability) -> &'static str {
        match a {
            UserConsentVerifierAvailability::Available => "available",
            UserConsentVerifierAvailability::DeviceNotPresent => "device_not_present",
            UserConsentVerifierAvailability::NotConfiguredForUser => "not_configured_for_user",
            UserConsentVerifierAvailability::DisabledByPolicy => "disabled_by_policy",
            UserConsentVerifierAvailability::DeviceBusy => "device_busy",
            _ => "other",
        }
    }

    /// `true` when the process has NO package identity (APPMODEL_ERROR_NO_PACKAGE = 15700): an ordinary desktop program.
    fn unpackaged() -> Option<bool> {
        use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
        let mut len: u32 = 0;
        // SAFETY: a null buffer with length 0 only asks for the required length; nothing is written.
        let rc = unsafe { GetCurrentPackageFullName(&mut len, std::ptr::null_mut()) };
        match rc {
            15700 => Some(true),
            0 | 122 => Some(false),
            _ => None,
        }
    }

    pub fn probe() -> Value {
        // WinRT calls need an initialised apartment: use a short-lived thread of our own so no other thread's apartment is touched.
        let handle = std::thread::spawn(|| {
            crate::cng_verify::init_apartment();
            let key_supported = KeyCredentialManager::IsSupportedAsync().and_then(|op| op.join());
            let consent = UserConsentVerifier::CheckAvailabilityAsync().and_then(|op| op.join());
            (key_supported, consent)
        });
        let (key_supported, consent) = match handle.join() {
            Ok(v) => v,
            Err(_) => return json!({ "state": "probe_thread_panicked" }),
        };
        let key_json = match &key_supported {
            Ok(b) => json!({ "result": "ok", "supported": b }),
            Err(e) => json!({ "result": "error", "hresult": format!("{:#010x}", e.code().0 as u32) }),
        };
        let consent_json = match &consent {
            Ok(a) => json!({ "result": "ok", "availability": availability_name(*a) }),
            Err(e) => json!({ "result": "error", "hresult": format!("{:#010x}", e.code().0 as u32) }),
        };
        let usable = matches!(key_supported, Ok(true)) && matches!(consent, Ok(UserConsentVerifierAvailability::Available));
        json!({
            "state": if usable { "api_reachable_and_hello_available" } else if key_supported.is_ok() && consent.is_ok() { "api_reachable_hello_not_available" } else { "api_error" },
            "keyCredentialManagerIsSupported": key_json,
            "userConsentVerifierAvailability": consent_json,
            "processHasNoPackageIdentity": unpackaged(),
            "readOnly": true,
            "keyCreated": false,
            "promptShown": false,
        })
    }
}

/// The real Windows Hello backend. COMPILED, GATED, NEVER RUN against a real credential: `create_key` and `sign_with_presence` can show
/// the Windows Hello PIN/biometric prompt and create a credential, which needs the user's separate explicit authorisation and is
/// not reachable from any command. Reaching it requires an `EnrollmentPermit` (explicitly enabled approval security) for creation and a
/// recorded key for signing.
#[cfg(windows)]
pub mod backend {
    use crate::{
        approval_store::EnrollmentPermit,
        os_approval::{
            bind_attestation, key_id_of, Assurance, BackendError, KeyInfo, OsCredentialBackend, PresenceSignature, UserVerification,
        },
    };
    use sha2::{Digest, Sha256};
    use windows::{
        core::{Array, HSTRING},
        Security::{
            Credentials::{
                KeyCredential, KeyCredentialAttestationStatus, KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
            },
            Cryptography::{Core::CryptographicPublicKeyBlobType, CryptographicBuffer},
        },
        Storage::Streams::IBuffer,
    };

    pub struct WindowsHelloBackend;

    /// Runs `f` on a private thread with its own multithreaded apartment (WinRT needs an initialised apartment).
    fn in_apartment<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, BackendError> {
        std::thread::spawn(move || {
            crate::cng_verify::init_apartment();
            f()
        })
        .join()
        .map_err(|_| BackendError::Failed)
    }

    fn bytes_of(buffer: &IBuffer) -> Result<Vec<u8>, BackendError> {
        let mut arr = Array::<u8>::new();
        CryptographicBuffer::CopyToByteArray(buffer, &mut arr).map_err(|_| BackendError::Failed)?;
        Ok(arr.to_vec())
    }

    /// The credential's identity; with `with_attestation` also the evidence the OS reports (this may take longer and may involve the OS
    /// vendor's services: it is therefore asked for only at enrollment, at key-store opening and just before a signature, never on every
    /// check). The evidence digest covers the CERTIFICATE CHAIN only (the attestation statement itself changes with every call) and is
    /// bound to this exact public key. Whether the chain digest is stable across calls is UNVERIFIED: if it is not, the key store fails
    /// closed (`approval_attestation_evidence_changed`). Nothing here claims verification: the claim is at most `OsReportedHardware`, and
    /// it only counts if the evidence is bound to the key (`established_assurance`).
    fn describe(cred: &KeyCredential, with_attestation: bool) -> Result<KeyInfo, BackendError> {
        let public =
            cred.RetrievePublicKeyWithBlobType(CryptographicPublicKeyBlobType::BCryptPublicKey).map_err(|_| BackendError::Failed)?;
        let public_key = bytes_of(&public)?;
        let key_id = key_id_of(&public_key);
        let mut info = KeyInfo {
            key_id: key_id.clone(),
            public_key,
            assurance: Assurance::OsSoftware,
            attestation_material_digest: None,
            attestation_digest: None,
        };
        if with_attestation {
            if let Ok(att) = cred.GetAttestationAsync().and_then(|op| op.join()) {
                if att.Status().ok() == Some(KeyCredentialAttestationStatus::Success) {
                    if let Some(chain) = att.CertificateChainBuffer().ok().and_then(|b| bytes_of(&b).ok()).filter(|c| !c.is_empty()) {
                        let material: String = Sha256::digest(&chain).iter().map(|b| format!("{b:02x}")).collect();
                        info.attestation_digest = Some(bind_attestation(&key_id, &material));
                        info.attestation_material_digest = Some(material);
                        info.assurance = Assurance::OsReportedHardware;
                    }
                }
            }
        }
        Ok(info)
    }

    impl OsCredentialBackend for WindowsHelloBackend {
        fn available(&self) -> bool {
            in_apartment(|| KeyCredentialManager::IsSupportedAsync().and_then(|op| op.join()).unwrap_or(false)).unwrap_or(false)
        }

        fn existing_key(&self, name: &str) -> Result<Option<KeyInfo>, BackendError> {
            let name = HSTRING::from(name);
            in_apartment(move || {
                let r = KeyCredentialManager::OpenAsync(&name).and_then(|op| op.join()).map_err(|_| BackendError::Unavailable)?;
                match r.Status().map_err(|_| BackendError::Unavailable)? {
                    KeyCredentialStatus::Success => describe(&r.Credential().map_err(|_| BackendError::Failed)?, false).map(Some),
                    KeyCredentialStatus::NotFound => Ok(None),
                    _ => Err(BackendError::Unavailable),
                }
            })?
        }

        fn attest(&self, name: &str) -> Result<Option<KeyInfo>, BackendError> {
            let name = HSTRING::from(name);
            in_apartment(move || {
                let r = KeyCredentialManager::OpenAsync(&name).and_then(|op| op.join()).map_err(|_| BackendError::Unavailable)?;
                match r.Status().map_err(|_| BackendError::Unavailable)? {
                    KeyCredentialStatus::Success => describe(&r.Credential().map_err(|_| BackendError::Failed)?, true).map(Some),
                    KeyCredentialStatus::NotFound => Ok(None),
                    _ => Err(BackendError::Unavailable),
                }
            })?
        }

        fn create_key(&self, name: &str, _permit: &EnrollmentPermit) -> Result<KeyInfo, BackendError> {
            let name = HSTRING::from(name);
            in_apartment(move || {
                let r = KeyCredentialManager::RequestCreateAsync(&name, KeyCredentialCreationOption::FailIfExists)
                    .and_then(|op| op.join())
                    .map_err(|_| BackendError::Unavailable)?;
                match r.Status().map_err(|_| BackendError::Unavailable)? {
                    KeyCredentialStatus::Success => describe(&r.Credential().map_err(|_| BackendError::Failed)?, true),
                    KeyCredentialStatus::UserCanceled => Err(BackendError::Declined),
                    KeyCredentialStatus::CredentialAlreadyExists => Err(BackendError::KeyExists),
                    _ => Err(BackendError::Failed),
                }
            })?
        }

        fn delete_key(&self, name: &str) -> Result<(), BackendError> {
            let name = HSTRING::from(name);
            in_apartment(move || KeyCredentialManager::DeleteAsync(&name).and_then(|op| op.join()).map_err(|_| BackendError::Failed))?
        }

        fn sign_with_presence(&self, name: &str, message: &[u8]) -> Result<PresenceSignature, BackendError> {
            let name = HSTRING::from(name);
            let message = message.to_vec();
            in_apartment(move || {
                let opened = KeyCredentialManager::OpenAsync(&name).and_then(|op| op.join()).map_err(|_| BackendError::Unavailable)?;
                if opened.Status().map_err(|_| BackendError::Unavailable)? != KeyCredentialStatus::Success {
                    return Err(BackendError::KeyMissing);
                }
                let cred = opened.Credential().map_err(|_| BackendError::Failed)?;
                let data = CryptographicBuffer::CreateFromByteArray(&message).map_err(|_| BackendError::Failed)?;
                // the OS asks the person (PIN/biometric) as part of THIS call; no signature exists unless that succeeded
                let r = cred.RequestSignAsync(&data).and_then(|op| op.join()).map_err(|_| BackendError::Unavailable)?;
                match r.Status().map_err(|_| BackendError::Unavailable)? {
                    KeyCredentialStatus::Success => {
                        let sig = bytes_of(&r.Result().map_err(|_| BackendError::Failed)?)?;
                        Ok(PresenceSignature { signature: sig, user_verification: UserVerification::Verified })
                    }
                    KeyCredentialStatus::UserCanceled => {
                        Ok(PresenceSignature { signature: Vec::new(), user_verification: UserVerification::Declined })
                    }
                    _ => Ok(PresenceSignature { signature: Vec::new(), user_verification: UserVerification::Unavailable }),
                }
            })?
        }
    }
}
