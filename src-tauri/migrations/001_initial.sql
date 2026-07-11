-- AI Tumor Board — initial schema (schema_version = 1)
-- Runs inside an already-keyed SQLCipher connection. Rollback-journal mode
-- (not WAL) keeps the assignment as a single portable file across macOS/Windows.

PRAGMA foreign_keys = ON;
PRAGMA journal_mode = DELETE;

-- Key/value app + file metadata (schema_version, format_version, role, etc.)
CREATE TABLE app_metadata (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE studies (
    study_id         TEXT PRIMARY KEY,
    title            TEXT NOT NULL,
    protocol_version TEXT NOT NULL,
    schema_version   INTEGER NOT NULL,
    contact_email    TEXT,
    instructions     TEXT,
    -- studySettings JSON (identity/display/timing/survey/export/blinding toggles)
    settings_json    TEXT,
    created_at       TEXT NOT NULL,
    exported_at      TEXT
);

CREATE TABLE reviewers (
    reviewer_id       TEXT PRIMARY KEY,
    display_name      TEXT,
    role              TEXT,          -- 'reviewer' | 'coordinator'
    assignment_status TEXT NOT NULL DEFAULT 'not_started'
                      -- 'not_started'|'ready'|'in_progress'|'partially_complete'|'complete'|'submitted'
);

CREATE TABLE patients (
    patient_id        TEXT PRIMARY KEY,
    study_id          TEXT NOT NULL,
    research_id       TEXT,
    -- model_id identifies which AI model/config produced this case's output.
    model_id          TEXT,
    display_label     TEXT NOT NULL,   -- shown label (defaults to model_id)
    clinical_question TEXT,
    cancer_type       TEXT,            -- from LLM session_metadata
    -- Phase-1 patient context (patient_profile, timeline, comorbidities,
    -- family history/genetics) as a JSON blob, populated on LLM import.
    context_json      TEXT,
    -- Phase-2 framing (decision_points, specialist_perspectives) as JSON.
    framing_json      TEXT,
    position          INTEGER NOT NULL,
    status            TEXT NOT NULL DEFAULT 'not_started',
                      -- 'not_started'|'in_progress'|'complete'|'reopened'
    started_at        TEXT,
    completed_at      TEXT,
    elapsed_seconds   INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (study_id) REFERENCES studies(study_id)
);

CREATE TABLE reviewer_assignments (
    reviewer_id TEXT NOT NULL,
    patient_id  TEXT NOT NULL,
    position    INTEGER NOT NULL,
    PRIMARY KEY (reviewer_id, patient_id),
    FOREIGN KEY (reviewer_id) REFERENCES reviewers(reviewer_id),
    FOREIGN KEY (patient_id)  REFERENCES patients(patient_id)
);

-- Source documents: prototype files.{notes,pathology,imaging,labs}.
-- Text stored inline; modest PDFs/images as encrypted BLOBs (size-limited).
CREATE TABLE source_documents (
    document_id    TEXT PRIMARY KEY,
    patient_id     TEXT NOT NULL,
    document_type  TEXT NOT NULL,   -- 'notes'|'pathology'|'imaging'|'labs'|...
    filename       TEXT,
    mime_type      TEXT,
    text_content   TEXT,
    binary_content BLOB,
    byte_size      INTEGER,
    sha256         TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    FOREIGN KEY (patient_id) REFERENCES patients(patient_id)
);

-- Raw multi-phase LLM import (phase3_recommendations .. phase6_synthesis),
-- preserved verbatim for provenance.
CREATE TABLE llm_runs (
    llm_run_id     TEXT PRIMARY KEY,
    patient_id     TEXT NOT NULL,
    model_name     TEXT,
    prompt_version TEXT,
    raw_json       TEXT NOT NULL,
    imported_at    TEXT NOT NULL,
    FOREIGN KEY (patient_id) REFERENCES patients(patient_id)
);

-- Normalized recommendations (merged across phases by recommendation_id).
CREATE TABLE recommendations (
    recommendation_id  TEXT PRIMARY KEY,
    patient_id         TEXT NOT NULL,
    llm_run_id         TEXT,
    position           INTEGER NOT NULL,
    priority_rank      INTEGER,
    temperature_level  INTEGER,
    temperature_label  TEXT,
    evidence_tier      TEXT,
    risk_score         REAL,
    safety_score       REAL,
    title              TEXT,
    recommendation_text TEXT NOT NULL,  -- condensed text (phase5 preferred over phase3)
    full_text          TEXT,
    rationale          TEXT,
    -- contraindications / drug_interactions / adverse_events / monitoring /
    -- uncertainty / bias flags / conflicts, kept as a JSON blob
    metadata_json      TEXT,
    is_custom          INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (patient_id) REFERENCES patients(patient_id),
    FOREIGN KEY (llm_run_id) REFERENCES llm_runs(llm_run_id)
);

CREATE TABLE recommendation_decisions (
    decision_id             TEXT PRIMARY KEY,
    recommendation_id       TEXT NOT NULL,
    reviewer_id             TEXT NOT NULL,
    status                  TEXT NOT NULL,  -- 'pending'|'used'|'used-and-edited'|'dismissed'
    original_text           TEXT,
    final_text              TEXT,
    edit_distance           INTEGER,
    similarity_percent      REAL,
    decision_elapsed_seconds INTEGER,
    dismissal_reason        TEXT,
    decided_at              TEXT,
    FOREIGN KEY (recommendation_id) REFERENCES recommendations(recommendation_id),
    FOREIGN KEY (reviewer_id)       REFERENCES reviewers(reviewer_id)
);

-- Block-based physician note. Physician-authored ('user') vs AI-derived ('ai')
-- blocks stay structurally distinct; AI blocks keep immutable original_text.
CREATE TABLE note_blocks (
    block_id          TEXT PRIMARY KEY,
    patient_id        TEXT NOT NULL,
    reviewer_id       TEXT NOT NULL,
    position          INTEGER NOT NULL,
    block_type        TEXT NOT NULL,   -- 'user' | 'ai'
    recommendation_id TEXT,
    original_text     TEXT,            -- immutable for 'ai' blocks
    current_text      TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    FOREIGN KEY (patient_id)  REFERENCES patients(patient_id),
    FOREIGN KEY (reviewer_id) REFERENCES reviewers(reviewer_id)
);

-- Per-recommendation (surveyData.perRec) and general (surveyData.general) surveys.
CREATE TABLE survey_responses (
    response_id       TEXT PRIMARY KEY,
    reviewer_id       TEXT NOT NULL,
    patient_id        TEXT,
    recommendation_id TEXT,
    question_id       TEXT NOT NULL,
    response_json     TEXT NOT NULL,
    created_at        TEXT NOT NULL
);

-- Meaningful state transitions only (no keystroke telemetry).
CREATE TABLE audit_events (
    event_id     TEXT PRIMARY KEY,
    reviewer_id  TEXT,
    patient_id   TEXT,
    event_type   TEXT NOT NULL,
    event_time   TEXT NOT NULL,
    payload_json TEXT
);

CREATE INDEX idx_documents_patient      ON source_documents(patient_id);
CREATE INDEX idx_recommendations_patient ON recommendations(patient_id);
CREATE INDEX idx_blocks_patient_reviewer ON note_blocks(patient_id, reviewer_id, position);
CREATE INDEX idx_audit_patient          ON audit_events(patient_id, event_time);
