mod ai;
mod capture;
mod commands;
mod models;
mod storage;
mod tray;

use commands::AppState;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_data_dir = dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("rlcollector");

    std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data directory");
    std::fs::create_dir_all(app_data_dir.join("screenshots"))
        .expect("Failed to create screenshots directory");

    let db_path = app_data_dir.join("rlcollector.db");
    let db = storage::Database::new(&db_path).expect("Failed to open database");

    let state = Arc::new(AppState {
        db,
        capturing: AtomicBool::new(false),
        capture_interval_ms: AtomicU64::new(30_000),
        capture_count: AtomicU64::new(0),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_capture_status,
            commands::start_capture,
            commands::stop_capture,
            commands::get_tasks,
            commands::get_task,
            commands::update_task,
            commands::delete_task,
            commands::get_setting,
            commands::update_setting,
        ])
        .setup(|app| {
            tray::setup_tray(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
