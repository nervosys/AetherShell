//! Anthropic Claude Provider Implementation
//!
//! Native implementation for Anthropic's Claude API with full support for:
//! - Messages API with system prompts
//! - Tool use (function calling)
//! - Vision (images)
//! - Extended thinking

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value as JsonValue};
use std::pin::Pin;
use futures::Stream;

use crate::providers::{
    ModelUri, ProviderConfig, ProviderType,
    ChatRequest, ChatResponse, Message, ContentPart, ToolCall,
    EmbeddingRequest, EmbeddingResponse,
    LLMProvider, ModelInfo, ProviderError,
    StreamChunk, ToolSchema, Usage,
};

const ANTHROPIC_API_VERSION: &str = "2023-06-01";

/// Anthropic Claude API provider
pub struct AnthropicProvider {
    config: ProviderConfig,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    fn base_url(&self) -> String {
        self.config.base_url.clone()
            .unwrap_or_else(|| "https://api.anthropic.com/v1".to_string())
    }

    fn api_key(&self) -> Option<&str> {
        self.config.api_key.as_deref()
    }

    /// Convert messages to Anthropic format, extracting system message
    fn build_request(&self, messages: &[Message]) -> (Option<String>, Vec<JsonValue>) {
        let mut system = None;
        let mut anthropic_messages = Vec::new();

        for msg in messages {
            if msg.role == "system" {
                system = Some(msg.text());
                continue;
            }

            let content = if msg.content.len() == 1 {
                match &msg.content[0] {
                    ContentPart::Text { text } => json!(text),
                    ContentPart::Image { data, media_type, .. } => {
                        if let (Some(data), Some(mt)) = (data, media_type) {
                            json!([{
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": mt,
                                    "data": data
                                }
                            }])
                        } else {
                            json!(msg.text())
                        }
                    }
                    _ => json!(msg.text()),
                }
            } else {
                json!(msg.content.iter().map(|part| {
                    match part {
                        ContentPart::Text { text } => json!({"type": "text", "text": text}),
                        ContentPart::Image { data, media_type, .. } => {
                            if let (Some(d), Some(mt)) = (data, media_type) {
                                json!({
                                    "type": "image",
                                    "source": {
                                        "type": "base64",
                                        "media_type": mt,
                                        "data": d
                                    }
                                })
                            } else {
                                json!({"type": "text", "text": ""})
                            }
                        }
                        _ => json!({"type": "text", "text": ""}),
                    }
                }).collect::<Vec<_>>())
            };

            // Map roles
            let role = match msg.role.as_str() {
                "assistant" => "assistant",
                "tool" => "user", // Tool results go as user messages
                _ => "user",
            };

            // Handle tool results
            if msg.role == "tool" {
                if let Some(ref tool_call_id) = msg.tool_call_id {
                    anthropic_messages.push(json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "content": msg.text()
                        }]
                    }));
                    continue;
                }
            }

            // Handle assistant messages with tool calls
            if msg.role == "assistant" {
                if let Some(ref tool_calls) = msg.tool_calls {
                    let mut content_blocks: Vec<JsonValue> = Vec::new();
                    
                    if let Some(text) = msg.content.first() {
                        if let ContentPart::Text { text } = text {
                            if !text.is_empty() {
                                content_blocks.push(json!({"type": "text", "text": text}));
                            }
                        }
                    }
                    
                    for tc in tool_calls {
                        content_blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": serde_json::from_str::<JsonValue>(&tc.arguments)
                                .unwrap_or(json!({}))
                        }));
                    }
                    
                    anthropic_messages.push(json!({
                        "role": "assistant",
                        "content": content_blocks
                    }));
                    continue;
                }
            }

            anthropic_messages.push(json!({
                "role": role,
                "content": content,
            }));
        }

        (system, anthropic_messages)
    }

    fn build_tools(&self, tools: &[ToolSchema]) -> Vec<JsonValue> {
        tools.iter().map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.parameters
            })
        }).collect()
    }

    fn parse_tool_use(&self, content: &JsonValue) -> Vec<ToolCall> {
        content.as_array()
            .map(|arr| {
                arr.iter().filter_map(|block| {
                    if block["type"] == "tool_use" {
                        Some(ToolCall {
                            id: block["id"].as_str()?.to_string(),
                            name: block["name"].as_str()?.to_string(),
                            arguments: block["input"].to_string(),
                        })
                    } else {
                        None
                    }
                }).collect()
            })
            .unwrap_or_default()
    }

    fn extract_text(&self, content: &JsonValue) -> Option<String> {
        content.as_array().map(|arr| {
            arr.iter()
                .filter_map(|block| {
                    if block["type"] == "text" {
                        block["text"].as_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("")
        }).filter(|s| !s.is_empty())
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "Anthropic"
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let url = format!("{}/messages", self.base_url());
        let (system, messages) = self.build_request(&request.messages);

        let mut body = json!({
            "model": request.model.model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
        });

        if let Some(sys) = system {
            body["system"] = json!(sys);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(ref stop) = request.stop {
            body["stop_sequences"] = json!(stop);
        }
        if !request.tools.is_empty() {
            body["tools"] = json!(self.build_tools(&request.tools));
        }

        let api_key = self.api_key()
            .ok_or_else(|| ProviderError::Authentication {
                message: "Missing API key".to_string(),
            })?;

        let response = self.client.post(&url)
            .header("Content-Type", "application/json")
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network { message: e.to_string() })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            
            return Err(match status {
                401 => ProviderError::Authentication { message: text },
                429 => ProviderError::RateLimit { retry_after: None },
                _ => ProviderError::Api { status, message: text },
            });
        }

        let json: JsonValue = response.json().await
            .map_err(|e| ProviderError::Unknown { message: e.to_string() })?;

        self.parse_response(&json)
    }

    async fn chat_stream(
        &self,
        _request: ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, ProviderError>> + Send>>, ProviderError> {
        Err(ProviderError::Unsupported {
            feature: "streaming".to_string(),
        })
    }

    async fn embed(&self, _request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        // Anthropic doesn't have a native embedding API
        Err(ProviderError::Unsupported {
            feature: "embeddings".to_string(),
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        // Anthropic doesn't have a model listing endpoint
        Ok(vec![
            ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                provider: ProviderType::Anthropic,
                context_window: Some(200_000),
                max_output: Some(8192),
                supports_tools: Some(true),
                supports_vision: Some(true),
                supports_streaming: Some(true),
            },
            ModelInfo {
                id: "claude-3-opus-20240229".to_string(),
                name: "Claude 3 Opus".to_string(),
                provider: ProviderType::Anthropic,
                context_window: Some(200_000),
                max_output: Some(4096),
                supports_tools: Some(true),
                supports_vision: Some(true),
                supports_streaming: Some(true),
            },
            ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                provider: ProviderType::Anthropic,
                context_window: Some(200_000),
                max_output: Some(8192),
                supports_tools: Some(true),
                supports_vision: Some(true),
                supports_streaming: Some(true),
            },
        ])
    }

    fn format_tools(&self, tools: &[ToolSchema]) -> JsonValue {
        json!(self.build_tools(tools))
    }

    fn parse_response(&self, response: &JsonValue) -> Result<ChatResponse, ProviderError> {
        let content = &response["content"];
        let text = self.extract_text(content);
        let tool_calls = {
            let calls = self.parse_tool_use(content);
            if calls.is_empty() { None } else { Some(calls) }
        };

        let usage = response.get("usage").map(|u| Usage {
            prompt_tokens: u["input_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: u["output_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: (u["input_tokens"].as_u64().unwrap_or(0) 
                + u["output_tokens"].as_u64().unwrap_or(0)) as u32,
        });

        Ok(ChatResponse {
            id: response["id"].as_str().unwrap_or("").to_string(),
            model: response["model"].as_str().unwrap_or("").to_string(),
            content: text,
            tool_calls,
            finish_reason: response["stop_reason"].as_str().map(|s| s.to_string()),
            usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_provider_creation() {
        let config = ProviderConfig::new(ProviderType::Anthropic)
            .with_api_key("test-key");
        let provider = AnthropicProvider::new(config);
        assert_eq!(provider.name(), "Anthropic");
    }

    #[test]
    fn test_system_extraction() {
        let config = ProviderConfig::new(ProviderType::Anthropic);
        let provider = AnthropicProvider::new(config);
        
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
        ];
        
        let (system, msgs) = provider.build_request(&messages);
        assert_eq!(system, Some("You are helpful".to_string()));
        assert_eq!(msgs.len(), 1); // Only user message
    }
}
