//! Contracts for the package and build-tool builtins: the npm, pnpm, yarn, bun,
//! deno, uv, poetry, cargo, go, pre-commit, ruff and black families.
//!
//! No test installs anything, reaches a registry, touches the user's home, or
//! runs cargo inside the repository.
//!
//! * In-process calls are refusals only. Each one fails validation before a
//!   process is spawned, and each is written so that a body which accepted it
//!   would still act only on names the test controls.
//! * Missing tools are checked with a child `ish` whose PATH names an empty
//!   directory, so the answer is E_TOOL_MISSING on every host, whether or not
//!   the tool is installed.
//! * Real runs happen only in a fresh temporary workspace, through a child `ish`
//!   with every cache and config directory pointed inside that workspace. Go
//!   builds a module with no dependencies and GOPROXY=off; ruff and black act
//!   on one temporary file; deno runs a script that imports nothing. Each real
//!   run is skipped when its tool is not installed, and the missing-tool test
//!   covers that case.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use iron_shell::builtins::{call, call_with_input, is_dispatched};
use iron_shell::env::Env;
use iron_shell::signature::signature_of;
use iron_shell::value::Value;

/// Deadline for a child `ish` that only refuses or reports a missing tool.
const DEADLINE: Duration = Duration::from_secs(30);
/// Deadline for a child that really runs a compiler, which can be slow on a
/// cold cache.
const LONG_DEADLINE: Duration = Duration::from_secs(300);

/// The 25 canonical names of the tranche and the one alias it declares
/// (`black` for `black_format`).
const TRANCHE: &[&str] = &[
    "black_format",
    "bun_install",
    "bun_run",
    "cargo_build_cmd",
    "cargo_run",
    "cargo_test_cmd",
    "deno_run",
    "deno_task",
    "go_build",
    "go_run",
    "go_test",
    "npm_install",
    "npm_run",
    "pnpm_install",
    "pnpm_run",
    "poetry_install",
    "poetry_run",
    "pre_commit_install",
    "pre_commit_run",
    "ruff_check",
    "ruff_format",
    "uv_pip",
    "uv_run",
    "uv_venv",
    "yarn_install",
    "yarn_run",
];

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

/// One test's temporary world. Removed when the test ends.
struct Fixture {
    root: PathBuf,
    workspace: PathBuf,
    session: PathBuf,
    caches: PathBuf,
    empty_path: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ae_pkg_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let fx = Self {
            workspace: root.join("ws"),
            session: root.join("session"),
            caches: root.join("caches"),
            empty_path: root.join("empty-path"),
            root,
        };
        for dir in [&fx.workspace, &fx.session, &fx.caches, &fx.empty_path] {
            std::fs::create_dir_all(dir).expect("create temporary directory");
        }
        fx
    }

    /// A directory inside the fixture but outside the workspace.
    fn outside(&self) -> PathBuf {
        let dir = self.root.join("outside");
        std::fs::create_dir_all(&dir).expect("create outside directory");
        dir
    }
}

struct Run {
    success: bool,
    output: String,
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

/// Run `ish -c <expr>` in the fixture's workspace. `path_var` is the PATH the
/// child sees; `extra` adds environment variables. Caches and tool homes are
/// all inside the fixture.
fn ae(fx: &Fixture, path_var: &str, extra: &[(&str, String)], expr: &str, limit: Duration) -> Run {
    let caches = &fx.caches;
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ish"));
    cmd.current_dir(&fx.workspace)
        .args(["-c", expr])
        .env("IRONSHELL_SESSION_DIR", &fx.session)
        .env("IRONSHELL_WORKSPACE", &fx.workspace)
        .env("PATH", path_var)
        .env("CHECKPOINT_DISABLE", "1")
        .env("RUFF_CACHE_DIR", caches.join("ruff"))
        .env("RUFF_NO_CACHE", "1")
        .env("BLACK_CACHE_DIR", caches.join("black"))
        .env("DENO_DIR", caches.join("deno"))
        .env("UV_CACHE_DIR", caches.join("uv"))
        .env("CARGO_HOME", caches.join("cargo-home"))
        .env("CARGO_TARGET_DIR", caches.join("cargo-target"))
        .env("GOTELEMETRY", "off")
        .env("GOTOOLCHAIN", "local")
        .env("GOPROXY", "off")
        .env("GOFLAGS", "-mod=mod")
        .env("GOCACHE", std::env::temp_dir().join("ae_pkg_gocache"))
        .env("GOPATH", caches.join("gopath"))
        .env("GOMODCACHE", caches.join("gopath").join("pkg").join("mod"))
        .env("GOENV", caches.join("goenv"))
        .env_remove("IRONSHELL_MODE")
        .env_remove("IRONSHELL_AGENT")
        .env_remove("IRONSHELL_POLICY")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in extra {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().expect("spawn ae");
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll ae") {
            break status;
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            panic!("ae did not finish within {limit:?}: {expr}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    Run {
        success: status.success(),
        output: format!("{stdout}{stderr}"),
    }
}

/// Whether a program answers a probe on this host. The probes are local.
fn installed(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// A path written into an `ish` string literal: forward slashes, no quotes.
fn literal(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// The refusal message for a call that must not succeed. Used only for calls
/// that return before any process is spawned.
fn refused(name: &str, args: Vec<Value>) -> String {
    let mut env = Env::new();
    match call(name, args, &mut env) {
        Ok(v) => panic!("{name} accepted a call it must refuse, and answered {v:?}"),
        Err(e) => e.to_string(),
    }
}

/// The refusal message for a piped call that must not succeed.
fn refused_piped(name: &str, args: Vec<Value>, input: Value) -> String {
    let mut env = Env::new();
    match call_with_input(name, args, Some(input), &mut env) {
        Ok(v) => panic!("{name} accepted pipeline input, and answered {v:?}"),
        Err(e) => e.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Declarations.
// ---------------------------------------------------------------------------

#[test]
fn every_tranche_name_is_declared_and_dispatched() {
    for name in TRANCHE {
        let sig = signature_of(name).unwrap_or_else(|| panic!("{name} has no declaration"));
        assert!(is_dispatched(name), "{name} is not dispatched");
        assert_eq!(sig.name, *name, "{name} is declared under another name");
    }
    let alias = signature_of("black").expect("the black alias is declared");
    assert_eq!(alias.name, "black_format");
    assert!(alias.aliases.contains(&"black"));
}

// ---------------------------------------------------------------------------
// In-process refusals: each returns before a process is spawned.
// ---------------------------------------------------------------------------

#[test]
fn option_like_names_are_refused_as_arguments() {
    // A leading '-' would reach the tool as one of its flags. Each of these is
    // a value a tool would otherwise read as an option.
    let cases: &[(&str, &str)] = &[
        ("uv_run", "--python=/tmp/x"),
        ("uv_pip", "-r"),
        ("uv_pip", "--index-url=https://example.test"),
        ("uv_venv", "-x"),
        ("poetry_run", "--help"),
        ("npm_run", "-x"),
        ("npm_install", "-g"),
        ("npm_install", "--registry=https://example.test"),
        ("pnpm_run", "-x"),
        ("pnpm_install", "-g"),
        ("yarn_run", "-x"),
        ("yarn_install", "--global"),
        ("bun_run", "--preload=x"),
        ("bun_install", "-g"),
        ("deno_run", "--allow-all"),
        ("deno_task", "-x"),
        ("pre_commit_run", "-x"),
        ("ruff_check", "--fix"),
        ("ruff_format", "--config=x"),
        ("black_format", "--code=x"),
        ("black", "-c"),
        ("go_run", "-exec=x"),
        ("go_build", "-toolexec=x"),
        ("go_build", "-o"),
        ("go_test", "-exec=x"),
    ];
    for (name, value) in cases {
        let msg = refused(name, vec![s(value)]);
        assert!(msg.contains("E_BAD_ARG"), "{name}({value:?}): {msg}");
    }
}

#[test]
fn cargo_refuses_options_it_must_not_receive() {
    // `--config` can point cargo at a wrapper; `--manifest-path` can reach a
    // project outside the workspace.
    for value in ["--config", "--manifest-path", "--target-dir", "-Zunstable"] {
        let msg = refused("cargo_run", vec![s(value)]);
        assert!(msg.contains("E_BAD_ARG"), "cargo_run({value}): {msg}");
        let msg = refused("cargo_test_cmd", vec![s(value)]);
        assert!(msg.contains("E_BAD_ARG"), "cargo_test_cmd({value}): {msg}");
    }
    // cargo_build_cmd takes only "--release". Its valid form is never called
    // in-process: it would build the repository.
    let msg = refused("cargo_build_cmd", vec![s("--verbose")]);
    assert!(msg.contains("E_BAD_ARG"), "{msg}");
}

#[test]
fn pipeline_input_is_refused_by_every_tranche_builtin() {
    // A builtin with a required argument refuses a bare call in its declared
    // signature, before its body runs. Each call here supplies valid arguments
    // so that the body is the one that refuses the pipeline input.
    let required: &[(&str, &str)] = &[
        ("uv_run", "a.py"),
        ("poetry_run", "pytest"),
        ("npm_run", "build"),
        ("pnpm_run", "build"),
        ("yarn_run", "build"),
        ("bun_run", "dev"),
        ("deno_run", "a.ts"),
        ("deno_task", "build"),
    ];
    for name in TRANCHE {
        let args: Vec<Value> = required
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| vec![s(v)])
            .unwrap_or_default();
        let msg = refused_piped(name, args, s("x"));
        assert!(
            msg.contains("E_BAD_ARG") && msg.contains("pipeline input"),
            "{name} took pipeline input: {msg}"
        );
    }
}

#[test]
fn surplus_arguments_are_refused_not_dropped() {
    let cases: &[(&str, Vec<Value>)] = &[
        ("poetry_install", vec![s("x")]),
        ("pre_commit_install", vec![s("x")]),
        ("npm_run", vec![s("build"), s("extra")]),
        ("pnpm_run", vec![s("build"), s("extra")]),
        ("yarn_run", vec![s("build"), s("extra")]),
        ("bun_run", vec![s("dev"), s("extra")]),
        ("deno_task", vec![s("build"), s("extra")]),
        ("deno_run", vec![s("a.ts"), s("extra")]),
        ("go_run", vec![s("."), s("extra")]),
        ("go_test", vec![s("./..."), s("extra")]),
        ("ruff_check", vec![s("a.py"), s("b.py")]),
        ("ruff_format", vec![s("a.py"), s("b.py")]),
        ("black_format", vec![s("a.py"), s("b.py")]),
        ("uv_venv", vec![s("a"), s("b")]),
        ("pre_commit_run", vec![s("a"), s("b")]),
        ("cargo_build_cmd", vec![s("--release"), s("--release")]),
        ("cargo_test_cmd", vec![s("a"), s("b")]),
    ];
    for (name, args) in cases {
        let msg = refused(name, args.clone());
        assert!(
            msg.contains("E_BAD_ARG"),
            "{name} with a surplus argument: {msg}"
        );
    }
}

#[test]
fn non_string_arguments_are_refused_not_dropped() {
    let cases: &[(&str, Vec<Value>)] = &[
        ("uv_run", vec![s("a.py"), Value::Int(1)]),
        ("uv_pip", vec![s("install"), Value::Int(1)]),
        ("poetry_run", vec![s("pytest"), Value::Int(1)]),
        ("npm_install", vec![s("left-pad"), Value::Int(1)]),
        ("pnpm_install", vec![Value::Int(1)]),
        ("yarn_install", vec![s("left-pad"), Value::Bool(true)]),
        ("bun_install", vec![Value::Int(1)]),
        ("cargo_run", vec![s("arg"), Value::Int(1)]),
        ("cargo_test_cmd", vec![Value::Int(1)]),
        ("go_build", vec![Value::Int(1)]),
        ("ruff_check", vec![Value::Int(1)]),
        ("npm_run", vec![Value::Int(1)]),
    ];
    for (name, args) in cases {
        let msg = refused(name, args.clone());
        assert!(
            msg.contains("E_BAD_ARG"),
            "{name} with a non-String argument: {msg}"
        );
    }
}

#[test]
fn local_path_specs_are_refused_for_installers() {
    // file: and link: install from a directory the workspace does not vet.
    for (name, spec) in [
        ("npm_install", "file:../elsewhere"),
        ("pnpm_install", "FILE:../elsewhere"),
        ("yarn_install", "link:../elsewhere"),
        ("bun_install", "file:/tmp/x"),
        ("uv_pip", "file:/tmp/x"),
    ] {
        let msg = refused(name, vec![s(spec)]);
        assert!(msg.contains("E_BAD_ARG"), "{name}({spec}): {msg}");
    }
}

#[test]
fn deno_refuses_remote_and_specifier_modules() {
    for spec in [
        "https://example.test/a.ts",
        "npm:left-pad",
        "jsr:@std/x",
        "file:///tmp/a.ts",
    ] {
        let msg = refused("deno_run", vec![s(spec)]);
        assert!(msg.contains("E_BAD_ARG"), "deno_run({spec}): {msg}");
    }
}

#[test]
fn uv_pip_offers_only_the_listed_subcommands() {
    for verb in ["compile", "sync", "bogus"] {
        let msg = refused("uv_pip", vec![s(verb), s("requests")]);
        assert!(msg.contains("E_BAD_ARG"), "uv_pip({verb}): {msg}");
    }
}

#[test]
fn pre_commit_run_takes_one_hook_or_all_files() {
    let msg = refused("pre_commit_run", vec![s("a"), s("--all-files")]);
    assert!(msg.contains("E_BAD_ARG"), "{msg}");
}

// ---------------------------------------------------------------------------
// Child runs: paths, missing tools, and real tools in a temporary workspace.
// ---------------------------------------------------------------------------

#[test]
fn paths_outside_the_workspace_are_refused_before_any_tool_runs() {
    let fx = Fixture::new("outside");
    let outside = literal(&fx.outside());
    // Each of these would act on the outside directory, and none can find a
    // tool to run because PATH is empty. The refusal is what is checked.
    let exprs = [
        format!("ruff_format(\"{outside}\")"),
        format!("black_format(\"{outside}\")"),
        format!("ruff_check(\"{outside}\")"),
        format!("go_build(\"{outside}\")"),
        format!("uv_venv(\"{outside}/.venv\")"),
        format!("npm_install(\"{outside}\")"),
        format!("uv_run(\"{outside}/a.py\")"),
        format!("deno_run(\"{outside}/a.ts\")"),
        "ruff_format(\"../escape\")".to_string(),
    ];
    let empty = fx.empty_path.display().to_string();
    for expr in &exprs {
        let run = ae(&fx, &empty, &[], expr, DEADLINE);
        assert!(
            !run.success && run.output.contains("E_OUTSIDE_WORKSPACE"),
            "{expr} was not refused as outside the workspace: {}",
            run.output
        );
    }
}

#[test]
fn a_missing_tool_is_the_coded_tool_missing_error() {
    // An empty PATH makes every tool absent on every host, so this checks the
    // coded answer without depending on what is installed.
    let fx = Fixture::new("missing");
    let empty = fx.empty_path.display().to_string();
    let exprs = [
        "uv_run(\"tool.py\")",
        "uv_pip(\"list\")",
        "uv_venv()",
        "poetry_run(\"pytest\")",
        "poetry_install()",
        "npm_run(\"build\")",
        "npm_install()",
        "pnpm_run(\"build\")",
        "pnpm_install()",
        "yarn_run(\"build\")",
        "yarn_install()",
        "bun_run(\"dev\")",
        "bun_install()",
        "cargo_run()",
        "cargo_build_cmd()",
        "cargo_test_cmd()",
        "go_run()",
        "go_build()",
        "go_test()",
        "deno_run(\"a.ts\")",
        "deno_task(\"build\")",
        "pre_commit_run()",
        "pre_commit_install()",
        "ruff_check()",
        "ruff_format()",
        "black_format()",
    ];
    for expr in exprs {
        let run = ae(&fx, &empty, &[], expr, DEADLINE);
        assert!(
            !run.success && run.output.contains("E_TOOL_MISSING"),
            "{expr} with no tool on PATH: {}",
            run.output
        );
    }
}

#[test]
fn go_build_builds_a_module_offline_and_reports_a_compile_error() {
    if !installed("go", &["version"]) {
        eprintln!("go is not installed; the missing-tool test covers this host");
        return;
    }
    let fx = Fixture::new("go");
    std::fs::write(
        fx.workspace.join("go.mod"),
        "module example.test/ok\n\ngo 1.18\n",
    )
    .expect("write go.mod");
    std::fs::write(
        fx.workspace.join("main.go"),
        "package main\n\nfunc main() {}\n",
    )
    .expect("write main.go");
    let path = std::env::var("PATH").unwrap_or_default();
    let run = ae(&fx, &path, &[], "go_build()", LONG_DEADLINE);
    assert!(
        run.success,
        "go_build of a valid module failed: {}",
        run.output
    );
    let binary = fx
        .workspace
        .join(if cfg!(windows) { "ok.exe" } else { "ok" });
    assert!(
        binary.is_file(),
        "go_build did not write {}",
        binary.display()
    );

    let broken = fx.workspace.join("broken");
    std::fs::create_dir_all(&broken).expect("create broken package");
    std::fs::write(
        broken.join("main.go"),
        "package main\n\nfunc main() { x := }\n",
    )
    .expect("write broken main.go");
    let run = ae(&fx, &path, &[], "go_build(\"./broken\")", LONG_DEADLINE);
    assert!(
        !run.success && run.output.contains("E_TOOL_FAILED"),
        "a compile error must be E_TOOL_FAILED, not a value: {}",
        run.output
    );
}

#[test]
fn ruff_reports_violations_as_an_array_and_formats_in_place() {
    if !installed("ruff", &["--version"]) {
        eprintln!("ruff is not installed; the missing-tool test covers this host");
        return;
    }
    let fx = Fixture::new("ruff");
    std::fs::write(fx.workspace.join("clean.py"), "x = 1\n").expect("write clean.py");
    std::fs::write(fx.workspace.join("dirty.py"), "import os\n").expect("write dirty.py");
    let path = std::env::var("PATH").unwrap_or_default();

    let run = ae(&fx, &path, &[], "ruff_check(\"dirty.py\")", DEADLINE);
    assert!(
        run.success,
        "a violation is an answer, not a failure: {}",
        run.output
    );
    assert!(run.output.contains("F401"), "{}", run.output);

    let run = ae(&fx, &path, &[], "ruff_check(\"clean.py\")", DEADLINE);
    assert!(run.success, "{}", run.output);
    assert!(run.output.contains("[]"), "{}", run.output);

    std::fs::write(fx.workspace.join("fmt.py"), "x=1\n").expect("write fmt.py");
    let run = ae(&fx, &path, &[], "ruff_format(\"fmt.py\")", DEADLINE);
    assert!(run.success, "{}", run.output);
    let text = std::fs::read_to_string(fx.workspace.join("fmt.py")).expect("read fmt.py");
    assert_eq!(text, "x = 1\n", "ruff_format did not rewrite the file");
}

#[test]
fn black_formats_one_file_in_the_workspace() {
    if !installed("black", &["--version"]) {
        eprintln!("black is not installed; the missing-tool test covers this host");
        return;
    }
    let fx = Fixture::new("black");
    std::fs::write(fx.workspace.join("b.py"), "x=1\n").expect("write b.py");
    let path = std::env::var("PATH").unwrap_or_default();
    let run = ae(&fx, &path, &[], "black_format(\"b.py\")", DEADLINE);
    assert!(run.success, "{}", run.output);
    let text = std::fs::read_to_string(fx.workspace.join("b.py")).expect("read b.py");
    assert_eq!(text, "x = 1\n");
}

#[test]
fn deno_grants_no_permission_it_was_not_asked_for() {
    if !installed("deno", &["--version"]) {
        eprintln!("deno is not installed; the missing-tool test covers this host");
        return;
    }
    let fx = Fixture::new("deno");
    std::fs::write(fx.workspace.join("hello.ts"), "console.log(\"ran\");\n")
        .expect("write hello.ts");
    std::fs::write(
        fx.workspace.join("read.ts"),
        "Deno.readTextFileSync(\"hello.ts\");\n",
    )
    .expect("write read.ts");
    let path = std::env::var("PATH").unwrap_or_default();

    let run = ae(&fx, &path, &[], "deno_run(\"hello.ts\")", DEADLINE);
    assert!(run.success && run.output.contains("ran"), "{}", run.output);

    // Without --allow-read the script is denied, and --no-prompt keeps the
    // denial from waiting on a prompt that nobody can answer.
    let run = ae(&fx, &path, &[], "deno_run(\"read.ts\")", DEADLINE);
    assert!(
        !run.success && run.output.contains("E_TOOL_FAILED"),
        "a denied read must be a coded failure: {}",
        run.output
    );
}
