//! Reliability: does a program parse/run without ambiguity, and when it fails, is
//! the failure *actionable*?
//!
//! Two things drive an agent's retry-token blowup. First, ambiguity: a form the
//! model mis-emits and has to redo. Second, dead-end errors: prose like
//! `"error near line 3"` tells the model nothing to branch on, so it guesses.
//! This module aggregates the outcomes of running a program over a set of
//! representative invocations into a pass rate and an *actionable-failure* rate
//! (failures that carry a structured, machine-branchable code + hint).

/// The outcome of one invocation, as classified by the caller.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy)]
pub struct Outcome {
    /// Did the invocation succeed (parse + run without error)?
    pub ok: bool,
    /// If it failed, was the error *structured/actionable* (stable code + hint an
    /// agent can branch on) rather than opaque prose? Ignored when `ok`.
    pub structured_error: bool,
}

impl Outcome {
    /// A successful invocation.
    pub fn ok() -> Self {
        Self {
            ok: true,
            structured_error: false,
        }
    }
    /// A failure carrying a structured, actionable error.
    pub fn structured_failure() -> Self {
        Self {
            ok: false,
            structured_error: true,
        }
    }
    /// A failure with only opaque prose (a dead end for self-correction).
    pub fn opaque_failure() -> Self {
        Self {
            ok: false,
            structured_error: false,
        }
    }
}

/// Aggregate reliability over a set of invocations.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct ReliabilityReport {
    /// Number of invocations assessed.
    pub total: usize,
    /// Number that succeeded.
    pub passed: usize,
    /// Number that failed (`total - passed`).
    pub failed: usize,
    /// Of the failures, how many carried a structured/actionable error.
    pub structured_failures: usize,
    /// `passed / total` (1.0 = never fails). 1.0 for an empty set.
    pub pass_rate: f64,
    /// Fraction of *all* cases that either passed or failed actionably — i.e. the
    /// agent was never left at a dead end. 1.0 for an empty set.
    pub actionable_rate: f64,
}

/// Assess reliability by running `run` over each case and aggregating outcomes.
pub fn assess_reliability<I>(cases: &[I], run: impl Fn(&I) -> Outcome) -> ReliabilityReport {
    let total = cases.len();
    let mut passed = 0usize;
    let mut structured_failures = 0usize;
    for case in cases {
        let o = run(case);
        if o.ok {
            passed += 1;
        } else if o.structured_error {
            structured_failures += 1;
        }
    }
    let failed = total - passed;
    let (pass_rate, actionable_rate) = if total == 0 {
        (1.0, 1.0)
    } else {
        (
            passed as f64 / total as f64,
            (passed + structured_failures) as f64 / total as f64,
        )
    };
    ReliabilityReport {
        total,
        passed,
        failed,
        structured_failures,
        pass_rate,
        actionable_rate,
    }
}

impl std::fmt::Display for ReliabilityReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "pass {:.0}% actionable {:.0}% ({}/{} ok, {} structured failures)",
            self.pass_rate * 100.0,
            self.actionable_rate * 100.0,
            self.passed,
            self.total,
            self.structured_failures
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_pass_is_perfect() {
        let cases = ["a", "b", "c"];
        let r = assess_reliability(&cases, |_| Outcome::ok());
        assert_eq!(r.passed, 3);
        assert_eq!(r.failed, 0);
        assert_eq!(r.pass_rate, 1.0);
        assert_eq!(r.actionable_rate, 1.0);
    }

    #[test]
    fn structured_failures_are_actionable_even_when_not_passing() {
        // 2 pass, 1 structured failure, 1 opaque failure of 4.
        let cases = [0, 1, 2, 3];
        let r = assess_reliability(&cases, |&i| match i {
            0 | 1 => Outcome::ok(),
            2 => Outcome::structured_failure(),
            _ => Outcome::opaque_failure(),
        });
        assert_eq!(r.passed, 2);
        assert_eq!(r.failed, 2);
        assert_eq!(r.structured_failures, 1);
        assert_eq!(r.pass_rate, 0.5);
        // passed (2) + structured failure (1) = 3/4 were not dead ends.
        assert_eq!(r.actionable_rate, 0.75);
    }

    #[test]
    fn empty_set_is_vacuously_reliable() {
        let cases: [&str; 0] = [];
        let r = assess_reliability(&cases, |_| Outcome::ok());
        assert_eq!(r.pass_rate, 1.0);
        assert_eq!(r.actionable_rate, 1.0);
    }
}
