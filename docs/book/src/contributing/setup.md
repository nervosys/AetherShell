# Development Setup

How to set up a development environment for contributing to AetherShell.

## Prerequisites

- **Rust** 1.75+ (install via [rustup](https://rustup.rs))
- **Git**
- **pkg-config** and **OpenSSL dev headers** (Linux)
- Optional: **Ollama** for local AI model testing

### Platform-Specific

**Ubuntu/Debian:**
```bash
sudo apt install build-essential pkg-config libssl-dev
```

**macOS:**
```bash
brew install openssl pkg-config
```

**Windows:**
```powershell
# Rust includes MSVC build tools; ensure Visual Studio C++ Build Tools are installed
```

## Clone and Build

```bash
git clone https://github.com/nervosys/AetherShell.git
cd AetherShell
cargo build --bins
```

This produces two binaries:
- `target/debug/ae` — the main shell
- `target/debug/aimodel` — the AI model management CLI

## Run Tests

```bash
cargo test                    # All tests (~272)
cargo test --lib              # Library unit tests
cargo test --test eval        # Specific test file
cargo test pipeline           # Tests matching "pipeline"
```

## Launch for Development

```bash
# REPL mode
cargo run

# TUI mode
cargo run -- --tui

# Execute a script
cargo run -- examples/00_hello.ae

# AI model server
cargo run --bin aimodel -- serve --port 8080
```

## Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `AETHER_AI` | Default AI provider | `openai` |
| `OPENAI_API_KEY` | OpenAI API key | `sk-...` |
| `OLLAMA_HOST` | Ollama server URL | `http://localhost:11434` |
| `AGENT_ALLOW_CMDS` | Agent tool allowlist | `ls,git,cat` |
| `AETHER_LOG` | Log level | `debug` |

## IDE Setup

### VS Code

Install the **AetherShell** extension from the marketplace, or load it from source:

```bash
cd vscode-extension
npm install
npm run compile
# Press F5 to launch Extension Development Host
```

Recommended extensions:
- **rust-analyzer** — Rust language support
- **Even Better TOML** — Cargo.toml editing
- **CodeLLDB** — Debugger

### Configuration

`.vscode/settings.json`:
```json
{
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.check.command": "clippy",
  "editor.formatOnSave": true
}
```

## Project Layout

| Directory | Contents |
|-----------|----------|
| `src/` | Core shell source |
| `src/ai_api/` | AI model server |
| `src/tui/` | Terminal UI |
| `src/transpile/` | Bash transpiler |
| `src/bin/` | Additional binaries |
| `tests/` | Integration tests |
| `test-scripts/` | Script-based tests |
| `examples/` | Example `.ae` scripts |
| `docs/book/` | mdBook documentation |
| `web/` | Web dashboard & WASM |
| `vscode-extension/` | VS Code extension |
