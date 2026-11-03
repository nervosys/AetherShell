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

/// Create a provider instance from configuration.
///
/// Bedrock is refused: it needs AWS SigV4 request signing (access key id,
/// secret, region), which is not implemented. The OpenAI-style bearer request
/// this used to build sent the AWS secret access key as a token. The refusal is
/// returned before any provider or HTTP client exists.
pub fn create_provider(config: ProviderConfig) -> anyhow::Result<Arc<dyn LLMProvider>> {
    let provider: Arc<dyn LLMProvider> = match config.provider {
        ProviderType::OpenAI => Arc::new(OpenAIProvider::new(config)),
        ProviderType::Anthropic => Arc::new(AnthropicProvider::new(config)),
        ProviderType::Google => Arc::new(GoogleProvider::new(config)),
        ProviderType::Ollama => Arc::new(OllamaProvider::new(config)),
        // IronGate speaks the OpenAI wire format, so it needs no adapter of its
        // own — what makes it different is *where* it sends the request next,
        // which is the gateway's business rather than the client's.
        ProviderType::IronGate => Arc::new(OpenAIProvider::new(config)),
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
        // Providers using OpenAI-compatible APIs
        ProviderType::Bedrock => return Err(bedrock_refusal()),
        ProviderType::Mistral => Arc::new(OpenAIProvider::new(config)),
        ProviderType::Cohere => Arc::new(OpenAIProvider::new(config)),
        ProviderType::TGI => Arc::new(OpenAIProvider::new(config)),
        ProviderType::LlamaCpp => Arc::new(OpenAIProvider::new(config)),
    };
    Ok(provider)
}

/// The structured refusal for Bedrock (`E_UNIMPLEMENTED`). It names no credential.
fn bedrock_refusal() -> anyhow::Error {
    crate::safety::unimplemented(
        "ai",
        "Amazon Bedrock is not supported: it needs AWS SigV4 request signing, which IronShell does not implement; NOTHING WAS SENT",
        "use anthropic:, openai: or irongate: instead",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::bridge::UniversalBackend;
    use crate::safety::{code_of, ErrorCode};

    /// Fake value, planted only in this process's environment for the test.
    const PLANTED_SECRET: &str = "FAKE-AWS-SECRET-FOR-TEST-ONLY-0000";

    #[test]
    fn bedrock_scheme_still_parses_to_bedrock() {
        assert_eq!(
            ProviderType::from_scheme("bedrock").unwrap(),
            ProviderType::Bedrock
        );
    }

    #[test]
    fn bedrock_is_refused_with_unimplemented_and_no_provider_is_built() {
        let config = ProviderConfig::new(ProviderType::Bedrock);
        let err = match create_provider(config) {
            Ok(_) => panic!("Bedrock must not construct a provider"),
            Err(e) => e,
        };
        assert_eq!(code_of(&err), ErrorCode::Unimplemented);
        let text = format!("{err:#}");
        assert!(text.contains("SigV4"), "refusal should say why: {text}");
    }

    #[test]
    fn bedrock_never_reads_the_aws_secret_and_error_does_not_echo_it() {
        std::env::set_var("AWS_SECRET_ACCESS_KEY", PLANTED_SECRET);
        let config = ProviderConfig::from_env(ProviderType::Bedrock).with_env_key();
        std::env::remove_var("AWS_SECRET_ACCESS_KEY");

        assert_eq!(ProviderType::Bedrock.api_key_env_var(), None);
        assert!(
            config.api_key.is_none(),
            "the AWS secret must not become a bearer key"
        );

        let err = match create_provider(config) {
            Ok(_) => panic!("Bedrock must not construct a provider"),
            Err(e) => e,
        };
        let text = format!("{err:#}");
        assert!(
            !text.contains(PLANTED_SECRET),
            "error leaked the secret: {text}"
        );
    }

    #[test]
    fn bedrock_uri_is_refused_by_the_bridge_before_any_request() {
        // Refused at construction, so no chat or HTTP call is ever made.
        let err = match UniversalBackend::from_uri("bedrock:anthropic.claude-v2") {
            Ok(_) => panic!("a bedrock: URI must not produce a backend"),
            Err(e) => e,
        };
        assert_eq!(code_of(&err), ErrorCode::Unimplemented);
    }
}
