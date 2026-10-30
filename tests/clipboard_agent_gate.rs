//! Agent mode does not read the clipboard's contents unless the operator
//! permits clear secret reads (`IRONSHELL_SECRETS=allow`) or the policy is
//! permissive: a clipboard often holds a password that redaction cannot
//! recognise.
//!
//! Every check that runs on Windows and macOS is a refusal made before any
//! clipboard is asked. The checks that a permitted call goes past the gate run
//! on Linux only, with no display, so they end at E_NO_UI and read nothing.

use iron_shell::builtins::call_with_input;
use iron_shell::env::Env;
use iron_shell::value::Value;

const READERS: [&str; 3] = ["clipboard_get", "clipboard_get_image", "clipboard_history"];

/// The tests in this file change process-wide variables; one at a time.
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn env() -> Env {
    static ISOLATED: std::sync::Once = std::sync::Once::new();
    ISOLATED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_clipboard_gate_{}", std::process::id()));
        std::env::set_var("IRONSHELL_SESSION_DIR", dir);
    });
    Env::new()
}

/// Agent mode with no opt-in: the state every test starts from.
fn agent_mode() {
    std::env::set_var("IRONSHELL_MODE", "agent");
    std::env::remove_var("IRONSHELL_AGENT");
    std::env::remove_var("IRONSHELL_SECRETS");
    std::env::remove_var("IRONSHELL_POLICY");
}

fn refusal(name: &str) -> String {
    match call_with_input(name, vec![], None, &mut env()) {
        Ok(v) => panic!("{name} answered {v:?} where it must refuse"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn agent_mode_refuses_every_clipboard_read() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    agent_mode();
    for name in READERS {
        let msg = refusal(name);
        assert!(msg.contains("E_POLICY_DENY"), "{name}: {msg}");
        assert!(msg.contains("NOTHING WAS READ"), "{name}: {msg}");
        assert!(msg.contains("IRONSHELL_SECRETS=allow"), "{name}: {msg}");
    }
    std::env::remove_var("IRONSHELL_MODE");
}

#[test]
fn a_bad_call_is_still_a_bad_call_in_agent_mode() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    agent_mode();
    for name in READERS {
        let msg = match call_with_input(name, vec![Value::Int(1)], None, &mut env()) {
            Ok(v) => panic!("{name}(1) answered {v:?}"),
            Err(e) => e.to_string(),
        };
        assert!(msg.contains("E_BAD_ARG"), "{name}(1): {msg}");
    }
    std::env::remove_var("IRONSHELL_MODE");
}

/// With the gate open each reader goes on to the display check, which fails
/// here because no display is set: the call got past the policy and read
/// nothing.
#[cfg(target_os = "linux")]
#[test]
fn an_opt_in_or_human_mode_lets_the_read_through() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("DISPLAY");
    std::env::remove_var("WAYLAND_DISPLAY");
    let past_the_gate = |setup: &dyn Fn(), why: &str| {
        agent_mode();
        setup();
        for name in ["clipboard_get", "clipboard_get_image"] {
            let msg = refusal(name);
            assert!(!msg.contains("E_POLICY_DENY"), "{why}, {name}: {msg}");
            assert!(msg.contains("E_NO_UI"), "{why}, {name}: {msg}");
        }
    };
    past_the_gate(
        &|| std::env::set_var("IRONSHELL_SECRETS", "allow"),
        "IRONSHELL_SECRETS=allow",
    );
    past_the_gate(
        &|| std::env::set_var("IRONSHELL_POLICY", "permissive"),
        "IRONSHELL_POLICY=permissive",
    );
    past_the_gate(&|| std::env::remove_var("IRONSHELL_MODE"), "human mode");
    // Writing is not gated, and with no display it is E_NO_UI before xclip
    // runs: xclip with no display used to surface as E_TOOL_FAILED.
    let msg = match call_with_input(
        "clipboard_set",
        vec![Value::Str("x".into())],
        None,
        &mut env(),
    ) {
        Ok(v) => panic!("clipboard_set answered {v:?} with no display"),
        Err(e) => e.to_string(),
    };
    assert!(msg.contains("E_NO_UI"), "clipboard_set: {msg}");
    // Listing types reads no contents and is never gated.
    agent_mode();
    let msg = refusal("clipboard_types");
    assert!(!msg.contains("E_POLICY_DENY"), "clipboard_types: {msg}");
    std::env::remove_var("IRONSHELL_MODE");
    std::env::remove_var("IRONSHELL_SECRETS");
    std::env::remove_var("IRONSHELL_POLICY");
}
