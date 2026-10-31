//! One spelling of "agent" and "on" across the safety gate.
//!
//! `IRONSHELL_MODE` used to be compared exactly by `safety::current_mode` and
//! case-insensitively by the plugin loader, so `IRONSHELL_MODE=Agent` was human
//! mode for the effect gate and agent mode for plugins. Both now go through
//! `safety::current_mode`, and the switches go through `safety::truthy_env`,
//! which trim and ignore case.

use iron_shell::safety::{current_mode, truthy_env, Mode};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

const VARS: &[&str] = &["IRONSHELL_MODE", "IRONSHELL_AGENT", "IRONSHELL_POLICY"];

/// Holds the env lock, clears the mode variables, and restores the caller's
/// values on drop, so a failing assertion cannot leak state into other tests.
struct Env {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Env {
    fn new() -> Self {
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = VARS
            .iter()
            .map(|&name| (name, std::env::var_os(name)))
            .collect();
        for name in VARS {
            std::env::remove_var(name);
        }
        Self {
            saved,
            _guard: guard,
        }
    }

    fn set(&self, name: &str, value: &str) {
        std::env::set_var(name, value);
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }
    }
}

#[test]
fn mode_agent_spellings_are_agent() {
    let env = Env::new();
    for value in ["agent", "Agent", "AGENT", " AGENT ", "\tagent\n"] {
        env.set("IRONSHELL_MODE", value);
        assert_eq!(current_mode(), Mode::Agent, "IRONSHELL_MODE={value:?}");
    }
}

#[test]
fn mode_other_values_are_human() {
    let env = Env::new();
    assert_eq!(current_mode(), Mode::Human, "unset");
    for value in ["human", "", "  ", "agents", "agent-ish", "0"] {
        env.set("IRONSHELL_MODE", value);
        assert_eq!(current_mode(), Mode::Human, "IRONSHELL_MODE={value:?}");
    }
}

#[test]
fn agent_switch_accepts_case_and_whitespace() {
    let env = Env::new();
    for value in ["1", "true", "TRUE", " yes ", "On", "\tON\n"] {
        env.set("IRONSHELL_AGENT", value);
        assert!(truthy_env("IRONSHELL_AGENT"), "IRONSHELL_AGENT={value:?}");
        assert_eq!(current_mode(), Mode::Agent, "IRONSHELL_AGENT={value:?}");
    }
}

#[test]
fn agent_switch_rejects_other_values() {
    let env = Env::new();
    for value in ["", "0", "false", "no", "off", "y", "truey"] {
        env.set("IRONSHELL_AGENT", value);
        assert!(!truthy_env("IRONSHELL_AGENT"), "IRONSHELL_AGENT={value:?}");
        assert_eq!(current_mode(), Mode::Human, "IRONSHELL_AGENT={value:?}");
    }
}
