# AetherShell Examples Index

This directory contains examples showcasing AetherShell's unique and powerful features.

## 🌟 Best Features Showcase

### 🥇 **Unique to AetherShell** (No Other Shell Has These!)

#### 16_mcp_servers.ae ⭐⭐⭐
**ESSENTIAL!** Run local MCP servers that give AI agents safe, controlled access to:
- Operating system tools (filesystem, processes, commands)
- Cloud services (AWS, Azure, Google Cloud)
- Development tools (Git, Docker, npm, databases)
- Web scraping and API access
- Custom tools in any language

**Run it:**
```bash
ae examples/16_mcp_servers.ae
```

**What you'll see:**
- Filesystem MCP server with safe file access
- Git MCP server for repository operations
- AWS MCP server for cloud infrastructure
- Docker MCP server for container management
- Database MCP server with read-only queries
- Web scraping MCP server
- Custom Python MCP server example
- Multi-server agent integration

---

#### 12_multi_agent_orchestration.ae ⭐⭐⭐
**The crown jewel!** Demonstrates features that make AetherShell revolutionary:
- Multi-agent swarms with different AI models
- A2A (Agent-to-Agent) communication protocol
- NANDA negotiation and consensus framework
- Model Context Protocol (MCP) integration
- Agent coordination strategies (Router, Round-Robin, Blackboard)

**Run it:**
```bash
ae examples/12_multi_agent_orchestration.ae
```

**What you'll see:**
- Research team swarm coordinating GPT-4, Claude, and local models
- Code review with specialized agents
- Content pipeline with blackboard communication
- Agents negotiating task allocation
- Agent-to-agent messaging in action

---

#### 13_multimodal_ai.ae ⭐⭐⭐
**Revolutionary multi-modal capabilities!** No other shell can process images, audio, and video like this:
- Image analysis with AI vision
- Batch image processing in pipelines
- Audio transcription and sentiment analysis
- Video content extraction
- Multi-modal combinations (text + image + audio + video)
- Real-world use cases (meeting minutes, content moderation, accessibility)

**Run it:**
```bash
ae examples/13_multimodal_ai.ae
```

**What you'll see:**
- AI analyzing screenshots and photos
- Automatic photo categorization
- Audio transcription
- Video summarization
- Combined media analysis (presentation analysis from slides + audio)

---

#### 15_ai_protocols.ae ⭐⭐⭐
**The three protocols that define AetherShell:**
- **MCP (Model Context Protocol)**: Standardized tool calling
- **A2A (Agent-to-Agent)**: Inter-agent communication
- **NANDA (Negotiation And Dynamic Agents)**: Consensus framework

**Run it:**
```bash
ae examples/15_ai_protocols.ae
```

**What you'll see:**
- MCP tools providing file access, web fetching, command execution
- Agents sending direct messages, broadcasting, delegating tasks
- Multi-agent negotiation with voting and consensus
- Complete integration: MCP + A2A + NANDA working together

---

#### 14_typed_pipelines.ae ⭐⭐
**Type system showcase!** Demonstrates Hindley-Milner type inference and functional programming:
- Automatic type inference
- Structured data vs text streams
- Type-safe transformations
- First-class functions and composition
- Pattern matching
- Complex multi-stage pipelines

**Run it:**
```bash
ae examples/14_typed_pipelines.ae
```

**What you'll see:**
- Types inferred automatically (no annotations needed)
- Structured records flowing through pipelines
- Higher-order functions and partial application
- Lazy evaluation for performance
- Comparison with Bash, PowerShell, and other shells

---

## 📚 Core Feature Examples

### 00_hello.ae
**Your first AetherShell program**
- Basic print statements
- Hello world example
- Introduction to syntax

### 01_pipelines.ae
**Pipeline fundamentals**
- Data flowing through transformations
- Functional pipeline composition
- Real-world data processing

### 02_tables.ae
**Structured data handling**
- Working with tables and records
- Filtering, mapping, reducing
- Type-safe data transformations

### 03_http.ae
**HTTP and web requests**
- Making API calls
- JSON parsing and manipulation
- Structured HTTP responses

### 04_match.ae
**Pattern matching**
- Match expressions
- Option types
- Error handling patterns

### 05_ai.ae
**Basic AI integration**
- Simple AI calls
- Different AI providers
- AI-assisted data processing

### 06_agent.ae
**Single agent usage**
- Creating AI agents
- Tool access for agents
- Agent execution flow

### 07_uri_types.ae
**Model URI syntax**
- Understanding `provider:model` format
- Different AI providers
- Model selection patterns

### 08_transpiler.bash
**Bash compatibility**
- Running bash scripts
- Transpilation examples
- Migration strategies

### 09_tui_multimodal.ae
**TUI multi-modal features**
- Terminal UI demonstrations
- Media handling in TUI
- Interactive AI sessions

### 10_tui_agent_swarm.ae
**TUI agent dashboard**
- Visualizing agent swarms in TUI
- Monitoring agent status
- Interactive swarm control

### 11_tui_showcase.ae
**Complete TUI feature tour**
- Dashboard with statistics and metrics
- Conversation export (Markdown/JSON)
- Search and filtering capabilities
- Advanced TUI features demonstration

### 19_showcase.ae
**Language feature showcase**
- Comprehensive demonstration of all core language features
- Pipelines, pattern matching, records
- Functional programming patterns
- Complete language tour

### 20_tui_a2a.ae
**Agent-to-Agent (A2A) protocol in TUI**
- Direct agent-to-agent messaging
- Broadcasting and delegation
- A2A protocol demonstration
- Interactive agent communication

### 21_tui_chat.ae
**TUI chat mode features**
- Interactive chat interface
- Media attachments in chat
- Configuration and settings
- Chat-focused TUI workflow

### 22_tui_mcp.ae
**Model Context Protocol in TUI**
- MCP server integration
- Tool calling interface
- Standardized tool access
- MCP visualization

### 23_tui_nanda.ae
**NANDA negotiation protocol**
- Multi-agent negotiation
- Voting and consensus
- Dynamic task allocation
- NANDA framework demonstration

---

## 🎯 Recommended Learning Path

### For Beginners:
1. Start with `00_hello.ae` - Get familiar with syntax
2. Try `01_pipelines.ae` - Understand data flow
3. Explore `02_tables.ae` - Learn structured data
4. Check `05_ai.ae` - Your first AI calls

### For AI Enthusiasts:
1. Start with `05_ai.ae` - Basic AI integration
2. Move to `06_agent.ae` - Single agent usage
3. Explore `12_multi_agent_orchestration.ae` - Multi-agent power ⭐
4. Try `13_multimodal_ai.ae` - Images, audio, video ⭐

### For Type System Fans:
1. Begin with `02_tables.ae` - See typed data in action
2. Study `14_typed_pipelines.ae` - Deep dive into types ⭐
3. Practice with `03_http.ae` - Type-safe HTTP
4. Advanced: `04_match.ae` - Pattern matching

### For Protocol Developers:
1. Start with `06_agent.ae` - Agent basics
2. Progress to `15_ai_protocols.ae` - Full protocol suite ⭐
3. Study `12_multi_agent_orchestration.ae` - See protocols in action ⭐

---

## 🚀 Quick Demos

### Demo 1: AI-Powered File Analysis
```bash
# Analyze your codebase with AI
ae -c "ls('.') | where(fn(f) => f.ext == '.rs') | map(fn(f) => {file: f.name, analysis: read_text(f.path) | ai('Summarize this code')})"
```

### Demo 2: Image Batch Processing
```bash
# Describe all images in a directory
ae -c "ls('./photos') | where(fn(f) => f.ext | in(['.jpg', '.png'])) | map(fn(f) => ai('Describe this image', {images: [f.path]}))"
```

### Demo 3: Multi-Agent Task
```bash
# Deploy an agent with tools (requires AETHER_AI config)
ae -c "agent({goal: 'Analyze project structure', tools: ['ls', 'cat'], max_steps: 3, dry_run: true})"
```

---

## 📖 Full Documentation

- **Main README**: [../README.md](../README.md)
- **Language Spec**: [../docs/specs/SPEC.md](../docs/specs/SPEC.md)
- **TUI Guide**: [../docs/TUI_GUIDE.md](../docs/TUI_GUIDE.md)
- **Competitive Analysis**: [../docs/COMPETITIVE_ANALYSIS.md](../docs/COMPETITIVE_ANALYSIS.md)
- **Why AetherShell**: [../docs/WHY_AETHERSHELL.md](../docs/WHY_AETHERSHELL.md)

---

## 🤝 Contributing Examples

Want to add an example? Please include:

1. **Clear comments** explaining what the code does
2. **Expected output** or results
3. **Prerequisites** (API keys, files needed, etc.)
4. **Unique value** - what does this example teach?

Submit via pull request to: https://github.com/nervosys/AetherShell

---

## 💡 Tips for Running Examples

### Setup API Keys (for AI features):
```bash
# OpenAI
export OPENAI_API_KEY="sk-..."

# Anthropic
export ANTHROPIC_API_KEY="sk-ant-..."

# Or use local models with Ollama (no API key needed!)
# Install Ollama, then:
export AETHER_AI=ollama
```

### For Agent Examples:
```bash
# Whitelist commands agents can execute
export AGENT_ALLOW_CMDS=ls,git,cat,head,tail
```

### TUI Mode:
```bash
# Launch with TUI for interactive demos
ae --tui

# Then load an example:
# :load examples/09_tui_multimodal.ae
```

---

## 🎯 What Makes These Examples Special?

### Unlike Examples in Other Shells:

**Bash examples:**
- Text processing, pipes, grep/awk/sed
- No AI, no types, error-prone

**PowerShell examples:**
- Object pipelines, .NET integration
- Verbose, no AI capabilities

**Nushell examples:**
- Structured data, tables
- No AI, no multi-agent features

**AetherShell examples:**
- ✅ Multi-agent AI orchestration
- ✅ Multi-modal (images, audio, video)
- ✅ Type-safe functional pipelines
- ✅ Agent communication protocols
- ✅ Negotiation and consensus
- ✅ Real AI-powered automation

**These examples showcase capabilities that literally don't exist in any other shell!**

---

## 🌟 Star Examples (Must Try!)

If you only have time for a few examples, try these:

1. **`12_multi_agent_orchestration.ae`** - See what makes AetherShell unique
2. **`13_multimodal_ai.ae`** - Experience multi-modal AI power
3. **`14_typed_pipelines.ae`** - Understand the type system
4. **`15_ai_protocols.ae`** - Learn the three protocols

These four examples demonstrate features found NOWHERE else in the shell ecosystem!

---

*Happy exploring! 🚀*
