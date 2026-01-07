use aether_shell::ai_api::*;
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use serde_json;

#[derive(Parser)]
#[command(name = "aimodel")]
#[command(about = "AI Model Management CLI")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Config file path (defaults to XDG config directory)
    #[arg(long, global = true)]
    pub config: Option<String>,

    /// Verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the API server
    Server(ServerArgs),

    /// List available models
    #[command(alias = "ls")]
    List(ListArgs),

    /// Download a model
    Download(DownloadArgs),

    /// Remove a model
    #[command(alias = "rm")]
    Remove(RemoveArgs),

    /// Search for models
    Search(SearchArgs),

    /// Convert model format
    Convert(ConvertArgs),

    /// Manage configuration
    Config(ConfigArgs),

    /// Storage management
    Storage(StorageArgs),

    /// Provider management
    Provider(ProviderArgs),

    /// Model aliases
    Alias(AliasArgs),

    /// LLM backend management
    Backend(BackendArgs),

    /// Secure API key management
    #[command(alias = "key")]
    Keys(KeysArgs),
}

#[derive(Args)]
pub struct ServerArgs {
    /// Server host
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Server port
    #[arg(long, default_value = "8080")]
    pub port: u16,

    /// Enable CORS
    #[arg(long)]
    pub cors: bool,

    /// Require API key
    #[arg(long)]
    pub require_api_key: bool,

    /// Background/daemon mode
    #[arg(long)]
    pub daemon: bool,
}

#[derive(Args)]
pub struct ListArgs {
    /// Filter by provider
    #[arg(long)]
    pub provider: Option<String>,

    /// Show only local models
    #[arg(long)]
    pub local: bool,

    /// Show detailed information
    #[arg(long)]
    pub detailed: bool,

    /// Output format (table, json)
    #[arg(long, default_value = "table")]
    pub format: String,
}

#[derive(Args)]
pub struct DownloadArgs {
    /// Model ID to download
    pub model_id: String,

    /// Source (huggingface, url)
    #[arg(long, default_value = "huggingface")]
    pub source: String,

    /// Preferred format
    #[arg(long)]
    pub format: Option<String>,

    /// Quantization level
    #[arg(long)]
    pub quantization: Option<String>,

    /// Force re-download if exists
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// Model ID to remove
    pub model_id: String,

    /// Remove without confirmation
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args)]
pub struct SearchArgs {
    /// Search query
    pub query: String,

    /// Source to search (huggingface)
    #[arg(long, default_value = "huggingface")]
    pub source: String,

    /// Limit results
    #[arg(long, default_value = "20")]
    pub limit: usize,
}

#[derive(Args)]
pub struct ConvertArgs {
    /// Source model path or ID
    pub source: String,

    /// Target format
    #[arg(long)]
    pub to: String,

    /// Output path
    #[arg(long)]
    pub output: Option<String>,

    /// Quantization type
    #[arg(long)]
    pub quantization: Option<String>,
}

#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show current configuration
    Show,

    /// Set configuration value
    Set {
        /// Configuration key (dot notation, e.g., server.port)
        key: String,
        /// Configuration value
        value: String,
    },

    /// Get configuration value
    Get {
        /// Configuration key
        key: String,
    },

    /// Reset to default configuration
    Reset,

    /// Create example configuration files
    Examples,
}

#[derive(Args)]
pub struct StorageArgs {
    #[command(subcommand)]
    pub action: StorageAction,
}

#[derive(Subcommand)]
pub enum StorageAction {
    /// Show storage statistics
    Stats,

    /// Clean up cache
    Cleanup {
        /// Maximum age in days
        #[arg(long, default_value = "30")]
        max_age: u64,
    },

    /// Show directory paths
    Paths,
}

#[derive(Args)]
pub struct ProviderArgs {
    #[command(subcommand)]
    pub action: ProviderAction,
}

#[derive(Subcommand)]
pub enum ProviderAction {
    /// List providers
    List,

    /// Test provider connection
    Test {
        /// Provider name
        provider: String,
    },

    /// Configure provider
    Configure {
        /// Provider name
        provider: String,
        /// API key
        #[arg(long)]
        api_key: Option<String>,
    },
}

#[derive(Args)]
pub struct AliasArgs {
    #[command(subcommand)]
    pub action: AliasAction,
}

#[derive(Subcommand)]
pub enum AliasAction {
    /// List aliases
    List,

    /// Add alias
    Add {
        /// Alias name
        alias: String,
        /// Model ID
        model_id: String,
    },

    /// Remove alias
    Remove {
        /// Alias name
        alias: String,
    },
}

#[derive(Args)]
pub struct BackendArgs {
    #[command(subcommand)]
    pub action: BackendAction,
}

#[derive(Subcommand)]
pub enum BackendAction {
    /// List available backends
    List,

    /// Start a backend
    Start {
        /// Backend name (vllm, tensorrt-llm, sglang, llama.cpp)  
        backend: String,
        /// Model path or name
        #[arg(long)]
        model: Option<String>,
        /// Custom endpoint
        #[arg(long)]
        endpoint: Option<String>,
        /// GPU memory fraction (0.0-1.0)
        #[arg(long)]
        gpu_memory: Option<f32>,
        /// Tensor parallel size
        #[arg(long)]
        tensor_parallel: Option<u32>,
    },

    /// Stop a backend
    Stop {
        /// Backend name
        backend: String,
    },

    /// Check backend status
    Status {
        /// Backend name (optional, shows all if not specified)
        backend: Option<String>,
    },

    /// Test backend connection
    Test {
        /// Backend name
        backend: String,
        /// Custom endpoint
        #[arg(long)]
        endpoint: Option<String>,
    },

    /// Auto-detect running backends
    Detect,
}

#[derive(Args)]
pub struct KeysArgs {
    #[command(subcommand)]
    pub action: KeysAction,
}

#[derive(Subcommand)]
pub enum KeysAction {
    /// Store an API key securely in OS credential store
    Store {
        /// Provider/service name (openai, anthropic, google, etc.)
        provider: String,
        /// API key (will be read from stdin if not provided for security)
        #[arg(long)]
        key: Option<String>,
    },

    /// Retrieve an API key from OS credential store
    Get {
        /// Provider/service name
        provider: String,
    },

    /// Delete an API key from OS credential store
    Delete {
        /// Provider/service name
        provider: String,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },

    /// List all stored API keys (shows providers only, not actual keys)
    List,

    /// Migrate API keys from environment variables to credential store
    Migrate {
        /// Provider name (openai, anthropic, google)
        #[arg(long)]
        provider: Option<String>,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },

    /// Validate API key format
    Validate {
        /// Provider/service name
        provider: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Print deprecation notice
    eprintln!("⚠️  DEPRECATION NOTICE:");
    eprintln!("⚠️  The 'aimodel' command is deprecated.");
    eprintln!("⚠️  Please use 'ae ai' instead:");
    eprintln!("⚠️  ");
    eprintln!("⚠️    aimodel serve     →  ae ai serve");
    eprintln!("⚠️    aimodel list      →  ae ai list");
    eprintln!("⚠️    aimodel download  →  ae ai download");
    eprintln!("⚠️    aimodel keys      →  ae ai keys");
    eprintln!();

    let cli = Cli::parse();

    // Initialize logging
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();
    }

    // Load configuration
    let config_manager = ConfigManager::new()?;
    let mut config = config_manager.load_config()?;

    // Override config with CLI args if provided
    if let Commands::Server(ref args) = cli.command {
        config.server.host = args.host.clone();
        config.server.port = args.port;
        config.server.enable_cors = args.cors;
        config.security.require_api_key = args.require_api_key;
    }

    match cli.command {
        Commands::Server(args) => run_server(config, args).await,
        Commands::List(args) => run_list(config, args).await,
        Commands::Download(args) => run_download(config, args).await,
        Commands::Remove(args) => run_remove(config, args).await,
        Commands::Search(args) => run_search(config, args).await,
        Commands::Convert(args) => run_convert(config, args).await,
        Commands::Config(args) => run_config(config_manager, args).await,
        Commands::Storage(args) => run_storage(config, args).await,
        Commands::Provider(args) => run_provider(config, args).await,
        Commands::Alias(args) => run_alias(config_manager, args).await,
        Commands::Backend(args) => run_backend(config, args).await,
        Commands::Keys(args) => run_keys(args).await,
    }
}

async fn run_server(config: APIConfig, args: ServerArgs) -> Result<()> {
    if args.daemon {
        println!("Starting server in daemon mode...");
        // TODO: Implement daemon mode
    }

    println!(
        "Starting AI Model API server on {}:{}",
        config.server.host, config.server.port
    );
    println!(
        "OpenAPI docs available at: http://{}:{}{}",
        config.server.host, config.server.port, config.server.openapi_path
    );

    start_server(config).await
}

async fn run_list(config: APIConfig, args: ListArgs) -> Result<()> {
    let api = AIModelAPI::new(config)?;
    let models = api.list_models().await?;

    let mut filtered_models = models;

    // Apply filters
    if let Some(provider) = &args.provider {
        filtered_models.retain(|m| &m.provider == provider);
    }

    if args.local {
        filtered_models.retain(|m| m.local_path.is_some());
    }

    match args.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&filtered_models)?);
        }
        _ => {
            // Table format
            println!(
                "{:<30} {:<15} {:<10} {:<15}",
                "Model ID", "Provider", "Format", "Size"
            );
            println!("{}", "-".repeat(70));

            for model in &filtered_models {
                let size = if let Some(bytes) = model.size_bytes {
                    human_bytes(bytes)
                } else {
                    "Unknown".to_string()
                };

                println!(
                    "{:<30} {:<15} {:<10} {:<15}",
                    model.id,
                    model.provider,
                    format!("{:?}", model.format),
                    size
                );

                if args.detailed {
                    if let Some(desc) = model.metadata.get("description") {
                        println!("  Description: {}", desc);
                    }
                    if let Some(path) = &model.local_path {
                        println!("  Local path: {}", path);
                    }
                    println!();
                }
            }
        }
    }

    Ok(())
}

async fn run_download(config: APIConfig, args: DownloadArgs) -> Result<()> {
    let storage = ModelStorage::new(&config.storage)?;
    let mut downloader = ModelDownloader::new(storage)?;

    let format_preference = args.format.as_ref().and_then(|f| match f.as_str() {
        "gguf" => Some(ModelFormat::GGUF),
        "safetensors" => Some(ModelFormat::SafeTensors),
        "pytorch" => Some(ModelFormat::PyTorch),
        "onnx" => Some(ModelFormat::ONNX),
        _ => None,
    });

    let request = DownloadRequest {
        model_id: args.model_id.clone(),
        source: ModelSource {
            origin: args.source,
            url: None,
            repository: Some(args.model_id.clone()),
            commit: None,
            license: None,
        },
        format_preference,
        quantization: args.quantization,
        validate_checksum: true,
    };

    println!("Downloading model: {}", args.model_id);

    let metadata = downloader.download_model(request).await?;

    println!("Successfully downloaded model:");
    println!("  ID: {}", metadata.id);
    println!("  Format: {:?}", metadata.format);
    println!("  Size: {}", human_bytes(metadata.size_bytes));
    println!("  Path: {}", metadata.file_path);

    Ok(())
}

async fn run_remove(config: APIConfig, args: RemoveArgs) -> Result<()> {
    let mut storage = ModelStorage::new(&config.storage)?;

    if !args.yes {
        print!(
            "Are you sure you want to remove model '{}'? (y/N): ",
            args.model_id
        );
        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().to_lowercase().starts_with('y') {
            println!("Cancelled.");
            return Ok(());
        }
    }

    storage.remove_model(&args.model_id)?;
    println!("Removed model: {}", args.model_id);

    Ok(())
}

async fn run_search(config: APIConfig, args: SearchArgs) -> Result<()> {
    let storage = ModelStorage::new(&config.storage)?;
    let downloader = ModelDownloader::new(storage)?;

    println!("Searching for models matching '{}'...", args.query);

    let results = downloader.search_models(&args.query, &args.source).await?;

    println!("\nFound {} models:", results.len());
    println!("{:<50} {:<15} {:<10}", "Model ID", "Downloads", "Pipeline");
    println!("{}", "-".repeat(75));

    for result in results.iter().take(args.limit) {
        let downloads = result
            .downloads
            .map(|d| d.to_string())
            .unwrap_or_else(|| "N/A".to_string());
        let pipeline_default = "N/A".to_string();
        let pipeline = result.pipeline_tag.as_ref().unwrap_or(&pipeline_default);

        println!("{:<50} {:<15} {:<10}", result.id, downloads, pipeline);
    }

    Ok(())
}

async fn run_convert(_config: APIConfig, args: ConvertArgs) -> Result<()> {
    let converter = ModelConverter::new();

    let target_format = match args.to.as_str() {
        "gguf" => ModelFormat::GGUF,
        "safetensors" => ModelFormat::SafeTensors,
        "pytorch" => ModelFormat::PyTorch,
        "onnx" => ModelFormat::ONNX,
        _ => return Err(anyhow::anyhow!("Unsupported target format: {}", args.to)),
    };

    let source_format = ModelFormat::PyTorch; // TODO: Auto-detect

    let output_path = args
        .output
        .unwrap_or_else(|| format!("{}.{}", args.source, args.to));

    let request = ConversionRequest {
        source_path: args.source,
        source_format,
        target_format,
        target_path: output_path,
        preserve_metadata: true,
        compression_level: None,
        quantization: args.quantization.as_ref().and_then(|q| match q.as_str() {
            "f16" => Some(QuantizationType::F16),
            "q4_0" => Some(QuantizationType::Q4_0),
            "q4_1" => Some(QuantizationType::Q4_1),
            "q5_0" => Some(QuantizationType::Q5_0),
            "q5_1" => Some(QuantizationType::Q5_1),
            "q8_0" => Some(QuantizationType::Q8_0),
            "q8_1" => Some(QuantizationType::Q8_1),
            _ => None,
        }),
    };

    println!("Converting model...");

    let result = converter.convert_model(request).await?;

    println!("Conversion completed:");
    println!("  Output: {}", result.target_path);
    println!("  Size: {}", human_bytes(result.target_size));
    println!("  Time: {}ms", result.conversion_time_ms);

    if !result.warnings.is_empty() {
        println!("  Warnings:");
        for warning in &result.warnings {
            println!("    - {}", warning);
        }
    }

    Ok(())
}

async fn run_config(config_manager: ConfigManager, args: ConfigArgs) -> Result<()> {
    match args.action {
        ConfigAction::Show => {
            let config = config_manager.load_config()?;
            println!("{}", toml::to_string_pretty(&config)?);
        }
        ConfigAction::Set { key, value } => {
            println!("Setting {}: {}", key, value);
            // TODO: Implement config setting
        }
        ConfigAction::Get { key } => {
            println!("Getting {}", key);
            // TODO: Implement config getting
        }
        ConfigAction::Reset => {
            let config = APIConfig::default();
            config_manager.save_config(&config)?;
            println!("Configuration reset to defaults");
        }
        ConfigAction::Examples => {
            config_manager.create_example_configs()?;
            println!(
                "Example configuration files created in {:?}",
                config_manager.get_config_directory()
            );
        }
    }

    Ok(())
}

async fn run_storage(config: APIConfig, args: StorageArgs) -> Result<()> {
    let storage = ModelStorage::new(&config.storage)?;

    match args.action {
        StorageAction::Stats => {
            let stats = storage.get_storage_stats();
            println!("Storage Statistics:");
            println!("  Models: {}", stats.model_count);
            println!("  Total size: {}", stats.total_size_human());
            println!("  Cache size: {}", stats.cache_size_human());
            println!("  Data directory: {:?}", stats.data_dir);
            println!("  Config directory: {:?}", stats.config_dir);

            if !stats.format_breakdown.is_empty() {
                println!("  Format breakdown:");
                for (format, count) in &stats.format_breakdown {
                    println!("    {:?}: {}", format, count);
                }
            }
        }
        StorageAction::Cleanup { max_age } => {
            let cleaned_bytes = storage.cleanup_cache(max_age)?;
            println!("Cleaned up {} of cache files", human_bytes(cleaned_bytes));
        }
        StorageAction::Paths => {
            let stats = storage.get_storage_stats();
            println!("Data directory: {:?}", stats.data_dir);
            println!("Config directory: {:?}", stats.config_dir);
        }
    }

    Ok(())
}

async fn run_provider(config: APIConfig, args: ProviderArgs) -> Result<()> {
    match args.action {
        ProviderAction::List => {
            println!("Available providers:");
            println!(
                "  openai: {}",
                if config.providers.openai.enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            println!(
                "  anthropic: {}",
                if config.providers.anthropic.enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            println!(
                "  local: {}",
                if config.providers.local.enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
        }
        ProviderAction::Test { provider } => {
            println!("Testing provider: {}", provider);
            // TODO: Implement provider testing
        }
        ProviderAction::Configure { provider, api_key } => {
            println!("Configuring provider: {}", provider);
            if let Some(key) = api_key {
                println!("API key provided (length: {})", key.len());
                // TODO: Implement provider configuration
            }
        }
    }

    Ok(())
}

async fn run_alias(config_manager: ConfigManager, args: AliasArgs) -> Result<()> {
    match args.action {
        AliasAction::List => {
            let aliases = config_manager.load_aliases()?;
            if aliases.is_empty() {
                println!("No aliases configured");
            } else {
                println!("Model aliases:");
                for (alias, model_id) in &aliases {
                    println!("  {} -> {}", alias, model_id);
                }
            }
        }
        AliasAction::Add { alias, model_id } => {
            config_manager.add_alias(alias.clone(), model_id.clone())?;
            println!("Added alias: {} -> {}", alias, model_id);
        }
        AliasAction::Remove { alias } => {
            config_manager.remove_alias(&alias)?;
            println!("Removed alias: {}", alias);
        }
    }

    Ok(())
}

async fn run_backend(config: APIConfig, args: BackendArgs) -> Result<()> {
    match args.action {
        BackendAction::List => {
            println!("Available LLM Backends:");
            println!("────────────────────────");

            let backends = vec![
                (
                    "vllm",
                    "vLLM High-Performance Inference",
                    config.providers.vllm.enabled,
                ),
                (
                    "tensorrt-llm",
                    "TensorRT-LLM NVIDIA Optimized",
                    config.providers.tensorrt_llm.enabled,
                ),
                (
                    "sglang",
                    "SGLang High-Throughput Serving",
                    config.providers.sglang.enabled,
                ),
                (
                    "llama.cpp",
                    "llama.cpp CPU/GPU Inference",
                    config.providers.llama_cpp.enabled,
                ),
            ];

            for (name, description, enabled) in backends {
                let status = if enabled {
                    "✓ enabled"
                } else {
                    "✗ disabled"
                };
                println!("{:<15} {:<35} {}", name, description, status);
            }
        }

        BackendAction::Start {
            backend,
            model,
            endpoint: _,
            gpu_memory,
            tensor_parallel,
        } => {
            println!("Starting {} backend...", backend);

            match backend.as_str() {
                "vllm" => {
                    let cmd = format!(
                        "python -m vllm.entrypoints.openai.api_server --model {} --host 0.0.0.0 --port 8000{}{}",
                        model.as_deref().unwrap_or("microsoft/DialoGPT-medium"),
                        gpu_memory.map(|g| format!(" --gpu-memory-utilization {}", g)).unwrap_or_default(),
                        tensor_parallel.map(|t| format!(" --tensor-parallel-size {}", t)).unwrap_or_default()
                    );
                    println!("Run: {}", cmd);
                }
                "tensorrt-llm" => {
                    println!("TensorRT-LLM startup requires pre-built engines.");
                    println!("Please refer to TensorRT-LLM documentation for model conversion.");
                }
                "sglang" => {
                    let cmd = format!(
                        "python -m sglang.launch_server --model-path {} --host 0.0.0.0 --port 30000{}",
                        model.as_deref().unwrap_or("microsoft/DialoGPT-medium"),
                        gpu_memory.map(|g| format!(" --mem-fraction-static {}", g)).unwrap_or_default()
                    );
                    println!("Run: {}", cmd);
                }
                "llama.cpp" => {
                    let model_path = model.as_deref().unwrap_or("./model.gguf");
                    let cmd = format!(
                        "./llama-server -m {} --host 0.0.0.0 --port 8080{}",
                        model_path,
                        gpu_memory
                            .map(|_| " --n-gpu-layers 32".to_string())
                            .unwrap_or_default()
                    );
                    println!("Run: {}", cmd);
                }
                _ => {
                    println!("Unknown backend: {}", backend);
                    println!("Available backends: vllm, tensorrt-llm, sglang, llama.cpp");
                }
            }
        }

        BackendAction::Stop { backend: _ } => {
            println!("Backend stopping not implemented - please stop the process manually.");
        }

        BackendAction::Status { backend } => {
            if let Some(backend_name) = backend {
                println!("Checking status of {} backend...", backend_name);
                // TODO: Actually check if the backend is running
                println!("Status check not yet implemented");
            } else {
                println!("Checking status of all backends...");
                // TODO: Check all configured backends
                println!("Status check not yet implemented");
            }
        }

        BackendAction::Test { backend, endpoint } => {
            println!("Testing {} backend connection...", backend);

            let test_endpoint = endpoint.unwrap_or_else(|| match backend.as_str() {
                "vllm" => "http://localhost:8000".to_string(),
                "tensorrt-llm" => "http://localhost:8001".to_string(),
                "sglang" => "http://localhost:30000".to_string(),
                "llama.cpp" => "http://localhost:8080".to_string(),
                _ => "http://localhost:8080".to_string(),
            });

            // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
            let client = aether_shell::security::create_secure_async_client()
                .unwrap_or_else(|_| reqwest::Client::new());

            match client
                .get(&format!("{}/v1/models", test_endpoint))
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        println!("✓ Backend {} is responding at {}", backend, test_endpoint);
                    } else {
                        println!(
                            "✗ Backend {} returned status: {}",
                            backend,
                            response.status()
                        );
                    }
                }
                Err(e) => {
                    println!(
                        "✗ Failed to connect to {} backend at {}: {}",
                        backend, test_endpoint, e
                    );
                }
            }
        }

        BackendAction::Detect => {
            println!("Auto-detecting running LLM backends...");

            let endpoints = vec![
                ("vllm", "http://localhost:8000"),
                ("tensorrt-llm", "http://localhost:8001"),
                ("sglang", "http://localhost:30000"),
                ("llama.cpp", "http://localhost:8080"),
            ];

            // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
            let client = aether_shell::security::create_secure_async_client()
                .unwrap_or_else(|_| reqwest::Client::new());

            for (name, endpoint) in endpoints {
                match client.get(&format!("{}/v1/models", endpoint)).send().await {
                    Ok(response) if response.status().is_success() => {
                        println!("✓ Found {} backend at {}", name, endpoint);
                    }
                    _ => {
                        // Try alternative health endpoints
                        if name == "llama.cpp" {
                            if let Ok(response) =
                                client.get(&format!("{}/health", endpoint)).send().await
                            {
                                if response.status().is_success() {
                                    println!("✓ Found {} backend at {}", name, endpoint);
                                    continue;
                                }
                            }
                        }
                        println!("✗ No {} backend found at {}", name, endpoint);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn run_keys(args: KeysArgs) -> Result<()> {
    use aether_shell::secure_config::SecureApiConfig;
    use std::io::{self, Write};

    match args.action {
        KeysAction::Store { provider, key } => {
            let api_key = if let Some(k) = key {
                k
            } else {
                // Read from stdin for security (doesn't show in shell history)
                print!("Enter API key for {}: ", provider);
                io::stdout().flush()?;

                let mut buffer = String::new();
                io::stdin().read_line(&mut buffer)?;
                buffer.trim().to_string()
            };

            if api_key.is_empty() {
                anyhow::bail!("API key cannot be empty");
            }

            // Use SecureApiConfig to store the key
            SecureApiConfig::store_in_keyring(&provider, &api_key)?;
            println!(
                "✓ API key for '{}' securely stored in OS credential store",
                provider
            );
            println!("\nThe key can now be used with:");
            println!("  - AetherShell AI functions");
            println!("  - `aimodel` server");
            println!("  - Environment variable fallback disabled");
        }

        KeysAction::Get { provider } => {
            let config = SecureApiConfig::from_keyring(
                &provider,
                String::new(), // endpoint not needed for get
                String::new(), // model not needed for get
                provider.clone(),
            )?;

            if let Some(key) = config.get_api_key() {
                // Only show first/last few characters for security
                let masked = if key.len() > 12 {
                    format!("{}...{}", &key[..6], &key[key.len() - 4..])
                } else {
                    "*".repeat(key.len())
                };

                println!("API key for '{}': {}", provider, masked);
                println!("Key length: {} characters", key.len());

                // Validate format
                if let Err(e) = config.validate_format() {
                    println!("⚠ Warning: {}", e);
                }
            } else {
                println!("No API key found for '{}'", provider);
            }
        }

        KeysAction::Delete { provider, yes } => {
            if !yes {
                print!("Delete API key for '{}'? [y/N]: ", provider);
                io::stdout().flush()?;

                let mut buffer = String::new();
                io::stdin().read_line(&mut buffer)?;

                if buffer.trim().to_lowercase() != "y" {
                    println!("Cancelled");
                    return Ok(());
                }
            }

            SecureApiConfig::delete_from_keyring(&provider)?;
            println!(
                "✓ API key for '{}' deleted from OS credential store",
                provider
            );
        }

        KeysAction::List => {
            println!("Stored API Keys (in OS credential store):");
            println!("──────────────────────────────────────────");

            // Common providers to check
            let providers = vec![
                "openai",
                "anthropic",
                "google",
                "cohere",
                "huggingface",
                "mistral",
                "groq",
            ];

            for provider in providers {
                use aether_shell::secure_config::SecureApiConfig;

                match SecureApiConfig::from_keyring(
                    provider,
                    String::new(),
                    String::new(),
                    provider.to_string(),
                ) {
                    Ok(config) => {
                        if config.has_api_key() {
                            println!("✓ {:<15} (stored)", provider);
                        }
                    }
                    Err(_) => {
                        // Not stored, skip
                    }
                }
            }

            println!("\nUse 'aimodel keys get <provider>' to view (masked) key details");
        }

        KeysAction::Migrate { provider, yes } => {
            let providers_to_migrate = if let Some(p) = provider {
                vec![p]
            } else {
                // Migrate all common providers
                vec![
                    "openai".to_string(),
                    "anthropic".to_string(),
                    "google".to_string(),
                ]
            };

            for provider_name in providers_to_migrate {
                let env_var = match provider_name.as_str() {
                    "openai" => "OPENAI_API_KEY",
                    "anthropic" => "ANTHROPIC_API_KEY",
                    "google" => "GOOGLE_API_KEY",
                    _ => {
                        println!("⚠ Unknown provider '{}', skipping", provider_name);
                        continue;
                    }
                };

                if let Ok(api_key) = std::env::var(env_var) {
                    if !yes {
                        print!(
                            "Migrate {} from environment variable {} to credential store? [y/N]: ",
                            provider_name, env_var
                        );
                        io::stdout().flush()?;

                        let mut buffer = String::new();
                        io::stdin().read_line(&mut buffer)?;

                        if buffer.trim().to_lowercase() != "y" {
                            println!("Skipped {}", provider_name);
                            continue;
                        }
                    }

                    SecureApiConfig::store_in_keyring(&provider_name, &api_key)?;
                    println!("✓ Migrated {} API key to credential store", provider_name);
                    println!("  You can now remove {} from your environment", env_var);
                } else {
                    println!("✗ No {} environment variable found", env_var);
                }
            }
        }

        KeysAction::Validate { provider } => {
            use aether_shell::secure_config::SecureApiConfig;

            let config = SecureApiConfig::from_keyring(
                &provider,
                String::new(),
                String::new(),
                provider.clone(),
            )?;

            match config.validate_format() {
                Ok(_) => {
                    println!("✓ API key for '{}' has valid format", provider);
                }
                Err(e) => {
                    println!("✗ API key for '{}' validation failed: {}", provider, e);
                }
            }
        }
    }

    Ok(())
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}
