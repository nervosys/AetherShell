# FAQ

## General

### What is AetherShell?

AetherShell is a next-generation shell written in Rust that combines typed functional programming with multimodal AI capabilities. Unlike traditional shells that pass raw text between commands, AetherShell uses structured data types (records, arrays, tables) throughout its pipeline system.

### How is AetherShell different from Bash/Zsh?

| Feature | Bash/Zsh | AetherShell |
|---------|----------|-------------|
| Data model | Raw text | Typed values (Int, Record, Array, ...) |
| Pipelines | Text streams | Structured data flow |
| Functions | String-based | Typed lambdas with inference |
| AI | None | Built-in LLM, agents, RAG |
| Pattern matching | Case statements | `match` expressions |
| Error handling | Exit codes | Result types with context |

### Can I use AetherShell as my daily driver?

AetherShell is under active development. It's excellent for data processing, AI automation, and scripting. For interactive daily use, you may want to keep your current shell available while adopting AetherShell incrementally.

### How do I run Bash commands in AetherShell?

Use the `sh` builtin:

```aethershell
sh "git status"
sh "docker ps"
```

## Language

### Why typed pipelines?

Typed pipelines eliminate an entire class of bugs. When `ls` returns an array of records with known fields, `where`, `map`, and `sort_by` can validate their arguments at parse time rather than failing silently at runtime.

### Does AetherShell support loops?

AetherShell favors functional iteration via `map`, `each`, `reduce`, and `where`. Recursion is supported for looping patterns:

```aethershell
let countdown = fn(n) => if n > 0 { echo n; countdown(n - 1) } else { echo "done" }
countdown 5
```

### What is `fn(x) => expr`?

This is a lambda (anonymous function). Lambdas are first-class values — they can be stored in variables, passed to builtins, and returned from functions:

```aethershell
let double = fn(x) => x * 2
[1,2,3] | map(double)   # [2, 4, 6]
```

## AI

### Which AI providers are supported?

AetherShell supports six providers via model URIs:

- `openai:gpt-4o` — OpenAI
- `ollama:llama3` — Local Ollama
- `compat:mixtral` — OpenAI-compatible APIs
- `tgi:model` — HuggingFace Text Generation Inference
- `vllm:model` — vLLM serving
- `llamacpp:model` — llama.cpp server

### Do I need an API key?

For cloud providers (OpenAI), yes — set `OPENAI_API_KEY`. For local providers (Ollama, llama.cpp), no API key is needed.

### How do agents work?

Agents use a ReAct (Reason + Act) loop: they think about the task, choose a tool, execute it, observe the result, and repeat until the goal is met:

```aethershell
agent {
  goal: "Find the largest file in the current directory",
  tools: ["ls", "sort_by"],
  max_steps: 5
}
```

### What is a swarm?

A swarm is a group of AI agents that collaborate on a task. A coordinator routes subtasks to specialized agents, and they share state via a blackboard:

```aethershell
swarm {
  goal: "Analyze this project",
  tools: ["ls", "cat", "grep"],
  max_steps: 20
}
```

## TUI

### How do I launch the TUI?

```bash
ae --tui
```

### What are the TUI tabs?

1. **Chat** — AI conversation interface
2. **Agent Swarm** — Monitor multi-agent collaboration
3. **Media Browser** — View images, audio, video
4. **Settings** — Configure providers and appearance
5. **Distributed** — Manage cluster nodes
6. **Reasoning** — Advanced chain-of-thought display
7. **Search** — Search chat history

### How do I switch tabs?

Press `Tab` and `Shift+Tab` to cycle through tabs, or press `1`–`7` in Normal mode.

## Troubleshooting

### `cargo build` fails with OpenSSL errors

Install OpenSSL development headers:

```bash
# Ubuntu/Debian
sudo apt install libssl-dev pkg-config

# macOS
brew install openssl
```

### AI commands return "no provider configured"

Set the provider environment variable:

```bash
export AETHER_AI=openai
export OPENAI_API_KEY=sk-...
```

### Tests fail with "connection refused"

Some tests require a running Ollama instance or API keys. Tests that need external services will skip gracefully if the required environment is not available.

### TUI looks broken

Ensure your terminal supports 256 colors and Unicode. Recommended terminals: **Windows Terminal**, **iTerm2**, **Alacritty**, **kitty**.
