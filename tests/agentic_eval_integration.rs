//! `agentic-eval` applied to AetherShell's real engine: asserts AetherShell scores
//! well on all four agentic axes when measured with the standalone evaluation
//! library against the actual tokenizer / evaluator / canonical renderer / effect
//! model. This is the regression test for the `examples/agentic_eval.rs` wiring.

use aethershell::builtins::{est_token_count, render_canonical};
use aethershell::env::Env;
use aethershell::eval::eval_program;
use aethershell::parser::parse_program;
use aethershell::safety::{self, SafetyError};
use aethershell::value::Value;

use agentic_eval::determinism::assess_determinism;
use agentic_eval::reliability::{assess_reliability, Outcome};
use agentic_eval::safety::{assess_safety_named, Effect, Mode};
use agentic_eval::tokens::{evaluate_with, Program};

fn eval_to_value(code: &str) -> anyhow::Result<Value> {
    let stmts = parse_program(code)?;
    let mut env = Env::new();
    eval_program(&stmts, &mut env)
}

#[test]
fn aethershell_token_surface_is_competitive_over_a_session() {
    // Flow AetherShell's real tokenizer through the library's cost model via
    // `evaluate_with`: legible .ae vs the .aeg cipher (charged its real ontology tax).
    let count = est_token_count;
    let legible = evaluate_with(
        &Program::new("legible", r#"ls(".") | where(fn(f) => f.size > 1000)"#)
            .with_standing_context("ls/where/map are standard names"),
        count,
    );
    let cipher = evaluate_with(
        &Program::new("cipher", r#"l.|w~.size>1k"#)
            .with_standing_context(aethershell::transpile::agentic::describe_ontology())
            .with_retries(1),
        count,
    );
    // Over a session the legible form wins once the cipher's standing-context tax
    // (the cheatsheet it must carry every turn) is counted.
    assert!(
        legible.total_over(30) < cipher.total_over(30),
        "legible {} should beat cipher {} over 30 turns",
        legible.total_over(30),
        cipher.total_over(30)
    );
}

#[test]
fn aethershell_canonical_render_is_deterministic() {
    let det = assess_determinism(6, || {
        let v = eval_to_value(r#"{ b: 2.0, a: 1, items: [3, 1, 2] }"#).expect("eval");
        render_canonical(&v).unwrap_or_default()
    });
    assert!(det.deterministic, "canonical render must be byte-stable");
    assert_eq!(det.distinct, 1);
    // Keys are sorted regardless of source insertion order.
    assert!(
        det.first.starts_with(r#"{"a":1,"b":2"#),
        "got: {}",
        det.first
    );
}

#[test]
fn aethershell_is_reliable_with_actionable_failures() {
    let programs = [
        r#"len([1, 2, 3])"#,
        r#"upper("hi")"#,
        r#"[1, 2, 3] | map(fn(x) => x + 1)"#,
        r#"env(123)"#, // wrong-typed arg → structured E_BAD_ARG
        r#"((("#,      // parse failure → opaque
    ];
    let rel = assess_reliability(&programs, |code| match eval_to_value(code) {
        Ok(_) => Outcome::ok(),
        Err(e) if e.downcast_ref::<SafetyError>().is_some() => Outcome::structured_failure(),
        Err(_) => Outcome::opaque_failure(),
    });
    assert_eq!(rel.passed, 3, "the three valid programs run");
    // env(123) is a structured (catchable) failure, not a dead end.
    assert_eq!(rel.structured_failures, 1);
    // 3 ok + 1 structured = 4/5 are actionable (not dead ends).
    assert!(
        (rel.actionable_rate - 0.8).abs() < 1e-9,
        "got {}",
        rel.actionable_rate
    );
}

#[test]
fn aethershell_bounds_dangerous_builtin_blast_radius_in_agent_mode() {
    let builtins = [
        "len",
        "file_read",
        "file_write",
        "http_get",
        "proc_kill",
        "rm",
        "sh",
    ];
    // Map names → effects through AetherShell's real classifier via the library's
    // `assess_safety_named` convenience (no hand-rolled filter_map).
    let report = assess_safety_named(
        &builtins,
        |b| Effect::from_name(safety::effect_of(b).as_str()),
        Mode::Agent,
    );
    assert_eq!(
        report.effects,
        builtins.len(),
        "every builtin maps to an effect"
    );
    // rm (Destructive), sh (Exec), proc_kill (Process) are the dangerous ones — all
    // gated behind approval under the agent policy, so the blast radius is bounded.
    assert!(
        report.bounded,
        "no dangerous builtin runs ungated in agent mode"
    );
    assert_eq!(report.dangerous_ungated, 0);
    assert_eq!(report.score, 1.0);
    assert_eq!(report.grade, 'A');
    assert_eq!(report.approval_gated, 3);
}

#[test]
fn agentic_eval_policy_stays_in_sync_with_aethershell() {
    // agentic-eval carries its OWN copy of the effect→decision policy (it must not
    // depend on aethershell). The applied safety score is only faithful if that copy
    // matches AetherShell's real policy — so assert they agree for every
    // (effect, mode) pair. This fails loudly if the two ever drift.
    use aethershell::safety as ae;
    use agentic_eval::safety as ag;

    // Ensure the default (non-permissive) policy table is in effect.
    std::env::remove_var("AETHER_POLICY");

    let label_ae = |d: ae::Decision| match d {
        ae::Decision::Allow => "allow",
        ae::Decision::Approve => "approve",
        ae::Decision::Deny => "deny",
    };
    let label_ag = |d: ag::Decision| match d {
        ag::Decision::Allow => "allow",
        ag::Decision::Approve => "approve",
        ag::Decision::Deny => "deny",
    };

    let effects = [
        (ae::Effect::Pure, ag::Effect::Pure),
        (ae::Effect::ReadLocal, ag::Effect::ReadLocal),
        (ae::Effect::WriteLocal, ag::Effect::WriteLocal),
        (ae::Effect::Network, ag::Effect::Network),
        (ae::Effect::Process, ag::Effect::Process),
        (ae::Effect::Destructive, ag::Effect::Destructive),
        (ae::Effect::Exec, ag::Effect::Exec),
        (ae::Effect::Privileged, ag::Effect::Privileged),
    ];
    let modes = [
        (ae::Mode::Human, ag::Mode::Human),
        (ae::Mode::Agent, ag::Mode::Agent),
    ];
    for (ae_e, ag_e) in effects {
        for (ae_m, ag_m) in modes {
            assert_eq!(
                label_ae(ae::decide(ae_e, ae_m)),
                label_ag(ag::decide(ag_e, ag_m)),
                "policy drift for effect {} in this mode",
                ag_e.name()
            );
        }
    }
}
