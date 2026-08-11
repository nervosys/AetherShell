//! Result handles — the value stays server-side, the reference crosses.
//!
//! Two things must hold, and the second is the one that makes the first safe:
//! the saving must be real, and the data must be recoverable *exactly*. A
//! summary that loses information is not a handle, it is truncation with better
//! manners.

use aethershell::value::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// The handle store is process-global — that is the point of it, since a handle
/// must outlive the call that produced it. So these tests must not run
/// concurrently: one test's `clear()` is another's missing handle.
///
/// This lock, not `--test-threads=1`. Requiring a flag makes the suite pass
/// only when invoked a particular way, which is how a green serial run hides a
/// race from the parallel one that CI actually performs.
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

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

fn try_call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env).map_err(|e| e.to_string())
}

/// A result big enough that sending it whole is the wrong thing to do.
fn big_table(n: i64) -> Value {
    Value::Array(
        (0..n)
            .map(|i| {
                rec(&[
                    ("id", Value::Int(1000 + i)),
                    (
                        "path",
                        Value::Str(format!("/srv/app/module_{i:03}/main.rs")),
                    ),
                    (
                        "status",
                        Value::Str(if i % 3 == 0 { "ok" } else { "stale" }.into()),
                    ),
                    ("bytes", Value::Int(4096 + i * 7)),
                ])
            })
            .collect(),
    )
}

fn render(v: &Value) -> String {
    aethershell::builtins::render_agent(v, None).expect("agent render")
}

#[test]
fn a_large_result_is_rendered_as_a_handle_not_as_data() {
    let _g = lock();
    aethershell::handles::clear();
    let out = render(&big_table(400));
    assert!(out.starts_with("@handle "), "expected a handle:\n{out}");
    assert!(out.contains("@shape array<record{"), "shape stated:\n{out}");
    assert!(out.contains("@items 400"), "size stated:\n{out}");
    assert!(out.contains("@hint handle("), "usage taught inline:\n{out}");
}

#[test]
fn the_handle_is_lossless() {
    let _g = lock();
    // The load-bearing property. Everything else is an optimisation; this is
    // what makes it safe to apply automatically.
    aethershell::handles::clear();
    let original = big_table(400);
    render(&original);
    let id = aethershell::handles::list()[0].id.clone();
    let recovered = call("handle", vec![Value::Str(id)]);
    assert_eq!(
        recovered, original,
        "a handle must return exactly what was computed"
    );
}

fn tokens(s: &str) -> usize {
    match call("tokens", vec![Value::Str(s.to_string())]) {
        Value::Int(n) => n as usize,
        Value::Record(m) => match m.get("tokens") {
            Some(Value::Int(n)) => *n as usize,
            other => panic!("unexpected tokens shape: {other:?}"),
        },
        other => panic!("unexpected tokens result: {other:?}"),
    }
}

#[test]
fn the_handle_costs_far_less_than_the_data_and_the_gap_widens_with_size() {
    let _g = lock();
    // Measured in tokens, not bytes — bytes are a proxy, and this project has
    // been bitten by quoting a proxy as the result. Run with
    // `--features real-tokens` for the exact cl100k count; the default build
    // reports the labelled heuristic.
    //
    // The handle summary is constant-size, so the ratio is not a fixed
    // property — it grows with the result. Asserting at two sizes keeps the
    // claim honest about which number belongs to which payload.
    let mut ratios = Vec::new();
    for n in [400, 2000] {
        aethershell::handles::clear();
        let v = big_table(n);
        let whole = match call("aecon", vec![v.clone()]) {
            Value::Str(s) => s,
            other => panic!("aecon should return a string, got {other:?}"),
        };
        let handled = render(&v);
        let (wt, ht) = (tokens(&whole), tokens(&handled));
        println!(
            "{n} rows — whole: {wt} tokens; handle: {ht} tokens ({:.1}x)",
            wt as f64 / ht as f64
        );
        assert!(
            ht * 8 < wt,
            "{n} rows: expected a large saving, {ht} vs {wt}"
        );
        ratios.push(wt as f64 / ht as f64);
    }
    assert!(
        ratios[1] > ratios[0],
        "the saving must grow with the result: {ratios:?}"
    );
}

#[test]
fn a_preview_states_how_much_it_omits() {
    let _g = lock();
    // A preview that reads like a complete result is how silent truncation
    // misleads. The count must be present and correct.
    aethershell::handles::clear();
    let out = render(&big_table(400));
    assert!(out.contains("@omitted 397"), "omission stated:\n{out}");
}

#[test]
fn a_small_result_is_returned_whole() {
    let _g = lock();
    // Handles must not tax the common case: below the threshold nothing changes.
    aethershell::handles::clear();
    let small = big_table(3);
    let out = render(&small);
    assert!(
        !out.starts_with("@handle"),
        "small results stay whole:\n{out}"
    );
    assert!(
        aethershell::handles::list().is_empty(),
        "a small result must not consume a handle"
    );
}

#[test]
fn a_giant_string_is_not_handled_because_it_could_not_be_narrowed() {
    let _g = lock();
    // Saving tokens by handing back a reference the agent cannot query would
    // trade a cost for a dead end.
    aethershell::handles::clear();
    let out = render(&Value::Array(vec![Value::Str("x".repeat(50_000))]));
    let _ = out;
    let huge = Value::Str("x".repeat(50_000));
    let rendered = aethershell::builtins::render_agent(&huge, None).expect("render");
    assert!(
        !rendered.starts_with("@handle"),
        "an opaque blob is sent as-is"
    );
}

#[test]
fn handles_lists_what_is_live_without_the_data() {
    let _g = lock();
    aethershell::handles::clear();
    render(&big_table(400));
    render(&big_table(500));
    let listed = call("handles", vec![]);
    match listed {
        Value::Array(rows) => {
            assert_eq!(rows.len(), 2);
            let first = format!("{:?}", rows[0]);
            assert!(first.contains("items"), "listing carries size: {first}");
            assert!(
                !first.contains("module_001"),
                "the listing must not re-send the data: {first}"
            );
        }
        other => panic!("expected an array, got {other:?}"),
    }
}

#[test]
fn an_unknown_handle_is_an_actionable_error_not_a_null() {
    let _g = lock();
    // Returning null would be indistinguishable from an empty result, and a
    // handle from a previous process is exactly the case an agent will hit.
    aethershell::handles::clear();
    let err = try_call("handle", vec![Value::Str("h99".into())])
        .expect_err("an unknown handle must error");
    assert!(err.contains("h99"), "names the id: {err}");
    assert!(
        err.contains("no handles live") || err.contains("live:"),
        "explains what is available: {err}"
    );
}

#[test]
fn dropping_a_handle_reports_whether_it_existed() {
    let _g = lock();
    aethershell::handles::clear();
    render(&big_table(400));
    let id = aethershell::handles::list()[0].id.clone();
    assert_eq!(
        call("handle_drop", vec![Value::Str(id.clone())]),
        Value::Bool(true)
    );
    assert_eq!(
        call("handle_drop", vec![Value::Str(id)]),
        Value::Bool(false)
    );
}
