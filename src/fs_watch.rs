//! Bounded native filesystem watches with explicit handles.
//!
//! `fs_watch` starts an OS notification watch (the `notify` crate) and returns
//! a handle; `fs_unwatch` stops it and returns what it saw. Watches live in a
//! process-global registry, so they end with the process. Events are buffered
//! per handle up to `max_events`; anything beyond that is counted in `dropped`.
use crate::{safety, value::Value};
use anyhow::Result;
use notify::{event::ModifyKind, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

/// At most this many watches may be live at once.
const MAX_LIVE: usize = 64;
const DEFAULT_MAX_EVENTS: i64 = 1000;
const MAX_EVENTS_LIMIT: i64 = 10_000;

/// One notification as the caller will see it.
struct Event {
    kind: &'static str,
    paths: Vec<String>,
    /// Only set for watcher errors, which have no paths to report.
    message: Option<String>,
}

#[derive(Default)]
struct Buffer {
    events: Vec<Event>,
    dropped: i64,
    capacity: usize,
    /// Set by `fs_unwatch` before it drains the buffer; later callbacks are ignored.
    closed: bool,
}

struct Watch {
    path: String,
    // Held only so that dropping it stops the OS watch.
    _watcher: RecommendedWatcher,
    buffer: Arc<Mutex<Buffer>>,
}

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

fn registry() -> &'static Mutex<HashMap<String, Watch>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Watch>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A callback panic must not wedge later calls, so poisoning is ignored.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn kind_name(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::Create(_) => "create",
        EventKind::Modify(ModifyKind::Name(_)) => "rename",
        EventKind::Modify(_) => "modify",
        EventKind::Remove(_) => "remove",
        EventKind::Access(_) => "access",
        EventKind::Any | EventKind::Other => "other",
    }
}

/// Parse the optional options Record into (recursive, max_events).
fn options(value: Option<&Value>) -> Result<(bool, usize)> {
    let mut recursive = false;
    let mut max_events = DEFAULT_MAX_EVENTS;
    let Some(value) = value else {
        return Ok((recursive, max_events as usize));
    };
    let Value::Record(map) = value else {
        return Err(safety::bad_arg(
            "fs_watch",
            "options Record",
            value.type_name(),
        ));
    };
    for (key, entry) in map {
        match (key.as_str(), entry) {
            ("recursive", Value::Bool(flag)) => recursive = *flag,
            ("recursive", other) => {
                return Err(safety::bad_arg(
                    "fs_watch",
                    "recursive Bool",
                    other.type_name(),
                ))
            }
            ("max_events", Value::Int(n)) if (1..=MAX_EVENTS_LIMIT).contains(n) => {
                max_events = *n;
            }
            ("max_events", Value::Int(n)) => {
                return Err(safety::bad_arg(
                    "fs_watch",
                    "max_events Int in 1..=10000",
                    &n.to_string(),
                ))
            }
            ("max_events", other) => {
                return Err(safety::bad_arg(
                    "fs_watch",
                    "max_events Int",
                    other.type_name(),
                ))
            }
            (other, _) => {
                return Err(safety::arg_err(format!(
                    "fs_watch: unknown option `{other}`; allowed keys are recursive and max_events"
                )))
            }
        }
    }
    Ok((recursive, max_events as usize))
}

pub(crate) fn watch(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if input.is_some() || !(1..=2).contains(&args.len()) {
        return Err(safety::arg_err(
            "fs_watch takes a path String and an optional options Record, with no pipeline input",
        ));
    }
    let Value::Str(path) = &args[0] else {
        return Err(safety::bad_arg(
            "fs_watch",
            "path String",
            args[0].type_name(),
        ));
    };
    let (recursive, capacity) = options(args.get(1))?;
    std::fs::metadata(path).map_err(|e| safety::fs_error("fs_watch", path, &e))?;

    // Held across creation so that the live-count check cannot race another call.
    let mut registry = lock(registry());
    if registry.len() >= MAX_LIVE {
        return Err(safety::budget_exceeded(
            "fs_watch",
            format!("{MAX_LIVE} watches are already live"),
            "stop one with fs_unwatch before starting another",
        ));
    }

    let buffer = Arc::new(Mutex::new(Buffer {
        capacity,
        ..Buffer::default()
    }));
    let sink = Arc::clone(&buffer);
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        let event = match result {
            Ok(event) => Event {
                kind: kind_name(&event.kind),
                paths: event
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect(),
                message: None,
            },
            Err(error) => Event {
                kind: "error",
                paths: Vec::new(),
                message: Some(error.to_string()),
            },
        };
        let mut buffer = lock(&sink);
        if buffer.closed {
            return;
        }
        if buffer.events.len() < buffer.capacity {
            buffer.events.push(event);
        } else {
            buffer.dropped += 1;
        }
    })
    .map_err(|e| safety::fs_error("fs_watch", path, &std::io::Error::other(e.to_string())))?;

    let mode = if recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };
    watcher
        .watch(Path::new(path), mode)
        .map_err(|e| safety::fs_error("fs_watch", path, &std::io::Error::other(e.to_string())))?;

    let handle = format!("watch-{}", NEXT_HANDLE.fetch_add(1, Ordering::Relaxed));
    registry.insert(
        handle.clone(),
        Watch {
            path: path.clone(),
            _watcher: watcher,
            buffer,
        },
    );
    Ok(Value::Str(handle))
}

fn event_value(event: Event) -> Value {
    let mut record = BTreeMap::new();
    record.insert("kind".to_string(), Value::Str(event.kind.to_string()));
    record.insert(
        "paths".to_string(),
        Value::Array(event.paths.into_iter().map(Value::Str).collect()),
    );
    if let Some(message) = event.message {
        record.insert("message".to_string(), Value::Str(message));
    }
    Value::Record(record)
}

pub(crate) fn unwatch(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if input.is_some() || args.len() != 1 {
        return Err(safety::arg_err(
            "fs_unwatch takes one handle String and no pipeline input",
        ));
    }
    let Value::Str(handle) = &args[0] else {
        return Err(safety::bad_arg(
            "fs_unwatch",
            "handle String",
            args[0].type_name(),
        ));
    };

    let watch = {
        let mut registry = lock(registry());
        match registry.remove(handle) {
            Some(watch) => watch,
            None => {
                return Err(safety::bad_arg(
                    "fs_unwatch",
                    &format!(
                        "a live watch handle ({} live watch handles now)",
                        registry.len()
                    ),
                    &format!("`{handle}`"),
                ))
            }
        }
    };

    // Close the buffer before draining it, so no callback can add an event
    // after this point. The watcher itself is dropped with `watch`.
    let (events, dropped) = {
        let mut buffer = lock(&watch.buffer);
        buffer.closed = true;
        (std::mem::take(&mut buffer.events), buffer.dropped)
    };
    let path = watch.path.clone();
    drop(watch);

    let mut record = BTreeMap::new();
    record.insert("path".to_string(), Value::Str(path));
    record.insert(
        "events".to_string(),
        Value::Array(events.into_iter().map(event_value).collect()),
    );
    record.insert("dropped".to_string(), Value::Int(dropped));
    Ok(Value::Record(record))
}
