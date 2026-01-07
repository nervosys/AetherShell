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

        // Register LLM backends if enabled
        if config.providers.vllm.enabled {
            providers.insert(
                "vllm".to_string(),
                Box::new(VLLMProvider::with_endpoint(
                    config.providers.vllm.endpoint.clone(),
                )),
            );
        }
        if config.providers.tensorrt_llm.enabled {
            providers.insert(
                "tensorrt-llm".to_string(),
                Box::new(TensorRTLLMProvider::with_endpoint(
                    config.providers.tensorrt_llm.endpoint.clone(),
                )),
            );
        }
        if config.providers.sglang.enabled {
            providers.insert(
                "sglang".to_string(),
                Box::new(SGLangProvider::with_endpoint(
                    config.providers.sglang.endpoint.clone(),
                )),
            );
        }
        if config.providers.llama_cpp.enabled {
            providers.insert(
                "llama.cpp".to_string(),
                Box::new(LlamaCppProvider::with_endpoint(
                    config.providers.llama_cpp.endpoint.clone(),
                )),
            );
        }

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

    /// Auto-detect available LLM backends
    pub async fn detect_backends(&self) -> Result<Vec<BackendInfo>> {
        let endpoints = vec![
            ("vllm", "http://localhost:8000"),
            ("tensorrt-llm", "http://localhost:8001"),
            ("sglang", "http://localhost:30000"),
            ("llama.cpp", "http://localhost:8080"),
        ];

        let mut detected = Vec::new();
        // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
        let client = crate::security::create_secure_async_client()
            .unwrap_or_else(|_| reqwest::Client::new());

        for (name, endpoint) in endpoints {
            let mut is_available = false;
            let mut models = Vec::new();

            // Try standard /v1/models endpoint
            if let Ok(response) = client.get(&format!("{}/v1/models", endpoint)).send().await {
                if response.status().is_success() {
                    is_available = true;
                    if let Ok(models_response) = response.json::<serde_json::Value>().await {
                        if let Some(data) = models_response.get("data").and_then(|d| d.as_array()) {
                            models = data
                                .iter()
                                .filter_map(|m| m.get("id").and_then(|id| id.as_str()))
                                .map(|s| s.to_string())
                                .collect();
                        }
                    }
                }
            } else if name == "llama.cpp" {
                // Try llama.cpp specific health endpoint
                if let Ok(response) = client.get(&format!("{}/health", endpoint)).send().await {
                    if response.status().is_success() {
                        is_available = true;
                        if let Ok(health_info) = response.json::<serde_json::Value>().await {
                            if let Some(model_name) =
                                health_info.get("model_name").and_then(|v| v.as_str())
                            {
                                models.push(model_name.to_string());
                            }
                        }
                    }
                }
            }

            detected.push(BackendInfo {
                name: name.to_string(),
                endpoint: endpoint.to_string(),
                available: is_available,
                models,
                backend_type: match name {
                    "vllm" => BackendType::VLLM,
                    "tensorrt-llm" => BackendType::TensorRTLLM,
                    "sglang" => BackendType::SGLang,
                    "llama.cpp" => BackendType::LlamaCpp,
                    _ => BackendType::Unknown,
                },
            });
        }

        Ok(detected)
    }

    /// Register a dynamically detected backend
    pub fn register_backend(&mut self, backend_info: BackendInfo) -> Result<()> {
        if !backend_info.available {
            return Err(anyhow::anyhow!(
                "Backend {} is not available",
                backend_info.name
            ));
        }

        let provider: Box<dyn ModelProvider> = match backend_info.backend_type {
            BackendType::VLLM => Box::new(VLLMProvider::with_endpoint(backend_info.endpoint)),
            BackendType::TensorRTLLM => {
                Box::new(TensorRTLLMProvider::with_endpoint(backend_info.endpoint))
            }
            BackendType::SGLang => Box::new(SGLangProvider::with_endpoint(backend_info.endpoint)),
            BackendType::LlamaCpp => {
                Box::new(LlamaCppProvider::with_endpoint(backend_info.endpoint))
            }
            BackendType::Unknown => return Err(anyhow::anyhow!("Unknown backend type")),
        };

        self.providers.insert(backend_info.name, provider);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct BackendInfo {
    pub name: String,
    pub endpoint: String,
    pub available: bool,
    pub models: Vec<String>,
    pub backend_type: BackendType,
}

#[derive(Debug, Clone)]
pub enum BackendType {
    VLLM,
    TensorRTLLM,
    SGLang,
    LlamaCpp,
    Unknown,
}
