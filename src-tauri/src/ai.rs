use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AiError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
    #[error("Failed to read image: {0}")]
    ImageReadFailed(String),
    #[error("API returned error: {0}")]
    ApiError(String),
    #[error("No API key configured")]
    NoApiKey,
}

#[derive(Debug, Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
}

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: Vec<Content>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum Content {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ImageSource },
}

#[derive(Debug, Serialize)]
struct ImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ResponseContent>,
}

#[derive(Debug, Deserialize)]
struct ResponseContent {
    text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TaskAnalysis {
    pub task_title: String,
    pub task_description: String,
    pub category: String,
    pub reasoning: String,
    pub is_new_task: bool,
}

pub async fn analyze_screenshot(
    client: &Client,
    api_key: &str,
    image_path: &Path,
    previous_context: Option<&str>,
) -> Result<TaskAnalysis, AiError> {
    let image_bytes =
        std::fs::read(image_path).map_err(|e| AiError::ImageReadFailed(e.to_string()))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&image_bytes);

    let media_type = if image_path.extension().is_some_and(|e| e == "png") {
        "image/png"
    } else {
        "image/webp"
    };

    let prompt = format!(
        "Analyze this screenshot of a user's screen. Determine what task they are working on.\n\
         {}\n\
         Respond with JSON only, no other text:\n\
         {{\"task_title\": \"short title\", \"task_description\": \"what they're doing\", \
         \"category\": \"coding|browsing|writing|communication|design|other\", \
         \"reasoning\": \"why you think this\", \"is_new_task\": true/false}}",
        previous_context
            .map(|c| format!("Previous task context: {}", c))
            .unwrap_or_default()
    );

    let request = ClaudeRequest {
        model: "claude-sonnet-4-5-20250929".to_string(),
        max_tokens: 1024,
        messages: vec![Message {
            role: "user".to_string(),
            content: vec![
                Content::Image {
                    source: ImageSource {
                        source_type: "base64".to_string(),
                        media_type: media_type.to_string(),
                        data: b64,
                    },
                },
                Content::Text { text: prompt },
            ],
        }],
    };

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AiError::ApiError(format!("{}: {}", status, body)));
    }

    let claude_resp: ClaudeResponse = resp.json().await?;
    let text = claude_resp
        .content
        .first()
        .and_then(|c| c.text.as_ref())
        .ok_or_else(|| AiError::ApiError("Empty response".to_string()))?;

    let analysis: TaskAnalysis =
        serde_json::from_str(text).map_err(|e| AiError::ApiError(format!("Parse error: {}", e)))?;

    Ok(analysis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_analysis_deserialization() {
        let json = r#"{
            "task_title": "Writing code",
            "task_description": "User is editing a Rust file",
            "category": "coding",
            "reasoning": "IDE is open with Rust code",
            "is_new_task": true
        }"#;
        let analysis: TaskAnalysis = serde_json::from_str(json).unwrap();
        assert_eq!(analysis.task_title, "Writing code");
        assert_eq!(analysis.category, "coding");
        assert!(analysis.is_new_task);
    }
}
