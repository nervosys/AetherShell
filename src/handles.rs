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
//! * **Process-lifetime, not persistent.** Handles live in memory for the
//!   session. A handle from a previous process is gone, and says so.

use crate::value::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;

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
pub fn put(v: Value, rendered_bytes: usize) -> Handle {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let id = format!("h{}", store.len() + 1);
    let h = Handle {
        id: id.clone(),
        shape: crate::shapes::observe(&v),
        items: item_count(&v),
        bytes: rendered_bytes,
        value: v,
    };
    store.push(h.clone());
    h
}

/// Retrieve a stored value.
pub fn get(id: &str) -> Option<Value> {
    let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store.iter().find(|h| h.id == id).map(|h| h.value.clone())
}

/// Every live handle, oldest first, without their values.
pub fn list() -> Vec<Handle> {
    let store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store
        .iter()
        .map(|h| Handle {
            value: Value::Null,
            ..h.clone()
        })
        .collect()
}

/// Release a handle. Returns whether it existed.
pub fn drop_handle(id: &str) -> bool {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let before = store.len();
    store.retain(|h| h.id != id);
    store.len() != before
}

/// Drop every handle. Used by tests and by session reset.
pub fn clear() {
    STORE.lock().unwrap_or_else(|e| e.into_inner()).clear();
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
