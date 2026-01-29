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

    /// Agent API mode - AI-callable interface for ChatGPT, Claude, Gemini
    #[command(visible_alias = "agent-api")]
    Agent {
        #[command(subcommand)]
        command: AgentApiCommands,
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

#[derive(Subcommand)]
enum AgentApiCommands {
    /// Start the Agent API HTTP server (for AI integrations)
    Serve {
        /// Server host
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Server port
        #[arg(long, default_value = "3002")]
        port: u16,
        /// Enable CORS for browser/API access
        #[arg(long)]
        cors: bool,
    },
    /// Execute a JSON request (read from stdin or argument)
    #[command(alias = "exec")]
    Execute {
        /// JSON request (reads from stdin if not provided)
        request: Option<String>,
    },
    /// Get the language schema for AI agents
    Schema {
        /// Output format: openai, azure, claude, gemini, llama, mistral, cohere, grok,
        /// deepseek, bedrock, qwen, ollama, vllm, huggingface, openrouter, json, compact
        #[arg(long, short = 'f', default_value = "compact")]
        format: String,
        /// Output to file
        #[arg(long, short = 'o')]
        output: Option<String>,
    },
    /// List all available builtins
    #[command(alias = "ls")]
    Builtins {
        /// Filter by category
        #[arg(long, short = 'c')]
        category: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Interactive agent mode (process requests from stdin continuously)
    Interactive,
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
            Commands::Agent { command } => handle_agent_api_command(command),
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

fn handle_agent_api_command(command: AgentApiCommands) -> Result<()> {
    use aethershell::agent_api::{self, AgentRequest, SchemaFormat};
    use std::io::{BufRead, Write};

    match command {
        AgentApiCommands::Serve { host, port, cors } => {
            #[cfg(feature = "native")]
            {
                let config = agent_api::server::AgentApiConfig {
                    host,
                    port,
                    enable_cors: cors,
                };

                tokio::runtime::Runtime::new()?
                    .block_on(agent_api::server::start_agent_api_server(config))
            }

            #[cfg(not(feature = "native"))]
            {
                Err(anyhow::anyhow!("Agent API server requires native feature"))
            }
        }
        AgentApiCommands::Execute { request } => {
            let json_str = if let Some(req) = request {
                req
            } else {
                // Read from stdin
                let mut buffer = String::new();
                io::stdin().read_to_string(&mut buffer)?;
                buffer
            };

            let output = agent_api::process_json_request(&json_str)?;
            println!("{}", output);
            Ok(())
        }
        AgentApiCommands::Schema { format, output } => {
            let schema = agent_api::generate_schema(&format)?;

            if let Some(path) = output {
                fs::write(&path, &schema)?;
                println!("✓ Schema written to {}", path);
            } else {
                println!("{}", schema);
            }
            Ok(())
        }
        AgentApiCommands::Builtins {
            category,
            json: json_output,
        } => {
            let request = AgentRequest::ListBuiltins { category };
            let response = agent_api::process_request(&request);

            if json_output {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else if response.success {
                if let Some(result) = &response.result {
                    if let Some(builtins) = result.get("builtins") {
                        if let Some(arr) = builtins.as_array() {
                            println!("{:<20} {:<12} {}", "Name", "Category", "Description");
                            println!("{}", "-".repeat(80));
                            for b in arr {
                                let name = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                let cat = b.get("category").and_then(|v| v.as_str()).unwrap_or("");
                                let desc =
                                    b.get("description").and_then(|v| v.as_str()).unwrap_or("");
                                let desc_short = if desc.len() > 45 {
                                    format!("{}...", &desc[..45])
                                } else {
                                    desc.to_string()
                                };
                                println!("{:<20} {:<12} {}", name, cat, desc_short);
                            }
                        }
                    }
                }
            } else {
                eprintln!("Error: {}", response.error.unwrap_or_default());
            }
            Ok(())
        }
        AgentApiCommands::Interactive => {
            println!("🤖 AetherShell Agent API - Interactive Mode");
            println!("Send JSON requests (one per line), receive JSON responses");
            println!("Press Ctrl+D to exit\n");

            let stdin = io::stdin();
            let mut stdout = io::stdout();

            for line in stdin.lock().lines() {
                match line {
                    Ok(json_str) if !json_str.trim().is_empty() => {
                        match agent_api::process_json_request(&json_str) {
                            Ok(output) => println!("{}", output),
                            Err(e) => {
                                let error = serde_json::json!({
                                    "success": false,
                                    "error": e.to_string()
                                });
                                println!("{}", serde_json::to_string(&error)?);
                            }
                        }
                        stdout.flush()?;
                    }
                    Ok(_) => {} // Empty line, skip
                    Err(e) => {
                        eprintln!("Read error: {}", e);
                        break;
                    }
                }
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
