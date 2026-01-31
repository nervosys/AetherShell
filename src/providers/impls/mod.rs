//! Concrete LLM Provider Implementations
//!
//! This module contains actual implementations of the `LLMProvider` trait
//! for each supported provider, handling API-specific details.

pub mod anthropic;
pub mod google;
pub mod ollama;
pub mod openai;

pub use anthropic::AnthropicProvider;
pub use google::GoogleProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;

use super::{LLMProvider, ProviderConfig, ProviderType};
use std::sync::Arc;

/// Create a provider instance from configuration
pub fn create_provider(config: ProviderConfig) -> Arc<dyn LLMProvider> {
    match config.provider {
        ProviderType::OpenAI => Arc::new(OpenAIProvider::new(config)),
        ProviderType::Anthropic => Arc::new(AnthropicProvider::new(config)),
        ProviderType::Google => Arc::new(GoogleProvider::new(config)),
        ProviderType::Ollama => Arc::new(OllamaProvider::new(config)),
        // OpenAI-compatible providers
        ProviderType::Azure
        | ProviderType::Together
        | ProviderType::Groq
        | ProviderType::Perplexity
        | ProviderType::Fireworks
        | ProviderType::DeepSeek
        | ProviderType::XAI
        | ProviderType::OpenRouter
        | ProviderType::VLLM
        | ProviderType::Local => Arc::new(OpenAIProvider::new(config)),
        // Providers needing specific implementations
        ProviderType::Bedrock => Arc::new(OpenAIProvider::new(config)), // TODO: AWS Bedrock
        ProviderType::Mistral => Arc::new(OpenAIProvider::new(config)), // Mistral uses OpenAI-compat
        ProviderType::Cohere => Arc::new(OpenAIProvider::new(config)),  // TODO: Cohere native
        ProviderType::TGI => Arc::new(OpenAIProvider::new(config)),     // TGI is OpenAI-compat
        ProviderType::LlamaCpp => Arc::new(OpenAIProvider::new(config)), // llama.cpp server
    }
}
