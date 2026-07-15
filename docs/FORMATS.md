# File formats: `.atb` (assignment) and `.atbr` (response)

Both hold patient-containing data and are **encrypted at rest**. No plaintext
database is ever written to disk.

## `.atb` — reviewer assignment

A single **SQLCipher** database (AES-256 pages + per-page HMAC-SHA512).

- **Key derivation:** the reviewer password → 256-bit key via **Argon2id**
  (64 MiB, 3 passes, 1 lane in format v1). The 16-byte random salt is stored in
  SQLCipher's plaintext header (readable before key derivation); it carries no
  PHI. KDF parameters are pinned by `FORMAT_VERSION` so older files keep opening.
- **Integrity:** on open, `PRAGMA cipher_integrity_check` HMAC-verifies every
  page. A wrong password, truncated file, or tampered ciphertext fails **without
  exposing any content**.
- **Contents** (schema in `src-tauri/migrations/001_initial.sql`): the study,
  the one reviewer, their assigned patients, source documents (text inline; small
  binaries as BLOBs), normalized recommendations, the reviewer's decisions, note
  blocks, survey responses, audit events, and `app_metadata` (`schema_version`,
  `format_version`, `assignment_id`, `coordinator_public_hex`).
- Built by the coordinator's package builder: validates each patient (research
  id, clinical question, ≥1 document, ≥1 recommendation), copies **only** the
  selected reviewer's patients into a fresh DB, reopens + integrity-checks, and
  reports a SHA-256.

## `.atbr` — reviewer response

`MAGIC("ATBR") | format_version(u16 LE) | header_len(u32 LE) | header(JSON) | SQLCipher-DB`

- The embedded DB is SQLCipher, keyed by a **random 256-bit DEK** generated per
  response.
- The DEK is **sealed to the coordinator's X25519 public key** (crypto_box
  sealed-box style: ephemeral keypair, no reviewer keypair or coordinator
  password needed). Only the coordinator's private key can open it. The sealed
  DEK lives (hex) in the plaintext header.
- The plaintext header carries **no PHI** — only `assignment_id`,
  `source_assignment_sha256`, `reviewer_id`, `app_version`, `schema_version`,
  `submitted_at`, and the sealed DEK.
- **Contents:** reviewer output only — patients (identifiers + status + timing),
  recommendation decisions (status, original/final text, edit distance,
  similarity), note blocks, surveys, and the timestamped audit trail. Source
  documents and raw LLM runs are intentionally excluded.
- Import verifies integrity, matches the parent assignment, **dedups** on
  (assignment_id, reviewer_id), and merges into the coordinator results store.

## LLM output format

Coordinator-imported AI JSON is validated against
`src-tauri/schemas/llm-output.schema.json` (prompt v1.0–v1.2). `recommendation_id`
may be integer or string; `patient_comorbidities` may be the v1.2 rich object
(CCI + treatment-relevant flags + summary) or the v1.0 array. Malformed JSON,
schema violations, out-of-range scores, and duplicate recommendation ids are
rejected. Phase-3 `recommendation_text` is shown; phase-6 uncertainty and
priority rationale surface as recommendation hover tooltips.

## Analysis export

The coordinator "Export analysis" writes:
- `*.json` — lossless: all raw response payloads plus a flat record per
  reviewer × patient × recommendation.
- `*.csv` — one row per reviewer × patient × recommendation, with the manuscript
  variables: elapsed time, final note text, original AI text, disposition
  (accepted/dismissed/ignored), `was_used`, `was_altered`, edit distance,
  similarity, authorship char/percent breakdown, scores, ids, timestamps. The
  timestamped engagement events (rec inserts/dismissals, tab dwell-time, patient
  start/complete, submit) are in the JSON for trajectory analysis.

## Versioning note

Changing the DB schema or either container format requires bumping
`SCHEMA_VERSION` / `FORMAT_VERSION` and providing a migration/compat path before
distributing to machines that already hold data files.
