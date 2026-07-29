//! §8b discipline check: every standard-vocabulary primitive name must be a
//! SINGLE BPE token, or it leaks the abstraction saving back. Audits MechGen's
//! registered SWE vocabulary (resolve.rs) against the real cl100k + o200k BPE.
//!
//!   cargo run -p agentic-eval --example vocabulary_audit --features real-tokens

use agentic_eval::tokens::Model;

/// MechGen's standard SWE vocabulary (resolve.rs register_builtins, §8).
const VOCAB: &[&str] = &[
    "map", "filter", "fold", "reduce", "sum", "len", "sort", "reverse", "zip", "freq", "first",
    "last", "count", "any", "all", "find", "take", "range", "keys", "values", "flatten", "group",
    "scan", "contains", "min", "max", "abs", // string / text vocabulary
    "split", "join", "chars", "words", "lines", "upper", "lower",
];

fn main() {
    let cl = Model::OpenAiGpt4;
    let o2 = Model::OpenAiGpt4o;
    println!("=== Standard-vocabulary tokenizer audit (§8b) ===");
    println!(
        "tokenizer: {}   names: {}\n",
        if cl.is_exact() {
            "REAL tiktoken (exact)"
        } else {
            "HEURISTIC — rerun with --features real-tokens"
        },
        VOCAB.len()
    );

    // Agents emit a name with a leading space; BPE is space-aware.
    let mut single = 0usize;
    let mut offenders: Vec<(&str, usize, usize)> = Vec::new();
    for &name in VOCAB {
        let ctx = format!(" {name}");
        let (c, o) = (cl.count(&ctx), o2.count(&ctx));
        if c <= 1 && o <= 1 {
            single += 1;
        } else {
            offenders.push((name, c, o));
        }
    }

    println!(
        "SINGLE BPE TOKEN (both tokenizers): {single}/{}",
        VOCAB.len()
    );
    if offenders.is_empty() {
        println!("  ✓ every vocabulary name is a single token — the §8b discipline holds.");
    } else {
        println!("\nOFFENDERS (rename or drop — a multi-token name negates the saving):");
        for (n, c, o) in &offenders {
            println!("  {n:<12} cl100k {c}  o200k {o}");
        }
    }

    println!("\nWHY IT MATTERS");
    println!("  The vocabulary's win is naming an intent in ~1 token. A 2-token name (e.g.");
    println!("  `frequencies` = 'frequ'+'encies') halves that. Picking `freq`/`map`/`fold` over");
    println!("  `frequencies`/`transform`/`accumulate` is tokenizer co-design, audited here.");
}
