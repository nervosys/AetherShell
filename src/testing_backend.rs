//! Pure typed snapshot comparison and Rust unit-test source generation.
use crate::{safety, value::Value};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};
fn bad(name: &str, message: &str) -> anyhow::Error {
    safety::arg_err(format!("{name}: {message}"))
}
fn budget(name: &str, message: &str) -> anyhow::Error {
    safety::budget_exceeded(name, message.into(), "use smaller snapshots or test cases")
}
fn row(fields: Vec<(&str, Value)>) -> Value {
    Value::Record(
        fields
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect::<BTreeMap<_, _>>(),
    )
}
fn validate(value: &Value, depth: usize, nodes: &mut usize, bytes: &mut usize) -> Result<()> {
    const NAME: &str = "test_snapshot";
    *nodes += 1;
    if depth > 32 || *nodes > 10000 {
        return Err(budget(
            NAME,
            "snapshots exceed depth 32 or 10000 combined nodes",
        ));
    }
    match value {
        Value::Null | Value::Bool(_) | Value::Int(_) => *bytes = bytes.saturating_add(8),
        Value::Float(x) if x.is_finite() => *bytes = bytes.saturating_add(8),
        Value::Str(s) => *bytes = bytes.saturating_add(s.len()),
        Value::Array(items) => {
            for item in items {
                validate(item, depth + 1, nodes, bytes)?
            }
        }
        Value::Record(fields) => {
            for (key, item) in fields {
                if key.len() > 256 {
                    return Err(budget(NAME, "snapshot keys exceed 256 bytes"));
                }
                *bytes = bytes.saturating_add(key.len());
                validate(item, depth + 1, nodes, bytes)?;
            }
        }
        _ => {
            return Err(bad(
                NAME,
                "snapshots support only Null, Bool, Int, finite Float, String, Array and Record",
            ))
        }
    }
    if *bytes > 1048576 {
        return Err(budget(
            NAME,
            "snapshots exceed 1 MiB combined scalar/key bytes",
        ));
    }
    Ok(())
}
fn difference(
    actual: Option<&Value>,
    expected: Option<&Value>,
    path: &str,
    count: &mut usize,
    rows: &mut Vec<Value>,
) {
    if actual == expected {
        return;
    }
    match (actual, expected) {
        (Some(Value::Record(a)), Some(Value::Record(e))) => {
            let keys = a.keys().chain(e.keys()).collect::<BTreeSet<_>>();
            for key in keys {
                let escaped = key.replace('~', "~0").replace('/', "~1");
                difference(
                    a.get(key),
                    e.get(key),
                    &format!("{path}/{escaped}"),
                    count,
                    rows,
                );
            }
        }
        (Some(Value::Array(a)), Some(Value::Array(e))) => {
            for i in 0..a.len().max(e.len()) {
                difference(a.get(i), e.get(i), &format!("{path}/{i}"), count, rows)
            }
        }
        _ => {
            *count += 1;
            if rows.len() < 128 {
                let kind = if actual.is_none() {
                    "removed"
                } else if expected.is_none() {
                    "added"
                } else {
                    "changed"
                };
                rows.push(row(vec![
                    ("path", Value::Str(path.into())),
                    ("kind", Value::Str(kind.into())),
                    ("actual", actual.cloned().unwrap_or(Value::Null)),
                    ("expected", expected.cloned().unwrap_or(Value::Null)),
                ]));
            }
        }
    }
}
fn snapshot(args: &[Value]) -> Result<Value> {
    let (mut nodes, mut bytes) = (0, 0);
    validate(&args[0], 0, &mut nodes, &mut bytes)?;
    validate(&args[1], 0, &mut nodes, &mut bytes)?;
    let (mut count, mut rows) = (0, Vec::new());
    difference(Some(&args[0]), Some(&args[1]), "", &mut count, &mut rows);
    Ok(row(vec![
        ("matched", Value::Bool(count == 0)),
        ("mismatches", Value::Int(count as i64)),
        ("truncated", Value::Bool(count > rows.len())),
        ("differences", Value::Array(rows)),
    ]))
}
fn expression(value: &Value, total: &mut usize) -> Result<syn::Expr> {
    const NAME: &str = "test_generate";
    let Value::Str(source) = value else {
        return Err(bad(NAME, "case expressions must be Rust source Strings"));
    };
    *total = total.saturating_add(source.len());
    if source.len() > 16384 || *total > 65536 {
        return Err(budget(
            NAME,
            "expressions exceed 16 KiB each or 64 KiB total",
        ));
    }
    if source.bytes().filter(u8::is_ascii_punctuation).count() > 128 {
        return Err(budget(
            NAME,
            "expression exceeds 128 raw ASCII punctuation characters, including strings/comments",
        ));
    }
    syn::parse_str(source).map_err(|_| bad(NAME, "invalid Rust expression"))
}
fn generate(args: &[Value]) -> Result<Value> {
    const NAME: &str = "test_generate";
    let Value::Str(target) = &args[0] else {
        return Err(bad(NAME, "function path must be String"));
    };
    if target.len() > 256 || target.bytes().filter(u8::is_ascii_punctuation).count() > 32 {
        return Err(bad(
            NAME,
            "function path exceeds 256 bytes or 32 punctuation characters",
        ));
    }
    let target: syn::Path =
        syn::parse_str(target).map_err(|_| bad(NAME, "invalid Rust function path"))?;
    let Value::Array(cases) = &args[1] else {
        return Err(bad(NAME, "cases must be Array"));
    };
    if cases.is_empty() || cases.len() > 128 {
        return Err(bad(NAME, "supply 1..128 test cases"));
    }
    let mut total = 0;
    let mut generated = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let Value::Record(case) = case else {
            return Err(bad(NAME, "test cases must be records"));
        };
        if case.len() != 2 {
            return Err(bad(NAME, "cases require exactly arguments and expected"));
        }
        let Some(Value::Array(arguments)) = case.get("arguments") else {
            return Err(bad(
                NAME,
                "case arguments must be Array of Rust expression Strings",
            ));
        };
        if arguments.len() > 64 {
            return Err(bad(NAME, "at most 64 arguments per case"));
        }
        let arguments = arguments
            .iter()
            .map(|value| expression(value, &mut total))
            .collect::<Result<Vec<_>>>()?;
        let expected = expression(
            case.get("expected")
                .ok_or_else(|| bad(NAME, "case expected expression required"))?,
            &mut total,
        )?;
        let ident = quote::format_ident!("case_{:03}", index);
        generated.push(quote::quote! { #[test] fn #ident() { assert_eq!(#target(#(#arguments),*), #expected); } });
    }
    let output=quote::quote! { #[cfg(test)] mod __ae_generated_tests { #[allow(unused_imports)] use super::*; #(#generated)* } }.to_string();
    if output.len() > 1048576 {
        return Err(budget(NAME, "generated source exceeds 1 MiB"));
    }
    Ok(Value::Str(output))
}
pub(super) fn run(name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if args.len() != 2 || input.is_some() {
        return Err(bad(name, "require two arguments without pipeline input"));
    }
    match name {
        "test_snapshot" => snapshot(&args),
        "test_generate" => generate(&args),
        _ => unreachable!("internal testing operation"),
    }
}
