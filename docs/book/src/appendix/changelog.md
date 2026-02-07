# Changelog

## v0.3.0 (Current)

### Features
- **Marketplace**: Agent marketplace with search, install, publish, and registry support
- **Web Dashboard**: React-based monitoring dashboard with real-time metrics via SSE
- **VS Code Extension**: v0.3.0 with LSP, syntax highlighting, AI panel, and marketplace browser
- **Python SDK**: v0.3.0 with AetherRuntime, PipelineBuilder, workflows, distributed computing, and LangChain integration
- **OS Providers**: Cross-platform hardware detection and system information
- **Dot Notation Modules**: Access module members with `module.function` syntax
- **External Tools**: MCP client/server integration for agent tool use
- **AI Coding Assistants**: Built-in code generation and review via AI
- **Hardware Detection**: CPU, GPU, memory, and disk profiling

### Improvements
- Agent API expanded to 25+ REST endpoints
- WebSocket and SSE streaming for real-time agent communication
- Marketplace frontend wired to backend RegistryClient
- Dashboard connected to real API endpoints

### Fixes
- VS Code extension `restartServer` command restored
- Dashboard mock data replaced with live API calls
- Marketplace search connected to backend registry

---

## v0.2.0

### Features
- **Agent Framework**: Single agents with ReAct loop and tool use
- **Swarm Intelligence**: Multi-agent coordination with blackboard communication
- **TUI**: Full terminal UI with 7 tabs (Chat, AgentSwarm, MediaBrowser, Settings, etc.)
- **RAG Pipeline**: Document indexing, semantic search, and retrieval-augmented generation
- **Knowledge Graphs**: Entity/relation storage with graph queries
- **Semantic Cache**: LLM response caching with similarity matching
- **Multimodal AI**: Image, audio, and video support in AI prompts

### Improvements
- 272 tests passing
- Type inference for lambdas and pipelines
- Bash transpiler for compatibility

---

## v0.1.0

### Features
- **Core Language**: Typed expression-based shell with AST evaluation
- **Pipeline Operator**: `|` for chaining operations on structured data
- **Value System**: Int, Float, String, Bool, Array, Record, Lambda, Null
- **Builtins**: 100+ built-in functions (filesystem, collections, strings, math, HTTP)
- **Pattern Matching**: `match` expressions with literal and wildcard patterns
- **Lambda Functions**: `fn(x) => expr` syntax
- **REPL**: Interactive read-eval-print loop
- **AI Integration**: Provider-agnostic LLM client with model URIs
- **Bash Transpiler**: Convert bash scripts to AetherShell syntax
