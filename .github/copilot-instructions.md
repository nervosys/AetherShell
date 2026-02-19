# Copilot Instructions for AetherShell

AetherShell is a shell built for AI agents. Traditional shells (Bash, PowerShell, Zsh) produce unstructured text that varies across OS versions, locales, and tool installations — making agent workflows fundamentally brittle. AetherShell eliminates this by providing a single cross-platform language where every command returns deterministic, typed output. An ontology built into the shell makes commands, arguments, and return types machine-discoverable without documentation scraping or prompt engineering. Written in Rust, it combines typed functional pipelines with multimodal AI capabilities.

## Core Architecture

### Language Design

- **Functional-first shell**: AST-based evaluation with Hindley-Milner type inference (`typecheck.rs`)
- **Typed pipelines**: Data flows as structured `Value` types (Int, Float, String, Array, Record, Lambda) not raw text
- **Expression-oriented**: Everything is an expression returning a `Value`, evaluated in `eval.rs`
- **Lambda syntax**: `fn(x) => x * 2` for functional programming patterns

### Key Module Boundaries

- **`ast.rs`**: Core AST definitions (Stmt, Expr, BinOp) - modify here for language extensions
- **`eval.rs`**: Expression evaluator - handles all runtime semantics
- **`parser.rs`**: AetherShell syntax → AST (distinct from bash transpilation)
- **`value.rs`**: Runtime value system with Record/Array/Lambda types
- **`builtins.rs`**: Shell builtins that return structured data (ls → Table, not text)
- **`transpile/bash.rs`**: Bash compatibility layer - separate from native AetherShell

### AI Integration Architecture

- **`ai.rs`**: Provider-agnostic LLM client with model URIs (`openai:gpt-4o-mini`, `ollama:llama3`, `lmstudio:model`)
- **`ai_api/`**: OpenRouter-style API server with local model management and format conversion
- **Multi-modal support**: Images, audio, video via `MultiModalContent` and `MultiModalMessage`
- **Agent framework**: Single agents (`agent`) and swarms with coordinator patterns
- **TUI integration**: `tui/` modules provide rich terminal interface for AI interactions

## Development Workflows

### Build & Test

```bash
cargo build --bins              # Builds both `ae` and `aimodel` binaries
cargo test                      # Comprehensive test suite including AI features
cargo run -- --tui             # Launch TUI mode for development testing
cargo run --bin aimodel        # AI model management CLI
```

### Testing Strategy

- **Unit tests**: Core language features (`tests/eval.rs`, `tests/pipeline.rs`)
- **Integration tests**: AI functionality (`tests/ai_*`), TUI components (`tests/tui_*`)
- **Manual tests**: `test-scripts/` for complex scenarios and bash compatibility
- **Smoke tests**: `tests/smoke.rs` for type inference validation

## Project-Specific Patterns

### Pipeline-First Design

```rust
// All operations return structured Values, not strings
[1,2,3] | map(fn(x) => x * 2) | reduce(fn(a,b) => a + b, 0)
ls "." | where(fn(r) => r.size > 1000) | select("name")
```

### AI Model URIs

Use the established URI scheme for model references:

- `openai:gpt-4o-mini` (OpenAI)
- `ollama:llama3` (Local Ollama)
- `lmstudio:model-name` (LM Studio)
- `compat:mixtral` (Compatibility mode)

### TUI Component Structure

- **`tui/app.rs`**: Main application state and event loop
- **`tui/ui.rs`**: Layout and rendering logic
- **`tui/events.rs`**: Input handling and key bindings
- **`tui/media.rs`**: Multimodal content display in terminal

### Value System Convention

When adding builtins, always return structured `Value` types:

```rust
// Good: Returns Value::Record for further pipeline processing
builtin_ls() -> Value::Array(vec![Value::Record(file_info)])

// Avoid: Raw string output breaks type safety
builtin_bad() -> Value::String("raw text output")
```

### Error Handling

- Use `anyhow::Result<Value>` for all evaluator functions
- Preserve context with `.context()` for debugging complex pipelines
- AI operations should gracefully degrade with informative errors

## Key Files for Common Tasks

- **Adding language features**: `ast.rs` → `parser.rs` → `eval.rs` → `typecheck.rs`
- **New builtins**: `builtins.rs` (ensure structured return types)
- **AI provider integration**: `ai_api/providers.rs`
- **TUI features**: `tui/app.rs` and corresponding UI modules
- **Bash compatibility**: `transpile/bash.rs` (keep separate from native features)
- **Zsh compatibility**: `transpile/zsh.rs` (Zsh-specific constructs, 100+ builtins)
- **PowerShell compatibility**: `transpile/powershell.rs` (cmdlet mappings, brace-based blocks)

## Environment Setup

Set these for full AI functionality:

- `AETHER_AI=openai` (default AI provider)
- `OPENAI_API_KEY=...` (or other provider keys)
- `AGENT_ALLOW_CMDS=ls,git,cat` (whitelist for agent tool use)

## AI Discoverability Files

AetherShell includes structured metadata files for AI model discovery:

- **`llms.txt`**: Short AI-readable summary following the [llms.txt standard](https://llmstxt.org)
- **`llms-full.txt`**: Complete AI context with syntax, modules, API, and architecture
- **`AGENTS.md`**: Agent discovery file for GitHub Copilot and AI assistants
- **`.well-known/ai-plugin.json`**: OpenAI ChatGPT/Codex plugin manifest
- **`.well-known/openapi.yaml`**: OpenAPI 3.1 specification for the Agent API
- **`.cursor/rules`**: Cursor IDE AI context
- **`.clinerules`**: Cline/Roo AI assistant context
- **`.windsurfrules`**: Windsurf IDE AI context

When modifying builtins, modules, or API endpoints, keep these files in sync.
