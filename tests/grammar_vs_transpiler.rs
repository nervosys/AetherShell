//! Which cipher forms the **grammar** understands, and which are transpiler-only.
//!
//! §4.3's goal is that the agent surface be parsed by the real lexer/parser, with
//! the `.aeg` transpiler reduced to at most a thin legacy shim. Phase 5 retired
//! the 10-pass pipeline to a single tokenizing `scan`, and several forms
//! (`|.field` projection, SI suffixes, `~x: body`, `?match`, `if`-expressions)
//! moved into the grammar for real.
//!
//! What was left was recorded in prose — "`\x:`/`~.field`/`>`-pipe and the
//! `!try`/`^cond` token overloads stay transpiler-only" — and prose is exactly
//! what goes stale. It did: the roadmap kept describing the blocker in terms of
//! `expand_lambdas`/`expand_pipelines`, functions deleted when Phase 5 landed.
//!
//! So this file makes the division mechanical instead. Each form is fed to the
//! **parser** directly. A form that stays transpiler-only must fail to parse (or
//! parse to something other than what the cipher means); a form the grammar has
//! adopted must parse. Either way, moving one across the line makes a test here
//! fail and say so, rather than leaving a paragraph quietly wrong.

use aethershell::env::Env;
use aethershell::transpile::agentic::transpile_agentic_to_ae;
use aethershell::value::Value;

/// Does the real grammar accept this source on its own?
fn grammar_accepts(src: &str) -> bool {
    aethershell::parser::parse_program(src).is_ok()
}

/// Evaluate `src` through the real grammar with **no** transpiler in front of it.
///
/// `Err` covers both "the parser has no production for this" and "it parsed but
/// meant something the evaluator rejected".
fn direct_eval(src: &str) -> Result<Value, String> {
    let stmts = aethershell::parser::parse_program(src).map_err(|e| e.to_string())?;
    let mut env = Env::new();
    aethershell::eval::eval_program(&stmts, &mut env).map_err(|e| e.to_string())
}

/// Evaluate through the transpiler, the way the `.aeg` surface actually runs.
fn eval_aeg(src: &str) -> Value {
    let ae = transpile_agentic_to_ae(src).expect("transpile");
    let stmts = aethershell::parser::parse_program(&ae).expect("parse transpiled output");
    let mut env = Env::new();
    aethershell::eval::eval_program(&stmts, &mut env).expect("eval")
}

#[test]
fn the_forms_the_grammar_has_adopted_parse_without_the_transpiler() {
    // Each of these was moved into the real grammar during Phase 5. If one stops
    // parsing, the grammar has regressed and the transpiler is silently carrying
    // it again.
    for src in [
        "[{a: 1}, {a: 2}] | .a",             // |.field projection
        "[1, 2, 3] | map(~x: x * 2)",        // ~x: body lambda
        "let n = 1k;",                       // SI suffix in the lexer
        "let v = if true { 1 } else { 2 };", // if-expression
    ] {
        assert!(
            grammar_accepts(src),
            "the grammar used to accept `{src}` directly — Phase 5 moved it in, \
             and something has taken it back out"
        );
    }
}

#[test]
fn the_transpiler_only_ciphers_are_still_transpiler_only() {
    // The honest statement of what blocks §4.3: the parser does not give these
    // forms the meaning the cipher does, so the transpiler cannot become a pure
    // shim over the grammar until it does.
    //
    // The test is *meaning*, not parseability, and `>` is why. `[1,2,3] > len()`
    // parses perfectly well — as a greater-than comparison. It is a valid program
    // that means something else entirely, which is a worse failure than a syntax
    // error and would have been invisible to a `parse_program(..).is_err()`
    // check. So: run it both ways and require them to disagree.
    let cases: [(&str, &str); 2] = [
        (r"[1, 2, 3] | map(\x: x * 2)", "backslash lambda"),
        ("[1, 2, 3] > len()", "`>`-as-pipe"),
    ];
    for (src, what) in cases {
        let through_transpiler = eval_aeg(src);
        let direct = direct_eval(src);
        assert_ne!(
            direct.as_ref().ok(),
            Some(&through_transpiler),
            "the grammar now gives {what} (`{src}`) the same meaning the cipher \
             has. That is progress, not a failure — move it to the adopted list \
             above, and update §4.3 and the phase-5 row, which call it \
             transpiler-only"
        );
    }
}

#[test]
fn a_cipher_the_grammar_reads_differently_is_worse_than_one_it_rejects() {
    // Worth stating on its own, because it is the reason §4.3 cannot be closed by
    // simply deleting the transpiler. `\x:` fails to parse — loud, safe. `>`
    // parses as a comparison — silent, and the same text means two different
    // programs depending on which surface reads it.
    assert!(
        !grammar_accepts(r"[1, 2, 3] | map(\x: x * 2)"),
        "the backslash lambda should have no production at all"
    );
    assert!(
        grammar_accepts("[1, 2, 3] > len()"),
        "`>` is a comparison operator in the grammar, so this parses"
    );
    assert_ne!(
        direct_eval("[1, 2, 3] > len()").ok(),
        Some(Value::Int(3)),
        "read by the grammar this is a comparison, not a pipe — if it ever \
         evaluates to 3 the two surfaces have converged and the roadmap should \
         say so"
    );
    assert_eq!(
        eval_aeg("[1, 2, 3] > len()"),
        Value::Int(3),
        "read by the transpiler it is a pipe"
    );
}

#[test]
fn the_transpiler_still_gives_those_ciphers_their_meaning() {
    // The other half: "transpiler-only" has to mean the transpiler *does* handle
    // them, not that they are broken everywhere.
    assert_eq!(
        eval_aeg(r"[1, 2, 3] | map(\x: x * 2)"),
        Value::Array(vec![Value::Int(2), Value::Int(4), Value::Int(6)]),
        "the backslash lambda is the transpiler's job and must still work"
    );
    assert_eq!(
        eval_aeg("[1, 2, 3] > len()"),
        Value::Int(3),
        "`>`-as-pipe is the transpiler's job and must still work"
    );
}

#[test]
fn the_ten_pass_pipeline_really_is_gone() {
    // The claim that went stale. Phase 5 deleted all 14 `expand_*`/
    // `preprocess_ultra` functions, but the roadmap kept naming
    // `expand_lambdas`/`expand_pipelines` as the blocker for another two
    // sessions. Assert against the source so the next reader gets a fact.
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/transpile/agentic.rs"
    ))
    .expect("src/transpile/agentic.rs is readable");
    for gone in [
        "fn expand_lambdas",
        "fn expand_pipelines",
        "fn expand_si_suffixes",
        "fn expand_match",
        "fn preprocess_ultra",
    ] {
        assert!(
            !src.contains(gone),
            "`{gone}` is back. The retired 10-pass pipeline was replaced by a \
             single left-to-right `scan`; re-introducing a pass reopens the \
             ordering hazard that retirement removed"
        );
    }
}
