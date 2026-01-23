use std::collections::BTreeMap;
use std::fmt;

use crate::ast::{BinOp, Expr, Stmt, UnOp};
use crate::types::Type;

pub type TypeEnv = BTreeMap<String, Type>;

/* =========================
Errors
========================= */

#[derive(Debug)]
pub enum TypeError {
    UnboundIdent(String),
    Mismatch { expected: Type, found: Type },
    BadBinary { op: BinOp, left: Type, right: Type },
    BadUnary { op: UnOp, inner: Type },
    NotCallable(Type),
    Other(String),
}

impl std::error::Error for TypeError {}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeError::UnboundIdent(name) => write!(f, "unbound identifier `{}`", name),
            TypeError::Mismatch { expected, found } => write!(
                f,
                "type mismatch: expected {:?}, found {:?}",
                expected, found
            ),
            TypeError::BadBinary { op, left, right } => write!(
                f,
                "invalid operand types for operator {:?}: {:?} and {:?}",
                op, left, right
            ),
            TypeError::BadUnary { op, inner } => write!(
                f,
                "invalid operand type for unary operator {:?}: {:?}",
                op, inner
            ),
            TypeError::NotCallable(t) => write!(f, "not a function: {:?}", t),
            TypeError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

/* =========================
Public entry points
========================= */

pub fn typecheck(stmts: &[Stmt]) -> Result<TypeEnv, TypeError> {
    let mut env = TypeEnv::new();
    for s in stmts {
        type_of_stmt(s, &mut env)?;
    }
    Ok(env)
}

pub fn type_of_stmt(stmt: &Stmt, env: &mut TypeEnv) -> Result<(), TypeError> {
    match stmt {
        Stmt::Let { name, value, .. } => {
            let t = type_of_expr(value, env)?;
            env.insert(name.clone(), t);
            Ok(())
        }
        Stmt::Expr(e) => {
            let _ = type_of_expr(e, env)?;
            Ok(())
        }
        Stmt::Import { .. } => {
            // Import statements don't contribute to type checking in this pass
            // The imported module's types would be resolved at runtime
            Ok(())
        }
        Stmt::Export { .. } => {
            // Export statements don't add new types, they just mark existing items for export
            Ok(())
        }
        Stmt::Cfg { body, .. } => {
            // Type check the body statement - the condition is evaluated at runtime
            type_of_stmt(body, env)
        }
    }
}

pub fn type_of_expr(expr: &Expr, env: &mut TypeEnv) -> Result<Type, TypeError> {
    match expr {
        // --- literals ---
        Expr::LitInt(_) => Ok(Type::Int),
        Expr::LitFloat(_) => Ok(Type::Float),
        Expr::LitStr(s) => {
            if looks_like_uri(s) {
                Ok(Type::Uri)
            } else {
                Ok(Type::String)
            }
        }
        Expr::LitBool(_) => Ok(Type::Bool),
        Expr::Null => Ok(Type::Null),

        // --- variables ---
        Expr::Ident(name) => env
            .get(name)
            .cloned()
            .ok_or_else(|| TypeError::UnboundIdent(name.clone())),

        // --- collections ---
        Expr::Array(elems) => {
            if elems.is_empty() {
                Ok(Type::Array(Box::new(Type::Any)))
            } else {
                let mut acc = type_of_expr(&elems[0], env)?;
                for e in &elems[1..] {
                    let t = type_of_expr(e, env)?;
                    acc = promote_array_elem(acc, t);
                }
                Ok(Type::Array(Box::new(acc)))
            }
        }
        Expr::Record(fields) => {
            let mut map = BTreeMap::new();
            for (k, v) in fields {
                let t = type_of_expr(v, env)?;
                map.insert(k.clone(), t);
            }
            Ok(Type::Record(map))
        }

        // --- lambda ---
        Expr::Lambda { params, body } => {
            // Parameters default to `Any` in the checker.
            let mut local = env.clone();
            let param_types = vec![Type::Any; params.len()];
            for p in params {
                local.insert(p.clone(), Type::Any);
            }
            let ret = type_of_expr(body, &mut local)?;
            Ok(Type::Lambda(param_types, Box::new(ret)))
        }

        // --- call ---
        Expr::Call {
            callee,
            args,
            named,
        } => {
            // If `callee` is an identifier for a known builtin, handle it first.
            if let Expr::Ident(name) = &**callee {
                if is_builtin(name) {
                    let mut arg_tys = Vec::with_capacity(args.len());
                    for a in args {
                        arg_tys.push(type_of_expr(a, env)?);
                    }
                    let named_map = typed_named_args(named, env)?;
                    return type_builtin_call(name, &arg_tys, &named_map);
                }
                // otherwise fall through to treat it as a variable holding a lambda
            }

            // Otherwise, infer callee type and try function application.
            let callee_ty = type_of_expr(callee, env)?;
            let mut arg_tys = Vec::with_capacity(args.len());
            for a in args {
                arg_tys.push(type_of_expr(a, env)?);
            }
            let _named_map = typed_named_args(named, env)?;

            match callee_ty {
                Type::Lambda(params, ret) => {
                    // Be forgiving on arity: unify what we can, still return `ret`.
                    let upto = params.len().min(arg_tys.len());
                    for i in 0..upto {
                        let _ = unify(params[i].clone(), arg_tys[i].clone());
                    }
                    Ok(*ret)
                }
                Type::Any => Ok(Type::Any),
                other => Err(TypeError::NotCallable(other)),
            }
        }

        // --- pipeline ---
        Expr::Pipe { left, right } => {
            let left_t = type_of_expr(left, env)?;
            if let Expr::Call {
                callee,
                args,
                named,
            } = &**right
            {
                // compute args with LHS as the first (piped) arg
                let mut arg_tys = Vec::new();
                arg_tys.push(left_t.clone());
                for a in args {
                    arg_tys.push(type_of_expr(a, env)?);
                }
                let named_map = typed_named_args(named, env)?;
                if let Expr::Ident(name) = &**callee {
                    if is_builtin(name) {
                        // Builtins in pipelines: precise typing
                        return type_builtin_call(name, &arg_tys, &named_map);
                    }
                }
                // If it's a lambda application, prefer the lambda's return.
                let callee_ty = type_of_expr(callee, env)?;
                match callee_ty {
                    Type::Lambda(_params, ret) => Ok(*ret),
                    Type::Any => Ok(Type::Any),
                    _ => Ok(Type::Any),
                }
            } else {
                Ok(Type::Any)
            }
        }

        // --- unary ---
        Expr::Unary { op, expr } => {
            let t = type_of_expr(expr, env)?;
            match op {
                UnOp::Neg => match t {
                    Type::Int | Type::Float => Ok(t),
                    Type::Any => Ok(Type::Any),
                    other => Err(TypeError::BadUnary {
                        op: *op,
                        inner: other,
                    }),
                },
                UnOp::Not => match t {
                    Type::Bool | Type::Any => Ok(Type::Bool),
                    other => Err(TypeError::BadUnary {
                        op: *op,
                        inner: other,
                    }),
                },
            }
        }

        // --- binary ---
        Expr::Binary { left, op, right } => {
            let lt = type_of_expr(left, env)?;
            let rt = type_of_expr(right, env)?;
            type_binary(op, lt, rt)
        }

        // --- member access: record.field ---
        Expr::MemberAccess { object, field: _ } => {
            let obj_type = type_of_expr(object, env)?;
            // For now, if the object is a Record type, we return Any
            // since we don't track field types in our simple type system
            match obj_type {
                Type::Record(_) => Ok(Type::Any),
                Type::Any => Ok(Type::Any), // Allow access on Any type
                other => Err(TypeError::Other(format!(
                    "cannot access field on non-record type: {:?}",
                    other
                ))),
            }
        }

        // --- pattern matching ---
        Expr::Match { scrutinee, arms } => {
            // Type check the scrutinee
            let _scrutinee_type = type_of_expr(scrutinee, env)?;

            // For simplicity, check all arms and return the type of the first arm's body
            // In a more sophisticated system, we'd verify all arms return the same type
            if arms.is_empty() {
                return Err(TypeError::Other("match expression has no arms".to_string()));
            }

            // Type check each arm's body (we don't deeply validate patterns yet)
            let mut arm_type = None;
            for arm in arms {
                // Create a temporary environment for pattern bindings
                // (we don't track exact pattern types, so we just check the body)
                let body_type = type_of_expr(&arm.body, env)?;

                if let Some(ref expected) = arm_type {
                    // In a real system, we'd verify all arms have compatible types
                    // For now, just use the first arm's type
                    if body_type != *expected && body_type != Type::Any && *expected != Type::Any {
                        // Allow Any to match anything
                    }
                } else {
                    arm_type = Some(body_type);
                }
            }

            Ok(arm_type.unwrap_or(Type::Any))
        }

        // --- async lambda ---
        Expr::AsyncLambda { params, body } => {
            // Parameters default to `Any` in the checker.
            let mut local = env.clone();
            let param_types = vec![Type::Any; params.len()];
            for p in params {
                local.insert(p.clone(), Type::Any);
            }
            let ret = type_of_expr(body, &mut local)?;
            // AsyncLambda returns a Future type (which we model as Any for now)
            Ok(Type::Lambda(param_types, Box::new(Type::Any)))
        }

        // --- await ---
        Expr::Await(inner) => {
            // Await unwraps a Future, but since we model futures as Any,
            // the result is also Any
            let _inner_type = type_of_expr(inner, env)?;
            Ok(Type::Any)
        }
    }
}

/* =========================
Helpers
========================= */

fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "map"
            | "reduce"
            | "where"
            | "select"
            | "group_by"
            | "agg"
            | "http_get"
            | "ls"
            | "list"
            | "pwd"
            | "cat"
            | "head"
            | "tail"
            | "find"
            | "sort"
            | "uniq"
            | "wc"
            | "grep"
            | "help"
            | "call"
            | "clear"
            | "echo"
            | "print"
            | "take"
            | "agent"
            | "swarm"
            // PowerShell-style cmdlets
            | "Get-Files" | "get-files"
            | "Get-Content" | "get-content"
            | "Select-Object" | "select-object"
            | "Where-Object" | "where-object"
            | "ForEach-Object" | "foreach-object" | "foreach"
            | "Sort-Object" | "sort-object"
            | "Group-Object" | "group-object" | "group"
            | "Measure-Object" | "measure-object" | "measure"
            // Nushell-style data commands
            | "from-json" | "from_json"
            | "to-json" | "to_json"
            | "from-csv" | "from_csv"
            | "to-csv" | "to_csv"
            | "from-yaml" | "from_yaml"
            | "to-yaml" | "to_yaml"
            | "columns"
            | "describe"
            // AI-enhanced commands
            | "ai-suggest" | "suggest"
            | "ai-explain" | "explain"
            | "ai-complete" | "complete"
            | "ai-fix" | "fix"
    )
}

fn typed_named_args(
    named: &[(String, Expr)],
    env: &mut TypeEnv,
) -> Result<BTreeMap<String, Type>, TypeError> {
    let mut m = BTreeMap::new();
    for (k, e) in named {
        m.insert(k.clone(), type_of_expr(e, env)?);
    }
    Ok(m)
}

fn promote_array_elem(a: Type, b: Type) -> Type {
    match (a, b) {
        (Type::Float, Type::Int) | (Type::Int, Type::Float) => Type::Float,
        (Type::Array(x), Type::Array(y)) => Type::Array(Box::new(unify(*x, *y))),
        (Type::Any, t) | (t, Type::Any) => t,
        (x, y) if x == y => x,
        (x, y) => unify(x, y),
    }
}

fn unify(a: Type, b: Type) -> Type {
    use Type::*;
    match (a, b) {
        (x, y) if x == y => x,
        (Any, t) | (t, Any) => t,
        (Int, Float) | (Float, Int) => Float,
        (Array(x), Array(y)) => Array(Box::new(unify(*x, *y))),
        (Record(mut rx), Record(ry)) => {
            let mut out = BTreeMap::new();
            for (k, vx) in rx.iter_mut() {
                if let Some(vy) = ry.get(k) {
                    out.insert(k.clone(), unify(vx.clone(), vy.clone()));
                } else {
                    out.insert(k.clone(), vx.clone());
                }
            }
            for (k, vy) in ry {
                out.entry(k).or_insert(vy);
            }
            Record(out)
        }
        (Table(mut sx), Table(sy)) => {
            let mut out = BTreeMap::new();
            for (k, vx) in sx.iter_mut() {
                if let Some(vy) = sy.get(k) {
                    out.insert(k.clone(), unify(vx.clone(), vy.clone()));
                } else {
                    out.insert(k.clone(), vx.clone());
                }
            }
            for (k, vy) in sy {
                out.entry(k).or_insert(vy);
            }
            Table(out)
        }
        _ => Any,
    }
}

fn type_binary(op: &BinOp, lt: Type, rt: Type) -> Result<Type, TypeError> {
    use BinOp::*;
    use Type::*;
    Ok(match op {
        Add | Sub | Mul | Div | Rem | BinOp::Pow => match (lt.clone(), rt.clone()) {
            (Int, Int) => Int,
            (Float, Int) | (Int, Float) | (Float, Float) => Float,
            (Any, t) | (t, Any) => t,
            _ => {
                return Err(TypeError::BadBinary {
                    op: *op,
                    left: lt,
                    right: rt,
                });
            }
        },
        Eq | Ne => Bool,
        Lt | Lte | Gt | Gte => match (lt.clone(), rt.clone()) {
            (Int, Int) | (Float, Float) | (Int, Float) | (Float, Int) | (String, String) => Bool,
            (Any, _) | (_, Any) => Bool,
            _ => {
                return Err(TypeError::BadBinary {
                    op: *op,
                    left: lt,
                    right: rt,
                });
            }
        },
        And | Or => match (lt.clone(), rt.clone()) {
            (Bool, Bool) | (Any, _) | (_, Any) => Bool,
            _ => {
                return Err(TypeError::BadBinary {
                    op: *op,
                    left: lt,
                    right: rt,
                });
            }
        },
    })
}

/* =========================
Builtin typing
========================= */

fn type_builtin_call(
    name: &str,
    args: &[Type],
    _named: &BTreeMap<String, Type>,
) -> Result<Type, TypeError> {
    match name {
        "map" => {
            // map(array<T>, fn(T)->U) -> array<U>
            if args.len() >= 2 {
                if let Type::Array(_elem) = &args[0] {
                    if let Type::Lambda(_params, ret) = &args[1] {
                        return Ok(Type::Array(ret.clone()));
                    }
                }
            }
            Ok(Type::Any)
        }
        "reduce" => {
            // reduce(array<T>, fn(Acc,T)->Acc, init:Acc) -> Acc
            if args.len() >= 3 {
                let acc_t = args[2].clone();
                return Ok(acc_t);
            }
            Ok(Type::Any)
        }
        "where" => {
            // where(array<T>, fn(T)->Bool) -> array<T>
            if let Some(Type::Array(t)) = args.get(0) {
                return Ok(Type::Array(t.clone()));
            }
            Ok(Type::Any)
        }
        "select" => {
            // select(table{S}, ...) -> table{S} (shape-preserving)
            if let Some(Type::Table(schema)) = args.get(0) {
                return Ok(Type::Table(schema.clone()));
            }
            Ok(Type::Any)
        }
        "group_by" => Ok(Type::Any),
        "agg" => Ok(Type::Any),
        "http_get" => {
            // http_get(url) -> { url:String, status:Int, headers:Record<String,String>, body:Any }
            let mut rec = BTreeMap::new();
            rec.insert("url".into(), Type::String);
            rec.insert("status".into(), Type::Int);
            let mut hdrs = BTreeMap::new();
            hdrs.insert("*".into(), Type::String); // schematic wildcard
            rec.insert("headers".into(), Type::Record(hdrs));
            rec.insert("body".into(), Type::Any);
            Ok(Type::Record(rec))
        }
        _ => Ok(Type::Any),
    }
}

/* =========================
URI detection
========================= */

fn looks_like_uri(s: &str) -> bool {
    if let Some(colon) = s.find(':') {
        if colon == 0 {
            return false;
        }
        let scheme = &s[..colon];
        let mut chars = scheme.chars();
        match chars.next() {
            Some(c) if c.is_ascii_alphabetic() => {}
            _ => return false,
        }
        for ch in chars {
            if !(ch.is_ascii_alphabetic()
                || ch.is_ascii_digit()
                || ch == '+'
                || ch == '-'
                || ch == '.')
            {
                return false;
            }
        }
        true
    } else {
        false
    }
}

/* =========================
Anyhow-friendly helpers
========================= */

/// Parse + typecheck a full source string; returns the type environment.
pub fn typecheck_program(src: &str) -> anyhow::Result<TypeEnv> {
    let stmts = crate::parser::parse_program(src)?;
    typecheck(&stmts).map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// Parse a source string and return the **type of the last expression**.
pub fn infer_last_type(src: &str) -> anyhow::Result<Type> {
    let stmts = crate::parser::parse_program(src)?;
    let mut env = TypeEnv::new();
    let mut last_ty = Type::Null;
    for s in &stmts {
        match s {
            Stmt::Let { name, value, .. } => {
                let t =
                    type_of_expr(value, &mut env).map_err(|e| anyhow::anyhow!(e.to_string()))?;
                env.insert(name.clone(), t.clone());
                last_ty = t;
            }
            Stmt::Expr(e) => {
                last_ty = type_of_expr(e, &mut env).map_err(|e| anyhow::anyhow!(e.to_string()))?;
            }
            Stmt::Import { .. } | Stmt::Export { .. } => {
                // Import/Export statements don't affect the type of the last expression
                // Keep last_ty unchanged
            }
            Stmt::Cfg { body, .. } => {
                // Type check the cfg body recursively
                match body.as_ref() {
                    Stmt::Let { name, value, .. } => {
                        let t = type_of_expr(value, &mut env)
                            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                        env.insert(name.clone(), t.clone());
                        last_ty = t;
                    }
                    Stmt::Expr(e) => {
                        last_ty = type_of_expr(e, &mut env)
                            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                    }
                    _ => {
                        // Recursively handle other statement types
                    }
                }
            }
        }
    }
    Ok(last_ty)
}

/// If you already have an AST and want the env with anyhow errors.
pub fn typecheck_stmts_anyhow(stmts: &[crate::ast::Stmt]) -> anyhow::Result<TypeEnv> {
    typecheck(stmts).map_err(|e| anyhow::anyhow!(e.to_string()))
}
