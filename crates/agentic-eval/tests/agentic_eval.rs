//! Integration tests exercising the public API across all four axes the library
//! evaluates for agentic AI use: token efficiency, determinism, reliability, and
//! safety — plus the combined `Evaluation`.

use agentic_eval::determinism::{assess_determinism, DeterminismReport};
use agentic_eval::reliability::{assess_reliability, Outcome, ReliabilityReport};
use agentic_eval::safety::{assess_safety, Effect, Mode, SafetyReport};
use agentic_eval::tokens::{compare, evaluate, evaluate_all, AgentCost, Model, Program};
use agentic_eval::Evaluation;
use std::cell::Cell;

// ── Token efficiency ──────────────────────────────────────────────────────────

#[test]
fn token_efficiency_counts_all_models_and_compares() {
    let p = Program::new("read", "file.read(\"README.md\")")
        .with_output("hello world")
        .with_standing_context("file.read(path) -> String");

    // Every supported model produces a cost with a non-zero input term.
    for (model, cost) in evaluate_all(&p) {
        assert!(cost.input > 0, "{} should count input", model.name());
        assert!(cost.output > 0);
        assert!(cost.standing_context > 0);
    }

    // A cipher with a heavy standing-context tax loses to a legible form over a
    // session even though its per-call input is smaller — the core finding.
    let cipher = Program::new("t", "F.r x")
        .with_standing_context("cipher cheatsheet line; ".repeat(200).as_str());
    let legible = Program::new("t", "file.read x").with_standing_context("one short line");
    let cmp = compare(&legible, &cipher, Model::OpenAiGpt4, 30);
    assert!(
        cmp.winner_is_a,
        "legible wins once standing context is counted"
    );
    assert!(cmp.ratio >= 1.0);

    // Single-shot vs amortized totals differ as expected.
    let c = evaluate(&p, Model::Heuristic);
    assert!(c.total_over(10) > c.total_over(1));
}

// ── Determinism ───────────────────────────────────────────────────────────────

#[test]
fn determinism_distinguishes_stable_from_varying_output() {
    // A deterministic renderer: same bytes every run.
    let stable: DeterminismReport = assess_determinism(8, || "a\tb\n1\t2".to_string());
    assert!(stable.deterministic);
    assert_eq!(stable.distinct, 1);

    // A renderer that embeds a changing timestamp: flagged non-deterministic.
    let tick = Cell::new(0u64);
    let varying = assess_determinism(8, || {
        let t = tick.get();
        tick.set(t + 1);
        format!("data ts={t}")
    });
    assert!(!varying.deterministic);
    assert_eq!(varying.distinct, 8);
}

// ── Reliability ───────────────────────────────────────────────────────────────

#[test]
fn reliability_tracks_pass_rate_and_actionable_failures() {
    // Simulate a program over 5 representative invocations: 3 succeed, 1 fails with
    // a structured (branchable) error, 1 fails opaquely.
    let cases = [0u8, 1, 2, 3, 4];
    let r: ReliabilityReport = assess_reliability(&cases, |&i| match i {
        0..=2 => Outcome::ok(),
        3 => Outcome::structured_failure(),
        _ => Outcome::opaque_failure(),
    });
    assert_eq!(r.total, 5);
    assert_eq!(r.passed, 3);
    assert_eq!(r.structured_failures, 1);
    assert!((r.pass_rate - 0.6).abs() < 1e-9);
    // 3 passed + 1 structured failure = 4/5 were not dead ends for self-correction.
    assert!((r.actionable_rate - 0.8).abs() < 1e-9);
}

// ── Safety ────────────────────────────────────────────────────────────────────

#[test]
fn safety_scores_blast_radius_gating_under_agent_policy() {
    // A program that reads, writes, deletes, and execs.
    let effects = [
        Effect::ReadLocal,
        Effect::WriteLocal,
        Effect::Destructive,
        Effect::Exec,
    ];
    let agent: SafetyReport = assess_safety(&effects, Mode::Agent);
    assert!(agent.bounded, "agent policy gates every dangerous effect");
    assert_eq!(agent.score, 1.0);
    assert_eq!(agent.grade, 'A');
    assert_eq!(agent.approval_gated, 2); // destructive + exec

    // The same effects under human mode are ungated → unbounded blast radius.
    let human = assess_safety(&effects, Mode::Human);
    assert!(!human.bounded);
    assert_eq!(human.grade, 'F');
}

// ── Combined evaluation ───────────────────────────────────────────────────────

#[test]
fn combined_evaluation_reports_overall_fitness() {
    let mut eval = Evaluation::new("file.read");
    eval.tokens = Some(AgentCost {
        standing_context: 20,
        input: 6,
        output: 12,
        retries: 0,
    });
    eval.determinism = Some(assess_determinism(3, || "stable".to_string()));
    eval.reliability = Some(assess_reliability(&[(), (), ()], |_| Outcome::ok()));
    eval.safety = Some(assess_safety(&[Effect::ReadLocal], Mode::Agent));

    // deterministic (1.0) + reliability (1.0) + safety (1.0) → fitness 1.0.
    let fitness = eval.fitness().expect("axes measured");
    assert!((fitness - 1.0).abs() < 1e-9, "got {fitness}");
}
