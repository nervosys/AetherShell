//! Contracts for the diag, test, refactor, docs, config, environment, feature
//! and syntax builtins of the declaration tranche.
//!
//! The cargo-backed builtins (`test_*`, `diag_*`, `docs_generate`, and the two
//! `cargo fix` builtins) are never run on a real project here, and never run
//! cargo on any directory that cargo could walk up from to reach this
//! repository. Each test either refuses the call before a process starts, or
//! uses a fixture directory that holds at most a stub Cargo.toml inside a
//! temporary workspace. A regression that let a call through would at worst
//! run cargo on that stub, which fails at once.
//!
//! Every test runs inside a `Sandbox`: the workspace, the config, data and
//! cache homes are redirected into a fresh temporary directory, and the
//! process environment is restored when it ends. The tests share one lock,
//! because the environment and the transaction are process-wide.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use aethershell::env::Env;
use aethershell::value::Value;

static SERIAL: Mutex<()> = Mutex::new(());

/// The variables a sandbox redirects. Each is restored when it ends.
const SANDBOX_VARS: [&str; 4] = [
    "AETHER_WORKSPACE",
    "AETHER_CONFIG_HOME",
    "AETHER_DATA_HOME",
    "AETHER_CACHE_HOME",
];

/// The cargo-backed builtins that take a project directory.
const PROJECT_BUILTINS: &[&str] = &[
    "test_run",
    "test_list",
    "test_bench",
    "diag_check",
    "diag_lint",
    "diag_errors",
    "diag_warnings",
    "diag_all",
    "diag_fix",
    "docs_generate",
    "refactor_remove_unused",
];

/// A temporary workspace with its own config, data and cache homes, and an
/// `outside` directory beside it, which is outside the workspace.
struct Sandbox {
    base: PathBuf,
    root: PathBuf,
    outside: PathBuf,
    saved: Vec<(&'static str, Option<OsString>)>,
    _serial: MutexGuard<'static, ()>,
}

impl Sandbox {
    fn new(tag: &str) -> Sandbox {
        let serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base =
            std::env::temp_dir().join(format!("ae_tranche_{tag}_{}_{stamp}", std::process::id()));
        let root = base.join("workspace");
        let outside = base.join("outside");
        std::fs::create_dir_all(&root).expect("create the sandbox workspace");
        std::fs::create_dir_all(&outside).expect("create the directory outside the workspace");
        let saved = SANDBOX_VARS
            .iter()
            .map(|k| (*k, std::env::var_os(k)))
            .collect();
        std::env::set_var("AETHER_WORKSPACE", &root);
        std::env::set_var("AETHER_CONFIG_HOME", base.join("config"));
        std::env::set_var("AETHER_DATA_HOME", base.join("data"));
        std::env::set_var("AETHER_CACHE_HOME", base.join("cache"));
        Sandbox {
            base,
            root,
            outside,
            saved,
            _serial: serial,
        }
    }

    /// A directory inside the workspace, created empty.
    fn dir(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::create_dir_all(&path).expect("create a sandbox directory");
        path
    }

    /// A directory inside the workspace holding a stub Cargo.toml. It has no
    /// sources, so cargo could not build it even if a call reached cargo.
    fn fixture_project(&self, name: &str) -> PathBuf {
        let path = self.dir(name);
        std::fs::write(path.join("Cargo.toml"), STUB_MANIFEST).expect("write the stub manifest");
        path
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

const STUB_MANIFEST: &str =
    "[package]\nname = \"ae_fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn path_value(path: &Path) -> Value {
    Value::Str(path.to_string_lossy().into_owned())
}

fn rec(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect::<BTreeMap<String, Value>>(),
    )
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    aethershell::builtins::call(name, args, &mut env)
}

fn call_piped(name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let mut env = Env::new();
    aethershell::builtins::call_with_input(name, args, Some(input), &mut env)
}

/// The coded refusal an error carries: the first `E_` token in its text.
fn code_of(e: &anyhow::Error) -> String {
    let text = e.to_string();
    match text.find("E_") {
        Some(i) => text[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {text}"),
    }
}

fn expect_code(result: anyhow::Result<Value>, want: &str) {
    match result {
        Ok(v) => panic!("answered {v:?}; expected {want}"),
        Err(e) => assert_eq!(code_of(&e), want, "wrong refusal: {e}"),
    }
}

// ── the project builtins: where they may run ───────────────────────────────

#[test]
fn project_builtins_refuse_a_missing_or_malformed_directory() {
    let sb = Sandbox::new("arg");
    for name in PROJECT_BUILTINS {
        expect_code(call(name, vec![]), "E_BAD_ARG");
        expect_code(call(name, vec![s("")]), "E_BAD_ARG");
        expect_code(call(name, vec![s("   ")]), "E_BAD_ARG");
        expect_code(call(name, vec![s("a\0b")]), "E_BAD_ARG");
        expect_code(call(name, vec![Value::Int(3)]), "E_BAD_ARG");
        expect_code(call(name, vec![s("a"), s("b")]), "E_BAD_ARG");
        expect_code(call_piped(name, vec![], s("a")), "E_BAD_ARG");
        expect_code(
            call(name, vec![path_value(&sb.root.join("absent"))]),
            "E_NOT_FOUND",
        );
    }
}

#[test]
fn a_directory_without_a_manifest_is_refused_as_not_found() {
    let sb = Sandbox::new("manifest");
    let bare = sb.dir("bare");
    for name in PROJECT_BUILTINS {
        expect_code(call(name, vec![path_value(&bare)]), "E_NOT_FOUND");
    }
}

#[test]
fn a_directory_outside_the_workspace_is_refused_before_anything_runs() {
    let sb = Sandbox::new("outside");
    // A real manifest outside the workspace: the refusal must come from the
    // workspace check, not from the manifest check.
    std::fs::write(sb.outside.join("Cargo.toml"), STUB_MANIFEST).expect("write stub");
    for name in PROJECT_BUILTINS {
        expect_code(
            call(name, vec![path_value(&sb.outside)]),
            "E_OUTSIDE_WORKSPACE",
        );
    }
}

#[test]
fn test_selectors_are_refused_before_any_process_starts() {
    let sb = Sandbox::new("selector");
    let project = path_value(&sb.fixture_project("proj"));
    for bad in ["", "-x", "--help", "a/b", "../x", "a b", "x;y", "x\0"] {
        expect_code(
            call("test_run_file", vec![project.clone(), s(bad)]),
            "E_BAD_ARG",
        );
    }
    for bad in ["", "-x", "a b", "x;y", "x\0"] {
        expect_code(
            call("test_run_function", vec![project.clone(), s(bad)]),
            "E_BAD_ARG",
        );
    }
    expect_code(call("test_run_file", vec![project.clone()]), "E_BAD_ARG");
    expect_code(
        call("test_run_function", vec![project.clone(), Value::Int(1)]),
        "E_BAD_ARG",
    );
    // A selector that is well formed is not refused by the checks above.
    // (It is not run here: the fixture has no test targets.)
}

#[test]
fn project_builtins_refuse_pipeline_input() {
    let sb = Sandbox::new("piped");
    let project = path_value(&sb.fixture_project("proj"));
    for name in PROJECT_BUILTINS {
        expect_code(call_piped(name, vec![project.clone()], s("x")), "E_BAD_ARG");
    }
}

// ── refactor_rename_file ─────────────────────────────────────────────────────

#[test]
fn rename_refuses_every_unsafe_request_and_changes_nothing() {
    let sb = Sandbox::new("rename");
    let a = sb.root.join("a.txt");
    let b = sb.root.join("b.txt");
    std::fs::write(&a, "alpha").unwrap();
    std::fs::write(&b, "beta").unwrap();
    let a_s = path_value(&a);
    let b_s = path_value(&b);

    expect_code(
        call("refactor_rename_file", vec![a_s.clone(), a_s.clone()]),
        "E_BAD_ARG",
    );
    expect_code(
        call("refactor_rename_file", vec![a_s.clone(), b_s.clone()]),
        "E_BAD_ARG",
    );
    expect_code(
        call(
            "refactor_rename_file",
            vec![path_value(&sb.root.join("missing")), s("c.txt")],
        ),
        "E_NOT_FOUND",
    );
    expect_code(
        call(
            "refactor_rename_file",
            vec![a_s.clone(), path_value(&sb.outside.join("a.txt"))],
        ),
        "E_OUTSIDE_WORKSPACE",
    );
    expect_code(
        call("refactor_rename_file", vec![s(""), s("c.txt")]),
        "E_BAD_ARG",
    );
    expect_code(
        call("refactor_rename_file", vec![Value::Int(1), s("c.txt")]),
        "E_BAD_ARG",
    );
    expect_code(call("refactor_rename_file", vec![a_s.clone()]), "E_BAD_ARG");
    expect_code(
        call_piped(
            "refactor_rename_file",
            vec![a_s.clone(), s("c.txt")],
            s("x"),
        ),
        "E_BAD_ARG",
    );

    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        "alpha",
        "source changed"
    );
    assert_eq!(
        std::fs::read_to_string(&b).unwrap(),
        "beta",
        "destination was overwritten"
    );
    assert!(!sb.root.join("c.txt").exists());
}

#[test]
fn rename_moves_a_file_inside_the_workspace_and_rollback_undoes_it() {
    let sb = Sandbox::new("rename_ok");
    let a = sb.root.join("a.txt");
    std::fs::write(&a, "alpha").unwrap();
    let c = sb.root.join("c.txt");

    assert_eq!(
        call("refactor_rename_file", vec![path_value(&a), path_value(&c)]).expect("rename"),
        Value::Bool(true)
    );
    assert!(!a.exists());
    assert_eq!(std::fs::read_to_string(&c).unwrap(), "alpha");

    // Back under a transaction, the same rename is undone by rollback.
    call("tx_begin", vec![]).expect("begin");
    call("refactor_rename_file", vec![path_value(&c), path_value(&a)]).expect("rename back");
    call("tx_rollback", vec![]).expect("rollback");
    assert!(c.exists(), "rollback did not restore the source");
    assert!(
        !a.exists(),
        "rollback did not remove the renamed destination"
    );
}

// ── environment ──────────────────────────────────────────────────────────────

#[test]
fn env_set_and_set_env_refuse_names_and_values_that_would_panic() {
    let _sb = Sandbox::new("env_refuse");
    for name in ["env_set", "set_env"] {
        expect_code(call(name, vec![s("A=B"), s("x")]), "E_BAD_ARG");
        expect_code(call(name, vec![s("A\0B"), s("x")]), "E_BAD_ARG");
        expect_code(call(name, vec![s(""), s("x")]), "E_BAD_ARG");
        expect_code(call(name, vec![s("AE_TRANCHE_T"), s("a\0b")]), "E_BAD_ARG");
        expect_code(call(name, vec![Value::Int(5), s("x")]), "E_BAD_ARG");
        expect_code(
            call(name, vec![s("AE_TRANCHE_T"), Value::Int(5)]),
            "E_BAD_ARG",
        );
        expect_code(call(name, vec![s("AE_TRANCHE_T")]), "E_BAD_ARG");
        expect_code(
            call(name, vec![s("AE_TRANCHE_T"), s("x"), s("y")]),
            "E_BAD_ARG",
        );
        expect_code(
            call_piped(name, vec![s("AE_TRANCHE_T"), s("x")], s("y")),
            "E_BAD_ARG",
        );
    }
    assert!(
        std::env::var_os("AE_TRANCHE_T").is_none(),
        "a refused call set the variable"
    );
}

#[test]
fn env_set_and_set_env_set_a_valid_variable() {
    let _sb = Sandbox::new("env_ok");
    let name = format!("AE_TRANCHE_ENV_{}", std::process::id());
    assert_eq!(
        call("env_set", vec![s(&name), s("first")]).expect("env_set"),
        Value::Bool(true)
    );
    assert_eq!(std::env::var(&name).unwrap(), "first");
    assert_eq!(
        call("set_env", vec![s(&name), s("second")]).expect("set_env"),
        Value::Null
    );
    assert_eq!(std::env::var(&name).unwrap(), "second");
    std::env::remove_var(&name);
}

#[test]
fn env_vars_returns_the_environment_as_a_record() {
    let _sb = Sandbox::new("env_vars");
    let name = format!("AE_TRANCHE_VARS_{}", std::process::id());
    std::env::set_var(&name, "visible");
    let got = call("env_vars", vec![]).expect("env_vars");
    std::env::remove_var(&name);
    match got {
        Value::Record(map) => assert_eq!(map.get(&name), Some(&s("visible"))),
        other => panic!("env_vars answered {other:?}, not a Record"),
    }
    expect_code(call("env_vars", vec![s("x")]), "E_BAD_ARG");
    expect_code(call_piped("env_vars", vec![], s("x")), "E_BAD_ARG");
}

// ── config ───────────────────────────────────────────────────────────────────

#[test]
fn config_set_refuses_bad_paths_and_values_and_saves_nothing() {
    let sb = Sandbox::new("config_refuse");
    let file = sb.base.join("config").join("config.toml");
    let refused: Vec<Vec<Value>> = vec![
        vec![s("ai.max_tokens"), Value::Int(-1)],
        vec![s("ai.max_tokens"), Value::Int(0)],
        vec![s("ai.max_tokens"), Value::Int(5_000_000_000)],
        vec![s("history.max_size"), Value::Int(0)],
        vec![s("history.max_size"), Value::Int(2_000_000)],
        vec![s("colors.enabled"), s("yes")],
        vec![s("colors.theme"), s("a b")],
        vec![s("colors.theme"), s("")],
        vec![s("ai.default_model"), s("")],
        vec![s("ai.default_model"), s("a\nb")],
        vec![s("nope"), Value::Bool(true)],
        vec![s("ai.streaming")],
        vec![s("ai.streaming"), Value::Bool(true), Value::Int(3)],
        vec![Value::Int(1), Value::Bool(true)],
    ];
    for args in refused {
        expect_code(call("config_set", args.clone()), "E_BAD_ARG");
    }
    assert!(!file.exists(), "a refused config_set wrote the config file");
    drop(sb);
}

#[test]
fn config_set_and_config_init_write_only_inside_the_sandbox() {
    let sb = Sandbox::new("config_ok");
    let file = sb.base.join("config").join("config.toml");

    let set = call("config_set", vec![s("ai.max_tokens"), Value::Int(2048)]).expect("config_set");
    assert_eq!(set, s("Config saved: ai.max_tokens = 2048"));
    let text = std::fs::read_to_string(&file).expect("config file written");
    assert!(
        text.contains("2048"),
        "the saved file does not hold the value"
    );

    let init = call("config_init", vec![]).expect("config_init");
    match init {
        Value::Str(text) => assert!(text.starts_with("Config file already exists"), "{text}"),
        other => panic!("config_init answered {other:?}"),
    }
    expect_code(call("config_init", vec![s("x")]), "E_BAD_ARG");
    expect_code(call_piped("config_init", vec![], s("x")), "E_BAD_ARG");
}

// ── feature flags ────────────────────────────────────────────────────────────

#[test]
fn feature_set_refuses_bad_names_and_surplus_arguments() {
    let _sb = Sandbox::new("feature_refuse");
    let long = "f".repeat(129);
    for name in ["", "a b", "x\n", long.as_str()] {
        expect_code(
            call("feature_set", vec![s(name), Value::Bool(true)]),
            "E_BAD_ARG",
        );
    }
    expect_code(call("feature_set", vec![s("ok"), s("yes")]), "E_BAD_ARG");
    expect_code(call("feature_set", vec![s("ok")]), "E_BAD_ARG");
    expect_code(
        call(
            "feature_set",
            vec![s("ok"), Value::Bool(true), Value::Int(3)],
        ),
        "E_BAD_ARG",
    );
    expect_code(
        call_piped(
            "feature_set",
            vec![Value::Bool(true), Value::Int(3)],
            s("ok"),
        ),
        "E_BAD_ARG",
    );
    expect_code(
        call_piped("feature_set", vec![s("yes")], s("ok")),
        "E_BAD_ARG",
    );
}

#[test]
fn feature_set_turns_a_flag_on_and_off_directly_and_piped() {
    let _sb = Sandbox::new("feature_ok");
    let name = format!("ae_tranche_flag_{}", std::process::id());
    assert_eq!(
        call("feature_set", vec![s(&name), Value::Bool(true)]).expect("on"),
        Value::Bool(true)
    );
    assert_eq!(
        call("feature_enabled", vec![s(&name)]).expect("read on"),
        Value::Bool(true)
    );
    assert_eq!(
        call_piped("feature_set", vec![Value::Bool(false)], s(&name)).expect("piped off"),
        Value::Bool(true)
    );
    assert_eq!(
        call("feature_enabled", vec![s(&name)]).expect("read off"),
        Value::Bool(false)
    );
}

// ── syntax knowledge base ────────────────────────────────────────────────────

#[test]
fn syntax_add_refuses_malformed_entries_before_anything_is_saved() {
    let _sb = Sandbox::new("syntax_refuse");
    let entry = |fields: &[(&str, Value)]| rec(fields);
    let base = [
        ("id", s("tranche_x")),
        ("name", s("X")),
        ("specification", s("spec")),
    ];
    let with = |extra: (&str, Value)| {
        let mut fields: Vec<(&str, Value)> = base.to_vec();
        fields.push(extra);
        entry(&fields)
    };
    // Missing name.
    expect_code(
        call("syntax_add", vec![entry(&[("id", s("x"))])]),
        "E_BAD_ARG",
    );
    // Wrong types.
    expect_code(
        call(
            "syntax_add",
            vec![entry(&[
                ("id", Value::Int(3)),
                ("name", s("n")),
                ("specification", s("s")),
            ])],
        ),
        "E_BAD_ARG",
    );
    expect_code(
        call("syntax_add", vec![with(("category", Value::Int(3)))]),
        "E_BAD_ARG",
    );
    expect_code(
        call("syntax_add", vec![with(("category", s("")))]),
        "E_BAD_ARG",
    );
    expect_code(
        call(
            "syntax_add",
            vec![with(("examples", Value::Array(vec![Value::Int(1)])))],
        ),
        "E_BAD_ARG",
    );
    // A misspelt field is refused rather than dropped.
    expect_code(
        call("syntax_add", vec![with(("exmples", Value::Array(vec![])))]),
        "E_BAD_ARG",
    );
    // Malformed id.
    expect_code(
        call(
            "syntax_add",
            vec![entry(&[
                ("id", s("bad id")),
                ("name", s("n")),
                ("specification", s("s")),
            ])],
        ),
        "E_BAD_ARG",
    );
    // Not a record at all, and both piped and direct.
    expect_code(call("syntax_add", vec![Value::Int(5)]), "E_BAD_ARG");
    expect_code(
        call_piped(
            "syntax_add",
            vec![entry(&[("id", s("a"))])],
            entry(&[("id", s("b"))]),
        ),
        "E_BAD_ARG",
    );
}
