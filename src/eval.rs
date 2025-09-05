use crate::ast::{BinOp, Expr, Stmt, UnOp};
use crate::builtins;
use crate::env::Env;
use crate::value::{Lambda, Value};
use anyhow::{Result, anyhow};
use std::collections::BTreeMap;

pub fn eval_program(stmts: &[Stmt], env: &mut Env) -> Result<Value> {
    let mut last = Value::Null;
    for s in stmts {
        last = eval_stmt(s, env)?;
    }
    Ok(last)
}

pub fn eval_stmt(stmt: &Stmt, env: &mut Env) -> Result<Value> {
    match stmt {
        Stmt::Let { name, value, .. } => {
            let v = eval_expr(value, env)?;
            env.set_var(name, v.clone());
            Ok(v)
        }
        Stmt::Expr(e) => eval_expr(e, env),
    }
}

pub fn eval_expr(expr: &Expr, env: &mut Env) -> Result<Value> {
    match expr {
        // ---------- literals ----------
        Expr::LitInt(n) => Ok(Value::Int(*n)),
        Expr::LitFloat(f) => Ok(Value::Float(*f)),
        Expr::LitStr(s) => Ok(Value::Str(s.clone())),
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
                    // If the bound var is a Lambda, call it. Otherwise fall back
                    // to builtin behavior (e.g. when shadowing with a non-function).
                    match v {
                        Value::Lambda(_) => return call_value_with_pipe(v, pin, vals, env),
                        _ => {
                            let mut all = Vec::new();
                            if let Some(p) = pin {
                                all.push(p);
                            }
                            all.extend(vals);
                            return builtins::call(name, all, env);
                        }
                    }
                } else {
                    // Not a bound var → treat as builtin name
                    let mut all = Vec::new();
                    if let Some(p) = pin {
                        all.push(p);
                    }
                    all.extend(vals);
                    return builtins::call(name, all, env);
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
                        Value::Lambda(_) => {
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
                    let b = args.pop().unwrap();
                    let a = args.pop().unwrap();
                    call_lambda2(&l, a, b, 0, env)
                }
                // One-arg: if we have one explicit arg, call directly
                (_, 1, 1) => call_lambda1(&l, args.pop().unwrap(), 0, env),
                _ => Err(anyhow!("lambda arity mismatch or missing input")),
            }
        }
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
    }
}

fn call_lambda1(l: &Lambda, x: Value, i: usize, env: &mut Env) -> Result<Value> {
    let p = l
        .params
        .get(0)
        .ok_or_else(|| anyhow!("lambda needs 1 param"))?
        .clone();

    // Save prev bindings
    let old_p = env.get_var(&p).cloned();
    env.set_var(&p, x);

    // Optional `i`
    let mut old_i: Option<Value> = None;
    if let Some(ip) = l.params.get(1) {
        if ip == "i" {
            old_i = env.get_var("i").cloned();
            env.set_var("i", Value::Int(i as i64));
        }
    }

    let out = eval_expr(&l.body, env);

    // Debug: show result of evaluating the lambda body
    // Removed debug eprintln statements

    // Restore
    if let Some(v) = old_i {
        env.set_var("i", v);
    } else if l.params.get(1).map(|s| s.as_str()) == Some("i") {
        env.del_var("i");
    }
    if let Some(v) = old_p {
        env.set_var(&p, v);
    } else {
        env.del_var(&p);
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

    let old_p1 = env.get_var(&p1).cloned();
    env.set_var(&p1, a);
    let old_p2 = env.get_var(&p2).cloned();
    env.set_var(&p2, b);

    let mut old_i: Option<Value> = None;
    if let Some(ip) = l.params.get(2) {
        if ip == "i" {
            old_i = env.get_var("i").cloned();
            env.set_var("i", Value::Int(i as i64));
        }
    }

    let out = eval_expr(&l.body, env);

    if let Some(v) = old_i {
        env.set_var("i", v);
    } else if l.params.get(2).map(|s| s.as_str()) == Some("i") {
        env.del_var("i");
    }

    if let Some(v) = old_p2 {
        env.set_var(&p2, v);
    } else {
        env.del_var(&p2);
    }
    if let Some(v) = old_p1 {
        env.set_var(&p1, v);
    } else {
        env.del_var(&p1);
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
