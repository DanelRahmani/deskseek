//! Serde shapes for the DeepSeek chat-completions API (OpenAI-compatible).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}

impl ToolCall {
    pub fn new(index_id: String) -> Self {
        Self {
            id: index_id,
            kind: "function".to_string(),
            function: FunctionCall {
                name: String::new(),
                arguments: String::new(),
            },
        }
    }
}

/// A message in the conversation. `reasoning_content` must be echoed back on
/// prior assistant turns whenever the request carries `tools`, or the API
/// returns 400 — see `crate::conversation::assemble`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thinking {
    #[serde(rename = "type")]
    pub kind: String,
}

impl Thinking {
    pub fn enabled(on: bool) -> Self {
        Self {
            kind: if on { "enabled" } else { "disabled" }.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolFunction,
}

impl Tool {
    pub fn function(name: &str, description: &str, parameters: serde_json::Value) -> Self {
        Self {
            kind: "function".to_string(),
            function: ToolFunction {
                name: name.to_string(),
                description: description.to_string(),
                parameters,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
}

#[derive(Debug, Clone, Deserialize, Default, Serialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
    #[serde(default)]
    pub prompt_cache_hit_tokens: u32,
    #[serde(default)]
    pub prompt_cache_miss_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamChunk {
    #[serde(default)]
    pub choices: Vec<ChunkChoice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkChoice {
    #[serde(default)]
    pub delta: Delta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Delta {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallDelta {
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub function: Option<FunctionDelta>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FunctionDelta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}
