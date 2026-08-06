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
    serve_with_timeout(token, 300).await
}

/// As `serve`, but with an explicit request deadline so the timeout test does
/// not have to wait out the 300-second default.
async fn serve_with_timeout(token: &str, request_timeout_secs: u64) -> u16 {
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
        request_timeout_secs,
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

/// An authenticated caller must not be able to hold a worker indefinitely.
///
/// `/api/v1/eval` evaluates arbitrary code, so a wedged request is a one-line
/// POST. The deadline has to *actually fire*, which is a stronger claim than
/// "a TimeoutLayer is mounted": the handler calls `process_request`
/// synchronously, so unless that work is moved off the async worker the
/// timeout branch is never polled and the layer is decorative.
#[tokio::test]
async fn a_wedged_request_is_cancelled_rather_than_holding_a_worker() {
    let token = "timeout-token";
    let port = serve_with_timeout(token, 1).await;

    let started = std::time::Instant::now();
    let res = reqwest::Client::new()
        .post(eval_url(port))
        .bearer_auth(token)
        .json(&serde_json::json!({ "code": "sleep 20" }))
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .expect("request");
    let elapsed = started.elapsed();

    assert_eq!(
        res.status(),
        reqwest::StatusCode::REQUEST_TIMEOUT,
        "a request exceeding the deadline must be cancelled with 408, not run \
         to completion (took {elapsed:?})"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "the deadline must fire near its configured 1s, not after the work \
         finishes — took {elapsed:?}, which means the timeout branch is not \
         being polled"
    );
}

/// The deadline must not sever the long-lived routes, which are supposed to
/// stay open past it.
#[tokio::test]
async fn streaming_routes_are_exempt_from_the_deadline() {
    let token = "stream-token";
    let port = serve_with_timeout(token, 1).await;

    let started = std::time::Instant::now();
    let res = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/api/v1/stream/eval"))
        .bearer_auth(token)
        .json(&serde_json::json!({ "type": "Eval", "code": "sleep 3" }))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .expect("request");

    assert_ne!(
        res.status(),
        reqwest::StatusCode::REQUEST_TIMEOUT,
        "SSE routes are long-lived by design and must not be wrapped by the \
         per-request deadline (took {:?})",
        started.elapsed()
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
