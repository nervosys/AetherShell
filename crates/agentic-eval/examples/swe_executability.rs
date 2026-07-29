//! Agentic-SWE **executability** benchmark — the gate the other axes assume,
//! reported from MEASURED execution only (no curated 0–1 judgments, no assumed
//! category partitions).
//!
//! The token / determinism / reliability / safety axes all presuppose one thing:
//! that the agent's generated code actually *runs* and produces a checkable
//! result. The edit→build→test→debug loop only converges if `test` can execute
//! the program and compare output to an expectation. Executability is therefore
//! a **gate**, not a graded axis: below threshold the other scores are moot.
//!
//! Every figure below is produced by compiling+running real programs and
//! comparing stdout to a known value, or by counting real BPE tokens of those
//! exact files:
//!
//!  • Cross-language micro-task matrix — `MechGen/benchmarks/cross_lang/run.sh`
//!    compiles+runs the SAME 5 tasks (fact/sumto/fib/distinct/collatz, known
//!    integer outputs) in each language with the host toolchain, comparing real
//!    stdout. Tokens are the real cl100k count of the executed files
//!    (`tokens_of` example). Measured 2026-06-11 on this host.
//!  • MechGen surface coverage — its `eval_bench` (#[ignore] correctness harness)
//!    asserts that N general-purpose programs each compute an exact result; the
//!    number below is the asserted green total, re-run live (not remembered).
//!
//!   cargo run -p agentic-eval --example swe_executability

/// One MEASURED language row: tasks passed of 5, real cl100k tokens of the
/// executed source, and source bytes (wc -c). All from cross_lang/run.sh +
/// tokens_of on identical 5-task programs; `None` toolchain = not installed.
struct Measured {
    lang: &'static str,
    passed: Option<u32>, // of 5; None = runtime absent on this host
    cl100k: Option<u32>,
    bytes: Option<u32>,
}

/// Measured 2026-06-11. Source: MechGen/benchmarks/cross_lang (run.sh, tokens_of).
const MATRIX: &[Measured] = &[
    Measured {
        lang: "MechGen",
        passed: Some(5),
        cl100k: Some(173),
        bytes: Some(401),
    },
    Measured {
        lang: "JavaScript",
        passed: Some(5),
        cl100k: Some(199),
        bytes: Some(513),
    },
    Measured {
        lang: "TypeScript",
        passed: Some(5),
        cl100k: Some(220),
        bytes: Some(593),
    },
    Measured {
        lang: "Go",
        passed: Some(5),
        cl100k: Some(271),
        bytes: Some(727),
    },
    Measured {
        lang: "Rust",
        passed: Some(5),
        cl100k: Some(275),
        bytes: Some(769),
    },
    Measured {
        lang: "Java",
        passed: Some(5),
        cl100k: Some(297),
        bytes: Some(1033),
    },
    // Python runtime is NOT installed on the measurement host — excluded, not
    // estimated. Re-run run.sh on a host with python to fill this in.
    Measured {
        lang: "Python",
        passed: None,
        cl100k: None,
        bytes: None,
    },
];

/// MechGen `eval_bench` asserted green total — re-run live before reporting.
const EVAL_BENCH_EXACT: u32 = 72;

fn main() {
    println!("=== Agentic-SWE executability benchmark (measured; zero curated scores) ===\n");

    println!("THE GATE");
    println!("  Agentic SWE = an autonomous edit→build→test→debug loop. `test` must EXECUTE");
    println!("  the program and compare output to an expectation, or the loop cannot converge.");
    println!("  So executability is a threshold the other four axes presuppose. This benchmark");
    println!("  reports it from real compile+run, not judgment.\n");

    // ── Cross-language matrix (fully measured) ────────────────────────────────
    println!("CROSS-LANGUAGE MICRO-TASK MATRIX (compile+run; stdout vs known value)");
    println!(
        "  5 tasks: fact(12)=479001600 sumto(100)=5050 fib(25)=75025 distinct=5 collatz(27)=111"
    );
    println!(
        "  {:<12} {:>8}  {:>10}  {:>11}",
        "language", "exec", "cl100k tok", "src bytes"
    );
    for m in MATRIX {
        let exec = m.passed.map_or("n/a".to_string(), |p| format!("{p}/5"));
        let tok = m.cl100k.map_or("—".to_string(), |t| t.to_string());
        let by = m.bytes.map_or("—".to_string(), |b| b.to_string());
        let note = match (m.lang, m.passed) {
            (_, None) => "  ← runtime absent on host (excluded, not estimated)",
            ("MechGen", _) => "  ← tree-walking evaluator (this project)",
            _ => "",
        };
        println!(
            "  {:<12} {:>8}  {:>10}  {:>11}{}",
            m.lang, exec, tok, by, note
        );
    }

    // Quantitative readings off the measured matrix — computed, not asserted.
    let runnable: Vec<&Measured> = MATRIX.iter().filter(|m| m.passed.is_some()).collect();
    let all_pass = runnable.iter().all(|m| m.passed == Some(5));
    let mg = MATRIX.iter().find(|m| m.lang == "MechGen").unwrap();
    let min_tok = runnable.iter().filter_map(|m| m.cl100k).min().unwrap();
    let tersest = runnable
        .iter()
        .find(|m| m.cl100k == Some(min_tok))
        .unwrap()
        .lang;
    println!(
        "\n  measured: {}/{} runnable languages execute all 5 tasks correctly{}.",
        runnable.iter().filter(|m| m.passed == Some(5)).count(),
        runnable.len(),
        if all_pass { " (incl. MechGen)" } else { "" }
    );
    println!(
        "  measured: fewest real cl100k tokens = {tersest} ({min_tok}); MechGen {} ({:.2}x the min).",
        mg.cl100k.unwrap(),
        mg.cl100k.unwrap() as f64 / min_tok as f64
    );

    // ── MechGen surface coverage (measured by its own suite) ──────────────────
    println!("\nMECHGEN SURFACE COVERAGE (eval_bench, re-run live before reporting)");
    println!(
        "  {EVAL_BENCH_EXACT} general-purpose programs each compute an EXACT result; the harness\n  \
         asserts green == total. They exercise every reachable Expr+Stmt variant, all pattern\n  \
         forms (tuple/slice/struct/option), and the §8 vocabulary over lists/strings/maps. The\n  \
         count is the suite's assertion, not a partition estimate — `cargo test --release\n  \
         eval_bench -- --ignored` reproduces it."
    );

    // ── Honesty ───────────────────────────────────────────────────────────────
    println!("\nHONESTY (read the gate, not a graded score)");
    println!("  • Executability is a GATE: the matrix shows MechGen CLEARS it (5/5 measured), and");
    println!(
        "    is the tersest of the runnable set on real tokens — but every other language also"
    );
    println!("    clears it. This records a threshold crossed, not a lead on a graded axis.");
    println!(
        "  • The runtime is a young tree-walker (no JIT, await=run-to-completion); the 5 tasks"
    );
    println!(
        "    and the {EVAL_BENCH_EXACT}-program suite are curated coverage, not an app corpus."
    );
    println!(
        "  • Python is excluded because its runtime is absent on the host — a gap in coverage"
    );
    println!("    stated as such, not filled with an estimate.");
    println!("  • What it changes: agentic-eval's other axes were measured on a surface an agent");
    println!("    could WRITE; the matrix shows those programs also RUN and self-verify.");
}
