use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::io::{self, Read};

use aethershell::{env::Env, eval, modules, parser, transpile};
use clap::{Parser, Subcommand};

/// Create an environment with all builtin modules pre-registered
fn create_env_with_modules() -> Env {
    let mut env = Env::new();
    for (name, module) in modules::all_modules() {
        env.register_module(name, module);
    }
    env
}

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

    /// Zsh compatibility mode
    #[arg(long, short = 'z')]
    zsh: bool,

    /// PowerShell compatibility mode
    #[arg(long, short = 'p')]
    pwsh: bool,

    /// Agentic syntax mode (token-minimized for AI agents)
    #[arg(long, short = 'a')]
    agentic: bool,

    /// Execute a command string
    #[arg(long, short = 'c')]
    command: Option<String>,

    /// Token budget for output: results over N estimated tokens are paged/truncated
    #[arg(long, value_name = "N")]
    budget: Option<usize>,

    /// Agent mode: default-deny dangerous effect classes (gate behind approval)
    #[arg(long)]
    agent: bool,

    /// Workspace root that confines destructive/write operations (the jail)
    #[arg(long, value_name = "DIR")]
    workspace: Option<String>,

    /// Safety policy: "strict" (default) or "permissive" (allow-all in agent mode)
    #[arg(long, value_name = "POLICY")]
    policy: Option<String>,

    /// Deterministic output: render results as canonical, byte-stable JSON
    /// (sorted keys, stable floats) for snapshot tests, caching, and diffs
    #[arg(long)]
    deterministic: bool,

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

    /// AI assistant - natural language to AetherShell transpilation
    #[command(visible_alias = "ai-assist")]
    Assist {
        /// Natural language query (reads from stdin if not provided)
        query: Option<String>,

        /// Execute the generated code immediately (default: show only)
        #[arg(long, short = 'x')]
        execute: bool,

        /// Interactive mode - continuous assistant session
        #[arg(long, short = 'i')]
        interactive: bool,

        /// Context-aware mode - include system info in prompts
        #[arg(long, short = 'c')]
        context: bool,

        /// Suggest useful commands based on current directory
        #[arg(long, short = 's')]
        suggest: bool,
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
        /// deepseek, bedrock, qwen, ollama, vllm, huggingface, openrouter,
        /// kimi, yi, glm, reka, ai21, perplexity, together, groq, fireworks, json, compact
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

    // An output token budget applies to all eval/print paths via the REPL.
    if let Some(n) = cli.budget {
        std::env::set_var("AE_TOKEN_BUDGET", n.to_string());
    }
    // Safety flags set the env vars the safety layer reads (see src/safety.rs).
    if cli.agent {
        std::env::set_var("AETHER_MODE", "agent");
    }
    if let Some(ws) = &cli.workspace {
        std::env::set_var("AETHER_WORKSPACE", ws);
    }
    if let Some(p) = &cli.policy {
        std::env::set_var("AETHER_POLICY", p);
    }
    if cli.deterministic {
        std::env::set_var("AE_DETERMINISTIC", "1");
    }

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
            Commands::Assist {
                query,
                execute,
                interactive,
                context,
                suggest,
            } => handle_assist(query, execute, interactive, context, suggest),
        };
    }

    // Handle bash mode with stdin
    if cli.bash && cli.file.is_none() && cli.command.is_none() {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        let code = transpile::bash::transpile_bash_to_ae(&buf)?;
        return run_code(&code);
    }
    // Handle zsh mode with stdin
    if cli.zsh && cli.file.is_none() && cli.command.is_none() {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        let code = transpile::zsh::transpile_zsh_to_ae(&buf)?;
        return run_code(&code);
    }
    // Handle pwsh mode with stdin
    if cli.pwsh && cli.file.is_none() && cli.command.is_none() {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        let code = transpile::powershell::transpile_powershell_to_ae(&buf)?;
        return run_code(&code);
    }
    // Handle agentic mode with stdin
    if cli.agentic && cli.file.is_none() && cli.command.is_none() {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        let code = transpile::agentic::transpile_agentic_to_ae(&buf)?;
        return run_code(&code);
    }

    // Handle -c/--command flag
    if let Some(cmd) = cli.command {
        let code = if cli.bash {
            transpile::bash::transpile_bash_to_ae(&cmd)?
        } else if cli.zsh {
            transpile::zsh::transpile_zsh_to_ae(&cmd)?
        } else if cli.pwsh {
            transpile::powershell::transpile_powershell_to_ae(&cmd)?
        } else if cli.agentic {
            transpile::agentic::transpile_agentic_to_ae(&cmd)?
        } else {
            cmd
        };
        return run_code(&code);
    }

    // Determine transpile mode from flags
    let transpile_mode = if cli.bash {
        Some(TranspileMode::Bash)
    } else if cli.zsh {
        Some(TranspileMode::Zsh)
    } else if cli.pwsh {
        Some(TranspileMode::PowerShell)
    } else if cli.agentic {
        Some(TranspileMode::Agentic)
    } else {
        None
    };

    // Handle file execution or REPL
    match cli.file {
        Some(file) => run_file(&file, transpile_mode)?,
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

            println!("{:<25} Description", "Tool Name");
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
    use aethershell::agent_api::{self, AgentRequest};
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
                            println!("{:<20} {:<12} Description", "Name", "Category");
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
fn handle_assist(
    query: Option<String>,
    execute: bool,
    interactive: bool,
    context: bool,
    suggest: bool,
) -> Result<()> {
    use std::io::{BufRead, Write};

    let system_prompt = build_assist_prompt();
    let ctx = if context {
        gather_system_context()
    } else {
        String::new()
    };

    // Suggest mode: analyze cwd and suggest useful commands
    if suggest {
        let suggest_prompt = format!(
            "{}\n\n{}\n\nBased on the context above, suggest 3-5 useful AetherShell commands \
             the user could run in this directory. Format each as a one-line comment explaining \
             what it does, followed by the code on the next line. Separate suggestions with blank lines.",
            system_prompt, ctx
        );
        let suggestions = aethershell::ai::complete_sync_router(&suggest_prompt)
            .context("AI completion failed — is AETHER_AI configured?")?;
        println!("{}", clean_ai_output(&suggestions));
        return Ok(());
    }

    if interactive {
        println!("Æther Assist — natural language → AetherShell");
        println!("Type your request, press Enter. Type 'exit' or Ctrl-D to quit.");
        if context {
            println!("Context-aware mode: system info included in prompts.");
        }
        println!();

        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let mut history: Vec<String> = Vec::new();

        loop {
            print!("assist> ");
            stdout.flush().ok();

            let mut line = String::new();
            if stdin.lock().read_line(&mut line)? == 0 {
                println!();
                break;
            }

            let input = line.trim();
            if input.is_empty() {
                continue;
            }
            if input == "exit" || input == "quit" {
                break;
            }

            // Build context-aware prompt with conversation history
            let history_context = if !history.is_empty() {
                let recent: Vec<String> = history
                    .iter()
                    .rev()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                format!("\nRecent conversation:\n{}\n", recent.join("\n"))
            } else {
                String::new()
            };

            match assist_once(&system_prompt, &ctx, &history_context, input, execute) {
                Ok(code) => {
                    history.push(format!("User: {}", input));
                    history.push(format!("Assistant: {}", code));
                }
                Err(e) => eprintln!("error: {e}"),
            }
            println!();
        }
        return Ok(());
    }

    // Single query mode
    let input = if let Some(q) = query {
        q
    } else {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        buf
    };

    assist_once(&system_prompt, &ctx, "", input.trim(), execute)?;
    Ok(())
}
/// Build the comprehensive system prompt for NL -> AetherShell transpilation
fn build_assist_prompt() -> String {
    "You are an AetherShell assistant. Convert natural language into AetherShell code.\n\n\
AetherShell is a typed functional shell where data flows as structured values through pipelines.\n\n\
## Core Syntax\n\
- Variables: x = 42, name = \"hello\", items = [1,2,3], config = {key: \"val\"}\n\
- Lambdas: fn(x) => x * 2, fn(a, b) => a + b\n\
- Pipelines: data | where(fn(x) => x > 0) | map(fn(x) => x * 2) | take(5)\n\
- Pattern matching: match val { 1 => \"one\", 2 => \"two\", _ => \"other\" }\n\
- Error handling: try { risky() } catch e { fallback }\n\
- String interpolation: \"Hello ${name}, you have ${count} items\"\n\
- Async: await http.get(url)\n\n\
## Module System (38 modules, 215+ builtins)\n\
### File & System\n\
- file.read(path), file.write(path, data), file.exists(path), file.copy(src, dst)\n\
- file.replace(path, old, new), file.patch(path, line_start, line_end, content)\n\
- file.insert(path, {after: pattern}, content), file.backup(path)\n\
- ls(path), cd(path), pwd(), cat(path), mkdir(path), rm(path)\n\
- sys.hostname(), sys.uptime(), sys.cpu_info(), sys.mem_info()\n\
- proc.list(), proc.kill(pid)\n\
- platform.os(), platform.arch(), platform.gpus(), platform.hardware_summary()\n\n\
### Data & Text\n\
- str.upper(s), str.lower(s), str.split(s, delim), str.trim(s), str.replace(s, old, new)\n\
- arr.range(n), arr.flatten(a), arr.unique(a), arr.sort(a), arr.reverse(a)\n\
- json.parse(s), json.stringify(v)\n\
- math.sqrt(x), math.pow(a, b), math.abs(x), math.ceil(x), math.floor(x)\n\n\
### Pipeline Operations\n\
- map(fn), where(fn), reduce(fn, init), sort(fn), reverse()\n\
- take(n), skip(n), head(n), tail(n), flatten(), unique()\n\
- select(field1, field2), get(key), keys(), values()\n\
- length(), enumerate(), zip(other)\n\n\
### Network & HTTP\n\
- net.ping(host), net.dns_lookup(host), net.interfaces()\n\
- http.get(url), http.post(url, body), http.put(url, body), http.delete(url)\n\n\
### Crypto & Database\n\
- crypto.uuid(), crypto.hash(algo, data), crypto.jwt_decode(token)\n\
- db.sqlite_open(path), db.sqlite_query(conn, sql)\n\n\
### AI & Agents\n\
- ai(prompt), ai(prompt, {context: data}), ai(prompt, {images: [path]})\n\
- agent(goal, tools), agent(goal, tools, {model: \"openai:gpt-4o\", max_steps: 10})\n\
- swarm({coordinator: goal, agents: [...], tools: [...]})\n\n\
### Protocols\n\
- mcp.tools(), mcp.call(tool, args)\n\
- a2a.send(agent, msg)\n\
- a2ui.notify(msg, level), a2ui.progress(label, fraction)\n\n\
### Enterprise\n\
- rbac.create(role, perms), rbac.grant(user, role)\n\
- audit.log(action, target, meta)\n\
- sso.init(provider, config)\n\n\
### Other Modules\n\
- shell.exec(cmd), env(key), which(cmd)\n\
- clip.read(), clip.write(text)\n\
- archive.compress(path, format), archive.extract(path)\n\
- cron.schedule(expr, cmd), svc.list(), pkg.list()\n\n\
## AI Model URIs\n\
- openai:gpt-4o, openai:gpt-4o-mini\n\
- ollama:llama3, ollama:codellama\n\
- anthropic:claude-3-opus\n\n\
RULES:\n\
1. Output ONLY valid AetherShell code — no markdown fences, no explanations.\n\
2. Use pipelines for data transformation chains.\n\
3. Prefer module builtins (file.read, sys.hostname) over raw shell commands.\n\
4. One complete program per response.\n\
5. Use the most appropriate module for each task.".to_string()
}
/// Gather system context for context-aware suggestions
fn gather_system_context() -> String {
    let mut ctx = String::from("## System Context\n");

    // Working directory
    if let Ok(cwd) = std::env::current_dir() {
        ctx.push_str(&format!("- Working directory: {}\n", cwd.display()));

        // List files in cwd (first 20)
        if let Ok(entries) = std::fs::read_dir(&cwd) {
            let mut files: Vec<String> = Vec::new();
            for entry in entries.flatten().take(20) {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir {
                    files.push(format!("{}/", name));
                } else {
                    files.push(name);
                }
            }
            ctx.push_str(&format!("- Directory contents: {}\n", files.join(", ")));
        }
    }

    // OS and platform
    ctx.push_str(&format!("- OS: {}\n", std::env::consts::OS));
    ctx.push_str(&format!("- Arch: {}\n", std::env::consts::ARCH));

    // Detect project type from files
    let project_indicators = [
        ("Cargo.toml", "Rust project"),
        ("package.json", "Node.js project"),
        ("pyproject.toml", "Python project"),
        ("go.mod", "Go project"),
        ("pom.xml", "Java/Maven project"),
        ("build.gradle", "Java/Gradle project"),
        ("Makefile", "Make-based project"),
        ("Dockerfile", "Docker containerized"),
        (".git", "Git repository"),
        ("docker-compose.yml", "Docker Compose setup"),
    ];

    let mut detected: Vec<&str> = Vec::new();
    for (file, label) in &project_indicators {
        if std::path::Path::new(file).exists() {
            detected.push(label);
        }
    }
    if !detected.is_empty() {
        ctx.push_str(&format!("- Project type: {}\n", detected.join(", ")));
    }

    // Relevant env vars (without values for security)
    let relevant_vars = [
        "AETHER_AI",
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "OLLAMA_HOST",
        "EDITOR",
        "SHELL",
        "TERM",
    ];
    let set_vars: Vec<&str> = relevant_vars
        .iter()
        .filter(|v| std::env::var(v).is_ok())
        .copied()
        .collect();
    if !set_vars.is_empty() {
        ctx.push_str(&format!("- Available env vars: {}\n", set_vars.join(", ")));
    }

    ctx
}
/// Clean AI output: strip markdown fences and whitespace
fn clean_ai_output(raw: &str) -> String {
    let cleaned = raw.trim();
    let cleaned = cleaned
        .strip_prefix("```aethershell")
        .or_else(|| cleaned.strip_prefix("```ae"))
        .or_else(|| cleaned.strip_prefix("```"))
        .unwrap_or(cleaned);
    let cleaned = cleaned.strip_suffix("```").unwrap_or(cleaned).trim();
    cleaned.to_string()
}

fn assist_once(
    system_prompt: &str,
    system_context: &str,
    history: &str,
    query: &str,
    execute: bool,
) -> Result<String> {
    use aethershell::ai;

    let prompt = format!(
        "{}\n\n{}{}\nUser request: {}",
        system_prompt, system_context, history, query
    );
    let code = ai::complete_sync_router(&prompt)
        .context("AI completion failed — is AETHER_AI configured?")?;

    let cleaned = clean_ai_output(&code);

    if execute {
        println!("# Generated code:");
        for line in cleaned.lines() {
            println!("  {}", line);
        }
        println!("# Running...\n");
        run_code(&cleaned)?;
    } else {
        println!("{}", cleaned);
    }
    Ok(cleaned.to_string())
}
fn repl() -> Result<()> {
    // Simple REPL; keep it lean and rely on your existing `repl.rs` if you have one.
    // Here we do a tiny inline REPL to avoid extra wires.
    use std::io::Write;
    let mut env = create_env_with_modules();
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

/// Transpilation mode for legacy shell compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TranspileMode {
    Bash,
    Zsh,
    PowerShell,
    Agentic,
}

/// Detect transpile mode from file extension (when no explicit flag is given).
fn detect_transpile_mode(path: &str) -> Option<TranspileMode> {
    let lower = path.to_lowercase();
    if lower.ends_with(".sh") || lower.ends_with(".bash") {
        Some(TranspileMode::Bash)
    } else if lower.ends_with(".zsh") {
        Some(TranspileMode::Zsh)
    } else if lower.ends_with(".ps1") || lower.ends_with(".psm1") || lower.ends_with(".psd1") {
        Some(TranspileMode::PowerShell)
    } else if lower.ends_with(".aeg") {
        Some(TranspileMode::Agentic)
    } else {
        None
    }
}

fn run_file(path: &str, explicit_mode: Option<TranspileMode>) -> Result<()> {
    let mut code = fs::read_to_string(path).with_context(|| format!("failed to read {}", path))?;

    // Explicit flag takes priority; otherwise auto-detect from extension
    let mode = explicit_mode.or_else(|| detect_transpile_mode(path));

    if let Some(m) = mode {
        code = match m {
            TranspileMode::Bash => transpile::bash::transpile_bash_to_ae(&code)
                .with_context(|| format!("bash\u{2192}aether transpile failed for {}", path))?,
            TranspileMode::Zsh => transpile::zsh::transpile_zsh_to_ae(&code)
                .with_context(|| format!("zsh\u{2192}aether transpile failed for {}", path))?,
            TranspileMode::PowerShell => transpile::powershell::transpile_powershell_to_ae(&code)
                .with_context(|| {
                format!("powershell\u{2192}aether transpile failed for {}", path)
            })?,
            TranspileMode::Agentic => transpile::agentic::transpile_agentic_to_ae(&code)
                .with_context(|| format!("agentic\u{2192}aether transpile failed for {}", path))?,
        };
    }

    run_code(&code)
}

fn run_code(code: &str) -> Result<()> {
    let mut env = create_env_with_modules();
    let exit_code = aethershell::repl::run_one(&mut env, code)?;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}
