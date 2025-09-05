use once_cell::sync::Lazy;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

use aurora_shell::{env::Env, value::Value};

// Global Env persisted across JS calls
static GLOBAL_ENV: Lazy<Mutex<Env>> = Lazy::new(|| Mutex::new(Env::default()));

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

#[wasm_bindgen]
pub fn ae_version() -> String {
    "ae-wasm 0.1.0".to_string()
}

#[wasm_bindgen]
pub fn ae_reset() {
    let mut env = GLOBAL_ENV.lock().unwrap();
    *env = Env::default();
}

#[wasm_bindgen]
pub fn ae_eval(line: &str) -> String {
    let mut env = GLOBAL_ENV.lock().unwrap();

    // Expect aurora_shell to expose a single-line entrypoint.
    // If you don't have this yet, add a tiny helper in aurora_shell (see step 3).
    match aurora_shell::repl::eval_line_public(line, &mut env) {
        Ok(v) => display(&v),
        Err(e) => format!("[error] {}", e),
    }
}
