//! Encrypted database layer (SQLCipher via rusqlite).
//!
//! Every patient-containing database is a SQLCipher file: pages are encrypted
//! at rest with AES-256 + per-page HMAC, so no plaintext DB is ever written to
//! disk. The 256-bit key is derived from the reviewer password with Argon2id
//! (see `crate::crypto`) and handed to SQLCipher in raw-key mode.

pub mod connection;
pub mod models;
pub mod repository;
pub mod seed;

pub const SCHEMA_VERSION: i64 = 1;
