//! Secure configuration management with memory sanitization
//!
//! This module provides secure handling of sensitive configuration data:
//! - API keys wrapped in Secret<String> (auto-zeroized on drop)
//! - Zeroizing temporary key usage
//! - OS credential store integration (keyring)
//!
//! Security Features:
//! - CWE-316: Cleartext Storage in Memory (MITIGATED)
//! - CWE-526: Cleartext Storage of Sensitive Information (MITIGATED)
//! - Memory zeroing on drop
//! - No key exposure in debug output
//! - No key leaks in error messages

use anyhow::{anyhow, Context, Result};
use secrecy::{ExposeSecret, Secret};
use std::fmt;
use zeroize::Zeroizing;

/// Secure API configuration with memory sanitization
#[derive(Clone)]
pub struct SecureApiConfig {
    /// API key - automatically zeroized on drop
    api_key: Option<Secret<String>>,
    /// API endpoint (not sensitive)
    pub endpoint: String,
    /// Model identifier (not sensitive)
    pub model: String,
    /// Provider name (not sensitive)
    pub provider: String,
}

impl SecureApiConfig {
    /// Create new secure config with API key
    pub fn new(api_key: String, endpoint: String, model: String, provider: String) -> Self {
        Self {
            api_key: Some(Secret::new(api_key)),
            endpoint,
            model,
            provider,
        }
    }

    /// Create config without API key (for providers that don't need it)
    pub fn without_key(endpoint: String, model: String, provider: String) -> Self {
        Self {
            api_key: None,
            endpoint,
            model,
            provider,
        }
    }

    /// Check if API key is present
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// Get API key for use in request headers
    ///
    /// SECURITY: This exposes the secret temporarily. Use immediately and don't store.
    pub fn get_api_key(&self) -> Option<&str> {
        self.api_key.as_ref().map(|s| s.expose_secret().as_str())
    }

    /// Create authorization header value
    ///
    /// Returns: "Bearer <api_key>" wrapped in Zeroizing
    pub fn create_auth_header(&self) -> Option<Zeroizing<String>> {
        self.api_key
            .as_ref()
            .map(|key| Zeroizing::new(format!("Bearer {}", key.expose_secret())))
    }

    /// Load API key from environment variable (legacy support)
    ///
    /// SECURITY: This retrieves from env vars which are visible to all processes.
    /// Prefer using `from_keyring()` for production.
    pub fn from_env(
        env_var: &str,
        endpoint: String,
        model: String,
        provider: String,
    ) -> Result<Self> {
        let api_key = std::env::var(env_var).with_context(|| {
            format!(
                "{} environment variable not set.\n\
                 For secure storage, use: ae keys store {} <your-key>",
                env_var, provider
            )
        })?;

        eprintln!(
            "[SECURITY WARNING] API key loaded from environment variable '{}'.\n\
             Environment variables are visible to all processes.\n\
             Recommend migrating to OS credential store with: ae keys store {} <key>",
            env_var, provider
        );

        Ok(Self::new(api_key, endpoint, model, provider))
    }

    /// Load API key from OS credential store (recommended)
    ///
    /// Uses OS-specific secure storage:
    /// - Windows: Credential Manager
    /// - macOS: Keychain
    /// - Linux: Secret Service API (libsecret)
    pub fn from_keyring(
        service: &str,
        endpoint: String,
        model: String,
        provider: String,
    ) -> Result<Self> {
        use keyring::Entry;

        let entry =
            Entry::new(service, "aethershell").context("Failed to access OS credential store")?;

        let api_key = entry.get_password().with_context(|| {
            format!(
                "API key not found in credential store for service '{}'.\n\
                     Store it with: ae keys store {} <your-key>",
                service, provider
            )
        })?;

        Ok(Self::new(api_key, endpoint, model, provider))
    }

    /// Try keyring first, fall back to environment variable
    pub fn from_keyring_or_env(
        service: &str,
        env_var: &str,
        endpoint: String,
        model: String,
        provider: String,
    ) -> Result<Self> {
        match Self::from_keyring(service, endpoint.clone(), model.clone(), provider.clone()) {
            Ok(config) => {
                eprintln!("[SECURITY] Using API key from OS credential store");
                Ok(config)
            }
            Err(_) => {
                eprintln!(
                    "[SECURITY] OS credential store not available, falling back to environment variable"
                );
                Self::from_env(env_var, endpoint, model, provider)
            }
        }
    }

    /// Store API key in OS credential store
    pub fn store_in_keyring(service: &str, api_key: &str) -> Result<()> {
        use keyring::Entry;

        let entry =
            Entry::new(service, "aethershell").context("Failed to access OS credential store")?;

        entry
            .set_password(api_key)
            .context("Failed to store API key in credential store")?;

        eprintln!(
            "[SECURITY] API key securely stored in OS credential store for service '{}'",
            service
        );

        Ok(())
    }

    /// Delete API key from OS credential store
    pub fn delete_from_keyring(service: &str) -> Result<()> {
        use keyring::Entry;

        let entry =
            Entry::new(service, "aethershell").context("Failed to access OS credential store")?;

        entry
            .delete_password()
            .context("Failed to delete API key from credential store")?;

        eprintln!(
            "[SECURITY] API key deleted from OS credential store for service '{}'",
            service
        );

        Ok(())
    }

    /// Validate API key format (without exposing the key)
    pub fn validate_format(&self) -> Result<()> {
        if let Some(key) = &self.api_key {
            let key_str = key.expose_secret();

            if key_str.is_empty() {
                return Err(anyhow!("API key is empty"));
            }

            if key_str.contains('\0') {
                return Err(anyhow!("API key contains null byte"));
            }

            // Provider-specific validation
            match self.provider.as_str() {
                "openai" => {
                    if !key_str.starts_with("sk-") && !key_str.starts_with("sk-proj-") {
                        return Err(anyhow!(
                            "OpenAI API key should start with 'sk-' or 'sk-proj-'"
                        ));
                    }
                    if key_str.len() < 20 || key_str.len() > 200 {
                        return Err(anyhow!("OpenAI API key length is suspicious"));
                    }
                }
                "anthropic" => {
                    if !key_str.starts_with("sk-ant-") {
                        return Err(anyhow!("Anthropic API key should start with 'sk-ant-'"));
                    }
                }
                _ => {
                    // Generic validation
                    if key_str.len() < 10 {
                        return Err(anyhow!("API key is too short"));
                    }
                    if key_str.len() > 500 {
                        return Err(anyhow!("API key is too long"));
                    }
                }
            }

            Ok(())
        } else {
            Err(anyhow!("No API key configured"))
        }
    }
}

impl fmt::Debug for SecureApiConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecureApiConfig")
            .field("api_key", &"<REDACTED>")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("provider", &self.provider)
            .finish()
    }
}

impl Drop for SecureApiConfig {
    fn drop(&mut self) {
        // Explicit drop - Secret<String> already zeroizes on drop
        // This ensures the order of operations
        self.api_key = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_config_creation() {
        let config = SecureApiConfig::new(
            "sk-test123".to_string(),
            "https://api.openai.com".to_string(),
            "gpt-4".to_string(),
            "openai".to_string(),
        );

        assert!(config.has_api_key());
        assert_eq!(config.endpoint, "https://api.openai.com");
        assert_eq!(config.model, "gpt-4");
    }

    #[test]
    fn test_no_key_exposure_in_debug() {
        let config = SecureApiConfig::new(
            "sk-secret123".to_string(),
            "https://api.openai.com".to_string(),
            "gpt-4".to_string(),
            "openai".to_string(),
        );

        let debug_output = format!("{:?}", config);
        assert!(!debug_output.contains("sk-secret"));
        assert!(debug_output.contains("<REDACTED>"));
    }

    #[test]
    fn test_auth_header_creation() {
        let config = SecureApiConfig::new(
            "sk-test123".to_string(),
            "https://api.openai.com".to_string(),
            "gpt-4".to_string(),
            "openai".to_string(),
        );

        let header = config.create_auth_header().unwrap();
        assert_eq!(*header, "Bearer sk-test123");

        // Header is zeroized when dropped
        drop(header);
    }

    #[test]
    fn test_openai_key_validation() {
        let config = SecureApiConfig::new(
            "sk-proj-test123456789012345".to_string(),
            "https://api.openai.com".to_string(),
            "gpt-4".to_string(),
            "openai".to_string(),
        );

        assert!(config.validate_format().is_ok());
    }

    #[test]
    fn test_invalid_openai_key() {
        let config = SecureApiConfig::new(
            "invalid-key".to_string(),
            "https://api.openai.com".to_string(),
            "gpt-4".to_string(),
            "openai".to_string(),
        );

        assert!(config.validate_format().is_err());
    }

    #[test]
    fn test_config_without_key() {
        let config = SecureApiConfig::without_key(
            "http://localhost:11434".to_string(),
            "llama3".to_string(),
            "ollama".to_string(),
        );

        assert!(!config.has_api_key());
        assert!(config.get_api_key().is_none());
    }
}
