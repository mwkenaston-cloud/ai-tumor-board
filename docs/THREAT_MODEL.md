# Threat model

Scope: a fully offline desktop app that distributes de-identified (or, if the
protocol permits, PHI) tumor-board cases to reviewers and collects their
responses. This document supports institutional / IRB / security review. It
describes what the app defends against and what it explicitly relies on the
institution to provide.

## Assets
- Patient case data inside `.atb` assignments.
- Reviewer responses inside `.atbr` files.
- The coordinator workspace (`coordinator-workspace-v2.atb`) and its X25519
  private key.
- Reviewer/assignment passwords.

## Trust boundaries
- **Instruction boundary:** the app takes instructions only from the local user.
  Imported content (documents, LLM JSON) is data, never executed; it is
  schema-validated and rendered through React escaping (no `innerHTML` of
  untrusted content).
- **Role boundary:** coordinator capabilities are gated by an Ed25519 credential
  verified against a compiled-in authority public key. Reviewers hold no key to
  any coordinator workspace, so the boundary is cryptographic, not a UI toggle.
  *Current state:* the authority key is the all-zero placeholder → the build runs
  in development mode (coordinator unrestricted). Provision the real key for
  production.

## What the app defends against
- **Data at rest:** SQLCipher (AES-256 + per-page HMAC). No plaintext DB on disk.
- **Wrong password / tampering:** page-1 keying rejects a wrong password;
  `cipher_integrity_check` catches tampering/corruption anywhere in the file.
  Neither exposes partial content.
- **Response confidentiality:** `.atbr` DEK is sealed to the coordinator's
  X25519 key; a response sealed to one coordinator cannot be opened by another
  (tested).
- **Response integrity/provenance:** each `.atbr` binds `assignment_id` and the
  source `.atb` SHA-256; import dedups on (assignment, reviewer).
- **Network exposure:** the app makes no network requests. Strict CSP blocks
  external `connect-src`/`img-src`/`frame-src`; fonts bundled; no analytics,
  telemetry, CDN, or LLM calls. Frontend has no filesystem access — all file/DB
  work is in narrowly-scoped Rust commands (Tauri capabilities: dialog + opener
  only).
- **Crash durability:** rollback-journal mode + atomic writes; force-kill loses
  at most the last debounced change.
- **PHI hygiene in the app:** no PHI in filenames, the plaintext container
  header, temp working files (they are SQLCipher-encrypted), or app logs.

## What the app does NOT solve (institutional responsibility)
- **Whether real PHI is appropriate** — use research IDs / de-identified content
  whenever the protocol allows. HIPAA compliance is a workflow property, not a
  code property.
- **Endpoint security** — full-disk encryption (FileVault/BitLocker), device
  management, screen lock, malware protection on both coordinator and reviewer
  machines.
- **Password delivery** — send `.atb` and its password through **separate**
  approved channels; never in the same email.
- **Email/transport of files** — recipient verification, secure mail, retention.
- **Password recovery policy** — decide **no recovery** (forgotten password ⇒
  file inaccessible) vs **coordinator key escrow** (seal a second copy of the
  key to a coordinator public key). Escrow is safer operationally; not yet
  implemented.
- **Key management** — safeguarding the coordinator X25519 private key and the
  study-authority Ed25519 signing key; loss/compromise of either is
  unrecoverable / breaks the boundary.
- **Code-signing / notarization** — required so users can trust and open the
  installer (see BUILD.md); unsigned builds are a distribution/trust risk.
- **Audit granularity** — the trajectory captures discrete actions with
  timestamps (rec decisions, tab dwell-time, start/end, submit), not
  keystroke-level free-text timing.

## Residual risks to review
- Development builds are coordinator-unrestricted until the authority key is
  provisioned.
- Argon2id parameters are conservative defaults pending security review.
- A malicious reviewer can still fabricate their own responses (inherent to any
  take-home instrument); provenance binding limits misattribution, not authoring.
