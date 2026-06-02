//! Determinism: does a program produce byte-identical output across runs?
//!
//! Agents parse, cache, and diff program output. Non-deterministic output (locale
//! dates, hash-ordered maps, terminal-width-dependent tables, embedded timestamps)
//! breaks all three: the agent can't reuse a cache, a diff is all noise, and a
//! parser keyed on column positions desyncs. This module runs an output-producer
//! repeatedly and reports whether every run was identical.

use std::collections::BTreeSet;

/// The result of a determinism assessment.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct DeterminismReport {
    /// How many times the producer was run.
    pub runs: usize,
    /// Number of *distinct* outputs observed (1 = fully deterministic).
    pub distinct: usize,
    /// True iff every run produced byte-identical output.
    pub deterministic: bool,
    /// The first output (representative sample).
    pub first: String,
}

/// Run `produce` `runs` times and report whether all outputs are byte-identical.
/// `runs` is clamped to ≥ 2 (one run can't show non-determinism).
pub fn assess_determinism(runs: usize, mut produce: impl FnMut() -> String) -> DeterminismReport {
    let runs = runs.max(2);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut first = String::new();
    for i in 0..runs {
        let out = produce();
        if i == 0 {
            first = out.clone();
        }
        seen.insert(out);
    }
    DeterminismReport {
        runs,
        distinct: seen.len(),
        deterministic: seen.len() == 1,
        first,
    }
}

impl std::fmt::Display for DeterminismReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({} distinct / {} runs)",
            if self.deterministic {
                "deterministic"
            } else {
                "NON-deterministic"
            },
            self.distinct,
            self.runs
        )
    }
}

/// Whether two renderings of the *same* value are byte-identical — the
/// cross-representation determinism check (e.g. does a canonical encoder produce
/// the same bytes regardless of input key order?).
pub fn stable_across(a: &str, b: &str) -> bool {
    a == b
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn constant_output_is_deterministic() {
        let r = assess_determinism(5, || "name\tsize\nfoo\t10".to_string());
        assert!(r.deterministic);
        assert_eq!(r.distinct, 1);
        assert_eq!(r.runs, 5);
    }

    #[test]
    fn varying_output_is_flagged_nondeterministic() {
        let n = Cell::new(0);
        // Simulates an embedded timestamp / counter that changes each run.
        let r = assess_determinism(4, || {
            let v = n.get();
            n.set(v + 1);
            format!("rows=3 generated_at={v}")
        });
        assert!(!r.deterministic);
        assert_eq!(r.distinct, 4);
    }

    #[test]
    fn single_run_is_clamped_to_two() {
        let r = assess_determinism(1, || "x".to_string());
        assert_eq!(r.runs, 2);
        assert!(r.deterministic);
    }

    #[test]
    fn stable_across_compares_bytes() {
        assert!(stable_across(r#"{"a":1,"b":2}"#, r#"{"a":1,"b":2}"#));
        // Different key order → not byte-stable (a canonical encoder should avoid).
        assert!(!stable_across(r#"{"a":1,"b":2}"#, r#"{"b":2,"a":1}"#));
    }
}
