//! AetherShell's edges into the NERVOSYS stack.
//!
//! Two kinds of test live here, and the split is deliberate:
//!
//! * **Wiring** — that `irongate:` parses, that the gateway is preferred, that
//!   the URL derivation is right. Pure, always runs.
//! * **Live** — that a running IronGate actually answers. Skipped when the
//!   stack is absent, and *loud* about skipping: a suite that silently passes
//!   because it tested nothing is the failure mode this project keeps hitting.

use aethershell::ai::{parse_model_ref, Provider};
use aethershell::model_plane;
use aethershell::providers::{ProviderConfig, ProviderType};
use std::sync::{Mutex, MutexGuard};

/// Serialises every test that reads or writes `IRONGATE_URL` / `IRONVAULT_BIN`.
///
/// Environment is process-global and `cargo test` runs in threads, so a test
/// that sets one of these leaks into whatever is running beside it. That is not
/// theoretical here: before this lock existed, the run printed
/// `SKIP: IronVault CLI 'definitely-not-a-real-binary-aethershell' not found`
/// from the *other* vault test — it had picked up the deliberately-bogus value
/// set by the error-message test. The suite stayed green while one test
/// silently stopped testing anything.
///
/// A lock in the file rather than `--test-threads=1`: the flag has to be
/// remembered at every call site, and forgetting it fails silently.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn env_guard() -> MutexGuard<'static, ()> {
    // A poisoned lock means another test panicked mid-mutation. The env may be
    // dirty, but failing every subsequent test on that is less useful than
    // continuing — the panic itself is already reported.
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

// ── Wiring: the parts that hold with nothing running ────────────────────────

#[test]
fn the_gateway_is_addressable_by_uri() {
    let m = parse_model_ref("irongate:auto");
    assert_eq!(m.provider, Provider::IronGate);
    assert_eq!(m.model, "auto");
}

#[test]
fn the_gateway_has_short_aliases() {
    // `gate` and `iron` because `irongate:` is a lot to type in a shell, and
    // the scheme is the thing a user retypes most.
    for alias in ["irongate", "gate", "iron"] {
        let m = parse_model_ref(&format!("{alias}:fast"));
        assert_eq!(
            m.provider,
            Provider::IronGate,
            "alias `{alias}` did not resolve to the gateway"
        );
    }
}

#[test]
fn a_bare_gateway_name_takes_the_default_virtual_model() {
    // `AETHER_AI=irongate` with no model must mean "let the gateway decide",
    // not "a model literally called irongate".
    let m = parse_model_ref("irongate");
    assert_eq!(m.provider, Provider::IronGate);
    assert!(
        !m.model.is_empty(),
        "a bare gateway ref must still name a virtual model"
    );
}

#[test]
fn the_provider_type_round_trips_through_its_scheme() {
    let pt = ProviderType::from_scheme("irongate").expect("scheme should resolve");
    assert_eq!(pt, ProviderType::IronGate);
    assert_eq!(pt.scheme(), "irongate");
}

#[test]
fn the_gateway_is_openai_compatible_and_carries_tools() {
    // It re-encodes for whatever it routes to, so a caller must be free to send
    // tools through it. Claiming otherwise would make the shell strip them.
    assert!(ProviderType::IronGate.is_openai_compatible());
    assert!(ProviderType::IronGate.supports_tools());
    assert!(ProviderType::IronGate.supports_streaming());
}

#[test]
fn the_gateway_appears_in_the_full_provider_list() {
    // `ProviderType::all()` drives `ai_providers()`; a provider missing from it
    // is invisible to any agent enumerating what the shell can reach.
    assert!(ProviderType::all().contains(&ProviderType::IronGate));
}

#[test]
fn the_default_endpoint_matches_the_gateways_own_default_port() {
    // 7700 is `[server] port` in irongate.example.toml. If that ever moves,
    // this is the test that should fail rather than a user's first request.
    assert_eq!(
        ProviderType::IronGate.default_base_url(),
        "http://localhost:7700/v1"
    );
}

#[test]
fn the_health_root_is_derived_from_the_configured_v1_url() {
    let _guard = env_guard();
    // /health and /v1/chat/completions are served by one process. Deriving one
    // from the other is what stops them pointing at different hosts.
    let root = model_plane::gate_root();
    let url = model_plane::gate_url();
    assert!(
        url.starts_with(&root),
        "root {root} is not a prefix of {url}"
    );
    assert!(!root.ends_with("/v1"), "root still carries the /v1 suffix");
    assert!(!root.ends_with('/'), "root has a trailing slash: {root}");
}

#[test]
fn the_endpoint_is_overridable_without_touching_the_code() {
    // An operator running the gateway on another host must not have to rebuild.
    // Checked through ProviderConfig because that is the path the provider
    // layer actually takes.
    let _guard = env_guard();
    let key = "IRONGATE_URL";
    let restore = std::env::var(key).ok();
    std::env::set_var(key, "http://gate.internal:9999/v1");

    let cfg = ProviderConfig::from_env(ProviderType::IronGate);
    let effective = cfg.effective_base_url();

    match restore {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }

    assert_eq!(effective, "http://gate.internal:9999/v1");
}

#[test]
fn the_new_builtins_declare_the_effects_they_actually_have() {
    // The house failure mode is a builtin that acts while `effect_of` reports
    // Pure, which tells every policy check it is safe. Assert directly rather
    // than trusting the ratchet's silence: a lint that goes blind reports zero.
    use aethershell::safety::{effect_of, Effect};

    assert_eq!(
        effect_of("ai_gateway"),
        Effect::Network,
        "ai_gateway makes an HTTP request"
    );
    for name in [
        "vault_models",
        "vault_conversions",
        "vault_convert",
        "ai_convert_model",
    ] {
        assert_eq!(
            effect_of(name),
            Effect::Exec,
            "{name} spawns the `iv` binary"
        );
    }
}

#[test]
fn conversion_no_longer_reports_success_for_a_file_copy() {
    // The old path-based form was backed by a converter whose every branch was
    // `fs::copy` and which returned `success: true` with a checksum of the
    // unchanged bytes. It must refuse rather than lie, and the refusal must
    // name where conversion moved to.
    use aethershell::value::Value;
    use std::collections::BTreeMap;

    let mut cfg = BTreeMap::new();
    cfg.insert("source".to_string(), Value::Str("model.pt".into()));
    cfg.insert("target".to_string(), Value::Str("model.safetensors".into()));
    cfg.insert("source_format".to_string(), Value::Str("pytorch".into()));
    cfg.insert(
        "target_format".to_string(),
        Value::Str("safetensors".into()),
    );

    let mut env = aethershell::env::Env::new();
    let err = aethershell::builtins::call("ai_convert_model", vec![Value::Record(cfg)], &mut env)
        .expect_err("the path form must be refused, not silently copied");

    let text = err.to_string();
    assert!(
        text.contains("IronVault") || text.contains("iv add"),
        "the refusal must point at where conversion actually happens, got: {text}"
    );
}

// ── Live: only meaningful with the stack running ────────────────────────────

/// Skip helper that says so out loud. A skipped test and a passing test look
/// identical in a summary line; printing the reason is the only thing that
/// makes the difference visible.
fn require_gateway() -> Option<MutexGuard<'static, ()>> {
    let guard = env_guard();
    if model_plane::gate_available() {
        // Handed back rather than dropped: the caller reads the same
        // environment again, and releasing here would reopen the race the lock
        // exists to close.
        return Some(guard);
    }
    eprintln!(
        "SKIP: IronGate not reachable at {} — start it to exercise this test",
        model_plane::gate_url()
    );
    None
}

fn require_vault() -> Option<MutexGuard<'static, ()>> {
    let guard = env_guard();
    if model_plane::vault_available() {
        return Some(guard);
    }
    eprintln!(
        "SKIP: IronVault CLI `{}` not found — `cargo install ironvault` to exercise this test",
        model_plane::vault_bin()
    );
    None
}

#[test]
fn a_running_gateway_reports_what_it_will_route() {
    let _guard = match require_gateway() {
        Some(g) => g,
        None => return,
    };
    let health = model_plane::gate_health().expect("gateway answered /health");
    // A gateway with zero providers registered is up but useless; that is worth
    // knowing rather than asserting away.
    eprintln!(
        "IronGate: {} provider(s), virtual models {:?}",
        health.providers, health.models
    );
}

#[test]
fn a_running_gateway_completes_a_prompt_and_says_where_it_went() {
    let _guard = match require_gateway() {
        Some(g) => g,
        None => return,
    };
    let done = match model_plane::gate_complete(
        &model_plane::gate_model(),
        "Reply with the word OK.",
        Some(16),
    ) {
        Ok(d) => d,
        Err(e) => {
            // Reachable but unable to serve — no route configured, every target
            // unhealthy, budget exhausted. Report it; do not fail the suite for
            // an operator's configuration.
            eprintln!("SKIP: gateway reachable but did not serve: {e}");
            return;
        }
    };
    assert!(!done.text.is_empty(), "gateway returned empty content");
    eprintln!(
        "routed to {:?} (difficulty {:?}) in {:.0}ms",
        done.target, done.difficulty, done.elapsed_ms
    );
}

#[test]
fn an_installed_vault_lists_its_conversion_paths() {
    let _guard = match require_vault() {
        Some(g) => g,
        None => return,
    };
    match model_plane::vault_conversions() {
        Ok(v) => eprintln!("IronVault conversions: {v}"),
        Err(e) => eprintln!("SKIP: vault present but list-conversions failed: {e}"),
    }
}

#[test]
fn the_vault_error_names_the_fix_when_the_cli_is_absent() {
    // The two failures a user actually hits are "not installed" and "no
    // passphrase". Neither is guessable from a bare non-zero exit, so the error
    // has to carry the remedy.
    let _guard = env_guard();
    let key = "IRONVAULT_BIN";
    let restore = std::env::var(key).ok();
    std::env::set_var(key, "definitely-not-a-real-binary-aethershell");

    let err = model_plane::vault_list().expect_err("a missing binary must fail");

    match restore {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }

    let text = err.to_string();
    assert!(
        text.contains("ironvault") || text.contains("IRONVAULT_BIN"),
        "error should name the fix, got: {text}"
    );
}
