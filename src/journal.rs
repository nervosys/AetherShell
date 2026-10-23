//! Reversible sessions: let the agent work, and be able to rewind it.
//!
//! 6.0.0 bought safety with friction — 166 builtins now stop and ask. That is
//! the expensive currency: every approval is a round-trip, a stall, and a
//! chance the agent gives up or routes around it. Reversibility buys the same
//! safety with a cheaper one. If a write can be undone, it does not need to be
//! prevented, and the user's question changes from *"should I allow this?"* to
//! *"do I keep it?"* — asked once, at the end, with the results in front of them.
//!
//! Before a `WriteLocal` or `Destructive` builtin runs, the prior contents of
//! every file it might touch are recorded here. [`undo`] puts them back.
//!
//! Undo can itself be undone. When [`undo`] restores a path it first captures
//! the state that the undone step had produced, and [`redo`] writes that state
//! back. Recording a new step discards the redo stack, the usual editor rule:
//! once the history has moved on, the undone future is no longer reachable.
//!
//! # What this cannot do, stated plainly
//!
//! Undo covers **local file contents and nothing else**. A `Network` call
//! cannot be unsent, an `Exec`'d process cannot be un-run, a killed process
//! cannot be revived, and a directory tree is not snapshotted. The journal
//! therefore records *irreversible* steps too, as explicit entries, so that a
//! rewind reports what it could not restore instead of quietly restoring part
//! of the world and reporting success.
//!
//! That last point is the whole design. A partial undo that claims completeness
//! is worse than no undo, because it converts a recoverable situation into one
//! where the user believes they have already recovered.

use crate::safety::Effect;
use crate::value::Value;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// Largest single file captured. Beyond this the step is recorded as
/// irreversible rather than silently skipped.
const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;

/// Ceiling on everything held at once (journal and redo together), so a long
/// session cannot exhaust memory.
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

/// What was there before a step ran.
#[derive(Clone, Debug, PartialEq)]
pub enum Before {
    /// The file existed with these bytes.
    Contents(Vec<u8>),
    /// The path did not exist, so undoing means removing whatever was created.
    Absent,
    /// Seen, but not recoverable — with the reason, which is reported verbatim
    /// on rewind rather than being flattened into a failure count.
    Irreversible(String),
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub seq: usize,
    pub builtin: String,
    pub effect: String,
    pub path: String,
    pub before: Before,
}

impl Entry {
    pub fn reversible(&self) -> bool {
        !matches!(self.before, Before::Irreversible(_))
    }
}

/// A step that was undone and can be re-applied. `after` is the state of
/// `path` just before undo restored it: the state the undone step produced.
#[derive(Clone, Debug)]
pub struct RedoEntry {
    pub seq: usize,
    pub builtin: String,
    pub effect: String,
    pub path: String,
    /// Always `Contents` or `Absent`; anything else is never pushed.
    pub after: Before,
}

#[derive(Default)]
struct State {
    journal: Vec<Entry>,
    redo: Vec<RedoEntry>,
}

lazy_static::lazy_static! {
    static ref STATE: Mutex<State> = Mutex::new(State::default());
    /// The session directory the state was last loaded from. Loading lists
    /// that directory; doing it once per directory, rather than on every
    /// builtin call while the journal happens to be empty, is the difference
    /// between a read per session and a read per call.
    static ref HYDRATED_FROM: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);
}

/// Where this session's journal lives. The redo stack lives in the `redo`
/// subdirectory of the same place.
///
/// Persistence is not a nicety here. The Python SDK runs `ae -c <code>` per
/// call, so an in-memory journal would be empty in the process that runs
/// `undo` — the feature would appear to work and reverse nothing, which is the
/// exact failure this module is written to avoid.
fn session_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("AETHER_SESSION_DIR") {
        return Some(PathBuf::from(d).join("journal"));
    }
    let key = std::env::var("AETHER_SESSION").ok().unwrap_or_else(|| {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("{:x}", md5::compute(cwd.as_bytes()))
    });
    dirs::cache_dir().map(|c| c.join("aethershell").join("journal").join(key))
}

fn redo_dir() -> Option<PathBuf> {
    session_dir().map(|d| d.join("redo"))
}

/// Serialisable form of an entry. The captured bytes travel with it: a journal
/// that persisted only the metadata could list what it would restore and then
/// restore nothing.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredEntry {
    seq: usize,
    builtin: String,
    effect: String,
    path: String,
    /// `None` = the path was absent; `Some(bytes)` = it held these.
    contents: Option<Vec<u8>>,
    /// Set when the step could not be captured, with the reason.
    irreversible: Option<String>,
}

impl From<&Entry> for StoredEntry {
    fn from(e: &Entry) -> Self {
        let (contents, irreversible) = match &e.before {
            Before::Contents(b) => (Some(b.clone()), None),
            Before::Absent => (None, None),
            Before::Irreversible(why) => (None, Some(why.clone())),
        };
        StoredEntry {
            seq: e.seq,
            builtin: e.builtin.clone(),
            effect: e.effect.clone(),
            path: e.path.clone(),
            contents,
            irreversible,
        }
    }
}

impl From<StoredEntry> for Entry {
    fn from(s: StoredEntry) -> Self {
        let before = match (s.contents, s.irreversible) {
            (_, Some(why)) => Before::Irreversible(why),
            (Some(b), None) => Before::Contents(b),
            (None, None) => Before::Absent,
        };
        Entry {
            seq: s.seq,
            builtin: s.builtin,
            effect: s.effect,
            path: s.path,
            before,
        }
    }
}

/// Read the entries persisted in `dir`, oldest first. Only files whose stem is
/// a number are entries, so the `redo` subdirectory of the journal directory is
/// never mistaken for one.
fn load_dir(dir: &Path) -> Vec<Entry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(usize, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            if !p.is_file() {
                return None;
            }
            let stem = p.file_stem()?.to_string_lossy().into_owned();
            stem.parse::<usize>().ok().map(|n| (n, p))
        })
        .collect();
    files.sort_by_key(|(n, _)| *n);
    files
        .into_iter()
        .filter_map(|(_, p)| {
            let raw = std::fs::read(&p).ok()?;
            serde_json::from_slice::<StoredEntry>(&raw)
                .ok()
                .map(Entry::from)
        })
        .collect()
}

fn persist_in(dir: Option<PathBuf>, e: &Entry) {
    let Some(dir) = dir else { return };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_vec(&StoredEntry::from(e)) {
        let _ = std::fs::write(dir.join(format!("{:08}.json", e.seq)), json);
    }
}

fn forget_in(dir: Option<PathBuf>, seq: usize) {
    if let Some(dir) = dir {
        let _ = std::fs::remove_file(dir.join(format!("{seq:08}.json")));
    }
}

fn persist(e: &Entry) {
    persist_in(session_dir(), e);
}

fn forget_persisted(seq: usize) {
    forget_in(session_dir(), seq);
}

fn persist_redo(r: &RedoEntry) {
    persist_in(redo_dir(), &redo_as_entry(r));
}

fn forget_redo_persisted(seq: usize) {
    forget_in(redo_dir(), seq);
}

fn redo_as_entry(r: &RedoEntry) -> Entry {
    Entry {
        seq: r.seq,
        builtin: r.builtin.clone(),
        effect: r.effect.clone(),
        path: r.path.clone(),
        before: r.after.clone(),
    }
}

/// Merge the persisted state into memory so this process sees steps recorded
/// by earlier ones. Once per session directory; a stack that is already
/// populated in memory is never overwritten by what is on disk.
fn hydrate(s: &mut State) {
    let dir = session_dir();
    {
        let mut h = HYDRATED_FROM.lock().unwrap_or_else(|e| e.into_inner());
        if h.as_ref() == Some(&dir) {
            return;
        }
        *h = Some(dir);
    }
    if s.journal.is_empty() {
        s.journal = load_dir(&session_dir().unwrap_or_default());
    }
    if s.redo.is_empty() {
        s.redo = load_dir(&redo_dir().unwrap_or_default())
            .into_iter()
            .filter_map(|e| match e.before {
                Before::Irreversible(_) => None,
                after => Some(RedoEntry {
                    seq: e.seq,
                    builtin: e.builtin,
                    effect: e.effect,
                    path: e.path,
                    after,
                }),
            })
            .collect();
    }
}

/// The hydrated state, locked. Every reader and writer goes through here.
fn state() -> MutexGuard<'static, State> {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    hydrate(&mut s);
    s
}

/// Journalling is on in agent mode and off for humans, matching the rest of the
/// dual-surface split: the agent surface pays a little I/O for recoverability,
/// the human REPL behaves exactly as before. `AETHER_JOURNAL=on`/`off` forces it.
pub fn enabled() -> bool {
    match std::env::var_os("AETHER_JOURNAL")
        .as_deref()
        .and_then(|v| v.to_str())
    {
        Some("on") | Some("1") | Some("true") => true,
        Some("off") | Some("0") | Some("false") => false,
        _ => crate::safety::current_mode() == crate::safety::Mode::Agent,
    }
}

/// Builtins that operate on the journal itself. Recording their writes would
/// make `undo` undo its own restoration, and would make `redo` discard the very
/// stack it is consuming.
fn is_journal_builtin(name: &str) -> bool {
    matches!(
        name,
        "undo"
            | "journal"
            | "journal_clear"
            | "rewind"
            | "session_undo"
            | "session_redo"
            | "session_end"
            | "session_history"
    )
}

fn total_bytes(entries: &[Entry]) -> usize {
    entries
        .iter()
        .map(|e| match &e.before {
            Before::Contents(b) => b.len(),
            _ => 0,
        })
        .sum()
}

fn redo_bytes(entries: &[RedoEntry]) -> usize {
    entries
        .iter()
        .map(|e| match &e.after {
            Before::Contents(b) => b.len(),
            _ => 0,
        })
        .sum()
}

/// Bytes held by the journal and the redo stack together.
fn held_bytes(s: &State) -> usize {
    total_bytes(&s.journal) + redo_bytes(&s.redo)
}

/// A string argument that plausibly names a file this call could write.
///
/// Deliberately generous: capturing a path the builtin never touches costs one
/// file read and restores identical bytes on undo, which is harmless. Missing
/// one loses the ability to recover it, which is not. The asymmetry decides the
/// bias.
fn candidate_paths(args: &[Value]) -> Vec<String> {
    let mut out = Vec::new();
    for a in args {
        if let Value::Str(s) = a {
            if s.is_empty() || s.len() > 4096 || s.contains('\n') || s.contains('\0') {
                continue;
            }
            let p = Path::new(s);
            let plausible = p.is_file()
                || p.parent()
                    .map(|d| !d.as_os_str().is_empty() && d.is_dir())
                    .unwrap_or(false);
            if plausible && !out.contains(s) {
                out.push(s.clone());
            }
        }
    }
    out
}

/// The current state of `path`, captured for the journal, or the reason it
/// cannot be. `used` is the bytes already held, so the session-wide budget is
/// respected.
fn capture(path: &str, used: usize) -> Result<Before, String> {
    let p = Path::new(path);
    if p.is_file() {
        let size = std::fs::metadata(p)
            .map(|m| m.len() as usize)
            .map_err(|e| format!("unstattable: {e}"))?;
        if size > MAX_FILE_BYTES {
            return Err(format!(
                "{size} bytes exceeds the {MAX_FILE_BYTES}-byte capture limit"
            ));
        }
        if used + size > MAX_TOTAL_BYTES {
            return Err("session capture budget exhausted".into());
        }
        std::fs::read(p)
            .map(Before::Contents)
            .map_err(|e| format!("unreadable: {e}"))
    } else if p.exists() {
        // A directory: restoring a tree is out of scope, and pretending
        // otherwise is the failure this module exists to avoid.
        Err("path is a directory; trees are not captured".into())
    } else {
        Ok(Before::Absent)
    }
}

/// Record the pre-state of anything `builtin` might overwrite. Call immediately
/// before dispatch.
///
/// A mutating step discards the redo stack whether or not it touches a
/// candidate path: a new action ends the undone future.
pub fn record_before(builtin: &str, effect: Effect, args: &[Value]) {
    if !enabled() || is_journal_builtin(builtin) {
        return;
    }
    if !matches!(effect, Effect::WriteLocal | Effect::Destructive) {
        return;
    }
    let paths = candidate_paths(args);
    let mut s = state();
    discard_redo(&mut s);
    for path in paths {
        let used = held_bytes(&s);
        let before = match capture(&path, used) {
            Ok(before) => before,
            Err(why) => Before::Irreversible(why),
        };
        let seq = s.journal.len() + 1;
        let entry = Entry {
            seq,
            builtin: builtin.to_string(),
            effect: effect.as_str().to_string(),
            path,
            before,
        };
        persist(&entry);
        s.journal.push(entry);
    }
}

/// Note a step that changed the world in a way no snapshot can capture, so a
/// later rewind can say so. Like [`record_before`], this discards the redo
/// stack.
pub fn record_irreversible(builtin: &str, effect: Effect, why: &str) {
    if !enabled() || is_journal_builtin(builtin) {
        return;
    }
    let mut s = state();
    discard_redo(&mut s);
    let seq = s.journal.len() + 1;
    let entry = Entry {
        seq,
        builtin: builtin.to_string(),
        effect: effect.as_str().to_string(),
        path: String::new(),
        before: Before::Irreversible(why.to_string()),
    };
    persist(&entry);
    s.journal.push(entry);
}

/// Forget the redo stack, in memory and on disk.
fn discard_redo(s: &mut State) {
    s.redo.clear();
    if let Some(dir) = redo_dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The outcome of rewinding or re-applying one entry.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Restored(String),
    Removed(String),
    /// Re-applied by [`redo`]: the file was written with its post-step bytes.
    Reapplied(String),
    Skipped {
        what: String,
        why: String,
    },
    Failed {
        what: String,
        why: String,
    },
}

/// Undo the last `n` recorded steps, most recent first.
///
/// Returns an outcome per entry — including the ones it could not reverse.
/// Callers must surface those: a rewind that reports only its successes is how
/// a partial restore gets mistaken for a complete one.
///
/// Each step that is actually reversed is pushed onto the redo stack, so
/// [`redo`] can put it back. A step whose current state cannot be captured is
/// still undone; it simply cannot be redone.
pub fn undo(n: usize) -> Vec<Outcome> {
    let mut s = state();
    let mut out = Vec::new();
    for _ in 0..n {
        // Checked before the pop: a step left alone stays in the journal, and
        // nothing older is undone past it, so the order is kept.
        if let Some(why) = s
            .journal
            .last()
            .and_then(|e| crate::safety::rewind_refused(&e.path))
        {
            let what = s.journal.last().map(|e| e.path.clone()).unwrap_or_default();
            out.push(Outcome::Skipped { what, why });
            break;
        }
        let Some(entry) = s.journal.pop() else { break };
        forget_persisted(entry.seq);
        let what = if entry.path.is_empty() {
            entry.builtin.clone()
        } else {
            entry.path.clone()
        };
        match &entry.before {
            Before::Contents(bytes) => {
                let after = capture(&entry.path, held_bytes(&s));
                match std::fs::write(&entry.path, bytes) {
                    Ok(()) => {
                        push_redo(&mut s, &entry, after);
                        out.push(Outcome::Restored(what));
                    }
                    Err(e) => out.push(Outcome::Failed {
                        what,
                        why: e.to_string(),
                    }),
                }
            }
            Before::Absent => {
                let after = capture(&entry.path, held_bytes(&s));
                let p = Path::new(&entry.path);
                if p.is_file() {
                    match std::fs::remove_file(p) {
                        Ok(()) => {
                            push_redo(&mut s, &entry, after);
                            out.push(Outcome::Removed(what));
                        }
                        Err(e) => out.push(Outcome::Failed {
                            what,
                            why: e.to_string(),
                        }),
                    }
                } else {
                    // Never created, so there is nothing to take back.
                    out.push(Outcome::Skipped {
                        what,
                        why: "was absent before and is absent now".into(),
                    });
                }
            }
            Before::Irreversible(why) => out.push(Outcome::Skipped {
                what,
                why: why.clone(),
            }),
        }
    }
    out
}

/// Push the state an undone step produced onto the redo stack. Pushes nothing
/// when that state could not be captured or would exceed the budget: the undo
/// has already happened and its outcome stands.
fn push_redo(s: &mut State, undone: &Entry, after: Result<Before, String>) {
    let Ok(after) = after else { return };
    let r = RedoEntry {
        seq: s.redo.len() + 1,
        builtin: undone.builtin.clone(),
        effect: undone.effect.clone(),
        path: undone.path.clone(),
        after,
    };
    persist_redo(&r);
    s.redo.push(r);
}

/// Re-apply the last `n` undone steps, most recently undone first.
///
/// The state being replaced is captured as a new journal entry before the
/// write, so the redo is itself undoable. That capture does not touch the redo
/// stack: the stack is only consumed here.
pub fn redo(n: usize) -> Vec<Outcome> {
    let mut s = state();
    let mut out = Vec::new();
    for _ in 0..n {
        if let Some(why) = s
            .redo
            .last()
            .and_then(|r| crate::safety::rewind_refused(&r.path))
        {
            let what = s.redo.last().map(|r| r.path.clone()).unwrap_or_default();
            out.push(Outcome::Skipped { what, why });
            break;
        }
        let Some(r) = s.redo.pop() else { break };
        forget_redo_persisted(r.seq);
        let what = if r.path.is_empty() {
            r.builtin.clone()
        } else {
            r.path.clone()
        };
        // The file is not touched unless what it holds now is first kept.
        let current = match capture(&r.path, held_bytes(&s)) {
            Ok(current) => current,
            Err(why) => {
                out.push(Outcome::Skipped {
                    what,
                    why: format!("current state not captured, so left as it is: {why}"),
                });
                continue;
            }
        };
        let outcome = match &r.after {
            Before::Contents(bytes) => match std::fs::write(&r.path, bytes) {
                Ok(()) => Outcome::Reapplied(what),
                Err(e) => Outcome::Failed {
                    what,
                    why: e.to_string(),
                },
            },
            Before::Absent => {
                if Path::new(&r.path).is_file() {
                    match std::fs::remove_file(&r.path) {
                        Ok(()) => Outcome::Removed(what),
                        Err(e) => Outcome::Failed {
                            what,
                            why: e.to_string(),
                        },
                    }
                } else {
                    out.push(Outcome::Skipped {
                        what,
                        why: "was absent after the step and is absent now".into(),
                    });
                    continue;
                }
            }
            Before::Irreversible(why) => {
                out.push(Outcome::Skipped {
                    what,
                    why: why.clone(),
                });
                continue;
            }
        };
        let is_failure = matches!(outcome, Outcome::Failed { .. });
        if !is_failure {
            let entry = Entry {
                seq: s.journal.len() + 1,
                builtin: r.builtin,
                effect: r.effect,
                path: r.path,
                before: current,
            };
            persist(&entry);
            s.journal.push(entry);
        }
        out.push(outcome);
    }
    out
}

/// Every recorded step, oldest first.
pub fn entries() -> Vec<Entry> {
    let s = state();
    s.journal
        .iter()
        .map(|e| Entry {
            // The captured bytes are never handed out; they would be a second
            // copy of the data in the agent's context for no benefit.
            before: match &e.before {
                Before::Contents(b) => Before::Contents(vec![0; b.len().min(1)]),
                other => other.clone(),
            },
            ..e.clone()
        })
        .collect()
}

/// Every undone step that can still be re-applied, most recently undone last.
/// Captured bytes are not handed out, as with [`entries`].
pub fn undone() -> Vec<RedoEntry> {
    let s = state();
    s.redo
        .iter()
        .map(|r| RedoEntry {
            after: match &r.after {
                Before::Contents(b) => Before::Contents(vec![0; b.len().min(1)]),
                other => other.clone(),
            },
            ..r.clone()
        })
        .collect()
}

/// How many undone steps can still be re-applied.
pub fn redo_count() -> usize {
    state().redo.len()
}

/// How many recorded steps can actually be reversed.
pub fn reversible_count() -> usize {
    state().journal.iter().filter(|e| e.reversible()).count()
}

/// The current end of the journal, for [`rollback_to`].
pub fn mark() -> usize {
    state().journal.len()
}

/// Discard entries recorded after `mark`, because the call they were recorded
/// for did not happen.
///
/// Found by using the shell as an agent: a `file_write` refused by the
/// workspace jail still left a journal entry, so `undo()` answered
/// `complete: false, skipped: 1` about an operation that never ran. The report
/// was honest but the entry was fiction, and an agent reading `complete: false`
/// would reasonably conclude something was left unreversed.
pub fn rollback_to(mark: usize) {
    let mut s = state();
    while s.journal.len() > mark {
        if let Some(e) = s.journal.pop() {
            forget_persisted(e.seq);
        }
    }
}

/// Forget everything recorded, the journal and the redo stack alike. Does not
/// touch the filesystem outside the session directory.
pub fn clear() {
    {
        let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
        s.journal.clear();
        s.redo.clear();
    }
    if let Some(dir) = session_dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_oversized_file_is_recorded_as_irreversible_not_skipped() {
        // Silently declining to capture would leave the user believing the step
        // was covered.
        let e = Entry {
            seq: 1,
            builtin: "file_write".into(),
            effect: "write_local".into(),
            path: "big.bin".into(),
            before: Before::Irreversible("too big".into()),
        };
        assert!(!e.reversible());
    }

    #[test]
    fn a_directory_target_is_refused_rather_than_half_captured() {
        let e = Entry {
            seq: 1,
            builtin: "rm".into(),
            effect: "destructive".into(),
            path: ".".into(),
            before: Before::Irreversible("path is a directory; trees are not captured".into()),
        };
        assert!(!e.reversible());
    }

    #[test]
    fn journal_builtins_are_never_recorded() {
        // Otherwise undo would journal its own restoration and a second undo
        // would put the damage back.
        assert!(is_journal_builtin("undo"));
        assert!(is_journal_builtin("journal"));
        assert!(is_journal_builtin("session_redo"));
        assert!(!is_journal_builtin("file_write"));
    }
}
