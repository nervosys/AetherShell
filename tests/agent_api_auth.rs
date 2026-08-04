//! The Agent API must not execute code for an unauthenticated caller.
//!
//! `POST /api/v1/eval` evaluates arbitrary AetherShell code, which can spawn
//! processes. Until 2026-07-30 the server applied no authentication to any
//! route, and — with CORS on by default — answered
//! `allow_origin(Any) / allow_methods(Any) / allow_headers(Any)`. Any web page
//! the user visited while the server was running could therefore preflight
//! successfully and POST to their loopback interface, making this drive-by
//! remote code execution rather than a local-only convenience.
//!
//! These tests drive the real router over a real TCP socket, so they exercise
//! the middleware as mounted rather than a reimplementation of it.

#![cfg(feature = "native")]

use aethershell::agent_api::server::{start_agent_api_server, AgentApiConfig};

/// Bind an ephemeral port, start the server on it, and wait for it to accept.
async fn serve(token: &str) -> u16 {
    // Port 0 asks the OS for a free port; find one, release it, then hand it to
    // the server. A small race remains but is confined to this test process.
    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("bind probe");
    let port = probe.local_addr().expect("addr").port();
    drop(probe);

    let config = AgentApiConfig {
        host: "127.0.0.1".to_string(),
        port,
        enable_cors: true,
        auth_token: Some(token.to_string()),
    };
    tokio::spawn(async move {
        let _ = start_agent_api_server(config).await;
    });

    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return port;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("agent api server did not start on port {port}");
}

fn eval_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/api/v1/eval")
}

#[tokio::test]
async fn eval_without_a_token_is_rejected() {
    let port = serve("correct-horse-battery-staple").await;
    let res = reqwest::Client::new()
        .post(eval_url(port))
        .json(&serde_json::json!({ "code": "1 + 1" }))
        .send()
        .await
        .expect("request");

    assert_eq!(
        res.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "unauthenticated eval must be refused — this endpoint runs arbitrary code"
    );
    assert!(
        res.headers()
            .contains_key(reqwest::header::WWW_AUTHENTICATE),
        "a 401 should advertise the scheme so clients know what to send"
    );
}

#[tokio::test]
async fn eval_with_the_wrong_token_is_rejected() {
    let port = serve("the-real-token").await;
    for wrong in [
        "not-the-token",
        // A prefix of the real token: guards against a comparison that stops at
        // the shorter length.
        "the-real",
        "",
    ] {
        let res = reqwest::Client::new()
            .post(eval_url(port))
            .bearer_auth(wrong)
            .json(&serde_json::json!({ "code": "1 + 1" }))
            .send()
            .await
            .expect("request");
        assert_eq!(
            res.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "token {wrong:?} must not be accepted"
        );
    }
}

#[tokio::test]
async fn eval_with_the_right_token_is_allowed() {
    let token = "a-token-that-should-work";
    let port = serve(token).await;
    let res = reqwest::Client::new()
        .post(eval_url(port))
        .bearer_auth(token)
        .json(&serde_json::json!({ "code": "1 + 1" }))
        .send()
        .await
        .expect("request");

    assert_ne!(
        res.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "the configured token must be accepted, or the server is simply broken \
         rather than secure"
    );
}

/// Liveness probes must keep working without a credential, or the exemption
/// carved out in the middleware is pointless.
#[tokio::test]
async fn health_stays_reachable_without_a_token() {
    let port = serve("some-token").await;
    let res = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .await
        .expect("request");

    assert_ne!(res.status(), reqwest::StatusCode::UNAUTHORIZED);
}
