//! IronShell WebAssembly Module
//!
//! This module re-exports the WASM bindings from the main ironshell crate.
//! The actual implementation is in src/wasm.rs with the `web` feature enabled.
//!
//! ## Usage (JavaScript)
//! ```javascript
//! import init, { IronShellWasm, ish_eval, ish_parse, ish_version } from '@nervosys/aethershell';
//! await init();
//!
//! // Simple one-off evaluation
//! const result = ish_eval('[1,2,3] | map(fn(x) => x * 2)');
//! console.log(JSON.parse(result)); // [2, 4, 6]
//!
//! // Persistent environment
//! const shell = new IronShellWasm();
//! shell.eval('let x = 42');
//! console.log(shell.eval_display('x * 2')); // "84"
//! ```

// This crate is WASM-only. Re-export everything from the main wasm module.
#[cfg(target_arch = "wasm32")]
pub use iron_shell::wasm::*;

// Compile error message for non-WASM targets
#[cfg(not(target_arch = "wasm32"))]
compile_error!("ironshell_wasm crate is only supported on wasm32 target. Use: cargo build --target wasm32-unknown-unknown");
