//! How many builtins report a failure with no usable code?
//!
//! `ErrorCode`'s documentation promises that "*every* failure is branchable on
//! `.code`", and `tests/every_failure_has_a_code.rs` holds that line for the
//! two exceptions found by benchmarking: parse errors, and the `sh()` refusal.
//! Both were found one at a time, by hand.
//!
//! Two more turned up the same way while writing declarations: `[1, 2, 3] |
//! head` and `uniq([1, 1, 2])` both answered `E_UNKNOWN` -- the one code the
//! taxonomy tells an agent *not* to reason about -- for conditions entirely
//! knowable before the body ran. Finding them one at a time is not a method.
//! This counts them, across the whole catalogue.
//!
//! # Why this sweeps a subset, and why in-process
//!
//! Widening it to the whole dispatch table was tried and reverted. The effect
//! gate makes calling arbitrary builtins *safe* -- agent mode plus a workspace
//! jail refuses every dangerous effect class before a body runs -- but it does
//! not make them *terminate*. `a2ui_confirm` waits for user confirmation and
//! never returns without a TTY, so the sweep hung on the fifteenth builtin
//! alphabetically and the run had to be killed.
//!
//! A test that can hang is worse than a narrow one: it does not fail, it stops
//! CI. Per-call timeouts need a thread per call, which is a lot of machinery
//! for a ratchet. So the wide sweep stays out-of-process in
//! `benches/agentic/uncoded.mjs`, where `spawnSync` has a timeout, and this
//! keeps the in-process line on a subset that is known to terminate.
//!
//! That hang was worth finding on its own account. The subprocess sweep had
//! been scoring `a2ui_confirm` as "accepted it and answered", because a killed
//! call yields empty output; it now reports hangs separately.

use aethershell::builtins::{call_with_input, is_dispatched};
use aethershell::env::Env;
use aethershell::safety::{effect_of, Effect};
use aethershell::value::Value;

/// Categories whose builtins transform data and return.
const SAFE_CATEGORIES: &[&str] = &[
    "Math",
    "String",
    "Array",
    "Aggregation",
    "Functional",
    "Text",
    "JSON",
];

/// Pure builtins in those categories.
///
/// Walks the category listings rather than asking about each builtin in turn:
/// `ontology_describe_json` rebuilds the whole catalogue per call, so the
/// per-name version did ~1,280 rebuilds and took 24 seconds.
fn sweepable() -> Vec<String> {
    let mut v: Vec<String> = SAFE_CATEGORIES
        .iter()
        .flat_map(|cat| {
            aethershell::agent_api::ontology_describe_json(cat)
                .get("builtins")
                .and_then(|b| b.as_array())
                .map(|bs| {
                    bs.iter()
                        .filter_map(|b| b.get("name").and_then(|n| n.as_str()))
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .filter(|n| effect_of(n) == Effect::Pure && is_dispatched(n))
        .collect();
    v.sort();
    v.dedup();
    v
}

/// An argument no builtin can reasonably accept.
fn nonsense() -> Vec<Value> {
    vec![Value::Record(
        [("unexpected".to_string(), Value::Bool(true))]
            .into_iter()
            .collect(),
    )]
}

/// The `E_*` code in a rendered failure, or `NO_CODE` when there is none.
fn code_of(e: &anyhow::Error) -> String {
    let s = e.to_string();
    match s.find("E_") {
        Some(i) => s[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => "NO_CODE".to_string(),
    }
}

/// Whether the failure is "the external tool is not installed here".
///
/// Says nothing about the builtin's argument handling -- the call never got
/// that far -- so it is counted apart from the shell's own defects. Still a
/// defect: `E_TOOL_MISSING` exists for it, and these are the stragglers that
/// propagate a bare `io::Error` with no context, so not even the tool's name
/// survives to convert.
fn tool_absent(e: &anyhow::Error) -> bool {
    let s = e.to_string();
    s.contains("not found: No such file") || s.contains("No such file or directory (os error 2)")
}

#[test]
fn the_uncoded_failure_count_does_not_grow() {
    let jail = std::env::temp_dir().join(format!("ae_census_{}", std::process::id()));
    std::fs::create_dir_all(&jail).expect("create jail");
    // Single-test binary: this process only. The gate refuses dangerous effect
    // classes before any body runs, which is what makes sweeping the whole
    // dispatch table defensible rather than brave.
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_POLICY", "strict");
    std::env::set_var("AETHER_WORKSPACE", &jail);
    std::env::set_var("AETHER_MAX_NET", "0");

    let names = sweepable();
    assert!(
        names.len() > 40,
        "only {} builtins were selected; the filter is broken, and a census over nothing proves nothing",
        names.len()
    );

    let mut uncoded: Vec<String> = Vec::new();
    let mut absent = 0usize;
    let (mut refused, mut answered, mut gated) = (0usize, 0usize, 0usize);
    for name in names.iter().map(String::as_str) {
        let mut env = Env::new();
        match call_with_input(name, nonsense(), None, &mut env) {
            Ok(_) => answered += 1,
            Err(e) => {
                let c = code_of(&e);
                match c.as_str() {
                    // The gate did its job. Coded by construction, and the body
                    // never ran, so this says nothing about argument handling.
                    "E_NEEDS_APPROVAL" | "E_POLICY_DENY" | "E_OUTSIDE_WORKSPACE" => gated += 1,
                    "E_UNKNOWN" | "NO_CODE" if tool_absent(&e) => absent += 1,
                    "E_UNKNOWN" | "NO_CODE" => uncoded.push(format!("{name} -> {c}")),
                    _ => refused += 1,
                }
            }
        }
    }

    // Non-vacuity: a census where nothing is refused says nothing about codes,
    // and one where the gate refuses everything says nothing either.
    assert!(
        refused > 10,
        "only {refused} builtins produced a coded refusal; either the argument \
         is not nonsensical or the gate is swallowing the whole sweep \
         ({gated} gated, {answered} answered)"
    );

    // A ratchet over the whole catalogue. It exists because ~390 error-handling
    // fixes brought this from 157 to 13 -- and all 13 that remain are an
    // external tool absent from the measuring machine, so the shell's own
    // uncoded failures are at zero. Nothing was stopping that coming back.
    //
    // It may fall -- each fall should be recorded here -- and must never rise.
    // `benches/agentic/uncoded.mjs` is the slow subprocess version that also
    // reports the split.
    // Zero within this scope. The catalogue-wide figure is 13, none of them
    // the shell's own, tracked by `benches/agentic/uncoded.mjs`.
    // `is_empty` rather than `len() <= 0`: against a baseline of zero a `<=`
    // on a `usize` can only ever be an equality, which clippy's
    // `absurd_extreme_comparisons` denies and is right to. This was fixed once
    // already and came back when the file was rewritten wholesale for a
    // widening attempt -- a rewrite loses the fixes an edit would have kept.
    assert!(
        uncoded.is_empty(),
        "{} builtins answer a nonsense argument with an uncoded failure, against \
         a baseline of zero. `E_UNKNOWN` is the one code an agent is told \
         not to reason about, so each is a knowable condition reported as \
         unknowable:\n{:#?}\n\
         ({refused} coded, {gated} gated, {absent} tool absent, {answered} \
         answered, {} swept)",
        uncoded.len(),
        uncoded,
        names.len()
    );

    let _ = std::fs::remove_dir_all(&jail);
}
