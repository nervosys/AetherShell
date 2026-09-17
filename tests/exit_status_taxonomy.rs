//! A failure's exit status should say which kind of failure it was.
//!
//! Measured against bash, PowerShell and nushell on ten induced failures
//! (`benches/agentic/errors.mjs`), AetherShell scored **0 of 10** on
//! exit-status granularity: everything exited 1. bash scored 5 of 10 and was
//! the only shell offering anything on that axis. It is free signal — a
//! caller, a `set -e` wrapper, or a CI step can branch on it without reading
//! stderr — and the taxonomy to produce it already existed in `ErrorCode`; it
//! just never reached the process boundary.
//!
//! Two of the numbers deliberately borrow bash rather than improve on it:
//! **127** for a name that does not resolve and **2** for a syntax error, so
//! that an agent which already knows shell conventions gets them for free. The
//! rest follow `sysexits.h`. `E_UNKNOWN` stays 1, because an unidentified
//! fault should look like the generic failure it is.

use aethershell::safety::ErrorCode;
use std::process::Command;

fn exit_of(args: &[&str]) -> i32 {
    Command::new(env!("CARGO_BIN_EXE_ae"))
        .args(args)
        .env("NO_COLOR", "1")
        .env("AETHER_ALLOW_SH", "")
        .output()
        .expect("run ae")
        .status
        .code()
        .unwrap_or(-1)
}

#[test]
fn success_is_still_zero() {
    assert_eq!(exit_of(&["-c", "1 + 1"]), 0);
}

#[test]
fn an_unknown_name_exits_127_as_bash_does() {
    assert_eq!(exit_of(&["-c", "nope()"]), 127);
}

#[test]
fn a_syntax_error_exits_2_as_bash_does() {
    assert_eq!(exit_of(&["-c", "[1,2,3] | map(fn(x) => x *"]), 2);
    // The lexer's own failures take the same path.
    assert_eq!(exit_of(&["-c", "let x = @"]), 2);
}

#[test]
fn a_malformed_call_exits_64() {
    // EX_USAGE.
    assert_eq!(exit_of(&["-c", r#"starts_with("abc")"#]), 64);
}

#[test]
fn a_refusal_exits_77() {
    // EX_NOPERM. The sh() gate is the refusal an agent meets most.
    assert_eq!(exit_of(&["-c", r#"sh("echo hi")"#]), 77);
}

#[test]
fn an_unidentified_failure_stays_1() {
    // E_UNKNOWN must not claim a precision the shell does not have.
    assert_eq!(exit_of(&["-c", r#"cat("/nope/nothing-here")"#]), 1);
}

// ── the mapping itself ──────────────────────────────────────────────────

#[test]
fn every_code_has_a_status_and_the_distinct_ones_are_distinct() {
    use ErrorCode::*;
    // Success is reserved.
    for c in [
        PolicyDeny,
        NeedsApproval,
        OutsideWorkspace,
        BadArg,
        BudgetExceeded,
        UnknownBuiltin,
        UnknownField,
        Unknown,
    ] {
        let code = c.exit_code();
        assert!(
            code > 0,
            "{} maps to {code}, which reads as success",
            c.as_str()
        );
        assert!(
            code < 256,
            "{} maps to {code}, outside a process status",
            c.as_str()
        );
    }

    // The two borrowed from bash must be exactly bash's.
    assert_eq!(UnknownBuiltin.exit_code(), 127);

    // A refusal and a malformed call must not look alike: the first says stop,
    // the second says fix the call. That distinction is the point of the axis.
    assert_ne!(PolicyDeny.exit_code(), BadArg.exit_code());
    assert_ne!(BudgetExceeded.exit_code(), BadArg.exit_code());
}

#[test]
fn retryability_and_exit_status_tell_the_same_story() {
    use ErrorCode::*;
    // A code that invites a corrected retry must not share a status with one
    // that does not, or an agent branching on either gets a different answer
    // depending which it read.
    for retryable in [BadArg, UnknownBuiltin, UnknownField] {
        for terminal in [PolicyDeny, BudgetExceeded] {
            assert_ne!(
                retryable.exit_code(),
                terminal.exit_code(),
                "{} (retryable) and {} (terminal) share an exit status",
                retryable.as_str(),
                terminal.as_str()
            );
        }
    }
}

// ── non-vacuity ─────────────────────────────────────────────────────────

#[test]
fn non_vacuity_these_commands_actually_fail_the_way_the_test_assumes() {
    // If any of these started succeeding, its test above would be asserting a
    // status nothing produces.
    for src in [
        "nope()",
        "[1,2,3] | map(fn(x) => x *",
        r#"starts_with("abc")"#,
        r#"sh("echo hi")"#,
    ] {
        assert_ne!(exit_of(&["-c", src]), 0, "{src} no longer fails");
    }
    // And the binary must be discriminating rather than returning one number.
    let statuses = [
        exit_of(&["-c", "nope()"]),
        exit_of(&["-c", r#"starts_with("abc")"#]),
        exit_of(&["-c", r#"sh("echo hi")"#]),
    ];
    assert_eq!(
        statuses
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3,
        "three different failures produced the same status: {statuses:?}"
    );
}
