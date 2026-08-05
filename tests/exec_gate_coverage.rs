//! Agent mode must gate the *capability* to run a command, not one builtin's name.
//!
//! Until 2026-08-04, `sh` was the only builtin that called `safety::guard` with
//! `Effect::Exec`. But `timeout`, `xargs`, `proc.spawn`, `nohup`, `strace`,
//! `ltrace` and the `perf` builtins all hand a caller-controlled string to a
//! shell. So in agent mode — with `sh` disabled outright, which is the intended
//! hardened configuration — an agent could still run any command it liked, with
//! no approval prompt and no `exec`-classified audit entry.
//!
//! This was demonstrated, not merely inferred: `timeout(5, "touch <marker>")`
//! returned exit code 0 and the marker file existed afterwards.
//!
//! These tests assert the property that matters — an ungated command does not
//! run — rather than any particular error text.

use aethershell::safety::{self, Effect};
use aethershell::value::Value;

// `AETHER_*` is process-global and tests run in parallel threads.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Enter agent mode with a fresh workspace, and return it.
fn agent_workspace(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ae_exec_gate_{}_{}", tag, std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    std::env::set_var("AETHER_WORKSPACE", &dir);
    std::env::set_var("AETHER_AUDIT_LOG", dir.join("audit.log"));
    std::env::set_var("AETHER_MODE", "agent");
    std::env::remove_var("AETHER_APPROVE");
    std::env::remove_var("AETHER_APPROVE_ALL");
    safety::governor_reset();
    dir
}

fn leave_agent_mode() {
    for k in [
        "AETHER_MODE",
        "AETHER_WORKSPACE",
        "AETHER_AUDIT_LOG",
        "AETHER_APPROVE",
        "AETHER_APPROVE_ALL",
    ] {
        std::env::remove_var(k);
    }
}

/// The observable proof: after an ungated call, the side effect must not exist.
#[test]
fn timeout_cannot_run_a_command_unapproved_in_agent_mode() {
    let _l = lock();
    let dir = agent_workspace("timeout");
    let marker = dir.join("PWNED.txt");
    let _ = std::fs::remove_file(&marker);

    let mut env = aethershell::env::Env::new();
    let res = aethershell::builtins::call(
        "timeout_cmd",
        vec![
            Value::Int(5),
            Value::Str(format!("touch {}", marker.display())),
        ],
        &mut env,
    );

    assert!(
        res.is_err(),
        "timeout ran a command in agent mode without approval: {res:?}"
    );
    assert!(
        !marker.exists(),
        "the command actually executed — the gate is decorative"
    );

    leave_agent_mode();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn proc_spawn_cannot_launch_a_program_unapproved_in_agent_mode() {
    let _l = lock();
    let dir = agent_workspace("spawn");
    let mut env = aethershell::env::Env::new();

    // A program that exists on both platforms, so a failure here is the gate
    // and not a missing binary.
    let program = if cfg!(windows) { "cmd" } else { "true" };
    let res = aethershell::builtins::call(
        "proc_spawn",
        vec![Value::Str(program.to_string())],
        &mut env,
    );
    assert!(
        res.is_err(),
        "proc.spawn launched a program in agent mode without approval: {res:?}"
    );

    leave_agent_mode();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Human mode is a REPL and must stay default-allow: the gate is about agents.
///
/// This asserts the call is not *refused by the gate*, not that it succeeds.
/// `timeout_cmd` shells out to GNU `timeout`, which macOS does not ship (it has
/// `gtimeout` via coreutils), so "the command ran" is a statement about the
/// runner's PATH rather than about the policy — and asserting it fails CI on
/// macOS for a reason that has nothing to do with what this test is for.
#[test]
fn human_mode_is_unaffected() {
    let _l = lock();
    leave_agent_mode();
    let dir = std::env::temp_dir().join(format!("ae_exec_gate_human_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let marker = dir.join("ok.txt");
    let _ = std::fs::remove_file(&marker);

    let mut env = aethershell::env::Env::new();
    let res = aethershell::builtins::call(
        "timeout_cmd",
        vec![
            Value::Int(5),
            Value::Str(format!("touch {}", marker.display())),
        ],
        &mut env,
    );

    if let Err(e) = &res {
        let rendered = e.to_string();
        for refusal in ["E_NEEDS_APPROVAL", "E_POLICY_DENY", "E_OUTSIDE_WORKSPACE"] {
            assert!(
                !rendered.contains(refusal),
                "human mode must not be gated, but got {refusal}: {rendered}"
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// The classification consumed by `agent_api`'s discovery must agree with the
/// gate. If these drift, an agent is told `timeout` is side-effect free.
#[test]
fn every_guarded_exec_builtin_is_classified_as_exec() {
    for name in [
        "sh",
        "timeout_cmd",
        "xargs_exec",
        "proc_spawn",
        "nohup_run",
        "strace_cmd",
        "ltrace_cmd",
        "perf_stat",
        "perf_record",
        "lxc_exec",
        "tmux_new",
        "tmux_send",
    ] {
        assert_eq!(
            safety::effect_of(name),
            Effect::Exec,
            "{name} runs a caller-supplied command but is not classified as exec"
        );
    }
}
