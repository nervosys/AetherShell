//! Opt-in local evaluation records (`IRONSHELL_RECORD`, src/record.rs).
//!
//! Four things must hold. Off means nothing is written. On means one line per
//! evaluation, with the source text only under `full`. A failure carries its
//! code and whether it was a refusal. And recording never changes the outcome
//! of the evaluation it records, including when the record cannot be written.
//!
//! Every test points `IRONSHELL_SESSION_DIR` at its own temp directory, so
//! nothing here can write the real session directory. Tests that set
//! environment variables hold `ENV_LOCK`.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use iron_shell::agent_api::server::{stream_eval_events, stream_pipeline_events};
use iron_shell::agent_api::{process_request, AgentRequest, AgentResponse, PipelineStep};
use iron_shell::env::Env;
use iron_shell::record::Surface;
use iron_shell::value::Value;
use serde_json::{json, Value as J};

/// Serialises the tests that set `IRONSHELL_SESSION_DIR`, `IRONSHELL_RECORD`
/// and `IRONSHELL_MODE`.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Holds the env lock and a fresh session directory for as long as it lives.
/// On drop it clears the variables it may have set and removes the directory.
struct Sandbox {
    _guard: MutexGuard<'static, ()>,
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = std::env::temp_dir().join(format!("ish-record-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("session")).expect("create session dir");
        std::env::set_var("IRONSHELL_SESSION_DIR", root.join("session"));
        std::env::remove_var("IRONSHELL_RECORD");
        std::env::remove_var("IRONSHELL_MODE");
        Sandbox {
            _guard: guard,
            root,
        }
    }

    fn session(&self) -> PathBuf {
        self.root.join("session")
    }

    fn record_file(&self) -> PathBuf {
        self.session().join("record.jsonl")
    }

    /// Every line of the record file, parsed.
    fn lines(&self) -> Vec<J> {
        let text = std::fs::read_to_string(self.record_file()).expect("record file exists");
        text.lines()
            .map(|l| serde_json::from_str(l).expect("each line is JSON"))
            .collect()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        std::env::remove_var("IRONSHELL_SESSION_DIR");
        std::env::remove_var("IRONSHELL_RECORD");
        std::env::remove_var("IRONSHELL_MODE");
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Evaluates `code` through the Agent API's `eval`, which records with the
/// `api` surface.
fn eval(code: &str) -> AgentResponse {
    process_request(&AgentRequest::Eval {
        code: code.to_string(),
    })
}

/// A path as IronShell string-literal text. Forward slashes, so that Windows
/// backslashes are not read as escapes.
fn shell_path(p: &std::path::Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

#[test]
fn off_by_default_writes_nothing() {
    let sb = Sandbox::new("off");
    // Unset, and every value other than exactly `1` or `full`.
    for value in [None, Some("0"), Some("yes"), Some("FULL"), Some("")] {
        match value {
            Some(v) => std::env::set_var("IRONSHELL_RECORD", v),
            None => std::env::remove_var("IRONSHELL_RECORD"),
        }
        let r = eval("1 + 1");
        assert!(r.success, "evaluation must still run: {:?}", r.error);
        assert!(
            !sb.record_file().exists(),
            "IRONSHELL_RECORD={value:?} must not create a record file"
        );
    }
}

#[test]
fn one_writes_a_line_without_source_text() {
    let sb = Sandbox::new("one");
    std::env::set_var("IRONSHELL_RECORD", "1");

    let r = eval("1 + 1");
    assert!(r.success);

    let lines = sb.lines();
    assert_eq!(lines.len(), 1, "one top-level evaluation, one line");
    let line = &lines[0];
    assert_eq!(line["v"], json!(1));
    assert_eq!(line["surface"], json!("api"));
    assert_eq!(line["ok"], json!(true));
    assert_eq!(line["heads"], json!([]), "1 + 1 calls no builtin");
    assert!(line["ts"].as_u64().is_some());
    assert!(line["seq"].as_u64().is_some());
    assert!(line["ms"].as_u64().is_some());
    assert_eq!(line["refused"], json!(false));
    assert!(
        line.get("cmd").is_none(),
        "source text is not stored by `1`"
    );
    assert!(line.get("code").is_none(), "no code on success");
    let hash = line["cmd_hash"].as_str().expect("cmd_hash");
    assert_eq!(hash.len(), 16);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

    let raw = std::fs::read_to_string(sb.record_file()).unwrap();
    assert!(
        !raw.contains("1 + 1"),
        "the source must not appear in the file"
    );
}

#[test]
fn full_includes_the_trimmed_source_and_top_level_heads() {
    let sb = Sandbox::new("full");
    std::env::set_var("IRONSHELL_RECORD", "full");

    let r = eval("  str.upper(\"ab\")  ");
    assert!(r.success, "{:?}", r.error);
    assert_eq!(r.result, Some(json!("AB")));

    let lines = sb.lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["cmd"], json!("str.upper(\"ab\")"));
    assert_eq!(lines[0]["heads"], json!(["str.upper"]));
}

#[test]
fn a_failing_call_records_its_code_and_retryability() {
    let sb = Sandbox::new("fail");
    std::env::set_var("IRONSHELL_RECORD", "1");

    let r = eval("no_such_builtin_for_record_test(1)");
    assert!(!r.success);

    let lines = sb.lines();
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(line["ok"], json!(false));
    assert_eq!(line["code"], json!("E_UNKNOWN_BUILTIN"));
    assert_eq!(line["retryable"], json!(true));
    assert_eq!(line["refused"], json!(false));
}

#[test]
fn a_refused_call_is_flagged_as_refused() {
    let sb = Sandbox::new("refused");
    std::env::set_var("IRONSHELL_RECORD", "1");
    std::env::set_var("IRONSHELL_MODE", "agent");

    // Destructive in agent mode: refused before anything is removed. The path
    // does not exist, so even an unguarded run could not delete anything.
    let target = sb.root.join("no-such-file-for-record-test");
    let r = eval(&format!("rm(\"{}\")", shell_path(&target)));
    assert!(!r.success, "agent mode must refuse rm without approval");

    let lines = sb.lines();
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(line["ok"], json!(false));
    assert_eq!(line["agent"], json!(true));
    assert_eq!(line["refused"], json!(true), "line: {line}");
    let code = line["code"].as_str().expect("a code on refusal");
    assert!(
        [
            "E_POLICY_DENY",
            "E_NEEDS_APPROVAL",
            "E_OUTSIDE_WORKSPACE",
            "E_OS_DENIED",
            "E_BUDGET_EXCEEDED"
        ]
        .contains(&code),
        "a refusal must carry a refusal code, got {code}"
    );
}

#[test]
fn the_file_and_c_surfaces_are_named() {
    let sb = Sandbox::new("surfaces");
    std::env::set_var("IRONSHELL_RECORD", "1");

    let mut env = Env::new();
    let _ = iron_shell::repl::run_one_as(&mut env, "1 + 1", Surface::File);
    let _ = iron_shell::repl::run_one(&mut env, "1 + 1");

    let lines = sb.lines();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["surface"], json!("file"));
    assert_eq!(lines[1]["surface"], json!("c"));
}

#[test]
fn summary_counts_a_retry_and_a_recovery() {
    let sb = Sandbox::new("summary");
    std::env::set_var("IRONSHELL_RECORD", "1");

    // Hand-written records, so the counts are known exactly.
    //  1. fails, file.read            (ts 1000)
    //  2. succeeds, file.read, 1 s later: a retry and a recovery
    //  3. refused, rm                 (ts 3000)
    //  4. succeeds, rm, 297 s later: same head, but outside the 120 s window
    //  5. fails, sys.hostname         (ts 400000), nothing follows
    let rows = [
        json!({"v":1,"ts":1000,"seq":0,"surface":"c","agent":false,"heads":["file.read"],
               "cmd_hash":"0000000000000001","ok":false,"code":"E_NOT_FOUND",
               "retryable":false,"refused":false,"ms":1}),
        json!({"v":1,"ts":2000,"seq":1,"surface":"c","agent":false,"heads":["file.read"],
               "cmd_hash":"0000000000000001","ok":true,"refused":false,"ms":1}),
        json!({"v":1,"ts":3000,"seq":2,"surface":"c","agent":true,"heads":["rm"],
               "cmd_hash":"0000000000000002","ok":false,"code":"E_POLICY_DENY",
               "retryable":false,"refused":true,"ms":1}),
        json!({"v":1,"ts":300000,"seq":3,"surface":"c","agent":true,"heads":["rm"],
               "cmd_hash":"0000000000000002","ok":true,"refused":false,"ms":1}),
        json!({"v":1,"ts":400000,"seq":4,"surface":"c","agent":false,"heads":["sys.hostname"],
               "cmd_hash":"0000000000000003","ok":false,"code":"E_UNKNOWN_BUILTIN",
               "retryable":true,"refused":false,"ms":1}),
    ];
    let mut text = String::new();
    for row in &rows {
        text.push_str(&row.to_string());
        text.push('\n');
    }
    std::fs::write(sb.record_file(), text).expect("write record file");

    let mut env = Env::new();
    let summary = iron_shell::builtins::call("record_summary", vec![], &mut env)
        .expect("record_summary runs");
    let Value::Record(m) = summary else {
        panic!("record_summary must return a Record");
    };
    assert_eq!(m["enabled"], Value::Bool(true));
    assert_eq!(m["attempts"], Value::Int(5));
    assert_eq!(m["failures"], Value::Int(3));
    assert_eq!(m["refusals"], Value::Int(1));
    assert_eq!(m["retries"], Value::Int(1));
    assert_eq!(m["recoveries"], Value::Int(1));
    assert_eq!(m["first_try_ok"], Value::Int(1));
    let Value::Record(by_code) = &m["by_code"] else {
        panic!("by_code must be a Record");
    };
    assert_eq!(by_code["E_NOT_FOUND"], Value::Int(1));
    assert_eq!(by_code["E_POLICY_DENY"], Value::Int(1));
    assert_eq!(by_code["E_UNKNOWN_BUILTIN"], Value::Int(1));
}

#[test]
fn summary_is_zero_and_disabled_when_recording_is_off() {
    let sb = Sandbox::new("summary-off");
    // A file exists, but recording is off, so it must not be read.
    std::fs::write(sb.record_file(), "{\"ts\":1,\"ok\":false}\n").unwrap();

    let mut env = Env::new();
    let summary = iron_shell::builtins::call("record_summary", vec![], &mut env).unwrap();
    let Value::Record(m) = summary else {
        panic!("record_summary must return a Record");
    };
    assert_eq!(m["enabled"], Value::Bool(false));
    assert_eq!(m["attempts"], Value::Int(0));
    assert_eq!(m["failures"], Value::Int(0));
}

#[test]
fn record_summary_refuses_piped_input() {
    let _sb = Sandbox::new("piped");
    let mut env = Env::new();
    let err = iron_shell::builtins::call_with_input(
        "record_summary",
        vec![],
        Some(Value::Str("x".to_string())),
        &mut env,
    )
    .expect_err("piped input must be refused");
    assert!(err.to_string().contains("E_BAD_ARG"), "{err}");
}

#[test]
fn record_summary_refuses_arguments() {
    let _sb = Sandbox::new("args");
    let mut env = Env::new();
    let err = iron_shell::builtins::call_with_input(
        "record_summary",
        vec![Value::Int(1)],
        None,
        &mut env,
    )
    .expect_err("an argument must be refused, not ignored");
    assert!(err.to_string().contains("E_BAD_ARG"), "{err}");
}

#[test]
fn a_record_that_cannot_be_written_does_not_change_the_result() {
    let sb = Sandbox::new("unwritable");
    // The session directory is a regular file, so creating records fails.
    let blocker = sb.root.join("not-a-directory");
    std::fs::write(&blocker, "x").expect("write blocker");
    std::env::set_var("IRONSHELL_SESSION_DIR", &blocker);

    std::env::remove_var("IRONSHELL_RECORD");
    let off = eval("str.upper(\"ab\")");
    std::env::set_var("IRONSHELL_RECORD", "1");
    let on = eval("str.upper(\"ab\")");

    assert!(on.success, "recording failure must not fail the call");
    assert_eq!(on.success, off.success);
    assert_eq!(on.result, off.result);
    assert_eq!(on.error, off.error);
    assert_eq!(on.result, Some(json!("AB")));
}

// ---------------------------------------------------------------------------
// Surfaces the first version did not reach: pipelines, streamed evaluations,
// calls whose arguments do not convert, and POSIX-subset scripts.
// ---------------------------------------------------------------------------

/// A pipeline of the given steps, run through the Agent API.
fn pipeline(steps: J, input: Option<J>) -> AgentResponse {
    let steps: Vec<PipelineStep> = serde_json::from_value(steps).expect("valid steps");
    process_request(&AgentRequest::Pipeline { steps, input })
}

/// Runs `ish` as its own process, with the record variables set for it. The
/// POSIX path exits the process, so it cannot be run in-process.
fn run_ish(sb: &Sandbox, args: &[&str], level: &str) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_ish"))
        .args(args)
        .env("IRONSHELL_SESSION_DIR", sb.session())
        .env("IRONSHELL_RECORD", level)
        .env_remove("IRONSHELL_MODE")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run ish")
}

#[test]
fn a_pipeline_is_one_api_attempt_with_its_steps_as_heads() {
    let sb = Sandbox::new("pipeline");
    std::env::set_var("IRONSHELL_RECORD", "1");

    let r = pipeline(
        json!([{ "builtin": "reverse" }, { "builtin": "len" }]),
        Some(json!([1, 2, 3])),
    );
    assert!(r.success, "{:?}", r.error);

    let lines = sb.lines();
    assert_eq!(lines.len(), 1, "one pipeline, one line");
    assert_eq!(lines[0]["surface"], json!("api"));
    assert_eq!(lines[0]["ok"], json!(true));
    assert_eq!(lines[0]["heads"], json!(["reverse", "len"]));
}

#[test]
fn a_failing_pipeline_step_keeps_its_code_and_retryability() {
    let sb = Sandbox::new("pipeline-fail");
    std::env::set_var("IRONSHELL_RECORD", "1");

    let r = pipeline(
        json!([{ "builtin": "no_such_builtin_for_record_test" }]),
        None,
    );
    assert!(!r.success);

    let lines = sb.lines();
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(line["surface"], json!("api"));
    assert_eq!(line["ok"], json!(false));
    assert_eq!(line["code"], json!("E_UNKNOWN_BUILTIN"));
    assert_eq!(line["retryable"], json!(true));
    assert_eq!(line["refused"], json!(false));
    assert_eq!(line["heads"], json!(["no_such_builtin_for_record_test"]));
}

#[test]
fn a_refused_pipeline_step_is_flagged_as_refused() {
    let sb = Sandbox::new("pipeline-refused");
    std::env::set_var("IRONSHELL_RECORD", "1");
    std::env::set_var("IRONSHELL_MODE", "agent");

    let target = sb.root.join("no-such-file-for-record-test");
    let r = pipeline(
        json!([{ "builtin": "rm", "args": [shell_path(&target)] }]),
        None,
    );
    assert!(!r.success, "agent mode must refuse rm without approval");

    let lines = sb.lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["refused"], json!(true), "line: {}", lines[0]);
    assert_eq!(lines[0]["agent"], json!(true));
}

#[test]
fn a_streamed_pipeline_is_one_api_attempt() {
    let sb = Sandbox::new("stream-pipeline");
    std::env::set_var("IRONSHELL_RECORD", "1");

    let steps: Vec<PipelineStep> =
        serde_json::from_value(json!([{ "builtin": "reverse" }, { "builtin": "len" }]))
            .expect("valid steps");
    let events = stream_pipeline_events(&steps, Some(&json!([1, 2, 3])), None);
    assert_eq!(events.last().map(|e| e.event.as_str()), Some("complete"));

    let lines = sb.lines();
    assert_eq!(lines.len(), 1, "one streamed pipeline, one line");
    assert_eq!(lines[0]["surface"], json!("api"));
    assert_eq!(lines[0]["ok"], json!(true));
    assert_eq!(lines[0]["heads"], json!(["reverse", "len"]));
}

#[test]
fn a_streamed_eval_is_one_api_attempt_with_its_source_under_full() {
    let sb = Sandbox::new("stream-eval");
    std::env::set_var("IRONSHELL_RECORD", "full");

    let ok = stream_eval_events("str.upper(\"ab\")", None);
    assert_eq!(ok.last().map(|e| e.event.as_str()), Some("complete"));
    let bad = stream_eval_events("no_such_builtin_for_record_test(1)", None);
    assert_eq!(bad.last().map(|e| e.event.as_str()), Some("error"));

    let lines = sb.lines();
    assert_eq!(lines.len(), 2, "one line per streamed evaluation");
    assert_eq!(lines[0]["surface"], json!("api"));
    assert_eq!(lines[0]["ok"], json!(true));
    assert_eq!(lines[0]["cmd"], json!("str.upper(\"ab\")"));
    assert_eq!(lines[0]["heads"], json!(["str.upper"]));
    assert_eq!(lines[1]["ok"], json!(false));
    assert_eq!(lines[1]["code"], json!("E_UNKNOWN_BUILTIN"));
}

#[test]
fn a_call_whose_arguments_do_not_convert_is_a_failed_attempt() {
    let sb = Sandbox::new("call-bad-args");
    std::env::set_var("IRONSHELL_RECORD", "1");

    // `reverse` declares no parameter named `nope`, so the arguments cannot
    // become IronShell source and nothing runs.
    let r = process_request(&AgentRequest::Call {
        builtin: "reverse".to_string(),
        args: json!({ "nope": 1 }),
    });
    assert!(!r.success);
    assert_eq!(r.code.as_deref(), Some("E_BAD_ARG"));

    let lines = sb.lines();
    assert_eq!(lines.len(), 1, "the refused call is still one attempt");
    let line = &lines[0];
    assert_eq!(line["surface"], json!("api"));
    assert_eq!(line["ok"], json!(false));
    assert_eq!(line["code"], json!("E_BAD_ARG"));
    assert_eq!(line["retryable"], json!(true));
    assert_eq!(line["heads"], json!(["reverse"]));
    assert_eq!(
        line["cmd_hash"],
        json!("cbf29ce484222325"),
        "hash of the empty source"
    );
}

#[test]
fn a_posix_script_is_recorded_under_sh_with_its_command_heads() {
    let sb = Sandbox::new("sh");
    let out = run_ish(
        &sb,
        &[
            "--bash",
            "-c",
            "if test 1 -eq 1; then echo hi; fi; echo done | wc -l",
        ],
        "1",
    );
    assert!(out.status.success(), "script must run: {out:?}");

    let lines = sb.lines();
    assert_eq!(lines.len(), 1, "one script, one line");
    let line = &lines[0];
    assert_eq!(line["surface"], json!("sh"));
    assert_eq!(line["ok"], json!(true));
    assert!(line.get("code").is_none());
    assert_eq!(line["heads"], json!(["test", "echo", "echo", "wc"]));
    assert!(line.get("cmd").is_none(), "no source text under `1`");
}

#[test]
fn a_nonzero_posix_status_is_a_failed_sh_attempt_without_a_code() {
    let sb = Sandbox::new("sh-status");
    let out = run_ish(&sb, &["--bash", "-c", "echo x; exit 3"], "1");
    assert_eq!(
        out.status.code(),
        Some(3),
        "the script's own status is kept"
    );

    let lines = sb.lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["surface"], json!("sh"));
    assert_eq!(lines[0]["ok"], json!(false));
    assert!(
        lines[0].get("code").is_none(),
        "a script's exit is not an IronShell code"
    );
    assert_eq!(lines[0]["refused"], json!(false));
}

#[test]
fn a_posix_script_stores_its_source_only_under_full() {
    let sb = Sandbox::new("sh-full");
    let out = run_ish(&sb, &["--bash", "-c", "echo hi"], "full");
    assert!(out.status.success());

    let lines = sb.lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["surface"], json!("sh"));
    assert_eq!(lines[0]["cmd"], json!("echo hi"));
    assert_eq!(lines[0]["heads"], json!(["echo"]));
}

#[test]
fn an_empty_head_never_counts_as_a_retry() {
    let sb = Sandbox::new("empty-head");
    std::env::set_var("IRONSHELL_RECORD", "1");

    // Two failed scripts whose first word is not a literal. They share the
    // empty head, which names no command, so neither is a retry of the other.
    let rows = [
        json!({"v":1,"ts":1000,"seq":0,"surface":"sh","agent":false,"heads":[""],
               "cmd_hash":"0000000000000001","ok":false,"refused":false,"ms":1}),
        json!({"v":1,"ts":2000,"seq":1,"surface":"sh","agent":false,"heads":[""],
               "cmd_hash":"0000000000000002","ok":true,"refused":false,"ms":1}),
    ];
    let mut text = String::new();
    for row in &rows {
        text.push_str(&row.to_string());
        text.push('\n');
    }
    std::fs::write(sb.record_file(), text).expect("write record file");

    let mut env = Env::new();
    let summary = iron_shell::builtins::call("record_summary", vec![], &mut env)
        .expect("record_summary runs");
    let Value::Record(m) = summary else {
        panic!("record_summary must return a Record");
    };
    assert_eq!(m["retries"], Value::Int(0));
    assert_eq!(m["recoveries"], Value::Int(0));
}
