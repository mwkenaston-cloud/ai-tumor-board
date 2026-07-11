// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod commands;
mod crypto;
mod db;

use commands::coordinator::{self, CoordinatorState};
use commands::reviewer;
use commands::SessionState;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(SessionState::default())
        .manage(CoordinatorState::default())
        .invoke_handler(tauri::generate_handler![
            cipher_version,
            reviewer::open_dev_assignment,
            reviewer::open_assignment,
            reviewer::load_assignment,
            reviewer::get_patient,
            reviewer::open_patient,
            reviewer::save_note_blocks,
            reviewer::save_decision,
            reviewer::complete_patient,
            reviewer::save_survey,
            reviewer::submit_assignment,
            reviewer::export_response,
            coordinator::coordinator_open_workspace,
            coordinator::coordinator_summary,
            coordinator::coordinator_add_patient,
            coordinator::coordinator_remove_patient,
            coordinator::coordinator_add_document,
            coordinator::coordinator_import_llm,
            coordinator::coordinator_build_package,
            coordinator::coordinator_import_response,
            coordinator::coordinator_list_results,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
