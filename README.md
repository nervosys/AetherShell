<p align="center">
  <img src="assets/banner.png" alt="Æther Shell" width="100%">
</p>

<p align="center">
  <a href="https://crates.io/crates/aethershell"><img src="https://img.shields.io/crates/v/aethershell.svg?style=flat-square&logo=rust&color=orange" alt="Crates.io"></a>
  <a href="https://github.com/nervosys/AetherShell/blob/master/LICENSE"><img src="https://img.shields.io/badge/license-Apache%202.0-blue.svg?style=flat-square" alt="License"></a>
  <a href="https://github.com/nervosys/AetherShell/stargazers"><img src="https://img.shields.io/github/stars/nervosys/AetherShell?style=flat-square&color=yellow" alt="Stars"></a>
</p>

<h3 align="center">The shell for AI agents. Typed pipelines. Multi-modal. Protocol-native.</h3>

<p align="center">
  <a href="#-quick-start">Quick Start</a> •
  <a href="#-modules">Modules</a> •
  <a href="#-ai-agents">AI Agents</a> •
  <a href="#-protocols">Protocols</a> •
  <a href="docs/TUI_GUIDE.md">TUI Guide</a>
</p>

---

## Quick Start

```bash
# Install
cargo install aethershell

# Or from source
git clone https://github.com/nervosys/AetherShell && cd AetherShell
cargo install --path . --bin ae

# Run
ae              # REPL
ae tui          # Interactive TUI
ae script.ae    # Run script
ae -c 'expr'    # Evaluate expression
```

```ae
# Typed pipelines, not text streams
ls("./src") | where(fn(f) => f.size > 1024) | take(5)

# Module system for clean APIs
file.exists("config.json")     # => {exists: true, is_file: true, ...}
sys.hostname()                 # => "my-machine"
crypto.uuid()                  # => "550e8400-e29b-41d4-a716-446655440000"

# AI with multi-modal support
ai("Explain this code", {context: file.read("main.rs")})
agent("Find bugs in src/", ["file.read", "grep"])
```

> Set `OPENAI_API_KEY` for AI features

---

## Modules

All 215+ builtins are organized into **31 namespaced modules**:

```ae
# File operations
file.read("config.toml")                    # Read file content
file.write("out.txt", "hello")              # Write => {success: true, bytes: 5}
file.exists("path")                         # Check => {exists: bool, is_file: bool, is_dir: bool}
file.copy("src", "dst")                     # Copy file or directory
file.move("old", "new")                     # Move/rename
file.backup("file.txt")                     # Create file.txt.bak
file.patch("file", 10, 20, "new content")   # Replace lines 10-20
file.mkdir("path/to/dir")                   # Create directories recursively

# System info
sys.hostname()                # => "my-machine"
sys.uptime()                  # => {days: 5, hours: 3, minutes: 42}
sys.cpu_info()                # => {cores: 8, model: "Apple M2", ...}
sys.mem_info()                # => {total: 16384, used: 8192, free: 8192}

# Network
net.interfaces()              # List network interfaces
net.ping("google.com")        # => {success: true, latency_ms: 12}
net.dns_lookup("github.com")  # => {ips: ["140.82.121.4"], ...}
http.get("https://api.github.com/users/octocat")

# Crypto
crypto.uuid()                              # Generate UUID
crypto.hash("sha256", "hello")             # => "2cf24dba5fb0a30e..."
crypto.jwt_decode(token)                   # Decode JWT

# Database
db.sqlite_open("app.db")                   # Open SQLite
db.sqlite_query(conn, "SELECT * FROM users")

# Math and strings
math.sqrt(16)                 # => 4.0
math.pow(2, 10)               # => 1024
str.upper("hello")            # => "HELLO"
str.split("a,b,c", ",")       # => ["a", "b", "c"]

# Arrays
arr.range(5)                  # => [0, 1, 2, 3, 4]
arr.flatten([[1,2], [3,4]])   # => [1, 2, 3, 4]
arr.unique([1, 2, 2, 3])      # => [1, 2, 3]
```

**All modules:** `file`, `sys`, `proc`, `fs`, `net`, `http`, `gui`, `web`, `crypto`, `db`, `svc`, `cron`, `archive`, `user`, `perm`, `pkg`, `hw`, `clip`, `input`, `ai`, `math`, `str`, `arr`, `json`, `mcp`, `shell`, `a2ui`, `a2a`, `nanda`, `rbac`, `audit`, `sso`, `cluster`, `nn`, `evo`, `rl`

---

## Language

```ae
# Types (inferred or explicit)
name = "AetherShell"                    # String
count = 42                              # Int
config: Record = {host: "localhost"}    # Explicit annotation

# Lambdas
double = fn(x) => x * 2
add = fn(a, b) => a + b

# Pipelines - typed data, not text
[1, 2, 3, 4, 5]
  | where(fn(x) => x > 2)               # [3, 4, 5]
  | map(fn(x) => x * 2)                 # [6, 8, 10]
  | reduce(fn(a, b) => a + b, 0)        # 24

# Pattern matching
grade = fn(score) => match score {
    90..100 => "A",
    80..89 => "B",
    _ => "C"
}

# Error handling
result = try { risky() } catch e { default }

# String interpolation
greeting = "Hello, ${name}!"
```

---

## AI Agents

```ae
# Simple query
ai("Explain recursion in one sentence")

# With context
ai("Summarize this file", {context: file.read("README.md")})

# Multi-modal (images, audio, video)
ai("What's in this image?", {images: ["photo.jpg"]})
ai("Transcribe this", {audio: ["meeting.mp3"]})

# Autonomous agent with tool access
agent("Find all TODOs in the codebase", ["file.read", "grep", "ls"])

# Agent with config
agent({
    goal: "Fix code style violations",
    tools: ["file.read", "file.write", "grep"],
    max_steps: 20,
    model: "openai:gpt-4o"
})

# Multi-agent swarm
swarm({
    coordinator: "Perform security audit",
    agents: [
        {role: "scanner", goal: "Find vulnerable deps"},
        {role: "reviewer", goal: "Check for injections"},
        {role: "reporter", goal: "Generate report"}
    ],
    tools: ["file.read", "grep", "http.get"]
})
```

---

## Protocols

AetherShell implements four agentic protocols:

### MCP (Model Context Protocol)
```ae
mcp.tools()                              # List 130+ tools
mcp.call("git", {command: "status"})     # Execute tool
mcp.connect("http://localhost:3001")     # Connect to server
```

### A2A (Agent-to-Agent)
```ae
a2a.send("analyzer", {task: "review", files: ls("./src")})
a2a.receive("analyzer")
```

### A2UI (Agent-to-User Interface)
```ae
a2ui.notify("Task complete", "success")
a2ui.progress("Processing", 0.75)
a2ui.confirm("Deploy to production?")
```

### NANDA (Consensus)
```ae
nanda.propose("deployment", {version: "2.0", threshold: 0.7})
nanda.vote("proposal_id", true)
```

---

## Enterprise

```ae
# RBAC
rbac.create("admin", ["read", "write", "delete"])
rbac.grant("alice", "admin")
rbac.check("alice", "config.toml", "write")

# Audit logging
audit.log("file_modified", "config.toml", {user: "alice"})
audit.query({action: "file_modified", since: "2024-01-01"})

# SSO
sso.init("okta", {client_id: "...", issuer: "https://..."})
sso.auth(callback_data)
```

---

## ML Built-ins

```ae
# Neural networks
net = nn.create("policy", [8, 16, 4])
output = nn.forward(net, [0.1, 0.2, ...])

# Evolution
pop = evo.population(100, "nn", {layers: [4, 8, 2]})
pop = evo.evolve(pop, fitness_fn, 50)
best = evo.best(pop)

# Reinforcement learning
agent = rl.agent("q-learner", 16, 4, {epsilon: 0.1})
action = rl.action(agent, state)
agent = rl.update(agent, state, action, reward, next_state)
```

---

## Development

```bash
# Build
cargo build --release --bins

# Test
cargo test

# TUI
ae tui

# VS Code extension
code --install-extension admercs.aethershell
```

### Project Structure
```
src/
  main.rs          # Entry point
  eval.rs          # Expression evaluator
  parser.rs        # AetherShell syntax parser
  builtins.rs      # 215+ builtin functions
  modules.rs       # Module system (file, sys, net, ...)
  ai.rs            # AI provider integration
  agent.rs         # Autonomous agent framework
  tui/             # Terminal UI components
```

---

## License

Apache 2.0 - see [LICENSE](LICENSE)

---

<p align="center">
  <strong>AetherShell</strong> - The OS interface for agentic AI<br>
  <a href="https://github.com/nervosys/AetherShell">GitHub</a> |
  <a href="https://crates.io/crates/aethershell">Crates.io</a> |
  <a href="https://discord.gg/aethershell">Discord</a>
</p>
