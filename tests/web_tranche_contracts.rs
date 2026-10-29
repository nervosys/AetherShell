//! Contracts for the `web_*` builtins that are declared in `src/signature.rs`.
//!
//! Network rule: nothing here contacts any host but 127.0.0.1. Refusal tests
//! use hosts that are never reached, because the refusal comes first. Positive
//! tests start a loopback server inside the test and drive the `ae` binary as a
//! child process with a 30 s deadline.
//!
//! `web_open_url` and `web_browser_open` launch the desktop browser, so they are
//! only ever called here with input that must be refused.

use aethershell::builtins;
use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, Once};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The body cap in `src/builtins.rs` (`WEB_BODY_MAX`).
const BODY_MAX: usize = 8 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(30);

// ── in-process calls ────────────────────────────────────────────────────

fn isolate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_web_tranche_{}", std::process::id()));
        std::env::set_var("AETHER_SESSION_DIR", dir);
    });
}

/// `AETHER_MODE` is process-global, and one test changes it. Every in-process
/// call takes this lock so that no call sees another test's mode.
static MODE_LOCK: Mutex<()> = Mutex::new(());

fn locked() -> std::sync::MutexGuard<'static, ()> {
    MODE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Human mode, whatever the environment says. In agent mode a network-classed
/// name is gated by the central guard before its body runs, so the coded answer
/// these tests check would be an approval request instead.
fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let _guard = locked();
    std::env::remove_var("AETHER_MODE");
    std::env::remove_var("AETHER_AGENT");
    invoke(name, args)
}

/// The call itself, for a caller that already holds `MODE_LOCK`.
fn invoke(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    isolate();
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

/// The first `E_...` code in an error's rendered text.
fn code_of(r: &anyhow::Result<Value>) -> String {
    match r {
        Ok(v) => panic!("expected a coded refusal, got {v:?}"),
        Err(e) => {
            let msg = format!("{e:#}");
            let at = msg
                .find("E_")
                .unwrap_or_else(|| panic!("no code in: {msg}"));
            msg[at..]
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || *c == '_')
                .collect()
        }
    }
}

#[test]
fn every_web_builtin_refuses_piped_input() {
    // A piped value that the builtin would ignore is a silent discard; each of
    // these must refuse it before any request, launch or write.
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("web_open_url", vec![text("https://example.invalid/")]),
        ("web_browser_open", vec![text("https://example.invalid/")]),
        ("web_fetch", vec![text("https://example.invalid/")]),
        ("web_get", vec![text("https://example.invalid/")]),
        ("web_post", vec![text("https://example.invalid/")]),
        ("web_json_get", vec![text("https://example.invalid/")]),
        ("web_json_post", vec![text("https://example.invalid/")]),
        ("web_scrape", vec![text("https://example.invalid/")]),
        ("web_download", vec![text("https://example.invalid/")]),
        ("web_headers", vec![text("https://example.invalid/")]),
        ("web_cookies", vec![text("https://example.invalid/")]),
        ("web_form_submit", vec![text("https://example.invalid/")]),
        (
            "web_upload_file",
            vec![text("https://example.invalid/"), text("a.txt")],
        ),
        (
            "web_rest_api",
            vec![text("GET"), text("https://example.invalid/")],
        ),
        (
            "web_graphql",
            vec![text("https://example.invalid/"), text("{ a }")],
        ),
        ("web_check_url", vec![text("https://example.invalid/")]),
        ("web_robots_txt", vec![text("https://example.invalid/")]),
        ("web_sitemap", vec![text("https://example.invalid/")]),
        ("web_websocket", vec![text("wss://example.invalid/")]),
        ("web_xpath", vec![text("<p/>"), text("//p")]),
    ];
    let _guard = locked();
    std::env::remove_var("AETHER_MODE");
    std::env::remove_var("AETHER_AGENT");
    isolate();
    for (name, args) in cases {
        let mut env = Env::new();
        let r = builtins::call_with_input(name, args, Some(text("piped")), &mut env);
        assert_eq!(code_of(&r), "E_BAD_ARG", "{name} must refuse piped input");
    }
}

#[test]
fn non_http_schemes_and_malformed_urls_are_refused_before_any_request() {
    // None of these may reach the network or the launcher. Each is a coded
    // E_BAD_ARG, and a reached host would have answered with a different code.
    let bad_urls = [
        "file:///etc/hosts",
        "javascript:alert(1)",
        "data:text/html,<p>x</p>",
        "ftp://example.invalid/x",
        "gopher://example.invalid/",
        "-x",
        "not a url",
        "example.invalid",
        "https://example.invalid/\nX-Injected: 1",
    ];
    let one_arg = [
        "web_fetch",
        "web_get",
        "web_post",
        "web_json_get",
        "web_json_post",
        "web_scrape",
        "web_headers",
        "web_cookies",
        "web_check_url",
        "web_robots_txt",
        "web_sitemap",
        "web_form_submit",
        "web_open_url",
        "web_browser_open",
        "web_download",
    ];
    for url in bad_urls {
        for name in one_arg {
            let r = call(name, vec![text(url)]);
            assert_eq!(code_of(&r), "E_BAD_ARG", "{name}({url:?})");
        }
        let r = call("web_download", vec![text(url), text("x.bin")]);
        assert_eq!(code_of(&r), "E_BAD_ARG", "web_download({url:?}, path)");
        let r = call("web_upload_file", vec![text(url), text("a.txt")]);
        assert_eq!(code_of(&r), "E_BAD_ARG", "web_upload_file({url:?})");
        let r = call("web_rest_api", vec![text("GET"), text(url)]);
        assert_eq!(code_of(&r), "E_BAD_ARG", "web_rest_api({url:?})");
        let r = call("web_graphql", vec![text(url), text("{ a }")]);
        assert_eq!(code_of(&r), "E_BAD_ARG", "web_graphql({url:?})");
    }
}

#[test]
fn a_refused_scheme_is_refused_before_the_network_guard_runs() {
    // In agent mode with no allowlist, a network call needs approval. A `file:`
    // download must be refused for its scheme first, so the answer is the
    // argument refusal and not an approval request.
    let _guard = locked();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::remove_var("AETHER_NET_ALLOW");
    let r = invoke(
        "web_download",
        vec![text("file:///etc/hosts"), text("x.bin")],
    );
    std::env::remove_var("AETHER_MODE");
    assert_eq!(code_of(&r), "E_BAD_ARG");
}

#[test]
fn browser_launchers_refuse_every_non_web_target() {
    // Refusals only: a positive call would launch the desktop browser.
    let targets = [
        "file:///C:/Windows/win.ini",
        "javascript:alert(1)",
        "data:text/html,<p>x</p>",
        "ms-msdt:/id PCWDiagnostic",
        "mailto:someone@example.invalid",
        "-a",
        "https://example.invalid/\u{7}",
        "",
    ];
    for target in targets {
        for name in ["web_open_url", "web_browser_open"] {
            let r = call(name, vec![text(target)]);
            assert_eq!(code_of(&r), "E_BAD_ARG", "{name}({target:?})");
        }
    }
}

#[test]
fn header_values_that_could_inject_a_line_are_refused_before_any_request() {
    // The port is one nothing listens on. A request that got past the check
    // would fail to connect (E_TOOL_FAILED), so E_BAD_ARG proves the refusal.
    let url = "http://127.0.0.1:9/";
    let cases: Vec<(&str, Value)> = vec![
        ("X-A", text("1\r\nX-B: 2")),
        ("X-A", text("1\nX-B: 2")),
        ("X-A", text("tab\there\u{0}")),
        ("X A", text("1")),
        ("@headers.txt", text("1")),
        ("X-N", Value::Int(5)),
    ];
    for (name, value) in cases {
        let mut headers = BTreeMap::new();
        headers.insert(name.to_string(), value);
        let r = call(
            "web_rest_api",
            vec![text("GET"), text(url), Value::Null, Value::Record(headers)],
        );
        assert_eq!(code_of(&r), "E_BAD_ARG", "header {name:?}");
    }
    let r = call("web_rest_api", vec![text("TRACE"), text(url)]);
    assert_eq!(
        code_of(&r),
        "E_BAD_ARG",
        "TRACE is not a method web_rest_api sends"
    );
}

#[test]
fn class_and_id_selectors_are_refused_rather_than_read_as_tags() {
    for selector in [".title", "#main", "h1 a", "a#x", ""] {
        let r = call(
            "web_scrape",
            vec![text("https://example.invalid/"), text(selector)],
        );
        assert_eq!(code_of(&r), "E_BAD_ARG", "selector {selector:?}");
    }
}

// web_websocket is implemented; its contracts, against a loopback server, are
// in tests/web_websocket_contracts.rs.
#[test]
fn xpath_refuses_what_it_does_not_support_and_bad_arity() {
    // The supported subset is tested in tests/web_xpath_contracts.rs.
    let r = call("web_xpath", vec![text("<p>x</p>"), text("count(//p)")]);
    assert_eq!(code_of(&r), "E_BAD_ARG", "web_xpath: {r:?}");
    let r = call("web_xpath", vec![text("<p>x</p>")]);
    assert_eq!(code_of(&r), "E_BAD_ARG");
}

// ── loopback server and `ae` child ──────────────────────────────────────

/// One scripted answer from the loopback server.
#[derive(Clone)]
struct Reply {
    status: u16,
    /// Extra header lines, each ending in CRLF.
    extra: &'static str,
    body: Vec<u8>,
}

fn reply(status: u16, extra: &'static str, body: &[u8]) -> Reply {
    Reply {
        status,
        extra,
        body: body.to_vec(),
    }
}

struct Loopback {
    port: u16,
    seen: Arc<Mutex<Vec<String>>>,
    handle: JoinHandle<()>,
}

impl Loopback {
    /// Wait for the server thread and return what it received, one entry per
    /// request, each the request line, headers and body as sent.
    fn finish(self) -> Vec<String> {
        let _ = self.handle.join();
        let seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        seen.clone()
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Read one request: its head, then as much body as its Content-Length
/// announces, to a bounded limit.
fn read_request(stream: &mut TcpStream) -> String {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() > 64 * 1024 {
            break buf.len();
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break buf.len(),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end.min(buf.len())]).into_owned();
    let length = head
        .lines()
        .find_map(|line| {
            let (k, v) = line.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0)
        .min(1 << 20);
    while buf.len() < head_end + length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    let end = (head_end + length).min(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

/// A loopback server that answers `replies` in order, one per connection, and
/// exits after the last one or after the 30 s deadline, whichever is first.
fn serve(replies: Vec<Reply>) -> Loopback {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    let port = listener.local_addr().expect("local addr").port();
    listener.set_nonblocking(true).expect("nonblocking");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_thread = Arc::clone(&seen);
    let handle = std::thread::spawn(move || {
        let deadline = Instant::now() + DEADLINE;
        for reply in replies {
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break Some(stream),
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            break None;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break None,
                }
            };
            let Some(mut stream) = stream else { return };
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            let request = read_request(&mut stream);
            if let Ok(mut seen) = seen_thread.lock() {
                seen.push(request);
            }
            let head = format!(
                "HTTP/1.1 {} Test\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n",
                reply.status,
                reply.body.len(),
                reply.extra
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&reply.body);
            let _ = stream.shutdown(Shutdown::Both);
        }
    });
    Loopback { port, seen, handle }
}

/// A fresh workspace for one test; `AETHER_WORKSPACE` points here, so relative
/// download and upload paths resolve inside it.
fn workspace(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ae_web_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("workspace");
    dir
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Run `expr` through the `ae` binary with a hard deadline, returning its
/// stdout and stderr together with colour codes removed.
fn ae(dir: &Path, expr: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ae"))
        .current_dir(dir)
        .env("AETHER_SESSION_DIR", dir.join("session"))
        .env("AETHER_WORKSPACE", dir)
        .env("AETHER_NET_ALLOW", "127.0.0.1")
        .env_remove("AETHER_MODE")
        .env_remove("AETHER_AGENT")
        .env_remove("AETHER_POLICY")
        .args(["-c", expr])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ae");
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let started = Instant::now();
    loop {
        if child.try_wait().expect("poll ae").is_some() {
            break;
        }
        if started.elapsed() >= DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            panic!("ae ran past 30 s: {expr}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let out = out.join().unwrap_or_default();
    let err = err.join().unwrap_or_default();
    strip_ansi(&format!(
        "{}{}",
        String::from_utf8_lossy(&out),
        String::from_utf8_lossy(&err)
    ))
}

// ── positive behaviour against the loopback server ──────────────────────

#[test]
fn post_sends_a_data_value_that_begins_with_at_sign_literally() {
    // curl's `-d @name` reads a file; the builtin must send the text itself.
    let dir = workspace("post_at");
    let server = serve(vec![reply(200, "", b"ok")]);
    let url = format!("http://127.0.0.1:{}/submit", server.port);
    let out = ae(&dir, &format!(r#"web_post("{url}", "@/etc/passwd")"#));
    let seen = server.finish();
    assert!(out.contains("ok"), "output: {out}");
    assert_eq!(seen.len(), 1);
    assert!(seen[0].ends_with("@/etc/passwd"), "sent: {}", seen[0]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_redirect_to_a_file_url_is_not_followed() {
    let dir = workspace("redir_file");
    let server = serve(vec![reply(302, "Location: file:///etc/hosts\r\n", b"")]);
    let url = format!("http://127.0.0.1:{}/start", server.port);
    let out = ae(
        &dir,
        &format!(r#"(try {{ web_fetch("{url}") }} catch e {{ e.error.code }})"#),
    );
    server.finish();
    assert!(out.contains("E_TOOL_FAILED"), "output: {out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_http_error_status_is_a_coded_failure_not_an_error_page() {
    let dir = workspace("fetch_404");
    let server = serve(vec![reply(404, "", b"<h1>missing</h1>")]);
    let url = format!("http://127.0.0.1:{}/gone", server.port);
    let out = ae(
        &dir,
        &format!(r#"(try {{ web_fetch("{url}") }} catch e {{ e.error.code }})"#),
    );
    server.finish();
    assert!(out.contains("E_TOOL_FAILED"), "output: {out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_body_over_the_cap_is_budget_exceeded_not_truncated() {
    let dir = workspace("fetch_cap");
    let big = vec![b'a'; BODY_MAX + 1];
    let server = serve(vec![reply(200, "", &big)]);
    let url = format!("http://127.0.0.1:{}/big", server.port);
    let out = ae(
        &dir,
        &format!(r#"(try {{ web_fetch("{url}") }} catch e {{ e.error.code }})"#),
    );
    server.finish();
    assert!(out.contains("E_BUDGET_EXCEEDED"), "output: {out}");
    assert!(
        !out.contains("aaaaaaaaaa"),
        "a prefix of the body was returned"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn download_writes_inside_the_workspace_and_reports_true() {
    let dir = workspace("download_ok");
    let server = serve(vec![reply(200, "", b"hello-download")]);
    let url = format!("http://127.0.0.1:{}/file.bin", server.port);
    let out = ae(&dir, &format!(r#"web_download("{url}", "out.txt")"#));
    server.finish();
    assert!(out.contains("true"), "output: {out}");
    let written = std::fs::read_to_string(dir.join("out.txt")).expect("out.txt");
    assert_eq!(written, "hello-download");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_download_is_a_coded_error_and_writes_nothing() {
    let dir = workspace("download_404");
    let server = serve(vec![reply(404, "", b"not here")]);
    let url = format!("http://127.0.0.1:{}/x", server.port);
    let out = ae(
        &dir,
        &format!(r#"(try {{ web_download("{url}", "gone.txt") }} catch e {{ e.error.code }})"#),
    );
    server.finish();
    assert!(out.contains("E_TOOL_FAILED"), "output: {out}");
    assert!(!dir.join("gone.txt").exists(), "a 404 page was saved");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_non_json_response_to_json_post_is_coded() {
    let dir = workspace("json_post");
    let server = serve(vec![reply(200, "", b"not json")]);
    let url = format!("http://127.0.0.1:{}/api", server.port);
    let out = ae(
        &dir,
        &format!(r#"(try {{ web_json_post("{url}", {{a: 1}}) }} catch e {{ e.error.code }})"#),
    );
    let seen = server.finish();
    assert!(out.contains("E_TOOL_FAILED"), "output: {out}");
    assert!(seen[0].contains(r#"{"a":1}"#), "sent: {}", seen[0]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rest_api_sends_its_method_body_and_headers() {
    let dir = workspace("rest");
    let server = serve(vec![reply(200, "", br#"{"ok": true}"#)]);
    let url = format!("http://127.0.0.1:{}/item", server.port);
    let out = ae(
        &dir,
        &format!(r#"web_rest_api("put", "{url}", {{a: 1}}, {{"X-Test": "yes"}}).ok"#),
    );
    let seen = server.finish();
    assert!(out.contains("true"), "output: {out}");
    assert!(seen[0].starts_with("PUT /item"), "request: {}", seen[0]);
    assert!(seen[0].contains("X-Test: yes"), "request: {}", seen[0]);
    assert!(seen[0].contains(r#"{"a":1}"#), "request: {}", seen[0]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headers_describe_the_final_response_after_redirects() {
    let dir = workspace("headers");
    let server = serve(vec![
        reply(302, "Location: /final\r\n", b""),
        reply(200, "X-Final: yes\r\n", b""),
    ]);
    let url = format!("http://127.0.0.1:{}/start", server.port);
    let out = ae(&dir, &format!(r#"web_headers("{url}")"#));
    server.finish();
    assert!(out.contains("x-final"), "output: {out}");
    assert!(
        !out.contains("location"),
        "a redirect hop's header leaked: {out}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cookies_collect_every_set_cookie_across_redirects() {
    let dir = workspace("cookies");
    let server = serve(vec![
        reply(302, "Location: /final\r\nSet-Cookie: a=1; Path=/\r\n", b""),
        reply(200, "Set-Cookie: b=2; Path=/\r\n", b""),
    ]);
    let url = format!("http://127.0.0.1:{}/start", server.port);
    let out = ae(&dir, &format!(r#"web_cookies("{url}")"#));
    server.finish();
    assert!(out.contains("a=1") && out.contains("b=2"), "output: {out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_url_is_true_for_success_and_false_for_an_error_status() {
    let dir = workspace("check");
    let server = serve(vec![reply(404, "", b""), reply(200, "", b"")]);
    let missing = format!("http://127.0.0.1:{}/missing", server.port);
    let present = format!("http://127.0.0.1:{}/present", server.port);
    let out = ae(&dir, &format!(r#"web_check_url("{missing}") == false"#));
    assert!(out.contains("true"), "404 should be false: {out}");
    let out = ae(&dir, &format!(r#"web_check_url("{present}") == true"#));
    assert!(out.contains("true"), "200 should be true: {out}");
    server.finish();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn form_submit_percent_encodes_keys_and_values() {
    let dir = workspace("form");
    let server = serve(vec![reply(200, "", b"thanks")]);
    let url = format!("http://127.0.0.1:{}/form", server.port);
    let out = ae(
        &dir,
        &format!(r#"web_form_submit("{url}", {{a: "1&b=2", c: 3}})"#),
    );
    let seen = server.finish();
    assert!(out.contains("thanks"), "output: {out}");
    assert!(
        seen[0].ends_with("a=1%26b%3D2&c=3"),
        "a value containing & or = was not encoded: {}",
        seen[0]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn upload_file_sends_the_workspace_file() {
    let dir = workspace("upload");
    std::fs::write(dir.join("upload.txt"), "hello-upload").expect("fixture");
    let server = serve(vec![reply(200, "", b"stored")]);
    let url = format!("http://127.0.0.1:{}/up", server.port);
    let out = ae(&dir, &format!(r#"web_upload_file("{url}", "upload.txt")"#));
    let seen = server.finish();
    assert!(out.contains("true"), "output: {out}");
    assert!(seen[0].contains("hello-upload"), "request: {}", seen[0]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scrape_returns_whole_elements_with_nesting_and_no_prefix_matches() {
    let dir = workspace("scrape");
    let html = b"<div><div>in</div></div><abbr>no</abbr><div>b</div>";
    let server = serve(vec![reply(200, "", html)]);
    let url = format!("http://127.0.0.1:{}/page", server.port);
    let out = ae(&dir, &format!(r#"web_scrape("{url}", "div") | len"#));
    server.finish();
    assert!(out.contains('2'), "two div elements expected: {out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn robots_txt_is_fetched_from_the_origin_root() {
    let dir = workspace("robots");
    let server = serve(vec![reply(200, "", b"User-agent: *\n")]);
    let url = format!("http://127.0.0.1:{}/a/page?x=1", server.port);
    let out = ae(&dir, &format!(r#"web_robots_txt("{url}")"#));
    let seen = server.finish();
    assert!(out.contains("User-agent"), "output: {out}");
    assert!(
        seen[0].starts_with("GET /robots.txt"),
        "request: {}",
        seen[0]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn graphql_posts_the_query_and_variables() {
    let dir = workspace("graphql");
    let server = serve(vec![reply(200, "", br#"{"data": {"a": 1}}"#)]);
    let url = format!("http://127.0.0.1:{}/graphql", server.port);
    let out = ae(
        &dir,
        &format!(r#"web_graphql("{url}", "{{ a }}", {{id: 1}}).data.a"#),
    );
    let seen = server.finish();
    assert!(out.contains('1'), "output: {out}");
    assert!(
        seen[0].contains(r#""query":"{ a }""#),
        "request: {}",
        seen[0]
    );
    assert!(
        seen[0].contains(r#""variables":{"id":1}"#),
        "request: {}",
        seen[0]
    );
    let _ = std::fs::remove_dir_all(&dir);
}
