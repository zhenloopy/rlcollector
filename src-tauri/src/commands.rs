use crate::capture;
use crate::models::{AnalysisStatus, CaptureSession, CaptureStatus, MonitorInfo, OllamaStatus, Screenshot, Task, TaskUpdate};
use crate::ollama_sidecar::{self, OllamaProcess};
use crate::storage::Database;
use log::{debug, error, info};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

/// Per-monitor state for change detection and summary tracking.
pub struct MonitorState {
    pub last_hash: [u8; 32],
    pub last_summary: String,
    pub name: String,
}

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
    pub analyzing_session_id: AtomicI64,
    pub cancel_analysis: AtomicBool,
    pub monitor_states: Mutex<HashMap<u32, MonitorState>>,
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
    let mode = state
        .db
        .get_setting("capture_monitor_mode")
        .unwrap_or(None)
        .unwrap_or_else(|| "default".to_string());
    let monitors_captured = {
        let ms = state.monitor_states.lock().unwrap();
        ms.len() as u32
    };
    CaptureStatus {
        active: state.capturing.load(Ordering::Relaxed),
        interval_ms: state.capture_interval_ms.load(Ordering::Relaxed),
        count: state.capture_count.load(Ordering::Relaxed),
        monitor_mode: mode,
        monitors_captured,
    }
}

#[tauri::command]
pub fn get_monitors() -> Result<Vec<MonitorInfo>, String> {
    capture::list_monitors().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn start_capture(state: State<'_, Arc<AppState>>, interval_ms: Option<u64>, description: Option<String>, title: Option<String>) -> Result<(), String> {
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
    let title_ref = title.as_deref().filter(|s| !s.trim().is_empty());
    let session_id = state.db.create_session(&session_timestamp, desc_ref, title_ref)
        .map_err(|e| format!("Failed to create capture session: {}", e))?;
    state.current_session_id.store(session_id, Ordering::Relaxed);
    info!("Created capture session {}", session_id);

    state.capturing.store(true, Ordering::Relaxed);

    // Clear monitor states for fresh session
    {
        let mut ms = state.monitor_states.lock().unwrap();
        ms.clear();
    }

    // Ensure screenshots directory exists
    std::fs::create_dir_all(&state.screenshots_dir)
        .map_err(|e| {
            error!("Failed to create screenshots directory: {}", e);
            format!("Failed to create screenshots directory: {}", e)
        })?;

    let app_state = Arc::clone(&state);

    let capture_handle = tauri::async_runtime::spawn(async move {
        loop {
            if !app_state.capturing.load(Ordering::Relaxed) {
                info!("Capture loop stopped");
                break;
            }

            // Read monitor mode settings
            let mode = app_state.db.get_setting("capture_monitor_mode")
                .unwrap_or(None)
                .unwrap_or_else(|| "default".to_string());
            let specific_id: Option<u32> = app_state.db.get_setting("capture_monitor_id")
                .unwrap_or(None)
                .and_then(|v| v.parse().ok());

            let now = SystemTime::now();
            let filename_ts = format_timestamp_for_filename(now);
            let db_timestamp = format_timestamp_for_db(now);
            let capture_group = filename_ts.clone();

            match capture::capture_monitors(&mode, specific_id) {
                Ok(captures) => {
                    let sid = app_state.current_session_id.load(Ordering::Relaxed);
                    let session_opt = if sid > 0 { Some(sid) } else { None };
                    let single = captures.len() == 1;
                    let mut saved_count = 0u32;

                    let mut monitor_states = app_state.monitor_states.lock().unwrap();

                    for cap in &captures {
                        let hash = capture::perceptual_hash(&cap.image);
                        let changed = match monitor_states.get(&cap.monitor_id) {
                            Some(ms) => capture::hash_distance(&hash, &ms.last_hash) >= 10,
                            None => true, // first capture for this monitor
                        };

                        if changed {
                            let filename = if single {
                                format!("screenshot_{}.webp", filename_ts)
                            } else {
                                format!("screenshot_{}_mon{}.webp", filename_ts, cap.monitor_id)
                            };

                            let path = app_state.screenshots_dir.join(&filename);
                            if let Err(e) = capture::save_image_as_webp(&cap.image, &path) {
                                error!("Failed to save screenshot: {}", e);
                                continue;
                            }

                            let relative_path = format!("screenshots/{}", filename);
                            match app_state.db.insert_screenshot(
                                &relative_path,
                                &db_timestamp,
                                None,
                                cap.monitor_id as i32,
                                session_opt,
                                Some(&capture_group),
                            ) {
                                Ok(_) => {
                                    let prev_summary = monitor_states
                                        .get(&cap.monitor_id)
                                        .map(|s| s.last_summary.clone())
                                        .unwrap_or_default();
                                    monitor_states.insert(cap.monitor_id, MonitorState {
                                        last_hash: hash,
                                        last_summary: prev_summary,
                                        name: cap.monitor_name.clone(),
                                    });
                                    saved_count += 1;
                                }
                                Err(e) => error!("Failed to insert screenshot into DB: {}", e),
                            }
                        } else {
                            // Unchanged — just update the hash
                            if let Some(ms) = monitor_states.get_mut(&cap.monitor_id) {
                                ms.last_hash = hash;
                            }
                        }
                    }
                    drop(monitor_states);

                    if saved_count > 0 {
                        let count = app_state.capture_count.fetch_add(saved_count as u64, Ordering::Relaxed) + saved_count as u64;
                        debug!("Captured {} screenshots (total: {})", saved_count, count);

                        // Auto-analysis logic
                        let analysis_mode = app_state.db.get_setting("analysis_mode")
                            .unwrap_or(None)
                            .unwrap_or_else(|| "batch".to_string());
                        let batch_size: u64 = app_state.db.get_setting("batch_size")
                            .unwrap_or(None)
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(10)
                            .max(1)
                            .min(100);

                        let should_analyze = if analysis_mode == "realtime" {
                            !app_state.analyzing.load(Ordering::Relaxed)
                        } else {
                            count % batch_size == 0
                        };

                        if should_analyze {
                            let analysis_state = Arc::clone(&app_state);
                            let session_for_analysis = sid;
                            let limit = if analysis_mode == "realtime" { 1 } else { batch_size as i64 };
                            tauri::async_runtime::spawn(async move {
                                if session_for_analysis > 0 {
                                    match run_session_analysis(&analysis_state, session_for_analysis, limit).await {
                                        Ok(n) if n > 0 => info!("Auto-analyzed {} screenshots for session {}", n, session_for_analysis),
                                        Ok(_) => {}
                                        Err(e) => debug!("Auto-analysis skipped: {}", e),
                                    }
                                }
                            });
                        }
                    }
                }
                Err(e) => {
                    error!("Screenshot capture failed: {}", e);
                }
            }

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

    let session_id = state.current_session_id.swap(0, Ordering::Relaxed);
    if session_id > 0 {
        let ended_at = format_timestamp_for_db(SystemTime::now());
        if let Err(e) = state.db.end_session(session_id, &ended_at) {
            error!("Failed to end capture session {}: {}", session_id, e);
        } else {
            info!("Ended capture session {}", session_id);
        }

        let analysis_state = Arc::clone(&state);
        tauri::async_runtime::spawn(async move {
            match run_session_analysis(&analysis_state, session_id, 0).await {
                Ok(n) if n > 0 => info!("Post-capture analysis: analyzed {} screenshots for session {}", n, session_id),
                Ok(_) => info!("Post-capture analysis: no unanalyzed screenshots for session {}", session_id),
                Err(e) => error!("Post-capture analysis failed for session {}: {}", session_id, e),
            }
        });
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
pub fn get_session_tasks(
    state: State<'_, Arc<AppState>>,
    session_id: i64,
) -> Result<Vec<Task>, String> {
    state
        .db
        .get_session_tasks(session_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_task_for_screenshot(
    state: State<'_, Arc<AppState>>,
    screenshot_id: i64,
) -> Result<Option<Task>, String> {
    state
        .db
        .get_task_for_screenshot(screenshot_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_screenshots_dir(state: State<'_, Arc<AppState>>) -> String {
    state.screenshots_dir.to_string_lossy().into_owned()
}

// --- Analysis pipeline ---

/// Group screenshots by capture_group. Screenshots with no group form individual groups.
fn group_by_capture_group(screenshots: &[Screenshot]) -> Vec<Vec<&Screenshot>> {
    let mut groups: std::collections::BTreeMap<String, Vec<&Screenshot>> = std::collections::BTreeMap::new();
    let mut ungrouped = Vec::new();

    for ss in screenshots {
        match &ss.capture_group {
            Some(group) => groups.entry(group.clone()).or_default().push(ss),
            None => ungrouped.push(ss),
        }
    }

    let mut result: Vec<Vec<&Screenshot>> = groups.into_values().collect();
    for ss in ungrouped {
        result.push(vec![ss]);
    }
    result
}

/// Shared analysis helper: processes screenshots with AI, grouping by capture_group.
async fn analyze_screenshots(
    state: &AppState,
    screenshots: &[crate::models::Screenshot],
    session_id: Option<i64>,
    session_description: Option<&str>,
) -> Result<u32, String> {
    if screenshots.is_empty() {
        return Ok(0);
    }

    let provider = state.db.get_setting("ai_provider")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "claude".to_string());

    let image_mode = state.db.get_setting("image_mode")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "downscale".to_string());

    info!("Analyzing {} screenshots with provider: {}, image_mode: {}, session_desc: {:?}",
        screenshots.len(), provider, image_mode, session_description);

    state.analyzing.store(true, Ordering::Relaxed);
    if let Some(sid) = session_id {
        state.analyzing_session_id.store(sid, Ordering::Relaxed);
    }
    state.cancel_analysis.store(false, Ordering::Relaxed);

    let client = reqwest::Client::new();
    let mut processed = 0u32;

    // Seed recent_contexts from existing tasks in this session
    let mut recent_contexts: std::collections::VecDeque<String> = std::collections::VecDeque::with_capacity(2);
    if let Some(sid) = session_id {
        if let Ok(seed_tasks) = state.db.get_recent_tasks_for_session(sid, 2) {
            for task in &seed_tasks {
                let desc = task.description.as_deref().unwrap_or("");
                recent_contexts.push_back(format!("{}: {}", task.title, desc));
            }
        }
    }

    // Group screenshots by capture_group for multi-monitor awareness
    let groups = group_by_capture_group(screenshots);

    for group in &groups {
        if state.cancel_analysis.load(Ordering::Relaxed) {
            info!("Analysis cancelled by user after {} groups", processed);
            break;
        }

        // Build image paths for this group
        let mut image_infos: Vec<(PathBuf, String, u32, u32, bool)> = Vec::new();
        for ss in group {
            let filename = ss.filepath
                .strip_prefix("screenshots/")
                .unwrap_or(&ss.filepath);
            let path = state.screenshots_dir.join(filename);
            // Use monitor name from monitor_states if available
            let monitor_name = {
                let ms = state.monitor_states.lock().unwrap();
                ms.get(&(ss.monitor_index as u32))
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| format!("Monitor {}", ss.monitor_index))
            };
            image_infos.push((path, monitor_name, 0, 0, false));
        }

        // Build changed monitors list
        let changed: Vec<crate::ai::ChangedMonitor<'_>> = image_infos.iter()
            .map(|(path, name, w, h, primary)| crate::ai::ChangedMonitor {
                monitor_name: name.as_str(),
                image_path: path.as_path(),
                width: *w,
                height: *h,
                is_primary: *primary,
            })
            .collect();

        // Build unchanged monitors list from monitor_states
        let unchanged_data: Vec<(String, String)> = {
            let ms = state.monitor_states.lock().unwrap();
            let group_monitor_ids: std::collections::HashSet<i32> =
                group.iter().map(|ss| ss.monitor_index).collect();
            ms.iter()
                .filter(|(id, _)| !group_monitor_ids.contains(&(**id as i32)))
                .filter(|(_, s)| !s.last_summary.is_empty())
                .map(|(_, s)| (s.name.clone(), s.last_summary.clone()))
                .collect()
        };
        let unchanged: Vec<crate::ai::UnchangedMonitor<'_>> = unchanged_data.iter()
            .map(|(name, summary)| crate::ai::UnchangedMonitor {
                monitor_name: name.as_str(),
                summary: summary.as_str(),
            })
            .collect();

        let contexts_vec: Vec<String> = recent_contexts.iter().cloned().collect();

        let result = if provider == "ollama" {
            let model = state.db.get_setting("ollama_model")
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| "qwen3-vl:8b".to_string());
            crate::ai::analyze_capture_ollama(
                &client, &model, &changed, &unchanged,
                &contexts_vec, session_description, &image_mode,
            ).await
        } else {
            let api_key = state.db.get_setting("ai_api_key")
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "No API key configured".to_string())?;
            crate::ai::analyze_capture(
                &client, &api_key, &changed, &unchanged,
                &contexts_vec, session_description, &image_mode,
            ).await
        };

        match result {
            Ok(analysis) => {
                if analysis.is_new_task {
                    let ts = &group[0].captured_at;
                    match state.db.insert_full_task(
                        &analysis.task_title,
                        &analysis.task_description,
                        &analysis.category,
                        ts,
                        &analysis.reasoning,
                    ) {
                        Ok(task_id) => {
                            for ss in group {
                                let _ = state.db.link_screenshot_to_task(task_id, ss.id);
                            }
                        }
                        Err(e) => error!("Failed to insert task: {}", e),
                    }
                } else {
                    // Link to most recent task
                    if let Ok(tasks) = state.db.get_tasks(1, 0) {
                        if let Some(task) = tasks.first() {
                            for ss in group {
                                let _ = state.db.link_screenshot_to_task(task.id, ss.id);
                            }
                        }
                    }
                }

                // Update monitor_states with returned summaries
                if !analysis.monitor_summaries.is_empty() {
                    let mut ms = state.monitor_states.lock().unwrap();
                    for (name, summary) in &analysis.monitor_summaries {
                        // Find the monitor state by name and update its summary
                        for (_, monitor_state) in ms.iter_mut() {
                            if monitor_state.name == *name {
                                monitor_state.last_summary = summary.clone();
                            }
                        }
                    }
                }

                let new_ctx = format!("{}: {}", analysis.task_title, analysis.task_description);
                recent_contexts.push_front(new_ctx);
                if recent_contexts.len() > 2 {
                    recent_contexts.pop_back();
                }

                processed += 1;
            }
            Err(e) => {
                error!("AI analysis failed for capture group: {}", e);
            }
        }
    }

    state.analyzing.store(false, Ordering::Relaxed);
    state.analyzing_session_id.store(0, Ordering::Relaxed);
    info!("Analyzed {} capture groups", processed);
    Ok(processed)
}

/// Core analysis logic for all unanalyzed screenshots globally.
async fn run_pending_analysis(state: &AppState, limit: i64) -> Result<u32, String> {
    let fetch_limit = if limit > 0 { limit } else { i64::MAX };
    let screenshots = state.db.get_unanalyzed_screenshots(fetch_limit)
        .map_err(|e| e.to_string())?;

    let session_id: Option<i64> = screenshots.first()
        .and_then(|ss| {
            state.db.get_screenshot_session_id(ss.id).ok().flatten()
        });

    let session_description: Option<String> = session_id
        .and_then(|sid| state.db.get_session(sid).ok())
        .and_then(|session| session.description);

    analyze_screenshots(state, &screenshots, session_id, session_description.as_deref()).await
}

/// Session-scoped analysis: process unanalyzed screenshots for a specific session.
async fn run_session_analysis(state: &AppState, session_id: i64, limit: i64) -> Result<u32, String> {
    let fetch_limit = if limit > 0 { limit } else { i64::MAX };
    let screenshots = state.db.get_unanalyzed_screenshots_for_session(session_id, fetch_limit)
        .map_err(|e| e.to_string())?;

    let session_description: Option<String> = state.db.get_session(session_id)
        .ok()
        .and_then(|s| s.description);

    analyze_screenshots(state, &screenshots, Some(session_id), session_description.as_deref()).await
}

#[tauri::command]
pub async fn analyze_pending(state: State<'_, Arc<AppState>>) -> Result<u32, String> {
    run_pending_analysis(&state, 0).await
}

#[tauri::command]
pub async fn analyze_session(state: State<'_, Arc<AppState>>, session_id: i64) -> Result<u32, String> {
    run_session_analysis(&state, session_id, 0).await
}

#[tauri::command]
pub async fn analyze_all_pending(state: State<'_, Arc<AppState>>) -> Result<u32, String> {
    let pending = state.db.get_pending_sessions(100, 0)
        .map_err(|e| e.to_string())?;
    let mut total = 0u32;
    for session in &pending {
        match run_session_analysis(&state, session.id, 0).await {
            Ok(n) => total += n,
            Err(e) => {
                error!("Analysis failed for session {}: {}", session.id, e);
                return Err(e);
            }
        }
    }
    Ok(total)
}

#[tauri::command]
pub fn get_pending_sessions(
    state: State<'_, Arc<AppState>>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<CaptureSession>, String> {
    state
        .db
        .get_pending_sessions(limit.unwrap_or(50), offset.unwrap_or(0))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_completed_sessions(
    state: State<'_, Arc<AppState>>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<CaptureSession>, String> {
    state
        .db
        .get_completed_sessions(limit.unwrap_or(50), offset.unwrap_or(0))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_session(state: State<'_, Arc<AppState>>, session_id: i64) -> Result<u32, String> {
    let paths = state.db.delete_session(session_id)
        .map_err(|e| e.to_string())?;
    let count = paths.len() as u32;

    for rel_path in &paths {
        let filename = rel_path
            .strip_prefix("screenshots/")
            .unwrap_or(rel_path);
        let full_path = state.screenshots_dir.join(filename);
        if let Err(e) = std::fs::remove_file(&full_path) {
            debug!("Could not remove file {}: {}", full_path.display(), e);
        }
    }

    info!("Deleted session {} ({} screenshots removed)", session_id, count);
    Ok(count)
}

#[tauri::command]
pub fn get_analysis_status(state: State<'_, Arc<AppState>>) -> AnalysisStatus {
    let analyzing = state.analyzing.load(Ordering::Relaxed);
    let sid = state.analyzing_session_id.load(Ordering::Relaxed);
    AnalysisStatus {
        analyzing,
        session_id: if analyzing && sid > 0 { Some(sid) } else { None },
    }
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

    if let Ok(models) = crate::ai::check_ollama_connection(&client).await {
        info!("Ollama already running externally");
        return Ok(OllamaStatus {
            available: true,
            models,
            source: "external".to_string(),
        });
    }

    let binary_path = OllamaProcess::find_binary(&state.app_data_dir)
        .ok_or_else(|| "Ollama binary not found. Place it in the app data directory or install it on your system PATH.".to_string())?;

    state.ollama_process.start(&binary_path)?;
    ollama_sidecar::wait_for_ready(&client, 20).await?;

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

#[tauri::command]
pub async fn highlight_monitors(
    app_handle: tauri::AppHandle,
    mode: String,
    monitor_id: Option<u32>,
) -> Result<(), String> {
    // Close any existing highlight windows
    for (label, window) in app_handle.webview_windows() {
        if label.starts_with("highlight_") {
            let _ = window.close();
        }
    }

    // Use Tauri's monitor API for DPI-aware physical coordinates
    let tauri_monitors = app_handle
        .available_monitors()
        .map_err(|e| e.to_string())?;
    let primary = app_handle.primary_monitor().map_err(|e| e.to_string())?;

    if tauri_monitors.is_empty() {
        return Ok(());
    }

    // Select target monitors based on mode
    let targets: Vec<&tauri::Monitor> = match mode.as_str() {
        "default" => {
            if let Some(ref p) = primary {
                vec![p]
            } else {
                tauri_monitors.first().into_iter().collect()
            }
        }
        "active" => {
            let (cx, cy) = capture::get_cursor_position();
            let active: Vec<_> = tauri_monitors
                .iter()
                .filter(|m| {
                    let pos = m.position();
                    let size = m.size();
                    cx >= pos.x
                        && cx < pos.x + size.width as i32
                        && cy >= pos.y
                        && cy < pos.y + size.height as i32
                })
                .collect();
            if active.is_empty() {
                if let Some(ref p) = primary {
                    vec![p]
                } else {
                    vec![]
                }
            } else {
                active
            }
        }
        "all" => tauri_monitors.iter().collect(),
        "specific" => {
            if let Some(id) = monitor_id {
                let xcap_monitors = capture::list_monitors().map_err(|e| e.to_string())?;
                if let Some(xcap_mon) = xcap_monitors.iter().find(|m| m.id == id) {
                    tauri_monitors
                        .iter()
                        .find(|m| m.name().as_deref() == Some(&xcap_mon.name))
                        .into_iter()
                        .collect()
                } else {
                    vec![]
                }
            } else {
                return Ok(());
            }
        }
        _ => return Ok(()),
    };

    if targets.is_empty() {
        return Ok(());
    }

    let mut labels = Vec::new();
    for (i, monitor) in targets.iter().enumerate() {
        let label = format!("highlight_{}", i);
        let url = WebviewUrl::App("overlay.html".into());

        match WebviewWindowBuilder::new(&app_handle, &label, url)
            .transparent(true)
            .background_color(tauri::window::Color(0, 0, 0, 0))
            .decorations(false)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .visible(false)
            .build()
        {
            Ok(window) => {
                let pos = monitor.position();
                let size = monitor.size();
                let _ = window.set_position(tauri::Position::Physical(
                    tauri::PhysicalPosition::new(pos.x, pos.y),
                ));
                let _ = window.set_size(tauri::Size::Physical(
                    tauri::PhysicalSize::new(size.width, size.height),
                ));
                let _ = window.set_ignore_cursor_events(true);
                labels.push(label);
            }
            Err(e) => {
                error!("Failed to create highlight window: {}", e);
            }
        }
    }

    // Brief delay for WebView2 to render content, then show all at once
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    for label in &labels {
        if let Some(window) = app_handle.get_webview_window(label) {
            let _ = window.show();
        }
    }

    // Close overlay windows after 4 seconds
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        for label in &labels {
            if let Some(window) = app_handle.get_webview_window(label) {
                let _ = window.close();
            }
        }
    });

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

    #[test]
    fn test_group_by_capture_group() {
        let screenshots = vec![
            Screenshot {
                id: 1, filepath: "a.webp".to_string(), captured_at: "2025-01-01T10:00:00".to_string(),
                active_window_title: None, monitor_index: 0, capture_group: Some("g1".to_string()),
            },
            Screenshot {
                id: 2, filepath: "b.webp".to_string(), captured_at: "2025-01-01T10:00:00".to_string(),
                active_window_title: None, monitor_index: 1, capture_group: Some("g1".to_string()),
            },
            Screenshot {
                id: 3, filepath: "c.webp".to_string(), captured_at: "2025-01-01T10:00:30".to_string(),
                active_window_title: None, monitor_index: 0, capture_group: Some("g2".to_string()),
            },
            Screenshot {
                id: 4, filepath: "d.webp".to_string(), captured_at: "2025-01-01T10:01:00".to_string(),
                active_window_title: None, monitor_index: 0, capture_group: None,
            },
        ];

        let groups = group_by_capture_group(&screenshots);
        assert_eq!(groups.len(), 3); // g1 (2 items), g2 (1 item), ungrouped (1 item)
        assert_eq!(groups[0].len(), 2); // g1
        assert_eq!(groups[1].len(), 1); // g2
        assert_eq!(groups[2].len(), 1); // ungrouped
    }
}
