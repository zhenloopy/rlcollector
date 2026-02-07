use crate::capture;
use crate::models::{CaptureStatus, Task, TaskUpdate};
use crate::storage::Database;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tauri::State;

pub struct AppState {
    pub db: Database,
    pub capturing: AtomicBool,
    pub capture_interval_ms: AtomicU64,
    pub capture_count: AtomicU64,
    pub screenshots_dir: PathBuf,
}

/// Format a SystemTime as an ISO 8601 string suitable for filenames.
/// Uses hyphens instead of colons so the filename is valid on all platforms.
fn format_timestamp_for_filename(time: SystemTime) -> String {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}-{:02}-{:02}",
        year, month, day, hours, minutes, seconds
    )
}

/// Format a SystemTime as an ISO 8601 string for database storage.
fn format_timestamp_for_db(time: SystemTime) -> String {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day).
/// Algorithm based on Howard Hinnant's civil_from_days.
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u64, m, d)
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
pub fn start_capture(state: State<'_, Arc<AppState>>, interval_ms: Option<u64>) -> Result<(), String> {
    // Guard against spawning multiple capture loops
    if state.capturing.load(Ordering::Relaxed) {
        return Ok(());
    }

    if let Some(ms) = interval_ms {
        state.capture_interval_ms.store(ms, Ordering::Relaxed);
    }
    state.capturing.store(true, Ordering::Relaxed);

    // Ensure screenshots directory exists
    std::fs::create_dir_all(&state.screenshots_dir)
        .map_err(|e| format!("Failed to create screenshots directory: {}", e))?;

    let app_state = Arc::clone(&state);

    tokio::spawn(async move {
        loop {
            // Check if we should stop
            if !app_state.capturing.load(Ordering::Relaxed) {
                break;
            }

            let now = SystemTime::now();
            let filename = format!("screenshot_{}.webp", format_timestamp_for_filename(now));
            let db_timestamp = format_timestamp_for_db(now);

            // Attempt to capture a screenshot
            match capture::capture_screen(&app_state.screenshots_dir, &filename) {
                Ok(_filepath) => {
                    let relative_path = format!("screenshots/{}", filename);
                    match app_state.db.insert_screenshot(
                        &relative_path,
                        &db_timestamp,
                        None,
                        0,
                    ) {
                        Ok(_) => {
                            app_state.capture_count.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            eprintln!("Failed to insert screenshot into DB: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Screenshot capture failed: {}", e);
                }
            }

            // Sleep for the configured interval
            let interval = app_state.capture_interval_ms.load(Ordering::Relaxed);
            tokio::time::sleep(std::time::Duration::from_millis(interval)).await;
        }
    });

    Ok(())
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

#[tauri::command]
pub async fn analyze_pending(state: State<'_, Arc<AppState>>) -> Result<u32, String> {
    let api_key = state.db.get_setting("ai_api_key")
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No API key configured".to_string())?;

    let screenshots = state.db.get_unanalyzed_screenshots(10)
        .map_err(|e| e.to_string())?;

    if screenshots.is_empty() {
        return Ok(0);
    }

    let client = reqwest::Client::new();
    let mut processed = 0u32;
    let mut last_context: Option<String> = None;

    for screenshot in &screenshots {
        let path = std::path::Path::new(&screenshot.filepath);
        match crate::ai::analyze_screenshot(
            &client,
            &api_key,
            path,
            last_context.as_deref(),
        ).await {
            Ok(analysis) => {
                if analysis.is_new_task {
                    match state.db.insert_full_task(
                        &analysis.task_title,
                        &analysis.task_description,
                        &analysis.category,
                        &screenshot.captured_at,
                        &analysis.reasoning,
                    ) {
                        Ok(task_id) => {
                            let _ = state.db.link_screenshot_to_task(task_id, screenshot.id);
                        }
                        Err(e) => eprintln!("Failed to insert task: {}", e),
                    }
                } else {
                    if let Ok(tasks) = state.db.get_tasks(1, 0) {
                        if let Some(task) = tasks.first() {
                            let _ = state.db.link_screenshot_to_task(task.id, screenshot.id);
                        }
                    }
                }
                last_context = Some(format!("{}: {}", analysis.task_title, analysis.task_description));
                processed += 1;
            }
            Err(e) => {
                eprintln!("AI analysis failed for screenshot {}: {}", screenshot.id, e);
            }
        }
    }

    Ok(processed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp_for_filename() {
        let epoch = SystemTime::UNIX_EPOCH;
        let result = format_timestamp_for_filename(epoch);
        assert_eq!(result, "1970-01-01T00-00-00");
    }

    #[test]
    fn test_format_timestamp_for_db() {
        let epoch = SystemTime::UNIX_EPOCH;
        let result = format_timestamp_for_db(epoch);
        assert_eq!(result, "1970-01-01T00:00:00");
    }

    #[test]
    fn test_days_to_ymd() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        assert_eq!(days_to_ymd(365), (1971, 1, 1));
        assert_eq!(days_to_ymd(18262), (2020, 1, 1));
    }
}
