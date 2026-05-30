//! Tests for the output-economy builtins: AECON compact rendering + token
//! estimate (docs/AGENTIC_FIRST_DESIGN.md §6.2). Exercised through the public
//! dispatch path, which also validates the 1109/1110 index alignment.

use aethershell::value::Value;
use std::collections::BTreeMap;

fn rec(pairs: &[(&str, Value)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v.clone());
    }
    Value::Record(m)
}

fn call(name: &str, args: Vec<Value>) -> Value {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env).expect("builtin call")
}

#[test]
fn aecon_emits_header_once_for_record_array() {
    let arr = Value::Array(vec![
        rec(&[
            ("name", Value::Str("main.rs".into())),
            ("size", Value::Int(1024)),
        ]),
        rec(&[
            ("name", Value::Str("lib.rs".into())),
            ("size", Value::Int(2048)),
        ]),
    ]);

    let out = match call("aecon", vec![arr]) {
        Value::Str(s) => s,
        other => panic!("aecon should return a string, got {other:?}"),
    };

    // A bare tab-separated header line, then one line per row (tight format).
    assert!(out.starts_with("name\tsize"), "got: {out}");
    assert_eq!(out.lines().count(), 3, "header + 2 rows");
    // Keys appear exactly once (in the header), not repeated per row.
    assert_eq!(out.matches("name").count(), 1, "field name emitted once");
    assert_eq!(out.matches("size").count(), 1, "field name emitted once");
    // Row values are present.
    assert!(out.contains("main.rs\t1024"));
    assert!(out.contains("lib.rs\t2048"));
}

#[test]
fn aecon_factors_out_constant_columns() {
    // 20 rows where `kind`/`owner` are constant but `name`/`size` vary.
    let rows: Vec<Value> = (0..20)
        .map(|i| {
            rec(&[
                ("name", Value::Str(format!("f{i}"))),
                ("size", Value::Int(i)),
                ("kind", Value::Str("file".into())),
                ("owner", Value::Str("alice".into())),
            ])
        })
        .collect();
    let arr = Value::Array(rows);

    let out = match call("aecon", vec![arr.clone()]) {
        Value::Str(s) => s,
        other => panic!("expected string, got {other:?}"),
    };
    // Constant columns appear once in a @const line; varying columns are the cols.
    assert!(out.contains("@const "), "constant columns factored: {out}");
    assert!(out.contains("kind=file"));
    assert!(out.contains("owner=alice"));
    assert!(out.starts_with("name\tsize"), "tight header lists varying cols: {out}");
    // "alice" appears exactly once (in @const), not 20 times.
    assert_eq!(out.matches("alice").count(), 1, "owner not repeated per row");
    assert_eq!(out.matches("file").count(), 1, "kind not repeated per row");

    // And it's cheaper in tokens than the same data without factoring (canonical).
    let factored = match call("tokens", vec![Value::Str(out)]) {
        Value::Int(n) => n,
        _ => panic!(),
    };
    let canon = match call("tokens", vec![call("canonical", vec![arr])]) {
        Value::Int(n) => n,
        _ => panic!(),
    };
    assert!(factored < canon, "factored ({factored}) < canonical ({canon})");
}

#[test]
fn aecon_is_cheaper_than_json_for_homogeneous_records() {
    // 5 rows × 3 fields — the common "ls / proc.list / docker.ps" shape.
    let rows: Vec<Value> = (0..5)
        .map(|i| {
            rec(&[
                ("name", Value::Str(format!("file{i}.rs"))),
                ("size", Value::Int(1000 + i)),
                ("kind", Value::Str("file".into())),
            ])
        })
        .collect();
    let arr = Value::Array(rows);

    // tokens(aecon(v)) should be strictly fewer than tokens(json.stringify(v)).
    let aecon_str = call("aecon", vec![arr.clone()]);
    let json_str = call("json_stringify", vec![arr.clone()]);

    let aecon_tokens = match call("tokens", vec![aecon_str]) {
        Value::Int(n) => n,
        other => panic!("tokens should return Int, got {other:?}"),
    };
    let json_tokens = match call("tokens", vec![json_str]) {
        Value::Int(n) => n,
        other => panic!("tokens should return Int, got {other:?}"),
    };

    assert!(
        aecon_tokens < json_tokens,
        "AECON ({aecon_tokens} tok) should beat JSON ({json_tokens} tok) for homogeneous records"
    );
}

#[test]
fn digest_summarizes_a_large_array_cheaply() {
    let rows: Vec<Value> = (0..500)
        .map(|i| rec(&[("id", Value::Int(i)), ("name", Value::Str(format!("n{i}")))]))
        .collect();
    let arr = Value::Array(rows);

    let d = call("digest", vec![arr.clone()]);
    let m = match &d {
        Value::Record(m) => m.clone(),
        other => panic!("digest should return a record, got {other:?}"),
    };
    assert_eq!(m.get("kind"), Some(&Value::Str("array".into())));
    assert_eq!(m.get("len"), Some(&Value::Int(500)));
    // Element shape conveys the record's field types.
    match m.get("element") {
        Some(Value::Record(e)) => {
            assert_eq!(e.get("id"), Some(&Value::Str("Int".into())));
            assert_eq!(e.get("name"), Some(&Value::Str("String".into())));
        }
        other => panic!("element shape should be a record, got {other:?}"),
    }
    // A 2-element sample is included.
    match m.get("sample") {
        Some(Value::Array(s)) => assert_eq!(s.len(), 2),
        other => panic!("sample should be an array, got {other:?}"),
    }
    // The digest itself costs far fewer tokens than the full value it describes.
    let full = match m.get("full_tokens") {
        Some(Value::Int(n)) => *n,
        _ => panic!("missing full_tokens"),
    };
    let digest_tokens = match call("tokens", vec![d.clone()]) {
        Value::Int(n) => n,
        other => panic!("tokens, got {other:?}"),
    };
    assert!(
        digest_tokens * 4 < full,
        "digest ({digest_tokens} tok) should be far cheaper than the full value ({full} tok)"
    );
}

#[test]
fn pick_projects_fields_and_saves_tokens() {
    let rows: Vec<Value> = (0..50)
        .map(|i| {
            rec(&[
                ("name", Value::Str(format!("file{i}"))),
                ("size", Value::Int(i)),
                ("owner", Value::Str("alice".into())),
                ("kind", Value::Str("file".into())),
            ])
        })
        .collect();
    let full = Value::Array(rows);

    // Keep only name+size; owner/kind are dropped.
    let picked = call(
        "pick",
        vec![full.clone(), Value::Str("name".into()), Value::Str("size".into())],
    );
    match &picked {
        Value::Array(a) => match &a[0] {
            Value::Record(m) => {
                assert!(m.contains_key("name") && m.contains_key("size"));
                assert!(!m.contains_key("owner") && !m.contains_key("kind"));
            }
            other => panic!("expected record, got {other:?}"),
        },
        other => panic!("expected array, got {other:?}"),
    }

    // Projection materially cuts tokens vs the full records (composes with aecon).
    let full_tok = match call("tokens", vec![call("aecon", vec![full])]) {
        Value::Int(n) => n,
        _ => panic!(),
    };
    let picked_tok = match call("tokens", vec![call("aecon", vec![picked])]) {
        Value::Int(n) => n,
        _ => panic!(),
    };
    assert!(
        picked_tok < full_tok,
        "pick should reduce tokens: {picked_tok} vs {full_tok}"
    );
}

#[test]
fn tokens_counts_string_content() {
    // "map read where" → three short words, ~1 token each under the heuristic.
    let n = match call("tokens", vec![Value::Str("map read where".into())]) {
        Value::Int(n) => n,
        other => panic!("expected Int, got {other:?}"),
    };
    assert!((3..=5).contains(&n), "expected ~3 tokens, got {n}");
}

#[test]
fn agent_response_carries_token_accounting() {
    use aethershell::agent_api::{process_request, AgentRequest};
    let req = AgentRequest::Eval {
        code: "1 + 1".to_string(),
    };
    let resp = process_request(&req);
    let meta = resp.metadata.expect("metadata present");
    let acct = meta
        .get("token_accounting")
        .expect("token_accounting present");
    assert!(
        acct.get("tokens_in").and_then(|v| v.as_u64()).unwrap_or(0) > 0,
        "tokens_in counted"
    );
    assert!(
        acct.get("tokens_total")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            > 0,
        "tokens_total counted"
    );
}

#[test]
fn budget_pages_rows_with_a_cursor_and_elision() {
    // 100 small records; a tight budget should show only some and offer a cursor.
    let rows: Vec<Value> = (0..100)
        .map(|i| rec(&[("id", Value::Int(i)), ("name", Value::Str(format!("item{i}")))]))
        .collect();
    let arr = Value::Array(rows);

    let page = match call("budget", vec![arr.clone(), Value::Int(40)]) {
        Value::Record(m) => m,
        other => panic!("budget should return a record, got {other:?}"),
    };
    let shown = match page.get("shown") {
        Some(Value::Int(n)) => *n,
        _ => panic!("missing shown"),
    };
    let total = page.get("total").cloned();
    assert_eq!(total, Some(Value::Int(100)));
    assert!(shown > 0 && shown < 100, "should page a subset, shown={shown}");
    assert_eq!(page.get("truncated"), Some(&Value::Bool(true)));
    // Page fits the budget (or is a single oversized row).
    if let Some(Value::Int(pt)) = page.get("page_tokens") {
        assert!(*pt <= 40 || shown == 1, "page_tokens={pt} should be <= budget");
    }
    // next_cursor advances; paging from it returns the remainder eventually.
    let next = match page.get("next_cursor") {
        Some(Value::Int(n)) => *n,
        other => panic!("expected an Int next_cursor, got {other:?}"),
    };
    assert_eq!(next, shown, "next_cursor should resume after the shown rows");

    // A generous budget shows everything and offers no further cursor.
    match call("budget", vec![arr, Value::Int(100_000)]) {
        Value::Record(m) => {
            assert_eq!(m.get("shown"), Some(&Value::Int(100)));
            assert_eq!(m.get("next_cursor"), Some(&Value::Null));
            assert_eq!(m.get("elided"), Some(&Value::Int(0)));
        }
        other => panic!("expected record, got {other:?}"),
    }
}

#[test]
fn budget_truncates_long_strings_losslessly() {
    let long = "x".repeat(1000);
    match call("budget", vec![Value::Str(long), Value::Int(10)]) {
        Value::Record(m) => {
            assert_eq!(m.get("truncated"), Some(&Value::Bool(true)));
            match m.get("elided_chars") {
                Some(Value::Int(n)) => assert!(*n > 0, "should report elided chars"),
                _ => panic!("missing elided_chars"),
            }
            match m.get("page") {
                Some(Value::Str(s)) => assert!(s.contains("more chars elided"), "marker present"),
                _ => panic!("missing page"),
            }
        }
        other => panic!("expected record, got {other:?}"),
    }
}

#[test]
fn ontology_manifest_is_compact_and_describe_expands() {
    // Root manifest is structural and cheap.
    let manifest = call("ontology_manifest", vec![]);
    let m = match &manifest {
        Value::Record(m) => m.clone(),
        other => panic!("manifest should be a record, got {other:?}"),
    };
    assert!(
        matches!(m.get("categories"), Some(Value::Array(a)) if !a.is_empty()),
        "manifest has categories"
    );
    assert!(matches!(m.get("total_builtins"), Some(Value::Int(n)) if *n > 0));

    // The manifest must be dramatically cheaper than the full ontology dump —
    // this is the standing-context win the Phase-1 benchmark called for.
    let manifest_tok = match call("tokens", vec![manifest.clone()]) {
        Value::Int(n) => n,
        other => panic!("{other:?}"),
    };
    let full = call("ontology_json", vec![]);
    let full_tok = match call("tokens", vec![full]) {
        Value::Int(n) => n,
        other => panic!("{other:?}"),
    };
    assert!(
        manifest_tok * 4 < full_tok,
        "manifest ({manifest_tok} tok) should be far smaller than full ontology ({full_tok} tok)"
    );

    // Expand a category from the manifest → its builtins.
    let cat = match m.get("categories") {
        Some(Value::Array(cats)) => match cats.first() {
            Some(Value::Record(c0)) => match c0.get("category") {
                Some(Value::Str(name)) => name.clone(),
                _ => panic!("category entry missing name"),
            },
            _ => panic!("category entry not a record"),
        },
        _ => unreachable!(),
    };
    match call("ontology_describe", vec![Value::Str(cat)]) {
        Value::Record(r) => assert!(
            matches!(r.get("builtins"), Some(Value::Array(a)) if !a.is_empty()),
            "describe(category) lists builtins"
        ),
        other => panic!("expected record, got {other:?}"),
    }

    // Expand a single builtin → full definition with an effect.
    match call("ontology_describe", vec![Value::Str("ls".into())]) {
        Value::Record(r) => {
            assert_eq!(r.get("builtin"), Some(&Value::Str("ls".into())));
            assert!(r.contains_key("effect"));
        }
        other => panic!("expected record, got {other:?}"),
    }

    // Unknown query → structured error.
    match call("ontology_describe", vec![Value::Str("definitely_not_a_thing_xyz".into())]) {
        Value::Record(r) => assert!(r.contains_key("error")),
        other => panic!("expected error record, got {other:?}"),
    }
}

#[test]
fn aecon_handles_scalars_and_single_record() {
    assert_eq!(call("aecon", vec![Value::Int(42)]), Value::Str("42".into()));
    let single = rec(&[("a", Value::Int(1)), ("b", Value::Bool(true))]);
    match call("aecon", vec![single]) {
        Value::Str(s) => assert_eq!(s, "{a=1 b=true}"),
        other => panic!("expected string, got {other:?}"),
    }
}
