//! `$VAR` in agentic mode must actually read the environment.
//!
//! `AGENTS.md` documents `$HOME` → `sys.env("HOME")` as a v4 feature, the
//! module sigil map advertises it as `S.e`, and the transpiler's own rule table
//! carries the worked example `("e$USER", "echo(sys.env(\"USER\")))"`. All three
//! emitted `sys.env(…)`, and `sys.env` did not exist: the `sys` module table had
//! `env_all` but never `env`. Every documented way to read an environment
//! variable in agentic mode failed with
//!
//! ```text
//! error[E_UNKNOWN_FIELD]: field 'env' not found in record
//! ```
//!
//! The transpiler tests passed throughout, because they assert on the *emitted
//! source* — and the emitted source was right. This is the second time in one
//! week that a `$VAR` feature shipped broken behind a test that checked the
//! text rather than the result (the bash-compat path was the first). So these
//! tests run the shell and read what comes back.

use std::process::Command;

const AE: &str = env!("CARGO_BIN_EXE_ae");

fn ae(args: &[&str], var: &str, value: &str) -> (String, bool) {
    let out = Command::new(AE)
        .args(args)
        .env(var, value)
        .output()
        .expect("spawn ae");
    (
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr),
        out.status.success(),
    )
}

/// A value no environment would supply by accident, so a passing assertion
/// cannot be some other variable leaking through.
const SENTINEL: &str = "ae-env-sentinel-94117";

#[test]
fn every_documented_spelling_reads_the_environment() {
    for (label, args) in [
        ("$VAR sugar", vec!["-a", "-c", "$AE_TEST_VAR"]),
        ("echo of $VAR", vec!["-a", "-c", "e$AE_TEST_VAR"]),
        ("module sigil S.e", vec!["-a", "-c", "S.e\"AE_TEST_VAR\""]),
        ("plain sys.env", vec!["-c", "sys.env(\"AE_TEST_VAR\")"]),
        ("bare env builtin", vec!["-c", "env(\"AE_TEST_VAR\")"]),
    ] {
        let (out, ok) = ae(&args, "AE_TEST_VAR", SENTINEL);
        assert!(ok, "{label}: ae exited non-zero: {out}");
        assert!(
            out.contains(SENTINEL),
            "{label}: did not read the variable; got: {out}"
        );
    }
}

#[test]
fn an_unset_variable_is_not_reported_as_set() {
    // Non-vacuity: if these spellings returned something constant, or echoed
    // their own argument, the test above would pass without reading anything.
    let out = Command::new(AE)
        .args(["-a", "-c", "$AE_DEFINITELY_UNSET_94117"])
        .env_remove("AE_DEFINITELY_UNSET_94117")
        .output()
        .expect("spawn ae");
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(
        !text.contains(SENTINEL),
        "an unset variable produced the sentinel: {text}"
    );
}

#[test]
fn the_default_argument_is_honoured() {
    // `env` carries a declared signature (`src/signature.rs`) whose second
    // parameter used to be accepted and discarded. Reached here through the
    // module alias, which is a different path to the same builtin.
    let (out, ok) = ae(
        &[
            "-c",
            r#"sys.env("AE_DEFINITELY_UNSET_94117", "fallback-ok")"#,
        ],
        "AE_UNRELATED",
        "x",
    );
    assert!(ok, "ae exited non-zero: {out}");
    assert!(out.contains("fallback-ok"), "default ignored; got: {out}");
}
