//! Optional support-bundle encryption envelope.
//!
//! `SUPPORT-001` produces a plaintext JSON `SupportBundle`. This
//! module wraps the serialised form in an [`EncryptedBundleEnvelope`]
//! carrying:
//!
//! * The encryption scheme identifier (`"age"`, `"pgp"`).
//! * The recipient identifier (an age public key string, or a PGP
//!   key fingerprint) — opaque to this crate, validated downstream.
//! * The ciphertext bytes encoded as base64.
//! * The SHA-256 of the *plaintext* bundle so support-side decoders
//!   can verify they decrypted the same bytes that were sealed.
//!
//! The crate ships an `Encryptor` trait so the actual age / PGP
//! integration lives behind a feature-gated implementation (the
//! `age` and `pgp` crates pull in heavyweight transitive deps;
//! keeping the trait separate means the foundation crate stays
//! dependency-light, and a downstream binary can wire whichever
//! library it wants).
//!
//! A `NullEncryptor` is bundled for testing: it base64-encodes the
//! plaintext as-is so the envelope shape can be exercised without
//! the crypto deps.

use base64_engine::Engine;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{SupportBundle, sha256_hex};

/// Top-level envelope around an encrypted support bundle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedBundleEnvelope {
    pub schema_id: String,
    pub schema_version: u32,
    pub scheme: String,
    pub recipient: String,
    /// Base64-encoded ciphertext.
    pub ciphertext_b64: String,
    /// `sha256:<hex>` of the plaintext bundle bytes before
    /// encryption. Lets the recipient verify the decryption.
    pub plaintext_sha256: String,
}

const ENVELOPE_SCHEMA_ID: &str = "plsql.support.bundle.encrypted";
const ENVELOPE_SCHEMA_VERSION: u32 = 1;

/// Pluggable encryption strategy. Implementations live in
/// downstream crates so this foundation crate stays light.
pub trait Encryptor {
    /// Lower-case scheme identifier (`"age"`, `"pgp"`, …).
    fn scheme(&self) -> &str;
    /// Opaque recipient identifier (age pubkey, PGP fingerprint).
    fn recipient(&self) -> &str;
    /// Encrypt `plaintext` returning the ciphertext bytes.
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptError>;
}

#[derive(Debug, Error)]
pub enum EncryptError {
    #[error("encryption backend error: {0}")]
    Backend(String),
    #[error("bundle serialisation failure: {0}")]
    Serde(String),
}

/// Encrypt a `SupportBundle` through the supplied [`Encryptor`].
pub fn encrypt_bundle<E: Encryptor>(
    bundle: &SupportBundle,
    encryptor: &E,
) -> Result<EncryptedBundleEnvelope, EncryptError> {
    let plaintext = serde_json::to_vec(bundle).map_err(|e| EncryptError::Serde(e.to_string()))?;
    let ciphertext = encryptor.encrypt(&plaintext)?;
    Ok(EncryptedBundleEnvelope {
        schema_id: ENVELOPE_SCHEMA_ID.into(),
        schema_version: ENVELOPE_SCHEMA_VERSION,
        scheme: encryptor.scheme().to_string(),
        recipient: encryptor.recipient().to_string(),
        ciphertext_b64: base64_engine::engine::general_purpose::STANDARD.encode(&ciphertext),
        plaintext_sha256: sha256_hex(&plaintext),
    })
}

/// Test-only encryptor that doesn't encrypt anything. Useful for
/// exercising the envelope shape in unit tests without pulling in
/// `age` / `pgp` deps. NOT suitable for production use.
#[derive(Clone, Debug)]
pub struct NullEncryptor {
    pub recipient: String,
}

impl NullEncryptor {
    #[must_use]
    pub fn new(recipient: &str) -> Self {
        Self {
            recipient: recipient.to_string(),
        }
    }
}

impl Encryptor for NullEncryptor {
    fn scheme(&self) -> &str {
        "null"
    }
    fn recipient(&self) -> &str {
        &self.recipient
    }
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptError> {
        Ok(plaintext.to_vec())
    }
}

// Lightweight base64 implementation kept in-crate so the
// foundation doesn't pull in a base64 dep.
mod base64_engine {
    pub trait Engine {
        fn encode(&self, bytes: &[u8]) -> String;
    }
    pub mod engine {
        pub mod general_purpose {
            use super::super::Engine;
            pub struct StandardEngine;
            pub const STANDARD: StandardEngine = StandardEngine;
            impl Engine for StandardEngine {
                fn encode(&self, bytes: &[u8]) -> String {
                    super::super::encode_standard(bytes)
                }
            }
        }
    }
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    pub fn encode_standard(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        let mut i = 0;
        while i + 3 <= bytes.len() {
            let triple =
                ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
            out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
            out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
            out.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
            out.push(CHARS[(triple & 0x3F) as usize] as char);
            i += 3;
        }
        let rem = bytes.len() - i;
        if rem == 1 {
            let triple = (bytes[i] as u32) << 16;
            out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
            out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
            out.push_str("==");
        } else if rem == 2 {
            let triple = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
            out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
            out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
            out.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
            out.push('=');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RedactionManifest, SupportBundleBuilder};

    fn bundle() -> SupportBundle {
        let mut b =
            SupportBundleBuilder::new("1.0", "2026-05-15T18:00:00Z", RedactionManifest::empty());
        b.operator_note("test").unwrap();
        b.add_input("query.sql", "SELECT 1 FROM dual");
        b.build()
    }

    #[test]
    fn encrypted_envelope_carries_schema_id_and_recipient() {
        let env = encrypt_bundle(&bundle(), &NullEncryptor::new("age1example...")).unwrap();
        assert_eq!(env.schema_id, "plsql.support.bundle.encrypted");
        assert_eq!(env.schema_version, 1);
        assert_eq!(env.scheme, "null");
        assert_eq!(env.recipient, "age1example...");
    }

    #[test]
    fn plaintext_sha256_present() {
        let env = encrypt_bundle(&bundle(), &NullEncryptor::new("r")).unwrap();
        assert!(env.plaintext_sha256.starts_with("sha256:"));
        // Same bundle → same sha256.
        let env2 = encrypt_bundle(&bundle(), &NullEncryptor::new("r")).unwrap();
        assert_eq!(env.plaintext_sha256, env2.plaintext_sha256);
    }

    #[test]
    fn ciphertext_b64_round_trips_through_null_encryptor() {
        let env = encrypt_bundle(&bundle(), &NullEncryptor::new("r")).unwrap();
        let raw = base64_engine::engine::general_purpose::STANDARD
            .encode(&serde_json::to_vec(&bundle()).unwrap());
        assert_eq!(env.ciphertext_b64, raw);
    }

    #[test]
    fn envelope_serialises_round_trip() {
        let env = encrypt_bundle(&bundle(), &NullEncryptor::new("r")).unwrap();
        let json = serde_json::to_string(&env).unwrap();
        let back: EncryptedBundleEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, env);
    }

    #[test]
    fn base64_handles_trailing_byte() {
        // base64 padding: 1 leftover byte → `xx==`.
        let s = base64_engine::engine::general_purpose::STANDARD.encode(b"A");
        assert_eq!(s, "QQ==");
    }

    #[test]
    fn base64_handles_two_trailing_bytes() {
        let s = base64_engine::engine::general_purpose::STANDARD.encode(b"AB");
        assert_eq!(s, "QUI=");
    }

    #[test]
    fn base64_handles_full_triples() {
        let s = base64_engine::engine::general_purpose::STANDARD.encode(b"ABC");
        assert_eq!(s, "QUJD");
    }

    struct FailingEncryptor;
    impl Encryptor for FailingEncryptor {
        fn scheme(&self) -> &str {
            "fail"
        }
        fn recipient(&self) -> &str {
            "x"
        }
        fn encrypt(&self, _plaintext: &[u8]) -> Result<Vec<u8>, EncryptError> {
            Err(EncryptError::Backend("nope".into()))
        }
    }

    #[test]
    fn encryptor_error_surfaces() {
        let err = encrypt_bundle(&bundle(), &FailingEncryptor).unwrap_err();
        assert!(matches!(err, EncryptError::Backend(_)));
    }
}
