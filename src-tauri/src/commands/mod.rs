//! Tauri command surface. All privileged work (file access, encryption, SQL)
//! happens here in Rust; the React frontend only invokes these typed commands
//! and never touches the filesystem or database directly.

pub mod reviewer;

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

/// The single open reviewer session (one assignment at a time).
pub struct Session {
    pub conn: Connection,
    pub path: PathBuf,
    pub reviewer_id: String,
}

/// Managed Tauri state. `Mutex` makes the (Send, !Sync) `Connection` shareable.
pub type SessionState = Mutex<Option<Session>>;
