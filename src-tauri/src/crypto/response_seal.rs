//! Sealing a small secret (the `.atbr` response DEK) to the coordinator's
//! X25519 public key, sealed-box style: the reviewer generates an ephemeral
//! keypair, encrypts to the coordinator's public key, and prepends the
//! ephemeral public key. Only the coordinator's private key can open it, and no
//! reviewer keypair or coordinator password is required to produce it.
//!
//! Wired into `.atbr` response assembly and coordinator import in Phase 4.
#![allow(dead_code)]

use crypto_box::{
    aead::{Aead, AeadCore, OsRng},
    PublicKey, SalsaBox, SecretKey,
};

const EPHEMERAL_PK_LEN: usize = 32;
const NONCE_LEN: usize = 24; // XSalsa20Poly1305 (crypto_box SalsaBox)

#[derive(Debug, thiserror::Error)]
pub enum SealError {
    #[error("encryption failed")]
    Encrypt,
    #[error("malformed sealed blob")]
    Malformed,
    #[error("decryption failed (wrong key or tampered)")]
    Decrypt,
}

/// Generate a coordinator X25519 keypair `(secret, public)` as raw 32-byte
/// arrays. The coordinator keeps `secret` private; `public` is embedded in
/// assignment packages so reviewers can seal responses to it.
pub fn generate_keypair() -> ([u8; 32], [u8; 32]) {
    let secret = SecretKey::generate(&mut OsRng);
    let public = secret.public_key();
    (secret.to_bytes(), public.to_bytes())
}

/// Derive the X25519 public key for a given secret (used to publish a
/// coordinator's public key from their stored secret).
pub fn public_from_secret(secret: &[u8; 32]) -> [u8; 32] {
    SecretKey::from(*secret).public_key().to_bytes()
}

/// Seal `plaintext` to `recipient_public`. Output = ephemeral_pk(32) ||
/// nonce(24) || ciphertext(+16 tag).
pub fn seal_to(recipient_public: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
    let recipient = PublicKey::from(*recipient_public);
    let ephemeral = SecretKey::generate(&mut OsRng);
    let ephemeral_public = ephemeral.public_key();

    let boxx = SalsaBox::new(&recipient, &ephemeral);
    let nonce = SalsaBox::generate_nonce(&mut OsRng);
    let ct = boxx.encrypt(&nonce, plaintext).map_err(|_| SealError::Encrypt)?;

    let mut out = Vec::with_capacity(EPHEMERAL_PK_LEN + NONCE_LEN + ct.len());
    out.extend_from_slice(ephemeral_public.as_bytes());
    out.extend_from_slice(nonce.as_slice());
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Open a sealed blob with the coordinator's `recipient_secret`.
pub fn unseal(recipient_secret: &[u8; 32], sealed: &[u8]) -> Result<Vec<u8>, SealError> {
    if sealed.len() < EPHEMERAL_PK_LEN + NONCE_LEN {
        return Err(SealError::Malformed);
    }
    let (eph_pk_bytes, rest) = sealed.split_at(EPHEMERAL_PK_LEN);
    let (nonce_bytes, ct) = rest.split_at(NONCE_LEN);

    let eph_pk_arr: [u8; 32] = eph_pk_bytes.try_into().map_err(|_| SealError::Malformed)?;
    let ephemeral_public = PublicKey::from(eph_pk_arr);
    let secret = SecretKey::from(*recipient_secret);

    let boxx = SalsaBox::new(&ephemeral_public, &secret);
    let nonce = crypto_box::aead::generic_array::GenericArray::from_slice(nonce_bytes);
    boxx.decrypt(nonce, ct).map_err(|_| SealError::Decrypt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_unseal_roundtrip() {
        let (secret, public) = generate_keypair();
        let dek = [7u8; 32];
        let sealed = seal_to(&public, &dek).unwrap();
        // Non-trivial expansion (ephemeral pk + nonce + tag).
        assert!(sealed.len() > 32 + 24 + 32);
        let opened = unseal(&secret, &sealed).unwrap();
        assert_eq!(opened, dek);
    }

    #[test]
    fn wrong_secret_cannot_open() {
        let (_secret, public) = generate_keypair();
        let (other_secret, _) = generate_keypair();
        let sealed = seal_to(&public, b"top secret key material").unwrap();
        assert!(matches!(unseal(&other_secret, &sealed), Err(SealError::Decrypt)));
    }

    #[test]
    fn tampered_blob_is_rejected() {
        let (secret, public) = generate_keypair();
        let mut sealed = seal_to(&public, &[9u8; 32]).unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xFF;
        assert!(matches!(unseal(&secret, &sealed), Err(SealError::Decrypt)));
    }

    #[test]
    fn truncated_blob_is_rejected() {
        let (secret, _public) = generate_keypair();
        assert!(matches!(unseal(&secret, &[0u8; 10]), Err(SealError::Malformed)));
    }
}
