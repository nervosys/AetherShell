//! Contracts for the linter, formatter and search builtins declared in
//! `src/signature.rs`: eslint, hadolint, mypy, prettier, pytest, shfmt, tokei,
//! hyperfine, ninja, mage, node, yq, fd, rg, rga, eza, fzf, delta and the
//! in-process search_* walkers.
//!
//! Test safety: nothing here walks the repository tree, reads the user's home,
//! or touches the network. Every call that searches or spawns a tool runs with
//! the working directory set to a fresh temp directory, which is also the
//! workspace for the length of the call. Refusal tests stop before any process
//! is spawned. A tool that is absent on this machine must answer E_TOOL_MISSING;
//! a tool that is present is exercised only on files the test wrote itself.

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, Once};

/// The working directory and the workspace variables are process-global, so
/// every call in this file takes this lock.
static LOCK: Mutex<()> = Mutex::new(());

fn locked() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn isolate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_lint_tranche_{}", std::process::id()));
        std::env::set_var("IRONSHELL_SESSION_DIR", dir);
    });
}

/// A call in human mode, with the process's working directory as it is.
fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let _guard = locked();
    std::env::remove_var("IRONSHELL_MODE");
    std::env::remove_var("IRONSHELL_AGENT");
    isolate();
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

/// A call in human mode with a piped subject.
fn piped(name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let _guard = locked();
    std::env::remove_var("IRONSHELL_MODE");
    std::env::remove_var("IRONSHELL_AGENT");
    isolate();
    let mut env = Env::new();
    builtins::call_with_input(name, args, Some(input), &mut env)
}

/// A call in human mode run from `dir`, which is also the workspace for the
/// call. The search walkers and the tools read the working directory, so this
/// is what keeps them off the repository.
fn call_here(dir: &Path, name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let _guard = locked();
    std::env::remove_var("IRONSHELL_MODE");
    std::env::remove_var("IRONSHELL_AGENT");
    isolate();
    let before = std::env::current_dir().expect("the working directory");
    std::env::set_current_dir(dir).expect("enter the temp directory");
    std::env::set_var("IRONSHELL_WORKSPACE", dir);
    let mut env = Env::new();
    let out = builtins::call(name, args, &mut env);
    std::env::remove_var("IRONSHELL_WORKSPACE");
    std::env::set_current_dir(&before).expect("restore the working directory");
    out
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
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
            std::env::temp_dir().join(format!("ae_lint_{tag}_{}_{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Whether `program` cannot be started on this machine. Any other spawn
/// outcome, even a failing exit, counts as present.
fn absent(program: &str) -> bool {
    match std::process::Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(_) => false,
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

/// A Node package-manager shim as the builtins spawn it.
fn shim(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.cmd")
    } else {
        name.to_string()
    }
}

#[test]
fn every_tranche_builtin_refuses_piped_input_it_does_not_take() {
    // A piped value would be silently discarded by a body that ignored it.
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("eslint_check", vec![]),
        ("hadolint_check", vec![]),
        ("mypy_check", vec![]),
        ("prettier_format", vec![text("a.js")]),
        ("pytest_run", vec![]),
        ("shfmt_format", vec![text("a.sh")]),
        ("tokei_count", vec![]),
        ("hyperfine_bench", vec![text("true")]),
        ("ninja_build", vec![]),
        ("mage_run", vec![text("build")]),
        ("node_run", vec![text("1")]),
        ("yq_query", vec![text(".a"), text("a: 1")]),
        ("fd_find", vec![text("x")]),
        ("rg_search", vec![text("x")]),
        ("rga_search", vec![text("x")]),
        ("eza_list", vec![]),
        ("delta_diff", vec![text("a"), text("b")]),
        ("search_by_size", vec![]),
        ("search_code", vec![text("x")]),
        ("search_grep", vec![text("x")]),
        ("search_regex", vec![text("x")]),
    ];
    for (name, args) in cases {
        let r = piped(name, args, text("ignored"));
        assert_eq!(code_of(&r), "E_BAD_ARG", "{name} accepted piped input");
    }
}

#[test]
fn an_option_shaped_operand_is_refused_before_any_process_starts() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("eslint_check", vec![text("--fix")]),
        ("hadolint_check", vec![text("-v")]),
        ("mypy_check", vec![text("--strict")]),
        ("prettier_format", vec![text("--write")]),
        ("shfmt_format", vec![text("-")]),
        ("tokei_count", vec![text("--sort")]),
        ("hyperfine_bench", vec![text("-N"), text("true")]),
        ("mage_run", vec![text("-l")]),
        ("ninja_build", vec![rec_of(&[("target", text("-t"))])]),
        ("yq_query", vec![text("--version"), text("a: 1")]),
        ("fd_find", vec![text("x"), text("-y")]),
        ("rg_search", vec![text("x"), text("-y")]),
        ("rga_search", vec![text("x"), text("-y")]),
        ("eza_list", vec![text("-la")]),
        ("delta_diff", vec![text("-a"), text("b")]),
    ];
    for (name, args) in cases {
        let r = call(name, args);
        assert_eq!(
            code_of(&r),
            "E_BAD_ARG",
            "{name} accepted an option-shaped operand"
        );
    }
}

fn rec_of(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}

#[test]
fn surplus_and_mistyped_arguments_are_refused_not_dropped() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("eza_list", vec![text("a"), text("b")]),
        ("search_by_size", vec![text("+1M"), text("x")]),
        ("search_by_size", vec![Value::Int(5)]),
        ("search_by_size", vec![text("bogus")]),
        ("search_code", vec![]),
        ("search_code", vec![text("a"), text("b")]),
        (
            "rg_search",
            vec![
                text("x"),
                text("a"),
                rec_of(&[("ignore_case", Value::Int(1))]),
            ],
        ),
        (
            "rg_search",
            vec![
                text("x"),
                text("a"),
                rec_of(&[("ignroe_case", Value::Bool(true))]),
            ],
        ),
        (
            "fd_find",
            vec![text("x"), text("a"), rec_of(&[("type", text("q"))])],
        ),
        ("ninja_build", vec![Value::Int(3)]),
        ("node_run", vec![text("   ")]),
        ("yq_query", vec![text("a")]),
        ("delta_diff", vec![text("a")]),
        ("fzf_select", vec![text("x")]),
    ];
    for (name, args) in cases {
        let r = call(name, args);
        assert_eq!(code_of(&r), "E_BAD_ARG", "{name} accepted a bad argument");
    }
}

#[test]
fn fzf_select_filters_piped_items_and_never_starts_a_picker() {
    let items = Value::Array(vec![text("apple"), text("banana"), text("Cherry")]);
    let out = piped("fzf_select", vec![text("an")], items).expect("a piped filter");
    assert_eq!(out, Value::Array(vec![text("banana")]));

    let all = piped("fzf_select", vec![], Value::Str("one\ntwo\n".into())).expect("all lines");
    assert_eq!(all, Value::Array(vec![text("one"), text("two")]));
}

#[test]
fn search_results_are_capped_with_a_budget_refusal_not_truncated() {
    let dir = TempDir::new("cap");
    let many: String = "needle\n".repeat(1001);
    std::fs::write(dir.path().join("many.txt"), many).unwrap();
    let r = call_here(dir.path(), "search_code", vec![text("needle")]);
    assert_eq!(code_of(&r), "E_BUDGET_EXCEEDED");
    let r = call_here(dir.path(), "search_grep", vec![text("needle")]);
    assert_eq!(code_of(&r), "E_BUDGET_EXCEEDED");
}

#[test]
fn a_content_search_answers_with_path_line_and_text() {
    let dir = TempDir::new("hit");
    std::fs::write(dir.path().join("a.txt"), "alpha\nneedle here\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), "nothing\n").unwrap();
    let out = call_here(dir.path(), "search_regex", vec![text("ne+dle")]).expect("a search");
    let Value::Array(hits) = out else {
        panic!("expected an array, got {out:?}");
    };
    assert_eq!(hits.len(), 1, "{hits:?}");
    let Value::Str(hit) = &hits[0] else {
        panic!("expected a string, got {:?}", hits[0]);
    };
    assert!(hit.contains("a.txt:2:needle here"), "{hit}");
}

#[test]
fn a_binary_file_is_skipped_by_content_search() {
    let dir = TempDir::new("binary");
    let mut bytes = b"needle".to_vec();
    bytes.push(0);
    bytes.extend_from_slice(b"needle\n");
    std::fs::write(dir.path().join("blob.bin"), bytes).unwrap();
    let out = call_here(dir.path(), "search_code", vec![text("needle")]).expect("a search");
    assert_eq!(out, Value::Array(vec![]));
}

#[test]
fn an_uncompilable_pattern_is_a_coded_refusal_before_any_walk() {
    let dir = TempDir::new("regex");
    let r = call_here(dir.path(), "search_code", vec![text("(")]);
    assert_eq!(code_of(&r), "E_BAD_ARG");
}

#[test]
fn search_by_size_validates_its_spec_even_in_an_empty_tree() {
    let dir = TempDir::new("size");
    let r = call_here(dir.path(), "search_by_size", vec![text("bogus")]);
    assert_eq!(code_of(&r), "E_BAD_ARG");
    let ok = call_here(dir.path(), "search_by_size", vec![text("+1k")]).expect("a valid spec");
    assert_eq!(ok, Value::Array(vec![]));
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_tree_is_not_followed_by_a_search() {
    let outside = TempDir::new("outside");
    std::fs::write(outside.path().join("secret.txt"), "needle\n").unwrap();
    let dir = TempDir::new("inside");
    std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
    let out = call_here(dir.path(), "search_code", vec![text("needle")]).expect("a search");
    assert_eq!(out, Value::Array(vec![]), "a symlink was followed");
    let sized = call_here(dir.path(), "search_by_size", vec![text("+1c")]).expect("a size search");
    assert_eq!(sized, Value::Array(vec![]), "a symlink was followed");
}

#[test]
fn a_leading_dash_pattern_is_data_for_ripgrep() {
    if absent("rg") {
        eprintln!("skipped: rg is not installed");
        return;
    }
    let dir = TempDir::new("rgdash");
    std::fs::write(dir.path().join("a.txt"), "-needle\n").unwrap();
    let out = call_here(dir.path(), "rg_search", vec![text("-needle")]).expect("a search");
    let Value::Array(matches) = out else {
        panic!("expected an array, got {out:?}");
    };
    assert_eq!(matches.len(), 1, "{matches:?}");
}

#[test]
fn ripgrep_with_no_matches_answers_an_empty_array() {
    if absent("rg") {
        eprintln!("skipped: rg is not installed");
        return;
    }
    let dir = TempDir::new("rgnone");
    std::fs::write(dir.path().join("a.txt"), "nothing\n").unwrap();
    let out = call_here(dir.path(), "rg_search", vec![text("absent-token")]).expect("a search");
    assert_eq!(out, Value::Array(vec![]));
}

#[test]
fn node_expression_failure_is_a_coded_tool_failure() {
    if absent("node") {
        eprintln!("skipped: node is not installed");
        return;
    }
    let dir = TempDir::new("node");
    let ok = call_here(dir.path(), "node_run", vec![text("console.log('hi')")]).expect("a run");
    let Value::Record(rec) = ok else {
        panic!("expected a record, got {ok:?}");
    };
    assert_eq!(rec.get("exit_code"), Some(&Value::Int(0)));
    let r = call_here(dir.path(), "node_run", vec![text("process.exit(3)")]);
    assert_eq!(code_of(&r), "E_TOOL_FAILED");
}

#[cfg(unix)]
#[test]
fn hyperfine_runs_each_command_without_a_shell() {
    if absent("hyperfine") {
        eprintln!("skipped: hyperfine is not installed");
        return;
    }
    let dir = TempDir::new("hyperfine");
    // Under a shell, `> marker.txt` would create the file. Without one, the
    // redirection is an argument to `true` and nothing is written.
    let r = call_here(
        dir.path(),
        "hyperfine_bench",
        vec![text("true > marker.txt")],
    );
    assert!(r.is_ok() || code_of(&r) == "E_TOOL_FAILED", "{r:?}");
    assert!(
        !dir.path().join("marker.txt").exists(),
        "a shell interpreted the redirection"
    );
}

#[test]
fn an_absent_tool_is_e_tool_missing() {
    let dir = TempDir::new("absent");
    std::fs::write(dir.path().join("Dockerfile"), "FROM scratch\n").unwrap();
    std::fs::write(dir.path().join("a.js"), "1;\n").unwrap();
    std::fs::write(dir.path().join("a.sh"), "echo a\n").unwrap();
    std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), "b\n").unwrap();

    // Each call runs in the temp directory only when its tool is absent, so no
    // present tool is ever started by this test.
    let cases: Vec<(&str, String, Vec<Value>)> = vec![
        ("eslint_check", shim("eslint"), vec![]),
        ("hadolint_check", "hadolint".into(), vec![]),
        ("mypy_check", "mypy".into(), vec![]),
        ("prettier_format", shim("prettier"), vec![text("a.js")]),
        ("pytest_run", "pytest".into(), vec![]),
        ("shfmt_format", "shfmt".into(), vec![text("a.sh")]),
        ("tokei_count", "tokei".into(), vec![]),
        ("hyperfine_bench", "hyperfine".into(), vec![text("true")]),
        ("ninja_build", "ninja".into(), vec![]),
        ("mage_run", "mage".into(), vec![text("build")]),
        ("node_run", "node".into(), vec![text("1")]),
        ("yq_query", "yq".into(), vec![text(".a"), text("a: 1")]),
        ("fd_find", "fd".into(), vec![text("x")]),
        ("rg_search", "rg".into(), vec![text("x")]),
        ("rga_search", "rga".into(), vec![text("x")]),
        ("eza_list", "eza".into(), vec![]),
        (
            "delta_diff",
            "delta".into(),
            vec![text("a.txt"), text("b.txt")],
        ),
    ];
    for (name, program, args) in cases {
        if !absent(&program) {
            eprintln!("skipped {name}: {program} is installed");
            continue;
        }
        let r = call_here(dir.path(), name, args);
        assert_eq!(code_of(&r), "E_TOOL_MISSING", "{name}");
    }
}
