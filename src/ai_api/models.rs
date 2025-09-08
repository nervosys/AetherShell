use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use std::collections::HashMap;
use chrono::{DateTime, Utc};


/// Information about an available AI model
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelInfo {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
    pub provider: String,
    pub context_length: Option<u32>,
    pub max_output: Option<u32>,
    pub per_request_limits: Option<PerRequestLimits>,
    pub pricing: Option<ModelPricing>,
    pub capabilities: ModelCapabilities,
    pub local_path: Option<String>,
    pub format: ModelFormat,
    pub size_bytes: Option<u64>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerRequestLimits {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub prompt: f64,  // Cost per 1K tokens
    pub completion: f64,  // Cost per 1K tokens
    pub image: Option<f64>,  // Cost per image
    pub request: Option<f64>,  // Cost per request
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub chat: bool,
    pub completions: bool,
    pub embeddings: bool,
    pub image_generation: bool,
    pub image_understanding: bool,
    pub audio_generation: bool,
    pub audio_understanding: bool,
    pub video_understanding: bool,
    pub function_calling: bool,
    pub streaming: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, ToSchema)]
pub enum ModelFormat {
    OpenAI,
    Anthropic,
    GGUF,
    SafeTensors,
    PyTorch,
    ONNX,
    TensorFlow,
    Huggingface,
}

/// Chat completion request compatible with OpenAI API
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub provider: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub n: Option<u32>,
    pub stream: Option<bool>,
    pub stop: Option<Vec<String>>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub logit_bias: Option<HashMap<String, f32>>,
    pub user: Option<String>,
    pub functions: Option<Vec<Function>>,
    pub function_call: Option<FunctionCall>,
    pub tools: Option<Vec<Tool>>,
    pub tool_choice: Option<ToolChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<String>,
    pub name: Option<String>,
    pub function_call: Option<FunctionCall>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub description: Option<String>,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: Function,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    None,
    Auto,
    Required,
    Function { function: FunctionCall },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionCall,
}

/// Chat completion response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Option<Usage>,
    pub system_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
    pub delta: Option<ChatMessage>,  // For streaming
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Embedding request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmbeddingRequest {
    pub model: String,
    pub provider: String,
    pub input: Vec<String>,
    pub encoding_format: Option<String>,
    pub dimensions: Option<u32>,
    pub user: Option<String>,
}

/// Embedding response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmbeddingResponse {
    pub object: String,
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingData {
    pub object: String,
    pub embedding: Vec<f32>,
    pub index: u32,
}

/// Local model metadata for XDG storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModelMetadata {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub version: String,
    pub format: ModelFormat,
    pub file_path: String,
    pub config_path: Option<String>,
    pub tokenizer_path: Option<String>,
    pub size_bytes: u64,
    pub sha256: String,
    pub downloaded_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub usage_count: u64,
    pub capabilities: ModelCapabilities,
    pub parameters: HashMap<String, serde_json::Value>,
    pub source: ModelSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSource {
    pub origin: String,  // "huggingface", "local", "openai", etc.
    pub url: Option<String>,
    pub repository: Option<String>,
    pub commit: Option<String>,
    pub license: Option<String>,
}

/// API Error types
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct APIError {
    pub error: ErrorDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorDetail {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub param: Option<String>,
    pub code: Option<String>,
}

impl std::fmt::Display for APIError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.error.error_type, self.error.message)
    }
}

impl std::error::Error for APIError {}
