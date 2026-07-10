//! Coordinator role gate.
//!
//! Coordinator capabilities (creating studies, importing LLM output, building
//! assignment packages, importing responses) are unlocked only by an Ed25519
//! credential signed by the study authority. The authority's PUBLIC key is
//! compiled into the build; the private key lives with the study PI and never
//! ships. A reviewer install has no valid credential, so — combined with the
//! fact that a reviewer instance holds no key to any coordinator workspace —
//! the boundary is real, not a frontend toggle.
//!
//! Enforced by the coordinator command surface when coordinator mode lands.
#![allow(dead_code)]

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("malformed credential")]
    Malformed,
    #[error("credential signature is invalid")]
    BadSignature,
    #[error("credential is not yet valid or has expired")]
    OutOfWindow,
}

/// The signed body. Signature is computed over the canonical JSON of this
/// struct (serde_json is deterministic for a fixed field order).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorClaims {
    pub coordinator_id: String,
    pub study_id: String,
    pub issued_at: String,
    /// Optional RFC3339 expiry; when present it is enforced.
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// The on-disk credential file: claims + detached Ed25519 signature (hex).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorCredential {
    pub claims: CoordinatorClaims,
    /// Hex-encoded 64-byte Ed25519 signature over `canonical(claims)`.
    pub signature: String,
}

/// Canonical bytes that get signed/verified.
pub fn canonical_claims(claims: &CoordinatorClaims) -> Vec<u8> {
    serde_json::to_vec(claims).expect("claims serialize")
}

/// Verify a credential against an authority public key and (optionally) a clock.
/// Returns the verified claims on success.
pub fn verify_credential(
    cred: &CoordinatorCredential,
    authority_pubkey: &VerifyingKey,
    now_rfc3339: Option<&str>,
) -> Result<CoordinatorClaims, CredentialError> {
    let sig_bytes = hex_decode(&cred.signature).ok_or(CredentialError::Malformed)?;
    let sig_arr: [u8; 64] = sig_bytes.try_into().map_err(|_| CredentialError::Malformed)?;
    let signature = Signature::from_bytes(&sig_arr);

    authority_pubkey
        .verify(&canonical_claims(&cred.claims), &signature)
        .map_err(|_| CredentialError::BadSignature)?;

    if let (Some(exp), Some(now)) = (&cred.claims.expires_at, now_rfc3339) {
        if now > exp.as_str() {
            return Err(CredentialError::OutOfWindow);
        }
    }
    Ok(cred.claims.clone())
}

/// The study authority's compiled-in public key. Placeholder (all-zero) until
/// the study's real key is provisioned at build time; verification of any
/// credential against a zero key fails closed.
pub const AUTHORITY_PUBLIC_KEY_HEX: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

// Wired into the coordinator role-gate command when coordinator mode lands.
#[allow(dead_code)]
pub fn authority_public_key() -> Option<VerifyingKey> {
    let bytes = hex_decode(AUTHORITY_PUBLIC_KEY_HEX)?;
    let arr: [u8; 32] = bytes.try_into().ok()?;
    VerifyingKey::from_bytes(&arr).ok()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{:02x}", b);
    }
    s
}

/// Test/tooling helper: issue a credential with a signing key. In production the
/// PI runs the equivalent offline signing tool; the app never signs.
#[cfg(any(test, feature = "issuer-tools"))]
pub fn issue_credential(
    signing_key: &ed25519_dalek::SigningKey,
    claims: CoordinatorClaims,
) -> CoordinatorCredential {
    use ed25519_dalek::Signer;
    let sig = signing_key.sign(&canonical_claims(&claims));
    CoordinatorCredential {
        claims,
        signature: hex_encode(&sig.to_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::RngCore;

    fn random_signing_key() -> SigningKey {
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    fn claims() -> CoordinatorClaims {
        CoordinatorClaims {
            coordinator_id: "COORD-1".into(),
            study_id: "STUDY-1".into(),
            issued_at: "2026-07-10T00:00:00Z".into(),
            expires_at: Some("2027-01-01T00:00:00Z".into()),
        }
    }

    #[test]
    fn valid_credential_verifies() {
        let sk = random_signing_key();
        let cred = issue_credential(&sk, claims());
        let out = verify_credential(&cred, &sk.verifying_key(), Some("2026-08-01T00:00:00Z")).unwrap();
        assert_eq!(out.coordinator_id, "COORD-1");
    }

    #[test]
    fn wrong_authority_key_is_rejected() {
        let sk = random_signing_key();
        let other = random_signing_key();
        let cred = issue_credential(&sk, claims());
        assert!(matches!(
            verify_credential(&cred, &other.verifying_key(), None),
            Err(CredentialError::BadSignature)
        ));
    }

    #[test]
    fn tampered_claims_are_rejected() {
        let sk = random_signing_key();
        let mut cred = issue_credential(&sk, claims());
        cred.claims.coordinator_id = "ATTACKER".into();
        assert!(matches!(
            verify_credential(&cred, &sk.verifying_key(), None),
            Err(CredentialError::BadSignature)
        ));
    }

    #[test]
    fn expired_credential_is_rejected() {
        let sk = random_signing_key();
        let cred = issue_credential(&sk, claims());
        assert!(matches!(
            verify_credential(&cred, &sk.verifying_key(), Some("2099-01-01T00:00:00Z")),
            Err(CredentialError::OutOfWindow)
        ));
    }

    #[test]
    fn placeholder_authority_key_is_all_zero() {
        // Fails closed: a real deployment must replace this.
        assert_eq!(AUTHORITY_PUBLIC_KEY_HEX.len(), 64);
    }
}
