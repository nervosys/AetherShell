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
///
/// A base URL that still holds a `{...}` template (Azure's default is one) is
/// refused the same way, with `E_BAD_STATE`, before anything is constructed, so
/// the API key is never sent to a literal placeholder host.
pub fn create_provider(config: ProviderConfig) -> anyhow::Result<Arc<dyn LLMProvider>> {
    // Bedrock first: its own refusal names the real cause.
    if config.provider == ProviderType::Bedrock {
        return Err(bedrock_refusal());
    }
    if let Some(placeholder) = unfilled_placeholder(&config.effective_base_url()) {
        return Err(unfilled_base_url_refusal(config.provider, &placeholder));
    }
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

/// The first `{...}` template left in a base URL, or the lone brace that marks
/// one. `None` when the URL has no braces at all.
fn unfilled_placeholder(url: &str) -> Option<String> {
    let start = url.find(['{', '}'])?;
    let rest = &url[start..];
    let len = rest.find('}').map_or(rest.len(), |end| end + 1);
    Some(rest[..len].to_string())
}

/// The structured refusal for a base URL that still has a template in it
/// (`E_BAD_STATE`). It names the call that supplies the real URL and never the
/// key. For Azure it also says which variable the provider layer does not read.
fn unfilled_base_url_refusal(provider: ProviderType, placeholder: &str) -> anyhow::Error {
    let state = match provider {
        ProviderType::Azure => format!(
            "azure: base URL still has the unfilled placeholder {placeholder}; set the real endpoint with ProviderConfig::with_base_url (AZURE_OPENAI_ENDPOINT is read only by the default ai() path, not by the provider layer); NOTHING WAS SENT"
        ),
        other => format!(
            "{}: base URL still has the unfilled placeholder {placeholder}; set the real URL with ProviderConfig::with_base_url; NOTHING WAS SENT",
            other.scheme()
        ),
    };
    crate::safety::bad_state("ai", &state, "ProviderConfig::with_base_url")
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

    /// Fake value, planted only in this process's environment for the test.
    const PLANTED_AZURE_KEY: &str = "FAKE-AZURE-KEY-FOR-TEST-ONLY-0000";

    #[test]
    fn azure_without_a_real_endpoint_is_refused_and_the_key_is_not_echoed() {
        std::env::set_var("AZURE_OPENAI_API_KEY", PLANTED_AZURE_KEY);
        let config = ProviderConfig::from_env(ProviderType::Azure).with_env_key();
        std::env::remove_var("AZURE_OPENAI_API_KEY");
        assert_eq!(config.api_key.as_deref(), Some(PLANTED_AZURE_KEY));

        let err = match create_provider(config) {
            Ok(_) => panic!("azure with a placeholder URL must not construct a provider"),
            Err(e) => e,
        };
        assert_eq!(code_of(&err), ErrorCode::BadState);
        let text = format!("{err:#}");
        assert!(
            !text.contains(PLANTED_AZURE_KEY),
            "error leaked the key: {text}"
        );
        assert!(
            text.contains("azure"),
            "refusal should name the provider: {text}"
        );
        assert!(
            text.contains("with_base_url"),
            "refusal should name the setting: {text}"
        );
    }

    #[test]
    fn azure_uri_is_refused_by_the_bridge_before_any_request() {
        let err = match UniversalBackend::from_uri("azure:gpt-4o/my-deployment") {
            Ok(_) => panic!("an azure: URI with no endpoint must not produce a backend"),
            Err(e) => e,
        };
        assert_eq!(code_of(&err), ErrorCode::BadState);
    }

    #[test]
    fn azure_with_a_real_endpoint_builds_a_config_without_braces() {
        let config = ProviderConfig::new(ProviderType::Azure)
            .with_base_url("https://fake-resource.openai.azure.com/openai/deployments/fake")
            .with_api_key(PLANTED_AZURE_KEY);
        assert!(!config.effective_base_url().contains('{'));
        assert!(
            create_provider(config).is_ok(),
            "a real endpoint must build a provider"
        );
    }

    #[test]
    fn a_placeholder_override_is_refused_for_any_provider() {
        let config = ProviderConfig::new(ProviderType::OpenAI).with_base_url("https://{host}/v1");
        let err = match create_provider(config) {
            Ok(_) => panic!("a templated base URL must not construct a provider"),
            Err(e) => e,
        };
        assert_eq!(code_of(&err), ErrorCode::BadState);
    }

    #[test]
    fn a_normal_url_provider_is_unaffected() {
        let config = ProviderConfig::new(ProviderType::OpenAI);
        assert_eq!(config.effective_base_url(), "https://api.openai.com/v1");
        assert!(create_provider(config).is_ok());
    }
}
