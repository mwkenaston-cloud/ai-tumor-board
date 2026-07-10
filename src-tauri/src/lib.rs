// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod commands;
mod crypto;
mod db;

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
