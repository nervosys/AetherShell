# AetherShell Roadmap

> **Last Updated:** February 13, 2026

This document tracks the development progress of AetherShell, the world's first agentic shell with typed functional pipelines and multi-modal AI.

---

## Progress Overview

| Category          | Status     | Details                      |
| ----------------- | ---------- | ---------------------------- |
| Core Language     | ✅ Complete | AST-based, Hindley-Milner    |
| Type System       | ✅ Complete | Full inference               |
| Builtins Library  | ✅ Complete | 215+ functions, 38 modules   |
| AI Integration    | ✅ Complete | Multi-provider, multi-modal  |
| Agent Framework   | ✅ Complete | Single + swarm + A2A         |
| TUI Interface     | ✅ Complete | Tabs, chat, dashboard        |
| Theme System      | ✅ Complete | 38 themes                    |
| Config System     | ✅ Complete | XDG-compliant                |
| Plugin System     | ✅ Complete | Dynamic loading, TOML        |
| Standard Library  | ✅ Complete | 7 modules (lib/)             |
| Performance       | ✅ Complete | 5 benchmark suites           |
| Test Coverage     | ✅ Complete | 1,169 tests, 100% pass       |
| Documentation     | ✅ Complete | Comprehensive                |
| Publishing        | ✅ Complete | crates.io v0.3.1             |
| WASM Support      | ✅ Complete | Browser REPL ready           |
| Enterprise        | ✅ Complete | RBAC, Audit, SSO             |
| LSP Server        | ✅ Complete | tower-lsp, crates.io         |
| VS Code Extension | ✅ Complete | Marketplace published        |
| MCP Protocol      | ✅ Complete | 130+ tools, HTTP server      |
| Agent API         | ✅ Complete | OpenAI/Claude/Gemini schemas |
| Distribution      | ✅ Complete | Homebrew, Docker, npm        |
| CI/CD             | ✅ Complete | GitHub Actions, CLA check    |
| Licensing         | ✅ Complete | AGPL-3.0 + commercial        |

---

## Version History

### v0.3.1 (Current) — February 2026
- [x] Published to crates.io as `aethershell v0.3.1`
- [x] Published `aethershell-lsp v0.2.0` to crates.io
- [x] VS Code extension v0.3.1 on Marketplace (`admercs.aethershell`)
- [x] License: AGPL-3.0-or-later with commercial dual-license
- [x] Contributor License Agreement (CLA) + CI enforcement
- [x] All 88 compiler warnings fixed (zero-warning build)
- [x] GitHub Releases with Windows + Linux binaries
- [x] Linguist submission package prepared (samples, grammar, guide)
- [x] Crate package optimized (10.1 MiB → 733 KiB compressed)
- [x] README badges: CI, crates.io version, downloads, VS Code, license, stars
- [x] Linux packages (.deb/.rpm) — Cargo.toml metadata + CI job
- [x] LangChain HTTP Agent API tools (`AgentAPIClient` + 3 tool classes)
- [x] System AI assistant (`ae assist`) — interactive/execute/context/suggest modes
- [x] AI discoverability metadata (llms.txt, AGENTS.md, OpenAPI, plugin manifest)
- [x] PyPI distribution for Python SDK (pyproject.toml, CI workflow)
- [x] Context-aware command suggestions and NL transpilation
- [x] Windows Terminal custom profile

### v0.3.0 — January 2026
- [x] Implicit match scrutinee in lambda bodies
- [x] Bash transpiler improvements
- [x] Python SDK (integrations/python/)
- [x] GitHub Actions CI/CD (ci.yml, release.yml, docker.yml, security-audit.yml)
- [x] CLA check workflow

### v0.2.0 — January 2026
- [x] Plugin system with dynamic TOML loading
- [x] 5 benchmark suites (parser, eval, pipeline, builtin, MCP)
- [x] Distributed computing builtins
- [x] Advanced AI (RAG, Knowledge Graphs, Semantic Caching)
- [x] Enterprise features (RBAC, Audit, SSO, Compliance)
- [x] LSP server (aethershell-lsp crate)
- [x] VS Code extension v0.2.0 with hover, symbols, folding
- [x] Error recovery with multi-error reporting
- [x] Async/await syntax
- [x] Debugging tools (debug, trace, assert, inspect)
- [x] Platform detection and feature flags
- [x] Conditional compilation (#[cfg(...)])
- [x] Standard library (lib/)
- [x] Module visibility (pub/private, export)
- [x] Package management (import, aether.toml)
- [x] WASM support with browser REPL
- [x] Distribution: Homebrew, Docker, npm, browser extension

### v0.1.x — December 2025
- [x] Core language: AST evaluator, Hindley-Milner type inference
- [x] Typed pipelines, first-class lambdas, pattern matching
- [x] 215+ builtins across functional, string, array, math, file, OS categories
- [x] AI integration: multi-provider, multi-modal (images, audio, video)
- [x] Agent framework: autonomous agents with tool access
- [x] Agent swarms with coordinator patterns
- [x] TUI interface with chat, agents, media viewer
- [x] MCP protocol: 130+ tools, HTTP server mode
- [x] Agent API: OpenAI/Claude/Gemini function calling schemas

---

## Completed Features (Full Detail)

### Core Language
- [x] AST-based evaluation engine
- [x] Hindley-Milner type inference
- [x] Typed pipelines (structured Value types, not text)
- [x] First-class functions and lambdas (`fn(x) => x * 2`)
- [x] N-ary lambda support (3+ parameters)
- [x] Zero-parameter lambdas
- [x] Pattern matching with guards
- [x] String interpolation (`"Hello ${name}"`)
- [x] Record and array literals
- [x] Try/catch/throw error handling
- [x] Async/await syntax
- [x] Implicit match scrutinee in lambdas
- [x] Conditional compilation (`#[cfg(...)]`)

### Module & Package System
- [x] Module visibility (pub/private, export)
- [x] Import syntax (path, namespaced, selective, aliased, registry)
- [x] Package manifest (aether.toml)
- [x] Module cache with cycle detection
- [x] Standard library (7 modules: prelude, math, string, collection, functional, io, test_stdlib)

### Builtins (215+ functions in 38 modules)
- [x] `file`, `sys`, `proc`, `fs`, `net`, `http`, `gui`, `web`
- [x] `crypto`, `db`, `svc`, `cron`, `archive`, `user`, `perm`, `pkg`
- [x] `hw`, `clip`, `input`, `ai`, `agent`, `math`, `str`, `arr`, `json`
- [x] `mcp`, `shell`, `platform`, `a2ui`, `a2a`, `nanda`
- [x] `rbac`, `audit`, `sso`, `cluster`, `nn`, `evo`, `rl`

### AI & Agents
- [x] Multi-provider: OpenAI, Anthropic, Ollama, vLLM, compatible
- [x] Model URIs: `openai:gpt-4o`, `ollama:llama3`, `compat:mixtral`
- [x] Multi-modal: images, audio, video
- [x] Autonomous agents with tool access and step limits
- [x] Agent swarms with coordinator patterns
- [x] Agent API server (`ae agent serve`)
- [x] OpenAI / Claude / Gemini function calling schemas
- [x] RAG, Knowledge Graphs, Semantic Caching
- [x] Fine-tuning management

### Protocols
- [x] MCP (Model Context Protocol): 130+ tools, 27 categories, HTTP server
- [x] A2A (Agent-to-Agent): inter-agent messaging
- [x] A2UI (Agent-to-User Interface): notifications, progress, confirmations
- [x] NANDA: consensus protocol

### TUI
- [x] Interactive terminal UI with tabs
- [x] Chat interface with context
- [x] Agent dashboard for swarm control
- [x] Media viewer for images/audio/video
- [x] 38 built-in color themes

### Enterprise
- [x] RBAC (role_create, role_grant, check_permission, etc.)
- [x] Audit logging (audit_log, audit_query, audit_export)
- [x] SSO integration (sso_init, sso_auth, sso_validate)
- [x] Compliance reporting
- [x] Distributed computing (cluster, job scheduling)

### ML Built-ins
- [x] Neural networks (nn_create, nn_forward)
- [x] Evolutionary algorithms (evo_population, evo_evolve)
- [x] Reinforcement learning (Q-Learning, DQN, Actor-Critic)
- [x] NEAT implementation
- [x] Consensus networks

### Tooling & Distribution
- [x] VS Code extension (`admercs.aethershell`) — syntax, snippets, hover, LSP
- [x] LSP server (`aethershell-lsp`) — completions, diagnostics, symbols, go-to-def
- [x] TextMate grammar + Markdown injection grammar
- [x] Homebrew formula
- [x] Docker image (multi-stage)
- [x] npm package (`@nervosys/aethershell`)
- [x] Python SDK (`integrations/python/`)
- [x] WASM support with browser REPL
- [x] GitHub Actions CI/CD (test, release, Docker, security audit, CLA)

---

## In Progress

### Q1 2026
- [ ] **GitHub Linguist submission** — Grammar, samples, and submission guide ready; blocked on ecosystem threshold (~200 repos with `.ae` files)
- [ ] **VS Code extension v0.3.1 marketplace update** — VSIX built, blocked on PAT renewal

---

## Planned Features

### Q2 2026 — Polish & Ecosystem
- [x] Linux packages (.deb, .rpm)
- [x] Windows Terminal integration (custom profile)
- [ ] VS Code Web extension
- [ ] PyPI distribution for Python SDK
- [x] LangChain tool integration
- [ ] JupyterLab extension
- [ ] Publish Linguist PR (pending ecosystem growth)
- [x] AI discoverability (llms.txt, AGENTS.md, ai-plugin.json, OpenAPI, IDE rules)
- [x] PyPI distribution for Python SDK

### Q3 2026 — System AI Assistant
- [x] System AI assistant mode (`ae assist`)
- [x] Context-aware command suggestions
- [x] Natural language → AetherShell transpilation
- [x] Conversation memory and session persistence
- [ ] Proactive monitoring and alerting

### Q4 2026 — Cloud & Scale
- [ ] Cloud platform (hosted AetherShell instances)
- [ ] Remote REPL via WebSocket
- [ ] Team workspaces with shared agents
- [ ] Marketplace for community plugins and agents
- [ ] Telemetry and usage analytics (opt-in)

### v1.0.0 — Production Release
- [ ] Stability freeze and backward compatibility guarantees
- [ ] Comprehensive API documentation (rustdoc)
- [ ] Security audit by third party
- [ ] Long-term support (LTS) commitment
- [ ] Migration guide from bash/zsh/PowerShell

---

## Known Issues

### Resolved
- [x] Parser: pipeline assignments without parentheses
- [x] Parser: proper semicolon statement separation
- [x] Parser: newline-aware word-call parsing
- [x] Parser: zero-parameter lambdas
- [x] Builtins: consistent call syntax (pipeline vs function)
- [x] All 88 compiler warnings fixed

### Open
- [ ] VS Code marketplace PAT expired (publisher `admercs`)

---

## Metrics

### Test Coverage
- **Tests:** 1,169 passing (0 failed, 19 ignored)
- **Test Files:** 59 integration test files
- **Test Suites:** 62 (unit + integration)
- **Pass Rate:** 100%
- **CI Status:** All workflows passing

### Codebase
- **Language:** Rust
- **Source Lines:** ~86,000 (src/)
- **Test Lines:** ~14,000 (tests/)
- **Builtins:** 215+
- **Modules:** 38
- **Themes:** 38

### Distribution
- **crates.io:** [aethershell v0.3.1](https://crates.io/crates/aethershell) + [aethershell-lsp v0.2.0](https://crates.io/crates/aethershell-lsp)
- **VS Code:** [admercs.aethershell](https://marketplace.visualstudio.com/items?itemName=admercs.aethershell)
- **GitHub:** [nervosys/AetherShell](https://github.com/nervosys/AetherShell) — v0.3.1 release with binaries
- **Homebrew:** `brew install nervosys/tap/aethershell`
- **Docker:** `docker pull nervosys/aethershell`
- **npm:** `npm install @nervosys/aethershell`

---

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines. All contributions require a signed [CLA](CLA.md).

**Priority areas:**
1. **High** — Ecosystem growth (`.ae` scripts, tutorials, blog posts)
2. **High** — Documentation improvements and examples
3. **Medium** — New builtins and module extensions
4. **Medium** — Platform-specific integrations (Linux packages, Windows Terminal)
5. **Good First Issues** — Theme additions, example scripts, typo fixes

---

## Contact

- **GitHub Issues:** [Report bugs or request features](https://github.com/nervosys/AetherShell/issues)
- **Discussions:** [Join the conversation](https://github.com/nervosys/AetherShell/discussions)
- **Website:** [nervosys.ai](https://nervosys.ai)
