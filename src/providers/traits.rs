//! Universal LLM Provider Traits
//!
//! Defines the core traits and interfaces that all LLM providers must implement,
//! enabling AetherShell to seamlessly switch between any AI backend.

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::pin::Pin;

use super::schema::ToolSchema;
use super::{ModelUri, ProviderConfig, ProviderType, ToolFormat};

// ============================================================================
// MESSAGE TYPES
// ============================================================================

/// Role in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
    Function,
}

/// Content types for multimodal messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Plain text content
    Text { text: String },
    /// Image content
    Image {
        #[serde(flatten)]
        source: ImageSource,
    },
    /// Audio content
    Audio {
        #[serde(flatten)]
        source: AudioSource,
    },
    /// Tool use request (from assistant)
    ToolUse {
        id: String,
        name: String,
        input: JsonValue,
    },
    /// Tool result (from user)
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

/// Image source variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ImageSource {
    /// Base64 encoded image
    Base64 { media_type: String, data: String },
    /// URL to image
    Url { url: String },
}

/// Audio source variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AudioSource {
    /// Base64 encoded audio
    Base64 { media_type: String, data: String },
    /// URL to audio
    Url { url: String },
}

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender
    pub role: Role,
    /// Message content (can be multimodal)
    pub content: Vec<ContentPart>,
    /// Optional name for the sender
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool call ID (for tool messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    /// Create a simple text message
    pub fn text(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![ContentPart::Text {
                text: content.into(),
            }],
            name: None,
            tool_call_id: None,
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self::text(Role::System, content)
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self::text(Role::User, content)
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::text(Role::Assistant, content)
    }

    /// Create a multimodal user message with image
    pub fn user_with_image(text: impl Into<String>, image_url: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![
                ContentPart::Text { text: text.into() },
                ContentPart::Image {
                    source: ImageSource::Url {
                        url: image_url.into(),
                    },
                },
            ],
            name: None,
            tool_call_id: None,
        }
    }

    /// Create a tool result message
    pub fn tool_result(
        tool_use_id: impl Into<String>,
        content: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: vec![ContentPart::ToolResult {
                tool_use_id: tool_use_id.into(),
                content: content.into(),
                is_error,
            }],
            name: None,
            tool_call_id: None,
        }
    }

    /// Get the text content of a message
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

// ============================================================================
// REQUEST/RESPONSE TYPES
// ============================================================================

/// Chat completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// The model to use
    pub model: ModelUri,
    /// Messages in the conversation
    pub messages: Vec<Message>,
    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Temperature (0-2)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Frequency penalty
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Presence penalty
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Tools available to the model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSchema>>,
    /// Tool choice configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Response format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    /// Seed for deterministic output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// User identifier for abuse detection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Provider-specific options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<HashMap<String, JsonValue>>,
}

impl ChatRequest {
    /// Create a new chat request
    pub fn new(model: ModelUri, messages: Vec<Message>) -> Self {
        Self {
            model,
            messages,
            max_tokens: None,
            temperature: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            seed: None,
            user: None,
            extra: None,
        }
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set tools
    pub fn with_tools(mut self, tools: Vec<ToolSchema>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Set tool choice
    pub fn with_tool_choice(mut self, choice: ToolChoice) -> Self {
        self.tool_choice = Some(choice);
        self
    }
}

/// Tool choice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// Let the model decide
    Auto,
    /// Don't use any tools
    None,
    /// Force a specific tool
    Tool { name: String },
    /// Require the model to call at least one tool
    Required,
}

/// Response format configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// Plain text response
    Text,
    /// JSON object response
    JsonObject,
    /// JSON with schema
    JsonSchema { schema: JsonValue },
}

/// Chat completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Response ID
    pub id: String,
    /// The model used
    pub model: String,
    /// The generated message
    pub message: Message,
    /// Token usage
    pub usage: TokenUsage,
    /// Finish reason
    pub finish_reason: FinishReason,
    /// Tool calls (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Provider-specific metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, JsonValue>>,
}

impl ChatResponse {
    /// Get the text content of the response
    pub fn text(&self) -> String {
        self.message.text_content()
    }

    /// Check if the response has tool calls
    pub fn has_tool_calls(&self) -> bool {
        self.tool_calls
            .as_ref()
            .map(|tc| !tc.is_empty())
            .unwrap_or(false)
    }
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    /// Cached tokens (if supported)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u32>,
}

/// Reason the model stopped generating
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Error,
    Unknown,
}

/// A tool call from the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this tool call
    pub id: String,
    /// The tool name
    pub name: String,
    /// Arguments as JSON
    pub arguments: JsonValue,
}

// ============================================================================
// STREAMING TYPES
// ============================================================================

/// A streaming chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Chunk ID
    pub id: Option<String>,
    /// Delta content
    pub delta: StreamDelta,
    /// Finish reason (on final chunk)
    pub finish_reason: Option<FinishReason>,
    /// Usage (on final chunk)
    pub usage: Option<TokenUsage>,
}

/// Delta content in a stream
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StreamDelta {
    /// Text content
    Text(String),
    /// Tool call delta
    ToolCall {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments: Option<String>,
    },
}

/// Stream type alias
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<StreamChunk, ProviderError>> + Send>>;

// ============================================================================
// EMBEDDING TYPES
// ============================================================================

/// Embedding request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    /// The model to use
    pub model: ModelUri,
    /// Input texts to embed
    pub input: Vec<String>,
    /// Encoding format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<EncodingFormat>,
    /// Dimensions (for models that support it)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
}

/// Embedding encoding format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncodingFormat {
    Float,
    Base64,
}

/// Embedding response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// The embeddings
    pub embeddings: Vec<Vec<f32>>,
    /// The model used
    pub model: String,
    /// Token usage
    pub usage: TokenUsage,
}

// ============================================================================
// MODEL INFO
// ============================================================================

/// Information about a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Model provider
    pub provider: ProviderType,
    /// Context window size
    pub context_length: u32,
    /// Maximum output tokens
    pub max_output_tokens: Option<u32>,
    /// Whether the model supports tools
    pub supports_tools: bool,
    /// Whether the model supports vision
    pub supports_vision: bool,
    /// Whether the model supports audio
    pub supports_audio: bool,
    /// Whether the model supports JSON mode
    pub supports_json_mode: bool,
    /// Input cost per million tokens
    pub input_cost_per_million: Option<f64>,
    /// Output cost per million tokens
    pub output_cost_per_million: Option<f64>,
    /// Model capabilities/tags
    pub capabilities: Vec<String>,
}

// ============================================================================
// ERROR TYPES
// ============================================================================

/// Provider errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderError {
    /// Authentication failed
    AuthenticationFailed { message: String },
    /// Rate limit exceeded
    RateLimited {
        message: String,
        retry_after: Option<u64>,
    },
    /// Invalid request
    InvalidRequest { message: String },
    /// Model not found
    ModelNotFound { model: String },
    /// Context length exceeded
    ContextLengthExceeded {
        max_tokens: u32,
        requested_tokens: u32,
    },
    /// Content filtered
    ContentFiltered { message: String },
    /// Provider unavailable
    Unavailable { message: String },
    /// Network error
    NetworkError { message: String },
    /// Timeout
    Timeout { seconds: u64 },
    /// Unknown error
    Unknown { message: String },
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthenticationFailed { message } => {
                write!(f, "Authentication failed: {}", message)
            }
            Self::RateLimited { message, .. } => write!(f, "Rate limited: {}", message),
            Self::InvalidRequest { message } => write!(f, "Invalid request: {}", message),
            Self::ModelNotFound { model } => write!(f, "Model not found: {}", model),
            Self::ContextLengthExceeded {
                max_tokens,
                requested_tokens,
            } => {
                write!(
                    f,
                    "Context length exceeded: {} > {}",
                    requested_tokens, max_tokens
                )
            }
            Self::ContentFiltered { message } => write!(f, "Content filtered: {}", message),
            Self::Unavailable { message } => write!(f, "Provider unavailable: {}", message),
            Self::NetworkError { message } => write!(f, "Network error: {}", message),
            Self::Timeout { seconds } => write!(f, "Timeout after {} seconds", seconds),
            Self::Unknown { message } => write!(f, "Unknown error: {}", message),
        }
    }
}

impl std::error::Error for ProviderError {}

// ============================================================================
// PROVIDER TRAIT
// ============================================================================

/// The core trait that all LLM providers must implement
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// Get the provider type
    fn provider_type(&self) -> ProviderType;

    /// Get the provider configuration
    fn config(&self) -> &ProviderConfig;

    /// Check if the provider is configured and ready
    async fn is_available(&self) -> bool;

    /// Validate the API key
    async fn validate_credentials(&self) -> Result<(), ProviderError>;

    /// List available models
    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError>;

    /// Get information about a specific model
    async fn get_model_info(&self, model: &str) -> Result<ModelInfo, ProviderError>;

    /// Chat completion (non-streaming)
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError>;

    /// Chat completion with streaming
    async fn chat_stream(&self, request: ChatRequest) -> Result<ChatStream, ProviderError>;

    /// Generate embeddings
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError>;

    /// Get the tool format for this provider
    fn tool_format(&self) -> ToolFormat;

    /// Check if this provider supports a capability
    fn supports(&self, capability: &str) -> bool;

    /// Get provider-specific headers
    fn auth_headers(&self) -> HashMap<String, String>;

    /// Transform a request for this provider's API format
    fn transform_request(&self, request: &ChatRequest) -> Result<JsonValue, ProviderError>;

    /// Parse a response from this provider's API format
    fn parse_response(&self, response: &JsonValue) -> Result<ChatResponse, ProviderError>;
}

/// Extension trait for additional provider capabilities
#[async_trait]
pub trait LLMProviderExt: LLMProvider {
    /// Simple text completion
    async fn complete(&self, prompt: &str) -> Result<String, ProviderError> {
        let request = ChatRequest::new(self.config().model_uri(), vec![Message::user(prompt)]);
        let response = self.chat(request).await?;
        Ok(response.text())
    }

    /// Chat with tool calls
    async fn chat_with_tools(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolSchema>,
    ) -> Result<ChatResponse, ProviderError> {
        let request = ChatRequest::new(self.config().model_uri(), messages).with_tools(tools);
        self.chat(request).await
    }

    /// Embed a single text
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>, ProviderError> {
        let request = EmbeddingRequest {
            model: self.config().model_uri(),
            input: vec![text.to_string()],
            encoding_format: Some(EncodingFormat::Float),
            dimensions: None,
        };
        let response = self.embed(request).await?;
        response
            .embeddings
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::Unknown {
                message: "No embedding returned".to_string(),
            })
    }

    /// Count tokens in a message (approximate if not supported natively)
    fn count_tokens(&self, text: &str) -> u32 {
        // Simple approximation: ~4 chars per token
        (text.len() / 4) as u32
    }
}

// Blanket implementation
impl<T: LLMProvider> LLMProviderExt for T {}

// ============================================================================
// PROVIDER FACTORY
// ============================================================================

/// Factory for creating providers
pub struct ProviderFactory;

impl ProviderFactory {
    /// Create a provider from a model URI
    pub fn from_uri(uri: &ModelUri) -> Result<Box<dyn LLMProvider>, ProviderError> {
        match uri.provider {
            ProviderType::OpenAI => {
                // Would create OpenAIProvider
                Err(ProviderError::Unknown {
                    message: "Provider implementation pending".to_string(),
                })
            }
            ProviderType::Anthropic => Err(ProviderError::Unknown {
                message: "Provider implementation pending".to_string(),
            }),
            _ => Err(ProviderError::Unknown {
                message: format!("Provider {:?} not implemented", uri.provider),
            }),
        }
    }

    /// Create a provider from type and config
    pub fn from_config(
        provider_type: ProviderType,
        _config: ProviderConfig,
    ) -> Result<Box<dyn LLMProvider>, ProviderError> {
        match provider_type {
            ProviderType::OpenAI => Err(ProviderError::Unknown {
                message: "Provider implementation pending".to_string(),
            }),
            _ => Err(ProviderError::Unknown {
                message: format!("Provider {:?} not implemented", provider_type),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello!");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.text_content(), "Hello!");
    }

    #[test]
    fn test_system_message() {
        let msg = Message::system("You are a helpful assistant.");
        assert_eq!(msg.role, Role::System);
    }

    #[test]
    fn test_chat_request_builder() {
        let uri = ModelUri::parse("openai:gpt-4o").unwrap();
        let request = ChatRequest::new(uri, vec![Message::user("Hi")])
            .with_max_tokens(100)
            .with_temperature(0.7);

        assert_eq!(request.max_tokens, Some(100));
        assert_eq!(request.temperature, Some(0.7));
    }

    #[test]
    fn test_response_text() {
        let response = ChatResponse {
            id: "test".to_string(),
            model: "gpt-4o".to_string(),
            message: Message::assistant("Hello there!"),
            usage: TokenUsage::default(),
            finish_reason: FinishReason::Stop,
            tool_calls: None,
            metadata: None,
        };
        assert_eq!(response.text(), "Hello there!");
    }
}
