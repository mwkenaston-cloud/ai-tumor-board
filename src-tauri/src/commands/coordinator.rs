//! Coordinator-side commands: workspace, patient/LLM entry, credential-gated
//! package build, and response import/merge. Gated by the Ed25519 role
//! credential — in an unprovisioned (development) build the gate reports
//! "development" and stays open; a provisioned build requires a valid credential.

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};

use crate::crypto::container::{self, ContainerRole};
use crate::crypto::{response_seal, role_credential};
use crate::db::packaging::{self, PackageReceipt};
use crate::db::repository::{self as repo, now_iso};
use crate::db::{llm_import, response};

const DEV_WORKSPACE_PASSWORD: &str = "coordinator-dev-password";

pub struct CoordinatorSession {
    pub conn: Connection,
    // Retained for future workspace backup/recovery.
    #[allow(dead_code)]
    pub path: PathBuf,
}

pub type CoordinatorState = Mutex<Option<CoordinatorSession>>;

fn map_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Reject coordinator work unless access is granted. Development builds are
/// allowed but flagged; provisioned builds require a valid credential file.
fn ensure_access(app: &AppHandle) -> Result<bool, String> {
    if !role_credential::is_provisioned() {
        return Ok(false); // development mode
    }
    let cred_path = app
        .path()
        .app_data_dir()
        .map_err(map_err)?
        .join("coordinator-credential.json");
    let bytes = std::fs::read(&cred_path)
        .map_err(|_| "coordinator credential not found".to_string())?;
    let cred: role_credential::CoordinatorCredential =
        serde_json::from_slice(&bytes).map_err(|_| "malformed credential".to_string())?;
    let authority = role_credential::authority_public_key().ok_or("no authority key")?;
    role_credential::verify_credential(&cred, &authority, Some(&now_iso())).map_err(map_err)?;
    Ok(true)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorSummary {
    pub study_title: String,
    pub provisioned: bool,
    pub patients: Vec<CoordPatient>,
    pub reviewers: Vec<CoordReviewer>,
    pub results_count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordPatient {
    pub id: String,
    pub research_id: Option<String>,
    pub model_id: Option<String>,
    pub cancer_type: Option<String>,
    pub clinical_question: Option<String>,
    pub document_count: i64,
    pub recommendation_count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordReviewer {
    pub reviewer_id: String,
    pub display_name: Option<String>,
    pub patient_count: i64,
}

fn with_session<T>(
    state: &State<CoordinatorState>,
    f: impl FnOnce(&mut CoordinatorSession) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = state.lock().unwrap();
    let s = guard.as_mut().ok_or("no coordinator workspace open")?;
    f(s)
}

/// Open or create the coordinator workspace and return its summary.
#[tauri::command]
pub fn coordinator_open_workspace(
    app: AppHandle,
    state: State<CoordinatorState>,
) -> Result<CoordinatorSummary, String> {
    let provisioned = ensure_access(&app)?;

    let dir = app.path().app_data_dir().map_err(map_err)?;
    std::fs::create_dir_all(&dir).map_err(map_err)?;
    let path = dir.join("coordinator-workspace-v2.atb");

    let conn = if path.exists() {
        container::open(&path, DEV_WORKSPACE_PASSWORD).map_err(map_err)?
    } else {
        let c = container::create(&path, DEV_WORKSPACE_PASSWORD, ContainerRole::Coordinator)
            .map_err(map_err)?;
        // Default study (with sensible display settings so reviewer views show
        // all badges) + a coordinator X25519 keypair for sealing responses.
        let default_settings = serde_json::json!({
            "showPriority": true, "showEvidence": true, "showSafety": true,
            "showTemperature": true, "showDetails": true, "allowDismiss": true,
            "showTimer": false, "perPatientSurvey": true, "generalSurvey": true
        })
        .to_string();
        c.execute(
            "INSERT OR REPLACE INTO studies(study_id, title, protocol_version, schema_version, settings_json, created_at)
             VALUES ('STUDY-1', 'AI Tumor Board', 'v1', 1, ?1, ?2)",
            params![default_settings, now_iso()],
        )
        .map_err(|e| map_err(e))?;
        let (secret, public) = response_seal::generate_keypair();
        repo::set_metadata(&c, "coordinator_secret_hex", &hex_encode(&secret)).map_err(map_err)?;
        repo::set_metadata(&c, "coordinator_public_hex", &hex_encode(&public)).map_err(map_err)?;
        c
    };

    let summary = build_summary(&conn, provisioned).map_err(map_err)?;
    *state.lock().unwrap() = Some(CoordinatorSession { conn, path });
    Ok(summary)
}

#[tauri::command]
pub fn coordinator_summary(
    app: AppHandle,
    state: State<CoordinatorState>,
) -> Result<CoordinatorSummary, String> {
    let provisioned = role_credential::is_provisioned();
    let _ = app;
    with_session(&state, |s| build_summary(&s.conn, provisioned).map_err(map_err))
}

#[tauri::command]
pub fn coordinator_add_patient(
    state: State<CoordinatorState>,
    research_id: String,
    model_id: String,
) -> Result<String, String> {
    with_session(&state, |s| {
        let count: i64 = s
            .conn
            .query_row("SELECT count(*) FROM patients", [], |r| r.get(0))
            .map_err(map_err)?;
        let patient_id = format!("PT-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        // display_label defaults to the model id until the LLM import fills in a
        // cancer type; clinical_question/context arrive with the LLM import.
        s.conn
            .execute(
                "INSERT INTO patients(patient_id, study_id, research_id, model_id, display_label, position, status, elapsed_seconds)
                 VALUES (?1, 'STUDY-1', ?2, ?3, ?3, ?4, 'not_started', 0)",
                params![patient_id, research_id, model_id, count],
            )
            .map_err(map_err)?;
        Ok(patient_id)
    })
}

/// Remove a patient and all of its dependent rows from the workspace.
#[tauri::command]
pub fn coordinator_remove_patient(
    state: State<CoordinatorState>,
    patient_id: String,
) -> Result<(), String> {
    with_session(&state, |s| {
        let tx = s.conn.transaction().map_err(map_err)?;
        // Remove decisions tied to this patient's recommendations first (FK order).
        tx.execute(
            "DELETE FROM recommendation_decisions WHERE recommendation_id IN
                (SELECT recommendation_id FROM recommendations WHERE patient_id = ?1)",
            params![patient_id],
        ).map_err(map_err)?;
        for table in ["note_blocks", "recommendations", "llm_runs", "source_documents", "reviewer_assignments", "survey_responses", "audit_events"] {
            tx.execute(&format!("DELETE FROM {table} WHERE patient_id = ?1"), params![patient_id]).map_err(map_err)?;
        }
        tx.execute("DELETE FROM patients WHERE patient_id = ?1", params![patient_id]).map_err(map_err)?;
        tx.commit().map_err(map_err)?;
        Ok(())
    })
}

#[tauri::command]
pub fn coordinator_add_document(
    state: State<CoordinatorState>,
    patient_id: String,
    document_type: String,
    filename: String,
    text_content: String,
) -> Result<(), String> {
    with_session(&state, |s| {
        let mut h = Sha256::new();
        h.update(text_content.as_bytes());
        let sha = hex_encode(&h.finalize());
        s.conn
            .execute(
                "INSERT INTO source_documents(document_id, patient_id, document_type, filename, mime_type, text_content, byte_size, sha256, created_at)
                 VALUES (?1,?2,?3,?4,'text/plain',?5,?6,?7,?8)",
                params![
                    uuid::Uuid::new_v4().to_string(), patient_id, document_type, filename,
                    text_content, text_content.len() as i64, sha, now_iso()
                ],
            )
            .map_err(map_err)?;
        Ok(())
    })
}

/// Import a single combined clinical-source `.txt`, splitting it into per-type
/// documents (imaging / clinical notes / pathology / labs). Replaces any
/// existing documents for the patient. Returns the number of sections stored.
#[tauri::command]
pub fn coordinator_import_document_file(
    state: State<CoordinatorState>,
    patient_id: String,
    path: String,
) -> Result<usize, String> {
    let text = std::fs::read_to_string(&path).map_err(map_err)?;
    let sections = crate::db::document_import::parse_combined(&text);
    if sections.is_empty() {
        return Err("No recognized sections found (expected 'Txt Imaging', 'Txt Clinical Notes', 'Txt Pathology', 'Txt Labs').".into());
    }
    with_session(&state, |s| {
        let tx = s.conn.transaction().map_err(map_err)?;
        tx.execute("DELETE FROM source_documents WHERE patient_id = ?1", params![patient_id]).map_err(map_err)?;
        for sec in &sections {
            let mut h = Sha256::new();
            h.update(sec.content.as_bytes());
            let sha = hex_encode(&h.finalize());
            tx.execute(
                "INSERT INTO source_documents(document_id, patient_id, document_type, filename, mime_type, text_content, byte_size, sha256, created_at)
                 VALUES (?1,?2,?3,?4,'text/plain',?5,?6,?7,?8)",
                params![
                    uuid::Uuid::new_v4().to_string(), patient_id, sec.document_type,
                    format!("{}.txt", sec.document_type), sec.content, sec.content.len() as i64, sha, now_iso()
                ],
            ).map_err(map_err)?;
        }
        tx.commit().map_err(map_err)?;
        Ok(sections.len())
    })
}

/// Validate + normalize + store LLM output for a patient. Returns the count of
/// normalized recommendations.
#[tauri::command]
pub fn coordinator_import_llm(
    state: State<CoordinatorState>,
    patient_id: String,
    raw_json: String,
) -> Result<usize, String> {
    with_session(&state, |s| {
        let normalized = llm_import::validate_and_normalize(&patient_id, &raw_json).map_err(map_err)?;
        let n = normalized.recommendations.len();
        llm_import::store_import(&s.conn, &patient_id, &raw_json, &normalized).map_err(map_err)?;
        Ok(n)
    })
}

/// Build a reviewer-specific `.atb` (credential-gated). Ensures the reviewer and
/// assignments exist, then packages only their patients.
#[tauri::command]
pub fn coordinator_build_package(
    app: AppHandle,
    state: State<CoordinatorState>,
    reviewer_id: String,
    display_name: String,
    patient_ids: Vec<String>,
    password: String,
    destination: String,
) -> Result<PackageReceipt, String> {
    ensure_access(&app)?;
    with_session(&state, |s| {
        s.conn
            .execute(
                "INSERT OR REPLACE INTO reviewers(reviewer_id, display_name, role, assignment_status)
                 VALUES (?1, ?2, 'reviewer', 'ready')",
                params![reviewer_id, display_name],
            )
            .map_err(map_err)?;
        for (pos, pid) in patient_ids.iter().enumerate() {
            s.conn
                .execute(
                    "INSERT OR REPLACE INTO reviewer_assignments(reviewer_id, patient_id, position) VALUES (?1,?2,?3)",
                    params![reviewer_id, pid, pos as i64],
                )
                .map_err(map_err)?;
        }
        let pub_hex = repo::get_metadata(&s.conn, "coordinator_public_hex")
            .map_err(map_err)?
            .ok_or("workspace missing coordinator key")?;
        let pubkey = hex32(&pub_hex).ok_or("bad coordinator key")?;
        let receipt = packaging::build_package(
            &s.conn,
            std::path::Path::new(&destination),
            &password,
            &reviewer_id,
            &patient_ids,
            &pubkey,
        )
        .map_err(map_err)?;

        // Record this batch so the Responses tab can show what was sent and who
        // has responded. A reviewer may receive multiple batches over time.
        ensure_sent_table(&s.conn).map_err(map_err)?;
        s.conn
            .execute(
                "INSERT OR REPLACE INTO sent_assignments(assignment_id, reviewer_id, display_name, patient_count, sha256, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![receipt.assignment_id, reviewer_id, display_name, receipt.patient_count as i64, receipt.sha256, now_iso()],
            )
            .map_err(map_err)?;
        // Snapshot which patients went into this batch (for the reviewer grid).
        for pid in &patient_ids {
            let (rid, mid): (Option<String>, Option<String>) = s
                .conn
                .query_row("SELECT research_id, model_id FROM patients WHERE patient_id = ?1", [pid], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap_or((None, None));
            s.conn
                .execute(
                    "INSERT OR REPLACE INTO sent_assignment_patients(assignment_id, patient_id, research_id, model_id) VALUES (?1,?2,?3,?4)",
                    params![receipt.assignment_id, pid, rid, mid],
                )
                .map_err(map_err)?;
        }
        Ok(receipt)
    })
}

fn ensure_sent_table(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sent_assignments (
             assignment_id TEXT PRIMARY KEY, reviewer_id TEXT NOT NULL, display_name TEXT,
             patient_count INTEGER NOT NULL, sha256 TEXT, created_at TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS sent_assignment_patients (
             assignment_id TEXT NOT NULL, patient_id TEXT NOT NULL, research_id TEXT, model_id TEXT,
             PRIMARY KEY (assignment_id, patient_id));",
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub reviewer_id: String,
    pub assignment_id: String,
    pub patient_count: usize,
}

/// Import a `.atbr` response (credential-gated): unseal with the workspace's
/// coordinator secret, verify, dedup, and merge into results.
#[tauri::command]
pub fn coordinator_import_response(
    app: AppHandle,
    state: State<CoordinatorState>,
    atbr_path: String,
) -> Result<ImportSummary, String> {
    ensure_access(&app)?;
    with_session(&state, |s| {
        let secret_hex = repo::get_metadata(&s.conn, "coordinator_secret_hex")
            .map_err(map_err)?
            .ok_or("workspace missing coordinator secret")?;
        let secret = hex32(&secret_hex).ok_or("bad coordinator secret")?;
        let imported =
            response::import_response(std::path::Path::new(&atbr_path), &secret).map_err(map_err)?;
        response::merge_into_results(&s.conn, &imported).map_err(map_err)?;
        Ok(ImportSummary {
            reviewer_id: imported.header.reviewer_id.clone(),
            assignment_id: imported.header.assignment_id.clone(),
            patient_count: imported.patients.len(),
        })
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultRow {
    pub assignment_id: String,
    pub reviewer_id: String,
    pub submitted_at: Option<String>,
    pub imported_at: String,
}

#[tauri::command]
pub fn coordinator_list_results(state: State<CoordinatorState>) -> Result<Vec<ResultRow>, String> {
    with_session(&state, |s| {
        // The results tables are created lazily on first import.
        let exists: i64 = s
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='imported_responses'",
                [],
                |r| r.get(0),
            )
            .map_err(map_err)?;
        if exists == 0 {
            return Ok(vec![]);
        }
        let mut stmt = s
            .conn
            .prepare("SELECT assignment_id, reviewer_id, submitted_at, imported_at FROM imported_responses ORDER BY imported_at DESC")
            .map_err(map_err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ResultRow {
                    assignment_id: r.get(0)?,
                    reviewer_id: r.get(1)?,
                    submitted_at: r.get(2)?,
                    imported_at: r.get(3)?,
                })
            })
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        Ok(rows)
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    pub assignment_id: String,
    pub reviewer_id: String,
    pub display_name: Option<String>,
    pub patient_count: i64,
    pub created_at: String,
    pub responded: bool,
    pub submitted_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AggRec {
    pub recommendation_id: String,
    pub title: String,
    pub accepted: usize,
    pub dismissed: usize,
    pub ignored: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AggPatient {
    pub patient_id: String,
    pub research_id: Option<String>,
    pub model_id: Option<String>,
    pub response_count: usize,
    pub avg_pct_physician_authored: f64,
    pub recommendations: Vec<AggRec>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponsesView {
    pub batches: Vec<Batch>,
    pub response_count: usize,
    pub reviewers: Vec<String>,
    pub patients: Vec<AggPatient>,
}

/// Batches sent + who has responded + aggregated findings across all imported
/// physician responses.
#[tauri::command]
pub fn coordinator_responses(state: State<CoordinatorState>) -> Result<ResponsesView, String> {
    with_session(&state, |s| build_responses_view(&s.conn).map_err(map_err))
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [name],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .unwrap_or(false)
}

fn build_responses_view(conn: &Connection) -> Result<ResponsesView, rusqlite::Error> {
    // Sent batches, with responded status joined from imported_responses.
    let mut batches = Vec::new();
    if table_exists(conn, "sent_assignments") {
        let has_imported = table_exists(conn, "imported_responses");
        let sql = if has_imported {
            "SELECT sa.assignment_id, sa.reviewer_id, sa.display_name, sa.patient_count, sa.created_at,
                    ir.submitted_at
             FROM sent_assignments sa
             LEFT JOIN imported_responses ir
               ON ir.assignment_id = sa.assignment_id AND ir.reviewer_id = sa.reviewer_id
             ORDER BY sa.created_at DESC"
        } else {
            "SELECT assignment_id, reviewer_id, display_name, patient_count, created_at, NULL
             FROM sent_assignments ORDER BY created_at DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        batches = stmt
            .query_map([], |r| {
                let submitted_at: Option<String> = r.get(5)?;
                Ok(Batch {
                    assignment_id: r.get(0)?,
                    reviewer_id: r.get(1)?,
                    display_name: r.get(2)?,
                    patient_count: r.get(3)?,
                    created_at: r.get(4)?,
                    responded: submitted_at.is_some(),
                    submitted_at,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }

    // Parse all imported response payloads.
    let mut payloads: Vec<serde_json::Value> = Vec::new();
    if table_exists(conn, "response_payloads") {
        let mut stmt = conn.prepare("SELECT payload_json FROM response_payloads")?;
        payloads = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .filter_map(|row| row.ok())
            .filter_map(|txt| serde_json::from_str::<serde_json::Value>(&txt).ok())
            .collect();
    }

    let mut reviewers: Vec<String> = payloads
        .iter()
        .filter_map(|p| p.get("header").and_then(|h| h.get("reviewerId")).and_then(|v| v.as_str()).map(String::from))
        .collect();
    reviewers.sort();
    reviewers.dedup();

    let patients = aggregate_patients(conn, &payloads)?;

    Ok(ResponsesView {
        batches,
        response_count: payloads.len(),
        reviewers,
        patients,
    })
}

fn aggregate_patients(
    conn: &Connection,
    payloads: &[serde_json::Value],
) -> Result<Vec<AggPatient>, rusqlite::Error> {
    use std::collections::BTreeMap;

    // For each patient, collect each reviewer's per-rec decision map + attribution.
    struct PerReviewer {
        statuses: BTreeMap<String, String>,
        pct: f64,
    }
    let mut by_patient: BTreeMap<String, Vec<PerReviewer>> = BTreeMap::new();

    for p in payloads {
        let Some(patients) = p.get("patients").and_then(|v| v.as_array()) else { continue };
        for entry in patients {
            let Some(pid) = entry.get("patientId").and_then(|v| v.as_str()) else { continue };
            let mut statuses = BTreeMap::new();
            if let Some(decisions) = entry.get("decisions").and_then(|v| v.as_array()) {
                for d in decisions {
                    if let (Some(rid), Some(st)) = (
                        d.get("recommendationId").and_then(|v| v.as_str()),
                        d.get("status").and_then(|v| v.as_str()),
                    ) {
                        statuses.insert(rid.to_string(), st.to_string());
                    }
                }
            }
            let pct = entry
                .get("attribution")
                .and_then(|a| a.get("pctPhysicianOriginal"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            by_patient.entry(pid.to_string()).or_default().push(PerReviewer { statuses, pct });
        }
    }

    let mut out = Vec::new();
    for (pid, reviews) in by_patient {
        if reviews.is_empty() {
            continue;
        }
        // Patient identity + recommendation set from the workspace.
        let (research_id, model_id): (Option<String>, Option<String>) = conn
            .query_row("SELECT research_id, model_id FROM patients WHERE patient_id = ?1", [&pid], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap_or((None, None));

        let mut rec_stmt =
            conn.prepare("SELECT recommendation_id, COALESCE(title, recommendation_id) FROM recommendations WHERE patient_id = ?1 ORDER BY position")?;
        let recs: Vec<(String, String)> = rec_stmt
            .query_map([&pid], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        let n = reviews.len();
        let recommendations = recs
            .into_iter()
            .map(|(rid, title)| {
                let mut accepted = 0;
                let mut dismissed = 0;
                for rv in &reviews {
                    match rv.statuses.get(&rid).map(String::as_str) {
                        Some("used") | Some("used-and-edited") => accepted += 1,
                        Some("dismissed") => dismissed += 1,
                        _ => {}
                    }
                }
                AggRec {
                    recommendation_id: rid,
                    title,
                    accepted,
                    dismissed,
                    ignored: n - accepted - dismissed,
                }
            })
            .collect();

        let avg_pct = reviews.iter().map(|r| r.pct).sum::<f64>() / n as f64;
        out.push(AggPatient {
            patient_id: pid,
            research_id,
            model_id,
            response_count: n,
            avg_pct_physician_authored: (avg_pct * 10.0).round() / 10.0,
            recommendations,
        });
    }
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridPatient {
    pub patient_id: String,
    pub research_id: Option<String>,
    pub model_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridAssignment {
    pub assignment_id: String,
    pub created_at: String,
    pub responded: bool,
    pub patients: Vec<GridPatient>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridReviewer {
    pub reviewer_id: String,
    pub display_name: Option<String>,
    pub assignments: Vec<GridAssignment>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewerGrid {
    pub reviewers: Vec<GridReviewer>,
}

/// Every reviewer that has been sent at least one batch, with each batch's
/// assigned patients and which `.atb` (assignment id) they went out in.
#[tauri::command]
pub fn coordinator_reviewers(state: State<CoordinatorState>) -> Result<ReviewerGrid, String> {
    with_session(&state, |s| build_reviewer_grid(&s.conn).map_err(map_err))
}

fn build_reviewer_grid(conn: &Connection) -> Result<ReviewerGrid, rusqlite::Error> {
    if !table_exists(conn, "sent_assignments") {
        return Ok(ReviewerGrid { reviewers: vec![] });
    }
    let has_imported = table_exists(conn, "imported_responses");
    let has_patients = table_exists(conn, "sent_assignment_patients");

    // Distinct reviewers, most recent first.
    let mut rstmt = conn.prepare(
        "SELECT reviewer_id, MAX(display_name), MAX(created_at) AS last
         FROM sent_assignments GROUP BY reviewer_id ORDER BY last DESC",
    )?;
    let reviewer_ids: Vec<(String, Option<String>)> = rstmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut reviewers = Vec::new();
    for (rid, name) in reviewer_ids {
        let mut astmt = conn.prepare(
            "SELECT assignment_id, created_at FROM sent_assignments WHERE reviewer_id = ?1 ORDER BY created_at DESC",
        )?;
        let asgs: Vec<(String, String)> = astmt
            .query_map([&rid], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut assignments = Vec::new();
        for (aid, created_at) in asgs {
            let responded = has_imported
                && conn
                    .query_row(
                        "SELECT count(*) FROM imported_responses WHERE assignment_id = ?1 AND reviewer_id = ?2",
                        params![aid, rid],
                        |r| r.get::<_, i64>(0),
                    )
                    .unwrap_or(0)
                    > 0;
            let patients = if has_patients {
                let mut pstmt = conn.prepare(
                    "SELECT patient_id, research_id, model_id FROM sent_assignment_patients WHERE assignment_id = ?1",
                )?;
                let rows: Vec<GridPatient> = pstmt
                    .query_map([&aid], |r| {
                        Ok(GridPatient { patient_id: r.get(0)?, research_id: r.get(1)?, model_id: r.get(2)? })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            } else {
                vec![]
            };
            assignments.push(GridAssignment { assignment_id: aid, created_at, responded, patients });
        }
        reviewers.push(GridReviewer { reviewer_id: rid, display_name: name, assignments });
    }
    Ok(ReviewerGrid { reviewers })
}

// ── Analysis export ─────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AnalysisRecord {
    reviewer_id: String,
    assignment_id: String,
    submitted_at: Option<String>,
    source_assignment_sha256: Option<String>,
    app_version: Option<String>,
    schema_version: Option<i64>,
    patient_id: String,
    research_id: Option<String>,
    model_id: Option<String>,
    cancer_type: Option<String>,
    clinical_question: Option<String>,
    patient_status: String,
    elapsed_seconds: i64,
    note_word_count: i64,
    note_char_count: i64,
    pct_physician_original: f64,
    pct_ai_unmodified: f64,
    pct_ai_edited: f64,
    pct_derived_from_llm: f64,
    chars_typed_by_physician: i64,
    chars_from_llm_unmodified: i64,
    chars_from_llm_edited: i64,
    final_note_text: String,
    recommendation_id: String,
    title: String,
    temperature_level: Option<i64>,
    temperature_label: Option<String>,
    evidence_tier: Option<String>,
    risk_score: Option<f64>,
    safety_score: Option<f64>,
    priority_rank: Option<i64>,
    original_ai_text: String,
    disposition: String, // accepted / dismissed / ignored
    status: String,      // used / used-and-edited / dismissed / (none)
    was_used: i64,
    was_altered: i64,
    edit_distance: Option<i64>,
    similarity_percent: Option<f64>,
    final_text_in_note: Option<String>,
    dismissal_reason: Option<String>,
    decided_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPaths {
    pub json_path: String,
    pub csv_path: String,
    pub record_count: usize,
}

/// Write a comprehensive analysis export: a lossless JSON (all responses + a
/// flat record per reviewer×patient×recommendation) and a poolable CSV of those
/// flat records with every manuscript-relevant variable.
#[tauri::command]
pub fn coordinator_export_analysis(
    state: State<CoordinatorState>,
    destination: String,
) -> Result<ExportPaths, String> {
    with_session(&state, |s| {
        let (records, raw): (Vec<AnalysisRecord>, Vec<serde_json::Value>) =
            build_analysis(&s.conn).map_err(map_err)?;
        let study_title: String = s
            .conn
            .query_row("SELECT title FROM studies LIMIT 1", [], |r| r.get(0))
            .unwrap_or_else(|_| "AI Tumor Board".into());

        let base = destination
            .trim_end_matches(".json")
            .trim_end_matches(".csv")
            .to_string();
        let json_path = format!("{base}.json");
        let csv_path = format!("{base}.csv");

        let doc = serde_json::json!({
            "format": "AI_TUMOR_BOARD_ANALYSIS",
            "version": 1,
            "exported_at": now_iso(),
            "study_title": study_title,
            "response_count": raw.len(),
            "records": records,
            "raw_responses": raw,
        });
        std::fs::write(&json_path, serde_json::to_string_pretty(&doc).unwrap()).map_err(map_err)?;
        std::fs::write(&csv_path, records_to_csv(&records)).map_err(map_err)?;

        Ok(ExportPaths { json_path, csv_path, record_count: records.len() })
    })
}

fn build_analysis(
    conn: &Connection,
) -> Result<(Vec<AnalysisRecord>, Vec<serde_json::Value>), rusqlite::Error> {
    let mut raw: Vec<serde_json::Value> = Vec::new();
    if table_exists(conn, "response_payloads") {
        let mut stmt = conn.prepare("SELECT payload_json FROM response_payloads")?;
        raw = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .filter_map(|row| row.ok())
            .filter_map(|txt| serde_json::from_str::<serde_json::Value>(&txt).ok())
            .collect();
    }

    let s = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_str()).map(String::from);
    let f = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
    let i = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(0);

    let mut records = Vec::new();
    for payload in &raw {
        let header = payload.get("header").cloned().unwrap_or(serde_json::Value::Null);
        let reviewer_id = s(&header, "reviewerId").unwrap_or_default();
        let assignment_id = s(&header, "assignmentId").unwrap_or_default();
        let submitted_at = s(&header, "submittedAt");
        let source_sha = s(&header, "sourceAssignmentSha256");
        let app_version = s(&header, "appVersion");
        let schema_version = header.get("schemaVersion").and_then(|x| x.as_i64());

        let Some(patients) = payload.get("patients").and_then(|v| v.as_array()) else { continue };
        for pt in patients {
            let patient_id = s(pt, "patientId").unwrap_or_default();
            let attribution = pt.get("attribution").cloned().unwrap_or(serde_json::Value::Null);
            let final_note = s(pt, "finalText").unwrap_or_default();

            // Patient identity from the workspace.
            let (model_id_ws, cancer_type, clinical_question): (Option<String>, Option<String>, Option<String>) = conn
                .query_row("SELECT model_id, cancer_type, clinical_question FROM patients WHERE patient_id = ?1", [&patient_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap_or((None, None, None));

            // Decisions keyed by recommendation id.
            let mut dmap: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
            if let Some(ds) = pt.get("decisions").and_then(|v| v.as_array()) {
                for d in ds {
                    if let Some(rid) = s(d, "recommendationId") {
                        dmap.insert(rid, d.clone());
                    }
                }
            }

            // Workspace recommendation set for this patient.
            let mut rstmt = conn.prepare(
                "SELECT recommendation_id, COALESCE(title, recommendation_id), temperature_level, temperature_label,
                        evidence_tier, risk_score, safety_score, priority_rank, recommendation_text
                 FROM recommendations WHERE patient_id = ?1 ORDER BY position",
            )?;
            let recs = rstmt.query_map([&patient_id], |r| {
                Ok((
                    r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?, r.get::<_, Option<f64>>(5)?, r.get::<_, Option<f64>>(6)?, r.get::<_, Option<i64>>(7)?, r.get::<_, String>(8)?,
                ))
            })?.collect::<Result<Vec<_>, _>>()?;

            for (rid, title, temp, temp_label, ev, risk, safety, prio, ai_text) in recs {
                let dec = dmap.get(&rid);
                let status = dec.and_then(|d| s(d, "status")).unwrap_or_default();
                let disposition = if status.starts_with("used") {
                    "accepted"
                } else if status == "dismissed" {
                    "dismissed"
                } else {
                    "ignored"
                };
                records.push(AnalysisRecord {
                    reviewer_id: reviewer_id.clone(),
                    assignment_id: assignment_id.clone(),
                    submitted_at: submitted_at.clone(),
                    source_assignment_sha256: source_sha.clone(),
                    app_version: app_version.clone(),
                    schema_version,
                    patient_id: patient_id.clone(),
                    research_id: s(pt, "researchId"),
                    model_id: model_id_ws.clone(),
                    cancer_type: cancer_type.clone(),
                    clinical_question: clinical_question.clone(),
                    patient_status: s(pt, "status").unwrap_or_default(),
                    elapsed_seconds: i(pt, "elapsedSeconds"),
                    note_word_count: i(&attribution, "wordCount"),
                    note_char_count: i(&attribution, "charCount"),
                    pct_physician_original: f(&attribution, "pctPhysicianOriginal"),
                    pct_ai_unmodified: f(&attribution, "pctAiUnmodified"),
                    pct_ai_edited: f(&attribution, "pctAiEdited"),
                    pct_derived_from_llm: f(&attribution, "pctDerivedFromLlm"),
                    chars_typed_by_physician: i(&attribution, "charsTypedByPhysician"),
                    chars_from_llm_unmodified: i(&attribution, "charsFromLlmUnmodified"),
                    chars_from_llm_edited: i(&attribution, "charsFromLlmEdited"),
                    final_note_text: final_note.clone(),
                    recommendation_id: rid,
                    title,
                    temperature_level: temp,
                    temperature_label: temp_label,
                    evidence_tier: ev,
                    risk_score: risk,
                    safety_score: safety,
                    priority_rank: prio,
                    original_ai_text: ai_text,
                    disposition: disposition.to_string(),
                    status: status.clone(),
                    was_used: if disposition == "accepted" { 1 } else { 0 },
                    was_altered: if status == "used-and-edited" { 1 } else { 0 },
                    edit_distance: dec.and_then(|d| d.get("editDistance").and_then(|x| x.as_i64())),
                    similarity_percent: dec.and_then(|d| d.get("similarityPercent").and_then(|x| x.as_f64())),
                    final_text_in_note: dec.and_then(|d| s(d, "finalText")),
                    dismissal_reason: dec.and_then(|d| s(d, "dismissalReason")),
                    decided_at: dec.and_then(|d| s(d, "decidedAt")),
                });
            }
        }
    }
    Ok((records, raw))
}

fn csv_field(s: &str) -> String {
    if s.contains(['"', ',', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn records_to_csv(records: &[AnalysisRecord]) -> String {
    let header = [
        "reviewer_id", "assignment_id", "submitted_at", "source_assignment_sha256", "app_version", "schema_version",
        "patient_id", "research_id", "model_id", "cancer_type", "clinical_question", "patient_status", "elapsed_seconds",
        "note_word_count", "note_char_count", "pct_physician_original", "pct_ai_unmodified", "pct_ai_edited",
        "pct_derived_from_llm", "chars_typed_by_physician", "chars_from_llm_unmodified", "chars_from_llm_edited",
        "recommendation_id", "title", "temperature_level", "temperature_label", "evidence_tier", "risk_score",
        "safety_score", "priority_rank", "disposition", "status", "was_used", "was_altered", "edit_distance",
        "similarity_percent", "original_ai_text", "final_text_in_note", "dismissal_reason", "decided_at", "final_note_text",
    ];
    let os = |o: &Option<String>| o.clone().unwrap_or_default();
    let oi = |o: &Option<i64>| o.map(|v| v.to_string()).unwrap_or_default();
    let of = |o: &Option<f64>| o.map(|v| v.to_string()).unwrap_or_default();

    let mut out = String::new();
    out.push_str(&header.join(","));
    out.push('\n');
    for r in records {
        let cols = [
            r.reviewer_id.clone(), r.assignment_id.clone(), os(&r.submitted_at), os(&r.source_assignment_sha256),
            os(&r.app_version), oi(&r.schema_version), r.patient_id.clone(), os(&r.research_id), os(&r.model_id),
            os(&r.cancer_type), os(&r.clinical_question), r.patient_status.clone(), r.elapsed_seconds.to_string(),
            r.note_word_count.to_string(), r.note_char_count.to_string(), r.pct_physician_original.to_string(),
            r.pct_ai_unmodified.to_string(), r.pct_ai_edited.to_string(), r.pct_derived_from_llm.to_string(),
            r.chars_typed_by_physician.to_string(), r.chars_from_llm_unmodified.to_string(), r.chars_from_llm_edited.to_string(),
            r.recommendation_id.clone(), r.title.clone(), oi(&r.temperature_level), os(&r.temperature_label),
            os(&r.evidence_tier), of(&r.risk_score), of(&r.safety_score), oi(&r.priority_rank), r.disposition.clone(),
            r.status.clone(), r.was_used.to_string(), r.was_altered.to_string(), oi(&r.edit_distance), of(&r.similarity_percent),
            r.original_ai_text.clone(), os(&r.final_text_in_note), os(&r.dismissal_reason), os(&r.decided_at), r.final_note_text.clone(),
        ];
        out.push_str(&cols.iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

// ── helpers ────────────────────────────────────────────────────────────────

fn build_summary(conn: &Connection, provisioned: bool) -> Result<CoordinatorSummary, rusqlite::Error> {
    let study_title: String = conn
        .query_row("SELECT title FROM studies LIMIT 1", [], |r| r.get(0))
        .unwrap_or_else(|_| "AI Tumor Board".to_string());

    let mut ps = conn.prepare(
        "SELECT p.patient_id, p.research_id, p.model_id, p.cancer_type, p.clinical_question,
                (SELECT count(*) FROM source_documents d WHERE d.patient_id = p.patient_id),
                (SELECT count(*) FROM recommendations r WHERE r.patient_id = p.patient_id)
         FROM patients p ORDER BY p.position",
    )?;
    let patients = ps
        .query_map([], |r| {
            Ok(CoordPatient {
                id: r.get(0)?,
                research_id: r.get(1)?,
                model_id: r.get(2)?,
                cancer_type: r.get(3)?,
                clinical_question: r.get(4)?,
                document_count: r.get(5)?,
                recommendation_count: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut rs = conn.prepare(
        "SELECT r.reviewer_id, r.display_name,
                (SELECT count(*) FROM reviewer_assignments ra WHERE ra.reviewer_id = r.reviewer_id)
         FROM reviewers r WHERE r.role = 'reviewer' ORDER BY r.reviewer_id",
    )?;
    let reviewers = rs
        .query_map([], |r| {
            Ok(CoordReviewer {
                reviewer_id: r.get(0)?,
                display_name: r.get(1)?,
                patient_count: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let results_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='imported_responses'",
            [],
            |r| r.get(0),
        )
        .and_then(|exists: i64| {
            if exists == 0 {
                Ok(0)
            } else {
                conn.query_row("SELECT count(*) FROM imported_responses", [], |r| r.get(0))
            }
        })
        .unwrap_or(0);

    Ok(CoordinatorSummary {
        study_title,
        provisioned,
        patients,
        reviewers,
        results_count,
    })
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{:02x}", b);
    }
    s
}

fn hex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect();
    bytes?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::container::{self, ContainerRole};

    fn tmp() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        p.push(format!("atb-agg-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p.join("ws.atb")
    }

    #[test]
    fn aggregates_decisions_across_reviewers() {
        let conn = container::create(&tmp(), "pw", ContainerRole::Coordinator).unwrap();
        conn.execute("INSERT INTO studies(study_id,title,protocol_version,schema_version,created_at) VALUES ('STUDY-1','AI Tumor Board','v1',1,'t')", []).unwrap();
        conn.execute("INSERT INTO patients(patient_id,study_id,research_id,model_id,display_label,position,status,elapsed_seconds) VALUES ('PT-1','STUDY-1','R1','m','L',0,'not_started',0)", []).unwrap();
        conn.execute("INSERT INTO recommendations(recommendation_id,patient_id,position,title,recommendation_text,is_custom) VALUES ('PT-1:1','PT-1',0,'Rec 1','t',0)", []).unwrap();
        conn.execute("INSERT INTO recommendations(recommendation_id,patient_id,position,title,recommendation_text,is_custom) VALUES ('PT-1:2','PT-1',1,'Rec 2','t',0)", []).unwrap();

        let payload = |reviewer: &str, s1: &str, s2: &str, pct: f64| {
            serde_json::json!({
                "header": {"reviewerId": reviewer},
                "patients": [{
                    "patientId": "PT-1",
                    "attribution": {"pctPhysicianOriginal": pct},
                    "decisions": [
                        {"recommendationId":"PT-1:1","status":s1},
                        {"recommendationId":"PT-1:2","status":s2}
                    ]
                }]
            })
        };
        // REV-A: rec1 used, rec2 dismissed. REV-B: rec1 used-and-edited, rec2 pending (ignored).
        let payloads = vec![
            payload("REV-A", "used", "dismissed", 40.0),
            payload("REV-B", "used-and-edited", "pending", 60.0),
        ];
        let agg = aggregate_patients(&conn, &payloads).unwrap();
        assert_eq!(agg.len(), 1);
        let p = &agg[0];
        assert_eq!(p.response_count, 2);
        assert_eq!(p.avg_pct_physician_authored, 50.0);
        let r1 = p.recommendations.iter().find(|r| r.recommendation_id == "PT-1:1").unwrap();
        assert_eq!((r1.accepted, r1.dismissed, r1.ignored), (2, 0, 0));
        let r2 = p.recommendations.iter().find(|r| r.recommendation_id == "PT-1:2").unwrap();
        assert_eq!((r2.accepted, r2.dismissed, r2.ignored), (0, 1, 1));
    }

    #[test]
    fn analysis_export_flattens_records() {
        let conn = container::create(&tmp(), "pw", ContainerRole::Coordinator).unwrap();
        conn.execute("INSERT INTO studies(study_id,title,protocol_version,schema_version,created_at) VALUES ('STUDY-1','AI Tumor Board','v1',1,'t')", []).unwrap();
        conn.execute("INSERT INTO patients(patient_id,study_id,research_id,model_id,cancer_type,clinical_question,display_label,position,status,elapsed_seconds) VALUES ('PT-1','STUDY-1','R1','m1','Lung','Tx?','L',0,'complete',300)", []).unwrap();
        conn.execute("INSERT INTO recommendations(recommendation_id,patient_id,position,title,recommendation_text,evidence_tier,risk_score,is_custom) VALUES ('PT-1:1','PT-1',0,'Rec 1','AI text one','I',2,0)", []).unwrap();
        conn.execute("INSERT INTO recommendations(recommendation_id,patient_id,position,title,recommendation_text,is_custom) VALUES ('PT-1:2','PT-1',1,'Rec 2','AI text two',0)", []).unwrap();
        conn.execute_batch("CREATE TABLE response_payloads(assignment_id TEXT, reviewer_id TEXT, payload_json TEXT);").unwrap();

        let payload = serde_json::json!({
            "header": {"reviewerId":"REV-A","assignmentId":"ASG-1","submittedAt":"2026-07-10T00:00:00Z"},
            "patients": [{
                "patientId":"PT-1","researchId":"R1","status":"complete","elapsedSeconds":300,
                "attribution": {"wordCount":50,"charCount":300,"pctPhysicianOriginal":40.0,"pctAiUnmodified":30.0,"pctAiEdited":30.0,"pctDerivedFromLlm":60.0,"charsTypedByPhysician":120,"charsFromLlmUnmodified":90,"charsFromLlmEdited":90},
                "finalText":"Final note text.",
                "decisions": [
                    {"recommendationId":"PT-1:1","status":"used-and-edited","editDistance":12,"similarityPercent":80.0,"finalText":"edited","decidedAt":"2026-07-10T00:01:00Z"},
                    {"recommendationId":"PT-1:2","status":"dismissed","dismissalReason":"not appropriate","decidedAt":"2026-07-10T00:02:00Z"}
                ]
            }]
        });
        conn.execute("INSERT INTO response_payloads(assignment_id,reviewer_id,payload_json) VALUES ('ASG-1','REV-A',?1)", [payload.to_string()]).unwrap();

        let (records, raw) = build_analysis(&conn).unwrap();
        assert_eq!(raw.len(), 1);
        assert_eq!(records.len(), 2);
        let r1 = records.iter().find(|r| r.recommendation_id == "PT-1:1").unwrap();
        assert_eq!(r1.disposition, "accepted");
        assert_eq!(r1.was_used, 1);
        assert_eq!(r1.was_altered, 1);
        assert_eq!(r1.edit_distance, Some(12));
        assert_eq!(r1.similarity_percent, Some(80.0));
        assert_eq!(r1.original_ai_text, "AI text one");
        assert_eq!(r1.elapsed_seconds, 300);
        assert_eq!(r1.model_id.as_deref(), Some("m1"));
        assert_eq!(r1.final_text_in_note.as_deref(), Some("edited"));
        let r2 = records.iter().find(|r| r.recommendation_id == "PT-1:2").unwrap();
        assert_eq!(r2.disposition, "dismissed");
        assert_eq!(r2.was_used, 0);
        assert_eq!(r2.dismissal_reason.as_deref(), Some("not appropriate"));
        // CSV = header + 2 rows.
        let csv = records_to_csv(&records);
        assert_eq!(csv.lines().count(), 3);
        assert!(csv.starts_with("reviewer_id,assignment_id"));
    }
}
