//! `ae -c` output must be consumable by another program.
//!
//! Until 10.0.1 the colour decision was `config.colors.enabled` and nothing
//! else — it never asked where the output was going. So
//!
//! ```text
//! ae -c '1 + 2' > out
//! ```
//!
//! wrote `\x1b[38;2;180;142;173m3\x1b[39m` into the file, and
//! `n=$(ae -c '1 + 2')` captured escape codes rather than `3`. Every consumer
//! downstream of the shell — a pipe, a command substitution, a CI assertion,
//! the project's own Homebrew formula, which asserts `assert_equal "3"` — got
//! bytes it had to strip before it could use them.
//!
//! This is tested by running the real binary with its stdout attached to a
//! pipe, because that *is* the condition under test: an in-process call cannot
//! observe it.

use std::process::{Command, Stdio};

/// The `ae` binary built alongside this test.
fn ae() -> std::path::PathBuf {
    // target/debug/deps/<test>-<hash> -> target/debug/ae
    let mut p = std::env::current_exe().expect("test exe path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join(format!("ae{}", std::env::consts::EXE_SUFFIX))
}

fn run(args: &[&str], env: &[(&str, &str)]) -> (String, String) {
    let mut cmd = Command::new(ae());
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Inherited settings must not decide the outcome.
        .env_remove("NO_COLOR")
        .env_remove("FORCE_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("AETHER_MODE");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run ae");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn assert_no_ansi(what: &str, s: &str) {
    assert!(
        !s.contains('\u{1b}'),
        "{what} contains an ANSI escape when writing to a pipe: {s:?}"
    );
}

#[test]
fn values_written_to_a_pipe_carry_no_escape_codes() {
    for expr in ["1 + 2", "[1, 2, 3]", "{a: 1}", "true", "null", "3.5"] {
        let (stdout, _) = run(&["-c", expr], &[]);
        assert_no_ansi(&format!("`ae -c {expr:?}` stdout"), &stdout);
    }
}

/// The exact assertion the Homebrew formula makes, which could not have passed
/// before this was fixed.
#[test]
fn arithmetic_is_exactly_the_number() {
    let (stdout, stderr) = run(&["-c", "1 + 2"], &[]);
    assert_eq!(
        stdout.trim(),
        "3",
        "stdout was {stdout:?} (stderr: {stderr:?})"
    );
}

#[test]
fn diagnostics_written_to_a_pipe_carry_no_escape_codes() {
    // stderr is redirected independently of stdout, so it needs its own check.
    let (_, stderr) = run(&["-c", "nope("], &[]);
    assert!(!stderr.is_empty(), "a parse error produced no diagnostic");
    assert_no_ansi("stderr", &stderr);
}

#[test]
fn no_color_is_honoured() {
    let (stdout, _) = run(&["-c", "[1, 2, 3]"], &[("NO_COLOR", "1")]);
    assert_no_ansi("stdout under NO_COLOR", &stdout);
}

/// The escape hatch: someone piping into `less -R` or a CI log that renders
/// colour must still be able to ask for it.
#[test]
fn colour_can_be_forced_back_on() {
    let (plain, _) = run(&["-c", "1 + 2"], &[]);
    let (forced, _) = run(&["-c", "1 + 2"], &[("FORCE_COLOR", "1")]);
    assert_no_ansi("unforced stdout", &plain);
    assert!(
        forced.contains('\u{1b}'),
        "FORCE_COLOR produced no colour ({forced:?}), so there is no way to get \
         colour through a pager"
    );
}

/// `NO_COLOR` is the stronger signal in every other tool that implements both.
#[test]
fn no_color_beats_force_color() {
    let (out, _) = run(&["-c", "1 + 2"], &[("FORCE_COLOR", "1"), ("NO_COLOR", "1")]);
    assert_no_ansi("stdout with both NO_COLOR and FORCE_COLOR", &out);
}
