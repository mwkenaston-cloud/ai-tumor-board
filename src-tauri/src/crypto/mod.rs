//! Cryptographic core for the AI Tumor Board app.
//!
//! - `key_derivation`: Argon2id password -> 256-bit key.
//! - `container`:       tie a password to a SQLCipher `.atb`/`.atbr` file.
//!
//! Design (v1 container format, subject to institutional security review — see
//! docs/ATB_FORMAT.md and the "Open items" in the plan):
//!   * The `.atb`/`.atbr` file is a bona fide SQLCipher database — pages are
//!     encrypted at rest; no plaintext DB ever hits disk.
//!   * The 256-bit page key = Argon2id(password, salt, PARAMS_V1).
//!   * The 16-byte `salt` is stored in SQLCipher's own plaintext header salt
//!     (first 16 bytes of the file) via raw-key-with-salt mode, so it can be
//!     read back before the key is derived. It carries no PHI.
//!   * KDF parameters are pinned by `FORMAT_VERSION`; older files remain
//!     openable by consulting the historical parameter table.

pub mod container;
pub mod key_derivation;

/// Bumped only when the on-disk KDF/cipher parameters change.
pub const FORMAT_VERSION: u16 = 1;
