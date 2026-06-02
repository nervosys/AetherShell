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
use agentic_eval::safety::{assess_safety, Effect, Mode};
use agentic_eval::tokens::AgentCost;

fn eval_to_value(code: &str) -> anyhow::Result<Value> {
    let stmts = parse_program(code)?;
    let mut env = Env::new();
    eval_program(&stmts, &mut env)
}

#[test]
fn aethershell_token_surface_is_competitive_over_a_session() {
    // Legible .ae vs .aeg cipher, charged the cipher its real ontology standing tax.
    let legible = AgentCost {
        standing_context: est_token_count("ls/where/map are standard names"),
        input: est_token_count(r#"ls(".") | where(fn(f) => f.size > 1000)"#),
        output: 0,
        retries: 0,
    };
    let cipher = AgentCost {
        standing_context: est_token_count(&aethershell::transpile::agentic::describe_ontology()),
        input: est_token_count(r#"l.|w~.size>1k"#),
        output: 0,
        retries: 1,
    };
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
    let effects: Vec<Effect> = builtins
        .iter()
        .filter_map(|b| Effect::from_name(safety::effect_of(b).as_str()))
        .collect();
    assert_eq!(effects.len(), builtins.len(), "every effect name maps");

    let report = assess_safety(&effects, Mode::Agent);
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
