pub mod config;
pub mod converters;
pub mod downloader;
pub mod models;
pub mod providers;
pub mod server;
pub mod storage;

pub use config::*;
pub use converters::*;
pub use downloader::*;
pub use models::*;
pub use providers::*;
pub use server::*;
pub use storage::*;

use anyhow::Result;
use std::collections::HashMap;

/// Main API client for interacting with different AI model providers
pub struct AIModelAPI {
    providers: HashMap<String, Box<dyn ModelProvider>>,
    storage: ModelStorage,
    config: APIConfig,
}

impl AIModelAPI {
    pub fn new(config: APIConfig) -> Result<Self> {
        let storage = ModelStorage::new(&config.storage)?;
        let mut providers: HashMap<String, Box<dyn ModelProvider>> = HashMap::new();

        // Register default providers
        providers.insert("openai".to_string(), Box::new(OpenAIProvider::new()));
        providers.insert("anthropic".to_string(), Box::new(AnthropicProvider::new()));
        providers.insert("local".to_string(), Box::new(LocalProvider::new()));

        Ok(Self {
            providers,
            storage,
            config,
        })
    }

    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let mut all_models = Vec::new();

        for (provider_name, provider) in &self.providers {
            let models = provider.list_models().await?;
            for mut model in models {
                model.provider = provider_name.clone();
                all_models.push(model);
            }
        }

        // Add locally stored models
        all_models.extend(self.storage.list_local_models()?);

        Ok(all_models)
    }

    pub async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        let provider = self
            .providers
            .get(&request.provider)
            .ok_or_else(|| anyhow::anyhow!("Provider not found: {}", request.provider))?;

        provider.chat_completion(request).await
    }

    pub async fn embeddings(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let provider = self
            .providers
            .get(&request.provider)
            .ok_or_else(|| anyhow::anyhow!("Provider not found: {}", request.provider))?;

        provider.embeddings(request).await
    }

    /// Get the current API configuration
    pub fn config(&self) -> &APIConfig {
        &self.config
    }

    /// Get mutable reference to storage for management operations
    pub fn storage_mut(&mut self) -> &mut ModelStorage {
        &mut self.storage
    }
}
