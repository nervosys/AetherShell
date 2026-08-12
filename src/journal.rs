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
use std::path::Path;
use std::sync::Mutex;

/// Largest single file captured. Beyond this the step is recorded as
/// irreversible rather than silently skipped.
const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;

/// Ceiling on everything held at once, so a long session cannot exhaust memory.
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

lazy_static::lazy_static! {
    static ref JOURNAL: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
}

/// Where this session's journal lives.
///
/// Persistence is not a nicety here. The Python SDK runs `ae -c <code>` per
/// call, so an in-memory journal would be empty in the process that runs
/// `undo` — the feature would appear to work and reverse nothing, which is the
/// exact failure this module is written to avoid.
fn session_dir() -> Option<std::path::PathBuf> {
    if let Ok(d) = std::env::var("AETHER_SESSION_DIR") {
        return Some(std::path::PathBuf::from(d).join("journal"));
    }
    let key = std::env::var("AETHER_SESSION").ok().unwrap_or_else(|| {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("{:x}", md5::compute(cwd.as_bytes()))
    });
    dirs::cache_dir().map(|c| c.join("aethershell").join("journal").join(key))
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

/// Read the persisted journal, oldest first.
fn load_persisted() -> Vec<Entry> {
    let Some(dir) = session_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<(usize, std::path::PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
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

fn persist(e: &Entry) {
    let Some(dir) = session_dir() else { return };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_vec(&StoredEntry::from(e)) {
        let _ = std::fs::write(dir.join(format!("{:08}.json", e.seq)), json);
    }
}

fn forget_persisted(seq: usize) {
    if let Some(dir) = session_dir() {
        let _ = std::fs::remove_file(dir.join(format!("{seq:08}.json")));
    }
}

/// Merge the persisted journal into memory so this process sees steps recorded
/// by earlier ones.
fn hydrate(j: &mut Vec<Entry>) {
    if !j.is_empty() {
        return;
    }
    *j = load_persisted();
}

/// Journalling is on in agent mode and off for humans, matching the rest of the
/// dual-surface split: the agent surface pays a little I/O for recoverability,
/// the human REPL behaves exactly as before. `AETHER_JOURNAL=on`/`off` forces it.
pub fn enabled() -> bool {
    match std::env::var("AETHER_JOURNAL").ok().as_deref() {
        Some("on") | Some("1") | Some("true") => true,
        Some("off") | Some("0") | Some("false") => false,
        _ => crate::safety::current_mode() == crate::safety::Mode::Agent,
    }
}

/// Builtins that operate on the journal itself. Recording their writes would
/// make `undo` undo its own restoration.
fn is_journal_builtin(name: &str) -> bool {
    matches!(name, "undo" | "journal" | "journal_clear" | "rewind")
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

/// Record the pre-state of anything `builtin` might overwrite. Call immediately
/// before dispatch.
pub fn record_before(builtin: &str, effect: Effect, args: &[Value]) {
    if !enabled() || is_journal_builtin(builtin) {
        return;
    }
    if !matches!(effect, Effect::WriteLocal | Effect::Destructive) {
        return;
    }
    let paths = candidate_paths(args);
    if paths.is_empty() {
        return;
    }
    let mut j = JOURNAL.lock().unwrap_or_else(|e| e.into_inner());
    hydrate(&mut j);
    let used = total_bytes(&j);
    for path in paths {
        let p = Path::new(&path);
        let before = if p.is_file() {
            match std::fs::metadata(p).map(|m| m.len() as usize) {
                Ok(sz) if sz > MAX_FILE_BYTES => Before::Irreversible(format!(
                    "{sz} bytes exceeds the {MAX_FILE_BYTES}-byte capture limit"
                )),
                Ok(sz) if used + sz > MAX_TOTAL_BYTES => {
                    Before::Irreversible("session capture budget exhausted".into())
                }
                Ok(_) => match std::fs::read(p) {
                    Ok(bytes) => Before::Contents(bytes),
                    Err(e) => Before::Irreversible(format!("unreadable: {e}")),
                },
                Err(e) => Before::Irreversible(format!("unstattable: {e}")),
            }
        } else if p.exists() {
            // A directory: restoring a tree is out of scope, and pretending
            // otherwise is the failure this module exists to avoid.
            Before::Irreversible("path is a directory; trees are not captured".into())
        } else {
            Before::Absent
        };
        let seq = j.len() + 1;
        let entry = Entry {
            seq,
            builtin: builtin.to_string(),
            effect: effect.as_str().to_string(),
            path,
            before,
        };
        persist(&entry);
        j.push(entry);
    }
}

/// Note a step that changed the world in a way no snapshot can capture, so a
/// later rewind can say so.
pub fn record_irreversible(builtin: &str, effect: Effect, why: &str) {
    if !enabled() || is_journal_builtin(builtin) {
        return;
    }
    let mut j = JOURNAL.lock().unwrap_or_else(|e| e.into_inner());
    hydrate(&mut j);
    let seq = j.len() + 1;
    let entry = Entry {
        seq,
        builtin: builtin.to_string(),
        effect: effect.as_str().to_string(),
        path: String::new(),
        before: Before::Irreversible(why.to_string()),
    };
    persist(&entry);
    j.push(entry);
}

/// The outcome of rewinding one entry.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Restored(String),
    Removed(String),
    Skipped { what: String, why: String },
    Failed { what: String, why: String },
}

/// Undo the last `n` recorded steps, most recent first.
///
/// Returns an outcome per entry — including the ones it could not reverse.
/// Callers must surface those: a rewind that reports only its successes is how
/// a partial restore gets mistaken for a complete one.
pub fn undo(n: usize) -> Vec<Outcome> {
    let mut j = JOURNAL.lock().unwrap_or_else(|e| e.into_inner());
    hydrate(&mut j);
    let mut out = Vec::new();
    for _ in 0..n {
        let Some(entry) = j.pop() else { break };
        forget_persisted(entry.seq);
        let what = if entry.path.is_empty() {
            entry.builtin.clone()
        } else {
            entry.path.clone()
        };
        match entry.before {
            Before::Contents(bytes) => match std::fs::write(&entry.path, &bytes) {
                Ok(()) => out.push(Outcome::Restored(what)),
                Err(e) => out.push(Outcome::Failed {
                    what,
                    why: e.to_string(),
                }),
            },
            Before::Absent => {
                let p = Path::new(&entry.path);
                if p.is_file() {
                    match std::fs::remove_file(p) {
                        Ok(()) => out.push(Outcome::Removed(what)),
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
            Before::Irreversible(why) => out.push(Outcome::Skipped { what, why }),
        }
    }
    out
}

/// Every recorded step, oldest first.
pub fn entries() -> Vec<Entry> {
    let mut j = JOURNAL.lock().unwrap_or_else(|e| e.into_inner());
    hydrate(&mut j);
    j.iter()
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

/// How many recorded steps can actually be reversed.
pub fn reversible_count() -> usize {
    let mut j = JOURNAL.lock().unwrap_or_else(|e| e.into_inner());
    hydrate(&mut j);
    j.iter().filter(|e| e.reversible()).count()
}

/// The current end of the journal, for [`rollback_to`].
pub fn mark() -> usize {
    let mut j = JOURNAL.lock().unwrap_or_else(|e| e.into_inner());
    hydrate(&mut j);
    j.len()
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
    let mut j = JOURNAL.lock().unwrap_or_else(|e| e.into_inner());
    while j.len() > mark {
        if let Some(e) = j.pop() {
            forget_persisted(e.seq);
        }
    }
}

/// Forget everything recorded. Does not touch the filesystem.
pub fn clear() {
    JOURNAL.lock().unwrap_or_else(|e| e.into_inner()).clear();
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
        assert!(!is_journal_builtin("file_write"));
    }
}
