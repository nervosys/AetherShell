# AetherShell Roadmap

> **Last Updated:** February 19, 2026

This document tracks the development progress of AetherShell, the world's first agentic shell with typed functional pipelines and multi-modal AI.

---

## Progress Overview

| Category          | Status     | Details                                          |
| ----------------- | ---------- | ------------------------------------------------ |
| Core Language     | ✅ Complete | AST-based, Hindley-Milner                        |
| Type System       | ✅ Complete | Full inference                                   |
| Builtins Library  | ✅ Complete | 1,100+ functions, 106 modules                    |
| AI Integration    | 🔄 Active   | 9 providers, unifying stacks                     |
| Agent Framework   | ✅ Complete | Single + swarm + A2A                             |
| AI Ontology       | 🔄 Active   | 1,100+/1,100+ builtins, 106 modules discoverable |
| TUI Interface     | ✅ Complete | Tabs, chat, dashboard                            |
| Theme System      | ✅ Complete | 38 themes                                        |
| Config System     | ✅ Complete | XDG-compliant                                    |
| Plugin System     | ✅ Complete | Dynamic loading, TOML                            |
| Standard Library  | ✅ Complete | 7 modules (lib/)                                 |
| Performance       | ✅ Complete | 5 benchmark suites                               |
| Test Coverage     | ✅ Complete | 1,237 tests, 100% pass                           |
| Documentation     | ✅ Complete | Comprehensive                                    |
| Publishing        | ✅ Complete | crates.io v0.3.1                                 |
| WASM Support      | ✅ Complete | Browser REPL ready                               |
| Enterprise        | ✅ Complete | RBAC, Audit, SSO                                 |
| LSP Server        | ✅ Complete | tower-lsp, crates.io                             |
| VS Code Extension | ✅ Complete | Marketplace published                            |
| MCP Protocol      | ✅ Complete | 130+ tools, HTTP server                          |
| Agent API         | ✅ Complete | 27 provider schema formats                       |
| Distribution      | ✅ Complete | Homebrew, Docker, npm                            |
| CI/CD             | ✅ Complete | GitHub Actions, CLA check                        |
| Licensing         | ✅ Complete | AGPL-3.0 + commercial                            |

---

## Version History

### v0.3.1 (Current) — February 2026
- [x] CLI tool wrappers (tools 1-150): 200+ builtins wrapping common CLI tools — process management (tmux, screen, htop), disk/filesystem (ncdu, duf, dust), debugging (gdb, valgrind, strace, ltrace), binary inspection (objdump, readelf, nm, ldd, strings, file), networking (iperf3, nmap, mtr, dig, nc), modern CLI replacements (bat, fd, rg, sd, zoxide, fzf, jq, yq, delta, hyperfine, tokei, just, direnv), version/runtime managers (asdf, mise, nvm, pyenv, rbenv), language toolchains (cargo, rustup, go, node, npm, pnpm, yarn, bun, deno, uv, pipx, poetry, pytest), dev tools (gh, glab, pre-commit, make, cmake, ninja, nodemon), containers (buildah, skopeo, trivy, podman-compose, docker-compose), linters/formatters (shellcheck, shfmt, black, ruff, eslint, prettier, clippy, rustfmt, golangci-lint)
- [x] Cross-platform OS commands: 219 new builtins for containers, Kubernetes, VMs/hypervisors, cloud/IaC, remote access, security, and monitoring
- [x] Cross-platform consistency audit: 100+ `cfg` blocks across 6 files reviewed, 40+ functions fixed for OS parity
- [x] Structured output: converted 20+ builtins from raw text to typed Records/Arrays (net_ping, net_traceroute, net_arp, net_route, net_ports, net_connections, net_bandwidth, svc_logs, cron_list, pkg_list, pkg_search, pkg_info, pkg_files, pkg_owner, sys_uptime, sys_info, proc_children, startup_list, and more)
- [x] PascalCase normalization: all Windows builtins using `json_to_value()` now return consistent lowercase field names (10 sites fixed)
- [x] macOS support: added missing implementations for 15+ builtins (tcpdump, ss_info→netstat, sys_uptime→sysctl, sys_info→sw_vers, net_bandwidth→netstat -ib, dmesg, fdisk_list→diskutil, proc_children→pgrep, startup_list→launchctl)
- [x] Cfg gate fixes: `dd_copy` and `chroot` widened from Linux-only to Unix (not(windows)), since both tools ship natively on macOS
- [x] Unified 20 builtins to use cross-platform Rust crates (sysinfo, chrono, sha2, md5, which) instead of OS-specific commands
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
- [x] 430+ builtins across functional, string, array, math, file, OS, container, VM, cloud, security categories
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

### Builtins (1,100+ functions in 106 modules)
- [x] `file`, `sys`, `proc`, `fs`, `net`, `http`, `gui`, `web`
- [x] `crypto`, `db`, `svc`, `cron`, `archive`, `user`, `perm`, `pkg`
- [x] `hw`, `clip`, `input`, `ai`, `agent`, `math`, `str`, `arr`, `json`
- [x] `mcp`, `shell`, `platform`, `a2ui`, `a2a`, `nanda`
- [x] `rbac`, `audit`, `sso`, `cluster`, `nn`, `evo`, `rl`
- [x] `docker`, `podman`, `container` — Container management
- [x] `k8s`, `helm` — Kubernetes orchestration
- [x] `vm`, `hyperv`, `virsh`, `wsl`, `qemu`, `lxc` — VM/hypervisor management
- [x] `terraform`, `ansible`, `pulumi`, `vagrant`, `packer` — Cloud/IaC
- [x] `ssh`, `scp`, `rsync`, `rdp` — Remote access
- [x] `firewall`, `selinux`, `apparmor`, `ssl` — Security
- [x] `monitor`, `perf`, `netstat` — System monitoring
- [x] `tmux`, `screen`, `valgrind`, `gdb`, `objdump`, `readelf` — Process/debug/binary tools
- [x] `zoxide`, `just`, `direnv`, `asdf`, `mise` — Modern CLI & runtime managers
- [x] `uv`, `pipx`, `poetry`, `cargo`, `rustup`, `go` — Language toolchains
- [x] `node`, `npm`, `pnpm`, `yarn`, `bun`, `deno` — JS/TS ecosystem
- [x] `gh`, `glab`, `pre_commit` — Dev tools
- [x] `buildah`, `skopeo`, `trivy`, `ruff` — Container & lint tools
- [x] `iperf3`, `nc`/`netcat` — Network testing
- [x] Cross-platform consistency: all builtins return structured typed output (Records/Arrays), consistent field names across Windows/Linux/macOS

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
- [x] Proactive monitoring and alerting
- [x] Shell transpilers: Bash, Zsh, PowerShell → AetherShell with block accumulation
- [x] CLI flags `--zsh`/`-z`, `--pwsh`/`-p` and auto-detection by file extension

### Q4 2026 — Cloud & Scale
- [x] Cloud platform (hosted AetherShell instances) — `cloud` module: 8 builtins (deploy, instances, instance_create/destroy/status/connect, regions, config)
- [x] Remote REPL via WebSocket — `repl` module: 5 builtins (serve, connect, sessions, disconnect, broadcast)
- [x] Team workspaces with shared agents — `workspace` module: 8 builtins (create, list, join, leave, members, share_agent, agents, sync)
- [x] Marketplace for community plugins and agents — `marketplace` module: 8 builtins (publish, search, install, uninstall, list, info, rate, update)
- [x] Telemetry and usage analytics (opt-in) — `telemetry` module: 6 builtins (enable, disable, status, report, events, reset)

### v1.0.0 — Production Release
- [x] Stability freeze and backward compatibility guarantees
- [x] Comprehensive API documentation (rustdoc)
- [x] Security audit (internal: 11 findings, all CRITICAL/HIGH remediated)
- [x] Long-term support (LTS) commitment (docs/LTS.md)
- [x] Migration guide from bash/zsh/PowerShell
- [x] Shell transpilers for seamless adoption (Bash, Zsh, PowerShell)

### v1.1.0 — AI Provider Unification & Ontology

**Goal:** Make every builtin, module, and AI provider machine-discoverable through a unified ontology, and consolidate the three parallel provider systems into one.

#### Provider Architecture
- [x] **Unify provider routing** — expanded `complete_sync_router()` in `ai.rs` to support 19+ providers via OpenAI-compatible endpoints with `complete_via_compat()` helper and `provider_base_url()` registry; model URI syntax `provider:model` supported
- [ ] **Implement `LLMProvider` trait** for all 19 declared providers — bridge.rs and impls/ exist on disk but need trait alignment with current API; routing currently handled via OpenAI-compat endpoints
- [ ] **Activate provider routing & fallback** — connect `RoutingRule`/`RoutingCondition` engine to live completions (routing rules defined, not yet wired)
- [x] **Add Ollama to `ai_api` server** — OllamaProvider with chat, embeddings, model listing, and auto-pull support
- [x] **LM Studio explicit detection** — auto-detect at `:1234` (OpenAI-compatible), `lmstudio:` URI scheme, LMStudioProvider in ai_api
- [x] **Model cost tracking** — `CostTracker` with `COST_TRACKER` global, `estimate_cost()` for 25+ models across 10 providers, `track_usage()` wired into `complete_via_compat()`, builtins `ai_usage()`, `ai_cost()`, `ai_reset_usage()`
- [x] **Update model metadata** — OpenAI pricing updated for GPT-4o/4.5/o3/o4-mini (Feb 2026), Anthropic expanded to 6 models (Claude 4 Opus/Sonnet, 3.5 Sonnet/Haiku, 3 Opus/Sonnet), embeddings pricing current

#### Local AI Infrastructure
- [x] **Implement `LocalProvider`** — full implementation with model directory scanning, llama.cpp delegation for chat/embeddings, GGUF/SafeTensors/ONNX format detection, local model metadata extraction
- [x] **Ollama model auto-pull** — `OllamaProvider::pull_model()` and `has_model()` methods for on-demand model fetching
- [x] **Backend health monitoring** — Ollama and LM Studio detection in `detect_backends()`, health checks via `validate_api_key()`
- [x] **GPU memory management** — `query_gpu_memory()` via nvidia-smi, `estimate_model_vram_mb()` with quantization-aware sizing (Q4/Q5/Q8/FP16/FP32), `model_fits_gpu()` assessment, builtins `ai_gpu_memory()` and `ai_model_fits()`
- [x] **Model format conversion** — wired `ai_api/converters.rs` to builtins: `ai_convert_model()`, `ai_supported_conversions()`, `ai_detect_format()` for GGUF ↔ SafeTensors ↔ ONNX ↔ PyTorch ↔ TensorFlow conversion

#### Ontology & Discoverability
- [x] **Dynamic builtin discovery** — `get_all_builtin_definitions()` auto-generates schemas from `BUILTIN_LOOKUP` for all 1,100+ builtins with categories, descriptions, return types, and alias detection
- [x] **Connect OS ontology to Agent API** — wired `OS_ONTOLOGY` and `CLI_TOOL_REGISTRY` into `build_language_ontology()` with `os_ontology` and `cli_tools` fields; all 20+ schema builders use unified `ontology_tools()` helper
- [x] **Unify tool schema systems** — added `ToolFormat` enum, `ToolSchema::from_builtin()`, `builtins_to_tools()` bridge in `providers/schema.rs`; all schema builders refactored to use single source of truth, eliminating ~400 lines of duplicated format logic
- [x] **CLI tool ontology mappings** — `CLIToolRegistry` with 90+ tool definitions across 15 categories, `detect_tool()` using `which` crate, `get_tool_version()`, `to_json_schema()` export; `CLI_TOOL_REGISTRY` lazy_static wired into Agent API
- [x] **Module-level schemas** — `get_module_definitions()` exposes typed signatures for all 106 modules with function mappings; `build_compact_ontology()` includes module data
- [x] **Runtime type introspection** — builtins `typeof()`, `type_name()`, `type_fields()`, `type_schema()`, `is_type()` expose runtime Value types; `type_schema()` generates JSON Schema from any Value; `type_name()` provides detailed inner types (e.g., `Array<Int>`, `Record{a: Int, b: String}`)
- [x] **Ontology versioning** — `ontology_version` field on `LanguageOntology`, included in compact and full schema exports
- [x] **Ontology export formats** — added JSON-LD (`to_json_ld()`), OWL/RDF Turtle (`to_owl_turtle()`), and SHACL (`to_shacl()`) exports on `OSOperationRegistry`; Agent API schema endpoint supports `jsonld`, `owl`, `shacl` format params; builtins `ontology_json()`, `ontology_jsonld()`, `ontology_owl()`, `ontology_shacl()`
- [x] **Update AI discoverability files** — synced `llms.txt`, `llms-full.txt`, `AGENTS.md`, `copilot-instructions.md` with LM Studio provider, ontology modules, and schema endpoints

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
- [ ] Pre-existing flaky test: `cfg_feature_enabled` (env var race in parallel test runs)
- [ ] 9 LOW-severity builtins still return raw text (`net_whois`, `at_list`, `pkg_sources`, `pkg_history`, `strace`, `sar`, `dmesg` fallback, `fdisk_list` macOS fallback, `capabilities`)
- [ ] Three parallel provider systems (`ai.rs`, `ai_api/providers.rs`, `providers/`) still exist but now unified at routing layer via `complete_via_compat()`
- [ ] `bridge.rs` and `impls/` in providers/ need trait alignment with current `LLMProvider` API (180 stale errors)
- [ ] Provider routing rules defined but not yet wired to live completions
- [ ] `platform` module registered 3x in modules.rs `all_modules()`

---

## Metrics

### Test Coverage
- **Tests:** 1,237 passing (0 failed, 19 ignored)
- **Test Files:** 62 integration test files
- **Test Suites:** 62 (unit + integration)
- **Pass Rate:** 100%
- **CI Status:** All workflows passing

### Codebase
- **Language:** Rust
- **Source Lines:** ~106,000 (src/)
- **Test Lines:** ~15,000 (tests/)
- **Builtins:** 1,100+ (1,381 names including aliases)
- **Modules:** 106
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
1. **Critical** — AI provider unification (merge `ai.rs` + `ai_api/providers.rs` + `providers/` into single registry)
2. **Critical** — Dynamic ontology (auto-generate schemas for all 1,100+ builtins from source)
3. **High** — `LLMProvider` trait implementations (19 providers declared, need concrete impls)
4. **High** — Local AI infrastructure (Candle/ONNX inference, Ollama auto-pull, GPU scheduling)
5. **Medium** — Ecosystem growth (`.ae` scripts, tutorials, blog posts)
6. **Good First Issues** — Update model metadata/pricing, theme additions, example scripts

---

## Contact

- **GitHub Issues:** [Report bugs or request features](https://github.com/nervosys/AetherShell/issues)
- **Discussions:** [Join the conversation](https://github.com/nervosys/AetherShell/discussions)
- **Website:** [nervosys.ai](https://nervosys.ai)
