//! Contracts for the SSRF check on outbound HTTP (audit AS-2026-24).
//!
//! The check is `security::check_http_url`, which every curl transfer goes
//! through (`web_curl_once`, and the hop loop in `wget_download`) and which
//! `http_get`, the workflow HTTP step and the marketplace hops also use.
//!
//! Network rule: the table tests use IP literals, which are checked without a
//! lookup and never connected to. The agent-mode tests start a loopback listener
//! and nothing else; each one asserts that the listener saw no connection.
//! Nothing here resolves a name except `.invalid`, which cannot resolve.

use iron_shell::safety::guard_redirect;
use iron_shell::security::{check_http_url, validate_http_url};
use std::io::{ErrorKind, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Once;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(30);

/// Scratch space for the audit log, so nothing lands in the real session dir.
fn isolate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_ssrf_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        std::env::set_var("IRONSHELL_SESSION_DIR", &dir);
        std::env::remove_var("IRONSHELL_NET_ALLOW");
        std::env::remove_var("IRONSHELL_TEST_ALLOW_LOOPBACK");
    });
}

/// Hosts that must be refused. Every one is an IP literal (or a name that
/// `url` or the local-name list maps to this machine), so no lookup happens.
#[test]
fn internal_hosts_in_every_spelling_are_refused() {
    isolate();
    let refused = [
        // IPv4-mapped IPv6 (::ffff:a.b.c.d)
        "http://[::ffff:127.0.0.1]/",
        "http://[::ffff:169.254.169.254]/",
        "http://[::ffff:10.0.0.1]/",
        "http://[::ffff:100.64.0.1]/",
        // IPv4-compatible IPv6 (::a.b.c.d)
        "http://[::127.0.0.1]/",
        "http://[::10.0.0.1]/",
        // NAT64 well-known prefix, embedded address internal
        "http://[64:ff9b::7f00:1]/",
        "http://[64:ff9b::a00:1]/",
        // 6to4, embedded address internal
        "http://[2002:7f00:1::]/",
        "http://[2002:c0a8:101::]/",
        // IPv4 ranges added to the check
        "http://100.64.0.1/",
        "http://0.0.0.0/",
        "http://0.1.2.3/",
        "http://192.0.0.1/",
        "http://198.18.0.1/",
        "http://224.0.0.1/",
        "http://240.0.0.1/",
        // IPv6 ranges: site-local, unique local, link local, multicast
        "http://[fec0::1]/",
        "http://[fd00::1]/",
        "http://[fe80::1]/",
        "http://[ff02::1]/",
        // Non-canonical IPv4 spellings. `url` normalises them to dotted form.
        "http://2130706433/",
        "http://0x7f.1/",
        "http://0177.0.0.1/",
        // Loopback names, the metadata address and the unspecified address
        "http://localhost/",
        "http://127.0.0.1/",
        "http://169.254.169.254/",
        "http://[::1]/",
    ];
    for url in refused {
        let err = validate_http_url("web_fetch", url)
            .err()
            .unwrap_or_else(|| panic!("{url} was accepted"));
        let msg = format!("{err:#}");
        assert!(msg.contains("SSRF protection"), "{url}: {msg}");
    }
}

/// Public addresses, in plain and embedded form, pass, and the check hands back
/// no pin for an IP literal because there is nothing to resolve.
#[test]
fn public_literal_hosts_are_not_treated_as_internal() {
    isolate();
    let public = [
        "http://8.8.8.8/",
        "http://[::ffff:8.8.8.8]/",
        "http://[::8.8.8.8]/",
        "http://[64:ff9b::808:808]/",
        "http://[2002:808:808::]/",
        "http://[2606:4700::1]/",
    ];
    for url in public {
        let checked =
            check_http_url("web_fetch", url).unwrap_or_else(|e| panic!("{url} refused: {e:#}"));
        assert_eq!(checked.url, url);
        assert!(checked.pin().is_none(), "{url}: an IP literal has no pin");
        assert!(checked.curl_resolve().is_none(), "{url}: no --resolve");
    }
}

/// A redirect hop is judged by the same check. The origin is public, so the
/// guard's allowlist does not apply (`IRONSHELL_NET_ALLOW` is unset).
#[test]
fn redirect_hops_are_judged_like_the_first_url() {
    isolate();
    for to in [
        "http://[::ffff:127.0.0.1]:9/",
        "http://169.254.169.254/",
        "http://2130706433/",
        "http://[64:ff9b::7f00:1]/",
        "http://[2002:7f00:1::]/",
        "http://100.64.0.1/",
    ] {
        let r = guard_redirect("http_get", "http://8.8.8.8/", to);
        assert!(
            r.is_err(),
            "a public origin redirected to {to} was accepted"
        );
    }
    // A hop that begins on a loopback address is judged too: the origin's own
    // address is no reason to let the next hop reach the metadata service.
    let r = guard_redirect("http_get", "http://127.0.0.1:9/", "http://169.254.169.254/");
    assert!(
        r.is_err(),
        "a loopback origin redirected to metadata was accepted"
    );
    // A public target is accepted and comes back as the checked URL.
    let ok = guard_redirect("http_get", "http://8.8.8.8/", "http://8.8.4.4/next")
        .expect("a public redirect target");
    assert_eq!(ok.url, "http://8.8.4.4/next");
    assert!(ok.pin().is_none());
}

/// Run `ish -c` in agent mode with the network approval granted, and no
/// allowlist, so the only thing that can refuse the request is the SSRF check.
fn ish_agent(dir: &Path, expr: &str) -> String {
    ish_agent_with(dir, expr, false)
}

/// As [`ish_agent`]; `seam` turns on the loopback test seam for this child only.
fn ish_agent_with(dir: &Path, expr: &str, seam: bool) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ish"));
    command
        .current_dir(dir)
        .env("IRONSHELL_SESSION_DIR", dir.join("session"))
        .env("IRONSHELL_WORKSPACE", dir)
        .env("IRONSHELL_MODE", "agent")
        .env("IRONSHELL_APPROVE_ALL", "1")
        .env_remove("IRONSHELL_NET_ALLOW")
        .env_remove("IRONSHELL_POLICY");
    if seam {
        command.env("IRONSHELL_TEST_ALLOW_LOOPBACK", "1");
    } else {
        command.env_remove("IRONSHELL_TEST_ALLOW_LOOPBACK");
    }
    let mut child = command
        .args(["-c", expr])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ish");
    let started = Instant::now();
    loop {
        if child.try_wait().expect("poll ish").is_some() {
            break;
        }
        if started.elapsed() >= DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            panic!("ish ran past 30 s: {expr}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut text = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut text);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut text);
    }
    text
}

/// A loopback listener that must receive nothing. Returns true when no
/// connection was made.
fn no_connection(listener: &TcpListener) -> bool {
    match listener.accept() {
        Err(e) => e.kind() == ErrorKind::WouldBlock,
        Ok(_) => false,
    }
}

#[test]
fn agent_mode_with_approval_refuses_loopback_before_any_connection() {
    isolate();
    let dir: PathBuf = std::env::temp_dir().join(format!("ae_ssrf_agent_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("session")).expect("session dir");
    let exprs = [
        "web_fetch(\"http://127.0.0.1:{P}/x\")",
        "web_fetch(\"http://[::ffff:127.0.0.1]:{P}/x\")",
        "http_get(\"http://127.0.0.1:{P}/x\")",
        "http_get(\"http://[::ffff:127.0.0.1]:{P}/x\")",
        "web_check_url(\"http://127.0.0.1:{P}/x\")",
    ];
    for template in exprs {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
        listener.set_nonblocking(true).expect("nonblocking");
        let port = listener.local_addr().expect("addr").port();
        let expr = template.replace("{P}", &port.to_string());
        let text = ish_agent(&dir, &expr);
        assert!(
            text.contains("SSRF protection"),
            "{expr}: not refused by the SSRF check: {text}"
        );
        assert!(no_connection(&listener), "{expr}: the listener was reached");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// With the test seam on, a loopback name is checked, pinned to every address
/// it resolved to, and reached through that pin. The listener is on 127.0.0.1;
/// whichever address `localhost` lists first, the pinned set has it, so the
/// request reaches the listener through curl's own fallback.
#[test]
fn a_name_is_pinned_to_its_checked_addresses_and_reached() {
    isolate();
    let dir: PathBuf = std::env::temp_dir().join(format!("ae_ssrf_pin_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("session")).expect("session dir");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    let port = listener.local_addr().expect("addr").port();
    listener.set_nonblocking(true).expect("nonblocking");
    let server = std::thread::spawn(move || {
        // Two requests: one through web_fetch (curl) and one through http_get
        // (reqwest), each pinned to the same checked addresses.
        let deadline = Instant::now() + DEADLINE * 2;
        let mut served = 0;
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let mut seen = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut buf) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => seen.extend_from_slice(&buf[..n]),
                        }
                    }
                    let reply = b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\npinned-ok";
                    let _ = std::io::Write::write_all(&mut stream, reply);
                    served += 1;
                    if served == 2 {
                        return true;
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return false;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return false,
            }
        }
    });
    let fetch = format!("web_fetch(\"http://localhost:{port}/x\")");
    let get = format!("http_get(\"http://localhost:{port}/x\")");
    let text_fetch = ish_agent_with(&dir, &fetch, true);
    let text_get = ish_agent_with(&dir, &get, true);
    let reached = server.join().expect("server thread");
    assert!(
        reached,
        "the pinned requests did not both reach the listener"
    );
    assert!(
        text_fetch.contains("pinned-ok"),
        "web_fetch output: {text_fetch}"
    );
    assert!(
        text_get.contains("pinned-ok"),
        "http_get output: {text_get}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
