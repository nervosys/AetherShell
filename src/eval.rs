use crate::ast::{BinOp, CfgCondition, Expr, Stmt, UnOp, Visibility};
use crate::builtins;
use crate::env::Env;
use crate::value::{AsyncLambda, Future, Lambda, Value};
use anyhow::{anyhow, Result};
use std::collections::BTreeMap;

/// Evaluate a cfg condition at runtime
fn eval_cfg_condition(condition: &CfgCondition) -> Result<bool> {
    match condition {
        CfgCondition::Platform(platform) => {
            let current_os = std::env::consts::OS;
            Ok(match platform.as_str() {
                "windows" => current_os == "windows",
                "linux" => current_os == "linux",
                "macos" => current_os == "macos",
                "unix" => current_os != "windows",
                other => current_os == other,
            })
        }
        CfgCondition::Feature(feature) => {
            // Check environment variable AETHER_FEATURES for enabled features
            let features = std::env::var("AETHER_FEATURES").unwrap_or_default();
            Ok(features.split(',').any(|f| f.trim() == feature))
        }
        CfgCondition::Not(inner) => Ok(!eval_cfg_condition(inner)?),
        CfgCondition::All(conditions) => {
            for cond in conditions {
                if !eval_cfg_condition(cond)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        CfgCondition::Any(conditions) => {
            for cond in conditions {
                if eval_cfg_condition(cond)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

pub fn eval_program(stmts: &[Stmt], env: &mut Env) -> Result<Value> {
    let mut last = Value::Null;
    for s in stmts {
        // Clear any pipe input between statements to prevent leakage
        env.set_input(None);
        last = eval_stmt(s, env)?;
    }
    Ok(last)
}

pub fn eval_stmt(stmt: &Stmt, env: &mut Env) -> Result<Value> {
    match stmt {
        Stmt::Let {
            name,
            value,
            is_mut,
            visibility,
        } => {
            let v = eval_expr(value, env)?;
            env.declare_var(name, v.clone(), *is_mut)
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            // Mark as public if visibility is Pub
            if *visibility == Visibility::Pub {
                env.set_public(name);
            }

            Ok(v)
        }
        Stmt::Expr(e) => eval_expr(e, env),
        Stmt::Import {
            items,
            source,
            alias,
        } => {
            // Import statements are handled by the ImportResolver
            // This is a fallback for when imports are evaluated directly
            #[cfg(feature = "native")]
            {
                use crate::packages::ImportResolver;

                let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                let mut resolver = ImportResolver::new(cwd);

                resolver.process_import(items, source, alias, env, |stmts, module_env| {
                    eval_program(stmts, module_env)
                })?;

                Ok(Value::Null)
            }
            #[cfg(not(feature = "native"))]
            {
                let _ = (items, source, alias);
                Err(anyhow!("import statements are not supported in this build"))
            }
        }
        Stmt::Export { items, from_source } => {
            #[cfg(feature = "native")]
            {
                if let Some(source) = from_source {
                    // Re-export from another module
                    use crate::packages::ImportResolver;

                    let cwd =
                        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                    let mut resolver = ImportResolver::new(cwd);

                    // Load the source module
                    let module_env = resolver
                        .load_module(source, |stmts, module_env| eval_program(stmts, module_env))?;

                    // Import and re-export specified items
                    for item in items {
                        let value = module_env.get_var(&item.name).cloned().ok_or_else(|| {
                            anyhow!("'{}' not found in module '{}'", item.name, source)
                        })?;

                        let export_name = item.alias.as_ref().unwrap_or(&item.name);
                        env.set_var_unchecked(export_name.clone(), value);
                        env.add_export(export_name);
                    }
                } else {
                    // Export local items
                    for item in items {
                        if env.get_var(&item.name).is_none() {
                            return Err(anyhow!("cannot export '{}': not defined", item.name));
                        }

                        let export_name = item.alias.as_ref().unwrap_or(&item.name);
                        if let Some(alias) = &item.alias {
                            // Create alias for the exported value
                            let value = env.get_var(&item.name).cloned().unwrap();
                            env.set_var_unchecked(alias.clone(), value);
                        }
                        env.add_export(export_name);
                    }
                }
                Ok(Value::Null)
            }
            #[cfg(not(feature = "native"))]
            {
                let _ = (items, from_source);
                Err(anyhow!("export statements are not supported in this build"))
            }
        }
        Stmt::Cfg { condition, body } => {
            // Evaluate the cfg condition at runtime
            if eval_cfg_condition(condition)? {
                eval_stmt(body, env)
            } else {
                Ok(Value::Null)
            }
        }
    }
}

pub fn eval_expr(expr: &Expr, env: &mut Env) -> Result<Value> {
    match expr {
        // ---------- literals ----------
        Expr::LitInt(n) => Ok(Value::Int(*n)),
        Expr::LitFloat(f) => Ok(Value::Float(*f)),
        Expr::LitStr(s) => {
            // Handle string interpolation: ${expr}
            if s.contains("${") {
                interpolate_string(s, env)
            } else {
                Ok(Value::Str(s.clone()))
            }
        }
        Expr::LitBool(b) => Ok(Value::Bool(*b)),
        Expr::Null => Ok(Value::Null),

        // ---------- variables ----------
        Expr::Ident(name) => Ok(env.get_var(name).cloned().unwrap_or(Value::Null)),

        // ---------- collections ----------
        Expr::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                out.push(eval_expr(it, env)?);
            }
            Ok(Value::Array(out))
        }
        Expr::Record(kvs) => {
            let mut m = BTreeMap::new();
            for (k, v) in kvs {
                m.insert(k.clone(), eval_expr(v, env)?);
            }
            Ok(Value::Record(m))
        }

        // ---------- lambda ----------
        Expr::Lambda { params, body } => Ok(Value::Lambda(Lambda {
            params: params.clone(),
            body: body.clone(),
        })),

        // ---------- async lambda ----------
        Expr::AsyncLambda { params, body } => Ok(Value::AsyncLambda(AsyncLambda {
            params: params.clone(),
            body: body.clone(),
        })),

        // ---------- await ----------
        Expr::Await(inner) => {
            let val = eval_expr(inner, env)?;
            match val {
                Value::Future(future) => {
                    // Execute the async lambda with its captured arguments
                    // Create a new environment for the lambda execution
                    let mut inner_env = Env::new();
                    for (param, arg) in future.lambda.params.iter().zip(future.args.iter()) {
                        let _ = inner_env.set_var(param.clone(), arg.clone());
                    }
                    eval_expr(&future.lambda.body, &mut inner_env)
                }
                // If it's not a future, just return the value (auto-await semantics)
                other => Ok(other),
            }
        }

        // ---------- try/catch ----------
        Expr::TryCatch {
            try_expr,
            catch_var,
            catch_expr,
        } => {
            // Try to evaluate the try expression
            match eval_expr(try_expr, env) {
                Ok(Value::Error(msg)) => {
                    // An error value was returned (from throw)
                    if let Some(var_name) = catch_var {
                        let _ = env.set_var(var_name.clone(), Value::Str(msg));
                    }
                    // Evaluate catch expression; if it also throws, that's the result
                    eval_expr(catch_expr, env)
                }
                Ok(val) => {
                    // Success - return the value
                    Ok(val)
                }
                Err(e) => {
                    // Runtime error - catch it. Safety refusals carry structured
                    // data: bind the catch variable to a Record {error: {code,
                    // message, hint, ...}} so agents can branch on `e.error.code`
                    // instead of parsing a string. Other errors stay as strings.
                    if let Some(var_name) = catch_var {
                        let caught = match e.downcast_ref::<crate::safety::SafetyError>() {
                            Some(se) => Value::from_json(&se.to_json()),
                            None => Value::Str(e.to_string()),
                        };
                        let _ = env.set_var(var_name.clone(), caught);
                    }
                    // Evaluate catch expression; if it also throws, that's the result
                    eval_expr(catch_expr, env)
                }
            }
        }

        // ---------- throw ----------
        Expr::Throw(inner) => {
            let val = eval_expr(inner, env)?;
            let msg = match val {
                Value::Str(s) => s,
                other => format!("{:?}", other),
            };
            Ok(Value::Error(msg))
        }

        // ---------- call ----------
        Expr::Call {
            callee,
            args,
            named: _,
        } => {
            // Collect evaluated positional args
            let mut vals = Vec::with_capacity(args.len());
            for a in args {
                vals.push(eval_expr(a, env)?);
            }

            // Capture pipe input if any (don’t move it)
            let pin = env.input().cloned();

            // 1) If callee is a plain identifier, prefer builtin (unless a var is bound)
            if let Expr::Ident(name) = &**callee {
                if let Some(v) = env.get_var(name).cloned() {
                    // If the bound var is a Lambda or AsyncLambda, call it. Otherwise fall back
                    // to builtin behavior (e.g. when shadowing with a non-function).
                    match v {
                        Value::Lambda(_) | Value::AsyncLambda(_) => {
                            return call_value_with_pipe(v, pin, vals, env)
                        }
                        _ => {
                            return builtins::call_with_input(name, vals, pin, env);
                        }
                    }
                } else {
                    // Not a bound var → treat as builtin name
                    return builtins::call_with_input(name, vals, pin, env);
                }
            }

            // 2) Otherwise evaluate callee and dispatch
            let f = eval_expr(callee, env)?;
            call_value_with_pipe(f, pin, vals, env)
        }

        // ---------- pipeline ----------
        Expr::Pipe { left, right } => {
            let left_val = eval_expr(left, env)?;

            // If RHS is an identifier: prefer calling a bound lambda with the
            // entire left value as an explicit arg; if not bound, treat as
            // builtin and pass left as pipe input.
            if let Expr::Ident(name) = &**right {
                if let Some(v) = env.get_var(name).cloned() {
                    match v {
                        Value::Lambda(_) | Value::AsyncLambda(_) => {
                            // pass as explicit arg so the user lambda receives the whole array
                            return call_value_with_pipe(v, None, vec![left_val], env);
                        }
                        _ => {
                            return crate::builtins::call_with_input(
                                name,
                                Vec::new(),
                                Some(left_val),
                                env,
                            );
                        }
                    }
                } else {
                    return crate::builtins::call_with_input(name, Vec::new(), Some(left_val), env);
                }
            }

            // If RHS is a lambda literal, evaluate it and call with pipe input
            // — this is the shorthand mapping form: `[arr] | fn(x)=> ...`.
            if let Expr::Lambda { .. } = &**right {
                let f = eval_expr(right, env)?;
                return call_value_with_pipe(f, Some(left_val), Vec::new(), env);
            }

            // Otherwise set pipe input and evaluate right normally (this allows
            // call expressions to pick up env.input()). Restore afterwards.
            let saved = env.input().cloned();
            env.set_input(Some(left_val));
            let res = eval_expr(right, env);
            // Restore
            match saved {
                Some(v) => env.set_input(Some(v)),
                None => env.set_input(None),
            }
            res
        }

        // ---------- unary ----------
        Expr::Unary { op, expr } => {
            let v = eval_expr(expr, env)?;
            match (op, v) {
                (UnOp::Neg, Value::Int(n)) => Ok(Value::Int(-n)),
                (UnOp::Neg, Value::Float(x)) => Ok(Value::Float(-x)),
                (UnOp::Not, v) => Ok(Value::Bool(!is_truthy(&v))),
                (_, other) => Err(anyhow!("bad unary op on {:?}", other)),
            }
        }

        // ---------- binary ----------
        Expr::Binary { left, op, right } => {
            let a = eval_expr(left, env)?;
            let b = eval_expr(right, env)?;
            binop(op, a, b)
        }

        // ---------- member access: record.field ----------
        Expr::MemberAccess { object, field } => {
            let obj = eval_expr(object, env)?;
            match obj {
                Value::Record(map) => map
                    .get(field)
                    .cloned()
                    .ok_or_else(|| anyhow!("field '{}' not found in record", field)),
                other => Err(anyhow!(
                    "cannot access field '{}' on non-record value: {:?}",
                    field,
                    other
                )),
            }
        }

        // ---------- pattern matching ----------
        Expr::Match { scrutinee, arms } => {
            let value = eval_expr(scrutinee, env)?;

            for arm in arms {
                // Try to match the pattern
                if let Some(bindings) = match_pattern(&arm.pattern, &value) {
                    // Check guard if present
                    if let Some(guard_expr) = &arm.guard {
                        // Create temporary environment with pattern bindings
                        let mut temp_env = env.clone();
                        for (name, val) in bindings.iter() {
                            temp_env.set_var_unchecked(name, val.clone());
                        }

                        let guard_result = eval_expr(guard_expr, &mut temp_env)?;
                        if !is_truthy(&guard_result) {
                            continue; // Guard failed, try next arm
                        }
                    }

                    // Pattern matched (and guard passed if present), bind variables and evaluate body
                    for (name, val) in bindings {
                        env.set_var_unchecked(&name, val);
                    }
                    return eval_expr(&arm.body, env);
                }
            }

            Err(anyhow!("match: no arm matched the value"))
        }
    }
}

/* ---------------- pattern matching helpers ---------------- */

use crate::ast::Pattern;
use std::collections::HashMap;

/// Attempt to match a pattern against a value.
/// Returns Some(bindings) if successful, None if no match.
fn match_pattern(pattern: &Pattern, value: &Value) -> Option<HashMap<String, Value>> {
    let mut bindings = HashMap::new();
    if match_pattern_impl(pattern, value, &mut bindings) {
        Some(bindings)
    } else {
        None
    }
}

fn match_pattern_impl(
    pattern: &Pattern,
    value: &Value,
    bindings: &mut HashMap<String, Value>,
) -> bool {
    match pattern {
        Pattern::Wildcard => true, // _ matches anything

        Pattern::Ident(name) => {
            // Variable binding - always matches and binds the value
            bindings.insert(name.clone(), value.clone());
            true
        }

        Pattern::LitInt(n) => matches!(value, Value::Int(v) if v == n),
        Pattern::LitStr(s) => matches!(value, Value::Str(v) if v == s),
        Pattern::LitBool(b) => matches!(value, Value::Bool(v) if v == b),
        Pattern::Null => matches!(value, Value::Null),

        Pattern::Constructor { name, args } => {
            // Match tagged records like Some(x) or None
            if let Value::Record(map) = value {
                // Check for _tag field
                if let Some(Value::Str(tag)) = map.get("_tag") {
                    if tag == name {
                        // Check arguments
                        if args.is_empty() {
                            // Constructor with no args (like None)
                            return true;
                        } else if args.len() == 1 {
                            // Constructor with one arg (like Some(x))
                            if let Some(inner) = map.get("_value") {
                                return match_pattern_impl(&args[0], inner, bindings);
                            }
                        }
                    }
                }
            }
            false
        }

        Pattern::Array(patterns) => {
            if let Value::Array(values) = value {
                if patterns.len() != values.len() {
                    return false;
                }
                for (pat, val) in patterns.iter().zip(values.iter()) {
                    if !match_pattern_impl(pat, val, bindings) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }

        Pattern::Record(field_patterns) => {
            if let Value::Record(map) = value {
                for (field_name, field_pattern) in field_patterns {
                    if let Some(field_value) = map.get(field_name) {
                        if !match_pattern_impl(field_pattern, field_value, bindings) {
                            return false;
                        }
                    } else {
                        return false; // Required field missing
                    }
                }
                true
            } else {
                false
            }
        }
    }
}

/* ---------------- dispatch helpers ---------------- */

/// Dispatch calling a Value `f` with optional pipe input and explicit args.
///
/// Rules:
/// - Lambda:
///     - if pipe input Some(Array) and arity==1 -> map over it
///     - if pipe input Some(v) and arity==1 and no explicit args -> call with v
///     - if arity matches explicit args (plus maybe pipe) -> call
/// - String/Uri → builtin with (pipe_input ++ args)
/// - Null → error
/// - Other → error
fn call_value_with_pipe(
    f: Value,
    pin: Option<Value>,
    mut args: Vec<Value>,
    env: &mut Env,
) -> Result<Value> {
    match f {
        Value::Lambda(l) => {
            match (pin, l.params.len(), args.len()) {
                // Zero-arg lambda: no params, no input required, no explicit args
                (_, 0, 0) => call_lambda0(&l, env),
                // Map: pin is array, lambda 1-ary, and no explicit args
                (Some(Value::Array(arr)), 1, 0) => {
                    let mut out = Vec::with_capacity(arr.len());
                    for (i, x) in arr.into_iter().enumerate() {
                        out.push(call_lambda1(&l, x, i, env)?);
                    }
                    Ok(Value::Array(out))
                }
                // Single-arg: use pipe input if provided and no explicit arg given
                (Some(v), 1, 0) => call_lambda1(&l, v, 0, env),
                // Two-arg: if we have two explicit args, call directly
                (_, 2, 2) if args.len() == 2 => {
                    // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
                    let b = args
                        .pop()
                        .ok_or_else(|| anyhow!("Expected second argument for lambda call"))?;
                    let a = args
                        .pop()
                        .ok_or_else(|| anyhow!("Expected first argument for lambda call"))?;
                    call_lambda2(&l, a, b, 0, env)
                }
                // One-arg: if we have one explicit arg, call directly
                (_, 1, 1) => {
                    // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
                    let arg = args
                        .pop()
                        .ok_or_else(|| anyhow!("Expected argument for lambda call"))?;
                    call_lambda1(&l, arg, 0, env)
                }
                // N-arg (3+): if we have exact arity match, call with all args
                (_, n, m) if n >= 3 && n == m => call_lambda_n(&l, args, env),
                _ => Err(anyhow!("lambda arity mismatch or missing input")),
            }
        }
        // Async lambda: create a Future instead of executing immediately
        Value::AsyncLambda(al) => {
            // Prepend pipe input if present
            let all_args = if let Some(p) = pin {
                let mut all = Vec::with_capacity(1 + args.len());
                all.push(p);
                all.extend(args);
                all
            } else {
                args
            };
            Ok(Value::Future(Future {
                lambda: al,
                args: all_args,
            }))
        }
        // Builtin reference from module: call the builtin by name
        Value::Builtin(b) => builtins::call_with_input(&b.name, args, pin, env),
        Value::Str(name) | Value::Uri(name) => {
            if let Some(p) = pin {
                let mut all = Vec::with_capacity(1 + args.len());
                all.push(p);
                all.extend(args);
                builtins::call(&name, all, env)
            } else {
                builtins::call(&name, args, env)
            }
        }
        Value::Null => Err(anyhow!("cannot call null")),
        other => Err(anyhow!("cannot call non-function value: {:?}", other)),
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        Value::Float(f) => *f != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::Uri(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Record(m) => !m.is_empty(),
        Value::Table(t) => !t.rows.is_empty(),
        Value::Lambda(_) => true,
        Value::AsyncLambda(_) => true,
        Value::Future(_) => true,
        Value::Error(_) => false,  // Errors are falsy
        Value::Builtin(_) => true, // Builtins are truthy
    }
}

/// Call a zero-parameter lambda
fn call_lambda0(l: &Lambda, env: &mut Env) -> Result<Value> {
    // Save and clear pipe input to prevent leakage
    let saved_pipe = env.input().cloned();
    env.set_input(None);

    let out = eval_expr(&l.body, env);

    // Restore pipe input
    match saved_pipe {
        Some(v) => env.set_input(Some(v)),
        None => env.set_input(None),
    }

    out
}

fn call_lambda1(l: &Lambda, x: Value, i: usize, env: &mut Env) -> Result<Value> {
    let p = l
        .params
        .get(0)
        .ok_or_else(|| anyhow!("lambda needs 1 param"))?
        .clone();

    // Save and clear pipe input to prevent leakage
    let saved_pipe = env.input().cloned();
    env.set_input(None);

    // Save prev bindings
    let old_p = env.get_var(&p).cloned();
    env.set_var_unchecked(&p, x);

    // Optional `i`
    let mut old_i: Option<Value> = None;
    if let Some(ip) = l.params.get(1) {
        if ip == "i" {
            old_i = env.get_var("i").cloned();
            env.set_var_unchecked("i", Value::Int(i as i64));
        }
    }

    let out = eval_expr(&l.body, env);

    // Debug: show result of evaluating the lambda body
    // Removed debug eprintln statements

    // Restore
    if let Some(v) = old_i {
        env.set_var_unchecked("i", v);
    } else if l.params.get(1).map(|s| s.as_str()) == Some("i") {
        env.del_var("i");
    }
    if let Some(v) = old_p {
        env.set_var_unchecked(&p, v);
    } else {
        env.del_var(&p);
    }

    // Restore pipe input
    match saved_pipe {
        Some(v) => env.set_input(Some(v)),
        None => env.set_input(None),
    }

    out
}

/// Call a lambda with N arguments (generic version for 3+ args)
fn call_lambda_n(l: &Lambda, args: Vec<Value>, env: &mut Env) -> Result<Value> {
    if args.len() != l.params.len() {
        return Err(anyhow!(
            "lambda expects {} arguments, got {}",
            l.params.len(),
            args.len()
        ));
    }

    // Save and clear pipe input to prevent leakage
    let saved_pipe = env.input().cloned();
    env.set_input(None);

    // Save old bindings and set new ones
    let mut old_bindings: Vec<(String, Option<Value>)> = Vec::with_capacity(l.params.len());
    for (param, arg) in l.params.iter().zip(args.into_iter()) {
        old_bindings.push((param.clone(), env.get_var(param).cloned()));
        env.set_var_unchecked(param, arg);
    }

    let out = eval_expr(&l.body, env);

    // Restore all bindings
    for (param, old_val) in old_bindings.into_iter().rev() {
        if let Some(v) = old_val {
            env.set_var_unchecked(&param, v);
        } else {
            env.del_var(&param);
        }
    }

    // Restore pipe input
    match saved_pipe {
        Some(v) => env.set_input(Some(v)),
        None => env.set_input(None),
    }

    out
}

fn call_lambda2(l: &Lambda, a: Value, b: Value, i: usize, env: &mut Env) -> Result<Value> {
    let p1 = l
        .params
        .get(0)
        .ok_or_else(|| anyhow!("lambda needs 2 params"))?
        .clone();
    let p2 = l
        .params
        .get(1)
        .ok_or_else(|| anyhow!("lambda needs 2 params"))?
        .clone();

    // Save and clear pipe input to prevent leakage
    let saved_pipe = env.input().cloned();
    env.set_input(None);

    let old_p1 = env.get_var(&p1).cloned();
    env.set_var_unchecked(&p1, a);
    let old_p2 = env.get_var(&p2).cloned();
    env.set_var_unchecked(&p2, b);

    let mut old_i: Option<Value> = None;
    if let Some(ip) = l.params.get(2) {
        if ip == "i" {
            old_i = env.get_var("i").cloned();
            env.set_var_unchecked("i", Value::Int(i as i64));
        }
    }

    let out = eval_expr(&l.body, env);

    if let Some(v) = old_i {
        env.set_var_unchecked("i", v);
    } else if l.params.get(2).map(|s| s.as_str()) == Some("i") {
        env.del_var("i");
    }

    if let Some(v) = old_p2 {
        env.set_var_unchecked(&p2, v);
    } else {
        env.del_var(&p2);
    }
    if let Some(v) = old_p1 {
        env.set_var_unchecked(&p1, v);
    } else {
        env.del_var(&p1);
    }

    // Restore pipe input
    match saved_pipe {
        Some(v) => env.set_input(Some(v)),
        None => env.set_input(None),
    }

    out
}

/* ---------------- binops & eq ---------------- */

fn value_eq(a: &Value, b: &Value) -> bool {
    use Value::*;
    match (a, b) {
        (Null, Null) => true,
        (Bool(x), Bool(y)) => x == y,
        (Int(x), Int(y)) => x == y,
        (Float(x), Float(y)) => x == y,
        (Int(x), Float(y)) => (*x as f64) == *y,
        (Float(x), Int(y)) => *x == (*y as f64),
        (Str(x), Str(y)) => x == y,
        (Uri(x), Uri(y)) => x == y,
        (Array(ax), Array(ay)) => {
            ax.len() == ay.len() && ax.iter().zip(ay).all(|(x, y)| value_eq(x, y))
        }
        (Record(rx), Record(ry)) => {
            rx.len() == ry.len()
                && rx
                    .iter()
                    .all(|(k, vx)| ry.get(k).map_or(false, |vy| value_eq(vx, vy)))
        }
        _ => false,
    }
}

fn binop(op: &BinOp, a: Value, b: Value) -> Result<Value> {
    use BinOp::*;
    Ok(match (op, a, b) {
        (Add, Value::Int(x), Value::Int(y)) => Value::Int(x + y),
        (Add, Value::Float(x), Value::Float(y)) => Value::Float(x + y),
        (Add, Value::Int(x), Value::Float(y)) => Value::Float((x as f64) + y),
        (Add, Value::Float(x), Value::Int(y)) => Value::Float(x + (y as f64)),
        // String concatenation
        (Add, Value::Str(x), Value::Str(y)) => Value::Str(format!("{}{}", x, y)),
        (Add, Value::Str(x), Value::Int(y)) => Value::Str(format!("{}{}", x, y)),
        (Add, Value::Str(x), Value::Float(y)) => Value::Str(format!("{}{}", x, y)),
        (Add, Value::Int(x), Value::Str(y)) => Value::Str(format!("{}{}", x, y)),
        (Add, Value::Float(x), Value::Str(y)) => Value::Str(format!("{}{}", x, y)),

        (Sub, Value::Int(x), Value::Int(y)) => Value::Int(x - y),
        (Sub, Value::Float(x), Value::Float(y)) => Value::Float(x - y),
        (Sub, Value::Int(x), Value::Float(y)) => Value::Float((x as f64) - y),
        (Sub, Value::Float(x), Value::Int(y)) => Value::Float(x - (y as f64)),

        (Mul, Value::Int(x), Value::Int(y)) => Value::Int(x * y),
        (Mul, Value::Float(x), Value::Float(y)) => Value::Float(x * y),
        (Mul, Value::Int(x), Value::Float(y)) => Value::Float((x as f64) * y),
        (Mul, Value::Float(x), Value::Int(y)) => Value::Float(x * (y as f64)),

        (Div, Value::Int(x), Value::Int(y)) => Value::Float((x as f64) / (y as f64)),
        (Div, Value::Float(x), Value::Float(y)) => Value::Float(x / y),
        (Div, Value::Int(x), Value::Float(y)) => Value::Float((x as f64) / y),
        (Div, Value::Float(x), Value::Int(y)) => Value::Float(x / (y as f64)),

        (Rem, Value::Int(x), Value::Int(y)) => Value::Int(x % y),

        (Eq, x, y) => Value::Bool(value_eq(&x, &y)),
        (Ne, x, y) => Value::Bool(!value_eq(&x, &y)),

        (Lt, Value::Int(x), Value::Int(y)) => Value::Bool(x < y),
        (Lt, Value::Float(x), Value::Float(y)) => Value::Bool(x < y),
        (Lte, Value::Int(x), Value::Int(y)) => Value::Bool(x <= y),
        (Lte, Value::Float(x), Value::Float(y)) => Value::Bool(x <= y),
        (Gt, Value::Int(x), Value::Int(y)) => Value::Bool(x > y),
        (Gt, Value::Float(x), Value::Float(y)) => Value::Bool(x > y),
        (Gte, Value::Int(x), Value::Int(y)) => Value::Bool(x >= y),
        (Gte, Value::Float(x), Value::Float(y)) => Value::Bool(x >= y),

        (And, x, y) => Value::Bool(is_truthy(&x) && is_truthy(&y)),
        (Or, x, y) => Value::Bool(is_truthy(&x) || is_truthy(&y)),

        // Power: numeric exponentiation. Promote to float when necessary.
        (Pow, Value::Int(x), Value::Int(y)) => {
            if y >= 0 {
                // use integer pow for non-negative integer exponents
                Value::Int(x.pow(y as u32))
            } else {
                // negative exponent -> float result
                Value::Float((x as f64).powf(y as f64))
            }
        }
        (Pow, Value::Float(x), Value::Float(y)) => Value::Float(x.powf(y)),
        (Pow, Value::Int(x), Value::Float(y)) => Value::Float((x as f64).powf(y)),
        (Pow, Value::Float(x), Value::Int(y)) => Value::Float(x.powf(y as f64)),

        (op, a, b) => return Err(anyhow!("unsupported op {:?} on {:?} and {:?}", op, a, b)),
    })
}

/// Interpolate ${expr} patterns in a string
fn interpolate_string(s: &str, env: &mut Env) -> Result<Value> {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'

            // Find the closing '}'
            let mut expr_str = String::new();
            let mut depth = 1;
            while let Some(ch) = chars.next() {
                if ch == '{' {
                    depth += 1;
                    expr_str.push(ch);
                } else if ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    expr_str.push(ch);
                } else {
                    expr_str.push(ch);
                }
            }

            // Parse and evaluate the expression
            match crate::parser::parse_program(&expr_str) {
                Ok(stmts) if !stmts.is_empty() => {
                    if let crate::ast::Stmt::Expr(expr) = &stmts[0] {
                        match eval_expr(expr, env) {
                            Ok(val) => {
                                // Convert value to string representation
                                result.push_str(&match val {
                                    Value::Str(s) => s,
                                    Value::Int(n) => n.to_string(),
                                    Value::Float(f) => f.to_string(),
                                    Value::Bool(b) => b.to_string(),
                                    Value::Null => "null".to_string(),
                                    other => format!("{:?}", other),
                                });
                            }
                            Err(e) => {
                                // On error, keep the ${expr} literal
                                result.push_str(&format!("${{{}}} [error: {}]", expr_str, e));
                            }
                        }
                    } else {
                        result.push_str(&format!("${{{}}}", expr_str));
                    }
                }
                _ => {
                    // Parse failed, keep literal
                    result.push_str(&format!("${{{}}}", expr_str));
                }
            }
        } else {
            result.push(ch);
        }
    }

    Ok(Value::Str(result))
}
