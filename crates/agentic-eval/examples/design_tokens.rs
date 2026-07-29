//! Measuring the *design levers* for token efficiency with the real BPE
//! tokenizers (cl100k + o200k). For each task, three semantically-equivalent
//! programs at different ceremony levels:
//!   A = ceremony-heavy (explicit types, braces, semicolons, imports, wrappers)
//!   B = current-MechGen-ish (sigils, `val/var`, partial inference)
//!   C = ab-initio (maximal inference, layout instead of braces/`;`, ambient
//!       builtins, terse safety sigils — safety/types live in the compiler, not
//!       the token budget)
//!
//! The point: the *payload* (names/ops/literals) is a floor, but the *ceremony*
//! is real tokens you can design away. C must cost the fewest tokens while still
//! denoting the same program (a compiler infers the rest).
//!
//!   cargo run -p agentic-eval --example design_tokens --features real-tokens

use agentic_eval::tokens::Model;

fn main() {
    let cl = Model::OpenAiGpt4;
    let o2 = Model::OpenAiGpt4o;
    println!("=== Token-efficiency design levers (real cl100k + o200k BPE) ===");
    println!(
        "tokenizer: {}\n",
        if cl.is_exact() {
            "REAL tiktoken (exact)"
        } else {
            "HEURISTIC — rerun with --features real-tokens"
        }
    );

    // Each task: (name, ceremony-heavy, current-ish, ab-initio).
    let tasks: &[(&str, &str, &str, &str)] = &[
        (
            "word-count",
            // A — ceremony-heavy
            "use std::collections::HashMap;\n\nfn count_words(text: &str) -> HashMap<String, u32> {\n    let mut counts: HashMap<String, u32> = HashMap::new();\n    for word in text.split_whitespace() {\n        *counts.entry(word.to_string()).or_insert(0) += 1;\n    }\n    counts\n}",
            // B — current-MechGen-ish (sigils, var, some inference)
            "fn count_words(text: &str) -> {s: u32} {\n    var counts = {s: u32}.new()\n    for word in text.split() {\n        counts.entry(word).or(0) += 1\n    }\n    counts\n}",
            // C — ab-initio (inference + layout + ambient builtins)
            "count_words text =\n  counts = {}\n  for w in split text\n    counts[w] += 1\n  counts",
        ),
        (
            "factorial",
            "fn factorial(n: u64) -> u64 {\n    if n <= 1 {\n        return 1;\n    }\n    n * factorial(n - 1)\n}",
            "fn factorial(n: u64) -> u64 {\n    if n <= 1 { 1 } else { n * factorial(n - 1) }\n}",
            "fact n =\n  if n <= 1: 1\n  else: n * fact (n - 1)",
        ),
        (
            "safe-divide", // returns optional/result — safety ceremony vs sigil
            "fn safe_div(a: i32, b: i32) -> Option<i32> {\n    if b == 0 {\n        return None;\n    }\n    Some(a / b)\n}",
            "fn safe_div(a: i32, b: i32) -> ?i32 {\n    if b == 0 { none } else { a / b }\n}",
            "div a b =\n  if b == 0: none\n  else: a / b",
        ),
    ];

    println!(
        "{:<13} {:>4} {:>9} {:>8} {:>7}",
        "task", "form", "cl100k", "o200k", "chars"
    );
    let (mut a_cl, mut b_cl, mut c_cl) = (0, 0, 0);
    let (mut a_o, mut b_o, mut c_o) = (0, 0, 0);
    for (name, a, b, c) in tasks {
        let row = |label: &str, s: &str| {
            println!(
                "{:<13} {:>4} {:>9} {:>8} {:>7}",
                "",
                label,
                cl.count(s),
                o2.count(s),
                s.chars().count()
            );
        };
        println!("[{name}]");
        row("A heavy", a);
        row("B curr", b);
        row("C abinit", c);
        a_cl += cl.count(a);
        b_cl += cl.count(b);
        c_cl += cl.count(c);
        a_o += o2.count(a);
        b_o += o2.count(b);
        c_o += o2.count(c);
    }

    println!("\nTOTALS (3 tasks):");
    println!("  A ceremony-heavy   cl100k {a_cl:>3}   o200k {a_o:>3}   (baseline)");
    println!(
        "  B current-ish      cl100k {b_cl:>3} ({:.0}%)   o200k {b_o:>3} ({:.0}%)",
        100.0 * b_cl as f64 / a_cl as f64,
        100.0 * b_o as f64 / a_o as f64
    );
    println!(
        "  C ab-initio        cl100k {c_cl:>3} ({:.0}%)   o200k {c_o:>3} ({:.0}%)",
        100.0 * c_cl as f64 / a_cl as f64,
        100.0 * c_o as f64 / a_o as f64
    );
    println!(
        "\n  → ab-initio cuts ~{:.0}% of cl100k tokens vs ceremony-heavy by REMOVING ceremony",
        100.0 * (1.0 - c_cl as f64 / a_cl as f64)
    );
    println!(
        "    (types/mutability/return/imports inferred; layout replaces braces+`;`; terse safety"
    );
    println!("    sigils; ambient builtins). The remaining tokens are the irreducible payload —");
    println!(
        "    names/ops/literals — which no design can remove. That residue IS the token floor."
    );
}
