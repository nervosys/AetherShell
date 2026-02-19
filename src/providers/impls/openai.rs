//! OpenAI Provider Implementation
//!
//! Supports OpenAI API and all OpenAI-compatible providers including:
//! Azure OpenAI, Together, Groq, Perplexity, Fireworks, DeepSeek, xAI, OpenRouter, vLLM

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;

use crate::providers::{
    ChatRequest, ChatResponse, ChatStream, ContentPart, EmbeddingRequest, EmbeddingResponse,
    FinishReason, ImageSource, LLMProvider, Message, ModelInfo, ProviderConfig, ProviderError,
    ProviderType, TokenUsage, ToolCall, ToolFormat, ToolSchema,
};

/// OpenAI API provider (also handles OpenAI-compatible APIs)
pub struct OpenAIProvider {
    config: ProviderConfig,
    client: Client,
}

impl OpenAIProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    fn base_url(&self) -> String {
        self.config.effective_base_url()
    }

    fn api_key(&self) -> Option<&str> {
        self.config.api_key.as_deref()
    }

    fn build_messages(&self, messages: &[Message]) -> Vec<JsonValue> {
        messages
            .iter()
            .map(|msg| {
                let content = if msg.content.len() == 1 {
                    match &msg.content[0] {
                        ContentPart::Text { text } => json!(text),
                        ContentPart::Image { source } => match source {
                            ImageSource::Url { url } => json!([
                                {"type": "image_url", "image_url": {"url": url}}
                            ]),
                            ImageSource::Base64 { media_type, data } => json!([
                                {"type": "image_url", "image_url": {"url": format!("data:{};base64,{}", media_type, data)}}
                            ]),
                        },
                        _ => json!(msg.text_content()),
                    }
                } else {
                    json!(msg
                        .content
                        .iter()
                        .map(|part| {
                            match part {
                                ContentPart::Text { text } => json!({"type": "text", "text": text}),
                                ContentPart::Image { source } => match source {
                                    ImageSource::Url { url } => json!({
                                        "type": "image_url",
                                        "image_url": {"url": url}
                                    }),
                                    ImageSource::Base64 { media_type, data } => json!({
                                        "type": "image_url",
                                        "image_url": {"url": format!("data:{};base64,{}", media_type, data)}
                                    }),
                                },
                                _ => json!({"type": "text", "text": ""}),
                            }
                        })
                        .collect::<Vec<_>>())
                };

                // Extract tool calls from content parts
                let tool_use_parts: Vec<&ContentPart> = msg.content.iter()
                    .filter(|p| matches!(p, ContentPart::ToolUse { .. }))
                    .collect();

                let mut msg_obj = json!({
                    "role": msg.role,
                    "content": content,
                });

                if let Some(ref name) = msg.name {
                    msg_obj["name"] = json!(name);
                }

                if !tool_use_parts.is_empty() {
                    let tc_json: Vec<JsonValue> = tool_use_parts.iter().map(|p| {
                        if let ContentPart::ToolUse { id, name, input } = p {
                            json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": input.to_string()
                                }
                            })
                        } else {
                            json!(null)
                        }
                    }).collect();
                    msg_obj["tool_calls"] = json!(tc_json);
                }

                if let Some(ref tool_call_id) = msg.tool_call_id {
                    msg_obj["tool_call_id"] = json!(tool_call_id);
                }

                msg_obj
            })
            .collect()
    }

    fn build_tools(&self, tools: &[ToolSchema]) -> Vec<JsonValue> {
        tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters
                    }
                })
            })
            .collect()
    }

    fn parse_tool_calls_json(&self, tool_calls: &JsonValue) -> Vec<ToolCall> {
        tool_calls
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                        let args: JsonValue = serde_json::from_str(args_str).unwrap_or(json!({}));
                        Some(ToolCall {
                            id: tc["id"].as_str()?.to_string(),
                            name: tc["function"]["name"].as_str()?.to_string(),
                            arguments: args,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn parse_finish_reason(&self, reason: Option<&str>) -> FinishReason {
        match reason {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::Length,
            Some("tool_calls") | Some("function_call") => FinishReason::ToolCalls,
            Some("content_filter") => FinishReason::ContentFilter,
            _ => FinishReason::Unknown,
        }
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    fn provider_type(&self) -> ProviderType {
        self.config.provider
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/models", self.base_url());
        let mut req = self.client.get(&url);
        if let Some(key) = self.api_key() {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        req.send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn validate_credentials(&self) -> Result<(), ProviderError> {
        if self.is_available().await {
            Ok(())
        } else {
            Err(ProviderError::AuthenticationFailed {
                message: "Failed to validate API key".to_string(),
            })
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let url = format!("{}/models", self.base_url());
        let mut req = self.client.get(&url);
        if let Some(key) = self.api_key() {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req.send().await.map_err(|e| ProviderError::NetworkError {
            message: e.to_string(),
        })?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let json: JsonValue = response.json().await.map_err(|e| ProviderError::Unknown {
            message: e.to_string(),
        })?;

        let models = json["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        Some(ModelInfo {
                            id: m["id"].as_str()?.to_string(),
                            name: m["id"].as_str()?.to_string(),
                            provider: self.config.provider,
                            context_length: m["context_length"].as_u64().unwrap_or(128_000) as u32,
                            max_output_tokens: None,
                            supports_tools: true,
                            supports_vision: false,
                            supports_audio: false,
                            supports_json_mode: true,
                            input_cost_per_million: None,
                            output_cost_per_million: None,
                            capabilities: vec!["chat".to_string()],
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }

    async fn get_model_info(&self, model: &str) -> Result<ModelInfo, ProviderError> {
        let models = self.list_models().await?;
        models
            .into_iter()
            .find(|m| m.id == model)
            .ok_or_else(|| ProviderError::ModelNotFound {
                model: model.to_string(),
            })
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.base_url());

        let mut body = json!({
            "model": request.model.model,
            "messages": self.build_messages(&request.messages),
        });

        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(max) = request.max_tokens {
            body["max_tokens"] = json!(max);
        }
        if let Some(ref stop) = request.stop {
            body["stop"] = json!(stop);
        }
        if let Some(ref tools) = request.tools {
            if !tools.is_empty() {
                body["tools"] = json!(self.build_tools(tools));
            }
        }

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if let Some(key) = self.api_key() {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        // Provider-specific headers
        if self.config.provider == ProviderType::OpenRouter {
            req = req.header("HTTP-Referer", "https://aethershell.dev");
            req = req.header("X-Title", "AetherShell");
        }

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::NetworkError {
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();

            return Err(match status {
                401 => ProviderError::AuthenticationFailed { message: text },
                429 => ProviderError::RateLimited {
                    message: text,
                    retry_after: None,
                },
                _ => ProviderError::Unknown {
                    message: format!("API error {}: {}", status, text),
                },
            });
        }

        let json: JsonValue = response.json().await.map_err(|e| ProviderError::Unknown {
            message: e.to_string(),
        })?;

        self.parse_response(&json)
    }

    async fn chat_stream(&self, _request: ChatRequest) -> Result<ChatStream, ProviderError> {
        Err(ProviderError::Unavailable {
            message: "Streaming not yet implemented".to_string(),
        })
    }

    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        let url = format!("{}/embeddings", self.base_url());

        let body = json!({
            "model": request.model.model,
            "input": request.input,
        });

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if let Some(key) = self.api_key() {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::NetworkError {
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::Unknown {
                message: format!("API error {}: {}", status, text),
            });
        }

        let json: JsonValue = response.json().await.map_err(|e| ProviderError::Unknown {
            message: e.to_string(),
        })?;

        let embeddings = json["data"]
            .as_array()
            .ok_or_else(|| ProviderError::Unknown {
                message: "Invalid response".to_string(),
            })?
            .iter()
            .filter_map(|d| {
                d["embedding"].as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect()
                })
            })
            .collect();

        Ok(EmbeddingResponse {
            embeddings,
            model: request.model.model.clone(),
            usage: TokenUsage::default(),
        })
    }

    fn tool_format(&self) -> ToolFormat {
        self.config.provider.tool_format()
    }

    fn supports(&self, capability: &str) -> bool {
        match capability {
            "chat" | "embeddings" => true,
            "tools" => self.config.provider.supports_tools(),
            "vision" => self.config.provider.supports_vision(),
            "streaming" => self.config.provider.supports_streaming(),
            _ => false,
        }
    }

    fn auth_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        if let Some(key) = self.api_key() {
            headers.insert("Authorization".to_string(), format!("Bearer {}", key));
        }
        headers
    }

    fn transform_request(&self, request: &ChatRequest) -> Result<JsonValue, ProviderError> {
        let mut body = json!({
            "model": request.model.model,
            "messages": self.build_messages(&request.messages),
        });
        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(max) = request.max_tokens {
            body["max_tokens"] = json!(max);
        }
        if let Some(ref tools) = request.tools {
            if !tools.is_empty() {
                body["tools"] = json!(self.build_tools(tools));
            }
        }
        Ok(body)
    }

    fn parse_response(&self, response: &JsonValue) -> Result<ChatResponse, ProviderError> {
        let choice = response["choices"]
            .get(0)
            .ok_or_else(|| ProviderError::Unknown {
                message: "No choices in response".to_string(),
            })?;

        let msg_json = &choice["message"];
        let content = msg_json["content"].as_str().unwrap_or("").to_string();

        let tool_calls = msg_json
            .get("tool_calls")
            .filter(|tc| !tc.is_null())
            .map(|tc| self.parse_tool_calls_json(tc));

        let finish_reason = self.parse_finish_reason(choice["finish_reason"].as_str());

        let usage = if let Some(u) = response.get("usage") {
            TokenUsage {
                prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
                total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as u32,
                cached_tokens: u
                    .get("cached_tokens")
                    .and_then(|v| v.as_u64().map(|n| n as u32)),
            }
        } else {
            TokenUsage::default()
        };

        Ok(ChatResponse {
            id: response["id"].as_str().unwrap_or("").to_string(),
            model: response["model"].as_str().unwrap_or("").to_string(),
            message: Message::assistant(content),
            usage,
            finish_reason,
            tool_calls,
            metadata: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_creation() {
        let config = ProviderConfig::new(ProviderType::OpenAI).with_api_key("test-key");
        let provider = OpenAIProvider::new(config);
        assert_eq!(provider.provider_type(), ProviderType::OpenAI);
    }

    #[test]
    fn test_message_building() {
        let config = ProviderConfig::new(ProviderType::OpenAI);
        let provider = OpenAIProvider::new(config);

        let messages = vec![Message::system("You are helpful"), Message::user("Hello")];

        let built = provider.build_messages(&messages);
        assert_eq!(built.len(), 2);
        assert_eq!(built[0]["role"], "system");
        assert_eq!(built[1]["role"], "user");
    }
}
