//! Result handles: stop sending the data at all.
//!
//! AECON made a payload roughly twice as cheap to send. Not sending it is
//! unboundedly cheaper. When a call returns 8,000 rows and the agent wants five
//! of them, every token spent on the other 7,995 is waste that no encoder can
//! recover — the win is in never materialising them in the context window.
//!
//! So on the agent path a result that exceeds a size threshold is kept here and
//! rendered as a **handle**: an id, its shape, its size, and a short preview.
//! The agent then composes against the handle — `handle("h1") | where(…) |
//! take(5)` — and only the small final result crosses back. The shell was
//! already a pipeline language; this lets the pipeline stay server-side, which
//! is what it was always good at.
//!
//! # Deliberate properties
//!
//! * **Nothing is lost.** The value is kept whole; the handle is a reference to
//!   it, not a lossy summary. `handle(id)` returns exactly what was computed.
//! * **The preview is honest about being partial.** It reports the total row
//!   count next to the rows shown, so an agent cannot mistake the preview for
//!   the result — the failure mode that makes silent truncation dangerous.
//! * **Human output is untouched.** This is reached from `render_agent` only.
//! * **They survive the process.** They have to: the Python SDK runs
//!   `ae -e <code>` per call, a fresh process each time, so an in-memory handle
//!   would be unresolvable by the very next call — the feature would be broken
//!   for its main consumer. Handles are written to a session directory keyed by
//!   workspace, and read back on demand.

use crate::value::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Where a session's handles live.
///
/// `AETHER_SESSION_DIR` overrides. Otherwise the directory is keyed by
/// `AETHER_SESSION`, falling back to a digest of the working directory, so two
/// projects do not share handles while successive calls in one project do.
/// Returns `None` when no cache directory can be determined, in which case
/// handles degrade to memory-only rather than failing.
fn session_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("AETHER_SESSION_DIR") {
        return Some(PathBuf::from(d));
    }
    let key = std::env::var("AETHER_SESSION").ok().unwrap_or_else(|| {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("{:x}", md5::compute(cwd.as_bytes()))
    });
    dirs::cache_dir().map(|c| c.join("aethershell").join("handles").join(key))
}

/// Persist one handle. Failure is deliberately silent: a handle that cannot be
/// written still works for the rest of *this* process, and breaking a builtin
/// call because a cache directory is unwritable would be a worse trade.
fn persist(h: &Handle) {
    let Some(dir) = session_dir() else { return };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    // serde's representation, not `Value::to_json` — the latter renders `Uri`
    // as a bare string, so reading it back would yield `Str` and the handle
    // would no longer be what was computed. See the round-trip test below.
    if let Ok(json) = serde_json::to_string(&(&h.shape, h.items, h.bytes, &h.value)) {
        let _ = std::fs::write(dir.join(format!("{}.json", h.id)), json);
    }
}

fn load(id: &str) -> Option<Handle> {
    let dir = session_dir()?;
    let raw = std::fs::read_to_string(dir.join(format!("{id}.json"))).ok()?;
    let (shape, items, bytes, value): (String, usize, usize, Value) =
        serde_json::from_str(&raw).ok()?;
    Some(Handle {
        id: id.to_string(),
        shape,
        items,
        bytes,
        value,
    })
}

/// Ids present on disk for this session.
fn persisted_ids() -> Vec<String> {
    let Some(dir) = session_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".json").map(|s| s.to_string())
        })
        .collect();
    // h2 before h10: numeric order, so "the next id" is actually the next.
    ids.sort_by_key(|id| id[1..].parse::<usize>().unwrap_or(0));
    ids
}

/// Rendered bytes above which a result is handled rather than sent whole.
/// `AETHER_HANDLE_BYTES=0` disables handling entirely and restores whole-result
/// rendering.
const DEFAULT_THRESHOLD_BYTES: usize = 2048;

/// Rows shown in a preview. Enough to see the shape and recognise the data,
/// far too few to be mistaken for the whole result.
const PREVIEW_ROWS: usize = 3;

#[derive(Clone, Debug)]
pub struct Handle {
    pub id: String,
    pub shape: String,
    /// Element count for an array, field count for a record, else 1.
    pub items: usize,
    /// Size of the value as it *would* have been rendered, i.e. what was saved.
    pub bytes: usize,
    pub value: Value,
}

lazy_static::lazy_static! {
    static ref STORE: Mutex<Vec<Handle>> = Mutex::new(Vec::new());
}

/// The byte threshold in force. `0` means handles are disabled.
pub fn threshold_bytes() -> usize {
    match std::env::var("AETHER_HANDLE_BYTES") {
        Ok(v) => v.trim().parse().unwrap_or(DEFAULT_THRESHOLD_BYTES),
        Err(_) => DEFAULT_THRESHOLD_BYTES,
    }
}

/// Whether a value is worth handling: big enough to matter, and structured
/// enough that an agent can actually narrow it down afterwards. Handing back a
/// reference to a single huge string would save tokens but leave the agent no
/// way to query it, which trades a cost for a dead end.
pub fn worth_handling(v: &Value, rendered_bytes: usize) -> bool {
    let t = threshold_bytes();
    if t == 0 || rendered_bytes <= t {
        return false;
    }
    matches!(v, Value::Array(_) | Value::Record(_) | Value::Table(_))
}

fn item_count(v: &Value) -> usize {
    match v {
        Value::Array(a) => a.len(),
        Value::Record(m) => m.len(),
        _ => 1,
    }
}

/// Store a value and return its handle id.
///
/// The id continues the session's numbering rather than this process's, so two
/// consecutive `ae -e` calls do not both mint `h1` and have the second silently
/// overwrite the first.
pub fn put(v: Value, rendered_bytes: usize) -> Handle {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let next = persisted_ids()
        .last()
        .and_then(|id| id[1..].parse::<usize>().ok())
        .unwrap_or(0)
        .max(store.len())
        + 1;
    let h = Handle {
        id: format!("h{next}"),
        shape: crate::shapes::observe(&v),
        items: item_count(&v),
        bytes: rendered_bytes,
        value: v,
    };
    store.push(h.clone());
    persist(&h);
    h
}

/// Retrieve a stored value — from memory, or from the session directory when
/// this is a different process than the one that produced it.
pub fn get(id: &str) -> Option<Value> {
    {
        let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = store.iter().find(|h| h.id == id) {
            return Some(h.value.clone());
        }
    }
    load(id).map(|h| h.value)
}

/// Every live handle, oldest first, without their values.
pub fn list() -> Vec<Handle> {
    let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<Handle> = store
        .iter()
        .map(|h| Handle {
            value: Value::Null,
            ..h.clone()
        })
        .collect();
    for id in persisted_ids() {
        if out.iter().any(|h| h.id == id) {
            continue;
        }
        if let Some(h) = load(&id) {
            out.push(Handle {
                value: Value::Null,
                ..h
            });
        }
    }
    out.sort_by_key(|h| h.id[1..].parse::<usize>().unwrap_or(0));
    out
}

/// Release a handle. Returns whether it existed, in memory or on disk.
pub fn drop_handle(id: &str) -> bool {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let before = store.len();
    store.retain(|h| h.id != id);
    let in_memory = store.len() != before;
    let on_disk = session_dir()
        .map(|d| std::fs::remove_file(d.join(format!("{id}.json"))).is_ok())
        .unwrap_or(false);
    in_memory || on_disk
}

/// Drop every handle, in memory and on disk.
pub fn clear() {
    STORE.lock().unwrap_or_else(|e| e.into_inner()).clear();
    if let Some(dir) = session_dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The first few elements of a value, for a preview that shows the shape
/// without the bulk.
pub fn preview_of(v: &Value) -> Value {
    match v {
        Value::Array(items) => Value::Array(items.iter().take(PREVIEW_ROWS).cloned().collect()),
        // A record is previewed by its keys: the field names are the part an
        // agent needs in order to write the next call, and the values may be
        // arbitrarily large.
        Value::Record(m) => {
            let mut out = BTreeMap::new();
            for (k, val) in m.iter().take(PREVIEW_ROWS) {
                out.insert(k.clone(), Value::Str(crate::shapes::observe(val)));
            }
            Value::Record(out)
        }
        other => other.clone(),
    }
}

/// How many items a preview omits.
pub fn omitted(v: &Value) -> usize {
    item_count(v).saturating_sub(PREVIEW_ROWS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn big_array(n: usize) -> Value {
        Value::Array((0..n).map(|i| Value::Int(i as i64)).collect())
    }

    #[test]
    fn persistence_encoding_is_lossless_where_plain_json_is_not() {
        // Checked before being relied on, not assumed — and the check found a
        // real problem. `Value::to_json` is a *presentation* format: it renders
        // `Uri("https://e.com")` as a bare string, so reading it back yields
        // `Str`. Persisting through it would quietly break the guarantee that a
        // handle returns exactly what was computed.
        //
        // serde's derived representation keeps the variant, so that is what the
        // session store uses.
        let mut m = std::collections::BTreeMap::new();
        m.insert("s".to_string(), Value::Str("x".into()));
        m.insert("i".to_string(), Value::Int(-3));
        m.insert("f".to_string(), Value::Float(1.5));
        m.insert("b".to_string(), Value::Bool(true));
        m.insert("u".to_string(), Value::Uri("https://e.com".into()));
        m.insert("n".to_string(), Value::Null);
        m.insert(
            "arr".to_string(),
            Value::Array(vec![Value::Int(1), Value::Str("a".into())]),
        );
        let original = Value::Record(m);

        let lossy = Value::from_json(&original.to_json());
        assert_ne!(
            lossy, original,
            "if to_json/from_json became lossless this note should be revisited"
        );

        let encoded = serde_json::to_string(&original).expect("encode");
        let back: Value = serde_json::from_str(&encoded).expect("decode");
        assert_eq!(back, original, "the session encoding must be lossless");
    }

    #[test]
    fn a_stored_value_comes_back_exactly() {
        clear();
        let v = big_array(100);
        let h = put(v.clone(), 9999);
        assert_eq!(get(&h.id), Some(v), "a handle must be lossless");
    }

    #[test]
    fn small_results_are_never_handled() {
        // The whole point is to spend tokens on data worth having; a short
        // result costs less to send than its own handle summary.
        assert!(!worth_handling(&big_array(2), 10));
    }

    #[test]
    fn an_unstructured_value_is_not_handled_however_large() {
        // A handle the agent cannot narrow down is a dead end, not a saving.
        let huge = Value::Str("x".repeat(100_000));
        assert!(!worth_handling(&huge, 100_000));
    }

    #[test]
    fn a_preview_reports_what_it_omits() {
        let v = big_array(50);
        assert_eq!(omitted(&v), 50 - PREVIEW_ROWS);
        match preview_of(&v) {
            Value::Array(a) => assert_eq!(a.len(), PREVIEW_ROWS),
            other => panic!("expected an array preview, got {other:?}"),
        }
    }

    #[test]
    fn dropping_a_handle_frees_it_and_reports_whether_it_existed() {
        clear();
        let h = put(big_array(10), 9999);
        assert!(drop_handle(&h.id));
        assert!(!drop_handle(&h.id), "dropping twice must report absence");
        assert_eq!(get(&h.id), None);
    }

    #[test]
    fn a_zero_threshold_disables_handling() {
        // The escape hatch must actually work, or a user who dislikes handles
        // has no way back to whole results.
        std::env::set_var("AETHER_HANDLE_BYTES", "0");
        assert!(!worth_handling(&big_array(1000), 1_000_000));
        std::env::remove_var("AETHER_HANDLE_BYTES");
    }
}
