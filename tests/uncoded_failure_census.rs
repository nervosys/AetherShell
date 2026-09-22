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
//! This counts them.
//!
//! # Safety, and why it is not simply `effect_of(name) == Pure`
//!
//! That predicate returns `Pure` for anything nobody classified, so on its own
//! it means "not known to be harmful", not "known harmless". Adding
//! `effect_is_declared` does not rescue it either: `classified_effect` only
//! names effects that *are* dangerous, so **nothing is ever declared `Pure`**
//! and that filter selects the empty set. A non-vacuity guard caught that on
//! the first run, which is the only reason this note is accurate rather than
//! confident.
//!
//! What does back the claim is `tests/effect_ratchet.rs`, which reads builtin
//! *bodies* for process construction, file writes and socket opens, and reports
//! zero builtins acting while classified `Pure`. That is explicitly a lower
//! bound -- a builtin delegating its effect to a helper is not detected -- so
//! this census narrows further, to the data-transformation categories where
//! delegating to a filesystem or process helper would be surprising.
//!
//! A smaller sweep than the dispatch table, deliberately: the point is to count
//! uncoded failures, not to discover the hard way which builtin deletes
//! something.

use aethershell::builtins::{call_with_input, is_dispatched};
use aethershell::env::Env;
use aethershell::safety::{effect_of, Effect};
use aethershell::value::Value;

/// Categories whose builtins transform data and nothing else.
const SAFE_CATEGORIES: &[&str] = &[
    "Math",
    "String",
    "Array",
    "Aggregation",
    "Functional",
    "Text",
];

/// Pure builtins in the data-transformation categories.
///
/// Walks the six category listings rather than asking about each builtin in
/// turn. `ontology_describe_json` rebuilds the whole catalogue on every call,
/// so the per-name version did ~1,280 rebuilds and took 24 seconds.
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

/// An argument no data-transformation builtin can reasonably accept.
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

#[test]
fn the_uncoded_failure_count_does_not_grow() {
    let pure = sweepable();
    assert!(
        pure.len() > 40,
        "only {} builtins were selected; the filter is broken, and a census over \
         nothing proves nothing",
        pure.len()
    );

    let mut uncoded: Vec<String> = Vec::new();
    let (mut refused, mut answered) = (0usize, 0usize);
    for name in pure.iter().map(String::as_str) {
        let mut env = Env::new();
        match call_with_input(name, nonsense(), None, &mut env) {
            Ok(_) => answered += 1,
            Err(e) => {
                refused += 1;
                let c = code_of(&e);
                if c == "E_UNKNOWN" || c == "NO_CODE" {
                    uncoded.push(format!("{name} -> {c}"));
                }
            }
        }
    }

    // Non-vacuity: a census where nothing is refused says nothing about codes.
    assert!(
        refused > 10,
        "only {refused} of {} builtins refused a nonsense argument; either the \
         argument is not nonsensical or the census is not reaching them",
        pure.len()
    );

    // The census is at zero, and this asserts exactly that. Written as
    // `is_empty` rather than `len() <= BASELINE`: against a baseline of zero a
    // `<=` on a `usize` can only ever be an equality, which clippy's
    // `absurd_extreme_comparisons` denies and is right to. If a genuine
    // non-zero baseline is ever needed this becomes a `<=` against a constant
    // that carries the list of what is allowed, and why.
    //
    // Zero within *this scope*, the data-transformation categories. Uncoded
    // failures do exist outside it -- `ab_encode(123)` answers `E_UNKNOWN` --
    // and widening the sweep is how to find them, not assuming this number
    // covers the dispatch table.
    assert!(
        uncoded.is_empty(),
        "{} builtins answer a nonsense argument with an uncoded failure. `E_UNKNOWN` is the one code an agent is told not to reason about, so each of these is a knowable condition reported as unknowable:\n{:#?}\n({refused} refused, {answered} answered, {} swept)",
        uncoded.len(),
        uncoded,
        pure.len()
    );
}
