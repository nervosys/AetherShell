//! Contracts for the mcp_* and a2ui_* builtins that had no declaration.
//!
//! Three properties are checked. A declared builtin refuses a call it cannot
//! honour, with a coded error, before its body runs. A piped value is refused
//! where the builtin does not take one, instead of being dropped. And the
//! a2ui prompts, with no UI attached, refuse `E_NO_UI` at once rather than
//! waiting out their timeout or answering for a user who is not there.
//!
//! Nothing here starts a server, listens on any address, or connects to an MCP
//! server. The only child processes are `ish` itself, each with a hard deadline,
//! and their home directory is a temporary one this test writes.

use iron_shell::builtins::{call, call_with_input, is_dispatched};
use iron_shell::env::Env;
use iron_shell::safety::SafetyError;
use iron_shell::signature::signature_of;
use iron_shell::value::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MCP_NAMES: &[&str] = &[
    "mcp_cache_clear",
    "mcp_cache_status",
    "mcp_call",
    "mcp_call_tool",
    "mcp_client",
    "mcp_connect",
    "mcp_detect",
    "mcp_list_prompts",
    "mcp_list_resources",
    "mcp_list_tools",
    "mcp_server_start",
    "mcp_servers",
    "mcp_tools",
];

const A2UI_NAMES: &[&str] = &[
    "a2ui_agent_thinking",
    "a2ui_confirm",
    "a2ui_progress",
    "a2ui_progress_complete",
    "a2ui_prompt",
    "a2ui_render_code",
    "a2ui_render_table",
    "a2ui_select",
];

/// An environment whose undo journal is this test process's own.
fn env() -> Env {
    static ISOLATED: std::sync::Once = std::sync::Once::new();
    ISOLATED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_mcp_tranche_{}", std::process::id()));
        std::env::set_var("IRONSHELL_SESSION_DIR", dir);
    });
    Env::new()
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn record(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

/// The stable code of a refusal, or the text of an uncoded one so the failure
/// says what escaped.
fn code_of(e: &anyhow::Error) -> String {
    match e.downcast_ref::<SafetyError>() {
        Some(se) => se.code.as_str().to_string(),
        None => format!("UNCODED: {e}"),
    }
}

fn refused_with(name: &str, args: Vec<Value>, input: Option<Value>) -> String {
    match call_with_input(name, args, input, &mut env()) {
        Ok(v) => panic!("{name} accepted a call it should refuse and answered {v:?}"),
        Err(e) => code_of(&e),
    }
}

// ── declarations exist and dispatch ──────────────────────────────────────

#[test]
fn every_builtin_in_this_tranche_is_declared_and_dispatched() {
    for name in MCP_NAMES.iter().chain(A2UI_NAMES) {
        assert!(signature_of(name).is_some(), "{name} is not declared");
        assert!(is_dispatched(name), "{name} is declared but not dispatched");
    }
}

// ── argument refusals are coded ──────────────────────────────────────────

#[test]
fn a_bad_argument_to_an_mcp_or_a2ui_builtin_is_a_coded_refusal() {
    let unknown_key = record(&[("bogus", s("x"))]);
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("mcp_servers", vec![Value::Int(1)]),
        ("mcp_detect", vec![Value::Int(5)]),
        ("mcp_cache_clear", vec![Value::Int(1)]),
        ("mcp_cache_status", vec![Value::Int(1)]),
        ("mcp_tools", vec![s("git")]),
        ("mcp_tools", vec![unknown_key.clone()]),
        (
            "mcp_tools",
            vec![record(&[("category", s("no-such-category"))])],
        ),
        (
            "mcp_tools",
            vec![record(&[("safety_level", s("reckless"))])],
        ),
        ("mcp_call", vec![]),
        ("mcp_call", vec![Value::Int(5)]),
        ("mcp_call", vec![s("ls"), Value::Int(5)]),
        ("mcp_connect", vec![]),
        ("mcp_connect", vec![Value::Int(1)]),
        ("mcp_server_start", vec![]),
        ("mcp_server_start", vec![s("fs")]),
        ("mcp_server_start", vec![record(&[("name", Value::Int(3))])]),
        ("mcp_server_start", vec![unknown_key]),
        ("a2ui_progress", vec![s("x"), Value::Int(-1), Value::Int(3)]),
        ("a2ui_progress", vec![s("x"), Value::Int(4), Value::Int(3)]),
        (
            "a2ui_progress",
            vec![s("x"), Value::Int(1), Value::Int(3), s("nope")],
        ),
        ("a2ui_progress", vec![s("x")]),
        ("a2ui_progress_complete", vec![s("not-a-uuid")]),
        ("a2ui_prompt", vec![]),
        ("a2ui_prompt", vec![s("x"), s("extra")]),
        ("a2ui_confirm", vec![]),
        ("a2ui_select", vec![s("pick")]),
        ("a2ui_select", vec![s("pick"), Value::Array(vec![])]),
        ("a2ui_render_table", vec![s("h")]),
        (
            "a2ui_render_table",
            vec![Value::Array(vec![s("a")]), Value::Int(3)],
        ),
        (
            "a2ui_render_table",
            vec![
                Value::Array(vec![s("a")]),
                Value::Array(vec![Value::Int(1)]),
            ],
        ),
        ("a2ui_render_code", vec![]),
        ("a2ui_agent_thinking", vec![]),
        (
            "a2ui_agent_thinking",
            vec![s("a1"), s("thinking"), Value::Int(-1)],
        ),
    ];
    for (name, args) in cases {
        let shown = format!("{args:?}");
        assert_eq!(
            refused_with(name, args, None),
            "E_BAD_ARG",
            "{name}({shown}) was not a coded E_BAD_ARG"
        );
    }
}

#[test]
fn an_unknown_mcp_tool_is_not_found_not_a_record_with_an_error_flag() {
    let mut e = env();
    let err = call("mcp_call", vec![s("no-such-tool-xyz")], &mut e)
        .expect_err("an unknown tool answered a value");
    assert_eq!(code_of(&err), "E_NOT_FOUND");
}

#[test]
fn an_mcp_tool_filter_that_is_not_understood_says_so() {
    let mut e = env();
    let err = call(
        "mcp_tools",
        vec![record(&[("safety_level", s("reckless"))])],
        &mut e,
    )
    .expect_err("an unknown safety level was taken as caution");
    let text = err.to_string();
    assert!(
        text.contains("reckless"),
        "the refusal does not name it: {text}"
    );
    // The known filters still work.
    let all = call("mcp_tools", vec![], &mut e).expect("mcp_tools()");
    assert!(matches!(all, Value::Array(_)));
    let none = call(
        "mcp_tools",
        vec![record(&[("search", s("zz-no-such-tool"))])],
        &mut e,
    )
    .expect("a search that matches nothing");
    assert_eq!(none, Value::Array(vec![]));
}

// ── piped input is refused where it is not documented ────────────────────

#[test]
fn a_piped_value_is_refused_by_builtins_that_take_no_subject() {
    let piped = Some(s("ignored?"));
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("mcp_servers", vec![]),
        ("mcp_detect", vec![]),
        ("mcp_cache_clear", vec![]),
        ("mcp_cache_status", vec![]),
        ("mcp_tools", vec![]),
        ("mcp_call", vec![s("ls")]),
        ("mcp_connect", vec![s("http://127.0.0.1:9")]),
        ("mcp_server_start", vec![record(&[("name", s("fs"))])]),
        ("a2ui_progress", vec![s("x"), Value::Int(1), Value::Int(3)]),
    ];
    for (name, args) in cases {
        assert_eq!(
            refused_with(name, args, piped.clone()),
            "E_BAD_ARG",
            "{name} accepted a piped value and dropped it"
        );
    }
}

// ── mcp_connect and mcp_server_start answer honestly ─────────────────────

#[test]
fn mcp_connect_to_an_endpoint_nothing_answers_on_says_so_in_the_record() {
    let mut e = env();
    let v = call("mcp_connect", vec![s("http://127.0.0.1:9")], &mut e).expect("record");
    let Value::Record(rec) = v else {
        panic!("mcp_connect answered {v:?}, not a record")
    };
    assert_eq!(rec.get("available"), Some(&Value::Bool(false)));
    assert!(
        rec.contains_key("error"),
        "the unavailable record has no error field"
    );
}

#[test]
fn mcp_server_start_declares_a_server_and_starts_none() {
    let mut e = env();
    let v = call(
        "mcp_server_start",
        vec![record(&[("name", s("fs")), ("type", s("builtin"))])],
        &mut e,
    )
    .expect("declaration");
    let Value::Record(rec) = v else {
        panic!("mcp_server_start answered {v:?}, not a record")
    };
    assert_eq!(rec.get("status"), Some(&s("declared (not started)")));
    let hint = rec
        .get("hint")
        .map(|h| h.to_display_string())
        .unwrap_or_default();
    assert!(
        hint.contains("ish mcp serve"),
        "the hint does not name the real server: {hint}"
    );
}

#[test]
fn mcp_server_start_keeps_the_name_and_type_it_was_given() {
    let mut e = env();
    let v = call(
        "mcp_server_start",
        vec![record(&[("name", s("docs")), ("type", s("remote"))])],
        &mut e,
    )
    .expect("declaration");
    let Value::Record(rec) = v else {
        panic!("not a record")
    };
    assert_eq!(rec.get("name"), Some(&s("docs")));
    assert_eq!(rec.get("type"), Some(&s("remote")));
}

// ── the a2ui prompts refuse with no UI attached, and display calls queue ──

#[test]
fn each_a2ui_prompt_refuses_e_no_ui_at_once_when_nothing_is_attached() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("a2ui_prompt", vec![s("Name?")]),
        ("a2ui_confirm", vec![s("Delete?")]),
        (
            "a2ui_select",
            vec![s("Pick"), Value::Array(vec![s("a"), s("b")])],
        ),
    ];
    for (name, args) in cases {
        let started = Instant::now();
        let code = refused_with(name, args, None);
        assert_eq!(code, "E_NO_UI", "{name} did not refuse for want of a UI");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{name} took {:?} to refuse",
            started.elapsed()
        );
    }
}

#[test]
fn a2ui_display_calls_queue_without_waiting_for_a_ui() {
    let mut e = env();
    let started = Instant::now();
    let progress = call(
        "a2ui_progress",
        vec![s("build"), Value::Int(1), Value::Int(3)],
        &mut e,
    )
    .expect("progress is queued");
    let Value::Record(rec) = progress else {
        panic!("a2ui_progress answered {progress:?}")
    };
    assert_eq!(rec.get("current"), Some(&Value::Int(1)));
    let id = rec
        .get("progress_id")
        .map(|v| v.to_display_string())
        .expect("progress_id");
    let done = call("a2ui_progress_complete", vec![s(&id)], &mut e).expect("complete");
    assert_eq!(done, Value::Bool(true));

    assert_eq!(
        call(
            "a2ui_render_table",
            vec![
                Value::Array(vec![s("a")]),
                Value::Array(vec![Value::Array(vec![s("1")])])
            ],
            &mut e
        )
        .expect("table"),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "a2ui_render_code",
            vec![s("rust"), s("fn main() {}")],
            &mut e
        )
        .expect("code"),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            "a2ui_agent_thinking",
            vec![s("a1"), s("planning"), Value::Int(2)],
            &mut e
        )
        .expect("thinking"),
        Value::Bool(true)
    );
    assert!(started.elapsed() < Duration::from_secs(10));
}

// ── child processes: the ish binary, with a deadline and a temporary home ──

/// Run `ish -c <expr>` with a temporary home and a hard deadline.
///
/// Returns whether it exited 0 inside 30 seconds (it is killed otherwise), the
/// text it printed to stdout and stderr, and how long it ran.
///
/// Stdout and stderr go to files, so a child that writes a lot cannot block on
/// a pipe nobody reads. Stdin is null, so nothing can wait for input.
fn run_ae(expr: &str, home: &Path) -> (bool, String, Duration) {
    std::fs::create_dir_all(home).expect("temporary home");
    let out_path = home.join("stdout.txt");
    let err_path = home.join("stderr.txt");
    let out = std::fs::File::create(&out_path).expect("stdout file");
    let err = std::fs::File::create(&err_path).expect("stderr file");
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ish"))
        .arg("-c")
        .arg(expr)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("APPDATA", home)
        .env("LOCALAPPDATA", home)
        .env("XDG_CONFIG_HOME", home)
        .env("IRONSHELL_SESSION_DIR", home.join("session"))
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
        .expect("spawn ae");
    let deadline = started + Duration::from_secs(30);
    // True only for an exit status of 0 inside the deadline.
    let finished = loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => break status.success(),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    };
    let stdout = std::fs::read_to_string(&out_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(&err_path).unwrap_or_default();
    (finished, format!("{stdout}{stderr}"), started.elapsed())
}

fn fresh_home(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ae-mcp-tranche-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a2ui_prompts_refuse_e_no_ui_promptly_in_a_child_process() {
    let home = fresh_home("no-ui");
    for expr in [
        r#"try { a2ui_confirm("Delete?") } catch e { e.error.code }"#,
        r#"try { a2ui_prompt("Name?") } catch e { e.error.code }"#,
        r#"try { a2ui_select("Pick", ["a", "b"]) } catch e { e.error.code }"#,
    ] {
        let (finished, out, took) = run_ae(expr, &home);
        assert!(finished, "{expr} did not finish within 30 s");
        assert!(out.contains("E_NO_UI"), "{expr} did not say E_NO_UI: {out}");
        // The UI timeout is 300 s; a refusal must not come near it.
        assert!(took < Duration::from_secs(20), "{expr} took {took:?}");
        assert!(
            !out.trim().ends_with("false"),
            "{expr} answered false: {out}"
        );
    }
}

#[test]
fn a2ui_progress_answers_in_a_child_process_without_a_ui() {
    let home = fresh_home("progress");
    let (finished, out, _) = run_ae(r#"a2ui_progress("x", 1, 3).current"#, &home);
    assert!(finished, "a2ui_progress did not finish");
    assert!(out.contains('1'), "a2ui_progress did not answer: {out}");
}

#[test]
fn mcp_server_start_returns_at_once_in_a_child_process() {
    let home = fresh_home("server-start");
    let (finished, out, took) = run_ae(r#"mcp_server_start({name: "fs"}).status"#, &home);
    assert!(finished, "mcp_server_start blocked");
    assert!(out.contains("declared (not started)"), "{out}");
    assert!(took < Duration::from_secs(20));
}

#[test]
fn mcp_connect_to_nothing_is_an_unavailable_record_in_a_child_process() {
    let home = fresh_home("connect");
    let (finished, out, _) = run_ae(r#"mcp_connect("http://127.0.0.1:9").available"#, &home);
    assert!(finished, "mcp_connect did not finish");
    assert!(out.contains("false"), "{out}");
}

#[test]
fn mcp_detection_does_not_read_or_echo_a_config_secret() {
    // A Claude Desktop style and a Cursor style config, each with a token. The
    // builtins probe loopback ports and do not read these; the test asserts the
    // token is never echoed, and that a config in a home directory changes
    // nothing.
    let home = fresh_home("config-secret");
    let secret = "sk-TEST-SECRET-0123456789";
    let desktop = home.join("AppData").join("Roaming").join("Claude");
    std::fs::create_dir_all(&desktop).expect("desktop config dir");
    std::fs::write(
        desktop.join("claude_desktop_config.json"),
        format!(r#"{{"mcpServers":{{"github":{{"command":"npx","env":{{"GITHUB_TOKEN":"{secret}"}}}}}}}}"#),
    )
    .expect("write desktop config");
    let cursor = home.join(".cursor");
    std::fs::create_dir_all(&cursor).expect("cursor dir");
    std::fs::write(
        cursor.join("mcp.json"),
        format!(r#"{{"mcpServers":{{"x":{{"env":{{"API_KEY":"{secret}"}}}}}}}}"#),
    )
    .expect("write cursor config");

    for expr in [
        "mcp_servers()",
        "mcp_detect()",
        "mcp_tools()",
        "mcp_cache_status()",
    ] {
        let (finished, out, _) = run_ae(expr, &home);
        assert!(finished, "{expr} did not finish");
        assert!(
            !out.contains(secret),
            "{expr} echoed the config's secret: {out}"
        );
    }
}

#[test]
fn mcp_cache_clear_and_status_work_in_a_child_process() {
    let home = fresh_home("cache");
    let (finished, out, _) = run_ae("mcp_cache_clear()", &home);
    assert!(finished, "mcp_cache_clear did not finish");
    assert!(out.contains("true"), "{out}");
    let (finished, out, _) = run_ae("mcp_cache_status().ttl_seconds", &home);
    assert!(finished, "mcp_cache_status did not finish");
    assert!(out.contains("30"), "{out}");
}
