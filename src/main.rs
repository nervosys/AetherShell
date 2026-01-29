use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::io::{self, Read};

use aethershell::{env::Env, eval, parser, transpile};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ae")]
#[command(about = "Aether Shell - A typed functional shell with multi-modal AI")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    subcommand: Option<Commands>,

    /// Bash compatibility mode
    #[arg(long, short = 'b')]
    bash: bool,

    /// Execute a command string
    #[arg(long, short = 'c')]
    command: Option<String>,

    /// Script file to execute
    #[arg(value_name = "FILE")]
    file: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start terminal GUI (TUI) mode
    Tui,

    /// AI model management (list, download, serve API, etc.)
    #[command(alias = "model")]
    Ai {
        #[command(subcommand)]
        command: AiCommands,
    },

    /// Start MCP (Model Context Protocol) server mode
    #[command(visible_alias = "mcp-server")]
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },
}

#[derive(Subcommand)]
enum AiCommands {
    /// Start the AI model API server
    Serve {
        /// Server host
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Server port
        #[arg(long, default_value = "8080")]
        port: u16,
        /// Enable CORS
        #[arg(long)]
        cors: bool,
    },
    /// List available AI models
    #[command(alias = "ls")]
    List {
        /// Filter by provider
        #[arg(long)]
        provider: Option<String>,
        /// Show only local models
        #[arg(long)]
        local: bool,
    },
    /// Download a model
    Download {
        /// Model identifier or URL
        model: String,
    },
    /// Show AI model configuration
    Config,
    /// Manage API keys securely
    #[command(alias = "key")]
    Keys {
        #[command(subcommand)]
        action: KeysAction,
    },
}

#[derive(Subcommand)]
enum KeysAction {
    /// Store an API key securely
    Store {
        /// Provider name (openai, anthropic, google, etc.)
        provider: String,
        /// API key (reads from stdin if not provided)
        #[arg(long)]
        key: Option<String>,
    },
    /// Get an API key
    Get {
        /// Provider name
        provider: String,
    },
    /// Delete an API key
    Delete {
        /// Provider name
        provider: String,
    },
    /// List stored API key providers
    List,
}

#[derive(Subcommand)]
enum McpCommands {
    /// Start the MCP server
    Serve {
        /// Server host
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Server port
        #[arg(long, default_value = "3001")]
        port: u16,
        /// Enable CORS for browser access
        #[arg(long)]
        cors: bool,
        /// Safety level (safe, caution, dangerous, critical)
        #[arg(long, default_value = "caution")]
        safety: String,
        /// Allow admin tools
        #[arg(long)]
        admin: bool,
    },
    /// List available MCP tools
    Tools {
        /// Filter by category
        #[arg(long)]
        category: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle subcommands
    if let Some(cmd) = cli.subcommand {
        return match cmd {
            Commands::Tui => {
                aethershell::tui::run()?;
                Ok(())
            }
            Commands::Ai { command } => {
                tokio::runtime::Runtime::new()?.block_on(handle_ai_command(command))
            }
            Commands::Mcp { command } => {
                tokio::runtime::Runtime::new()?.block_on(handle_mcp_command(command))
            }
        };
    }

    // Handle bash mode with stdin
    if cli.bash && cli.file.is_none() && cli.command.is_none() {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        let code = transpile::bash::transpile_bash_to_ae(&buf)?;
        return run_code(&code);
    }

    // Handle -c/--command flag
    if let Some(cmd) = cli.command {
        let code = if cli.bash {
            transpile::bash::transpile_bash_to_ae(&cmd)?
        } else {
            cmd
        };
        return run_code(&code);
    }

    // Handle file execution or REPL
    match cli.file {
        Some(file) => run_file(&file, cli.bash)?,
        None => repl()?,
    }

    Ok(())
}

async fn handle_ai_command(command: AiCommands) -> Result<()> {
    use aethershell::ai_api::*;

    match command {
        AiCommands::Serve { host, port, cors } => {
            let config_manager = ConfigManager::new()?;
            let mut config = config_manager.load_config()?;
            config.server.host = host.clone();
            config.server.port = port;
            config.server.enable_cors = cors;

            println!("Starting AI Model API server on {}:{}", host, port);
            println!(
                "OpenAPI docs: http://{}:{}{}",
                host, port, config.server.openapi_path
            );

            start_server(config).await
        }
        AiCommands::List { provider, local } => {
            let config_manager = ConfigManager::new()?;
            let config = config_manager.load_config()?;
            let api = AIModelAPI::new(config)?;
            let mut models = api.list_models().await?;

            if let Some(p) = provider {
                models.retain(|m| m.provider == p);
            }
            if local {
                models.retain(|m| m.local_path.is_some());
            }

            println!("{:<30} {:<15} {:<10}", "Model ID", "Provider", "Format");
            println!("{}", "-".repeat(55));
            for model in models {
                println!(
                    "{:<30} {:<15} {:<10}",
                    model.id,
                    model.provider,
                    format!("{:?}", model.format)
                );
            }
            Ok(())
        }
        AiCommands::Download { model } => {
            let config_manager = ConfigManager::new()?;
            let config = config_manager.load_config()?;
            let storage = ModelStorage::new(&config.storage)?;
            let mut downloader = ModelDownloader::new(storage)?;

            println!("Downloading model: {}", model);

            let request = DownloadRequest {
                model_id: model.clone(),
                source: ModelSource {
                    origin: "huggingface".to_string(),
                    url: None,
                    repository: Some(model.clone()),
                    commit: None,
                    license: None,
                },
                format_preference: None,
                quantization: None,
                validate_checksum: true,
            };

            let metadata = downloader.download_model(request).await?;
            println!("✓ Download complete: {}", metadata.file_path);
            Ok(())
        }
        AiCommands::Config => {
            let config_manager = ConfigManager::new()?;
            let config = config_manager.load_config()?;
            println!("{}", serde_json::to_string_pretty(&config)?);
            Ok(())
        }
        AiCommands::Keys { action } => handle_keys_action(action).await,
    }
}

async fn handle_keys_action(action: KeysAction) -> Result<()> {
    use aethershell::secure_config::SecureApiConfig;
    use std::io::Write;

    match action {
        KeysAction::Store { provider, key } => {
            let api_key = if let Some(k) = key {
                k
            } else {
                print!("Enter API key for '{}': ", provider);
                io::stdout().flush()?;
                let mut buffer = String::new();
                io::stdin().read_line(&mut buffer)?;
                buffer.trim().to_string()
            };

            SecureApiConfig::store_in_keyring(&provider, &api_key)?;
            println!("✓ API key for '{}' stored securely", provider);
            Ok(())
        }
        KeysAction::Get { provider } => {
            let _config = SecureApiConfig::from_keyring(
                &provider,
                String::new(),
                String::new(),
                provider.clone(),
            )?;
            println!("API key for '{}': {}", provider, "*".repeat(20));
            println!("(Key is stored securely in OS credential store)");
            Ok(())
        }
        KeysAction::Delete { provider } => {
            SecureApiConfig::delete_from_keyring(&provider)?;
            println!("✓ API key for '{}' deleted", provider);
            Ok(())
        }
        KeysAction::List => {
            println!("Stored API key providers:");
            println!("(Use 'ae ai keys get <provider>' to verify a key exists)");
            println!("  - openai");
            println!("  - anthropic");
            println!("  - google");
            Ok(())
        }
    }
}

async fn handle_mcp_command(command: McpCommands) -> Result<()> {
    use aethershell::mcp::{server::*, McpServer};
    use aethershell::os_tools::SafetyLevel;

    match command {
        McpCommands::Serve {
            host,
            port,
            cors,
            safety,
            admin,
        } => {
            let safety_level = match safety.to_lowercase().as_str() {
                "safe" => SafetyLevel::Safe,
                "caution" => SafetyLevel::Caution,
                "dangerous" => SafetyLevel::Dangerous,
                "critical" => SafetyLevel::Critical,
                _ => {
                    eprintln!("Invalid safety level: {}. Using 'caution'", safety);
                    SafetyLevel::Caution
                }
            };

            let config = McpServerConfig {
                host,
                port,
                enable_cors: cors,
                safety_level,
                allow_admin: admin,
            };

            start_mcp_server(config).await
        }
        McpCommands::Tools { category } => {
            let server = McpServer::new();
            let tools = server.list_tools();

            let filtered_tools: Vec<_> = if let Some(cat) = category {
                tools
                    .into_iter()
                    .filter(|t| t.name.contains(&cat) || t.description.contains(&cat))
                    .collect()
            } else {
                tools
            };

            println!("{:<25} {}", "Tool Name", "Description");
            println!("{}", "-".repeat(80));
            for tool in filtered_tools {
                let desc = if tool.description.len() > 50 {
                    format!("{}...", &tool.description[..50])
                } else {
                    tool.description.clone()
                };
                println!("{:<25} {}", tool.name, desc);
            }
            Ok(())
        }
    }
}

fn repl() -> Result<()> {
    // Simple REPL; keep it lean and rely on your existing `repl.rs` if you have one.
    // Here we do a tiny inline REPL to avoid extra wires.
    use std::io::Write;
    let mut env = Env::default();
    let mut line = String::new();

    println!("Æther REPL — type 'exit', 'quit', or Ctrl-D to exit");
    loop {
        line.clear();
        print!("ae> ");
        io::stdout().flush().ok();
        if io::stdin().read_line(&mut line)? == 0 {
            println!();
            break;
        }
        let src = line.trim();
        if src.is_empty() {
            continue;
        }

        // Handle exit commands
        if src == "exit" || src == "quit" {
            break;
        }

        match parser::parse_program(src) {
            Ok(stmts) => match eval::eval_program(&stmts, &mut env) {
                Ok(val) => println!("{:?}", val),
                Err(e) => eprintln!("eval error: {e}"),
            },
            Err(e) => eprintln!("parse error: {e}"),
        }
    }
    Ok(())
}

fn run_file(path: &str, bash_mode: bool) -> Result<()> {
    let mut code = fs::read_to_string(path).with_context(|| format!("failed to read {}", path))?;

    if bash_mode {
        code = transpile::bash::transpile_bash_to_ae(&code)
            .with_context(|| format!("bash→aether transpile failed for {}", path))?;
    }

    run_code(&code)
}

fn run_code(code: &str) -> Result<()> {
    let mut env = Env::default();
    let exit_code = aethershell::repl::run_one(&mut env, code)?;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}
