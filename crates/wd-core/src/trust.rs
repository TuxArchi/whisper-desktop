//! Authenticity of the download catalogue.
//!
//! The application ships a public key and refuses any catalogue whose
//! signature does not match. Without this, whoever controls the hosting can
//! serve arbitrary binaries; the SHA-256 inside the catalogue only protects
//! against a corrupted download, not a malicious one.

use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// Ed25519 public key of the project (raw 32 bytes, base64).
/// Regenerate with `node scripts/trust.mjs keygen` and update this constant.
pub const CATALOG_PUBLIC_KEY: &str = "yTr7vAs25nvtFhPwfGeaoLXcezMl+eqNoSMASXvm6cE=";

/// Throwaway key used to sign the test fixtures. Not shipped.
#[cfg(test)]
pub const TEST_PUBLIC_KEY: &str = "tVmwxlLcSXKvCB6nCBUb8FgOIuBmCC6znxS6tK1CEMg=";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TrustError {
    #[error("la clave pública no es válida")]
    InvalidPublicKey,
    #[error("la firma no tiene el formato esperado")]
    InvalidSignature,
    #[error("la firma no corresponde a este catálogo")]
    Mismatch,
}

/// Verifies `data` against a base64 Ed25519 signature.
pub fn verify_with(
    public_key_b64: &str,
    data: &[u8],
    signature_b64: &str,
) -> Result<(), TrustError> {
    let key_bytes: [u8; 32] = STANDARD
        .decode(public_key_b64.trim())
        .map_err(|_| TrustError::InvalidPublicKey)?
        .try_into()
        .map_err(|_| TrustError::InvalidPublicKey)?;

    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| TrustError::InvalidPublicKey)?;

    let signature_bytes: [u8; 64] = STANDARD
        .decode(signature_b64.trim())
        .map_err(|_| TrustError::InvalidSignature)?
        .try_into()
        .map_err(|_| TrustError::InvalidSignature)?;

    key.verify(data, &Signature::from_bytes(&signature_bytes))
        .map_err(|_| TrustError::Mismatch)
}

/// Verifies against the key embedded in the application.
pub fn verify(data: &[u8], signature_b64: &str) -> Result<(), TrustError> {
    verify_with(CATALOG_PUBLIC_KEY, data, signature_b64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Public key of the throwaway keypair used to sign the test fixtures.
    fn payload() -> Vec<u8> {
        include_bytes!("../../../tests/fixtures/catalog-signed.json").to_vec()
    }

    fn signature() -> String {
        include_str!("../../../tests/fixtures/catalog-signed.sig")
            .trim()
            .to_string()
    }

    #[test]
    fn accepts_a_valid_signature() {
        verify_with(TEST_PUBLIC_KEY, &payload(), &signature()).expect("firma válida");
    }

    #[test]
    fn rejects_a_modified_catalogue() {
        let mut tampered = payload();
        let position = tampered
            .windows(8)
            .position(|window| window == b"example.")
            .expect("marcador");
        tampered[position] = b'E';
        assert_eq!(
            verify_with(TEST_PUBLIC_KEY, &tampered, &signature()),
            Err(TrustError::Mismatch)
        );
    }

    #[test]
    fn rejects_a_signature_from_another_key() {
        assert_eq!(
            verify_with(CATALOG_PUBLIC_KEY, &payload(), &signature()),
            Err(TrustError::Mismatch)
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(
            verify_with(TEST_PUBLIC_KEY, b"hola", "no-es-base64"),
            Err(TrustError::InvalidSignature)
        );
        assert_eq!(
            verify_with("no-es-base64", b"hola", &signature()),
            Err(TrustError::InvalidPublicKey)
        );
    }
}
