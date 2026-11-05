//! A CLI flag may add an authentication requirement; it may not silently drop one.
//!
//! `aimodel` folded four CLI overrides into the loaded config by assignment:
//!
//! ```text
//! config.server.host             = args.host.clone();
//! config.server.port             = args.port;
//! config.server.enable_cors      = args.cors;
//! config.security.require_api_key = args.require_api_key;   // <- fails open
//! ```
//!
//! The first three are fail-safe in that position. A configured value replaced by
//! a CLI default lands on `127.0.0.1`, the original port, and CORS off — all
//! *more* restrictive than what the config asked for. The fourth is the one where
//! the CLI default is the dangerous direction: `require_api_key` is a bare
//! `#[arg(long)] bool`, so it is `false` whenever the flag is absent.
//!
//! So a user who wrote `require_api_key = true` in their config and ran
//! `aimodel server` got a server with authentication **off** — obeyed for
//! everything except the one setting where being wrong is a security failure
//! rather than an inconvenience. The AI API's routes are not read-only: model
//! download, model convert (which spawns a converter), model delete, storage
//! cleanup.
//!
//! `check_startup` closes the second half. Nothing couples "I changed the bind
//! address" to "I turned authentication on", so a non-loopback bind without keys
//! was one careless step away. It is now refused before anything is bound, with
//! the setting that would allow it named in the error.

use iron_shell::ai_api::config::{check_startup, is_loopback, resolve_auth_requirement, APIConfig};

#[test]
fn an_absent_flag_cannot_switch_configured_auth_off() {
    // The regression. Config says yes, flag is absent (false) — the answer is yes.
    assert!(
        resolve_auth_requirement(true, false),
        "a configured `require_api_key = true` was dropped by the CLI default; \
         this is the fail-open assignment that made `aimodel server` start an \
         unauthenticated server for someone who had asked for authentication"
    );
}

#[test]
fn the_flag_can_still_turn_auth_on() {
    // The other direction must keep working, or the fix is just a removal.
    assert!(
        resolve_auth_requirement(false, true),
        "--require-api-key must still enable it"
    );
}

#[test]
fn both_off_stays_off() {
    // Not a hidden default change: with neither asking for it, nothing is added.
    assert!(!resolve_auth_requirement(false, false));
}

fn config_at(host: &str, require: bool, keys: &[&str]) -> APIConfig {
    let mut config = APIConfig::default();
    config.server.host = host.to_string();
    config.security.require_api_key = require;
    config.security.api_keys = keys.iter().map(|k| k.to_string()).collect();
    config
}

#[test]
fn binding_beyond_loopback_without_auth_is_refused() {
    for host in ["0.0.0.0", "::", "192.168.1.10", "10.0.0.5", "example.com"] {
        let config = config_at(host, false, &[]);
        let err = check_startup(&config.server, &config.security)
            .expect_err("a non-loopback bind with no keys must be refused");
        let text = err.to_string();
        assert!(
            text.contains(host),
            "the error should name the address: {text}"
        );
        assert!(
            text.contains("api_keys"),
            "the error should say what to set: {text}"
        );
    }
}

#[test]
fn a_non_loopback_bind_needs_both_the_switch_and_a_key() {
    // Keys without the switch, and the switch without keys, are both refused.
    let keys_only = config_at("0.0.0.0", false, &["k"]);
    assert!(check_startup(&keys_only.server, &keys_only.security).is_err());
    let switch_only = config_at("0.0.0.0", true, &[]);
    assert!(check_startup(&switch_only.server, &switch_only.security).is_err());
    let both = config_at("0.0.0.0", true, &["k"]);
    assert!(check_startup(&both.server, &both.security).is_ok());
}

#[test]
fn loopback_without_auth_is_allowed() {
    // The default posture — localhost-only, no key — is deliberate and common.
    for host in ["127.0.0.1", "localhost", "::1", "[::1]", "127.0.0.5"] {
        let config = config_at(host, false, &[]);
        assert!(
            check_startup(&config.server, &config.security).is_ok(),
            "{host} is loopback and should start without keys"
        );
    }
}

#[test]
fn enable_tls_is_refused_because_tls_is_not_implemented() {
    let mut config = config_at("127.0.0.1", false, &[]);
    config.security.enable_tls = true;
    let err = check_startup(&config.server, &config.security)
        .expect_err("enable_tls must not start a plain-HTTP server");
    assert!(err.to_string().contains("TLS"), "{err}");
}

#[test]
fn an_unrecognised_host_is_treated_as_exposed() {
    // Conservative by design: a spelling this cannot classify is refused without keys.
    assert!(!is_loopback("example.com"));
    assert!(!is_loopback(""));
    assert!(!is_loopback("not an address"));
}
