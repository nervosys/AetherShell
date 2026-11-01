//! A program the shell starts does not inherit the shell's own `IRONSHELL_*`
//! settings. A child that prints its environment must not show one, and it must
//! still have the ordinary variables it needs, such as `PATH`.
//!
//! Each test starts a real child: `env` on Unix, `cmd /c set` on Windows. The
//! marker is set on the process that does the spawning, which is the shell.
//! Setting it on `ish` itself is different: `ish` is the parent there, and its
//! own settings are read normally.

use iron_shell::builtins::call;
use iron_shell::env::Env;
use iron_shell::os_tools::run_capped;
use iron_shell::safety::CHILD_ENV_KEEP;
use iron_shell::value::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// The shell's own setting that must not reach a child.
const MARKER: &str = "IRONSHELL_TEST_SECRET";
const MARKER_VALUE: &str = "b6-marker-value";

/// A variable the caller names on purpose, so it reaches the child.
const ALLOWED: &str = "IRONSHELL_TEST_KEPT";

/// `IRONSHELL_MODE` and the process environment are process-global, and these
/// tests change them.
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// The program that prints the child's environment, and its arguments.
#[cfg(unix)]
fn env_printer() -> (&'static str, Vec<String>) {
    ("env", Vec::new())
}

#[cfg(windows)]
fn env_printer() -> (&'static str, Vec<String>) {
    ("cmd", vec!["/c".to_string(), "set".to_string()])
}

/// The variable names in `NAME=value` output, one per line.
fn names_in(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.split_once('=').map(|(name, _)| name.to_string()))
        .collect()
}

fn has_name(names: &[String], wanted: &str) -> bool {
    names.iter().any(|n| n.eq_ignore_ascii_case(wanted))
}

/// The names only, for a failure message. The values can be credentials, so a
/// failure never prints them.
fn listed(names: &[String]) -> String {
    names.join(", ")
}

/// The environment a child prints, started by the shell's bounded runner.
fn bounded_env_of_child() -> String {
    let (program, args) = env_printer();
    let run = run_capped(
        "child_env",
        "env_printer",
        program,
        &args,
        64 * 1024,
        64 * 1024,
        60,
    )
    .expect("the env printer runs");
    assert!(
        run.success,
        "the env printer exits 0; stderr: {}",
        run.stderr
    );
    run.stdout
}

#[test]
fn a_builtin_child_does_not_inherit_an_ironshell_setting() {
    let _guard = lock();
    std::env::set_var(MARKER, MARKER_VALUE);
    let text = bounded_env_of_child();
    std::env::remove_var(MARKER);

    let names = names_in(&text);
    assert!(
        !has_name(&names, MARKER),
        "the child inherited {MARKER}; its variables were: {}",
        listed(&names)
    );
    assert!(
        has_name(&names, "PATH"),
        "an ordinary variable must still reach the child; its variables were: {}",
        listed(&names)
    );
}

#[test]
fn an_ish_script_does_not_pass_an_ironshell_setting_to_its_child() {
    let _guard = lock();
    let (program, args) = env_printer();
    let script = format!("{program} {}", args.join(" "));
    let output = Command::new(env!("CARGO_BIN_EXE_ish"))
        .args(["-b", "-c", &script])
        .env(MARKER, MARKER_VALUE)
        .output()
        .expect("ish runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();

    let names = names_in(&text);
    assert!(
        !has_name(&names, MARKER),
        "a child of ish inherited {MARKER}; its variables were: {}",
        listed(&names)
    );
    assert!(
        has_name(&names, "PATH"),
        "an ordinary variable must still reach the child; its variables were: {}",
        listed(&names)
    );
}

#[test]
fn a_variable_the_script_names_reaches_its_child() {
    let _guard = lock();
    let (program, args) = env_printer();
    let script = format!("{ALLOWED}=kept {program} {}", args.join(" "));
    let output = Command::new(env!("CARGO_BIN_EXE_ish"))
        .args(["-b", "-c", &script])
        .env(MARKER, MARKER_VALUE)
        .output()
        .expect("ish runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();

    let names = names_in(&text);
    assert!(
        has_name(&names, ALLOWED),
        "a variable the script sets for its child must reach it; its variables were: {}",
        listed(&names)
    );
    assert!(
        !has_name(&names, MARKER),
        "naming one variable must not open the others; its variables were: {}",
        listed(&names)
    );
}

#[test]
fn a_child_ish_started_in_agent_mode_stays_in_agent_mode() {
    let _guard = lock();
    std::env::set_var("IRONSHELL_MODE", "agent");
    // The child is this shell, asked for its own safety envelope. Without the
    // mode pin it would start in human mode, which is ungated.
    let run = run_capped(
        "child_env",
        "ish",
        env!("CARGO_BIN_EXE_ish"),
        &["-c".to_string(), "safety_status().mode".to_string()],
        64 * 1024,
        64 * 1024,
        60,
    );
    std::env::remove_var("IRONSHELL_MODE");
    let run = run.expect("the child ish runs");
    assert!(run.success, "the child ish exits 0; stderr: {}", run.stderr);
    assert!(
        run.stdout.contains("agent") && !run.stdout.contains("human"),
        "a child ish started by an agent-mode shell must stay in agent mode; got:\n{}",
        run.stdout
    );
}

#[test]
fn a_child_keeps_what_confines_a_shell_and_loses_credentials_and_opt_outs() {
    let _guard = lock();
    // A descendant `ish` reads these, so losing them would loosen it.
    std::env::set_var("IRONSHELL_WORKSPACE", "/confined");
    std::env::set_var("IRONSHELL_MAX_OPS", "7");
    std::env::set_var("IRONSHELL_API_KEY", "not-a-real-key");
    std::env::set_var("IRONSHELL_APPROVE", "all");
    std::env::set_var("IRONSHELL_REDACT", "off");
    let names = names_in(&bounded_env_of_child());
    for var in [
        "IRONSHELL_WORKSPACE",
        "IRONSHELL_MAX_OPS",
        "IRONSHELL_API_KEY",
        "IRONSHELL_APPROVE",
        "IRONSHELL_REDACT",
    ] {
        std::env::remove_var(var);
    }
    for kept in ["IRONSHELL_WORKSPACE", "IRONSHELL_MAX_OPS"] {
        assert!(has_name(&names, kept), "{kept} did not reach the child");
    }
    for gone in ["IRONSHELL_API_KEY", "IRONSHELL_APPROVE", "IRONSHELL_REDACT"] {
        assert!(!has_name(&names, gone), "{gone} reached the child");
    }
}

#[test]
fn any_child_of_an_agent_shell_carries_the_mode() {
    let _guard = lock();
    // The env printer is not `ish`, but it stands for any program that could
    // start one. Set through IRONSHELL_AGENT, so nothing inherits the mode name.
    std::env::set_var("IRONSHELL_AGENT", "1");
    let names = names_in(&bounded_env_of_child());
    std::env::remove_var("IRONSHELL_AGENT");
    assert!(
        has_name(&names, "IRONSHELL_MODE"),
        "an agent shell's child lost the mode: {}",
        listed(&names)
    );
}

/// A child argv that writes the environment to `file`. The redirect needs a
/// shell, so the shell is the program.
fn env_into_file(file: &Path) -> Vec<String> {
    let text = file.display().to_string();
    if cfg!(windows) {
        vec!["cmd".into(), "/c".into(), format!("set > {text}")]
    } else {
        vec!["sh".into(), "-c".into(), format!("env > {text}")]
    }
}

#[test]
fn a_child_started_by_proc_spawn_does_not_inherit_an_ironshell_setting() {
    let _guard = lock();
    let file = std::env::temp_dir().join(format!("child_env_spawn_{}.txt", std::process::id()));
    let argv = env_into_file(&file);
    let mut env = Env::new();
    std::env::set_var(MARKER, MARKER_VALUE);
    let spawned = call(
        "proc_spawn",
        argv.iter().map(|a| Value::Str(a.clone())).collect(),
        &mut env,
    );
    std::env::remove_var(MARKER);
    let pid = match spawned.expect("proc_spawn runs") {
        Value::Int(pid) => pid,
        other => panic!("proc_spawn answers an Int pid, got {other:?}"),
    };
    call(
        "proc_wait",
        vec![Value::Int(pid), Value::Int(60_000)],
        &mut env,
    )
    .expect("proc_wait reaps the child");
    let text = std::fs::read_to_string(&file).expect("the child wrote its environment");
    let _ = std::fs::remove_file(&file);

    let names = names_in(&text);
    assert!(
        !has_name(&names, MARKER),
        "a child started by proc_spawn inherited {MARKER}; its variables were: {}",
        listed(&names)
    );
    assert!(
        has_name(&names, "PATH"),
        "an ordinary variable must still reach a proc_spawn child; its variables were: {}",
        listed(&names)
    );
}

/// The `stdout` of the first run in a `test_watch` result.
fn first_run_stdout(result: &Value) -> String {
    let Value::Record(map) = result else {
        panic!("test_watch answers a Record, got {result:?}");
    };
    let Some(Value::Array(runs)) = map.get("runs") else {
        panic!("test_watch answers runs as an Array, got {result:?}");
    };
    let Some(Value::Record(run)) = runs.first() else {
        panic!("test_watch answers no run");
    };
    match run.get("stdout") {
        Some(Value::Str(text)) => text.clone(),
        other => panic!("a run's stdout is not a String: {other:?}"),
    }
}

#[test]
fn a_test_watch_child_does_not_inherit_an_ironshell_setting() {
    let _guard = lock();
    let (program, args) = env_printer();
    let mut argv = vec![Value::Str(program.to_string())];
    argv.extend(args.into_iter().map(Value::Str));
    let paths = Value::Array(vec![Value::Str(format!(
        "{}/Cargo.toml",
        env!("CARGO_MANIFEST_DIR")
    ))]);
    let mut options = BTreeMap::new();
    options.insert("max_runs".to_string(), Value::Int(1));

    std::env::set_var(MARKER, MARKER_VALUE);
    let result = call(
        "test_watch",
        vec![Value::Array(argv), paths, Value::Record(options)],
        &mut Env::new(),
    );
    std::env::remove_var(MARKER);
    let text = first_run_stdout(&result.expect("test_watch runs"));

    let names = names_in(&text);
    assert!(
        !has_name(&names, MARKER),
        "a test_watch child inherited {MARKER}; its variables were: {}",
        listed(&names)
    );
    assert!(
        has_name(&names, "PATH"),
        "an ordinary variable must still reach a test_watch child; its variables were: {}",
        listed(&names)
    );
}

/// Fake credentials the shell's process holds. Named as real ones are, so the
/// credential rule applies to them. Only these fake values are ever set.
const FAKE_SECRET: &str = "CHILD_ENV_TEST_API_KEY";
const FAKE_TOKEN: &str = "CHILD_ENV_TEST_TOKEN";
const FAKE_WELL_KNOWN: &str = "GH_TOKEN";
const FAKE_VALUE: &str = "fake-not-a-credential";

/// The environment a child prints with the mode and keep list as given, and
/// the fake credentials set on the shell. Returns the names the child saw.
fn child_names_with(agent: bool, keep: Option<&str>) -> Vec<String> {
    if agent {
        std::env::set_var("IRONSHELL_MODE", "agent");
    } else {
        std::env::remove_var("IRONSHELL_MODE");
        std::env::remove_var("IRONSHELL_AGENT");
    }
    match keep {
        Some(names) => std::env::set_var(CHILD_ENV_KEEP, names),
        None => std::env::remove_var(CHILD_ENV_KEEP),
    }
    for name in [FAKE_SECRET, FAKE_TOKEN, FAKE_WELL_KNOWN] {
        std::env::set_var(name, FAKE_VALUE);
    }
    let names = names_in(&bounded_env_of_child());
    for name in [FAKE_SECRET, FAKE_TOKEN, FAKE_WELL_KNOWN, CHILD_ENV_KEEP] {
        std::env::remove_var(name);
    }
    if agent {
        std::env::remove_var("IRONSHELL_MODE");
    }
    names
}

#[test]
fn an_agent_shell_child_does_not_inherit_a_credential() {
    let _guard = lock();
    let names = child_names_with(true, None);
    for name in [FAKE_SECRET, FAKE_TOKEN, FAKE_WELL_KNOWN] {
        assert!(
            !has_name(&names, name),
            "an agent-mode child inherited the credential {name}; its variables were: {}",
            listed(&names)
        );
    }
    assert!(
        has_name(&names, "PATH"),
        "an ordinary variable must still reach an agent-mode child"
    );
}

#[test]
fn an_agent_shell_child_keeps_the_credentials_its_keep_list_names() {
    let _guard = lock();
    let keep = format!(" {FAKE_TOKEN} ,, {FAKE_WELL_KNOWN}");
    let names = child_names_with(true, Some(keep.as_str()));
    for kept in [FAKE_TOKEN, FAKE_WELL_KNOWN] {
        assert!(
            has_name(&names, kept),
            "{kept} is in {CHILD_ENV_KEEP} but did not reach the child; its variables were: {}",
            listed(&names)
        );
    }
    assert!(
        !has_name(&names, FAKE_SECRET),
        "naming some credentials must not open the others; its variables were: {}",
        listed(&names)
    );
}

#[test]
fn a_human_shell_child_inherits_a_credential_unchanged() {
    let _guard = lock();
    let names = child_names_with(false, None);
    for name in [FAKE_SECRET, FAKE_TOKEN, FAKE_WELL_KNOWN] {
        assert!(
            has_name(&names, name),
            "a human-mode child lost {name}, which human mode leaves alone; its variables were: {}",
            listed(&names)
        );
    }
}

#[test]
fn the_named_well_known_credentials_are_secret_names() {
    // The credential rule reuses the redaction list. These are the names it
    // must cover, so a change to that list cannot quietly let one through.
    for name in [
        "GITHUB_TOKEN",
        "GH_TOKEN",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "CHILD_ENV_TEST_API_KEY",
        "DB_PASSWORD",
        "DB_PASSWD",
        "APP_SECRET_KEY",
        "GOOGLE_APPLICATION_CREDENTIALS",
    ] {
        assert!(
            iron_shell::safety::is_secret_name(name),
            "{name} must be a secret name"
        );
    }
}

/// Files that start a program outside a runner that strips the shell's
/// variables, with how many such starts each may keep. A count only goes
/// down: a new unguarded start fails, and so does a count that has dropped
/// without its allowance being lowered.
const UNGUARDED_SPAWNS: &[(&str, usize)] = &[
    // Command builders whose caller runs them (zip_command, git_command), and
    // spawns in #[test] fixtures.
    ("builtins.rs", 11),
];

/// Calls that run a child with the shell's variables stripped, directly or
/// through another such runner.
const RUNNERS: &[&str] = &[
    "strip_ironshell_env(",
    "run_tree_cmd(",
    "run_tree_cmd_observed(",
    "run_tree(",
    "run_output(",
    "run_checked(",
    "run_tool(",
    "bounded_cmd_output(",
    "output_with_timeout(",
    "run_capped(",
    "proc_tool(",
    "probe_text(",
    "probe_text_if_installed(",
    "hypervisor_probe_ok(",
];

/// How many lines after a `Command::new(` may hold its runner call. A
/// function that starts a new function ends the search.
const SPAWN_WINDOW: usize = 60;

/// How many lines before a `Command::new(` may hold a runner call, for a
/// command built inside a block that is handed to a runner.
const LOOKBACK: usize = 6;

fn calls_runner(line: &str) -> bool {
    RUNNERS.iter().any(|r| line.contains(r))
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("src is readable") {
        let path = entry.expect("a src entry").path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn starts_a_function(line: &str) -> bool {
    let t = line.trim_start();
    [
        "fn ",
        "pub fn ",
        "pub(crate) fn ",
        "pub(super) fn ",
        "async fn ",
    ]
    .iter()
    .any(|p| t.starts_with(p))
}

/// The `Command::new(` lines in `text` with no runner call in their function,
/// as (line number, trimmed line).
fn unguarded_spawns_in(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with("//") || !t.contains("Command::new(") {
            continue;
        }
        let guarded = lines[i.saturating_sub(LOOKBACK)..i]
            .iter()
            .any(|l| calls_runner(l))
            || lines[i..]
                .iter()
                .take(SPAWN_WINDOW)
                .enumerate()
                .take_while(|(k, l)| *k == 0 || !starts_a_function(l))
                .any(|(_, l)| calls_runner(l));
        if !guarded {
            found.push((i + 1, t.to_string()));
        }
    }
    found
}

#[test]
fn every_spawn_in_src_strips_the_shell_or_is_on_the_ratchet() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    let mut problems = Vec::new();
    for file in files {
        let rel = file
            .strip_prefix(&root)
            .expect("a file under src")
            .to_string_lossy()
            .replace('\\', "/");
        // Outside this rule by decision: the aimodel binary is not part of ish.
        if rel == "bin/aimodel.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("a src file reads");
        let found = unguarded_spawns_in(&text);
        let allowed = UNGUARDED_SPAWNS
            .iter()
            .find(|(f, _)| *f == rel)
            .map_or(0, |(_, n)| *n);
        let lines: Vec<String> = found.iter().map(|(n, _)| n.to_string()).collect();
        if found.len() > allowed {
            problems.push(format!(
                "src/{rel}: {} unguarded spawns, allowed {allowed}, at lines {}",
                found.len(),
                lines.join(", ")
            ));
        } else if found.len() < allowed {
            problems.push(format!(
                "src/{rel}: {} unguarded spawns, allowance is {allowed}; lower it",
                found.len()
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "unguarded child spawns in src:\n{}",
        problems.join("\n")
    );
}
