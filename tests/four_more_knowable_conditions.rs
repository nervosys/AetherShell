//! Four conditions the taxonomy was reporting as unidentifiable.
//!
//! The catalogue sweep (`benches/agentic/uncoded.mjs`) got the shell's own
//! uncoded failures down to sixteen and then stalled, because the remaining
//! sixteen were described as "argument and type errors" and were nothing of
//! the kind. Reading them one at a time -- which is what the sweep is for --
//! they were six distinct conditions wearing one label:
//!
//! | builtin | said | actually |
//! | --- | --- | --- |
//! | `crypto.verify_signature` | `E_UNKNOWN` | the shell does not do this |
//! | `tx_commit` | `E_UNKNOWN` | no transaction is open |
//! | `finetune_status` | `E_UNKNOWN` | that job does not exist |
//! | `docker_ps` | `E_UNKNOWN` | docker ran and failed |
//! | `head` | `E_UNKNOWN` | a genuine argument error |
//! | `gui_dialog_file_save` | *nothing, ever* | nobody is there to answer |
//!
//! Only one of the six was the argument error the label claimed. Two of them
//! -- `crypto.cert_parse` and `crypto.verify_cert` -- were worse than uncoded:
//! they reported `E_BAD_ARG`, telling an agent to retry with different
//! arguments when no arguments would ever work.

use aethershell::safety::ErrorCode;

/// Every code's exit status and retryability, asserted together so that a
/// taxonomy that quietly collapsed two variants into one would fail here
/// rather than in a benchmark six months later.
#[test]
fn the_four_codes_carry_distinct_contracts() {
    for (code, text, exit, retryable) in [
        (ErrorCode::Unimplemented, "E_UNIMPLEMENTED", 70, false),
        (ErrorCode::BadState, "E_BAD_STATE", 76, true),
        (ErrorCode::NotFound, "E_NOT_FOUND", 66, true),
        (ErrorCode::ToolFailed, "E_TOOL_FAILED", 69, false),
    ] {
        assert_eq!(code.as_str(), text);
        assert_eq!(code.exit_code(), exit, "{text} exit status");
        assert_eq!(code.retryable(), retryable, "{text} retryability");
    }

    // `BadState` is the odd one: retryable, but not by correcting this call.
    // The hint has to name the call that establishes the state, or "retry"
    // means "run the identical thing again", which is exactly the loop the
    // `retryable` flag exists to prevent.
    let e = aethershell::safety::bad_state("tx_commit", "no active transaction", "tx_begin");
    let rendered = e.to_string();
    assert!(
        rendered.contains("tx_begin"),
        "a retryable state error must name the prerequisite; got: {rendered}"
    );

    // Non-vacuity: four codes, four distinct strings. A copy-paste that left
    // two variants sharing an `as_str` would satisfy every assertion above.
    let names = [
        ErrorCode::Unimplemented.as_str(),
        ErrorCode::BadState.as_str(),
        ErrorCode::NotFound.as_str(),
        ErrorCode::ToolFailed.as_str(),
        ErrorCode::ToolMissing.as_str(),
        ErrorCode::Unknown.as_str(),
    ];
    let mut seen = names.to_vec();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen.len(),
        names.len(),
        "two codes share a string: {names:?}"
    );
}

/// `ToolMissing` and `ToolFailed` are opposite ends of the same call, and the
/// advice that fits one is actively wrong for the other.
#[test]
fn a_tool_that_ran_is_not_a_tool_that_is_absent() {
    let missing = aethershell::safety::tool_missing("eza", "eza", "No such file or directory");
    let failed = aethershell::safety::tool_failed("eza", "eza -la", Some(2), "bad flag");

    assert!(missing.to_string().contains("not installed"));
    assert!(
        !failed.to_string().contains("not installed"),
        "eza IS installed here; telling an agent to install it is advice that \
         cannot work: {failed}"
    );
    assert!(
        failed.to_string().contains("exited 2"),
        "the tool's own exit status is the actionable part: {failed}"
    );
}

/// The empty-stderr case, which is what made `docker_ps` unreadable.
///
/// The old message was `"docker ps --format {{json .}} failed: "` -- it ended
/// in a colon and said nothing, because the daemon was not running and docker
/// printed nothing. A message that trails off reads as a truncation bug; the
/// silence was the entire diagnosis.
#[test]
fn an_empty_stderr_is_stated_not_rendered_as_silence() {
    let e = aethershell::safety::tool_failed("docker_ps", "docker ps", Some(1), "   ");
    let s = e.to_string();
    assert!(
        s.contains("printed nothing to stderr"),
        "an empty stderr must be said out loud; got: {s}"
    );
    assert!(
        !s.trim_end().ends_with(':'),
        "a message ending in a colon looks truncated: {s}"
    );

    // And a signal kill is not an exit code.
    let killed = aethershell::safety::tool_failed("docker_ps", "docker ps", None, "");
    assert!(
        killed.to_string().contains("killed by a signal"),
        "None is not exit 0: {killed}"
    );
}

/// An interactive desktop dialog must refuse in agent mode rather than block.
///
/// This is the one defect here that a test could never have found by reading
/// the code: the failure mode is *not returning*, so a suite that called it
/// would hang rather than fail. It took a sweep with a per-call timeout --
/// `gui_dialog_file_save` was killed at ten seconds -- to see it at all.
#[test]
fn an_interactive_dialog_refuses_rather_than_blocks() {
    // The only test in this binary that touches the environment.
    std::env::set_var("AETHER_MODE", "agent");

    let e = aethershell::safety::refuse_if_headless("gui_dialog_file_save")
        .expect_err("agent mode has no human to dismiss a dialog");
    assert!(
        e.to_string().contains("no UI"),
        "expected E_NO_UI, got: {e}"
    );

    // Non-vacuity: it must NOT refuse for a human at a terminal, or this is a
    // test that GUI dialogs are simply disabled.
    std::env::remove_var("AETHER_MODE");
    std::env::remove_var("AETHER_AGENT");
    assert!(
        aethershell::safety::refuse_if_headless("gui_dialog_file_save").is_ok(),
        "human mode still has a desktop; the guard is for agent mode only"
    );
}

/// A failure to *start* a process is classified, not propagated raw.
///
/// `Command::new(prog).output()?` was the single commonest uncoded failure in
/// the shell: 346 sites propagating a bare `io::Error`, so what reached an
/// agent was `No such file or directory (os error 2)` -- no code, no builtin,
/// not even the name of the program that was missing. Thirteen builtins were
/// still answering exactly that *after* `E_TOOL_MISSING` shipped, because that
/// code had been added only at the sites which already named their tool.
#[test]
fn a_process_that_cannot_start_says_which_one_and_why() {
    use std::io::{Error, ErrorKind};

    let absent = aethershell::safety::spawn_error(
        "env_go",
        "go",
        &Error::new(
            ErrorKind::NotFound,
            "No such file or directory (os error 2)",
        ),
    );
    let s = absent.to_string();
    assert!(
        s.contains("E_TOOL_MISSING"),
        "expected a coded failure: {s}"
    );
    assert!(
        s.contains("go"),
        "the program's name is the one thing the bare io::Error lost: {s}"
    );

    // Not every spawn failure is an absent tool, and "install it" would be
    // advice that cannot work for the rest.
    let unusable = aethershell::safety::spawn_error(
        "env_go",
        "go",
        &Error::new(ErrorKind::PermissionDenied, "permission denied"),
    );
    let u = unusable.to_string();
    assert!(
        !u.contains("is not installed"),
        "go IS installed; it could not be run: {u}"
    );
    assert!(u.contains("E_TOOL_FAILED"), "expected E_TOOL_FAILED: {u}");

    // Non-vacuity: the two branches must actually differ, or this asserts that
    // one message happens to contain two substrings.
    assert_ne!(s, u, "both spawn failures rendered identically");
}

/// A shell-out that never returns must be given up on.
///
/// `Command::output()` waits forever, which is right for an interactive shell
/// and wrong for a builtin an agent calls: a hang burns the whole turn and
/// returns nothing to reason about, where a failure can at least be branched
/// on. Both hangs this found were invisible to the test suite, because a test
/// that calls one does not fail -- it stops CI.
#[test]
fn a_shell_out_that_never_returns_is_given_up_on() {
    // `sleep 30` is the cheapest reliable hang there is. Windows `timeout`
    // is not a substitute: it reads the console and exits immediately when
    // stdin is null, which this helper makes it. `ping -n 31` waits.
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("ping");
        c.args(["-n", "31", "127.0.0.1"]);
        c
    } else {
        let mut c = std::process::Command::new("sleep");
        c.arg("30");
        c
    };
    cmd.env("LC_ALL", "C");

    let began = std::time::Instant::now();
    let r = aethershell::safety::output_with_timeout(cmd, 1);
    let took = began.elapsed();

    match r {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // No `sleep` on this host; nothing was measured, so claim nothing.
            eprintln!("skipped: no sleep/timeout binary here");
            return;
        }
        Err(e) => assert_eq!(
            e.kind(),
            std::io::ErrorKind::TimedOut,
            "expected a timeout: {e}"
        ),
        Ok(o) => panic!("a 30s sleep returned inside 1s: {o:?}"),
    }

    // Non-vacuity in the other direction: it must give up *near* the deadline,
    // not merely eventually. A guard that returns after 30 seconds has not
    // prevented the hang, it has renamed it.
    assert!(
        took < std::time::Duration::from_secs(10),
        "gave up only after {took:?}; that is not a timeout"
    );
}

/// ...and a command that finishes must still hand back its output intact.
///
/// The obvious way to write the guard above -- poll `try_wait` and read the
/// pipes afterwards -- deadlocks as soon as a child writes more than a pipe
/// buffer, which would turn a timeout into a new way to hang. This is the
/// test for that, so it deliberately asks for more output than a pipe holds.
#[test]
fn a_bounded_shell_out_still_returns_everything_it_printed() {
    let mut cmd = std::process::Command::new(if cfg!(windows) { "cmd" } else { "sh" });
    if cfg!(windows) {
        cmd.args(["/C", "echo hello"]);
    } else {
        // ~500 KB, comfortably past a 64 KB pipe buffer.
        cmd.args(["-c", "yes hello | head -c 500000"]);
    }

    match aethershell::safety::output_with_timeout(cmd, 30) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipped: no shell binary here");
        }
        Err(e) => panic!("a fast command timed out: {e}"),
        Ok(o) => {
            assert!(o.status.success(), "command failed: {o:?}");
            assert!(
                o.stdout.len() > 100_000 || cfg!(windows),
                "only {} bytes came back; the pipe was not drained",
                o.stdout.len()
            );
        }
    }
}

/// A builtin that does nothing must not report it as an ordinary `false`.
///
/// Nine builtins in the catalogue were `fn(_args, _input) -> Ok(Bool(false))`:
/// they ignored their arguments entirely and always answered "no". An agent
/// calling `user_lock("alice")` got `false` at exit 0, which is
/// indistinguishable from "the lock failed" — so it might retry, or route
/// around, or believe the account was already locked. The truth was that
/// AetherShell does not implement account locking at all, while the ontology
/// listed it as a callable capability.
///
/// Found by `benches/agentic/silent-success.mjs`, which asks a question the
/// uncoded sweeps do not: of the builtins that ANSWER a nonsense argument,
/// which answer with a value carrying no information?
#[test]
fn an_unimplemented_builtin_says_so_rather_than_returning_false() {
    let jail = std::env::temp_dir().join(format!("ae_stub_{}", std::process::id()));
    std::fs::create_dir_all(&jail).expect("create jail");

    for name in [
        // Answered `false`.
        "user_lock",
        "user_unlock",
        "cron_enable",
        "cron_disable",
        "at_remove",
        "acl_set",
        "session_undo",
        "session_redo",
        "net_send",
        // Answered an empty collection, which reads as a definitive result:
        // `code_references("foo")` said there were none.
        "code_references",
        "code_callers",
        "test_passing",
        "test_failing",
        "search_semantic",
        "docs_signatures",
        // Answered the ADVICE as the return value. `refactor_rename` handed
        // back the String "Use IDE rename functionality" at exit 0, which an
        // agent cannot tell from renamed code, and `docs_api` returned a
        // documentation path as though it had generated the documentation.
        "refactor_rename",
        "refactor_extract_function",
        "input_form",
        "test_watch",
        "docs_api",
    ] {
        let mut env = aethershell::env::Env::new();
        let r = aethershell::builtins::call_with_input(
            name,
            vec![aethershell::value::Value::Str("anything".to_string())],
            None,
            &mut env,
        );
        match r {
            Ok(v) => panic!("{name} answered {v:?} instead of saying it is not implemented"),
            Err(e) => {
                let s = e.to_string();
                assert!(
                    s.contains("E_UNIMPLEMENTED"),
                    "{name} should be E_UNIMPLEMENTED; got: {s}"
                );
                // The safety consequence has to be in the message, not the
                // hint: "nothing was changed" is the part that stops an agent
                // assuming the effect happened.
                // The consequence must be stated in the message, in capitals,
                // because that is the part an agent reads first and the part
                // that stops it assuming the effect happened. The wording
                // differs on purpose: an effectful builtin changed nothing, a
                // read-only one performed no analysis, and `crypto.verify_*`
                // verified nothing -- collapsing those into one phrase would
                // lose the consequence that actually matters in each case.
                const SAID_NOTHING_HAPPENED: &[&str] = &[
                    "NOTHING WAS CHANGED",
                    "NOTHING WAS DONE",
                    "NO ANALYSIS WAS PERFORMED",
                    "NOTHING WAS VERIFIED",
                ];
                assert!(
                    SAID_NOTHING_HAPPENED.iter().any(|p| s.contains(p)),
                    "{name} must say the effect did not happen: {s}"
                );
                // The specific pathology: advice must not come back as a
                // value. `Err` already guarantees that here, but the point
                // is worth asserting rather than assuming -- the whole
                // failure was that this text looked like a result.
                assert!(
                    !s.starts_with("Use "),
                    "{name} is still answering with instructions: {s}"
                );
            }
        }
    }

    let _ = std::fs::remove_dir_all(&jail);
}

/// The discovery surface must be byte-stable, or an agent cannot cache it.
///
/// `tools()` collected from a `HashMap`, whose iteration order Rust randomises
/// per process, so it returned the same set in a different order on every
/// call. That is the agent-facing tool catalogue — the thing an agent fetches
/// once and caches — and it could not be diffed, cached or hashed.
///
/// Determinism is one of the four axes `docs/TYPED_SHELL_RESPONSE.md` argues
/// on, and E1 measured it over *data queries* only. Nobody had pointed it at
/// the discovery surface itself. It was found by a differential probe for a
/// different defect, which had to exclude nondeterministic builtins and so
/// listed them.
#[test]
fn the_tool_catalogue_is_byte_stable() {
    use aethershell::value::Value;

    let call = || {
        let mut env = aethershell::env::Env::new();
        aethershell::builtins::call_with_input("tools", vec![], None, &mut env)
            .expect("tools() should answer")
    };

    let first = call();

    // Names in order, which is what an order bug actually perturbs.
    let names_of = |v: &Value| -> Vec<String> {
        match v {
            Value::Array(items) => items
                .iter()
                .filter_map(|it| match it {
                    Value::Record(r) => match r.get("name") {
                        Some(Value::Str(s)) => Some(s.clone()),
                        _ => None,
                    },
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    };

    let baseline = names_of(&first);
    assert!(
        baseline.len() > 20,
        "only {} tools listed; this asserts stability over nothing",
        baseline.len()
    );

    // Five rounds: a HashMap order clash can repeat by chance, and a single
    // agreeing pair is exactly what let this ship.
    for round in 0..5 {
        assert_eq!(
            names_of(&call()),
            baseline,
            "tools() changed order on round {round}; it cannot be cached"
        );
    }

    // Non-vacuity: sorted order is the property being claimed, so check it
    // rather than only that two runs agree — two unsorted runs can agree.
    let mut sorted = baseline.clone();
    sorted.sort();
    assert_eq!(baseline, sorted, "tools() is stable but not sorted");
}
