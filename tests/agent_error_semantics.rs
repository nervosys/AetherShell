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
fn unknown_builtin_is_retryable_only_when_a_real_name_is_offered() {
    // A misspelling with a candidate is retried under that candidate.
    let with = safety::unknown_builtin("file_read", vec!["file.read".to_string()]);
    let j = structured(&with).to_json();
    assert_eq!(j["error"]["code"], "E_UNKNOWN_BUILTIN");
    assert_eq!(j["error"]["retryable"], true);
    assert_eq!(j["error"]["did_you_mean"][0], "file.read");
    // With nothing close, the same name fails again.
    let without = safety::unknown_builtin("nosuchthing", Vec::new());
    assert_eq!(structured(&without).to_json()["error"]["retryable"], false);
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

#[test]
fn uniq_is_described_as_adjacent_only_and_points_to_unique() {
    // uniq removes adjacent duplicates only (like Unix uniq). The description
    // must say so and name the all-duplicates builtin. Behaviour is unchanged.
    let sig = iron_shell::signature::signature_of("uniq").expect("uniq has a signature");
    assert!(sig.doc.contains("ADJACENT"), "{}", sig.doc);
    assert!(
        sig.doc.contains("arr.unique") && sig.doc.contains("unique"),
        "{}",
        sig.doc
    );
    assert!(
        !sig.doc.contains("Remove duplicate elements."),
        "{}",
        sig.doc
    );
    // Behaviour: adjacent only, so the second 1 survives.
    let prog = parse_program("[1, 1, 2, 1] | uniq").expect("parse");
    let v = eval_program(&prog, &mut Env::new()).expect("eval");
    match v {
        iron_shell::value::Value::Array(a) => {
            assert_eq!(a.len(), 3, "uniq must stay adjacent-only")
        }
        other => panic!("uniq returned {other:?}, not an Array"),
    }
}

#[test]
fn math_sqrt_with_three_arguments_says_at_most_one_not_zero() {
    // sqrt takes its subject as the one argument. The bound must count it.
    let (code, text) = agent_run("math.sqrt(1,2,3)");
    assert_eq!(code, 64, "{text}");
    assert!(text.contains("at most 1 argument(s)"), "{text}");
    assert!(!text.contains("at most 0"), "{text}");
}

#[test]
fn an_error_at_end_of_input_names_the_last_line_with_content() {
    // A trailing newline must not move the report onto a line that is not there.
    let cases = [
        ("x = (1 +", "line 1,"),
        ("x = (1 +\n", "line 1,"),
        ("x = (1 +\n\n\n", "line 1,"),
        ("a = 1\nb = 2\nc = (3 +\n", "line 3,"),
    ];
    for (src, want) in cases {
        let err = parse_program(src).expect_err(src).to_string();
        assert!(err.contains(want), "{src:?}: expected {want} in {err}");
        assert!(
            !err.contains("line 2,"),
            "{src:?} reported a line past the source: {err}"
        );
    }
}

#[test]
fn a_missing_path_hint_says_where_a_relative_path_resolves() {
    let (code, text) = agent_run(r#"file.read("no-such-d3-file.txt")"#);
    assert_eq!(code, 66, "{text}");
    assert!(text.contains("resolved against"), "{text}");
    assert!(!text.contains("use an identifier"), "{text}");
}

/// Run `ish --agent -c <src>` in a scratch directory and return (exit, stdout+stderr).
fn agent_run(src: &str) -> (i32, String) {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("agent_error_semantics");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ish"))
        .args(["--agent", "-c", src])
        .current_dir(&dir)
        .env("IRONSHELL_SESSION_DIR", dir.join("session"))
        .env("NO_COLOR", "1")
        .env("IRONSHELL_ALLOW_SH", "")
        .output()
        .expect("run ish");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

#[test]
fn an_unknown_name_is_reported_unknown_before_the_approval_gate() {
    // `exec` is classified Exec by name, but no implementation serves it.
    let (code, text) = agent_run(r#"exec("x")"#);
    assert!(text.contains("E_UNKNOWN_BUILTIN"), "{text}");
    assert!(!text.contains("E_NEEDS_APPROVAL"), "{text}");
    assert_eq!(code, 127, "{text}");
}

#[test]
fn did_you_mean_offers_the_module_spelling_ahead_of_edit_distance_guesses() {
    // `file_read` is not a builtin; `file.read` is the call the agent meant.
    let dym = iron_shell::builtins::did_you_mean("file_read");
    assert_eq!(
        dym.first().map(String::as_str),
        Some("file.read"),
        "{dym:?}"
    );
    // A name whose prefix is not a module gets no module-form guess.
    let none = iron_shell::builtins::did_you_mean("nosuch_thing");
    assert!(none.iter().all(|s| !s.contains('.')), "{none:?}");
}

#[test]
fn the_agent_sees_the_module_spelling_in_did_you_mean() {
    let (code, text) = agent_run(r#"file_read("a.txt")"#);
    assert_eq!(code, 127, "{text}");
    assert!(text.contains("E_UNKNOWN_BUILTIN"), "{text}");
    assert!(text.contains(r#""did_you_mean":["file.read""#), "{text}");
}

#[test]
fn a_served_gated_builtin_still_needs_approval() {
    // The gate must still stop a real destructive builtin. The target does not
    // exist, and the refusal comes before any action is taken.
    let (code, text) = agent_run(r#"rm("no-such-file-d3.txt")"#);
    assert!(text.contains("E_NEEDS_APPROVAL"), "{text}");
    assert_eq!(code, 75, "{text}");
}
