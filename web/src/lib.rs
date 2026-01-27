//! AetherShell WebAssembly Module
//!
//! This module re-exports the WASM bindings from the main aethershell crate.
//! The actual implementation is in src/wasm.rs with the `web` feature enabled.
//!
//! ## Usage (JavaScript)
//! ```javascript
//! import init, { AetherWasm, ae_eval, ae_parse, ae_version } from '@nervosys/aethershell';
//! await init();
//!
//! // Simple one-off evaluation
//! const result = ae_eval('[1,2,3] | map(fn(x) => x * 2)');
//! console.log(JSON.parse(result)); // [2, 4, 6]
//!
//! // Persistent environment
//! const shell = new AetherWasm();
//! shell.eval('let x = 42');
//! console.log(shell.eval_display('x * 2')); // "84"
//! ```

// Re-export everything from the main wasm module
pub use aethershell::wasm::*;
