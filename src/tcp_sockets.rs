//! Bounded native TCP operations. Handles belong to this process and must be closed.
use super::guard_network;
use crate::{safety, value::Value};
use anyhow::Result;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

const MAX_HANDLES: usize = 128;
const MAX_BYTES: usize = 1024 * 1024;
struct Socket {
    stream: Mutex<TcpStream>,
    shutdown: TcpStream,
    peer: String,
    timeout: Duration,
    closed: AtomicBool,
}
static SOCKETS: OnceLock<Mutex<HashMap<String, Arc<Socket>>>> = OnceLock::new();
fn registry() -> &'static Mutex<HashMap<String, Arc<Socket>>> {
    SOCKETS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn bad(name: &str, message: &str) -> anyhow::Error {
    safety::arg_err(format!("{name}: {message}"))
}
fn io_error(name: &str, error: std::io::Error) -> anyhow::Error {
    anyhow::Error::new(safety::SafetyError {
        code: safety::ErrorCode::Io, builtin: name.into(),
        message: format!("{name}: {error}"),
        hint: "A failed send may have transmitted a prefix; do not blindly retry. A receive timeout is not EOF.".into(),
        approval: None, did_you_mean: vec![], expected: "successful TCP operation".into(), got: format!("{:?}", error.kind()),
    })
}
fn shape(name: &str, args: &[Value], input: Option<Value>, min: usize, max: usize) -> Result<()> {
    if input.is_some() || args.len() < min || args.len() > max {
        return Err(bad(
            name,
            "wrong argument count or unexpected pipeline input",
        ));
    }
    Ok(())
}
fn string<'a>(name: &str, value: &'a Value) -> Result<&'a str> {
    match value {
        Value::Str(s) => Ok(s),
        _ => Err(bad(name, "expected String")),
    }
}
fn integer(name: &str, value: Option<&Value>, default: i64, max: i64) -> Result<i64> {
    match value {
        None => Ok(default),
        Some(Value::Int(n)) if (1..=max).contains(n) => Ok(*n),
        _ => Err(bad(name, "expected positive bounded Int")),
    }
}
fn transaction(name: &str) -> Result<()> {
    if crate::tx::is_active() {
        return Err(safety::unimplemented(
            name,
            "TCP effects and receive cursors cannot be rolled back",
            "finish the transaction first",
        ));
    }
    Ok(())
}
fn lookup(name: &str, value: &Value) -> Result<(String, Arc<Socket>)> {
    let id = string(name, value)?;
    let sockets = registry()
        .lock()
        .map_err(|_| safety::bad_state(name, "socket registry poisoned", "restart the process"))?;
    let socket = sockets.get(id).cloned().ok_or_else(|| {
        safety::bad_state(
            name,
            "unknown or closed socket handle",
            "use a live handle returned by net_connect or net_listen in this process",
        )
    })?;
    Ok((id.into(), socket))
}
fn register(name: &str, stream: TcpStream, timeout: Duration) -> Result<Value> {
    // Accepted sockets may inherit nonblocking mode on Windows.
    stream
        .set_nonblocking(false)
        .map_err(|e| io_error(name, e))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| io_error(name, e))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| io_error(name, e))?;
    let peer = format!(
        "tcp://{}",
        stream.peer_addr().map_err(|e| io_error(name, e))?
    );
    let shutdown = stream.try_clone().map_err(|e| io_error(name, e))?;
    let mut sockets = registry()
        .lock()
        .map_err(|_| safety::bad_state(name, "socket registry poisoned", "restart the process"))?;
    if sockets.len() >= MAX_HANDLES {
        return Err(safety::budget_exceeded(
            name,
            "128 open socket handles reached".into(),
            "close an existing handle",
        ));
    }
    let id = format!("tcp-{}", uuid::Uuid::new_v4());
    sockets.insert(
        id.clone(),
        Arc::new(Socket {
            stream: Mutex::new(stream),
            shutdown,
            peer,
            timeout,
            closed: AtomicBool::new(false),
        }),
    );
    Ok(Value::Str(id))
}
pub(super) fn guard_and_open(
    name: &str,
    args: Vec<Value>,
    input: Option<Value>,
    listen: bool,
) -> Result<Value> {
    shape(name, &args, input, 2, 3)?;
    let ip = string(name, &args[0])?.parse::<IpAddr>().map_err(|_| {
        bad(
            name,
            "expected literal IPv4 or IPv6 address; resolve hostnames separately",
        )
    })?;
    let port = integer(name, args.get(1), 1, 65535)? as u16;
    let timeout = Duration::from_millis(integer(name, args.get(2), 1000, 10000)? as u64);
    transaction(name)?;
    let addr = SocketAddr::new(ip, port);
    guard_network(name, &format!("tcp://{addr}"))?;
    // Check before opening and again during registration, including concurrent opens.
    if registry()
        .lock()
        .map_err(|_| safety::bad_state(name, "socket registry poisoned", "restart the process"))?
        .len()
        >= MAX_HANDLES
    {
        return Err(safety::budget_exceeded(
            name,
            "128 open socket handles reached".into(),
            "close an existing handle",
        ));
    }
    let io_timeout = safety::bounded_io_timeout(name, timeout)?;
    let stream = if listen {
        let listener = TcpListener::bind(addr).map_err(|e| io_error(name, e))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| io_error(name, e))?;
        let deadline = Instant::now() + io_timeout;
        loop {
            crate::safety::check_deadline()?;
            if Instant::now() >= deadline {
                return Err(io_error(
                    name,
                    std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "no connection accepted before timeout",
                    ),
                ));
            }
            match listener.accept() {
                Ok((stream, peer)) => {
                    // The allowed bind address is not authorization for an arbitrary incoming peer.
                    guard_network(name, &format!("tcp://{peer}"))?;
                    break stream;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(io_error(
                            name,
                            std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "no connection accepted before timeout",
                            ),
                        ));
                    }
                    std::thread::sleep(remaining.min(Duration::from_millis(5)));
                }
                Err(e) => return Err(io_error(name, e)),
            }
        }
    } else {
        TcpStream::connect_timeout(&addr, io_timeout).map_err(|e| io_error(name, e))?
    };
    register(name, stream, timeout)
}
pub(super) fn guard_and_send(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let name = "net_send";
    shape(name, &args, input, 2, 2)?;
    let bytes = match &args[1] {
        Value::Str(s) if s.len() <= MAX_BYTES => s.as_bytes().to_vec(),
        Value::Array(a) if a.len() <= MAX_BYTES => a
            .iter()
            .map(|v| match v {
                Value::Int(n) if (0..=255).contains(n) => Ok(*n as u8),
                _ => Err(bad(name, "bytes must be Ints in 0..255")),
            })
            .collect::<Result<Vec<_>>>()?,
        _ => {
            return Err(bad(
                name,
                "data must be String or byte Array, at most 1 MiB",
            ))
        }
    };
    let (_, socket) = lookup(name, &args[0])?;
    transaction(name)?;
    guard_network(name, &socket.peer)?;
    let mut stream = socket.stream.try_lock().map_err(|_| {
        safety::bad_state(
            name,
            "socket busy or poisoned",
            "finish the other operation or close the handle",
        )
    })?;
    let deadline = Instant::now() + safety::bounded_io_timeout(name, socket.timeout)?;
    let mut sent = 0;
    while sent < bytes.len() {
        if socket.closed.load(Ordering::Acquire) {
            return Err(safety::bad_state(
                name,
                "socket closed during send; a prefix may have been transmitted",
                "use another connection; do not blindly retry",
            ));
        }
        crate::safety::check_deadline()?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io_error(
                name,
                std::io::Error::new(std::io::ErrorKind::TimedOut, "send deadline exceeded"),
            ));
        }
        stream
            .set_write_timeout(Some(remaining.min(Duration::from_millis(50))))
            .map_err(|e| io_error(name, e))?;
        match stream.write(&bytes[sent..]) {
            Ok(0) => {
                return Err(io_error(
                    name,
                    std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "socket stopped accepting data",
                    ),
                ))
            }
            Ok(n) => sent += n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::Interrupted
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                ) =>
            {
                continue
            }
            Err(e) => return Err(io_error(name, e)),
        }
    }
    Ok(Value::Int(sent as i64))
}
pub(super) fn guard_and_recv(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let name = "net_recv";
    shape(name, &args, input, 1, 2)?;
    let limit = integer(name, args.get(1), 4096, MAX_BYTES as i64)? as usize;
    let (_, socket) = lookup(name, &args[0])?;
    transaction(name)?;
    guard_network(name, &socket.peer)?;
    let mut stream = socket.stream.try_lock().map_err(|_| {
        safety::bad_state(
            name,
            "socket busy or poisoned",
            "finish the other operation or close the handle",
        )
    })?;
    crate::safety::check_deadline()?;
    let deadline = Instant::now() + safety::bounded_io_timeout(name, socket.timeout)?;
    let mut bytes = vec![0; limit];
    let count = loop {
        if socket.closed.load(Ordering::Acquire) {
            return Err(safety::bad_state(
                name,
                "socket closed during receive",
                "use another connection",
            ));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io_error(
                name,
                std::io::Error::new(std::io::ErrorKind::TimedOut, "receive deadline exceeded"),
            ));
        }
        // Some OSes do not wake a blocking read on shutdown of a clone.
        stream
            .set_read_timeout(Some(remaining.min(Duration::from_millis(50))))
            .map_err(|e| io_error(name, e))?;
        match stream.read(&mut bytes) {
            Ok(count) => break count,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::Interrupted
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                ) =>
            {
                continue
            }
            Err(e) => return Err(io_error(name, e)),
        }
    };
    bytes.truncate(count);
    Ok(Value::Record(
        [
            (
                "bytes".into(),
                Value::Array(bytes.into_iter().map(|b| Value::Int(b as i64)).collect()),
            ),
            ("eof".into(), Value::Bool(count == 0)),
        ]
        .into_iter()
        .collect(),
    ))
}
pub(super) fn guard_and_close(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let name = "net_close";
    shape(name, &args, input, 1, 1)?;
    let (id, socket) = lookup(name, &args[0])?;
    transaction(name)?;
    guard_network(name, &socket.peer)?;
    // Do not wait for the stream mutex. Pending I/O observes closed within a bounded slice.
    socket.closed.store(true, Ordering::Release);
    let result = socket.shutdown.shutdown(Shutdown::Both);
    registry()
        .lock()
        .map_err(|_| safety::bad_state(name, "socket registry poisoned", "restart the process"))?
        .remove(&id);
    match result {
        Ok(()) => Ok(Value::Bool(true)),
        Err(e) if e.kind() == std::io::ErrorKind::NotConnected => Ok(Value::Bool(true)),
        Err(e) => Err(io_error(name, e)),
    }
}
