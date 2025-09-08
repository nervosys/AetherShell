use clap::{Args, Parser, Subcommand};
use aether_shell::ai_api::*;
use anyhow::Result;
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

#[tokio::main]
async fn main() -> Result<()> {
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
    }
}

async fn run_server(config: APIConfig, args: ServerArgs) -> Result<()> {
    if args.daemon {
        println!("Starting server in daemon mode...");
        // TODO: Implement daemon mode
    }
    
    println!("Starting AI Model API server on {}:{}", config.server.host, config.server.port);
    println!("OpenAPI docs available at: http://{}:{}{}", 
             config.server.host, config.server.port, config.server.openapi_path);
    
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
            println!("{:<30} {:<15} {:<10} {:<15}", "Model ID", "Provider", "Format", "Size");
            println!("{}", "-".repeat(70));
            
            for model in &filtered_models {
                let size = if let Some(bytes) = model.size_bytes {
                    human_bytes(bytes)
                } else {
                    "Unknown".to_string()
                };
                
                println!("{:<30} {:<15} {:<10} {:<15}", 
                         model.id, model.provider, format!("{:?}", model.format), size);
                
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
        print!("Are you sure you want to remove model '{}'? (y/N): ", args.model_id);
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
        let downloads = result.downloads.map(|d| d.to_string()).unwrap_or_else(|| "N/A".to_string());
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
    
    let output_path = args.output.unwrap_or_else(|| {
        format!("{}.{}", args.source, args.to)
    });
    
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
            println!("Example configuration files created in {:?}", 
                     config_manager.get_config_directory());
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
            println!("  openai: {}", if config.providers.openai.enabled { "enabled" } else { "disabled" });
            println!("  anthropic: {}", if config.providers.anthropic.enabled { "enabled" } else { "disabled" });
            println!("  local: {}", if config.providers.local.enabled { "enabled" } else { "disabled" });
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
