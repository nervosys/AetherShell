//! Contracts for the network and debug tool builtins declared in
//! `src/signature.rs` by the tranche: host_lookup, iperf3_client, mtr_trace,
//! nc_connect, nmap_scan, nmap_quick, socat_relay, wget_download, http_get,
//! strace_cmd, ltrace_cmd, lldb_run, nm_symbols, journalctl, lnav_open,
//! multitail_open, logrotate_run, top_snapshot, timeout_cmd, xargs_exec.
//!
//! Network rule: nothing here contacts a host but 127.0.0.1 port 1, or a
//! loopback listener that a test starts itself. Every refusal is decided before
//! a tool runs. Nothing attaches a tracer to a process this file did not start,
//! reads a system journal, runs nmap against a host, or runs logrotate on a
//! system configuration. Tests that spawn `ae` give it a 30 s deadline and a
//! closed stdin, and the only programs run are this test binary and the
//! `top_snapshot` process table read.

use aethershell::builtins;
use aethershell::env::Env;
use aethershell::value::Value;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, Once};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(30);

// ── isolation and in-process calls ───────────────────────────────────────

fn base() -> PathBuf {
    std::env::temp_dir().join(format!("ae_nettools_{}", std::process::id()))
}

/// The workspace every test uses, set through `AETHER_WORKSPACE` before the first
/// call, so child processes inherit it too.
fn isolate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = std::fs::remove_dir_all(base());
        let ws = base().join("ws");
        std::fs::create_dir_all(&ws).expect("workspace");
        std::env::set_var("AETHER_SESSION_DIR", base().join("session"));
        std::env::set_var("AETHER_WORKSPACE", &ws);
    });
}

fn workspace() -> PathBuf {
    isolate();
    base().join("ws")
}

/// `AETHER_MODE` is process-global, so every in-process call takes this lock.
static MODE_LOCK: Mutex<()> = Mutex::new(());

fn locked() -> std::sync::MutexGuard<'static, ()> {
    MODE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// A call in human mode, the mode in which the coded refusals below are reached.
fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let _guard = locked();
    std::env::remove_var("AETHER_MODE");
    std::env::remove_var("AETHER_AGENT");
    isolate();
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

fn piped(name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let _guard = locked();
    std::env::remove_var("AETHER_MODE");
    std::env::remove_var("AETHER_AGENT");
    isolate();
    let mut env = Env::new();
    builtins::call_with_input(name, args, Some(input), &mut env)
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

/// The first `E_...` code in an error's rendered text. A success is a failure here.
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

fn path_text(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

// ── argument and policy refusals: decided before any tool runs ───────────

#[test]
fn malformed_arguments_are_refused_as_bad_arg_before_any_tool_runs() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("host_lookup", vec![]),
        ("host_lookup", vec![text("-x")]),
        ("host_lookup", vec![Value::Int(5)]),
        ("mtr_trace", vec![text("-x")]),
        ("mtr_trace", vec![text("127.0.0.1"), Value::Int(0)]),
        ("nc_connect", vec![text("-e"), Value::Int(1)]),
        ("nc_connect", vec![text("127.0.0.1"), Value::Int(0)]),
        ("nc_connect", vec![text("127.0.0.1")]),
        ("iperf3_client", vec![text("-x")]),
        ("iperf3_client", vec![text("127.0.0.1"), Value::Int(70000)]),
        ("nmap_quick", vec![text("-x")]),
        ("nmap_scan", vec![text("-x")]),
        ("nmap_scan", vec![text("x"), Value::Int(3)]),
        ("socat_relay", vec![text("-x"), text("TCP:127.0.0.1:1")]),
        (
            "socat_relay",
            vec![text("TCP:127.0.0.1:0"), text("TCP:127.0.0.1:1")],
        ),
        (
            "socat_relay",
            vec![text("TCP:a:b:1"), text("TCP:127.0.0.1:1")],
        ),
        (
            "socat_relay",
            vec![text("TCP:127.0.0.1:1,fork"), text("TCP:127.0.0.1:1")],
        ),
        (
            "wget_download",
            vec![text("ftp://example.invalid/f"), text("f.txt")],
        ),
        ("wget_download", vec![text("-x")]),
        (
            "wget_download",
            vec![text("http://example.invalid/f"), text("-o")],
        ),
        ("http_get", vec![Value::Int(5)]),
        ("http_get", vec![text("file:///etc/hosts")]),
        ("strace_cmd", vec![]),
        ("strace_cmd", vec![text("-p 1")]),
        ("strace_cmd", vec![text("   ")]),
        ("ltrace_cmd", vec![text("-p 1")]),
        ("lldb_run", vec![text("-x")]),
        ("lldb_run", vec![text("")]),
        ("nm_symbols", vec![text("-x")]),
        ("nm_symbols", vec![text("")]),
        ("journalctl", vec![Value::Int(0)]),
        ("journalctl", vec![Value::Int(5), text("-x")]),
        ("lnav_open", vec![text("-x")]),
        ("multitail_open", vec![text("-y"), text("x")]),
        ("multitail_open", vec![]),
        ("logrotate_run", vec![text("-x")]),
        ("logrotate_run", vec![text("x"), text("--force")]),
        ("top_snapshot", vec![Value::Int(0)]),
        ("top_snapshot", vec![Value::Int(10001)]),
        ("top_snapshot", vec![Value::Int(1), Value::Int(2)]),
        ("timeout_cmd", vec![Value::Int(0), text("x")]),
        ("timeout_cmd", vec![Value::Int(1), text("-x")]),
        ("timeout_cmd", vec![Value::Int(1)]),
        (
            "xargs_exec",
            vec![text("-x"), Value::Array(vec![text("a")])],
        ),
        ("xargs_exec", vec![text("x"), Value::Int(5)]),
        (
            "xargs_exec",
            vec![text("x"), Value::Array(vec![text("-y")])],
        ),
    ];
    for (name, args) in cases {
        let r = call(name, args);
        assert_eq!(code_of(&r), "E_BAD_ARG", "{name} must refuse as E_BAD_ARG");
    }
}

#[test]
fn a_journal_read_refuses_its_arguments_on_every_os_and_is_unimplemented_off_linux() {
    // Argument checks run on every OS, so this never reads a journal.
    assert_eq!(
        code_of(&call("journalctl", vec![Value::Int(0)])),
        "E_BAD_ARG"
    );
    // Windows and macOS are deliberate further backends: the call reads the
    // System event log or the unified log (signature.rs, journalctl), so it
    // is not unimplemented there.
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        assert_eq!(
            code_of(&call(
                "journalctl",
                vec![Value::Int(5), text("sshd.service")]
            )),
            "E_UNIMPLEMENTED"
        );
    }
}

#[test]
fn every_tranche_builtin_refuses_piped_input_it_does_not_document() {
    let names = [
        "host_lookup",
        "mtr_trace",
        "nc_connect",
        "iperf3_client",
        "nmap_quick",
        "nmap_scan",
        "socat_relay",
        "wget_download",
        "strace_cmd",
        "ltrace_cmd",
        "lldb_run",
        "nm_symbols",
        "journalctl",
        "lnav_open",
        "multitail_open",
        "logrotate_run",
        "top_snapshot",
        "timeout_cmd",
    ];
    for name in names {
        let r = piped(name, vec![text("x")], text("piped"));
        assert!(
            r.is_err(),
            "{name} accepted piped input it does not document"
        );
    }
}

#[test]
fn http_get_takes_a_piped_url_and_refuses_it_when_it_is_not_http() {
    let r = piped("http_get", vec![], text("file:///etc/hosts"));
    assert_eq!(code_of(&r), "E_BAD_ARG");
}

#[test]
fn http_get_refuses_loopback_and_private_targets_before_any_request() {
    for url in ["http://127.0.0.1:1/", "http://localhost:1/"] {
        let r = call("http_get", vec![text(url)]);
        assert_eq!(code_of(&r), "E_POLICY_DENY", "{url}");
    }
}

#[test]
fn socat_refuses_every_program_file_and_listener_address() {
    let refused = [
        ("EXEC:/bin/sh", "TCP:127.0.0.1:1"),
        ("SYSTEM:id", "TCP:127.0.0.1:1"),
        ("SHELL:sh", "TCP:127.0.0.1:1"),
        ("OPEN:/etc/passwd", "TCP:127.0.0.1:1"),
        ("TCP-LISTEN:1", "TCP:127.0.0.1:1"),
        ("TCP:127.0.0.1:1", "CREATE:x"),
        ("UDP-LISTEN:1", "STDIO"),
    ];
    for (a, b) in refused {
        let r = call("socat_relay", vec![text(a), text(b)]);
        assert_eq!(code_of(&r), "E_POLICY_DENY", "{a} / {b}");
    }
}

#[test]
fn nmap_refuses_scripts_output_files_and_unknown_options() {
    let cases: Vec<(Value, &str)> = vec![
        (
            Value::Record(
                [("flags".to_string(), text("--script=vuln"))]
                    .into_iter()
                    .collect(),
            ),
            "E_POLICY_DENY",
        ),
        (
            Value::Record(
                [("flags".to_string(), text("-oN /tmp/x"))]
                    .into_iter()
                    .collect(),
            ),
            "E_POLICY_DENY",
        ),
        (
            Value::Record([("script".to_string(), text("vuln"))].into_iter().collect()),
            "E_BAD_ARG",
        ),
        (
            Value::Record([("ports".to_string(), text("70000"))].into_iter().collect()),
            "E_BAD_ARG",
        ),
        (
            Value::Record([("ports".to_string(), text("9-1"))].into_iter().collect()),
            "E_BAD_ARG",
        ),
    ];
    for (options, want) in cases {
        let r = call("nmap_scan", vec![text("x"), options]);
        assert_eq!(code_of(&r), want);
    }
}

#[test]
fn logrotate_refuses_paths_outside_the_workspace_and_program_directives() {
    let ws = workspace();
    let outside_dir = base().join("outside");
    std::fs::create_dir_all(&outside_dir).expect("outside dir");
    let outside = outside_dir.join("system.conf");
    std::fs::write(&outside, "daily\n").expect("outside config");
    let r = call("logrotate_run", vec![text(&path_text(&outside))]);
    assert_eq!(code_of(&r), "E_OUTSIDE_WORKSPACE");

    let missing = call("logrotate_run", vec![text("no-such.conf")]);
    assert_eq!(code_of(&missing), "E_NOT_FOUND");

    let directives = [
        "/var/log/x.log {\n  daily\n  postrotate\n    id\n  endscript\n}\n",
        "logs/app.log {\n  daily\n  mail ops@example.invalid\n}\n",
        "../escape.log {\n  daily\n}\n",
        "logs/app.log {\n  daily\n}\ninclude /etc/logrotate.d\n",
    ];
    for (i, body) in directives.iter().enumerate() {
        let conf = ws.join(format!("case{i}.conf"));
        std::fs::write(&conf, body).expect("write config");
        let r = call("logrotate_run", vec![text(&format!("case{i}.conf"))]);
        assert_eq!(code_of(&r), "E_POLICY_DENY", "case {i}");
    }
}

#[test]
fn lnav_and_multitail_are_refused_without_a_terminal_and_never_hang() {
    // The child has a closed stdin, so it has no terminal, and must answer
    // E_NO_UI within the deadline rather than start an interactive viewer.
    let ws = workspace();
    let log = ws.join("app.log");
    std::fs::write(&log, "one\ntwo\n").expect("log");
    for expr in [
        format!("lnav_open(\"{}\")", path_text(&log)),
        format!("multitail_open(\"{}\")", path_text(&log)),
    ] {
        let out = ae(&ws, &expr, None);
        assert!(out.contains("E_NO_UI"), "{expr} did not refuse: {out}");
    }
}

// ── missing tools are coded, not uncoded strings ──────────────────────────

#[test]
fn a_missing_tool_is_e_tool_missing_under_an_empty_path() {
    let ws = workspace();
    let empty = base().join("empty-path");
    std::fs::create_dir_all(&empty).expect("empty path dir");
    let cases: Vec<(&str, &[&str])> = vec![
        ("mtr_trace(\"127.0.0.1\")", &["E_TOOL_MISSING"]),
        ("nc_connect(\"127.0.0.1\", 1)", &["E_TOOL_MISSING"]),
        ("iperf3_client(\"127.0.0.1\", 1)", &["E_TOOL_MISSING"]),
        ("nmap_quick(\"127.0.0.1\")", &["E_TOOL_MISSING"]),
        (
            "socat_relay(\"TCP:127.0.0.1:1\", \"TCP:127.0.0.1:1\")",
            &["E_TOOL_MISSING"],
        ),
        (
            "wget_download(\"http://127.0.0.1:1/x\", \"x.txt\")",
            &["E_TOOL_MISSING", "E_TOOL_FAILED"],
        ),
        ("strace_cmd(\"ls\")", &["E_TOOL_MISSING"]),
        ("ltrace_cmd(\"ls\")", &["E_TOOL_MISSING"]),
        ("xargs_exec(\"ls\", [\"x\"])", &["E_TOOL_MISSING"]),
        ("timeout_cmd(5, \"ls\")", &["E_TOOL_MISSING"]),
    ];
    for (expr, codes) in cases {
        let out = ae(&ws, expr, Some(empty.as_path()));
        assert!(
            codes.iter().any(|c| out.contains(c)),
            "{expr}: expected one of {codes:?}, got: {out}"
        );
    }
}

// ── real behaviour where the host allows it, against port 1 or a listener ─

#[test]
fn nc_connect_reports_a_closed_loopback_port_as_not_open() {
    let r = call("nc_connect", vec![text("127.0.0.1"), Value::Int(1)]);
    match r {
        Ok(Value::Record(rec)) => {
            assert!(matches!(rec.get("open"), Some(Value::Bool(false))));
            assert!(matches!(rec.get("port"), Some(Value::Int(1))));
        }
        other => assert_eq!(code_of(&other), "E_TOOL_MISSING"),
    }
}

#[test]
fn iperf3_to_a_closed_loopback_port_is_a_coded_failure() {
    let r = call("iperf3_client", vec![text("127.0.0.1"), Value::Int(1)]);
    let code = code_of(&r);
    assert!(
        code == "E_TOOL_FAILED" || code == "E_TOOL_MISSING",
        "got {code}"
    );
}

#[test]
fn wget_download_to_a_closed_loopback_port_leaves_no_file() {
    let ws = workspace();
    let r = call(
        "wget_download",
        vec![text("http://127.0.0.1:1/x.txt"), text("closed.txt")],
    );
    let code = code_of(&r);
    assert!(
        code == "E_TOOL_FAILED" || code == "E_TOOL_MISSING",
        "got {code}"
    );
    assert!(
        !ws.join("closed.txt").exists(),
        "a failed download left a file"
    );
}

#[test]
fn wget_download_writes_a_loopback_response_into_the_workspace() {
    let ws = workspace();
    let server = serve(vec![reply(200, "", b"hello")]);
    let url = format!("http://127.0.0.1:{}/a.txt", server.port);
    let r = call("wget_download", vec![text(&url), text("got.txt")]);
    match &r {
        Ok(Value::Record(rec)) => {
            assert!(matches!(rec.get("size"), Some(Value::Int(5))));
            assert_eq!(
                std::fs::read_to_string(ws.join("got.txt")).expect("downloaded file"),
                "hello"
            );
        }
        _ => assert_eq!(code_of(&r), "E_TOOL_MISSING"),
    }
}

#[test]
fn wget_download_does_not_follow_a_redirect_to_a_file_url() {
    let ws = workspace();
    let server = serve(vec![reply(302, "Location: file:///etc/hosts\r\n", b"")]);
    let url = format!("http://127.0.0.1:{}/r", server.port);
    let r = call("wget_download", vec![text(&url), text("redirected.txt")]);
    let code = code_of(&r);
    assert!(
        code == "E_TOOL_FAILED" || code == "E_TOOL_MISSING",
        "a file redirect was not refused: {code}"
    );
    assert!(
        !ws.join("redirected.txt").exists(),
        "a redirect wrote a file"
    );
}

#[test]
fn wget_download_refuses_an_existing_output_without_touching_it() {
    let ws = workspace();
    std::fs::write(ws.join("keep.txt"), "original").expect("existing output");
    let r = call(
        "wget_download",
        vec![text("http://127.0.0.1:1/x"), text("keep.txt")],
    );
    assert_eq!(code_of(&r), "E_BAD_ARG");
    assert_eq!(
        std::fs::read_to_string(ws.join("keep.txt")).expect("kept"),
        "original"
    );
}

#[test]
fn top_snapshot_returns_at_most_the_requested_count_with_typed_fields() {
    let r = call("top_snapshot", vec![Value::Int(3)]);
    match r {
        Ok(Value::Array(rows)) => {
            assert!(!rows.is_empty() && rows.len() <= 3, "{} rows", rows.len());
            for row in rows {
                let Value::Record(rec) = row else {
                    panic!("a snapshot row is not a Record");
                };
                assert!(matches!(rec.get("pid"), Some(Value::Int(_))));
                assert!(matches!(rec.get("cpu_percent"), Some(Value::Float(_))));
            }
        }
        other => panic!("top_snapshot(3) did not return an Array: {other:?}"),
    }
}

#[test]
fn nm_reads_a_non_object_file_as_a_coded_failure() {
    let ws = workspace();
    let text_file = ws.join("not-an-object.txt");
    std::fs::write(&text_file, "plain text, not an object file\n").expect("text file");
    let r = call("nm_symbols", vec![text(&path_text(&text_file))]);
    let code = code_of(&r);
    assert!(
        code == "E_TOOL_FAILED" || code == "E_TOOL_MISSING",
        "got {code}"
    );
    let missing = call("nm_symbols", vec![text("no-such-object.o")]);
    assert_eq!(code_of(&missing), "E_NOT_FOUND");
}

#[test]
fn lldb_and_tracers_refuse_missing_programs_as_not_found() {
    let r = call("lldb_run", vec![text("no-such-program")]);
    assert_eq!(code_of(&r), "E_NOT_FOUND");
}

#[test]
fn xargs_runs_this_test_binary_once_per_item_without_a_shell() {
    let exe = std::env::current_exe().expect("test binary");
    let exe_text = path_text(&exe);
    if exe_text.contains(' ') {
        return;
    }
    // libtest's --list exits 0; the item is passed as its filter, never through a shell.
    let r = call(
        "xargs_exec",
        vec![
            text(&format!("{exe_text} --list")),
            Value::Array(vec![text("no_test_has_this_name")]),
        ],
    );
    match r {
        Ok(Value::Array(rows)) => {
            assert_eq!(rows.len(), 1);
            let Value::Record(rec) = &rows[0] else {
                panic!("a per-item result is not a Record");
            };
            assert!(matches!(rec.get("exit_code"), Some(Value::Int(0))));
        }
        other => panic!("xargs_exec did not run the binary: {other:?}"),
    }
}

#[test]
fn xargs_refuses_more_items_than_its_limit() {
    let items: Vec<Value> = (0..257).map(|i| text(&format!("i{i}"))).collect();
    let r = call("xargs_exec", vec![text("ls"), Value::Array(items)]);
    assert_eq!(code_of(&r), "E_BUDGET_EXCEEDED");
}

#[test]
fn timeout_runs_this_test_binary_and_returns_its_exit_code() {
    let exe = std::env::current_exe().expect("test binary");
    let exe_text = path_text(&exe);
    if exe_text.contains(' ') {
        return;
    }
    let r = call(
        "timeout_cmd",
        vec![Value::Int(60), text(&format!("{exe_text} --list"))],
    );
    match r {
        Ok(Value::Record(rec)) => {
            assert!(matches!(rec.get("exit_code"), Some(Value::Int(0))));
        }
        other => panic!("timeout_cmd did not return a record: {other:?}"),
    }
}

// ── loopback server the tests start and stop ────────────────────────────

struct Reply {
    status: u16,
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
    #[allow(dead_code)]
    handle: JoinHandle<()>,
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn read_request(stream: &mut TcpStream) {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    while find(&buf, b"\r\n\r\n").is_none() && buf.len() <= 64 * 1024 {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
}

/// Answer `replies` in order, one per connection, on 127.0.0.1 with an
/// ephemeral port, and stop after the last or at the deadline.
fn serve(replies: Vec<Reply>) -> Loopback {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    let port = listener.local_addr().expect("local addr").port();
    listener.set_nonblocking(true).expect("nonblocking");
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
            read_request(&mut stream);
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
    Loopback { port, handle }
}

// ── `ae` as a child process, with a hard deadline and a closed stdin ─────

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut out);
        }
        out
    })
}

/// Run `expr` through the `ae` binary in `dir`, optionally with `path_dir` as
/// its whole PATH, and return stdout and stderr together.
fn ae(dir: &Path, expr: &str, path_dir: Option<&Path>) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ae"));
    cmd.current_dir(dir)
        .env("AETHER_SESSION_DIR", dir.join("session"))
        .env("AETHER_WORKSPACE", dir)
        .env_remove("AETHER_MODE")
        .env_remove("AETHER_AGENT")
        .env_remove("AETHER_POLICY")
        .args(["-c", expr])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(p) = path_dir {
        cmd.env("PATH", p);
    }
    let mut child = cmd.spawn().expect("spawn ae");
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
    format!(
        "{}{}",
        String::from_utf8_lossy(&out),
        String::from_utf8_lossy(&err)
    )
}
