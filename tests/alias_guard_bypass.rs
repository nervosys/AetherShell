//! A guarded builtin's other spelling was not guarded.
//!
//! `safety::guard_dispatch` takes its decision from `effect_of(builtin)` — the
//! name as typed. Dispatch, though, is by *implementation*: `BUILTIN_LOOKUP`
//! aliases share a dispatch index, and a fallback arm serves every literal on
//! its left. So `sh` and `shell` run the same code, and `vault_convert` and
//! `vault-convert` run the same code, while only one spelling of each was
//! classified.
//!
//! The unclassified spelling reads as `Pure`. `centrally_enforced(Pure)` is
//! false, so `guard_dispatch` returns `Ok` before any policy runs — and because
//! the audit line is only written for `WriteLocal`/`Network`, the call leaves no
//! trace either. An agent refused `sh` needed only to type `shell`.
//!
//! `tests/effect_alias_agreement.rs` holds the whole surface — 104 alias groups
//! disagreed when it was written. This file is the demonstration: the same
//! sequence an agent would run.

use aethershell::env::Env;
use aethershell::safety::{effect_of, Effect};
use aethershell::value::Value;
use std::sync::Mutex;

static ENV: Mutex<()> = Mutex::new(());

fn call(name: &str, args: Vec<&str>) -> Result<Value, String> {
    let mut env = Env::new();
    let args = args
        .into_iter()
        .map(|a| Value::Str(a.to_string()))
        .collect();
    aethershell::builtins::call(name, args, &mut env).map_err(|e| e.to_string())
}

fn agent_mode() {
    let ws = std::env::temp_dir().join("ae_alias_bypass");
    std::fs::create_dir_all(&ws).unwrap();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &ws);
    std::env::remove_var("AETHER_POLICY");
    aethershell::safety::set_principal(None);
}

#[test]
fn the_other_spelling_of_a_shell_passthrough_is_guarded_too() {
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    agent_mode();

    // The guarded spelling. If this ever stops being refused the test below
    // proves nothing, so assert it rather than assume it.
    let refused =
        call("sh", vec!["echo", "hi"]).expect_err("`sh` is Exec and must be refused in agent mode");

    // The same dispatch index, one letter longer.
    let via_alias = call("shell", vec!["echo", "hi"]);
    assert!(
        via_alias.is_err(),
        "`shell` reached the same implementation as `sh` without meeting the \
         gate that refused it ({refused}). effect_of(\"sh\")={} but \
         effect_of(\"shell\")={}",
        effect_of("sh").as_str(),
        effect_of("shell").as_str()
    );
}

#[test]
fn a_hyphenated_alias_carries_its_twins_classification() {
    // The fallback half of the dispatcher, where `"vault-convert" |
    // "vault_convert" => bi_vault_convert(..)` is literally one arm.
    for (classified, alias) in [
        ("vault_convert", "vault-convert"),
        ("vault_models", "vault-models"),
        ("vault_conversions", "vault-conversions"),
        ("ai_convert_model", "ai-convert-model"),
        ("ai_gateway", "ai-gateway"),
        ("ai_gateway", "irongate"),
    ] {
        assert_ne!(
            effect_of(classified),
            Effect::Pure,
            "{classified} is the classified spelling; this test is pointless if it is Pure"
        );
        assert_eq!(
            effect_of(alias),
            effect_of(classified),
            "`{alias}` and `{classified}` are the same match arm, so a policy \
             decision that depends on the name is a policy decision an agent picks"
        );
    }
}

#[test]
fn an_underscore_alias_carries_it_too() {
    // The fast half: same dispatch index, different spellings.
    for (classified, alias) in [
        ("sh", "shell"),
        ("nc_connect", "nc"),
        ("nc_connect", "netcat"),
        ("net_ping", "ping"),
        ("fs_du", "du"),
        ("pytest_run", "pytest"),
        ("gdb_run", "gdb"),
    ] {
        assert_ne!(
            effect_of(classified),
            Effect::Pure,
            "{classified} is the classified spelling; this test is pointless if it is Pure"
        );
        assert_eq!(
            effect_of(alias),
            effect_of(classified),
            "`{alias}` and `{classified}` share a dispatch index"
        );
    }
}

#[test]
fn the_alias_that_ran_a_debugger_is_refused_now() {
    // The measurement that made this a bug rather than an untidiness. In agent
    // mode `lldb_run` was refused for requiring approval, and `lldb` — the same
    // dispatch index — ran the debugger to completion and returned its output.
    //
    // The assertion is on the *shape* of the refusal, not on the tool being
    // absent: with the gate restored the call is refused before anything is
    // spawned, so this holds on a machine with lldb installed and on one
    // without, which is the difference between a test and a coincidence.
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    agent_mode();

    for name in ["lldb_run", "lldb"] {
        let err = match call(name, vec!["nonexistent_target_xyz"]) {
            Err(e) => e,
            Ok(v) => panic!(
                "`{name}` reaches an external debugger and must meet the gate, but it ran: {v:?}"
            ),
        };
        assert!(
            err.contains("approval"),
            "`{name}` failed, but not by being gated — it got as far as running \
             something and failed there instead:\n{err}"
        );
    }
}
