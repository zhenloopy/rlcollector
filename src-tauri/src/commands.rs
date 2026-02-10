use crate::capture;
use crate::models::{CaptureSession, CaptureStatus, OllamaStatus, Screenshot, Task, TaskUpdate};
use crate::ollama_sidecar::{self, OllamaProcess};
use crate::storage::Database;
use log::{debug, error, info};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tauri::{Manager, State};

pub struct AppState {
    pub db: Database,
    pub capturing: AtomicBool,
    pub capture_interval_ms: AtomicU64,
    pub capture_count: AtomicU64,
    pub screenshots_dir: PathBuf,
    pub current_session_id: AtomicI64,
    pub app_data_dir: PathBuf,
    pub ollama_process: OllamaProcess,
    pub analyzing: AtomicBool,
    pub cancel_analysis: AtomicBool,
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
pub fn start_capture(state: State<'_, Arc<AppState>>, interval_ms: Option<u64>, description: Option<String>) -> Result<(), String> {
    // Guard against spawning multiple capture loops
    if state.capturing.load(Ordering::Relaxed) {
        return Ok(());
    }

    let interval = interval_ms.unwrap_or_else(|| state.capture_interval_ms.load(Ordering::Relaxed));
    info!("Starting capture with interval {}ms", interval);

    if let Some(ms) = interval_ms {
        state.capture_interval_ms.store(ms, Ordering::Relaxed);
    }

    // Create a new capture session
    let session_timestamp = format_timestamp_for_db(SystemTime::now());
    let desc_ref = description.as_deref().filter(|s| !s.trim().is_empty());
    let session_id = state.db.create_session(&session_timestamp, desc_ref)
        .map_err(|e| format!("Failed to create capture session: {}", e))?;
    state.current_session_id.store(session_id, Ordering::Relaxed);
    info!("Created capture session {}", session_id);

    state.capturing.store(true, Ordering::Relaxed);

    // Ensure screenshots directory exists
    std::fs::create_dir_all(&state.screenshots_dir)
        .map_err(|e| {
            error!("Failed to create screenshots directory: {}", e);
            format!("Failed to create screenshots directory: {}", e)
        })?;

    let app_state = Arc::clone(&state);

    let capture_handle = tauri::async_runtime::spawn(async move {
        loop {
            // Check if we should stop
            if !app_state.capturing.load(Ordering::Relaxed) {
                info!("Capture loop stopped");
                break;
            }

            let now = SystemTime::now();
            let filename = format!("screenshot_{}.webp", format_timestamp_for_filename(now));
            let db_timestamp = format_timestamp_for_db(now);

            // Attempt to capture a screenshot
            match capture::capture_screen(&app_state.screenshots_dir, &filename) {
                Ok(_filepath) => {
                    let relative_path = format!("screenshots/{}", filename);
                    let sid = app_state.current_session_id.load(Ordering::Relaxed);
                    let session_opt = if sid > 0 { Some(sid) } else { None };
                    match app_state.db.insert_screenshot(
                        &relative_path,
                        &db_timestamp,
                        None,
                        0,
                        session_opt,
                    ) {
                        Ok(_) => {
                            let count = app_state.capture_count.fetch_add(1, Ordering::Relaxed) + 1;
                            debug!("Screenshot captured: {} (total: {})", filename, count);

                            // Auto-analyze every 10 captures
                            if count % 10 == 0 {
                                let analysis_state = Arc::clone(&app_state);
                                tauri::async_runtime::spawn(async move {
                                    match run_pending_analysis(&analysis_state, 10).await {
                                        Ok(n) if n > 0 => info!("Auto-analyzed {} screenshots", n),
                                        Ok(_) => {}
                                        Err(e) => debug!("Auto-analysis skipped: {}", e),
                                    }
                                });
                            }
                        }
                        Err(e) => {
                            error!("Failed to insert screenshot into DB: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Screenshot capture failed: {}", e);
                }
            }

            // Sleep for the configured interval
            let interval = app_state.capture_interval_ms.load(Ordering::Relaxed);
            tokio::time::sleep(std::time::Duration::from_millis(interval)).await;
        }
    });

    // Monitor the capture task for panics
    tauri::async_runtime::spawn(async move {
        if let Err(e) = capture_handle.await {
            error!("Capture task failed: {}", e);
        }
    });

    Ok(())
}

#[tauri::command]
pub fn stop_capture(state: State<'_, Arc<AppState>>) {
    info!("Stopping capture");
    state.capturing.store(false, Ordering::Relaxed);

    // End the current capture session
    let session_id = state.current_session_id.swap(0, Ordering::Relaxed);
    if session_id > 0 {
        let ended_at = format_timestamp_for_db(SystemTime::now());
        if let Err(e) = state.db.end_session(session_id, &ended_at) {
            error!("Failed to end capture session {}: {}", session_id, e);
        } else {
            info!("Ended capture session {}", session_id);
        }
    }
}

#[tauri::command]
pub fn get_current_session(state: State<'_, Arc<AppState>>) -> Result<Option<CaptureSession>, String> {
    let session_id = state.current_session_id.load(Ordering::Relaxed);
    if session_id <= 0 {
        return Ok(None);
    }
    match state.db.get_session(session_id) {
        Ok(session) => Ok(Some(session)),
        Err(e) => {
            // QueryReturnedNoRows means session doesn't exist
            if e.to_string().contains("Query returned no rows") {
                Ok(None)
            } else {
                Err(e.to_string())
            }
        }
    }
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
pub fn get_log_path(app_handle: tauri::AppHandle) -> Result<String, String> {
    let log_dir = app_handle
        .path()
        .app_log_dir()
        .map_err(|e| format!("Failed to resolve log directory: {}", e))?;
    Ok(log_dir.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn get_sessions(
    state: State<'_, Arc<AppState>>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<CaptureSession>, String> {
    state
        .db
        .get_sessions(limit.unwrap_or(50), offset.unwrap_or(0))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_session_screenshots(
    state: State<'_, Arc<AppState>>,
    session_id: i64,
) -> Result<Vec<Screenshot>, String> {
    state
        .db
        .get_session_screenshots(session_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_screenshots_dir(state: State<'_, Arc<AppState>>) -> String {
    state.screenshots_dir.to_string_lossy().into_owned()
}

/// Core analysis logic, callable from both the Tauri command and the background auto-analysis.
/// `limit` caps how many screenshots to process (0 = all).
async fn run_pending_analysis(state: &AppState, limit: i64) -> Result<u32, String> {
    let provider = state.db.get_setting("ai_provider")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "claude".to_string());

    let image_mode = state.db.get_setting("image_mode")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "downscale".to_string());

    let fetch_limit = if limit > 0 { limit } else { i64::MAX };
    let screenshots = state.db.get_unanalyzed_screenshots(fetch_limit)
        .map_err(|e| e.to_string())?;

    if screenshots.is_empty() {
        return Ok(0);
    }

    // Look up session description from the first screenshot's session
    let session_description: Option<String> = screenshots.first()
        .and_then(|ss| {
            state.db.get_screenshot_session_id(ss.id).ok().flatten()
        })
        .and_then(|sid| {
            state.db.get_session(sid).ok()
        })
        .and_then(|session| session.description);

    info!("Analyzing {} pending screenshots with provider: {}, image_mode: {}, session_desc: {:?}",
        screenshots.len(), provider, image_mode, session_description);

    state.analyzing.store(true, Ordering::Relaxed);
    state.cancel_analysis.store(false, Ordering::Relaxed);

    let client = reqwest::Client::new();
    let mut processed = 0u32;
    let mut last_context: Option<String> = None;

    for screenshot in &screenshots {
        if state.cancel_analysis.load(Ordering::Relaxed) {
            info!("Analysis cancelled by user after {} screenshots", processed);
            break;
        }
        // Resolve the relative DB path against the screenshots directory
        let filename = screenshot.filepath
            .strip_prefix("screenshots/")
            .unwrap_or(&screenshot.filepath);
        let image_path = state.screenshots_dir.join(filename);

        let result = if provider == "ollama" {
            let model = state.db.get_setting("ollama_model")
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| "qwen3-vl:8b".to_string());
            crate::ai::analyze_screenshot_ollama(
                &client,
                &model,
                &image_path,
                last_context.as_deref(),
                session_description.as_deref(),
                &image_mode,
            ).await
        } else {
            let api_key = state.db.get_setting("ai_api_key")
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "No API key configured".to_string())?;
            crate::ai::analyze_screenshot(
                &client,
                &api_key,
                &image_path,
                last_context.as_deref(),
                session_description.as_deref(),
                &image_mode,
            ).await
        };

        match result {
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
                        Err(e) => error!("Failed to insert task: {}", e),
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
                error!("AI analysis failed for screenshot {}: {}", screenshot.id, e);
            }
        }
    }

    state.analyzing.store(false, Ordering::Relaxed);
    info!("Analyzed {} screenshots", processed);
    Ok(processed)
}

#[tauri::command]
pub async fn analyze_pending(state: State<'_, Arc<AppState>>) -> Result<u32, String> {
    run_pending_analysis(&state, 0).await
}

#[tauri::command]
pub fn cancel_analysis(state: State<'_, Arc<AppState>>) {
    info!("Cancelling analysis");
    state.cancel_analysis.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn clear_pending(state: State<'_, Arc<AppState>>) -> Result<u32, String> {
    let paths = state.db.delete_unanalyzed_screenshots()
        .map_err(|e| e.to_string())?;
    let count = paths.len() as u32;

    // Remove files from disk
    for rel_path in &paths {
        let filename = rel_path
            .strip_prefix("screenshots/")
            .unwrap_or(rel_path);
        let full_path = state.screenshots_dir.join(filename);
        if let Err(e) = std::fs::remove_file(&full_path) {
            debug!("Could not remove file {}: {}", full_path.display(), e);
        }
    }

    info!("Cleared {} pending screenshots", count);
    Ok(count)
}

#[tauri::command]
pub async fn check_ollama(state: State<'_, Arc<AppState>>) -> Result<OllamaStatus, String> {
    let client = reqwest::Client::new();
    match crate::ai::check_ollama_connection(&client).await {
        Ok(models) => {
            let source = if state.ollama_process.is_managed() {
                "bundled".to_string()
            } else {
                "external".to_string()
            };
            Ok(OllamaStatus {
                available: true,
                models,
                source,
            })
        }
        Err(_) => Ok(OllamaStatus {
            available: false,
            models: vec![],
            source: String::new(),
        }),
    }
}

#[tauri::command]
pub async fn ensure_ollama(state: State<'_, Arc<AppState>>) -> Result<OllamaStatus, String> {
    let client = reqwest::Client::new();

    // 1. Check if Ollama is already running externally
    if let Ok(models) = crate::ai::check_ollama_connection(&client).await {
        info!("Ollama already running externally");
        return Ok(OllamaStatus {
            available: true,
            models,
            source: "external".to_string(),
        });
    }

    // 2. Find the binary
    let binary_path = OllamaProcess::find_binary(&state.app_data_dir)
        .ok_or_else(|| "Ollama binary not found. Place it in the app data directory or install it on your system PATH.".to_string())?;

    // 3. Start the process
    state.ollama_process.start(&binary_path)?;

    // 4. Wait for it to become ready (20 attempts * 500ms = 10s)
    ollama_sidecar::wait_for_ready(&client, 20).await?;

    // 5. Get model list
    let models = crate::ai::check_ollama_connection(&client)
        .await
        .map_err(|e| format!("Ollama started but failed to connect: {}", e))?;

    info!("Ollama started successfully from {}", binary_path.display());
    Ok(OllamaStatus {
        available: true,
        models,
        source: "bundled".to_string(),
    })
}

#[tauri::command]
pub async fn ollama_pull(model: String) -> Result<(), String> {
    info!("Pulling Ollama model: {}", model);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post("http://localhost:11434/api/pull")
        .json(&serde_json::json!({ "name": model, "stream": false }))
        .send()
        .await
        .map_err(|e| format!("Pull request failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Pull failed: {}", body));
    }

    info!("Successfully pulled model: {}", model);
    Ok(())
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
