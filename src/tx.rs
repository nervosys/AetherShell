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
//! Scope (v1): single (non-nested) transaction with **named savepoints**
//! (`savepoint`/`rollback_to` for partial rollback, SQL-style); files **and
//! directory trees** (a recursive `rmdir` backs up and restores the whole tree).
//! Backups live under `<workspace>/.ae/tx/<id>/`. Pairs naturally with the safety
//! model — a destructive batch can be planned, approved, attempted, and rolled
//! back atomically.

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

struct Transaction {
    id: String,
    dir: PathBuf,
    undos: Vec<Undo>,
    seen: HashSet<PathBuf>,
    n: u64,
    /// Named savepoints: `(name, index into `undos`)`. `rollback_to` reverts the
    /// operations recorded after a savepoint while leaving the transaction open.
    savepoints: Vec<(String, usize)>,
}

/// Restore a single recorded path to its pre-modification state. Returns whether
/// anything was restored. Shared by full `rollback` and `rollback_to`.
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
    static ref TX: Mutex<Option<Transaction>> = Mutex::new(None);
}

fn absolutize(path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        crate::safety::workspace_root().join(p)
    }
}

/// Whether a transaction is currently active.
pub fn is_active() -> bool {
    TX.lock().map(|g| g.is_some()).unwrap_or(false)
}

/// `(transaction id, recorded operation count)` if a transaction is active.
pub fn status() -> Option<(String, usize)> {
    TX.lock()
        .ok()
        .and_then(|g| g.as_ref().map(|t| (t.id.clone(), t.undos.len())))
}

/// Begin a transaction. Errors if one is already active (no nesting in v1).
pub fn begin() -> Result<String> {
    let mut g = TX.lock().map_err(|_| anyhow!("tx lock poisoned"))?;
    if g.is_some() {
        return Err(anyhow!(
            "a transaction is already active; commit or rollback first"
        ));
    }
    let id = format!(
        "tx_{}_{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    );
    let dir = crate::safety::workspace_root()
        .join(".ae")
        .join("tx")
        .join(&id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| anyhow!("tx_begin: cannot create journal dir: {}", e))?;
    *g = Some(Transaction {
        id: id.clone(),
        dir,
        undos: Vec::new(),
        seen: HashSet::new(),
        n: 0,
        savepoints: Vec::new(),
    });
    Ok(id)
}

/// Record the pre-modification state of `path`. No-op when no transaction is
/// active or when the path's pre-transaction state was already captured. Call
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
    if !tx.seen.insert(abs.clone()) {
        return; // earliest (pre-tx) state already recorded for this path
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

/// Commit the active transaction: keep all changes, discard the journal.
/// Returns the number of recorded operations.
pub fn commit() -> Result<usize> {
    let mut g = TX.lock().map_err(|_| anyhow!("tx lock poisoned"))?;
    let tx = g
        .take()
        .ok_or_else(|| anyhow!("tx_commit: no active transaction"))?;
    let n = tx.undos.len();
    let _ = std::fs::remove_dir_all(&tx.dir);
    Ok(n)
}

/// Roll back the active transaction: restore every recorded path to its
/// pre-transaction state (replaying undos in reverse). Returns the number of
/// paths restored.
pub fn rollback() -> Result<usize> {
    let mut g = TX.lock().map_err(|_| anyhow!("tx lock poisoned"))?;
    let tx = g
        .take()
        .ok_or_else(|| anyhow!("tx_rollback: no active transaction"))?;
    let mut restored = 0usize;
    for u in tx.undos.iter().rev() {
        if restore_one(u) {
            restored += 1;
        }
    }
    let _ = std::fs::remove_dir_all(&tx.dir);
    Ok(restored)
}

/// Mark a named savepoint within the active transaction. A later `rollback_to`
/// reverts only the operations recorded after this point. Errors if no
/// transaction is active. Re-using a name adds a new savepoint; `rollback_to`
/// targets the most recent one with that name (SQL semantics).
pub fn savepoint(name: &str) -> Result<()> {
    let mut g = TX.lock().map_err(|_| anyhow!("tx lock poisoned"))?;
    let tx = g
        .as_mut()
        .ok_or_else(|| anyhow!("tx_savepoint: no active transaction"))?;
    let idx = tx.undos.len();
    tx.savepoints.push((name.to_string(), idx));
    Ok(())
}

/// Roll back to a named savepoint: revert (in reverse) every operation recorded
/// after it, leaving the transaction open and the savepoint itself intact (so it
/// can be rolled back to again). Savepoints created after it are released.
/// Returns the number of paths restored. Errors if no transaction is active or
/// the savepoint is unknown.
pub fn rollback_to(name: &str) -> Result<usize> {
    let mut g = TX.lock().map_err(|_| anyhow!("tx lock poisoned"))?;
    let tx = g
        .as_mut()
        .ok_or_else(|| anyhow!("tx_rollback_to: no active transaction"))?;
    let pos = tx
        .savepoints
        .iter()
        .rposition(|(n, _)| n == name)
        .ok_or_else(|| anyhow!("tx_rollback_to: no savepoint named '{}'", name))?;
    let idx = tx.savepoints[pos].1;
    // Detach the operations recorded after the savepoint, then revert them so
    // their paths can be re-snapshotted if modified again.
    let tail: Vec<Undo> = tx.undos.drain(idx..).collect();
    let mut restored = 0usize;
    for u in tail.iter().rev() {
        tx.seen.remove(&u.original);
        if restore_one(u) {
            restored += 1;
        }
    }
    // Release savepoints created after the targeted one (keep it).
    tx.savepoints.truncate(pos + 1);
    Ok(restored)
}
