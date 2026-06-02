//! Filesystem transactions / checkpoints (docs/AGENTIC_FIRST_DESIGN.md §9).
//!
//! A capability no conventional shell offers: bracket a multi-step,
//! file-effecting agent action between `tx_begin` and `tx_commit`/`tx_rollback`,
//! and a failure midway can be **undone**. While a transaction is active,
//! effecting builtins call [`snapshot`] before they modify or delete a path; the
//! journal records the pre-modification state (a backup copy, or a "did not
//! exist" marker for newly-created paths). `rollback` replays those records in
//! reverse to restore the workspace to its pre-transaction state.
//!
//! **Nesting.** `begin` nests: a second `begin` while one is active pushes a child
//! frame (SQL nested-transaction semantics). A child `commit` keeps the child's
//! changes but folds them into the parent (so a later parent `rollback` still
//! undoes them — nothing is durable until the **outermost** commit); a child
//! `rollback` reverts only the child's operations, leaving the parent open. Each
//! frame keeps its own `seen` set and captures its own pre-image, so an inner
//! rollback restores a path to its pre-*inner* state even when an outer frame
//! also touched it, and replaying inner-then-outer undos in reverse restores the
//! pre-*outer* state. Named savepoints (`savepoint`/`rollback_to`) give SQL-style
//! partial rollback within the innermost frame. Files **and directory trees** (a
//! recursive `rmdir` backs up and restores the whole tree). All frames share one
//! backup store under `<workspace>/.ae/tx/<root-id>/`, removed when the outermost
//! frame ends. Pairs naturally with the safety model — a destructive batch can be
//! planned, approved, attempted, and rolled back atomically.

use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Undo {
    original: PathBuf,
    /// Backup copy of the prior content, or `None` if the path did not exist.
    backup: Option<PathBuf>,
    existed: bool,
    /// Whether the snapshotted path was a directory tree (vs a single file).
    is_dir: bool,
}

/// Recursively copy a directory tree `src` → `dst` (files and subdirectories;
/// symlinks are skipped in v1). Used to back up and restore directory trees.
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_tree(&from, &to)?;
        } else if ft.is_file() {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// One nesting level. `start` is the index into the shared `undos` where this
/// frame began; `seen` dedups snapshots *within this frame* so each level captures
/// its own pre-image; `savepoints` are partial-rollback markers within the frame.
struct Frame {
    id: String,
    start: usize,
    seen: HashSet<PathBuf>,
    savepoints: Vec<(String, usize)>,
}

/// The active transaction: a shared append-only undo journal plus a stack of
/// nesting frames (`frames[0]` is outermost). One backup directory + counter is
/// shared across all frames.
struct TxState {
    dir: PathBuf,
    undos: Vec<Undo>,
    n: u64,
    frames: Vec<Frame>,
}

/// Restore a single recorded path to its pre-modification state. Returns whether
/// anything was restored. Shared by full `rollback`, `commit`-fold, and `rollback_to`.
fn restore_one(u: &Undo) -> bool {
    if u.existed {
        if let Some(b) = &u.backup {
            if u.is_dir {
                let _ = std::fs::remove_dir_all(&u.original);
                copy_tree(b, &u.original).is_ok()
            } else {
                if let Some(parent) = u.original.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::copy(b, &u.original).is_ok()
            }
        } else {
            false
        }
    } else if u.original.is_file() {
        let _ = std::fs::remove_file(&u.original);
        true
    } else if u.original.is_dir() {
        let _ = std::fs::remove_dir_all(&u.original);
        true
    } else {
        false
    }
}

lazy_static! {
    static ref TX: Mutex<Option<TxState>> = Mutex::new(None);
}

fn absolutize(path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        crate::safety::workspace_root().join(p)
    }
}

fn new_id() -> String {
    format!(
        "tx_{}_{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

/// Whether a transaction is currently active (any nesting depth).
pub fn is_active() -> bool {
    TX.lock().map(|g| g.is_some()).unwrap_or(false)
}

/// Current nesting depth (0 when no transaction is active).
pub fn depth() -> usize {
    TX.lock()
        .ok()
        .and_then(|g| g.as_ref().map(|t| t.frames.len()))
        .unwrap_or(0)
}

/// `(innermost frame id, total recorded operation count)` if active.
pub fn status() -> Option<(String, usize)> {
    TX.lock().ok().and_then(|g| {
        g.as_ref().map(|t| {
            let id = t.frames.last().map(|f| f.id.clone()).unwrap_or_default();
            (id, t.undos.len())
        })
    })
}

/// Begin a transaction, or **nest** a child frame if one is already active.
/// Returns the new frame's id.
pub fn begin() -> Result<String> {
    let mut g = TX.lock().map_err(|_| anyhow!("tx lock poisoned"))?;
    let id = new_id();
    match g.as_mut() {
        Some(tx) => {
            // Nested begin: push a child frame over the shared journal.
            let start = tx.undos.len();
            tx.frames.push(Frame {
                id: id.clone(),
                start,
                seen: HashSet::new(),
                savepoints: Vec::new(),
            });
        }
        None => {
            let dir = crate::safety::workspace_root()
                .join(".ae")
                .join("tx")
                .join(&id);
            std::fs::create_dir_all(&dir)
                .map_err(|e| anyhow!("tx_begin: cannot create journal dir: {}", e))?;
            *g = Some(TxState {
                dir,
                undos: Vec::new(),
                n: 0,
                frames: vec![Frame {
                    id: id.clone(),
                    start: 0,
                    seen: HashSet::new(),
                    savepoints: Vec::new(),
                }],
            });
        }
    }
    Ok(id)
}

/// Record the pre-modification state of `path` in the innermost frame. No-op when
/// no transaction is active or when this frame already captured the path. Call
/// this *before* a builtin modifies or deletes the path.
pub fn snapshot(path: &str) {
    let mut g = match TX.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let tx = match g.as_mut() {
        Some(t) => t,
        None => return,
    };
    let abs = absolutize(path);
    // Per-frame dedup: the innermost frame captures its own pre-image even if an
    // outer frame already snapshotted this path (so inner rollback is correct).
    let already_captured = match tx.frames.last_mut() {
        Some(f) => !f.seen.insert(abs.clone()),
        None => return,
    };
    if already_captured {
        return;
    }
    let existed = abs.exists();
    let is_dir = existed && abs.is_dir();
    let backup = if existed && abs.is_file() {
        let bpath = tx.dir.join(format!("b{}", tx.n));
        tx.n += 1;
        std::fs::copy(&abs, &bpath).ok().map(|_| bpath)
    } else if is_dir {
        // Back up the whole tree so a recursive delete can be undone.
        let bpath = tx.dir.join(format!("d{}", tx.n));
        tx.n += 1;
        copy_tree(&abs, &bpath).ok().map(|_| bpath)
    } else {
        None
    };
    tx.undos.push(Undo {
        original: abs,
        backup,
        existed,
        is_dir,
    });
}

/// Commit the innermost frame. A **nested** commit keeps the frame's changes but
/// folds its undos into the parent (so a later parent rollback still reverts them
/// — nothing is durable until the outermost commit). The **outermost** commit makes
/// all changes durable and discards the journal. Returns the number of operations
/// recorded in the committed frame.
pub fn commit() -> Result<usize> {
    let mut g = TX.lock().map_err(|_| anyhow!("tx lock poisoned"))?;
    let tx = g
        .as_mut()
        .ok_or_else(|| anyhow!("tx_commit: no active transaction"))?;
    let frame = tx
        .frames
        .pop()
        .ok_or_else(|| anyhow!("tx_commit: no active transaction"))?;
    let ops = tx.undos.len().saturating_sub(frame.start);
    if tx.frames.is_empty() {
        // Outermost commit: changes become durable, journal discarded.
        let dir = tx.dir.clone();
        *g = None;
        let _ = std::fs::remove_dir_all(&dir);
    }
    // Nested commit: leave `undos` in place — they are now owned by the parent
    // frame's range and will be reverted by a parent rollback if it occurs.
    Ok(ops)
}

/// Roll back the innermost frame: restore the paths it recorded (replaying its
/// undos in reverse), leaving any parent frame open. If it was the outermost
/// frame, the journal is discarded. Returns the number of paths restored.
pub fn rollback() -> Result<usize> {
    let mut g = TX.lock().map_err(|_| anyhow!("tx lock poisoned"))?;
    let tx = g
        .as_mut()
        .ok_or_else(|| anyhow!("tx_rollback: no active transaction"))?;
    let frame = tx
        .frames
        .pop()
        .ok_or_else(|| anyhow!("tx_rollback: no active transaction"))?;
    let tail: Vec<Undo> = tx.undos.drain(frame.start..).collect();
    let mut restored = 0usize;
    for u in tail.iter().rev() {
        if restore_one(u) {
            restored += 1;
        }
    }
    if tx.frames.is_empty() {
        let dir = tx.dir.clone();
        *g = None;
        let _ = std::fs::remove_dir_all(&dir);
    }
    Ok(restored)
}

/// Mark a named savepoint within the innermost frame. A later `rollback_to`
/// reverts only the operations recorded after this point. Errors if no
/// transaction is active. Re-using a name adds a new savepoint; `rollback_to`
/// targets the most recent one with that name (SQL semantics).
pub fn savepoint(name: &str) -> Result<()> {
    let mut g = TX.lock().map_err(|_| anyhow!("tx lock poisoned"))?;
    let tx = g
        .as_mut()
        .ok_or_else(|| anyhow!("tx_savepoint: no active transaction"))?;
    let idx = tx.undos.len();
    let frame = tx
        .frames
        .last_mut()
        .ok_or_else(|| anyhow!("tx_savepoint: no active transaction"))?;
    frame.savepoints.push((name.to_string(), idx));
    Ok(())
}

/// Roll back to a named savepoint within the innermost frame: revert (in reverse)
/// every operation recorded after it, leaving the transaction open and the
/// savepoint itself intact (so it can be rolled back to again). Savepoints created
/// after it are released. Returns the number of paths restored. Errors if no
/// transaction is active or the savepoint is unknown.
pub fn rollback_to(name: &str) -> Result<usize> {
    let mut g = TX.lock().map_err(|_| anyhow!("tx lock poisoned"))?;
    let tx = g
        .as_mut()
        .ok_or_else(|| anyhow!("tx_rollback_to: no active transaction"))?;
    let (idx, pos) = {
        let frame = tx
            .frames
            .last()
            .ok_or_else(|| anyhow!("tx_rollback_to: no active transaction"))?;
        let pos = frame
            .savepoints
            .iter()
            .rposition(|(n, _)| n == name)
            .ok_or_else(|| anyhow!("tx_rollback_to: no savepoint named '{}'", name))?;
        (frame.savepoints[pos].1, pos)
    };
    // Detach the operations recorded after the savepoint, then revert them so
    // their paths can be re-snapshotted (within this frame) if modified again.
    let tail: Vec<Undo> = tx.undos.drain(idx..).collect();
    let mut restored = 0usize;
    {
        let frame = tx.frames.last_mut().unwrap();
        for u in tail.iter().rev() {
            frame.seen.remove(&u.original);
            if restore_one(u) {
                restored += 1;
            }
        }
        // Release savepoints created after the targeted one (keep it).
        frame.savepoints.truncate(pos + 1);
    }
    Ok(restored)
}
