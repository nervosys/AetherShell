//! Realized token win from the landed inference migration (return-type +
//! parameter-type inference). Each pair is the SAME function, annotated vs the
//! now-valid inferred form (`fn sq(n: i32) -> i32 {…}` → `f sq(n) {…}`), counted
//! with the real cl100k + o200k BPE.
//!
//!   cargo run -p agentic-eval --example inference_tokens --features real-tokens

use agentic_eval::tokens::Model;

fn main() {
    let cl = Model::OpenAiGpt4;
    let o2 = Model::OpenAiGpt4o;
    println!("=== Realized token win: type inference (return + params) ===");
    println!(
        "tokenizer: {}\n",
        if cl.is_exact() { "REAL tiktoken (exact)" } else { "HEURISTIC — rerun with --features real-tokens" }
    );

    // (name, annotated [valid before], inferred [valid AFTER the landed change]).
    let pairs: &[(&str, &str, &str)] = &[
        ("square", "fn square(n: i32) -> i32 { n * n }", "f square(n) { n * n }"),
        ("add", "fn add(a: i32, b: i32) -> i32 { a + b }", "f add(a, b) { a + b }"),
        (
            "factorial",
            "fn factorial(n: u64) -> u64 { if n <= 1 { 1 } else { n * factorial(n - 1) } }",
            "f factorial(n) { if n <= 1 { 1 } else { n * factorial(n - 1) } }",
        ),
    ];

    println!("{:<11} {:>9} {:>9} {:>9}", "fn", "annot", "inferred", "saved");
    let (mut ann_cl, mut inf_cl, mut ann_o, mut inf_o) = (0, 0, 0, 0);
    for (name, ann, inf) in pairs {
        let (a, i) = (cl.count(ann), cl.count(inf));
        println!("{name:<11} {a:>9} {i:>9} {:>8}%", 100 - 100 * i / a);
        ann_cl += a; inf_cl += i;
        ann_o += o2.count(ann); inf_o += o2.count(inf);
    }
    println!("\nTOTAL  cl100k {ann_cl} → {inf_cl} ({}% saved)   o200k {ann_o} → {inf_o} ({}% saved)",
        100 - 100 * inf_cl / ann_cl, 100 - 100 * inf_o / ann_o);
    println!("\nThe inferred forms are now ACCEPTED by the compiler (return + param inference,");
    println!("recursion-correct), so this saving is real, not hypothetical. The remaining tokens");
    println!("are the payload (names/ops/literals) — the irreducible floor. Offside layout (drop");
    println!("braces/`;`) is the next lever still staged.");
}
