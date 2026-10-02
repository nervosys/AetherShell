//! Pure, bounded AetherShell source inspection. Never evaluate analyzed code.
use crate::{
    ast::{Expr, Stmt, Visibility},
    parser::{parse_program, source_identifiers, SourceIdentifier},
    safety, signature,
    value::Value,
};
use anyhow::Result;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

fn canonical_builtin(name: &str) -> Option<&str> {
    if crate::builtins::is_dispatched(name) {
        return Some(name);
    }
    static MODULES: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    MODULES
        .get_or_init(|| {
            let mut aliases = BTreeMap::new();
            for (module, value) in crate::modules::all_modules() {
                if let Value::Record(fields) = value {
                    for (field, value) in fields {
                        if let Value::Builtin(builtin) = value {
                            aliases.insert(format!("{module}.{field}"), builtin.name);
                        }
                    }
                }
            }
            aliases
        })
        .get(name)
        .map(String::as_str)
}
fn parse_failure(name: &str, error: anyhow::Error) -> anyhow::Error {
    if error.downcast_ref::<safety::SafetyError>().is_some() {
        return error;
    }
    let message = error
        .downcast_ref::<crate::parser::ParseError>()
        .map(|e| e.message.clone())
        .unwrap_or_else(|| error.to_string());
    safety::arg_err(format!("{name}: invalid AetherShell source: {message}"))
}

fn record(fields: Vec<(&str, Value)>) -> Value {
    Value::Record(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn strings(items: impl IntoIterator<Item = String>) -> Value {
    Value::Array(items.into_iter().map(Value::Str).collect())
}
fn string<'a>(name: &str, value: &'a Value) -> Result<&'a str> {
    match value {
        Value::Str(s) => Ok(s),
        _ => Err(safety::arg_err(format!("{name}: expected String"))),
    }
}
fn symbol(name: &str, value: &Value) -> Result<String> {
    let s = string(name, value)?;
    if name == "code_signature" && s.len() <= 256 && canonical_builtin(s).is_some() {
        return Ok(s.into());
    }
    if s.len() > 256
        || s.is_empty()
        || !s.split('.').all(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
                && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        })
    {
        return Err(safety::arg_err(format!(
            "{name}: expected an identifier or dotted identifier, at most 256 bytes"
        )));
    }
    Ok(s.into())
}
struct Document {
    statements: Vec<Stmt>,
    identifiers: Vec<SourceIdentifier>,
}
impl Document {
    fn parse(name: &str, source: &str) -> Result<Self> {
        if source.len() > 65536 {
            return Err(safety::budget_exceeded(
                name,
                "source exceeds 64 KiB".into(),
                "analyze a smaller source unit",
            ));
        }
        if source.bytes().filter(u8::is_ascii_punctuation).count() > 256 {
            return Err(safety::budget_exceeded(name, "source exceeds 256 raw ASCII punctuation characters".into(), "analyze a smaller source unit; this conservative complexity bound includes comments and strings"));
        }
        let identifiers = source_identifiers(name, source).map_err(|e| parse_failure(name, e))?;
        let statements = parse_program(source).map_err(|e| parse_failure(name, e))?;
        Ok(Self {
            statements,
            identifiers,
        })
    }
    fn bindings(&self) -> Vec<(&str, &Expr, bool, Visibility)> {
        fn collect<'a>(stmt: &'a Stmt, out: &mut Vec<(&'a str, &'a Expr, bool, Visibility)>) {
            match stmt {
                Stmt::Let {
                    name,
                    value,
                    is_mut,
                    visibility,
                } => out.push((name, value, *is_mut, *visibility)),
                Stmt::Cfg { body, .. } => collect(body, out),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for stmt in &self.statements {
            collect(stmt, &mut out);
        }
        out
    }
    fn definitions(&self, name: &str) -> Value {
        Value::Array(
            self.bindings()
                .into_iter()
                .filter(|(n, ..)| *n == name)
                .map(|(n, value, is_mut, visibility)| {
                    let (kind, params) = match value {
                        Expr::Lambda { params, .. } => ("lambda", params.clone()),
                        Expr::AsyncLambda { params, .. } => ("async_lambda", params.clone()),
                        _ => ("variable", Vec::new()),
                    };
                    record(vec![
                        ("name", Value::Str(n.into())),
                        ("kind", Value::Str(kind.into())),
                        ("mutable", Value::Bool(is_mut)),
                        ("public", Value::Bool(visibility == Visibility::Pub)),
                        ("params", strings(params)),
                        (
                            "value_ast",
                            Value::from_json(
                                &serde_json::to_value(value).expect("serializable AST"),
                            ),
                        ),
                    ])
                })
                .collect(),
        )
    }
    fn signature(&self, name: &str) -> Value {
        let bindings = self
            .bindings()
            .into_iter()
            .filter(|(n, ..)| *n == name)
            .collect::<Vec<_>>();
        if !bindings.is_empty() {
            let signatures = bindings
                .into_iter()
                .filter_map(|(_, value, ..)| match value {
                    Expr::Lambda { params, .. } | Expr::AsyncLambda { params, .. } => {
                        Some(format!("{name}({}) -> Any", params.join(", ")))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            return record(vec![
                ("name", Value::Str(name.into())),
                ("found", Value::Bool(!signatures.is_empty())),
                ("origin", Value::Str("source".into())),
                ("signatures", strings(signatures)),
                ("documentation", Value::Str(String::new())),
            ]);
        }
        if let Some(sig) = canonical_builtin(name).and_then(signature::signature_of) {
            record(vec![
                ("name", Value::Str(name.into())),
                ("found", Value::Bool(true)),
                ("origin", Value::Str("builtin".into())),
                ("signatures", strings([sig.render()])),
                ("documentation", Value::Str(sig.doc.into())),
            ])
        } else {
            record(vec![
                ("name", Value::Str(name.into())),
                ("found", Value::Bool(false)),
                ("origin", Value::Str("unresolved".into())),
                ("signatures", Value::Array(Vec::new())),
                ("documentation", Value::Str(String::new())),
            ])
        }
    }
}
fn call_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(s) => Some(s.clone()),
        Expr::MemberAccess { object, field } => call_name(object).map(|s| format!("{s}.{field}")),
        _ => None,
    }
}
fn calls(expr: &Expr, names: &mut BTreeSet<String>, dynamic: &mut usize) {
    match expr {
        Expr::Call {
            callee,
            args,
            named,
        } => {
            match call_name(callee) {
                Some(name) => {
                    names.insert(name);
                }
                None => *dynamic += 1,
            }
            calls(callee, names, dynamic);
            for arg in args {
                calls(arg, names, dynamic);
            }
            for (_, arg) in named {
                calls(arg, names, dynamic);
            }
        }
        Expr::Pipe { left, right } => {
            calls(left, names, dynamic);
            if let Expr::Ident(name) = right.as_ref() {
                names.insert(name.clone());
            } else if matches!(right.as_ref(), Expr::MemberAccess { .. }) {
                if let Some(name) = call_name(right) {
                    if canonical_builtin(&name).is_some() {
                        names.insert(name);
                    }
                }
                calls(right, names, dynamic);
            } else {
                calls(right, names, dynamic);
            }
        }
        Expr::Array(items) => {
            for item in items {
                calls(item, names, dynamic);
            }
        }
        Expr::Record(items) => {
            for (_, item) in items {
                calls(item, names, dynamic);
            }
        }
        Expr::Lambda { body, .. } | Expr::AsyncLambda { body, .. } => calls(body, names, dynamic),
        Expr::Await(expr) | Expr::Throw(expr) | Expr::Unary { expr, .. } => {
            calls(expr, names, dynamic)
        }
        Expr::TryCatch {
            try_expr,
            catch_expr,
            ..
        } => {
            calls(try_expr, names, dynamic);
            calls(catch_expr, names, dynamic);
        }
        Expr::Binary { left, right, .. } => {
            calls(left, names, dynamic);
            calls(right, names, dynamic);
        }
        Expr::MemberAccess { object, .. } => calls(object, names, dynamic),
        Expr::Match { scrutinee, arms } => {
            calls(scrutinee, names, dynamic);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    calls(guard, names, dynamic);
                }
                calls(&arm.body, names, dynamic);
            }
        }
        Expr::LitInt(_)
        | Expr::LitFloat(_)
        | Expr::LitStr(_)
        | Expr::LitBool(_)
        | Expr::Null
        | Expr::Ident(_) => {}
    }
}
fn position(name: &str, value: &Value) -> Result<usize> {
    match value {
        Value::Int(n) if *n > 0 && *n <= 65537 => Ok(*n as usize),
        _ => Err(safety::arg_err(format!(
            "{name}: line and column must be positive Ints at most 65537"
        ))),
    }
}
pub(super) fn analyze(name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (min, max) = if name == "code_hover" {
        (3, 3)
    } else if name == "code_completions" {
        (1, 2)
    } else {
        (2, 2)
    };
    if input.is_some() || !(min..=max).contains(&args.len()) {
        return Err(safety::arg_err(format!(
            "{name}: wrong argument count or unexpected pipeline input"
        )));
    }
    let source = string(name, &args[0])?;
    let requested = if name == "code_hover" {
        String::new()
    } else if name == "code_completions" {
        let prefix = args
            .get(1)
            .map(|v| string(name, v))
            .transpose()?
            .unwrap_or("");
        if prefix.len() > 256 {
            return Err(safety::arg_err(format!("{name}: prefix exceeds 256 bytes")));
        }
        prefix.into()
    } else {
        symbol(name, &args[1])?
    };
    let doc = Document::parse(name, source)?;
    let answer = match name {
        "code_definition" => doc.definitions(&requested),
        "code_references" => Value::Array(
            doc.identifiers
                .iter()
                .filter(|id| {
                    if requested.contains('.') {
                        id.terminal && id.qualified == requested
                    } else {
                        id.text == requested
                    }
                })
                .map(|id| {
                    record(vec![
                        ("name", Value::Str(id.text.clone())),
                        ("qualified", Value::Str(id.qualified.clone())),
                        ("line", Value::Int(id.line as i64)),
                        ("column", Value::Int(id.column as i64)),
                        (
                            "end_column",
                            Value::Int((id.column + id.text.chars().count()) as i64),
                        ),
                    ])
                })
                .collect(),
        ),
        "code_signature" => doc.signature(&requested),
        "code_hover" => {
            let line = position(name, &args[1])?;
            let column = position(name, &args[2])?;
            let rows = source.split('\n').collect::<Vec<_>>();
            if line > rows.len() || column > rows[line - 1].chars().count() + 1 {
                return Err(safety::arg_err("code_hover: position is outside source"));
            }
            match doc.identifiers.iter().find(|id| {
                id.line == line
                    && (id.column..id.column + id.text.chars().count()).contains(&column)
            }) {
                Some(id) => record(vec![
                    ("found", Value::Bool(true)),
                    ("name", Value::Str(id.text.clone())),
                    ("qualified", Value::Str(id.qualified.clone())),
                    ("line", Value::Int(id.line as i64)),
                    ("column", Value::Int(id.column as i64)),
                    ("definitions", doc.definitions(&id.text)),
                    ("signature", doc.signature(&id.qualified)),
                ]),
                None => record(vec![("found", Value::Bool(false))]),
            }
        }
        "code_callees" => {
            let mut names = BTreeSet::new();
            let mut dynamic = 0;
            let mut found = false;
            for (n, value, ..) in doc.bindings() {
                if n == requested && matches!(value, Expr::Lambda { .. } | Expr::AsyncLambda { .. })
                {
                    found = true;
                    calls(value, &mut names, &mut dynamic);
                }
            }
            record(vec![
                ("name", Value::Str(requested)),
                ("found", Value::Bool(found)),
                ("callees", strings(names)),
                ("dynamic_calls", Value::Int(dynamic as i64)),
            ])
        }
        "code_callers" => {
            fn inspect(stmt: &Stmt, target: &str, found: &mut BTreeSet<String>) {
                let (owner, expr) = match stmt {
                    Stmt::Let { name, value, .. } => (
                        if matches!(value, Expr::Lambda { .. } | Expr::AsyncLambda { .. }) {
                            name.as_str()
                        } else {
                            "<top-level>"
                        },
                        value,
                    ),
                    Stmt::Expr(expr) => ("<top-level>", expr),
                    Stmt::Cfg { body, .. } => {
                        inspect(body, target, found);
                        return;
                    }
                    _ => return,
                };
                let mut names = BTreeSet::new();
                let mut dynamic = 0;
                calls(expr, &mut names, &mut dynamic);
                if names.contains(target) {
                    found.insert(owner.into());
                }
            }
            let mut names = BTreeSet::new();
            for stmt in &doc.statements {
                inspect(stmt, &requested, &mut names);
            }
            strings(names)
        }
        "code_completions" => {
            let mut results = BTreeMap::new();
            for name in crate::builtins::BUILTIN_LOOKUP
                .keys()
                .filter(|n| n.starts_with(&requested))
            {
                let sig = signature::signature_of(name);
                results.insert(
                    name.to_string(),
                    record(vec![
                        ("name", Value::Str(name.to_string())),
                        ("kind", Value::Str("builtin".into())),
                        ("declared", Value::Bool(sig.is_some())),
                        (
                            "signature",
                            sig.map(|s| Value::Str(s.render())).unwrap_or(Value::Null),
                        ),
                    ]),
                );
            }
            for (n, value, ..) in doc
                .bindings()
                .into_iter()
                .filter(|(n, ..)| n.starts_with(&requested))
            {
                results.insert(
                    n.into(),
                    record(vec![
                            ("name", Value::Str(n.into())),
                            (
                                "kind",
                                Value::Str(
                                    if matches!(
                                        value,
                                        Expr::Lambda { .. } | Expr::AsyncLambda { .. }
                                    ) {
                                        "function"
                                    } else {
                                        "variable"
                                    }
                                    .into(),
                                ),
                            ),
                            ("declared", Value::Bool(true)),
                            ("signature", doc.signature(n)),
                        ]),
                );
            }
            Value::Array(results.into_values().collect())
        }
        _ => unreachable!("fixed source navigation dispatcher"),
    };
    // Refuse oversized results rather than presenting a partial source index.
    if serde_json::to_vec(&answer.to_json())?.len() > 16 * 1024 * 1024 {
        return Err(safety::budget_exceeded(
            name,
            "source analysis result exceeds 16 MiB".into(),
            "analyze a smaller source unit or narrower prefix",
        ));
    }
    Ok(answer)
}
