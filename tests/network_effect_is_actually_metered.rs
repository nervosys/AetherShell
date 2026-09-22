//! A builtin that reaches the network must be charged for it.
//!
//! `benches/agentic/uncoded.mjs` flagged `marketplace_search` as never
//! returning. It was not hanging: `packages::RegistryClient` has a 15-second
//! HTTP timeout and the probe killed it at 10. Chasing that turned up
//! something worse than a hang.
//!
//! Under `--agent --policy strict` with `AETHER_MAX_NET=0`,
//! `marketplace_search("x")` exited **0** after a 20-second round trip to
//! packages.nervosys.ai. An agent with networking budgeted to zero could still
//! make an outbound request.
//!
//! Three things had to be true for that, and fixing only the first would have
//! looked like a fix:
//!
//! 1. `marketplace_search`, `_info` and `_update` were `Effect::Pure` by
//!    fall-through, while `marketplace_publish` -- the same family, the same
//!    client -- was correctly `Network`. That asymmetry is what marks it as an
//!    oversight rather than a decision.
//! 2. Classifying them `Network` did not gate them. `centrally_enforced`
//!    covers `Process | Destructive | Exec | Privileged`; `Network` is not in
//!    it, so `guard_dispatch` charges nothing for a network builtin.
//! 3. A network builtin is metered only where it calls `guard_network` itself,
//!    as `http_get` does.
//!
//! `effect_ratchet.rs` could not have caught this: it reads builtin *bodies*
//! for socket construction, and this socket opens inside `RegistryClient`.
//! That is the lower bound its own documentation names, with a real instance
//! attached.

use aethershell::safety::{effect_is_declared, effect_of, Effect};

/// The marketplace builtins that reach `packages.nervosys.ai`.
const REACHES_REGISTRY: &[&str] = &[
    "marketplace_search",
    "marketplace_info",
    "marketplace_update",
    "marketplace_publish",
];

#[test]
fn every_builtin_that_reaches_the_registry_is_classified_network() {
    for name in REACHES_REGISTRY {
        assert_eq!(
            effect_of(name),
            Effect::Network,
            "{name} calls RegistryClient over HTTPS; classifying it otherwise \
             means the governor never charges it"
        );
        // Declared, not defaulted. `effect_of` returns `Pure` for anything
        // nobody classified, so without this the assertion above could pass on
        // a builtin that had simply fallen through to a different default.
        assert!(
            effect_is_declared(name),
            "{name}'s effect must be stated, not inherited from a fall-through"
        );
    }
}

#[test]
fn every_network_builtin_is_metered_not_just_the_ones_that_opted_in() {
    // `Network` used not to be centrally enforced, so `AETHER_MAX_NET`
    // governed only the builtins that happened to call `guard_network`
    // themselves. Of 57 classified `Network`, 53 were never charged.
    //
    // The hole was invisible because the builtin everyone reaches for was the
    // one that worked: `http_get` self-guards. `scp_upload` did not -- under
    // `--agent --policy strict` with `AETHER_MAX_NET=0` it invoked `scp`, and
    // failed only because the local file was absent.
    //
    // These are builtins that do *not* call `guard_network`, so they are
    // metered only by the central path. If that regresses, they stop being
    // charged and nothing else here would notice.
    const NOT_SELF_GUARDED: &[&str] = &["scp_upload", "git_fetch", "host_lookup", "k8s_pods"];

    let jail = std::env::temp_dir().join(format!("ae_netall_{}", std::process::id()));
    std::fs::create_dir_all(&jail).expect("jail");
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_POLICY", "strict");
    std::env::set_var("AETHER_WORKSPACE", &jail);
    std::env::set_var("AETHER_MAX_NET", "0");

    for name in NOT_SELF_GUARDED {
        assert_eq!(
            effect_of(name),
            Effect::Network,
            "{name} must be classified Network for the central path to charge it"
        );
        let mut env = aethershell::env::Env::new();
        let out = aethershell::builtins::call(name, vec![], &mut env);
        let msg = match out {
            Ok(v) => panic!("{name} ran with a zero network budget and answered {v:?}"),
            Err(e) => e.to_string(),
        };
        assert!(
            msg.contains("E_BUDGET_EXCEEDED"),
            "{name} was not charged against the network budget: {msg}"
        );
    }

    let _ = std::fs::remove_dir_all(&jail);
}

#[test]
fn a_zero_network_budget_refuses_the_registry_call() {
    // The end-to-end property. Runs in-process, so the env is this test's own.
    let jail = std::env::temp_dir().join(format!("ae_net_{}", std::process::id()));
    std::fs::create_dir_all(&jail).expect("jail");
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_POLICY", "strict");
    std::env::set_var("AETHER_WORKSPACE", &jail);
    std::env::set_var("AETHER_MAX_NET", "0");

    let mut env = aethershell::env::Env::new();
    let started = std::time::Instant::now();
    let out = aethershell::builtins::call(
        "marketplace_search",
        vec![aethershell::value::Value::Str("x".into())],
        &mut env,
    );
    let elapsed = started.elapsed();

    match out {
        Ok(v) => panic!(
            "the registry call was allowed with a zero network budget and \
             answered {v:?}"
        ),
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains("E_BUDGET_EXCEEDED"), "wrong refusal: {msg}");
        }
    }
    // It must be refused *before* the request, not after a 15s timeout.
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "refused only after {elapsed:?}; the request was made first"
    );

    // Non-vacuity: the budget must not refuse everything. A pure builtin in
    // the same process, same env, still works.
    let mut env2 = aethershell::env::Env::new();
    assert!(
        aethershell::builtins::call(
            "upper",
            vec![aethershell::value::Value::Str("ok".into())],
            &mut env2
        )
        .is_ok(),
        "the zero network budget is refusing non-network builtins too"
    );

    let _ = std::fs::remove_dir_all(&jail);
}
