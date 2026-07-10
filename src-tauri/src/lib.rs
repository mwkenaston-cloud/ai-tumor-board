// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod db;

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
        .invoke_handler(tauri::generate_handler![cipher_version])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
