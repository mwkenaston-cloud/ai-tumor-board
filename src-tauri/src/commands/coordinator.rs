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
    pub display_label: String,
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
    let path = dir.join("coordinator-workspace.atb");

    let conn = if path.exists() {
        container::open(&path, DEV_WORKSPACE_PASSWORD).map_err(map_err)?
    } else {
        let c = container::create(&path, DEV_WORKSPACE_PASSWORD, ContainerRole::Coordinator)
            .map_err(map_err)?;
        // Default study + a coordinator X25519 keypair for sealing responses.
        c.execute(
            "INSERT OR REPLACE INTO studies(study_id, title, protocol_version, schema_version, created_at)
             VALUES ('STUDY-1', 'New Study', 'v1', 1, ?1)",
            [now_iso()],
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
    display_label: String,
    clinical_question: String,
) -> Result<String, String> {
    with_session(&state, |s| {
        let count: i64 = s
            .conn
            .query_row("SELECT count(*) FROM patients", [], |r| r.get(0))
            .map_err(map_err)?;
        let patient_id = format!("PT-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        s.conn
            .execute(
                "INSERT INTO patients(patient_id, study_id, research_id, display_label, clinical_question, position, status, elapsed_seconds)
                 VALUES (?1, 'STUDY-1', ?2, ?3, ?4, ?5, 'not_started', 0)",
                params![patient_id, research_id, display_label, clinical_question, count],
            )
            .map_err(map_err)?;
        Ok(patient_id)
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
        packaging::build_package(
            &s.conn,
            std::path::Path::new(&destination),
            &password,
            &reviewer_id,
            &patient_ids,
            &pubkey,
        )
        .map_err(map_err)
    })
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

// ── helpers ────────────────────────────────────────────────────────────────

fn build_summary(conn: &Connection, provisioned: bool) -> Result<CoordinatorSummary, rusqlite::Error> {
    let study_title: String = conn
        .query_row("SELECT title FROM studies LIMIT 1", [], |r| r.get(0))
        .unwrap_or_else(|_| "New Study".to_string());

    let mut ps = conn.prepare(
        "SELECT p.patient_id, p.research_id, p.display_label, p.clinical_question,
                (SELECT count(*) FROM source_documents d WHERE d.patient_id = p.patient_id),
                (SELECT count(*) FROM recommendations r WHERE r.patient_id = p.patient_id)
         FROM patients p ORDER BY p.position",
    )?;
    let patients = ps
        .query_map([], |r| {
            Ok(CoordPatient {
                id: r.get(0)?,
                research_id: r.get(1)?,
                display_label: r.get(2)?,
                clinical_question: r.get(3)?,
                document_count: r.get(4)?,
                recommendation_count: r.get(5)?,
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
