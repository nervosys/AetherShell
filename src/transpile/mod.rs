//! Transpilers that convert other shells/languages into Aether Shell source.
//!
//! Currently includes:
//! - Bash → Aether transpiler for a useful subset
//! - Zsh → Aether transpiler with Zsh-specific features
//! - PowerShell → Aether transpiler for a useful subset
//! - Agentic → token-minimized syntax for AI agent consumption
//!
//! Re-exported as:
//! - `aethershell::transpile::bash::transpile_bash_to_ae`
//! - `aethershell::transpile::zsh::transpile_zsh_to_ae`
//! - `aethershell::transpile::powershell::transpile_powershell_to_ae`
//! - `aethershell::transpile::agentic::transpile_agentic_to_ae`

pub mod agentic;
pub mod bash;
pub mod powershell;
pub mod zsh;
