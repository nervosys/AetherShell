# Æther Shell (æ) 🚀

*The world's first multi-agent shell with typed functional pipelines and multi-modal AI. Built in Rust for safety and performance, featuring revolutionary AI protocols found nowhere else.*

> **"What if your shell could coordinate teams of AI agents, negotiate consensus, and process images, audio, and video—all with type-safe functional pipelines?"**

---

## ⚡ Quick Start

```bash
# Install
git clone https://github.com/nervosys/AetherShell && cd AetherShell
cargo install --path . --bin ae

# Launch interactive TUI
ae --tui

# Or classic REPL
ae
```

```ae
# Typed pipelines - not text streams!
[1,2,3,4,5] | map(fn(x) => x * 2) | reduce(fn(a,b) => a + b, 0)
# => 30

# AI with vision
ai("Describe this image", {images: ["photo.jpg"]})

# Multi-agent swarm
swarm([
  {id: "researcher", model: "openai:gpt-4", role: "Research"},
  {id: "writer", model: "anthropic:claude-3", role: "Write"}
], "router")

# Neural network evolution
let brain = nn_create("agent", [4, 8, 2])
let pop = population(50, "nn", evolution_config({layer_sizes: [4, 8, 2]}))
let trained = evolve(pop, fitness_fn, 100)
```

**Set your API key:** `export OPENAI_API_KEY="sk-..."` or `ae ai keys store openai sk-...`

📖 **[Full Documentation](#-quick-start-guide)** | 🎮 **[TUI Guide](docs/TUI_GUIDE.md)** | 📚 **[Examples](examples/)**

---

## 🎯 **What Makes AetherShell Unique?**

AetherShell is the **ONLY shell in the world** that combines:

### 🥇 **Exclusive Features (No Competitors)**

1. **Multi-Agent Orchestration** 🤖
   - Deploy swarms of AI agents with different models and capabilities
   - Coordinate agents with Router, Round-Robin, or Blackboard strategies
   - Share context and results across agent teams
   - **No other shell can do this!**

2. **AI Communication Protocols** 💬
   - **MCP**: Model Context Protocol for standardized tool integration
   - **A2A**: Agent-to-Agent messaging with direct/broadcast/delegate
   - **NANDA**: Negotiation And Dynamic Agents for consensus
   - **AgenticBinary**: Maximum information density binary protocol (16 semantic opcodes)
   - **Syntax KB**: Persistent knowledge base for protocol discovery and sharing
   - **These protocols are AetherShell exclusives!**

3. **Local MCP Servers** 🔧
   - Run local MCP servers that give AI agents controlled access to:
     - Operating system (filesystem, processes, commands)
     - Cloud services (AWS, Azure, Google Cloud)
     - Development tools (Git, Docker, databases)
     - Web scraping and custom APIs
   - Better than raw command execution—structured and safe!
   - **No other shell has this integration!**

4. **Multi-Modal AI Native** 🎨
   - Analyze images, transcribe audio, process video directly in pipelines
   - Mix text + images + audio + video in single AI queries
   - No other shell has native multi-modal support!

5. **Typed Functional Pipelines** 💎
   - Hindley-Milner type inference (like Haskell, OCaml)
   - Structured data: Records, Arrays, Tables—not text streams
   - First-class functions, pattern matching, lazy evaluation
   - Type safety prevents entire classes of shell scripting errors

6. **🧠 Neural Networks & Evolutionary Learning** 🆕
   - In-shell neural network creation, training, and mutation
   - Consensus networks for multi-agent distributed decision making
   - Evolutionary algorithms with population-based optimization
   - NEAT topology evolution and coevolution for protocol learning
   - **Train AI swarms to develop their own communication protocols!**

### ✨ **Revolutionary Features**

### 🧠 **AI Integration Beyond Competition**

- **Multi-Agent Swarms**: Coordinate teams of AI agents working together on complex tasks
- **Vision AI**: Analyze images, screenshots, and visual content directly in your terminal
- **Audio Processing**: Transcribe speech, analyze audio files, and voice commands
- **Video Analysis**: Process video content with AI-powered insights
- **Smart Agents**: Deploy specialized AI agents with tool access and reasoning
- **Protocol Support**: MCP, A2A, and NANDA for advanced agent coordination
- **🆕 Model Management**: OpenRouter-style API server with local model management and format conversion
- **🆕 Neural Networks**: Create and evolve neural networks directly in the shell
- **🆕 Evolutionary Learning**: Population-based optimization and coevolution

### 🎨 **Beautiful Terminal UI (TUI)**

- **Interactive Interface**: Modern, responsive terminal GUI with real-time updates
- **Media Viewer**: Display images, play audio, and preview videos in terminal
- **Chat Interface**: Conversational AI with context-aware responses
- **Agent Dashboard**: Monitor and control your AI agent swarms
- **Multimodal Sessions**: Seamlessly mix text, images, audio in conversations

### 💪 **Advanced Programming Features**

- **Typed Pipelines**: Pass structured records/tables, not just raw text
- **Rust-Grade Safety**: Memory-safe runtime with zero-cost abstractions
- **Strong Type System**: Hindley–Milner inference with algebraic data types
- **Metaprogramming**: Hygienic macros and AST manipulation
- **Async/Await**: Built-in structured concurrency and cancellation
- **POSIX Compatibility**: Run existing tools seamlessly

### 🔄 **Seamless Interoperability**

- **Bash Compatibility**: Transpile and run existing `.sh` scripts
- **Command Integration**: Auto-wrap unknown commands in safe shells
- **Multi-Backend AI**: Support for OpenAI, Anthropic, and local providers via unified API
- **OS Tools Database**: Cross-platform native command integration
- **🆕 XDG Compliance**: Standards-compliant local storage and configuration management

---

## � Project Structure

This project is now organized with a clean directory structure:

- **📂 `src/`** - Core Rust source code
- **📂 `docs/`** - Documentation, specs, and development notes  
- **📂 `examples/`** - AetherShell example scripts
- **📂 `demos/`** - Showcase demos and advanced examples
- **📂 `test-scripts/`** - Manual test scripts (builtins & integration)
- **📂 `tests/`** - Rust unit and integration tests
- **📂 `web/`** - Web terminal components
- **📂 `temp/`** - Temporary files (gitignored)

See [PROJECT_STRUCTURE.md](./PROJECT_STRUCTURE.md) for detailed organization information.

---

## �🚀 Quick Start Guide

### Installation

```bash
git clone https://github.com/nervosys/AetherShell
cd AetherShell

# Install both binaries
cargo install --path . --bins

# Or install individually
cargo install --path . --bin ae       # Main Aether Shell
cargo install --path . --bin aimodel  # AI Model Management CLI (deprecated, use 'ae ai' instead)
```

### VS Code Extension

Get professional IDE support for AetherShell:

- **Syntax highlighting** for `.ae` files
- **Code snippets** for agents, swarms, MCP servers
- **Run code** directly from editor (Ctrl+Shift+R)
- **Auto-completion** for built-in functions
- **Hover documentation** for AI features
- **Integrated REPL and TUI**

📖 **Quick Reference**: See [QUICK_REFERENCE.md](docs/QUICK_REFERENCE.md) for all syntax, patterns, and snippets!

**Install:**

```bash
cd vscode-extension
npm install
npm run compile
# Press F5 to test, or package for distribution
```

See `vscode-extension/README.md` for details.

### Launch Options

**Classic REPL Mode:**

```bash
ae
```

**🎨 Interactive TUI Mode (Recommended):**

```bash
ae --tui
```

> **Note**: TUI requires a terminal with full ANSI support (Windows Terminal, native PowerShell, or modern terminal emulators). VS Code integrated terminal may have limited support. See [TUI Guide](docs/TUI_GUIDE.md) for details.

**Run Scripts:**

```bash
ae script.ae          # Run Aether script
ae --bash script.sh   # Run Bash script in compatibility mode
```

### REPL Commands

- **exit** / **quit**: Exit the REPL
- **Ctrl+D**: Exit the REPL (EOF)
- **Ctrl+C**: Interrupt current operation

---

## 🎯 Experience the Magic

### 🤖 **Multi-Agent Orchestration (UNIQUE!)**

**Deploy a research team with different AI models:**

```ae
# Coordinate GPT-4, Claude, and local models working together!
swarm([
  {id: "researcher", model: "openai:gpt-4", role: "Find research papers"},
  {id: "analyst", model: "anthropic:claude-3-opus", role: "Analyze trends"},
  {id: "writer", model: "openai:gpt-4o-mini", role: "Write summary"}
], "router")
```

**Agents communicate via A2A Protocol:**

```ae
# Create agents that talk to each other
coordinator = agent("Coordinate tasks", ["management"])
worker = agent("Process data", ["analysis"])

# Direct messaging
coordinator.send_message(worker.id, "Process sales_2024.csv")

# Broadcasting
coordinator.broadcast("New task available")

# Task delegation with context
coordinator.delegate_task(worker.id, "analyze_data", {
  source: "sales.csv",
  metrics: ["revenue", "growth"]
})
```

**Negotiate consensus with NANDA Protocol:**

```ae
# Agents vote on proposals to reach consensus
coordinator = nanda_coordinator(agents, 0.75, 3)  # 75% threshold

# Propose a decision
neg_id = coordinator.propose(agent_id, {
```ae
# Create agents that talk to each other
coordinator := agent("Coordinate tasks", ["management"])
worker := agent("Process data", ["analysis"])

# Direct messaging
coordinator.send_message(worker.id, "Process sales_2024.csv")

# Broadcasting
coordinator.broadcast("New task available")

# Task delegation with context
coordinator.delegate_task(worker.id, "analyze_data", {
  source: "sales.csv",
  metrics: ["revenue", "growth"]
})
```

**Negotiate consensus with NANDA Protocol:**

```ae
# Agents vote on proposals to reach consensus
coordinator = nanda_coordinator(agents, 0.75, 3)  # 75% threshold

# Propose task allocation
neg_id = coordinator.propose(agent_id, {
  type: "TaskAllocation",
  task: build_dashboard_task,
  allocation: {...}
})

# Agents vote
coordinator.vote(neg_id, "agent1", {type: "Accept"})
coordinator.vote(neg_id, "agent2", {type: "Accept"})
coordinator.vote(neg_id, "agent3", {type: "Accept"})

# Check consensus
status = coordinator.get_status(neg_id)  # => "Accepted"
```

### 🔧 **Local MCP Servers (Safe Tool Access)**

**Give AI agents access to OS and cloud tools via local MCP servers:**

```ae
# Start filesystem MCP server (safe, controlled access)
fs_server = mcp_server_start({
  name: "filesystem",
  type: "builtin",
  config: {
    allowed_paths: ["./", "~/Projects"],
    excluded_patterns: [".git/", "node_modules/"]
  }
})

# Agent with filesystem tools
agent = agent_with_mcp(
  "Organize project files",
  ["mcp:read_file", "mcp:list_dir", "mcp:search_files"],
  fs_server.endpoint
)

# Agent uses MCP tools safely
todos = agent.call_mcp_tool("search_files", {
  path: "./src",
  pattern: "TODO:|FIXME:"
})
```

**AWS infrastructure with MCP:**

```ae
# Start AWS MCP server (read-only for safety)
aws_server = mcp_server_start({
  name: "aws",
  type: "cloud",
  provider: "aws",
  config: {
    region: "us-east-1",
    services: ["s3", "ec2", "lambda"],
    read_only: true
  }
})

# DevOps agent with cloud tools
devops = agent_with_mcp(
  "Analyze infrastructure",
  ["mcp:s3_list", "mcp:ec2_describe", "mcp:lambda_list"],
  aws_server.endpoint
)

# Agent analyzes your AWS setup
analysis = devops.execute("Check EC2 instances and suggest optimizations")
```

**Docker, Git, Databases—all via MCP:**

```ae
# Multiple MCP servers running simultaneously
git_server = mcp_server_start({name: "git", type: "builtin"})
docker_server = mcp_server_start({name: "docker", type: "builtin"})
db_server = mcp_server_start({name: "postgres", type: "database"})

# Agent with access to all tools
full_stack_agent = agent_with_mcp(
  "DevOps assistant",
  ["mcp:git_status", "mcp:docker_ps", "mcp:db_query"],
  [git_server.endpoint, docker_server.endpoint, db_server.endpoint]
)
```

### 🎨 **Multi-Modal AI (Images, Audio, Video)**

**Analyze images with AI:**

```ae
# Single image
ai("What do you see in this image?", {images: ["screenshot.png"]})

# Compare multiple images
ai("Compare these photos and find similarities", {
  images: ["photo1.jpg", "photo2.jpg", "photo3.jpg"]
})

# Batch process with typed pipelines
ls("./photos")
  | where(fn(f) => f.ext in [".jpg", ".png"])
  | map(fn(photo) => {
      path: photo.path,
      description: ai("Describe briefly", {images: [photo.path]})
    })
  | save_json("photo_catalog.json")
```

**Audio transcription and analysis:**

```ae
# Transcribe audio
ai("Transcribe this audio", {audio: ["meeting.mp3"]})

# Analyze sentiment
ai("What is the speaker's tone and sentiment?", {
  audio: ["interview.mp3"]
})

# Summarize podcast
ai("Extract key takeaways from this podcast", {
  audio: ["episode_42.mp3"]
})
```

**Video content processing:**

```ae
# Summarize video
ai("Summarize the key points from this video", {
  video: ["presentation.mp4"]
})

# Extract tutorial steps
ai("List the step-by-step instructions from this tutorial", {
  video: ["coding_tutorial.mp4"]
})
```

**Multi-modal combinations:**

```ae
# Analyze presentation with slides + audio
ai("Create comprehensive summary of this presentation", {
  images: ["slide1.png", "slide2.png", "slide3.png"],
  audio: ["narration.mp3"]
})

# Meeting minutes from multiple sources
ai("Generate meeting minutes with action items", {
  audio: ["meeting_audio.mp3"],
  images: ["whiteboard_photo.jpg"],
  video: ["screen_share.mp4"]
})
```

### � **Typed Functional Pipelines**

**Structured data, not text streams:**

```ae
# ls returns typed records, not strings!
ls(".")
  | where(fn(f) => f.size > 1000 && f.ext == ".rs")
  | map(fn(f) => {
      name: f.name,
      size_kb: f.size / 1024.0,
      age_days: (now() - f.modified) / 86400
    })
  | sort_by(fn(f) => f.size_kb, "desc")
  | take(10)
```

**Type-safe with Hindley-Milner inference:**

```ae
# Types are inferred automatically
numbers = [1, 2, 3, 4, 5]  # Array<Int>
doubled = numbers | map(fn(x) => x * 2)  # Array<Int>
sum = doubled | reduce(fn(a, b) => a + b, 0)  # Int

# Complex types work seamlessly
employees = [
  {name: "Alice", age: 30, salary: 75000.0},
  {name: "Bob", age: 25, salary: 65000.0}
]  # Array<Record<name: String, age: Int, salary: Float>>

high_earners = employees
  | where(fn(e) => e.salary > 70000.0)  # Note: use 70000.0 for Float comparison
  | map(fn(e) => {name: e.name, monthly: e.salary / 12.0})
```

**First-class functions:**

```ae
# Functions are values - pass lambdas to higher-order functions
double = fn(x) => x * 2
triple = fn(x) => x * 3
square = fn(x) => x * x

[1,2,3,4,5] | map(double)   # => [2,4,6,8,10]
[1,2,3,4,5] | map(triple)   # => [3,6,9,12,15]
[1,2,3,4,5] | map(square)   # => [1,4,9,16,25]

# Chain operations
[1,2,3,4,5] | map(fn(x) => x + 1) | map(fn(x) => x * x)  # => [4,9,16,25,36]
```

---

## 🔥 Powerful Real-World Examples

### 🤖 Multi-Agent Code Review System

```ae
# Deploy specialized agents for comprehensive code review
swarm([
  {
    id: "security", 
    model: "openai:gpt-4",
    role: "Check for security vulnerabilities",
    tools: ["mcp:read_file", "mcp:execute_command"]
  },
  {
    id: "performance",
    model: "anthropic:claude-3-opus", 
    role: "Analyze performance and optimization opportunities"
  },
  {
    id: "style",
    model: "openai:gpt-4o-mini",
    role: "Check code style and best practices"
  }
], "round_robin", read_text("src/main.rs"))

# Agents communicate findings via A2A
# Reach consensus on changes via NANDA
# Result: Multi-perspective code review impossible with other shells!
```

### 📊 Intelligent Data Processing Pipeline

```ae
# Type-safe data transformation with AI insights
sales_data := load_csv("sales_2024.csv")
  | where(fn(r) => r.amount > 1000)
  | group_by(fn(r) => r.region)
  | map(fn(g) => {
      region: g.key,
      total: sum(g.values.map(fn(v) => v.amount)),
      count: len(g.values),
      top_products: g.values
        | sort_by(fn(v) => v.amount, "desc")
        | take(3)
        | map(fn(v) => v.product)
    })
  | sort_by(fn(r) => r.total, "desc")

# Get AI insights on the processed data
sales_data | ai("Analyze these sales trends and provide recommendations")

# All types are checked at compile time!
# Array<Record<region: String, total: Float, count: Int, top_products: Array<String>>>
```

### 🎨 Multi-Modal Content Creation

```ae
# Combine multiple media types with AI agents
content_swarm := swarm([
  {
    id: "researcher",
    model: "anthropic:claude-3-sonnet",
    role: "Analyze images and gather context",
    tools: ["mcp:fetch_url"]
  },
  {
    id: "writer",
    model: "openai:gpt-4o",
    role: "Create engaging content from research"
  },
  {
    id: "editor",
    model: "anthropic:claude-3-opus",
    role: "Polish and fact-check"
  }
], "blackboard")

# Feed multi-modal data to the swarm
result := content_swarm.execute({
  images: ["product_photo.jpg", "infographic.png"],
  audio: ["customer_testimonial.mp3"],
  video: ["demo_video.mp4"],
  task: "Create comprehensive product review article"
})

# No other shell can orchestrate multi-agent multi-modal workflows!
```

### 🔍 Smart File Organization with AI Vision

```ae
# Automatically categorize and tag photos using AI
ls("~/Pictures")
  | where(fn(f) => f.ext in [".jpg", ".png", ".jpeg"])
  | map(fn(photo) => {
      path: photo.path,
      name: photo.name,
      # AI analyzes each image
      analysis: ai("Describe: scene type, objects, colors, mood", {
        images: [photo.path]
      }),
      # Extract structured data
      tags: ai("Generate 5 relevant tags for this image", {
        images: [photo.path]
      }) | split(",") | map(fn(t) => t.trim())
    })
  | group_by(fn(p) => p.analysis.scene_type)
  | each(fn(group) => {
      # Create folders and organize
      mkdir("./organized/${group.key}")
      group.values | each(fn(photo) => {
        copy(photo.path, "./organized/${group.key}/${photo.name}")
      })
    })

# Typed pipelines + AI vision = Smart automation!
```

### 🌐 Distributed Agent Network

```ae
# Create a negotiation-based task allocation system
agents := [
  agent("db_specialist", ["database", "sql", "optimization"]),
  agent("api_specialist", ["rest", "graphql", "microservices"]),
  agent("frontend_specialist", ["react", "ui", "ux"]),
  agent("ml_specialist", ["machine-learning", "pytorch", "data-science"])
]

# Setup NANDA coordinator for consensus
coordinator := nanda_coordinator(
  agents | map(fn(a) => a.id),
  0.75,  # 75% consensus required
  3      # Minimum 3 agents must vote
)

# Propose complex project breakdown
project := {
  name: "AI-powered analytics dashboard",
  tasks: [
    {id: 1, desc: "Design data schema", capabilities: ["database"]},
    {id: 2, desc: "Build ML models", capabilities: ["machine-learning"]},
    {id: 3, desc: "Create API endpoints", capabilities: ["rest"]},
    {id: 4, desc: "Build dashboard UI", capabilities: ["react", "ui"]}
  ]
}

# Agents negotiate task allocation automatically
allocator := nanda_task_allocator(coordinator)
project.tasks | each(fn(task) => allocator.add_task(task))

agent_capabilities := agents | map(fn(a) => {
  id: a.id,
  capabilities: a.capabilities
}) | to_map

negotiations := allocator.allocate_tasks(agent_capabilities)

# Vote and reach consensus
negotiations | each(fn(neg) => {
  # Agents automatically accept tasks matching their capabilities
  coordinator.vote(neg.id, neg.assigned_agent, {type: "Accept"})
})

# Result: Optimal task allocation through AI negotiation
# This coordination is impossible with traditional shells!
```

---

## 🧠 Core Language Features

### Basic Syntax

**Comments:**

```ae
# Line comments use hash (shell style - preferred)
# Comments are ignored during execution

print("Hello") # Inline comments also work

// C-style comments are also supported for compatibility
```

**Hello World:**

```ae
print("Hello, Aether!")
```

**Variables:**

```ae
# Simple = for type inference (recommended)
name = "world"         # Type inferred as String
count = 42             # Type inferred as Int
items = [1, 2, 3]      # Type inferred as Array<Int>

# Mutable variables
mut counter = 0        # Mutable
mut total = 100        # Also mutable

# Alternative syntax (explicit)
let name = "world"     # Explicit let keyword
let mut counter = 0    # Traditional mutable
```

**Structured Pipelines:**

```ae
[1,2,3,4] | map fn(x) => x*x | reduce fn(a,b) => a+b 0
# → 30
```

**Pattern Matching:**

```ae
# Match on arrays and literals
let nums = [1, 2, 3]
match nums {
  [] => print("empty"),
  [x] => print("single: ${x}"),
  [x, y, ...rest] => print("multiple: ${x}, ${y}")
}

# Match with Option types
let val = Some(42)
print(val)  # => {_tag: "Some", _value: 42}
```

**Typed HTTP:**

```ae
resp := http_get "https://api.github.com"
print(resp.status)
print(resp.headers."content-type")
```

---

## 🤖 AI Model Management System

### **New: OpenRouter-Style API Server**

Aether Shell now includes a comprehensive AI model management system with an OpenRouter-compatible API server, XDG-compliant local storage, and advanced model format conversion capabilities.

#### **🚀 Key Features**

- **🔌 Multi-Provider Support**: Seamlessly integrate OpenAI, Anthropic, and local models through a unified API
- **📁 XDG-Compliant Storage**: Local model storage following XDG Base Directory specification
- **🔄 Format Conversion**: Convert between GGUF, SafeTensors, PyTorch, ONNX, and TensorFlow formats
- **📥 Model Downloads**: Direct integration with Hugging Face Hub and custom model repositories
- **🌐 HTTP API Server**: OpenAI-compatible REST API with Swagger documentation
- **⚙️ CLI Management**: Comprehensive command-line interface for all operations

#### **🛠️ AI Model CLI (`ae ai`)**

**Start the API Server:**

```bash
# Start server with default settings
ae ai serve

# Custom host and port with CORS enabled
ae ai serve --host 0.0.0.0 --port 3000 --cors
```

**Model Management:**

```bash
# List all available models (local + remote providers)
ae ai list

# List models from specific provider
ae ai list --provider openai

# List only local models
ae ai list --local

# Download models locally
ae ai download microsoft/DialoGPT-medium
```

**API Key Management:**

```bash
# Store API key securely in OS credential store
ae ai keys store openai --key sk-your-key-here

# Get API key (shows masked version)
ae ai keys get openai

# Delete API key
ae ai keys delete openai

# List all stored API key providers
ae ai keys list
```

**Configuration:**

```bash
# Show current AI configuration
ae ai config
```

> **Note:** The old `aimodel` command is deprecated but still available for backward compatibility. It will show a deprecation warning and suggest using `ae ai` instead.
>
> **Advanced features** (coming soon): Model format conversion, storage management, provider configuration, and LLM backend management will be integrated into `ae ai` in future releases.

**Supported LLM Backends:**

- **🔥 vLLM**: High-performance inference with PagedAttention (`http://localhost:8000`)
- **⚡ TensorRT-LLM**: NVIDIA GPU-optimized inference (`http://localhost:8001`)  
- **🌊 SGLang**: High-throughput serving with RadixAttention (`http://localhost:30000`)
- **🦙 llama.cpp**: CPU/GPU inference for GGUF models (`http://localhost:8080`)

#### **🌐 HTTP API Endpoints**

Once the server is running, you can access these OpenAI-compatible endpoints:

```bash
# List available models
curl http://localhost:8080/v1/models

# Chat completions with different providers
# Using OpenAI
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-api-key" \
  -d '{
    "model": "gpt-3.5-turbo",
    "provider": "openai",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'

# Using vLLM backend
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "microsoft/DialoGPT-medium",
    "provider": "vllm",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'

# Using llama.cpp backend
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama-2-7b-chat",
    "provider": "llama.cpp",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'

# Generate embeddings
curl -X POST http://localhost:8080/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "model": "text-embedding-ada-002",
    "provider": "openai", 
    "input": "Text to embed"
  }'

# Server health check
curl http://localhost:8080/health

# API documentation
open http://localhost:8080/swagger-ui
```

#### **📁 Local Storage Structure**

Models are stored following XDG Base Directory specification:

```bash
~/.local/share/ai-models/          # Linux/macOS
%APPDATA%/ai-models/               # Windows
├── models/
│   ├── gguf/                     # GGUF format models
│   ├── safetensors/              # SafeTensors format  
│   ├── pytorch/                  # PyTorch models
│   └── onnx/                     # ONNX models
├── metadata/                     # Model metadata and index
└── cache/                        # Temporary download cache

~/.config/ai-models/               # Configuration directory
├── config.toml                   # Main configuration
├── providers.toml                # Provider settings
└── aliases.json                  # Model aliases
```

#### **⚙️ LLM Backend Configuration**

Configure your LLM backends in `~/.config/aether/providers.toml`:

```toml
[llm_backends]
# vLLM configuration
vllm_endpoint = "http://localhost:8000"
vllm_auto_start = true
vllm_gpu_memory_utilization = 0.9
vllm_tensor_parallel_size = 1

# TensorRT-LLM configuration  
tensorrt_endpoint = "http://localhost:8001"
tensorrt_auto_start = false
tensorrt_max_batch_size = 8
tensorrt_max_input_len = 2048
tensorrt_max_output_len = 1024

# SGLang configuration
sglang_endpoint = "http://localhost:30000" 
sglang_auto_start = true
sglang_mem_fraction_static = 0.8
sglang_tp_size = 1

# llama.cpp configuration
llamacpp_endpoint = "http://localhost:8080"
llamacpp_auto_start = true
llamacpp_context_size = 4096
llamacpp_gpu_layers = -1  # Use all GPU layers available
```

#### **🔧 Integration with Aether Shell**

The AI model system integrates seamlessly with Aether Shell's existing AI features:

```ae
# Use different LLM backends seamlessly
vllm_response := ai("vllm:microsoft/DialoGPT-medium", "Hello, how are you?")
sglang_response := ai("sglang:meta-llama/Llama-2-7b-chat-hf", "Explain machine learning")
llamacpp_response := ai("llama.cpp:llama-2-7b-chat", "Write code to sort an array")

# Switch between providers dynamically  
openai_response := ai("openai:gpt-4", "Explain quantum computing")
anthropic_response := ai("anthropic:claude-3", "Write a poem about code")

# Auto-detect and use fastest available backend
fastest_backend := ai_backends() | first
response := ai("${fastest_backend}:best-model", "What's the weather like?")

# Model information and backend health checks
models := http_get("http://localhost:8080/v1/models")
local_models := models.data | where fn(m) => m.provider in ["vllm", "sglang", "llama.cpp"]

# Performance comparison across backends
backends := ["vllm", "sglang", "llama.cpp"]
results := backends | each fn(backend) => {
    start_time := now()
    response := ai("${backend}:model", "Hello")
    end_time := now()
    {backend: backend, latency_ms: end_time - start_time, response: response}
}
```

---

## 🎮 TUI Interface Guide

### Navigation

- **Tab**: Switch between Chat, Agents, Media, Help tabs
- **Arrow Keys**: Navigate lists and selections
- **Space**: Select/deselect media files
- **Enter**: Send messages, activate agents
- **Esc**: Exit to normal mode or quit application
- **q**: Quit application (from normal mode)
- **Ctrl+C**: Force quit application

### Media Tab Features

- **Image Viewer**: Display images directly in terminal using advanced algorithms
- **Audio Player**: Play audio files with waveform visualization  
- **Video Preview**: Extract frames and metadata from video files
- **Format Support**: 20+ media formats including PNG, JPG, MP3, MP4, WEBM, GIF
- **Batch Selection**: Select multiple files for multimodal AI analysis

### Agent Management

- **Create Agents**: Spawn AI agents with specific capabilities
- **Monitor Status**: Real-time agent status and task progress
- **Swarm Coordination**: Deploy coordinated teams of agents
- **Task Assignment**: Distribute work across agent networks
- **Strategy Selection**: Choose from Round-Robin, Load-Balanced, or Specialized coordination

### Chat Interface

- **Multimodal Messages**: Include text, images, audio, and video in conversations
- **Context Awareness**: AI remembers conversation history and attached media
- **Export Options**: Save conversations as Markdown or JSON
- **Session Management**: Multiple chat sessions with persistent history
- **Auto-Summarization**: Intelligent conversation summarization

---

## 🛠️ Advanced Features

### Bash Compatibility

**Run old scripts seamlessly:**

```bash
ae --bash script.sh
```

**Or pipe Bash from stdin:**

```bash
echo 'echo hello | wc -l' | ae -b
```

**Transpiler magic - turns this:**

```bash
echo hello | wc -l
```

**into Aether:**

```ae
echo("hello") | sh(["wc","-l"])
```

---

## 🎨 AI & Media Configuration

### Supported AI Backends

AetherShell supports multiple AI inference backends for maximum flexibility:

- **OpenAI** (`openai:gpt-4o-mini`) - Cloud API for GPT-4V vision, GPT-4 text, Whisper audio
- **Ollama** (`ollama:llama3`) - Local server for LLaVA vision, Llama/Mistral text
- **vLLM** (`vllm:meta-llama/Llama-3-8B`) - High-performance local inference with PagedAttention
- **llama.cpp** (`llamacpp:model`) - Efficient CPU/GPU inference with GGUF models
- **TGI** (`tgi:mixtral`) - HuggingFace Text Generation Inference
- **OpenAI-Compatible** (`compat:mixtral`) - Any OpenAI-compatible API server

**📖 See [docs/AI_BACKENDS.md](docs/AI_BACKENDS.md) for detailed backend configuration guide**

### Media Format Support

- **Images**: PNG, JPG, JPEG, WEBP, GIF, BMP, TIFF, ICO, SVG
- **Audio**: MP3, WAV, FLAC, OGG, M4A, AAC, WMA  
- **Video**: MP4, AVI, MOV, MKV, WEBM, FLV, WMV

### Environment Setup

```bash
# For OpenAI integration
export OPENAI_API_KEY="your-api-key"

# For agent command permissions  
export AGENT_ALLOW_CMDS="ls,git,curl,python"

# For custom AI backends
export AI_BACKEND="ollama"  # or "openai" or "custom"
```

---

## 🚀 Example Workflows

### 1. **Document Analysis Pipeline**

```bash
ae --tui
# 1. Load PDFs, images, audio recordings in Media tab
# 2. Select multiple files (Space key)
# 3. Chat: "Analyze these documents and create a summary report"
# 4. AI processes all media types and generates comprehensive analysis
```

### 2. **Content Creation Swarm**

```ae
# Deploy specialized agents for blog creation
swarm "create tech blog post" [
  "researcher:gather latest AI trends",
  "writer:draft engaging content", 
  "editor:polish and optimize SEO",
  "designer:suggest visual elements"
] --strategy=specialized --max_iterations=3
```

### 3. **Interactive Media Analysis**

```ae
# Batch process images with AI descriptions
ls("./photos") 
  | where fn(f) => f.ext in [".jpg", ".png"]
  | map fn(img) => {
      file: img.path,
      analysis: img | ai("describe this image in detail"),
      tags: img | ai("generate 5 relevant tags")
    }
  | save_json("photo_analysis.json")
```

### 4. **Voice-Controlled Automation**

```ae
# Record voice command and execute
audio_input := record_audio(5) # 5 seconds
command := audio_input | ai("transcribe and extract shell command")
result := command | sh([]) 
print("Executed: ${command}")
print("Result: ${result}")
```

---

## 🧪 Developer Features

### Type System

- **Hindley-Milner inference**: Automatic type deduction
- **Algebraic data types**: `Option<T>`, `Result<T,E>`, custom enums
- **Strong safety**: Compile-time error prevention
- **Generic programming**: Parametric polymorphism

### Metaprogramming

- **Hygienic macros**: Safe code generation
- **AST manipulation**: Runtime code transformation  
- **Quoting/splicing**: Embed code as data

### Concurrency

- **Async/await**: Built-in structured concurrency
- **Cancellation**: Graceful task termination
- **Pipelines**: Parallel data processing

### OS Tools Integration

- **Cross-platform database**: 25+ native OS tools (Linux/Windows/macOS)
- **Safety levels**: Safe, Moderate, RequiresAdmin classification
- **Command recommendations**: AI-powered tool suggestions
- **Platform filtering**: OS-specific tool availability

---

## 📊 Performance & Testing

### Benchmarks

- **Memory safe**: Zero buffer overflows or memory leaks
- **Fast execution**: Rust-powered performance
- **Concurrent pipelines**: Multi-core utilization
- **Efficient AI calls**: Batched multimodal requests

### Test Coverage

- **450+ tests**: Comprehensive test suite
- **Unit tests**: Individual component validation
- **Integration tests**: End-to-end workflow testing
- **TUI tests**: Interactive interface validation
- **AI tests**: Multimodal backend testing
- **OS Tools tests**: Cross-platform command database validation
- **Neural/Evolution tests**: ML primitive validation

---

## 📚 Documentation & Examples

### Example Scripts

- [`examples/00_hello.ae`](examples/00_hello.ae): Basic syntax introduction
- [`examples/05_ai.ae`](examples/05_ai.ae): AI integration examples
- [`examples/06_agent.ae`](examples/06_agent.ae): Agent deployment
- [`examples/09_tui_basic.ae`](examples/09_tui_basic.ae): TUI usage guide
- [`examples/10_multimodal.ae`](examples/10_multimodal.ae): Multimodal AI workflows
- [`examples/11_agent_swarm.ae`](examples/11_agent_swarm.ae): Advanced swarm coordination
- [`examples/12_syntax_kb.ae`](examples/12_syntax_kb.ae): Syntax Knowledge Base and AgenticBinary protocol
- [`examples/13_agent_coordination.ae`](examples/13_agent_coordination.ae): Real-world multi-agent task distribution

### Learning Resources

#### 📚 Documentation Guides

- **[Quick Reference](docs/QUICK_REFERENCE.md)**: One-page guide to all syntax and patterns
- **[Type System Guide](docs/TYPE_SYSTEM_GUIDE.md)**: Deep dive into `:=` vs `=` and type inference
- **[MCP Servers Guide](docs/MCP_SERVERS_GUIDE.md)**: Complete reference for infrastructure integration
- **[AI Protocols Report](docs/AI_PROTOCOLS_FINAL_REPORT.md)**: A2A and NANDA implementation details
- **[Syntax KB Guide](docs/SYNTAX_KB.md)**: AgenticBinary protocol and knowledge base reference
- **[Syntax KB Quick Ref](docs/SYNTAX_KB_QUICK_REF.md)**: Quick reference for Syntax KB builtins
- **[Competitive Analysis](docs/COMPETITIVE_ANALYSIS.md)**: How AetherShell compares to alternatives
- **[Why AetherShell?](docs/WHY_AETHERSHELL.md)**: Philosophy and unique features

#### 🧪 Test Examples

- **Type system**: See `tests/typecheck.rs` for comprehensive examples
- **Bash compatibility**: Check `tests/transpile_bash.rs` for transpilation rules
- **AI integration**: Explore `tests/multimodal_ai.rs` for backend implementation
- **TUI features**: Review `tests/tui_*.rs` for interface testing
- **OS Tools**: Examine `tests/os_tools.rs` for cross-platform tool usage

---

## � Security

AetherShell implements comprehensive security controls to protect your credentials, data, and system:

### Secure API Key Management

**OS Credential Store Integration** 🔐

API keys are stored securely in your operating system's native credential manager:

- **Windows**: Windows Credential Manager
- **macOS**: Keychain
- **Linux**: Secret Service API (libsecret)

```bash
# Store your API key securely
ae keys store openai sk-your-key-here

# View stored keys (masked for security)
ae keys get openai
# Output: sk-...key...1234

# List all stored providers
ae keys list

# Migrate from environment variables
ae keys migrate openai
```

**Memory Protection** 🛡️

API keys are protected in memory using:

- `Secret<String>` wrapping prevents accidental exposure
- Automatic zeroization on drop clears memory
- No key exposure in debug output, logs, or error messages
- Temporary auth headers are automatically zeroized after use

**Best Practices:**

```bash
# ✅ DO: Use secure credential store
ae keys store openai $OPENAI_API_KEY

# ✅ DO: Remove from environment after migration
unset OPENAI_API_KEY

# ❌ DON'T: Keep keys in shell history or environment
export OPENAI_API_KEY="sk-..."  # Insecure!
```

### Additional Security Features

- **Path Traversal Prevention**: Symlink validation and path sanitization
- **SSRF Protection**: Blocks access to internal IPs (AWS metadata, private networks)
- **Resource Limits**: File size limits (100MB default), memory quotas
- **TLS Hardening**: TLS 1.2+ enforcement with secure cipher suites
- **Input Validation**: Comprehensive sanitization of user input and AI prompts
- **Command Whitelisting**: Configurable allowlist for agent tool use

### Security Documentation

- **[Security Audit](docs/SECURITY_AUDIT_RED_TEAM.md)**: Comprehensive red team assessment
- **[Security Fixes](docs/SECURITY_FIXES_IMPLEMENTED.md)**: Implemented mitigations and status
- **[Memory Sanitization](docs/MEMORY_SANITIZATION_HIGH-002.md)**: API key protection details

**Security Status**: 40% risk reduction achieved (6.8/10 → 4.1/10)

---

## 🛣️ Roadmap

### Recently Completed ✅ (January 2026)

- **✅ Neural Network Primitives**: In-shell neural network creation, forward pass, mutation, crossover
- **✅ Consensus Networks**: Multi-agent distributed decision making with message passing
- **✅ Evolutionary Algorithms**: Population-based optimization with configurable strategies
- **✅ Coevolution**: Multi-population coevolution for protocol learning
- **✅ NEAT Support**: Topology-evolving neuroevolution
- **✅ AI Model Management**: OpenRouter-style API server with multi-provider support
- **✅ Local Model Storage**: XDG-compliant storage with format conversion
- **✅ Model Downloads**: Hugging Face integration and CLI management tools
- **✅ Streaming AI responses**: Real-time token streaming via SSE in API server
- **✅ Reinforcement Learning**: Q-Learning, SARSA, Policy Gradient, Actor-Critic, DQN
- **✅ Distributed agents**: Network-connected agent swarms with latency/geo/cost optimization
- **✅ IDE integration**: VS Code extension and LSP language server

### Near-term (Q1 2026)

- **Plugin system**: Extensible architecture for custom backends  
- **Advanced media**: Video streaming and real-time audio processing
- **Mobile TUI**: Touch-friendly interface adaptations
- **WASM support**: Browser-based shell via WebAssembly

### Long-term (2026+)

- **Module system**: Package management and imports
- **Advanced AI strategies**: Multi-modal reasoning and planning
- **Cloud deployment**: Hosted agent swarms

---

## 🤝 Contributing

We welcome contributions! Here's how to get started:

1. **Fork the repository**
2. **Check the test suite**: `cargo test --tests`
3. **Add your feature** with corresponding tests
4. **Ensure TUI compatibility** if UI changes are involved
5. **Submit a pull request** with clear description

### Development Setup

```bash
git clone https://github.com/nervosys/AetherShell
cd AetherShell
cargo build --release
cargo test --tests --all-features
```

---

## 📜 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

**Ready to experience the future of shell interaction? Start with `ae --tui` and prepare to be amazed! 🚀**
