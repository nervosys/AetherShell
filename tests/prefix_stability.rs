//! Cross-turn prefix stability — measuring the one big README number that was
//! reasoned about rather than measured.
//!
//! The claim: a ~90%-stable prompt prefix is ~4.1× cheaper over 20 turns, and
//! deterministic output is what keeps the prefix stable. That is a property of how
//! provider prompt-caching bills repeated prefixes, so it is checkable: replay a
//! realistic multi-turn session, count what a cache would and would not charge for,
//! and report the ratio.
//!
//! What this measures is the *shell's* contribution — whether AECON output is
//! byte-stable across turns for unchanged data, and therefore cacheable at all.
//! Provider cache-hit pricing is a modelled multiplier, stated as an assumption
//! below rather than smuggled into the result.

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

/// A service listing at turn `t`: 20 rows of which one changes per turn — a
/// deliberately ordinary agent workload (poll a resource, watch one field move).
fn listing(turn: usize) -> Value {
    Value::Array(
        (0..20)
            .map(|i| {
                let changed = i == turn % 20;
                rec(&[
                    ("name", Value::Str(format!("svc-{i:02}"))),
                    ("region", Value::Str("us-west-2".into())),
                    (
                        "state",
                        Value::Str(if changed { "restarting" } else { "running" }.into()),
                    ),
                    ("port", Value::Int(8000 + i as i64)),
                ])
            })
            .collect(),
    )
}

/// Length of the shared leading run of two strings, in bytes.
fn shared_prefix_len(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

#[test]
fn identical_data_renders_byte_identically() {
    // The precondition for any cross-turn caching at all. If the encoder were
    // non-deterministic — map iteration order, a timestamp, a float format — every
    // turn would miss the cache and the rest of this file would be meaningless.
    let a = encode(&listing(3));
    for _ in 0..8 {
        assert_eq!(encode(&listing(3)), a, "encoding must be byte-stable");
    }
}

#[test]
fn an_unchanged_turn_is_a_total_cache_hit() {
    let a = encode(&listing(3));
    let b = encode(&listing(3));
    assert_eq!(
        shared_prefix_len(&a, &b),
        a.len(),
        "an unchanged result must share its whole prefix"
    );
}

#[test]
fn report_prefix_stability_over_a_twenty_turn_session() {
    const TURNS: usize = 20;
    // Modelled, not measured here: providers bill a cached prefix token at a
    // fraction of an uncached one. 0.1 is the common published figure. The ratio
    // below scales linearly with this, so it is stated rather than hidden.
    const CACHED_RATE: f64 = 0.1;

    let mut uncached = 0usize;
    let mut billed = 0.0f64;
    let mut prev: Option<String> = None;
    let mut stability: Vec<f64> = Vec::new();

    for t in 0..TURNS {
        let out = encode(&listing(t));
        let total = tokens(&out);
        uncached += total;

        match &prev {
            None => billed += total as f64,
            Some(p) => {
                let shared_bytes = shared_prefix_len(p, &out);
                // Convert the shared byte run into a token count by re-tokenizing
                // exactly that slice, so the figure stays in the same units.
                let shared = tokens(&out[..shared_bytes]);
                let shared = shared.min(total);
                stability.push(shared as f64 / total as f64);
                billed += shared as f64 * CACHED_RATE + (total - shared) as f64;
            }
        }
        prev = Some(out);
    }

    let mean_stability = stability.iter().sum::<f64>() / stability.len() as f64;
    let ratio = uncached as f64 / billed;
    println!(
        "20 turns · mean prefix stability {:.1}% · uncached {} tok · cache-billed {:.0} tok · {:.2}x cheaper (cached rate {})",
        mean_stability * 100.0,
        uncached,
        billed,
        ratio,
        CACHED_RATE
    );

    // The load-bearing claim is directional and modest: a mostly-unchanged result
    // must retain most of its prefix, so caching has something to bite on. The
    // headline multiplier is reported, not asserted — it depends on the provider's
    // rate, and pinning it here would turn an assumption into a fake measurement.
    assert!(
        mean_stability > 0.5,
        "prefix stability collapsed to {:.1}% — caching would not help",
        mean_stability * 100.0
    );
    assert!(ratio > 1.0, "caching must not cost more than not caching");
}

#[test]
fn a_reordered_result_destroys_the_prefix() {
    // The honest counterweight: stability is a property of *deterministic ordering*,
    // not of the format. If a builtin returned rows in nondeterministic order, the
    // shared prefix would collapse and the caching benefit with it. This documents
    // what the benefit depends on.
    let a = encode(&listing(3));
    let reversed = match listing(3) {
        Value::Array(mut rows) => {
            rows.reverse();
            Value::Array(rows)
        }
        other => other,
    };
    let b = encode(&reversed);
    let shared = shared_prefix_len(&a, &b);
    assert!(
        shared < a.len() / 2,
        "reordering should cost most of the prefix, shared {shared} of {}",
        a.len()
    );
}
