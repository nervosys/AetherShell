//! Hardening of the `aimodel` HTTP daemon (AS-2026-25, AS-2026-28).
//!
//! Each test starts the real router on a loopback listener and talks to it
//! with raw HTTP/1.1 over `std::net`, so the Host check, the peer check and the
//! rate limiter are exercised as a browser or a shell would reach them. Nothing
//! here connects to a host other than 127.0.0.1, and every file the server
//! writes lands under a temp directory set in `init`.

use iron_shell::ai_api::config::APIConfig;
use iron_shell::ai_api::server::APIServer;
use iron_shell::ai_api::storage::{model_file_stem, url_model_id, validate_model_id};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Once;
use std::time::Duration;

/// Point the model and config directories at a temp tree, once, before any
/// server reads them. Every test calls this first.
fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let base = std::env::var_os("IRONSHELL_SESSION_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("aimodel-hardening-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::env::set_var("XDG_DATA_HOME", base.join("data"));
        std::env::set_var("XDG_CONFIG_HOME", base.join("config"));
        std::env::set_var("IRONSHELL_SESSION_DIR", base.join("session"));
    });
}

/// A config that listens on an ephemeral loopback port.
fn loopback_config() -> APIConfig {
    let mut config = APIConfig::default();
    config.server.host = "127.0.0.1".to_string();
    config.server.port = 0;
    config
}

/// Serve `config` on a fresh loopback port and return its address. The server
/// runs on its own thread and runtime for the life of the test process.
fn serve(config: APIConfig) -> SocketAddr {
    init();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let server = APIServer::new(config).unwrap();
            let app = server.create_router().await.unwrap();
            listener.set_nonblocking(true).unwrap();
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let _ = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });
    });
    addr
}

/// Send one raw request and return the status code and the full response.
/// `Connection: close` in every request makes the server end the stream, so
/// reading to EOF gets the whole response.
fn send(addr: SocketAddr, raw: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream.write_all(raw.as_bytes()).unwrap();
    let mut out = Vec::new();
    let _ = stream.read_to_end(&mut out);
    let text = String::from_utf8_lossy(&out).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    (status, text)
}

fn request(method: &str, path: &str, host: Option<&str>, extra: &[(&str, &str)]) -> String {
    let mut raw = format!("{method} {path} HTTP/1.1\r\n");
    if let Some(host) = host {
        raw.push_str(&format!("Host: {host}\r\n"));
    }
    for (name, value) in extra {
        raw.push_str(&format!("{name}: {value}\r\n"));
    }
    raw.push_str("Connection: close\r\n\r\n");
    raw
}

/// Percent-encode everything outside the unreserved set, so a URL can sit in
/// one path segment.
fn encode_segment(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Run a future to completion on a fresh runtime.
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(fut)
}

#[test]
fn a_foreign_host_header_is_refused() {
    let addr = serve(loopback_config());
    let port = addr.port();

    // A DNS-rebinding page reaches the server under its own name.
    let (status, body) = send(
        addr,
        &request("GET", "/v1/health", Some("evil.example"), &[]),
    );
    assert_eq!(status, 403, "{body}");
    let (status, _) = send(
        addr,
        &request(
            "GET",
            "/v1/storage/stats",
            Some(&format!("evil.example:{port}")),
            &[("X-IronShell-Client", "1")],
        ),
    );
    assert_eq!(
        status, 403,
        "a foreign name must not reach the stats route either"
    );

    // A request with no Host at all is refused too.
    let (status, _) = send(addr, &request("GET", "/v1/health", None, &[]));
    assert_eq!(status, 403);

    // The loopback names are answered.
    for host in [
        format!("localhost:{port}"),
        format!("127.0.0.1:{port}"),
        format!("[::1]:{port}"),
    ] {
        let (status, body) = send(addr, &request("GET", "/v1/health", Some(&host), &[]));
        assert_eq!(status, 200, "Host {host} should be served: {body}");
    }
}

#[test]
fn a_non_loopback_bind_without_keys_refuses_to_start() {
    let mut config = APIConfig::default();
    config.server.host = "0.0.0.0".to_string();
    config.server.port = 0;
    init();
    // If the check were missing, `start` would bind every interface and never
    // return, so the timeout is what turns that regression into a failure.
    let result = block_on(async {
        tokio::time::timeout(Duration::from_secs(10), async {
            APIServer::new(config).unwrap().start().await
        })
        .await
    });
    let err = result
        .expect("start must return, not bind 0.0.0.0")
        .expect_err("a non-loopback bind with no API keys must be refused");
    let text = err.to_string();
    assert!(text.contains("0.0.0.0"), "{text}");
    assert!(text.contains("api_keys"), "{text}");
}

#[test]
fn enable_tls_refuses_to_start() {
    let mut config = loopback_config();
    config.security.enable_tls = true;
    init();
    let result = block_on(async {
        tokio::time::timeout(Duration::from_secs(10), async {
            APIServer::new(config).unwrap().start().await
        })
        .await
    });
    let err = result
        .expect("start must return")
        .expect_err("enable_tls must refuse, since TLS is not implemented");
    assert!(err.to_string().contains("TLS"), "{err}");
}

#[test]
fn a_download_of_a_loopback_url_is_refused() {
    let addr = serve(loopback_config());
    let path = format!(
        "/v1/models/{}/download",
        encode_segment("http://127.0.0.1/")
    );
    let body_json = r#"{"source":"url"}"#;
    let raw = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nX-IronShell-Client: 1\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body_json}",
        body_json.len()
    );
    let (status, body) = send(addr, &raw);
    assert_eq!(status, 400, "{body}");
    assert!(
        body.contains("127.0.0.1") && body.contains("blocked"),
        "the refusal should name the address and say it was blocked: {body}"
    );
}

#[test]
fn a_model_id_that_could_leave_the_directory_is_refused() {
    for bad in [
        "..\\evil",
        "../evil",
        "org/../evil",
        "C:evil",
        "C:\\evil",
        "a\0b",
        ".hidden",
        "a//b",
        "/absolute",
        "trailing/",
        "",
        "has space",
        "x:y",
    ] {
        assert!(
            validate_model_id(bad).is_err(),
            "model id {bad:?} must be refused"
        );
        assert!(
            model_file_stem(bad).is_err(),
            "no file name may be built from {bad:?}"
        );
    }

    assert!(validate_model_id("meta-llama/Llama-2-7b.gguf").is_ok());
    assert_eq!(model_file_stem("org/model-7b").unwrap(), "org_model-7b");

    // A URL-derived id is safe by construction.
    let derived = url_model_id("https://example.com/a/../b/model.gguf").unwrap();
    assert!(validate_model_id(&derived).is_ok(), "{derived}");
    assert!(!derived.contains(".."), "{derived}");
}

#[test]
fn a_model_id_with_dotdot_or_backslash_is_refused_over_http() {
    let addr = serve(loopback_config());

    // A delete naming a traversal id, percent-encoded so it reaches the handler.
    let traversal = encode_segment("..\\evil");
    let (status, body) = send(
        addr,
        &request(
            "DELETE",
            &format!("/v1/models/{traversal}"),
            Some("localhost"),
            &[("X-IronShell-Client", "1")],
        ),
    );
    assert_eq!(status, 400, "{body}");

    // The same id as a download target is refused before any request is made.
    let (status, body) = send(
        addr,
        &request(
            "POST",
            &format!("/v1/models/{traversal}/download"),
            Some("localhost"),
            &[("X-IronShell-Client", "1"), ("Content-Length", "0")],
        ),
    );
    assert_eq!(status, 400, "{body}");
}

#[test]
fn the_rate_limit_answers_429_past_the_window() {
    let mut config = loopback_config();
    config.security.rate_limiting.requests_per_minute = 3;
    config.security.rate_limiting.requests_per_hour = 1000;
    let addr = serve(config);

    for n in 1..=3 {
        let (status, body) = send(addr, &request("GET", "/v1/health", Some("localhost"), &[]));
        assert_eq!(status, 200, "request {n} is within the limit: {body}");
    }
    let (status, body) = send(addr, &request("GET", "/v1/health", Some("localhost"), &[]));
    assert_eq!(status, 429, "the fourth request in the minute: {body}");
    assert!(body.contains("rate_limit_exceeded"), "{body}");
    assert!(
        body.to_ascii_lowercase().contains("retry-after: 60"),
        "{body}"
    );
}

#[test]
fn storage_stats_without_credentials_is_refused() {
    let addr = serve(loopback_config());

    // No key and no client header: refused.
    let (status, body) = send(
        addr,
        &request("GET", "/v1/storage/stats", Some("localhost"), &[]),
    );
    assert_eq!(status, 403, "{body}");

    // The provider validation route discloses whether keys are set, so it is
    // refused the same way.
    let (status, body) = send(
        addr,
        &request(
            "POST",
            "/v1/providers/openai/validate",
            Some("localhost"),
            &[],
        ),
    );
    assert_eq!(status, 403, "{body}");

    // With the client header from a loopback peer, the stats are served. The
    // body carries directories, and no key.
    let (status, body) = send(
        addr,
        &request(
            "GET",
            "/v1/storage/stats",
            Some("localhost"),
            &[("X-IronShell-Client", "1")],
        ),
    );
    assert_eq!(status, 200, "{body}");
}

#[test]
fn a_configured_key_is_checked_against_every_key() {
    let mut config = loopback_config();
    config.security.require_api_key = true;
    config.security.api_keys = vec![
        "first-key-value".to_string(),
        "second-key-value".to_string(),
    ];
    let addr = serve(config);

    for (token, expect) in [
        ("first-key-value", 200),
        ("second-key-value", 200),
        ("first-key-valuX", 401),
        ("nope", 401),
    ] {
        let bearer = format!("Bearer {token}");
        let (status, body) = send(
            addr,
            &request(
                "GET",
                "/v1/storage/stats",
                Some("localhost"),
                &[("Authorization", bearer.as_str())],
            ),
        );
        assert_eq!(status, expect, "token {token}: {body}");
    }
}

// ----- Key output: the aimodel binary never prints a stored key -----
//
// These run the real binary with every config and data path in a temp
// directory. The OS credential store (`aimodel keys`) is not run: it is the
// user's real store and has no override, so it is not exercised here.

/// A fresh home for one binary run, with its config directory created.
fn fresh_home(name: &str) -> PathBuf {
    init();
    let home = std::env::var_os("IRONSHELL_SESSION_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("aimodel-keys-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("config").join("ai-models")).unwrap();
    home
}

/// Every path the binary reads or writes comes from `home`.
fn aimodel(home: &std::path::Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_aimodel"));
    cmd.env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("HOME", home)
        .env("APPDATA", home.join("appdata"))
        .env("IRONSHELL_SESSION_DIR", home.join("session"));
    // The binary reads provider keys from the environment; a developer's real
    // keys must not reach a test run.
    for (name, _) in std::env::vars_os() {
        let upper = name.to_string_lossy().to_ascii_uppercase();
        if upper.ends_with("_API_KEY")
            || upper.ends_with("_TOKEN")
            || upper.starts_with("IRONGATE_")
            || upper.starts_with("AIMODEL_")
        {
            cmd.env_remove(&name);
        }
    }
    cmd
}

/// Write `config` as the binary's config.toml under `home`.
fn write_config(home: &std::path::Path, config: &APIConfig) {
    let path = home.join("config").join("ai-models").join("config.toml");
    std::fs::write(path, toml::to_string_pretty(config).unwrap()).unwrap();
}

/// A config holding two raw keys: one in the server key list, one provider key.
fn config_with_keys() -> (APIConfig, [&'static str; 2]) {
    let raw = ["ak-RAW-SERVER-KEY-7f3a9c2e", "sk-RAW-PROVIDER-KEY-91be04d8"];
    let mut config = APIConfig::default();
    config.security.api_keys = vec![raw[0].to_string()];
    config.providers.openai.api_key = Some(raw[1].to_string());
    (config, raw)
}

/// Run `cmd` with `stdin` piped in and return success and stdout + stderr.
fn run_with_stdin(mut cmd: std::process::Command, stdin: &[u8]) -> (bool, String) {
    use std::process::Stdio;
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A command that does not read stdin may exit first; a broken pipe is fine.
    let _ = child.stdin.take().unwrap().write_all(stdin);
    let out = child.wait_with_output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[test]
fn config_show_prints_no_stored_key() {
    let home = fresh_home("config-show");
    let (config, raw) = config_with_keys();
    write_config(&home, &config);

    let mut cmd = aimodel(&home);
    cmd.args(["config", "show"]);
    let (ok, out) = run_with_stdin(cmd, b"");
    assert!(ok, "{out}");
    assert!(
        out.contains("<redacted>"),
        "keys should be replaced by a marker: {out}"
    );
    for key in raw {
        assert!(!out.contains(key), "a stored key was printed: {out}");
    }
}

#[test]
fn provider_configure_reads_the_key_from_stdin_without_echoing_it() {
    let key = "sk-STDIN-KEY-5d2e8a41c0";
    for (args, input) in [
        (
            vec!["provider", "configure", "openai", "--api-key-stdin"],
            format!("{key}\n"),
        ),
        (
            vec!["provider", "configure", "openai", "--api-key", "-"],
            format!("{key}\n"),
        ),
    ] {
        let home = fresh_home("stdin");
        let mut cmd = aimodel(&home);
        cmd.args(&args);
        let (ok, out) = run_with_stdin(cmd, input.as_bytes());
        assert!(ok, "{args:?}: {out}");
        // The key was read: its length is reported, trimmed of the newline.
        assert!(
            out.contains(&format!("API key provided (length: {})", key.len())),
            "{args:?}: {out}"
        );
        assert!(!out.contains(key), "{args:?} echoed the key: {out}");
        assert!(!out.contains(&key[..10]), "{args:?} echoed a prefix: {out}");
    }
}

#[test]
fn provider_configure_on_the_command_line_warns_and_does_not_echo() {
    let key = "sk-ARGV-KEY-0a9b8c7d6e";
    let home = fresh_home("argv");
    let mut cmd = aimodel(&home);
    cmd.args(["provider", "configure", "openai", "--api-key", key]);
    let (ok, out) = run_with_stdin(cmd, b"");
    assert!(ok, "{out}");
    assert!(
        out.contains("visible to other users"),
        "the argv warning is missing: {out}"
    );
    assert!(!out.contains(key), "the key was echoed: {out}");
}

#[test]
fn server_startup_prints_no_stored_key() {
    use std::io::BufRead;
    use std::process::Stdio;

    let home = fresh_home("startup");
    let (config, raw) = config_with_keys();
    write_config(&home, &config);

    // Port 0: the daemon binds an ephemeral loopback port. It runs in the
    // foreground, so this test reads its startup output and then ends it.
    let mut child = aimodel(&home)
        .args(["server", "--host", "127.0.0.1", "--port", "0"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    for pipe in [Box::new(stdout) as Box<dyn Read + Send>, Box::new(stderr)] {
        let seen = seen.clone();
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(pipe).lines().map_while(Result::ok) {
                let is_banner = line.contains("OpenAPI docs");
                seen.lock().unwrap().push_str(&line);
                seen.lock().unwrap().push('\n');
                if is_banner {
                    let _ = tx.send(());
                }
            }
        });
    }
    let started = rx.recv_timeout(Duration::from_secs(60)).is_ok();
    // Let any later log line arrive before the process is ended.
    std::thread::sleep(Duration::from_secs(1));
    let _ = child.kill();
    let _ = child.wait();
    let out = seen.lock().unwrap().clone();

    assert!(
        started,
        "the server never printed its startup banner: {out}"
    );
    assert!(out.contains("Starting AI Model API server"), "{out}");
    for key in raw {
        assert!(
            !out.contains(key),
            "a stored key was printed at startup: {out}"
        );
    }
}

/// Creating an API key returns the raw key once. Nothing kept or serialized
/// with the record may hold it: not the audit entries, not the stored record.
#[test]
fn api_key_creation_keeps_the_raw_key_out_of_records() {
    use iron_shell::auth::{AuthManager, RbacManager};
    use std::sync::Arc;

    let auth = AuthManager::new(Arc::new(RbacManager::new()));
    let user = auth.register_user("keyuser", "a-password-value").unwrap();
    let (record, raw) = auth.create_api_key(&user.id, "output-test").unwrap();
    assert!(!raw.is_empty());

    let record_json = serde_json::to_string(&record).unwrap();
    assert!(
        !record_json.contains(&raw),
        "the stored record holds the raw key"
    );
    let audit = auth.get_audit_log(100);
    assert!(!audit.is_empty(), "key creation should be audited");
    for entry in &audit {
        let json = serde_json::to_string(entry).unwrap();
        assert!(
            !json.contains(&raw),
            "an audit entry holds the raw key: {json}"
        );
    }
}
