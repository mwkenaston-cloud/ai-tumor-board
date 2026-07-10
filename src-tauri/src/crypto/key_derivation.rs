//! Argon2id key derivation: reviewer password -> 256-bit SQLCipher key.

use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use zeroize::Zeroize;

/// Argon2id parameters for a given format version. These are deliberately
/// conservative defaults pending institutional security review; changing them
/// requires a new `FORMAT_VERSION` and an entry in `params_for_version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    pub mem_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

/// v1: 64 MiB, 3 passes, 1 lane.
pub const PARAMS_V1: KdfParams = KdfParams {
    mem_kib: 64 * 1024,
    iterations: 3,
    parallelism: 1,
};

pub const SALT_LEN: usize = 16;
pub const KEY_LEN: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum KdfError {
    #[error("unsupported format version: {0}")]
    UnsupportedVersion(u16),
    #[error("argon2 failure: {0}")]
    Argon2(String),
}

/// Return the KDF parameters that were in force for a given container format
/// version, so older files keep opening after we harden defaults.
pub fn params_for_version(format_version: u16) -> Result<KdfParams, KdfError> {
    match format_version {
        1 => Ok(PARAMS_V1),
        v => Err(KdfError::UnsupportedVersion(v)),
    }
}

/// Generate a fresh random 16-byte salt using the OS CSPRNG.
pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    salt
}

/// Derive a 256-bit key from `password` and `salt` using Argon2id with the
/// given parameters. The caller is responsible for zeroizing the returned key
/// when done (see `container` for how it is handed to SQLCipher and dropped).
pub fn derive_key(
    password: &str,
    salt: &[u8; SALT_LEN],
    params: &KdfParams,
) -> Result<[u8; KEY_LEN], KdfError> {
    let a2params = Params::new(
        params.mem_kib,
        params.iterations,
        params.parallelism,
        Some(KEY_LEN),
    )
    .map_err(|e| KdfError::Argon2(e.to_string()))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, a2params);

    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| {
            key.zeroize();
            KdfError::Argon2(e.to_string())
        })?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derivation_is_deterministic_for_same_inputs() {
        let salt = [42u8; SALT_LEN];
        let k1 = derive_key("correct horse battery staple", &salt, &PARAMS_V1).unwrap();
        let k2 = derive_key("correct horse battery staple", &salt, &PARAMS_V1).unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn different_password_or_salt_changes_key() {
        let salt_a = [1u8; SALT_LEN];
        let salt_b = [2u8; SALT_LEN];
        let base = derive_key("pw", &salt_a, &PARAMS_V1).unwrap();
        assert_ne!(base, derive_key("pw2", &salt_a, &PARAMS_V1).unwrap());
        assert_ne!(base, derive_key("pw", &salt_b, &PARAMS_V1).unwrap());
    }

    #[test]
    fn salts_are_random_and_distinct() {
        assert_ne!(generate_salt(), generate_salt());
    }

    #[test]
    fn unknown_version_is_rejected() {
        assert!(matches!(
            params_for_version(999),
            Err(KdfError::UnsupportedVersion(999))
        ));
    }
}
