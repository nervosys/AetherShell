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

// ── Polymorphic shapes ──────────────────────────────────────────────────────
//
// A fixed shape is proven by two probes agreeing. A *relative* shape needs the
// opposite: the two probes must disagree in exactly the way `T` predicts. If
// `first` returned `int` for both an int array and a str array, `T` would be
// the wrong description.

/// Probes for the polymorphic set: a builtin and argument sets whose first
/// argument has deliberately different element types.
fn poly_probes() -> Vec<(&'static str, Vec<Vec<Value>>)> {
    let ints = Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    let strs = Value::Array(vec![
        Value::Str("a".into()),
        Value::Str("b".into()),
        Value::Str("c".into()),
    ]);
    let bools = Value::Array(vec![Value::Bool(true), Value::Bool(false)]);
    vec![
        (
            "first",
            vec![vec![ints.clone()], vec![strs.clone()], vec![bools.clone()]],
        ),
        ("last", vec![vec![ints.clone()], vec![strs.clone()]]),
        ("reverse", vec![vec![ints.clone()], vec![strs.clone()]]),
        (
            "take",
            vec![
                vec![ints.clone(), Value::Int(2)],
                vec![strs.clone(), Value::Int(2)],
            ],
        ),
        ("unique", vec![vec![ints.clone()], vec![strs.clone()]]),
        (
            "values",
            vec![
                vec![rec(&[("a", Value::Int(1)), ("b", Value::Int(2))])],
                vec![rec(&[("a", Value::Str("x".into()))])],
            ],
        ),
    ]
}

#[test]
fn every_polymorphic_shape_predicts_the_actual_result() {
    use aethershell::shapes::{element_of, instantiate, polymorphic_shape_of};

    let probes = poly_probes();
    let mut wrong = Vec::new();
    for (name, declared) in aethershell::shapes::POLYMORPHIC {
        let Some((_, arg_sets)) = probes.iter().find(|(n, _)| n == name) else {
            wrong.push(format!("  {name}: declared `{declared}` but has no probe"));
            continue;
        };
        for args in arg_sets {
            let Some(element) = element_of(&args[0]) else {
                wrong.push(format!(
                    "  {name}: probe argument has no single element type"
                ));
                continue;
            };
            let expected = instantiate(declared, &element);
            match call(name, args.clone()) {
                None => wrong.push(format!("  {name}: probe errored")),
                Some(v) => {
                    let actual = observe(&v);
                    if actual != expected {
                        wrong.push(format!(
                            "  {name}: with T={element} expected `{expected}`, observed `{actual}`"
                        ));
                    }
                }
            }
        }
        assert_eq!(
            polymorphic_shape_of(name),
            Some(*declared),
            "lookup must agree with the table"
        );
    }
    assert!(
        wrong.is_empty(),
        "{} polymorphic shape(s) do not predict the result:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn a_polymorphic_shape_actually_varies_with_its_input() {
    // The claim `T` makes is that the result *tracks* the argument. If a probe
    // set produced one shape regardless of input, `T` would be an unnecessary
    // and misleading way to say something fixed.
    let probes = poly_probes();
    let (_, arg_sets) = probes.iter().find(|(n, _)| *n == "first").expect("probed");
    let observed: Vec<String> = arg_sets
        .iter()
        .filter_map(|a| call("first", a.clone()))
        .map(|v| observe(&v))
        .collect();
    let distinct: std::collections::BTreeSet<&String> = observed.iter().collect();
    assert!(
        distinct.len() > 1,
        "expected `first` to vary with its input, always got {observed:?}"
    );
}

#[test]
fn a_builtin_is_never_both_fixed_and_polymorphic() {
    // The two tables answer the same question; overlapping would let them
    // disagree, and a consumer would have no way to know which is meant.
    let overlap: Vec<&str> = DECLARED
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| aethershell::shapes::POLYMORPHIC.iter().any(|(p, _)| p == n))
        .collect();
    assert!(overlap.is_empty(), "declared in both tables: {overlap:?}");
}

#[test]
fn sum_stays_undeclared_because_its_rule_is_promotion_not_substitution() {
    // Guards the honesty of the notation itself. `sum` over ints yields `int`,
    // over floats `float` — that tracks a promotion rule, not the element type,
    // so `T` would be a plausible-looking lie.
    use aethershell::shapes::{polymorphic_shape_of, shape_of};
    assert_eq!(shape_of("sum"), None);
    assert_eq!(polymorphic_shape_of("sum"), None);
}

// ── Field examples ──────────────────────────────────────────────────────────

/// Whether an example reads as a filesystem path rather than a plain string.
fn path_shaped(s: &str) -> bool {
    s.contains('/') || s.contains('\\')
}

/// The part after the last separator, treating both kinds as separators so a
/// Windows example and a POSIX listing are comparable.
fn final_component(s: &str) -> &str {
    s.rsplit(['/', '\\']).next().unwrap_or(s)
}

#[test]
fn the_path_example_resolves_on_every_runner_layout() {
    // The bug this replaces was invisible on Windows and broke all three CI
    // platforms, so the fix is checked against the layouts it has to survive
    // rather than only against the one this test happens to run on.
    let example = aethershell::shapes::LS_PATH_EXAMPLE;
    assert!(
        path_shaped(example),
        "the ls.path example must read as a path: {example}"
    );
    assert_eq!(
        final_component(example),
        "agent.rs",
        "the example must name a file that is actually in src/"
    );

    for row in [
        "/home/runner/work/AetherShell/AetherShell/src/agent.rs", // ubuntu
        "/Users/runner/work/AetherShell/AetherShell/src/agent.rs", // macos
        "\\\\?\\D:\\a\\AetherShell\\AetherShell\\src\\agent.rs",  // windows
        "src/agent.rs",                                           // relative
    ] {
        assert_eq!(
            final_component(row),
            final_component(example),
            "example `{example}` does not resolve against `{row}`"
        );
    }
}

#[test]
fn every_field_example_is_what_the_builtin_actually_returns() {
    // An unverified example is worse than none: it would be a confident,
    // checkable-looking claim that a filter could be written against and fail
    // silently — precisely the failure the examples exist to prevent.
    use aethershell::shapes::FIELD_EXAMPLES;

    let probes = probes();
    let mut wrong = Vec::new();
    for (builtin, field, example) in FIELD_EXAMPLES {
        let Some((_, arg_sets)) = probes.iter().find(|(n, _)| n == builtin) else {
            wrong.push(format!("  {builtin}.{field}: no probe"));
            continue;
        };
        // Check every probe argument, not just the first. `ls(".")` is the
        // repo root and contains no `.rs` files, so looking only there
        // "disproved" a correct example — the check was too narrow, not the
        // example wrong.
        let mut rows: Vec<Value> = Vec::new();
        for args in arg_sets {
            if let Some(Value::Array(mut r)) = call(builtin, args.clone()) {
                rows.append(&mut r);
            }
        }
        if rows.is_empty() {
            wrong.push(format!("  {builtin}.{field}: no probe returned any rows"));
            continue;
        }
        // The example must be *shaped* like a real value of that field. Exact
        // equality is wrong — a path or timestamp differs per machine.
        //
        // Leading characters were the old rule, and they were themselves
        // machine-specific: they passed only on the box the example was
        // captured on. `\\?\C:\...` cannot prefix a POSIX path, and even among
        // POSIX runners `/home/runner` and `/Users/runner` agree on exactly one
        // character. For a path-shaped example, compare the final component
        // instead — stable on every platform, and stronger, since it names a
        // file that must actually be in the listing.
        let matched = rows.iter().any(|r| match r {
            Value::Record(m) => match m.get(*field) {
                Some(Value::Str(s)) if path_shaped(example) => {
                    s == example || final_component(s) == final_component(example)
                }
                Some(Value::Str(s)) => {
                    s == example || s.starts_with(&example[..example.len().min(2)])
                }
                Some(Value::Int(_)) => example.chars().all(|c| c.is_ascii_digit()),
                _ => false,
            },
            _ => false,
        });
        if !matched {
            wrong.push(format!(
                "  {builtin}.{field}: no returned row resembles the example `{example}`"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} field example(s) do not match reality:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn the_example_that_caught_a_real_mistake_is_present() {
    // `ls().ext` is `".rs"`, not `"rs"`. Writing the filter from the type alone
    // returned an empty set with no error. Pinned so the example is not tidied
    // away as redundant.
    let ext = aethershell::shapes::field_examples("ls")
        .into_iter()
        .find(|(f, _)| *f == "ext");
    assert_eq!(
        ext,
        Some(("ext", ".rs")),
        "the leading dot is the whole point of this example"
    );
}
