//! `IRONSHELL_MAX_OUTPUT_BYTES` caps what a one-shot `ish -c` prints.
//!
//! The cut is on the printed result only: the exit status is decided before
//! anything is printed, errors are not cut, and unset means no cap at all.
//! Driven through the real binary, because the three print paths (human, agent,
//! deterministic) all live in `run_one`.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// The `ish` binary built alongside this test.
fn ish() -> PathBuf {
    let mut p = std::env::current_exe().expect("test exe path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join(format!("ish{}", std::env::consts::EXE_SUFFIX))
}

/// A string of 31 two-byte characters (62 bytes). Almost any cap lands in the
/// middle of one, which is the case the UTF-8 boundary rule is for.
const LONG: &str = r#""ééééééééééééééééééééééééééééééé""#;

fn run(args: &[&str], cap: Option<&str>) -> Output {
    let session = std::env::temp_dir().join(format!("ish-outcap-{}", std::process::id()));
    let mut cmd = Command::new(ish());
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("IRONSHELL_MODE")
        .env_remove("IRONSHELL_AGENT")
        .env_remove("IRONSHELL_DETERMINISTIC")
        .env_remove("IRONSHELL_MAX_OUTPUT_BYTES")
        .env("IRONSHELL_SESSION_DIR", &session);
    if let Some(cap) = cap {
        cmd.env("IRONSHELL_MAX_OUTPUT_BYTES", cap);
    }
    let out = cmd.output().expect("run ish");
    let _ = std::fs::remove_dir_all(&session);
    out
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("ish prints valid UTF-8")
}

/// The three print paths: human, agent (AECON) and deterministic (canonical).
const MODES: &[&[&str]] = &[&["-c"], &["--agent", "-c"], &["--deterministic", "-c"]];

#[test]
fn a_cut_lands_on_a_utf8_boundary_and_the_omission_is_counted() {
    for mode in MODES {
        let mut args: Vec<&str> = mode.to_vec();
        args.push(LONG);
        let full = run(&args, None);
        let full_out = text(&full.stdout);
        let body = full_out
            .strip_suffix('\n')
            .expect("the result ends in a newline");
        assert!(
            body.len() > 16,
            "{mode:?}: the result is too short: {body:?}"
        );

        for cap in [1usize, 2, 5, 9, 17] {
            let capped = run(&args, Some(&cap.to_string()));
            let out = text(&capped.stdout);
            let kept = out
                .strip_suffix('\n')
                .expect("a cut result still ends in a newline");
            let largest_boundary = (0..=cap)
                .rev()
                .find(|i| body.is_char_boundary(*i))
                .expect("0 is a boundary");
            assert_eq!(
                kept.len(),
                largest_boundary,
                "{mode:?} cap {cap}: cut at {} not at the last boundary {largest_boundary}",
                kept.len()
            );
            assert!(body.starts_with(kept), "{mode:?} cap {cap}: not a prefix");
            let omitted = body.len() - kept.len();
            let err = String::from_utf8_lossy(&capped.stderr);
            assert!(
                err.contains(&format!("{omitted} bytes omitted")),
                "{mode:?} cap {cap}: stderr does not count {omitted} omitted bytes: {err}"
            );
            assert_eq!(
                capped.status.code(),
                full.status.code(),
                "{mode:?} cap {cap}: a cut changed the exit status"
            );
        }
    }
}

#[test]
fn unset_means_the_whole_result_and_no_notice() {
    for mode in MODES {
        let mut args: Vec<&str> = mode.to_vec();
        args.push(LONG);
        let unset = run(&args, None);
        let big = run(&args, Some("1000000"));
        assert_eq!(
            unset.stdout, big.stdout,
            "{mode:?}: a cap above the size cut it"
        );
        assert!(
            !String::from_utf8_lossy(&unset.stderr).contains("omitted"),
            "{mode:?}: a notice with no cut"
        );
        assert!(
            !String::from_utf8_lossy(&big.stderr).contains("omitted"),
            "{mode:?}: a notice although the result fit"
        );
    }
}

#[test]
fn an_error_is_not_cut_and_keeps_its_exit_status() {
    // A parse error prints nothing on stdout; the cap must not touch its message.
    let plain = run(&["-c", "(("], None);
    let capped = run(&["-c", "(("], Some("1"));
    assert_eq!(
        capped.status.code(),
        plain.status.code(),
        "exit status changed"
    );
    assert_ne!(plain.status.code(), Some(0), "the probe must be an error");
    assert_eq!(capped.stdout, plain.stdout);
    let err = String::from_utf8_lossy(&capped.stderr);
    assert!(
        !err.contains("omitted"),
        "an error was reported as cut: {err}"
    );
    assert_eq!(
        String::from_utf8_lossy(&capped.stderr),
        String::from_utf8_lossy(&plain.stderr),
        "the cap changed the error message"
    );
}

#[test]
fn a_value_that_is_not_a_number_is_warned_about_and_does_not_cut() {
    let out = run(&["-c", LONG], Some("lots"));
    let full = run(&["-c", LONG], None);
    assert_eq!(out.stdout, full.stdout, "a bad cap cut the output");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("IRONSHELL_MAX_OUTPUT_BYTES") && err.contains("no cap applied"),
        "a bad cap was not reported: {err}"
    );
}
