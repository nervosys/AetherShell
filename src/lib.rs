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
//! - [`builtins`] — 430+ built-in functions across 50 modules
//! - [`modules`] — Module system mapping `module.function()` syntax to builtins
//! - [`ai`] / [`ai_api`] — Multi-provider AI integration
//! - [`agent`] / [`agent_api`] — Autonomous agent framework and HTTP API
//! - [`mcp`] — Model Context Protocol (130+ tools)
//! - [`metrics`] — Prometheus-compatible metrics, alerting, and monitoring
//! - [`tui`] — Rich terminal UI with chat, agents, and media viewer

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
/// Built-in functions — 430+ typed builtins across 50 modules.
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
/// Marketplace — community plugin and agent marketplace.
#[cfg(feature = "native")]
pub mod marketplace;
/// MCP — Model Context Protocol with 130+ tools and HTTP server.
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
/// Secure configuration — encrypted secrets and credential storage.
#[cfg(feature = "native")]
pub mod secure_config;
/// Security — RBAC, audit logging, SSO integration.
#[cfg(feature = "native")]
pub mod security;
/// Syntax knowledge base — command documentation and help system.
#[cfg(feature = "native")]
pub mod syntax_kb;
/// Transpiler — AetherShell ↔ Bash compatibility layer.
#[cfg(feature = "native")]
pub mod transpile;
/// Terminal UI — rich TUI with chat, agent dashboard, and media viewer.
#[cfg(feature = "native")]
pub mod tui;
/// Workflows — workflow engine for multi-step automation.
#[cfg(feature = "native")]
pub mod workflows;

/// WASM bindings — browser REPL and web integration.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub mod wasm;
