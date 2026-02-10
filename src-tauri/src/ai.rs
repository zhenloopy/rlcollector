use base64::Engine;
use log::{error, info};
use reqwest::Client;
use serde::{Deserialize, Serialize};
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
    #[error("No API key configured")]
    NoApiKey,
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
}

/// Load an image from disk, apply preprocessing based on image_mode, and return base64 + media type.
fn preprocess_and_encode(image_path: &Path, image_mode: &str) -> Result<(String, &'static str), AiError> {
    let raw_bytes = std::fs::read(image_path).map_err(|e| {
        error!("Failed to read image {}: {}", image_path.display(), e);
        AiError::ImageReadFailed(e.to_string())
    })?;

    // Load image into RgbaImage for preprocessing
    let img = image::load_from_memory(&raw_bytes)
        .map_err(|e| AiError::ImageReadFailed(format!("Failed to decode image: {}", e)))?
        .to_rgba8();

    let processed = match image_mode {
        "active_window" => {
            let cropped = capture::crop_active_window(&img);
            capture::resize_for_analysis(&cropped, 1280)
        }
        _ => {
            // "downscale" or default
            capture::resize_for_analysis(&img, 1280)
        }
    };

    let webp_bytes = capture::encode_webp_bytes(&processed)
        .map_err(|e| AiError::ImageReadFailed(format!("Failed to encode preprocessed image: {}", e)))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&webp_bytes);
    Ok((b64, "image/webp"))
}

/// Build the analysis prompt, optionally incorporating session context.
fn build_prompt(previous_context: Option<&str>, session_description: Option<&str>) -> String {
    let context_line = previous_context
        .map(|c| format!("Previous task context: {}\n", c))
        .unwrap_or_default();

    if let Some(desc) = session_description {
        format!(
            "The user is working on: {desc}. \
             Look at this screenshot and briefly describe what specific step or subtask they are currently on.\n\
             {context_line}\
             Respond with JSON only, no other text:\n\
             {{\"task_title\": \"short title\", \"task_description\": \"what they're doing\", \
             \"category\": \"coding|browsing|writing|communication|design|other\", \
             \"reasoning\": \"why you think this\", \"is_new_task\": true/false}}"
        )
    } else {
        format!(
            "Analyze this screenshot of a user's screen. Determine what task they are working on.\n\
             {context_line}\
             Respond with JSON only, no other text:\n\
             {{\"task_title\": \"short title\", \"task_description\": \"what they're doing\", \
             \"category\": \"coding|browsing|writing|communication|design|other\", \
             \"reasoning\": \"why you think this\", \"is_new_task\": true/false}}"
        )
    }
}

pub async fn analyze_screenshot(
    client: &Client,
    api_key: &str,
    image_path: &Path,
    previous_context: Option<&str>,
    session_description: Option<&str>,
    image_mode: &str,
) -> Result<TaskAnalysis, AiError> {
    info!("Analyzing screenshot: {}", image_path.display());
    let (b64, media_type) = preprocess_and_encode(image_path, image_mode)?;
    let prompt = build_prompt(previous_context, session_description);

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

    // Strip markdown code fences if present (e.g. ```json ... ```)
    let cleaned = text.trim();
    let cleaned = if cleaned.starts_with("```") {
        let stripped = cleaned
            .strip_prefix("```json")
            .or_else(|| cleaned.strip_prefix("```"))
            .unwrap_or(cleaned);
        stripped
            .strip_suffix("```")
            .unwrap_or(stripped)
            .trim()
    } else {
        cleaned
    };

    let analysis: TaskAnalysis =
        serde_json::from_str(cleaned).map_err(|e| {
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

pub async fn analyze_screenshot_ollama(
    client: &Client,
    model: &str,
    image_path: &Path,
    previous_context: Option<&str>,
    session_description: Option<&str>,
    image_mode: &str,
) -> Result<TaskAnalysis, AiError> {
    info!("Analyzing screenshot with Ollama ({}): {}", model, image_path.display());
    let (b64, _media_type) = preprocess_and_encode(image_path, image_mode)?;

    let context_line = previous_context
        .map(|c| format!("Previous task context: {}\n", c))
        .unwrap_or_default();

    let prompt = if let Some(desc) = session_description {
        format!(
            "The user is working on: {desc}. \
             Look at this screenshot and briefly describe what specific step or subtask they are currently on.\n\
             {context_line}\
             Respond with JSON matching the schema provided in the format field."
        )
    } else {
        format!(
            "Analyze this screenshot of a user's screen. Determine what task they are working on.\n\
             {context_line}\
             Respond with JSON matching the schema provided in the format field."
        )
    };

    let format_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "task_title": { "type": "string" },
            "task_description": { "type": "string" },
            "category": { "type": "string", "enum": ["coding", "browsing", "writing", "communication", "design", "other"] },
            "reasoning": { "type": "string" },
            "is_new_task": { "type": "boolean" }
        },
        "required": ["task_title", "task_description", "category", "reasoning", "is_new_task"]
    });

    let request = OllamaRequest {
        model: model.to_string(),
        messages: vec![OllamaMessage {
            role: "user".to_string(),
            content: prompt,
            images: vec![b64],
        }],
        stream: false,
        format: format_schema,
        options: Some(serde_json::json!({
            "temperature": 0.3,
            "num_predict": 256
        })),
    };

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
    info!("Raw Ollama response: {}", ollama_resp.message.content);

    let analysis: TaskAnalysis =
        serde_json::from_str(&ollama_resp.message.content).map_err(|e| {
            error!("Failed to parse Ollama response: {} — raw text: {}", e, ollama_resp.message.content);
            AiError::ApiError(format!("Parse error: {}", e))
        })?;

    Ok(analysis)
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
        assert_eq!(message["role"], "user");
        assert_eq!(message["content"].as_array().unwrap().len(), 2);
        let image_content = &message["content"][0];
        assert_eq!(image_content["type"], "image");
        assert_eq!(image_content["source"]["type"], "base64");
        assert_eq!(image_content["source"]["media_type"], "image/png");
        let text_content = &message["content"][1];
        assert_eq!(text_content["type"], "text");
        assert_eq!(text_content["text"], "Analyze this screenshot");
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
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["images"][0], "dGVzdA==");
        assert!(json["format"].is_object());
        assert_eq!(json["options"]["temperature"], 0.3);
        assert_eq!(json["options"]["num_predict"], 256);
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
        assert_eq!(analysis.category, "coding");
        assert!(analysis.is_new_task);
    }

    #[test]
    fn test_ollama_tags_deserialization() {
        let json = r#"{"models": [{"name": "qwen3-vl:8b"}, {"name": "llama3:8b"}]}"#;
        let tags: OllamaTagsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(tags.models.len(), 2);
        assert_eq!(tags.models[0].name, "qwen3-vl:8b");
        assert_eq!(tags.models[1].name, "llama3:8b");
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
        let text = empty_response.content.first().and_then(|c| c.text.as_ref());
        assert!(text.is_none(), "Empty response should have no text content");

        let no_text_response = ClaudeResponse {
            content: vec![ResponseContent { text: None }],
        };
        let text = no_text_response.content.first().and_then(|c| c.text.as_ref());
        assert!(text.is_none(), "Response with None text should have no text content");
    }
}
