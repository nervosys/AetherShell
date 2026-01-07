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
            "gpt-4-turbo" | "gpt-4-turbo-preview" => Some(128000),
            "gpt-4" => Some(8192),
            "gpt-3.5-turbo" => Some(4096),
            "gpt-3.5-turbo-16k" => Some(16384),
            _ => None,
        }
    }

    fn get_pricing(&self, model_id: &str) -> Option<ModelPricing> {
        match model_id {
            "gpt-4-turbo" => Some(ModelPricing {
                prompt: 0.01,
                completion: 0.03,
                image: None,
                request: None,
            }),
            "gpt-4" => Some(ModelPricing {
                prompt: 0.03,
                completion: 0.06,
                image: None,
                request: None,
            }),
            "gpt-3.5-turbo" => Some(ModelPricing {
                prompt: 0.0015,
                completion: 0.002,
                image: None,
                request: None,
            }),
            _ => None,
        }
    }

    fn get_capabilities(&self, model_id: &str) -> ModelCapabilities {
        ModelCapabilities {
            chat: true,
            completions: true,
            embeddings: model_id.contains("embedding"),
            image_generation: model_id.contains("dall-e"),
            image_understanding: model_id.contains("vision") || model_id.contains("gpt-4"),
            audio_generation: false,
            audio_understanding: false,
            video_understanding: false,
            function_calling: !model_id.contains("embedding"),
            streaming: true,
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
            ModelInfo {
                id: "claude-3-opus-20240229".to_string(),
                object: "model".to_string(),
                created: 1709251200, // Approximate
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
pub struct LocalProvider {
    // This would integrate with llama.cpp, candle, or other local inference engines
}

impl LocalProvider {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl ModelProvider for LocalProvider {
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Return locally available models
        // This would scan the XDG model directory
        Ok(vec![])
    }

    async fn chat_completion(
        &self,
        _request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        // Implement local inference
        Err(anyhow::anyhow!("Local inference not yet implemented"))
    }

    async fn embeddings(&self, _request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        // Implement local embeddings
        Err(anyhow::anyhow!("Local embeddings not yet implemented"))
    }

    async fn validate_api_key(&self) -> Result<bool> {
        // Local provider doesn't need API keys
        Ok(true)
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
