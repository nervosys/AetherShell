//! Migration step 4 (AB_INITIO_DESIGN.md §6): audit every MechGen reserved word
//! against the real BPE tokenizers (cl100k + o200k). A token-efficient surface
//! wants every keyword to be a **single BPE token**; this finds the offenders so
//! they can get a single-token agent-mode form. The analogue of the ontology
//! drift-guard, but for tokenizer alignment.
//!
//!   cargo run -p agentic-eval --example keyword_audit --features real-tokens

use agentic_eval::tokens::Model;

/// MechGen's reserved words (from prototype/src/lexer.rs KEYWORDS).
const KEYWORDS: &[&str] = &[
    "C", "D", "E", "Err", "I", "M", "None", "Ok", "S", "Some", "T", "U", "Y", "Z",
    "af", "agent", "async", "break", "const", "continue", "data", "defer", "df",
    "effect", "else", "enum", "evolve", "extend", "extern", "f", "fact", "fitness",
    "fn", "for", "forward", "fx", "gd", "genome", "grad", "grammar_extension",
    "guard", "handle", "hx", "if", "impl", "in", "is", "kb", "layer", "let", "loop",
    "m", "match", "mod", "mut", "mutate", "net", "or", "param", "pipeline", "pub",
    "query", "ret", "return", "rule", "select", "sp", "spec", "static", "struct",
    "sw", "swarm", "swarm_fan_out", "swarm_map_reduce", "swarm_pipeline",
    "swarm_race", "swarm_saga", "tensor", "train", "trait", "type", "u", "uf",
    "unsafe", "use", "v", "val", "var", "where", "while", "xd", "xn", "yield", "yl",
];

fn main() {
    let cl = Model::OpenAiGpt4;
    let o2 = Model::OpenAiGpt4o;
    println!("=== MechGen keyword tokenizer audit (migration step 4) ===");
    println!(
        "tokenizer: {}   keywords: {}\n",
        if cl.is_exact() { "REAL tiktoken (exact)" } else { "HEURISTIC — rerun with --features real-tokens" },
        KEYWORDS.len()
    );

    // A keyword usually appears with a leading space in code; BPE is space-aware,
    // so " return" can differ from "return". Audit the in-context form (leading
    // space) — that is what an agent actually emits.
    let mut offenders: Vec<(&str, usize, usize)> = Vec::new();
    let mut single = 0usize;
    for &kw in KEYWORDS {
        let ctx = format!(" {kw}");
        let c = cl.count(&ctx);
        let o = o2.count(&ctx);
        if c <= 1 && o <= 1 {
            single += 1;
        } else {
            offenders.push((kw, c, o));
        }
    }

    println!("SINGLE BPE TOKEN (both tokenizers): {single}/{}", KEYWORDS.len());
    println!("\nOFFENDERS (>1 token in cl100k or o200k):");
    if offenders.is_empty() {
        println!("  (none)");
    } else {
        offenders.sort_by(|a, b| b.1.cmp(&a.1));
        for (kw, c, o) in &offenders {
            println!("  {kw:<20} cl100k {c}  o200k {o}");
        }
    }

    println!("\nVERDICT");
    println!(
        "  {}/{} keywords are already single-token (the agent-mode single/double-char forms",
        single, KEYWORDS.len()
    );
    println!("  f/m/v/u/… and common words if/for/match/… cost exactly one token).");
    if !offenders.is_empty() {
        println!("  The {} offenders are compound/rare words (snake_case splits on `_`); each should", offenders.len());
        println!("  get a single-token agent-mode alias. They are specialized (swarm combinators,");
        println!("  grammar extension) — rare in practice, so the realized token cost is small, but");
        println!("  the surface is not yet uniformly single-token. This is the concrete step-4 work-list.");
    }
}
