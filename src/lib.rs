pub mod agent;
pub mod ai;
pub mod ai_api;
pub mod ast;
pub mod builtins;
pub mod config; // XDG-compliant configuration system
pub mod env;
pub mod eval;
pub mod evolution;
pub mod mcp; // Model Context Protocol server/client for AI tool access
pub mod neural;
pub mod os_tools;
pub mod parser;
pub mod plugins;
pub mod repl;
pub mod rl;
pub mod rlm; // Recursive Language Models for hierarchical agent spawning
pub mod secure_config;
pub mod security;
pub mod shell_features;
pub mod syntax_kb;
pub mod transpile;
pub mod tui;
pub mod typecheck;
pub mod types;
pub mod value;
