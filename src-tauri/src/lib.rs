// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod commands;
mod crypto;
mod db;

use std::sync::Mutex;

// Emitter/Manager are only used in the macOS file-open handler below.
#[cfg(target_os = "macos")]
use tauri::{Emitter, Manager};

use commands::coordinator::{self, CoordinatorState};
use commands::reviewer;
use commands::SessionState;

/// A path handed to the app via file association (double-click an `.atb`), held
/// until the frontend picks it up on startup.
type PendingOpen = Mutex<Option<String>>;

/// Report the SQLCipher version compiled into this build. Used as a startup
/// self-check that encryption support is present.
#[tauri::command]
fn cipher_version() -> Option<String> {
    // A transient in-memory keyed connection is enough to read the pragma.
    let conn = rusqlite::Connection::open_in_memory().ok()?;
    conn.query_row("PRAGMA cipher_version", [], |r| r.get(0))
        .ok()
        .flatten()
}

/// Return (and clear) any assignment file the app was launched with.
#[tauri::command]
fn take_pending_open(state: tauri::State<PendingOpen>) -> Option<String> {
    state.lock().unwrap().take()
}

fn is_assignment(path: &str) -> bool {
    path.ends_with(".atb")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Windows/Linux deliver an associated file as a CLI argument.
    let initial_open = std::env::args().skip(1).find(|a| is_assignment(a));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(SessionState::default())
        .manage(CoordinatorState::default())
        .manage(PendingOpen::new(initial_open))
        .invoke_handler(tauri::generate_handler![
            cipher_version,
            take_pending_open,
            reviewer::open_dev_assignment,
            reviewer::open_assignment,
            reviewer::reset_session,
            reviewer::reset_patient,
            reviewer::load_assignment,
            reviewer::get_patient,
            reviewer::open_patient,
            reviewer::save_note_blocks,
            reviewer::save_elapsed,
            reviewer::append_audit,
            reviewer::save_decision,
            reviewer::complete_patient,
            reviewer::save_survey,
            reviewer::submit_assignment,
            reviewer::export_response,
            reviewer::export_response_to_downloads,
            coordinator::coordinator_open_workspace,
            coordinator::coordinator_summary,
            coordinator::coordinator_add_patient,
            coordinator::coordinator_remove_patient,
            coordinator::coordinator_add_reviewer,
            coordinator::coordinator_assign_patients,
            coordinator::coordinator_delete_reviewer,
            coordinator::coordinator_delete_response,
            coordinator::coordinator_add_document,
            coordinator::coordinator_import_document_file,
            coordinator::coordinator_import_llm,
            coordinator::coordinator_build_package,
            coordinator::coordinator_import_response,
            coordinator::coordinator_list_results,
            coordinator::coordinator_responses,
            coordinator::coordinator_reviewers,
            coordinator::coordinator_export_analysis,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {
            // macOS delivers file-association opens as an Apple event (not argv).
            // `RunEvent::Opened` only exists on macOS, so gate the whole handler.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = _event {
                for url in urls {
                    if let Ok(p) = url.to_file_path() {
                        let path = p.to_string_lossy().to_string();
                        if is_assignment(&path) {
                            if let Some(state) = _app.try_state::<PendingOpen>() {
                                *state.lock().unwrap() = Some(path.clone());
                            }
                            // If the window is already up, tell the UI to open it.
                            let _ = _app.emit("open-assignment-file", path);
                        }
                    }
                }
            }
        });
}
