//! AetherShell WASM bindings
//!
//! This module provides WebAssembly bindings for running AetherShell
//! in the browser. It exposes a full API for parsing and evaluating
//! AetherShell code with a comprehensive set of WASM-safe builtins.
//!
//! ## Supported Features
//! - All core language constructs (let, fn, match, if/else)
//! - Typed pipelines with map, where, reduce, etc.
//! - Pattern matching with guards
//! - String interpolation
//! - Arrays, records, and lambdas
//! - 40+ built-in functions (excluding filesystem/network)

#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "web")]
use crate::{ast::Stmt, env::Env, parser::parse_program, value::Value};

#[cfg(feature = "web")]
use std::collections::HashMap;

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
    env: Env,
}

#[cfg(feature = "web")]
#[wasm_bindgen]
impl AetherWasm {
    /// Create a new AetherShell runtime
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        AetherWasm { env: Env::default() }
    }

    /// Parse AetherShell code and return JSON representation of the AST
    pub fn parse(&self, code: &str) -> Result<String, JsValue> {
        match parse_program(code) {
            Ok(stmts) => serde_json::to_string_pretty(&stmts)
                .map_err(|e| JsValue::from_str(&format!("JSON error: {}", e))),
            Err(e) => Err(JsValue::from_str(&format!("Parse error: {}", e))),
        }
    }

    /// Evaluate AetherShell code and return the result as JSON
    pub fn eval(&mut self, code: &str) -> Result<String, JsValue> {
        let stmts = parse_program(code)
            .map_err(|e| JsValue::from_str(&format!("Parse error: {}", e)))?;
        
        let result = eval_wasm(&stmts, &mut self.env)
            .map_err(|e| JsValue::from_str(&format!("Eval error: {}", e)))?;
        
        value_to_json(&result)
    }

    /// Evaluate and return a human-readable string (for REPL display)
    pub fn eval_display(&mut self, code: &str) -> Result<String, JsValue> {
        let stmts = parse_program(code)
            .map_err(|e| JsValue::from_str(&format!("Parse error: {}", e)))?;
        
        let result = eval_wasm(&stmts, &mut self.env)
            .map_err(|e| JsValue::from_str(&format!("Eval error: {}", e)))?;
        
        Ok(format_value(&result))
    }

    /// Reset the environment (clear all variables)
    pub fn reset(&mut self) {
        self.env = Env::default();
    }

    /// Get all defined variable names
    pub fn variables(&self) -> Vec<JsValue> {
        self.env
            .all_names()
            .into_iter()
            .map(|s| JsValue::from_str(&s))
            .collect()
    }

    /// Get version info
    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Get list of available builtins
    pub fn builtins(&self) -> Vec<JsValue> {
        WASM_BUILTINS
            .iter()
            .map(|s| JsValue::from_str(s))
            .collect()
    }
}

#[cfg(feature = "web")]
const WASM_BUILTINS: &[&str] = &[
    // Core
    "print", "echo", "type_of", "len", "keys", "values",
    // Functional
    "map", "where", "filter", "reduce", "take", "skip", "first", "last", "any", "all",
    // String
    "split", "join", "trim", "upper", "lower", "replace", "contains", "starts_with", "ends_with",
    // Array
    "flatten", "reverse", "slice", "range", "zip", "push", "concat", "sort",
    // Math
    "abs", "min", "max", "sqrt", "pow", "floor", "ceil", "round", "sum", "avg", "product",
    // Utility
    "unique", "to_string", "to_int", "to_float",
];

/// Lightweight evaluator for WASM (no async, no filesystem)
#[cfg(feature = "web")]
fn eval_wasm(stmts: &[Stmt], env: &mut Env) -> Result<Value, String> {
    let mut result = Value::Null;
    for stmt in stmts {
        result = eval_stmt_wasm(stmt, env)?;
    }
    Ok(result)
}

#[cfg(feature = "web")]
fn eval_stmt_wasm(stmt: &Stmt, env: &mut Env) -> Result<Value, String> {
    match stmt {
        Stmt::Let { name, value, .. } => {
            let val = eval_expr_wasm(value, env)?;
            env.set(name.clone(), val.clone());
            Ok(val)
        }
        Stmt::Expr(expr) => eval_expr_wasm(expr, env),
        Stmt::Assign { name, value } => {
            let val = eval_expr_wasm(value, env)?;
            env.set(name.clone(), val.clone());
            Ok(val)
        }
    }
}

#[cfg(feature = "web")]
fn eval_expr_wasm(expr: &crate::ast::Expr, env: &mut Env) -> Result<Value, String> {
    use crate::ast::{BinOp, Expr};
    
    match expr {
        Expr::Null => Ok(Value::Null),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Int(n) => Ok(Value::Int(*n)),
        Expr::Float(f) => Ok(Value::Float(*f)),
        Expr::Str(s) => Ok(Value::Str(interpolate_string(s, env)?)),
        Expr::Ident(name) => env.get(name).cloned().ok_or_else(|| format!("undefined: {}", name)),
        
        Expr::Array(items) => {
            let vals: Result<Vec<Value>, String> = items.iter().map(|e| eval_expr_wasm(e, env)).collect();
            Ok(Value::Array(vals?))
        }
        
        Expr::Record(fields) => {
            let mut map = HashMap::new();
            for (k, v) in fields {
                map.insert(k.clone(), eval_expr_wasm(v, env)?);
            }
            Ok(Value::Record(map))
        }
        
        Expr::Lambda { params, body } => Ok(Value::Lambda(crate::value::Lambda {
            params: params.clone(),
            body: (**body).clone(),
        })),
        
        Expr::BinOp { op, left, right } => {
            let lv = eval_expr_wasm(left, env)?;
            let rv = eval_expr_wasm(right, env)?;
            eval_binop_wasm(*op, &lv, &rv)
        }
        
        Expr::UnaryNot(inner) => {
            match eval_expr_wasm(inner, env)? {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => Err("! requires boolean".to_string()),
            }
        }
        
        Expr::UnaryMinus(inner) => {
            match eval_expr_wasm(inner, env)? {
                Value::Int(n) => Ok(Value::Int(-n)),
                Value::Float(f) => Ok(Value::Float(-f)),
                _ => Err("- requires number".to_string()),
            }
        }
        
        Expr::Call { func, args } => eval_call_wasm(func, args, None, env),
        
        Expr::Pipeline { left, right } => {
            let lv = eval_expr_wasm(left, env)?;
            eval_pipeline_wasm(right, lv, env)
        }
        
        Expr::Member { object, field } => {
            let obj = eval_expr_wasm(object, env)?;
            match obj {
                Value::Record(map) => map.get(field).cloned().ok_or_else(|| format!("no field: {}", field)),
                _ => Err("member access requires record".to_string()),
            }
        }
        
        Expr::Index { object, index } => {
            let obj = eval_expr_wasm(object, env)?;
            let idx = eval_expr_wasm(index, env)?;
            match (&obj, &idx) {
                (Value::Array(arr), Value::Int(i)) => {
                    let i = *i as usize;
                    arr.get(i).cloned().ok_or_else(|| format!("index out of bounds: {}", i))
                }
                (Value::Record(map), Value::Str(k)) => {
                    map.get(k).cloned().ok_or_else(|| format!("no key: {}", k))
                }
                _ => Err("invalid index operation".to_string()),
            }
        }
        
        Expr::Match { value, arms } => {
            let val = eval_expr_wasm(value, env)?;
            for (pattern, guard, body) in arms {
                if let Some(bindings) = match_pattern_wasm(pattern, &val) {
                    let mut inner = env.clone();
                    for (k, v) in bindings {
                        inner.set(k, v);
                    }
                    if let Some(g) = guard {
                        if let Value::Bool(true) = eval_expr_wasm(g, &mut inner)? {
                            return eval_expr_wasm(body, &mut inner);
                        }
                    } else {
                        return eval_expr_wasm(body, &mut inner);
                    }
                }
            }
            Err("no matching pattern".to_string())
        }
        
        Expr::If { condition, then_branch, else_branch } => {
            match eval_expr_wasm(condition, env)? {
                Value::Bool(true) => eval_expr_wasm(then_branch, env),
                Value::Bool(false) => {
                    if let Some(eb) = else_branch {
                        eval_expr_wasm(eb, env)
                    } else {
                        Ok(Value::Null)
                    }
                }
                _ => Err("if condition must be boolean".to_string()),
            }
        }
        
        _ => Err("unsupported expression in WASM mode".to_string()),
    }
}

#[cfg(feature = "web")]
fn eval_binop_wasm(op: crate::ast::BinOp, lv: &Value, rv: &Value) -> Result<Value, String> {
    use crate::ast::BinOp;
    
    match (op, lv, rv) {
        // Arithmetic
        (BinOp::Add, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
        (BinOp::Add, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (BinOp::Add, Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
        (BinOp::Add, Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + *b as f64)),
        (BinOp::Add, Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", a, b))),
        
        (BinOp::Sub, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
        (BinOp::Sub, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
        (BinOp::Sub, Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - b)),
        (BinOp::Sub, Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - *b as f64)),
        
        (BinOp::Mul, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
        (BinOp::Mul, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
        (BinOp::Mul, Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
        (BinOp::Mul, Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * *b as f64)),
        
        (BinOp::Div, Value::Int(a), Value::Int(b)) if *b != 0 => Ok(Value::Int(a / b)),
        (BinOp::Div, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
        (BinOp::Div, Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 / b)),
        (BinOp::Div, Value::Float(a), Value::Int(b)) => Ok(Value::Float(a / *b as f64)),
        
        (BinOp::Mod, Value::Int(a), Value::Int(b)) if *b != 0 => Ok(Value::Int(a % b)),
        
        (BinOp::Pow, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.pow(*b as u32))),
        (BinOp::Pow, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(*b))),
        (BinOp::Pow, Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).powf(*b))),
        (BinOp::Pow, Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.powf(*b as f64))),
        
        // Comparison
        (BinOp::Eq, a, b) => Ok(Value::Bool(values_equal(a, b))),
        (BinOp::Neq, a, b) => Ok(Value::Bool(!values_equal(a, b))),
        
        (BinOp::Lt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
        (BinOp::Lt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a < b)),
        (BinOp::Lte, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
        (BinOp::Lte, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a <= b)),
        (BinOp::Gt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
        (BinOp::Gt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a > b)),
        (BinOp::Gte, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
        (BinOp::Gte, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a >= b)),
        
        // Logical
        (BinOp::And, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a && *b)),
        (BinOp::Or, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a || *b)),
        
        _ => Err(format!("invalid operation: {:?} on {:?} and {:?}", op, lv, rv)),
    }
}

#[cfg(feature = "web")]
fn eval_call_wasm(func: &str, args: &[crate::ast::Expr], piped: Option<Value>, env: &mut Env) -> Result<Value, String> {
    let mut evaled: Vec<Value> = args.iter().map(|a| eval_expr_wasm(a, env)).collect::<Result<_, _>>()?;
    
    if let Some(pv) = piped {
        evaled.insert(0, pv);
    }
    
    match func {
        "print" | "echo" => Ok(evaled.first().cloned().unwrap_or(Value::Null)),
        "type_of" => {
            let v = evaled.first().ok_or("type_of requires 1 arg")?;
            Ok(Value::Str(match v {
                Value::Null => "Null", Value::Bool(_) => "Bool", Value::Int(_) => "Int",
                Value::Float(_) => "Float", Value::Str(_) => "String", Value::Array(_) => "Array",
                Value::Record(_) => "Record", Value::Lambda(_) => "Lambda", Value::Table(_) => "Table",
                Value::Uri(_) => "Uri",
            }.to_string()))
        }
        "len" => {
            match evaled.first().ok_or("len requires 1 arg")? {
                Value::Str(s) => Ok(Value::Int(s.len() as i64)),
                Value::Array(a) => Ok(Value::Int(a.len() as i64)),
                Value::Record(r) => Ok(Value::Int(r.len() as i64)),
                _ => Err("len requires string, array, or record".to_string()),
            }
        }
        "keys" => match evaled.first().ok_or("keys requires 1 arg")? {
            Value::Record(r) => Ok(Value::Array(r.keys().map(|k| Value::Str(k.clone())).collect())),
            _ => Err("keys requires record".to_string()),
        },
        "values" => match evaled.first().ok_or("values requires 1 arg")? {
            Value::Record(r) => Ok(Value::Array(r.values().cloned().collect())),
            _ => Err("values requires record".to_string()),
        },
        
        // Functional
        "map" => {
            let (arr, func) = (evaled.first().ok_or("map requires array")?, evaled.get(1).ok_or("map requires function")?);
            match (arr, func) {
                (Value::Array(items), Value::Lambda(lam)) => {
                    let results: Result<Vec<Value>, String> = items.iter().map(|item| {
                        let mut inner = env.clone();
                        if let Some(p) = lam.params.first() { inner.set(p.clone(), item.clone()); }
                        eval_expr_wasm(&lam.body, &mut inner)
                    }).collect();
                    Ok(Value::Array(results?))
                }
                _ => Err("map requires (array, lambda)".to_string()),
            }
        }
        "where" | "filter" => {
            let (arr, func) = (evaled.first().ok_or("where requires array")?, evaled.get(1).ok_or("where requires function")?);
            match (arr, func) {
                (Value::Array(items), Value::Lambda(lam)) => {
                    let results: Result<Vec<Value>, String> = items.iter().filter_map(|item| {
                        let mut inner = env.clone();
                        if let Some(p) = lam.params.first() { inner.set(p.clone(), item.clone()); }
                        match eval_expr_wasm(&lam.body, &mut inner) {
                            Ok(Value::Bool(true)) => Some(Ok(item.clone())),
                            Ok(Value::Bool(false)) => None,
                            Ok(_) => Some(Err("predicate must return bool".to_string())),
                            Err(e) => Some(Err(e)),
                        }
                    }).collect();
                    Ok(Value::Array(results?))
                }
                _ => Err("where requires (array, lambda)".to_string()),
            }
        }
        "reduce" => {
            let (arr, func, init) = (evaled.first().ok_or("reduce requires array")?, evaled.get(1).ok_or("reduce requires function")?, evaled.get(2).ok_or("reduce requires initial")?);
            match (arr, func) {
                (Value::Array(items), Value::Lambda(lam)) => {
                    let mut acc = init.clone();
                    for item in items {
                        let mut inner = env.clone();
                        if lam.params.len() >= 2 {
                            inner.set(lam.params[0].clone(), acc);
                            inner.set(lam.params[1].clone(), item.clone());
                        }
                        acc = eval_expr_wasm(&lam.body, &mut inner)?;
                    }
                    Ok(acc)
                }
                _ => Err("reduce requires (array, lambda, init)".to_string()),
            }
        }
        "take" => match (evaled.first(), evaled.get(1)) {
            (Some(Value::Array(items)), Some(Value::Int(n))) => Ok(Value::Array(items.iter().take(*n as usize).cloned().collect())),
            _ => Err("take requires (array, int)".to_string()),
        },
        "skip" => match (evaled.first(), evaled.get(1)) {
            (Some(Value::Array(items)), Some(Value::Int(n))) => Ok(Value::Array(items.iter().skip(*n as usize).cloned().collect())),
            _ => Err("skip requires (array, int)".to_string()),
        },
        "first" => match evaled.first() {
            Some(Value::Array(items)) => Ok(items.first().cloned().unwrap_or(Value::Null)),
            _ => Err("first requires array".to_string()),
        },
        "last" => match evaled.first() {
            Some(Value::Array(items)) => Ok(items.last().cloned().unwrap_or(Value::Null)),
            _ => Err("last requires array".to_string()),
        },
        "any" => {
            let (arr, func) = (evaled.first().ok_or("any requires array")?, evaled.get(1).ok_or("any requires function")?);
            match (arr, func) {
                (Value::Array(items), Value::Lambda(lam)) => {
                    for item in items {
                        let mut inner = env.clone();
                        if let Some(p) = lam.params.first() { inner.set(p.clone(), item.clone()); }
                        if let Value::Bool(true) = eval_expr_wasm(&lam.body, &mut inner)? { return Ok(Value::Bool(true)); }
                    }
                    Ok(Value::Bool(false))
                }
                _ => Err("any requires (array, lambda)".to_string()),
            }
        }
        "all" => {
            let (arr, func) = (evaled.first().ok_or("all requires array")?, evaled.get(1).ok_or("all requires function")?);
            match (arr, func) {
                (Value::Array(items), Value::Lambda(lam)) => {
                    for item in items {
                        let mut inner = env.clone();
                        if let Some(p) = lam.params.first() { inner.set(p.clone(), item.clone()); }
                        if let Value::Bool(false) = eval_expr_wasm(&lam.body, &mut inner)? { return Ok(Value::Bool(false)); }
                    }
                    Ok(Value::Bool(true))
                }
                _ => Err("all requires (array, lambda)".to_string()),
            }
        }
        
        // String
        "split" => match (evaled.first(), evaled.get(1)) {
            (Some(Value::Str(s)), Some(Value::Str(d))) => Ok(Value::Array(s.split(d.as_str()).map(|p| Value::Str(p.to_string())).collect())),
            _ => Err("split requires (string, string)".to_string()),
        },
        "join" => match (evaled.first(), evaled.get(1)) {
            (Some(Value::Array(items)), Some(Value::Str(d))) => {
                let strs: Vec<String> = items.iter().map(format_value).collect();
                Ok(Value::Str(strs.join(d)))
            }
            _ => Err("join requires (array, string)".to_string()),
        },
        "trim" => match evaled.first() {
            Some(Value::Str(s)) => Ok(Value::Str(s.trim().to_string())),
            _ => Err("trim requires string".to_string()),
        },
        "upper" => match evaled.first() {
            Some(Value::Str(s)) => Ok(Value::Str(s.to_uppercase())),
            _ => Err("upper requires string".to_string()),
        },
        "lower" => match evaled.first() {
            Some(Value::Str(s)) => Ok(Value::Str(s.to_lowercase())),
            _ => Err("lower requires string".to_string()),
        },
        "replace" => match (evaled.first(), evaled.get(1), evaled.get(2)) {
            (Some(Value::Str(s)), Some(Value::Str(f)), Some(Value::Str(t))) => Ok(Value::Str(s.replace(f.as_str(), t))),
            _ => Err("replace requires (string, string, string)".to_string()),
        },
        "contains" => match (evaled.first(), evaled.get(1)) {
            (Some(Value::Str(s)), Some(Value::Str(sub))) => Ok(Value::Bool(s.contains(sub.as_str()))),
            (Some(Value::Array(arr)), Some(v)) => Ok(Value::Bool(arr.iter().any(|x| values_equal(x, v)))),
            _ => Err("contains requires (string, string) or (array, value)".to_string()),
        },
        "starts_with" => match (evaled.first(), evaled.get(1)) {
            (Some(Value::Str(s)), Some(Value::Str(p))) => Ok(Value::Bool(s.starts_with(p.as_str()))),
            _ => Err("starts_with requires (string, string)".to_string()),
        },
        "ends_with" => match (evaled.first(), evaled.get(1)) {
            (Some(Value::Str(s)), Some(Value::Str(su))) => Ok(Value::Bool(s.ends_with(su.as_str()))),
            _ => Err("ends_with requires (string, string)".to_string()),
        },
        
        // Array
        "flatten" => match evaled.first() {
            Some(Value::Array(items)) => {
                let mut result = Vec::new();
                for item in items {
                    if let Value::Array(inner) = item { result.extend(inner.clone()); } 
                    else { result.push(item.clone()); }
                }
                Ok(Value::Array(result))
            }
            _ => Err("flatten requires array".to_string()),
        },
        "reverse" => match evaled.first() {
            Some(Value::Array(items)) => { let mut r = items.clone(); r.reverse(); Ok(Value::Array(r)) }
            _ => Err("reverse requires array".to_string()),
        },
        "slice" => match (evaled.first(), evaled.get(1), evaled.get(2)) {
            (Some(Value::Array(items)), Some(Value::Int(s)), Some(Value::Int(e))) => Ok(Value::Array(items[*s as usize..(*e as usize).min(items.len())].to_vec())),
            (Some(Value::Str(str)), Some(Value::Int(s)), Some(Value::Int(e))) => Ok(Value::Str(str[*s as usize..(*e as usize).min(str.len())].to_string())),
            _ => Err("slice requires (array|string, int, int)".to_string()),
        },
        "range" => match (evaled.first(), evaled.get(1)) {
            (Some(Value::Int(s)), Some(Value::Int(e))) => Ok(Value::Array((*s..*e).map(Value::Int).collect())),
            _ => Err("range requires (int, int)".to_string()),
        },
        "zip" => match (evaled.first(), evaled.get(1)) {
            (Some(Value::Array(a)), Some(Value::Array(b))) => Ok(Value::Array(a.iter().zip(b.iter()).map(|(x, y)| Value::Array(vec![x.clone(), y.clone()])).collect())),
            _ => Err("zip requires (array, array)".to_string()),
        },
        "push" => match (evaled.first(), evaled.get(1)) {
            (Some(Value::Array(items)), Some(val)) => { let mut r = items.clone(); r.push(val.clone()); Ok(Value::Array(r)) }
            _ => Err("push requires (array, value)".to_string()),
        },
        "concat" => match (evaled.first(), evaled.get(1)) {
            (Some(Value::Array(a)), Some(Value::Array(b))) => { let mut r = a.clone(); r.extend(b.clone()); Ok(Value::Array(r)) }
            _ => Err("concat requires (array, array)".to_string()),
        },
        "sort" => match evaled.first() {
            Some(Value::Array(items)) => {
                let mut r = items.clone();
                r.sort_by(|a, b| match (a, b) {
                    (Value::Int(x), Value::Int(y)) => x.cmp(y),
                    (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
                    (Value::Str(x), Value::Str(y)) => x.cmp(y),
                    _ => std::cmp::Ordering::Equal,
                });
                Ok(Value::Array(r))
            }
            _ => Err("sort requires array".to_string()),
        },
        
        // Math
        "abs" => match evaled.first() {
            Some(Value::Int(n)) => Ok(Value::Int(n.abs())),
            Some(Value::Float(f)) => Ok(Value::Float(f.abs())),
            _ => Err("abs requires number".to_string()),
        },
        "min" => {
            if let Some(Value::Array(items)) = evaled.first() {
                return items.iter().min_by(|a, b| match (a, b) {
                    (Value::Int(x), Value::Int(y)) => x.cmp(y),
                    (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
                    _ => std::cmp::Ordering::Equal,
                }).cloned().ok_or("empty array".to_string());
            }
            match (evaled.first(), evaled.get(1)) {
                (Some(Value::Int(x)), Some(Value::Int(y))) => Ok(Value::Int(*x.min(y))),
                (Some(Value::Float(x)), Some(Value::Float(y))) => Ok(Value::Float(x.min(*y))),
                _ => Err("min requires numbers".to_string()),
            }
        }
        "max" => {
            if let Some(Value::Array(items)) = evaled.first() {
                return items.iter().max_by(|a, b| match (a, b) {
                    (Value::Int(x), Value::Int(y)) => x.cmp(y),
                    (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
                    _ => std::cmp::Ordering::Equal,
                }).cloned().ok_or("empty array".to_string());
            }
            match (evaled.first(), evaled.get(1)) {
                (Some(Value::Int(x)), Some(Value::Int(y))) => Ok(Value::Int(*x.max(y))),
                (Some(Value::Float(x)), Some(Value::Float(y))) => Ok(Value::Float(x.max(*y))),
                _ => Err("max requires numbers".to_string()),
            }
        }
        "sqrt" => match evaled.first() {
            Some(Value::Int(n)) => Ok(Value::Float((*n as f64).sqrt())),
            Some(Value::Float(f)) => Ok(Value::Float(f.sqrt())),
            _ => Err("sqrt requires number".to_string()),
        },
        "pow" => match (evaled.first(), evaled.get(1)) {
            (Some(Value::Int(b)), Some(Value::Int(e))) => Ok(Value::Int(b.pow(*e as u32))),
            (Some(Value::Float(b)), Some(Value::Float(e))) => Ok(Value::Float(b.powf(*e))),
            (Some(Value::Int(b)), Some(Value::Float(e))) => Ok(Value::Float((*b as f64).powf(*e))),
            (Some(Value::Float(b)), Some(Value::Int(e))) => Ok(Value::Float(b.powf(*e as f64))),
            _ => Err("pow requires numbers".to_string()),
        },
        "floor" => match evaled.first() {
            Some(Value::Float(f)) => Ok(Value::Int(f.floor() as i64)),
            Some(Value::Int(n)) => Ok(Value::Int(*n)),
            _ => Err("floor requires number".to_string()),
        },
        "ceil" => match evaled.first() {
            Some(Value::Float(f)) => Ok(Value::Int(f.ceil() as i64)),
            Some(Value::Int(n)) => Ok(Value::Int(*n)),
            _ => Err("ceil requires number".to_string()),
        },
        "round" => match evaled.first() {
            Some(Value::Float(f)) => Ok(Value::Int(f.round() as i64)),
            Some(Value::Int(n)) => Ok(Value::Int(*n)),
            _ => Err("round requires number".to_string()),
        },
        "sum" => match evaled.first() {
            Some(Value::Array(items)) => {
                let (mut sum, mut fsum, mut has_float) = (0i64, 0.0f64, false);
                for item in items {
                    match item {
                        Value::Int(n) => { sum += n; fsum += *n as f64; }
                        Value::Float(f) => { has_float = true; fsum += f; }
                        _ => return Err("sum requires numeric array".to_string()),
                    }
                }
                Ok(if has_float { Value::Float(fsum) } else { Value::Int(sum) })
            }
            _ => Err("sum requires array".to_string()),
        },
        "avg" => match evaled.first() {
            Some(Value::Array(items)) if !items.is_empty() => {
                let mut sum = 0.0f64;
                for item in items {
                    match item {
                        Value::Int(n) => sum += *n as f64,
                        Value::Float(f) => sum += f,
                        _ => return Err("avg requires numeric array".to_string()),
                    }
                }
                Ok(Value::Float(sum / items.len() as f64))
            }
            Some(Value::Array(_)) => Ok(Value::Float(0.0)),
            _ => Err("avg requires array".to_string()),
        },
        "product" => match evaled.first() {
            Some(Value::Array(items)) => {
                let (mut prod, mut fprod, mut has_float) = (1i64, 1.0f64, false);
                for item in items {
                    match item {
                        Value::Int(n) => { prod *= n; fprod *= *n as f64; }
                        Value::Float(f) => { has_float = true; fprod *= f; }
                        _ => return Err("product requires numeric array".to_string()),
                    }
                }
                Ok(if has_float { Value::Float(fprod) } else { Value::Int(prod) })
            }
            _ => Err("product requires array".to_string()),
        },
        
        // Utility
        "unique" => match evaled.first() {
            Some(Value::Array(items)) => {
                let mut seen = Vec::new();
                for item in items {
                    if !seen.iter().any(|x| values_equal(x, item)) { seen.push(item.clone()); }
                }
                Ok(Value::Array(seen))
            }
            _ => Err("unique requires array".to_string()),
        },
        "to_string" => Ok(Value::Str(format_value(evaled.first().ok_or("to_string requires value")?))),
        "to_int" => match evaled.first() {
            Some(Value::Int(n)) => Ok(Value::Int(*n)),
            Some(Value::Float(f)) => Ok(Value::Int(*f as i64)),
            Some(Value::Str(s)) => s.parse::<i64>().map(Value::Int).map_err(|e| e.to_string()),
            _ => Err("to_int requires number or string".to_string()),
        },
        "to_float" => match evaled.first() {
            Some(Value::Int(n)) => Ok(Value::Float(*n as f64)),
            Some(Value::Float(f)) => Ok(Value::Float(*f)),
            Some(Value::Str(s)) => s.parse::<f64>().map(Value::Float).map_err(|e| e.to_string()),
            _ => Err("to_float requires number or string".to_string()),
        },
        
        // User-defined function
        _ => {
            if let Some(Value::Lambda(lam)) = env.get(func) {
                let lam = lam.clone();
                let mut inner = env.clone();
                for (i, param) in lam.params.iter().enumerate() {
                    if let Some(arg) = evaled.get(i) { inner.set(param.clone(), arg.clone()); }
                }
                eval_expr_wasm(&lam.body, &mut inner)
            } else {
                Err(format!("unknown function: {}", func))
            }
        }
    }
}

#[cfg(feature = "web")]
fn eval_pipeline_wasm(expr: &crate::ast::Expr, piped: Value, env: &mut Env) -> Result<Value, String> {
    use crate::ast::Expr;
    match expr {
        Expr::Call { func, args } => eval_call_wasm(func, args, Some(piped), env),
        Expr::Ident(name) => eval_call_wasm(name, &[], Some(piped), env),
        _ => Err("pipeline requires function call".to_string()),
    }
}

#[cfg(feature = "web")]
fn match_pattern_wasm(pattern: &crate::ast::MatchPattern, value: &Value) -> Option<Vec<(String, Value)>> {
    use crate::ast::MatchPattern;
    match pattern {
        MatchPattern::Wildcard => Some(vec![]),
        MatchPattern::Literal(lit) => {
            let lit_val = match lit {
                crate::ast::Expr::Int(n) => Value::Int(*n),
                crate::ast::Expr::Float(f) => Value::Float(*f),
                crate::ast::Expr::Str(s) => Value::Str(s.clone()),
                crate::ast::Expr::Bool(b) => Value::Bool(*b),
                crate::ast::Expr::Null => Value::Null,
                _ => return None,
            };
            if values_equal(&lit_val, value) { Some(vec![]) } else { None }
        }
        MatchPattern::Binding(name) => Some(vec![(name.clone(), value.clone())]),
        MatchPattern::Array(patterns) => {
            if let Value::Array(items) = value {
                if patterns.len() != items.len() { return None; }
                let mut bindings = Vec::new();
                for (p, v) in patterns.iter().zip(items.iter()) {
                    bindings.extend(match_pattern_wasm(p, v)?);
                }
                Some(bindings)
            } else { None }
        }
        MatchPattern::Record(fields) => {
            if let Value::Record(map) = value {
                let mut bindings = Vec::new();
                for (k, p) in fields {
                    let v = map.get(k)?;
                    bindings.extend(match_pattern_wasm(p, v)?);
                }
                Some(bindings)
            } else { None }
        }
        _ => None,
    }
}

#[cfg(feature = "web")]
fn interpolate_string(s: &str, env: &mut Env) -> Result<String, String> {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next();
            let mut var = String::new();
            while let Some(&ch) = chars.peek() {
                if ch == '}' { chars.next(); break; }
                var.push(chars.next().unwrap());
            }
            if let Some(val) = env.get(&var) { result.push_str(&format_value(val)); }
            else { result.push_str(&format!("${{{}}}", var)); }
        } else { result.push(c); }
    }
    Ok(result)
}

#[cfg(feature = "web")]
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => (x - y).abs() < f64::EPSILON,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| values_equal(a, b)),
        (Value::Record(x), Value::Record(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|yv| values_equal(v, yv))),
        _ => false,
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
            let items: Result<Vec<String>, JsValue> = rec.iter().map(|(k, v)| value_to_json(v).map(|vj| format!("\"{}\": {}", k, vj))).collect();
            Ok(format!("{{{}}}", items?.join(", ")))
        }
        Value::Table(t) => {
            let rows: Result<Vec<String>, JsValue> = t.rows.iter().map(|row| {
                let items: Result<Vec<String>, JsValue> = row.iter().map(|(k, v)| value_to_json(v).map(|vj| format!("\"{}\": {}", k, vj))).collect();
                items.map(|i| format!("{{{}}}", i.join(", ")))
            }).collect();
            Ok(format!("[{}]", rows?.join(", ")))
        }
        Value::Lambda(lam) => Ok(format!("\"<lambda({})>\"", lam.params.join(", "))),
    }
}

#[cfg(feature = "web")]
fn format_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Str(s) => s.clone(),
        Value::Uri(u) => format!("<{}>", u),
        Value::Array(arr) => format!("[{}]", arr.iter().map(format_value).collect::<Vec<_>>().join(", ")),
        Value::Record(rec) => format!("{{{}}}", rec.iter().map(|(k, v)| format!("{}: {}", k, format_value(v))).collect::<Vec<_>>().join(", ")),
        Value::Table(t) => format!("<table: {} rows>", t.rows.len()),
        Value::Lambda(lam) => format!("fn({}) => ...", lam.params.join(", ")),
    }
}

// Non-WASM stub
#[cfg(not(feature = "web"))]
pub struct AetherWasm;

#[cfg(not(feature = "web"))]
impl AetherWasm {
    pub fn new() -> Self { AetherWasm }
    pub fn parse(&self, _code: &str) -> Result<String, String> { Err("WASM not enabled".to_string()) }
    pub fn version(&self) -> String { env!("CARGO_PKG_VERSION").to_string() }
}
