pub mod ast;
pub mod env;
pub mod parser;
pub mod types;
pub mod value;

// Core modules available in both native and WASM builds
pub mod shell_features;
pub mod typecheck;

// Native-only modules (including eval which depends on builtins)
#[cfg(feature = "native")]
pub mod agent;
#[cfg(feature = "native")]
pub mod agent_api;
#[cfg(feature = "native")]
pub mod ai;
#[cfg(feature = "native")]
pub mod ai_api;
#[cfg(feature = "native")]
pub mod builtins;
#[cfg(feature = "native")]
pub mod config;
#[cfg(feature = "native")]
pub mod eval;
#[cfg(feature = "native")]
pub mod evolution;
#[cfg(feature = "native")]
pub mod mcp;
#[cfg(feature = "native")]
pub mod metrics;
#[cfg(feature = "native")]
pub mod neural;
#[cfg(feature = "native")]
pub mod os_tools;
#[cfg(feature = "native")]
pub mod packages;
#[cfg(feature = "native")]
pub mod persistence;
#[cfg(feature = "native")]
pub mod plugins;
#[cfg(feature = "native")]
pub mod repl;
#[cfg(feature = "native")]
pub mod rl;
#[cfg(feature = "native")]
pub mod rlm;
#[cfg(feature = "native")]
pub mod secure_config;
#[cfg(feature = "native")]
pub mod security;
#[cfg(feature = "native")]
pub mod syntax_kb;
#[cfg(feature = "native")]
pub mod transpile;
#[cfg(feature = "native")]
pub mod tui;
#[cfg(feature = "native")]
pub mod workflows;

// WASM module - only compiled when targeting wasm32 with web feature
#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub mod wasm;
