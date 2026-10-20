//! Contracts for `import` and `export` of `.ae` file modules
//! (`src/module_loader.rs`). Every fixture lives in a fresh temp directory.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aethershell::env::Env;
use aethershell::safety::{ErrorCode, SafetyError};
use aethershell::value::Value;

/// Tests in this binary share the process environment (`AETHER_MODE`,
/// `AETHER_WORKSPACE`) and the process-wide module cache, so they run one at a time.
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// A fresh directory under the temp dir, unique per call.
fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("ae_mic_{tag}_{}_{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).expect("write fixture");
    p
}

/// Run `code` as a script whose directory is `dir`, the way `ae FILE` does.
fn run_in(dir: &Path, code: &str, env: &mut Env) -> anyhow::Result<Value> {
    let script = dir.join("main.ae");
    let _entry = aethershell::module_loader::enter_entry_file(&script);
    let stmts = aethershell::parser::parse_program(code)?;
    aethershell::eval::eval_program(&stmts, env)
}

fn fresh_env() -> Env {
    aethershell::modules::env_with_modules()
}

fn code_of(e: &anyhow::Error) -> ErrorCode {
    e.downcast_ref::<SafetyError>()
        .map(|s| s.code)
        .unwrap_or_else(|| panic!("expected a coded error, got: {e:#}"))
}

fn message_of(e: &anyhow::Error) -> String {
    match e.downcast_ref::<SafetyError>() {
        Some(s) => s.message.clone(),
        None => format!("{e:#}"),
    }
}

/// Restores the agent-mode variables when a test ends, pass or fail.
struct AgentEnv;

impl AgentEnv {
    fn set(workspace: &Path) -> Self {
        std::env::set_var("AETHER_MODE", "agent");
        std::env::set_var("AETHER_WORKSPACE", workspace);
        AgentEnv
    }
}

impl Drop for AgentEnv {
    fn drop(&mut self) {
        std::env::remove_var("AETHER_WORKSPACE");
        std::env::remove_var("AETHER_MODE");
    }
}

const MATH: &str = "pub let double = fn(x) => x * 2\nlet secret = 41\nexport { double }\n";

#[test]
fn named_import_binds_the_export() {
    let _g = lock();
    let dir = scratch("named");
    write(&dir, "m.ae", MATH);
    let mut env = fresh_env();
    let v = run_in(
        &dir,
        "import { double } from \"m.ae\"\ndouble(21)",
        &mut env,
    )
    .unwrap();
    assert_eq!(v, Value::Int(42));
    assert!(
        env.get_var("secret").is_none(),
        "unexported name must not leak"
    );
}

#[test]
fn named_import_with_alias_binds_the_alias() {
    let _g = lock();
    let dir = scratch("alias_named");
    write(&dir, "m.ae", MATH);
    let mut env = fresh_env();
    let v = run_in(
        &dir,
        "import { double as twice } from \"m.ae\"\ntwice(4)",
        &mut env,
    )
    .unwrap();
    assert_eq!(v, Value::Int(8));
    assert!(env.get_var("double").is_none(), "only the alias is bound");
}

#[test]
fn whole_module_import_binds_exports_into_scope() {
    let _g = lock();
    let dir = scratch("whole");
    write(&dir, "m.ae", MATH);
    let mut env = fresh_env();
    run_in(&dir, "import \"m.ae\"", &mut env).unwrap();
    assert!(env.get_var("double").is_some());
    assert!(env.get_var("secret").is_none());
}

#[test]
fn whole_module_import_with_alias_binds_a_record_of_exports() {
    let _g = lock();
    let dir = scratch("record");
    write(&dir, "m.ae", MATH);
    let mut env = fresh_env();
    run_in(&dir, "import \"m.ae\" as m", &mut env).unwrap();
    match env.get_var("m") {
        Some(Value::Record(fields)) => {
            assert!(fields.contains_key("double"));
            assert!(!fields.contains_key("secret"), "unexported name in record");
        }
        other => panic!("expected a Record bound as `m`, got {other:?}"),
    }
}

#[test]
fn importing_an_unexported_name_is_not_found_and_names_module_and_name() {
    let _g = lock();
    let dir = scratch("unexported");
    write(&dir, "m.ae", MATH);
    let mut env = fresh_env();
    let err = run_in(&dir, "import { secret } from \"m.ae\"", &mut env).unwrap_err();
    assert_eq!(code_of(&err), ErrorCode::NotFound);
    let msg = message_of(&err);
    assert!(msg.contains("secret"), "message names the item: {msg}");
    assert!(msg.contains("m.ae"), "message names the module: {msg}");
    assert!(env.get_var("secret").is_none());
}

#[test]
fn missing_module_is_not_found() {
    let _g = lock();
    let dir = scratch("missing");
    let mut env = fresh_env();
    let err = run_in(&dir, "import \"nope.ae\"", &mut env).unwrap_err();
    assert_eq!(code_of(&err), ErrorCode::NotFound);
    assert!(message_of(&err).contains("nope.ae"));
}

#[test]
fn url_sources_are_refused() {
    let _g = lock();
    let dir = scratch("url");
    let mut env = fresh_env();
    let err = run_in(&dir, "import \"https://example.com/m.ae\"", &mut env).unwrap_err();
    assert_eq!(code_of(&err), ErrorCode::BadArg);
}

#[test]
fn import_cycle_is_a_coded_error_naming_the_cycle() {
    let _g = lock();
    let dir = scratch("cycle");
    write(&dir, "a.ae", "import \"b.ae\"\npub let a = 1\n");
    write(&dir, "b.ae", "import \"a.ae\"\npub let b = 2\n");
    let mut env = fresh_env();
    let err = run_in(&dir, "import \"a.ae\"", &mut env).unwrap_err();
    assert_eq!(code_of(&err), ErrorCode::BadArg);
    let msg = message_of(&err);
    assert!(msg.contains("cycle"), "message says cycle: {msg}");
    assert!(
        msg.contains("a.ae") && msg.contains("b.ae"),
        "names both: {msg}"
    );
}

#[test]
fn import_depth_is_bounded() {
    let _g = lock();
    let dir = scratch("depth");
    let chain = aethershell::module_loader::MAX_IMPORT_DEPTH + 8;
    for i in 0..chain {
        let body = if i + 1 < chain {
            format!("import \"m{}.ae\"\npub let v{i} = {i}\n", i + 1)
        } else {
            format!("pub let v{i} = {i}\n")
        };
        write(&dir, &format!("m{i}.ae"), &body);
    }
    let mut env = fresh_env();
    let err = run_in(&dir, "import \"m0.ae\"", &mut env).unwrap_err();
    assert_eq!(code_of(&err), ErrorCode::BudgetExceeded);
    assert!(message_of(&err).contains("depth"));
}

#[test]
fn oversized_module_is_refused_before_it_is_evaluated() {
    let _g = lock();
    let dir = scratch("oversize");
    let limit = aethershell::module_loader::MAX_MODULE_BYTES as usize;
    let mut body = String::from("# ");
    body.push_str(&"x".repeat(limit));
    body.push('\n');
    write(&dir, "big.ae", &body);
    let mut env = fresh_env();
    let err = run_in(&dir, "import \"big.ae\"", &mut env).unwrap_err();
    assert_eq!(code_of(&err), ErrorCode::BudgetExceeded);
}

#[test]
fn reimporting_a_module_evaluates_it_once() {
    let _g = lock();
    let dir = scratch("cached");
    let marker = dir.join("marker.txt");
    let module = format!(
        "file.append(\"{}\", \"ran\\n\")\npub let v = 1\n",
        marker.display().to_string().replace('\\', "/")
    );
    write(&dir, "side.ae", &module);
    let mut env = fresh_env();
    run_in(
        &dir,
        "import \"side.ae\"\nimport \"side.ae\" as s\nimport { v } from \"side.ae\"",
        &mut env,
    )
    .unwrap();
    let mut other = fresh_env();
    run_in(&dir, "import \"side.ae\"", &mut other).unwrap();
    let ran = std::fs::read_to_string(&marker).expect("module ran at least once");
    assert_eq!(
        ran.lines().count(),
        1,
        "module body ran {} times",
        ran.lines().count()
    );
}

#[test]
fn errors_inside_a_module_carry_the_module_path() {
    let _g = lock();
    let dir = scratch("inner_error");
    write(&dir, "bad.ae", "pub let ok = 1\nthrow \"boom\"\n");
    let mut env = fresh_env();
    let err = run_in(&dir, "import \"bad.ae\"", &mut env).unwrap_err();
    let msg = message_of(&err);
    assert!(msg.contains("bad.ae"), "module path in message: {msg}");
    assert!(msg.contains("boom"), "inner cause kept: {msg}");
}

#[test]
fn nested_module_errors_name_every_module_on_the_way_out() {
    let _g = lock();
    let dir = scratch("nested_error");
    write(&dir, "inner.ae", "throw \"deep\"\n");
    write(&dir, "outer.ae", "import \"inner.ae\"\npub let x = 1\n");
    let mut env = fresh_env();
    let err = run_in(&dir, "import \"outer.ae\"", &mut env).unwrap_err();
    let msg = message_of(&err);
    assert!(
        msg.contains("inner.ae") && msg.contains("outer.ae"),
        "both: {msg}"
    );
}

#[test]
fn export_list_in_a_directly_run_script_is_recorded_without_error() {
    let _g = lock();
    let dir = scratch("script_export");
    let mut env = fresh_env();
    run_in(&dir, "let x = 1\nexport { x }", &mut env).unwrap();
    assert!(env.is_exported("x"));
}

#[test]
fn export_of_an_undefined_name_is_not_found() {
    let _g = lock();
    let dir = scratch("export_undefined");
    let mut env = fresh_env();
    let err = run_in(&dir, "export { nothing_here }", &mut env).unwrap_err();
    assert_eq!(code_of(&err), ErrorCode::NotFound);
}

#[test]
fn export_with_alias_exports_the_alias() {
    let _g = lock();
    let dir = scratch("export_alias");
    write(&dir, "m.ae", "let inner = 5\nexport { inner as outer }\n");
    let mut env = fresh_env();
    let v = run_in(&dir, "import { outer } from \"m.ae\"\nouter", &mut env).unwrap();
    assert_eq!(v, Value::Int(5));
}

#[test]
fn re_export_passes_through_another_modules_export() {
    let _g = lock();
    let dir = scratch("reexport");
    write(&dir, "m.ae", MATH);
    write(&dir, "r.ae", "export { double } from \"m.ae\"\n");
    let mut env = fresh_env();
    let v = run_in(
        &dir,
        "import { double } from \"r.ae\"\ndouble(10)",
        &mut env,
    )
    .unwrap();
    assert_eq!(v, Value::Int(20));
}

#[test]
fn agent_mode_refuses_an_import_from_outside_the_workspace() {
    let _g = lock();
    let root = scratch("jail");
    let ws = root.join("workspace");
    let outside = root.join("outside");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    write(&outside, "m.ae", MATH);
    write(&ws, "inside.ae", MATH);
    let _agent = AgentEnv::set(&ws);

    // Positive control: a module inside the workspace still imports.
    let mut env = fresh_env();
    let v = run_in(
        &ws,
        "import { double } from \"inside.ae\"\ndouble(3)",
        &mut env,
    )
    .unwrap();
    assert_eq!(v, Value::Int(6));

    // Relative escape from the importing file's directory.
    let mut env = fresh_env();
    let err = run_in(&ws, "import \"../outside/m.ae\"", &mut env).unwrap_err();
    assert_eq!(code_of(&err), ErrorCode::OutsideWorkspace);

    // Absolute path outside the workspace.
    let abs = outside.join("m.ae");
    let src = format!(
        "import \"{}\"",
        abs.display().to_string().replace('\\', "/")
    );
    let mut env = fresh_env();
    let err = run_in(&ws, &src, &mut env).unwrap_err();
    assert_eq!(code_of(&err), ErrorCode::OutsideWorkspace);
}

#[test]
fn relative_import_resolves_against_the_importing_modules_directory() {
    let _g = lock();
    let dir = scratch("nested_dir");
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    write(&dir.join("lib"), "util.ae", "pub let three = 3\n");
    // `lib/outer.ae` imports `util.ae` next to itself, not from the script's directory.
    write(
        &dir.join("lib"),
        "outer.ae",
        "import \"util.ae\"\npub let four = three + 1\n",
    );
    let mut env = fresh_env();
    let v = run_in(
        &dir,
        "import { four } from \"lib/outer.ae\"\nfour",
        &mut env,
    )
    .unwrap();
    assert_eq!(v, Value::Int(4));
    assert!(
        env.get_var("three").is_none(),
        "a module's own imports are not its exports"
    );
}
