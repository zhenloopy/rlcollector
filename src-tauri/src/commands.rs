use crate::models::{CaptureStatus, Task, TaskUpdate};
use crate::storage::Database;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::State;

pub struct AppState {
    pub db: Database,
    pub capturing: AtomicBool,
    pub capture_interval_ms: AtomicU64,
    pub capture_count: AtomicU64,
}

#[tauri::command]
pub fn get_capture_status(state: State<'_, Arc<AppState>>) -> CaptureStatus {
    CaptureStatus {
        active: state.capturing.load(Ordering::Relaxed),
        interval_ms: state.capture_interval_ms.load(Ordering::Relaxed),
        count: state.capture_count.load(Ordering::Relaxed),
    }
}

#[tauri::command]
pub fn start_capture(state: State<'_, Arc<AppState>>, interval_ms: Option<u64>) {
    if let Some(ms) = interval_ms {
        state.capture_interval_ms.store(ms, Ordering::Relaxed);
    }
    state.capturing.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn stop_capture(state: State<'_, Arc<AppState>>) {
    state.capturing.store(false, Ordering::Relaxed);
}

#[tauri::command]
pub fn get_tasks(
    state: State<'_, Arc<AppState>>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<Task>, String> {
    state
        .db
        .get_tasks(limit.unwrap_or(50), offset.unwrap_or(0))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_task(state: State<'_, Arc<AppState>>, id: i64) -> Result<Task, String> {
    state.db.get_task(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_task(
    state: State<'_, Arc<AppState>>,
    id: i64,
    update: TaskUpdate,
) -> Result<(), String> {
    state.db.update_task(id, &update).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_task(state: State<'_, Arc<AppState>>, id: i64) -> Result<(), String> {
    state.db.delete_task(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_setting(state: State<'_, Arc<AppState>>, key: String) -> Result<Option<String>, String> {
    state.db.get_setting(&key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_setting(
    state: State<'_, Arc<AppState>>,
    key: String,
    value: String,
) -> Result<(), String> {
    state.db.set_setting(&key, &value).map_err(|e| e.to_string())
}
