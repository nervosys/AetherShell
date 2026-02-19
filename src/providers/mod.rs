//! Universal LLM Provider Interface
//!
//! AetherShell's provider-agnostic abstraction layer that enables any AI model
//! to interact with the operating system through a standardized interface.
//!
//! ## Supported Providers
//!
//! | Provider | URI Scheme | Example |
//! |----------|------------|---------|
//! | OpenAI | `openai:` | `openai:gpt-4o` |
//! | Anthropic | `anthropic:` | `anthropic:claude-3-5-sonnet` |
//! | Google | `google:` | `google:gemini-pro` |
//! | Azure OpenAI | `azure:` | `azure:gpt-4/deployment-name` |
//! | AWS Bedrock | `bedrock:` | `bedrock:anthropic.claude-v2` |
//! | Ollama | `ollama:` | `ollama:llama3` |
//! | Together AI | `together:` | `together:meta-llama/Llama-3-70b` |
//! | Groq | `groq:` | `groq:mixtral-8x7b` |
//! | Mistral | `mistral:` | `mistral:mistral-large` |
//! | Cohere | `cohere:` | `cohere:command-r-plus` |
//! | Perplexity | `perplexity:` | `perplexity:llama-3.1-sonar` |
//! | Fireworks | `fireworks:` | `fireworks:llama-v3-70b` |
//! | DeepSeek | `deepseek:` | `deepseek:deepseek-chat` |
//! | xAI | `xai:` | `xai:grok-beta` |
//! | OpenRouter | `openrouter:` | `openrouter:anthropic/claude-3` |
//! | vLLM | `vllm:` | `vllm:meta-llama/Llama-3` |
//! | TGI | `tgi:` | `tgi:mixtral` |
//! | llama.cpp | `llamacpp:` | `llamacpp:model.gguf` |
//! | Local/Custom | `local:` | `local:http://localhost:8080` |

// NOTE: bridge.rs and impls/ exist but need trait alignment before compilation.
// The universal provider routing uses ai.rs's openai_compat backend for now.
// pub mod bridge;
// pub mod impls;
pub mod ontology;
pub mod platform;
pub mod registry;
pub mod schema;
pub mod tools;
pub mod traits;

pub use ontology::*;
pub use platform::{
    ExecutionResult, PlatformCapabilities, PlatformExecutor, PlatformRegistry, PLATFORM_EXECUTOR,
    PLATFORM_REGISTRY,
};
pub use registry::*;
pub use schema::*;
pub use tools::{AetherTool, AetherToolRegistry, ToolResult, AETHER_TOOLS};
pub use traits::*;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Model URI for provider-agnostic model selection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelUri {
    pub provider: ProviderType,
    pub model: String,
    pub deployment: Option<String>,
    #[serde(skip)]
    pub options: HashMap<String, String>,
}

impl ModelUri {
    pub fn parse(uri: &str) -> Result<Self> {
        let (scheme, rest) = uri
            .split_once(':')
            .ok_or_else(|| anyhow!("Invalid model URI: missing scheme"))?;

        let provider = ProviderType::from_scheme(scheme)?;

        let (model, deployment) = if rest.contains('/') && provider == ProviderType::Azure {
            let parts: Vec<&str> = rest.splitn(2, '/').collect();
            (parts[0].to_string(), Some(parts[1].to_string()))
        } else {
            (rest.to_string(), None)
        };

        Ok(Self {
            provider,
            model,
            deployment,
            options: HashMap::new(),
        })
    }

    pub fn to_string(&self) -> String {
        let base = format!("{}:{}", self.provider.scheme(), self.model);
        if let Some(ref deployment) = self.deployment {
            format!("{}/{}", base, deployment)
        } else {
            base
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    OpenAI,
    Anthropic,
    Google,
    Azure,
    Bedrock,
    Ollama,
    Together,
    Groq,
    Mistral,
    Cohere,
    Perplexity,
    Fireworks,
    DeepSeek,
    XAI,
    OpenRouter,
    VLLM,
    TGI,
    LlamaCpp,
    Local,
}

impl ProviderType {
    pub fn scheme(&self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
            Self::Azure => "azure",
            Self::Bedrock => "bedrock",
            Self::Ollama => "ollama",
            Self::Together => "together",
            Self::Groq => "groq",
            Self::Mistral => "mistral",
            Self::Cohere => "cohere",
            Self::Perplexity => "perplexity",
            Self::Fireworks => "fireworks",
            Self::DeepSeek => "deepseek",
            Self::XAI => "xai",
            Self::OpenRouter => "openrouter",
            Self::VLLM => "vllm",
            Self::TGI => "tgi",
            Self::LlamaCpp => "llamacpp",
            Self::Local => "local",
        }
    }

    pub fn from_scheme(scheme: &str) -> Result<Self> {
        match scheme.to_lowercase().as_str() {
            "openai" => Ok(Self::OpenAI),
            "anthropic" | "claude" => Ok(Self::Anthropic),
            "google" | "gemini" => Ok(Self::Google),
            "azure" | "azure-openai" => Ok(Self::Azure),
            "bedrock" | "aws" => Ok(Self::Bedrock),
            "ollama" => Ok(Self::Ollama),
            "together" => Ok(Self::Together),
            "groq" => Ok(Self::Groq),
            "mistral" => Ok(Self::Mistral),
            "cohere" => Ok(Self::Cohere),
            "perplexity" | "pplx" => Ok(Self::Perplexity),
            "fireworks" => Ok(Self::Fireworks),
            "deepseek" => Ok(Self::DeepSeek),
            "xai" | "grok" => Ok(Self::XAI),
            "openrouter" | "or" => Ok(Self::OpenRouter),
            "vllm" => Ok(Self::VLLM),
            "tgi" => Ok(Self::TGI),
            "llamacpp" | "llama-cpp" | "gguf" => Ok(Self::LlamaCpp),
            "local" | "custom" | "compat" => Ok(Self::Local),
            _ => Err(anyhow!("Unknown provider scheme: {}", scheme)),
        }
    }

    pub fn default_base_url(&self) -> &'static str {
        match self {
            Self::OpenAI => "https://api.openai.com/v1",
            Self::Anthropic => "https://api.anthropic.com/v1",
            Self::Google => "https://generativelanguage.googleapis.com/v1beta",
            Self::Azure => "https://{resource}.openai.azure.com/openai/deployments/{deployment}",
            Self::Bedrock => "https://bedrock-runtime.{region}.amazonaws.com",
            Self::Ollama => "http://localhost:11434/api",
            Self::Together => "https://api.together.xyz/v1",
            Self::Groq => "https://api.groq.com/openai/v1",
            Self::Mistral => "https://api.mistral.ai/v1",
            Self::Cohere => "https://api.cohere.ai/v1",
            Self::Perplexity => "https://api.perplexity.ai",
            Self::Fireworks => "https://api.fireworks.ai/inference/v1",
            Self::DeepSeek => "https://api.deepseek.com/v1",
            Self::XAI => "https://api.x.ai/v1",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::VLLM => "http://localhost:8000/v1",
            Self::TGI => "http://localhost:8080",
            Self::LlamaCpp => "http://localhost:8080",
            Self::Local => "http://localhost:8080/v1",
        }
    }

    pub fn api_key_env_var(&self) -> Option<&'static str> {
        match self {
            Self::OpenAI => Some("OPENAI_API_KEY"),
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
            Self::Google => Some("GOOGLE_API_KEY"),
            Self::Azure => Some("AZURE_OPENAI_API_KEY"),
            Self::Bedrock => Some("AWS_SECRET_ACCESS_KEY"),
            Self::Ollama => None,
            Self::Together => Some("TOGETHER_API_KEY"),
            Self::Groq => Some("GROQ_API_KEY"),
            Self::Mistral => Some("MISTRAL_API_KEY"),
            Self::Cohere => Some("COHERE_API_KEY"),
            Self::Perplexity => Some("PERPLEXITY_API_KEY"),
            Self::Fireworks => Some("FIREWORKS_API_KEY"),
            Self::DeepSeek => Some("DEEPSEEK_API_KEY"),
            Self::XAI => Some("XAI_API_KEY"),
            Self::OpenRouter => Some("OPENROUTER_API_KEY"),
            Self::VLLM => None,
            Self::TGI => None,
            Self::LlamaCpp => None,
            Self::Local => Some("LOCAL_API_KEY"),
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::OpenAI,
            Self::Anthropic,
            Self::Google,
            Self::Azure,
            Self::Bedrock,
            Self::Ollama,
            Self::Together,
            Self::Groq,
            Self::Mistral,
            Self::Cohere,
            Self::Perplexity,
            Self::Fireworks,
            Self::DeepSeek,
            Self::XAI,
            Self::OpenRouter,
            Self::VLLM,
            Self::TGI,
            Self::LlamaCpp,
            Self::Local,
        ]
    }

    pub fn is_openai_compatible(&self) -> bool {
        matches!(
            self,
            Self::OpenAI
                | Self::Azure
                | Self::Together
                | Self::Groq
                | Self::Perplexity
                | Self::Fireworks
                | Self::DeepSeek
                | Self::XAI
                | Self::OpenRouter
                | Self::VLLM
                | Self::Local
        )
    }

    pub fn supports_tools(&self) -> bool {
        matches!(
            self,
            Self::OpenAI
                | Self::Anthropic
                | Self::Google
                | Self::Azure
                | Self::Bedrock
                | Self::Together
                | Self::Groq
                | Self::Mistral
                | Self::Cohere
                | Self::Fireworks
                | Self::DeepSeek
                | Self::XAI
                | Self::OpenRouter
        )
    }

    pub fn supports_vision(&self) -> bool {
        matches!(
            self,
            Self::OpenAI
                | Self::Anthropic
                | Self::Google
                | Self::Azure
                | Self::Together
                | Self::Groq
                | Self::Fireworks
                | Self::OpenRouter
        )
    }

    pub fn supports_streaming(&self) -> bool {
        !matches!(self, Self::Bedrock)
    }

    pub fn tool_format(&self) -> ToolFormat {
        match self {
            Self::Anthropic => ToolFormat::Anthropic,
            Self::Google => ToolFormat::Google,
            Self::Cohere => ToolFormat::Cohere,
            Self::Mistral => ToolFormat::Mistral,
            _ => ToolFormat::OpenAI,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolFormat {
    OpenAI,
    Anthropic,
    Google,
    Cohere,
    Mistral,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider: ProviderType,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub organization: Option<String>,
    pub project: Option<String>,
    pub default_model: Option<String>,
    pub timeout_secs: Option<u64>,
    pub max_retries: Option<u32>,
    pub extra: HashMap<String, String>,
}

impl ProviderConfig {
    pub fn new(provider: ProviderType) -> Self {
        Self {
            provider,
            api_key: None,
            base_url: None,
            organization: None,
            project: None,
            default_model: None,
            timeout_secs: Some(120),
            max_retries: Some(3),
            extra: HashMap::new(),
        }
    }

    pub fn from_env(provider: ProviderType) -> Self {
        let api_key = provider
            .api_key_env_var()
            .and_then(|var| std::env::var(var).ok());
        Self {
            provider,
            api_key,
            base_url: None,
            organization: None,
            project: None,
            default_model: None,
            timeout_secs: Some(120),
            max_retries: Some(3),
            extra: HashMap::new(),
        }
    }

    pub fn with_env_key(mut self) -> Self {
        if let Some(var) = self.provider.api_key_env_var() {
            self.api_key = std::env::var(var).ok();
        }
        self
    }

    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = Some(model.into());
        self
    }

    pub fn effective_base_url(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| self.provider.default_base_url().to_string())
    }

    pub fn model_uri(&self) -> ModelUri {
        let model = self
            .default_model
            .clone()
            .unwrap_or_else(|| "default".to_string());
        ModelUri {
            provider: self.provider,
            model,
            deployment: None,
            options: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_uri_parse() {
        let uri = ModelUri::parse("openai:gpt-4o").unwrap();
        assert_eq!(uri.provider, ProviderType::OpenAI);
        assert_eq!(uri.model, "gpt-4o");
        assert!(uri.deployment.is_none());

        let uri = ModelUri::parse("azure:gpt-4/my-deployment").unwrap();
        assert_eq!(uri.provider, ProviderType::Azure);
        assert_eq!(uri.model, "gpt-4");
        assert_eq!(uri.deployment, Some("my-deployment".to_string()));
    }

    #[test]
    fn test_provider_schemes() {
        assert_eq!(
            ProviderType::from_scheme("openai").unwrap(),
            ProviderType::OpenAI
        );
        assert_eq!(
            ProviderType::from_scheme("claude").unwrap(),
            ProviderType::Anthropic
        );
        assert!(ProviderType::from_scheme("unknown").is_err());
    }

    #[test]
    fn test_provider_all() {
        let all = ProviderType::all();
        assert_eq!(all.len(), 19);
    }
}
