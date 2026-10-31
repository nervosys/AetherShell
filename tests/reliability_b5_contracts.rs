//! Reliability contracts for the network and dialog paths the B5 fixes touched.
//!
//! Each test drives the real code against a server it starts on 127.0.0.1, or
//! against a port that is closed, and never against a real host. The redirect
//! checks judge the target of each hop, and a loopback start is allowed through
//! the egress allowlist, so the tests set `IRONSHELL_NET_ALLOW` to loopback.
//! A redirect to a private address is then refused by that allowlist before
//! anything connects there.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iron_shell::marketplace::RegistryClient;
use iron_shell::plugins::{MarketplaceRegistry, PluginRegistry};
use iron_shell::safety::{ErrorCode, SafetyError};
use iron_shell::value::Value;

/// Tests that read or set the process environment run one at a time.
static ENV: Mutex<()> = Mutex::new(());

fn lock_env() -> std::sync::MutexGuard<'static, ()> {
    ENV.lock().unwrap_or_else(|e| e.into_inner())
}

/// Loopback is the only host the allowlist permits in these tests.
fn allow_loopback_only() {
    std::env::set_var("IRONSHELL_NET_ALLOW", "127.0.0.1");
    std::env::remove_var("IRONSHELL_POLICY");
}

/// A TCP port that nothing listens on: bound, then released.
fn closed_base() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    let port = listener.local_addr().expect("local address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// One answer the test server gives.
struct Reply {
    status: u16,
    location: Option<String>,
    body: String,
}

fn answer(status: u16, body: &str) -> Reply {
    Reply {
        status,
        location: None,
        body: body.to_string(),
    }
}

fn redirect(to: &str) -> Reply {
    Reply {
        status: 302,
        location: Some(to.to_string()),
        body: String::new(),
    }
}

/// A local HTTP server. `handler` gets the server's base URL, the method and
/// the path, and every request is recorded as (method, path) in arrival order.
struct Server {
    base: String,
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

fn start(handler: impl Fn(&str, &str, &str) -> Reply + Send + Sync + 'static) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    let port = listener.local_addr().expect("local address").port();
    let base = format!("http://127.0.0.1:{port}");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (serve_base, serve_seen) = (base.clone(), Arc::clone(&seen));
    // Each connection is one request, answered with Connection: close, so a
    // client's next hop is a new request the server records. The thread lives
    // until the test process exits.
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let Some((method, path)) = read_request(&mut stream) else {
                continue;
            };
            serve_seen
                .lock()
                .unwrap()
                .push((method.clone(), path.clone()));
            let reply = handler(&serve_base, &method, &path);
            let mut head = format!(
                "HTTP/1.1 {} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n",
                reply.status,
                reply.body.len()
            );
            if let Some(location) = &reply.location {
                head.push_str(&format!("Location: {location}\r\n"));
            }
            head.push_str("\r\n");
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(reply.body.as_bytes());
            let _ = stream.flush();
        }
    });
    Server { base, seen }
}

/// The method and path of one request, with its body read and discarded.
fn read_request(stream: &mut TcpStream) -> Option<(String, String)> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut parts = lines.next()?.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    while buf.len() < head_end + length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Some((method, path))
}

/// The structured error behind an anyhow error, as its code and retryability.
fn coded(e: &anyhow::Error) -> (ErrorCode, bool) {
    let se = e
        .downcast_ref::<SafetyError>()
        .unwrap_or_else(|| panic!("the error is not coded: {e}"));
    (se.code, se.code.retryable())
}

fn one_marketplace(base: &str) -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    registry.__test_set_marketplaces(vec![MarketplaceRegistry {
        name: "test".to_string(),
        url: base.to_string(),
        enabled: true,
    }]);
    registry
}

/// A plugin search answer with one plugin in it, as the marketplace sends it.
const ONE_PLUGIN: &str = r#"[{"id":"acme","name":"Acme","version":"1.0.0","description":"d","author":"a","downloads":3,"categories":[],"keywords":[],"homepage":null,"repository":null}]"#;

// ---------------------------------------------------------------------------
// Marketplace down: a network failure, never a missing plugin
// ---------------------------------------------------------------------------

#[test]
fn a_closed_marketplace_is_a_network_failure_not_a_missing_plugin() {
    let _env = lock_env();
    let mut registry = one_marketplace(&closed_base());
    let err = registry
        .install_from_marketplace("acme")
        .expect_err("nothing is listening, so the install cannot succeed");
    let (code, retryable) = coded(&err);
    assert_eq!(code, ErrorCode::Network, "got: {err}");
    assert!(retryable, "a network failure is retryable");
    assert_eq!(code.as_str(), "E_NETWORK");
    assert_eq!(code.exit_code(), 69);
}

#[test]
fn a_404_does_not_excuse_a_closed_marketplace_beside_it() {
    let _env = lock_env();
    let listed_nowhere = start(|_, _, _| answer(404, "{}"));
    let mut registry = PluginRegistry::new();
    registry.__test_set_marketplaces(vec![
        MarketplaceRegistry {
            name: "answers".to_string(),
            url: listed_nowhere.base.clone(),
            enabled: true,
        },
        MarketplaceRegistry {
            name: "down".to_string(),
            url: closed_base(),
            enabled: true,
        },
    ]);
    let err = registry
        .install_from_marketplace("acme")
        .expect_err("the plugin may be in the marketplace that is down");
    let (code, _) = coded(&err);
    assert_eq!(code, ErrorCode::Network, "got: {err}");
}

#[test]
fn a_real_404_from_every_marketplace_is_not_found() {
    let _env = lock_env();
    let server = start(|_, _, _| answer(404, "{}"));
    let mut registry = one_marketplace(&server.base);
    let err = registry
        .install_from_marketplace("acme")
        .expect_err("no marketplace lists the plugin");
    let (code, retryable) = coded(&err);
    assert_eq!(code, ErrorCode::NotFound, "got: {err}");
    assert!(retryable, "a missing name is retryable with another name");
}

#[test]
fn a_server_error_from_the_marketplace_is_a_network_failure() {
    let _env = lock_env();
    let server = start(|_, _, _| answer(503, "unavailable"));
    let mut registry = one_marketplace(&server.base);
    let err = registry
        .install_from_marketplace("acme")
        .expect_err("the marketplace answered 503");
    let (code, _) = coded(&err);
    assert_eq!(code, ErrorCode::Network, "got: {err}");
}

// ---------------------------------------------------------------------------
// Redirects are judged hop by hop (plugins, marketplace, workflow HTTP)
// ---------------------------------------------------------------------------

#[test]
fn a_plugin_search_redirect_to_a_private_address_is_refused_before_it_is_requested() {
    let _env = lock_env();
    allow_loopback_only();
    let server = start(|_, _, _| redirect("http://10.255.255.1/search?q=acme"));
    let registry = one_marketplace(&server.base);
    let err = registry
        .search_marketplace("acme")
        .expect_err("the hop leaves the allowlist");
    let (code, _) = coded(&err);
    assert_eq!(code, ErrorCode::PolicyDeny, "got: {err}");
    assert!(
        err.to_string().contains("redirect"),
        "the refusal names the redirect: {err}"
    );
    assert_eq!(
        server.seen.lock().unwrap().len(),
        1,
        "only the first request reached the server"
    );
}

#[test]
fn a_plugin_search_follows_a_relative_redirect_inside_the_allowlist() {
    let _env = lock_env();
    allow_loopback_only();
    let server = start(|_, _, path| {
        if path.starts_with("/search") {
            redirect("/final")
        } else {
            answer(200, ONE_PLUGIN)
        }
    });
    let registry = one_marketplace(&server.base);
    let found = registry
        .search_marketplace("acme")
        .expect("the redirect is followed");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "acme");
    let seen = server.seen.lock().unwrap().clone();
    assert_eq!(
        seen,
        vec![
            ("GET".to_string(), "/search?q=acme".to_string()),
            ("GET".to_string(), "/final".to_string())
        ],
        "the second hop is the redirect's target"
    );
}

#[test]
fn a_redirect_loop_stops_at_the_hop_ceiling_as_a_budget_error() {
    let _env = lock_env();
    allow_loopback_only();
    let server = start(|base, _, path| redirect(&format!("{base}{path}")));
    let registry = one_marketplace(&server.base);
    let err = registry
        .search_marketplace("loop")
        .expect_err("a loop never answers");
    let (code, _) = coded(&err);
    assert_eq!(code, ErrorCode::BudgetExceeded, "got: {err}");
    // The first request and the ten hops web_curl allows: eleven in all.
    assert_eq!(server.seen.lock().unwrap().len(), 11);
}

#[test]
fn the_registry_client_refuses_a_redirect_to_a_private_address() {
    let _env = lock_env();
    allow_loopback_only();
    let server = start(|_, _, _| redirect("http://10.255.255.1/api/v1/agents/acme"));
    let client = RegistryClient::new(Some(server.base.clone())).expect("registry client");
    let outcome = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(client.get("acme", None));
    let err = match outcome {
        Ok(_) => panic!("a refused hop must not yield an agent"),
        Err(e) => e,
    };
    let (code, _) = coded(&err);
    assert_eq!(code, ErrorCode::PolicyDeny, "got: {err}");
}

#[test]
fn the_registry_client_says_404_is_not_found_and_503_is_a_network_failure() {
    let _env = lock_env();
    let runtime = tokio::runtime::Runtime::new().expect("runtime");

    let missing = start(|_, _, _| answer(404, "{}"));
    let client = RegistryClient::new(Some(missing.base.clone())).expect("registry client");
    let err = match runtime.block_on(client.get("acme", None)) {
        Ok(_) => panic!("a 404 must not yield an agent"),
        Err(e) => e,
    };
    assert_eq!(coded(&err).0, ErrorCode::NotFound, "got: {err}");

    let broken = start(|_, _, _| answer(503, "unavailable"));
    let client = RegistryClient::new(Some(broken.base.clone())).expect("registry client");
    let err = match runtime.block_on(client.get("acme", None)) {
        Ok(_) => panic!("a 503 must not yield an agent"),
        Err(e) => e,
    };
    assert_eq!(coded(&err).0, ErrorCode::Network, "got: {err}");
}

#[test]
fn the_registry_client_reports_a_closed_registry_as_a_network_failure() {
    let _env = lock_env();
    let client = RegistryClient::new(Some(closed_base())).expect("registry client");
    let err = match tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(client.get("acme", None))
    {
        Ok(_) => panic!("nothing is listening"),
        Err(e) => e,
    };
    let (code, retryable) = coded(&err);
    assert_eq!(code, ErrorCode::Network, "got: {err}");
    assert!(retryable);
}

#[test]
fn a_workflow_post_redirect_is_followed_as_a_get_and_judged_per_hop() {
    let _env = lock_env();
    allow_loopback_only();
    let server = start(|_, _, path| {
        if path == "/start" {
            redirect("/final")
        } else {
            answer(200, "final-body")
        }
    });
    let outcome = iron_shell::workflows::__test_http_send(
        "POST",
        &format!("{}/start", server.base),
        Some(Value::Str("payload".to_string())),
    )
    .expect("the redirect is followed");
    let Value::Record(record) = outcome else {
        panic!("a workflow HTTP step answers a record");
    };
    assert_eq!(record.get("status"), Some(&Value::Int(200)));
    assert_eq!(
        record.get("body"),
        Some(&Value::Str("final-body".to_string()))
    );
    let seen = server.seen.lock().unwrap().clone();
    assert_eq!(
        seen,
        vec![
            ("POST".to_string(), "/start".to_string()),
            ("GET".to_string(), "/final".to_string())
        ],
        "a 302 answering a POST continues as a GET, as reqwest and curl do"
    );
}

#[test]
fn a_workflow_redirect_to_a_private_address_is_refused() {
    let _env = lock_env();
    allow_loopback_only();
    let server = start(|_, _, _| redirect("http://10.255.255.1/x"));
    let err = iron_shell::workflows::__test_http_send("GET", &format!("{}/", server.base), None)
        .expect_err("the hop leaves the allowlist");
    let (code, _) = coded(&err);
    assert_eq!(code, ErrorCode::PolicyDeny, "got: {err}");
    assert_eq!(server.seen.lock().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Desktop dialogs are bounded: the deadline helper kills a child that never ends
// ---------------------------------------------------------------------------

#[test]
fn a_child_that_never_answers_is_stopped_at_the_deadline() {
    // A harmless long-lived child on loopback stands in for a dialog nobody
    // dismisses. The dialogs run through the same helper.
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("ping");
        c.args(["-n", "60", "127.0.0.1"]);
        c
    } else {
        let mut c = Command::new("sleep");
        c.arg("60");
        c
    };
    cmd.stdin(std::process::Stdio::null());
    let started = Instant::now();
    let io_err = iron_shell::safety::output_with_timeout(cmd, 2)
        .expect_err("the child outlives its deadline");
    let elapsed = started.elapsed();
    let err = iron_shell::safety::spawn_error("gui_dialog_message", "zenity", &io_err);
    let (code, _) = coded(&err);
    assert_eq!(code, ErrorCode::BudgetExceeded, "got: {err}");
    assert!(
        elapsed < Duration::from_secs(30),
        "stopped at the deadline, not when the child ends: {elapsed:?}"
    );
}
