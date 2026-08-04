//! A fixed program name is not enough: the *arguments* must not be able to
//! turn a benign-looking builtin into arbitrary execution.
//!
//! Two distinct defects, both found on 2026-08-04 and both verified by
//! execution rather than by reading:
//!
//! 1. **PowerShell injection (CWE-78).** Windows builtins build commands by
//!    interpolating values into single-quoted PowerShell literals, e.g.
//!    `format!("Start-Service '{}'", name)`. A value containing `'` closes the
//!    string and the rest is executed. Proof: a service name of
//!    `x'; New-Item -ItemType File -Path '<tmp>' -Force; '` created the file.
//!
//! 2. **Option injection (CWE-88).** `tar -cvf out.tar <files>` with a "file"
//!    named `--use-compress-program=sh -c '…'` runs that command; Info-ZIP's
//!    `-TT` does the same. Both were reachable with no policy gate, so they
//!    bypassed the `Effect::Exec` approval added the same day.

use aethershell::safety::{ps_quote, reject_option_like};

/// The escaping rule itself. PowerShell closes a single-quoted string on `'`
/// and escapes one by doubling it; nothing else is special in that context.
#[test]
fn ps_quote_neutralizes_the_quote_that_ends_the_string() {
    assert_eq!(ps_quote("plain"), "'plain'");

    // The exact payload that was demonstrated to execute.
    let attack = "x'; New-Item -ItemType File -Path 'C:\\tmp\\pwned' -Force; '";
    let quoted = ps_quote(attack);

    assert!(quoted.starts_with('\'') && quoted.ends_with('\''));

    // Every quote in the interior must be doubled, so the literal cannot be
    // terminated early. Strip the delimiters, then check the interior has no
    // odd-length run of quotes.
    let interior = &quoted[1..quoted.len() - 1];
    for run in interior.split(|c| c != '\'').filter(|r| !r.is_empty()) {
        assert!(
            run.len() % 2 == 0,
            "an odd run of quotes escapes the literal: {run:?}"
        );
    }
}

/// Nothing should be altered other than quotes — the value still has to be the
/// path the caller asked for.
#[test]
fn ps_quote_leaves_everything_else_alone() {
    for s in [
        r"C:\Program Files\thing.txt",
        "$env:PATH",
        "back`tick",
        "semi;colon",
        "a b c",
    ] {
        assert_eq!(
            ps_quote(s),
            format!("'{s}'"),
            "only the quote character needs escaping in a single-quoted literal"
        );
    }
}

/// Option-like positional arguments are refused before the tool ever sees them.
#[test]
fn option_like_paths_are_refused() {
    for payload in [
        "--use-compress-program=sh -c 'id'",
        "--to-command=sh -c 'id'",
        "-TTsh -c 'id'",
        "-I/bin/sh",
        "--exclude=x",
    ] {
        let r = reject_option_like("tar_create", &[payload.to_string()]);
        assert!(
            r.is_err(),
            "{payload:?} must be refused — several archivers execute it"
        );
    }
}

/// The check must not break ordinary paths, or callers will route around it.
#[test]
fn ordinary_paths_are_accepted() {
    let ok: Vec<String> = [
        "file.txt",
        "./file.txt",
        "dir/file.txt",
        r"C:\Users\x\file.txt",
        "/abs/path",
        // The documented escape hatch for a file genuinely named "-weird".
        "./-weird",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    assert!(
        reject_option_like("tar_create", &ok).is_ok(),
        "normal paths must pass, including the './-name' workaround"
    );
}
