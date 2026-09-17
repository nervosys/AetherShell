//! Rank shells for agentic AI use, best-first by composite fitness.
//!
//!   cargo run -p agentic-eval --example shell_benchmark
//!
//! The scores are curated, but each is anchored to an executed run in
//! `benches/agentic/` — see the module docs for the mapping. Rows that were
//! never executed say so, in the table and again in their evidence.

use agentic_eval::shells::{compare_shells, profile, rank_shells, Shell};

fn main() {
    println!("Shells for agentic AI use — ranked by composite fitness");
    println!("Axes anchored to benches/agentic/ (E1–E6); see shells.rs for the mapping.\n");

    println!(
        "{:<20} {:>8} {:>8} {:>8} {:>8} {:>8}  {}",
        "shell", "fitness", "tokens", "determ.", "reliab.", "safety", "basis"
    );
    println!("{}", "-".repeat(78));
    for p in rank_shells() {
        println!(
            "{:<20} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>8.2}  {}",
            p.shell.name(),
            p.fitness(),
            p.token_efficiency,
            p.determinism,
            p.reliability,
            p.safety,
            if p.measured {
                "executed"
            } else {
                "ANALOGY to bash"
            }
        );
    }

    // The head-to-head that the response document turns on.
    let c = compare_shells(Shell::AetherShellAgent, Shell::Bash);
    println!("\nHead-to-head — aethershell-agent vs bash (+ = better for agentic use):");
    println!(
        "  fitness {:+.2}   tokens {:+.2}   determinism {:+.2}   reliability {:+.2}   safety {:+.2}",
        c.fitness, c.token_efficiency, c.determinism, c.reliability, c.safety
    );

    // The one that goes the other way, stated as plainly as the one that
    // doesn't: nushell reached a full error taxonomy first.
    let n = compare_shells(Shell::Nushell, Shell::AetherShell);
    println!("\nHead-to-head — nushell vs aethershell (default renderer):");
    println!(
        "  fitness {:+.2}   tokens {:+.2}   determinism {:+.2}   reliability {:+.2}   safety {:+.2}",
        n.fitness, n.token_efficiency, n.determinism, n.reliability, n.safety
    );

    println!("\nEvidence, per shell:");
    for s in Shell::all() {
        let p = profile(s);
        println!(
            "\n  {} ({})",
            s.name(),
            if p.measured {
                "executed"
            } else {
                "NOT executed"
            }
        );
        for e in &p.evidence {
            println!("    - {e}");
        }
    }

    println!(
        "\nConflict of interest: Nervosys builds AetherShell, and two of these rows are ours.\n\
         The measurements behind every axis are in benches/agentic/results/, including\n\
         E1, where AetherShell places fourth of six."
    );
}
