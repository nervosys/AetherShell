//! Repair rate: when a call fails, can the *error alone* fix it?
//!
//! [`reliability`](crate::reliability) asks whether a failure was actionable —
//! whether it carried a code an agent could branch on. That is a property of the
//! error. This module asks the harder question, which is a property of the whole
//! loop: replaying the call with **nothing but the error record** as new context,
//! does the next attempt succeed?
//!
//! The distinction matters because "actionable" is cheap to claim and easy to get
//! wrong. An error can carry a stable code, a confident hint and a suggestion that
//! is simply not correct — it looks actionable at every layer and still leads the
//! agent into a second failure. Only replaying the repair distinguishes the two,
//! which is why the measurement takes a closure that actually re-runs the call
//! rather than a classification supplied by the caller.
//!
//! The harness is execution-agnostic: the caller supplies `attempt` (run a case)
//! and `repair` (propose a corrected case from the error facts). A repair strategy
//! may be mechanical — substitute the first `did_you_mean` candidate — or a model
//! call; the harness scores either the same way.

/// The machine-readable facts an agent gets back from a failed attempt. This is
/// deliberately *only* what a structured error carries: a repair strategy that
/// needs more than this is not repairing from the error.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Default)]
pub struct ErrorFacts {
    /// Stable code (e.g. `E_BAD_ARG`). Empty when the failure was uncoded.
    pub code: String,
    /// Whether the producer believes a corrected retry can succeed.
    pub retryable: bool,
    /// Candidate corrections, if the error offered any.
    pub suggestions: Vec<String>,
}

impl ErrorFacts {
    /// An uncoded failure — the dead-end case self-healing has to eliminate.
    pub fn uncoded() -> Self {
        Self::default()
    }

    /// Whether these facts give a strategy anything to act on at all.
    pub fn is_actionable(&self) -> bool {
        !self.code.is_empty() && self.retryable
    }
}

/// The result of running one attempt.
#[derive(Debug, Clone)]
pub enum Attempt {
    /// The call succeeded.
    Ok,
    /// The call failed, carrying these facts.
    Failed(ErrorFacts),
}

/// What happened to one case over the attempt → repair → retry cycle.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairOutcome {
    /// The case never failed; nothing to repair. Excluded from the repair rate,
    /// since counting it would inflate the score with cases that prove nothing.
    NeverFailed,
    /// Failed, and the retry built from the error succeeded. The win condition.
    Repaired,
    /// Failed with actionable facts, but the retry still failed. The error
    /// *looked* repairable and was not — the case this measurement exists to
    /// catch, and the one a claim of "structured errors ⇒ self-correction"
    /// silently assumes away.
    MisleadingError,
    /// Failed with facts offering nothing to act on (uncoded, or explicitly not
    /// retryable). An honest dead end, not a wrong answer.
    DeadEnd,
    /// Failed actionably, but the strategy declined to propose a repair.
    NoRepairProposed,
}

/// Aggregate repair performance over a corpus of deliberately-broken calls.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct RepairReport {
    /// Cases assessed.
    pub total: usize,
    /// Cases that failed on the first attempt (the denominator that matters).
    pub failed_first: usize,
    /// Failures fixed by a retry built from the error alone.
    pub repaired: usize,
    /// Failures whose error implied a fix that did not work.
    pub misleading: usize,
    /// Failures that offered nothing to act on.
    pub dead_ends: usize,
    /// Failures where the strategy proposed nothing.
    pub no_repair_proposed: usize,
    /// `repaired / failed_first` — the number §11's self-correction metric wants.
    /// 1.0 for a corpus in which nothing failed (vacuous; check `failed_first`).
    pub repair_rate: f64,
    /// Fraction of failures that carried actionable facts, whether or not the
    /// repair then worked. The gap between this and `repair_rate` is precisely
    /// the misleading-error rate — errors that pass every structural check and
    /// still send the agent the wrong way.
    pub actionable_rate: f64,
    /// Per-case outcomes, in corpus order.
    pub outcomes: Vec<RepairOutcome>,
}

/// Run each case, and on failure ask `repair` for a corrected case built from the
/// error facts, then re-run it.
///
/// `repair` returns `None` to decline. It receives only [`ErrorFacts`] alongside
/// the original case — by construction it cannot consult anything the agent would
/// not have had.
pub fn assess_repair<I>(
    cases: &[I],
    attempt: impl Fn(&I) -> Attempt,
    repair: impl Fn(&I, &ErrorFacts) -> Option<I>,
) -> RepairReport {
    let mut outcomes = Vec::with_capacity(cases.len());
    let (mut failed_first, mut repaired, mut misleading) = (0usize, 0usize, 0usize);
    let (mut dead_ends, mut no_repair, mut actionable) = (0usize, 0usize, 0usize);

    for case in cases {
        let outcome = match attempt(case) {
            Attempt::Ok => RepairOutcome::NeverFailed,
            Attempt::Failed(facts) => {
                failed_first += 1;
                if !facts.is_actionable() {
                    dead_ends += 1;
                    RepairOutcome::DeadEnd
                } else {
                    actionable += 1;
                    match repair(case, &facts) {
                        None => {
                            no_repair += 1;
                            RepairOutcome::NoRepairProposed
                        }
                        Some(fixed) => match attempt(&fixed) {
                            Attempt::Ok => {
                                repaired += 1;
                                RepairOutcome::Repaired
                            }
                            Attempt::Failed(_) => {
                                misleading += 1;
                                RepairOutcome::MisleadingError
                            }
                        },
                    }
                }
            }
        };
        outcomes.push(outcome);
    }

    let (repair_rate, actionable_rate) = if failed_first == 0 {
        (1.0, 1.0)
    } else {
        (
            repaired as f64 / failed_first as f64,
            actionable as f64 / failed_first as f64,
        )
    };

    RepairReport {
        total: cases.len(),
        failed_first,
        repaired,
        misleading,
        dead_ends,
        no_repair_proposed: no_repair,
        repair_rate,
        actionable_rate,
        outcomes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_corpus_that_never_fails_reports_a_vacuous_rate() {
        let r = assess_repair(&[1, 2, 3], |_| Attempt::Ok, |_, _| None);
        assert_eq!(r.failed_first, 0);
        assert_eq!(r.repair_rate, 1.0);
        assert!(r.outcomes.iter().all(|o| *o == RepairOutcome::NeverFailed));
    }

    #[test]
    fn an_uncoded_failure_is_a_dead_end_not_a_repair_opportunity() {
        let r = assess_repair(
            &[0],
            |_| Attempt::Failed(ErrorFacts::uncoded()),
            |_, _| Some(0),
        );
        assert_eq!(r.dead_ends, 1);
        assert_eq!(r.repair_rate, 0.0);
        // The strategy was never consulted — there was nothing to consult it with.
        assert_eq!(r.no_repair_proposed, 0);
    }

    /// The case the whole module exists for: an error that is structurally
    /// perfect and substantively wrong scores as `MisleadingError`, and drags
    /// `repair_rate` below `actionable_rate` where it is visible.
    #[test]
    fn a_confidently_wrong_suggestion_scores_as_misleading_not_actionable() {
        let facts = ErrorFacts {
            code: "E_UNKNOWN_BUILTIN".into(),
            retryable: true,
            suggestions: vec!["still_wrong".into()],
        };
        let r = assess_repair(
            &[0],
            move |_| Attempt::Failed(facts.clone()),
            |_, f| f.suggestions.first().map(|_| 0),
        );
        assert_eq!(r.misleading, 1);
        assert_eq!(r.repair_rate, 0.0);
        assert_eq!(
            r.actionable_rate, 1.0,
            "it looked actionable at every layer"
        );
    }

    #[test]
    fn a_correct_suggestion_scores_as_repaired() {
        let r = assess_repair(
            &[0i32, 1],
            |c| {
                if *c == 0 {
                    Attempt::Failed(ErrorFacts {
                        code: "E_UNKNOWN_BUILTIN".into(),
                        retryable: true,
                        suggestions: vec!["1".into()],
                    })
                } else {
                    Attempt::Ok
                }
            },
            |_, f| f.suggestions.first().and_then(|s| s.parse().ok()),
        );
        assert_eq!(r.repaired, 1);
        assert_eq!(r.failed_first, 1);
        assert_eq!(r.repair_rate, 1.0);
    }
}
