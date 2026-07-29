//! Second-pass lever: once the *surface* is at the payload floor (inference,
//! `;`-removal — landed, MechGen #1 on swe_token_benchmark), the only remaining
//! per-call token lever is RAISING THE ABSTRACTION — a high-level primitive
//! expresses an SWE intent in fewer *payload* tokens than hand-rolling it.
//!
//! Measured with the real cl100k/o200k BPE: hand-rolled (compiles today) vs the
//! same intent via a standard-vocabulary primitive. The combinators (map/filter/
//! fold/reduce/sum/freq/sort/…) are now REGISTERED in MechGen (resolve + type,
//! `--check`ed) and audited single-token (`vocabulary_audit`); precise totality
//! typing is a staged backend follow-on. The point is the token delta they buy.
//!
//!   cargo run -p agentic-eval --example abstraction_tokens --features real-tokens

use agentic_eval::tokens::Model;

fn main() {
    let cl = Model::OpenAiGpt4;
    let o2 = Model::OpenAiGpt4o;
    println!("=== Abstraction as the post-floor token lever (real BPE) ===");
    println!(
        "tokenizer: {}\n",
        if cl.is_exact() {
            "REAL tiktoken (exact)"
        } else {
            "HEURISTIC — rerun with --features real-tokens"
        }
    );

    // (intent, hand-rolled [compiles today], with-vocabulary [proposed primitive])
    let cases: &[(&str, &str, &str)] = &[
        (
            "sum a list",
            "f sum(xs)\n  var t = 0\n  for x in xs\n    t = t + x\n  t",
            "f sum(xs)\n  fold(xs, 0, +)",
        ),
        (
            "word frequencies",
            "f wc(ws)\n  var m = {}\n  for w in ws\n    m[w] = m[w] + 1\n  m",
            "f wc(ws)\n  freq(ws)",
        ),
        (
            "evens, doubled",
            "f f(xs)\n  var out = []\n  for x in xs\n    if x % 2 == 0\n      out.push(x * 2)\n  out",
            "f f(xs)\n  xs | filter even | map double",
        ),
        (
            "max of a list",
            "f max(xs)\n  var m = xs[0]\n  for x in xs\n    if x > m\n      m = x\n  m",
            "f max(xs)\n  reduce(xs, max)",
        ),
    ];

    println!(
        "{:<18} {:>9} {:>9} {:>7}",
        "intent", "handrolled", "vocab", "saved"
    );
    let (mut h_cl, mut v_cl, mut h_o, mut v_o) = (0, 0, 0, 0);
    for (name, hand, vocab) in cases {
        let (h, v) = (cl.count(hand), cl.count(vocab));
        println!("{name:<18} {h:>9} {v:>9} {:>6}%", 100 - 100 * v / h);
        h_cl += h;
        v_cl += v;
        h_o += o2.count(hand);
        v_o += o2.count(vocab);
    }
    println!(
        "\nTOTAL  cl100k {h_cl} → {v_cl} ({}% saved)   o200k {h_o} → {v_o} ({}% saved)",
        100 - 100 * v_cl / h_cl,
        100 - 100 * v_o / h_o
    );

    println!("\nFINDING");
    println!(
        "  At the surface floor, abstraction is the only per-call token lever left, and it is"
    );
    println!("  POSITIVE-SUM: a single-token, total, capability-typed primitive (a) cuts payload");
    println!(
        "  tokens (above), (b) RAISES reliability (no hand-rolled off-by-one / empty-list bug),"
    );
    println!("  and (c) preserves safety (the primitive's effect rides its type to the boundary).");
    println!(
        "  Encoding tricks (binary, dense UTF-8) and layout were all token-neutral-or-worse —"
    );
    println!(
        "  vocabulary is the one that pays. The discipline: name primitives as single BPE tokens,"
    );
    println!("  make them total, and choose them by the empirical frequency of SWE intents.");
}
