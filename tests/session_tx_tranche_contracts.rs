//! Contracts for the `session_*` and `tx_*` builtins.
//!
//! Every case drives the real `ae` binary with `-c`, from a sandbox that
//! this test made under the system temp directory. The sandbox has a `work`
//! directory (the cwd, the workspace and, for the git cases, the repository)
//! and a `state` directory (`AETHER_SESSION_DIR`). The undo journal and the
//! transaction journal are keyed by directory, so no case can reach the
//! journal of a real checkout. Nothing here calls a mutating builtin in
//! process: the repository's own journal is never touched.
//!
//! Each child has a hard deadline of 30 seconds and a closed stdin.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(30);

/// A throwaway tree that only this test creates and removes.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("ae_session_tx_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("work")).expect("create work dir");
        std::fs::create_dir_all(root.join("state")).expect("create state dir");
        Sandbox { root }
    }

    fn work(&self) -> PathBuf {
        self.root.join("work")
    }

    fn file(&self, name: &str) -> PathBuf {
        self.work().join(name)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Out {
    ok: bool,
    stdout: String,
    stderr: String,
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// Run `ae -c expr` in the sandbox's work directory.
fn ae(sb: &Sandbox, expr: &str) -> Out {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ae"))
        .args(["-c", expr])
        .current_dir(sb.work())
        .env("AETHER_WORKSPACE", sb.work())
        .env("AETHER_SESSION_DIR", sb.root.join("state"))
        // A git call in the sandbox must not find a repository above it.
        .env("GIT_CEILING_DIRECTORIES", &sb.root)
        .env("GIT_AUTHOR_NAME", "ae test")
        .env("GIT_AUTHOR_EMAIL", "ae-test@example.invalid")
        .env("GIT_COMMITTER_NAME", "ae test")
        .env("GIT_COMMITTER_EMAIL", "ae-test@example.invalid")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ae");
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll ae") {
            break status;
        }
        if start.elapsed() > DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            panic!("ae -c {expr:?} ran past {DEADLINE:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Out {
        ok: status.success(),
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    }
}

fn ok(sb: &Sandbox, expr: &str) -> Out {
    let out = ae(sb, expr);
    assert!(
        out.ok,
        "ae -c {expr:?} failed\nstdout: {}\nstderr: {}",
        out.stdout, out.stderr
    );
    out
}

fn refused(sb: &Sandbox, expr: &str, code: &str) -> Out {
    let out = ae(sb, expr);
    assert!(
        !out.ok,
        "ae -c {expr:?} was accepted; it should be refused as {code}\nstdout: {}",
        out.stdout
    );
    assert!(
        out.stderr.contains(&format!("error[{code}]")),
        "ae -c {expr:?} should fail as {code}\nstderr: {}",
        out.stderr
    );
    out
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "ae test")
        .env("GIT_AUTHOR_EMAIL", "ae-test@example.invalid")
        .env("GIT_COMMITTER_NAME", "ae test")
        .env("GIT_COMMITTER_EMAIL", "ae-test@example.invalid")
        .stdin(Stdio::null())
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A repository in the sandbox's work directory with one commit: f.txt = "one".
fn repo_with_commit(sb: &Sandbox) {
    git(&sb.work(), &["init", "-q"]);
    // No trailing newline: a checkout on a host with autocrlf rewrites line endings.
    std::fs::write(sb.file("f.txt"), "one").expect("write f.txt");
    git(&sb.work(), &["add", "f.txt"]);
    git(&sb.work(), &["commit", "-q", "-m", "init"]);
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

// ── transactions: round trips on files, verified independently ───────────

#[test]
fn begin_savepoint_rollback_to_commit_round_trip_on_files() {
    let sb = Sandbox::new("tx_round_trip");
    std::fs::write(sb.file("keep.txt"), "keep").unwrap();
    std::fs::write(sb.file("edit.txt"), "original").unwrap();

    // One process: a transaction lives in memory for this run only.
    let out = ok(
        &sb,
        r#"tx_begin(); file_write("edit.txt", "changed-1"); tx_savepoint("mid"); file_write("edit.txt", "changed-2"); file_write("made.txt", "new"); tx_rollback_to("mid"); tx_commit()"#,
    );
    assert!(out.stdout.contains("committed: true"), "{}", out.stdout);
    // Only the write made before the savepoint is kept on the commit.
    assert!(out.stdout.contains("operations: 1"), "{}", out.stdout);

    assert_eq!(read(&sb.file("edit.txt")), "changed-1");
    assert_eq!(read(&sb.file("keep.txt")), "keep");
    assert!(!sb.file("made.txt").exists(), "made.txt must be undone");
    // The outermost commit removes the journal it was keeping.
    let journal = sb.work().join(".ae").join("tx");
    let left = std::fs::read_dir(&journal).map(|d| d.count()).unwrap_or(0);
    assert_eq!(left, 0, "journal entries left after commit");
}

#[test]
fn begin_then_rollback_restores_every_file_it_touched() {
    let sb = Sandbox::new("tx_rollback");
    std::fs::write(sb.file("a.txt"), "A").unwrap();
    std::fs::write(sb.file("b.txt"), "B").unwrap();

    let out = ok(
        &sb,
        r#"tx_begin(); file_write("a.txt", "A2"); file_write("new.txt", "N"); tx_rollback()"#,
    );
    assert!(out.stdout.contains("rolled_back: true"), "{}", out.stdout);
    assert!(out.stdout.contains("restored: 2"), "{}", out.stdout);

    assert_eq!(read(&sb.file("a.txt")), "A");
    assert_eq!(read(&sb.file("b.txt")), "B");
    assert!(
        !sb.file("new.txt").exists(),
        "a created file must be removed"
    );
}

#[test]
fn nested_rollback_reverts_only_the_child_frame() {
    let sb = Sandbox::new("tx_nested");
    std::fs::write(sb.file("a.txt"), "A").unwrap();
    std::fs::write(sb.file("b.txt"), "B").unwrap();

    ok(
        &sb,
        r#"tx_begin(); file_write("a.txt", "A2"); tx_begin(); file_write("b.txt", "B2"); tx_rollback(); tx_commit()"#,
    );

    assert_eq!(read(&sb.file("a.txt")), "A2", "parent write survives");
    assert_eq!(read(&sb.file("b.txt")), "B", "child write is reverted");
}

// ── session_*: git-backed, in a repository in the sandbox ───────────────

#[test]
fn checkpoint_then_change_then_restore_and_rollback() {
    let sb = Sandbox::new("session_restore");
    repo_with_commit(&sb);
    std::fs::write(sb.file("f.txt"), "two").unwrap();

    let out = ok(&sb, r#"session_checkpoint("c1")"#);
    assert!(out.stdout.contains("true"), "{}", out.stdout);
    // The checkpoint keeps the work and leaves the tree alone.
    assert_eq!(read(&sb.file("f.txt")), "two");

    std::fs::write(sb.file("f.txt"), "three").unwrap();

    // A rollback resets the tree to HEAD, discarding the change after the
    // checkpoint.
    ok(&sb, "session_rollback()");
    assert_eq!(read(&sb.file("f.txt")), "one");

    // The checkpoint is still there, and re-applying it brings back the
    // checkpointed content on the clean tree.
    ok(&sb, "session_restore(0)");
    assert_eq!(read(&sb.file("f.txt")), "two");
}

#[test]
fn changes_diff_and_checkpoints_report_what_happened() {
    let sb = Sandbox::new("session_report");
    repo_with_commit(&sb);

    // Clean tree: nothing to checkpoint, and that is an answer, not an error.
    let clean = ok(&sb, "session_checkpoint()");
    assert!(clean.stdout.contains("false"), "{}", clean.stdout);

    std::fs::write(sb.file("f.txt"), "two").unwrap();

    let changes = ok(&sb, "session_changes()");
    assert!(changes.stdout.contains("f.txt"), "{}", changes.stdout);

    let diff = ok(&sb, r#"session_diff_since("HEAD")"#);
    assert!(diff.stdout.contains("+two"), "{}", diff.stdout);
    assert!(diff.stdout.contains("-one"), "{}", diff.stdout);

    ok(&sb, r#"session_checkpoint("before-edit")"#);
    let listed = ok(&sb, "session_checkpoints()");
    assert!(listed.stdout.contains("before-edit"), "{}", listed.stdout);
}

#[test]
fn rollback_refused_option_leaves_the_tree_alone() {
    let sb = Sandbox::new("session_rollback_refused");
    repo_with_commit(&sb);
    std::fs::write(sb.file("f.txt"), "two").unwrap();

    refused(&sb, r#"session_rollback("--hard")"#, "E_BAD_ARG");
    refused(&sb, "session_rollback(1)", "E_BAD_ARG");
    assert_eq!(
        read(&sb.file("f.txt")),
        "two",
        "a refused rollback must not reset"
    );
}

#[test]
fn export_writes_the_patch_where_it_is_asked() {
    let sb = Sandbox::new("session_export");
    repo_with_commit(&sb);
    std::fs::write(sb.file("f.txt"), "two").unwrap();

    let out = ok(&sb, r#"session_export("out.patch")"#);
    assert!(out.stdout.contains("out.patch"), "{}", out.stdout);

    let patch = read(&sb.file("out.patch"));
    assert!(patch.contains("+two"), "{patch}");
    assert!(patch.contains("-one"), "{patch}");
}

#[test]
fn restore_of_a_missing_checkpoint_is_not_found_not_a_tool_failure() {
    let sb = Sandbox::new("session_restore_missing");
    repo_with_commit(&sb);

    refused(&sb, "session_restore(5)", "E_NOT_FOUND");
}

// ── wrong state: coded, never false or Null ──────────────────────────────

#[test]
fn commit_rollback_savepoint_without_a_transaction_are_bad_state() {
    let sb = Sandbox::new("tx_no_state");
    refused(&sb, "tx_commit()", "E_BAD_STATE");
    refused(&sb, "tx_rollback()", "E_BAD_STATE");
    refused(&sb, r#"tx_savepoint("s")"#, "E_BAD_STATE");
    refused(&sb, r#"tx_rollback_to("s")"#, "E_BAD_STATE");
}

#[test]
fn rollback_to_an_unknown_savepoint_is_bad_state() {
    let sb = Sandbox::new("tx_unknown_savepoint");
    refused(&sb, r#"tx_begin(); tx_rollback_to("nope")"#, "E_BAD_STATE");
}

#[test]
fn session_reads_outside_a_repository_are_bad_state() {
    let sb = Sandbox::new("session_no_repo");
    refused(&sb, "session_changes()", "E_BAD_STATE");
    refused(&sb, "session_checkpoint()", "E_BAD_STATE");
    refused(&sb, "session_rollback()", "E_BAD_STATE");
    std::fs::write(sb.file("keep.txt"), "keep").unwrap();
    refused(&sb, r#"session_rollback("HEAD")"#, "E_BAD_STATE");
    assert_eq!(read(&sb.file("keep.txt")), "keep");
}

// ── argument and pipeline refusals: nothing runs ─────────────────────────

#[test]
fn tx_argument_and_pipeline_refusals() {
    let sb = Sandbox::new("tx_refusals");
    refused(&sb, "tx_begin(1)", "E_BAD_ARG");
    refused(&sb, r#""x" | tx_begin()"#, "E_BAD_ARG");
    refused(&sb, "tx_commit(1)", "E_BAD_ARG");
    refused(&sb, r#""x" | tx_commit()"#, "E_BAD_ARG");
    refused(&sb, "tx_rollback(1)", "E_BAD_ARG");
    refused(&sb, r#""x" | tx_rollback()"#, "E_BAD_ARG");
    refused(&sb, "tx_savepoint()", "E_BAD_ARG");
    refused(&sb, "tx_savepoint(1)", "E_BAD_ARG");
    refused(&sb, r#""x" | tx_savepoint("s")"#, "E_BAD_ARG");
    refused(&sb, "tx_rollback_to()", "E_BAD_ARG");
    refused(&sb, "tx_rollback_to(1)", "E_BAD_ARG");
    refused(&sb, r#""x" | tx_rollback_to("s")"#, "E_BAD_ARG");
}

#[test]
fn session_argument_and_pipeline_refusals() {
    let sb = Sandbox::new("session_refusals");
    refused(&sb, "session_checkpoint(1)", "E_BAD_ARG");
    refused(&sb, r#"session_checkpoint("a", "b")"#, "E_BAD_ARG");
    refused(&sb, r#""x" | session_checkpoint()"#, "E_BAD_ARG");
    refused(&sb, r#"session_restore(-1)"#, "E_BAD_ARG");
    refused(&sb, r#"session_restore("x")"#, "E_BAD_ARG");
    refused(&sb, r#""x" | session_restore()"#, "E_BAD_ARG");
    refused(&sb, "session_checkpoints(1)", "E_BAD_ARG");
    refused(&sb, r#""x" | session_checkpoints()"#, "E_BAD_ARG");
    refused(&sb, r#"session_diff_since(1)"#, "E_BAD_ARG");
    refused(&sb, r#"session_diff_since("--output=x")"#, "E_BAD_ARG");
    refused(&sb, r#""x" | session_diff_since()"#, "E_BAD_ARG");
    refused(&sb, "session_changes(1)", "E_BAD_ARG");
    refused(&sb, r#""x" | session_changes()"#, "E_BAD_ARG");
    refused(&sb, r#"session_rollback(1)"#, "E_BAD_ARG");
    refused(&sb, r#""x" | session_rollback()"#, "E_BAD_ARG");
    refused(&sb, r#"session_export(1)"#, "E_BAD_ARG");
    refused(&sb, r#""x" | session_export()"#, "E_BAD_ARG");
}
