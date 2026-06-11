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
        // Multi-statement body — fully brace-free + semicolon-free + inferred
        // (return + param inference, `;`-optional, AND layout blocks — all landed).
        (
            "area3",
            "fn area3(w: i32, h: i32) -> i32 { val a = w * h; val b = a + a; b }",
            "f area3(w, h)\n  val a = w * h\n  val b = a + a\n  b",
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
    println!("are the payload (names/ops/literals) — the irreducible floor. area3 is now FULLY");
    println!("brace-free + semicolon-free + inferred (layout blocks landed) — the form-C surface.");

    // Honest sub-finding: is dropping braces a token win, or just aesthetic?
    let braced_nosemi = "f area3(w, h) { val a = w * h\n val b = a + a\n b }";
    let layout = "f area3(w, h)\n  val a = w * h\n  val b = a + a\n  b";
    println!(
        "\nBRACE vs LAYOUT (same fn, no `;`): braced {} → layout {} cl100k tokens",
        cl.count(braced_nosemi),
        cl.count(layout)
    );
    println!("  Dropping braces is ~token-NEUTRAL (often slightly worse): BPE charges for the");
    println!("  indentation whitespace about what the two brace tokens saved. Same lesson as the");
    println!("  'digital rain' — whitespace tokenizes too. The real wins were inference + `;`,");
    println!("  NOT braces→layout. Layout is a readability/aesthetic choice, not a token lever.");

    // Effect inference (trust-boundary model): a PRIVATE effectful function now
    // infers its effects and drops the `/ effect` annotation — the public
    // boundary still declares them, so safety is unchanged (boundary-enforced).
    let with_eff = "f process(x: i32) / io { print(x) }";
    let no_eff = "f process(x: i32) { print(x) }";
    println!(
        "\nEFFECT ANNOTATION (private fn): `/ io` {} → inferred {} cl100k tokens (−{})",
        cl.count(with_eff),
        cl.count(no_eff),
        cl.count(with_eff) - cl.count(no_eff)
    );
    println!("  Private effectful fns now infer effects (the pub boundary still declares them) —");
    println!("  a real, SAFETY-PRESERVING token saving: enforcement moved to the module surface.");
}
