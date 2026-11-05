use crate::ai_api::storage::StorageConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Main configuration for the AI API system
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct APIConfig {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub providers: ProvidersConfig,
    pub security: SecurityConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub max_connections: usize,
    pub request_timeout_seconds: u64,
    pub enable_cors: bool,
    pub cors_origins: Vec<String>,
    pub enable_openapi: bool,
    pub openapi_path: String,
    /// Extra `Host` header values the server answers to, in addition to
    /// `localhost`, loopback addresses and the bind host. Anything else is
    /// refused, which is what stops a DNS-rebinding page from reaching the
    /// server. Empty by default.
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    pub openai: ProviderConfig,
    pub anthropic: ProviderConfig,
    pub local: LocalProviderConfig,
    pub ollama: LLMBackendConfig,
    pub lmstudio: LLMBackendConfig,
    pub vllm: LLMBackendConfig,
    pub tensorrt_llm: LLMBackendConfig,
    pub sglang: LLMBackendConfig,
    pub llama_cpp: LLMBackendConfig,
    pub custom: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub base_url: Option<String>,
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub rate_limit_requests_per_minute: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalProviderConfig {
    pub enabled: bool,
    pub inference_engine: InferenceEngine,
    pub max_models_loaded: usize,
    pub model_cache_size_gb: f64,
    pub gpu_layers: Option<u32>,
    pub context_size: u32,
    pub threads: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMBackendConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub auto_start: bool,
    pub start_command: Option<String>,
    pub stop_command: Option<String>,
    pub health_check_interval_seconds: u64,
    pub model_path: Option<String>,
    pub gpu_memory_fraction: Option<f32>,
    pub max_batch_size: Option<u32>,
    pub tensor_parallel_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InferenceEngine {
    LlamaCpp,
    VLLM,
    TensorRTLLM,
    SGLang,
    Candle,
    Onnx,
    TensorFlow,
    PyTorch,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    pub require_api_key: bool,
    pub api_keys: Vec<String>,
    pub rate_limiting: RateLimitConfig,
    pub enable_tls: bool,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
}

/// Per-client request limits, enforced by a fixed-window counter keyed on the
/// peer IP. Over the limit a request gets `429`. `requests_per_minute` and
/// `requests_per_hour` are enforced; a value of `0` leaves that window
/// unlimited. `burst_size`, `by_api_key` and `enabled = false` are the only
/// other settings; `burst_size` and `by_api_key` are not enforced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub burst_size: u32,
    pub by_ip: bool,
    pub by_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub format: LogFormat,
    pub output: LogOutput,
    pub log_requests: bool,
    pub log_responses: bool,
    pub log_errors: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    Json,
    Pretty,
    Compact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogOutput {
    Stdout,
    Stderr,
    File { path: String },
    Syslog,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            max_connections: 1000,
            request_timeout_seconds: 300,
            enable_cors: true,
            // SECURITY FIX (LOW-003): Default to localhost only for security
            // Users can override in config.toml to allow specific origins
            cors_origins: vec![
                "http://localhost:3000".to_string(),
                "http://127.0.0.1:3000".to_string(),
            ],
            enable_openapi: true,
            openapi_path: "/docs".to_string(),
            allowed_hosts: Vec::new(),
        }
    }
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            openai: ProviderConfig {
                enabled: true,
                api_key: None,
                api_key_env: Some("OPENAI_API_KEY".to_string()),
                base_url: Some("https://api.openai.com/v1".to_string()),
                timeout_seconds: 60,
                max_retries: 3,
                rate_limit_requests_per_minute: Some(60),
            },
            anthropic: ProviderConfig {
                enabled: true,
                api_key: None,
                api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
                base_url: Some("https://api.anthropic.com/v1".to_string()),
                timeout_seconds: 60,
                max_retries: 3,
                rate_limit_requests_per_minute: Some(60),
            },
            local: LocalProviderConfig {
                enabled: true,
                inference_engine: InferenceEngine::LlamaCpp,
                max_models_loaded: 3,
                model_cache_size_gb: 8.0,
                gpu_layers: None,
                context_size: 4096,
                threads: None,
            },
            ollama: LLMBackendConfig {
                enabled: true,
                endpoint: "http://localhost:11434".to_string(),
                api_key: None,
                api_key_env: None,
                timeout_seconds: 120,
                max_retries: 2,
                auto_start: false,
                start_command: Some("ollama serve".to_string()),
                stop_command: None,
                health_check_interval_seconds: 30,
                model_path: None,
                gpu_memory_fraction: None,
                max_batch_size: None,
                tensor_parallel_size: None,
            },
            lmstudio: LLMBackendConfig {
                enabled: false,
                endpoint: "http://localhost:1234".to_string(),
                api_key: None,
                api_key_env: None,
                timeout_seconds: 120,
                max_retries: 2,
                auto_start: false,
                start_command: None,
                stop_command: None,
                health_check_interval_seconds: 30,
                model_path: None,
                gpu_memory_fraction: None,
                max_batch_size: None,
                tensor_parallel_size: None,
            },
            vllm: LLMBackendConfig {
                enabled: false,
                endpoint: "http://localhost:8000".to_string(),
                api_key: None,
                api_key_env: Some("VLLM_API_KEY".to_string()),
                timeout_seconds: 60,
                max_retries: 3,
                auto_start: false,
                start_command: None,
                stop_command: None,
                health_check_interval_seconds: 30,
                model_path: None,
                gpu_memory_fraction: Some(0.9),
                max_batch_size: Some(256),
                tensor_parallel_size: Some(1),
            },
            tensorrt_llm: LLMBackendConfig {
                enabled: false,
                endpoint: "http://localhost:8001".to_string(),
                api_key: None,
                api_key_env: Some("TENSORRT_LLM_API_KEY".to_string()),
                timeout_seconds: 60,
                max_retries: 3,
                auto_start: false,
                start_command: None,
                stop_command: None,
                health_check_interval_seconds: 30,
                model_path: None,
                gpu_memory_fraction: Some(0.9),
                max_batch_size: Some(128),
                tensor_parallel_size: Some(1),
            },
            sglang: LLMBackendConfig {
                enabled: false,
                endpoint: "http://localhost:30000".to_string(),
                api_key: None,
                api_key_env: Some("SGLANG_API_KEY".to_string()),
                timeout_seconds: 60,
                max_retries: 3,
                auto_start: false,
                start_command: None,
                stop_command: None,
                health_check_interval_seconds: 30,
                model_path: None,
                gpu_memory_fraction: Some(0.8),
                max_batch_size: Some(512),
                tensor_parallel_size: Some(1),
            },
            llama_cpp: LLMBackendConfig {
                enabled: false,
                endpoint: "http://localhost:8080".to_string(),
                api_key: None,
                api_key_env: Some("LLAMACPP_API_KEY".to_string()),
                timeout_seconds: 60,
                max_retries: 3,
                auto_start: false,
                start_command: None,
                stop_command: None,
                health_check_interval_seconds: 30,
                model_path: None,
                gpu_memory_fraction: None, // CPU inference by default
                max_batch_size: Some(8),
                tensor_parallel_size: None,
            },
            custom: HashMap::new(),
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_minute: 60,
            requests_per_hour: 1000,
            burst_size: 10,
            by_ip: true,
            by_api_key: true,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            format: LogFormat::Pretty,
            output: LogOutput::Stdout,
            log_requests: true,
            log_responses: false,
            log_errors: true,
        }
    }
}

/// Configuration manager for loading and saving configs
pub struct ConfigManager {
    config_dir: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Result<Self> {
        let config_dir = Self::get_config_dir()?;
        fs::create_dir_all(&config_dir)?;

        Ok(Self { config_dir })
    }

    /// Get XDG config directory
    fn get_config_dir() -> Result<PathBuf> {
        if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
            Ok(PathBuf::from(xdg_config_home).join("ai-models"))
        } else if let Ok(home) = std::env::var("HOME") {
            Ok(PathBuf::from(home).join(".config/ai-models"))
        } else {
            // Windows fallback
            if let Ok(appdata) = std::env::var("APPDATA") {
                Ok(PathBuf::from(appdata).join("ai-models"))
            } else {
                Err(anyhow::anyhow!("Cannot determine config directory"))
            }
        }
    }

    /// Load configuration from file
    pub fn load_config(&self) -> Result<APIConfig> {
        let config_path = self.config_dir.join("config.toml");

        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            let config: APIConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            // Create default config
            let config = APIConfig::default();
            self.save_config(&config)?;
            Ok(config)
        }
    }

    /// Save configuration to file. It can hold `api_keys`, so on Unix it is
    /// written 0600 (see `journal::write_private`).
    pub fn save_config(&self, config: &APIConfig) -> Result<()> {
        let config_path = self.config_dir.join("config.toml");
        let content = toml::to_string_pretty(config)?;
        crate::journal::write_private(&config_path, content.as_bytes())?;
        Ok(())
    }

    /// Load provider configurations
    pub fn load_providers_config(&self) -> Result<ProvidersConfig> {
        let providers_path = self.config_dir.join("providers.toml");

        if providers_path.exists() {
            let content = fs::read_to_string(&providers_path)?;
            let config: ProvidersConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = ProvidersConfig::default();
            self.save_providers_config(&config)?;
            Ok(config)
        }
    }

    /// Save provider configurations. These can hold `api_key`, so on Unix the
    /// file is written 0600 as well.
    pub fn save_providers_config(&self, config: &ProvidersConfig) -> Result<()> {
        let providers_path = self.config_dir.join("providers.toml");
        let content = toml::to_string_pretty(config)?;
        crate::journal::write_private(&providers_path, content.as_bytes())?;
        Ok(())
    }

    /// Load model aliases
    pub fn load_aliases(&self) -> Result<HashMap<String, String>> {
        let aliases_path = self.config_dir.join("aliases.toml");

        if aliases_path.exists() {
            let content = fs::read_to_string(&aliases_path)?;
            let aliases: HashMap<String, String> = toml::from_str(&content)?;
            Ok(aliases)
        } else {
            Ok(HashMap::new())
        }
    }

    /// Save model aliases
    pub fn save_aliases(&self, aliases: &HashMap<String, String>) -> Result<()> {
        let aliases_path = self.config_dir.join("aliases.toml");
        let content = toml::to_string_pretty(aliases)?;
        fs::write(&aliases_path, content)?;
        Ok(())
    }

    /// Add or update a model alias
    pub fn add_alias(&self, alias: String, model_id: String) -> Result<()> {
        let mut aliases = self.load_aliases()?;
        aliases.insert(alias, model_id);
        self.save_aliases(&aliases)
    }

    /// Remove a model alias
    pub fn remove_alias(&self, alias: &str) -> Result<()> {
        let mut aliases = self.load_aliases()?;
        if aliases.remove(alias).is_none() {
            return Err(anyhow::anyhow!("Alias {} not found", alias));
        }
        self.save_aliases(&aliases)
    }

    /// Get configuration directory path
    pub fn get_config_directory(&self) -> &PathBuf {
        &self.config_dir
    }

    /// Validate configuration
    pub fn validate_config(&self, config: &APIConfig) -> Result<Vec<String>> {
        let mut warnings = Vec::new();

        // Validate server config
        if config.server.port < 1024 {
            warnings.push("Port number below 1024 may require root privileges".to_string());
        }

        // Validate provider configs
        if config.providers.openai.enabled
            && config.providers.openai.api_key.is_none()
            && config.providers.openai.api_key_env.is_none()
        {
            warnings.push("OpenAI provider is enabled but no API key configured".to_string());
        }

        if config.providers.anthropic.enabled
            && config.providers.anthropic.api_key.is_none()
            && config.providers.anthropic.api_key_env.is_none()
        {
            warnings.push("Anthropic provider is enabled but no API key configured".to_string());
        }

        // Validate TLS config
        if config.security.enable_tls
            && (config.security.tls_cert_path.is_none() || config.security.tls_key_path.is_none())
        {
            warnings.push("TLS is enabled but certificate/key paths not configured".to_string());
        }

        // Validate storage config
        if let Some(max_cache) = config.storage.max_cache_size_gb {
            if max_cache < 1 {
                warnings.push("Cache size below 1GB may cause frequent cleanup".to_string());
            }
        }

        Ok(warnings)
    }

    /// Create example configuration files
    pub fn create_example_configs(&self) -> Result<()> {
        let example_dir = self.config_dir.join("examples");
        fs::create_dir_all(&example_dir)?;

        // Example main config
        let example_config = r#"
[server]
host = "127.0.0.1"
port = 8080
max_connections = 1000
request_timeout_seconds = 300
enable_cors = true
cors_origins = ["*"]
enable_openapi = true
openapi_path = "/docs"

[storage]
max_cache_size_gb = 10
auto_cleanup_days = 30

[security]
require_api_key = false
api_keys = []
enable_tls = false

[security.rate_limiting]
enabled = true
requests_per_minute = 60
requests_per_hour = 1000
burst_size = 10
by_ip = true
by_api_key = true

[logging]
level = "Info"
format = "Pretty"
output = "Stdout"
log_requests = true
log_responses = false
log_errors = true
"#;

        fs::write(example_dir.join("config.toml"), example_config)?;

        // Example providers config
        let example_providers = r#"
[openai]
enabled = true
api_key_env = "OPENAI_API_KEY"
base_url = "https://api.openai.com/v1"
timeout_seconds = 60
max_retries = 3
rate_limit_requests_per_minute = 60

[anthropic]
enabled = true
api_key_env = "ANTHROPIC_API_KEY"
base_url = "https://api.anthropic.com/v1"
timeout_seconds = 60
max_retries = 3
rate_limit_requests_per_minute = 60

[local]
enabled = true
inference_engine = "LlamaCpp"
max_models_loaded = 3
model_cache_size_gb = 8.0
context_size = 4096
"#;

        fs::write(example_dir.join("providers.toml"), example_providers)?;

        // Example aliases
        let example_aliases = r#"
# Model aliases for easier access
gpt4 = "gpt-4-turbo"
gpt35 = "gpt-3.5-turbo"
claude = "claude-3-sonnet-20240229"
llama = "meta-llama/Llama-2-7b-chat-hf"
"#;

        fs::write(example_dir.join("aliases.toml"), example_aliases)?;

        Ok(())
    }
}

/// Fold a CLI `--require-api-key` flag into the configured value.
///
/// The flag **may only ever turn authentication on**. `aimodel` used to assign
/// it straight over the config — `config.security.require_api_key =
/// args.require_api_key` — and the flag is a bare `#[arg(long)] bool`, so it is
/// `false` whenever absent. A user who wrote `require_api_key = true` in their
/// config and then ran `aimodel server` had authentication silently switched
/// **off**. A CLI flag may add a requirement; it may not silently drop one.
pub fn resolve_auth_requirement(configured: bool, flag: bool) -> bool {
    configured || flag
}

/// Refuse a configuration the server cannot run safely. Called before anything
/// is bound, by `APIServer::start` and by `aimodel`.
///
/// * `enable_tls` is refused: the server has no TLS. Serving plain HTTP while
///   the config says TLS would mislead the operator, so a terminating proxy is
///   the supported route.
/// * A host that is not loopback is refused unless `require_api_key` is on and
///   at least one key is configured. Its routes download, convert and delete
///   models and spend provider keys, so they must not be open to the network.
pub fn check_startup(server: &ServerConfig, security: &SecurityConfig) -> Result<()> {
    if security.enable_tls {
        anyhow::bail!(
            "[security] enable_tls = true, but this server does not implement TLS, \
             so it would serve plain HTTP under a TLS setting. Set enable_tls = false, \
             bind 127.0.0.1, and put a TLS-terminating reverse proxy in front of it."
        );
    }
    if is_loopback(&server.host) {
        return Ok(());
    }
    let mut missing = Vec::new();
    if !security.require_api_key {
        missing.push("require_api_key = true (or --require-api-key)");
    }
    if security.api_keys.is_empty() {
        missing.push("at least one entry in api_keys");
    }
    if missing.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "refusing to bind {host}: it is not a loopback address, so every route (model \
         download, convert, delete, storage cleanup, provider validation) would be \
         reachable by anything that can route to this host, and provider keys could be \
         spent by it. To serve it, set [security] {needs}. To keep it local, bind \
         127.0.0.1.",
        host = server.host,
        needs = missing.join(" and ")
    )
}

/// Whether `host` names only this machine.
///
/// Deliberately conservative: anything not recognisably loopback counts as
/// exposed, so an unfamiliar spelling errs toward warning rather than silence.
/// `0.0.0.0` and `::` are the wildcards that bind every interface.
pub fn is_loopback(host: &str) -> bool {
    let h = host.trim().trim_start_matches('[').trim_end_matches(']');
    if h.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match h.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Removing an alias that is not stored is an error and leaves the stored
    /// aliases alone. Uses a temp dir set on the struct, never the real config dir.
    #[test]
    fn remove_alias_reports_an_absent_alias() {
        let dir = std::env::temp_dir().join(format!("ae-remove-alias-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let manager = ConfigManager {
            config_dir: dir.clone(),
        };

        manager
            .add_alias("fast".to_string(), "local/model".to_string())
            .unwrap();
        assert!(manager.remove_alias("slow").is_err());
        assert_eq!(manager.load_aliases().unwrap().len(), 1);
        assert!(manager.remove_alias("fast").is_ok());
        assert!(manager.load_aliases().unwrap().is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}
