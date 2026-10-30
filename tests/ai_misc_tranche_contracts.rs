//! Contracts for the ai, plan, task, cluster and archive builtins declared in
//! the fourth declaration tranche.
//!
//! The approval flow is the part that matters most. `plan` derives a token from
//! the exact ops; `approve` is refused in agent mode; `apply` runs a batch only
//! when its token is approved, and an in-process approval is spent by the apply
//! it authorises. `set_env`, `env_set` and `env_unset` may not change an
//! `IRONSHELL_*` variable in agent mode, because the gate reads its mode and its
//! approvals from those variables.
//!
//! Nothing here calls a model provider, opens the network, opens an audio
//! device, or runs a program the test does not control. Every file operation is
//! in a fresh directory under the temp dir, which is also the workspace.
//! Process-wide state (environment, approvals, transactions) is touched only
//! under `LOCK`.

use iron_shell::builtins::{call, call_with_input};
use iron_shell::env::Env;
use iron_shell::safety;
use iron_shell::value::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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

/// The first `E_...` code in a refusal, or the value rendered when it succeeded.
fn outcome(r: anyhow::Result<Value>) -> String {
    match r {
        Ok(v) => format!("OK {v}"),
        Err(e) => {
            let text = e.to_string();
            match text.find("E_") {
                Some(i) => text[i..]
                    .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
                    .next()
                    .unwrap_or("E_?")
                    .to_string(),
                None => format!("NO_CODE: {text}"),
            }
        }
    }
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

fn rec(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn field(v: &Value, key: &str) -> Value {
    match v {
        Value::Record(m) => m.get(key).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

fn token_of(plan: &Value) -> String {
    match field(plan, "token") {
        Value::Str(t) => t,
        other => panic!("plan returned no token string: {other}"),
    }
}

/// A fresh workspace directory, made the jail for the duration of one test.
struct Workspace(PathBuf);

impl Workspace {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "ae_misc_tranche_{tag}_{}_{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create workspace");
        std::env::set_var("IRONSHELL_WORKSPACE", &dir);
        Workspace(dir)
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        std::env::remove_var("IRONSHELL_WORKSPACE");
        std::env::remove_var("IRONSHELL_MODE");
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Agent mode for the duration of a test; cleared on drop.
struct AgentMode;

impl AgentMode {
    fn enter() -> Self {
        std::env::set_var("IRONSHELL_MODE", "agent");
        AgentMode
    }
}

impl Drop for AgentMode {
    fn drop(&mut self) {
        std::env::remove_var("IRONSHELL_MODE");
    }
}

fn write_op(path: &str, content: &str) -> Value {
    rec(&[
        ("op", s("write")),
        ("path", s(path)),
        ("content", s(content)),
    ])
}

// ── the approval flow ─────────────────────────────────────────────────────

#[test]
fn an_agent_may_not_approve_its_own_plan() {
    let _g = lock();
    let ws = Workspace::new("approve_refused");
    let _a = AgentMode::enter();
    let target = ws.path("f.txt");
    let ops = Value::Array(vec![write_op(&target, "x")]);
    let token = token_of(&run("plan", vec![ops.clone()]).expect("plan"));
    assert_eq!(outcome(run("approve", vec![s(&token)])), "E_POLICY_DENY");
    // The refusal left the plan unapproved, so apply does not run it.
    assert_eq!(
        field(&run("apply", vec![ops]).expect("apply"), "status"),
        s("needs_approval")
    );
    assert!(!Path::new(&target).exists());
}

#[test]
fn an_agent_cannot_flip_the_gate_through_the_environment() {
    let _g = lock();
    let _ws = Workspace::new("env_gate");
    let _a = AgentMode::enter();
    for name in ["set_env", "env_set"] {
        let args = vec![s("IRONSHELL_APPROVE_ALL"), s("1")];
        assert_eq!(outcome(run(name, args)), "E_POLICY_DENY", "{name}");
    }
    assert_eq!(
        outcome(run("set_env", vec![s("ironshell_mode"), s("human")])),
        "E_POLICY_DENY",
        "names are matched case-insensitively, as Windows matches them"
    );
    assert_eq!(
        outcome(run("env_unset", vec![s("IRONSHELL_MODE")])),
        "E_POLICY_DENY"
    );
    assert!(std::env::var("IRONSHELL_APPROVE_ALL").is_err());
    assert_eq!(std::env::var("IRONSHELL_MODE").as_deref(), Ok("agent"));
}

/// Removes an environment variable when dropped, so a failed assertion cannot
/// leave a gate switched on for the tests that follow.
struct EnvVarGuard(&'static str);

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        std::env::remove_var(self.0);
    }
}

#[test]
fn a_child_process_never_inherits_a_gate_variable_in_agent_mode() {
    let _g = lock();
    let _ws = Workspace::new("posix_child");
    let _a = AgentMode::enter();
    // Approve the exec itself, so the only thing refused below is the variable.
    std::env::set_var("IRONSHELL_APPROVE_ALL", "1");
    let _approve = EnvVarGuard("IRONSHELL_APPROVE_ALL");
    let program = "ae_no_such_program_xyz";
    // With the gate variable as a prefix: refused before any spawn (126).
    assert_eq!(
        iron_shell::posix::run(&format!("IRONSHELL_MODE=human {program}")),
        Some(126)
    );
    // Without it, the same command reaches the spawn and is not found (127).
    assert_eq!(iron_shell::posix::run(program), Some(127));
}

#[test]
fn an_agent_may_still_set_ordinary_variables() {
    let _g = lock();
    let _ws = Workspace::new("env_ordinary");
    let _a = AgentMode::enter();
    let name = "AE_MISC_TRANCHE_ORDINARY";
    assert!(run("set_env", vec![s(name), s("v")]).is_ok());
    assert_eq!(std::env::var(name).as_deref(), Ok("v"));
    assert!(run("env_unset", vec![s(name)]).is_ok());
}

#[test]
fn a_granted_plan_is_spent_by_the_apply_it_authorises() {
    let _g = lock();
    let ws = Workspace::new("spent");
    let _a = AgentMode::enter();
    let f = ws.path("once.txt");
    let ops = Value::Array(vec![write_op(&f, "first")]);
    let token = token_of(&run("plan", vec![ops.clone()]).expect("plan"));

    // A human approval, in-process. The first apply runs the batch.
    safety::grant_approval(&token);
    assert_eq!(
        field(&run("apply", vec![ops.clone()]).expect("apply"), "status"),
        s("applied")
    );
    assert_eq!(std::fs::read_to_string(&f).expect("written"), "first");

    // The same token no longer authorises a second run of the same batch.
    std::fs::write(&f, "changed since").expect("change the file");
    assert_eq!(
        field(&run("apply", vec![ops]).expect("apply again"), "status"),
        s("needs_approval")
    );
    assert_eq!(
        std::fs::read_to_string(&f).expect("unchanged"),
        "changed since"
    );
}

#[test]
fn an_approval_covers_only_the_plan_it_was_given_for() {
    let _g = lock();
    let ws = Workspace::new("bound");
    let _a = AgentMode::enter();
    let approved = Value::Array(vec![write_op(&ws.path("a.txt"), "approved")]);
    let other = Value::Array(vec![write_op(&ws.path("b.txt"), "swapped")]);
    let token = token_of(&run("plan", vec![approved]).expect("plan"));
    safety::grant_approval(&token);

    // A different batch has a different token, so the grant does not reach it.
    let swapped = run("apply", vec![other]).expect("apply");
    assert_eq!(field(&swapped, "status"), s("needs_approval"));
    assert!(!Path::new(&ws.path("b.txt")).exists());
    safety::revoke_approval(&token);
}

#[test]
fn a_plan_token_changes_with_the_destination_and_the_content() {
    let _g = lock();
    let ws = Workspace::new("token_binds");
    let copy = |content: &str, dest: &str| {
        Value::Array(vec![rec(&[
            ("op", s("copy")),
            ("path", s(&ws.path("src.txt"))),
            ("dest", s(dest)),
            ("content", s(content)),
        ])])
    };
    let t1 = token_of(&run("plan", vec![copy("x", &ws.path("d1"))]).expect("plan"));
    let t2 = token_of(&run("plan", vec![copy("x", &ws.path("d2"))]).expect("plan"));
    let t3 = token_of(&run("plan", vec![copy("y", &ws.path("d1"))]).expect("plan"));
    assert_ne!(t1, t2, "a different destination must change the token");
    assert_ne!(t1, t3, "different content must change the token");
}

#[test]
fn apply_refuses_a_path_outside_the_workspace_in_agent_mode() {
    let _g = lock();
    let _ws = Workspace::new("jail");
    let _a = AgentMode::enter();
    let outside = std::env::temp_dir().join(format!("ae_misc_outside_{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&outside);
    let ops = Value::Array(vec![write_op(&outside.to_string_lossy(), "x")]);
    let token = token_of(&run("plan", vec![ops.clone()]).expect("plan"));
    safety::grant_approval(&token);
    let r = run("apply", vec![ops]).expect("apply answers a record");
    assert_eq!(field(&r, "status"), s("failed"));
    assert!(!outside.exists(), "a jailed write must never land");
    safety::revoke_approval(&token);
}

// ── plan and plan_diff ────────────────────────────────────────────────────

#[test]
fn plan_diff_reports_the_lines_that_change() {
    let _g = lock();
    let ws = Workspace::new("diff");
    let f = ws.path("d.txt");
    std::fs::write(&f, "a\nb\n").expect("seed");
    let ops = Value::Array(vec![write_op(&f, "a\nc\n")]);
    let r = run("plan_diff", vec![ops]).expect("plan_diff");
    assert_eq!(field(&r, "added"), Value::Int(1));
    assert_eq!(field(&r, "removed"), Value::Int(1));
    // A diff writes nothing.
    assert_eq!(std::fs::read_to_string(&f).expect("still there"), "a\nb\n");
}

#[test]
fn plan_refuses_a_copy_without_a_destination() {
    let _g = lock();
    let ws = Workspace::new("plan_dest");
    let ops = Value::Array(vec![rec(&[("op", s("copy")), ("path", s(&ws.path("x")))])]);
    assert_eq!(outcome(run("plan", vec![ops])), "E_BAD_ARG");
}

#[test]
fn plan_and_apply_refuse_non_array_ops() {
    let _g = lock();
    for name in ["plan", "plan_diff", "apply"] {
        assert_eq!(outcome(run(name, vec![s("write")])), "E_BAD_ARG", "{name}");
    }
    assert_eq!(outcome(run("approve", vec![Value::Int(5)])), "E_BAD_ARG");
}

// ── argument and pipeline refusals in the tranche ─────────────────────────

#[test]
fn in_refuses_a_collection_it_cannot_search() {
    let _g = lock();
    assert_eq!(
        outcome(run("in", vec![Value::Int(1), Value::Int(5)])),
        "E_BAD_ARG"
    );
    assert_eq!(outcome(run("in", vec![Value::Int(1), s("x")])), "E_BAD_ARG");
    assert_eq!(
        outcome(run("in", vec![Value::Int(1), Value::Int(2), Value::Int(3)])),
        "E_BAD_ARG"
    );
}

#[test]
fn in_answers_membership_by_value() {
    let _g = lock();
    let arr = Value::Array(vec![Value::Int(1), Value::Float(2.5), s("x")]);
    assert_eq!(
        run("in", vec![Value::Int(1), arr.clone()]).expect("in"),
        Value::Bool(true)
    );
    assert_eq!(
        run("in", vec![Value::Float(2.5), arr.clone()]).expect("in"),
        Value::Bool(true)
    );
    assert_eq!(
        run("in", vec![Value::Int(9), arr]).expect("in"),
        Value::Bool(false)
    );
    let nested = Value::Array(vec![Value::Array(vec![Value::Int(1)])]);
    assert_eq!(
        piped(
            "in",
            vec![nested.clone()],
            Value::Array(vec![Value::Int(1)])
        )
        .expect("in"),
        Value::Bool(true),
        "a piped needle is the whole input: [1] is an element of [[1]]"
    );
    assert_eq!(
        piped(
            "in",
            vec![Value::Array(vec![Value::Int(2)])],
            Value::Array(vec![Value::Int(1)])
        )
        .expect("in"),
        Value::Bool(false)
    );
    let r = rec(&[("k", Value::Int(1))]);
    assert_eq!(run("in", vec![s("k"), r]).expect("in"), Value::Bool(true));
}

#[test]
fn aggregate_results_takes_a_piped_strategy_and_refuses_what_it_would_skip() {
    let _g = lock();
    let v = Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    assert_eq!(
        piped("aggregate_results", vec![s("count")], v.clone()).expect("count"),
        Value::Int(3)
    );
    assert_eq!(
        piped("aggregate_results", vec![s("first")], v.clone()).expect("first"),
        Value::Int(1)
    );
    assert_eq!(
        outcome(run("aggregate_results", vec![v, s("nope")])),
        "E_BAD_ARG"
    );
    let bad = Value::Array(vec![Value::Int(1), s("x")]);
    assert_eq!(
        outcome(run("aggregate_results", vec![bad, s("sum")])),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run(
            "aggregate_results",
            vec![Value::Array(vec![]), s("last")]
        )),
        "E_BAD_ARG"
    );
}

#[test]
fn tools_refuses_a_filter_that_is_not_a_string() {
    let _g = lock();
    assert_eq!(outcome(run("tools", vec![Value::Int(5)])), "E_BAD_ARG");
    assert_eq!(
        outcome(run("tools", vec![s("linux"), s("windows")])),
        "E_BAD_ARG"
    );
    assert!(matches!(
        run("tools", vec![]).expect("tools"),
        Value::Array(_)
    ));
}

#[test]
fn tool_exec_refuses_before_it_runs_anything() {
    let _g = lock();
    assert_eq!(
        outcome(run("tool_exec", vec![s("no_such_tool_xyz")])),
        "E_NOT_FOUND"
    );
    assert_eq!(
        outcome(run(
            "tool_exec",
            vec![s("no_such_tool_xyz"), Value::Array(vec![Value::Int(1)])]
        )),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run(
            "tool_exec",
            vec![s("no_such_tool_xyz"), Value::Null, Value::Int(1)]
        )),
        "E_BAD_ARG"
    );
}

#[test]
fn task_run_refuses_option_like_and_spaced_names() {
    let _g = lock();
    assert_eq!(
        outcome(run("task_run", vec![s("--taskfile=/etc/x")])),
        "E_BAD_ARG"
    );
    assert_eq!(outcome(run("task_run", vec![s("a b")])), "E_BAD_ARG");
    assert_eq!(outcome(run("task_run", vec![])), "E_BAD_ARG");
    assert_eq!(outcome(run("task_list", vec![s("x")])), "E_BAD_ARG");
}

#[test]
fn argumentless_builtins_refuse_arguments() {
    let _g = lock();
    assert_eq!(
        outcome(run("telemetry_reset", vec![Value::Int(1)])),
        "E_BAD_ARG"
    );
    assert_eq!(outcome(run("hw_audio", vec![s("x")])), "E_BAD_ARG");
    assert_eq!(
        outcome(run("ai_backends", vec![Value::Int(1)])),
        "E_BAD_ARG"
    );
}

#[test]
fn trace_refuses_a_label_that_is_not_text_and_returns_its_value() {
    let _g = lock();
    assert_eq!(
        outcome(run("trace", vec![Value::Int(1), Value::Int(2)])),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run("trace", vec![s("a"), Value::Int(1), Value::Int(2)])),
        "E_BAD_ARG"
    );
    assert_eq!(
        run("trace", vec![s("step"), Value::Int(5)]).expect("trace"),
        Value::Int(5)
    );
    assert_eq!(
        piped("trace", vec![s("x")], Value::Int(7)).expect("piped trace"),
        Value::Int(7)
    );
}

#[test]
fn bzip2_decompress_refuses_a_non_bz2_name_and_piped_input() {
    let _g = lock();
    let ws = Workspace::new("bzip2_refuse");
    let f = ws.path("data.gz");
    std::fs::write(&f, b"not really").expect("seed");
    assert_eq!(outcome(run("bzip2_decompress", vec![s(&f)])), "E_BAD_ARG");
    assert_eq!(
        outcome(run("bzip2_decompress", vec![s("-x.bz2")])),
        "E_BAD_ARG"
    );
    assert!(piped("bzip2_decompress", vec![s(&f)], s("x")).is_err());
}

#[test]
fn save_json_writes_inside_the_workspace_and_refuses_outside() {
    let _g = lock();
    let ws = Workspace::new("save_json");
    let first = run(
        "save_json",
        vec![s("out.json"), rec(&[("k", Value::Int(1))])],
    )
    .expect("save");
    assert_eq!(field(&first, "overwrote"), Value::Bool(false));
    assert!(Path::new(&ws.path("out.json")).exists());

    // Saving again reports the replacement rather than hiding it.
    let again = run("save_json", vec![s("out.json"), Value::Int(2)]).expect("resave");
    assert_eq!(field(&again, "overwrote"), Value::Bool(true));

    let outside = std::env::temp_dir().join(format!("ae_save_outside_{}.json", std::process::id()));
    let _ = std::fs::remove_file(&outside);
    let r = run(
        "save_json",
        vec![s(&outside.to_string_lossy()), Value::Int(1)],
    );
    assert!(r.is_err(), "a write outside the workspace must be refused");
    assert!(!outside.exists());
    assert_eq!(
        outcome(run(
            "save_json",
            vec![s("x.json"), Value::Int(1), Value::Int(2)]
        )),
        "E_BAD_ARG"
    );
}

#[test]
fn tar_create_refuses_an_existing_archive_and_option_like_names() {
    let _g = lock();
    let ws = Workspace::new("tar");
    let member = ws.path("m.txt");
    std::fs::write(&member, "payload").expect("seed");
    let existing = ws.path("exists.tar");
    std::fs::write(&existing, b"keep me").expect("seed archive");
    assert_eq!(
        outcome(run("tar_create", vec![s(&existing), s(&member)])),
        "E_BAD_STATE",
        "an existing archive must not be truncated"
    );
    assert_eq!(std::fs::read(&existing).expect("intact"), b"keep me");
    assert_eq!(
        outcome(run("tar_create", vec![s("-x.tar"), s(&member)])),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run("tar_create", vec![s(&ws.path("a.tar"))])),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run("tar_create", vec![s(&ws.path("a.tar")), Value::Int(1)])),
        "E_BAD_ARG"
    );
}

#[test]
fn tar_create_refuses_a_member_outside_the_workspace() {
    let _g = lock();
    let ws = Workspace::new("tar_member");
    let outside = std::env::temp_dir().join(format!("ae_tar_member_{}.txt", std::process::id()));
    std::fs::write(&outside, "elsewhere").expect("seed outside");
    let r = run(
        "tar_create",
        vec![s(&ws.path("out.tar")), s(&outside.to_string_lossy())],
    );
    let _ = std::fs::remove_file(&outside);
    assert_eq!(outcome(r), "E_POLICY_DENY");
    assert!(!Path::new(&ws.path("out.tar")).exists());
}

// ── cluster, cache, rag, knowledge graph, fine-tuning ─────────────────────

#[test]
fn cluster_builtins_refuse_bad_shapes() {
    let _g = lock();
    assert_eq!(
        outcome(run("cluster_create", vec![Value::Int(5)])),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run("cluster_create", vec![s("a"), s("b")])),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run(
            "cluster_add_node",
            vec![s("n-refuse-1"), s("10.0.0.1:80"), s("gpu")]
        )),
        "E_BAD_ARG",
        "capabilities must be an Array, not silently emptied"
    );
    assert_eq!(
        outcome(run(
            "cluster_add_node",
            vec![s("n-refuse-2"), s("bad address")]
        )),
        "E_BAD_ARG"
    );
}

#[test]
fn cluster_add_node_registers_rather_than_claiming_online() {
    let _g = lock();
    let r = run(
        "cluster_add_node",
        vec![s("n-registered-1"), s("10.0.0.2:80")],
    )
    .expect("add");
    assert_eq!(field(&r, "status"), s("registered"));
}

#[test]
fn cluster_create_name_defaults_only_when_absent() {
    let _g = lock();
    let r = run("cluster_create", vec![]).expect("default name");
    assert_eq!(field(&r, "name"), s("default"));
    let r = run("cluster_create", vec![s("alpha")]).expect("named");
    assert_eq!(field(&r, "name"), s("alpha"));
    assert_eq!(outcome(run("cluster_create", vec![s("")])), "E_BAD_ARG");
}

#[test]
fn kg_builtins_refuse_bad_pipelines_and_missing_entities() {
    let _g = lock();
    assert!(piped("kg_add", vec![], s("not a record")).is_err());
    assert_eq!(
        outcome(run(
            "kg_add",
            vec![s("person"), s("Ada"), s("not a record")]
        )),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run(
            "kg_relate",
            vec![s("no-such-a"), s("no-such-b"), s("knows")]
        )),
        "E_NOT_FOUND"
    );
}

#[test]
fn semantic_cache_and_rag_index_bound_what_they_keep() {
    let _g = lock();
    let big = "x".repeat((256 * 1024) + 1);
    assert_eq!(
        outcome(run("semantic_cache", vec![s("q"), s(&big)])),
        "E_BUDGET_EXCEEDED"
    );
    assert_eq!(
        outcome(run("semantic_cache", vec![s(""), s("r")])),
        "E_BAD_ARG"
    );
    let huge = "y".repeat((1 << 20) + 1);
    assert_eq!(
        outcome(run("rag_index", vec![s(&huge)])),
        "E_BUDGET_EXCEEDED"
    );
    assert_eq!(outcome(run("rag_index", vec![Value::Int(5)])), "E_BAD_ARG");
}

#[test]
fn finetune_start_refuses_rather_than_reporting_a_job() {
    let _g = lock();
    assert_eq!(
        outcome(run("finetune_start", vec![s("base"), s("train.jsonl")])),
        "E_UNIMPLEMENTED"
    );
    assert_eq!(
        outcome(run("finetune_status", vec![s("ft_none")])),
        "E_NOT_FOUND"
    );
    assert_eq!(
        outcome(run("finetune_cancel", vec![s("ft_none")])),
        "E_NOT_FOUND"
    );
    assert_eq!(outcome(run("finetune_start", vec![s("base")])), "E_BAD_ARG");
}

// ── recursive agents: budgets are refused before any model call ──────────

#[test]
fn rlm_budgets_are_bounded_and_refused_before_a_run() {
    let _g = lock();
    let no_tools = || Value::Array(vec![]);
    let cfg = |k: &str, v: Value| rec(&[(k, v)]);
    assert_eq!(
        outcome(run(
            "rlm_agent",
            vec![s("g"), no_tools(), cfg("max_depth", Value::Int(-1))]
        )),
        "E_BAD_ARG",
        "a negative depth must not wrap to an unbounded one"
    );
    assert_eq!(
        outcome(run(
            "rlm_agent",
            vec![s("g"), no_tools(), cfg("max_agents", Value::Int(1_000_000))]
        )),
        "E_BUDGET_EXCEEDED"
    );
    assert_eq!(
        outcome(run(
            "rlm_agent",
            vec![s("g"), no_tools(), cfg("max_agents_typo", Value::Int(2))]
        )),
        "E_BAD_ARG",
        "an unknown key is refused, not defaulted"
    );
    assert_eq!(
        outcome(run(
            "rlm_agent",
            vec![s("g"), Value::Array(vec![Value::Int(3)])]
        )),
        "E_BAD_ARG"
    );
    assert_eq!(
        outcome(run(
            "rlm_agent",
            vec![s("g"), Value::Array(vec![s("no_such_builtin_xyz")])]
        )),
        "E_NOT_FOUND",
        "a tool name that is not a builtin is refused, not dropped"
    );
    assert_eq!(
        outcome(run(
            "rlm_spawn",
            vec![s("w"), s("g"), Value::Array(vec![s("no_such_builtin_xyz")])]
        )),
        "E_NOT_FOUND"
    );
    assert_eq!(outcome(run("rlm_spawn", vec![s("w")])), "E_BAD_ARG");
}

// ── the declarations' own examples ───────────────────────────────────────

/// Every canonical name this tranche declares, except `hw_audio`, whose answer
/// depends on the host and is covered by `declared_signatures`.
const DECLARED: &[&str] = &[
    "kg_add",
    "kg_relate",
    "cluster_create",
    "cluster_add_node",
    "semantic_cache",
    "aggregate_results",
    "rag_index",
    "finetune_start",
    "finetune_status",
    "finetune_cancel",
    "rlm_agent",
    "rlm_spawn",
    "ai_backends",
    "tools",
    "tool_exec",
    "trace",
    "in",
    "plan",
    "plan_diff",
    "apply",
    "approve",
    "tar_create",
    "task_run",
    "task_list",
    "bzip2_decompress",
    "save_json",
    "telemetry_reset",
];

/// The fixed text an example must produce, with ANSI colour and quotes removed.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out.trim().trim_matches('"').to_string()
}

#[test]
fn the_tranche_declarations_answer_their_own_examples() {
    // The examples read the process environment (mode, workspace), so they run
    // under the same lock as every test that sets it.
    let _g = lock();
    // Run on a thread with a larger stack: evaluating an example is recursive.
    let body = || {
        for name in DECLARED {
            let sig = iron_shell::signature::signature_of(name)
                .unwrap_or_else(|| panic!("{name} is not declared"));
            assert!(!sig.examples.is_empty(), "{name} declares no example");
            for (code, want) in sig.examples {
                let stmts = iron_shell::parser::parse_program(code)
                    .unwrap_or_else(|e| panic!("{name}: {code}: {e}"));
                let mut env = Env::new();
                let got = iron_shell::eval::eval_program(&stmts, &mut env)
                    .unwrap_or_else(|e| panic!("{name}'s example failed: {code}: {e}"));
                let shown = plain(&format!("{got}"));
                assert!(
                    shown == plain(want),
                    "{name}'s example claims {want:?} but answered {shown:?}: {code}"
                );
            }
        }
    };
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(body)
        .expect("spawn")
        .join()
        .expect("examples answer what they claim");
}
