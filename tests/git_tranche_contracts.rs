//! Contracts for the git_* builtins.
//!
//! Each test builds its own repository in a temporary directory and never
//! touches the checkout this test runs from. The builtins act on the process
//! working directory (or a directory argument), so they are driven through
//! the real `ae` binary with its working directory set to that repository;
//! every child has a 30 second deadline and a non-interactive stdin. Effects
//! are checked with the git command line, never with the builtin's own
//! answer.
//!
//! Git's global configuration is replaced by an empty file, so no user hook,
//! signing key or identity leaks in. Identities are written only to each
//! test repository's local config.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use aethershell::builtins::call_with_input;
use aethershell::env::Env;
use aethershell::value::Value;

const DEADLINE: Duration = Duration::from_secs(30);

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// One test's temporary world: a repository, an origin, a session directory
/// and an empty global git configuration. Removed when the test ends.
struct Fixture {
    parent: PathBuf,
    global: PathBuf,
    session: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.parent);
    }
}

struct Run {
    success: bool,
    stdout: String,
    stderr: String,
}

fn drain<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let parent =
            std::env::temp_dir().join(format!("ae_git_tranche_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&parent);
        std::fs::create_dir_all(&parent).expect("create fixture directory");
        let global = parent.join("empty-gitconfig");
        std::fs::write(&global, "").expect("write empty global config");
        let session = parent.join("session");
        std::fs::create_dir_all(&session).expect("create session directory");
        Self {
            parent,
            global,
            session,
        }
    }

    fn git_command(&self, dir: &Path) -> Command {
        let mut cmd = Command::new("git");
        cmd.current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", &self.global)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(Stdio::null());
        cmd
    }

    /// git's stdout on success, its stderr on failure.
    fn git_try(&self, dir: &Path, args: &[&str]) -> Result<String, String> {
        let out = self
            .git_command(dir)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
    }

    fn git(&self, dir: &Path, args: &[&str]) -> String {
        self.git_try(dir, args)
            .unwrap_or_else(|e| panic!("git {args:?} in {}: {e}", dir.display()))
    }

    /// A repository on `main` with one commit, `init`, holding README.md.
    fn init_repo(&self) -> PathBuf {
        let repo = self.parent.join("repo");
        std::fs::create_dir_all(&repo).expect("create repository directory");
        self.git(&repo, &["init", "-q"]);
        self.git(&repo, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        self.git(&repo, &["config", "--local", "user.name", "AE Test"]);
        self.git(
            &repo,
            &["config", "--local", "user.email", "ae-test@example.invalid"],
        );
        self.git(&repo, &["config", "--local", "commit.gpgsign", "false"]);
        std::fs::write(repo.join("README.md"), "v1\n").expect("write README");
        self.git(&repo, &["add", "README.md"]);
        self.git(&repo, &["commit", "-q", "-m", "init"]);
        repo
    }

    /// Run `ae -c <expr>` in `cwd` with a hard deadline.
    fn ae(&self, cwd: &Path, expr: &str) -> Run {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ae"))
            .current_dir(cwd)
            .env("AETHER_SESSION_DIR", &self.session)
            .env("GIT_CONFIG_GLOBAL", &self.global)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(["-c", expr])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn ae");
        let out = drain(child.stdout.take());
        let err = drain(child.stderr.take());
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().expect("poll ae") {
                break status;
            }
            if started.elapsed() > DEADLINE {
                let _ = child.kill();
                let _ = child.wait();
                panic!("ae did not finish within 30 s: {expr}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        Run {
            success: status.success(),
            stdout: out.join().unwrap_or_default(),
            stderr: err.join().unwrap_or_default(),
        }
    }

    /// The printed value of `expr`, which must succeed.
    fn value(&self, cwd: &Path, expr: &str) -> String {
        let run = self.ae(cwd, expr);
        assert!(
            run.success,
            "{expr} exited non-zero\nstdout: {}\nstderr: {}",
            run.stdout, run.stderr
        );
        run.stdout.trim_end().to_string()
    }

    /// The value of `expr` as JSON, which must succeed.
    fn json(&self, cwd: &Path, expr: &str) -> serde_json::Value {
        let printed = self.value(cwd, &format!("json.stringify({expr})"));
        serde_json::from_str(&printed)
            .unwrap_or_else(|e| panic!("{expr} did not print JSON ({e}): {printed}"))
    }

    /// The error record `expr` raised. A call that succeeds is a failure of
    /// the test, so it panics with what was answered.
    fn refusal(&self, cwd: &Path, expr: &str) -> serde_json::Value {
        let wrapped = format!("try {{ {expr} }} catch e {{ json.stringify(e.error) }}");
        let run = self.ae(cwd, &wrapped);
        let printed = run.stdout.trim_end();
        let parsed: serde_json::Value = serde_json::from_str(printed).unwrap_or_else(|_| {
            panic!(
                "{expr} was not refused; it answered: {printed} {}",
                run.stderr
            )
        });
        assert!(
            parsed.get("code").is_some(),
            "{expr} raised no coded error: {parsed}"
        );
        parsed
    }

    fn code(&self, cwd: &Path, expr: &str) -> String {
        self.refusal(cwd, expr)["code"]
            .as_str()
            .expect("code is a string")
            .to_string()
    }
}

fn lines(s: &str) -> Vec<String> {
    s.lines().map(str::to_string).collect()
}

/// Whether `dir` lies inside some repository. A temporary directory normally
/// does not, but a test about "outside a repository" must not assume it.
fn inside_a_repository(fx: &Fixture, dir: &Path) -> bool {
    fx.git_try(dir, &["rev-parse", "--is-inside-work-tree"])
        .map(|out| out.trim() == "true")
        .unwrap_or(false)
}

// ── refusals: argument shape, types, options, pipeline input ───────────────

/// Every refusal is coded E_BAD_ARG and reaches no git: the repository is
/// unchanged afterwards, and an option-like value never runs as an option.
#[test]
fn refusals_are_coded_and_leave_the_repository_as_it_was() {
    if !git_available() {
        eprintln!("skipped: git is not installed here");
        return;
    }
    let fx = Fixture::new("refusals");
    let repo = fx.init_repo();
    std::fs::write(repo.join("ok.txt"), "untracked\n").unwrap();
    let status_before = fx.git(&repo, &["status", "--porcelain"]);
    let head_before = fx.git(&repo, &["rev-parse", "HEAD"]);

    let refused = [
        // arity
        "git_status(\"a\", \"b\")",
        "git_log(2, 3)",
        "git_blame()",
        "git_checkout()",
        "git_commit()",
        "git_add()",
        "git_merge()",
        "git_rebase()",
        "git_cherry_pick()",
        "git_ignore()",
        "git_stash(\"x\")",
        "git_stash_pop(\"x\")",
        "git_fetch(\"--upload-pack=x\")",
        "git_pull(\"--rebase\")",
        "git_push(\"--force\")",
        "git_clean(true, true)",
        // wrong types
        "git_status(1)",
        "git_log(\"3\")",
        "git_log(0)",
        "git_log(-2)",
        "git_show(1)",
        "git_rev_parse(1)",
        "git_clean(\"yes\")",
        "git_clean(1)",
        "git_add(\"ok.txt\", 1)",
        "git_commit(1)",
        "git_ignore(\"\")",
        "git_ignore(\"a\\nb\")",
        // option-like values, which must never reach git as options
        "git_show(\"--output=x\")",
        "git_rev_parse(\"--git-dir\")",
        "git_blame(\"--follow\")",
        "git_checkout(\"--force\")",
        "git_checkout(\"\")",
        "git_add(\"--all\")",
        "git_merge(\"--abort\")",
        "git_rebase(\"--root\")",
        "git_cherry_pick(\"--continue\")",
        "git_reset(\"--bogus\")",
        "git_reset(\"\")",
        // pipeline input
        "\"x\" | git_status()",
        "\"x\" | git_log()",
        "\"x\" | git_diff()",
        "\"x\" | git_stash()",
        "\"x\" | git_stash_pop()",
        "\"x\" | git_fetch()",
        "\"x\" | git_push()",
        "\"x\" | git_add(\"ok.txt\")",
        "\"x\" | git_clean()",
    ];
    for expr in refused {
        let error = fx.refusal(&repo, expr);
        assert_eq!(error["code"], "E_BAD_ARG", "{expr}: {error}");
    }

    assert_eq!(fx.git(&repo, &["status", "--porcelain"]), status_before);
    assert_eq!(fx.git(&repo, &["rev-parse", "HEAD"]), head_before);
    assert!(
        !repo.join("x").exists(),
        "--output=x reached git as an option and wrote a file"
    );
    assert!(
        !repo.join(".gitignore").exists(),
        "a refused git_ignore still wrote .gitignore"
    );
}

/// Pipeline input to the read-only builtins is refused in-process. Nothing is
/// run, so this is safe to call from the test process.
#[test]
fn read_only_builtins_refuse_pipeline_input_in_process() {
    let mut env = Env::new();
    for name in ["git_status", "git_diff", "git_diff_staged", "git_log"] {
        let refused = call_with_input(name, vec![], Some(Value::Str("x".into())), &mut env);
        assert!(refused.is_err(), "{name} accepted pipeline input");
    }
}

// ── outside a repository ────────────────────────────────────────────────────

/// Every git builtin, outside a repository, is E_BAD_STATE: not Null, "" or
/// [], which were the answers before, and not a success.
#[test]
fn outside_a_repository_every_git_builtin_is_bad_state() {
    if !git_available() {
        eprintln!("skipped: git is not installed here");
        return;
    }
    let fx = Fixture::new("outside");
    let plain = fx.parent.join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    if inside_a_repository(&fx, &plain) {
        eprintln!("skipped: the temporary directory lies inside a repository");
        return;
    }
    let calls = [
        "git_status()",
        "git_diff()",
        "git_diff_staged()",
        "git_log()",
        "git_blame(\"README.md\")",
        "git_show()",
        "git_rev_parse()",
        "git_clean()",
        "git_add(\"x\")",
        "git_checkout(\"main\")",
        "git_commit(\"m\")",
        "git_reset()",
        "git_stash()",
        "git_stash_pop()",
        "git_fetch()",
        "git_pull()",
        "git_push()",
        "git_merge(\"main\")",
        "git_rebase(\"main\")",
        "git_cherry_pick(\"HEAD\")",
        "git_ignore(\"*.log\")",
    ];
    for expr in calls {
        assert_eq!(fx.code(&plain, expr), "E_BAD_STATE", "{expr}");
    }
    assert!(
        !plain.join(".gitignore").exists(),
        "git_ignore wrote a .gitignore outside a repository"
    );
}

/// A directory that does not exist is E_NOT_FOUND, not a missing git: the
/// directory is the child's working directory, and spawning into it fails
/// with the same error a missing program does. An option-like directory is
/// simply a directory that is not there.
#[test]
fn a_missing_directory_is_not_found_not_a_missing_git() {
    if !git_available() {
        eprintln!("skipped: git is not installed here");
        return;
    }
    let fx = Fixture::new("missing_dir");
    let repo = fx.init_repo();
    assert_eq!(fx.code(&repo, "git_status(\"no-such-dir\")"), "E_NOT_FOUND");
    assert_eq!(fx.code(&repo, "git_diff(\"no-such-dir\")"), "E_NOT_FOUND");
    assert_eq!(
        fx.code(&repo, "git_diff_staged(\"--upload-pack=x\")"),
        "E_NOT_FOUND"
    );
}

// ── read-only builtins answer what git answers ──────────────────────────────

#[test]
fn read_only_builtins_answer_what_git_answers() {
    if !git_available() {
        eprintln!("skipped: git is not installed here");
        return;
    }
    let fx = Fixture::new("read_only");
    let repo = fx.init_repo();
    std::fs::write(repo.join("README.md"), "v1\nchanged\n").unwrap();
    std::fs::write(repo.join("u.txt"), "untracked\n").unwrap();
    std::fs::write(repo.join("second.txt"), "two\n").unwrap();
    fx.git(&repo, &["add", "second.txt"]);
    fx.git(&repo, &["commit", "-q", "-m", "second"]);
    std::fs::write(repo.join("README.md"), "v1\nchanged again\n").unwrap();

    let status = fx.git(&repo, &["status", "--porcelain", "-b"]);
    let got: Vec<String> = serde_json::from_value(fx.json(&repo, "git_status()")).unwrap();
    assert_eq!(got, lines(&status));
    assert!(got[0].starts_with("## main"), "branch line: {}", got[0]);

    let log = fx.git(&repo, &["log", "--oneline", "-n", "10"]);
    let got: Vec<String> = serde_json::from_value(fx.json(&repo, "git_log()")).unwrap();
    assert_eq!(got, lines(&log));
    let got: Vec<String> = serde_json::from_value(fx.json(&repo, "git_log(1)")).unwrap();
    assert_eq!(got.len(), 1);
    assert!(got[0].contains("second"), "newest first: {got:?}");

    let head = fx.git(&repo, &["rev-parse", "HEAD"]);
    let previous = fx.git(&repo, &["rev-parse", "HEAD~1"]);
    let got: String = serde_json::from_value(fx.json(&repo, "git_rev_parse()")).unwrap();
    assert_eq!(got, head.trim());
    let got: String = serde_json::from_value(fx.json(&repo, "git_rev_parse(\"HEAD~1\")")).unwrap();
    assert_eq!(got, previous.trim());

    let show = fx.git(&repo, &["show", "HEAD"]);
    let got: String = serde_json::from_value(fx.json(&repo, "git_show()")).unwrap();
    assert_eq!(got, show);

    let diff = fx.git(&repo, &["diff"]);
    let got: String = serde_json::from_value(fx.json(&repo, "git_diff()")).unwrap();
    assert_eq!(got, diff);
    assert!(
        got.contains("+v1") || got.contains("changed again"),
        "{got}"
    );
    let staged = fx.git(&repo, &["diff", "--staged"]);
    let got: String = serde_json::from_value(fx.json(&repo, "git_diff_staged()")).unwrap();
    assert_eq!(got, staged);
    assert_eq!(got, "", "nothing is staged");

    let blame = fx.git(&repo, &["blame", "--", "README.md"]);
    let got: String = serde_json::from_value(fx.json(&repo, "git_blame(\"README.md\")")).unwrap();
    assert_eq!(got, blame);
}

/// A directory argument selects the repository the status is taken from.
#[test]
fn a_directory_argument_selects_its_repository() {
    if !git_available() {
        eprintln!("skipped: git is not installed here");
        return;
    }
    let fx = Fixture::new("dir_arg");
    let repo = fx.init_repo();
    let sub = repo.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("inner.txt"), "x\n").unwrap();
    let from_sub = fx.git(&sub, &["status", "--porcelain", "-b"]);
    let got: Vec<String> = serde_json::from_value(fx.json(&repo, "git_status(\"sub\")")).unwrap();
    assert_eq!(got, lines(&from_sub));
}

// ── mutating builtins do what git does, verified with the git CLI ──────────

#[test]
fn mutating_builtins_change_the_repository_as_git_does() {
    if !git_available() {
        eprintln!("skipped: git is not installed here");
        return;
    }
    let fx = Fixture::new("mutating");
    let repo = fx.init_repo();

    // add and commit
    std::fs::write(repo.join("new.txt"), "n\n").unwrap();
    assert_eq!(fx.value(&repo, "git_add(\"new.txt\")"), "true");
    assert_eq!(
        fx.git(&repo, &["diff", "--cached", "--name-only"]).trim(),
        "new.txt"
    );
    assert_eq!(fx.value(&repo, "git_commit(\"add new\")"), "true");
    assert_eq!(
        fx.git(&repo, &["log", "-1", "--format=%s"]).trim(),
        "add new"
    );

    // checkout a branch, then commit on it
    fx.git(&repo, &["branch", "feature"]);
    assert_eq!(fx.value(&repo, "git_checkout(\"feature\")"), "true");
    assert_eq!(
        fx.git(&repo, &["branch", "--show-current"]).trim(),
        "feature"
    );
    std::fs::write(repo.join("f.txt"), "f\n").unwrap();
    fx.git(&repo, &["add", "f.txt"]);
    fx.git(&repo, &["commit", "-q", "-m", "feature work"]);
    assert_eq!(fx.value(&repo, "git_checkout(\"main\")"), "true");
    assert_eq!(fx.git(&repo, &["branch", "--show-current"]).trim(), "main");

    // merge the feature branch (fast-forward)
    assert_eq!(fx.value(&repo, "git_merge(\"feature\")"), "true");
    assert_eq!(
        fx.git(&repo, &["log", "-1", "--format=%s"]).trim(),
        "feature work"
    );

    // cherry-pick a commit from a side branch
    fx.git(&repo, &["checkout", "-q", "-b", "side"]);
    std::fs::write(repo.join("s.txt"), "s\n").unwrap();
    fx.git(&repo, &["add", "s.txt"]);
    fx.git(&repo, &["commit", "-q", "-m", "pick me"]);
    fx.git(&repo, &["checkout", "-q", "main"]);
    assert_eq!(fx.value(&repo, "git_cherry_pick(\"side\")"), "true");
    assert_eq!(
        fx.git(&repo, &["log", "-1", "--format=%s"]).trim(),
        "pick me"
    );

    // rebase a branch with its own commit onto a main that moved on
    fx.git(&repo, &["checkout", "-q", "-b", "rb"]);
    std::fs::write(repo.join("rb.txt"), "rb\n").unwrap();
    fx.git(&repo, &["add", "rb.txt"]);
    fx.git(&repo, &["commit", "-q", "-m", "rb work"]);
    fx.git(&repo, &["checkout", "-q", "main"]);
    std::fs::write(repo.join("main.txt"), "m\n").unwrap();
    fx.git(&repo, &["add", "main.txt"]);
    fx.git(&repo, &["commit", "-q", "-m", "main moved"]);
    assert_eq!(fx.value(&repo, "git_checkout(\"rb\")"), "true");
    assert_eq!(fx.value(&repo, "git_rebase(\"main\")"), "true");
    assert!(
        fx.git_try(&repo, &["merge-base", "--is-ancestor", "main", "rb"])
            .is_ok(),
        "rebase did not put rb on top of main"
    );
    assert_eq!(
        fx.git(&repo, &["log", "-1", "--format=%s"]).trim(),
        "rb work"
    );
    assert_eq!(fx.value(&repo, "git_checkout(\"main\")"), "true");

    // reset (mixed) unstages and keeps the file
    std::fs::write(repo.join("staged.txt"), "s\n").unwrap();
    fx.git(&repo, &["add", "staged.txt"]);
    assert_eq!(fx.value(&repo, "git_reset()"), "true");
    assert_eq!(
        fx.git(&repo, &["diff", "--cached", "--name-only"]).trim(),
        ""
    );
    assert!(
        repo.join("staged.txt").exists(),
        "mixed reset removed the file"
    );

    // reset to a revision moves the branch and keeps the work tree
    let before = fx.git(&repo, &["rev-parse", "HEAD"]);
    let parent = fx.git(&repo, &["rev-parse", "HEAD~1"]);
    assert_eq!(fx.value(&repo, "git_reset(\"HEAD~1\")"), "true");
    assert_eq!(fx.git(&repo, &["rev-parse", "HEAD"]), parent);
    assert_ne!(before, parent);
    fx.git(&repo, &["reset", "-q", "--hard", before.trim()]);

    // reset --hard discards uncommitted edits
    std::fs::write(repo.join("README.md"), "discard me\n").unwrap();
    assert_eq!(fx.value(&repo, "git_reset(\"--hard\")"), "true");
    assert_eq!(
        std::fs::read_to_string(repo.join("README.md")).unwrap(),
        "v1\n"
    );

    // stash and pop
    std::fs::write(repo.join("README.md"), "stash me\n").unwrap();
    assert_eq!(fx.value(&repo, "git_stash()"), "true");
    assert!(fx.git(&repo, &["stash", "list"]).contains("WIP"));
    assert_eq!(
        std::fs::read_to_string(repo.join("README.md")).unwrap(),
        "v1\n"
    );
    assert_eq!(fx.value(&repo, "git_stash_pop()"), "true");
    assert_eq!(
        std::fs::read_to_string(repo.join("README.md")).unwrap(),
        "stash me\n"
    );
    assert_eq!(fx.git(&repo, &["stash", "list"]).trim(), "");
    fx.git(&repo, &["checkout", "-q", "--", "README.md"]);

    // ignore: appends once, then reports the pattern already present
    assert_eq!(fx.value(&repo, "git_ignore(\"*.log\")"), "true");
    assert_eq!(fx.value(&repo, "git_ignore(\"*.log\")"), "false");
    let ignore = std::fs::read_to_string(repo.join(".gitignore")).unwrap();
    assert_eq!(ignore.matches("*.log").count(), 1, "{ignore:?}");
    assert!(fx.git_try(&repo, &["check-ignore", "-q", "x.log"]).is_ok());

    // clean: a dry run lists and keeps, false removes
    std::fs::write(repo.join("junk.txt"), "junk\n").unwrap();
    let dry: Vec<String> = serde_json::from_value(fx.json(&repo, "git_clean()")).unwrap();
    assert!(dry.iter().any(|l| l.contains("junk.txt")), "{dry:?}");
    assert!(repo.join("junk.txt").exists(), "a dry run removed the file");
    let removed: Vec<String> = serde_json::from_value(fx.json(&repo, "git_clean(false)")).unwrap();
    assert!(
        removed.iter().any(|l| l.contains("junk.txt")),
        "{removed:?}"
    );
    assert!(
        !repo.join("junk.txt").exists(),
        "git_clean(false) kept the file"
    );
}

/// Checking out a path is refused as an invalid reference, so a file's
/// uncommitted edits survive. Without the `--`, git restored the file.
#[test]
fn checkout_of_a_path_never_discards_an_edit() {
    if !git_available() {
        eprintln!("skipped: git is not installed here");
        return;
    }
    let fx = Fixture::new("checkout_path");
    let repo = fx.init_repo();
    std::fs::write(repo.join("README.md"), "edited, not committed\n").unwrap();
    let error = fx.refusal(&repo, "git_checkout(\"README.md\")");
    assert_eq!(error["code"], "E_TOOL_FAILED", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap_or("")
            .contains("invalid reference"),
        "{error}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("README.md")).unwrap(),
        "edited, not committed\n"
    );
}

/// The ignore file is never overwritten when it cannot be read. A directory
/// named .gitignore is unreadable as a file, and the call must say so.
#[test]
fn an_unreadable_gitignore_is_an_error_and_is_not_overwritten() {
    if !git_available() {
        eprintln!("skipped: git is not installed here");
        return;
    }
    let fx = Fixture::new("ignore_unreadable");
    let repo = fx.init_repo();
    std::fs::create_dir_all(repo.join(".gitignore")).unwrap();
    let code = fx.code(&repo, "git_ignore(\"*.log\")");
    assert!(code.starts_with("E_"), "{code}");
    assert_ne!(code, "E_BAD_STATE");
    assert!(
        repo.join(".gitignore").is_dir(),
        "the unreadable .gitignore was replaced"
    );
}

// ── remote builtins against a local bare origin ─────────────────────────────

#[test]
fn fetch_pull_and_push_use_the_origin_remote() {
    if !git_available() {
        eprintln!("skipped: git is not installed here");
        return;
    }
    let fx = Fixture::new("remote");
    let origin = fx.parent.join("origin.git");
    std::fs::create_dir_all(&origin).unwrap();
    fx.git(&origin, &["init", "-q", "--bare"]);
    fx.git(&origin, &["symbolic-ref", "HEAD", "refs/heads/main"]);

    let repo = fx.init_repo();
    let origin_str = origin.to_string_lossy().into_owned();
    fx.git(&repo, &["remote", "add", "origin", &origin_str]);
    fx.git(&repo, &["push", "-q", "-u", "origin", "main"]);

    // A second clone pushes a change the repository has not seen.
    let other = fx.parent.join("other");
    fx.git(&fx.parent, &["clone", "-q", &origin_str, "other"]);
    fx.git(&other, &["config", "--local", "user.name", "AE Other"]);
    fx.git(
        &other,
        &["config", "--local", "user.email", "other@example.invalid"],
    );
    fx.git(&other, &["config", "--local", "commit.gpgsign", "false"]);
    std::fs::write(other.join("other.txt"), "o\n").unwrap();
    fx.git(&other, &["add", "other.txt"]);
    fx.git(&other, &["commit", "-q", "-m", "remote change"]);
    fx.git(&other, &["push", "-q", "origin", "main"]);
    let remote_head = fx.git(&other, &["rev-parse", "HEAD"]);

    assert_eq!(fx.value(&repo, "git_fetch()"), "true");
    assert_eq!(
        fx.git(&repo, &["rev-parse", "origin/main"]),
        remote_head,
        "fetch did not update origin/main"
    );

    assert_eq!(fx.value(&repo, "git_pull()"), "true");
    assert_eq!(
        fx.git(&repo, &["rev-parse", "HEAD"]),
        remote_head,
        "pull did not fast-forward the branch"
    );

    std::fs::write(repo.join("mine.txt"), "m\n").unwrap();
    fx.git(&repo, &["add", "mine.txt"]);
    fx.git(&repo, &["commit", "-q", "-m", "local work"]);
    let local_head = fx.git(&repo, &["rev-parse", "HEAD"]);
    assert_eq!(fx.value(&repo, "git_push()"), "true");
    assert_eq!(
        fx.git(&origin, &["rev-parse", "main"]),
        local_head,
        "push did not reach origin"
    );
}

/// With no remote configured, fetch fails with git's own words, as E_TOOL_FAILED.
#[test]
fn a_remote_builtin_without_a_remote_is_tool_failed() {
    if !git_available() {
        eprintln!("skipped: git is not installed here");
        return;
    }
    let fx = Fixture::new("no_remote");
    let repo = fx.init_repo();
    let error = fx.refusal(&repo, "git_push()");
    assert_eq!(error["code"], "E_TOOL_FAILED", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap_or("")
            .contains("No configured push destination"),
        "{error}"
    );
}

// ── failing git: E_TOOL_FAILED with the tail of its output ──────────────────

#[test]
fn a_failing_git_is_tool_failed_with_the_tail_of_its_output() {
    if !git_available() {
        eprintln!("skipped: git is not installed here");
        return;
    }
    let fx = Fixture::new("tool_failed");
    let repo = fx.init_repo();

    // stderr
    let error = fx.refusal(&repo, "git_checkout(\"no-such-branch\")");
    assert_eq!(error["code"], "E_TOOL_FAILED", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap_or("")
            .contains("invalid reference: no-such-branch"),
        "{error}"
    );

    // git reports "nothing to commit" on stdout alone; that is the reason
    let error = fx.refusal(&repo, "git_commit(\"nothing staged\")");
    assert_eq!(error["code"], "E_TOOL_FAILED", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap_or("")
            .contains("nothing to commit"),
        "{error}"
    );

    let error = fx.refusal(&repo, "git_stash_pop()");
    assert_eq!(error["code"], "E_TOOL_FAILED", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap_or("")
            .contains("No stash entries"),
        "{error}"
    );

    // A long argument: git echoes it in its error, and the message stays
    // bounded. (git itself cuts its message near 4 KiB on this platform, so
    // the tail rule's own truncation is not reached through a real git here;
    // it is bounded by git_failure_tail's 4 KiB limit.)
    let long = "x".repeat(20_000);
    let error = fx.refusal(&repo, &format!("git_checkout(\"{long}\")"));
    assert_eq!(error["code"], "E_TOOL_FAILED", "{error}");
    let message = error["message"].as_str().unwrap_or("");
    assert!(message.len() < 6_000, "message is {} bytes", message.len());
    assert!(message.contains("invalid reference: xxx"), "{message:.200}");
}
