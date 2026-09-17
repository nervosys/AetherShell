//! Bash compatibility mode must not answer `null` where bash answers.
//!
//! Measured while asking whether a shell-native agentic benchmark (AgentBench's
//! OS environment, the SWE-bench family) could be pointed at `ae -b`. The
//! answer is mostly no — `benches/agentic/bashcompat.mjs` has the numbers — but
//! one of the failures was worse than incompatibility:
//!
//! ```text
//! $ ae -b -c 'echo $HOME'
//! null                       # exit 0
//! $ ae -b -c 'echo "home is $HOME"'
//! home is null               # exit 0
//! ```
//!
//! Every environment-variable expansion returned `null` and exited 0. The
//! transpiler emitted a bare identifier `HOME` on the theory that a preceding
//! `NAME=value` had bound it; with nothing bound, an unbound identifier
//! evaluates to null rather than raising. A confident wrong answer at exit 0,
//! for the single most common expansion in shell.
//!
//! Underneath it, `env(name, default)` accepted a second argument and discarded
//! it, so an unset variable could not be given bash's empty-string semantics —
//! the same defect shape as `round(x, digits)` ignoring `digits`
//! (`tests/aggregate_and_round.rs`).
//!
//! These tests drive the real binary, because the defect lived in the path from
//! transpiler to evaluator and nothing below that boundary could see it.

use std::process::Command;

fn ae(args: &[&str]) -> (String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_ae"))
        .args(args)
        .env("NO_COLOR", "1")
        .env("AETHER_ALLOW_SH", "")
        .env("AE_COMPAT_FIXTURE", "fixture-value")
        .env_remove("AE_COMPAT_UNSET")
        .output()
        .expect("run ae");
    (
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn a_lone_variable_reads_the_environment() {
    let (out, code) = ae(&["-b", "-c", "echo $AE_COMPAT_FIXTURE"]);
    assert_eq!(out, "fixture-value", "the variable did not resolve");
    assert_eq!(code, 0);
}

#[test]
fn an_interpolated_variable_resolves_too() {
    // The standalone and interpolated forms went down different paths, and
    // only one of them was fixed first. Both are asserted so they cannot
    // diverge again.
    let (out, _) = ae(&["-b", "-c", "echo \"value is $AE_COMPAT_FIXTURE\""]);
    assert_eq!(out, "value is fixture-value");

    let (braced, _) = ae(&["-b", "-c", "echo \"[${AE_COMPAT_FIXTURE}]\""]);
    assert_eq!(braced, "[fixture-value]");
}

#[test]
fn an_unset_variable_is_empty_as_bash_has_it_not_the_word_null() {
    // bash prints an empty line. Printing "null" is not a smaller difference
    // than printing nothing: it is a value an agent may go on to use.
    let (out, code) = ae(&["-b", "-c", "echo $AE_COMPAT_UNSET"]);
    assert_eq!(out, "", "an unset variable must render empty, got {out:?}");
    assert_eq!(code, 0, "bash exits 0 here");

    let (interp, _) = ae(&["-b", "-c", "echo \"x${AE_COMPAT_UNSET}y\""]);
    assert_eq!(
        interp, "xy",
        "unset inside a string must vanish, got {interp:?}"
    );
}

#[test]
fn no_expansion_anywhere_produces_the_word_null() {
    // The specific regression, stated as the thing that must never come back.
    for script in [
        "echo $AE_COMPAT_UNSET",
        "echo \"a $AE_COMPAT_UNSET b\"",
        "echo $AE_COMPAT_FIXTURE",
        "echo \"a $AE_COMPAT_FIXTURE b\"",
    ] {
        let (out, _) = ae(&["-b", "-c", script]);
        assert!(
            !out.contains("null"),
            "{script} produced {out:?}; variable expansion is emitting null again"
        );
    }
}

// ── the builtin underneath ──────────────────────────────────────────────

#[test]
fn env_honours_the_default_it_used_to_discard() {
    let (out, _) = ae(&["-c", r#"env("AE_COMPAT_UNSET", "fallback")"#]);
    assert_eq!(out, "fallback", "env ignored its default argument");

    // A set variable still wins over the default.
    let (set, _) = ae(&["-c", r#"env("AE_COMPAT_FIXTURE", "fallback")"#]);
    assert_eq!(set, "fixture-value");
}

#[test]
fn env_without_a_default_is_unchanged() {
    // Widening the overload must not alter the one-argument behaviour that
    // existing scripts depend on: unset stays null, not empty string.
    let (out, _) = ae(&["-c", r#"to_string(env("AE_COMPAT_UNSET"))"#]);
    assert_eq!(out, "null", "the single-argument form changed shape");
}

// ── non-vacuity ─────────────────────────────────────────────────────────

#[test]
fn non_vacuity_the_fixture_variable_is_actually_set_and_the_other_is_not() {
    // If the harness failed to set AE_COMPAT_FIXTURE, the assertions above
    // would be comparing two kinds of nothing.
    let (set, _) = ae(&["-c", r#"env("AE_COMPAT_FIXTURE")"#]);
    assert_eq!(
        set, "fixture-value",
        "the fixture variable is not reaching ae"
    );

    let (unset, _) = ae(&["-c", r#"to_string(env("AE_COMPAT_UNSET"))"#]);
    assert_eq!(
        unset, "null",
        "AE_COMPAT_UNSET is set; the unset tests are not testing unset"
    );

    // And compat mode must still be doing something: a plain command runs.
    let (echoed, code) = ae(&["-b", "-c", "echo plain"]);
    assert_eq!(
        (echoed.as_str(), code),
        ("plain", 0),
        "compat mode is not running at all"
    );
}

#[test]
fn a_literal_run_stays_one_string_in_the_transpiled_source() {
    // The lexer emits one text piece per character. The first version of the
    // concatenation fix joined them all, turning `echo hello` into
    // `echo("h" + "e" + "l" + "l" + "o")` -- correct, and four times the
    // tokens, in a shell whose case rests on token efficiency.
    let src =
        aethershell::transpile::bash::transpile_bash_to_ae("echo hello there").expect("transpile");
    assert!(
        src.contains(r#"echo("hello", "there")"#) || src.contains(r#"echo("hello""#),
        "literal run was split into per-character concatenation: {src}"
    );
    assert!(
        !src.contains(r#""h" + "e""#),
        "per-character concatenation is back: {src}"
    );

    // And a mixed literal/variable argument keeps the literal whole.
    let mixed = aethershell::transpile::bash::transpile_bash_to_ae("echo \"home is $HOME\"")
        .expect("transpile");
    assert!(
        mixed.contains(r#""home is " + env("HOME", "")"#),
        "mixed argument is not one literal plus one lookup: {mixed}"
    );
}
