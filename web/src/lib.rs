//! AetherShell WebAssembly Module
//!
//! This module provides WebAssembly bindings for AetherShell, enabling
//! browser-based execution of AetherShell code with AI capabilities.
//!
//! ## Features
//! - Core evaluation of AetherShell code
//! - JSON-based Value serialization for JS interop
//! - A2UI event subscription for UI integration
//! - Pipeline execution helpers
//!
//! ## Usage (JavaScript)
//! ```javascript
//! import init, { AetherShell } from '@nervosys/aethershell';
//! await init();
//!
//! const shell = new AetherShell();
//! const result = shell.eval('[1,2,3] | map(fn(x) => x * 2)');
//! console.log(result); // [2, 4, 6]
//! ```

use once_cell::sync::Lazy;
use std::collections::VecDeque;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

use aether_shell::{env::Env, value::Value};

// Global Env persisted across JS calls
static GLOBAL_ENV: Lazy<Mutex<Env>> = Lazy::new(|| Mutex::new(Env::default()));

// Event queue for A2UI events to be consumed by JavaScript
static EVENT_QUEUE: Lazy<Mutex<VecDeque<String>>> = Lazy::new(|| Mutex::new(VecDeque::new()));

// ============================================================================
// Value Display and Serialization
// ============================================================================

fn display(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(x) => x.to_string(),
        Value::Str(s) => s.clone(),
        Value::Uri(u) => u.clone(),
        Value::Array(a) => format!("[len={}]", a.len()),
        Value::Record(_) => "{…}".into(),
        Value::Table(t) => format!("<Table rows={}>", t.rows.len()),
        Value::Lambda(_) => "<lambda>".into(),
    }
}

/// Convert a Value to JSON for JavaScript consumption
fn value_to_json(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(x) => {
            if x.is_nan() {
                "null".to_string()
            } else if x.is_infinite() {
                if *x > 0.0 {
                    "1e308".to_string()
                } else {
                    "-1e308".to_string()
                }
            } else {
                x.to_string()
            }
        }
        Value::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Value::Uri(u) => format!("\"{}\"", u.replace('\\', "\\\\").replace('"', "\\\"")),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(|v| value_to_json(v)).collect();
            format!("[{}]", items.join(","))
        }
        Value::Record(rec) => {
            let pairs: Vec<String> = rec
                .iter()
                .map(|(k, v)| format!("\"{}\":{}", k, value_to_json(v)))
                .collect();
            format!("{{{}}}", pairs.join(","))
        }
        Value::Table(t) => {
            let rows: Vec<String> = t.rows.iter().map(|row| value_to_json(row)).collect();
            format!(
                "{{\"columns\":{},\"rows\":[{}]}}",
                value_to_json(&Value::Array(
                    t.columns.iter().map(|c| Value::Str(c.clone())).collect()
                )),
                rows.join(",")
            )
        }
        Value::Lambda(_) => "\"<lambda>\"".to_string(),
    }
}

// ============================================================================
// Core Functions (Standalone)
// ============================================================================

/// Get the WASM module version
#[wasm_bindgen]
pub fn ae_version() -> String {
    "ae-wasm 0.2.0".to_string()
}

/// Reset the global environment
#[wasm_bindgen]
pub fn ae_reset() {
    let mut env = GLOBAL_ENV.lock().unwrap();
    *env = Env::default();
}

/// Evaluate a single line of AetherShell code
#[wasm_bindgen]
pub fn ae_eval(line: &str) -> String {
    let mut env = GLOBAL_ENV.lock().unwrap();

    match aether_shell::repl::eval_line_public(line, &mut env) {
        Ok(v) => display(&v),
        Err(e) => format!("[error] {}", e),
    }
}

/// Evaluate code and return JSON result
#[wasm_bindgen]
pub fn ae_eval_json(line: &str) -> String {
    let mut env = GLOBAL_ENV.lock().unwrap();

    match aether_shell::repl::eval_line_public(line, &mut env) {
        Ok(v) => value_to_json(&v),
        Err(e) => format!("{{\"error\":\"{}\"}}", e.replace('"', "\\\"")),
    }
}

/// Get a variable from the environment as JSON
#[wasm_bindgen]
pub fn ae_get_var(name: &str) -> String {
    let env = GLOBAL_ENV.lock().unwrap();
    match env.get_var(name) {
        Some(v) => value_to_json(v),
        None => "null".to_string(),
    }
}

/// Set a variable in the environment from JSON
#[wasm_bindgen]
pub fn ae_set_var(name: &str, json_value: &str) -> bool {
    let mut env = GLOBAL_ENV.lock().unwrap();

    // Parse simple JSON values
    let value = if json_value == "null" {
        Value::Null
    } else if json_value == "true" {
        Value::Bool(true)
    } else if json_value == "false" {
        Value::Bool(false)
    } else if let Ok(n) = json_value.parse::<i64>() {
        Value::Int(n)
    } else if let Ok(f) = json_value.parse::<f64>() {
        Value::Float(f)
    } else if json_value.starts_with('"') && json_value.ends_with('"') {
        Value::Str(json_value[1..json_value.len() - 1].to_string())
    } else {
        // For complex values, try evaluating as AetherShell
        match aether_shell::repl::eval_line_public(json_value, &mut env) {
            Ok(v) => v,
            Err(_) => return false,
        }
    };

    env.set_var(name.to_string(), value);
    true
}

/// List all available builtins
#[wasm_bindgen]
pub fn ae_list_builtins() -> String {
    // Return commonly used builtins as JSON array
    let builtins = vec![
        "print",
        "len",
        "type",
        "range",
        "map",
        "filter",
        "reduce",
        "sort",
        "reverse",
        "flatten",
        "unique",
        "join",
        "split",
        "trim",
        "upper",
        "lower",
        "contains",
        "replace",
        "slice",
        "keys",
        "values",
        "get",
        "set",
        "merge",
        "sum",
        "avg",
        "min",
        "max",
        "abs",
        "round",
        "floor",
        "ceil",
        "sqrt",
        "now",
        "date",
        "time",
        "format",
        "parse_json",
        "to_json",
        "http_get",
        "http_post",
        "read_file",
        "write_file",
        "ai",
        "agent",
        "swarm",
        "a2ui_notify",
        "a2ui_prompt",
    ];
    format!(
        "[{}]",
        builtins
            .iter()
            .map(|b| format!("\"{}\"", b))
            .collect::<Vec<_>>()
            .join(",")
    )
}

// ============================================================================
// A2UI Event Queue (for JavaScript callbacks)
// ============================================================================

/// Push an A2UI event to the queue (called from Rust side)
pub fn push_a2ui_event(event_json: &str) {
    let mut queue = EVENT_QUEUE.lock().unwrap();
    queue.push_back(event_json.to_string());
}

/// Pop an A2UI event from the queue (called from JavaScript)
#[wasm_bindgen]
pub fn ae_poll_event() -> Option<String> {
    let mut queue = EVENT_QUEUE.lock().unwrap();
    queue.pop_front()
}

/// Get the number of pending A2UI events
#[wasm_bindgen]
pub fn ae_event_count() -> usize {
    let queue = EVENT_QUEUE.lock().unwrap();
    queue.len()
}

/// Clear all pending A2UI events
#[wasm_bindgen]
pub fn ae_clear_events() {
    let mut queue = EVENT_QUEUE.lock().unwrap();
    queue.clear();
}

// ============================================================================
// AetherShell Class (Object-Oriented Interface)
// ============================================================================

/// AetherShell instance for object-oriented usage from JavaScript
#[wasm_bindgen]
pub struct AetherShell {
    env: Env,
}

#[wasm_bindgen]
impl AetherShell {
    /// Create a new AetherShell instance with its own environment
    #[wasm_bindgen(constructor)]
    pub fn new() -> AetherShell {
        AetherShell {
            env: Env::default(),
        }
    }

    /// Evaluate AetherShell code and return string result
    pub fn eval(&mut self, code: &str) -> String {
        match aether_shell::repl::eval_line_public(code, &mut self.env) {
            Ok(v) => display(&v),
            Err(e) => format!("[error] {}", e),
        }
    }

    /// Evaluate AetherShell code and return JSON result
    #[wasm_bindgen(js_name = evalJson)]
    pub fn eval_json(&mut self, code: &str) -> String {
        match aether_shell::repl::eval_line_public(code, &mut self.env) {
            Ok(v) => value_to_json(&v),
            Err(e) => format!("{{\"error\":\"{}\"}}", e.replace('"', "\\\"")),
        }
    }

    /// Execute a pipeline on input data
    #[wasm_bindgen]
    pub fn pipe(&mut self, input_json: &str, operations: &str) -> String {
        // Set input as $_ variable
        if self.set_var("_input", input_json) {
            let code = format!("$_input | {}", operations);
            self.eval_json(&code)
        } else {
            "{\"error\":\"Failed to parse input\"}".to_string()
        }
    }

    /// Get a variable from the environment
    #[wasm_bindgen(js_name = getVar)]
    pub fn get_var(&self, name: &str) -> String {
        match self.env.get_var(name) {
            Some(v) => value_to_json(v),
            None => "null".to_string(),
        }
    }

    /// Set a variable in the environment
    #[wasm_bindgen(js_name = setVar)]
    pub fn set_var(&mut self, name: &str, json_value: &str) -> bool {
        let value = if json_value == "null" {
            Value::Null
        } else if json_value == "true" {
            Value::Bool(true)
        } else if json_value == "false" {
            Value::Bool(false)
        } else if let Ok(n) = json_value.parse::<i64>() {
            Value::Int(n)
        } else if let Ok(f) = json_value.parse::<f64>() {
            Value::Float(f)
        } else if json_value.starts_with('"') && json_value.ends_with('"') {
            Value::Str(json_value[1..json_value.len() - 1].to_string())
        } else if json_value.starts_with('[') || json_value.starts_with('{') {
            // For arrays and objects, evaluate as AetherShell literal
            match aether_shell::repl::eval_line_public(json_value, &mut self.env) {
                Ok(v) => v,
                Err(_) => return false,
            }
        } else {
            Value::Str(json_value.to_string())
        };

        self.env.set_var(name.to_string(), value);
        true
    }

    /// Reset the environment to default state
    pub fn reset(&mut self) {
        self.env = Env::default();
    }

    /// Get the version string
    pub fn version(&self) -> String {
        ae_version()
    }
}

impl Default for AetherShell {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TypeScript Type Definitions (for documentation)
// ============================================================================

// The following would be in a separate .d.ts file for npm package:
//
// declare module '@nervosys/aethershell' {
//   export function ae_version(): string;
//   export function ae_reset(): void;
//   export function ae_eval(line: string): string;
//   export function ae_eval_json(line: string): string;
//   export function ae_get_var(name: string): string;
//   export function ae_set_var(name: string, json_value: string): boolean;
//   export function ae_list_builtins(): string;
//   export function ae_poll_event(): string | undefined;
//   export function ae_event_count(): number;
//   export function ae_clear_events(): void;
//
//   export class AetherShell {
//     constructor();
//     eval(code: string): string;
//     evalJson(code: string): string;
//     pipe(input_json: string, operations: string): string;
//     getVar(name: string): string;
//     setVar(name: string, json_value: string): boolean;
//     reset(): void;
//     version(): string;
//   }
//
//   export default function init(): Promise<void>;
// }
