//! Proof for every advertised return shape.
//!
//! `shapes::DECLARED` may only contain shapes this file can reproduce by calling
//! the builtin. Two invariants keep it honest in both directions:
//!
//! * a declared shape with no probe, or one whose probe disagrees, fails; and
//! * a probe whose builtin is not declared fails.
//!
//! The second matters as much as the first. Without it a probe could be quietly
//! dropped when it became inconvenient, leaving a claim standing with nothing
//! behind it — which is exactly how the effect misclassifications survived.

use aethershell::shapes::{observe, DECLARED};
use aethershell::value::Value;
use std::collections::BTreeMap;

fn rec(pairs: &[(&str, Value)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v.clone());
    }
    Value::Record(m)
}

/// Call a builtin, returning `None` if it errors. A probe that cannot run is a
/// failed proof, not a skipped one.
fn call(name: &str, args: Vec<Value>) -> Option<Value> {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env).ok()
}

/// The probe set: a builtin, and **several** argument sets that exercise it
/// deterministically without touching the network, spawning a process, or
/// writing anything.
///
/// Every argument set must produce the *same* shape. That requirement is the
/// point of the plural: a combinator like `first` or `values` returns whatever
/// type it was handed, so a single probe would "prove" `array<int>` purely
/// because the test happened to pass integers. Varying the element type turns
/// that accident into a detectable disagreement, and an input-dependent shape
/// is then correctly refused rather than advertised as fixed.
///
/// Determinism decides membership. `platform_hostname` has a perfectly stable
/// shape, but proving it here would make the suite depend on an external
/// binary; such builtins stay undeclared until a probe can establish them
/// honestly.
fn probes() -> Vec<(&'static str, Vec<Vec<Value>>)> {
    let table = Value::Array(vec![
        rec(&[("n", Value::Int(1)), ("s", Value::Str("a".into()))]),
        rec(&[("n", Value::Int(2)), ("s", Value::Str("b".into()))]),
    ]);
    vec![
        ("pwd", vec![vec![]]),
        (
            "ls",
            vec![vec![Value::Str(".".into())], vec![Value::Str("src".into())]],
        ),
        (
            "range",
            vec![
                vec![Value::Int(1), Value::Int(4)],
                vec![Value::Int(0), Value::Int(2)],
            ],
        ),
        (
            "len",
            vec![
                vec![Value::Array(vec![Value::Int(1), Value::Int(2)])],
                vec![Value::Array(vec![Value::Str("a".into())])],
            ],
        ),
        (
            "keys",
            vec![
                vec![rec(&[("a", Value::Int(1)), ("b", Value::Int(2))])],
                vec![rec(&[("z", Value::Str("s".into()))])],
            ],
        ),
        (
            "values",
            vec![
                vec![rec(&[("a", Value::Int(1))])],
                vec![rec(&[("a", Value::Str("s".into()))])],
            ],
        ),
        (
            "upper",
            vec![
                vec![Value::Str("ab".into())],
                vec![Value::Str("xyz".into())],
            ],
        ),
        (
            "split",
            vec![
                vec![Value::Str("a,b,c".into()), Value::Str(",".into())],
                vec![Value::Str("a b".into()), Value::Str(" ".into())],
            ],
        ),
        (
            "aecon",
            vec![
                vec![table.clone()],
                vec![Value::Array(vec![rec(&[("x", Value::Bool(true))])])],
            ],
        ),
        (
            "tokens",
            vec![
                vec![Value::Str("hello world".into())],
                vec![Value::Str("a".into())],
            ],
        ),
        (
            "type_of",
            vec![
                vec![Value::Int(1)],
                vec![Value::Str("s".into())],
                vec![Value::Bool(true)],
            ],
        ),
        (
            "sum",
            vec![
                vec![Value::Array(vec![Value::Int(1), Value::Int(2)])],
                vec![Value::Array(vec![Value::Float(1.5), Value::Float(2.5)])],
            ],
        ),
        (
            "unique",
            vec![
                vec![Value::Array(vec![Value::Int(1), Value::Int(1)])],
                vec![Value::Array(vec![
                    Value::Str("a".into()),
                    Value::Str("a".into()),
                ])],
            ],
        ),
        (
            "first",
            vec![
                vec![Value::Array(vec![Value::Int(1), Value::Int(2)])],
                vec![Value::Array(vec![Value::Str("a".into())])],
            ],
        ),
        (
            "reverse",
            vec![
                vec![Value::Array(vec![Value::Int(1), Value::Int(2)])],
                vec![Value::Array(vec![
                    Value::Str("a".into()),
                    Value::Str("b".into()),
                ])],
            ],
        ),
        ("ontology_manifest", vec![vec![]]),
    ]
}

/// Observe a builtin across all its argument sets. Returns the agreed shape, or
/// an explanation of why no shape can be advertised.
fn agreed_shape(name: &str, arg_sets: &[Vec<Value>]) -> Result<String, String> {
    let mut seen: Vec<String> = Vec::new();
    for args in arg_sets {
        match call(name, args.clone()) {
            None => return Err(format!("{name}: probe errored")),
            Some(v) => {
                let s = observe(&v);
                if !seen.contains(&s) {
                    seen.push(s);
                }
            }
        }
    }
    match seen.len() {
        0 => Err(format!("{name}: no probes")),
        1 => Ok(seen.remove(0)),
        _ => Err(format!(
            "{name}: shape depends on the input ({}) — not advertisable as fixed",
            seen.join(" vs ")
        )),
    }
}

#[test]
fn report_observed_shapes() {
    // Run with --nocapture to regenerate the DECLARED table from evidence.
    println!("--- observed shapes ---");
    for (name, arg_sets) in probes() {
        match agreed_shape(name, &arg_sets) {
            Ok(s) => println!("    (\"{name}\", \"{s}\"),"),
            Err(why) => println!("    // {why}"),
        }
    }
}

#[test]
fn every_declared_shape_is_proven() {
    let probes = probes();
    let mut wrong = Vec::new();
    for (name, declared) in DECLARED {
        let Some((_, arg_sets)) = probes.iter().find(|(n, _)| n == name) else {
            wrong.push(format!("  {name}: declared `{declared}` but has no probe"));
            continue;
        };
        match agreed_shape(name, arg_sets) {
            Err(why) => wrong.push(format!("  declared `{declared}` but {why}")),
            Ok(actual) if actual != *declared => wrong.push(format!(
                "  {name}: declared `{declared}`, observed `{actual}`"
            )),
            Ok(_) => {}
        }
    }
    assert!(
        wrong.is_empty(),
        "{} advertised shape(s) are not what the builtin returns:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn a_probe_that_disagrees_with_itself_is_not_declared() {
    // The load-bearing rule: an input-dependent shape must never reach DECLARED.
    // `values` returns whatever the record held, so its two probes disagree —
    // and a single probe would have "proved" whichever type the test happened
    // to use. Assert the mechanism actually fires, or it is decoration.
    let probes = probes();
    let (_, arg_sets) = probes
        .iter()
        .find(|(n, _)| *n == "values")
        .expect("values is probed");
    let result = agreed_shape("values", arg_sets);
    assert!(
        result.is_err(),
        "expected `values` to be detected as input-dependent, got {result:?}"
    );
    assert!(
        !DECLARED.iter().any(|(n, _)| *n == "values"),
        "an input-dependent shape must not be advertised as fixed"
    );
}

#[test]
fn every_probe_is_declared_or_provably_undeclarable() {
    // Stops a probe from being dropped while its claim stays standing, and stops
    // a builtin from being declared without one. A probe may be absent from
    // DECLARED only when it cannot agree with itself.
    let mut bad = Vec::new();
    for (name, arg_sets) in probes() {
        let declared = DECLARED.iter().any(|(d, _)| *d == name);
        match (agreed_shape(name, &arg_sets), declared) {
            (Ok(s), false) => bad.push(format!(
                "  {name}: probes agree on `{s}` but it is never advertised"
            )),
            (Err(why), true) => bad.push(format!("  {name}: advertised, but {why}")),
            _ => {}
        }
    }
    assert!(
        bad.is_empty(),
        "{} probe(s) out of step with shapes::DECLARED:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn declared_is_sorted_and_unique() {
    let names: Vec<&str> = DECLARED.iter().map(|(n, _)| *n).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted, "shapes::DECLARED must be sorted and unique");
}
