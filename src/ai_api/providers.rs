use crate::ai_api::models::*;
use crate::secure_config::SecureApiConfig;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;

// SECURITY FIX (LOW-002): Helper to create secure HTTP clients with timeouts
fn create_secure_client() -> Client {
    crate::security::create_secure_async_client().unwrap_or_else(|_| Client::new())
}

/// Trait for AI model providers
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse>;
    async fn embeddings(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse>;
    async fn validate_api_key(&self) -> Result<bool>;
    fn get_provider_name(&self) -> &str;
}

/// OpenAI API provider
pub struct OpenAIProvider {
    client: Client,
    config: Option<SecureApiConfig>,
    base_url: String,
}

impl OpenAIProvider {
    pub fn new() -> Self {
        // Try to load from keyring or environment
        let config = SecureApiConfig::from_keyring_or_env(
            "openai",
            "OPENAI_API_KEY",
            "https://api.openai.com".to_string(),
            "gpt-4o-mini".to_string(),
            "openai".to_string(),
        )
        .ok();

        Self {
            client: create_secure_client(),
            config,
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    pub fn with_api_key(api_key: String) -> Self {
        let config = SecureApiConfig::new(
            api_key,
            "https://api.openai.com".to_string(),
            "gpt-4o-mini".to_string(),
            "openai".to_string(),
        );

        Self {
            client: create_secure_client(),
            config: Some(config),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    fn get_auth_header(&self) -> Option<String> {
        self.config
            .as_ref()
            .and_then(|c| c.create_auth_header())
            .map(|h| h.to_string())
    }
}

#[async_trait]
impl ModelProvider for OpenAIProvider {
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let auth_header = self.get_auth_header().unwrap_or_default();

        let response = self
            .client
            .get(&format!("{}/models", self.base_url))
            .header("Authorization", auth_header)
            .send()
            .await?;

        let models_response: serde_json::Value = response.json().await?;
        let mut models = Vec::new();

        if let Some(data) = models_response.get("data").and_then(|d| d.as_array()) {
            for model in data {
                let model_info = ModelInfo {
                    id: model
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    object: model
                        .get("object")
                        .and_then(|v| v.as_str())
                        .unwrap_or("model")
                        .to_string(),
                    created: model.get("created").and_then(|v| v.as_i64()).unwrap_or(0),
                    owned_by: model
                        .get("owned_by")
                        .and_then(|v| v.as_str())
                        .unwrap_or("openai")
                        .to_string(),
                    provider: "openai".to_string(),
                    context_length: self.get_context_length(
                        &model.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                    ),
                    max_output: None,
                    per_request_limits: None,
                    pricing: self
                        .get_pricing(&model.get("id").and_then(|v| v.as_str()).unwrap_or("")),
                    capabilities: self
                        .get_capabilities(&model.get("id").and_then(|v| v.as_str()).unwrap_or("")),
                    local_path: None,
                    format: ModelFormat::OpenAI,
                    size_bytes: None,
                    metadata: HashMap::new(),
                };
                models.push(model_info);
            }
        }

        Ok(models)
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        // Remove provider field for OpenAI API compatibility
        let _provider = request.provider.clone();

        let payload = json!({
            "model": request.model,
            "messages": request.messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "top_p": request.top_p,
            "n": request.n,
            "stream": request.stream,
            "stop": request.stop,
            "presence_penalty": request.presence_penalty,
            "frequency_penalty": request.frequency_penalty,
            "logit_bias": request.logit_bias,
            "user": request.user,
            "functions": request.functions,
            "function_call": request.function_call,
            "tools": request.tools,
            "tool_choice": request.tool_choice,
        });

        let auth_header = self.get_auth_header().unwrap_or_default();

        let response = self
            .client
            .post(&format!("{}/chat/completions", self.base_url))
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("OpenAI API error: {}", error_text));
        }

        let chat_response: ChatCompletionResponse = response.json().await?;
        Ok(chat_response)
    }

    async fn embeddings(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let payload = json!({
            "model": request.model,
            "input": request.input,
            "encoding_format": request.encoding_format,
            "dimensions": request.dimensions,
            "user": request.user,
        });

        let auth_header = self.get_auth_header().unwrap_or_default();

        let response = self
            .client
            .post(&format!("{}/embeddings", self.base_url))
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("OpenAI API error: {}", error_text));
        }

        let embedding_response: EmbeddingResponse = response.json().await?;
        Ok(embedding_response)
    }

    async fn validate_api_key(&self) -> Result<bool> {
        // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
        let auth_header = match self.get_auth_header() {
            Some(header) => header,
            None => return Ok(false),
        };

        let response = self
            .client
            .get(&format!("{}/models", self.base_url))
            .header("Authorization", auth_header)
            .send()
            .await?;

        Ok(response.status().is_success())
    }

    fn get_provider_name(&self) -> &str {
        "openai"
    }
}

impl OpenAIProvider {
    fn get_context_length(&self, model_id: &str) -> Option<u32> {
        match model_id {
            // GPT-4o family
            "gpt-4o" | "gpt-4o-2024-11-20" | "gpt-4o-2024-08-06" | "gpt-4o-2024-05-13" => {
                Some(128000)
            }
            "gpt-4o-mini" | "gpt-4o-mini-2024-07-18" => Some(128000),
            "gpt-4o-audio-preview" | "gpt-4o-realtime-preview" => Some(128000),
            // GPT-4.5
            "gpt-4.5-preview" | "gpt-4.5-preview-2025-02-27" => Some(128000),
            // o-series reasoning models
            "o3" | "o3-2025-04-16" => Some(200000),
            "o3-mini" | "o3-mini-2025-01-31" => Some(200000),
            "o4-mini" | "o4-mini-2025-04-16" => Some(200000),
            "o1" | "o1-2024-12-17" => Some(200000),
            "o1-mini" | "o1-mini-2024-09-12" => Some(128000),
            "o1-preview" | "o1-preview-2024-09-12" => Some(128000),
            // GPT-4 Turbo
            "gpt-4-turbo" | "gpt-4-turbo-2024-04-09" | "gpt-4-turbo-preview" => Some(128000),
            "gpt-4-1106-preview" | "gpt-4-0125-preview" => Some(128000),
            // GPT-4
            "gpt-4" | "gpt-4-0613" => Some(8192),
            "gpt-4-32k" | "gpt-4-32k-0613" => Some(32768),
            // GPT-3.5
            "gpt-3.5-turbo" | "gpt-3.5-turbo-0125" => Some(16385),
            "gpt-3.5-turbo-16k" => Some(16384),
            "gpt-3.5-turbo-instruct" => Some(4096),
            // Embeddings
            "text-embedding-3-large" => Some(8191),
            "text-embedding-3-small" => Some(8191),
            "text-embedding-ada-002" => Some(8191),
            _ => {
                // Heuristic for unknown models
                if model_id.starts_with("gpt-4o") || model_id.starts_with("gpt-4.5") {
                    Some(128000)
                } else if model_id.starts_with("o1")
                    || model_id.starts_with("o3")
                    || model_id.starts_with("o4")
                {
                    Some(200000)
                } else {
                    None
                }
            }
        }
    }

    fn get_pricing(&self, model_id: &str) -> Option<ModelPricing> {
        // Prices per 1K tokens (as of Feb 2026)
        match model_id {
            // GPT-4o (latest)
            "gpt-4o" | "gpt-4o-2024-11-20" | "gpt-4o-2024-08-06" => Some(ModelPricing {
                prompt: 0.0025,
                completion: 0.01,
                image: Some(0.003613),
                request: None,
            }),
            "gpt-4o-mini" | "gpt-4o-mini-2024-07-18" => Some(ModelPricing {
                prompt: 0.00015,
                completion: 0.0006,
                image: Some(0.001838),
                request: None,
            }),
            "gpt-4o-audio-preview" => Some(ModelPricing {
                prompt: 0.0025,
                completion: 0.01,
                image: None,
                request: None,
            }),
            // GPT-4.5
            "gpt-4.5-preview" | "gpt-4.5-preview-2025-02-27" => Some(ModelPricing {
                prompt: 0.075,
                completion: 0.15,
                image: None,
                request: None,
            }),
            // o-series reasoning
            "o3" | "o3-2025-04-16" => Some(ModelPricing {
                prompt: 0.01,
                completion: 0.04,
                image: None,
                request: None,
            }),
            "o3-mini" | "o3-mini-2025-01-31" => Some(ModelPricing {
                prompt: 0.0011,
                completion: 0.0044,
                image: None,
                request: None,
            }),
            "o4-mini" | "o4-mini-2025-04-16" => Some(ModelPricing {
                prompt: 0.0011,
                completion: 0.0044,
                image: None,
                request: None,
            }),
            "o1" | "o1-2024-12-17" => Some(ModelPricing {
                prompt: 0.015,
                completion: 0.06,
                image: None,
                request: None,
            }),
            "o1-mini" | "o1-mini-2024-09-12" => Some(ModelPricing {
                prompt: 0.003,
                completion: 0.012,
                image: None,
                request: None,
            }),
            // GPT-4 Turbo
            "gpt-4-turbo" | "gpt-4-turbo-2024-04-09" => Some(ModelPricing {
                prompt: 0.01,
                completion: 0.03,
                image: None,
                request: None,
            }),
            // GPT-4
            "gpt-4" | "gpt-4-0613" => Some(ModelPricing {
                prompt: 0.03,
                completion: 0.06,
                image: None,
                request: None,
            }),
            "gpt-4-32k" => Some(ModelPricing {
                prompt: 0.06,
                completion: 0.12,
                image: None,
                request: None,
            }),
            // GPT-3.5
            "gpt-3.5-turbo" | "gpt-3.5-turbo-0125" => Some(ModelPricing {
                prompt: 0.0005,
                completion: 0.0015,
                image: None,
                request: None,
            }),
            // Embeddings
            "text-embedding-3-large" => Some(ModelPricing {
                prompt: 0.00013,
                completion: 0.0,
                image: None,
                request: None,
            }),
            "text-embedding-3-small" => Some(ModelPricing {
                prompt: 0.00002,
                completion: 0.0,
                image: None,
                request: None,
            }),
            _ => None,
        }
    }

    fn get_capabilities(&self, model_id: &str) -> ModelCapabilities {
        let is_o_series =
            model_id.starts_with("o1") || model_id.starts_with("o3") || model_id.starts_with("o4");
        ModelCapabilities {
            chat: true,
            completions: !model_id.contains("embedding"),
            embeddings: model_id.contains("embedding"),
            image_generation: model_id.contains("dall-e") || model_id.contains("gpt-image"),
            image_understanding: model_id.contains("gpt-4o")
                || model_id.contains("gpt-4-turbo")
                || model_id.contains("gpt-4.5")
                || is_o_series,
            audio_generation: model_id.contains("tts") || model_id.contains("audio"),
            audio_understanding: model_id.contains("whisper") || model_id.contains("audio"),
            video_understanding: false,
            function_calling: !model_id.contains("embedding")
                && !model_id.contains("dall-e")
                && !model_id.contains("tts")
                && !model_id.contains("whisper"),
            streaming: !is_o_series,
        }
    }
}

/// Anthropic API provider
pub struct AnthropicProvider {
    client: Client,
    config: Option<SecureApiConfig>,
    base_url: String,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        // Try to load from keyring or environment
        let config = SecureApiConfig::from_keyring_or_env(
            "anthropic",
            "ANTHROPIC_API_KEY",
            "https://api.anthropic.com".to_string(),
            "claude-3-opus-20240229".to_string(),
            "anthropic".to_string(),
        )
        .ok();

        Self {
            client: create_secure_client(),
            config,
            base_url: "https://api.anthropic.com/v1".to_string(),
        }
    }

    pub fn with_api_key(api_key: String) -> Self {
        let config = SecureApiConfig::new(
            api_key,
            "https://api.anthropic.com".to_string(),
            "claude-3-opus-20240229".to_string(),
            "anthropic".to_string(),
        );

        Self {
            client: create_secure_client(),
            config: Some(config),
            base_url: "https://api.anthropic.com/v1".to_string(),
        }
    }

    fn get_api_key(&self) -> Option<String> {
        self.config
            .as_ref()
            .and_then(|c| c.get_api_key().map(|s| s.to_string()))
    }
}

#[async_trait]
impl ModelProvider for AnthropicProvider {
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Anthropic doesn't have a public models endpoint, so we return known models
        let models = vec![
            // Claude 4 (Opus) — latest flagship
            ModelInfo {
                id: "claude-sonnet-4-20250514".to_string(),
                object: "model".to_string(),
                created: 1747267200,
                owned_by: "anthropic".to_string(),
                provider: "anthropic".to_string(),
                context_length: Some(200000),
                max_output: Some(16384),
                per_request_limits: None,
                pricing: Some(ModelPricing {
                    prompt: 0.003,
                    completion: 0.015,
                    image: None,
                    request: None,
                }),
                capabilities: ModelCapabilities {
                    chat: true,
                    completions: true,
                    embeddings: false,
                    image_generation: false,
                    image_understanding: true,
                    audio_generation: false,
                    audio_understanding: false,
                    video_understanding: false,
                    function_calling: true,
                    streaming: true,
                },
                local_path: None,
                format: ModelFormat::Anthropic,
                size_bytes: None,
                metadata: HashMap::new(),
            },
            // Claude 4 Opus
            ModelInfo {
                id: "claude-opus-4-20250514".to_string(),
                object: "model".to_string(),
                created: 1747267200,
                owned_by: "anthropic".to_string(),
                provider: "anthropic".to_string(),
                context_length: Some(200000),
                max_output: Some(32000),
                per_request_limits: None,
                pricing: Some(ModelPricing {
                    prompt: 0.015,
                    completion: 0.075,
                    image: None,
                    request: None,
                }),
                capabilities: ModelCapabilities {
                    chat: true,
                    completions: true,
                    embeddings: false,
                    image_generation: false,
                    image_understanding: true,
                    audio_generation: false,
                    audio_understanding: false,
                    video_understanding: false,
                    function_calling: true,
                    streaming: true,
                },
                local_path: None,
                format: ModelFormat::Anthropic,
                size_bytes: None,
                metadata: HashMap::new(),
            },
            // Claude 3.5 Sonnet (latest GA)
            ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                object: "model".to_string(),
                created: 1729555200,
                owned_by: "anthropic".to_string(),
                provider: "anthropic".to_string(),
                context_length: Some(200000),
                max_output: Some(8192),
                per_request_limits: None,
                pricing: Some(ModelPricing {
                    prompt: 0.003,
                    completion: 0.015,
                    image: None,
                    request: None,
                }),
                capabilities: ModelCapabilities {
                    chat: true,
                    completions: true,
                    embeddings: false,
                    image_generation: false,
                    image_understanding: true,
                    audio_generation: false,
                    audio_understanding: false,
                    video_understanding: false,
                    function_calling: true,
                    streaming: true,
                },
                local_path: None,
                format: ModelFormat::Anthropic,
                size_bytes: None,
                metadata: HashMap::new(),
            },
            // Claude 3.5 Haiku
            ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                object: "model".to_string(),
                created: 1729555200,
                owned_by: "anthropic".to_string(),
                provider: "anthropic".to_string(),
                context_length: Some(200000),
                max_output: Some(8192),
                per_request_limits: None,
                pricing: Some(ModelPricing {
                    prompt: 0.0008,
                    completion: 0.004,
                    image: None,
                    request: None,
                }),
                capabilities: ModelCapabilities {
                    chat: true,
                    completions: true,
                    embeddings: false,
                    image_generation: false,
                    image_understanding: true,
                    audio_generation: false,
                    audio_understanding: false,
                    video_understanding: false,
                    function_calling: true,
                    streaming: true,
                },
                local_path: None,
                format: ModelFormat::Anthropic,
                size_bytes: None,
                metadata: HashMap::new(),
            },
            // Claude 3 Opus
            ModelInfo {
                id: "claude-3-opus-20240229".to_string(),
                object: "model".to_string(),
                created: 1709251200,
                owned_by: "anthropic".to_string(),
                provider: "anthropic".to_string(),
                context_length: Some(200000),
                max_output: Some(4096),
                per_request_limits: None,
                pricing: Some(ModelPricing {
                    prompt: 0.015,
                    completion: 0.075,
                    image: None,
                    request: None,
                }),
                capabilities: ModelCapabilities {
                    chat: true,
                    completions: true,
                    embeddings: false,
                    image_generation: false,
                    image_understanding: true,
                    audio_generation: false,
                    audio_understanding: false,
                    video_understanding: false,
                    function_calling: true,
                    streaming: true,
                },
                local_path: None,
                format: ModelFormat::Anthropic,
                size_bytes: None,
                metadata: HashMap::new(),
            },
            // Claude 3 Sonnet
            ModelInfo {
                id: "claude-3-sonnet-20240229".to_string(),
                object: "model".to_string(),
                created: 1709251200,
                owned_by: "anthropic".to_string(),
                provider: "anthropic".to_string(),
                context_length: Some(200000),
                max_output: Some(4096),
                per_request_limits: None,
                pricing: Some(ModelPricing {
                    prompt: 0.003,
                    completion: 0.015,
                    image: None,
                    request: None,
                }),
                capabilities: ModelCapabilities {
                    chat: true,
                    completions: true,
                    embeddings: false,
                    image_generation: false,
                    image_understanding: true,
                    audio_generation: false,
                    audio_understanding: false,
                    video_understanding: false,
                    function_calling: true,
                    streaming: true,
                },
                local_path: None,
                format: ModelFormat::Anthropic,
                size_bytes: None,
                metadata: HashMap::new(),
            },
        ];

        Ok(models)
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        // Convert OpenAI format to Anthropic format
        let system_message = request
            .messages
            .iter()
            .find(|m| m.role == "system")
            .map(|m| m.content.as_ref().unwrap_or(&"".to_string()).clone());

        let messages: Vec<_> = request
            .messages
            .iter()
            .filter(|m| m.role != "system")
            .map(|m| {
                json!({
                    "role": if m.role == "assistant" { "assistant" } else { "user" },
                    "content": m.content.as_ref().unwrap_or(&"".to_string())
                })
            })
            .collect();

        let mut payload = json!({
            "model": request.model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
        });

        if let Some(system) = system_message {
            payload["system"] = json!(system);
        }

        if let Some(temp) = request.temperature {
            payload["temperature"] = json!(temp);
        }

        if let Some(top_p) = request.top_p {
            payload["top_p"] = json!(top_p);
        }

        let api_key = self.get_api_key().unwrap_or_default();

        let response = self
            .client
            .post(&format!("{}/messages", self.base_url))
            .header("x-api-key", api_key)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Anthropic API error: {}", error_text));
        }

        let anthropic_response: serde_json::Value = response.json().await?;

        // Convert Anthropic response to OpenAI format
        let content = anthropic_response
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        let usage = Usage {
            prompt_tokens: anthropic_response
                .get("usage")
                .and_then(|u| u.get("input_tokens"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as u32,
            completion_tokens: anthropic_response
                .get("usage")
                .and_then(|u| u.get("output_tokens"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as u32,
            total_tokens: 0, // Will be calculated below
        };

        let total_tokens = usage.prompt_tokens + usage.completion_tokens;

        Ok(ChatCompletionResponse {
            id: anthropic_response
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string(),
            object: "chat.completion".to_string(),
            created: chrono::Utc::now().timestamp(),
            model: request.model,
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: Some(content),
                    name: None,
                    function_call: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
                finish_reason: Some("stop".to_string()),
                delta: None,
            }],
            usage: Some(Usage {
                total_tokens,
                ..usage
            }),
            system_fingerprint: None,
        })
    }

    async fn embeddings(&self, _request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        Err(anyhow::anyhow!("Anthropic does not support embeddings"))
    }

    async fn validate_api_key(&self) -> Result<bool> {
        let api_key = match self.get_api_key() {
            Some(key) => key,
            None => return Ok(false),
        };

        // Test with a minimal request
        let test_payload = json!({
            "model": "claude-3-sonnet-20240229",
            "messages": [{"role": "user", "content": "Hi"}],
            "max_tokens": 10
        });

        let response = self
            .client
            .post(&format!("{}/messages", self.base_url))
            .header("x-api-key", api_key)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&test_payload)
            .send()
            .await?;

        Ok(response.status().is_success())
    }

    fn get_provider_name(&self) -> &str {
        "anthropic"
    }
}

/// Local model provider for GGUF and other local formats
///
/// Scans XDG-standard model directories for locally downloaded models and
/// delegates inference to a local llama.cpp server if available.
pub struct LocalProvider {
    client: Client,
    model_dirs: Vec<std::path::PathBuf>,
    llama_cpp_url: Option<String>,
}

impl LocalProvider {
    pub fn new() -> Self {
        let mut model_dirs = Vec::new();

        // XDG data directories
        if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
            model_dirs.push(std::path::PathBuf::from(&data_home).join("aethershell/models"));
        }
        // Default: ~/.local/share/aethershell/models (Unix) or %APPDATA%/aethershell/models (Windows)
        if let Some(home) = dirs_next_home() {
            #[cfg(not(target_os = "windows"))]
            model_dirs.push(home.join(".local/share/aethershell/models"));
            #[cfg(target_os = "windows")]
            {
                if let Ok(appdata) = std::env::var("APPDATA") {
                    model_dirs.push(std::path::PathBuf::from(appdata).join("aethershell\\models"));
                }
            }
            model_dirs.push(home.join(".cache/lm-studio/models"));
            model_dirs.push(home.join(".ollama/models"));
        }

        // Also support AETHER_MODEL_DIR env var
        if let Ok(dir) = std::env::var("AETHER_MODEL_DIR") {
            model_dirs.push(std::path::PathBuf::from(dir));
        }

        // Detect local llama.cpp server
        let llama_cpp_url = std::env::var("LLAMA_CPP_URL").ok().or_else(|| {
            // Try default port
            Some("http://127.0.0.1:8080".to_string())
        });

        Self {
            client: Client::new(),
            model_dirs,
            llama_cpp_url,
        }
    }

    /// Scan model directories for GGUF, SafeTensors, and ONNX model files
    fn scan_local_models(&self) -> Vec<ModelInfo> {
        let mut models = Vec::new();
        let extensions = ["gguf", "safetensors", "onnx", "bin", "pt"];

        for dir in &self.model_dirs {
            if !dir.exists() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if extensions.contains(&ext.as_str()) {
                        let filename = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let size_bytes = std::fs::metadata(&path).ok().map(|m| m.len());
                        let format = match ext.as_str() {
                            "gguf" => ModelFormat::GGUF,
                            "safetensors" => ModelFormat::SafeTensors,
                            "onnx" => ModelFormat::ONNX,
                            _ => ModelFormat::Other(ext.clone()),
                        };

                        // Estimate context length from model name heuristics
                        let context_length = if filename.contains("128k") {
                            Some(131072)
                        } else if filename.contains("32k") {
                            Some(32768)
                        } else if filename.contains("16k") {
                            Some(16384)
                        } else if filename.contains("8k") {
                            Some(8192)
                        } else {
                            Some(4096) // conservative default
                        };

                        models.push(ModelInfo {
                            id: format!("local:{}", filename),
                            object: "model".to_string(),
                            created: path
                                .metadata()
                                .ok()
                                .and_then(|m| {
                                    m.modified().ok().map(|t| {
                                        t.duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_secs()
                                            as i64
                                    })
                                })
                                .unwrap_or(0),
                            owned_by: "local".to_string(),
                            provider: "local".to_string(),
                            context_length,
                            max_output: context_length.map(|c| c / 4),
                            per_request_limits: None,
                            pricing: Some(ModelPricing {
                                prompt: 0.0,
                                completion: 0.0,
                                image: None,
                                request: None,
                            }),
                            capabilities: ModelCapabilities {
                                chat: true,
                                completions: true,
                                embeddings: filename.contains("embed"),
                                image_generation: false,
                                image_understanding: filename.contains("vision")
                                    || filename.contains("llava"),
                                audio_generation: false,
                                audio_understanding: filename.contains("whisper"),
                                video_understanding: false,
                                function_calling: false,
                                streaming: true,
                            },
                            local_path: Some(path.to_string_lossy().to_string()),
                            format,
                            size_bytes,
                            metadata: HashMap::new(),
                        });
                    }
                }
            }
            // Also scan subdirectories one level deep
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let subdir = entry.path();
                    if subdir.is_dir() {
                        if let Ok(sub_entries) = std::fs::read_dir(&subdir) {
                            for sub_entry in sub_entries.flatten() {
                                let path = sub_entry.path();
                                let ext = path
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .unwrap_or("")
                                    .to_lowercase();
                                if extensions.contains(&ext.as_str()) {
                                    let dirname = subdir
                                        .file_name()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("unknown");
                                    let filename = path
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("unknown");
                                    let model_id = format!("local:{}/{}", dirname, filename);
                                    let size_bytes = std::fs::metadata(&path).ok().map(|m| m.len());
                                    let format = match ext.as_str() {
                                        "gguf" => ModelFormat::GGUF,
                                        "safetensors" => ModelFormat::SafeTensors,
                                        "onnx" => ModelFormat::ONNX,
                                        _ => ModelFormat::Other(ext.clone()),
                                    };

                                    models.push(ModelInfo {
                                        id: model_id,
                                        object: "model".to_string(),
                                        created: 0,
                                        owned_by: "local".to_string(),
                                        provider: "local".to_string(),
                                        context_length: Some(4096),
                                        max_output: Some(1024),
                                        per_request_limits: None,
                                        pricing: Some(ModelPricing {
                                            prompt: 0.0,
                                            completion: 0.0,
                                            image: None,
                                            request: None,
                                        }),
                                        capabilities: ModelCapabilities {
                                            chat: true,
                                            completions: true,
                                            embeddings: false,
                                            image_generation: false,
                                            image_understanding: false,
                                            audio_generation: false,
                                            audio_understanding: false,
                                            video_understanding: false,
                                            function_calling: false,
                                            streaming: true,
                                        },
                                        local_path: Some(path.to_string_lossy().to_string()),
                                        format,
                                        size_bytes,
                                        metadata: HashMap::new(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        models
    }

    /// Check if llama.cpp server is running and healthy
    async fn check_llama_cpp_health(&self) -> bool {
        if let Some(url) = &self.llama_cpp_url {
            if let Ok(resp) = self
                .client
                .get(format!("{}/health", url))
                .timeout(std::time::Duration::from_secs(2))
                .send()
                .await
            {
                return resp.status().is_success();
            }
        }
        false
    }
}

/// Get the user's home directory
fn dirs_next_home() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(std::path::PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(std::path::PathBuf::from)
    }
}

#[async_trait]
impl ModelProvider for LocalProvider {
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Scan local model directories for available models
        let mut models = self.scan_local_models();

        // Also check llama.cpp server for loaded models
        if self.check_llama_cpp_health().await {
            if let Some(url) = &self.llama_cpp_url {
                if let Ok(resp) = self
                    .client
                    .get(format!("{}/v1/models", url))
                    .timeout(std::time::Duration::from_secs(5))
                    .send()
                    .await
                {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                            for model in data {
                                let id = model
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                // Don't duplicate if already found in scan
                                if !models.iter().any(|m| m.id.contains(&id)) {
                                    models.push(ModelInfo {
                                        id: format!("local:{}", id),
                                        object: "model".to_string(),
                                        created: 0,
                                        owned_by: "local".to_string(),
                                        provider: "local".to_string(),
                                        context_length: Some(4096),
                                        max_output: Some(1024),
                                        per_request_limits: None,
                                        pricing: Some(ModelPricing {
                                            prompt: 0.0,
                                            completion: 0.0,
                                            image: None,
                                            request: None,
                                        }),
                                        capabilities: ModelCapabilities {
                                            chat: true,
                                            completions: true,
                                            embeddings: false,
                                            image_generation: false,
                                            image_understanding: false,
                                            audio_generation: false,
                                            audio_understanding: false,
                                            video_understanding: false,
                                            function_calling: false,
                                            streaming: true,
                                        },
                                        local_path: None,
                                        format: ModelFormat::GGUF,
                                        size_bytes: None,
                                        metadata: HashMap::new(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(models)
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        // Delegate to llama.cpp server if available
        if let Some(url) = &self.llama_cpp_url {
            if self.check_llama_cpp_health().await {
                let body = json!({
                    "model": request.model,
                    "messages": request.messages.iter().map(|m| json!({
                        "role": m.role,
                        "content": m.content.as_ref().unwrap_or(&String::new())
                    })).collect::<Vec<_>>(),
                    "temperature": request.temperature.unwrap_or(0.7),
                    "max_tokens": request.max_tokens.unwrap_or(2048),
                    "stream": false
                });

                let resp = self
                    .client
                    .post(format!("{}/v1/chat/completions", url))
                    .json(&body)
                    .timeout(std::time::Duration::from_secs(300))
                    .send()
                    .await?;

                if resp.status().is_success() {
                    let result: serde_json::Value = resp.json().await?;
                    let content = result
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("message"))
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();

                    let usage = result.get("usage").cloned().unwrap_or(json!({}));

                    return Ok(ChatCompletionResponse {
                        id: result
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("local-0")
                            .to_string(),
                        object: "chat.completion".to_string(),
                        created: chrono::Utc::now().timestamp(),
                        model: request.model.clone(),
                        choices: vec![ChatChoice {
                            index: 0,
                            message: ChatMessage {
                                role: "assistant".to_string(),
                                content: Some(content),
                                name: None,
                                tool_calls: None,
                                function_call: None,
                                tool_call_id: None,
                            },
                            finish_reason: Some("stop".to_string()),
                            delta: None,
                        }],
                        usage: Some(Usage {
                            prompt_tokens: usage
                                .get("prompt_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u32,
                            completion_tokens: usage
                                .get("completion_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u32,
                            total_tokens: usage
                                .get("total_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u32,
                        }),
                        system_fingerprint: None,
                    });
                }
            }
        }

        Err(anyhow::anyhow!(
            "Local inference requires a running llama.cpp server. \
             Start one with: llama-server -m <model.gguf> --port 8080\n\
             Or set LLAMA_CPP_URL to point to your server."
        ))
    }

    async fn embeddings(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        // Delegate embeddings to llama.cpp server if available
        if let Some(url) = &self.llama_cpp_url {
            if self.check_llama_cpp_health().await {
                let body = json!({
                    "model": request.model,
                    "input": request.input
                });

                let resp = self
                    .client
                    .post(format!("{}/v1/embeddings", url))
                    .json(&body)
                    .timeout(std::time::Duration::from_secs(60))
                    .send()
                    .await?;

                if resp.status().is_success() {
                    let result: serde_json::Value = resp.json().await?;
                    let data = result
                        .get("data")
                        .and_then(|d| d.as_array())
                        .cloned()
                        .unwrap_or_default();

                    let embeddings: Vec<EmbeddingData> = data
                        .iter()
                        .enumerate()
                        .map(|(i, d)| EmbeddingData {
                            object: "embedding".to_string(),
                            index: i as u32,
                            embedding: d
                                .get("embedding")
                                .and_then(|e| e.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_f64())
                                        .map(|v| v as f32)
                                        .collect()
                                })
                                .unwrap_or_default(),
                        })
                        .collect();

                    return Ok(EmbeddingResponse {
                        object: "list".to_string(),
                        data: embeddings,
                        model: request.model,
                        usage: Usage {
                            prompt_tokens: result
                                .get("usage")
                                .and_then(|u| u.get("prompt_tokens"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u32,
                            completion_tokens: 0,
                            total_tokens: result
                                .get("usage")
                                .and_then(|u| u.get("total_tokens"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u32,
                        },
                    });
                }
            }
        }

        Err(anyhow::anyhow!(
            "Local embeddings require a running llama.cpp server with embedding support."
        ))
    }

    async fn validate_api_key(&self) -> Result<bool> {
        // Local provider doesn't need API keys — check if server is available
        Ok(self.check_llama_cpp_health().await)
    }

    fn get_provider_name(&self) -> &str {
        "local"
    }
}

/// vLLM provider for high-performance inference
pub struct VLLMProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl VLLMProvider {
    pub fn new() -> Self {
        Self::with_endpoint("http://localhost:8000".to_string())
    }

    pub fn with_endpoint(endpoint: String) -> Self {
        let api_key = std::env::var("VLLM_API_KEY").ok();
        Self {
            client: create_secure_client(),
            base_url: endpoint,
            api_key,
        }
    }
}

#[async_trait]
impl ModelProvider for VLLMProvider {
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(&format!("{}/v1/models", self.base_url))
            .send()
            .await?;

        let models_response: serde_json::Value = response.json().await?;
        let mut models = Vec::new();

        if let Some(data) = models_response.get("data").and_then(|d| d.as_array()) {
            for model in data {
                let model_info = ModelInfo {
                    id: model
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    object: "model".to_string(),
                    created: chrono::Utc::now().timestamp(),
                    owned_by: "vllm".to_string(),
                    provider: "vllm".to_string(),
                    context_length: model
                        .get("max_model_len")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32),
                    max_output: None,
                    per_request_limits: None,
                    pricing: None, // Local inference is free
                    capabilities: ModelCapabilities {
                        chat: true,
                        completions: true,
                        embeddings: false,
                        image_generation: false,
                        image_understanding: false,
                        audio_generation: false,
                        audio_understanding: false,
                        video_understanding: false,
                        function_calling: false,
                        streaming: true,
                    },
                    local_path: None,
                    format: ModelFormat::GGUF, // vLLM typically uses GGUF or similar
                    size_bytes: None,
                    metadata: HashMap::new(),
                };
                models.push(model_info);
            }
        }

        Ok(models)
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        let payload = json!({
            "model": request.model,
            "messages": request.messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "top_p": request.top_p,
            "n": request.n,
            "stream": request.stream,
            "stop": request.stop,
            "presence_penalty": request.presence_penalty,
            "frequency_penalty": request.frequency_penalty,
        });

        let mut req = self
            .client
            .post(&format!("{}/v1/chat/completions", self.base_url))
            .header("Content-Type", "application/json")
            .json(&payload);

        if let Some(api_key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("vLLM API error: {}", error_text));
        }

        let chat_response: ChatCompletionResponse = response.json().await?;
        Ok(chat_response)
    }

    async fn embeddings(&self, _request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        Err(anyhow::anyhow!("vLLM embeddings not implemented"))
    }

    async fn validate_api_key(&self) -> Result<bool> {
        let response = self
            .client
            .get(&format!("{}/v1/models", self.base_url))
            .send()
            .await?;

        Ok(response.status().is_success())
    }

    fn get_provider_name(&self) -> &str {
        "vllm"
    }
}

/// TensorRT-LLM provider for NVIDIA GPU optimized inference
pub struct TensorRTLLMProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl TensorRTLLMProvider {
    pub fn new() -> Self {
        Self::with_endpoint("http://localhost:8001".to_string())
    }

    pub fn with_endpoint(endpoint: String) -> Self {
        let api_key = std::env::var("TENSORRT_LLM_API_KEY").ok();
        Self {
            client: create_secure_client(),
            base_url: endpoint,
            api_key,
        }
    }
}

#[async_trait]
impl ModelProvider for TensorRTLLMProvider {
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(&format!("{}/v1/models", self.base_url))
            .send()
            .await?;

        let models_response: serde_json::Value = response.json().await?;
        let mut models = Vec::new();

        if let Some(data) = models_response.get("data").and_then(|d| d.as_array()) {
            for model in data {
                let model_info = ModelInfo {
                    id: model
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    object: "model".to_string(),
                    created: chrono::Utc::now().timestamp(),
                    owned_by: "tensorrt-llm".to_string(),
                    provider: "tensorrt-llm".to_string(),
                    context_length: Some(8192), // TensorRT-LLM typically has good context support
                    max_output: Some(4096),
                    per_request_limits: None,
                    pricing: None, // Local inference is free
                    capabilities: ModelCapabilities {
                        chat: true,
                        completions: true,
                        embeddings: false,
                        image_generation: false,
                        image_understanding: false,
                        audio_generation: false,
                        audio_understanding: false,
                        video_understanding: false,
                        function_calling: false,
                        streaming: true,
                    },
                    local_path: None,
                    format: ModelFormat::TensorRT,
                    size_bytes: None,
                    metadata: HashMap::new(),
                };
                models.push(model_info);
            }
        }

        Ok(models)
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        let payload = json!({
            "model": request.model,
            "messages": request.messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "top_p": request.top_p,
            "stream": request.stream,
            "stop": request.stop,
        });

        let mut req = self
            .client
            .post(&format!("{}/v1/chat/completions", self.base_url))
            .header("Content-Type", "application/json")
            .json(&payload);

        if let Some(api_key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("TensorRT-LLM API error: {}", error_text));
        }

        let chat_response: ChatCompletionResponse = response.json().await?;
        Ok(chat_response)
    }

    async fn embeddings(&self, _request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        Err(anyhow::anyhow!("TensorRT-LLM embeddings not implemented"))
    }

    async fn validate_api_key(&self) -> Result<bool> {
        let response = self
            .client
            .get(&format!("{}/v1/models", self.base_url))
            .send()
            .await?;

        Ok(response.status().is_success())
    }

    fn get_provider_name(&self) -> &str {
        "tensorrt-llm"
    }
}

/// SGLang provider for high-throughput serving
pub struct SGLangProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl SGLangProvider {
    pub fn new() -> Self {
        Self::with_endpoint("http://localhost:30000".to_string())
    }

    pub fn with_endpoint(endpoint: String) -> Self {
        let api_key = std::env::var("SGLANG_API_KEY").ok();
        Self {
            client: create_secure_client(),
            base_url: endpoint,
            api_key,
        }
    }
}

#[async_trait]
impl ModelProvider for SGLangProvider {
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(&format!("{}/v1/models", self.base_url))
            .send()
            .await?;

        let models_response: serde_json::Value = response.json().await?;
        let mut models = Vec::new();

        if let Some(data) = models_response.get("data").and_then(|d| d.as_array()) {
            for model in data {
                let model_info = ModelInfo {
                    id: model
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    object: "model".to_string(),
                    created: chrono::Utc::now().timestamp(),
                    owned_by: "sglang".to_string(),
                    provider: "sglang".to_string(),
                    context_length: Some(32768), // SGLang supports long contexts
                    max_output: Some(8192),
                    per_request_limits: None,
                    pricing: None, // Local inference is free
                    capabilities: ModelCapabilities {
                        chat: true,
                        completions: true,
                        embeddings: false,
                        image_generation: false,
                        image_understanding: false,
                        audio_generation: false,
                        audio_understanding: false,
                        video_understanding: false,
                        function_calling: true, // SGLang supports function calling
                        streaming: true,
                    },
                    local_path: None,
                    format: ModelFormat::GGUF,
                    size_bytes: None,
                    metadata: HashMap::new(),
                };
                models.push(model_info);
            }
        }

        Ok(models)
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        let payload = json!({
            "model": request.model,
            "messages": request.messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "top_p": request.top_p,
            "n": request.n,
            "stream": request.stream,
            "stop": request.stop,
            "tools": request.tools,
            "tool_choice": request.tool_choice,
        });

        let mut req = self
            .client
            .post(&format!("{}/v1/chat/completions", self.base_url))
            .header("Content-Type", "application/json")
            .json(&payload);

        if let Some(api_key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("SGLang API error: {}", error_text));
        }

        let chat_response: ChatCompletionResponse = response.json().await?;
        Ok(chat_response)
    }

    async fn embeddings(&self, _request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        Err(anyhow::anyhow!("SGLang embeddings not implemented"))
    }

    async fn validate_api_key(&self) -> Result<bool> {
        let response = self
            .client
            .get(&format!("{}/v1/models", self.base_url))
            .send()
            .await?;

        Ok(response.status().is_success())
    }

    fn get_provider_name(&self) -> &str {
        "sglang"
    }
}

/// llama.cpp provider for CPU and GPU inference
pub struct LlamaCppProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl LlamaCppProvider {
    pub fn new() -> Self {
        Self::with_endpoint("http://localhost:8080".to_string())
    }

    pub fn with_endpoint(endpoint: String) -> Self {
        let api_key = std::env::var("LLAMACPP_API_KEY").ok();
        Self {
            client: create_secure_client(),
            base_url: endpoint,
            api_key,
        }
    }
}

#[async_trait]
impl ModelProvider for LlamaCppProvider {
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // llama.cpp doesn't have a standard models endpoint, so we try to get info from the health endpoint
        let response = self
            .client
            .get(&format!("{}/health", self.base_url))
            .send()
            .await?;

        if response.status().is_success() {
            let health_info: serde_json::Value = response.json().await?;

            let model_name = health_info
                .get("model_name")
                .and_then(|v| v.as_str())
                .unwrap_or("llama-model")
                .to_string();

            let context_length = health_info
                .get("n_ctx")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .unwrap_or(2048);

            let models = vec![ModelInfo {
                id: model_name.clone(),
                object: "model".to_string(),
                created: chrono::Utc::now().timestamp(),
                owned_by: "llama.cpp".to_string(),
                provider: "llama.cpp".to_string(),
                context_length: Some(context_length),
                max_output: Some(context_length / 2), // Conservative estimate
                per_request_limits: None,
                pricing: None, // Local inference is free
                capabilities: ModelCapabilities {
                    chat: true,
                    completions: true,
                    embeddings: true, // llama.cpp supports embeddings
                    image_generation: false,
                    image_understanding: false,
                    audio_generation: false,
                    audio_understanding: false,
                    video_understanding: false,
                    function_calling: false,
                    streaming: true,
                },
                local_path: None,
                format: ModelFormat::GGUF,
                size_bytes: None,
                metadata: HashMap::new(),
            }];

            Ok(models)
        } else {
            Ok(vec![])
        }
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        // Convert messages to a single prompt for llama.cpp
        let mut prompt = String::new();
        for msg in &request.messages {
            let default_content = "".to_string();
            let content = msg.content.as_ref().unwrap_or(&default_content);
            match msg.role.as_str() {
                "system" => prompt.push_str(&format!("System: {}\n", content)),
                "user" => prompt.push_str(&format!("User: {}\n", content)),
                "assistant" => prompt.push_str(&format!("Assistant: {}\n", content)),
                _ => prompt.push_str(&format!("{}: {}\n", msg.role, content)),
            }
        }
        prompt.push_str("Assistant: ");

        let payload = json!({
            "prompt": prompt,
            "n_predict": request.max_tokens.unwrap_or(512),
            "temperature": request.temperature.unwrap_or(0.7),
            "top_p": request.top_p.unwrap_or(1.0),
            "stream": request.stream.unwrap_or(false),
            "stop": request.stop,
        });

        let mut req = self
            .client
            .post(&format!("{}/completion", self.base_url))
            .header("Content-Type", "application/json")
            .json(&payload);

        if let Some(api_key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("llama.cpp API error: {}", error_text));
        }

        let llama_response: serde_json::Value = response.json().await?;

        let content = llama_response
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let usage = Usage {
            prompt_tokens: llama_response
                .get("tokens_evaluated")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            completion_tokens: llama_response
                .get("tokens_predicted")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            total_tokens: 0, // Will be calculated below
        };

        let total_tokens = usage.prompt_tokens + usage.completion_tokens;

        Ok(ChatCompletionResponse {
            id: format!(
                "llamacpp-{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
            object: "chat.completion".to_string(),
            created: chrono::Utc::now().timestamp(),
            model: request.model,
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: Some(content),
                    name: None,
                    function_call: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
                finish_reason: Some("stop".to_string()),
                delta: None,
            }],
            usage: Some(Usage {
                total_tokens,
                ..usage
            }),
            system_fingerprint: None,
        })
    }

    async fn embeddings(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let input_text = if request.input.len() == 1 {
            request.input[0].clone()
        } else {
            request.input.join(" ")
        };

        let payload = json!({
            "content": input_text,
        });

        let mut req = self
            .client
            .post(&format!("{}/embedding", self.base_url))
            .header("Content-Type", "application/json")
            .json(&payload);

        if let Some(api_key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("llama.cpp embedding error: {}", error_text));
        }

        let llama_response: serde_json::Value = response.json().await?;

        let embedding = llama_response
            .get("embedding")
            .and_then(|v| v.as_array())
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect::<Vec<f32>>();

        Ok(EmbeddingResponse {
            object: "list".to_string(),
            data: vec![EmbeddingData {
                object: "embedding".to_string(),
                index: 0,
                embedding,
            }],
            model: request.model,
            usage: Usage {
                prompt_tokens: input_text.split_whitespace().count() as u32,
                completion_tokens: 0,
                total_tokens: input_text.split_whitespace().count() as u32,
            },
        })
    }

    async fn validate_api_key(&self) -> Result<bool> {
        let response = self
            .client
            .get(&format!("{}/health", self.base_url))
            .send()
            .await?;

        Ok(response.status().is_success())
    }

    fn get_provider_name(&self) -> &str {
        "llama.cpp"
    }
}

// ============================================================================
// Ollama Provider
// ============================================================================

/// Ollama provider — local model serving via ollama (http://localhost:11434)
pub struct OllamaProvider {
    client: Client,
    base_url: String,
}

impl OllamaProvider {
    pub fn new(base_url: Option<&str>) -> Self {
        Self {
            client: create_secure_client(),
            base_url: base_url
                .unwrap_or("http://localhost:11434")
                .trim_end_matches('/')
                .to_string(),
        }
    }

    /// Pull a model if not already available (auto-pull)
    pub async fn pull_model(&self, model: &str) -> Result<()> {
        let response = self
            .client
            .post(&format!("{}/api/pull", self.base_url))
            .json(&json!({ "name": model, "stream": false }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!(
                "Ollama pull failed for '{}': {}",
                model,
                error_text
            ));
        }
        Ok(())
    }

    /// Check if a specific model is available locally
    pub async fn has_model(&self, model: &str) -> bool {
        if let Ok(models) = self.list_models().await {
            models
                .iter()
                .any(|m| m.id == model || m.id.starts_with(&format!("{}:", model)))
        } else {
            false
        }
    }
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(&format!("{}/api/tags", self.base_url))
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let body: serde_json::Value = response.json().await?;
        let models = body
            .get("models")
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(models
            .iter()
            .filter_map(|m| {
                let name = m.get("name")?.as_str()?.to_string();
                let size = m.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
                let modified = m
                    .get("modified_at")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();

                // Infer context length from model family
                let context_length = if name.contains("llama") {
                    131072
                } else if name.contains("mistral") || name.contains("mixtral") {
                    32768
                } else if name.contains("gemma") {
                    8192
                } else if name.contains("phi") {
                    131072
                } else if name.contains("qwen") {
                    131072
                } else if name.contains("deepseek") {
                    131072
                } else {
                    4096
                };

                Some(ModelInfo {
                    id: name.clone(),
                    object: "model".to_string(),
                    created: 0,
                    owned_by: "local".to_string(),
                    provider: "ollama".to_string(),
                    context_length: Some(context_length),
                    max_output: None,
                    per_request_limits: None,
                    pricing: None,
                    capabilities: ModelCapabilities {
                        chat: true,
                        completions: true,
                        embeddings: true,
                        image_generation: false,
                        image_understanding: name.contains("llava") || name.contains("vision"),
                        audio_generation: false,
                        audio_understanding: false,
                        video_understanding: false,
                        function_calling: name.contains("llama") || name.contains("mistral"),
                        streaming: true,
                    },
                    local_path: None,
                    format: ModelFormat::GGUF,
                    size_bytes: Some(size),
                    metadata: {
                        let mut m = HashMap::new();
                        if !modified.is_empty() {
                            m.insert(
                                "modified_at".to_string(),
                                serde_json::Value::String(modified),
                            );
                        }
                        m
                    },
                })
            })
            .collect())
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        // Convert to Ollama /api/chat format
        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect();

        let mut body = json!({
            "model": request.model,
            "messages": messages,
            "stream": false
        });

        if let Some(temp) = request.temperature {
            body["options"] = json!({ "temperature": temp });
        }
        if let Some(max) = request.max_tokens {
            if let Some(opts) = body.get_mut("options") {
                opts["num_predict"] = json!(max);
            } else {
                body["options"] = json!({ "num_predict": max });
            }
        }

        let response = self
            .client
            .post(&format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;

            // Auto-pull if model not found
            if status.as_u16() == 404 || error_text.contains("not found") {
                return Err(anyhow::anyhow!(
                    "Model '{}' not found. Use ollama pull {} or set auto_pull: true. Error: {}",
                    request.model,
                    request.model,
                    error_text
                ));
            }
            return Err(anyhow::anyhow!(
                "Ollama chat error ({}): {}",
                status,
                error_text
            ));
        }

        let ollama_resp: serde_json::Value = response.json().await?;

        let content = ollama_resp
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let eval_count = ollama_resp
            .get("eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(content.split_whitespace().count() as u64) as u32;
        let prompt_eval_count = ollama_resp
            .get("prompt_eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        Ok(ChatCompletionResponse {
            id: format!("ollama-{}", chrono::Utc::now().timestamp()),
            object: "chat.completion".to_string(),
            created: chrono::Utc::now().timestamp(),
            model: request.model,
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: Some(content),
                    name: None,
                    function_call: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
                finish_reason: Some("stop".to_string()),
                delta: None,
            }],
            usage: Some(Usage {
                prompt_tokens: prompt_eval_count,
                completion_tokens: eval_count,
                total_tokens: prompt_eval_count + eval_count,
            }),
            system_fingerprint: None,
        })
    }

    async fn embeddings(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let input_text = request.input.join(" ");

        let response = self
            .client
            .post(&format!("{}/api/embeddings", self.base_url))
            .json(&json!({
                "model": request.model,
                "prompt": input_text
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Ollama embedding error: {}", error_text));
        }

        let ollama_resp: serde_json::Value = response.json().await?;

        let embedding = ollama_resp
            .get("embedding")
            .and_then(|e| e.as_array())
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect::<Vec<f32>>();

        Ok(EmbeddingResponse {
            object: "list".to_string(),
            data: vec![EmbeddingData {
                object: "embedding".to_string(),
                index: 0,
                embedding,
            }],
            model: request.model,
            usage: Usage {
                prompt_tokens: input_text.split_whitespace().count() as u32,
                completion_tokens: 0,
                total_tokens: input_text.split_whitespace().count() as u32,
            },
        })
    }

    async fn validate_api_key(&self) -> Result<bool> {
        // Ollama doesn't use API keys. Check if server is reachable.
        let response = self
            .client
            .get(&self.base_url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await;

        Ok(response.is_ok())
    }

    fn get_provider_name(&self) -> &str {
        "ollama"
    }
}

// ============================================================================
// LM Studio Provider
// ============================================================================

/// LM Studio provider — local models via OpenAI-compatible API (http://localhost:1234)
pub struct LMStudioProvider {
    client: Client,
    base_url: String,
}

impl LMStudioProvider {
    pub fn new(base_url: Option<&str>) -> Self {
        Self {
            client: create_secure_client(),
            base_url: base_url
                .unwrap_or("http://localhost:1234")
                .trim_end_matches('/')
                .to_string(),
        }
    }
}

#[async_trait]
impl ModelProvider for LMStudioProvider {
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(&format!("{}/v1/models", self.base_url))
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let body: serde_json::Value = response.json().await?;
        let models = body
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(models
            .iter()
            .filter_map(|m| {
                let id = m.get("id")?.as_str()?.to_string();
                Some(ModelInfo {
                    id: id.clone(),
                    object: "model".to_string(),
                    created: m.get("created").and_then(|c| c.as_i64()).unwrap_or(0),
                    owned_by: "lmstudio".to_string(),
                    provider: "lmstudio".to_string(),
                    context_length: Some(4096),
                    max_output: None,
                    per_request_limits: None,
                    pricing: None,
                    capabilities: ModelCapabilities {
                        chat: true,
                        completions: true,
                        embeddings: true,
                        image_generation: false,
                        image_understanding: false,
                        audio_generation: false,
                        audio_understanding: false,
                        video_understanding: false,
                        function_calling: true,
                        streaming: true,
                    },
                    local_path: None,
                    format: ModelFormat::GGUF,
                    size_bytes: None,
                    metadata: HashMap::new(),
                })
            })
            .collect())
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        // LM Studio speaks OpenAI-compatible API
        let response = self
            .client
            .post(&format!("{}/v1/chat/completions", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("LM Studio error: {}", error_text));
        }

        let result: ChatCompletionResponse = response.json().await?;
        Ok(result)
    }

    async fn embeddings(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let response = self
            .client
            .post(&format!("{}/v1/embeddings", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("LM Studio embedding error: {}", error_text));
        }

        let result: EmbeddingResponse = response.json().await?;
        Ok(result)
    }

    async fn validate_api_key(&self) -> Result<bool> {
        let response = self
            .client
            .get(&format!("{}/v1/models", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await;

        Ok(response.map(|r| r.status().is_success()).unwrap_or(false))
    }

    fn get_provider_name(&self) -> &str {
        "lmstudio"
    }
}
