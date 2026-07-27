//! `.atbr` response files: reviewer output only, encrypted with a random DEK
//! that is sealed to the coordinator's X25519 public key (no reviewer keypair
//! or coordinator password needed to produce one). Plus the coordinator-side
//! import/verify/dedup/merge.
//!
//! On-disk layout:
//!   MAGIC(4) || format_version(u16 LE) || header_len(u32 LE) || header(JSON) || SQLCipher-DB
//! The header is plaintext but carries no PHI — only study/reviewer identifiers,
//! the source-assignment hash, versions, timestamp, and the sealed DEK.
//!
//! The import/merge side is command-wired with coordinator mode; it is fully
//! exercised now by unit tests.
#![allow(dead_code)]

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use super::connection::{self as dbc, DbError};
use super::metrics::{self, AttributionMetrics};
use super::models::{NoteBlock, RecommendationDecision};
use super::repository as repo;
use crate::crypto::response_seal;

const MAGIC: &[u8; 4] = b"ATBR";
pub const FORMAT_VERSION: u16 = 1;
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const SCHEMA_VERSION: i64 = super::SCHEMA_VERSION;

#[derive(Debug, thiserror::Error)]
pub enum ResponseError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("seal/unseal failed: {0}")]
    Seal(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("malformed response file")]
    Malformed,
    #[error("unsupported response format version: {0}")]
    UnsupportedVersion(u16),
    #[error("this response was already imported")]
    Duplicate,
}

impl From<std::io::Error> for ResponseError {
    fn from(e: std::io::Error) -> Self {
        ResponseError::Io(e.to_string())
    }
}

impl From<rusqlite::Error> for ResponseError {
    fn from(e: rusqlite::Error) -> Self {
        ResponseError::Db(DbError::from(e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseHeader {
    pub format: String,
    pub format_version: u16,
    pub assignment_id: String,
    pub source_assignment_sha256: String,
    pub reviewer_id: String,
    pub app_version: String,
    pub schema_version: i64,
    pub submitted_at: String,
    /// Hex-encoded DEK sealed to the coordinator's X25519 public key.
    pub sealed_dek: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseReceipt {
    pub sha256: String,
    pub reviewer_id: String,
    pub patient_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatientResponse {
    pub patient_id: String,
    pub research_id: Option<String>,
    pub status: String,
    pub elapsed_seconds: i64,
    pub final_text: String,
    pub attribution: AttributionMetrics,
    pub decisions: Vec<RecommendationDecision>,
    pub note_blocks: Vec<NoteBlock>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedResponse {
    pub header: ResponseHeader,
    /// Reviewer identity for analysis (id + human name + clinical specialty).
    pub reviewer_name: Option<String>,
    pub reviewer_specialty: Option<String>,
    pub patients: Vec<PatientResponse>,
    pub surveys: serde_json::Value,
    pub audit_count: i64,
    /// Timestamped engagement trajectory (rec inserts/dismissals, tab views,
    /// patient start/complete, submit) for analysis.
    pub events: serde_json::Value,
}

// ── Build (reviewer side) ─────────────────────────────────────────────────

/// Assemble a `.atbr` from the reviewer's live session DB. `coordinator_public`
/// is the X25519 key embedded in the assignment package.
pub fn build_response(
    session: &Connection,
    session_path: &Path,
    out_path: &Path,
    assignment_id: &str,
    coordinator_public: &[u8; 32],
) -> Result<ResponseReceipt, ResponseError> {
    let reviewer_id = repo::first_reviewer_id(session)?;
    // Bring each accepted recommendation's decision row into agreement with the
    // reviewer's final edited note before we freeze it into the response, so the
    // per-recommendation columns (status, edit_distance, similarity, final_text)
    // reflect edits made after insertion — not just the insert-time snapshot.
    reconcile_decisions_from_blocks(session, &reviewer_id)?;
    let source_sha = sha256_file(session_path)?;

    // Random data-encryption key for this response.
    let mut dek = [0u8; 32];
    rand::rngs::OsRng.fill_bytes_compat(&mut dek);

    // Build the encrypted response DB in a temp file (SQLCipher-encrypted with
    // the DEK — never plaintext on disk), then fold its bytes into the container.
    let tmp = out_path.with_extension("atbr.tmp");
    let patient_count = {
        let resp = dbc::open_encrypted(&tmp, &dek)?;
        dbc::initialize_schema(&resp)?;
        copy_reviewer_output(session, &resp, &reviewer_id)?
    };
    let db_bytes = std::fs::read(&tmp)?;
    let _ = std::fs::remove_file(&tmp);

    let sealed = response_seal::seal_to(coordinator_public, &dek)
        .map_err(|e| ResponseError::Seal(e.to_string()))?;
    dek.zeroize();

    let header = ResponseHeader {
        format: "AI_TUMOR_BOARD_RESPONSE".into(),
        format_version: FORMAT_VERSION,
        assignment_id: assignment_id.to_string(),
        source_assignment_sha256: source_sha,
        reviewer_id: reviewer_id.clone(),
        app_version: APP_VERSION.to_string(),
        schema_version: SCHEMA_VERSION,
        submitted_at: repo::now_iso(),
        sealed_dek: hex_encode(&sealed),
    };
    write_container(out_path, &header, &db_bytes)?;

    Ok(ResponseReceipt {
        sha256: sha256_file(out_path)?,
        reviewer_id,
        patient_count,
    })
}

// ── Import (coordinator side) ─────────────────────────────────────────────

/// Open and verify a `.atbr` with the coordinator's X25519 secret key. A wrong
/// key, tampered blob, or unsupported version fails without exposing content.
pub fn import_response(
    atbr_path: &Path,
    coordinator_secret: &[u8; 32],
) -> Result<ImportedResponse, ResponseError> {
    let bytes = std::fs::read(atbr_path)?;
    let (header, db_bytes) = parse_container(&bytes)?;
    if header.format_version != FORMAT_VERSION {
        return Err(ResponseError::UnsupportedVersion(header.format_version));
    }

    let sealed = hex_decode(&header.sealed_dek).ok_or(ResponseError::Malformed)?;
    let mut dek_vec = response_seal::unseal(coordinator_secret, &sealed)
        .map_err(|e| ResponseError::Seal(e.to_string()))?;
    let dek: [u8; 32] = dek_vec.as_slice().try_into().map_err(|_| ResponseError::Malformed)?;

    let tmp = atbr_path.with_extension("atbr.import.tmp");
    std::fs::write(&tmp, &db_bytes)?;
    let result = (|| {
        let conn = dbc::open_encrypted(&tmp, &dek)?;
        dbc::verify_integrity(&conn)?;
        read_payload(&conn, &header)
    })();
    let _ = std::fs::remove_file(&tmp);
    dek_vec.zeroize();

    result
}

fn read_payload(conn: &Connection, header: &ResponseHeader) -> Result<ImportedResponse, ResponseError> {
    let reviewer_id = &header.reviewer_id;
    let (reviewer_name, reviewer_specialty): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT display_name, specialty FROM reviewers WHERE reviewer_id = ?1",
            [reviewer_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((None, None));
    let asg = repo::load_assignment(conn, reviewer_id)?;

    let mut patients = Vec::new();
    for summary in &asg.patients {
        let p = repo::load_patient(conn, reviewer_id, &summary.id)?;
        let attribution = metrics::attribution_metrics(&p.note_blocks);
        let final_text = metrics::derive_note_text(&p.note_blocks);
        patients.push(PatientResponse {
            patient_id: p.id,
            research_id: p.research_id,
            status: p.status,
            elapsed_seconds: p.elapsed_seconds,
            final_text,
            attribution,
            decisions: p.decisions,
            note_blocks: p.note_blocks,
        });
    }

    let mut stmt = conn.prepare(
        "SELECT patient_id, question_id, response_json FROM survey_responses ORDER BY created_at",
    )?;
    let surveys: Vec<serde_json::Value> = stmt
        .query_map([], |r| {
            let pid: Option<String> = r.get(0)?;
            let qid: String = r.get(1)?;
            let resp: String = r.get(2)?;
            Ok(serde_json::json!({
                "patientId": pid,
                "questionId": qid,
                "response": serde_json::from_str::<serde_json::Value>(&resp).unwrap_or(serde_json::Value::String(resp)),
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let audit_count: i64 = conn.query_row("SELECT count(*) FROM audit_events", [], |r| r.get(0))?;

    let mut estmt = conn.prepare(
        "SELECT event_type, patient_id, event_time, payload_json FROM audit_events ORDER BY event_time",
    )?;
    let events: Vec<serde_json::Value> = estmt
        .query_map([], |r| {
            let etype: String = r.get(0)?;
            let pid: Option<String> = r.get(1)?;
            let etime: String = r.get(2)?;
            let payload: Option<String> = r.get(3)?;
            Ok(serde_json::json!({
                "eventType": etype,
                "patientId": pid,
                "eventTime": etime,
                "payload": payload.and_then(|p| serde_json::from_str::<serde_json::Value>(&p).ok()),
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ImportedResponse {
        header: header.clone(),
        reviewer_name,
        reviewer_specialty,
        patients,
        surveys: serde_json::Value::Array(surveys),
        audit_count,
        events: serde_json::Value::Array(events),
    })
}

/// Verify + dedup + merge an imported response into a coordinator results DB.
/// Rejects a second import of the same (assignment_id, reviewer_id) and
/// preserves the payload verbatim as JSON.
pub fn merge_into_results(
    results: &Connection,
    imported: &ImportedResponse,
) -> Result<(), ResponseError> {
    ensure_results_tables(results)?;
    let h = &imported.header;
    let exists: i64 = results.query_row(
        "SELECT count(*) FROM imported_responses WHERE assignment_id = ?1 AND reviewer_id = ?2",
        params![h.assignment_id, h.reviewer_id],
        |r| r.get(0),
    )?;
    if exists > 0 {
        return Err(ResponseError::Duplicate);
    }
    results.execute(
        "INSERT INTO imported_responses(assignment_id, reviewer_id, source_sha256, submitted_at, imported_at)
         VALUES (?1,?2,?3,?4,?5)",
        params![h.assignment_id, h.reviewer_id, h.source_assignment_sha256, h.submitted_at, repo::now_iso()],
    )?;
    let payload = serde_json::to_string(imported).unwrap_or_default();
    results.execute(
        "INSERT INTO response_payloads(assignment_id, reviewer_id, payload_json) VALUES (?1,?2,?3)",
        params![h.assignment_id, h.reviewer_id, payload],
    )?;
    Ok(())
}

fn ensure_results_tables(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS imported_responses (
             assignment_id TEXT NOT NULL, reviewer_id TEXT NOT NULL,
             source_sha256 TEXT, submitted_at TEXT, imported_at TEXT NOT NULL,
             PRIMARY KEY (assignment_id, reviewer_id));
         CREATE TABLE IF NOT EXISTS response_payloads (
             assignment_id TEXT NOT NULL, reviewer_id TEXT NOT NULL, payload_json TEXT NOT NULL,
             PRIMARY KEY (assignment_id, reviewer_id));",
    )?;
    Ok(())
}

/// Recompute accepted-recommendation decisions from the reviewer's final note
/// blocks. For every AI block still present in the note, the matching decision
/// is set to `used` (verbatim) or `used-and-edited` (changed), with a fresh
/// edit distance, similarity, and final text. Dismissed and untouched
/// recommendations are left as they are.
fn reconcile_decisions_from_blocks(conn: &Connection, reviewer_id: &str) -> Result<(), ResponseError> {
    // (recommendation_id, original_text, current_text) for this reviewer's AI blocks.
    let mut stmt = conn.prepare(
        "SELECT recommendation_id, COALESCE(original_text, ''), current_text
         FROM note_blocks
         WHERE reviewer_id = ?1 AND block_type = 'ai' AND recommendation_id IS NOT NULL",
    )?;
    let blocks: Vec<(String, String, String)> = stmt
        .query_map([reviewer_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;

    for (rec_id, original, current) in blocks {
        let edited = current.trim() != original.trim();
        let status = if edited { "used-and-edited" } else { "used" };
        let dist = metrics::levenshtein(&original, &current) as i64;
        let sim = metrics::similarity_percent(&original, &current);
        // Update the existing decision if present; otherwise record one so an
        // edited-but-never-formally-decided block still appears in the export.
        let changed = conn.execute(
            "UPDATE recommendation_decisions
             SET status = ?1, edit_distance = ?2, similarity_percent = ?3, final_text = ?4
             WHERE reviewer_id = ?5 AND recommendation_id = ?6",
            params![status, dist, sim, current, reviewer_id, rec_id],
        )?;
        if changed == 0 {
            conn.execute(
                "INSERT INTO recommendation_decisions
                   (decision_id, recommendation_id, reviewer_id, status, original_text, final_text, edit_distance, similarity_percent, decided_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    format!("{reviewer_id}:{rec_id}"), rec_id, reviewer_id, status,
                    original, current, dist, sim, repo::now_iso()
                ],
            )?;
        }
    }
    Ok(())
}

// ── Reviewer-output copy (build side) ─────────────────────────────────────

/// Copy the reviewer's slice into the response DB: study, reviewer, assignments,
/// patients, recommendations, decisions, note blocks, surveys, audit. Source
/// documents and raw LLM runs are intentionally excluded from the response.
fn copy_reviewer_output(
    src: &Connection,
    dst: &Connection,
    reviewer_id: &str,
) -> Result<usize, ResponseError> {
    // study
    let study = src.query_row(
        "SELECT study_id, title, protocol_version, schema_version, contact_email, instructions, settings_json, created_at FROM studies LIMIT 1",
        [],
        |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,i64>(3)?, r.get::<_,Option<String>>(4)?, r.get::<_,Option<String>>(5)?, r.get::<_,Option<String>>(6)?, r.get::<_,String>(7)?)),
    ).map_err(DbError::from)?;
    dst.execute(
        "INSERT OR REPLACE INTO studies(study_id,title,protocol_version,schema_version,contact_email,instructions,settings_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![study.0, study.1, study.2, study.3, study.4, study.5, study.6, study.7],
    ).map_err(DbError::from)?;

    // reviewer
    let rev = src.query_row(
        "SELECT display_name, specialty, role, assignment_status FROM reviewers WHERE reviewer_id = ?1",
        [reviewer_id],
        |r| Ok((r.get::<_,Option<String>>(0)?, r.get::<_,Option<String>>(1)?, r.get::<_,Option<String>>(2)?, r.get::<_,String>(3)?)),
    ).map_err(DbError::from)?;
    dst.execute(
        "INSERT OR REPLACE INTO reviewers(reviewer_id,display_name,specialty,role,assignment_status) VALUES (?1,?2,?3,?4,?5)",
        params![reviewer_id, rev.0, rev.1, rev.2, rev.3],
    ).map_err(DbError::from)?;

    // assigned patient ids
    let mut ps = src.prepare("SELECT patient_id, position FROM reviewer_assignments WHERE reviewer_id = ?1 ORDER BY position").map_err(DbError::from)?;
    let assigned: Vec<(String, i64)> = ps.query_map([reviewer_id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(DbError::from)?.collect::<Result<_,_>>().map_err(DbError::from)?;

    for (pid, pos) in &assigned {
        let p = src.query_row(
            "SELECT study_id, research_id, model_id, display_label, clinical_question, cancer_type, status, started_at, completed_at, elapsed_seconds FROM patients WHERE patient_id = ?1",
            [pid],
            |r| Ok((r.get::<_,String>(0)?, r.get::<_,Option<String>>(1)?, r.get::<_,Option<String>>(2)?, r.get::<_,String>(3)?, r.get::<_,Option<String>>(4)?, r.get::<_,Option<String>>(5)?, r.get::<_,String>(6)?, r.get::<_,Option<String>>(7)?, r.get::<_,Option<String>>(8)?, r.get::<_,i64>(9)?)),
        ).map_err(DbError::from)?;
        dst.execute(
            "INSERT OR REPLACE INTO patients(patient_id,study_id,research_id,model_id,display_label,clinical_question,cancer_type,position,status,started_at,completed_at,elapsed_seconds) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![pid, p.0, p.1, p.2, p.3, p.4, p.5, pos, p.6, p.7, p.8, p.9],
        ).map_err(DbError::from)?;
        dst.execute(
            "INSERT OR REPLACE INTO reviewer_assignments(reviewer_id,patient_id,position) VALUES (?1,?2,?3)",
            params![reviewer_id, pid, pos],
        ).map_err(DbError::from)?;

        // recommendations (kept so decisions' FK resolves and coordinator can diff)
        copy_recommendations(src, dst, pid)?;
    }

    // decisions for this reviewer
    dst_copy_decisions(src, dst, reviewer_id)?;
    // note blocks for this reviewer
    dst_copy_blocks(src, dst, reviewer_id)?;
    // surveys + audit for this reviewer
    dst_copy_surveys_audit(src, dst, reviewer_id)?;

    Ok(assigned.len())
}

/// Copy `recommendations` rows for one patient (15 columns selected + patient_id).
fn copy_recommendations(src: &Connection, dst: &Connection, patient_id: &str) -> Result<(), ResponseError> {
    let mut s = src.prepare(
        "SELECT recommendation_id, llm_run_id, position, priority_rank, temperature_level, temperature_label,
                evidence_tier, risk_score, safety_score, title, recommendation_text, full_text, rationale, metadata_json, is_custom
         FROM recommendations WHERE patient_id = ?1",
    ).map_err(DbError::from)?;
    let rows = s.query_map([patient_id], |r| {
        Ok((
            r.get::<_,String>(0)?, r.get::<_,Option<String>>(1)?, r.get::<_,i64>(2)?, r.get::<_,Option<i64>>(3)?, r.get::<_,Option<i64>>(4)?,
            r.get::<_,Option<String>>(5)?, r.get::<_,Option<String>>(6)?, r.get::<_,Option<f64>>(7)?, r.get::<_,Option<f64>>(8)?, r.get::<_,Option<String>>(9)?,
            r.get::<_,String>(10)?, r.get::<_,Option<String>>(11)?, r.get::<_,Option<String>>(12)?, r.get::<_,Option<String>>(13)?, r.get::<_,i64>(14)?,
        ))
    }).map_err(DbError::from)?;
    for row in rows {
        let d = row.map_err(DbError::from)?;
        // llm_run_id is nulled: the response deliberately omits the raw LLM runs
        // (source material), so keep no dangling FK to a run that isn't copied.
        let _ = &d.1;
        dst.execute(
            "INSERT OR REPLACE INTO recommendations(recommendation_id,patient_id,llm_run_id,position,priority_rank,temperature_level,temperature_label,evidence_tier,risk_score,safety_score,title,recommendation_text,full_text,rationale,metadata_json,is_custom)
             VALUES (?1,?2,NULL,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![d.0, patient_id, d.2, d.3, d.4, d.5, d.6, d.7, d.8, d.9, d.10, d.11, d.12, d.13, d.14],
        ).map_err(DbError::from)?;
    }
    Ok(())
}

fn dst_copy_decisions(src: &Connection, dst: &Connection, reviewer_id: &str) -> Result<(), ResponseError> {
    let mut s = src.prepare("SELECT decision_id, recommendation_id, status, original_text, final_text, edit_distance, similarity_percent, decision_elapsed_seconds, dismissal_reason, decided_at FROM recommendation_decisions WHERE reviewer_id = ?1").map_err(DbError::from)?;
    let rows = s.query_map([reviewer_id], |r| Ok((
        r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,Option<String>>(3)?, r.get::<_,Option<String>>(4)?,
        r.get::<_,Option<i64>>(5)?, r.get::<_,Option<f64>>(6)?, r.get::<_,Option<i64>>(7)?, r.get::<_,Option<String>>(8)?, r.get::<_,Option<String>>(9)?,
    ))).map_err(DbError::from)?;
    for row in rows {
        let d = row.map_err(DbError::from)?;
        dst.execute(
            "INSERT OR REPLACE INTO recommendation_decisions(decision_id,recommendation_id,reviewer_id,status,original_text,final_text,edit_distance,similarity_percent,decision_elapsed_seconds,dismissal_reason,decided_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![d.0, d.1, reviewer_id, d.2, d.3, d.4, d.5, d.6, d.7, d.8, d.9],
        ).map_err(DbError::from)?;
    }
    Ok(())
}

fn dst_copy_blocks(src: &Connection, dst: &Connection, reviewer_id: &str) -> Result<(), ResponseError> {
    let mut s = src.prepare("SELECT block_id, patient_id, position, block_type, recommendation_id, original_text, current_text, created_at, updated_at FROM note_blocks WHERE reviewer_id = ?1").map_err(DbError::from)?;
    let rows = s.query_map([reviewer_id], |r| Ok((
        r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,i64>(2)?, r.get::<_,String>(3)?, r.get::<_,Option<String>>(4)?,
        r.get::<_,Option<String>>(5)?, r.get::<_,String>(6)?, r.get::<_,String>(7)?, r.get::<_,String>(8)?,
    ))).map_err(DbError::from)?;
    for row in rows {
        let b = row.map_err(DbError::from)?;
        dst.execute(
            "INSERT OR REPLACE INTO note_blocks(block_id,patient_id,reviewer_id,position,block_type,recommendation_id,original_text,current_text,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![b.0, b.1, reviewer_id, b.2, b.3, b.4, b.5, b.6, b.7, b.8],
        ).map_err(DbError::from)?;
    }
    Ok(())
}

fn dst_copy_surveys_audit(src: &Connection, dst: &Connection, reviewer_id: &str) -> Result<(), ResponseError> {
    let mut s = src.prepare("SELECT response_id, patient_id, recommendation_id, question_id, response_json, created_at FROM survey_responses WHERE reviewer_id = ?1").map_err(DbError::from)?;
    let rows = s.query_map([reviewer_id], |r| Ok((
        r.get::<_,String>(0)?, r.get::<_,Option<String>>(1)?, r.get::<_,Option<String>>(2)?, r.get::<_,String>(3)?, r.get::<_,String>(4)?, r.get::<_,String>(5)?,
    ))).map_err(DbError::from)?;
    for row in rows {
        let v = row.map_err(DbError::from)?;
        dst.execute("INSERT OR REPLACE INTO survey_responses(response_id,reviewer_id,patient_id,recommendation_id,question_id,response_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![v.0, reviewer_id, v.1, v.2, v.3, v.4, v.5]).map_err(DbError::from)?;
    }
    let mut a = src.prepare("SELECT event_id, patient_id, event_type, event_time, payload_json FROM audit_events WHERE reviewer_id = ?1").map_err(DbError::from)?;
    let arows = a.query_map([reviewer_id], |r| Ok((
        r.get::<_,String>(0)?, r.get::<_,Option<String>>(1)?, r.get::<_,String>(2)?, r.get::<_,String>(3)?, r.get::<_,Option<String>>(4)?,
    ))).map_err(DbError::from)?;
    for row in arows {
        let e = row.map_err(DbError::from)?;
        dst.execute("INSERT OR REPLACE INTO audit_events(event_id,reviewer_id,patient_id,event_type,event_time,payload_json) VALUES (?1,?2,?3,?4,?5,?6)",
            params![e.0, reviewer_id, e.1, e.2, e.3, e.4]).map_err(DbError::from)?;
    }
    Ok(())
}

// ── Container (de)serialization ───────────────────────────────────────────

fn write_container(path: &Path, header: &ResponseHeader, db_bytes: &[u8]) -> Result<(), ResponseError> {
    let header_json = serde_json::to_vec(header).map_err(|_| ResponseError::Malformed)?;
    let mut out = Vec::with_capacity(4 + 2 + 4 + header_json.len() + db_bytes.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&(header_json.len() as u32).to_le_bytes());
    out.extend_from_slice(&header_json);
    out.extend_from_slice(db_bytes);
    std::fs::write(path, out)?;
    Ok(())
}

fn parse_container(bytes: &[u8]) -> Result<(ResponseHeader, Vec<u8>), ResponseError> {
    if bytes.len() < 10 || &bytes[0..4] != MAGIC {
        return Err(ResponseError::Malformed);
    }
    let header_len = u32::from_le_bytes(bytes[6..10].try_into().unwrap()) as usize;
    let header_start: usize = 10;
    let header_end = header_start.checked_add(header_len).ok_or(ResponseError::Malformed)?;
    if bytes.len() < header_end {
        return Err(ResponseError::Malformed);
    }
    let header: ResponseHeader =
        serde_json::from_slice(&bytes[header_start..header_end]).map_err(|_| ResponseError::Malformed)?;
    let db_bytes = bytes[header_end..].to_vec();
    Ok((header, db_bytes))
}

// ── small helpers ─────────────────────────────────────────────────────────

fn sha256_file(path: &Path) -> Result<String, ResponseError> {
    let bytes = std::fs::read(path)?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(hex_encode(&h.finalize()))
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{:02x}", b);
    }
    s
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

/// Small shim so we can call `fill_bytes` without importing the RngCore trait at
/// every call site.
trait FillCompat {
    fn fill_bytes_compat(&mut self, dest: &mut [u8]);
}
impl FillCompat for rand::rngs::OsRng {
    fn fill_bytes_compat(&mut self, dest: &mut [u8]) {
        use rand::RngCore;
        self.fill_bytes(dest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::container::{self, ContainerRole};
    use crate::crypto::response_seal;
    use crate::db::{repository as repo, seed};

    fn tempfile(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        p.push(format!("atbr-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p.join(name)
    }

    /// Seed a session, do some reviewer work, and return (path, connection).
    fn worked_session() -> (std::path::PathBuf, Connection) {
        let path = tempfile("assignment.atb");
        let mut conn = container::create(&path, "pw", ContainerRole::Reviewer).unwrap();
        seed::seed_demo(&conn, "REV-1").unwrap();
        let p = repo::load_patient(&conn, "REV-1", "PT-1").unwrap();
        let rec_id = p.recommendations[0].id.clone();
        let blocks = vec![
            NoteBlock { id: "b1".into(), block_type: "user".into(), recommendation_id: None, original_text: None, current_text: "Physician plan here.".into(), position: 0 },
            NoteBlock { id: "b2".into(), block_type: "ai".into(), recommendation_id: Some(rec_id.clone()), original_text: Some("orig".into()), current_text: "orig".into(), position: 1 },
        ];
        repo::save_note_blocks(&mut conn, "REV-1", "PT-1", &blocks).unwrap();
        repo::upsert_decision(&conn, "REV-1", &RecommendationDecision {
            recommendation_id: rec_id, status: "used".into(), original_text: Some("orig".into()),
            final_text: Some("orig".into()), edit_distance: Some(0), similarity_percent: Some(100.0),
            decision_elapsed_seconds: Some(10), dismissal_reason: None, decided_at: Some(repo::now_iso()),
        }).unwrap();
        repo::save_survey(&conn, "REV-1", None, "general", "{\"trust\":\"4\"}").unwrap();
        (path, conn)
    }

    #[test]
    fn build_import_roundtrip() {
        let (path, conn) = worked_session();
        let (secret, public) = response_seal::generate_keypair();
        let out = tempfile("response.atbr");

        let receipt = build_response(&conn, &path, &out, "ASG-1", &public).unwrap();
        assert_eq!(receipt.patient_count, 2);
        assert_eq!(receipt.reviewer_id, "REV-1");

        let imported = import_response(&out, &secret).unwrap();
        assert_eq!(imported.header.assignment_id, "ASG-1");
        assert_eq!(imported.patients.len(), 2);
        let pt1 = imported.patients.iter().find(|p| p.patient_id == "PT-1").unwrap();
        assert!(pt1.final_text.contains("Physician plan here."));
        assert!(pt1.attribution.char_count > 0);
        assert_eq!(pt1.decisions.len(), 1);
    }

    #[test]
    fn editing_an_inserted_rec_reconciles_the_decision() {
        // Seed, insert a rec verbatim (decision 'used', distance 0), then edit
        // the AI block. The exported decision must reflect the edit.
        let path = tempfile("assignment.atb");
        let mut conn = container::create(&path, "pw", ContainerRole::Reviewer).unwrap();
        seed::seed_demo(&conn, "REV-1").unwrap();
        let p = repo::load_patient(&conn, "REV-1", "PT-1").unwrap();
        let rec = p.recommendations[0].clone();

        let blocks = vec![
            NoteBlock { id: "b1".into(), block_type: "ai".into(), recommendation_id: Some(rec.id.clone()),
                original_text: Some(rec.text.clone()), current_text: format!("{} — with a physician addendum.", rec.text), position: 0 },
        ];
        repo::save_note_blocks(&mut conn, "REV-1", "PT-1", &blocks).unwrap();
        // Insert-time decision snapshot: verbatim.
        repo::upsert_decision(&conn, "REV-1", &RecommendationDecision {
            recommendation_id: rec.id.clone(), status: "used".into(), original_text: Some(rec.text.clone()),
            final_text: Some(rec.text.clone()), edit_distance: Some(0), similarity_percent: Some(100.0),
            decision_elapsed_seconds: None, dismissal_reason: None, decided_at: Some(repo::now_iso()),
        }).unwrap();

        let (secret, public) = response_seal::generate_keypair();
        let out = tempfile("response-edit.atbr");
        build_response(&conn, &path, &out, "ASG-1", &public).unwrap();
        let imported = import_response(&out, &secret).unwrap();

        let pt = imported.patients.iter().find(|p| p.patient_id == "PT-1").unwrap();
        let dec = pt.decisions.iter().find(|d| d.recommendation_id == rec.id).unwrap();
        assert_eq!(dec.status, "used-and-edited", "edited block should reconcile to used-and-edited");
        assert!(dec.edit_distance.unwrap() > 0, "edit distance should be nonzero after an edit");
        assert!(dec.final_text.as_deref().unwrap().contains("physician addendum"));
    }

    #[test]
    fn wrong_coordinator_key_cannot_import() {
        let (path, conn) = worked_session();
        let (_secret, public) = response_seal::generate_keypair();
        let (other_secret, _) = response_seal::generate_keypair();
        let out = tempfile("response2.atbr");
        build_response(&conn, &path, &out, "ASG-1", &public).unwrap();
        assert!(matches!(import_response(&out, &other_secret), Err(ResponseError::Seal(_))));
    }

    #[test]
    fn tampered_response_is_rejected() {
        let (path, conn) = worked_session();
        let (secret, public) = response_seal::generate_keypair();
        let out = tempfile("response3.atbr");
        build_response(&conn, &path, &out, "ASG-1", &public).unwrap();
        let mut bytes = std::fs::read(&out).unwrap();
        let n = bytes.len();
        bytes[n - 16] ^= 0xFF; // corrupt the SQLCipher DB region
        std::fs::write(&out, &bytes).unwrap();
        assert!(import_response(&out, &secret).is_err());
    }

    #[test]
    fn duplicate_import_is_rejected() {
        let (path, conn) = worked_session();
        let (secret, public) = response_seal::generate_keypair();
        let out = tempfile("response4.atbr");
        build_response(&conn, &path, &out, "ASG-1", &public).unwrap();
        let imported = import_response(&out, &secret).unwrap();

        let results_path = tempfile("results.atb");
        let results = container::create(&results_path, "coord-pw", ContainerRole::Coordinator).unwrap();
        merge_into_results(&results, &imported).unwrap();
        assert!(matches!(merge_into_results(&results, &imported), Err(ResponseError::Duplicate)));
    }
}
