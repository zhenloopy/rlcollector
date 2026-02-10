use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screenshot {
    pub id: i64,
    pub filepath: String,
    pub captured_at: String,
    pub active_window_title: Option<String>,
    pub monitor_index: i32,
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
pub struct Settings {
    pub capture_interval_ms: u64,
    pub ai_api_key: Option<String>,
    pub screenshots_dir: String,
    pub compress_to_webp: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSession {
    pub id: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub screenshot_count: i64,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaStatus {
    pub available: bool,
    pub models: Vec<String>,
    pub source: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            capture_interval_ms: 30_000,
            ai_api_key: None,
            screenshots_dir: String::from("screenshots"),
            compress_to_webp: true,
        }
    }
}
