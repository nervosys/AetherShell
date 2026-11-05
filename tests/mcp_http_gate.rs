//! The MCP HTTP server runs OS tools on behalf of whoever holds its token, so
//! the gate has to hold over HTTP, not just in the library.
//!
//! Each test spawns the real `ish mcp serve` on a free loopback port, in a
//! temporary working directory, and talks to it over a raw TCP connection (the
//! dev-dependencies carry no HTTP client). Effects are checked on the
//! filesystem afterwards: a refusal that reads well but wrote the file is the
//! failure this file exists to catch.
//!
//! The original hole: `POST /mcp/v1/tools/curl/execute` with
//! `{"method":"-o","data":"<outside path>"}` made curl write its response to
//! that path. Agent mode did not stop it because OS tools never reached the
//! effect gate.

use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

const AE: &str = env!("CARGO_BIN_EXE_ish");
const TOKEN: &str = "mcp-gate-test-token";

struct Server {
    child: Child,
    port: u16,
    root: PathBuf,
    workspace: PathBuf,
    log: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A port the OS has just handed out on loopback. Racy in principle, and the
/// spawn below fails loudly if it is taken, which is the honest outcome.
fn free_loopback_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind loopback")
        .local_addr()
        .expect("local addr")
        .port()
}

/// Start `ish mcp serve` with the given `IRONSHELL_MODE` (or none, to test the
/// default), and wait until `/health` answers.
fn start(tag: &str, mode: Option<&str>) -> Server {
    let root = std::env::temp_dir().join(format!("ae_mcp_gate_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let workspace = root.join("workspace");
    std::fs::create_dir_all(&workspace).expect("create workspace");
    std::fs::create_dir_all(root.join("elsewhere")).expect("create outside dir");
    let log = root.join("server.log");
    let log_file = File::create(&log).expect("create server log");
    let port = free_loopback_port();

    let mut cmd = Command::new(AE);
    cmd.args([
        "mcp",
        "serve",
        "--host",
        "127.0.0.1",
        "--port",
        &port.to_string(),
    ])
    .current_dir(&workspace)
    .env("IRONSHELL_API_TOKEN", TOKEN)
    .env("IRONSHELL_SESSION_DIR", root.join("session"))
    .env_remove("IRONSHELL_WORKSPACE")
    .env_remove("IRONSHELL_AGENT")
    .stdout(log_file.try_clone().expect("clone log"))
    .stderr(log_file);
    match mode {
        Some(m) => cmd.env("IRONSHELL_MODE", m),
        None => cmd.env_remove("IRONSHELL_MODE"),
    };
    let child = cmd.spawn().expect("spawn ish mcp serve");
    let server = Server {
        child,
        port,
        root,
        workspace,
        log,
    };

    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some((200, _)) = request(port, "GET", "/health", None, "") {
            return server;
        }
        if Instant::now() > deadline {
            panic!(
                "mcp serve did not answer /health on port {port}; log:\n{}",
                std::fs::read_to_string(&server.log).unwrap_or_default()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// One HTTP/1.1 request over a fresh loopback connection. `None` when the
/// connection itself fails, so the readiness loop can retry.
fn request(
    port: u16,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &str,
) -> Option<(u16, String)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .expect("read timeout");
    let auth = match token {
        Some(t) => format!("Authorization: Bearer {t}\r\n"),
        None => String::new(),
    };
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\
         Content-Type: application/json\r\n{auth}Content-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes()).ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n")?;
    let status: u16 = head.split_whitespace().nth(1)?.parse().ok()?;
    Some((status, body.to_string()))
}

fn call_tool(
    server: &Server,
    tool: &str,
    arguments: serde_json::Value,
) -> (u16, serde_json::Value) {
    let body = serde_json::json!({ "arguments": arguments }).to_string();
    let (status, text) = request(
        server.port,
        "POST",
        &format!("/mcp/v1/tools/{tool}/execute"),
        Some(TOKEN),
        &body,
    )
    .expect("tool call reached the server");
    let json = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
    (status, json)
}

/// The coded error inside a failed tool result: the text is
/// `Tool execution failed: {"error":{"code":...}}` for a refusal.
fn refusal_code(result: &serde_json::Value) -> Option<String> {
    let text = result["content"][0]["text"].as_str()?;
    let json = text.strip_prefix("Tool execution failed: ").unwrap_or(text);
    let parsed: serde_json::Value = serde_json::from_str(json).ok()?;
    parsed["error"]["code"].as_str().map(str::to_string)
}

fn outside_path(server: &Server, name: &str) -> PathBuf {
    server.root.join("elsewhere").join(name)
}

#[test]
fn the_default_mode_is_agent() {
    // `serving an agent implies agent mode`: with no IRONSHELL_MODE set, the
    // startup banner must report the agent profile, not the human one.
    let server = start("default_mode", None);
    let banner = std::fs::read_to_string(&server.log).unwrap_or_default();
    assert!(
        banner.contains("Effect gate: agent mode"),
        "default `ish mcp serve` did not start in agent mode:\n{banner}"
    );
}

#[test]
fn method_dash_o_writes_nothing_outside_the_workspace() {
    // The reproduced exploit, verbatim in shape. Refused before anything is
    // spawned: the file must not exist, and the refusal must be the coded
    // argument error, not an approval request.
    let server = start("method_dash_o", Some("agent"));
    let target = outside_path(&server, "pwned.txt");
    let (status, result) = call_tool(
        &server,
        "curl",
        serde_json::json!({
            "url": format!("http://127.0.0.1:{}/x", server.port),
            "method": "-o",
            "data": target.to_string_lossy(),
        }),
    );
    assert_eq!(status, 400, "{result}");
    assert_eq!(
        refusal_code(&result).as_deref(),
        Some("E_BAD_ARG"),
        "a method outside the declared enum must be refused as a bad argument: {result}"
    );
    assert!(
        !target.exists(),
        "curl wrote {} through an option-like method",
        target.display()
    );
}

#[test]
fn a_legal_call_is_held_for_approval_and_writes_nothing() {
    // A well-formed request in agent mode is not refused for its shape; it is
    // held by the effect gate. The path is outside the workspace, so this also
    // shows the gate ran before the spawn: no file appears.
    let server = start("legal_call", Some("agent"));
    let target = outside_path(&server, "held.txt");
    let (status, result) = call_tool(
        &server,
        "curl",
        serde_json::json!({
            "url": format!("http://127.0.0.1:{}/x", server.port),
            "method": "GET",
            "output": target.to_string_lossy(),
        }),
    );
    assert_eq!(status, 400, "{result}");
    assert_eq!(
        refusal_code(&result).as_deref(),
        Some("E_NEEDS_APPROVAL"),
        "an agent-mode exec must return the approval error: {result}"
    );
    assert!(
        !target.exists(),
        "a held call still wrote {}",
        target.display()
    );
    // The denial itself is audited under `.ironshell/`; nothing else may appear.
    let stray: Vec<String> = std::fs::read_dir(&server.workspace)
        .expect("read workspace")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name != ".ironshell")
        .collect();
    assert!(
        stray.is_empty(),
        "a held call created files in the workspace: {stray:?}"
    );
}

#[test]
fn an_unknown_enum_value_is_refused() {
    let server = start("enum", Some("agent"));
    let (status, result) = call_tool(
        &server,
        "curl",
        serde_json::json!({
            "url": format!("http://127.0.0.1:{}/x", server.port),
            "method": "TRACE",
        }),
    );
    assert_eq!(status, 400, "{result}");
    assert_eq!(
        refusal_code(&result).as_deref(),
        Some("E_BAD_ARG"),
        "{result}"
    );
}

#[test]
fn an_option_like_string_is_refused_for_a_parameter_that_is_not_a_flag() {
    let server = start("option_like", Some("agent"));
    let (status, result) = call_tool(
        &server,
        "curl",
        serde_json::json!({
            "url": format!("http://127.0.0.1:{}/x", server.port),
            "headers": ["-H"],
        }),
    );
    assert_eq!(status, 400, "{result}");
    assert_eq!(
        refusal_code(&result).as_deref(),
        Some("E_BAD_ARG"),
        "{result}"
    );
}

#[test]
fn an_unauthenticated_tool_call_is_refused_before_any_gate() {
    // The token check sits outside the admission and gate layers: a stranger
    // gets 401 and never reaches the tool.
    let server = start("unauth", Some("agent"));
    let target = outside_path(&server, "stranger.txt");
    let body = serde_json::json!({
        "arguments": { "url": "http://127.0.0.1:1/x", "method": "-o",
                       "data": target.to_string_lossy() }
    })
    .to_string();
    let (status, _) = request(
        server.port,
        "POST",
        "/mcp/v1/tools/curl/execute",
        None,
        &body,
    )
    .expect("request reached the server");
    assert_eq!(status, 401);
    assert!(!target.exists());
}
