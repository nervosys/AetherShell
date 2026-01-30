# Configuration

AetherShell can be configured through environment variables and config files.

## Configuration File

The main configuration file is located at:
- Linux/macOS: `~/.config/aethershell/config.toml`
- Windows: `%APPDATA%\aethershell\config.toml`

### Example Configuration

```toml
# AetherShell Configuration

[general]
# Default shell features
history_size = 10000
auto_save_history = true
multiline_prompt = true

[ai]
# Default AI provider
default_provider = "openai"
default_model = "gpt-4o-mini"

# Response settings
max_tokens = 4096
temperature = 0.7
stream = true

[ai.providers.openai]
api_key = "${OPENAI_API_KEY}"  # Use env var
base_url = "https://api.openai.com/v1"

[ai.providers.anthropic]
api_key = "${ANTHROPIC_API_KEY}"
default_model = "claude-3-sonnet-20240229"

[ai.providers.ollama]
base_url = "http://localhost:11434"
default_model = "llama3"

[tui]
# TUI settings
theme = "catppuccin-mocha"
show_images = true
image_protocol = "kitty"  # kitty, iterm, sixel
max_image_width = 80
show_timestamps = true

[agent]
# Agent defaults
allowed_commands = ["ls", "cat", "grep", "find", "curl"]
max_tool_calls = 10
timeout_seconds = 300

[server]
# API server settings
host = "127.0.0.1"
port = 3002
enable_cors = true

[logging]
level = "info"  # debug, info, warn, error
file = "~/.local/share/aethershell/aethershell.log"
```

## Environment Variables

| Variable            | Description            | Example                      |
| ------------------- | ---------------------- | ---------------------------- |
| `AETHER_AI`         | Default AI provider    | `openai`, `claude`, `ollama` |
| `OPENAI_API_KEY`    | OpenAI API key         | `sk-...`                     |
| `ANTHROPIC_API_KEY` | Anthropic API key      | `sk-ant-...`                 |
| `GOOGLE_API_KEY`    | Google AI API key      | `AIza...`                    |
| `OLLAMA_HOST`       | Ollama server URL      | `http://localhost:11434`     |
| `AGENT_ALLOW_CMDS`  | Allowed agent commands | `ls,cat,grep`                |
| `AETHERSHELL_LOG`   | Log level              | `debug`, `info`, `warn`      |

### Setting Environment Variables

In your shell config (`.bashrc`, `.zshrc`, etc.):

```bash
export OPENAI_API_KEY="sk-your-key-here"
export AETHER_AI="openai"
export AGENT_ALLOW_CMDS="ls,cat,grep,find,curl,http_get"
```

Or in AetherShell:

```aethershell
env_set("OPENAI_API_KEY", "sk-your-key-here")
```

## AI Provider Setup

### OpenAI

```bash
export OPENAI_API_KEY="sk-..."
```

Available models: `gpt-4o`, `gpt-4o-mini`, `gpt-4-turbo`, `gpt-3.5-turbo`

### Anthropic (Claude)

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

Available models: `claude-3-opus`, `claude-3-sonnet`, `claude-3-haiku`

### Google (Gemini)

```bash
export GOOGLE_API_KEY="AIza..."
```

Available models: `gemini-pro`, `gemini-pro-vision`

### Local Models (Ollama)

1. Install Ollama: https://ollama.ai
2. Pull a model: `ollama pull llama3`
3. Use in AetherShell:

```aethershell
ai("Hello", { model: "ollama:llama3" })
```

### Multiple Providers

Use model URIs to specify the provider:

```aethershell
# OpenAI
ai("Query", { model: "openai:gpt-4o" })

# Anthropic
ai("Query", { model: "claude:claude-3-sonnet" })

# Ollama (local)
ai("Query", { model: "ollama:llama3" })

# OpenRouter
ai("Query", { model: "openrouter:anthropic/claude-3-opus" })
```

## Command-Line Options

```bash
ae --help

Options:
  -e, --eval <CODE>       Evaluate code directly
  -c, --command <CMD>     Run a single command
  --tui                   Start in TUI mode
  --no-history           Disable history
  --config <PATH>        Use alternate config file
  --log-level <LEVEL>    Set log level
  --server               Start API server mode
  --port <PORT>          API server port (default: 3002)
```

## Profile Scripts

AetherShell runs profile scripts on startup:

- `~/.config/aethershell/init.ae` - Runs on every startup
- `~/.config/aethershell/login.ae` - Runs on login shells

Example `init.ae`:

```aethershell
# Set up aliases
let ll = fn() => ls "." | sort_by("modified", "desc")
let search = fn(pattern) => grep pattern "."

# Configure AI
env_set("AETHER_AI", "openai")

# Welcome message
print("Welcome to AetherShell! 🐚")
```
