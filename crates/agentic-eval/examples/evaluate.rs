//! Demonstrate agentic-eval across all four axes on two encodings of the same
//! task: a legible form vs. a terse "cipher" form.
//!
//! Run: `cargo run -p agentic-eval --example evaluate`
//!  or: `cargo run -p agentic-eval --example evaluate --features real-tokens`

use agentic_eval::determinism::assess_determinism;
use agentic_eval::reliability::{assess_reliability, Outcome};
use agentic_eval::safety::{assess_safety, Effect, Mode};
use agentic_eval::tokens::{compare, Model, Program};

fn main() {
    println!("agentic-eval — four-axis evaluation of programs for agentic AI\n");

    // Two encodings of "read a file and keep the large entries".
    let legible = Program::new(
        "legible",
        r#"ls("./src") | where(fn(f) => f.size > 1000) | map(fn(f) => f.name)"#,
    )
    .with_standing_context("ls/where/map are standard, high-probability names")
    .with_output("name\nfoo.rs\nbar.rs");
    let cipher = Program::new("cipher", r#"l./src|w~.size>1k|m~.name"#)
        .with_standing_context(&"single-letter+sigil cheatsheet line; ".repeat(120))
        .with_output("name\nfoo.rs\nbar.rs")
        .with_retries(8); // terse cipher is mis-emitted more often

    // ── 1. Token efficiency ──────────────────────────────────────────────
    println!("[1] Token efficiency (amortized over 30 turns):");
    for model in [
        Model::OpenAiGpt4,
        Model::OpenAiGpt4o,
        Model::AnthropicClaude,
    ] {
        let cmp = compare(&legible, &cipher, model, 30);
        println!(
            "  {:<28} legible={:>6}  cipher={:>6}  → {} wins ({:.2}x){}",
            model.name(),
            cmp.a_total,
            cmp.b_total,
            if cmp.winner_is_a { "legible" } else { "cipher" },
            cmp.ratio,
            if model.is_exact() { "" } else { " [est]" },
        );
    }

    // ── 2. Determinism ───────────────────────────────────────────────────
    // A canonical renderer (byte-stable) vs. one that embeds a timestamp.
    let canonical = assess_determinism(5, || "name\nfoo.rs\nbar.rs".to_string());
    let mut t = 0u64;
    let noisy = assess_determinism(5, || {
        t += 1;
        format!("name\nfoo.rs\nbar.rs  # at {t}")
    });
    println!("\n[2] Determinism:");
    println!(
        "  canonical output : deterministic={} ({} distinct / {} runs)",
        canonical.deterministic, canonical.distinct, canonical.runs
    );
    println!(
        "  timestamped output: deterministic={} ({} distinct / {} runs)",
        noisy.deterministic, noisy.distinct, noisy.runs
    );

    // ── 3. Reliability ───────────────────────────────────────────────────
    // The legible form parses on all 6 sample invocations; the cipher mis-parses
    // twice but at least returns a structured error once.
    let samples = [0, 1, 2, 3, 4, 5];
    let legible_rel = assess_reliability(&samples, |_| Outcome::ok());
    let cipher_rel = assess_reliability(&samples, |&i| match i {
        4 => Outcome::structured_failure(),
        5 => Outcome::opaque_failure(),
        _ => Outcome::ok(),
    });
    println!("\n[3] Reliability:");
    println!(
        "  legible: pass {:.0}%  actionable {:.0}%",
        legible_rel.pass_rate * 100.0,
        legible_rel.actionable_rate * 100.0
    );
    println!(
        "  cipher : pass {:.0}%  actionable {:.0}%",
        cipher_rel.pass_rate * 100.0,
        cipher_rel.actionable_rate * 100.0
    );

    // ── 4. Safety ────────────────────────────────────────────────────────
    // The task reads + lists (ReadLocal); a "rm the small files" variant adds a
    // Destructive effect. Score the gating under the agent policy.
    let read_only = assess_safety(&[Effect::ReadLocal, Effect::WriteLocal], Mode::Agent);
    let destructive = assess_safety(
        &[Effect::ReadLocal, Effect::Destructive, Effect::Exec],
        Mode::Agent,
    );
    println!("\n[4] Safety (agent policy):");
    println!(
        "  read+write task : grade {} (bounded={}, {} approval-gated)",
        read_only.grade, read_only.bounded, read_only.approval_gated
    );
    println!(
        "  rm+exec task    : grade {} (bounded={}, {} approval-gated, {} denied)",
        destructive.grade, destructive.bounded, destructive.approval_gated, destructive.denied
    );

    println!("\nSummary: the legible form is competitive-or-cheaper on tokens once standing");
    println!("context counts, more deterministic and reliable to parse, and the agent policy");
    println!("bounds the blast radius of even the destructive variant.");
}
