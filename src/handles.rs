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
//!   `ish -e <code>` per call, a fresh process each time, so an in-memory handle
//!   would be unresolvable by the very next call — the feature would be broken
//!   for its main consumer. Handles are written to a session directory keyed by
//!   workspace, and read back on demand.

use crate::value::Value;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// Where a session's handles live.
///
/// `IRONSHELL_SESSION_DIR` overrides. Otherwise the directory is keyed by
/// `IRONSHELL_SESSION`, falling back to a digest of the working directory, so two
/// projects do not share handles while successive calls in one project do.
/// Returns `None` when no cache directory can be determined, in which case
/// handles degrade to memory-only rather than failing.
fn session_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("IRONSHELL_SESSION_DIR") {
        return Some(PathBuf::from(d));
    }
    let key = std::env::var("IRONSHELL_SESSION").ok().unwrap_or_else(|| {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("{:x}", md5::compute(cwd.as_bytes()))
    });
    dirs::cache_dir().map(|c| c.join("ironshell").join("handles").join(key))
}

/// What became of a handle written to the session directory.
enum Persisted {
    Written,
    /// Another writer already holds this id. Its file is left alone.
    Taken,
    Failed,
}

/// Persist one handle, and report what happened. Failure is deliberately
/// silent: a handle that cannot be written still works for the rest of *this*
/// process, and breaking a builtin call because a cache directory is unwritable
/// would be a worse trade. The file is only ever created, never replaced: an id
/// names one value, and another process in this session may have just taken it.
fn persist(dir: &Path, h: &Handle) -> Persisted {
    if std::fs::create_dir_all(dir).is_err() {
        return Persisted::Failed;
    }
    // serde's representation, not `Value::to_json` — the latter renders `Uri`
    // as a bare string, so reading it back would yield `Str` and the handle
    // would no longer be what was computed. See the round-trip test below.
    let Ok(json) = serde_json::to_string(&(&h.shape, h.items, h.bytes, &h.value)) else {
        return Persisted::Failed;
    };
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(format!("{}.json", h.id)));
    match file {
        Ok(mut f) => match f.write_all(json.as_bytes()) {
            Ok(()) => Persisted::Written,
            Err(_) => Persisted::Failed,
        },
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Persisted::Taken,
        Err(_) => Persisted::Failed,
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
    match session_dir() {
        Some(dir) => persisted_ids_in(&dir),
        None => Vec::new(),
    }
}

/// Ids of the handle files in `dir`, in numeric order.
fn persisted_ids_in(dir: &Path) -> Vec<String> {
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
/// `IRONSHELL_HANDLE_BYTES=0` disables handling entirely and restores whole-result
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

/// What the last listing of a session directory found, and the directory's
/// modification time as it was when that listing began.
///
/// `put` needs the count and the highest id on every call. Listing a directory
/// of thousands of handles on each put made filling a session quadratic, so the
/// answer is kept here and the directory is listed again only when it has
/// changed. Creating or removing a file moves a directory's mtime, which is how
/// a write by another process is noticed. This process's own writes are added
/// to the tally by `note_put`.
struct DirTally {
    dir: PathBuf,
    mtime: Option<SystemTime>,
    count: usize,
    max_id: usize,
}

static TALLY: Mutex<Option<DirTally>> = Mutex::new(None);

/// How many ids `put` tries when other writers keep taking the next one.
const PERSIST_TRIES: usize = 4;

/// Within this many handles of the cap, `put` lists the directory every time
/// rather than trusting the tally. The mtime check misses a write that lands in
/// the same timestamp tick as ours (FAT keeps two seconds), so near the limit
/// the count is re-read exactly instead.
const NEAR_CAP_LISTING: usize = 64;

fn dir_mtime(dir: &Path) -> Option<SystemTime> {
    std::fs::metadata(dir).and_then(|m| m.modified()).ok()
}

/// How many handles the session directory holds, and the highest id number.
fn session_count(dir: &Path, cap: usize) -> (usize, usize) {
    // Read the mtime before listing. A write that lands during the listing
    // moves it, so the next put sees a change and lists again.
    let now = dir_mtime(dir);
    let mut tally = TALLY.lock().unwrap_or_else(|e| e.into_inner());
    if let (Some(t), Some(now)) = (tally.as_ref(), now) {
        if t.dir == dir && t.mtime == Some(now) && t.count + NEAR_CAP_LISTING < cap {
            return (t.count, t.max_id);
        }
    }
    let ids = persisted_ids_in(dir);
    let max_id = ids
        .last()
        .and_then(|id| id[1..].parse::<usize>().ok())
        .unwrap_or(0);
    *tally = Some(DirTally {
        dir: dir.to_path_buf(),
        mtime: now,
        count: ids.len(),
        max_id,
    });
    (ids.len(), max_id)
}

/// Record a handle this process has just written to `dir`. The mtime is read
/// after the write, so a write by another process that lands between the two
/// is folded into it and not counted. That window is microseconds wide, and the
/// near-cap listing above is what corrects any miss it leaves.
fn note_put(dir: &Path, number: usize) {
    let mtime = dir_mtime(dir);
    let mut tally = TALLY.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(t) = tally.as_mut().filter(|t| t.dir == dir) {
        t.count += 1;
        t.max_id = t.max_id.max(number);
        t.mtime = mtime;
    }
}

/// Forget the tally, so the next `put` lists the directory in full. Used where
/// files are removed, which `note_put` does not account for.
fn forget_tally() {
    *TALLY.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The byte threshold in force. `0` means handles are disabled.
pub fn threshold_bytes() -> usize {
    match std::env::var("IRONSHELL_HANDLE_BYTES") {
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

/// Most handles one session keeps. Each holds a whole result, in memory and on
/// disk, so an unbounded store is unbounded growth.
pub const MAX_HANDLES: usize = 4096;

/// Store a value and return its handle id.
///
/// The id continues the session's numbering rather than this process's, so two
/// consecutive `ish -e` calls do not both mint `h1` and have the second silently
/// overwrite the first.
///
/// Refused with `E_BUDGET_EXCEEDED` once the session holds `MAX_HANDLES`. An
/// existing handle is never evicted to make room: a handle that vanished would
/// make a later `handle(id)` fail for a reason the agent cannot see.
pub fn put(v: Value, rendered_bytes: usize) -> anyhow::Result<Handle> {
    put_capped(v, rendered_bytes, MAX_HANDLES)
}

fn put_capped(v: Value, rendered_bytes: usize, cap: usize) -> anyhow::Result<Handle> {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let dir = session_dir();
    let (on_disk, last_number) = match &dir {
        Some(d) => session_count(d, cap),
        None => (0, 0),
    };
    let live = on_disk.max(store.len());
    if live >= cap {
        return Err(crate::safety::budget_exceeded(
            "handle",
            format!("the session already holds {live} result handles, the limit"),
            "release one with handle_drop(id) before the next large result is handled",
        ));
    }
    let mut number = last_number.max(store.len()) + 1;
    let mut h = Handle {
        id: format!("h{number}"),
        shape: crate::shapes::observe(&v),
        items: item_count(&v),
        bytes: rendered_bytes,
        value: v,
    };
    if let Some(d) = &dir {
        // The tally can miss another writer, so a taken id means list again.
        for _ in 0..PERSIST_TRIES {
            match persist(d, &h) {
                Persisted::Written => {
                    note_put(d, number);
                    break;
                }
                Persisted::Failed => break,
                Persisted::Taken => {
                    forget_tally();
                    let (_, last) = session_count(d, cap);
                    number = last.max(store.len()) + 1;
                    h.id = format!("h{number}");
                }
            }
        }
    }
    store.push(h.clone());
    Ok(h)
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
    forget_tally();
    in_memory || on_disk
}

/// Drop every handle, in memory and on disk.
pub fn clear() {
    let mut store = STORE.lock().unwrap_or_else(|e| e.into_inner());
    store.clear();
    if let Some(dir) = session_dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
    forget_tally();
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

    /// Serialises every test that touches the process-global `STORE` or the
    /// `IRONSHELL_HANDLE_BYTES` env var.
    ///
    /// `put` derives the next id from `store.len()`, so two tests that each
    /// `clear()` then `put()` both mint `h1`. That is not theoretical: CI's
    /// Windows runner failed on `dropping_a_handle_frees_it_and_reports_whether_it_existed`
    /// asserting `get(&h.id) == None` while another test had just re-created
    /// the same id. It passed on ubuntu and macos in the same run, and passes
    /// in isolation everywhere -- the signature of a scheduling race, not a
    /// platform bug.
    ///
    /// Poison-tolerant: a panicking test has already been reported, and
    /// failing every subsequent one on top of that hides more than it reveals.
    /// A lock in the file rather than `--test-threads=1`, which has to be
    /// remembered at every call site and fails silently when it is not.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn store_lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn big_array(n: usize) -> Value {
        Value::Array((0..n).map(|i| Value::Int(i as i64)).collect())
    }

    #[test]
    fn a_taken_id_is_skipped_and_the_other_writer_keeps_its_file() {
        let _guard = store_lock();
        let dir = std::env::temp_dir().join(format!("ish-handle-taken-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let previous = std::env::var_os("IRONSHELL_SESSION_DIR");
        std::env::set_var("IRONSHELL_SESSION_DIR", &dir);
        clear();

        let first = put(big_array(3), 10).unwrap();
        // Another process takes the next id, and the tally does not see it: the
        // mtime is refreshed the way a write folded into our own would be.
        let foreign = dir.join("h2.json");
        std::fs::write(&foreign, "theirs").unwrap();
        note_put(&dir, 1);
        let second = put(big_array(4), 10).unwrap();
        let kept = std::fs::read_to_string(&foreign).unwrap();

        clear();
        match previous {
            Some(p) => std::env::set_var("IRONSHELL_SESSION_DIR", p),
            None => std::env::remove_var("IRONSHELL_SESSION_DIR"),
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(first.id, "h1");
        assert_eq!(kept, "theirs", "the other writer's handle was overwritten");
        assert_eq!(second.id, "h3", "the taken id was not skipped");
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
        let _guard = store_lock();
        clear();
        let v = big_array(100);
        let h = put(v.clone(), 9999).expect("under the cap");
        assert_eq!(get(&h.id), Some(v), "a handle must be lossless");
    }

    #[test]
    fn small_results_are_never_handled() {
        let _guard = store_lock();
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
        let _guard = store_lock();
        clear();
        let h = put(big_array(10), 9999).expect("under the cap");
        assert!(drop_handle(&h.id));
        assert!(!drop_handle(&h.id), "dropping twice must report absence");
        assert_eq!(get(&h.id), None);
    }

    #[test]
    fn a_full_store_refuses_a_new_handle_instead_of_evicting() {
        let _guard = store_lock();
        // Point the store at a scratch directory so the test does not write
        // into the real session cache, and restore the caller's setting after.
        let dir = std::env::temp_dir().join(format!("ae-handles-cap-{}", std::process::id()));
        let previous = std::env::var_os("IRONSHELL_SESSION_DIR");
        std::env::set_var("IRONSHELL_SESSION_DIR", &dir);
        clear();

        let kept: Vec<String> = (0..3)
            .map(|_| {
                put_capped(Value::Int(1), 9999, 3)
                    .expect("under the cap")
                    .id
            })
            .collect();
        let refused = put_capped(Value::Int(1), 9999, 3).map_err(|e| format!("{e}"));
        let first_still_there = get(&kept[0]).is_some();

        clear();
        match previous {
            Some(v) => std::env::set_var("IRONSHELL_SESSION_DIR", v),
            None => std::env::remove_var("IRONSHELL_SESSION_DIR"),
        }
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            first_still_there,
            "the cap must not evict an existing handle"
        );
        let message = refused.expect_err("the fourth handle must be refused");
        assert!(message.contains("E_BUDGET_EXCEEDED"), "{message}");
        assert!(message.contains("handle_drop"), "{message}");
    }

    #[test]
    fn a_zero_threshold_disables_handling() {
        let _guard = store_lock();
        // The escape hatch must actually work, or a user who dislikes handles
        // has no way back to whole results.
        std::env::set_var("IRONSHELL_HANDLE_BYTES", "0");
        assert!(!worth_handling(&big_array(1000), 1_000_000));
        std::env::remove_var("IRONSHELL_HANDLE_BYTES");
    }
}
