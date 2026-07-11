//! Encrypted database layer (SQLCipher via rusqlite).
//!
//! Every patient-containing database is a SQLCipher file: pages are encrypted
//! at rest with AES-256 + per-page HMAC, so no plaintext DB is ever written to
//! disk. The 256-bit key is derived from the reviewer password with Argon2id
//! (see `crate::crypto`) and handed to SQLCipher in raw-key mode.

pub mod connection;
pub mod llm_import;
pub mod metrics;
pub mod models;
pub mod packaging;
pub mod repository;
pub mod response;
pub mod seed;

#[cfg(test)]
mod integration_tests;

pub const SCHEMA_VERSION: i64 = 2;
