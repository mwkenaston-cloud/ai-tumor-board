//! Password <-> SQLCipher container glue for `.atb` (assignment) and `.atbr`
//! (response) files.

use std::io::Read;
use std::path::Path;

use rusqlite::Connection;
use zeroize::Zeroize;

use super::key_derivation::{self, KdfError, SALT_LEN};
use super::FORMAT_VERSION;
use crate::db::connection::{self as dbc, DbError};

#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    #[error(transparent)]
    Kdf(#[from] KdfError),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("io error: {0}")]
    Io(String),
    #[error("not a valid container file")]
    Malformed,
}

impl From<std::io::Error> for ContainerError {
    fn from(e: std::io::Error) -> Self {
        ContainerError::Io(e.to_string())
    }
}

/// Which kind of container this is; recorded (encrypted) in `app_metadata`.
// Coordinator/Response variants are wired up in later phases (package build,
// response export); kept here so the container format is complete.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerRole {
    /// Coordinator workspace / assignment source.
    Coordinator,
    /// Reviewer assignment (`.atb`).
    Reviewer,
    /// Reviewer response (`.atbr`).
    Response,
}

impl ContainerRole {
    fn as_str(self) -> &'static str {
        match self {
            ContainerRole::Coordinator => "coordinator",
            ContainerRole::Reviewer => "reviewer",
            ContainerRole::Response => "response",
        }
    }
}

/// Create a new encrypted container at `path`, keyed by `password`, with the
/// initial schema applied. A fresh random salt is generated and persisted in
/// the SQLCipher header. Returns the open, keyed connection.
pub fn create(
    path: &Path,
    password: &str,
    role: ContainerRole,
) -> Result<Connection, ContainerError> {
    let salt = key_derivation::generate_salt();
    let params = key_derivation::params_for_version(FORMAT_VERSION)?;
    let mut key = key_derivation::derive_key(password, &salt, &params)?;

    let conn = dbc::open_encrypted_with_salt(path, &key, &salt);
    key.zeroize();
    let conn = conn?;

    dbc::initialize_schema(&conn)?;
    conn.execute(
        "INSERT OR REPLACE INTO app_metadata(key, value) VALUES ('format_version', ?1)",
        [FORMAT_VERSION.to_string()],
    )
    .map_err(DbError::from)?;
    conn.execute(
        "INSERT OR REPLACE INTO app_metadata(key, value) VALUES ('role', ?1)",
        [role.as_str()],
    )
    .map_err(DbError::from)?;
    Ok(conn)
}

/// Open an existing encrypted container. Reads the plaintext header salt, derives
/// the key with Argon2id, and verifies. A wrong password, truncated file, or
/// tampered ciphertext all fail without exposing any content.
pub fn open(path: &Path, password: &str) -> Result<Connection, ContainerError> {
    let salt = read_header_salt(path)?;
    // v1 pins the KDF params by FORMAT_VERSION. When multiple formats exist,
    // this is where we would consult the file's declared version.
    let params = key_derivation::params_for_version(FORMAT_VERSION)?;
    let mut key = key_derivation::derive_key(password, &salt, &params)?;

    let conn = dbc::open_encrypted_with_salt(path, &key, &salt);
    key.zeroize();
    let conn = conn?;
    // Page-1 keying above catches a wrong password; this catches tampering or
    // corruption anywhere else in the file before we trust any content.
    dbc::verify_integrity(&conn)?;
    Ok(conn)
}

/// Read the 16-byte plaintext salt SQLCipher stores at the start of the file.
fn read_header_salt(path: &Path) -> Result<[u8; SALT_LEN], ContainerError> {
    let mut f = std::fs::File::open(path)?;
    let mut salt = [0u8; SALT_LEN];
    f.read_exact(&mut salt)
        .map_err(|_| ContainerError::Malformed)?;
    Ok(salt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("atb-container-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn create_open_roundtrip_with_password() {
        let path = tempdir().join("assignment.atb");
        {
            let conn = create(&path, "s3cret-passphrase", ContainerRole::Reviewer).unwrap();
            conn.execute(
                "INSERT INTO studies(study_id,title,protocol_version,schema_version,created_at)
                 VALUES ('S1','Study','v1',1,'2026-07-10T00:00:00Z')",
                [],
            )
            .unwrap();
        }
        let conn = open(&path, "s3cret-passphrase").unwrap();
        let title: String = conn
            .query_row("SELECT title FROM studies WHERE study_id='S1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(title, "Study");
        let role: String = conn
            .query_row("SELECT value FROM app_metadata WHERE key='role'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(role, "reviewer");
    }

    #[test]
    fn wrong_password_fails_without_exposing_data() {
        let path = tempdir().join("secret.atb");
        {
            let _conn = create(&path, "right-password", ContainerRole::Reviewer).unwrap();
        }
        let err = open(&path, "WRONG-password").expect_err("must reject");
        assert!(
            matches!(err, ContainerError::Db(DbError::BadKeyOrCorrupt)),
            "got {err:?}"
        );
    }

    #[test]
    fn tampered_ciphertext_is_detected() {
        let path = tempdir().join("tampered.atb");
        {
            let conn = create(&path, "pw", ContainerRole::Reviewer).unwrap();
            conn.execute(
                "INSERT INTO studies(study_id,title,protocol_version,schema_version,created_at)
                 VALUES ('S1','Study','v1',1,'2026-07-10T00:00:00Z')",
                [],
            )
            .unwrap();
        }
        // Flip a byte in the encrypted page region (well past the 16-byte salt).
        let mut bytes = std::fs::read(&path).unwrap();
        let idx = bytes.len() - 32;
        bytes[idx] ^= 0xFF;
        std::fs::write(&path, &bytes).unwrap();

        let err = open(&path, "pw").expect_err("tamper must be detected");
        assert!(
            matches!(err, ContainerError::Db(DbError::BadKeyOrCorrupt)),
            "got {err:?}"
        );
    }

    #[test]
    fn truncated_file_is_rejected() {
        let path = tempdir().join("truncated.atb");
        std::fs::write(&path, [0u8; 4]).unwrap(); // shorter than the 16-byte salt
        let err = open(&path, "pw").expect_err("truncated must fail");
        assert!(matches!(err, ContainerError::Malformed), "got {err:?}");
    }
}
