<p align="center">
  <img src="assets/logo.svg" alt="Æther Shell" width="180">
</p>

<h1 align="center">Æther Shell</h1>

<p align="center">
  <a href="https://crates.io/crates/aether_shell"><img src="https://img.shields.io/crates/v/aether_shell.svg?style=flat-square&logo=rust&color=orange" alt="Crates.io"></a>
  <a href="https://github.com/nervosys/AetherShell/actions"><img src="https://img.shields.io/github/actions/workflow/status/nervosys/AetherShell/rust.yml?style=flat-square&logo=github" alt="Build Status"></a>
  <a href="https://github.com/nervosys/AetherShell/blob/master/LICENSE"><img src="https://img.shields.io/badge/license-Apache%202.0-blue.svg?style=flat-square" alt="License"></a>
  <a href="https://github.com/nervosys/AetherShell/stargazers"><img src="https://img.shields.io/github/stars/nervosys/AetherShell?style=flat-square&color=yellow" alt="Stars"></a>
</p>

<p align="center">
  <strong>The world's first multi-agent shell with typed functional pipelines and multi-modal AI.</strong><br>
  <em>Built in Rust for safety and performance, featuring revolutionary AI protocols found nowhere else.</em>
</p>

<p align="center">
  <a href="#-quick-start">Quick Start</a> •
  <a href="#-features">Features</a> •
  <a href="#-examples">Examples</a> •
  <a href="docs/TUI_GUIDE.md">TUI Guide</a> •
  <a href="#-documentation">Docs</a> •
  <a href="#-contributing">Contributing</a>
</p>

---

<p align="center">
  <img src="assets/screenshot.svg" alt="AetherShell Terminal Demo" width="800">
</p>

---

## 🚀 Quick Start

```bash
# Install from source
git clone https://github.com/nervosys/AetherShell && cd AetherShell
cargo install --path . --bin ae

# Launch interactive TUI (recommended)
ae --tui

# Or classic REPL
ae
```

```ae
# Typed pipelines — not text streams!
[1, 2, 3, 4, 5] | map(fn(x) => x * 2) | sum()
# => 30

# AI query
ai("Explain quantum computing in simple terms")

# AI with vision
ai("Describe this image", {images: ["photo.jpg"]})

# AI agent with tool access
agent("Find all TODO comments in src/", ["ls", "cat", "grep"])

# 130+ MCP tools
mcp_tools() | len()  # => 130
```

> **📝 Note:** Set `OPENAI_API_KEY` for AI features: `export OPENAI_API_KEY="sk-..."`

---

## ✨ Features

<table>
<tr>
<td width="50%">

### 🤖 AI-Native Shell
- **Multi-modal AI**: Images, audio, video analysis
- **Autonomous agents** with tool access
- **130+ MCP tools** across 27 categories
- **Multi-provider**: OpenAI, Ollama, local models

</td>
<td width="50%">

### 💎 Typed Pipelines
- **Hindley-Milner** type inference
- **Structured data**: Records, Arrays, Tables
- **First-class functions** and lambdas
- **Pattern matching** expressions

</td>
</tr>
<tr>
<td width="50%">

### 🧠 ML Primitives
- **Neural networks** creation & evolution
- **Reinforcement learning** (Q-Learning, DQN, etc.)
- **Evolutionary algorithms** & NEAT
- **Consensus networks** for distributed AI

</td>
<td width="50%">

### 🎨 Beautiful TUI
- **Interactive terminal UI** with tabs
- **Media viewer** for images/audio/video
- **Agent dashboard** for swarm control
- **Chat interface** with context

</td>
</tr>
</table>

---

## 🎯 What Makes AetherShell Unique?

AetherShell is the **only shell** combining these capabilities:

| Feature                             | AetherShell | Traditional Shells | Nushell |
| ----------------------------------- | :---------: | :----------------: | :-----: |
| AI Agents with Tools                |      ✅      |         ❌          |    ❌    |
| Multi-modal AI (Vision/Audio/Video) |      ✅      |         ❌          |    ❌    |
| MCP Protocol (130+ tools)           |      ✅      |         ❌          |    ❌    |
| Neural Networks Built-in            |      ✅      |         ❌          |    ❌    |
| Hindley-Milner Types                |      ✅      |         ❌          |    ✅    |
| Typed Pipelines                     |      ✅      |         ❌          |    ✅    |
| Agent-to-Agent Protocol (A2A)       |      ✅      |         ❌          |    ❌    |
| Consensus Protocol (NANDA)          |      ✅      |         ❌          |    ❌    |

---

## 📖 Examples

### AI Agents with Tool Access

```ae
# Deploy an AI agent that can use shell tools
agent("Analyze the project structure and find large files", ["ls", "cat", "wc"])

# Agent with configuration
agent({
  goal: "Find security issues in the codebase",
  tools: ["grep", "cat", "ls"],
  max_steps: 10,
  dry_run: true  # Preview actions first
})
```

### Multi-Modal AI

```ae
# Analyze images
ai("What's in this screenshot?", {images: ["screenshot.png"]})

# Process audio
ai("Transcribe and summarize this meeting", {audio: ["meeting.mp3"]})

# Video analysis
ai("Extract the key steps from this tutorial", {video: ["tutorial.mp4"]})
```

### Typed Functional Pipelines

```ae
# Structured data processing — not text parsing!
ls("./src")
  | where(fn(f) => f.ext == ".rs" && f.size > 1000)
  | map(fn(f) => {name: f.name, kb: f.size / 1024})
  | sort_by(fn(f) => f.kb, "desc")
  | take(5)

# Statistical operations
[1, 2, 3, 4, 5] | sum()      # => 15
[10, 20, 30] | avg()         # => 20.0
[1, 2, 1, 3] | unique()      # => [1, 2, 3]
{a: 1, b: 2} | values()      # => [1, 2]
```

### MCP Tools (Model Context Protocol)

```ae
# 130 tools across 27 categories
let tools = mcp_tools()
print(len(tools))  # => 130

# Filter by category
mcp_tools({category: "development"})     # git, cargo, npm, etc.
mcp_tools({category: "machinelearning"}) # ollama, tensorboard, etc.
mcp_tools({category: "kubernetes"})      # kubectl, helm, k9s, etc.

# Execute tools via MCP
mcp_call("git", {command: "status"})
```

### Neural Networks & Evolution

```ae
# Create a neural network
let brain = nn_create("agent", [4, 8, 2])

# Evolutionary optimization
let pop = population(100, {genome_size: 10})
let evolved = evolve(pop, fitness_fn, {generations: 50})

# Reinforcement learning
let agent = rl_agent("learner", 16, 4)
```

---

## 🎮 TUI Interface

Launch the beautiful terminal UI with `ae --tui`:

| Tab        | Description                                |
| ---------- | ------------------------------------------ |
| **Chat**   | Conversational AI with multi-modal support |
| **Agents** | Deploy and monitor AI agent swarms         |
| **Media**  | View images, play audio, preview videos    |
| **Help**   | Quick reference and documentation          |

**Keyboard shortcuts:**
- `Tab` — Switch tabs
- `Enter` — Send message / activate
- `Space` — Select media files
- `q` — Quit
- `Ctrl+C` — Force quit

📖 **Full guide:** [docs/TUI_GUIDE.md](docs/TUI_GUIDE.md)

---

## 📦 Installation

### From Source (Recommended)

```bash
git clone https://github.com/nervosys/AetherShell
cd AetherShell
cargo install --path . --bin ae
```

### From Crates.io

```bash
cargo install aether_shell
```

### VS Code Extension

Get syntax highlighting, snippets, and integrated REPL:

```bash
cd editors/vscode
npm install && npm run compile
# Press F5 to test
```

---

## ⚙️ Configuration

### Environment Variables

```bash
# AI Provider (required for AI features)
export OPENAI_API_KEY="sk-..."

# Agent permissions
export AGENT_ALLOW_CMDS="ls,git,curl,python"

# Alternative AI backend
export AETHER_AI="ollama"  # or "openai"
```

### Secure Key Storage

```bash
# Store keys in OS credential manager (recommended)
ae keys store openai sk-your-key-here

# View stored keys (masked)
ae keys list
```

---

## 📚 Documentation

| Document                                             | Description               |
| ---------------------------------------------------- | ------------------------- |
| [Quick Reference](docs/QUICK_REFERENCE.md)           | One-page syntax guide     |
| [TUI Guide](docs/TUI_GUIDE.md)                       | Terminal UI documentation |
| [Type System](docs/TYPE_SYSTEM_GUIDE.md)             | Type inference details    |
| [MCP Servers](docs/MCP_SERVERS_GUIDE.md)             | Tool integration guide    |
| [AI Backends](docs/AI_BACKENDS.md)                   | Provider configuration    |
| [Security](docs/security/SECURITY_AUDIT_RED_TEAM.md) | Security assessment       |

### Example Scripts

| File                                                  | Topic            |
| ----------------------------------------------------- | ---------------- |
| [00_hello.ae](examples/00_hello.ae)                   | Basic syntax     |
| [05_ai.ae](examples/05_ai.ae)                         | AI integration   |
| [06_agent.ae](examples/06_agent.ae)                   | Agent deployment |
| [09_tui_multimodal.ae](examples/09_tui_multimodal.ae) | Multi-modal TUI  |

---

## 🧪 Testing

```bash
# Run all tests (90 library tests)
cargo test --lib

# Run specific test suites
cargo test --test eval          # Evaluator tests
cargo test --test mcp           # MCP protocol tests
cargo test --test tui_          # TUI tests
```

**Test coverage:** 140+ tests covering core functionality, MCP protocol, TUI, AI backends, neural networks, and OS tools.

---

## 🛣️ Roadmap

### ✅ Completed (January 2026)
- Neural network primitives & evolutionary algorithms
- 130+ MCP tools with protocol compliance
- Multi-modal AI (images, audio, video)
- Reinforcement learning (Q-Learning, DQN, Actor-Critic)
- Distributed agent swarms
- VS Code extension

### 🔜 Coming Soon
- Plugin system for custom backends
- WASM support (browser-based shell)
- Advanced video streaming
- Package management & imports

---

## 🤝 Contributing

We welcome contributions! See our development setup:

```bash
git clone https://github.com/nervosys/AetherShell
cd AetherShell
cargo build
cargo test --lib
```

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Submit a pull request

---

## 📜 License

Licensed under the [Apache License 2.0](LICENSE).

---

<p align="center">
  <strong>Ready to experience the future of shell interaction?</strong><br><br>
  <code>ae --tui</code>
</p>

<p align="center">
  <a href="https://github.com/nervosys/AetherShell">⭐ Star us on GitHub</a> •
  <a href="https://github.com/nervosys/AetherShell/issues">🐛 Report Issues</a> •
  <a href="https://github.com/nervosys/AetherShell/discussions">💬 Discussions</a>
</p>
