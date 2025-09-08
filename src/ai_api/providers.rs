use crate::ai_api::models::*;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;

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
    api_key: Option<String>,
    base_url: String,
}

impl OpenAIProvider {
    pub fn new() -> Self {
        let api_key = std::env::var("OPENAI_API_KEY").ok();
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    pub fn with_api_key(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key: Some(api_key),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }
}

#[async_trait]
impl ModelProvider for OpenAIProvider {
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(&format!("{}/models", self.base_url))
            .header(
                "Authorization",
                format!(
                    "Bearer {}",
                    self.api_key.as_ref().unwrap_or(&"".to_string())
                ),
            )
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

        let response = self
            .client
            .post(&format!("{}/chat/completions", self.base_url))
            .header(
                "Authorization",
                format!(
                    "Bearer {}",
                    self.api_key.as_ref().unwrap_or(&"".to_string())
                ),
            )
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

        let response = self
            .client
            .post(&format!("{}/embeddings", self.base_url))
            .header(
                "Authorization",
                format!(
                    "Bearer {}",
                    self.api_key.as_ref().unwrap_or(&"".to_string())
                ),
            )
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
        if self.api_key.is_none() {
            return Ok(false);
        }

        let response = self
            .client
            .get(&format!("{}/models", self.base_url))
            .header(
                "Authorization",
                format!("Bearer {}", self.api_key.as_ref().unwrap()),
            )
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
    api_key: Option<String>,
    base_url: String,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok();
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.anthropic.com/v1".to_string(),
        }
    }

    pub fn with_api_key(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key: Some(api_key),
            base_url: "https://api.anthropic.com/v1".to_string(),
        }
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

        let response = self
            .client
            .post(&format!("{}/messages", self.base_url))
            .header(
                "x-api-key",
                self.api_key.as_ref().unwrap_or(&"".to_string()),
            )
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
        if self.api_key.is_none() {
            return Ok(false);
        }

        // Test with a minimal request
        let test_payload = json!({
            "model": "claude-3-sonnet-20240229",
            "messages": [{"role": "user", "content": "Hi"}],
            "max_tokens": 10
        });

        let response = self
            .client
            .post(&format!("{}/messages", self.base_url))
            .header("x-api-key", self.api_key.as_ref().unwrap())
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
