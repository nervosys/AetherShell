//! Transpilers that convert other shells/languages into Aurora Shell source.
//!
//! Currently includes a pragmatic Bash → Aurora transpiler for a useful subset.
//! Re-exported as `aurora_shell::transpile::bash::transpile_bash_to_ae`.

pub mod bash;
