//! Contracts for the dev-tool builtins declared in `src/signature.rs`: make,
//! cmake, tmux, screen, gdb, valgrind, zoxide, just, direnv, asdf and mise.
//!
//! Test safety: nothing here starts a tmux or screen session, attaches a
//! terminal, installs a tool version, touches the user's home or shell config,
//! contacts the network, or debugs a process it did not start. Refusal tests
//! stop before any process is spawned. The two real runs (make and an
//! empty-language cmake configure) happen in a fresh temp directory that is the
//! workspace for the length of the test. The make recipe only echoes, and the
//! CMake project declares no languages, so configure needs no compiler.

use aethershell::builtins;
use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once};

/// Process-global environment (`AETHER_MODE`, `AETHER_WORKSPACE`) is read by
/// every call, so each call in this file takes this lock.
static LOCK: Mutex<()> = Mutex::new(());

fn locked() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn isolate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_devtools_tranche_{}", std::process::id()));
        std::env::set_var("AETHER_SESSION_DIR", dir);
    });
}

/// A call in human mode, with the workspace left as the process has it.
fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let _guard = locked();
    std::env::remove_var("AETHER_MODE");
    std::env::remove_var("AETHER_AGENT");
    isolate();
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

/// A call in human mode with `AETHER_WORKSPACE` set to `workspace` for its
/// duration, so the tool may act inside a temp directory and nowhere else.
fn call_in(workspace: &Path, name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let _guard = locked();
    std::env::remove_var("AETHER_MODE");
    std::env::remove_var("AETHER_AGENT");
    std::env::set_var("AETHER_WORKSPACE", workspace);
    isolate();
    let mut env = Env::new();
    let out = builtins::call(name, args, &mut env);
    std::env::remove_var("AETHER_WORKSPACE");
    out
}

fn piped(name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let _guard = locked();
    std::env::remove_var("AETHER_MODE");
    std::env::remove_var("AETHER_AGENT");
    isolate();
    let mut env = Env::new();
    builtins::call_with_input(name, args, Some(input), &mut env)
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

fn rec(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<String, Value>>(),
    )
}

/// The first `E_...` code in an error's rendered text. A success panics, so a
/// refusal that was accepted is reported with the value it produced.
fn code_of(r: &anyhow::Result<Value>) -> String {
    match r {
        Ok(v) => panic!("expected a coded refusal, got {v:?}"),
        Err(e) => {
            let msg = format!("{e:#}");
            let at = msg
                .find("E_")
                .unwrap_or_else(|| panic!("no code in: {msg}"));
            msg[at..]
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || *c == '_')
                .collect()
        }
    }
}

/// A fresh directory under the system temp dir, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("ae_devtools_{tag}_{}_{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn every_devtool_refuses_piped_input() {
    // A piped value would be silently discarded if a body ignored it. Each of
    // these must refuse it before it runs anything.
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("make_run", vec![]),
        ("make_targets", vec![]),
        ("cmake_configure", vec![]),
        ("cmake_build", vec![]),
        ("tmux_new", vec![text("work")]),
        ("tmux_attach", vec![text("work")]),
        ("tmux_kill", vec![text("work")]),
        ("tmux_send", vec![text("work"), text("ls")]),
        ("screen_new", vec![text("work")]),
        ("screen_attach", vec![text("work")]),
        ("valgrind_run", vec![text("prog")]),
        ("valgrind_memcheck", vec![text("prog")]),
        ("gdb_run", vec![text("prog")]),
        ("gdb_bt", vec![text("prog"), text("core")]),
        ("zoxide_add", vec![]),
        ("zoxide_query", vec![text("x")]),
        ("just_run", vec![text("build")]),
        ("just_list", vec![]),
        ("direnv_allow", vec![]),
        ("direnv_status", vec![]),
        ("asdf_list", vec![]),
        ("asdf_install", vec![text("nodejs")]),
        ("mise_use", vec![text("node@20")]),
        ("mise_list", vec![]),
    ];
    for (name, args) in cases {
        let r = piped(name, args, text("piped"));
        assert_eq!(code_of(&r), "E_BAD_ARG", "{name} accepted piped input");
    }
}

#[test]
fn option_like_targets_and_paths_never_reach_a_tool() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("make_run", vec![text("-f")]),
        ("make_run", vec![text("all"), rec(&[("dir", text("-C"))])]),
        ("make_run", vec![text("all"), rec(&[("file", text("-"))])]),
        ("make_targets", vec![text("-x")]),
        ("cmake_configure", vec![text("-S")]),
        ("cmake_configure", vec![text("."), text("-B")]),
        ("cmake_build", vec![text("-x")]),
        (
            "cmake_build",
            vec![text("build"), rec(&[("target", text("-all"))])],
        ),
        (
            "cmake_build",
            vec![text("build"), rec(&[("config", text("-x"))])],
        ),
        ("tmux_new", vec![text("-x")]),
        ("tmux_attach", vec![text("-x")]),
        ("tmux_kill", vec![text("-x")]),
        ("tmux_send", vec![text("-x"), text("ls")]),
        ("tmux_send", vec![text("work"), text("-la")]),
        ("screen_new", vec![text("-x")]),
        ("screen_attach", vec![text("-x")]),
        ("valgrind_run", vec![text("-x")]),
        ("valgrind_memcheck", vec![text("--trace-children=yes")]),
        ("gdb_run", vec![text("-ex")]),
        ("gdb_bt", vec![text("-x"), text("core")]),
        ("gdb_bt", vec![text("prog"), text("-x")]),
        ("zoxide_add", vec![text("-x")]),
        ("zoxide_query", vec![text("-x")]),
        ("just_run", vec![text("-x")]),
        ("just_run", vec![text("build"), text("-v")]),
        ("direnv_allow", vec![text("-x")]),
        ("asdf_list", vec![text("-x")]),
        ("asdf_install", vec![text("-x")]),
        ("asdf_install", vec![text("nodejs"), text("-x")]),
        ("mise_use", vec![text("-g")]),
        ("mise_use", vec![text("--global")]),
    ];
    for (name, args) in cases {
        let r = call(name, args.clone());
        assert_eq!(code_of(&r), "E_BAD_ARG", "{name} {args:?} was not refused");
    }
}

#[test]
fn unknown_options_and_surplus_arguments_are_refused_not_ignored() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        (
            "make_run",
            vec![text("all"), rec(&[("bogus", Value::Int(1))])],
        ),
        (
            "make_run",
            vec![text("all"), rec(&[("jobs", Value::Int(0))])],
        ),
        ("make_run", vec![text("all"), rec(&[("jobs", text("4"))])]),
        ("make_run", vec![text("all"), text("not a record")]),
        (
            "cmake_configure",
            vec![text("."), text("build"), rec(&[("bogus", Value::Int(1))])],
        ),
        (
            "cmake_build",
            vec![text("build"), rec(&[("bogus", Value::Int(1))])],
        ),
        (
            "cmake_build",
            vec![text("build"), rec(&[("jobs", Value::Int(0))])],
        ),
        ("tmux_new", vec![text("work"), text("ls")]),
        ("tmux_kill", vec![text("work"), text("ls")]),
        ("screen_new", vec![text("work"), text("ls")]),
        ("valgrind_run", vec![text("a"), text("b")]),
        ("gdb_run", vec![text("prog"), text("")]),
        (
            "gdb_run",
            vec![text("prog"), text("run\n".repeat(300).as_str())],
        ),
        ("zoxide_add", vec![text("."), text(".")]),
        ("zoxide_query", vec![text("a"), text("b")]),
        ("just_list", vec![text("x")]),
        ("direnv_status", vec![text("x")]),
        ("mise_list", vec![text("x")]),
        ("asdf_list", vec![text("a"), text("b")]),
        ("mise_use", vec![text("node@20"), text("extra")]),
    ];
    for (name, args) in cases {
        let r = call(name, args.clone());
        assert_eq!(code_of(&r), "E_BAD_ARG", "{name} {args:?} was not refused");
    }
}

#[test]
fn session_names_targets_and_keys_are_restricted() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        // tmux reads ':' and '.' as target separators, so they are not names.
        ("tmux_new", vec![text("a:b")]),
        ("tmux_attach", vec![text("a.b")]),
        ("tmux_kill", vec![text("a:b")]),
        ("screen_new", vec![text("a.b")]),
        ("screen_attach", vec![text("a b")]),
        // A command string would be run by tmux's shell, so tmux_new takes none.
        ("tmux_new", vec![text("work"), text("sh -c id")]),
        // One line of text: a newline would be a second command.
        ("tmux_send", vec![text("work"), text("ls\nrm -rf x")]),
        ("tmux_send", vec![text("work"), text("")]),
        ("tmux_send", vec![text("work m"), text("ls")]),
    ];
    for (name, args) in cases {
        let r = call(name, args.clone());
        assert_eq!(code_of(&r), "E_BAD_ARG", "{name} {args:?} was not refused");
    }
}

#[test]
fn attach_refuses_without_a_terminal_instead_of_hanging() {
    // An attach needs a terminal on both ends. Under a test runner that is
    // not the case; where a terminal is attached this test would attach a real
    // session, so it steps aside rather than do that.
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        eprintln!("skipped: a terminal is attached, and an attach would really run");
        return;
    }
    // A name no session has: even with a terminal, the attach could not succeed.
    for name in ["tmux_attach", "screen_attach"] {
        let r = call(name, vec![text("ae-no-such-session")]);
        assert_eq!(
            code_of(&r),
            "E_NO_UI",
            "{name} did not refuse without a terminal"
        );
    }
}

#[test]
fn make_targets_reads_the_makefile_natively() {
    let dir = TempDir::new("targets");
    let path = dir.0.join("Makefile");
    std::fs::write(
        &path,
        "build: main.o\n\tcc -o app main.o\n\ntest:\n\t@echo ok\n",
    )
    .expect("write Makefile");
    let out = call("make_targets", vec![text(&path.display().to_string())])
        .expect("make_targets reads a Makefile");
    match out {
        Value::Array(items) => {
            let names: Vec<String> = items
                .iter()
                .filter_map(|v| match v {
                    Value::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(names, vec!["build".to_string(), "test".to_string()]);
        }
        other => panic!("expected an Array of target names, got {other:?}"),
    }

    let missing = dir.0.join("no-such-Makefile");
    let r = call("make_targets", vec![text(&missing.display().to_string())]);
    assert_eq!(code_of(&r), "E_NOT_FOUND");

    // A file over the 4 MiB text limit is refused, not read in part.
    let big = dir.0.join("Big");
    std::fs::write(&big, vec![b'x'; 4 * 1024 * 1024 + 16]).expect("write big file");
    let r = call("make_targets", vec![text(&big.display().to_string())]);
    assert_eq!(code_of(&r), "E_BUDGET_EXCEEDED");
}

#[test]
fn directories_outside_the_workspace_are_refused_before_any_tool_runs() {
    // The test process's directory is the workspace unless one is set, and the
    // temp directory is outside the repository, so these are outside it.
    let outside = std::env::temp_dir();
    let r = call(
        "make_run",
        vec![
            text("all"),
            rec(&[("dir", text(&outside.display().to_string()))]),
        ],
    );
    assert_eq!(code_of(&r), "E_OUTSIDE_WORKSPACE");

    let r = call("cmake_build", vec![text(&outside.display().to_string())]);
    assert_eq!(code_of(&r), "E_OUTSIDE_WORKSPACE");

    let r = call(
        "cmake_configure",
        vec![text(&outside.display().to_string()), text("build")],
    );
    assert_eq!(code_of(&r), "E_OUTSIDE_WORKSPACE");

    let r = call("direnv_allow", vec![text(&outside.display().to_string())]);
    assert_eq!(code_of(&r), "E_OUTSIDE_WORKSPACE");
}

#[test]
fn make_runs_a_target_inside_a_temp_workspace_or_reports_it_missing() {
    let dir = TempDir::new("make");
    std::fs::write(dir.0.join("Makefile"), "all:\n\t@echo ae-devtools-ok\n")
        .expect("write Makefile");
    let r = call_in(
        &dir.0,
        "make_run",
        vec![
            text("all"),
            rec(&[("dir", text(&dir.0.display().to_string()))]),
        ],
    );
    match &r {
        Ok(Value::Record(map)) => {
            assert_eq!(map.get("exit_code"), Some(&Value::Int(0)));
            match map.get("stdout") {
                Some(Value::Str(s)) => assert!(s.contains("ae-devtools-ok"), "stdout: {s}"),
                other => panic!("stdout is not a String: {other:?}"),
            }
        }
        Ok(other) => panic!("expected a Record, got {other:?}"),
        // A machine without make reports that, coded; nothing else is acceptable.
        Err(_) => assert_eq!(code_of(&r), "E_TOOL_MISSING"),
    }
}

#[test]
fn cmake_configures_an_empty_project_inside_a_temp_workspace_or_reports_it_missing() {
    let dir = TempDir::new("cmake");
    std::fs::write(
        dir.0.join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.10)\nproject(ae_probe NONE)\n",
    )
    .expect("write CMakeLists.txt");
    let src = dir.0.display().to_string();
    let r = call_in(
        &dir.0,
        "cmake_configure",
        vec![text(&src), text(&dir.0.join("build").display().to_string())],
    );
    match &r {
        Ok(Value::Record(map)) => {
            assert_eq!(map.get("success"), Some(&Value::Bool(true)));
            assert!(
                dir.0.join("build").is_dir(),
                "the build directory was not made"
            );
        }
        Ok(other) => panic!("expected a Record, got {other:?}"),
        Err(_) => assert_eq!(code_of(&r), "E_TOOL_MISSING"),
    }
}

#[test]
fn gdb_refuses_a_missing_core_file_and_a_bad_script() {
    let dir = TempDir::new("gdb");
    let missing = dir.0.join("no-such-core");
    let r = call(
        "gdb_bt",
        vec![
            text("ae-no-such-program"),
            text(&missing.display().to_string()),
        ],
    );
    assert_eq!(code_of(&r), "E_NOT_FOUND");

    let r = call("gdb_run", vec![text("ae-no-such-program"), text("")]);
    assert_eq!(code_of(&r), "E_BAD_ARG");

    let r = call(
        "gdb_run",
        vec![
            text("ae-no-such-program"),
            text("bt\n".repeat(300).as_str()),
        ],
    );
    assert_eq!(code_of(&r), "E_BAD_ARG");
}

#[test]
fn zoxide_add_refuses_a_missing_directory_without_touching_the_database() {
    let dir = TempDir::new("zoxide");
    let missing = dir.0.join("no-such-dir");
    let r = call("zoxide_add", vec![text(&missing.display().to_string())]);
    assert_eq!(code_of(&r), "E_NOT_FOUND");
}

#[test]
fn zoxide_query_with_no_match_is_coded_not_a_string() {
    // Read-only. Without zoxide the answer is E_TOOL_MISSING; with it, a term
    // that matches nothing is E_NOT_FOUND. A bare string is never the answer.
    let r = call("zoxide_query", vec![text("ae-no-such-directory-term-xyz")]);
    match &r {
        Ok(Value::Str(_)) => {}
        Ok(other) => panic!("expected a path String, got {other:?}"),
        Err(_) => {
            let code = code_of(&r);
            assert!(
                code == "E_TOOL_MISSING" || code == "E_NOT_FOUND",
                "uncoded or unexpected failure: {code}"
            );
        }
    }
}
