use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screenshot {
    pub id: i64,
    pub filepath: String,
    pub captured_at: String,
    pub active_window_title: Option<String>,
    pub monitor_index: i32,
    pub capture_group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub ai_reasoning: Option<String>,
    pub user_verified: bool,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureStatus {
    pub active: bool,
    pub interval_ms: u64,
    pub count: u64,
    pub monitor_mode: String,
    pub monitors_captured: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskUpdate {
    pub title: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub ended_at: Option<String>,
    pub user_verified: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSession {
    pub id: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub screenshot_count: i64,
    pub description: Option<String>,
    pub title: Option<String>,
    pub unanalyzed_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaStatus {
    pub available: bool,
    pub models: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisStatus {
    pub analyzing: bool,
    pub session_id: Option<i64>,
}
