//! A program the shell starts does not inherit the shell's own `IRONSHELL_*`
//! settings. A child that prints its environment must not show one, and it must
//! still have the ordinary variables it needs, such as `PATH`.
//!
//! Each test starts a real child: `env` on Unix, `cmd /c set` on Windows. The
//! marker is set on the process that does the spawning, which is the shell.
//! Setting it on `ish` itself is different: `ish` is the parent there, and its
//! own settings are read normally.

use iron_shell::os_tools::run_capped;
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
