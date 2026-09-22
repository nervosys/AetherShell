//! A prompt nobody can answer must not be answered.
//!
//! `benches/agentic/uncoded.mjs` was scoring five builtins as "accepted it and
//! answered" because a killed call yields empty output. Teaching it to detect
//! the timeout kill showed what they were really doing, and it was worse than
//! hanging.
//!
//! `A2UIChannel` is a buffer: `send` queues an event whether or not a UI is
//! listening, and nothing tracked whether one ever drained it. So
//! `wait_for_response` waited its full **300-second** timeout and returned
//! `Cancelled`, which `prompt_confirm` maps to `Ok(false)`. An agent calling
//! `a2ui_confirm` with no UI stalled five minutes and received `false` --
//! indistinguishable from the user having declined.
//!
//! That is the defect class this whole body of work is about, with the sharpest
//! possible payload: the invented value is a **decision**. An agent told "the
//! user declined" will not retry, will not escalate, and will report that the
//! human said no.

use aethershell::safety::ErrorCode;

#[test]
fn the_code_says_nobody_is_reachable_rather_than_ask_again() {
    let e = ErrorCode::NoUi;
    assert_eq!(e.as_str(), "E_NO_UI");

    // Not retryable: no correction to the call attaches a UI.
    assert!(!e.retryable(), "retrying cannot conjure a listener");

    // EX_UNAVAILABLE -- the thing that would have answered is not there.
    assert_eq!(e.exit_code(), 69);

    // The distinction that justifies a separate code: `NeedsApproval` means a
    // human must decide and *can* be asked, so it is retryable. This means
    // nobody is reachable. Collapsing them would tell an agent to wait for
    // someone who is not coming.
    assert!(
        ErrorCode::NeedsApproval.retryable(),
        "test assumes NeedsApproval is the retryable one"
    );
    assert_ne!(e.as_str(), ErrorCode::NeedsApproval.as_str());
    assert_ne!(e.as_str(), ErrorCode::Unknown.as_str());
}

#[test]
fn the_error_names_the_way_out() {
    let msg = aethershell::safety::no_ui("a2ui_confirm").to_string();
    assert!(msg.contains("E_NO_UI"), "not coded: {msg}");
    assert!(
        msg.contains("a2ui_confirm"),
        "does not name the caller: {msg}"
    );
    assert!(
        msg.contains("--tui") || msg.contains("attach"),
        "does not say how to make the call work: {msg}"
    );
}

#[test]
fn an_unattached_prompt_refuses_instead_of_inventing_a_refusal() {
    // The whole point: `false` was a *decision* the shell made up. Whatever
    // comes back now, it must not be a plain boolean.
    let mut env = aethershell::env::Env::new();
    let out = aethershell::builtins::call(
        "a2ui_confirm",
        vec![aethershell::value::Value::Str("proceed?".into())],
        &mut env,
    );
    match out {
        Ok(v) => panic!(
            "a prompt with no UI answered {v:?}; an agent cannot tell that from \
             the user declining"
        ),
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains("E_NO_UI"), "wrong code: {msg}");
        }
    }
}

#[test]
fn it_refuses_promptly_rather_than_after_the_timeout() {
    // The old path waited 300 seconds. A bound well under that, but loose
    // enough not to flake on a loaded machine: anything in this range proves
    // the wait was skipped, and the test cannot pass by being slow.
    let mut env = aethershell::env::Env::new();
    let start = std::time::Instant::now();
    let _ = aethershell::builtins::call(
        "a2ui_confirm",
        vec![aethershell::value::Value::Str("proceed?".into())],
        &mut env,
    );
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "took {elapsed:?}; the 300-second wait is back"
    );
}

#[test]
fn a_channel_that_has_been_read_is_still_allowed_to_wait() {
    // Non-vacuity, and the guard against over-fixing: refusing every prompt in
    // agent mode would have been simpler and wrong, because agent-plus-attached
    // -TUI is what A2UI is *for*. Draining the channel marks it attached, and
    // an attached channel must go back to waiting rather than refusing.
    let ch = aethershell::ai::a2ui::A2UIChannel::new();
    assert!(!ch.ui_attached(), "a fresh channel has no UI");
    let _ = ch.receive_all().expect("drain");
    assert!(
        ch.ui_attached(),
        "draining the channel is what a UI does; it must count as attached"
    );
}
