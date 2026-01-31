//! Google Gemini Provider Implementation
//!
//! Native implementation for Google's Generative AI API (Gemini)

use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde_json::{json, Value as JsonValue};
use std::pin::Pin;

use crate::providers::{
    ChatRequest, ChatResponse, ContentPart, EmbeddingRequest, EmbeddingResponse, LLMProvider,
    Message, ModelInfo, ModelUri, ProviderConfig, ProviderError, ProviderType, StreamChunk,
    ToolCall, ToolSchema, Usage,
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
            if msg.role == "system" {
                system_instruction = Some(json!({
                    "parts": [{"text": msg.text()}]
                }));
                continue;
            }

            let parts: Vec<JsonValue> = msg
                .content
                .iter()
                .map(|part| match part {
                    ContentPart::Text { text } => json!({"text": text}),
                    ContentPart::Image {
                        data, media_type, ..
                    } => {
                        if let (Some(d), Some(mt)) = (data, media_type) {
                            json!({
                                "inline_data": {
                                    "mime_type": mt,
                                    "data": d
                                }
                            })
                        } else {
                            json!({"text": ""})
                        }
                    }
                    _ => json!({"text": ""}),
                })
                .collect();

            // Map roles
            let role = match msg.role.as_str() {
                "assistant" => "model",
                "tool" => "function",
                _ => "user",
            };

            // Handle function responses
            if msg.role == "tool" {
                if let Some(ref name) = msg.name {
                    contents.push(json!({
                        "role": "function",
                        "parts": [{
                            "functionResponse": {
                                "name": name,
                                "response": {
                                    "content": msg.text()
                                }
                            }
                        }]
                    }));
                    continue;
                }
            }

            // Handle function calls from model
            if msg.role == "assistant" {
                if let Some(ref tool_calls) = msg.tool_calls {
                    let mut all_parts = parts.clone();
                    for tc in tool_calls {
                        all_parts.push(json!({
                            "functionCall": {
                                "name": tc.name,
                                "args": serde_json::from_str::<JsonValue>(&tc.arguments)
                                    .unwrap_or(json!({}))
                            }
                        }));
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
                            arguments: fc["args"].to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn extract_text(&self, parts: &JsonValue) -> Option<String> {
        parts
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|part| part["text"].as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .filter(|s| !s.is_empty())
    }
}

#[async_trait]
impl LLMProvider for GoogleProvider {
    fn name(&self) -> &str {
        "Google"
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let api_key = self
            .api_key()
            .ok_or_else(|| ProviderError::Authentication {
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

        if !request.tools.is_empty() {
            body["tools"] = self.build_tools(&request.tools);
        }

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network {
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();

            return Err(match status {
                401 | 403 => ProviderError::Authentication { message: text },
                429 => ProviderError::RateLimit { retry_after: None },
                _ => ProviderError::Api {
                    status,
                    message: text,
                },
            });
        }

        let json: JsonValue = response.json().await.map_err(|e| ProviderError::Unknown {
            message: e.to_string(),
        })?;

        self.parse_response(&json)
    }

    async fn chat_stream(
        &self,
        _request: ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, ProviderError>> + Send>>, ProviderError>
    {
        Err(ProviderError::Unsupported {
            feature: "streaming".to_string(),
        })
    }

    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        let api_key = self
            .api_key()
            .ok_or_else(|| ProviderError::Authentication {
                message: "Missing API key".to_string(),
            })?;

        // Use embedding model
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
            .map_err(|e| ProviderError::Network {
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status,
                message: text,
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
            usage: None,
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(vec![
            ModelInfo {
                id: "gemini-1.5-pro".to_string(),
                name: "Gemini 1.5 Pro".to_string(),
                provider: ProviderType::Google,
                context_window: Some(2_000_000),
                max_output: Some(8192),
                supports_tools: Some(true),
                supports_vision: Some(true),
                supports_streaming: Some(true),
            },
            ModelInfo {
                id: "gemini-1.5-flash".to_string(),
                name: "Gemini 1.5 Flash".to_string(),
                provider: ProviderType::Google,
                context_window: Some(1_000_000),
                max_output: Some(8192),
                supports_tools: Some(true),
                supports_vision: Some(true),
                supports_streaming: Some(true),
            },
            ModelInfo {
                id: "gemini-2.0-flash-exp".to_string(),
                name: "Gemini 2.0 Flash".to_string(),
                provider: ProviderType::Google,
                context_window: Some(1_000_000),
                max_output: Some(8192),
                supports_tools: Some(true),
                supports_vision: Some(true),
                supports_streaming: Some(true),
            },
        ])
    }

    fn format_tools(&self, tools: &[ToolSchema]) -> JsonValue {
        self.build_tools(tools)
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

        let usage = response.get("usageMetadata").map(|u| Usage {
            prompt_tokens: u["promptTokenCount"].as_u64().unwrap_or(0) as u32,
            completion_tokens: u["candidatesTokenCount"].as_u64().unwrap_or(0) as u32,
            total_tokens: u["totalTokenCount"].as_u64().unwrap_or(0) as u32,
        });

        Ok(ChatResponse {
            id: "".to_string(),
            model: response["modelVersion"].as_str().unwrap_or("").to_string(),
            content: text,
            tool_calls,
            finish_reason: candidate["finishReason"].as_str().map(|s| s.to_string()),
            usage,
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
        assert_eq!(provider.name(), "Google");
    }
}
