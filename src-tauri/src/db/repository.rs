//! CRUD over a keyed SQLCipher connection. All SQL lives here; the frontend
//! only ever sees the DTOs in `super::models`, never table names.

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use super::connection::DbError;
use super::models::*;

fn json_from_text(s: Option<String>) -> Option<Value> {
    s.and_then(|t| serde_json::from_str(&t).ok())
}

// ── app_metadata ───────────────────────────────────────────────────────────

pub fn get_metadata(conn: &Connection, key: &str) -> Result<Option<String>, DbError> {
    conn.query_row("SELECT value FROM app_metadata WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(Into::into)
}

pub fn set_metadata(conn: &Connection, key: &str, value: &str) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR REPLACE INTO app_metadata(key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

// ── Reads ────────────────────────────────────────────────────────────────

/// The reviewer this package belongs to. Assignment packages contain exactly
/// one reviewer; prefer a row explicitly marked as `reviewer`.
pub fn first_reviewer_id(conn: &Connection) -> Result<String, DbError> {
    conn.query_row(
        "SELECT reviewer_id FROM reviewers ORDER BY (role = 'reviewer') DESC LIMIT 1",
        [],
        |r| r.get(0),
    )
    .map_err(Into::into)
}

/// Load the assignment header + patient summaries for a reviewer.
pub fn load_assignment(conn: &Connection, reviewer_id: &str) -> Result<Assignment, DbError> {
    let (study_id, title, protocol, schema_version, contact, instructions, settings_text): (
        String,
        String,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = conn.query_row(
        "SELECT study_id, title, protocol_version, schema_version, contact_email, instructions, settings_json
         FROM studies LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
    )?;

    let (display_name, state): (Option<String>, String) = conn.query_row(
        "SELECT display_name, assignment_status FROM reviewers WHERE reviewer_id = ?1",
        [reviewer_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let mut stmt = conn.prepare(
        "SELECT p.patient_id, p.research_id, p.display_label, ra.position, p.status, p.elapsed_seconds
         FROM patients p
         JOIN reviewer_assignments ra ON ra.patient_id = p.patient_id
         WHERE ra.reviewer_id = ?1
         ORDER BY ra.position",
    )?;
    let patients = stmt
        .query_map([reviewer_id], |r| {
            Ok(PatientSummary {
                id: r.get(0)?,
                research_id: r.get(1)?,
                display_label: r.get(2)?,
                position: r.get(3)?,
                status: r.get(4)?,
                elapsed_seconds: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Assignment {
        study_id,
        study_title: title,
        protocol_version: protocol,
        schema_version,
        contact_email: contact,
        instructions,
        settings: json_from_text(settings_text).unwrap_or(Value::Null),
        reviewer_id: reviewer_id.to_string(),
        reviewer_display_name: display_name,
        state,
        patients,
    })
}

/// Load one patient with documents, recommendations, this reviewer's decisions,
/// and note blocks.
pub fn load_patient(
    conn: &Connection,
    reviewer_id: &str,
    patient_id: &str,
) -> Result<Patient, DbError> {
    let (research_id, display_label, clinical_question, position, status, started_at, completed_at, elapsed): (
        Option<String>, String, Option<String>, i64, String, Option<String>, Option<String>, i64,
    ) = conn.query_row(
        "SELECT p.research_id, p.display_label, p.clinical_question, ra.position, p.status,
                p.started_at, p.completed_at, p.elapsed_seconds
         FROM patients p
         JOIN reviewer_assignments ra ON ra.patient_id = p.patient_id AND ra.reviewer_id = ?2
         WHERE p.patient_id = ?1",
        params![patient_id, reviewer_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
    )?;

    let documents = load_documents(conn, patient_id)?;
    let recommendations = load_recommendations(conn, patient_id)?;
    let decisions = load_decisions(conn, reviewer_id, patient_id)?;
    let note_blocks = load_note_blocks(conn, reviewer_id, patient_id)?;

    Ok(Patient {
        id: patient_id.to_string(),
        research_id,
        display_label,
        clinical_question,
        position,
        status,
        started_at,
        completed_at,
        elapsed_seconds: elapsed,
        documents,
        recommendations,
        decisions,
        note_blocks,
    })
}

fn load_documents(conn: &Connection, patient_id: &str) -> Result<Vec<SourceDocument>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT document_id, document_type, filename, mime_type, text_content, byte_size, sha256
         FROM source_documents WHERE patient_id = ?1 ORDER BY document_id",
    )?;
    let rows = stmt
        .query_map([patient_id], |r| {
            Ok(SourceDocument {
                id: r.get(0)?,
                patient_id: patient_id.to_string(),
                document_type: r.get(1)?,
                filename: r.get(2)?,
                mime_type: r.get(3)?,
                text_content: r.get(4)?,
                byte_size: r.get(5)?,
                sha256: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn load_recommendations(conn: &Connection, patient_id: &str) -> Result<Vec<Recommendation>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT recommendation_id, position, priority_rank, temperature_level, temperature_label,
                evidence_tier, risk_score, safety_score, title, recommendation_text, full_text,
                rationale, metadata_json, is_custom
         FROM recommendations WHERE patient_id = ?1 ORDER BY position",
    )?;
    let rows = stmt
        .query_map([patient_id], |r| {
            let meta: Option<String> = r.get(12)?;
            Ok(Recommendation {
                id: r.get(0)?,
                patient_id: patient_id.to_string(),
                position: r.get(1)?,
                priority_rank: r.get(2)?,
                temperature_level: r.get(3)?,
                temperature_label: r.get(4)?,
                evidence_tier: r.get(5)?,
                risk_score: r.get(6)?,
                safety_score: r.get(7)?,
                title: r.get(8)?,
                text: r.get(9)?,
                full_text: r.get(10)?,
                rationale: r.get(11)?,
                metadata: json_from_text(meta),
                is_custom: r.get::<_, i64>(13)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn load_decisions(
    conn: &Connection,
    reviewer_id: &str,
    patient_id: &str,
) -> Result<Vec<RecommendationDecision>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT d.recommendation_id, d.status, d.original_text, d.final_text, d.edit_distance,
                d.similarity_percent, d.decision_elapsed_seconds, d.dismissal_reason, d.decided_at
         FROM recommendation_decisions d
         JOIN recommendations r ON r.recommendation_id = d.recommendation_id
         WHERE d.reviewer_id = ?1 AND r.patient_id = ?2",
    )?;
    let rows = stmt
        .query_map(params![reviewer_id, patient_id], |r| {
            Ok(RecommendationDecision {
                recommendation_id: r.get(0)?,
                status: r.get(1)?,
                original_text: r.get(2)?,
                final_text: r.get(3)?,
                edit_distance: r.get(4)?,
                similarity_percent: r.get(5)?,
                decision_elapsed_seconds: r.get(6)?,
                dismissal_reason: r.get(7)?,
                decided_at: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn load_note_blocks(
    conn: &Connection,
    reviewer_id: &str,
    patient_id: &str,
) -> Result<Vec<NoteBlock>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT block_id, block_type, recommendation_id, original_text, current_text, position
         FROM note_blocks WHERE patient_id = ?1 AND reviewer_id = ?2 ORDER BY position",
    )?;
    let rows = stmt
        .query_map(params![patient_id, reviewer_id], |r| {
            Ok(NoteBlock {
                id: r.get(0)?,
                block_type: r.get(1)?,
                recommendation_id: r.get(2)?,
                original_text: r.get(3)?,
                current_text: r.get(4)?,
                position: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ── Writes ───────────────────────────────────────────────────────────────

/// Replace all note blocks for (patient, reviewer) in one transaction — the
/// block array order is the source of truth, so we delete-then-insert.
pub fn save_note_blocks(
    conn: &mut Connection,
    reviewer_id: &str,
    patient_id: &str,
    blocks: &[NoteBlock],
) -> Result<(), DbError> {
    let now = now_iso();
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM note_blocks WHERE patient_id = ?1 AND reviewer_id = ?2",
        params![patient_id, reviewer_id],
    )?;
    for (i, b) in blocks.iter().enumerate() {
        tx.execute(
            "INSERT INTO note_blocks(block_id, patient_id, reviewer_id, position, block_type,
                 recommendation_id, original_text, current_text, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
            params![
                b.id, patient_id, reviewer_id, i as i64, b.block_type,
                b.recommendation_id, b.original_text, b.current_text, now
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Insert or update a reviewer's decision on a recommendation.
pub fn upsert_decision(
    conn: &Connection,
    reviewer_id: &str,
    decision: &RecommendationDecision,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO recommendation_decisions(
             decision_id, recommendation_id, reviewer_id, status, original_text, final_text,
             edit_distance, similarity_percent, decision_elapsed_seconds, dismissal_reason, decided_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
         ON CONFLICT(decision_id) DO UPDATE SET
             status=excluded.status, original_text=excluded.original_text,
             final_text=excluded.final_text, edit_distance=excluded.edit_distance,
             similarity_percent=excluded.similarity_percent,
             decision_elapsed_seconds=excluded.decision_elapsed_seconds,
             dismissal_reason=excluded.dismissal_reason, decided_at=excluded.decided_at",
        params![
            format!("{reviewer_id}:{}", decision.recommendation_id),
            decision.recommendation_id, reviewer_id, decision.status, decision.original_text,
            decision.final_text, decision.edit_distance, decision.similarity_percent,
            decision.decision_elapsed_seconds, decision.dismissal_reason, decision.decided_at
        ],
    )?;
    Ok(())
}

pub fn set_patient_status(
    conn: &Connection,
    patient_id: &str,
    status: &str,
    started_at: Option<&str>,
    completed_at: Option<&str>,
    elapsed_seconds: Option<i64>,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE patients SET status = ?2,
             started_at = COALESCE(?3, started_at),
             completed_at = COALESCE(?4, completed_at),
             elapsed_seconds = COALESCE(?5, elapsed_seconds)
         WHERE patient_id = ?1",
        params![patient_id, status, started_at, completed_at, elapsed_seconds],
    )?;
    Ok(())
}

pub fn set_assignment_state(
    conn: &Connection,
    reviewer_id: &str,
    state: &str,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE reviewers SET assignment_status = ?2 WHERE reviewer_id = ?1",
        params![reviewer_id, state],
    )?;
    Ok(())
}

pub fn save_survey(
    conn: &Connection,
    reviewer_id: &str,
    patient_id: Option<&str>,
    question_id: &str,
    response_json: &str,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO survey_responses(response_id, reviewer_id, patient_id, question_id, response_json, created_at)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            uuid::Uuid::new_v4().to_string(), reviewer_id, patient_id, question_id, response_json, now_iso()
        ],
    )?;
    Ok(())
}

pub fn append_audit(
    conn: &Connection,
    reviewer_id: Option<&str>,
    event: &AuditEvent,
) -> Result<(), DbError> {
    let payload = event.payload.as_ref().map(|v| v.to_string());
    conn.execute(
        "INSERT INTO audit_events(event_id, reviewer_id, patient_id, event_type, event_time, payload_json)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            uuid::Uuid::new_v4().to_string(), reviewer_id, event.patient_id,
            event.event_type, event.event_time, payload
        ],
    )?;
    Ok(())
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::container::{self, ContainerRole};

    fn tempfile(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("atb-repo-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p.join(name)
    }

    #[test]
    fn seed_load_and_persist_roundtrip() {
        let path = tempfile("study.atb");
        let mut conn = container::create(&path, "pw", ContainerRole::Reviewer).unwrap();
        super::super::seed::seed_demo(&conn, "REV-1").unwrap();

        // Assignment + summaries
        let asg = load_assignment(&conn, "REV-1").unwrap();
        assert_eq!(asg.patients.len(), 2);
        assert_eq!(asg.reviewer_id, "REV-1");

        // Patient with recs
        let p = load_patient(&conn, "REV-1", "PT-1").unwrap();
        assert!(p.recommendations.len() >= 1);
        let rec_id = p.recommendations[0].id.clone();

        // Save note blocks (user + ai) and a decision, then reload.
        let blocks = vec![
            NoteBlock { id: "b1".into(), block_type: "user".into(), recommendation_id: None, original_text: None, current_text: "Assessment.".into(), position: 0 },
            NoteBlock { id: "b2".into(), block_type: "ai".into(), recommendation_id: Some(rec_id.clone()), original_text: Some("orig".into()), current_text: "orig edited".into(), position: 1 },
        ];
        save_note_blocks(&mut conn, "REV-1", "PT-1", &blocks).unwrap();
        upsert_decision(&conn, "REV-1", &RecommendationDecision {
            recommendation_id: rec_id.clone(), status: "used-and-edited".into(),
            original_text: Some("orig".into()), final_text: Some("orig edited".into()),
            edit_distance: Some(7), similarity_percent: Some(65.0),
            decision_elapsed_seconds: Some(30), dismissal_reason: None,
            decided_at: Some(now_iso()),
        }).unwrap();

        let reloaded = load_patient(&conn, "REV-1", "PT-1").unwrap();
        assert_eq!(reloaded.note_blocks.len(), 2);
        assert_eq!(reloaded.note_blocks[1].block_type, "ai");
        assert_eq!(reloaded.decisions.len(), 1);
        assert_eq!(reloaded.decisions[0].status, "used-and-edited");

        // Idempotent decision update.
        upsert_decision(&conn, "REV-1", &RecommendationDecision {
            recommendation_id: rec_id, status: "dismissed".into(), original_text: None,
            final_text: None, edit_distance: None, similarity_percent: None,
            decision_elapsed_seconds: None, dismissal_reason: Some("not indicated".into()),
            decided_at: Some(now_iso()),
        }).unwrap();
        let after = load_patient(&conn, "REV-1", "PT-1").unwrap();
        assert_eq!(after.decisions.len(), 1);
        assert_eq!(after.decisions[0].status, "dismissed");
    }
}
