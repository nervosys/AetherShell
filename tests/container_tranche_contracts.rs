//! Contracts for the docker_* and podman_* builtins.
//!
//! No test here may reach a real container engine, because a developer's
//! machine may run Docker Desktop with containers and images that matter.
//!
//! - Refusals are argument checks that return before any process starts, so
//!   they run in-process. Each one is asserted to be a refusal, not a call.
//! - A call that would start docker or podman runs in a child `ish` process
//!   whose DOCKER_HOST and CONTAINER_HOST name an endpoint that does not exist.
//!   The call must then fail as a coded tool failure, or as E_TOOL_MISSING where
//!   the CLI is absent. A bare Null, false or empty answer fails the test.
//! - Unix tests put a fake `docker` first on the child's PATH. It records the
//!   argv it receives, so the argument shape is checked without a daemon.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Once;
use std::time::{Duration, Instant};

use iron_shell::builtins::{call_with_input, is_dispatched};
use iron_shell::env::Env;
use iron_shell::signature::signature_of;
use iron_shell::value::Value;

const DEADLINE: Duration = Duration::from_secs(30);

const CONTAINER_BUILTINS: &[&str] = &[
    "docker_build",
    "docker_compose_down",
    "docker_compose_ps",
    "docker_compose_up",
    "docker_cp",
    "docker_exec",
    "docker_inspect",
    "docker_logs",
    "docker_pull",
    "docker_push",
    "docker_rm",
    "docker_run",
    "docker_stop",
    "docker_tag",
    "docker_top",
    "podman_build",
    "podman_exec",
    "podman_images",
    "podman_logs",
    "podman_ps",
    "podman_pull",
    "podman_rm",
    "podman_run",
    "podman_stop",
];

/// Keep the in-process calls away from the journal and from agent-mode
/// approval, which would answer before the body's own refusal.
fn isolate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_container_tranche_{}", std::process::id()));
        std::env::set_var("IRONSHELL_SESSION_DIR", dir);
        for var in ["IRONSHELL_MODE", "IRONSHELL_AGENT", "IRONSHELL_POLICY"] {
            std::env::remove_var(var);
        }
    });
}

fn env() -> Env {
    isolate();
    Env::new()
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn arr(items: &[&str]) -> Value {
    Value::Array(items.iter().map(|item| s(item)).collect())
}

/// The refusal a call produces, which must be an error. A call that answers
/// is a bug here, because every call in this file must stop before a process.
fn refusal(name: &str, args: Vec<Value>, input: Option<Value>) -> String {
    match call_with_input(name, args, input, &mut env()) {
        Ok(value) => panic!("{name} answered {value:?} for a call it must refuse"),
        Err(e) => e.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Declarations and dispatch
// ---------------------------------------------------------------------------

#[test]
fn every_container_builtin_is_declared_and_dispatched() {
    for name in CONTAINER_BUILTINS {
        assert!(signature_of(name).is_some(), "{name} has no declaration");
        assert!(is_dispatched(name), "{name} is declared but not dispatched");
    }
}

// ---------------------------------------------------------------------------
// Refusals, in-process, before any process starts
// ---------------------------------------------------------------------------

#[test]
fn option_like_operands_are_refused_before_any_process_starts() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("docker_build", vec![s("--output=/tmp/x")]),
        ("docker_build", vec![s("app:1"), s("-ctx")]),
        ("docker_compose_down", vec![s("-f")]),
        ("docker_compose_ps", vec![s("--project-directory=/")]),
        ("docker_compose_up", vec![Value::Bool(true), s("-f")]),
        ("docker_cp", vec![s("-x"), s("web:/tmp")]),
        ("docker_exec", vec![s("--privileged"), arr(&["id"])]),
        ("docker_inspect", vec![s("-x")]),
        ("docker_logs", vec![s("--follow")]),
        ("docker_pull", vec![s("-q")]),
        ("docker_push", vec![s("--all-tags")]),
        ("docker_rm", vec![s("-f")]),
        ("docker_run", vec![s("--privileged")]),
        ("docker_stop", vec![s("-t")]),
        ("docker_tag", vec![s("-x"), s("y")]),
        ("docker_top", vec![s("-x")]),
        ("podman_build", vec![s("--output=/tmp/x")]),
        ("podman_exec", vec![s("--privileged"), arr(&["id"])]),
        ("podman_logs", vec![s("--follow")]),
        ("podman_pull", vec![s("-q")]),
        ("podman_rm", vec![s("-f")]),
        ("podman_run", vec![s("--privileged")]),
        ("podman_stop", vec![s("-t")]),
    ];
    for (name, args) in cases {
        let message = refusal(name, args, None);
        assert!(
            message.contains("E_BAD_ARG"),
            "{name} did not refuse an option-like operand as E_BAD_ARG: {message}"
        );
    }
}

#[test]
fn piped_input_is_refused_by_every_container_builtin() {
    let plausible: Vec<(&str, Vec<Value>)> = vec![
        ("docker_build", vec![s("app:1")]),
        ("docker_compose_down", vec![]),
        ("docker_compose_ps", vec![]),
        ("docker_compose_up", vec![]),
        ("docker_cp", vec![s("/tmp/x"), s("web:/x")]),
        ("docker_exec", vec![s("web"), arr(&["id"])]),
        ("docker_inspect", vec![s("web")]),
        ("docker_logs", vec![s("web")]),
        ("docker_pull", vec![s("alpine")]),
        ("docker_push", vec![s("alpine")]),
        ("docker_rm", vec![s("web")]),
        ("docker_run", vec![s("alpine")]),
        ("docker_stop", vec![s("web")]),
        ("docker_tag", vec![s("a:1"), s("b:1")]),
        ("docker_top", vec![s("web")]),
        ("podman_build", vec![s("app:1")]),
        ("podman_exec", vec![s("web"), arr(&["id"])]),
        ("podman_images", vec![]),
        ("podman_logs", vec![s("web")]),
        ("podman_ps", vec![]),
        ("podman_pull", vec![s("alpine")]),
        ("podman_rm", vec![s("web")]),
        ("podman_run", vec![s("alpine")]),
        ("podman_stop", vec![s("web")]),
    ];
    assert_eq!(
        plausible.len(),
        CONTAINER_BUILTINS.len(),
        "the pipeline table must cover every container builtin"
    );
    for (name, args) in plausible {
        let message = refusal(name, args, Some(s("piped text")));
        assert!(
            message.contains("takes no piped input"),
            "{name} did not refuse piped input in its own body: {message}"
        );
    }
}

#[test]
fn argv_must_be_an_array_and_never_a_shell_string() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("docker_exec", vec![s("web"), s("id -u")]),
        ("docker_exec", vec![s("web"), Value::Array(vec![])]),
        (
            "docker_exec",
            vec![s("web"), Value::Array(vec![s("id"), Value::Int(5)])],
        ),
        ("docker_run", vec![s("alpine"), s("sh -c 'echo hi'")]),
        ("podman_exec", vec![s("web"), s("id -u")]),
        ("podman_exec", vec![s("web"), Value::Array(vec![])]),
        (
            "podman_run",
            vec![s("alpine"), Value::Array(vec![s("ok\u{0}bad")])],
        ),
    ];
    for (name, args) in cases {
        let message = refusal(name, args, None);
        assert!(
            message.contains("E_BAD_ARG"),
            "{name} accepted a shell string or a malformed argv: {message}"
        );
    }
}

#[test]
fn run_flags_must_be_well_formed_and_typed() {
    let with_flags = |flags: BTreeMap<String, Value>| {
        vec![s("alpine"), Value::Array(vec![]), Value::Record(flags)]
    };
    let mut bad_name = BTreeMap::new();
    bad_name.insert("Bad Name".to_string(), Value::Bool(true));
    let mut option_value = BTreeMap::new();
    option_value.insert("name".to_string(), s("-x"));
    let mut float_value = BTreeMap::new();
    float_value.insert("cpus".to_string(), Value::Float(1.5));

    for (name, flags) in [
        ("docker_run", bad_name.clone()),
        ("docker_run", option_value.clone()),
        ("docker_run", float_value.clone()),
        ("podman_run", bad_name),
        ("podman_run", option_value),
        ("podman_run", float_value),
    ] {
        let message = refusal(name, with_flags(flags), None);
        assert!(
            message.contains("E_BAD_ARG"),
            "{name} accepted a malformed flag: {message}"
        );
    }
}

#[test]
fn log_tails_are_bounded_on_both_clis() {
    for name in ["docker_logs", "podman_logs"] {
        for tail in [0, 10_001] {
            let message = refusal(name, vec![s("web"), Value::Int(tail)], None);
            assert!(
                message.contains("E_BAD_ARG"),
                "{name} accepted tail {tail}: {message}"
            );
        }
    }
}

#[test]
fn docker_cp_needs_exactly_one_container_operand() {
    for (src, dest) in [
        ("a.txt", "b.txt"),
        ("web:/a", "db:/b"),
        ("/tmp/x", "web:"),
        ("-x", "web:/tmp"),
    ] {
        let message = refusal("docker_cp", vec![s(src), s(dest)], None);
        assert!(
            message.contains("E_BAD_ARG"),
            "docker_cp({src:?}, {dest:?}) was not refused: {message}"
        );
    }
}

#[test]
fn compose_up_refuses_a_foreground_run() {
    let message = refusal("docker_compose_up", vec![Value::Bool(false)], None);
    assert!(
        message.contains("E_BAD_ARG"),
        "a foreground compose up was accepted: {message}"
    );
}

// ---------------------------------------------------------------------------
// Child-process calls against an engine that does not exist
// ---------------------------------------------------------------------------

/// An endpoint that no engine listens on, unique to this test process.
fn unreachable_endpoint() -> String {
    let pid = std::process::id();
    if cfg!(windows) {
        format!("npipe:////./pipe/ae_no_such_docker_{pid}")
    } else {
        format!("unix:///nonexistent/ae_{pid}.sock")
    }
}

/// A temporary working directory with a build context. Removed on drop.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ae_container_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("docker-config")).expect("create fixture");
        std::fs::create_dir_all(root.join("session")).expect("create session directory");
        std::fs::write(root.join("Dockerfile"), "FROM scratch\n").expect("write Dockerfile");
        std::fs::write(root.join("payload.txt"), "payload\n").expect("write payload");
        Self { root }
    }

    /// The fixture path with forward slashes, for use inside an ae string.
    fn ish_path(&self, file: &str) -> String {
        self.root.join(file).to_string_lossy().replace('\\', "/")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Run {
    success: bool,
    stdout: String,
    stderr: String,
}

fn drain<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// Run `ish -c <expr>` in `fixture` with the engine pointed at nowhere, and with
/// `extra_env` applied last. Hard deadline: the child is killed after 30 s.
fn ae_child(fixture: &Fixture, expr: &str, extra_env: &[(&str, String)]) -> Run {
    let endpoint = unreachable_endpoint();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ish"));
    cmd.current_dir(&fixture.root)
        .env("IRONSHELL_SESSION_DIR", fixture.root.join("session"))
        .env("DOCKER_HOST", &endpoint)
        .env("CONTAINER_HOST", &endpoint)
        .env("DOCKER_CONFIG", fixture.root.join("docker-config"))
        .env_remove("DOCKER_CONTEXT")
        .env_remove("IRONSHELL_MODE")
        .env_remove("IRONSHELL_AGENT")
        .env_remove("IRONSHELL_POLICY");
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    let mut child = cmd
        .args(["-c", expr])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ae");
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll ae") {
            break status;
        }
        if started.elapsed() > DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            panic!("ae did not finish within 30 s: {expr}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Run {
        success: status.success(),
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    }
}

/// The error code a call produced, from `try { call } catch e { e.error.code }`.
fn caught_code(fixture: &Fixture, call: &str) -> String {
    let run = ae_child(
        fixture,
        &format!("(try {{ {call} }} catch e {{ e.error.code }})"),
        &[],
    );
    run.stdout.trim().trim_matches('"').to_string()
}

#[test]
fn every_container_call_fails_coded_when_the_engine_is_unreachable() {
    let fixture = Fixture::new("unreachable");
    let ctx = fixture.ish_path("");
    let payload = fixture.ish_path("payload.txt");
    let calls: Vec<(&str, String)> = vec![
        (
            "docker_build",
            format!(r#"docker_build("ae-no-such:1", "{ctx}")"#),
        ),
        ("docker_compose_down", "docker_compose_down()".to_string()),
        ("docker_compose_ps", "docker_compose_ps()".to_string()),
        ("docker_compose_up", "docker_compose_up()".to_string()),
        (
            "docker_cp",
            format!(r#"docker_cp("{payload}", "ae_no_such:/tmp/payload.txt")"#),
        ),
        (
            "docker_exec",
            r#"docker_exec("ae_no_such", ["id"])"#.to_string(),
        ),
        (
            "docker_inspect",
            r#"docker_inspect("ae_no_such")"#.to_string(),
        ),
        ("docker_logs", r#"docker_logs("ae_no_such", 5)"#.to_string()),
        (
            "docker_pull",
            r#"docker_pull("ae-no-such:latest")"#.to_string(),
        ),
        (
            "docker_push",
            r#"docker_push("ae-no-such:latest")"#.to_string(),
        ),
        ("docker_rm", r#"docker_rm("ae_no_such")"#.to_string()),
        (
            "docker_run",
            r#"docker_run("alpine", ["true"], {rm: true})"#.to_string(),
        ),
        ("docker_stop", r#"docker_stop("ae_no_such")"#.to_string()),
        (
            "docker_tag",
            r#"docker_tag("ae-no-such:a", "ae-no-such:b")"#.to_string(),
        ),
        ("docker_top", r#"docker_top("ae_no_such")"#.to_string()),
        (
            "podman_build",
            format!(r#"podman_build("ae-no-such:1", "{ctx}")"#),
        ),
        (
            "podman_exec",
            r#"podman_exec("ae_no_such", ["id"])"#.to_string(),
        ),
        ("podman_images", "podman_images()".to_string()),
        ("podman_logs", r#"podman_logs("ae_no_such", 5)"#.to_string()),
        ("podman_ps", "podman_ps()".to_string()),
        (
            "podman_pull",
            r#"podman_pull("ae-no-such:latest")"#.to_string(),
        ),
        ("podman_rm", r#"podman_rm("ae_no_such")"#.to_string()),
        (
            "podman_run",
            r#"podman_run("alpine", ["true"], {rm: true})"#.to_string(),
        ),
        ("podman_stop", r#"podman_stop("ae_no_such")"#.to_string()),
    ];
    assert_eq!(calls.len(), CONTAINER_BUILTINS.len());
    for (name, call) in &calls {
        let code = caught_code(&fixture, call);
        assert!(
            code == "E_TOOL_FAILED" || code == "E_TOOL_MISSING",
            "{name} answered {code:?} (expected a coded tool failure or E_TOOL_MISSING)\n  call: {call}"
        );
    }
}

/// On Windows a program is searched for in the application's own directory and
/// in the system directories before the PATH it is given, so an empty PATH does
/// not hide a `docker.exe` installed there (CI runners have one). Returns such a
/// copy, if there is one.
#[cfg(windows)]
fn docker_outside_path_override() -> Option<PathBuf> {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let mut dirs = vec![PathBuf::from(&root).join("System32"), PathBuf::from(&root)];
    if let Some(dir) = PathBuf::from(env!("CARGO_BIN_EXE_ish")).parent() {
        dirs.push(dir.to_path_buf());
    }
    dirs.into_iter()
        .map(|dir| dir.join("docker.exe"))
        .find(|path| path.is_file())
}

#[test]
fn an_absent_cli_is_coded_as_tool_missing() {
    // The premise is an absent docker CLI. Where one sits in a directory that
    // no PATH override reaches, that premise is false and the test cannot hide it.
    #[cfg(windows)]
    {
        if let Some(copy) = docker_outside_path_override() {
            eprintln!(
                "skipped: {} is outside the PATH override, so docker cannot be hidden here",
                copy.display()
            );
            return;
        }
    }
    let fixture = Fixture::new("absent");
    let empty = fixture.root.join("empty-path");
    std::fs::create_dir_all(&empty).expect("create empty PATH directory");
    let run = ae_child(
        &fixture,
        r#"(try { docker_stop("ae_no_such") } catch e { e.error.code })"#,
        &[("PATH", empty.to_string_lossy().into_owned())],
    );
    assert!(
        run.success,
        "the caught error should exit 0: {}",
        run.stderr
    );
    assert!(
        run.stdout.contains("E_TOOL_MISSING"),
        "a missing docker CLI was not reported as E_TOOL_MISSING\nstdout: {}\nstderr: {}",
        run.stdout,
        run.stderr
    );
}

// ---------------------------------------------------------------------------
// Unix only: a fake `docker` records the argv it receives
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod fake_cli {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Put a `docker` script first on PATH. It appends each argument on its own
    /// line to $AE_FAKE_ARGV, prints $AE_FAKE_STDOUT, and runs $AE_FAKE_BODY.
    fn install(fixture: &Fixture, body: &str) -> Vec<(&'static str, String)> {
        let bin = fixture.root.join("fake-bin");
        std::fs::create_dir_all(&bin).expect("create fake bin");
        let script = bin.join("docker");
        let text = format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> \"$AE_FAKE_ARGV\"; done\n{body}\n"
        );
        std::fs::write(&script, text).expect("write fake docker");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake docker");
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        vec![
            ("PATH", path),
            (
                "AE_FAKE_ARGV",
                fixture.root.join("argv.txt").to_string_lossy().into_owned(),
            ),
        ]
    }

    fn argv_lines(fixture: &Fixture) -> Vec<String> {
        std::fs::read_to_string(fixture.root.join("argv.txt"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn docker_run_hands_the_engine_an_argv_with_no_shell_and_no_option_leak() {
        let fixture = Fixture::new("run-argv");
        let env = install(&fixture, "printf 'started\\n'");
        let run = ae_child(
            &fixture,
            r#"docker_run("alpine", ["-x", "a b; touch pwned"], {rm: true, name: "web"})"#,
            &env,
        );
        assert!(run.success, "docker_run failed: {}", run.stderr);
        assert_eq!(run.stdout.trim(), "started");
        assert_eq!(
            argv_lines(&fixture),
            vec![
                "run",
                "--name",
                "web",
                "--rm",
                "--",
                "alpine",
                "-x",
                "a b; touch pwned",
            ]
        );
        assert!(
            !fixture.root.join("pwned").exists(),
            "an argv element was interpreted by a shell"
        );
    }

    #[test]
    fn docker_exec_passes_the_command_as_argv_after_the_container() {
        let fixture = Fixture::new("exec-argv");
        let env = install(&fixture, "printf 'uid=0\\n'");
        let run = ae_child(
            &fixture,
            r#"docker_exec("web", ["-u", "root", "id"])"#,
            &env,
        );
        assert!(run.success, "docker_exec failed: {}", run.stderr);
        assert_eq!(run.stdout.trim(), "uid=0");
        assert_eq!(
            argv_lines(&fixture),
            vec!["exec", "--", "web", "-u", "root", "id"]
        );
    }

    #[test]
    fn logs_return_stdout_and_the_containers_stderr() {
        let fixture = Fixture::new("logs-both");
        let env = install(&fixture, "printf 'err-line\\n' >&2; printf 'out-line\\n'");
        let run = ae_child(&fixture, r#"docker_logs("web")"#, &env);
        assert!(run.success, "docker_logs failed: {}", run.stderr);
        assert!(
            run.stdout.contains("out-line"),
            "stdout lost: {}",
            run.stdout
        );
        assert!(
            run.stdout.contains("err-line"),
            "stderr lost: {}",
            run.stdout
        );
        assert_eq!(
            argv_lines(&fixture),
            vec!["logs", "--tail", "1000", "--", "web"],
            "the default tail is 1000 lines"
        );
    }

    #[test]
    fn logs_over_the_output_bound_are_refused_not_truncated() {
        let fixture = Fixture::new("logs-bound");
        let env = install(&fixture, "head -c 5242880 /dev/zero | tr '\\0' 'a'");
        let run = ae_child(
            &fixture,
            r#"(try { docker_logs("web") } catch e { e.error.code })"#,
            &env,
        );
        assert_eq!(
            run.stdout.trim().trim_matches('"'),
            "E_BUDGET_EXCEEDED",
            "a 5 MiB log was not refused: stderr {}",
            run.stderr
        );
    }

    #[test]
    fn a_nonzero_exit_is_a_coded_failure_carrying_the_stderr_tail() {
        let fixture = Fixture::new("fail-tail");
        let env = install(&fixture, "printf 'Error: no such container\\n' >&2; exit 1");
        let run = ae_child(&fixture, r#"docker_stop("web")"#, &env);
        assert!(
            !run.success,
            "a failed stop reported success: {}",
            run.stdout
        );
        assert!(
            run.stderr.contains("E_TOOL_FAILED") && run.stderr.contains("no such container"),
            "the failure was not coded with its stderr: {}",
            run.stderr
        );
    }
}
