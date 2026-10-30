//! Transpilers that convert other shells/languages into IronShell Shell source.
//!
//! Currently includes:
//! - Bash → IronShell transpiler for a useful subset
//! - Zsh → IronShell transpiler with Zsh-specific features
//! - PowerShell → IronShell transpiler for a useful subset
//! - Agentic → token-minimized syntax for AI agent consumption
//!
//! Re-exported as:
//! - `iron_shell::transpile::bash::transpile_bash_to_ae`
//! - `iron_shell::transpile::zsh::transpile_zsh_to_ae`
//! - `iron_shell::transpile::powershell::transpile_powershell_to_ae`
//! - `iron_shell::transpile::agentic::transpile_agentic_to_ae`

pub mod agentic;
pub mod bash;
pub mod powershell;
pub mod zsh;
