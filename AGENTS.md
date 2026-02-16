# AGENTS.md - AetherShell Agent & AI Discovery

> This file helps AI coding assistants (GitHub Copilot, Claude, ChatGPT, Cursor, Windsurf, Cody, local models) understand how to work with AetherShell.

## Identity

- **Project**: AetherShell
- **Binary**: `ae`
- **Language**: Rust
- **Version**: 0.3.1
- **Repository**: https://github.com/nervosys/AetherShell
- **License**: AGPL-3.0-or-later
- **File Extension**: `.ae`

## What Is AetherShell?

AetherShell is a typed, functional shell where data flows as structured values (Int, Float, String, Array, Record, Lambda) through pipelines - not raw text. It has 215+ builtins in 38 modules, native AI agents with multi-modal support, and implements the MCP, A2A, A2UI, and NANDA agentic protocols.

## How To Use AetherShell (for AI agents)

### Option 1: Generate AetherShell Code

Write `.ae` scripts or REPL expressions:

```ae
# Typed pipeline
ls("./src") | where(fn(f) => f.size > 1024) | map(fn(f) => f.name)

# AI query
ai("Explain this code", {context: file.read("main.rs")})

# File editing (safe, cross-platform)
file.replace("config.rs", "DEBUG = false", "DEBUG = true")
```

### Option 2: Call the Agent API (HTTP)

Start: `ae agent serve` (port 3002)

```bash
# Call a builtin
curl -X POST http://localhost:3002/api/v1/execute \
  -H "Content-Type: application/json" \
  -d '{"action": "call", "builtin": "ls", "args": {"path": "."}}'

# Get schema for your AI provider
curl http://localhost:3002/api/v1/schema/openai
```

### Option 3: Python SDK + LangChain

```python
from aethershell import AetherRuntime
from aethershell.langchain import get_agent_api_tools

runtime = AetherRuntime()
result = runtime.eval('sys.hostname()')

# LangChain tools (connect to Agent API server)
tools = get_agent_api_tools("http://localhost:3002")
```

## AetherShell Syntax Quick Reference

```
# Variables (inferred types)
x = 42                              # Int
s = "hello"                         # String
a = [1, 2, 3]                       # Array<Int>
r = {name: "ae", version: "0.3.1"} # Record

# Lambdas
double = fn(x) => x * 2
add = fn(a, b) => a + b

# Pipelines
[1,2,3] | map(fn(x) => x * 2) | reduce(fn(a,b) => a + b, 0)

# Pattern matching
match score { 90..100 => "A", 80..89 => "B", _ => "F" }

# Error handling
result = try { risky() } catch e { "fallback" }

# String interpolation
msg = "Hello ${name}, you have ${count} items"

# Module calls
file.read("path")     sys.hostname()     net.ping("host")
crypto.uuid()         math.sqrt(16)      arr.range(10)
```

## Module Directory (38 modules)

| Module | Purpose | Example |
|--------|---------|---------|
| `file` | File I/O | `file.read("f.txt")`, `file.write("f.txt", data)` |
| `sys` | System info | `sys.hostname()`, `sys.uptime()`, `sys.cpu_info()` |
| `proc` | Processes | `proc.list()`, `proc.kill(pid)` |
| `net` | Network | `net.ping("host")`, `net.dns_lookup("host")` |
| `http` | HTTP client | `http.get(url)`, `http.post(url, body)` |
| `crypto` | Cryptography | `crypto.uuid()`, `crypto.hash("sha256", data)` |
| `db` | Database | `db.sqlite_open("db")`, `db.sqlite_query(c, sql)` |
| `math` | Mathematics | `math.sqrt(x)`, `math.pow(a, b)` |
| `str` | Strings | `str.upper(s)`, `str.split(s, ",")` |
| `arr` | Arrays | `arr.range(n)`, `arr.flatten(a)`, `arr.unique(a)` |
| `json` | JSON | `json.parse(s)`, `json.stringify(v)` |
| `platform` | Platform | `platform.os()`, `platform.arch()`, `platform.gpus()` |
| `ai` | AI queries | `ai("prompt")`, `ai("model:name", "prompt")` |
| `agent` | Agents | `agent("goal", tools)`, `swarm({...})` |
| `mcp` | MCP protocol | `mcp.tools()`, `mcp.call("tool", args)` |
| `a2a` | Agent-to-Agent | `a2a.send("agent", msg)` |
| `a2ui` | Agent-to-UI | `a2ui.notify("msg", "type")` |
| `nanda` | Consensus | `nanda.propose("id", config)` |
| `rbac` | Access control | `rbac.create("role", perms)` |
| `audit` | Audit logging | `audit.log("action", "target", meta)` |
| `sso` | Single sign-on | `sso.init("provider", config)` |
| `nn` | Neural nets | `nn.create("name", layers)` |
| `evo` | Evolution | `evo.population(n, type, config)` |
| `rl` | Reinforcement | `rl.agent("type", states, actions, config)` |
| `fs` | Filesystem | Filesystem operations |
| `gui` | GUI | GUI operations |
| `web` | Web | Web operations |
| `svc` | Services | Service management |
| `cron` | Scheduling | Cron/schedule operations |
| `archive` | Archives | Compression/extraction |
| `user` | Users | User management |
| `perm` | Permissions | Permission operations |
| `pkg` | Packages | Package management |
| `hw` | Hardware | Hardware access |
| `clip` | Clipboard | Clipboard operations |
| `input` | Input | User input |
| `shell` | Shell | Shell operations |
| `cluster` | Cluster | Distributed computing |

## Agent API Server Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/v1/execute` | Execute any AgentRequest |
| `POST` | `/api/v1/call/:builtin` | Call a single builtin |
| `POST` | `/api/v1/pipeline` | Execute a pipeline |
| `POST` | `/api/v1/eval` | Evaluate AetherShell code |
| `POST` | `/api/v1/stream/execute` | Stream execution (SSE) |
| `GET` | `/api/v1/ws` | WebSocket connection |
| `GET` | `/api/v1/schema` | Full JSON schema |
| `GET` | `/api/v1/schema/:format` | Provider-specific schema |
| `GET` | `/api/v1/builtins` | List builtins |
| `GET` | `/api/v1/builtins/:name` | Describe a builtin |
| `GET` | `/api/v1/types` | Type information |
| `GET` | `/health` | Health check |

Schema formats: `openai`, `claude`, `gemini`, `llama`, `mistral`, `cohere`, `grok`, `deepseek`, `bedrock`, `azure_openai`, `qwen`, `ollama`, `vllm`, `huggingface`, `openrouter`, `together`, `groq`, `fireworks`, `ontology`

## Development

```bash
cargo build --bins          # Build ae + aimodel
cargo test                  # Run 1,169 tests
cargo run -- --tui          # TUI mode
cargo run -- -c 'expr'      # Quick eval
```

### Key Source Files

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI entry point |
| `src/ast.rs` | AST definitions |
| `src/parser.rs` | Parser |
| `src/eval.rs` | Evaluator |
| `src/value.rs` | Value types |
| `src/builtins.rs` | 215+ builtins |
| `src/ai.rs` | AI provider router |
| `src/agent.rs` | Agent framework |
| `src/agent_api.rs` | Agent API + HTTP server |
| `src/typecheck.rs` | Hindley-Milner inference |
| `src/tui/` | Terminal UI |

## Context Files

| File | Purpose |
|------|---------|
| `llms.txt` | Short AI context (llms.txt standard) |
| `llms-full.txt` | Complete AI context |
| `AGENTS.md` | This file - agent discovery |
| `.github/copilot-instructions.md` | GitHub Copilot instructions |
| `.well-known/ai-plugin.json` | OpenAI plugin manifest |
| `.well-known/openapi.yaml` | OpenAPI 3.1 spec for Agent API |
| `docs/specs/SPEC.md` | Language specification |
| `ROADMAP.md` | Development roadmap |