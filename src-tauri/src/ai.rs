use base64::Engine;
use log::{error, info};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;
use crate::capture;

#[derive(Error, Debug)]
pub enum AiError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
    #[error("Failed to read image: {0}")]
    ImageReadFailed(String),
    #[error("API returned error: {0}")]
    ApiError(String),
    #[error("Ollama is not available: {0}")]
    OllamaUnavailable(String),
}

#[derive(Debug, Serialize)]
pub(crate) struct ClaudeRequest {
    pub(crate) model: String,
    pub(crate) max_tokens: u32,
    pub(crate) messages: Vec<Message>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Message {
    pub(crate) role: String,
    pub(crate) content: Vec<Content>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub(crate) enum Content {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ImageSource },
}

#[derive(Debug, Serialize)]
pub(crate) struct ImageSource {
    #[serde(rename = "type")]
    pub(crate) source_type: String,
    pub(crate) media_type: String,
    pub(crate) data: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeResponse {
    pub(crate) content: Vec<ResponseContent>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResponseContent {
    pub(crate) text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TaskAnalysis {
    pub task_title: String,
    pub task_description: String,
    pub category: String,
    pub reasoning: String,
    pub is_new_task: bool,
    #[serde(default)]
    pub monitor_summaries: HashMap<String, String>,
}

/// Info about a changed monitor whose image will be sent to the AI.
pub struct ChangedMonitor<'a> {
    pub monitor_name: &'a str,
    pub image_path: &'a Path,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

/// Info about an unchanged monitor (text summary only).
pub struct UnchangedMonitor<'a> {
    pub monitor_name: &'a str,
    pub summary: &'a str,
}

/// Load an image from disk, apply preprocessing based on image_mode, and return base64 + media type.
fn preprocess_and_encode(image_path: &Path, image_mode: &str) -> Result<(String, &'static str), AiError> {
    let raw_bytes = std::fs::read(image_path).map_err(|e| {
        error!("Failed to read image {}: {}", image_path.display(), e);
        AiError::ImageReadFailed(e.to_string())
    })?;

    let img = image::load_from_memory(&raw_bytes)
        .map_err(|e| AiError::ImageReadFailed(format!("Failed to decode image: {}", e)))?
        .to_rgba8();

    let processed = match image_mode {
        "active_window" => {
            let cropped = capture::crop_active_window(&img);
            capture::resize_for_analysis(&cropped, 1280)
        }
        _ => capture::resize_for_analysis(&img, 1280),
    };

    let webp_bytes = capture::encode_webp_bytes(&processed)
        .map_err(|e| AiError::ImageReadFailed(format!("Failed to encode preprocessed image: {}", e)))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&webp_bytes);
    Ok((b64, "image/webp"))
}

// --- Prompt builders ---

/// Build the analysis prompt for single-monitor mode.
fn build_prompt(previous_contexts: &[String], session_description: Option<&str>) -> String {
    let context_section = build_context_section(previous_contexts);

    if let Some(desc) = session_description {
        format!(
            "The user is working on: {desc}. \
             Look at this screenshot and briefly describe what specific step or subtask they are currently on.\n\
             {context_section}\
             Respond with JSON only, no other text:\n\
             {{\"task_title\": \"short title\", \"task_description\": \"what they're doing\", \
             \"category\": \"coding|browsing|writing|communication|design|other\", \
             \"reasoning\": \"why you think this\", \"is_new_task\": true/false}}"
        )
    } else {
        format!(
            "Analyze this screenshot of a user's screen. Determine what task they are working on.\n\
             {context_section}\
             Respond with JSON only, no other text:\n\
             {{\"task_title\": \"short title\", \"task_description\": \"what they're doing\", \
             \"category\": \"coding|browsing|writing|communication|design|other\", \
             \"reasoning\": \"why you think this\", \"is_new_task\": true/false}}"
        )
    }
}

/// Build the analysis prompt for multi-monitor mode (Claude).
fn build_multi_prompt(
    changed: &[ChangedMonitor<'_>],
    unchanged: &[UnchangedMonitor<'_>],
    previous_contexts: &[String],
    session_description: Option<&str>,
    total_monitors: usize,
) -> String {
    let context_section = build_context_section(previous_contexts);

    let mut monitors_section = String::new();

    // Changed monitors (images attached)
    monitors_section.push_str("MONITORS WITH NEW SCREENSHOTS (images attached in order):\n");
    for (i, cm) in changed.iter().enumerate() {
        let primary_tag = if cm.is_primary { ", primary" } else { "" };
        monitors_section.push_str(&format!(
            "- Monitor \"{}\" ({}x{}{}): see image {}\n",
            cm.monitor_name, cm.width, cm.height, primary_tag, i + 1
        ));
    }

    // Unchanged monitors (text summaries)
    if !unchanged.is_empty() {
        monitors_section.push_str("\nUNCHANGED MONITORS (text summary from last capture):\n");
        for um in unchanged {
            monitors_section.push_str(&format!(
                "- Monitor \"{}\": {}\n",
                um.monitor_name, um.summary
            ));
        }
    }

    let session_ctx = if let Some(desc) = session_description {
        format!("The user is working on: {}.\n", desc)
    } else {
        String::new()
    };

    // Build monitor_summaries keys for the JSON schema
    let monitor_names: Vec<String> = changed.iter().map(|m| m.monitor_name.to_string())
        .chain(unchanged.iter().map(|m| m.monitor_name.to_string()))
        .collect();
    let summaries_example: String = monitor_names.iter()
        .map(|n| format!("\"{}\": \"1-sentence description\"", n))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "You are analyzing a multi-monitor desktop capture taken at a single moment.\n\
         The user has {total_monitors} monitors.\n\n\
         {monitors_section}\n\
         {session_ctx}\
         {context_section}\
         Analyze what the user is doing across all monitors. Focus on the changed \
         monitor(s) — a change on any monitor may indicate a task switch.\n\n\
         Respond with JSON only, no other text:\n\
         {{\"task_title\": \"short title\", \"task_description\": \"what they're doing\", \
         \"category\": \"coding|browsing|writing|communication|design|other\", \
         \"reasoning\": \"why you think this\", \"is_new_task\": true/false, \
         \"monitor_summaries\": {{{summaries_example}}}}}"
    )
}

fn build_context_section(previous_contexts: &[String]) -> String {
    if previous_contexts.is_empty() {
        return String::new();
    }
    let mut section = String::from("Recent task history (most recent first):\n");
    for (i, ctx) in previous_contexts.iter().enumerate() {
        section.push_str(&format!("  {}. {}\n", i + 1, ctx));
    }
    section.push_str("Use this context to decide if the current screenshot shows a continuation of a recent task or a new one.\n");
    section
}

/// Strip markdown code fences from AI response text.
fn strip_code_fences(text: &str) -> &str {
    let cleaned = text.trim();
    if cleaned.starts_with("```") {
        let stripped = cleaned
            .strip_prefix("```json")
            .or_else(|| cleaned.strip_prefix("```"))
            .unwrap_or(cleaned);
        stripped.strip_suffix("```").unwrap_or(stripped).trim()
    } else {
        cleaned
    }
}

// --- Claude API ---

/// Analyze one or more monitor captures using the Claude API.
/// For single-monitor: pass one image in `changed`, empty `unchanged`.
/// For multi-monitor: pass changed images + unchanged summaries.
pub async fn analyze_capture(
    client: &Client,
    api_key: &str,
    changed: &[ChangedMonitor<'_>],
    unchanged: &[UnchangedMonitor<'_>],
    previous_contexts: &[String],
    session_description: Option<&str>,
    image_mode: &str,
) -> Result<TaskAnalysis, AiError> {
    if changed.is_empty() {
        return Err(AiError::ApiError("No images to analyze".to_string()));
    }

    let is_multi = changed.len() > 1 || !unchanged.is_empty();
    let total_monitors = changed.len() + unchanged.len();

    info!(
        "Analyzing capture (Claude): {} changed, {} unchanged monitors",
        changed.len(),
        unchanged.len()
    );

    // Build content: images first, then prompt text
    let mut content = Vec::new();
    for cm in changed {
        let (b64, media_type) = preprocess_and_encode(cm.image_path, image_mode)?;
        content.push(Content::Image {
            source: ImageSource {
                source_type: "base64".to_string(),
                media_type: media_type.to_string(),
                data: b64,
            },
        });
    }

    let prompt = if is_multi {
        build_multi_prompt(changed, unchanged, previous_contexts, session_description, total_monitors)
    } else {
        build_prompt(previous_contexts, session_description)
    };
    content.push(Content::Text { text: prompt });

    let request = ClaudeRequest {
        model: "claude-sonnet-4-5-20250929".to_string(),
        max_tokens: 1024,
        messages: vec![Message {
            role: "user".to_string(),
            content,
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
        error!("Claude API error {}: {}", status, body);
        return Err(AiError::ApiError(format!("{}: {}", status, body)));
    }

    let claude_resp: ClaudeResponse = resp.json().await?;
    let text = claude_resp
        .content
        .first()
        .and_then(|c| c.text.as_ref())
        .ok_or_else(|| AiError::ApiError("Empty response".to_string()))?;

    info!("Raw AI response text: {}", text);
    let cleaned = strip_code_fences(text);

    let analysis: TaskAnalysis = serde_json::from_str(cleaned).map_err(|e| {
        error!("Failed to parse AI response: {} — raw text: {}", e, cleaned);
        AiError::ApiError(format!("Parse error: {}", e))
    })?;

    Ok(analysis)
}

// --- Ollama types and functions ---

#[derive(Debug, Serialize)]
pub(crate) struct OllamaRequest {
    pub(crate) model: String,
    pub(crate) messages: Vec<OllamaMessage>,
    pub(crate) stream: bool,
    pub(crate) format: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) options: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub(crate) struct OllamaMessage {
    pub(crate) role: String,
    pub(crate) content: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) images: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OllamaResponse {
    pub(crate) message: OllamaResponseMessage,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OllamaResponseMessage {
    pub(crate) content: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OllamaTagsResponse {
    pub(crate) models: Vec<OllamaModelInfo>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OllamaModelInfo {
    pub(crate) name: String,
}

/// Build Ollama prompt for multi-monitor (same structure as Claude but references format field).
fn build_multi_prompt_ollama(
    changed: &[ChangedMonitor<'_>],
    unchanged: &[UnchangedMonitor<'_>],
    previous_contexts: &[String],
    session_description: Option<&str>,
    total_monitors: usize,
) -> String {
    let context_section = build_context_section(previous_contexts);

    let mut monitors_section = String::new();
    monitors_section.push_str("MONITORS WITH NEW SCREENSHOTS (images attached in order):\n");
    for (i, cm) in changed.iter().enumerate() {
        let primary_tag = if cm.is_primary { ", primary" } else { "" };
        monitors_section.push_str(&format!(
            "- Monitor \"{}\" ({}x{}{}): see image {}\n",
            cm.monitor_name, cm.width, cm.height, primary_tag, i + 1
        ));
    }
    if !unchanged.is_empty() {
        monitors_section.push_str("\nUNCHANGED MONITORS (text summary from last capture):\n");
        for um in unchanged {
            monitors_section.push_str(&format!(
                "- Monitor \"{}\": {}\n",
                um.monitor_name, um.summary
            ));
        }
    }

    let session_ctx = if let Some(desc) = session_description {
        format!("The user is working on: {}.\n", desc)
    } else {
        String::new()
    };

    format!(
        "You are analyzing a multi-monitor desktop capture taken at a single moment.\n\
         The user has {total_monitors} monitors.\n\n\
         {monitors_section}\n\
         {session_ctx}\
         {context_section}\
         Analyze what the user is doing across all monitors. Focus on the changed \
         monitor(s).\n\n\
         Respond with JSON matching the schema provided in the format field."
    )
}

/// Analyze one or more monitor captures using Ollama.
pub async fn analyze_capture_ollama(
    client: &Client,
    model: &str,
    changed: &[ChangedMonitor<'_>],
    unchanged: &[UnchangedMonitor<'_>],
    previous_contexts: &[String],
    session_description: Option<&str>,
    image_mode: &str,
) -> Result<TaskAnalysis, AiError> {
    if changed.is_empty() {
        return Err(AiError::ApiError("No images to analyze".to_string()));
    }

    let is_multi = changed.len() > 1 || !unchanged.is_empty();
    let total_monitors = changed.len() + unchanged.len();

    info!(
        "Analyzing capture (Ollama {}): {} changed, {} unchanged monitors",
        model,
        changed.len(),
        unchanged.len()
    );

    // Encode all images
    let mut b64_images = Vec::new();
    for cm in changed {
        let (b64, _) = preprocess_and_encode(cm.image_path, image_mode)?;
        b64_images.push(b64);
    }

    let prompt = if is_multi {
        build_multi_prompt_ollama(changed, unchanged, previous_contexts, session_description, total_monitors)
    } else {
        let context_section = build_context_section(previous_contexts);
        if let Some(desc) = session_description {
            format!(
                "The user is working on: {desc}. \
                 Look at this screenshot and briefly describe what specific step or subtask they are currently on.\n\
                 {context_section}\
                 Respond with JSON matching the schema provided in the format field."
            )
        } else {
            format!(
                "Analyze this screenshot of a user's screen. Determine what task they are working on.\n\
                 {context_section}\
                 Respond with JSON matching the schema provided in the format field."
            )
        }
    };

    let mut format_properties = serde_json::json!({
        "task_title": { "type": "string" },
        "task_description": { "type": "string" },
        "category": { "type": "string", "enum": ["coding", "browsing", "writing", "communication", "design", "other"] },
        "reasoning": { "type": "string" },
        "is_new_task": { "type": "boolean" }
    });
    let mut required = vec!["task_title", "task_description", "category", "reasoning", "is_new_task"];

    if is_multi {
        format_properties.as_object_mut().unwrap().insert(
            "monitor_summaries".to_string(),
            serde_json::json!({ "type": "object" }),
        );
        required.push("monitor_summaries");
    }

    let format_schema = serde_json::json!({
        "type": "object",
        "properties": format_properties,
        "required": required
    });

    let request = OllamaRequest {
        model: model.to_string(),
        messages: vec![OllamaMessage {
            role: "user".to_string(),
            content: prompt,
            images: b64_images,
        }],
        stream: false,
        format: format_schema,
        options: Some(serde_json::json!({
            "temperature": 0.3,
            "num_predict": 512,
            "num_ctx": 8192
        })),
    };

    let max_attempts = 2;
    for attempt in 1..=max_attempts {
        let resp = client
            .post("http://localhost:11434/api/chat")
            .json(&request)
            .send()
            .await
            .map_err(|e| AiError::OllamaUnavailable(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            error!("Ollama API error {}: {}", status, body);
            return Err(AiError::ApiError(format!("{}: {}", status, body)));
        }

        let ollama_resp: OllamaResponse = resp.json().await?;
        let content = &ollama_resp.message.content;
        info!("Raw Ollama response: {}", content);

        if content.trim().is_empty() {
            if attempt < max_attempts {
                info!(
                    "Ollama returned empty response (attempt {}/{}), retrying after delay...",
                    attempt, max_attempts
                );
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                continue;
            }
            error!(
                "Ollama returned empty response after {} attempts",
                max_attempts
            );
            return Err(AiError::ApiError(
                "Ollama returned empty response (possible VRAM pressure)".to_string(),
            ));
        }

        let analysis: TaskAnalysis = serde_json::from_str(content).map_err(|e| {
            error!(
                "Failed to parse Ollama response: {} — raw text: {}",
                e, content
            );
            AiError::ApiError(format!("Parse error: {}", e))
        })?;

        return Ok(analysis);
    }

    Err(AiError::ApiError("Ollama analysis failed".to_string()))
}

pub async fn check_ollama_connection(client: &Client) -> Result<Vec<String>, AiError> {
    let resp = client
        .get("http://localhost:11434/api/tags")
        .send()
        .await
        .map_err(|e| AiError::OllamaUnavailable(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(AiError::OllamaUnavailable(format!(
            "HTTP {}",
            resp.status()
        )));
    }

    let tags: OllamaTagsResponse = resp.json().await?;
    Ok(tags.models.into_iter().map(|m| m.name).collect())
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
        assert!(analysis.monitor_summaries.is_empty());
    }

    #[test]
    fn test_task_analysis_with_monitor_summaries() {
        let json = r#"{
            "task_title": "Writing code",
            "task_description": "User is editing a Rust file",
            "category": "coding",
            "reasoning": "IDE is open with Rust code",
            "is_new_task": true,
            "monitor_summaries": {
                "DISPLAY1": "VS Code with Rust file open",
                "DISPLAY2": "Browser showing documentation"
            }
        }"#;
        let analysis: TaskAnalysis = serde_json::from_str(json).unwrap();
        assert_eq!(analysis.monitor_summaries.len(), 2);
        assert_eq!(
            analysis.monitor_summaries.get("DISPLAY1").unwrap(),
            "VS Code with Rust file open"
        );
    }

    #[test]
    fn test_claude_request_serialization() {
        let request = ClaudeRequest {
            model: "claude-sonnet-4-5-20250929".to_string(),
            max_tokens: 1024,
            messages: vec![Message {
                role: "user".to_string(),
                content: vec![
                    Content::Image {
                        source: ImageSource {
                            source_type: "base64".to_string(),
                            media_type: "image/png".to_string(),
                            data: "dGVzdA==".to_string(),
                        },
                    },
                    Content::Text {
                        text: "Analyze this screenshot".to_string(),
                    },
                ],
            }],
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["model"], "claude-sonnet-4-5-20250929");
        assert_eq!(json["max_tokens"], 1024);
        assert_eq!(json["messages"].as_array().unwrap().len(), 1);
        let message = &json["messages"][0];
        assert_eq!(message["content"].as_array().unwrap().len(), 2);
        assert_eq!(message["content"][0]["type"], "image");
        assert_eq!(message["content"][1]["type"], "text");
    }

    #[test]
    fn test_ollama_request_serialization() {
        let request = OllamaRequest {
            model: "qwen3-vl:8b".to_string(),
            messages: vec![OllamaMessage {
                role: "user".to_string(),
                content: "Analyze this screenshot".to_string(),
                images: vec!["dGVzdA==".to_string()],
            }],
            stream: false,
            format: serde_json::json!({"type": "object"}),
            options: Some(serde_json::json!({"temperature": 0.3, "num_predict": 256})),
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["model"], "qwen3-vl:8b");
        assert_eq!(json["stream"], false);
        assert_eq!(json["messages"][0]["images"][0], "dGVzdA==");
    }

    #[test]
    fn test_ollama_response_deserialization() {
        let json = r#"{
            "message": {
                "role": "assistant",
                "content": "{\"task_title\":\"Writing code\",\"task_description\":\"Editing Rust\",\"category\":\"coding\",\"reasoning\":\"IDE open\",\"is_new_task\":true}"
            }
        }"#;
        let resp: OllamaResponse = serde_json::from_str(json).unwrap();
        let analysis: TaskAnalysis = serde_json::from_str(&resp.message.content).unwrap();
        assert_eq!(analysis.task_title, "Writing code");
        assert!(analysis.is_new_task);
    }

    #[test]
    fn test_ollama_tags_deserialization() {
        let json = r#"{"models": [{"name": "qwen3-vl:8b"}, {"name": "llama3:8b"}]}"#;
        let tags: OllamaTagsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(tags.models.len(), 2);
    }

    #[test]
    fn test_ollama_message_skips_empty_images() {
        let msg = OllamaMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
            images: vec![],
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert!(json.get("images").is_none());
    }

    #[test]
    fn test_empty_response_handling() {
        let empty_response = ClaudeResponse { content: vec![] };
        let text = empty_response
            .content
            .first()
            .and_then(|c| c.text.as_ref());
        assert!(text.is_none());
    }

    #[test]
    fn test_strip_code_fences() {
        assert_eq!(strip_code_fences("hello"), "hello");
        assert_eq!(strip_code_fences("```json\n{}\n```"), "{}");
        assert_eq!(strip_code_fences("```\n{}\n```"), "{}");
        assert_eq!(strip_code_fences("  ```json\n{\"a\":1}\n```  "), "{\"a\":1}");
    }

    #[test]
    fn test_build_prompt_no_context() {
        let prompt = build_prompt(&[], None);
        assert!(prompt.contains("Analyze this screenshot"));
        assert!(prompt.contains("task_title"));
    }

    #[test]
    fn test_build_prompt_with_session() {
        let prompt = build_prompt(&[], Some("writing a blog post"));
        assert!(prompt.contains("writing a blog post"));
    }

    #[test]
    fn test_build_multi_prompt() {
        let changed = vec![
            ChangedMonitor {
                monitor_name: "DISPLAY1",
                image_path: Path::new("test.webp"),
                width: 1920,
                height: 1080,
                is_primary: true,
            },
        ];
        let unchanged = vec![
            UnchangedMonitor {
                monitor_name: "DISPLAY2",
                summary: "Browser with docs",
            },
        ];
        let prompt = build_multi_prompt(&changed, &unchanged, &[], None, 2);
        assert!(prompt.contains("2 monitors"));
        assert!(prompt.contains("DISPLAY1"));
        assert!(prompt.contains("1920x1080"));
        assert!(prompt.contains("DISPLAY2"));
        assert!(prompt.contains("Browser with docs"));
        assert!(prompt.contains("monitor_summaries"));
    }
}
