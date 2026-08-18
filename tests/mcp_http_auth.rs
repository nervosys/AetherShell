//! The MCP HTTP server executes builtins. It must not do so for strangers.
//!
//! It had no authentication at all, while `POST /mcp/v1/tools/:name/execute`
//! runs any builtin and `enable_cors` defaulted to true. With `--cors` that was
//! demonstrably exploitable, not theoretically:
//!
//! ```text
//! POST http://127.0.0.1:PORT/mcp/v1/tools/read_text/execute
//! Origin: https://evil.example
//! {"arguments":{"args":["C:/Windows/win.ini"]}}
//!   -> 200, file contents, access-control-allow-origin: *
//! ```
//!
//! Loopback binding is not a defence — the browser is on the same machine,
//! which is the whole reason CORS exists. `agent_api` already carried a comment
//! explaining that its own `allow_origin(Any)` is tolerable only *because* a
//! bearer token is required; this server simply lacked the token.

use aethershell::mcp::server::McpServerConfig;

#[test]
fn cors_is_off_by_default() {
    // A library caller taking `Default` must not silently opt into letting any
    // web page drive a builtin-executing server.
    assert!(
        !McpServerConfig::default().enable_cors,
        "default CORS must be off on a server that executes builtins"
    );
}

#[test]
fn the_default_config_carries_no_preset_token() {
    // `None` means "mint one and print it", which is what keeps the server from
    // ever being reachable without a credential. A hardcoded default would be
    // worse than none.
    assert!(
        McpServerConfig::default().auth_token.is_none(),
        "a shipped default token would be a shared secret, not a secret"
    );
}

#[test]
fn the_config_exposes_a_token_field_at_all() {
    // Pins the field's existence: the vulnerability was not a wrong value but a
    // missing concept, and a refactor that dropped the field would reintroduce
    // it silently while every other test still passed.
    let cfg = McpServerConfig {
        auth_token: Some("t".to_string()),
        ..Default::default()
    };
    assert_eq!(cfg.auth_token.as_deref(), Some("t"));
}
