//! Google Gemini Provider Implementation
//!
//! Native implementation for Google's Generative AI API (Gemini)

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;

use crate::providers::{
    ChatRequest, ChatResponse, ChatStream, ContentPart, EmbeddingRequest, EmbeddingResponse,
    FinishReason, ImageSource, LLMProvider, Message, ModelInfo, ProviderConfig,
    ProviderError, ProviderType, Role, TokenUsage, ToolCall, ToolFormat, ToolSchema,
};

/// Google Gemini API provider
pub struct GoogleProvider {
    config: ProviderConfig,
    client: Client,
}

impl GoogleProvider {
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
            .unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".to_string())
    }

    fn api_key(&self) -> Option<&str> {
        self.config.api_key.as_deref()
    }

    /// Convert messages to Gemini format
    fn build_contents(&self, messages: &[Message]) -> (Option<JsonValue>, Vec<JsonValue>) {
        let mut system_instruction = None;
        let mut contents = Vec::new();

        for msg in messages {
            if msg.role == Role::System {
                system_instruction = Some(json!({
                    "parts": [{"text": msg.text_content()}]
                }));
                continue;
            }

            let parts: Vec<JsonValue> = msg
                .content
                .iter()
                .map(|part| match part {
                    ContentPart::Text { text } => json!({"text": text}),
                    ContentPart::Image { source } => match source {
                        ImageSource::Base64 { media_type, data } => json!({
                            "inline_data": {
                                "mime_type": media_type,
                                "data": data
                            }
                        }),
                        ImageSource::Url { url } => json!({
                            "file_data": { "file_uri": url }
                        }),
                    },
                    _ => json!({"text": ""}),
                })
                .collect();

            // Map roles
            let role = match msg.role {
                Role::Assistant => "model",
                Role::Function => "function",
                _ => "user",
            };

            // Handle function/tool responses
            if msg.role == Role::Tool {
                if let Some(ref name) = msg.name {
                    contents.push(json!({
                        "role": "function",
                        "parts": [{
                            "functionResponse": {
                                "name": name,
                                "response": {
                                    "content": msg.text_content()
                                }
                            }
                        }]
                    }));
                    continue;
                }
            }

            // Handle assistant messages with tool calls
            if msg.role == Role::Assistant {
                let tool_uses: Vec<&ContentPart> = msg.content.iter()
                    .filter(|p| matches!(p, ContentPart::ToolUse { .. }))
                    .collect();

                if !tool_uses.is_empty() {
                    let mut all_parts = parts.clone();
                    for tu in &tool_uses {
                        if let ContentPart::ToolUse { name, input, .. } = tu {
                            all_parts.push(json!({
                                "functionCall": {
                                    "name": name,
                                    "args": input
                                }
                            }));
                        }
                    }
                    contents.push(json!({
                        "role": "model",
                        "parts": all_parts
                    }));
                    continue;
                }
            }

            contents.push(json!({
                "role": role,
                "parts": parts,
            }));
        }

        (system_instruction, contents)
    }

    fn build_tools(&self, tools: &[ToolSchema]) -> JsonValue {
        json!([{
            "function_declarations": tools.iter().map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters
                })
            }).collect::<Vec<_>>()
        }])
    }

    fn parse_function_calls(&self, parts: &JsonValue) -> Vec<ToolCall> {
        parts
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|part| {
                        part.get("functionCall").map(|fc| ToolCall {
                            id: format!("call_{}", uuid::Uuid::new_v4()),
                            name: fc["name"].as_str().unwrap_or("").to_string(),
                            arguments: fc["args"].clone(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn extract_text(&self, parts: &JsonValue) -> String {
        parts
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|part| part["text"].as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default()
    }

    fn parse_finish_reason(&self, reason: Option<&str>) -> FinishReason {
        match reason {
            Some("STOP") => FinishReason::Stop,
            Some("MAX_TOKENS") => FinishReason::Length,
            Some("SAFETY") => FinishReason::ContentFilter,
            _ => FinishReason::Unknown,
        }
    }
}

#[async_trait]
impl LLMProvider for GoogleProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Google
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
                message: "Missing GOOGLE_API_KEY".to_string(),
            })
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(vec![
            ModelInfo {
                id: "gemini-2.0-flash".to_string(),
                name: "Gemini 2.0 Flash".to_string(),
                provider: ProviderType::Google,
                context_length: 1_000_000,
                max_output_tokens: Some(8192),
                supports_tools: true,
                supports_vision: true,
                supports_audio: true,
                supports_json_mode: true,
                input_cost_per_million: Some(0.10),
                output_cost_per_million: Some(0.40),
                capabilities: vec!["chat".to_string(), "vision".to_string(), "tools".to_string(), "audio".to_string()],
            },
            ModelInfo {
                id: "gemini-2.5-pro".to_string(),
                name: "Gemini 2.5 Pro".to_string(),
                provider: ProviderType::Google,
                context_length: 1_000_000,
                max_output_tokens: Some(65536),
                supports_tools: true,
                supports_vision: true,
                supports_audio: true,
                supports_json_mode: true,
                input_cost_per_million: Some(1.25),
                output_cost_per_million: Some(10.0),
                capabilities: vec!["chat".to_string(), "vision".to_string(), "tools".to_string(), "audio".to_string()],
            },
            ModelInfo {
                id: "gemini-2.5-flash".to_string(),
                name: "Gemini 2.5 Flash".to_string(),
                provider: ProviderType::Google,
                context_length: 1_000_000,
                max_output_tokens: Some(65536),
                supports_tools: true,
                supports_vision: true,
                supports_audio: true,
                supports_json_mode: true,
                input_cost_per_million: Some(0.15),
                output_cost_per_million: Some(0.60),
                capabilities: vec!["chat".to_string(), "vision".to_string(), "tools".to_string(), "audio".to_string()],
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
        let api_key = self
            .api_key()
            .ok_or_else(|| ProviderError::AuthenticationFailed {
                message: "Missing API key".to_string(),
            })?;

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url(),
            request.model.model,
            api_key
        );

        let (system_instruction, contents) = self.build_contents(&request.messages);

        let mut body = json!({
            "contents": contents,
        });

        if let Some(sys) = system_instruction {
            body["system_instruction"] = sys;
        }

        let mut generation_config = json!({});
        if let Some(temp) = request.temperature {
            generation_config["temperature"] = json!(temp);
        }
        if let Some(max) = request.max_tokens {
            generation_config["maxOutputTokens"] = json!(max);
        }
        if let Some(ref stop) = request.stop {
            generation_config["stopSequences"] = json!(stop);
        }
        if generation_config != json!({}) {
            body["generationConfig"] = generation_config;
        }

        if let Some(ref tools) = request.tools {
            if !tools.is_empty() {
                body["tools"] = self.build_tools(tools);
            }
        }

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
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
                401 | 403 => ProviderError::AuthenticationFailed { message: text },
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
        let api_key = self
            .api_key()
            .ok_or_else(|| ProviderError::AuthenticationFailed {
                message: "Missing API key".to_string(),
            })?;

        let model = if request.model.model.contains("embedding") {
            request.model.model.clone()
        } else {
            "text-embedding-004".to_string()
        };

        let url = format!(
            "{}/models/{}:batchEmbedContents?key={}",
            self.base_url(),
            model,
            api_key
        );

        let requests: Vec<JsonValue> = request
            .input
            .iter()
            .map(|text| {
                json!({
                    "model": format!("models/{}", model),
                    "content": {"parts": [{"text": text}]}
                })
            })
            .collect();

        let body = json!({ "requests": requests });

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
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

        let embeddings = json["embeddings"]
            .as_array()
            .ok_or_else(|| ProviderError::Unknown {
                message: "Invalid response".to_string(),
            })?
            .iter()
            .filter_map(|e| {
                e["values"].as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect()
                })
            })
            .collect();

        Ok(EmbeddingResponse {
            embeddings,
            model,
            usage: TokenUsage::default(),
        })
    }

    fn tool_format(&self) -> ToolFormat {
        ToolFormat::Google
    }

    fn supports(&self, capability: &str) -> bool {
        match capability {
            "chat" | "tools" | "vision" | "embeddings" | "audio" | "json_mode" => true,
            "streaming" => false,
            _ => false,
        }
    }

    fn auth_headers(&self) -> HashMap<String, String> {
        // Google uses query params for auth, not headers
        HashMap::new()
    }

    fn transform_request(&self, request: &ChatRequest) -> Result<JsonValue, ProviderError> {
        let (system_instruction, contents) = self.build_contents(&request.messages);
        let mut body = json!({ "contents": contents });
        if let Some(sys) = system_instruction {
            body["system_instruction"] = sys;
        }
        if let Some(ref tools) = request.tools {
            if !tools.is_empty() {
                body["tools"] = self.build_tools(tools);
            }
        }
        Ok(body)
    }

    fn parse_response(&self, response: &JsonValue) -> Result<ChatResponse, ProviderError> {
        let candidate = response["candidates"]
            .get(0)
            .ok_or_else(|| ProviderError::Unknown {
                message: "No candidates in response".to_string(),
            })?;

        let parts = &candidate["content"]["parts"];
        let text = self.extract_text(parts);
        let tool_calls = {
            let calls = self.parse_function_calls(parts);
            if calls.is_empty() {
                None
            } else {
                Some(calls)
            }
        };

        let usage = if let Some(u) = response.get("usageMetadata") {
            TokenUsage {
                prompt_tokens: u["promptTokenCount"].as_u64().unwrap_or(0) as u32,
                completion_tokens: u["candidatesTokenCount"].as_u64().unwrap_or(0) as u32,
                total_tokens: u["totalTokenCount"].as_u64().unwrap_or(0) as u32,
                cached_tokens: u.get("cachedContentTokenCount").and_then(|v| v.as_u64().map(|n| n as u32)),
            }
        } else {
            TokenUsage::default()
        };

        let finish_reason = self.parse_finish_reason(
            candidate["finishReason"].as_str(),
        );

        Ok(ChatResponse {
            id: "".to_string(),
            model: response["modelVersion"].as_str().unwrap_or("").to_string(),
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
    fn test_google_provider_creation() {
        let config = ProviderConfig::new(ProviderType::Google).with_api_key("test-key");
        let provider = GoogleProvider::new(config);
        assert_eq!(provider.provider_type(), ProviderType::Google);
    }
}
