# Changelog

All notable changes to AetherShell will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **New crate `agentic-eval`** (`crates/agentic-eval`) — a standalone, reusable
  library for evaluating how well a *program* serves an agentic AI system across
  four axes, each with tests: **token efficiency** (standing-context + input +
  output + retry cost under OpenAI GPT-4 `cl100k`, GPT-4o `o200k`, and a documented
  Claude approximation; exact with `--features real-tokens`, heuristic otherwise),
  **determinism** (byte-stability of output across runs), **reliability** (pass rate
  + structured/actionable-failure rate over representative invocations), and
  **safety** (blast-radius gating score from a program's declared effects under an
  agent policy). Execution-agnostic (closures/effects supplied by the caller),
  zero heavy deps by default. Includes an `evaluate` example and 21 tests.
- **Cross-tokenizer benchmark check (§4/Phase 1).** `examples/token_bench.rs` now
  re-runs the §4 cipher-vs-legible criterion under a second real BPE (GPT-4o
  `o200k_base`) in addition to `cl100k_base`, and the legible-first verdict holds
  under both (standing-context ~11× under each) — confirming the result isn't
  cl100k-specific. Corpus broadened from 10 to 13 tasks. (Anthropic ships no offline
  Claude tokenizer crate, so `o200k_base` serves as the cross-provider proxy.)
- **Streaming evaluation (§6.3).** New `eval::eval_stream(code, env, on_item)`
  produces results incrementally instead of materializing the whole value first.
  For a final pipeline `source | map/where/filter…` over an array source, each
  element is pushed through the stage chain one at a time and emitted as soon as it
  survives — true stage-by-stage streaming, lazy per element. Each stage is driven
  through the same pipe mechanism the eager evaluator uses (no second map/where
  implementation; `eval_expr`/`eval_program` untouched). Whole-collection stages
  (sort/take/reduce/uniq) or non-array sources fall back to eager evaluation, still
  emitting elements after, so correctness holds for every input. The
  `/api/v1/stream/eval` SSE route now emits `chunk` events as items are produced.
- **MCP stdio transport (§9).** `ae [--agent] mcp stdio` runs a strict JSON-RPC 2.0
  MCP server over stdin/stdout (the canonical MCP transport) — `initialize`,
  `tools/list`, `tools/call`, `ping` — exposing every builtin as a tool routed
  through the safety model (policy/jail/approval/audit). `McpServer::serve_stdio`
  drives the loop; `McpServer::handle_rpc` is the unit-tested per-request dispatch.
- **Nested transactions (§9).** `tx_begin` now nests: a second begin while a
  transaction is active pushes a child frame (SQL nested-transaction semantics). A
  child `tx_commit` folds its changes into the parent (nothing is durable until the
  outermost commit); a child `tx_rollback` reverts only the child's operations and
  leaves the parent open. Each frame captures its own pre-image, so inner rollback
  is correct even when an outer frame touched the same path; outer rollback still
  undoes everything. `tx_begin`/`tx_status` report nesting `depth`. `apply` now
  nests cleanly inside an outer transaction instead of erroring.
- **RBAC startup config (§13).** `RbacManager::from_config_str` parses a TOML
  config (roles with permissions + inheritance; principals with role assignments +
  direct grants), and `safety::init_rbac_from_env()` (called from `main` at boot)
  loads it from `AETHER_RBAC_CONFIG` (or `<workspace>/.ae/rbac.toml`), installs the
  manager, and sets the acting principal (`AETHER_PRINCIPAL`, else the config's
  `principal`). The authorization model can now be configured from a file at
  startup, not just via in-shell `rbac_*` calls.
- **Plan/Apply `copy` and `move` ops (§9).** `plan`/`apply` now support `copy` and
  `move` operations alongside `write`/`append`/`rm`/`mkdir`; each takes a `dest` (or
  `to`) destination path. Both endpoints are workspace-jailed and snapshotted, so a
  copy/move participates in the atomic transaction and rolls back cleanly on failure.

### Changed
- **Boundary type-checking (§8) — reliability.** The shared argument-extraction
  helpers (`expect_string`/`expect_int`/`expect_array`/`need_lambda`, ~90 call
  sites) now emit a structured, catchable `E_BAD_ARG` (`safety::bad_arg`) naming
  both the expected and the actual type, instead of ad-hoc `anyhow!` prose. A
  wrong-typed argument to any builtin using them is now branchable by agents
  (caught by try/catch as `{error:{code,message,hint,…}}`) for self-correction.
  The arity (missing-arg) counterpart `arg(builtin, args, idx, expected)` gives the
  same structured `E_BAD_ARG`; the core agent-facing verbs (`map`/`where`/`reduce`/
  `take`/`call`/`agent`/`swarm`/`mcp_call`) now use it. `type_builtin_call` static
  inference also gained shape-preserving array transforms (`sort`/`uniq`/`take`/
  `head`/`tail`) and `len`/`wc` → `Int`. **All ~490 remaining prose arity/usage
  errors across the builtins (plus the evaluator's lambda-arity checks) are now
  structured `E_BAD_ARG`** via `safety::arg_err(message)` — message text preserved,
  error type upgraded, so every argument/arity failure is branchable and renders as
  legible prose for humans. Non-argument errors (API-response parsing, feature
  gating, URL validation, workflow definitions) were intentionally left untouched.
- **Richer human error rendering (§8) — legibility.** The human REPL unpacks an
  uncaught `SafetyError` into legible prose — `error[CODE]: message`, an indented
  `hint:` line, and (for approvable actions) the exact `AETHER_APPROVE=…` re-run
  incantation — instead of printing the raw JSON. Agent mode still emits the JSON
  so the structured fields survive for programmatic branching.

### Added
- **Resource governors (§7.6)** — a per-run blast-radius envelope enforced at the
  `guard()` chokepoint, agent-mode only, all limits opt-in via env (unset =
  unlimited). A breach returns the structured, non-retryable `E_BUDGET_EXCEEDED`
  so an agent stops instead of looping:
  - `AETHER_MAX_OPS` — total guarded operations per run.
  - `AETHER_MAX_FILES` — filesystem ops (WriteLocal + Destructive).
  - `AETHER_MAX_PROCS` — process/exec ops (Process + Exec).
  - `AETHER_MAX_NET` — network egress ops (Network). Enforced by routing **every
    egress builtin** through `guard()` with the `Network` effect via the
    `guard_network` helper — `http_get`, `curl_exec`, `wget_download`, and the full
    `web_*` fetch family (`web_fetch`/`web_get`/`web_post`/`web_json_get`/
    `web_json_post`/`web_scrape`/`web_download`/`web_headers`/`web_cookies`/
    `web_form_submit`/`web_upload_file`/`web_rest_api`/`web_graphql`/`web_check_url`;
    `web_robots_txt`/`web_sitemap` inherit via delegation) — which also brings every
    network call under the hash-chained audit log.
  - `AETHER_TIMEOUT_MS` — wall-clock budget since the first guarded op.

  New builtins `governor_status()` (counts/limits/elapsed, also folded into
  `safety_status()`) and `governor_reset()` (start a fresh envelope). New
  `E_BUDGET_EXCEEDED` safety error code.
- **Secret hygiene (§7.6)** — two deterministic defenses keep credentials out of
  the agent's context window and the audit log, opt-out via `AETHER_REDACT=off`:
  - **Shape redaction**: known secret *shapes* — provider key prefixes
    (`sk-`/`sk-ant-`, `ghp_`/`gho_`…, `xox*-`, `AIza…`, Stripe `sk_live`/`rk_test`),
    AWS access-key ids (`AKIA…`), JWTs, PEM `PRIVATE KEY` blocks,
    `scheme://user:password@host` URL credentials (password only), and
    `key=secret`/`key: secret` assignment forms — are replaced with `[REDACTED]`
    on the **agent render path** and in **every audit entry** before it is hashed
    and persisted. Ordinary text is unchanged byte-for-byte; the hash chain still
    verifies over redacted content.
  - **Env name gating**: in agent mode, reading a secret-*named* env var (`*_KEY`,
    `*TOKEN*`, `*SECRET*`, `*PASSWORD*`, … — `KEY` alone excluded) via
    `env`/`sys.env`/`env.var`/`env.vars` returns an opaque `[REDACTED:NAME]` handle
    instead of the value, unless `AETHER_SECRETS=allow`. Human mode returns the
    value (legibility).
- New env vars: `AETHER_REDACT=off` (disable all redaction) and
  `AETHER_SECRETS=allow` (permit clear reads of secret-named env vars in agent
  mode).

## [1.3.1] - 2026-06-01

Patch: workspace-jail/path-resolution correctness and security fixes for agent
mode. No API changes.

### Fixed
- `file_write` and `file_append` are now jailed to the workspace in agent mode
  (the `WriteLocal` effect guard, matching `rm`/`rmdir`): a write to an absolute
  path outside the workspace is rejected with `E_OUTSIDE_WORKSPACE`. Allowed by
  policy (no approval prompt) — only the workspace containment is enforced. Human
  mode is unaffected.
- In a **jailed context** (agent mode or an explicit `AETHER_WORKSPACE`), relative
  paths passed to effecting builtins (`file_write`/`file_append`/`rm`/`rmdir`) now
  resolve against the **workspace root** instead of the process CWD. This closes a
  soundness gap where a relative-path write could escape the workspace yet pass the
  jail check (when CWD ≠ workspace), and keeps the write, the jail check, and the
  transaction journal in agreement. Human mode is unchanged (relative → CWD).
- Path access is no longer sandboxed to the project directory in **human mode** —
  `ae` now behaves like a normal interactive shell (read/list/write any path).
  The workspace jail applies only in **agent mode** (`--agent`/`AETHER_MODE=agent`)
  or when a workspace/allowed-base-dir is explicitly configured. Always-on hygiene
  (null-byte, length, blocked-pattern, symlink checks) is unchanged.

## [1.3.0] - 2026-05-31

Agentic-first release: AetherShell is now optimized end-to-end for AI agents —
token-efficient structured output, a real safety/transaction model, and a unified
single-pass agentic parser. Backward-compatible; all additions, measured with the
real GPT-4 cl100k tokenizer.

### Added
- **AECON (Aether Compact Object Notation)** — compact structured output: a
  header-once tabular form with three deterministic, gated compression levers:
  constant-column factoring (`@const`), dictionary encoding for low-cardinality
  string columns (`@dict`), and delta encoding for large slowly-varying integer
  columns (`@delta`). ~2.8× fewer output tokens than POSIX shells and ~2.6× vs
  PowerShell's parseable JSON on realistic tabular results.
- **Lossless reversibility** — `aecon_decode` reverses the tabular form; optional
  `@type` tags make numeric-looking strings and integral floats round-trip exactly.
- **Agent-mode default rendering** — under `AETHER_MODE=agent`, results render as
  compact AECON automatically across the CLI, HTTP Agent API, and MCP server.
- **Token-economy builtins** — `aecon`, `aecon_decode`, `pick` (source-side field
  projection), `budget` (token-bounded paging with a cursor), `digest`
  (constant-token structural summary), `canonical` (deterministic JSON), `tokens`
  (cl100k estimate), and `ontology_manifest`/`ontology_describe` (progressive
  ontology disclosure).
- **`--deterministic` / `AE_DETERMINISTIC`** — render results as canonical,
  byte-stable JSON for snapshot tests, content-addressable caching, and diffs.
- **Safety model** — an effect taxonomy (Pure → Privileged), a
  capability→policy→approval→audit pipeline, content-bound approval tokens, a
  workspace jail, a hash-chained tamper-evident audit log, RBAC, structured `E_*`
  errors, and `safety_status` introspection. CLI flags `--agent`/`--workspace`/
  `--policy`.
- **Filesystem transactions & checkpoints** — `tx_begin`/`tx_commit`/`tx_rollback`/
  `tx_status` with **named savepoints** (`tx_savepoint`/`tx_rollback_to`) for
  partial rollback. Covers file writes/appends, deletes, recursive directory
  trees, sqlite databases, and the key-value store. No conventional shell offers
  this.
- **`plan` / `apply`** — Terraform-style declarative destructive batches: a
  reviewable typed plan plus a content-bound approval token, applied atomically
  inside a transaction with automatic rollback on any failure.
- **Persistent key-value store** — `db_kv_get`/`set`/`delete`/`keys`/`store`,
  transactional via the sqlite snapshot chokepoint.
- **Grammar additions** — native `|.field` projection, SI numeric suffixes
  (`1k`/`1M`/`1G`), `~x: body` lambdas, `?val{arms}` match, and
  `if cond { … } else { … }` expressions.
- **MCP server** now exposes builtins as effect-tagged tools; the **HTTP Agent
  API** gained per-call token accounting and real chunked streaming.
- **Cross-shell token benchmark** (`cargo run --example shell_bench --features
  real-tokens`) comparing AetherShell to Bash/Zsh/Fish/Nushell/PowerShell.
- **Real GPT-4 cl100k tokenizer** behind `--features real-tokens` for exact,
  authoritative token measurement (heuristic otherwise).

### Changed
- **Agentic `.aeg` transpiler rewritten as a single tokenizing pass** (Phase 5),
  retiring the former 10-pass text-substitution pipeline (~2,000 lines removed).
  Terse tokens can no longer be mis-expanded by pass ordering.
- Measure-first verdict committed the agent surface to **legible-first** syntax;
  the `.aeg` cipher remains an opt-in legacy surface.

### Fixed
- Zero-warning build restored across all targets.
- Two stale PowerShell transpiler integration tests (now assert the emitted
  `file.read`/`proc.list`); an `AETHER_MODE` env-var test race serialized.

## [1.2.0] - 2026-02-15

### Added
- Package registry client connecting marketplace builtins to packages.nervosys.ai
- Remote search, install, publish, update, and yank for community packages
- Tarball download and extraction for registry packages (flate2 + tar)
- Auth token support via `AETHER_REGISTRY_TOKEN` / `AETHERSHELL_TOKEN` env vars
- Expanded Candle backend with transformer architectures (Llama, Mistral, Phi)

### Changed
- Version bump from 0.3.1 to 1.2.0 across all project files and metadata
- Marketplace builtins now try remote registry first with local fallback

### Fixed
- Security test race condition — Mutex serialization for `configure_command_security()` tests
- Flaky `friendly_user_allowed_commands` test now stable (5/5 consecutive runs)

## [0.3.1] - 2026-02-10

### Changed
- License changed to AGPL-3.0-or-later with commercial dual-license
- Crate package size optimized from 10.1 MiB to 733 KiB compressed

### Added
- Contributor License Agreement (CLA) with CI enforcement
- GitHub Linguist submission package (samples, grammar, guide)
- README badges for CI, downloads, VS Code marketplace
- Windows Terminal profile fragment (`editors/windows-terminal/`)

### Fixed
- All 88 compiler warnings resolved (zero-warning build)
- Python SDK license corrected to AGPL-3.0
- Unused `SchemaFormat` import in binary crate

## [0.3.0] - 2026-01-30

### Added
- Implicit match scrutinee in lambda bodies
- Python SDK (`integrations/python/`)
- GitHub Actions CI/CD (ci.yml, release.yml, docker.yml, security-audit.yml)
- CLA check workflow
- Bash transpiler improvements

## [0.2.0] - 2026-01-15

### Added - Plugin System
- Plugin architecture with dynamic TOML manifest loading
- 7 plugin builtins: `plugins()`, `plugin_info()`, `plugin_enable()`, `plugin_disable()`, `plugin_load()`, `plugin_unload()`, `plugin_categories()`
- Example plugins (hello-plugin, math-utils, string-utils)
- Built-in file handlers (JSON, CSV, TOML)

### Added - Language Features
- Async/await syntax (`async fn(x) => expr`, `await expr`)
- Try/catch/throw error handling
- Conditional compilation (`#[cfg(platform)]`, `#[cfg(feature = "name")]`)
- Module visibility (pub/private, export, re-export)
- Package management (import syntax, aether.toml, registry)
- N-ary lambda support (3+ parameters)
- Zero-parameter lambdas
- Standard library (7 modules in lib/)
- Platform detection builtins
- Runtime feature flags
- Debugging tools (debug, trace, assert, inspect)
- Error recovery with multi-error reporting

### Added - Enterprise
- RBAC (role_create, role_grant, check_permission)
- Audit logging (audit_log, audit_query, audit_export)
- SSO integration (sso_init, sso_auth, sso_validate)
- Compliance reporting

### Added - AI
- RAG (Retrieval-Augmented Generation)
- Knowledge graphs
- Semantic caching
- Fine-tuning management

### Added - Infrastructure
- LSP server (aethershell-lsp crate with tower-lsp)
- VS Code extension v0.2.0 (hover, symbols, folding)
- Distributed computing builtins (cluster, job scheduling)
- 5 benchmark suites (parser, eval, pipeline, builtin, MCP)
- WASM support with browser REPL
- Distribution: Homebrew, Docker, npm, browser extension

## [0.1.0] - 2025-10-22

### Added - Core Language Features
- **Typed Functional Language**: Hindley-Milner type inference
- **Structured Data**: Records, Arrays, Tables (not text streams)
- **First-Class Functions**: Lambdas, closures, higher-order functions
- **Pattern Matching**: Match expressions with guards
- **Pipelines**: Typed data transformation pipelines
- **Mutable Variables**: `mut` keyword for mutable bindings
- **Dot Notation**: Field access on records (`record.field`)

### Added - AI Integration
- **Multi-Modal AI**: Native support for images, audio, and video
- **AI Function**: `ai(prompt)` and `ai(prompt, {images: [...], audio: [...], video: [...]})`
- **Multi-Agent System**: Agent creation, coordination, and swarms
- **Agent Protocols**:
  - MCP (Model Context Protocol) for tool integration
  - A2A (Agent-to-Agent) communication
  - NANDA (Negotiation And Dynamic Agents) consensus
- **AI Model Management**: OpenRouter-style API server
- **Local Model Support**: XDG-compliant storage and format conversion
- **Provider Support**: OpenAI, Anthropic, Ollama, and custom backends
- **LLM Backend Integration**: vLLM, TensorRT-LLM, SGLang, llama.cpp

### Added - Builtins
- **File Operations**: `read_text(path)`, `ls(path)`
- **Data Operations**: `map()`, `where()`, `reduce()`, `group()`
- **Type Operations**: `type_of(value)`, `keys(record)`, `len(value)`
- **HTTP**: `http_get(url)`, `http_post(url, data)`
- **Utilities**: `print()`, `range()`, `take()`, `first()`

### Added - Terminal UI
- **Interactive TUI**: Modern terminal interface (`ae tui`)
- **Media Viewer**: Display images, audio, video in terminal
- **Chat Interface**: Conversational AI with multimodal support
- **Agent Dashboard**: Monitor and control agent swarms
- **Media Selection**: Batch selection for multimodal queries

### Added - Infrastructure
- **Bash Compatibility**: Transpiler for running .sh scripts
- **REPL Mode**: Interactive shell with command history
- **Script Execution**: Run .ae files directly
- **XDG Compliance**: Standard config and data directories
- **Cross-Platform**: Windows, Linux, macOS support

### Added - AI Model CLI (`aimodel`)
- **Server Management**: Start/stop API server with OpenAI-compatible endpoints
- **Model Management**: List, search, download, remove models
- **Format Conversion**: Convert between GGUF, SafeTensors, PyTorch, ONNX, TensorFlow
- **Storage Management**: XDG-compliant local storage with statistics
- **Provider Configuration**: Manage API keys for OpenAI, Anthropic, etc.
- **Backend Management**: Auto-detect and configure vLLM, SGLang, TensorRT-LLM, llama.cpp
- **Alias System**: Create shortcuts for frequently used models

### Added - Examples
- `00_hello.ae`: Hello world and basic syntax
- `01_pipelines.ae`: Pipeline transformations
- `02_tables.ae`: Table operations and file system queries
- `03_http.ae`: HTTP requests with typed responses
- `04_match.ae`: Pattern matching examples
- `05_ai.ae`: AI integration basics
- `06_agent.ae`: Agent deployment
- `07_uri_types.ae`: URI type system
- `12_multi_agent_orchestration.ae`: Multi-agent architecture patterns
- `13_multimodal_ai.ae`: Multi-modal AI concepts
- `14_typed_pipelines.ae`: Type-safe functional programming
- `15_ai_protocols.ae`: MCP, A2A, NANDA demonstrations
- `16_mcp_servers.ae`: MCP server concepts
- `17_syntax_showcase.ae`: Core language features
- `18_mutable_variables.ae`: Mutable variable patterns
- `19_showcase.ae`: Pipeline showcase
- `98_dot_notation.ae`: Comprehensive dot notation examples
- `99_comprehensive_test.ae`: All features validation

### Added - Documentation
- Comprehensive README with feature overview
- Quick Reference guide for all syntax
- MCP Servers integration guide
- AI Protocols technical report
- Competitive analysis
- Project structure documentation
- Contributing guidelines
- Type system deep dive
- TUI usage guide

### Added - Testing
- 25+ unit tests for core language features
- Integration tests for AI functionality
- TUI component tests
- Multimodal AI backend tests
- Bash transpiler tests
- Example validation (18/18 passing)
- OS tools cross-platform tests

## Technical Details

### Language Design
- **Parser**: Hand-written recursive descent parser
- **Type System**: Hindley-Milner with type inference
- **Evaluator**: Tree-walking interpreter with environment chains
- **Values**: Sum type for Int, Float, String, Bool, Array, Record, Table, Lambda, Uri, Null
- **Error Handling**: Anyhow for ergonomic error propagation

### AI Architecture
- **Backend Trait**: `MultiModalLlmBackend` for provider abstraction
- **Message Format**: Unified message structure for all providers
- **Media Processing**: Image display (viuer), audio playback (rodio)
- **API Server**: Axum-based HTTP server with OpenAI-compatible endpoints
- **Format Conversion**: Support for 5+ model formats with quantization
- **Local Storage**: XDG Base Directory compliance

### TUI Implementation
- **Framework**: Ratatui for rendering
- **State Management**: Application state machine with tab navigation
- **Media Display**: Terminal-based image rendering and audio waveforms
- **Input Handling**: Crossterm for keyboard/mouse events

### Performance
- **Memory Safe**: Rust's ownership system prevents memory bugs
- **Zero Copy**: Efficient data handling where possible
- **Async Support**: Tokio runtime for concurrent operations
- **Lazy Evaluation**: Pipelines evaluate on-demand

## Breaking Changes

N/A - Initial release

## Migration Guide

N/A - Initial release

## Known Issues

- String multiplication (e.g., `"=" * 50`) not supported - use literal strings
- If statements only work in expression context (ternary) - use `match` for statement-level conditionals
- Pipeline ending with `| len` requires workaround - split into two statements
- Numeric array indexing (`array.0`) not supported - use `map` or functional patterns

## Future Plans

See [ROADMAP.md](ROADMAP.md) for upcoming features.

[Unreleased]: https://github.com/nervosys/AetherShell/compare/v1.2.0...HEAD
[1.2.0]: https://github.com/nervosys/AetherShell/compare/v0.3.1...v1.2.0
[0.3.1]: https://github.com/nervosys/AetherShell/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/nervosys/AetherShell/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/nervosys/AetherShell/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/nervosys/AetherShell/releases/tag/v0.1.0
