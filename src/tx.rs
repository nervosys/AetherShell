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

use anyhow::Result;
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
    /// Set for a metadata-only record (permissions and, on Unix, ownership):
    /// no content is backed up and the content fields above are unused.
    meta: Option<MetaSnap>,
}

/// Permissions (and on Unix uid/gid) of the path as resolved through symlinks,
/// matching what `fs_chmod`/`fs_chown` change, plus its modified and accessed
/// times, which `touch` changes. Either time is `None` where the platform or
/// filesystem does not report it; such a time is left alone on restore.
struct MetaSnap {
    permissions: std::fs::Permissions,
    #[cfg(unix)]
    uid: u32,
    #[cfg(unix)]
    gid: u32,
    modified: Option<std::time::SystemTime>,
    accessed: Option<std::time::SystemTime>,
}

/// Open `path` only to set its times. A directory cannot be opened for
/// writing, so it is opened read-only on Unix (futimens needs ownership, not
/// a write handle) and with backup semantics and attribute access on Windows.
pub(crate) fn open_for_times(path: &Path) -> std::io::Result<std::fs::File> {
    if std::fs::metadata(path)?.is_dir() {
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
            return std::fs::OpenOptions::new()
                .access_mode(FILE_WRITE_ATTRIBUTES)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                .open(path);
        }
        #[cfg(not(windows))]
        return std::fs::File::open(path);
    }
    std::fs::File::options().write(true).open(path)
}

/// Set a path's modified and accessed times (a `None` leaves that time as it
/// is). Setting times needs a handle with write access, which a read-only file
/// refuses (on Windows always; on Unix for a non-owner). In that case the file
/// is made writable for this call only: `restore_one` restores the recorded
/// mode after it, so the temporary bits never outlive the rollback. That is
/// why the `set_readonly(false)` lint (which flags world-writable files) is
/// allowed here.
#[allow(clippy::permissions_set_readonly_false)]
fn set_times_of(
    path: &Path,
    accessed: Option<std::time::SystemTime>,
    modified: Option<std::time::SystemTime>,
) -> std::io::Result<()> {
    let mut times = std::fs::FileTimes::new();
    if let Some(t) = accessed {
        times = times.set_accessed(t);
    }
    if let Some(t) = modified {
        times = times.set_modified(t);
    }
    let open = || open_for_times(path);
    let file = match open() {
        Ok(f) => f,
        Err(e) => {
            let mut perms = std::fs::metadata(path)?.permissions();
            if !perms.readonly() {
                return Err(e);
            }
            // Only the owner's write bit, for as long as the times take; the
            // recorded mode goes back on right after. `set_readonly(false)`
            // on Unix would make the file writable by everyone meanwhile.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                perms.set_mode(perms.mode() | 0o200);
            }
            #[cfg(not(unix))]
            perms.set_readonly(false);
            std::fs::set_permissions(path, perms)?;
            open()?
        }
    };
    file.set_times(times)
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
            copy_file(&from, &to)?;
        }
    }
    Ok(())
}

/// Copy a file's content, then its permissions where the filesystem allows.
///
/// `std::fs::copy` fails as a whole when it cannot set the copy's permissions,
/// which a non-owner cannot do on some mounts (WSL's /mnt/c, many network
/// shares). The content had been copied, but the error made `snapshot` keep no
/// backup, so a rollback restored nothing and still reported success. The
/// content is what a rollback needs; the mode is restored when it can be.
fn copy_file(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut src = std::fs::File::open(from)?;
    let mut dst = std::fs::File::create(to)?;
    std::io::copy(&mut src, &mut dst)?;
    drop(dst);
    if let Ok(md) = std::fs::metadata(from) {
        let _ = std::fs::set_permissions(to, md.permissions());
    }
    Ok(())
}

/// A rollback that could not put everything back. The transaction is over
/// either way; the journal directory is kept, so what was not restored can be
/// recovered by hand.
fn rollback_incomplete(
    builtin: &str,
    failures: &[String],
    journal: Option<&Path>,
) -> anyhow::Error {
    let kept = journal
        .map(|d| format!("; the journal is kept at `{}`", d.display()))
        .unwrap_or_default();
    anyhow::Error::new(crate::safety::SafetyError {
        code: crate::safety::ErrorCode::Io,
        message: format!(
            "{builtin}: {} change(s) could not be restored: {}{kept}",
            failures.len(),
            failures.join("; ")
        ),
        builtin: builtin.to_string(),
        hint: "check those paths by hand; the rest of the transaction was undone".to_string(),
        approval: None,
        did_you_mean: Vec::new(),
        expected: String::new(),
        got: String::new(),
    })
}

/// One nesting level. `start` is the index into the shared `undos` where this
/// frame began; `seen` dedups snapshots *within this frame* so each level captures
/// its own pre-image; `savepoints` are partial-rollback markers within the frame.
struct Frame {
    id: String,
    start: usize,
    seen: HashSet<PathBuf>,
    /// Separate dedup set for metadata snapshots, so a content snapshot of a path
    /// never suppresses its metadata snapshot (or the reverse).
    seen_meta: HashSet<PathBuf>,
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
/// Undo one record. `Ok(true)` restored something, `Ok(false)` had nothing to
/// undo (a path that was to be created never was), and `Err` names what could
/// not be put back.
fn restore_one(u: &Undo) -> std::result::Result<bool, String> {
    let at = u.original.display();
    if let Some(m) = &u.meta {
        // Order: owner, then times, then the mode, last. chown can clear
        // setuid/setgid bits, so the mode follows it. Times need a writable
        // file, and the recorded mode may be read-only (the file was read-only
        // before the transaction, or fs_chmod made it so inside it): applying
        // that mode first would make the times write fail on Windows. Times
        // come first, and `set_times_of` copes with a file that is read-only
        // right now. The mode is applied even when the times fail, so a failed
        // time restore never leaves the wrong mode behind; the failure is still
        // reported.
        #[cfg(unix)]
        std::os::unix::fs::chown(&u.original, Some(m.uid), Some(m.gid))
            .map_err(|e| format!("`{at}` owner: {e}"))?;
        let times = if m.modified.is_some() || m.accessed.is_some() {
            set_times_of(&u.original, m.accessed, m.modified)
                .map_err(|e| format!("`{at}` times: {e}"))
        } else {
            Ok(())
        };
        std::fs::set_permissions(&u.original, m.permissions.clone())
            .map_err(|e| format!("`{at}` permissions: {e}"))?;
        times?;
        return Ok(true);
    }
    if u.existed {
        let Some(b) = &u.backup else {
            return Err(format!(
                "`{at}`: no backup could be taken before it changed"
            ));
        };
        if u.is_dir {
            let _ = std::fs::remove_dir_all(&u.original);
            copy_tree(b, &u.original).map_err(|e| format!("`{at}`: {e}"))?;
        } else {
            if let Some(parent) = u.original.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            copy_file(b, &u.original).map_err(|e| format!("`{at}`: {e}"))?;
        }
        Ok(true)
    } else if let Some(removed) = remove_created_symlink(&u.original) {
        if removed {
            Ok(true)
        } else {
            Err(format!("`{at}`: the link it created could not be removed"))
        }
    } else if u.original.is_file() {
        std::fs::remove_file(&u.original).map_err(|e| format!("`{at}`: {e}"))?;
        Ok(true)
    } else if u.original.is_dir() {
        std::fs::remove_dir_all(&u.original).map_err(|e| format!("`{at}`: {e}"))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// If `path` is itself a symlink, remove the link (never its target) and return
/// `Some(true)`; return `None` when it is not a symlink. `is_file`/`is_dir`
/// follow links, so a link to a directory would reach `remove_dir_all` on the
/// link path and a dangling link would be left behind; `symlink_metadata`
/// inspects the link itself and deletion must never follow it.
fn remove_created_symlink(path: &Path) -> Option<bool> {
    let md = std::fs::symlink_metadata(path).ok()?;
    if !md.file_type().is_symlink() {
        return None;
    }
    #[cfg(windows)]
    let result = {
        use std::os::windows::fs::FileTypeExt;
        if md.file_type().is_symlink_dir() {
            std::fs::remove_dir(path)
        } else {
            std::fs::remove_file(path)
        }
    };
    #[cfg(not(windows))]
    let result = std::fs::remove_file(path);
    Some(result.is_ok())
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

/// Builtins whose effect a rollback cannot restore: they send, start, stop,
/// install, signal or commit something, or write through an external tool
/// that no snapshot names. The dispatcher refuses them while a transaction is
/// active, before validation or the body runs, so `tx_rollback` never reports
/// a clean undo over a change it left in place. Builtins that refuse or
/// snapshot in their own bodies are not listed. `tests/tx_irreversible.rs`
/// pins this list against every builtin classified as more than a read.
pub const IRREVERSIBLE: &[&str] = &[
    "act",
    "add_entity",
    "add_node",
    "add_relation",
    "age_decrypt",
    "age_encrypt",
    "age_keygen",
    "agent_with_mcp",
    "ansible_galaxy",
    "ansible_playbook",
    "ansible_vault",
    "asdf_install",
    "black",
    "buildah_build",
    "bun_install",
    "bun_run",
    "bzip2_decompress",
    "cargo_build_cmd",
    "cargo_run",
    "cargo_test_cmd",
    "chgrp",
    "clipboard_clear",
    "clipboard_set",
    "clipboard_set_image",
    "cloud_instance_destroy",
    "cluster",
    "cmake_build",
    "cmake_configure",
    "config_init",
    "config_set",
    "db_sqlite_open",
    "delete_role",
    "deno_run",
    "deno_task",
    "diag_all",
    "diag_check",
    "diag_errors",
    "diag_fix",
    "diag_lint",
    "diag_warnings",
    "direnv_allow",
    "docker_build",
    "docker_compose_down",
    "docker_compose_up",
    "docker_cp",
    "docker_exec",
    "docker_pull",
    "docker_push",
    "docker_rm",
    "docker_run",
    "docker_stop",
    "docker_tag",
    "docs_generate",
    "gdb",
    "gh_issue",
    "gh_pr",
    "git_add",
    "git_checkout",
    "git_cherry_pick",
    "git_clean",
    "git_commit",
    "git_fetch",
    "git_merge",
    "git_pull",
    "git_push",
    "git_rebase",
    "git_reset",
    "git_stash",
    "git_stash_pop",
    "glab_issue",
    "glab_mr",
    "go_build",
    "go_run",
    "go_test",
    "gpg_decrypt",
    "gpg_encrypt",
    "gui_close_window",
    "gui_focus_window",
    "gui_key_combo",
    "gui_key_press",
    "gui_maximize_window",
    "gui_minimize_window",
    "gui_mouse_click",
    "gui_mouse_double_click",
    "gui_mouse_drag",
    "gui_mouse_move",
    "gui_mouse_scroll",
    "gui_move_window",
    "gui_notify",
    "gui_resize_window",
    "gui_screenshot",
    "gui_type_text",
    "helm_install",
    "helm_uninstall",
    "helm_upgrade",
    "hyperfine",
    "index_docs",
    "init_sso",
    "just",
    "k8s_apply",
    "k8s_context",
    "k8s_delete",
    "k8s_exec",
    "k8s_rollout_restart",
    "k8s_scale",
    "kill",
    "lazygit",
    "lldb",
    "logrotate",
    "ltrace",
    "mage",
    "make_run",
    "marketplace_install",
    "marketplace_publish",
    "marketplace_rate",
    "marketplace_uninstall",
    "mcp_call",
    "mise_use",
    "monitor_alert_create",
    "monitor_alert_remove",
    "monitor_perf_record",
    "monitor_perf_stat",
    "ninja_build",
    "node_run",
    "npm_install",
    "npm_run",
    "openssl_genrsa",
    "pipx_install",
    "pkill",
    "platform_db_delete",
    "platform_db_import",
    "platform_db_init",
    "platform_db_store",
    "plugin_disable",
    "plugin_enable",
    "plugin_load",
    "plugin_unload",
    "pnpm_install",
    "pnpm_run",
    "podman_build",
    "podman_exec",
    "podman_pull",
    "podman_rm",
    "podman_run",
    "podman_stop",
    "poetry_install",
    "poetry_run",
    "pre_commit_install",
    "pre_commit_run",
    "prettier",
    "proc_resume",
    "proc_set_priority",
    "proc_spawn",
    "proc_suspend",
    "pytest",
    "rbac_grant",
    "rbac_login",
    "rbac_logout",
    "rbac_principal",
    "rbac_register",
    "recursive_agent",
    "refactor_remove_unused",
    "rlm_spawn",
    "rsync",
    "ruff_format",
    "rustup_update",
    "scp_download",
    "scp_upload",
    "screen_attach",
    "screen_new",
    "session_checkpoint",
    "session_end",
    "session_redo",
    "session_restore",
    "session_rollback",
    "session_undo",
    "sh",
    "skopeo_copy",
    "ssh_copy_id",
    "ssh_exec",
    "ssh_keygen",
    "ssh_tunnel",
    "strace",
    "svc_disable",
    "svc_enable",
    "svc_restart",
    "svc_start",
    "svc_stop",
    "syntax_add",
    "tar_create",
    "task_run",
    "terraform_apply",
    "terraform_destroy",
    "terraform_init",
    "terraform_workspace",
    "test_bench",
    "test_list",
    "test_run",
    "test_run_file",
    "test_run_function",
    "test_watch",
    "timeout",
    "tmux_attach",
    "tmux_kill",
    "tmux_new",
    "tmux_send",
    "tool_exec",
    "uv_pip",
    "uv_run",
    "uv_venv",
    "valgrind",
    "valgrind_memcheck",
    "web_browser_open",
    "web_form_submit",
    "web_graphql",
    "web_json_post",
    "web_open_url",
    "web_post",
    "web_rest_api",
    "web_upload_file",
    "web_websocket",
    "xargs",
    "xz_compress",
    "xz_decompress",
    "yarn_install",
    "yarn_run",
    "zoxide_add",
    "zstd_compress",
    "zstd_decompress",
];

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
    let mut g = TX
        .lock()
        .map_err(|_| crate::safety::lock_poisoned("tx_begin", "the transaction state"))?;
    let id = new_id();
    match g.as_mut() {
        Some(tx) => {
            // Nested begin: push a child frame over the shared journal.
            let start = tx.undos.len();
            tx.frames.push(Frame {
                id: id.clone(),
                start,
                seen: HashSet::new(),
                seen_meta: HashSet::new(),
                savepoints: Vec::new(),
            });
        }
        None => {
            let dir = crate::safety::workspace_root()
                .join(".ae")
                .join("tx")
                .join(&id);
            // Coded, so a read-only or full workspace reads as that, not as
            // an unknown fault.
            std::fs::create_dir_all(&dir)
                .map_err(|e| crate::safety::fs_error("tx_begin", &dir, &e))?;
            *g = Some(TxState {
                dir,
                undos: Vec::new(),
                n: 0,
                frames: vec![Frame {
                    id: id.clone(),
                    start: 0,
                    seen: HashSet::new(),
                    seen_meta: HashSet::new(),
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
    // symlink_metadata: a dangling link exists (as a link) even though `exists()`
    // follows it and says no.
    let existed = std::fs::symlink_metadata(&abs).is_ok();
    let is_dir = existed && abs.is_dir();
    let backup = if existed && abs.is_file() {
        let bpath = tx.dir.join(format!("b{}", tx.n));
        tx.n += 1;
        copy_file(&abs, &bpath).ok().map(|_| bpath)
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
        meta: None,
    });
}

/// Record the permissions, the modified and accessed times (and, on Unix,
/// uid/gid) of `path` in the innermost frame, without backing up content. Symlinks are followed, matching `fs_chmod`
/// and `fs_chown`. No-op when inactive, when the frame already captured this
/// path's metadata, or when the path cannot be inspected (the change will then
/// fail on its own).
pub fn snapshot_metadata(path: &str) {
    let mut g = match TX.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let tx = match g.as_mut() {
        Some(t) => t,
        None => return,
    };
    let abs = absolutize(path);
    let md = match std::fs::metadata(&abs) {
        Ok(m) => m,
        Err(_) => return,
    };
    match tx.frames.last_mut() {
        Some(f) => {
            if !f.seen_meta.insert(abs.clone()) {
                return;
            }
        }
        None => return,
    }
    #[cfg(unix)]
    let (uid, gid) = {
        use std::os::unix::fs::MetadataExt;
        (md.uid(), md.gid())
    };
    tx.undos.push(Undo {
        original: abs,
        backup: None,
        existed: true,
        is_dir: false,
        meta: Some(MetaSnap {
            permissions: md.permissions(),
            #[cfg(unix)]
            uid,
            #[cfg(unix)]
            gid,
            modified: md.modified().ok(),
            accessed: md.accessed().ok(),
        }),
    });
}

/// Commit the innermost frame. A **nested** commit keeps the frame's changes but
/// folds its undos into the parent (so a later parent rollback still reverts them
/// — nothing is durable until the outermost commit). The **outermost** commit makes
/// all changes durable and discards the journal. Returns the number of operations
/// recorded in the committed frame.
pub fn commit() -> Result<usize> {
    let mut g = TX
        .lock()
        .map_err(|_| crate::safety::lock_poisoned("tx_commit", "the transaction state"))?;
    let tx = g.as_mut().ok_or_else(|| {
        crate::safety::bad_state("tx_commit", "no active transaction", "tx_begin")
    })?;
    let frame = tx.frames.pop().ok_or_else(|| {
        crate::safety::bad_state("tx_commit", "no active transaction", "tx_begin")
    })?;
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
    let mut g = TX
        .lock()
        .map_err(|_| crate::safety::lock_poisoned("tx_rollback", "the transaction state"))?;
    let tx = g.as_mut().ok_or_else(|| {
        crate::safety::bad_state("tx_rollback", "no active transaction", "tx_begin")
    })?;
    let frame = tx.frames.pop().ok_or_else(|| {
        crate::safety::bad_state("tx_rollback", "no active transaction", "tx_begin")
    })?;
    let tail: Vec<Undo> = tx.undos.drain(frame.start..).collect();
    let mut restored = 0usize;
    let mut failures = Vec::new();
    for u in tail.iter().rev() {
        match restore_one(u) {
            Ok(true) => restored += 1,
            Ok(false) => {}
            Err(why) => failures.push(why),
        }
    }
    let mut kept = None;
    if tx.frames.is_empty() {
        let dir = tx.dir.clone();
        *g = None;
        if failures.is_empty() {
            let _ = std::fs::remove_dir_all(&dir);
        } else {
            kept = Some(dir);
        }
    }
    if !failures.is_empty() {
        return Err(rollback_incomplete(
            "tx_rollback",
            &failures,
            kept.as_deref(),
        ));
    }
    Ok(restored)
}

/// Mark a named savepoint within the innermost frame. A later `rollback_to`
/// reverts only the operations recorded after this point. Errors if no
/// transaction is active. Re-using a name adds a new savepoint; `rollback_to`
/// targets the most recent one with that name (SQL semantics).
pub fn savepoint(name: &str) -> Result<()> {
    let mut g = TX
        .lock()
        .map_err(|_| crate::safety::lock_poisoned("tx_savepoint", "the transaction state"))?;
    let tx = g.as_mut().ok_or_else(|| {
        crate::safety::bad_state("tx_savepoint", "no active transaction", "tx_begin")
    })?;
    let idx = tx.undos.len();
    let frame = tx.frames.last_mut().ok_or_else(|| {
        crate::safety::bad_state("tx_savepoint", "no active transaction", "tx_begin")
    })?;
    frame.savepoints.push((name.to_string(), idx));
    // A path captured before this point is in `seen`, so a later write to it
    // would not be snapshotted again, and rollback_to could not undo that
    // write. Forget the dedup sets: the next write re-captures the state the
    // savepoint froze. Replaying undos in reverse still restores the earlier
    // pre-image on a full rollback.
    frame.seen.clear();
    frame.seen_meta.clear();
    Ok(())
}

/// Roll back to a named savepoint within the innermost frame: revert (in reverse)
/// every operation recorded after it, leaving the transaction open and the
/// savepoint itself intact (so it can be rolled back to again). Savepoints created
/// after it are released. Returns the number of paths restored. Errors if no
/// transaction is active or the savepoint is unknown.
pub fn rollback_to(name: &str) -> Result<usize> {
    let mut g = TX
        .lock()
        .map_err(|_| crate::safety::lock_poisoned("tx_rollback_to", "the transaction state"))?;
    let tx = g.as_mut().ok_or_else(|| {
        crate::safety::bad_state("tx_rollback_to", "no active transaction", "tx_begin")
    })?;
    let (idx, pos) = {
        let frame = tx.frames.last().ok_or_else(|| {
            crate::safety::bad_state("tx_rollback_to", "no active transaction", "tx_begin")
        })?;
        let pos = frame
            .savepoints
            .iter()
            .rposition(|(n, _)| n == name)
            .ok_or_else(|| {
                crate::safety::bad_state(
                    "tx_rollback_to",
                    &format!("no savepoint named '{}'", name),
                    "tx_savepoint(name) inside the transaction",
                )
            })?;
        (frame.savepoints[pos].1, pos)
    };
    // Detach the operations recorded after the savepoint, then revert them so
    // their paths can be re-snapshotted (within this frame) if modified again.
    let tail: Vec<Undo> = tx.undos.drain(idx..).collect();
    let mut restored = 0usize;
    let mut failures = Vec::new();
    {
        let frame = tx.frames.last_mut().unwrap();
        for u in tail.iter().rev() {
            if u.meta.is_some() {
                frame.seen_meta.remove(&u.original);
            } else {
                frame.seen.remove(&u.original);
            }
            match restore_one(u) {
                Ok(true) => restored += 1,
                Ok(false) => {}
                Err(why) => failures.push(why),
            }
        }
        // Release savepoints created after the targeted one (keep it).
        frame.savepoints.truncate(pos + 1);
    }
    if !failures.is_empty() {
        return Err(rollback_incomplete("tx_rollback_to", &failures, None));
    }
    Ok(restored)
}
