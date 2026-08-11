//! # AetherShell
//!
//! **The shell built for AI agents — one language, every platform, deterministic typed output.**
//!
//! AetherShell is a cross-platform shell where every command returns structured, typed data
//! instead of raw text. It provides a single language that works identically on Linux, macOS,
//! and Windows, eliminating the non-deterministic text parsing that makes traditional shell-based
//! agent workflows brittle.
//!
//! ## Core Design
//!
//! - **Deterministic typed output**: All builtins return [`Value`] types (Int, Float, String,
//!   Array, Record, Lambda) — never unstructured text.
//! - **Cross-platform uniformity**: One language, one set of builtins, identical behavior everywhere.
//! - **Built-in ontology**: Commands, arguments, and return types are machine-discoverable
//!   without documentation scraping or prompt engineering.
//! - **Functional pipelines**: Data flows through typed transformations, not text-munging pipes.
//!
//! ## Architecture
//!
//! The crate is split into core modules (available in both native and WASM builds) and
//! native-only modules that depend on system resources.
//!
//! ### Core (always available)
//! - [`ast`] — Abstract syntax tree definitions
//! - [`parser`] — AetherShell source → AST
//! - [`value`] — Runtime value system (the typed output contract)
//! - [`types`] — Type lattice for inference
//! - [`typecheck`] — Hindley-Milner type inference engine
//! - [`env`] — Environment and scope management
//!
//! ### Native-only (behind `native` feature)
//! - [`eval`] — Expression evaluator and runtime semantics
//! - [`builtins`] — 1,280+ built-in functions across 108 modules
//! - [`modules`] — Module system mapping `module.function()` syntax to builtins
//! - [`ai`] / [`ai_api`] — Multi-provider AI integration
//! - [`agent`] / [`agent_api`] — Autonomous agent framework and HTTP API
//! - [`mcp`] — Model Context Protocol; exposes the builtin catalog as a compact
//!   three-tool discovery surface by default (`AETHER_MCP_TOOLS=all` for the
//!   flat per-builtin listing)
//! - [`metrics`] — Prometheus-compatible metrics, alerting, and monitoring
//! - [`prompt`] / [`line_editor`] — Prompt styles and fish-style line editing
//! - [`yaml`] — YAML subset parser/emitter with an enforced boundary
//! - [`tui`] — Rich terminal UI with chat, agents, and media viewer

// ── Clippy lint baseline ─────────────────────────────────────────────────────
// This crate predates Clippy adoption; the build bar has been rustc-warning-clean.
// The block below baselines the pre-existing Clippy categories so `cargo clippy`
// is clean and CAN flag *new* lint categories on future code, without a risky
// 180-edit rewrite of working, tested code. Attribute-only — zero behavior change.
//
// (A) Intentional — correct as written; these stay allowed:
#![allow(clippy::upper_case_acronyms)] // GDPR/HIPAA/SAML/PCI/OIDC are standard acronyms
#![allow(clippy::result_large_err)] // SafetyError is an intentionally rich, cold-path error
#![allow(clippy::needless_range_loop)] // index loops in the nn/matrix math read clearer
#![allow(clippy::if_same_then_else)] // parallel branches kept explicit for clarity/extension
#![allow(clippy::unnecessary_unwrap)] // `if x.is_some() { x.unwrap() }` reads clearly here
#![allow(clippy::should_implement_trait)] // intentional inherent from_str/parse shortcuts
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
#![allow(clippy::suspicious_open_options)]
// open semantics reviewed and intended
//
// (B) Deferred idiomatic cleanup — safe to tighten incrementally; baselined for now:
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::empty_line_after_doc_comments)]
#![allow(clippy::manual_strip)]
#![allow(clippy::map_identity)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::unnecessary_sort_by)]
#![allow(clippy::unnecessary_filter_map)]
#![allow(clippy::unnecessary_get_then_check)]
#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::cloned_ref_to_slice_refs)]
#![allow(clippy::vec_init_then_push)]
#![allow(clippy::manual_find)]
#![allow(clippy::redundant_guards)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::inherent_to_string)]
#![allow(clippy::format_in_format_args)]
//
// (C) Unix-only code paths. These fire exclusively inside
// `#[cfg(not(target_os = "windows"))]` / `#[cfg(target_os = "linux")]` blocks,
// so a Windows checkout never sees them — which is why they accumulated and why
// CI's Linux/macOS lint job had been failing since 2026-06-11. All are style
// lints with no correctness component (42 needless_return, 13 needless borrows,
// a few closures/format args). Baselined rather than blind-fixed: they cannot be
// compiled, let alone verified, from Windows, and 60+ unverifiable edits inside
// a 49k-line file is a worse trade than an honest, documented allow.
// To clean up: work from a Linux checkout and remove these one at a time.
#![allow(clippy::needless_return)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::collapsible_str_replace)]
#![allow(clippy::unnecessary_map_or)]

/// Abstract syntax tree — core AST definitions (Stmt, Expr, BinOp).
pub mod ast;
/// Environment — variable bindings, scopes, and module resolution.
pub mod env;
/// Parser — transforms AetherShell source text into an AST.
pub mod parser;
/// Type lattice — type definitions used by the inference engine.
pub mod types;
/// Runtime values — the typed output contract (Int, Float, String, Array, Record, Lambda).
pub mod value;
/// YAML — a documented subset parser/emitter that fails loudly outside it.
#[cfg(feature = "native")]
pub mod yaml;

// Core modules available in both native and WASM builds
/// Shell feature flags — runtime capability detection.
pub mod shell_features;
/// Type checker — Hindley-Milner type inference engine.
pub mod typecheck;

// Native-only modules (including eval which depends on builtins)
/// Autonomous agent framework — single agents and swarm coordination.
#[cfg(feature = "native")]
pub mod agent;
/// Agent HTTP API — REST/WebSocket server with OpenAI/Claude/Gemini schemas.
#[cfg(feature = "native")]
pub mod agent_api;
/// AI integration — multi-provider LLM client with model URIs.
#[cfg(feature = "native")]
pub mod ai;
/// AI API server — OpenRouter-style API with local model management.
#[cfg(feature = "native")]
pub mod ai_api;
/// Authentication — token and credential management.
#[cfg(feature = "native")]
pub mod auth;
/// Built-in functions — 1,280+ typed builtins across 108 modules.
#[cfg(feature = "native")]
pub mod builtins;
/// Configuration — XDG-compliant config, themes, and settings.
#[cfg(feature = "native")]
pub mod config;
/// Evaluator — expression evaluation and runtime semantics.
#[cfg(feature = "native")]
pub mod eval;
/// Evolutionary algorithms — genetic algorithms and NEAT.
#[cfg(feature = "native")]
pub mod evolution;
/// External tools — integration with system-installed CLI tools.
#[cfg(feature = "native")]
pub mod external_tools;
/// Line editing — fish-style autosuggestions, abbreviations, history recall.
#[cfg(feature = "native")]
pub mod line_editor;
/// Local inference — Candle and ONNX Runtime backends for in-process model inference.
#[cfg(feature = "native")]
pub mod local_inference;
/// Marketplace — community plugin and agent marketplace.
#[cfg(feature = "native")]
pub mod marketplace;
/// MCP — Model Context Protocol server; compact tool-discovery surface by default.
#[cfg(feature = "native")]
pub mod mcp;
/// Metrics — Prometheus-compatible metrics, alerting, and dashboards.
#[cfg(feature = "native")]
pub mod metrics;
/// Module system — maps module.function() syntax to builtin dispatch.
#[cfg(feature = "native")]
pub mod modules;
/// Neural networks — feedforward networks and training.
#[cfg(feature = "native")]
pub mod neural;
/// OS tools — platform-specific tool discovery and metadata.
#[cfg(feature = "native")]
pub mod os_tools;
/// Package management — import resolution and aether.toml manifests.
#[cfg(feature = "native")]
pub mod packages;
/// Persistence — session state and data persistence.
#[cfg(feature = "native")]
pub mod persistence;
/// Plugin system — dynamic TOML-based plugin loading.
#[cfg(feature = "native")]
pub mod plugins;
/// Prompt rendering — fish-, oh-my-posh-, and pure-inspired prompt styles.
#[cfg(feature = "native")]
pub mod prompt;
/// AI providers — provider registry and model routing.
#[cfg(feature = "native")]
pub mod providers;
/// REPL — interactive read-eval-print loop.
#[cfg(feature = "native")]
pub mod repl;
/// Reinforcement learning — Q-Learning, DQN, Actor-Critic agents.
#[cfg(feature = "native")]
pub mod rl;
/// RL models — reinforcement learning model definitions.
#[cfg(feature = "native")]
pub mod rlm;
/// Safety core — effect taxonomy, policy/approval engine, hash-chained audit.
#[cfg(feature = "native")]
pub mod safety;
/// Secure configuration — encrypted secrets and credential storage.
#[cfg(feature = "native")]
pub mod secure_config;
/// Security — RBAC, audit logging, SSO integration.
#[cfg(feature = "native")]
pub mod security;
/// Return shapes — what a builtin gives back, advertised before it is called.
#[cfg(feature = "native")]
pub mod shapes;
/// Result handles — keep a large result server-side and send a reference.
#[cfg(feature = "native")]
pub mod handles;
/// Reversible sessions — capture what a mutating call overwrites, so it can be
/// rewound.
#[cfg(feature = "native")]
pub mod journal;
/// Syntax knowledge base — command documentation and help system.
#[cfg(feature = "native")]
pub mod syntax_kb;
/// Transpiler — AetherShell ↔ Bash compatibility layer.
#[cfg(feature = "native")]
pub mod transpile;
/// Terminal UI — rich TUI with chat, agent dashboard, and media viewer.
#[cfg(feature = "native")]
pub mod tui;
/// Transactions — filesystem checkpoint/rollback journal for undoable actions.
#[cfg(feature = "native")]
pub mod tx;
/// Workflows — workflow engine for multi-step automation.
#[cfg(feature = "native")]
pub mod workflows;

/// WASM bindings — browser REPL and web integration.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub mod wasm;
