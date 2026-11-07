//! What an agent is told about a failure has to be true.
//!
//! Found by testing the released 14.0.0 binary as an agent. Three failures
//! reported `retryable: true` although the same call fails identically every
//! time: an unknown builtin, a path outside the workspace, and a zero divisor.
//! The outside-workspace hint also told the agent to set `IRONSHELL_WORKSPACE`,
//! which is the operator's decision, not the agent's.

use iron_shell::env::Env;
use iron_shell::safety::{self, ErrorCode, SafetyError};
use iron_shell::{eval::eval_program, parser::parse_program};

fn structured(e: &anyhow::Error) -> &SafetyError {
    e.downcast_ref::<SafetyError>()
        .unwrap_or_else(|| panic!("not a structured error: {e:#}"))
}

#[test]
fn retryable_is_false_where_the_same_call_fails_again() {
    assert!(!ErrorCode::UnknownBuiltin.retryable());
    assert!(!ErrorCode::OutsideWorkspace.retryable());
    assert!(!ErrorCode::BadValue.retryable());
}

#[test]
fn retryable_stays_true_where_a_corrected_call_can_succeed() {
    assert!(ErrorCode::BadArg.retryable());
    assert!(ErrorCode::NotFound.retryable());
    assert!(ErrorCode::NeedsApproval.retryable());
}

#[test]
fn unknown_builtin_json_is_not_retryable_and_keeps_did_you_mean() {
    let e = safety::unknown_builtin("file_read", vec!["file.read".to_string()]);
    let j = structured(&e).to_json();
    assert_eq!(j["error"]["code"], "E_UNKNOWN_BUILTIN");
    assert_eq!(j["error"]["retryable"], false);
    assert_eq!(j["error"]["did_you_mean"][0], "file.read");
}

#[test]
fn zero_divisor_is_coded_bad_arg_not_retryable_with_a_real_hint() {
    for src in ["1 / 0", "1 % 0"] {
        let prog = parse_program(src).expect("parse");
        let e = eval_program(&prog, &mut Env::new()).expect_err(src);
        let j = structured(&e).to_json();
        assert_eq!(j["error"]["code"], "E_BAD_ARG", "{src}");
        assert_eq!(j["error"]["retryable"], false, "{src}");
        let hint = j["error"]["hint"].as_str().unwrap_or_default();
        assert!(
            !hint.contains("argument count"),
            "{src}: the hint is the generic arity hint: {hint}"
        );
        assert!(hint.contains("divisor"), "{src}: {hint}");
    }
}

#[test]
fn outside_workspace_hint_names_the_root_and_never_tells_the_agent_to_widen() {
    let root = safety::workspace_root().display().to_string();
    let e = safety::outside_workspace("file_write", "/elsewhere/x.txt");
    let j = structured(&e).to_json();
    assert_eq!(j["error"]["code"], "E_OUTSIDE_WORKSPACE");
    assert_eq!(j["error"]["retryable"], false);
    let hint = j["error"]["hint"].as_str().unwrap_or_default();
    assert!(
        hint.contains(&root),
        "hint must name the root {root}: {hint}"
    );
    assert!(
        hint.contains("inside"),
        "hint must say to use a path inside: {hint}"
    );
    assert!(
        !hint.contains("IRONSHELL_WORKSPACE") && !hint.contains("--workspace"),
        "hint must not tell the agent how to widen its jail: {hint}"
    );
    assert_eq!(safety::outside_workspace_hint(), hint);
}
