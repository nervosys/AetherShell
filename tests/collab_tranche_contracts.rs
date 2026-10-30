//! Contracts for the nanda, workspace, repl, sess, a2a and job builtins.
//!
//! Each builtin here either keeps real in-process state or refuses with a coded
//! error. None of these tests opens a socket, reaches a model, or runs a command:
//! the subprocess checks run `ish` with a hard deadline, a disposable workspace and
//! session directory, and a network budget of zero where a model call could start.

use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use iron_shell::builtins::{call, call_with_input};
use iron_shell::env::Env;
use iron_shell::value::Value;

/// The builtins below share process-wide tables (proposals, workspaces, sessions,
/// the A2A registry). Each test holds this lock so none sees another's state.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

fn record(pairs: Vec<(&str, Value)>) -> Value {
    Value::Record(
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<BTreeMap<String, Value>>(),
    )
}

fn fields(v: Value) -> BTreeMap<String, Value> {
    match v {
        Value::Record(r) => r,
        other => panic!("expected a Record, got {other:?}"),
    }
}

fn ok(name: &str, args: Vec<Value>) -> Value {
    let mut env = Env::new();
    call(name, args, &mut env).unwrap_or_else(|e| panic!("{name} refused a valid call: {e}"))
}

/// The refusal text of a call that must fail.
fn refusal(name: &str, args: Vec<Value>) -> String {
    let mut env = Env::new();
    match call(name, args, &mut env) {
        Ok(v) => panic!("{name} answered {v:?} where it should refuse"),
        Err(e) => e.to_string(),
    }
}

fn expect_code(name: &str, args: Vec<Value>, code: &str) {
    let msg = refusal(name, args);
    assert!(msg.contains(code), "{name}: wanted {code}, got: {msg}");
}

fn text(r: &BTreeMap<String, Value>, key: &str) -> String {
    match r.get(key) {
        Some(Value::Str(s)) => s.clone(),
        other => panic!("{key}: expected a String, got {other:?}"),
    }
}

/// Abort every pending proposal, so one test's leftovers cannot fill the store
/// and fail the next one.
fn clear_pending_nanda() {
    let all = match ok("nanda_status", vec![]) {
        Value::Array(all) => all,
        other => panic!("nanda_status() gave {other:?}"),
    };
    for p in all {
        let p = fields(p);
        if text(&p, "status") == "pending" {
            ok("nanda_abort", vec![s(&text(&p, "id"))]);
        }
    }
}

fn nanda_propose(electorate: i64, threshold: f64) -> String {
    clear_pending_nanda();
    let rec = fields(ok(
        "nanda_propose",
        vec![
            s("collab-test"),
            record(vec![
                ("electorate", Value::Int(electorate)),
                ("threshold", Value::Float(threshold)),
            ]),
        ],
    ));
    text(&rec, "id")
}

// ── nanda ───────────────────────────────────────────────────────────────────

#[test]
fn nanda_propose_refuses_a_threshold_outside_zero_one() {
    let _g = serial();
    for bad in [
        Value::Float(0.0),
        Value::Float(1.5),
        Value::Int(0),
        s("half"),
    ] {
        expect_code(
            "nanda_propose",
            vec![s("p"), record(vec![("threshold", bad)])],
            "E_BAD_ARG",
        );
    }
    // An Int threshold of 1 is read as 1.0, not silently replaced with the default.
    clear_pending_nanda();
    let rec = fields(ok(
        "nanda_propose",
        vec![s("p"), record(vec![("threshold", Value::Int(1))])],
    ));
    assert_eq!(rec.get("threshold"), Some(&Value::Float(1.0)));
}

#[test]
fn nanda_propose_refuses_an_electorate_outside_range() {
    let _g = serial();
    for bad in [Value::Int(0), Value::Int(1025), s("many")] {
        expect_code(
            "nanda_propose",
            vec![s("p"), record(vec![("electorate", bad)])],
            "E_BAD_ARG",
        );
    }
}

#[test]
fn nanda_commit_needs_quorum_and_refuses_a_second_commit() {
    let _g = serial();
    let id = nanda_propose(3, 0.5);

    // One ballot of three is not half the electorate: no quorum, no commit.
    ok("nanda_vote", vec![s(&id), Value::Bool(true)]);
    expect_code("nanda_commit", vec![s(&id)], "E_BAD_STATE");

    // Two ballots reach quorum, and both approve.
    ok("nanda_vote", vec![s(&id), Value::Bool(true)]);
    let committed = fields(ok("nanda_commit", vec![s(&id)]));
    assert_eq!(committed.get("committed"), Some(&Value::Bool(true)));
    assert_eq!(committed.get("approval_ratio"), Some(&Value::Float(1.0)));

    // A decided proposal takes no further ballots and cannot be committed again.
    expect_code(
        "nanda_vote",
        vec![s(&id), Value::Bool(false)],
        "E_BAD_STATE",
    );
    expect_code("nanda_commit", vec![s(&id)], "E_BAD_STATE");
    expect_code("nanda_abort", vec![s(&id)], "E_BAD_STATE");

    let consensus = fields(ok("nanda_consensus", vec![s(&id)]));
    assert_eq!(consensus.get("reached"), Some(&Value::Bool(true)));
}

#[test]
fn nanda_rejects_a_proposal_below_its_threshold_and_records_it() {
    let _g = serial();
    let id = nanda_propose(2, 0.5);
    ok("nanda_vote", vec![s(&id), Value::Bool(false)]);
    ok("nanda_vote", vec![s(&id), Value::Bool(false)]);
    expect_code("nanda_commit", vec![s(&id)], "E_POLICY_DENY");
    let status = fields(ok("nanda_status", vec![s(&id)]));
    assert_eq!(text(&status, "status"), "rejected");
}

#[test]
fn nanda_votes_are_bounded_by_the_electorate() {
    let _g = serial();
    let id = nanda_propose(1, 0.5);
    ok("nanda_vote", vec![s(&id), Value::Bool(true)]);
    expect_code(
        "nanda_vote",
        vec![s(&id), Value::Bool(true)],
        "E_BUDGET_EXCEEDED",
    );
    let quorum = fields(ok("nanda_quorum", vec![s(&id)]));
    assert_eq!(quorum.get("votes_cast"), Some(&Value::Int(1)));
    assert_eq!(quorum.get("participation"), Some(&Value::Float(1.0)));
}

#[test]
fn nanda_consensus_with_no_ballots_is_not_reached() {
    let _g = serial();
    // With no ballots and a threshold of 0.5, the old code divided nothing and
    // reported no consensus only by accident. Now it is not reached, and not a NaN.
    let id = nanda_propose(4, 0.5);
    let consensus = fields(ok("nanda_consensus", vec![s(&id)]));
    assert_eq!(consensus.get("reached"), Some(&Value::Bool(false)));
    assert_eq!(consensus.get("approval_ratio"), Some(&Value::Float(0.0)));
    let quorum = fields(ok("nanda_quorum", vec![s(&id)]));
    assert_eq!(quorum.get("quorum_reached"), Some(&Value::Bool(false)));
    ok("nanda_abort", vec![s(&id)]);
}

#[test]
fn nanda_unknown_ids_and_bad_arguments_are_coded() {
    let _g = serial();
    expect_code(
        "nanda_vote",
        vec![s("prop_none"), Value::Bool(true)],
        "E_NOT_FOUND",
    );
    expect_code("nanda_commit", vec![s("prop_none")], "E_NOT_FOUND");
    expect_code("nanda_abort", vec![s("prop_none")], "E_NOT_FOUND");
    expect_code("nanda_consensus", vec![s("prop_none")], "E_NOT_FOUND");
    expect_code("nanda_quorum", vec![s("prop_none")], "E_NOT_FOUND");
    expect_code("nanda_status", vec![Value::Int(7)], "E_BAD_ARG");
    expect_code("nanda_vote", vec![s("prop_none"), s("yes")], "E_BAD_ARG");
}

#[test]
fn nanda_pending_proposals_are_capped_and_decided_ones_are_evicted() {
    let _g = serial();
    clear_pending_nanda();
    // Propose until the store refuses. The bound stops a store that never refuses.
    let mut ids: Vec<String> = Vec::new();
    loop {
        let mut env = Env::new();
        match call("nanda_propose", vec![s("fill"), record(vec![])], &mut env) {
            Ok(v) => ids.push(text(&fields(v), "id")),
            Err(e) => {
                assert!(e.to_string().contains("E_BUDGET_EXCEEDED"), "{e}");
                break;
            }
        }
        assert!(
            ids.len() <= 256,
            "more than 256 pending proposals were held"
        );
    }
    assert!(
        !ids.is_empty(),
        "the store refused before holding any proposal"
    );
    // Deciding one frees its slot: a decided proposal is evicted for the new one.
    ok("nanda_abort", vec![s(&ids[0])]);
    ok("nanda_propose", vec![s("fresh"), record(vec![])]);
    // Leave no pending proposal behind for the other tests.
    clear_pending_nanda();
}

// ── workspace ───────────────────────────────────────────────────────────────

/// One test for the workspace contracts: the store is process-wide and has a cap
/// with no way to delete a workspace, so the cap is exercised last, here, and the
/// other workspace checks run before it.
#[test]
fn workspace_contracts() {
    let _g = serial();

    // Creation refuses an empty name and unknown options rather than dropping them.
    expect_code("workspace_create", vec![s("")], "E_BAD_ARG");
    expect_code(
        "workspace_create",
        vec![s("team"), record(vec![("bogus", Value::Int(1))])],
        "E_BAD_ARG",
    );

    // The creator is the owner and sole member, and the owner cannot leave.
    let ws = fields(ok(
        "workspace_create",
        vec![s("team"), record(vec![("description", s("d"))])],
    ));
    let id = text(&ws, "id");
    let members = match ok("workspace_members", vec![s(&id)]) {
        Value::Array(m) => m,
        other => panic!("expected Array, got {other:?}"),
    };
    assert_eq!(members.len(), 1);
    expect_code("workspace_leave", vec![s(&id)], "E_POLICY_DENY");
    expect_code("workspace_join", vec![s(&id)], "E_BAD_STATE");
    expect_code("workspace_members", vec![s("no-such")], "E_NOT_FOUND");

    // An agent is shareable only once registered, and only once per workspace.
    expect_code(
        "workspace_share_agent",
        vec![s(&id), s("collab-test-unregistered")],
        "E_NOT_FOUND",
    );
    ok("a2a_register", vec![s("collab-test-agent")]);
    let shared = fields(ok(
        "workspace_share_agent",
        vec![s(&id), s("collab-test-agent")],
    ));
    assert_eq!(shared.get("total_agents"), Some(&Value::Int(1)));
    expect_code(
        "workspace_share_agent",
        vec![s(&id), s("collab-test-agent")],
        "E_BAD_STATE",
    );
    assert_eq!(
        ok("workspace_agents", vec![s(&id)]),
        Value::Array(vec![s("collab-test-agent")])
    );
    ok("a2a_unregister", vec![s("collab-test-agent")]);

    // Sync has no peer to talk to: it refuses instead of reporting a sync.
    expect_code("workspace_sync", vec![s(&id)], "E_UNIMPLEMENTED");
    expect_code("workspace_sync", vec![s("no-such")], "E_NOT_FOUND");

    // The store is capped. Creation fills it and then refuses with a budget error.
    let mut created = 0usize;
    loop {
        let mut env = Env::new();
        match call("workspace_create", vec![s("bulk")], &mut env) {
            Ok(_) => created += 1,
            Err(e) => {
                assert!(
                    e.to_string().contains("E_BUDGET_EXCEEDED"),
                    "a full store must refuse with a budget error: {e}"
                );
                break;
            }
        }
        assert!(created <= 256, "more than 256 workspaces were created");
    }
}

// ── repl ────────────────────────────────────────────────────────────────────

#[test]
fn repl_transports_refuse_rather_than_report_a_session() {
    let _g = serial();
    // repl_serve() with no options would bind a real listener, so every case
    // here is refused before any socket is opened.
    expect_code("repl_serve", vec![Value::Int(5)], "E_BAD_ARG");
    expect_code(
        "repl_serve",
        vec![record(vec![("host", s("10.0.0.1"))])],
        "E_BAD_ARG",
    );
    expect_code(
        "repl_serve",
        vec![record(vec![("stop", Value::Bool(true))])],
        "E_BAD_STATE",
    );
    expect_code("repl_connect", vec![Value::Int(5)], "E_BAD_ARG");
    expect_code(
        "repl_connect",
        vec![s("tcp://127.0.0.1:1"), record(vec![("token", s("t"))])],
        "E_BAD_ARG",
    );
    expect_code("repl_broadcast", vec![s("1 + 1")], "E_UNIMPLEMENTED");
    expect_code("repl_disconnect", vec![s("no-such-session")], "E_NOT_FOUND");
    assert!(matches!(ok("repl_sessions", vec![]), Value::Array(_)));
}

// ── sess ────────────────────────────────────────────────────────────────────

#[test]
fn sessions_keep_bindings_and_count_their_evals() {
    let _g = serial();
    let id = match fields(ok("sess_open", vec![])).get("session") {
        Some(Value::Str(id)) => id.clone(),
        other => panic!("sess_open gave {other:?}"),
    };
    ok("sess_eval", vec![s(&id), s("let x = 41")]);
    assert_eq!(ok("sess_eval", vec![s(&id), s("x + 1")]), Value::Int(42));
    let usage = fields(ok("sess_usage", vec![s(&id)]));
    assert_eq!(usage.get("evals"), Some(&Value::Int(2)));
    ok("sess_close", vec![s(&id)]);
    expect_code("sess_eval", vec![s(&id), s("1")], "E_NOT_FOUND");
}

#[test]
fn sessions_refuse_oversized_code_and_bound_their_count() {
    let _g = serial();
    let huge = "x".repeat(64 * 1024 + 1);
    expect_code(
        "sess_eval",
        vec![s("no-such"), s(&huge)],
        "E_BUDGET_EXCEEDED",
    );
    expect_code(
        "sess_eval",
        vec![s("no-such-session"), s("1")],
        "E_NOT_FOUND",
    );

    let mut open = Vec::new();
    loop {
        let mut env = Env::new();
        match call("sess_open", vec![], &mut env) {
            Ok(v) => {
                let r = fields(v);
                open.push(text(&r, "session"));
            }
            Err(e) => {
                assert!(e.to_string().contains("E_BUDGET_EXCEEDED"), "{e}");
                break;
            }
        }
        assert!(open.len() <= 64, "more than 64 sessions opened");
    }
    for id in &open {
        ok("sess_close", vec![s(id)]);
    }
}

// ── a2a ─────────────────────────────────────────────────────────────────────

#[test]
fn a2a_send_queues_in_process_and_bounds_the_queue() {
    let _g = serial();
    expect_code(
        "a2a_send",
        vec![s("collab-unregistered"), s("hi")],
        "E_NOT_FOUND",
    );
    expect_code("a2a_send", vec![Value::Int(5), s("hi")], "E_BAD_ARG");

    ok("a2a_register", vec![s("collab-queue-agent")]);
    let sent = fields(ok("a2a_send", vec![s("collab-queue-agent"), s("hi")]));
    assert_eq!(sent.get("sent"), Some(&Value::Bool(true)));
    assert_eq!(
        ok("a2a_receive", vec![s("collab-queue-agent")]),
        Value::Array(vec![s("hi")])
    );

    for _ in 0..1024 {
        ok("a2a_send", vec![s("collab-queue-agent"), s("m")]);
    }
    expect_code(
        "a2a_send",
        vec![s("collab-queue-agent"), s("m")],
        "E_BUDGET_EXCEEDED",
    );
    ok("a2a_receive", vec![s("collab-queue-agent")]);

    let huge = "y".repeat(64 * 1024 + 1);
    expect_code(
        "a2a_send",
        vec![s("collab-queue-agent"), s(&huge)],
        "E_BUDGET_EXCEEDED",
    );
    ok("a2a_unregister", vec![s("collab-queue-agent")]);
}

// ── agent_with_mcp ──────────────────────────────────────────────────────────

#[test]
fn agent_with_mcp_refuses_bad_input_before_any_model_call() {
    let _g = serial();
    expect_code(
        "agent_with_mcp",
        vec![Value::Int(5), Value::Array(vec![])],
        "E_BAD_ARG",
    );
    expect_code(
        "agent_with_mcp",
        vec![s("x"), Value::Array(vec![Value::Int(1)])],
        "E_BAD_ARG",
    );
    expect_code(
        "agent_with_mcp",
        vec![s("x"), Value::Array(vec![s("no_such_tool_xyz")])],
        "E_NOT_FOUND",
    );
    // Only an endpoint `detect_mcp_servers` found is contacted; any other is
    // not found, before any model call.
    expect_code(
        "agent_with_mcp",
        vec![s("x"), Value::Array(vec![]), s("http://127.0.0.1:9")],
        "E_NOT_FOUND",
    );
    let too_many = Value::Array(vec![s("now"); 33]);
    expect_code(
        "agent_with_mcp",
        vec![s("x"), too_many],
        "E_BUDGET_EXCEEDED",
    );

    let mut env = Env::new();
    let piped = call_with_input(
        "agent_with_mcp",
        vec![s("x"), Value::Array(vec![])],
        Some(Value::Int(1)),
        &mut env,
    );
    assert!(piped.is_err(), "a piped subject must be refused");
}

#[test]
fn piped_input_is_refused_by_the_builtins_that_take_none() {
    let _g = serial();
    for name in ["nanda_status", "sess_open", "a2a_send", "job_submit"] {
        let mut env = Env::new();
        let err = call_with_input(name, vec![], Some(Value::Int(1)), &mut env)
            .expect_err(&format!("{name} accepted piped input"));
        assert!(
            err.to_string().contains("E_BAD_ARG"),
            "{name}: wanted E_BAD_ARG, got {err}"
        );
    }
}

// ── job ─────────────────────────────────────────────────────────────────────

#[test]
fn job_submit_refuses_and_issues_no_id() {
    let _g = serial();
    expect_code(
        "job_submit",
        vec![s("build"), s("cargo test")],
        "E_UNIMPLEMENTED",
    );
    expect_code(
        "submit",
        vec![s("build"), s("cargo test")],
        "E_UNIMPLEMENTED",
    );
    expect_code(
        "job_submit",
        vec![Value::Int(5), s("cargo test")],
        "E_BAD_ARG",
    );
    expect_code("job_status", vec![s("job_none")], "E_NOT_FOUND");
    expect_code("job_cancel", vec![s("job_none")], "E_NOT_FOUND");
    assert_eq!(ok("job_list", vec![]), Value::Array(vec![]));
}

// ── subprocess: the guards hold inside a real `ish` run ──────────────────────

/// Run `ish -c <code>` with a hard deadline, disposable state, and the given
/// environment. Returns (succeeded, stdout + stderr, lower-cased).
fn run_ae(code: &str, dir: &std::path::Path, extra: &[(&str, &str)]) -> (bool, String) {
    let session_dir = dir.join("sessions");
    let workspace = dir.join("ws");
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    std::fs::create_dir_all(&workspace).expect("create workspace dir");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ish"));
    cmd.arg("-c")
        .arg(code)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("IRONSHELL_SESSION_DIR", &session_dir)
        .env("IRONSHELL_WORKSPACE", &workspace);
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("spawn ae");
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll ae") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("ae did not finish within 30 seconds: {code}");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut out = String::new();
    if let Some(mut so) = child.stdout.take() {
        let _ = so.read_to_string(&mut out);
    }
    if let Some(mut se) = child.stderr.take() {
        let _ = se.read_to_string(&mut out);
    }
    (status.success(), out.to_lowercase())
}

fn disposable_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ae_collab_{}_{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create disposable dir");
    dir
}

#[test]
fn agent_mode_gates_a_write_made_inside_a_session() {
    let _g = serial();
    let dir = disposable_dir("sess_gate");
    // A file well outside the disposable workspace, in the temp directory.
    let outside = dir.join("outside.txt");
    let outside_ae = outside.to_string_lossy().replace('\\', "/");
    let code =
        format!("sess_eval(sess_open().session, \"file.write(\\\"{outside_ae}\\\", \\\"x\\\")\")");
    let (succeeded, out) = run_ae(&code, &dir, &[("IRONSHELL_MODE", "agent")]);
    assert!(!succeeded, "agent mode must refuse the write: {out}");
    assert!(
        out.contains("outside") || out.contains("e_outside_workspace"),
        "the refusal should name the jail: {out}"
    );
    assert!(
        !outside.exists(),
        "the refused write must not create the file"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn agent_with_mcp_stops_at_the_network_budget_before_any_model_call() {
    let _g = serial();
    let dir = disposable_dir("mcp_budget");
    // A budget of zero network operations refuses the model call before it starts.
    let (succeeded, out) = run_ae(
        "agent_with_mcp(\"hello\", [\"now\"])",
        &dir,
        &[("IRONSHELL_MODE", "agent"), ("IRONSHELL_MAX_NET", "0")],
    );
    assert!(!succeeded, "the run must be refused: {out}");
    assert!(
        out.contains("e_budget_exceeded"),
        "wanted a budget refusal: {out}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
