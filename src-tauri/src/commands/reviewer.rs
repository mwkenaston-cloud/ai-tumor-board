//! Reviewer-side commands. Each mutating command commits a transaction to the
//! keyed SQLCipher file, so a force-kill loses at most the last un-flushed
//! change (rollback-journal durability).

use tauri::{AppHandle, Manager, State};

use super::{Session, SessionState};
use crate::crypto::container::{self, ContainerRole};
use crate::db::models::{Assignment, AuditEvent, NoteBlock, Patient, RecommendationDecision};
use crate::db::response::{self, ResponseReceipt};
use crate::db::{repository as repo, seed};

/// DEV convenience password. The real reviewer flow (Phase 3) derives the key
/// from the reviewer's own password entered on the unlock screen; this exists
/// only so `npm run tauri dev` has a persisted assignment to work against.
const DEV_PASSWORD: &str = "dev-password";

fn map_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn app_data_file(app: &AppHandle, name: &str) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(map_err)?;
    std::fs::create_dir_all(&dir).map_err(map_err)?;
    Ok(dir.join(name))
}

/// Open (or create + seed) the local dev assignment and make it the active
/// session. Returns the assignment header for the lobby.
#[tauri::command]
pub fn open_dev_assignment(
    app: AppHandle,
    state: State<SessionState>,
) -> Result<Assignment, String> {
    let reviewer_id = "REV-004".to_string();
    // Filename carries the schema version so a stale dev file from an older
    // schema is simply ignored rather than failing to open.
    let path = app_data_file(&app, "dev-assignment-v2.atb")?;

    let fresh = !path.exists();
    let conn = if fresh {
        let c = container::create(&path, DEV_PASSWORD, ContainerRole::Reviewer).map_err(map_err)?;
        seed::seed_demo(&c, &reviewer_id).map_err(map_err)?;
        c
    } else {
        container::open(&path, DEV_PASSWORD).map_err(map_err)?
    };

    let assignment = repo::load_assignment(&conn, &reviewer_id).map_err(map_err)?;
    *state.lock().unwrap() = Some(Session { conn, path, reviewer_id });
    Ok(assignment)
}

/// Open a real assignment `.atb` with the reviewer's password. Wrong password,
/// truncated, or tampered files fail here without exposing content.
#[tauri::command]
pub fn open_assignment(
    state: State<SessionState>,
    path: String,
    password: String,
) -> Result<Assignment, String> {
    let pathbuf = std::path::PathBuf::from(&path);
    let conn = container::open(&pathbuf, &password).map_err(map_err)?;
    let reviewer_id = repo::first_reviewer_id(&conn).map_err(map_err)?;
    let assignment = repo::load_assignment(&conn, &reviewer_id).map_err(map_err)?;
    repo::append_audit(
        &conn,
        Some(&reviewer_id),
        &AuditEvent { event_type: "ASSIGNMENT_OPENED".into(), patient_id: None, event_time: repo::now_iso(), payload: None },
    )
    .map_err(map_err)?;
    *state.lock().unwrap() = Some(Session { conn, path: pathbuf, reviewer_id });
    Ok(assignment)
}

fn with_session<T>(
    state: &State<SessionState>,
    f: impl FnOnce(&mut Session) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = state.lock().unwrap();
    let session = guard.as_mut().ok_or("no open assignment")?;
    f(session)
}

#[tauri::command]
pub fn load_assignment(state: State<SessionState>) -> Result<Assignment, String> {
    with_session(&state, |s| {
        repo::load_assignment(&s.conn, &s.reviewer_id).map_err(map_err)
    })
}

#[tauri::command]
pub fn get_patient(state: State<SessionState>, patient_id: String) -> Result<Patient, String> {
    with_session(&state, |s| {
        repo::load_patient(&s.conn, &s.reviewer_id, &patient_id).map_err(map_err)
    })
}

/// Mark a patient opened (in_progress + started_at) and return its full record.
#[tauri::command]
pub fn open_patient(state: State<SessionState>, patient_id: String) -> Result<Patient, String> {
    with_session(&state, |s| {
        let existing = repo::load_patient(&s.conn, &s.reviewer_id, &patient_id).map_err(map_err)?;
        let now = repo::now_iso();
        let new_status = if existing.status == "complete" { "reopened" } else { "in_progress" };
        let started = if existing.started_at.is_none() { Some(now.as_str()) } else { None };
        repo::set_patient_status(&s.conn, &patient_id, new_status, started, None, None)
            .map_err(map_err)?;
        repo::append_audit(
            &s.conn,
            Some(&s.reviewer_id),
            &AuditEvent { event_type: "PATIENT_OPENED".into(), patient_id: Some(patient_id.clone()), event_time: now, payload: None },
        )
        .map_err(map_err)?;
        repo::load_patient(&s.conn, &s.reviewer_id, &patient_id).map_err(map_err)
    })
}

#[tauri::command]
pub fn save_note_blocks(
    state: State<SessionState>,
    patient_id: String,
    blocks: Vec<NoteBlock>,
) -> Result<(), String> {
    with_session(&state, |s| {
        repo::save_note_blocks(&mut s.conn, &s.reviewer_id, &patient_id, &blocks).map_err(map_err)
    })
}

/// Persist accumulated review time for a patient (called when leaving the
/// review screen without completing, so time survives queue in/out).
#[tauri::command]
pub fn save_elapsed(
    state: State<SessionState>,
    patient_id: String,
    elapsed_seconds: i64,
) -> Result<(), String> {
    with_session(&state, |s| {
        repo::set_elapsed(&s.conn, &patient_id, elapsed_seconds).map_err(map_err)
    })
}

#[tauri::command]
pub fn save_decision(
    state: State<SessionState>,
    decision: RecommendationDecision,
) -> Result<(), String> {
    with_session(&state, |s| {
        repo::upsert_decision(&s.conn, &s.reviewer_id, &decision).map_err(map_err)
    })
}

#[tauri::command]
pub fn complete_patient(
    app: AppHandle,
    state: State<SessionState>,
    patient_id: String,
    elapsed_seconds: i64,
) -> Result<(), String> {
    with_session(&state, |s| {
        let now = repo::now_iso();
        repo::set_patient_status(&s.conn, &patient_id, "complete", None, Some(&now), Some(elapsed_seconds))
            .map_err(map_err)?;
        repo::append_audit(
            &s.conn,
            Some(&s.reviewer_id),
            &AuditEvent { event_type: "PATIENT_COMPLETED".into(), patient_id: Some(patient_id.clone()), event_time: now, payload: None },
        )
        .map_err(map_err)
    })?;
    write_recovery_copy(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn save_survey(
    state: State<SessionState>,
    patient_id: Option<String>,
    answers: serde_json::Value,
) -> Result<(), String> {
    with_session(&state, |s| {
        repo::save_survey(
            &s.conn,
            &s.reviewer_id,
            patient_id.as_deref(),
            if patient_id.is_some() { "per_patient" } else { "general" },
            &answers.to_string(),
        )
        .map_err(map_err)?;
        repo::append_audit(
            &s.conn,
            Some(&s.reviewer_id),
            &AuditEvent { event_type: "SURVEY_COMPLETED".into(), patient_id: patient_id.clone(), event_time: repo::now_iso(), payload: None },
        )
        .map_err(map_err)
    })
}

#[tauri::command]
pub fn submit_assignment(
    app: AppHandle,
    state: State<SessionState>,
    general_answers: serde_json::Value,
) -> Result<(), String> {
    with_session(&state, |s| {
        repo::save_survey(&s.conn, &s.reviewer_id, None, "general", &general_answers.to_string())
            .map_err(map_err)?;
        repo::set_assignment_state(&s.conn, &s.reviewer_id, "submitted").map_err(map_err)?;
        repo::append_audit(
            &s.conn,
            Some(&s.reviewer_id),
            &AuditEvent { event_type: "ASSIGNMENT_SUBMITTED".into(), patient_id: None, event_time: repo::now_iso(), payload: None },
        )
        .map_err(map_err)
    })?;
    write_recovery_copy(&app, &state);
    Ok(())
}

/// Build and seal a `.atbr` response file at `destination`, using the
/// assignment id and coordinator public key embedded in the assignment package.
#[tauri::command]
pub fn export_response(
    state: State<SessionState>,
    destination: String,
) -> Result<ResponseReceipt, String> {
    with_session(&state, |s| {
        let assignment_id = repo::get_metadata(&s.conn, "assignment_id")
            .map_err(map_err)?
            .ok_or("assignment is missing an assignment id")?;
        let pub_hex = repo::get_metadata(&s.conn, "coordinator_public_hex")
            .map_err(map_err)?
            .ok_or("assignment has no coordinator key; rebuild the package")?;
        let pubkey = hex32(&pub_hex).ok_or("coordinator key is malformed")?;
        response::build_response(
            &s.conn,
            &s.path,
            std::path::Path::new(&destination),
            &assignment_id,
            &pubkey,
        )
        .map_err(map_err)
    })
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

/// Copy the current assignment file into a rotating recovery slot in app-data.
/// Since the file is SQLCipher-encrypted, the recovery copy is also encrypted —
/// no plaintext ever lands on disk.
fn write_recovery_copy(app: &AppHandle, state: &State<SessionState>) {
    let src = { state.lock().unwrap().as_ref().map(|s| s.path.clone()) };
    let Some(src) = src else { return };
    let Ok(dir) = app.path().app_data_dir() else { return };
    let recovery = dir.join("recovery");
    if std::fs::create_dir_all(&recovery).is_err() {
        return;
    }
    let current = recovery.join("assignment-current.atb");
    let previous = recovery.join("assignment-previous.atb");
    if current.exists() {
        let _ = std::fs::rename(&current, &previous);
    }
    let _ = std::fs::copy(&src, &current);
}
