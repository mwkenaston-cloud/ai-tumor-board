//! Opening and initializing SQLCipher-encrypted connections.

use rusqlite::{Connection, OpenFlags};

use super::SCHEMA_VERSION;

/// The initial schema, embedded at compile time so no external file is needed
/// at runtime.
const MIGRATION_001: &str = include_str!("../../migrations/001_initial.sql");

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("wrong password or corrupted/tampered database")]
    BadKeyOrCorrupt,
    #[error("this build was compiled without SQLCipher support")]
    NotEncrypted,
}

/// Format a 32-byte key as the SQLCipher raw-key pragma literal:
/// `PRAGMA key = "x'<64 hex chars>'"`. Raw-key mode bypasses SQLCipher's own
/// KDF because we derive the key ourselves with Argon2id.
fn raw_key_pragma(key: &[u8; 32]) -> String {
    let mut hex = String::with_capacity(64);
    for b in key {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", b);
    }
    format!("PRAGMA key = \"x'{hex}'\";")
}

/// Open (or create) a SQLCipher database at `path`, keyed with `key`.
///
/// The key must be applied before any other statement. We then force a read of
/// `sqlite_master`; on an existing file an incorrect key (or tampered
/// ciphertext) fails HMAC verification here and surfaces as `BadKeyOrCorrupt`
/// rather than exposing any partial content.
pub fn open_encrypted(path: &std::path::Path, key: &[u8; 32]) -> Result<Connection, DbError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    apply_key_and_verify(&conn, key)?;
    Ok(conn)
}

/// Open an in-memory encrypted database (used by tests and transient work).
#[cfg(test)]
pub fn open_in_memory(key: &[u8; 32]) -> Result<Connection, DbError> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(&raw_key_pragma(key))?;
    // In-memory has no existing header to verify against, so just confirm cipher.
    ensure_sqlcipher(&conn)?;
    Ok(conn)
}

fn apply_key_and_verify(conn: &Connection, key: &[u8; 32]) -> Result<(), DbError> {
    conn.execute_batch(&raw_key_pragma(key))?;
    ensure_sqlcipher(conn)?;
    // Touching the schema forces SQLCipher to decrypt & HMAC-verify page 1.
    match conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)) {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(_, _)) => Err(DbError::BadKeyOrCorrupt),
        Err(e) => Err(DbError::Sqlite(e)),
    }
}

/// Confirm the connection is actually SQLCipher (not vanilla SQLite). A
/// non-empty `cipher_version` guarantees encryption is in force.
fn ensure_sqlcipher(conn: &Connection) -> Result<(), DbError> {
    let version: Option<String> = conn
        .query_row("PRAGMA cipher_version", [], |r| r.get(0))
        .ok()
        .flatten();
    match version {
        Some(v) if !v.is_empty() => Ok(()),
        _ => Err(DbError::NotEncrypted),
    }
}

/// Return the SQLCipher version string (e.g. "4.6.1 community").
pub fn cipher_version(conn: &Connection) -> Option<String> {
    conn.query_row("PRAGMA cipher_version", [], |r| r.get(0))
        .ok()
        .flatten()
}

/// Apply the initial schema to a freshly created, keyed connection and record
/// the schema version.
pub fn initialize_schema(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(MIGRATION_001)?;
    conn.execute(
        "INSERT OR REPLACE INTO app_metadata(key, value) VALUES ('schema_version', ?1)",
        [SCHEMA_VERSION.to_string()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    #[test]
    fn sqlcipher_is_active() {
        let conn = open_in_memory(&test_key(1)).expect("open");
        let v = cipher_version(&conn).expect("cipher_version present");
        assert!(!v.is_empty(), "expected a SQLCipher version, got empty");
    }

    #[test]
    fn schema_initializes_and_roundtrips() {
        let dir = tempdir();
        let path = dir.join("study.atb");
        let key = test_key(7);

        {
            let conn = open_encrypted(&path, &key).expect("create");
            initialize_schema(&conn).expect("migrate");
            conn.execute(
                "INSERT INTO studies(study_id, title, protocol_version, schema_version, created_at)
                 VALUES ('S1', 'Test Study', 'v1', 1, '2026-07-10T00:00:00Z')",
                [],
            )
            .expect("insert study");
        } // connection dropped/closed

        // Reopen with the correct key: data survives.
        let conn = open_encrypted(&path, &key).expect("reopen");
        let title: String = conn
            .query_row("SELECT title FROM studies WHERE study_id='S1'", [], |r| r.get(0))
            .expect("read back");
        assert_eq!(title, "Test Study");
    }

    #[test]
    fn wrong_key_is_rejected() {
        let dir = tempdir();
        let path = dir.join("secret.atb");

        {
            let conn = open_encrypted(&path, &test_key(3)).expect("create");
            initialize_schema(&conn).expect("migrate");
        }

        // Opening with a different key must fail at verification — never expose data.
        let err = open_encrypted(&path, &test_key(4)).expect_err("wrong key must fail");
        assert!(matches!(err, DbError::BadKeyOrCorrupt), "got: {err:?}");
    }

    /// Minimal unique temp dir without pulling in an extra crate.
    fn tempdir() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("atb-test-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
