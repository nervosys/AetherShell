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
//! `resolve_auth_requirement` makes the fold one-way and reports the second half
//! of the problem: nothing couples "I changed the bind address" to "I turned
//! authentication on", so binding a non-loopback address without a key is
//! possible in one careless step. That is a legitimate choice behind a trusted
//! boundary, so it warns rather than refuses — but it must not be silent.

use aethershell::ai_api::config::{is_loopback, resolve_auth_requirement};

#[test]
fn an_absent_flag_cannot_switch_configured_auth_off() {
    // The regression. Config says yes, flag is absent (false) — the answer is yes.
    let (effective, _) = resolve_auth_requirement(true, false, "127.0.0.1");
    assert!(
        effective,
        "a configured `require_api_key = true` was dropped by the CLI default; \
         this is the fail-open assignment that made `aimodel server` start an \
         unauthenticated server for someone who had asked for authentication"
    );
}

#[test]
fn the_flag_can_still_turn_auth_on() {
    // The other direction must keep working, or the fix is just a removal.
    let (effective, _) = resolve_auth_requirement(false, true, "127.0.0.1");
    assert!(effective, "--require-api-key must still enable it");
}

#[test]
fn both_off_stays_off() {
    // Not a hidden default change: with neither asking for it, nothing is added.
    let (effective, _) = resolve_auth_requirement(false, false, "127.0.0.1");
    assert!(!effective);
}

#[test]
fn binding_beyond_loopback_without_auth_is_reported() {
    for host in ["0.0.0.0", "::", "192.168.1.10", "10.0.0.5"] {
        let (effective, warning) = resolve_auth_requirement(false, false, host);
        assert!(!effective);
        let w = warning.unwrap_or_else(|| {
            panic!("binding {host} with no API key required must not be silent")
        });
        assert!(
            w.contains(host),
            "the warning should name the address it is about: {w}"
        );
    }
}

#[test]
fn loopback_without_auth_is_not_nagged_about() {
    // The default posture — localhost-only, no key — is deliberate and common.
    // Warning about it would train people to ignore the warning that matters.
    for host in ["127.0.0.1", "localhost", "::1", "[::1]", "127.0.0.5"] {
        let (_, warning) = resolve_auth_requirement(false, false, host);
        assert!(
            warning.is_none(),
            "{host} is loopback and should not warn: {warning:?}"
        );
    }
}

#[test]
fn an_exposed_host_with_auth_on_is_not_warned_about() {
    let (effective, warning) = resolve_auth_requirement(true, false, "0.0.0.0");
    assert!(effective);
    assert!(
        warning.is_none(),
        "the warning is about being exposed *without* auth, not about being exposed"
    );
}

#[test]
fn an_unrecognised_host_errs_toward_warning() {
    // Conservative by design: a spelling this cannot classify is treated as
    // exposed, so the failure mode is a needless warning rather than silence
    // about a reachable server.
    assert!(!is_loopback("example.com"));
    assert!(!is_loopback(""));
    assert!(!is_loopback("not an address"));
    let (_, warning) = resolve_auth_requirement(false, false, "example.com");
    assert!(warning.is_some());
}
