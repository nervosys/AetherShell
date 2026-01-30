//! Ollama Provider Implementation
//!
//! Local LLM provider using Ollama's API

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
        self.config.base_url.clone()
            .unwrap_or_else(|| "http://localhost:11434".to_string())
    }

    fn build_messages(&self, messages: &[Message]) -> Vec<JsonValue> {
        messages.iter().map(|msg| {
            let mut msg_obj = json!({
                "role": msg.role,
                "content": msg.text(),
            });

            // Handle images for vision models
            let images: Vec<String> = msg.content.iter().filter_map(|part| {
                if let ContentPart::Image { data, .. } = part {
                    data.clone()
                } else {
                    None
                }
            }).collect();

            if !images.is_empty() {
                msg_obj["images"] = json!(images);
            }

            msg_obj
        }).collect()
    }

    fn build_tools(&self, tools: &[ToolSchema]) -> Vec<JsonValue> {
        tools.iter().map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters
                }
            })
        }).collect()
    }
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    fn name(&self) -> &str {
        "Ollama"
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
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

        if !request.tools.is_empty() {
            body["tools"] = json!(self.build_tools(&request.tools));
        }

        let response = self.client.post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network { message: e.to_string() })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message: text });
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

    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        let url = format!("{}/api/embed", self.base_url());

        // Ollama expects a single prompt for embedding
        let prompt = request.input.join(" ");

        let body = json!({
            "model": request.model.model,
            "input": prompt,
        });

        let response = self.client.post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network { message: e.to_string() })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message: text });
        }

        let json: JsonValue = response.json().await
            .map_err(|e| ProviderError::Unknown { message: e.to_string() })?;

        let embeddings = json["embeddings"].as_array()
            .or_else(|| json["embedding"].as_array().map(|arr| {
                // Single embedding case - wrap in array
                vec![json!(arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>())].leak()
            }))
            .ok_or_else(|| ProviderError::Unknown { message: "Invalid response".to_string() })?
            .iter()
            .map(|emb| {
                emb.as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
                    .unwrap_or_default()
            })
            .collect();

        Ok(EmbeddingResponse {
            embeddings,
            model: request.model.model.clone(),
            usage: None,
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let url = format!("{}/api/tags", self.base_url());

        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| ProviderError::Network { message: e.to_string() })?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let json: JsonValue = response.json().await
            .map_err(|e| ProviderError::Unknown { message: e.to_string() })?;

        let models = json["models"].as_array()
            .map(|arr| {
                arr.iter().filter_map(|m| {
                    let name = m["name"].as_str()?.to_string();
                    Some(ModelInfo {
                        id: name.clone(),
                        name,
                        provider: ProviderType::Ollama,
                        context_window: None,
                        max_output: None,
                        supports_tools: Some(true),
                        supports_vision: None, // Depends on model
                        supports_streaming: Some(true),
                    })
                }).collect()
            })
            .unwrap_or_default();

        Ok(models)
    }

    fn format_tools(&self, tools: &[ToolSchema]) -> JsonValue {
        json!(self.build_tools(tools))
    }

    fn parse_response(&self, response: &JsonValue) -> Result<ChatResponse, ProviderError> {
        let message = &response["message"];
        let content = message["content"].as_str().map(|s| s.to_string());

        // Parse tool calls if present
        let tool_calls = message.get("tool_calls")
            .and_then(|tc| tc.as_array())
            .map(|arr| {
                arr.iter().filter_map(|tc| {
                    Some(ToolCall {
                        id: format!("call_{}", uuid::Uuid::new_v4()),
                        name: tc["function"]["name"].as_str()?.to_string(),
                        arguments: tc["function"]["arguments"].to_string(),
                    })
                }).collect::<Vec<_>>()
            })
            .filter(|v: &Vec<ToolCall>| !v.is_empty());

        let usage = Some(Usage {
            prompt_tokens: response["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
            completion_tokens: response["eval_count"].as_u64().unwrap_or(0) as u32,
            total_tokens: (response["prompt_eval_count"].as_u64().unwrap_or(0) 
                + response["eval_count"].as_u64().unwrap_or(0)) as u32,
        });

        Ok(ChatResponse {
            id: "".to_string(),
            model: response["model"].as_str().unwrap_or("").to_string(),
            content,
            tool_calls,
            finish_reason: Some("stop".to_string()),
            usage,
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
        assert_eq!(provider.name(), "Ollama");
    }

    #[test]
    fn test_default_base_url() {
        let config = ProviderConfig::new(ProviderType::Ollama);
        let provider = OllamaProvider::new(config);
        assert_eq!(provider.base_url(), "http://localhost:11434");
    }
}
