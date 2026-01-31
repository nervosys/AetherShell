//! OpenAI Provider Implementation
//!
//! Supports OpenAI API and all OpenAI-compatible providers including:
//! Azure OpenAI, Together, Groq, Perplexity, Fireworks, DeepSeek, xAI, OpenRouter, vLLM

use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::pin::Pin;

use crate::providers::{
    ChatRequest, ChatResponse, ContentPart, EmbeddingRequest, EmbeddingResponse, LLMProvider,
    Message, ModelInfo, ModelUri, ProviderConfig, ProviderError, ProviderType, StreamChunk,
    ToolCall, ToolSchema,
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
                        ContentPart::Image { url, .. } => json!([
                            {"type": "image_url", "image_url": {"url": url}}
                        ]),
                        _ => json!(msg.text()),
                    }
                } else {
                    json!(msg
                        .content
                        .iter()
                        .map(|part| {
                            match part {
                                ContentPart::Text { text } => json!({"type": "text", "text": text}),
                                ContentPart::Image { url, .. } => json!({
                                    "type": "image_url",
                                    "image_url": {"url": url}
                                }),
                                _ => json!({"type": "text", "text": ""}),
                            }
                        })
                        .collect::<Vec<_>>())
                };

                let mut msg_obj = json!({
                    "role": msg.role,
                    "content": content,
                });

                if let Some(ref name) = msg.name {
                    msg_obj["name"] = json!(name);
                }

                if let Some(ref tool_calls) = msg.tool_calls {
                    msg_obj["tool_calls"] = json!(tool_calls
                        .iter()
                        .map(|tc| json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.arguments
                            }
                        }))
                        .collect::<Vec<_>>());
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

    fn parse_tool_calls(&self, tool_calls: &JsonValue) -> Vec<ToolCall> {
        tool_calls
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        Some(ToolCall {
                            id: tc["id"].as_str()?.to_string(),
                            name: tc["function"]["name"].as_str()?.to_string(),
                            arguments: tc["function"]["arguments"].as_str()?.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    fn name(&self) -> &str {
        match self.config.provider {
            ProviderType::OpenAI => "OpenAI",
            ProviderType::Azure => "Azure OpenAI",
            ProviderType::Together => "Together AI",
            ProviderType::Groq => "Groq",
            ProviderType::Perplexity => "Perplexity",
            ProviderType::Fireworks => "Fireworks AI",
            ProviderType::DeepSeek => "DeepSeek",
            ProviderType::XAI => "xAI",
            ProviderType::OpenRouter => "OpenRouter",
            ProviderType::VLLM => "vLLM",
            ProviderType::Local => "Local",
            _ => "OpenAI-Compatible",
        }
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
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
        if !request.tools.is_empty() {
            body["tools"] = json!(self.build_tools(&request.tools));
        }

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if let Some(key) = self.api_key() {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        // Provider-specific headers
        match self.config.provider {
            ProviderType::Anthropic => {
                req = req.header("x-api-key", self.api_key().unwrap_or(""));
                req = req.header("anthropic-version", "2023-06-01");
            }
            ProviderType::OpenRouter => {
                req = req.header("HTTP-Referer", "https://aethershell.dev");
                req = req.header("X-Title", "AetherShell");
            }
            _ => {}
        }

        let response = req
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
                401 => ProviderError::Authentication { message: text },
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
        request: ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, ProviderError>> + Send>>, ProviderError>
    {
        // For now, return an error - streaming requires more complex implementation
        Err(ProviderError::Unsupported {
            feature: "streaming".to_string(),
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
            usage: None,
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let url = format!("{}/models", self.base_url());

        let mut req = self.client.get(&url);

        if let Some(key) = self.api_key() {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req.send().await.map_err(|e| ProviderError::Network {
            message: e.to_string(),
        })?;

        if !response.status().is_success() {
            return Ok(vec![]); // Many providers don't support model listing
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
                            context_window: m["context_length"].as_u64().map(|n| n as u32),
                            max_output: None,
                            supports_tools: None,
                            supports_vision: None,
                            supports_streaming: Some(true),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }

    fn format_tools(&self, tools: &[ToolSchema]) -> JsonValue {
        json!(self.build_tools(tools))
    }

    fn parse_response(&self, response: &JsonValue) -> Result<ChatResponse, ProviderError> {
        let choice = response["choices"]
            .get(0)
            .ok_or_else(|| ProviderError::Unknown {
                message: "No choices in response".to_string(),
            })?;

        let message = &choice["message"];
        let content = message["content"].as_str().map(|s| s.to_string());

        let tool_calls = message
            .get("tool_calls")
            .filter(|tc| !tc.is_null())
            .map(|tc| self.parse_tool_calls(tc));

        let finish_reason = choice["finish_reason"].as_str().map(|s| s.to_string());

        let usage = response.get("usage").map(|u| crate::providers::Usage {
            prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as u32,
        });

        Ok(ChatResponse {
            id: response["id"].as_str().unwrap_or("").to_string(),
            model: response["model"].as_str().unwrap_or("").to_string(),
            content,
            tool_calls,
            finish_reason,
            usage,
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
        assert_eq!(provider.name(), "OpenAI");
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
