//! Provider Bridge - Connects new providers module with existing AI system
//!
//! This module provides compatibility between the new universal provider interface
//! and the existing `ai.rs` functionality, enabling gradual migration.

use crate::providers::{
    self, ChatRequest, ChatResponse, Message, ModelUri, ProviderConfig, ProviderType,
    create_provider, LLMProvider,
};
use crate::ai::{self, ChatMessage, ModelRef, Provider as LegacyProvider};
use anyhow::{anyhow, Result};
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Convert legacy Provider to new ProviderType
pub fn legacy_to_provider_type(legacy: &LegacyProvider) -> ProviderType {
    match legacy {
        LegacyProvider::OpenAI => ProviderType::OpenAI,
        LegacyProvider::Ollama => ProviderType::Ollama,
        LegacyProvider::OpenAICompat => ProviderType::Local,
        LegacyProvider::Tgi => ProviderType::TGI,
        LegacyProvider::VLlm => ProviderType::VLLM,
        LegacyProvider::LlamaCpp => ProviderType::LlamaCpp,
        LegacyProvider::Stub => ProviderType::Local,
    }
}

/// Convert new ProviderType to legacy Provider
pub fn provider_type_to_legacy(pt: ProviderType) -> LegacyProvider {
    match pt {
        ProviderType::OpenAI => LegacyProvider::OpenAI,
        ProviderType::Ollama => LegacyProvider::Ollama,
        ProviderType::TGI => LegacyProvider::Tgi,
        ProviderType::VLLM => LegacyProvider::VLlm,
        ProviderType::LlamaCpp => LegacyProvider::LlamaCpp,
        _ => LegacyProvider::OpenAICompat,
    }
}

/// Convert legacy ModelRef to new ModelUri
pub fn model_ref_to_uri(model_ref: &ModelRef) -> ModelUri {
    let provider = legacy_to_provider_type(&model_ref.provider);
    ModelUri {
        provider,
        model: model_ref.model.clone(),
        deployment: None,
        options: std::collections::HashMap::new(),
    }
}

/// Convert new ModelUri to legacy ModelRef
pub fn uri_to_model_ref(uri: &ModelUri) -> ModelRef {
    ModelRef {
        provider: provider_type_to_legacy(uri.provider),
        model: uri.model.clone(),
    }
}

/// Convert legacy ChatMessage to new Message
pub fn chat_message_to_message(cm: &ChatMessage) -> Message {
    Message::new(&cm.role, &cm.content)
}

/// Convert new Message to legacy ChatMessage
pub fn message_to_chat_message(msg: &Message) -> ChatMessage {
    ChatMessage {
        role: msg.role.clone(),
        content: msg.text(),
    }
}

/// Universal completion using the new provider system
/// 
/// This function uses the new provider infrastructure while maintaining
/// compatibility with existing code.
pub fn complete_with_provider(prompt: &str, model_uri: Option<&str>) -> Result<String> {
    let uri = if let Some(uri_str) = model_uri {
        ModelUri::parse(uri_str)?
    } else {
        // Fall back to environment-configured default
        let aether_ai = std::env::var("AETHER_AI").unwrap_or_else(|_| "openai".to_string());
        let model_ref = ai::parse_model_ref(&aether_ai);
        model_ref_to_uri(&model_ref)
    };

    let config = ProviderConfig::from_env(uri.provider);
    let provider = create_provider(config);

    // Create runtime and execute
    let rt = Runtime::new().map_err(|e| anyhow!("Failed to create async runtime: {}", e))?;
    
    let request = ChatRequest::simple(uri, prompt);
    let response = rt.block_on(provider.chat(request))
        .map_err(|e| anyhow!("Provider error: {}", e))?;

    response.content.ok_or_else(|| anyhow!("No content in response"))
}

/// Chat with messages using the new provider system
pub fn chat_with_provider(messages: &[ChatMessage], model_uri: Option<&str>) -> Result<String> {
    let uri = if let Some(uri_str) = model_uri {
        ModelUri::parse(uri_str)?
    } else {
        let aether_ai = std::env::var("AETHER_AI").unwrap_or_else(|_| "openai".to_string());
        let model_ref = ai::parse_model_ref(&aether_ai);
        model_ref_to_uri(&model_ref)
    };

    let config = ProviderConfig::from_env(uri.provider);
    let provider = create_provider(config);

    let msgs: Vec<Message> = messages.iter().map(chat_message_to_message).collect();
    let request = ChatRequest::new(uri, msgs);

    let rt = Runtime::new().map_err(|e| anyhow!("Failed to create async runtime: {}", e))?;
    let response = rt.block_on(provider.chat(request))
        .map_err(|e| anyhow!("Provider error: {}", e))?;

    response.content.ok_or_else(|| anyhow!("No content in response"))
}

/// Get available models from a provider
pub fn list_provider_models(provider_uri: &str) -> Result<Vec<String>> {
    let provider_type = ProviderType::from_scheme(provider_uri)?;
    let config = ProviderConfig::from_env(provider_type);
    let provider = create_provider(config);

    let rt = Runtime::new().map_err(|e| anyhow!("Failed to create async runtime: {}", e))?;
    let models = rt.block_on(provider.list_models())
        .map_err(|e| anyhow!("Provider error: {}", e))?;

    Ok(models.into_iter().map(|m| m.id).collect())
}

/// Create embeddings using the new provider system
pub fn embed_with_provider(texts: &[String], model_uri: Option<&str>) -> Result<Vec<Vec<f32>>> {
    use crate::providers::EmbeddingRequest;

    let uri = if let Some(uri_str) = model_uri {
        ModelUri::parse(uri_str)?
    } else {
        // Default to OpenAI embeddings
        ModelUri::parse("openai:text-embedding-3-small")?
    };

    let config = ProviderConfig::from_env(uri.provider);
    let provider = create_provider(config);

    let request = EmbeddingRequest {
        model: uri,
        input: texts.to_vec(),
        dimensions: None,
    };

    let rt = Runtime::new().map_err(|e| anyhow!("Failed to create async runtime: {}", e))?;
    let response = rt.block_on(provider.embed(request))
        .map_err(|e| anyhow!("Provider error: {}", e))?;

    Ok(response.embeddings)
}

/// Provider wrapper that implements the legacy LlmBackend trait
pub struct UniversalBackend {
    provider: Arc<dyn LLMProvider>,
    model_uri: ModelUri,
}

impl UniversalBackend {
    pub fn new(model_uri: ModelUri) -> Self {
        let config = ProviderConfig::from_env(model_uri.provider);
        let provider = create_provider(config);
        Self { provider, model_uri }
    }

    pub fn from_uri(uri_str: &str) -> Result<Self> {
        let model_uri = ModelUri::parse(uri_str)?;
        Ok(Self::new(model_uri))
    }
}

impl ai::LlmBackend for UniversalBackend {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String> {
        let msgs: Vec<Message> = messages.iter().map(chat_message_to_message).collect();
        let request = ChatRequest::new(self.model_uri.clone(), msgs);

        let rt = Runtime::new()?;
        let response = rt.block_on(self.provider.chat(request))
            .map_err(|e| anyhow!("{}", e))?;

        response.content.ok_or_else(|| anyhow!("No content"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_provider_conversion() {
        assert_eq!(legacy_to_provider_type(&LegacyProvider::OpenAI), ProviderType::OpenAI);
        assert_eq!(legacy_to_provider_type(&LegacyProvider::Ollama), ProviderType::Ollama);
    }

    #[test]
    fn test_model_ref_conversion() {
        let model_ref = ModelRef {
            provider: LegacyProvider::OpenAI,
            model: "gpt-4o".to_string(),
        };
        let uri = model_ref_to_uri(&model_ref);
        assert_eq!(uri.provider, ProviderType::OpenAI);
        assert_eq!(uri.model, "gpt-4o");
    }

    #[test]
    fn test_message_conversion() {
        let cm = ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        };
        let msg = chat_message_to_message(&cm);
        assert_eq!(msg.role, "user");
        assert_eq!(msg.text(), "Hello");
    }
}
