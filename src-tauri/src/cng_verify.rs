//! Windows: verifies an RSA PKCS#1 v1.5 / SHA-256 signature with a PUBLIC key blob (`BCRYPT_RSAPUBLIC_BLOB`) through the Windows
//! cryptography provider (WinRT `CryptographicEngine`, which is CNG underneath).
//!
//! This is what the approval path uses to check a Windows Hello signature against the public key recorded at enrollment. It holds no
//! private key, shows no prompt and talks to no credential service. Its behaviour is tested against ephemeral software keys that the
//! test itself generates in memory (`CreateKeyPair`: never stored, never a Windows Hello credential).
//!
//! WinRT is used on purpose: it needs no additional `windows-sys` feature, so the file-system-only Win32 surface that the discovery
//! layer's guard test pins stays exactly what it was.
#![cfg(windows)]
#![allow(dead_code)]

use crate::os_approval::PublicKeyVerifier;
use windows::Security::Cryptography::{
    Core::{AsymmetricAlgorithmNames, AsymmetricKeyAlgorithmProvider, CryptographicEngine, CryptographicPublicKeyBlobType},
    CryptographicBuffer,
};

windows_link::link!("combase.dll" "system" fn RoInitialize(init_type : i32) -> i32);

/// Gives the calling thread a multithreaded WinRT apartment ("already initialised" is not an error).
pub(crate) fn init_apartment() {
    // SAFETY: plain apartment initialisation of the calling thread (RO_INIT_MULTITHREADED = 1); the result is deliberately ignored.
    let _ = unsafe { RoInitialize(1) };
}

pub struct CngRsaSha256Verifier;

impl CngRsaSha256Verifier {
    fn try_verify(public_key: &[u8], message: &[u8], signature: &[u8]) -> windows::core::Result<bool> {
        let provider = AsymmetricKeyAlgorithmProvider::OpenAlgorithm(&AsymmetricAlgorithmNames::RsaSignPkcs1Sha256()?)?;
        let key = provider.ImportPublicKeyWithBlobType(
            &CryptographicBuffer::CreateFromByteArray(public_key)?,
            CryptographicPublicKeyBlobType::BCryptPublicKey,
        )?;
        CryptographicEngine::VerifySignature(
            &key,
            &CryptographicBuffer::CreateFromByteArray(message)?,
            &CryptographicBuffer::CreateFromByteArray(signature)?,
        )
    }
}

impl PublicKeyVerifier for CngRsaSha256Verifier {
    fn verify(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> bool {
        if public_key.is_empty() || signature.is_empty() || public_key.len() > 4096 || signature.len() > 1024 {
            return false;
        }
        let (k, m, s) = (public_key.to_vec(), message.to_vec(), signature.to_vec());
        // WinRT needs an initialised apartment: use a private thread so no other thread's apartment is touched; any error is "not verified"
        std::thread::spawn(move || {
            init_apartment();
            CngRsaSha256Verifier::try_verify(&k, &m, &s).unwrap_or(false)
        })
        .join()
        .unwrap_or(false)
    }
}

#[cfg(test)]
pub mod testkeys {
    //! Ephemeral software RSA keys for tests only (in memory, never stored, never a Windows Hello credential).
    use super::*;
    use windows::Security::Cryptography::Core::CryptographicKey;

    pub struct SoftKey {
        key: CryptographicKey,
        pub public_blob: Vec<u8>,
    }

    // SAFETY: WinRT CryptographicKey objects are agile and the tests only use them from the thread that created them or after joining.
    unsafe impl Send for SoftKey {}

    fn bytes(b: &windows::Storage::Streams::IBuffer) -> Vec<u8> {
        let mut arr = windows::core::Array::<u8>::new();
        CryptographicBuffer::CopyToByteArray(b, &mut arr).unwrap();
        arr.to_vec()
    }

    impl SoftKey {
        pub fn generate() -> SoftKey {
            init_apartment();
            let provider = AsymmetricKeyAlgorithmProvider::OpenAlgorithm(&AsymmetricAlgorithmNames::RsaSignPkcs1Sha256().unwrap()).unwrap();
            let key = provider.CreateKeyPair(2048).unwrap();
            let public_blob = bytes(&key.ExportPublicKeyWithBlobType(CryptographicPublicKeyBlobType::BCryptPublicKey).unwrap());
            SoftKey { key, public_blob }
        }

        pub fn sign(&self, message: &[u8]) -> Vec<u8> {
            init_apartment();
            bytes(&CryptographicEngine::Sign(&self.key, &CryptographicBuffer::CreateFromByteArray(message).unwrap()).unwrap())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{testkeys::SoftKey, *};

    #[test]
    fn a_genuine_signature_verifies_and_every_alteration_is_refused() {
        let k = SoftKey::generate();
        let msg = b"canonical approval payload";
        let sig = k.sign(msg);
        let v = CngRsaSha256Verifier;
        assert!(v.verify(&k.public_blob, msg, &sig));
        assert!(!v.verify(&k.public_blob, b"canonical approval payloaD", &sig), "another message");
        let mut bad = sig.clone();
        bad[10] ^= 1;
        assert!(!v.verify(&k.public_blob, msg, &bad), "a flipped bit");
        assert!(!v.verify(&k.public_blob, msg, &sig[..sig.len() - 1]), "a truncated signature");
        let other = SoftKey::generate();
        assert!(!v.verify(&other.public_blob, msg, &sig), "another key");
        assert!(!v.verify(b"not a key blob", msg, &sig), "garbage key");
        assert!(!v.verify(&[], msg, &sig) && !v.verify(&k.public_blob, msg, &[]), "empty inputs");
        assert!(!v.verify(&k.public_blob, msg, &[0u8; 256]), "an all-zero signature");
    }

    #[test]
    fn the_verifier_never_accepts_a_plain_digest_in_place_of_a_signature() {
        // a plain SHA-256 of the message (what a "fallback" would produce) is not a signature at all
        use sha2::{Digest, Sha256};
        let k = SoftKey::generate();
        let digest = Sha256::digest(b"m");
        assert!(!CngRsaSha256Verifier.verify(&k.public_blob, b"m", &digest));
    }

    #[test]
    fn the_public_key_blob_has_the_documented_rsa_public_blob_shape() {
        // BCRYPT_RSAPUBLIC_BLOB starts with the magic "RSA1" (0x31415352 little endian); this is what Windows Hello's public key blob type is asked for
        let k = SoftKey::generate();
        assert_eq!(&k.public_blob[..4], b"RSA1");
    }
}
