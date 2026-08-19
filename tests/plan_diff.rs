//! `plan()` says how many operations and of what kind. `plan_diff()` says what
//! they would actually do.
//!
//! Two plans with identical summaries can write entirely different bytes, so a
//! summary is enough to judge a plan's *shape* and not enough to judge whether
//! it is right. This is the review surface the roadmap called a "textual plan
//! diff view".

use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;

fn call(name: &str, args: Vec<Value>) -> Value {
    let mut env = Env::new();
    aethershell::builtins::call(name, args, &mut env).expect("builtin call")
}

fn op(kind: &str, path: &str, content: Option<&str>) -> Value {
    let mut m = BTreeMap::new();
    m.insert("op".to_string(), Value::Str(kind.to_string()));
    m.insert("path".to_string(), Value::Str(path.to_string()));
    if let Some(c) = content {
        m.insert("content".to_string(), Value::Str(c.to_string()));
    }
    Value::Record(m)
}

fn field<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    match v {
        Value::Record(m) => m.get(k),
        _ => None,
    }
}

fn int(v: &Value, k: &str) -> i64 {
    match field(v, k) {
        Some(Value::Int(n)) => *n,
        other => panic!("expected int at {k}, got {other:?}"),
    }
}

fn tmp(tag: &str) -> String {
    let p = std::env::temp_dir().join(format!("ae_plandiff_{tag}.txt"));
    let _ = std::fs::remove_file(&p);
    p.to_string_lossy().to_string()
}

#[test]
fn a_rewrite_reports_only_the_lines_that_change() {
    let path = tmp("rewrite");
    std::fs::write(&path, "alpha\nbeta\ngamma\n").unwrap();
    let out = call(
        "plan_diff",
        vec![Value::Array(vec![op(
            "write",
            &path,
            Some("alpha\ndelta\ngamma\n"),
        )])],
    );
    assert_eq!(int(&out, "added"), 1, "delta is the only new line");
    assert_eq!(int(&out, "removed"), 1, "beta is the only line lost");
}

#[test]
fn writing_a_new_file_is_all_additions_and_says_it_does_not_exist() {
    let path = tmp("create");
    let out = call(
        "plan_diff",
        vec![Value::Array(vec![op("write", &path, Some("one\ntwo\n"))])],
    );
    assert_eq!(int(&out, "added"), 2);
    assert_eq!(int(&out, "removed"), 0);
    let entry = match field(&out, "diff") {
        Some(Value::Array(a)) => a[0].clone(),
        other => panic!("expected a diff array, got {other:?}"),
    };
    assert_eq!(
        field(&entry, "exists"),
        Some(&Value::Bool(false)),
        "a reviewer must be able to tell a create from an overwrite"
    );
}

#[test]
fn a_delete_accounts_for_every_line_it_removes() {
    // The case where a summary is least informative: `plan()` reports one
    // delete whether the file holds a line or a thousand.
    let path = tmp("delete");
    std::fs::write(&path, "a\nb\nc\nd\n").unwrap();
    let out = call("plan_diff", vec![Value::Array(vec![op("rm", &path, None)])]);
    assert_eq!(int(&out, "removed"), 4);
    assert_eq!(int(&out, "added"), 0);
}

#[test]
fn an_append_does_not_report_the_untouched_body_as_changed() {
    // Appending must not read as rewriting the file — that would make every
    // append look like a whole-file change and train reviewers to skim.
    let path = tmp("append");
    std::fs::write(&path, "keep1\nkeep2\n").unwrap();
    let out = call(
        "plan_diff",
        vec![Value::Array(vec![op("append", &path, Some("added\n"))])],
    );
    assert_eq!(int(&out, "added"), 1);
    assert_eq!(int(&out, "removed"), 0);
}

#[test]
fn a_large_change_is_capped_and_says_how_much_it_elided() {
    // Bounded output with the omission reported, matching `budget()`. Silently
    // truncating a diff is how a reviewer approves what they did not see.
    let path = tmp("large");
    let body: String = (0..500).map(|i| format!("line{i}\n")).collect();
    let out = call(
        "plan_diff",
        vec![Value::Array(vec![op("write", &path, Some(&body))])],
    );
    assert_eq!(int(&out, "added"), 500);
    let entry = match field(&out, "diff") {
        Some(Value::Array(a)) => a[0].clone(),
        other => panic!("expected a diff array, got {other:?}"),
    };
    let shown = match field(&entry, "lines") {
        Some(Value::Array(a)) => a.len() as i64,
        other => panic!("expected lines, got {other:?}"),
    };
    assert!(shown < 500, "the diff must be capped, showed {shown}");
    assert_eq!(
        int(&entry, "elided"),
        500 - shown,
        "what was omitted must be counted, not dropped"
    );
}

#[test]
fn the_diff_token_matches_the_plans_token() {
    // `plan()` mints an approval token over the ops. Reviewing a diff and then
    // approving a *different* token would defeat the point, so they must agree.
    let path = tmp("token");
    let ops = Value::Array(vec![op("write", &path, Some("x\n"))]);
    let planned = call("plan", vec![ops.clone()]);
    let diffed = call("plan_diff", vec![ops]);
    assert_eq!(
        field(&planned, "token"),
        field(&diffed, "token"),
        "the diff must describe the very plan the token approves"
    );
}

#[test]
fn plan_diff_declares_that_it_reads_local_state() {
    // It opens every target file. Advertising `Pure` would tell an agent the
    // call is referentially transparent and safe to cache, when its answer
    // changes the moment anything touches those paths.
    use aethershell::safety::{effect_is_declared, effect_of, Effect};
    assert_eq!(effect_of("plan_diff"), Effect::ReadLocal);
    assert!(effect_is_declared("plan_diff"));
}
