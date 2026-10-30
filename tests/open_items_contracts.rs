//! Regression contracts for the open review items on tool_exec, the rag
//! builtins, the network budget of the AI backend probes, the REPL and cloud
//! connect refusals, the a2a bounds, and the flows the docs now describe.
//!
//! Nothing here reaches a network host, an AI provider, or a real remote. Every
//! file is in a fresh directory under the temp dir. Where a test runs a child
//! process it is this test binary itself, selected by name and told what to do
//! through an environment variable, so it behaves the same on Windows and Linux.

use iron_shell::builtins::{call, call_with_input};
use iron_shell::env::Env;
use iron_shell::os_tools::run_capped;
use iron_shell::value::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Process-wide state (environment, the governor, the rag index, the a2a and
/// nanda tables, the REPL store) is touched only under this lock.
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn run(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call(name, args, &mut env)
}

fn piped(name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let mut env = Env::new();
    call_with_input(name, args, Some(input), &mut env)
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

/// The first `E_...` code in a refusal, or the value rendered when it succeeded.
fn outcome(r: anyhow::Result<Value>) -> String {
    match r {
        Ok(v) => format!("OK {v}"),
        Err(e) => code_of(&e.to_string()),
    }
}

fn code_of(text: &str) -> String {
    match text.find("E_") {
        Some(i) => text[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {text}"),
    }
}

fn array_len(r: anyhow::Result<Value>) -> usize {
    match r {
        Ok(Value::Array(items)) => items.len(),
        other => panic!("expected an Array, got {other:?}"),
    }
}

fn field(v: &Value, key: &str) -> Value {
    match v {
        Value::Record(m) => m.get(key).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// Sets environment variables for the lifetime of the guard and puts back what
/// they were on drop. Callers hold [`lock`].
struct EnvSet(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl EnvSet {
    fn new(pairs: &[(&'static str, &str)]) -> EnvSet {
        let saved = pairs
            .iter()
            .map(|(k, _)| (*k, std::env::var_os(k)))
            .collect();
        for (k, v) in pairs {
            std::env::set_var(k, v);
        }
        EnvSet(saved)
    }
}

impl Drop for EnvSet {
    fn drop(&mut self) {
        for (k, v) in &self.0 {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}

/// Environment variable that turns this test binary into a child process.
const CHILD_MODE: &str = "AE_OPEN_ITEMS_CHILD";

/// Sets [`CHILD_MODE`] for the children spawned while it lives.
struct ChildMode;

impl ChildMode {
    fn set(mode: &str) -> ChildMode {
        std::env::set_var(CHILD_MODE, mode);
        ChildMode
    }
}

impl Drop for ChildMode {
    fn drop(&mut self) {
        std::env::remove_var(CHILD_MODE);
    }
}

/// The program and argv that re-run this test binary's `child_entry` test.
fn self_child() -> (String, Vec<String>) {
    let exe = std::env::current_exe()
        .expect("current_exe")
        .to_string_lossy()
        .into_owned();
    let args = vec![
        "--exact".to_string(),
        "child_entry".to_string(),
        "--nocapture".to_string(),
        "--test-threads=1".to_string(),
    ];
    (exe, args)
}

/// The child entry point. Does nothing unless the parent set [`CHILD_MODE`].
#[test]
fn child_entry() {
    use std::io::Write;
    match std::env::var(CHILD_MODE).as_deref() {
        Ok("flood") => {
            // About 4 MiB of stdout: far above the 64 KiB cap the test uses.
            let line = "x".repeat(1023);
            let mut out = std::io::stdout().lock();
            for _ in 0..4096 {
                let _ = writeln!(out, "{line}");
            }
            let _ = out.flush();
        }
        Ok("sleep") => std::thread::sleep(Duration::from_secs(60)),
        _ => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 1. tool_exec: bounded output and a deadline that kills the child.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn bounded_run_truncates_and_flags_flood_output() {
    let _g = lock();
    let _mode = ChildMode::set("flood");
    let (exe, args) = self_child();
    let cap = 64 * 1024;
    let out = run_capped("tool_exec", "flood", &exe, &args, cap, 4096, 120)
        .expect("a child that prints a lot still finishes");
    assert!(out.success, "the child exits 0");
    assert!(out.stdout_truncated, "stdout past the cap is flagged");
    assert!(
        out.stdout.len() <= cap,
        "stdout is held to the cap, got {} bytes",
        out.stdout.len()
    );
    // The head is what the child printed first: libtest's own header, before
    // the flood. A tail-keeping cap would not start with it.
    assert!(
        out.stdout.starts_with("\nrunning 1 test") || out.stdout.starts_with("running 1 test"),
        "the head of the output is kept, got {:?}",
        &out.stdout[..out.stdout.len().min(40)]
    );
}

#[test]
fn bounded_run_kills_a_child_past_its_deadline() {
    let _g = lock();
    let _mode = ChildMode::set("sleep");
    let (exe, args) = self_child();
    let started = Instant::now();
    let err = run_capped("tool_exec", "sleep", &exe, &args, 1024, 1024, 1)
        .expect_err("a child that never exits is refused");
    let elapsed = started.elapsed();
    assert_eq!(code_of(&err.to_string()), "E_BUDGET_EXCEEDED");
    assert!(
        elapsed < Duration::from_secs(30),
        "the child was killed at the deadline, not left to sleep 60 s (took {elapsed:?})"
    );
}

#[test]
fn a_missing_program_is_a_coded_tool_missing_refusal() {
    let _g = lock();
    let err = run_capped(
        "tool_exec",
        "missing",
        "no_such_program_ironshell_open_items",
        &[],
        1024,
        1024,
        30,
    )
    .expect_err("a program that is not there cannot run");
    assert_eq!(code_of(&err.to_string()), "E_TOOL_MISSING");
}

#[test]
fn tool_exec_refuses_a_non_string_argument_before_running() {
    let _g = lock();
    let got = outcome(run("tool_exec", vec![s("ls"), Value::Int(3)]));
    assert_eq!(got, "E_BAD_ARG");
}

// ─────────────────────────────────────────────────────────────────────────
// 2. rag_search and rag_query: top_k is an integer from 1 to a ceiling, and a
//    piped query takes top_k from its only argument.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn rag_top_k_refusals_are_coded_and_bounded() {
    let _g = lock();
    assert_eq!(
        outcome(run("rag_search", vec![s("q"), Value::Int(-1)])),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run("rag_search", vec![s("q"), Value::Int(0)])),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run("rag_search", vec![s("q"), s("3")])),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run("rag_search", vec![s("q"), Value::Int(101)])),
        "E_BUDGET_EXCEEDED"
    );
    assert_eq!(
        outcome(run(
            "rag_search",
            vec![s("q"), Value::Int(2), Value::Int(3)]
        )),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run("rag_query", vec![s("q"), Value::Int(-5)])),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run("rag_query", vec![s("q"), Value::Int(21)])),
        "E_BUDGET_EXCEEDED"
    );
}

#[test]
fn piped_rag_search_takes_top_k_from_its_only_argument() {
    let _g = lock();
    run("rag_clear", vec![]).expect("clear the process index");
    for (i, text) in ["alpha beta gamma", "delta epsilon", "zeta eta theta"]
        .iter()
        .enumerate()
    {
        run("rag_index", vec![s(text), s(&format!("doc{i}.txt"))]).expect("index a document");
    }

    // Piped: the query is the input and the one argument is top_k.
    assert_eq!(
        array_len(piped("rag_search", vec![Value::Int(1)], s("alpha"))),
        1
    );
    // The count is read, not defaulted: a piped negative count is refused.
    assert_eq!(
        outcome(piped("rag_search", vec![Value::Int(-1)], s("alpha"))),
        "E_BAD_ARG"
    );
    // No count: the default of 5 is above the three documents indexed.
    assert_eq!(array_len(piped("rag_search", vec![], s("alpha"))), 3);
    // A piped call with a query argument too is refused, not misread.
    assert_eq!(
        outcome(piped("rag_search", vec![s("alpha")], s("alpha"))),
        "E_BAD_ARG"
    );

    // rag_query, piped: the one argument is top_k, and the record counts its docs.
    let q = piped("rag_query", vec![Value::Int(2)], s("alpha")).expect("piped rag_query");
    assert_eq!(field(&q, "doc_count"), Value::Int(2));

    run("rag_clear", vec![]).expect("clear the index again");
}

// ─────────────────────────────────────────────────────────────────────────
// 3. ai_backends: each endpoint probe is charged before it connects.
// ─────────────────────────────────────────────────────────────────────────

/// With the budget at zero, the first probe's own charge refuses before any
/// socket opens. The probes are called directly, not through the builtin, so the
/// refusal can only come from a probe's charge. No endpoint is contacted: the
/// refusal is earlier than the connection.
#[test]
fn backend_probes_are_refused_by_the_budget_before_any_connection() {
    let _g = lock();
    let _env = EnvSet::new(&[("IRONSHELL_MODE", "agent"), ("IRONSHELL_MAX_NET", "0")]);
    iron_shell::safety::governor_reset();

    let err = iron_shell::ai::try_detect_available_backends()
        .expect_err("a zero network budget refuses the first probe");
    assert_eq!(code_of(&err.to_string()), "E_BUDGET_EXCEEDED");

    // Through the builtin the call's own charge refuses first, the same way.
    assert_eq!(outcome(run("ai_backends", vec![])), "E_BUDGET_EXCEEDED");

    iron_shell::safety::governor_reset();
}

// ─────────────────────────────────────────────────────────────────────────
// 4. cloud and REPL connects: refused, and no session is recorded.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn connects_refuse_and_record_no_repl_session() {
    let _g = lock();
    let before = run("repl_sessions", vec![]).expect("list sessions");

    assert_eq!(
        outcome(run("cloud_instance_connect", vec![s("i-0123456789")])),
        "E_UNIMPLEMENTED"
    );
    // A non-literal target is refused before any connection is attempted.
    assert_eq!(
        outcome(run(
            "repl_connect",
            vec![
                s("nowhere.invalid"),
                Value::Record(std::collections::BTreeMap::from([(
                    "token".to_string(),
                    s("t"),
                )])),
            ]
        )),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run(
            "repl_serve",
            vec![Value::Record(std::collections::BTreeMap::from([(
                "stop".to_string(),
                Value::Bool(true),
            )]))]
        )),
        "E_BAD_STATE"
    );

    let after = run("repl_sessions", vec![]).expect("list sessions again");
    assert_eq!(before, after, "the session table is unchanged");
    assert!(
        matches!(&after, Value::Array(items) if items.is_empty()),
        "no session was recorded: {after}"
    );
}

#[test]
fn there_is_no_cloud_connect_builtin_that_could_fabricate_a_session() {
    let _g = lock();
    assert!(
        run("cloud_connect", vec![s("i-0123456789")]).is_err(),
        "cloud_connect is not a builtin, so it cannot answer with a session"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// 5. a2a: agents, messages and recipients are bounded.
// ─────────────────────────────────────────────────────────────────────────

fn unregister_all(names: &[String]) {
    for n in names {
        let _ = run("a2a_unregister", vec![s(n)]);
    }
}

#[test]
fn a2a_register_refuses_past_its_agent_ceiling() {
    let _g = lock();
    let names: Vec<String> = (0..256)
        .map(|i| format!("open-items-agent-{i:03}"))
        .collect();
    for n in &names {
        run("a2a_register", vec![s(n)]).expect("within the ceiling");
    }
    assert_eq!(
        outcome(run("a2a_register", vec![s("open-items-agent-overflow")])),
        "E_BUDGET_EXCEEDED"
    );
    // Re-registering a name already held is not growth, so it is allowed.
    assert!(run("a2a_register", vec![s(&names[0])]).is_ok());
    unregister_all(&names);
}

#[test]
fn a2a_register_refuses_a_non_record_info_surplus_args_and_oversize_info() {
    let _g = lock();
    assert_eq!(
        outcome(run(
            "a2a_register",
            vec![s("open-items-info"), s("not a record")]
        )),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run(
            "a2a_register",
            vec![s("open-items-info"), Value::Null, Value::Null]
        )),
        "E_BAD_ARG"
    );
    let mut big = BTreeMap::new();
    big.insert("blob".to_string(), Value::Str("x".repeat(70 * 1024)));
    assert_eq!(
        outcome(run(
            "a2a_register",
            vec![s("open-items-info"), Value::Record(big)]
        )),
        "E_BUDGET_EXCEEDED"
    );
    assert!(
        run("a2a_agents", vec![])
            .map(|v| !format!("{v}").contains("open-items-info"))
            .unwrap_or(false),
        "no refused registration was stored"
    );
}

#[test]
fn a2a_broadcast_refuses_oversize_surplus_piped_and_wide_fan_out() {
    let _g = lock();
    assert_eq!(
        outcome(run(
            "a2a_broadcast",
            vec![Value::Str("x".repeat(64 * 1024 + 1))]
        )),
        "E_BUDGET_EXCEEDED"
    );
    assert_eq!(
        outcome(run("a2a_broadcast", vec![s("a"), s("b")])),
        "E_BAD_ARG"
    );
    assert_eq!(outcome(piped("a2a_broadcast", vec![], s("a"))), "E_BAD_ARG");

    // More recipients than one broadcast reaches is refused, and nothing queues.
    let names: Vec<String> = (0..65).map(|i| format!("open-items-fan-{i:02}")).collect();
    for n in &names {
        run("a2a_register", vec![s(n)]).expect("register a recipient");
    }
    assert_eq!(
        outcome(run("a2a_broadcast", vec![s("hi")])),
        "E_BUDGET_EXCEEDED"
    );
    assert_eq!(array_len(run("a2a_receive", vec![s(&names[0])])), 0);
    unregister_all(&names);
}

#[test]
fn a2a_broadcast_is_all_or_nothing_when_one_queue_is_full() {
    let _g = lock();
    let names = vec![
        "open-items-q-full".to_string(),
        "open-items-q-open".to_string(),
    ];
    for n in &names {
        run("a2a_register", vec![s(n)]).expect("register");
    }
    for _ in 0..1024 {
        run("a2a_send", vec![s(&names[0]), s("m")]).expect("fill the queue to its ceiling");
    }
    assert_eq!(
        outcome(run("a2a_broadcast", vec![s("hi")])),
        "E_BUDGET_EXCEEDED"
    );
    // The refusal queued nothing, not even for the agent whose queue had room.
    assert_eq!(array_len(run("a2a_receive", vec![s(&names[1])])), 0);
    assert_eq!(array_len(run("a2a_receive", vec![s(&names[0])])), 1024);
    unregister_all(&names);
}

// ─────────────────────────────────────────────────────────────────────────
// 6. The flows the docs now describe.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn job_submission_refuses_and_the_job_table_stays_empty() {
    let _g = lock();
    assert_eq!(
        outcome(run("job_submit", vec![s("build"), s("cargo test")])),
        "E_UNIMPLEMENTED"
    );
    assert_eq!(
        outcome(run("submit", vec![s("build"), s("cargo test")])),
        "E_UNIMPLEMENTED"
    );
    assert_eq!(array_len(run("job_list", vec![])), 0);
    assert_eq!(
        outcome(run("job_status", vec![s("job-abc123")])),
        "E_NOT_FOUND"
    );
}

#[test]
fn the_nanda_flow_in_the_book_tallies_and_commits_in_process() {
    let _g = lock();
    let p = run(
        "nanda_propose",
        vec![
            s("open-items-deploy"),
            Value::Record(BTreeMap::from([
                ("threshold".to_string(), Value::Float(0.66)),
                ("electorate".to_string(), Value::Int(3)),
            ])),
        ],
    )
    .expect("propose");
    let id = match field(&p, "id") {
        Value::Str(id) => id,
        other => panic!("a proposal carries its id, got {other:?}"),
    };
    run("nanda_vote", vec![s(&id), Value::Bool(true)]).expect("vote");
    run("nanda_vote", vec![s(&id), Value::Bool(true)]).expect("vote");

    let quorum = run("nanda_quorum", vec![s(&id)]).expect("quorum");
    assert_eq!(field(&quorum, "quorum_reached"), Value::Bool(true));
    assert_eq!(field(&quorum, "votes_cast"), Value::Int(2));

    let consensus = run("nanda_consensus", vec![s(&id)]).expect("consensus");
    assert_eq!(field(&consensus, "reached"), Value::Bool(true));

    run("nanda_commit", vec![s(&id)]).expect("commit with quorum and approval");
}
