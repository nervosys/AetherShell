//! Anthropic Claude Provider Implementation
//!
//! Native implementation for Anthropic's Claude API with full support for:
//! - Messages API with system prompts
//! - Tool use (function calling)
//! - Vision (images)

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;

use crate::providers::{
    ChatRequest, ChatResponse, ChatStream, ContentPart, EmbeddingRequest, EmbeddingResponse,
    FinishReason, ImageSource, LLMProvider, Message, ModelInfo, ProviderConfig, ProviderError,
    ProviderType, Role, StreamChunk, StreamDelta, TokenUsage, ToolCall, ToolFormat, ToolSchema,
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
        self.config
            .base_url
            .clone()
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
            if msg.role == Role::System {
                system = Some(msg.text_content());
                continue;
            }

            // Handle tool result messages
            if msg.role == Role::Tool {
                if let Some(ref tool_call_id) = msg.tool_call_id {
                    anthropic_messages.push(json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "content": msg.text_content()
                        }]
                    }));
                    continue;
                }
            }

            // Handle assistant messages with tool use
            if msg.role == Role::Assistant {
                let tool_uses: Vec<&ContentPart> = msg
                    .content
                    .iter()
                    .filter(|p| matches!(p, ContentPart::ToolUse { .. }))
                    .collect();

                if !tool_uses.is_empty() {
                    let mut content_blocks: Vec<JsonValue> = Vec::new();

                    // Add text content
                    let text = msg.text_content();
                    if !text.is_empty() {
                        content_blocks.push(json!({"type": "text", "text": text}));
                    }

                    // Add tool use blocks
                    for tu in &tool_uses {
                        if let ContentPart::ToolUse { id, name, input } = tu {
                            content_blocks.push(json!({
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": input
                            }));
                        }
                    }

                    anthropic_messages.push(json!({
                        "role": "assistant",
                        "content": content_blocks
                    }));
                    continue;
                }
            }

            // Standard message handling
            let content = if msg.content.len() == 1 {
                match &msg.content[0] {
                    ContentPart::Text { text } => json!(text),
                    ContentPart::Image { source } => match source {
                        ImageSource::Base64 { media_type, data } => json!([{
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": media_type,
                                "data": data
                            }
                        }]),
                        ImageSource::Url { url } => json!([{
                            "type": "image",
                            "source": {
                                "type": "url",
                                "url": url
                            }
                        }]),
                    },
                    _ => json!(msg.text_content()),
                }
            } else {
                json!(msg
                    .content
                    .iter()
                    .map(|part| match part {
                        ContentPart::Text { text } => json!({"type": "text", "text": text}),
                        ContentPart::Image { source } => match source {
                            ImageSource::Base64 { media_type, data } => json!({
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": media_type,
                                    "data": data
                                }
                            }),
                            ImageSource::Url { url } => json!({
                                "type": "image",
                                "source": { "type": "url", "url": url }
                            }),
                        },
                        _ => json!({"type": "text", "text": ""}),
                    })
                    .collect::<Vec<_>>())
            };

            let role = match msg.role {
                Role::Assistant => "assistant",
                _ => "user",
            };

            anthropic_messages.push(json!({
                "role": role,
                "content": content,
            }));
        }

        (system, anthropic_messages)
    }

    fn build_tools(&self, tools: &[ToolSchema]) -> Vec<JsonValue> {
        tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.parameters
                })
            })
            .collect()
    }

    fn parse_tool_use(&self, content: &JsonValue) -> Vec<ToolCall> {
        content
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|block| {
                        if block["type"] == "tool_use" {
                            Some(ToolCall {
                                id: block["id"].as_str()?.to_string(),
                                name: block["name"].as_str()?.to_string(),
                                arguments: block["input"].clone(),
                            })
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn extract_text(&self, content: &JsonValue) -> String {
        content
            .as_array()
            .map(|arr| {
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
            })
            .unwrap_or_default()
    }

    fn parse_finish_reason(&self, reason: Option<&str>) -> FinishReason {
        match reason {
            Some("end_turn") | Some("stop") => FinishReason::Stop,
            Some("max_tokens") => FinishReason::Length,
            Some("tool_use") => FinishReason::ToolCalls,
            Some("content_filter") => FinishReason::ContentFilter,
            _ => FinishReason::Unknown,
        }
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Anthropic
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn is_available(&self) -> bool {
        self.api_key().is_some()
    }

    async fn validate_credentials(&self) -> Result<(), ProviderError> {
        if self.api_key().is_some() {
            Ok(())
        } else {
            Err(ProviderError::AuthenticationFailed {
                message: "Missing ANTHROPIC_API_KEY".to_string(),
            })
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(vec![
            ModelInfo {
                id: "claude-4-opus-20260214".to_string(),
                name: "Claude 4 Opus".to_string(),
                provider: ProviderType::Anthropic,
                context_length: 200_000,
                max_output_tokens: Some(16384),
                supports_tools: true,
                supports_vision: true,
                supports_audio: false,
                supports_json_mode: true,
                input_cost_per_million: Some(15.0),
                output_cost_per_million: Some(75.0),
                capabilities: vec![
                    "chat".to_string(),
                    "vision".to_string(),
                    "tools".to_string(),
                ],
            },
            ModelInfo {
                id: "claude-4-sonnet-20260214".to_string(),
                name: "Claude 4 Sonnet".to_string(),
                provider: ProviderType::Anthropic,
                context_length: 200_000,
                max_output_tokens: Some(16384),
                supports_tools: true,
                supports_vision: true,
                supports_audio: false,
                supports_json_mode: true,
                input_cost_per_million: Some(3.0),
                output_cost_per_million: Some(15.0),
                capabilities: vec![
                    "chat".to_string(),
                    "vision".to_string(),
                    "tools".to_string(),
                ],
            },
            ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                provider: ProviderType::Anthropic,
                context_length: 200_000,
                max_output_tokens: Some(8192),
                supports_tools: true,
                supports_vision: true,
                supports_audio: false,
                supports_json_mode: true,
                input_cost_per_million: Some(3.0),
                output_cost_per_million: Some(15.0),
                capabilities: vec![
                    "chat".to_string(),
                    "vision".to_string(),
                    "tools".to_string(),
                ],
            },
            ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                provider: ProviderType::Anthropic,
                context_length: 200_000,
                max_output_tokens: Some(8192),
                supports_tools: true,
                supports_vision: true,
                supports_audio: false,
                supports_json_mode: true,
                input_cost_per_million: Some(0.80),
                output_cost_per_million: Some(4.0),
                capabilities: vec![
                    "chat".to_string(),
                    "vision".to_string(),
                    "tools".to_string(),
                ],
            },
        ])
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
        if let Some(ref tools) = request.tools {
            if !tools.is_empty() {
                body["tools"] = json!(self.build_tools(tools));
            }
        }

        let api_key = self
            .api_key()
            .ok_or_else(|| ProviderError::AuthenticationFailed {
                message: "Missing API key".to_string(),
            })?;

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
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

    async fn chat_stream(&self, request: ChatRequest) -> Result<ChatStream, ProviderError> {
        let url = format!("{}/messages", self.base_url());
        let (system, messages) = self.build_request(&request.messages);

        let mut body = json!({
            "model": request.model.model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "stream": true,
        });

        if let Some(sys) = system {
            body["system"] = json!(sys);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(ref tools) = request.tools {
            if !tools.is_empty() {
                body["tools"] = json!(self.build_tools(tools));
            }
        }

        let api_key = self
            .api_key()
            .ok_or_else(|| ProviderError::AuthenticationFailed {
                message: "Missing API key".to_string(),
            })?;

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
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

        use futures::StreamExt;
        let byte_stream = response.bytes_stream();

        let stream = async_stream::stream! {
            let mut buffer = String::new();
            futures::pin_mut!(byte_stream);

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        yield Err(ProviderError::NetworkError { message: e.to_string() });
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(json) = serde_json::from_str::<JsonValue>(data) {
                            let event_type = json["type"].as_str().unwrap_or("");

                            match event_type {
                                "content_block_delta" => {
                                    let delta = &json["delta"];
                                    let stream_delta = if let Some(text) = delta["text"].as_str() {
                                        StreamDelta::Text(text.to_string())
                                    } else if delta["type"].as_str() == Some("input_json_delta") {
                                        StreamDelta::ToolCall {
                                            index: json["index"].as_u64().unwrap_or(0) as usize,
                                            id: None,
                                            name: None,
                                            arguments: delta["partial_json"].as_str().map(String::from),
                                        }
                                    } else {
                                        continue;
                                    };

                                    yield Ok(StreamChunk {
                                        id: None,
                                        delta: stream_delta,
                                        finish_reason: None,
                                        usage: None,
                                    });
                                }
                                "content_block_start" => {
                                    if let Some(cb) = json["content_block"].as_object() {
                                        if cb.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                                            yield Ok(StreamChunk {
                                                id: cb.get("id").and_then(|v| v.as_str()).map(String::from),
                                                delta: StreamDelta::ToolCall {
                                                    index: json["index"].as_u64().unwrap_or(0) as usize,
                                                    id: cb.get("id").and_then(|v| v.as_str()).map(String::from),
                                                    name: cb.get("name").and_then(|v| v.as_str()).map(String::from),
                                                    arguments: None,
                                                },
                                                finish_reason: None,
                                                usage: None,
                                            });
                                        }
                                    }
                                }
                                "message_delta" => {
                                    let finish = json["delta"]["stop_reason"].as_str().and_then(|r| match r {
                                        "end_turn" | "stop" => Some(FinishReason::Stop),
                                        "max_tokens" => Some(FinishReason::Length),
                                        "tool_use" => Some(FinishReason::ToolCalls),
                                        _ => None,
                                    });
                                    let usage = json["usage"].as_object().map(|u| TokenUsage {
                                        prompt_tokens: 0,
                                        completion_tokens: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                                        total_tokens: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                                        cached_tokens: None,
                                    });
                                    yield Ok(StreamChunk {
                                        id: None,
                                        delta: StreamDelta::Text(String::new()),
                                        finish_reason: finish,
                                        usage,
                                    });
                                }
                                "message_stop" => {
                                    return;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn embed(&self, _request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        Err(ProviderError::Unavailable {
            message: "Anthropic doesn't have a native embedding API".to_string(),
        })
    }

    fn tool_format(&self) -> ToolFormat {
        ToolFormat::Anthropic
    }

    fn supports(&self, capability: &str) -> bool {
        match capability {
            "chat" | "tools" | "vision" => true,
            "streaming" => true,
            "embeddings" | "audio" => false,
            _ => false,
        }
    }

    fn auth_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        if let Some(key) = self.api_key() {
            headers.insert("x-api-key".to_string(), key.to_string());
            headers.insert(
                "anthropic-version".to_string(),
                ANTHROPIC_API_VERSION.to_string(),
            );
        }
        headers
    }

    fn transform_request(&self, request: &ChatRequest) -> Result<JsonValue, ProviderError> {
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
        if let Some(ref tools) = request.tools {
            if !tools.is_empty() {
                body["tools"] = json!(self.build_tools(tools));
            }
        }
        Ok(body)
    }

    fn parse_response(&self, response: &JsonValue) -> Result<ChatResponse, ProviderError> {
        let content = &response["content"];
        let text = self.extract_text(content);
        let tool_calls = {
            let calls = self.parse_tool_use(content);
            if calls.is_empty() {
                None
            } else {
                Some(calls)
            }
        };

        let usage = if let Some(u) = response.get("usage") {
            TokenUsage {
                prompt_tokens: u["input_tokens"].as_u64().unwrap_or(0) as u32,
                completion_tokens: u["output_tokens"].as_u64().unwrap_or(0) as u32,
                total_tokens: (u["input_tokens"].as_u64().unwrap_or(0)
                    + u["output_tokens"].as_u64().unwrap_or(0))
                    as u32,
                cached_tokens: u
                    .get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64().map(|n| n as u32)),
            }
        } else {
            TokenUsage::default()
        };

        let finish_reason = self.parse_finish_reason(response["stop_reason"].as_str());

        Ok(ChatResponse {
            id: response["id"].as_str().unwrap_or("").to_string(),
            model: response["model"].as_str().unwrap_or("").to_string(),
            message: Message::assistant(text),
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
    fn test_anthropic_provider_creation() {
        let config = ProviderConfig::new(ProviderType::Anthropic).with_api_key("test-key");
        let provider = AnthropicProvider::new(config);
        assert_eq!(provider.provider_type(), ProviderType::Anthropic);
    }

    #[test]
    fn test_system_extraction() {
        let config = ProviderConfig::new(ProviderType::Anthropic);
        let provider = AnthropicProvider::new(config);

        let messages = vec![Message::system("You are helpful"), Message::user("Hello")];

        let (system, msgs) = provider.build_request(&messages);
        assert_eq!(system, Some("You are helpful".to_string()));
        assert_eq!(msgs.len(), 1);
    }
}
