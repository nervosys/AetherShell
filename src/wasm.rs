//! AetherShell WASM bindings
//!
//! This module provides WebAssembly bindings for running AetherShell
//! in the browser. It exposes a simplified API for parsing and evaluating
//! AetherShell code.
//!
//! Note: The WASM build supports core language features (arithmetic, strings,
//! arrays, records, lambdas) but not filesystem or network builtins.

#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "web")]
use crate::{parser::parse_program, value::Value};

/// Initialize the WASM module with panic hooks for better error messages
#[cfg(feature = "web")]
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// The AetherShell WASM runtime
#[cfg(feature = "web")]
#[wasm_bindgen]
pub struct AetherWasm {
    // In WASM mode, we use a simplified environment without builtins
}

#[cfg(feature = "web")]
#[wasm_bindgen]
impl AetherWasm {
    /// Create a new AetherShell runtime
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        AetherWasm {}
    }

    /// Parse AetherShell code and return JSON representation of the AST
    pub fn parse(&self, code: &str) -> Result<String, JsValue> {
        match parse_program(code) {
            Ok(stmts) => serde_json::to_string_pretty(&stmts)
                .map_err(|e| JsValue::from_str(&format!("JSON error: {}", e))),
            Err(e) => Err(JsValue::from_str(&format!("Parse error: {}", e))),
        }
    }

    /// Get version info
    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

#[cfg(feature = "web")]
fn value_to_json(value: &Value) -> Result<String, JsValue> {
    match value {
        Value::Null => Ok("null".to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Int(n) => Ok(n.to_string()),
        Value::Float(f) => Ok(f.to_string()),
        Value::Str(s) => Ok(format!("\"{}\"", s.escape_default())),
        Value::Uri(u) => Ok(format!("\"{}\"", u)),
        Value::Array(arr) => {
            let items: Result<Vec<String>, JsValue> = arr.iter().map(value_to_json).collect();
            Ok(format!("[{}]", items?.join(", ")))
        }
        Value::Record(rec) => {
            let items: Result<Vec<String>, JsValue> = rec
                .iter()
                .map(|(k, v)| value_to_json(v).map(|vj| format!("\"{}\": {}", k, vj)))
                .collect();
            Ok(format!("{{{}}}", items?.join(", ")))
        }
        Value::Table(t) => {
            let rows: Result<Vec<String>, JsValue> = t
                .rows
                .iter()
                .map(|row| {
                    let items: Result<Vec<String>, JsValue> = row
                        .iter()
                        .map(|(k, v)| value_to_json(v).map(|vj| format!("\"{}\": {}", k, vj)))
                        .collect();
                    items.map(|i| format!("{{{}}}", i.join(", ")))
                })
                .collect();
            Ok(format!("[{}]", rows?.join(", ")))
        }
        Value::Lambda(lam) => Ok(format!("\"<lambda({})>\"", lam.params.join(", "))),
    }
}

// Non-WASM stub for when the feature is disabled
#[cfg(not(feature = "web"))]
pub struct AetherWasm;

#[cfg(not(feature = "web"))]
impl AetherWasm {
    pub fn new() -> Self {
        AetherWasm
    }
}
