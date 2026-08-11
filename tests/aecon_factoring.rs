//! Tests for the AECON factoring work added after 5.0.0: `@suffix` (shared
//! trailing runs, symmetric to `@prefix` and composable with it on the same
//! column), `@same` (repeat elision for run-structured columns), and the exact
//! character-cost model that decides which encoding each column receives.
//!
//! Every pass here is a *lossless* transform, so the load-bearing assertion in
//! each test is the round-trip: `aecon_decode(aecon(x)) == x`. The token win is
//! asserted separately, and only where it is real.

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

fn encode(v: &Value) -> String {
    match call("aecon", vec![v.clone()]) {
        Value::Str(s) => s,
        other => panic!("aecon should return a string, got {other:?}"),
    }
}

fn roundtrip(v: &Value) -> Value {
    call("aecon_decode", vec![Value::Str(encode(v))])
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

/// 8 distinct `.rs` filenames: no shared prefix, a 3-char shared suffix.
fn shared_suffix_table() -> Value {
    let names = [
        "main.rs",
        "lib.rs",
        "parser.rs",
        "eval.rs",
        "value.rs",
        "safety.rs",
        "types.rs",
        "env.rs",
    ];
    Value::Array(
        names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                rec(&[
                    ("file", Value::Str((*n).into())),
                    ("size", Value::Int(100 + i as i64)),
                ])
            })
            .collect(),
    )
}

#[test]
fn suffix_factoring_emits_the_shared_extension_once() {
    let arr = shared_suffix_table();
    let out = encode(&arr);

    assert!(out.contains("@suffix file: .rs"), "got: {out}");
    // The extension is emitted once, in the metadata line — not on any row.
    assert_eq!(
        out.matches(".rs").count(),
        1,
        "extension emitted once:\n{out}"
    );
    assert!(out.contains("\nmain\t"), "row keeps only the stem:\n{out}");
}

#[test]
fn suffix_factoring_round_trips_exactly() {
    let arr = shared_suffix_table();
    assert_eq!(roundtrip(&arr), arr, "decode must reconstruct the rows");
}

#[test]
fn suffix_factoring_is_a_real_token_win() {
    let arr = shared_suffix_table();
    let compact = tokens(&encode(&arr));
    // The same table with the extension left on every row is what the pass
    // replaces; construct it by giving each file a distinct extension so no
    // shared suffix exists to factor.
    let unfactored = Value::Array(
        match &arr {
            Value::Array(rows) => rows.clone(),
            _ => unreachable!(),
        }
        .into_iter()
        .enumerate()
        .map(|(i, r)| match r {
            Value::Record(mut m) => {
                if let Some(Value::Str(f)) = m.get("file") {
                    let stem = f.trim_end_matches(".rs").to_string();
                    m.insert("file".into(), Value::Str(format!("{stem}.r{i}")));
                }
                Value::Record(m)
            }
            other => other,
        })
        .collect(),
    );
    let baseline = tokens(&encode(&unfactored));
    assert!(
        compact < baseline,
        "@suffix should cost fewer tokens: {compact} vs {baseline}"
    );
}

#[test]
fn a_column_can_carry_both_a_prefix_and_a_suffix() {
    let stems = [
        "main", "lib", "parser", "eval", "value", "safety", "types", "env",
    ];
    let arr = Value::Array(
        stems
            .iter()
            .map(|s| rec(&[("path", Value::Str(format!("/src/aether/{s}.rs")))]))
            .collect(),
    );
    let out = encode(&arr);

    assert!(out.contains("@prefix path: /src/aether/"), "got: {out}");
    assert!(out.contains("@suffix path: .rs"), "got: {out}");
    assert_eq!(roundtrip(&arr), arr, "prefix+suffix must round-trip");
}

#[test]
fn a_prefix_and_suffix_never_overlap_on_the_shortest_value() {
    // `log_.txt` is exactly the prefix followed by the suffix — its residue after
    // both are stripped is empty. Naive independent prefix/suffix searches would
    // double-count those characters and corrupt the value; the suffix is searched
    // in the prefix's residue precisely so this cannot happen.
    let names = [
        "log_.txt",
        "log_alpha.txt",
        "log_beta.txt",
        "log_gamma.txt",
        "log_delta.txt",
        "log_epsilon.txt",
        "log_zeta.txt",
        "log_eta.txt",
    ];
    let arr = Value::Array(
        names
            .iter()
            .map(|n| rec(&[("name", Value::Str((*n).into()))]))
            .collect(),
    );

    assert_eq!(
        roundtrip(&arr),
        arr,
        "the fully-consumed value must survive the round-trip"
    );
}

/// 10 rows grouped into runs. A dictionary is a live candidate here (83 bytes vs
/// 88 raw), but eliding the runs beats it outright (60), so the cost model leaves
/// the column raw and `@same` is the pass under test.
fn run_structured_table() -> Value {
    let modules = [
        "parser",
        "parser",
        "evaluator",
        "evaluator",
        "typechecker",
        "typechecker",
        "scheduler",
        "scheduler",
        "allocator",
        "collector",
    ];
    Value::Array(
        modules
            .iter()
            .enumerate()
            .map(|(i, m)| {
                rec(&[
                    ("module", Value::Str((*m).into())),
                    ("line", Value::Int(i as i64)),
                ])
            })
            .collect(),
    )
}

#[test]
fn same_elides_a_repeated_cell_and_names_the_column() {
    let arr = run_structured_table();
    let out = encode(&arr);

    assert!(out.contains("@same module"), "got: {out}");
    // Each run's value survives once, not once per row.
    assert_eq!(
        out.matches("evaluator").count(),
        1,
        "repeat elided, not repeated:\n{out}"
    );
    // Columns render key-sorted (`line`, then `module`), so an elided run shows as
    // a line ending in a bare tab.
    assert!(out.contains("\n1\t\n"), "elided cell is empty:\n{out}");
}

#[test]
fn same_round_trips_exactly() {
    let arr = run_structured_table();
    assert_eq!(roundtrip(&arr), arr, "elided runs must be reconstructed");
}

#[test]
fn same_is_a_real_token_win() {
    let arr = run_structured_table();
    let compact = tokens(&encode(&arr));
    // Break every run by making each value distinct; nothing is then elidable.
    let unfactored = Value::Array(
        match &arr {
            Value::Array(rows) => rows.clone(),
            _ => unreachable!(),
        }
        .into_iter()
        .enumerate()
        .map(|(i, r)| match r {
            Value::Record(mut m) => {
                if let Some(Value::Str(s)) = m.get("module") {
                    m.insert("module".into(), Value::Str(format!("{s}{i}")));
                }
                Value::Record(m)
            }
            other => other,
        })
        .collect(),
    );
    let baseline = tokens(&encode(&unfactored));
    assert!(
        compact < baseline,
        "@same should cost fewer tokens: {compact} vs {baseline}"
    );
}

#[test]
fn an_empty_string_survives_repeat_elision() {
    // The empty cell is `@same`'s sentinel, and a genuine empty string renders
    // JSON-quoted (`""`) — so the two can never be confused. Pin that down.
    let arr = Value::Array(
        (0..10)
            .map(|i| {
                rec(&[
                    (
                        "note",
                        Value::Str(if i < 5 { String::new() } else { "done".into() }),
                    ),
                    ("n", Value::Int(i)),
                ])
            })
            .collect(),
    );
    assert_eq!(roundtrip(&arr), arr, "empty strings must survive exactly");
}

#[test]
fn a_row_factored_away_to_nothing_is_still_a_row() {
    // A single-column table whose value is entirely consumed by `@prefix`+`@suffix`
    // renders as a bare empty line. The decoder used to skip every empty line, so
    // that row vanished — a silent, lossy drop that predates `@suffix` (`@prefix`
    // alone reproduces it). Interior blank lines are now read as rows.
    let arr = Value::Array(
        [
            "log_.txt",
            "log_alpha.txt",
            "log_beta.txt",
            "log_gamma.txt",
            "log_delta.txt",
            "log_epsilon.txt",
        ]
        .iter()
        .map(|n| rec(&[("name", Value::Str((*n).into()))]))
        .collect(),
    );
    let decoded = roundtrip(&arr);
    match &decoded {
        Value::Array(rows) => assert_eq!(rows.len(), 6, "no row may be dropped"),
        other => panic!("expected an array, got {other:?}"),
    }
    assert_eq!(decoded, arr);
}

#[test]
fn a_column_without_runs_is_left_alone() {
    // No false firing: alternating values have no repeats to elide, so naming the
    // column in `@same` would cost tokens and save none.
    let arr = Value::Array(
        (0..10)
            .map(|i| {
                rec(&[
                    ("host", Value::Str(format!("node-{i}-of-the-cluster"))),
                    ("n", Value::Int(i)),
                ])
            })
            .collect(),
    );
    let out = encode(&arr);
    assert!(!out.contains("@same"), "nothing to elide:\n{out}");
    assert_eq!(roundtrip(&arr), arr);
}

/// Reports what the two passes are worth on a realistic grouped listing, against
/// the same rows with nothing left to factor. Run with `--features real-tokens`
/// for the true cl100k number; the default build reports the labeled heuristic.
#[test]
fn report_the_gain_on_a_realistic_grouped_listing() {
    let stages = ["build", "test", "deploy"];
    let grouped = Value::Array(
        (0..30)
            .map(|i| {
                rec(&[
                    ("stage", Value::Str(stages[(i / 10) as usize].into())),
                    (
                        "artifact",
                        Value::Str(format!("/out/aethershell/step_{i:02}_result.json")),
                    ),
                    ("ms", Value::Int(120 + i)),
                ])
            })
            .collect(),
    );
    // Same shape, nothing factorable: distinct stages, no shared path run.
    let scattered = Value::Array(
        (0..30)
            .map(|i| {
                rec(&[
                    (
                        "stage",
                        Value::Str(format!("{}{i}", stages[(i % 3) as usize])),
                    ),
                    ("artifact", Value::Str(format!("{i:02}_result_{i}.j{i}"))),
                    ("ms", Value::Int(120 + i)),
                ])
            })
            .collect(),
    );

    let (a, b) = (tokens(&encode(&grouped)), tokens(&encode(&scattered)));
    println!("grouped (factored): {a} tokens; unfactorable equivalent: {b} tokens");
    assert!(a < b, "factoring must not cost more: {a} vs {b}");
    assert_eq!(roundtrip(&grouped), grouped);
}

#[test]
fn a_tab_in_a_value_can_no_longer_corrupt_its_own_dictionary() {
    // `@dict` emits its distinct values tab-separated, but used to accept any
    // `Value::Str` — so a value *containing* a tab split into two dictionary
    // entries and every row after it decoded to the wrong string. Eligibility is
    // now bare-safe strings only, which makes this a correctness gate, not a
    // heuristic. (The old heuristic fired on exactly this shape: 2 distinct
    // values over 8 rows, average length 4.)
    let arr = Value::Array(
        (0..8)
            .map(|i| {
                rec(&[
                    (
                        "label",
                        Value::Str(if i % 2 == 0 { "okay" } else { "a\tbcd" }.into()),
                    ),
                    ("n", Value::Int(i)),
                ])
            })
            .collect(),
    );
    assert_eq!(roundtrip(&arr), arr, "a tabbed value must survive exactly");
}

#[test]
fn prefix_factoring_wins_the_column_when_it_beats_a_dictionary() {
    // Repeating values make `@dict` a candidate, and it used to be tried first and
    // win by default. Here the shared path prefix and extension compress far
    // harder, so the cost model must hand the column to `@prefix`/`@suffix`.
    let stems = ["alpha", "bravo", "charlie", "delta", "echo"];
    let arr = Value::Array(
        (0..10)
            .map(|i| {
                rec(&[(
                    "path",
                    Value::Str(format!("/very/long/shared/path/{}.rs", stems[i % 5])),
                )])
            })
            .collect(),
    );
    let out = encode(&arr);
    assert!(
        out.contains("@prefix path: /very/long/shared/path/"),
        "{out}"
    );
    assert!(out.contains("@suffix path: .rs"), "{out}");
    assert!(
        !out.contains("@dict"),
        "dictionary should have lost:\n{out}"
    );
    assert_eq!(roundtrip(&arr), arr);
}

#[test]
fn a_dictionary_now_fires_where_the_old_heuristic_declined_it() {
    // 6 distinct values over 10 rows. The old gate required `d <= rows/2`, so it
    // refused — even though the values are long enough that a dictionary pays
    // comfortably. Nothing here shares a prefix or suffix, so `@dict` is the only
    // encoding that can win.
    let words = [
        "quartzoperations",
        "umbrellaprotocol",
        "javelinsequence",
        "tangerineharbor",
        "viridianlattice",
        "nocturnebasilica",
    ];
    let arr = Value::Array(
        (0..10)
            .map(|i| {
                rec(&[
                    ("tag", Value::Str(words[i % 6].into())),
                    ("n", Value::Int(i as i64)),
                ])
            })
            .collect(),
    );
    let out = encode(&arr);
    assert!(out.contains("@dict tag: "), "dictionary should pay:\n{out}");
    assert_eq!(roundtrip(&arr), arr);
}

#[test]
fn nothing_factorable_costs_nothing_extra() {
    // The cost model must never choose an encoding that inflates the result: with
    // no repeats and no shared runs, every metadata line is pure overhead.
    let arr = Value::Array(
        (0..10)
            .map(|i| {
                rec(&[
                    ("a", Value::Str(format!("{i}x"))),
                    ("b", Value::Int(i as i64)),
                ])
            })
            .collect(),
    );
    let out = encode(&arr);
    assert!(!out.contains('@'), "no metadata line should appear:\n{out}");
    assert_eq!(roundtrip(&arr), arr);
}

#[test]
fn the_new_passes_compose_with_the_existing_ones() {
    // A realistic grouped listing: a constant column, a run-structured one, a
    // path column with both a shared prefix and a shared extension, and an id.
    let arr = Value::Array(
        (0..12)
            .map(|i| {
                let stage = ["build", "build", "build", "test"][(i / 3) as usize % 4];
                rec(&[
                    ("repo", Value::Str("aethershell".into())),
                    ("stage", Value::Str(stage.into())),
                    (
                        "path",
                        Value::Str(format!("/src/aether/module_{i}_impl.rs")),
                    ),
                    ("id", Value::Int(1_700_000_000 + i * 7)),
                ])
            })
            .collect(),
    );
    let out = encode(&arr);
    assert!(out.contains("@const repo=aethershell"), "got: {out}");
    assert!(
        out.contains("@prefix path: /src/aether/module_"),
        "got: {out}"
    );
    assert!(out.contains("@suffix path: _impl.rs"), "got: {out}");
    assert_eq!(roundtrip(&arr), arr, "all passes together must round-trip");
}
