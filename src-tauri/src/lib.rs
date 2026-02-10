mod ai;
mod capture;
mod commands;
mod models;
mod storage;
mod tray;

use commands::AppState;
use log::info;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64};
use std::sync::Arc;
use tauri_plugin_log::{Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_data_dir = dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("rlcollector");

    if let Err(e) = std::fs::create_dir_all(&app_data_dir) {
        eprintln!("Failed to create app data directory: {}", e);
        return;
    }
    if let Err(e) = std::fs::create_dir_all(app_data_dir.join("screenshots")) {
        eprintln!("Failed to create screenshots directory: {}", e);
        return;
    }

    let db_path = app_data_dir.join("rlcollector.db");
    let db = match storage::Database::new(&db_path) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Failed to open database: {}", e);
            return;
        }
    };

    let state = Arc::new(AppState {
        db,
        capturing: AtomicBool::new(false),
        capture_interval_ms: AtomicU64::new(30_000),
        capture_count: AtomicU64::new(0),
        screenshots_dir: app_data_dir.join("screenshots"),
        current_session_id: AtomicI64::new(0),
    });

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                ])
                .level(log::LevelFilter::Debug)
                .build(),
        )
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
            commands::analyze_pending,
            commands::get_log_path,
            commands::get_sessions,
            commands::get_session_screenshots,
            commands::get_screenshots_dir,
        ])
        .setup(move |app| {
            // Set panic hook here so the log plugin is already initialized
            std::panic::set_hook(Box::new(|info| {
                log::error!("PANIC: {}", info);
            }));

            info!("RLCollector started, data dir: {}", app_data_dir.display());
            tray::setup_tray(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
