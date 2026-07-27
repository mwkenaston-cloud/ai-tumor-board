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

The coordinator "Export analysis" writes **four files** next to the chosen path
(`NAME.json`, `NAME.csv`, `NAME_surveys.csv`, `NAME_events.csv`):

- **`NAME.json`** — the lossless archive. Top-level `records`, `surveys`,
  `events` (the flattened tables below) plus `raw_responses` (every imported
  `.atbr` payload verbatim, which itself contains every captured datapoint).
- **`NAME.csv`** — one row per reviewer × patient × recommendation.
- **`NAME_surveys.csv`** — long format, one row per survey answer.
- **`NAME_events.csv`** — the timestamped engagement timeline.

### Main CSV — column dictionary

| Column | Meaning |
|---|---|
| `reviewer_id` / `reviewer_name` / `specialty` | Reviewer identity + clinical specialty (from the coordinator roster). |
| `assignment_id` / `submitted_at` / `source_assignment_sha256` | Which package this response came from, when submitted, and the source `.atb` hash. |
| `app_version` / `schema_version` | Build + schema that produced the response. |
| `patient_id` / `research_id` / `model_id` / `cancer_type` / `clinical_question` | Case identity. |
| `patient_status` / `elapsed_seconds` | Completion state and total time on that patient. |
| `note_word_count` / `note_char_count` | Size of the final note. |
| `pct_physician_original` | % of note characters typed by the physician (their own blocks). |
| `pct_ai_unmodified` | % of note characters from AI text left **verbatim**. |
| `pct_ai_edited` | % of note characters from AI text the physician **changed** after inserting. |
| `pct_derived_from_llm` | `pct_ai_unmodified + pct_ai_edited`. |
| `chars_typed_by_physician` / `chars_from_llm_unmodified` / `chars_from_llm_edited` | Same split in raw characters. |
| `recommendation_id` / `title` | The recommendation this row is about. |
| `temperature_level` / `temperature_label` / `evidence_tier` / `risk_score` / `safety_score` / `priority_rank` | The AI's scores for that recommendation. |
| `disposition` | `accepted` (inserted), `dismissed`, or `ignored` (neither). |
| `status` | `used`, `used-and-edited`, `dismissed`, or `pending`. |
| `was_used` | 1 if accepted/inserted. |
| `was_altered` | 1 if inserted **and then edited** (`status = used-and-edited`). |
| `edit_distance` / `similarity_percent` | Character Levenshtein distance and % similarity between the AI original and the physician's final version of that recommendation. |
| `original_ai_text` | The recommendation text as the AI wrote it. |
| `final_text_in_note` | The physician's final version of that recommendation's block. |
| `dismissal_reason` / `decided_at` | Why dismissed (if given) and when the decision was made. |
| `final_note_text` | The full final note (repeated on each of the patient's rows). |

**Attribution vs. per-recommendation columns.** The `pct_*` / `chars_*` columns
are computed from note-block provenance (physician-authored vs AI-derived
blocks). The per-recommendation `status` / `edit_distance` / `similarity_percent`
/ `final_text_in_note` are **reconciled from the physician's final note at export
time**, so editing an inserted recommendation is reflected as `used-and-edited`
with a real edit distance — not the insert-time snapshot. One caveat inherent to
any provenance-based measure: if a physician retypes or pastes AI text into their
*own* block instead of using **Insert**, it counts as physician-original, because
the app tracks how text entered the note, not post-hoc string resemblance.

### `NAME_surveys.csv`

`reviewer_id, reviewer_name, specialty, assignment_id, scope, patient_id,
question_key, answer` — one row per answer. `scope` is `per_patient` (with a
`patient_id`) or `general`.

### `NAME_events.csv`

`reviewer_id, reviewer_name, specialty, assignment_id, patient_id, event_type,
event_time, payload_json` — the engagement trajectory: `PATIENT_OPENED/COMPLETED`,
`RECOMMENDATION_INSERTED/DISMISSED/REMOVED`, `TAB_VIEWED` (with per-tab dwell
seconds), `SURVEY_COMPLETED`, `ASSIGNMENT_SUBMITTED`, etc.

## Versioning note

Changing the DB schema or either container format requires bumping
`SCHEMA_VERSION` / `FORMAT_VERSION` and providing a migration/compat path before
distributing to machines that already hold data files.
