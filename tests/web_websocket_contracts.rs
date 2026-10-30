//! Contracts for `web_websocket`, a bounded one-shot WebSocket exchange.
//!
//! Network rule: nothing here contacts any host but 127.0.0.1. Each positive
//! test starts a loopback WebSocket server on its own thread; every server
//! gives up on its own after a deadline, so no test can hang on one.

#![cfg(feature = "native")]

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};
use tungstenite::Message;

/// How long a test server waits for its one client before giving up.
const SERVER_DEADLINE: Duration = Duration::from_secs(20);

fn isolate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_web_websocket_{}", std::process::id()));
        std::env::set_var("IRONSHELL_SESSION_DIR", dir);
    });
}

/// `IRONSHELL_MODE` is process-global. In agent mode a Network builtin is gated
/// before its body runs, so every call here runs in human mode under one lock.
static MODE_LOCK: Mutex<()> = Mutex::new(());

fn call(args: Vec<Value>) -> anyhow::Result<Value> {
    let _guard = MODE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("IRONSHELL_MODE");
    std::env::remove_var("IRONSHELL_AGENT");
    isolate();
    let mut env = Env::new();
    builtins::call("web_websocket", args, &mut env)
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

fn rec(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
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

fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    match v {
        Value::Record(m) => m.get(key).unwrap_or_else(|| panic!("no {key} in {v:?}")),
        other => panic!("expected a Record, got {other:?}"),
    }
}

/// A loopback WebSocket server for one client. `serve` runs on the accepted
/// socket. The thread is detached: a test that never connects leaves it
/// blocked in accept until the test binary exits, and every read and write on
/// an accepted socket times out after `SERVER_DEADLINE`.
fn server(
    serve: impl FnOnce(&mut tungstenite::WebSocket<std::net::TcpStream>) + Send + 'static,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    let port = listener.local_addr().expect("local addr").port();
    std::thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        let _ = stream.set_read_timeout(Some(SERVER_DEADLINE));
        let _ = stream.set_write_timeout(Some(SERVER_DEADLINE));
        if let Ok(mut ws) = tungstenite::accept(stream) {
            serve(&mut ws);
        }
    });
    format!("ws://127.0.0.1:{port}/socket")
}

#[test]
fn an_echo_server_answers_each_sent_message() {
    let url = server(|ws| {
        for _ in 0..2 {
            match ws.read() {
                Ok(m @ Message::Text(_)) => {
                    let _ = ws.send(m);
                }
                _ => return,
            }
        }
        // Keep the socket open until the client closes it.
        while ws.read().is_ok() {}
    });
    let out = call(vec![
        text(&url),
        Value::Array(vec![text("a"), text("b")]),
        rec(&[("max_messages", Value::Int(2))]),
    ])
    .expect("an echo exchange");
    assert_eq!(
        field(&out, "messages"),
        &Value::Array(vec![text("a"), text("b")])
    );
    assert_eq!(field(&out, "closed_by_server"), &Value::Bool(false));
    assert_eq!(field(&out, "timed_out"), &Value::Bool(false));
}

#[test]
fn a_server_that_closes_returns_what_it_sent() {
    let url = server(|ws| {
        for m in ["one", "two", "three"] {
            let _ = ws.send(Message::text(m));
        }
        let _ = ws.close(None);
        while ws.read().is_ok() {}
    });
    let out = call(vec![text(&url)]).expect("a closing server");
    assert_eq!(
        field(&out, "messages"),
        &Value::Array(vec![text("one"), text("two"), text("three")])
    );
    assert_eq!(field(&out, "closed_by_server"), &Value::Bool(true));
    assert_eq!(field(&out, "timed_out"), &Value::Bool(false));
}

#[test]
fn a_binary_frame_comes_back_as_base64() {
    let url = server(|ws| {
        let _ = ws.send(Message::binary(vec![0u8, 1, 2, 255]));
        let _ = ws.close(None);
        while ws.read().is_ok() {}
    });
    let out = call(vec![text(&url)]).expect("a binary frame");
    assert_eq!(
        field(&out, "messages"),
        &Value::Array(vec![rec(&[("binary", text("AAEC/w=="))])])
    );
}

#[test]
fn a_silent_server_times_out_with_an_answer_not_an_error() {
    let url = server(|ws| while ws.read().is_ok() {});
    let started = Instant::now();
    let out = call(vec![
        text(&url),
        Value::Null,
        rec(&[("timeout_secs", Value::Int(1))]),
    ])
    .expect("a timed-out exchange is an answer");
    let took = started.elapsed();
    assert_eq!(field(&out, "messages"), &Value::Array(vec![]));
    assert_eq!(field(&out, "timed_out"), &Value::Bool(true));
    assert!(took < Duration::from_secs(5), "took {took:?}");
}

#[test]
fn more_than_eight_mib_is_a_budget_error() {
    let url = server(|ws| {
        let chunk = "x".repeat(1024 * 1024);
        for _ in 0..9 {
            if ws.send(Message::text(chunk.clone())).is_err() {
                return;
            }
        }
        while ws.read().is_ok() {}
    });
    let r = call(vec![
        text(&url),
        Value::Null,
        rec(&[("timeout_secs", Value::Int(20))]),
    ]);
    assert_eq!(code_of(&r), "E_BUDGET_EXCEEDED");
}

#[test]
fn nothing_listening_is_a_tool_failure() {
    // Bind and drop: the port is very likely free and refuses connections.
    let port = TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .expect("bind")
        .port();
    let r = call(vec![text(&format!("ws://127.0.0.1:{port}/"))]);
    assert_eq!(code_of(&r), "E_TOOL_FAILED");
}

#[test]
fn bad_calls_are_refused_before_anything_is_opened() {
    let url = "ws://127.0.0.1:9/";
    for args in [
        vec![],
        vec![Value::Int(1)],
        vec![text("http://127.0.0.1:9/")],
        vec![text("not a url")],
        vec![text(url), Value::Int(5)],
        vec![text(url), Value::Array(vec![Value::Int(1)])],
        vec![text(url), Value::Null, Value::Int(1)],
        vec![
            text(url),
            Value::Null,
            rec(&[("max_messages", Value::Int(0))]),
        ],
        vec![
            text(url),
            Value::Null,
            rec(&[("max_messages", Value::Int(1001))]),
        ],
        vec![
            text(url),
            Value::Null,
            rec(&[("timeout_secs", Value::Int(121))]),
        ],
        vec![text(url), Value::Null, rec(&[("timeout_secs", text("1"))])],
        vec![text(url), Value::Null, rec(&[("colour", Value::Int(1))])],
        vec![text(url), Value::Null, Value::Null, Value::Null],
    ] {
        let r = call(args.clone());
        assert_eq!(code_of(&r), "E_BAD_ARG", "args {args:?}");
    }
}

#[test]
fn piped_input_is_refused() {
    isolate();
    let mut env = Env::new();
    let r = builtins::call_with_input(
        "web_websocket",
        vec![text("ws://127.0.0.1:9/")],
        Some(text("piped")),
        &mut env,
    );
    assert_eq!(code_of(&r), "E_BAD_ARG");
}
