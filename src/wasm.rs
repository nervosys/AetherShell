//! AetherShell WASM bindings
//!
//! Provides WebAssembly bindings for running AetherShell in the browser.
//! This module exposes parsing and basic evaluation capabilities.

#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "web")]
use crate::{env::Env, parser::parse_program, value::Value};

#[cfg(feature = "web")]
use crate::ast::{BinOp, Expr, Pattern, Stmt, UnOp};

#[cfg(feature = "web")]
use crate::value::Lambda;

#[cfg(feature = "web")]
use std::collections::BTreeMap;

/// Initialize WASM module
#[cfg(feature = "web")]
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// AetherShell WASM runtime
#[cfg(feature = "web")]
#[wasm_bindgen]
pub struct AetherWasm {
    env: Env,
}

#[cfg(feature = "web")]
#[wasm_bindgen]
impl AetherWasm {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        AetherWasm {
            env: Env::default(),
        }
    }

    pub fn parse(&self, code: &str) -> Result<String, JsValue> {
        match parse_program(code) {
            Ok(stmts) => serde_json::to_string_pretty(&stmts)
                .map_err(|e| JsValue::from_str(&format!("JSON error: {}", e))),
            Err(e) => Err(JsValue::from_str(&format!("Parse error: {}", e))),
        }
    }

    pub fn eval(&mut self, code: &str) -> Result<String, JsValue> {
        let stmts =
            parse_program(code).map_err(|e| JsValue::from_str(&format!("Parse error: {}", e)))?;
        let result = eval_wasm(&stmts, &mut self.env)
            .map_err(|e| JsValue::from_str(&format!("Eval error: {}", e)))?;
        value_to_json(&result)
    }

    pub fn eval_display(&mut self, code: &str) -> Result<String, JsValue> {
        let stmts =
            parse_program(code).map_err(|e| JsValue::from_str(&format!("Parse error: {}", e)))?;
        let result = eval_wasm(&stmts, &mut self.env)
            .map_err(|e| JsValue::from_str(&format!("Eval error: {}", e)))?;
        Ok(format_value(&result))
    }

    pub fn reset(&mut self) {
        self.env = Env::default();
    }

    pub fn variables(&self) -> Vec<JsValue> {
        self.env
            .vars()
            .keys()
            .map(|s| JsValue::from_str(s))
            .collect()
    }

    pub fn get_var(&self, name: &str) -> Result<String, JsValue> {
        match self.env.get_var(name) {
            Some(v) => value_to_json(v),
            None => Err(JsValue::from_str(&format!("Variable '{}' not found", name))),
        }
    }

    pub fn set_var(&mut self, name: &str, json: &str) -> Result<(), JsValue> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| JsValue::from_str(&format!("JSON parse error: {}", e)))?;
        let aether_value = json_to_value(&value);
        self.env.set_var_unchecked(name, aether_value);
        Ok(())
    }

    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    pub fn builtins(&self) -> Vec<JsValue> {
        WASM_BUILTINS.iter().map(|s| JsValue::from_str(s)).collect()
    }
}

#[cfg(feature = "web")]
const WASM_BUILTINS: &[&str] = &[
    "abs",
    "ceil",
    "floor",
    "round",
    "sqrt",
    "pow",
    "min",
    "max",
    "sum",
    "len",
    "upper",
    "lower",
    "trim",
    "split",
    "join",
    "replace",
    "contains",
    "map",
    "where",
    "reduce",
    "first",
    "last",
    "take",
    "skip",
    "reverse",
    "sort",
    "to_string",
    "to_int",
    "to_float",
    "parse_json",
    "to_json",
    "type_of",
    "is_null",
    "is_array",
    "is_record",
    "is_string",
    "is_int",
    "is_float",
    "range",
    "keys",
    "values",
    "entries",
    "merge",
    "get",
    "set",
    "print",
];

// ============================================================================
// Evaluation
// ============================================================================

#[cfg(feature = "web")]
fn eval_wasm(stmts: &[Stmt], env: &mut Env) -> Result<Value, String> {
    let mut last = Value::Null;
    for s in stmts {
        env.set_input(None);
        last = eval_stmt_wasm(s, env)?;
    }
    Ok(last)
}

#[cfg(feature = "web")]
fn eval_stmt_wasm(stmt: &Stmt, env: &mut Env) -> Result<Value, String> {
    match stmt {
        Stmt::Let {
            name,
            value,
            is_mut,
            ..
        } => {
            let v = eval_expr_wasm(value, env)?;
            env.declare_var(name, v.clone(), *is_mut).map_err(|e| e)?;
            Ok(v)
        }
        Stmt::Expr(e) => eval_expr_wasm(e, env),
        Stmt::Import { .. } => Err("import not supported in WASM".to_string()),
        Stmt::Export { .. } => Err("export not supported in WASM".to_string()),
        Stmt::Cfg { .. } => Err("cfg not supported in WASM".to_string()),
    }
}

#[cfg(feature = "web")]
fn eval_expr_wasm(expr: &Expr, env: &mut Env) -> Result<Value, String> {
    match expr {
        Expr::LitBool(b) => Ok(Value::Bool(*b)),
        Expr::LitInt(n) => Ok(Value::Int(*n)),
        Expr::LitFloat(f) => Ok(Value::Float(*f)),
        Expr::LitStr(s) => Ok(Value::Str(interpolate_string(s, env)?)),
        Expr::Null => Ok(Value::Null),
        Expr::Ident(name) => env
            .get_var(name)
            .cloned()
            .ok_or_else(|| format!("undefined variable: {}", name)),
        Expr::Array(items) => {
            let mut vals = Vec::with_capacity(items.len());
            for e in items {
                vals.push(eval_expr_wasm(e, env)?);
            }
            Ok(Value::Array(vals))
        }
        Expr::Record(fields) => {
            let mut map = BTreeMap::new();
            for (k, v) in fields {
                map.insert(k.clone(), eval_expr_wasm(v, env)?);
            }
            Ok(Value::Record(map))
        }
        Expr::Lambda { params, body } => Ok(Value::Lambda(Lambda {
            params: params.clone(),
            body: body.clone(),
        })),
        Expr::Binary { left, op, right } => {
            let l = eval_expr_wasm(left, env)?;
            let r = eval_expr_wasm(right, env)?;
            eval_binop(op, &l, &r)
        }
        Expr::Unary { op, expr: e } => {
            let val = eval_expr_wasm(e, env)?;
            match (op, &val) {
                (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                (UnOp::Neg, Value::Int(n)) => Ok(Value::Int(-n)),
                (UnOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
                _ => Err(format!("invalid unary operation on {}", val.type_name())),
            }
        }
        Expr::Call {
            callee,
            args,
            named,
        } => {
            if let Expr::Ident(func_name) = callee.as_ref() {
                let mut evaled_args = Vec::with_capacity(args.len());
                for a in args {
                    evaled_args.push(eval_expr_wasm(a, env)?);
                }
                let mut evaled_named = Vec::with_capacity(named.len());
                for (k, v) in named {
                    evaled_named.push((k.clone(), eval_expr_wasm(v, env)?));
                }
                eval_call_wasm(func_name, &evaled_args, &evaled_named, None, env)
            } else {
                let callee_val = eval_expr_wasm(callee, env)?;
                if let Value::Lambda(lam) = callee_val {
                    let mut evaled_args = Vec::with_capacity(args.len());
                    for a in args {
                        evaled_args.push(eval_expr_wasm(a, env)?);
                    }
                    eval_lambda(&lam, &evaled_args, env)
                } else {
                    Err(format!("cannot call {}", callee_val.type_name()))
                }
            }
        }
        Expr::Pipe { left, right } => {
            let left_val = eval_expr_wasm(left, env)?;
            eval_piped_wasm(right, left_val, env)
        }
        Expr::MemberAccess { object, field } => {
            let obj = eval_expr_wasm(object, env)?;
            match obj {
                Value::Record(map) => map
                    .get(field)
                    .cloned()
                    .ok_or_else(|| format!("field '{}' not found", field)),
                Value::Table(table) => {
                    let col: Vec<Value> = table
                        .rows
                        .iter()
                        .filter_map(|row: &BTreeMap<String, Value>| row.get(field).cloned())
                        .collect();
                    Ok(Value::Array(col))
                }
                _ => Err(format!("cannot access field on {}", obj.type_name())),
            }
        }
        Expr::Match { scrutinee, arms } => {
            let val = eval_expr_wasm(scrutinee, env)?;
            for arm in arms {
                if let Some(bindings) = match_pattern(&arm.pattern, &val) {
                    if let Some(guard) = &arm.guard {
                        let mut inner = env.clone();
                        for (k, v) in &bindings {
                            inner.set_var_unchecked(k.clone(), v.clone());
                        }
                        let guard_result = eval_expr_wasm(guard, &mut inner)?;
                        if !guard_result.as_bool().map_err(|e| e.to_string())? {
                            continue;
                        }
                    }
                    let mut inner = env.clone();
                    for (k, v) in bindings {
                        inner.set_var_unchecked(k, v);
                    }
                    return eval_expr_wasm(&arm.body, &mut inner);
                }
            }
            Err("no match arm matched".to_string())
        }
        Expr::AsyncLambda { .. } => Err("async not supported in WASM".to_string()),
        Expr::Await(_) => Err("await not supported in WASM".to_string()),
        Expr::TryCatch {
            try_expr,
            catch_var,
            catch_expr,
        } => match eval_expr_wasm(try_expr, env) {
            Ok(v) => Ok(v),
            Err(e) => {
                let mut inner = env.clone();
                if let Some(var) = catch_var {
                    inner.set_var_unchecked(var.clone(), Value::Str(e));
                }
                eval_expr_wasm(catch_expr, &mut inner)
            }
        },
        Expr::Throw(e) => {
            let val = eval_expr_wasm(e, env)?;
            Err(format_value(&val))
        }
    }
}

#[cfg(feature = "web")]
fn eval_piped_wasm(expr: &Expr, piped: Value, env: &mut Env) -> Result<Value, String> {
    match expr {
        Expr::Call {
            callee,
            args,
            named,
        } => {
            if let Expr::Ident(func_name) = callee.as_ref() {
                let mut evaled_args = Vec::with_capacity(args.len());
                for a in args {
                    evaled_args.push(eval_expr_wasm(a, env)?);
                }
                let mut evaled_named = Vec::with_capacity(named.len());
                for (k, v) in named {
                    evaled_named.push((k.clone(), eval_expr_wasm(v, env)?));
                }
                eval_call_wasm(func_name, &evaled_args, &evaled_named, Some(piped), env)
            } else {
                Err("piped expression must be a function call".to_string())
            }
        }
        Expr::Ident(name) => {
            if let Some(Value::Lambda(lam)) = env.get_var(name) {
                let lam = lam.clone();
                eval_lambda(&lam, &[piped], env)
            } else {
                eval_call_wasm(name, &[], &[], Some(piped), env)
            }
        }
        _ => Err("cannot pipe to this expression".to_string()),
    }
}

#[cfg(feature = "web")]
fn eval_lambda(lam: &Lambda, args: &[Value], env: &mut Env) -> Result<Value, String> {
    if args.len() != lam.params.len() {
        return Err(format!(
            "expected {} arguments, got {}",
            lam.params.len(),
            args.len()
        ));
    }
    let mut inner = env.clone();
    for (param, arg) in lam.params.iter().zip(args.iter()) {
        inner.set_var_unchecked(param.clone(), arg.clone());
    }
    eval_expr_wasm(&lam.body, &mut inner)
}

#[cfg(feature = "web")]
fn eval_binop(op: &BinOp, left: &Value, right: &Value) -> Result<Value, String> {
    match (op, left, right) {
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
        (BinOp::Div, Value::Float(a), Value::Float(b)) if *b != 0.0 => Ok(Value::Float(a / b)),
        (BinOp::Div, Value::Int(a), Value::Float(b)) if *b != 0.0 => {
            Ok(Value::Float(*a as f64 / b))
        }
        (BinOp::Div, Value::Float(a), Value::Int(b)) if *b != 0 => Ok(Value::Float(a / *b as f64)),
        (BinOp::Div, _, _) => Err("division by zero".to_string()),
        (BinOp::Rem, Value::Int(a), Value::Int(b)) if *b != 0 => Ok(Value::Int(a % b)),
        (BinOp::Rem, _, _) => Err("modulo by zero".to_string()),
        (BinOp::Pow, Value::Int(a), Value::Int(b)) => Ok(Value::Float((*a as f64).powf(*b as f64))),
        (BinOp::Pow, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(*b))),
        (BinOp::Pow, Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).powf(*b))),
        (BinOp::Pow, Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.powf(*b as f64))),
        (BinOp::Eq, a, b) => Ok(Value::Bool(values_equal(a, b))),
        (BinOp::Ne, a, b) => Ok(Value::Bool(!values_equal(a, b))),
        (BinOp::Lt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
        (BinOp::Lt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a < b)),
        (BinOp::Lte, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
        (BinOp::Lte, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a <= b)),
        (BinOp::Gt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
        (BinOp::Gt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a > b)),
        (BinOp::Gte, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
        (BinOp::Gte, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a >= b)),
        (BinOp::And, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a && *b)),
        (BinOp::Or, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a || *b)),
        _ => Err(format!(
            "invalid operation {:?} on {} and {}",
            op,
            left.type_name(),
            right.type_name()
        )),
    }
}

#[cfg(feature = "web")]
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => (a - b).abs() < f64::EPSILON,
        (Value::Int(a), Value::Float(b)) => (*a as f64 - b).abs() < f64::EPSILON,
        (Value::Float(a), Value::Int(b)) => (a - *b as f64).abs() < f64::EPSILON,
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && {
                let mut result = true;
                for i in 0..a.len() {
                    if !values_equal(&a[i], &b[i]) {
                        result = false;
                        break;
                    }
                }
                result
            }
        }
        (Value::Record(a), Value::Record(b)) => {
            if a.len() != b.len() {
                return false;
            }
            for (k, v) in a.iter() {
                match b.get(k) {
                    Some(bv) if values_equal(v, bv) => {}
                    _ => return false,
                }
            }
            true
        }
        _ => false,
    }
}

#[cfg(feature = "web")]
fn match_pattern(pattern: &Pattern, value: &Value) -> Option<Vec<(String, Value)>> {
    match (pattern, value) {
        (Pattern::Wildcard, _) => Some(vec![]),
        (Pattern::Ident(name), val) => Some(vec![(name.clone(), val.clone())]),
        (Pattern::LitInt(a), Value::Int(b)) if a == b => Some(vec![]),
        (Pattern::LitStr(a), Value::Str(b)) if a == b => Some(vec![]),
        (Pattern::LitBool(a), Value::Bool(b)) if a == b => Some(vec![]),
        (Pattern::Null, Value::Null) => Some(vec![]),
        (Pattern::Array(pats), Value::Array(vals)) if pats.len() == vals.len() => {
            let mut bindings = vec![];
            for i in 0..pats.len() {
                bindings.extend(match_pattern(&pats[i], &vals[i])?);
            }
            Some(bindings)
        }
        (Pattern::Record(pats), Value::Record(vals)) => {
            let mut bindings = vec![];
            for (key, pat) in pats {
                let val = vals.get(key)?;
                bindings.extend(match_pattern(pat, val)?);
            }
            Some(bindings)
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
            let mut expr_str = String::new();
            let mut depth = 1;
            while let Some(c) = chars.next() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                expr_str.push(c);
            }
            let stmts =
                parse_program(&expr_str).map_err(|e| format!("interpolation error: {}", e))?;
            if let Some(Stmt::Expr(e)) = stmts.first() {
                let val = eval_expr_wasm(e, env)?;
                result.push_str(&format_value(&val));
            }
        } else {
            result.push(c);
        }
    }
    Ok(result)
}

// ============================================================================
// Builtins
// ============================================================================

#[cfg(feature = "web")]
fn eval_call_wasm(
    func: &str,
    args: &[Value],
    _named: &[(String, Value)],
    piped: Option<Value>,
    env: &mut Env,
) -> Result<Value, String> {
    // User-defined function
    if let Some(Value::Lambda(lam)) = env.get_var(func) {
        let lam = lam.clone();
        let mut all_args: Vec<Value> = piped.into_iter().collect();
        all_args.extend(args.iter().cloned());
        return eval_lambda(&lam, &all_args, env);
    }

    let input = piped.as_ref();
    match func {
        // Math
        "abs" => {
            let val = require_arg(input, args, 0)?;
            match val {
                Value::Int(n) => Ok(Value::Int(n.abs())),
                Value::Float(f) => Ok(Value::Float(f.abs())),
                _ => Err("abs requires a number".to_string()),
            }
        }
        "ceil" => {
            let val = require_arg(input, args, 0)?;
            match val {
                Value::Float(f) => Ok(Value::Int(f.ceil() as i64)),
                Value::Int(n) => Ok(Value::Int(*n)),
                _ => Err("ceil requires a number".to_string()),
            }
        }
        "floor" => {
            let val = require_arg(input, args, 0)?;
            match val {
                Value::Float(f) => Ok(Value::Int(f.floor() as i64)),
                Value::Int(n) => Ok(Value::Int(*n)),
                _ => Err("floor requires a number".to_string()),
            }
        }
        "round" => {
            let val = require_arg(input, args, 0)?;
            match val {
                Value::Float(f) => Ok(Value::Int(f.round() as i64)),
                Value::Int(n) => Ok(Value::Int(*n)),
                _ => Err("round requires a number".to_string()),
            }
        }
        "sqrt" => {
            let val = require_arg(input, args, 0)?;
            match val {
                Value::Int(n) => Ok(Value::Float((*n as f64).sqrt())),
                Value::Float(f) => Ok(Value::Float(f.sqrt())),
                _ => Err("sqrt requires a number".to_string()),
            }
        }
        "pow" => {
            let base = require_arg(input, args, 0)?;
            let exp_idx = if input.is_some() { 0 } else { 1 };
            let exp = args.get(exp_idx).ok_or("pow requires an exponent")?;
            match (base, exp) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Float((*a as f64).powf(*b as f64))),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(*b))),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).powf(*b))),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.powf(*b as f64))),
                _ => Err("pow requires numbers".to_string()),
            }
        }
        "min" => {
            let arr = require_array(input, args)?;
            let mut min_val: Option<f64> = None;
            for v in arr {
                let n = match v {
                    Value::Int(n) => *n as f64,
                    Value::Float(f) => *f,
                    _ => continue,
                };
                min_val = Some(min_val.map_or(n, |m| if n < m { n } else { m }));
            }
            min_val
                .map(Value::Float)
                .ok_or_else(|| "min requires a non-empty numeric array".to_string())
        }
        "max" => {
            let arr = require_array(input, args)?;
            let mut max_val: Option<f64> = None;
            for v in arr {
                let n = match v {
                    Value::Int(n) => *n as f64,
                    Value::Float(f) => *f,
                    _ => continue,
                };
                max_val = Some(max_val.map_or(n, |m| if n > m { n } else { m }));
            }
            max_val
                .map(Value::Float)
                .ok_or_else(|| "max requires a non-empty numeric array".to_string())
        }
        "sum" => {
            let arr = require_array(input, args)?;
            let mut total: f64 = 0.0;
            for v in arr {
                match v {
                    Value::Int(n) => total += *n as f64,
                    Value::Float(f) => total += f,
                    _ => {}
                }
            }
            Ok(Value::Float(total))
        }
        // String
        "len" => {
            let val = require_arg(input, args, 0)?;
            match val {
                Value::Str(s) => Ok(Value::Int(s.len() as i64)),
                Value::Array(a) => Ok(Value::Int(a.len() as i64)),
                Value::Record(r) => Ok(Value::Int(r.len() as i64)),
                _ => Err("len requires a string, array, or record".to_string()),
            }
        }
        "upper" => {
            let val = require_arg(input, args, 0)?;
            if let Value::Str(s) = val {
                Ok(Value::Str(s.to_uppercase()))
            } else {
                Err("upper requires a string".to_string())
            }
        }
        "lower" => {
            let val = require_arg(input, args, 0)?;
            if let Value::Str(s) = val {
                Ok(Value::Str(s.to_lowercase()))
            } else {
                Err("lower requires a string".to_string())
            }
        }
        "trim" => {
            let val = require_arg(input, args, 0)?;
            if let Value::Str(s) = val {
                Ok(Value::Str(s.trim().to_string()))
            } else {
                Err("trim requires a string".to_string())
            }
        }
        "split" => {
            let val = require_arg(input, args, 0)?;
            let sep_idx = if input.is_some() { 0 } else { 1 };
            let sep = args
                .get(sep_idx)
                .and_then(|v| {
                    if let Value::Str(s) = v {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .unwrap_or(" ");
            if let Value::Str(s) = val {
                let parts: Vec<Value> = s.split(sep).map(|p| Value::Str(p.to_string())).collect();
                Ok(Value::Array(parts))
            } else {
                Err("split requires a string".to_string())
            }
        }
        "join" => {
            let arr = require_array(input, args)?;
            let sep_idx = if input.is_some() { 0 } else { 1 };
            let sep = args
                .get(sep_idx)
                .and_then(|v| {
                    if let Value::Str(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            let mut parts: Vec<String> = Vec::with_capacity(arr.len());
            for v in arr {
                parts.push(format_value(v));
            }
            Ok(Value::Str(parts.join(&sep)))
        }
        "replace" => {
            let val = require_arg(input, args, 0)?;
            let from_idx = if input.is_some() { 0 } else { 1 };
            let to_idx = if input.is_some() { 1 } else { 2 };
            let from = args
                .get(from_idx)
                .and_then(|v| {
                    if let Value::Str(s) = v {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .ok_or("replace requires 'from' string")?;
            let to = args
                .get(to_idx)
                .and_then(|v| {
                    if let Value::Str(s) = v {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .ok_or("replace requires 'to' string")?;
            if let Value::Str(s) = val {
                Ok(Value::Str(s.replace(from, to)))
            } else {
                Err("replace requires a string".to_string())
            }
        }
        "contains" => {
            let val = require_arg(input, args, 0)?;
            let needle_idx = if input.is_some() { 0 } else { 1 };
            let needle = args
                .get(needle_idx)
                .ok_or("contains requires a search value")?;
            match (val, needle) {
                (Value::Str(s), Value::Str(n)) => Ok(Value::Bool(s.contains(n.as_str()))),
                (Value::Array(arr), n) => {
                    let found = arr.iter().any(|v: &Value| values_equal(v, n));
                    Ok(Value::Bool(found))
                }
                _ => Err("contains requires a string or array".to_string()),
            }
        }
        // Array
        "map" => {
            let arr = require_array(input, args)?;
            let func_idx = if input.is_some() { 0 } else { 1 };
            let func = args.get(func_idx).ok_or("map requires a function")?;
            if let Value::Lambda(lam) = func {
                let mut results = Vec::with_capacity(arr.len());
                for item in arr {
                    results.push(eval_lambda(lam, &[item.clone()], env)?);
                }
                Ok(Value::Array(results))
            } else {
                Err("map requires a lambda function".to_string())
            }
        }
        "where" => {
            let arr = require_array(input, args)?;
            let func_idx = if input.is_some() { 0 } else { 1 };
            let func = args.get(func_idx).ok_or("where requires a function")?;
            if let Value::Lambda(lam) = func {
                let mut results = Vec::new();
                for item in arr {
                    let result = eval_lambda(lam, &[item.clone()], env)?;
                    match result {
                        Value::Bool(true) => results.push(item.clone()),
                        Value::Bool(false) => {}
                        _ => return Err("where predicate must return Bool".to_string()),
                    }
                }
                Ok(Value::Array(results))
            } else {
                Err("where requires a lambda function".to_string())
            }
        }
        "reduce" => {
            let arr = require_array(input, args)?;
            let func_idx = if input.is_some() { 0 } else { 1 };
            let init_idx = if input.is_some() { 1 } else { 2 };
            let func = args.get(func_idx).ok_or("reduce requires a function")?;
            let init = args
                .get(init_idx)
                .ok_or("reduce requires an initial value")?;
            if let Value::Lambda(lam) = func {
                let mut acc = init.clone();
                for item in arr {
                    acc = eval_lambda(lam, &[acc, item.clone()], env)?;
                }
                Ok(acc)
            } else {
                Err("reduce requires a lambda function".to_string())
            }
        }
        "first" => {
            let arr = require_array(input, args)?;
            arr.first()
                .cloned()
                .ok_or_else(|| "array is empty".to_string())
        }
        "last" => {
            let arr = require_array(input, args)?;
            arr.last()
                .cloned()
                .ok_or_else(|| "array is empty".to_string())
        }
        "take" => {
            let arr = require_array(input, args)?;
            let n_idx = if input.is_some() { 0 } else { 1 };
            let n = args
                .get(n_idx)
                .and_then(|v| {
                    if let Value::Int(n) = v {
                        Some(*n as usize)
                    } else {
                        None
                    }
                })
                .ok_or("take requires a count")?;
            let taken: Vec<Value> = arr.iter().take(n).cloned().collect();
            Ok(Value::Array(taken))
        }
        "skip" => {
            let arr = require_array(input, args)?;
            let n_idx = if input.is_some() { 0 } else { 1 };
            let n = args
                .get(n_idx)
                .and_then(|v| {
                    if let Value::Int(n) = v {
                        Some(*n as usize)
                    } else {
                        None
                    }
                })
                .ok_or("skip requires a count")?;
            let skipped: Vec<Value> = arr.iter().skip(n).cloned().collect();
            Ok(Value::Array(skipped))
        }
        "reverse" => {
            let arr = require_array(input, args)?;
            let reversed: Vec<Value> = arr.iter().rev().cloned().collect();
            Ok(Value::Array(reversed))
        }
        "sort" => {
            let arr = require_array(input, args)?;
            let mut sorted = arr.clone();
            sorted.sort_by(|a: &Value, b: &Value| match (a, b) {
                (Value::Int(x), Value::Int(y)) => x.cmp(y),
                (Value::Float(x), Value::Float(y)) => {
                    x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                }
                (Value::Str(x), Value::Str(y)) => x.cmp(y),
                _ => std::cmp::Ordering::Equal,
            });
            Ok(Value::Array(sorted))
        }
        // Type conversion
        "to_string" => {
            let val = require_arg(input, args, 0)?;
            Ok(Value::Str(format_value(val)))
        }
        "to_int" => {
            let val = require_arg(input, args, 0)?;
            match val {
                Value::Int(n) => Ok(Value::Int(*n)),
                Value::Float(f) => Ok(Value::Int(*f as i64)),
                Value::Str(s) => s
                    .parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| format!("cannot parse '{}' as int", s)),
                Value::Bool(b) => Ok(Value::Int(if *b { 1 } else { 0 })),
                _ => Err(format!("cannot convert {} to int", val.type_name())),
            }
        }
        "to_float" => {
            let val = require_arg(input, args, 0)?;
            match val {
                Value::Int(n) => Ok(Value::Float(*n as f64)),
                Value::Float(f) => Ok(Value::Float(*f)),
                Value::Str(s) => s
                    .parse::<f64>()
                    .map(Value::Float)
                    .map_err(|_| format!("cannot parse '{}' as float", s)),
                _ => Err(format!("cannot convert {} to float", val.type_name())),
            }
        }
        "parse_json" => {
            let val = require_arg(input, args, 0)?;
            if let Value::Str(s) = val {
                let json: serde_json::Value =
                    serde_json::from_str(s).map_err(|e| format!("JSON parse error: {}", e))?;
                Ok(json_to_value(&json))
            } else {
                Err("parse_json requires a string".to_string())
            }
        }
        "to_json" => {
            let val = require_arg(input, args, 0)?;
            let json = value_to_serde_json(val);
            serde_json::to_string_pretty(&json)
                .map(Value::Str)
                .map_err(|e| format!("JSON error: {}", e))
        }
        // Type checking
        "type_of" => {
            let val = require_arg(input, args, 0)?;
            Ok(Value::Str(val.type_name().to_string()))
        }
        "is_null" => {
            let val = require_arg(input, args, 0)?;
            Ok(Value::Bool(matches!(val, Value::Null)))
        }
        "is_array" => {
            let val = require_arg(input, args, 0)?;
            Ok(Value::Bool(matches!(val, Value::Array(_))))
        }
        "is_record" => {
            let val = require_arg(input, args, 0)?;
            Ok(Value::Bool(matches!(val, Value::Record(_))))
        }
        "is_string" => {
            let val = require_arg(input, args, 0)?;
            Ok(Value::Bool(matches!(val, Value::Str(_))))
        }
        "is_int" => {
            let val = require_arg(input, args, 0)?;
            Ok(Value::Bool(matches!(val, Value::Int(_))))
        }
        "is_float" => {
            let val = require_arg(input, args, 0)?;
            Ok(Value::Bool(matches!(val, Value::Float(_))))
        }
        // Utility
        "range" => {
            let start = args
                .first()
                .and_then(|v| {
                    if let Value::Int(n) = v {
                        Some(*n)
                    } else {
                        None
                    }
                })
                .ok_or("range requires start")?;
            let end = args
                .get(1)
                .and_then(|v| {
                    if let Value::Int(n) = v {
                        Some(*n)
                    } else {
                        None
                    }
                })
                .ok_or("range requires end")?;
            let step = args
                .get(2)
                .and_then(|v| {
                    if let Value::Int(n) = v {
                        Some(*n)
                    } else {
                        None
                    }
                })
                .unwrap_or(1);
            if step == 0 {
                return Err("range step cannot be zero".to_string());
            }
            let mut arr = Vec::new();
            let mut i = start;
            if step > 0 {
                while i < end {
                    arr.push(Value::Int(i));
                    i += step;
                }
            } else {
                while i > end {
                    arr.push(Value::Int(i));
                    i += step;
                }
            }
            Ok(Value::Array(arr))
        }
        "keys" => {
            let val = require_arg(input, args, 0)?;
            if let Value::Record(map) = val {
                let keys: Vec<Value> = map.keys().map(|k| Value::Str(k.clone())).collect();
                Ok(Value::Array(keys))
            } else {
                Err("keys requires a record".to_string())
            }
        }
        "values" => {
            let val = require_arg(input, args, 0)?;
            if let Value::Record(map) = val {
                let vals: Vec<Value> = map.values().cloned().collect();
                Ok(Value::Array(vals))
            } else {
                Err("values requires a record".to_string())
            }
        }
        "entries" => {
            let val = require_arg(input, args, 0)?;
            if let Value::Record(map) = val {
                let mut entries = Vec::with_capacity(map.len());
                for (k, v) in map.iter() {
                    let mut rec = BTreeMap::new();
                    rec.insert("key".to_string(), Value::Str(k.clone()));
                    rec.insert("value".to_string(), v.clone());
                    entries.push(Value::Record(rec));
                }
                Ok(Value::Array(entries))
            } else {
                Err("entries requires a record".to_string())
            }
        }
        "merge" => {
            let val1 = require_arg(input, args, 0)?;
            let val2_idx = if input.is_some() { 0 } else { 1 };
            let val2 = args.get(val2_idx).ok_or("merge requires two records")?;
            match (val1, val2) {
                (Value::Record(a), Value::Record(b)) => {
                    let mut merged = a.clone();
                    for (k, v) in b.iter() {
                        merged.insert(k.clone(), v.clone());
                    }
                    Ok(Value::Record(merged))
                }
                _ => Err("merge requires two records".to_string()),
            }
        }
        "get" => {
            let val = require_arg(input, args, 0)?;
            let key_idx = if input.is_some() { 0 } else { 1 };
            let key = args.get(key_idx).ok_or("get requires a key")?;
            match (val, key) {
                (Value::Record(map), Value::Str(k)) => {
                    Ok(map.get(k).cloned().unwrap_or(Value::Null))
                }
                (Value::Array(arr), Value::Int(i)) => {
                    let idx = if *i < 0 { arr.len() as i64 + i } else { *i } as usize;
                    Ok(arr.get(idx).cloned().unwrap_or(Value::Null))
                }
                _ => Err("get requires a record/key or array/index".to_string()),
            }
        }
        "set" => {
            let val = require_arg(input, args, 0)?;
            let key_idx = if input.is_some() { 0 } else { 1 };
            let new_val_idx = if input.is_some() { 1 } else { 2 };
            let key = args.get(key_idx).ok_or("set requires a key")?;
            let new_val = args.get(new_val_idx).ok_or("set requires a value")?;
            match (val, key) {
                (Value::Record(map), Value::Str(k)) => {
                    let mut new_map = map.clone();
                    new_map.insert(k.clone(), new_val.clone());
                    Ok(Value::Record(new_map))
                }
                (Value::Array(arr), Value::Int(i)) => {
                    let idx = if *i < 0 { arr.len() as i64 + i } else { *i } as usize;
                    let mut new_arr = arr.clone();
                    if idx < new_arr.len() {
                        new_arr[idx] = new_val.clone();
                    }
                    Ok(Value::Array(new_arr))
                }
                _ => Err("set requires a record/key or array/index".to_string()),
            }
        }
        "print" => {
            let val = require_arg(input, args, 0)?;
            Ok(Value::Str(format_value(val)))
        }
        _ => Err(format!("unknown function: {}", func)),
    }
}

#[cfg(feature = "web")]
fn require_arg<'a>(
    piped: Option<&'a Value>,
    args: &'a [Value],
    index: usize,
) -> Result<&'a Value, String> {
    piped
        .or_else(|| args.get(index))
        .ok_or_else(|| "missing required argument".to_string())
}

#[cfg(feature = "web")]
fn require_array<'a>(
    piped: Option<&'a Value>,
    args: &'a [Value],
) -> Result<&'a Vec<Value>, String> {
    let val = piped
        .or_else(|| args.first())
        .ok_or("missing array argument")?;
    match val {
        Value::Array(arr) => Ok(arr),
        _ => Err(format!("expected array, got {}", val.type_name())),
    }
}

// ============================================================================
// JSON conversion
// ============================================================================

#[cfg(feature = "web")]
fn value_to_json(value: &Value) -> Result<String, JsValue> {
    let json = value_to_serde_json(value);
    serde_json::to_string_pretty(&json)
        .map_err(|e| JsValue::from_str(&format!("JSON error: {}", e)))
}

#[cfg(feature = "web")]
fn value_to_serde_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(n) => serde_json::json!(*n),
        Value::Float(f) => serde_json::json!(*f),
        Value::Str(s) => serde_json::Value::String(s.clone()),
        Value::Uri(u) => serde_json::Value::String(u.clone()),
        Value::Array(arr) => {
            let items: Vec<serde_json::Value> = arr.iter().map(value_to_serde_json).collect();
            serde_json::Value::Array(items)
        }
        Value::Record(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), value_to_serde_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Table(table) => {
            let rows: Vec<serde_json::Value> = table
                .rows
                .iter()
                .map(|row: &BTreeMap<String, Value>| {
                    let obj: serde_json::Map<String, serde_json::Value> = row
                        .iter()
                        .map(|(k, v)| (k.clone(), value_to_serde_json(v)))
                        .collect();
                    serde_json::Value::Object(obj)
                })
                .collect();
            serde_json::json!({"type": "Table", "schema": table.schema, "rows": rows})
        }
        Value::Lambda(lam) => serde_json::json!({"type": "Lambda", "params": lam.params}),
        Value::AsyncLambda(lam) => serde_json::json!({"type": "AsyncLambda", "params": lam.params}),
        Value::Future(_) => serde_json::json!({"type": "Future"}),
        Value::Error(e) => serde_json::json!({"type": "Error", "message": e}),
        Value::Builtin(b) => serde_json::json!({"type": "Builtin", "name": b.name}),
    }
}

#[cfg(feature = "web")]
fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::Str(s.clone()),
        serde_json::Value::Array(arr) => {
            let items: Vec<Value> = arr.iter().map(json_to_value).collect();
            Value::Array(items)
        }
        serde_json::Value::Object(obj) => {
            let map: BTreeMap<String, Value> = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Value::Record(map)
        }
    }
}

#[cfg(feature = "web")]
fn format_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => {
            if f.fract() == 0.0 {
                format!("{:.1}", f)
            } else {
                f.to_string()
            }
        }
        Value::Str(s) => s.clone(),
        Value::Uri(u) => u.clone(),
        Value::Array(arr) => {
            let mut items = Vec::with_capacity(arr.len());
            for v in arr {
                items.push(format_value(v));
            }
            format!("[{}]", items.join(", "))
        }
        Value::Record(map) => {
            let mut items = Vec::with_capacity(map.len());
            for (k, v) in map.iter() {
                items.push(format!("{}: {}", k, format_value(v)));
            }
            format!("{{{}}}", items.join(", "))
        }
        Value::Table(table) => {
            let mut lines = vec![];
            if !table.schema.is_empty() {
                lines.push(table.schema.join(" | "));
                let dashes: Vec<&str> = table.schema.iter().map(|_| "---").collect();
                lines.push(dashes.join(" | "));
            }
            for row in &table.rows {
                let mut cells = Vec::with_capacity(table.schema.len());
                for col in &table.schema {
                    let cell = row.get(col).map(format_value).unwrap_or_default();
                    cells.push(cell);
                }
                lines.push(cells.join(" | "));
            }
            lines.join("\n")
        }
        Value::Lambda(lam) => format!("fn({}) => ...", lam.params.join(", ")),
        Value::AsyncLambda(lam) => format!("async fn({}) => ...", lam.params.join(", ")),
        Value::Future(_) => "<Future>".to_string(),
        Value::Error(e) => format!("Error: {}", e),
        Value::Builtin(b) => format!("<builtin:{}>", b.name),
    }
}

// ============================================================================
// Standalone exports
// ============================================================================

#[cfg(feature = "web")]
#[wasm_bindgen]
pub fn ae_eval(code: &str) -> Result<String, JsValue> {
    let stmts =
        parse_program(code).map_err(|e| JsValue::from_str(&format!("Parse error: {}", e)))?;
    let mut env = Env::default();
    let result = eval_wasm(&stmts, &mut env)
        .map_err(|e| JsValue::from_str(&format!("Eval error: {}", e)))?;
    value_to_json(&result)
}

#[cfg(feature = "web")]
#[wasm_bindgen]
pub fn ae_parse(code: &str) -> Result<String, JsValue> {
    match parse_program(code) {
        Ok(stmts) => serde_json::to_string_pretty(&stmts)
            .map_err(|e| JsValue::from_str(&format!("JSON error: {}", e))),
        Err(e) => Err(JsValue::from_str(&format!("Parse error: {}", e))),
    }
}

#[cfg(feature = "web")]
#[wasm_bindgen]
pub fn ae_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
