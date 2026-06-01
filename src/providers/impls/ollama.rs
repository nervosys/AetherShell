//! Ollama Provider Implementation
//!
//! Local LLM provider using Ollama's API

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;

use crate::providers::{
    ChatRequest, ChatResponse, ChatStream, ContentPart, EmbeddingRequest, EmbeddingResponse,
    FinishReason, ImageSource, LLMProvider, Message, ModelInfo, ProviderConfig, ProviderError,
    ProviderType, Role, StreamChunk, StreamDelta, TokenUsage, ToolCall, ToolFormat, ToolSchema,
};

/// Ollama local LLM provider
pub struct OllamaProvider {
    config: ProviderConfig,
    client: Client,
}

impl OllamaProvider {
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
            .unwrap_or_else(|| "http://localhost:11434".to_string())
    }

    fn build_messages(&self, messages: &[Message]) -> Vec<JsonValue> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::Assistant => "assistant",
                    Role::Tool | Role::Function => "tool",
                    _ => "user",
                };

                let mut msg_obj = json!({
                    "role": role,
                    "content": msg.text_content(),
                });

                // Handle images for vision models
                let images: Vec<String> = msg
                    .content
                    .iter()
                    .filter_map(|part| {
                        if let ContentPart::Image { source } = part {
                            match source {
                                ImageSource::Base64 { data, .. } => Some(data.clone()),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    })
                    .collect();

                if !images.is_empty() {
                    msg_obj["images"] = json!(images);
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
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Ollama
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url());
        self.client
            .get(&url)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn validate_credentials(&self) -> Result<(), ProviderError> {
        if self.is_available().await {
            Ok(())
        } else {
            Err(ProviderError::Unavailable {
                message: "Ollama is not running at the configured address".to_string(),
            })
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let url = format!("{}/api/tags", self.base_url());

        let response =
            self.client
                .get(&url)
                .send()
                .await
                .map_err(|e| ProviderError::NetworkError {
                    message: e.to_string(),
                })?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let json: JsonValue = response.json().await.map_err(|e| ProviderError::Unknown {
            message: e.to_string(),
        })?;

        let models = json["models"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let name = m["name"].as_str()?.to_string();
                        let size = m["size"].as_u64().unwrap_or(0);
                        Some(ModelInfo {
                            id: name.clone(),
                            name,
                            provider: ProviderType::Ollama,
                            context_length: 128_000, // Most modern Ollama models
                            max_output_tokens: None,
                            supports_tools: true,
                            supports_vision: false,
                            supports_audio: false,
                            supports_json_mode: true,
                            input_cost_per_million: Some(0.0),
                            output_cost_per_million: Some(0.0),
                            capabilities: vec![
                                "chat".to_string(),
                                "local".to_string(),
                                format!("size:{}", size),
                            ],
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
            .find(|m| m.id == model || m.id.starts_with(&format!("{}:", model)))
            .ok_or_else(|| ProviderError::ModelNotFound {
                model: model.to_string(),
            })
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let url = format!("{}/api/chat", self.base_url());

        let mut body = json!({
            "model": request.model.model,
            "messages": self.build_messages(&request.messages),
            "stream": false,
        });

        if let Some(temp) = request.temperature {
            body["options"] = json!({"temperature": temp});
        }
        if let Some(ref tools) = request.tools {
            if !tools.is_empty() {
                body["tools"] = json!(self.build_tools(tools));
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
            return Err(ProviderError::Unknown {
                message: format!("API error {}: {}", status, text),
            });
        }

        let json: JsonValue = response.json().await.map_err(|e| ProviderError::Unknown {
            message: e.to_string(),
        })?;

        self.parse_response(&json)
    }

    async fn chat_stream(&self, request: ChatRequest) -> Result<ChatStream, ProviderError> {
        let url = format!("{}/api/chat", self.base_url());

        let mut body = json!({
            "model": request.model.model,
            "messages": self.build_messages(&request.messages),
            "stream": true,
        });

        if let Some(temp) = request.temperature {
            body["options"] = json!({"temperature": temp});
        }
        if let Some(ref tools) = request.tools {
            if !tools.is_empty() {
                body["tools"] = json!(self.build_tools(tools));
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
            return Err(ProviderError::Unknown {
                message: format!("API error {}: {}", status, text),
            });
        }

        use futures::StreamExt;
        let byte_stream = response.bytes_stream();

        // Ollama streams NDJSON (one JSON object per line), not SSE
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

                    if line.is_empty() {
                        continue;
                    }

                    if let Ok(json) = serde_json::from_str::<JsonValue>(&line) {
                        let done = json["done"].as_bool().unwrap_or(false);

                        let stream_delta = if let Some(content) = json["message"]["content"].as_str() {
                            if content.is_empty() && !done {
                                continue;
                            }
                            StreamDelta::Text(content.to_string())
                        } else {
                            StreamDelta::Text(String::new())
                        };

                        let finish_reason = if done { Some(FinishReason::Stop) } else { None };

                        let usage = if done {
                            Some(TokenUsage {
                                prompt_tokens: json["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
                                completion_tokens: json["eval_count"].as_u64().unwrap_or(0) as u32,
                                total_tokens: (json["prompt_eval_count"].as_u64().unwrap_or(0)
                                    + json["eval_count"].as_u64().unwrap_or(0)) as u32,
                                cached_tokens: None,
                            })
                        } else {
                            None
                        };

                        yield Ok(StreamChunk {
                            id: None,
                            delta: stream_delta,
                            finish_reason,
                            usage,
                        });

                        if done {
                            return;
                        }
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        let url = format!("{}/api/embed", self.base_url());

        let body = json!({
            "model": request.model.model,
            "input": request.input,
        });

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

        let embeddings = if let Some(arr) = json["embeddings"].as_array() {
            arr.iter()
                .map(|emb| {
                    emb.as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_f64().map(|f| f as f32))
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        } else if let Some(arr) = json["embedding"].as_array() {
            vec![arr
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect()]
        } else {
            return Err(ProviderError::Unknown {
                message: "Invalid embedding response".to_string(),
            });
        };

        Ok(EmbeddingResponse {
            embeddings,
            model: request.model.model.clone(),
            usage: TokenUsage::default(),
        })
    }

    fn tool_format(&self) -> ToolFormat {
        ToolFormat::OpenAI
    }

    fn supports(&self, capability: &str) -> bool {
        match capability {
            "chat" | "tools" | "embeddings" | "json_mode" => true,
            "vision" | "audio" => false,
            "streaming" => true,
            _ => false,
        }
    }

    fn auth_headers(&self) -> HashMap<String, String> {
        HashMap::new() // Ollama doesn't need auth
    }

    fn transform_request(&self, request: &ChatRequest) -> Result<JsonValue, ProviderError> {
        let mut body = json!({
            "model": request.model.model,
            "messages": self.build_messages(&request.messages),
            "stream": false,
        });
        if let Some(ref tools) = request.tools {
            if !tools.is_empty() {
                body["tools"] = json!(self.build_tools(tools));
            }
        }
        Ok(body)
    }

    fn parse_response(&self, response: &JsonValue) -> Result<ChatResponse, ProviderError> {
        let message = &response["message"];
        let content = message["content"].as_str().unwrap_or("").to_string();

        // Parse tool calls if present
        let tool_calls = message
            .get("tool_calls")
            .and_then(|tc| tc.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        Some(ToolCall {
                            id: format!("call_{}", uuid::Uuid::new_v4()),
                            name: tc["function"]["name"].as_str()?.to_string(),
                            arguments: tc["function"]["arguments"].clone(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|v: &Vec<ToolCall>| !v.is_empty());

        let usage = TokenUsage {
            prompt_tokens: response["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
            completion_tokens: response["eval_count"].as_u64().unwrap_or(0) as u32,
            total_tokens: (response["prompt_eval_count"].as_u64().unwrap_or(0)
                + response["eval_count"].as_u64().unwrap_or(0)) as u32,
            cached_tokens: None,
        };

        Ok(ChatResponse {
            id: "".to_string(),
            model: response["model"].as_str().unwrap_or("").to_string(),
            message: Message::assistant(content),
            usage,
            finish_reason: FinishReason::Stop,
            tool_calls,
            metadata: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_creation() {
        let config = ProviderConfig::new(ProviderType::Ollama);
        let provider = OllamaProvider::new(config);
        assert_eq!(provider.provider_type(), ProviderType::Ollama);
    }

    #[test]
    fn test_default_base_url() {
        let config = ProviderConfig::new(ProviderType::Ollama);
        let provider = OllamaProvider::new(config);
        assert_eq!(provider.base_url(), "http://localhost:11434");
    }
}
