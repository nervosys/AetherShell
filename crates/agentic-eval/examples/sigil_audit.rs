//! Sigil audit: does AetherShell's agentic syntax actually tokenize the way its
//! design assumes?
//!
//! The agentic mode's saving is built on single-character sigils (`F.r`, `~.`,
//! `w~.size>1k`). That saving is only real if the *tokenizer* agrees — BPE is
//! trained on natural text and source code, and an unusual punctuation cluster
//! can easily cost more tokens than the verbose form it replaced. This audit
//! measures each construct against the real cl100k/o200k BPE and compares
//! candidate alternatives, so sigil choices are made on data instead of
//! intuition.
//!
//!   cargo run -p agentic-eval --example sigil_audit --features real-tokens

use agentic_eval::tokens::Model;

/// One construct: what an agent writes today, and what it means.
struct Construct {
    agentic: &'static str,
    verbose: &'static str,
    /// Alternative encodings to test against the incumbent.
    alternatives: &'static [&'static str],
}

const CONSTRUCTS: &[Construct] = &[
    Construct {
        agentic: "F.r(\"p\")",
        verbose: "file.read(\"p\")",
        alternatives: &["f.r(\"p\")", "fr(\"p\")", "@f.r(\"p\")"],
    },
    Construct {
        agentic: "S.h()",
        verbose: "sys.hostname()",
        alternatives: &["s.h()", "sh()"],
    },
    Construct {
        agentic: "DK.p()",
        verbose: "docker.ps()",
        alternatives: &["dk.p()", "dkp()"],
    },
    Construct {
        agentic: "w~.size>1k",
        verbose: "where(fn(__) => __.size > 1000)",
        alternatives: &["w.size>1k", "w~size>1k", "w:.size>1k"],
    },
    Construct {
        agentic: "m~x:x*2",
        verbose: "map(fn(x) => x * 2)",
        alternatives: &["m\\x:x*2", "mx:x*2", "m x=>x*2"],
    },
    Construct {
        agentic: "l\"./src\"|w~.size>1k|m~.name",
        verbose: "ls(\"./src\") | where(fn(__) => __.size > 1000) | map(fn(__) => __.name)",
        alternatives: &["l\"./src\"|w.size>1k|.name", "l./src|w~.size>1k|m~.name"],
    },
    Construct {
        agentic: "?val{A=>\"x\",_=>\"z\"}",
        verbose: "match val { A => \"x\", _ => \"z\" }",
        alternatives: &["?val{A:\"x\",_:\"z\"}", "?val{A>\"x\",_>\"z\"}"],
    },
    Construct {
        agentic: "!{expr}{\"fb\"}",
        verbose: "try { expr } catch e { \"fb\" }",
        alternatives: &["!expr{\"fb\"}", "expr!\"fb\""],
    },
    Construct {
        agentic: "x:=0",
        verbose: "let mut x = 0",
        alternatives: &["x~0", "mx=0"],
    },
    Construct {
        agentic: "$HOME",
        verbose: "sys.env(\"HOME\")",
        alternatives: &["E.HOME", "$\"HOME\""],
    },
];

/// Constructs that appear in nearly every pipeline; a token saved here is
/// multiplied across the whole corpus.
const HOT_PATH: &[&str] = &["|", "|.", "~.", "~", ":=", "=>", "{", "}"];

fn main() {
    let cl = Model::OpenAiGpt4;
    let o2 = Model::OpenAiGpt4o;

    println!("=== Agentic sigil audit ===");
    println!(
        "tokenizer: {}\n",
        if cl.is_exact() {
            "REAL tiktoken cl100k + o200k (exact)"
        } else {
            "HEURISTIC — rerun with --features real-tokens for real numbers"
        }
    );

    // ---- 1. Does each construct beat the verbose form, and by how much? ----
    println!("CONSTRUCT COST (cl100k / o200k)");
    println!(
        "{:<34} {:>9} {:>9} {:>8}",
        "construct", "agentic", "verbose", "saved"
    );
    let (mut total_a, mut total_v) = (0usize, 0usize);
    for c in CONSTRUCTS {
        let (a, v) = (cl.count(c.agentic), cl.count(c.verbose));
        total_a += a;
        total_v += v;
        let saved = v as i64 - a as i64;
        let flag = if saved <= 0 { "  <-- NO WIN" } else { "" };
        println!(
            "{:<34} {:>4}/{:<4} {:>4}/{:<4} {:>+8}{}",
            c.agentic,
            a,
            o2.count(c.agentic),
            v,
            o2.count(c.verbose),
            saved,
            flag
        );
    }
    let pct = 100.0 * (total_v - total_a) as f64 / total_v as f64;
    println!("\n  corpus: {total_a} vs {total_v} tokens — {pct:.1}% saved by agentic mode\n");

    // ---- 2. Is the incumbent encoding the cheapest available? ----
    println!("ALTERNATIVE ENCODINGS (cl100k; * = beats the incumbent)");
    let mut wins: Vec<(&str, &str, usize, usize)> = Vec::new();
    for c in CONSTRUCTS {
        let base = cl.count(c.agentic);
        let mut line = format!("  {:<30} {base:>3}", c.agentic);
        for alt in c.alternatives {
            let n = cl.count(alt);
            let marker = if n < base { "*" } else { " " };
            line.push_str(&format!("   {alt} {n}{marker}"));
            if n < base {
                wins.push((c.agentic, alt, base, n));
            }
        }
        println!("{line}");
    }

    // ---- 3. Hot-path atoms ----
    println!("\nHOT-PATH ATOMS (cost in context, i.e. preceded by an identifier)");
    for atom in HOT_PATH {
        let in_context = format!("a{atom}b");
        println!(
            "  {:<6} standalone {}   in-context {}",
            atom,
            cl.count(atom),
            cl.count(&in_context)
        );
    }

    // ---- 4. Realized saving from the v3 bare-dot lambda ----
    //
    // Adopted in src/transpile/agentic.rs. These are the pipelines an agent
    // actually emits, measured before and after the change, so the claim is a
    // measurement rather than an extrapolation from the isolated construct.
    const REALIZED: &[(&str, &str)] = &[
        ("l\".\"|w~.size>1k", "l\".\"|w.size>1k"),
        (
            "l\"./src\"|w~.size>1k|m~.name",
            "l\"./src\"|w.size>1k|.name",
        ),
        (
            "F.r(\"log\")|w~.level=='E'|m~.msg|t10",
            "F.r(\"log\")|w.level=='E'|.msg|t10",
        ),
        ("P.l()|w~.cpu>80|m~.pid", "P.l()|w.cpu>80|.pid"),
    ];
    println!("\nREALIZED SAVING (v3 bare-dot lambda, cl100k)");
    let (mut before, mut after) = (0usize, 0usize);
    for (old, new) in REALIZED {
        let (b, a) = (cl.count(old), cl.count(new));
        before += b;
        after += a;
        println!("  {old:<38} {b:>3} -> {a:<3} ({:+})", a as i64 - b as i64);
    }
    println!(
        "  {:<38} {before:>3} -> {after:<3} ({:.1}% fewer tokens on predicate pipelines)",
        "TOTAL",
        100.0 * (before - after) as f64 / before as f64
    );

    // ---- 5. Verdict ----
    // A cheaper encoding is only worth adopting if it stays unambiguous, so
    // each candidate carries an explicit disposition. Recording the rejections
    // matters as much as the adoptions: without them this audit re-proposes
    // the same unsafe encodings every time it runs.
    const ADOPTED: &[(&str, &str)] = &[
        (
            "w.size>1k",
            "shipped as the v3 bare-dot lambda (pipe position only)",
        ),
        (
            "l\"./src\"|w.size>1k|.name",
            "same lever, plus |.field projection",
        ),
    ];
    const REJECTED: &[(&str, &str)] = &[
        (
            "fr(\"p\")",
            "collides with user identifiers — `fr` is a legal variable name",
        ),
        ("sh()", "`sh` is already the shell-exec builtin"),
        (
            "w~size>1k",
            "ambiguous with a lambda parameter named `size`",
        ),
        (
            "w:.size>1k",
            "`:` is the lambda body separator; reusing it needs lookahead",
        ),
        (
            "mx:x*2",
            "`mx` reads as an identifier, not map-with-parameter-x",
        ),
        (
            "m\\x:x*2",
            "already supported as the v1 form; no change needed",
        ),
        (
            "!expr{\"fb\"}",
            "unbraced try body has no unambiguous end without lookahead",
        ),
        ("expr!\"fb\"", "postfix `!` collides with boolean negation"),
    ];

    println!("\nVERDICT");
    let disposition = |alt: &str| -> Option<(&'static str, &'static str)> {
        ADOPTED
            .iter()
            .find(|(a, _)| *a == alt)
            .map(|(_, why)| ("ADOPTED", *why))
            .or_else(|| {
                REJECTED
                    .iter()
                    .find(|(a, _)| *a == alt)
                    .map(|(_, why)| ("rejected", *why))
            })
    };

    let mut undecided = 0usize;
    for (base, alt, b, n) in &wins {
        match disposition(alt) {
            Some((tag, why)) => {
                println!("  [{tag}] {alt}  ({b} -> {n} tokens)");
                println!("           {why}");
            }
            None => {
                undecided += 1;
                println!(
                    "  [OPEN]     {base}  ->  {alt}   {b} -> {n} tokens  (-{})",
                    b - n
                );
            }
        }
    }
    if undecided == 0 {
        println!(
            "\n  Every cheaper encoding has a recorded disposition. The remaining\n  \
             sigils are at a local optimum under cl100k."
        );
    } else {
        println!(
            "\n  {undecided} encoding(s) have no recorded disposition — decide and record\n  \
             them here. Check src/transpile/agentic.rs CONFLICT_RULES first."
        );
    }
}
