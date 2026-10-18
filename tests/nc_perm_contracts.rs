//! Contracts for nc_listen (one-shot TCP listener) and perm_check's execute
//! mode. Env-mutating tests share one lock; the process env is restored on drop.
use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use std::{
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

static LOCK: Mutex<()> = Mutex::new(());

const ENV_KEYS: [&str; 6] = [
    "NO_COLOR",
    "AETHER_WORKSPACE",
    "AETHER_AUDIT_LOG",
    "AETHER_AUDIT_KEY",
    "AETHER_NET_ALLOW",
    "PATHEXT",
];

struct Fixture {
    root: PathBuf,
    vars: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("ae-nc-perm-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let fixture = Self {
            root,
            vars: ENV_KEYS
                .iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        };
        for key in ENV_KEYS {
            std::env::remove_var(key);
        }
        std::env::set_var("NO_COLOR", "1");
        std::env::set_var("AETHER_WORKSPACE", &fixture.root);
        std::env::set_var("AETHER_AUDIT_LOG", fixture.root.join("audit.log"));
        std::env::set_var("AETHER_AUDIT_KEY", "11".repeat(32));
        fixture
    }

    fn path(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        while aethershell::tx::rollback().is_ok() {}
        for (key, value) in &self.vars {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        if self
            .root
            .canonicalize()
            .is_ok_and(|resolved| resolved == self.root)
        {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

fn s(text: &str) -> Value {
    Value::Str(text.into())
}

fn code(result: anyhow::Result<Value>, expected: &str) {
    let e = result.unwrap_err();
    assert!(e.to_string().contains(expected), "{e}");
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

/// Connects once the listener is bound (retrying while it is not), sends the
/// payload, and closes, which gives the listener its EOF.
fn client(port: u16, payload: Vec<u8>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match TcpStream::connect_timeout(&addr, Duration::from_millis(100)) {
                Ok(stream) => break stream,
                Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => panic!("client never connected: {e}"),
            }
        };
        stream.write_all(&payload).unwrap();
    })
}

fn record(value: Value) -> std::collections::BTreeMap<String, Value> {
    match value {
        Value::Record(fields) => fields.into_iter().collect(),
        other => panic!("expected Record, got {other:?}"),
    }
}

#[test]
fn nc_listen_reads_one_client_to_eof_and_reports_it() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    std::env::set_var("AETHER_NET_ALLOW", "127.0.0.1");
    let port = free_port();
    let sender = client(port, b"hello".to_vec());
    let mut env = Env::default();
    let fields = record(
        call(
            "nc_listen",
            vec![Value::Int(port as i64), edit_timeout(3000)],
            &mut env,
        )
        .unwrap(),
    );
    sender.join().unwrap();
    assert!(matches!(&fields["data"], Value::Str(d) if d == "hello"));
    assert!(matches!(fields["bytes"], Value::Int(5)));
    assert!(matches!(fields["truncated"], Value::Bool(false)));
    assert!(matches!(&fields["peer"], Value::Str(p) if p.starts_with("tcp://127.0.0.1:")));
    // The listener was closed on return, so the port can be bound again.
    assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
}

#[test]
fn nc_listen_truncates_at_the_byte_limit_and_says_so() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    std::env::set_var("AETHER_NET_ALLOW", "127.0.0.1");
    let port = free_port();
    let sender = client(port, b"abcdefghij".to_vec());
    let mut env = Env::default();
    let options = Value::Record(
        [
            ("timeout_ms".to_string(), Value::Int(3000)),
            ("max_bytes".to_string(), Value::Int(4)),
        ]
        .into_iter()
        .collect(),
    );
    let fields = record(
        call(
            "nc_listen",
            vec![Value::Int(port as i64), options],
            &mut env,
        )
        .unwrap(),
    );
    sender.join().unwrap();
    assert!(matches!(&fields["data"], Value::Str(d) if d == "abcd"));
    assert!(matches!(fields["bytes"], Value::Int(4)));
    assert!(matches!(fields["truncated"], Value::Bool(true)));
}

#[test]
fn nc_listen_exact_limit_is_not_truncated() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    std::env::set_var("AETHER_NET_ALLOW", "127.0.0.1");
    let port = free_port();
    let sender = client(port, b"abcd".to_vec());
    let mut env = Env::default();
    let options = Value::Record(
        [
            ("timeout_ms".to_string(), Value::Int(3000)),
            ("max_bytes".to_string(), Value::Int(4)),
        ]
        .into_iter()
        .collect(),
    );
    let fields = record(
        call(
            "nc_listen",
            vec![Value::Int(port as i64), options],
            &mut env,
        )
        .unwrap(),
    );
    sender.join().unwrap();
    assert!(matches!(&fields["data"], Value::Str(d) if d == "abcd"));
    assert!(matches!(fields["truncated"], Value::Bool(false)));
}

#[test]
fn nc_listen_without_a_client_is_a_timeout_in_bounded_time() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    std::env::set_var("AETHER_NET_ALLOW", "127.0.0.1");
    let port = free_port();
    let started = Instant::now();
    let mut env = Env::default();
    code(
        call(
            "nc_listen",
            vec![Value::Int(port as i64), edit_timeout(100)],
            &mut env,
        ),
        "E_IO",
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn nc_listen_bind_address_and_peer_pass_network_policy() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    // Loopback is not allowed by this policy, so the bind is refused before listening.
    std::env::set_var("AETHER_NET_ALLOW", "10.255.255.1");
    let port = free_port();
    let mut env = Env::default();
    assert!(call(
        "nc_listen",
        vec![Value::Int(port as i64), edit_timeout(50)],
        &mut env
    )
    .is_err());
    assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
}

#[test]
fn nc_listen_refuses_bad_arguments_and_pipeline_input() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for args in [
        vec![s("8080")],
        vec![Value::Int(0)],
        vec![Value::Int(65536)],
        vec![Value::Int(-1)],
        vec![Value::Int(8080), s("not a record")],
        vec![
            Value::Int(8080),
            Value::Record([("bogus".to_string(), Value::Int(1))].into_iter().collect()),
        ],
        vec![
            Value::Int(8080),
            Value::Record([("host".to_string(), s("not an ip"))].into_iter().collect()),
        ],
        vec![
            Value::Int(8080),
            Value::Record(
                [("timeout_ms".to_string(), Value::Int(10001))]
                    .into_iter()
                    .collect(),
            ),
        ],
        vec![
            Value::Int(8080),
            Value::Record(
                [("max_bytes".to_string(), Value::Int(0))]
                    .into_iter()
                    .collect(),
            ),
        ],
    ] {
        code(call("nc_listen", args, &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input(
            "nc_listen",
            vec![Value::Int(8080)],
            Some(s("piped")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
}

fn edit_timeout(ms: i64) -> Value {
    Value::Record(
        [("timeout_ms".to_string(), Value::Int(ms))]
            .into_iter()
            .collect(),
    )
}

#[cfg(windows)]
#[test]
fn perm_check_execute_on_windows_follows_pathext() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::fs::write(fixture.path("x.exe"), "bin").unwrap();
    std::fs::write(fixture.path("x.txt"), "text").unwrap();
    std::fs::write(fixture.path("UPPER.EXE"), "bin").unwrap();
    std::fs::create_dir(fixture.path("folder.exe")).unwrap();
    let exec = |path: &str, env: &mut Env| call("perm_check", vec![s(path), s("execute")], env);
    assert!(matches!(
        exec(&fixture.path("x.exe"), &mut env).unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        exec(&fixture.path("UPPER.EXE"), &mut env).unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        exec(&fixture.path("x.txt"), &mut env).unwrap(),
        Value::Bool(false)
    ));
    assert!(matches!(
        exec(&fixture.path("folder.exe"), &mut env).unwrap(),
        Value::Bool(false)
    ));
    // A missing path is still the filesystem error, not false.
    assert!(exec(&fixture.path("missing.exe"), &mut env).is_err());
    // PATHEXT decides, and an empty or unset value falls back to the default.
    std::env::set_var("PATHEXT", ".TXT");
    assert!(matches!(
        exec(&fixture.path("x.txt"), &mut env).unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        exec(&fixture.path("x.exe"), &mut env).unwrap(),
        Value::Bool(false)
    ));
    std::env::set_var("PATHEXT", "");
    assert!(matches!(
        exec(&fixture.path("x.exe"), &mut env).unwrap(),
        Value::Bool(true)
    ));
}

#[cfg(windows)]
#[test]
fn windows_execute_is_a_naming_rule_not_an_acl_check() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::fs::write(fixture.path("plain.dat"), "x").unwrap();
    std::env::set_var("PATHEXT", ".DAT");
    assert!(matches!(
        call(
            "perm_check",
            vec![s(&fixture.path("plain.dat")), s("x")],
            &mut env
        )
        .unwrap(),
        Value::Bool(true)
    ));
}

#[cfg(unix)]
#[test]
fn unix_execute_follows_the_mode_bits() {
    use std::os::unix::fs::PermissionsExt;
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let file = fixture.path("prog");
    std::fs::write(&file, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        call("perm_check", vec![s(&file), s("execute")], &mut env).unwrap(),
        Value::Bool(true)
    ));
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(
        call("perm_check", vec![s(&file), s("execute")], &mut env).unwrap(),
        Value::Bool(false)
    ));
}
