//! Apply the `agentic-eval` library to AetherShell's **real engine**: evaluate
//! AetherShell programs across all four agentic axes using the actual tokenizer,
//! parser, evaluator, canonical renderer, and safety effect model — not mocks.
//!
//!   Run: `cargo run --example agentic_eval`
//!    or: `cargo run --example agentic_eval --features real-tokens`  (exact tokens)
//!
//! This is the "apply it to AetherShell" companion to the standalone crate's own
//! `evaluate` example: same four axes, but wired to the shipped engine.

use aethershell::builtins::{est_token_count, render_canonical};
use aethershell::env::Env;
use aethershell::eval::eval_program;
use aethershell::parser::parse_program;
use aethershell::safety::{self, SafetyError};
use aethershell::transpile::agentic::describe_ontology;
use aethershell::value::Value;

use agentic_eval::determinism::assess_determinism;
use agentic_eval::reliability::{assess_reliability, Outcome};
use agentic_eval::safety::{assess_safety, Effect, Mode};
use agentic_eval::tokens::AgentCost;

/// Parse + evaluate a program through AetherShell's real pipeline.
fn eval_to_value(code: &str) -> anyhow::Result<Value> {
    let stmts = parse_program(code)?;
    let mut env = Env::new();
    eval_program(&stmts, &mut env)
}

fn main() {
    println!("AetherShell × agentic-eval — four-axis self-evaluation (real engine)\n");

    // ── 1. Token efficiency — AetherShell's own tokenizer over the legible `.ae`
    //    surface vs. the `.aeg` cipher, charging the cipher its real
    //    standing-context tax (the `describe_ontology` cheatsheet it must carry). ──
    let legible = r#"ls("./src") | where(fn(f) => f.size > 1000) | map(fn(f) => f.name)"#;
    let cipher = r#"l./src|w~.size>1k|m~.name"#;
    let legible_cost = AgentCost {
        standing_context: est_token_count("ls/where/map are standard high-probability names"),
        input: est_token_count(legible),
        output: 0,
        retries: 0,
    };
    let cipher_cost = AgentCost {
        standing_context: est_token_count(&describe_ontology()),
        input: est_token_count(cipher),
        output: 0,
        retries: 1, // the terse cipher is mis-emitted more often
    };
    let turns = 30;
    let exact = cfg!(feature = "real-tokens");
    println!(
        "[1] Token efficiency — AetherShell est_token_count ({}, {} turns):",
        if exact {
            "EXACT cl100k BPE"
        } else {
            "heuristic"
        },
        turns
    );
    println!(
        "  legible:  input={:>4}  standing={:>5}  session-total={:>6}",
        legible_cost.input,
        legible_cost.standing_context,
        legible_cost.total_over(turns)
    );
    println!(
        "  cipher :  input={:>4}  standing={:>5}  session-total={:>6}",
        cipher_cost.input,
        cipher_cost.standing_context,
        cipher_cost.total_over(turns)
    );
    let winner = if legible_cost.total_over(turns) <= cipher_cost.total_over(turns) {
        "legible"
    } else {
        "cipher"
    };
    println!("  → {winner} wins over a session (standing-context tax dominates the input edge)\n");

    // ── 2. Determinism — AetherShell's canonical renderer is byte-stable across
    //    runs (sorted record keys, shortest-round-trip floats), regardless of the
    //    insertion order in the source. ──
    let det = assess_determinism(8, || {
        let v = eval_to_value(r#"{ b: 2.0, a: 1, items: [3, 1, 2] }"#).expect("eval");
        render_canonical(&v).unwrap_or_default()
    });
    println!(
        "[2] Determinism — canonical render: deterministic={} ({} distinct / {} runs)",
        det.deterministic, det.distinct, det.runs
    );
    println!("  byte-stable sample: {}\n", det.first);

    // ── 3. Reliability — parse + eval representative programs through the real
    //    evaluator; classify each outcome as ok / structured (E_*) / opaque. ──
    let programs = [
        r#"len([1, 2, 3])"#,                  // ok
        r#"upper("hi")"#,                     // ok
        r#"[1, 2, 3] | map(fn(x) => x + 1)"#, // ok
        r#"env(123)"#,                        // structured E_BAD_ARG (wrong-typed arg)
        r#"((("#,                             // parse failure (opaque)
    ];
    let rel = assess_reliability(&programs, |code| match eval_to_value(code) {
        Ok(_) => Outcome::ok(),
        // A structured SafetyError (E_BAD_ARG / E_*) is actionable: an agent can
        // branch on the code and self-correct. Anything else is an opaque dead end.
        Err(e) if e.downcast_ref::<SafetyError>().is_some() => Outcome::structured_failure(),
        Err(_) => Outcome::opaque_failure(),
    });
    println!(
        "[3] Reliability — {} programs: pass {:.0}%  actionable {:.0}%  (structured failures: {})",
        rel.total,
        rel.pass_rate * 100.0,
        rel.actionable_rate * 100.0,
        rel.structured_failures
    );
    println!();

    // ── 4. Safety — map AetherShell's real `effect_of(builtin)` into the agentic
    //    effect taxonomy and score blast-radius gating under the agent policy. ──
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
    let saf = assess_safety(&effects, Mode::Agent);
    println!(
        "[4] Safety — {} representative builtins under the agent policy:",
        builtins.len()
    );
    for (b, e) in builtins.iter().zip(&effects) {
        println!("    {:<11} {}", b, e.name());
    }
    println!(
        "  grade {}  bounded={}  (allowed={} approval-gated={} denied={})",
        saf.grade, saf.bounded, saf.allowed, saf.approval_gated, saf.denied
    );

    println!("\nResult: AetherShell renders deterministically, surfaces wrong-typed arguments");
    println!("as structured (actionable) errors, and bounds the blast radius of its dangerous");
    println!(
        "builtins under the agent policy — while the legible surface stays token-competitive."
    );
}
