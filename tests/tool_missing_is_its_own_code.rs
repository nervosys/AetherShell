//! "The tool is not installed" is not an unidentified fault.
//!
//! `benches/agentic/uncoded.mjs` swept the catalogue and found 140 builtins
//! answering `E_UNKNOWN` -- the one code the taxonomy tells an agent *not* to
//! reason about. 57 of them were not defects in argument handling at all: the
//! external tool the builtin shells out to simply was not installed on the
//! measuring machine.
//!
//! That mattered twice over. It inflated a claim about the shell with a fact
//! about the laptop, and it reported as unidentifiable one of the most
//! identifiable failures the shell has. `E_TOOL_MISSING` says it plainly, and
//! says the one thing that helps: install the tool. No correction to the call
//! will.

use aethershell::safety::ErrorCode;

/// A tool no machine running this suite will have installed.
const ABSENT: &str = "definitely-not-installed-94117";

#[test]
fn the_code_carries_the_right_contract() {
    let e = ErrorCode::ToolMissing;
    assert_eq!(e.as_str(), "E_TOOL_MISSING");

    // Not retryable: the same call fails identically until someone installs
    // the tool, which is an action outside this process. An agent that retries
    // burns budget for nothing.
    assert!(!e.retryable(), "retrying cannot fix a missing tool");

    // 127 is bash's "command not found". A missing external tool is that
    // condition one level out, and an agent that knows shell reads it for free.
    assert_eq!(e.exit_code(), 127);
    assert_eq!(
        ErrorCode::UnknownBuiltin.exit_code(),
        e.exit_code(),
        "both are a name that does not resolve to something runnable"
    );

    // Non-vacuity: the codes must actually differ from each other, or the
    // assertions above hold for a taxonomy that collapsed to one value.
    assert_ne!(e.as_str(), ErrorCode::Unknown.as_str());
    assert_ne!(e.exit_code(), ErrorCode::BadArg.exit_code());
}

#[test]
fn the_error_names_the_tool_and_what_to_do() {
    let err = aethershell::safety::tool_missing("fmt_python", ABSENT, "No such file or directory");
    let msg = err.to_string();

    assert!(msg.contains("E_TOOL_MISSING"), "not coded: {msg}");
    assert!(msg.contains(ABSENT), "does not name the tool: {msg}");
    assert!(
        msg.contains("install"),
        "does not say the one thing that helps: {msg}"
    );
    assert!(
        msg.contains("no change to the call will help"),
        "an agent should be told not to retry with a corrected call: {msg}"
    );
    // The builtin is still identified, so a caller can tell *which* call failed.
    assert!(
        msg.contains("fmt_python"),
        "does not name the builtin: {msg}"
    );
}

#[test]
fn a_builtin_whose_tool_is_absent_reports_it() {
    // End to end, through the real dispatcher. `black` is a Python formatter;
    // if it happens to be installed here this asserts nothing, so the test says
    // so rather than passing quietly.
    let mut env = aethershell::env::Env::new();
    let out = aethershell::builtins::call(
        "black",
        vec![aethershell::value::Value::Str(".".into())],
        &mut env,
    );
    match out {
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("E_TOOL_MISSING"),
                "a builtin shelling out to an absent tool must say so: {msg}"
            );
            assert!(!msg.contains("E_UNKNOWN"), "{msg}");
        }
        Ok(_) => {
            eprintln!("note: `black` is installed on this machine, so this case was not exercised")
        }
    }
}
