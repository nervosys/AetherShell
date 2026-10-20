//! Contracts for the k8s_* builtins.
//!
//! No test here reaches a cluster. The user's kubeconfig is never read: every
//! kubectl call a test makes runs in a child `ae` process whose `KUBECONFIG`
//! names a temporary file, with one context and one server,
//! https://127.0.0.1:1, where nothing listens. kubectl therefore fails at
//! once with a connection error, or the call fails as E_TOOL_MISSING when
//! kubectl is not installed. Each child has a 30 second deadline and a
//! non-interactive stdin.
//!
//! In-process calls are made only for refusals whose body returns before any
//! process is spawned; the tests say so where they are made.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use aethershell::builtins::{call, call_with_input};
use aethershell::env::Env;
use aethershell::signature::signature_of;
use aethershell::value::Value;

const DEADLINE: Duration = Duration::from_secs(30);

/// The only server any kubeconfig in this file names. Nothing listens on port 1.
const DEAD_SERVER: &str = "127.0.0.1:1";

/// A kubeconfig with one context, `dead-ctx`, on the dead server.
const KUBECONFIG: &str = r#"apiVersion: v1
kind: Config
clusters:
- name: dead
  cluster:
    server: https://127.0.0.1:1
contexts:
- name: dead-ctx
  context:
    cluster: dead
    user: nobody
current-context: dead-ctx
users:
- name: nobody
  user:
    token: unused
"#;

const MANIFEST: &str = "apiVersion: v1\nkind: Namespace\nmetadata:\n  name: ae-test\n";

/// One test's temporary world. Removed when the test ends.
struct Fixture {
    root: PathBuf,
    kubeconfig: PathBuf,
    workspace: PathBuf,
    session: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ae_k8s_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let workspace = root.join("ws");
        let session = root.join("session");
        std::fs::create_dir_all(&workspace).expect("create workspace");
        std::fs::create_dir_all(&session).expect("create session directory");
        let kubeconfig = root.join("kubeconfig");
        std::fs::write(&kubeconfig, KUBECONFIG).expect("write temporary kubeconfig");
        Self {
            root,
            kubeconfig,
            workspace,
            session,
        }
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

/// Run `ae -c <expr>` in the fixture's workspace, against the dead kubeconfig,
/// with an optional AETHER_MODE. Stdout and stderr come back together.
fn ae(fx: &Fixture, mode: Option<&str>, expr: &str) -> Run {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ae"));
    cmd.current_dir(&fx.workspace)
        .args(["-c", expr])
        .env("KUBECONFIG", &fx.kubeconfig)
        .env("AETHER_SESSION_DIR", &fx.session)
        .env("AETHER_WORKSPACE", &fx.workspace)
        .env_remove("AETHER_MODE")
        .env_remove("AETHER_AGENT")
        .env_remove("AETHER_POLICY")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(mode) = mode {
        cmd.env("AETHER_MODE", mode);
    }
    let mut child = cmd.spawn().expect("spawn ae");
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
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    Run {
        success: status.success(),
        output: format!("{stdout}{stderr}"),
    }
}

/// Whether kubectl is installed. `kubectl version --client` contacts no server.
fn kubectl_present() -> bool {
    Command::new("kubectl")
        .args(["version", "--client"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// A path written into an `ae` string literal: forward slashes, no quotes.
fn path_literal(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// The refusal message for a call that must not succeed. Used only for
/// refusals whose body returns before any spawn.
fn refused(name: &str, args: Vec<Value>) -> String {
    let mut env = Env::new();
    match call(name, args, &mut env) {
        Ok(v) => panic!("{name} accepted a call it must refuse, and answered {v:?}"),
        Err(e) => e.to_string(),
    }
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

// ---------------------------------------------------------------------------
// In-process: refusals that return before any process is spawned.
// ---------------------------------------------------------------------------

#[test]
fn argument_refusals_come_before_any_kubectl_call() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        // An option-looking resource, namespace or name is refused, never passed on.
        ("k8s_get", vec![s("-o")]),
        ("k8s_get", vec![s("pods"), s("-A")]),
        ("k8s_get", vec![s("pods"), s("a b")]),
        ("k8s_get", vec![s("pods"), s("")]),
        ("k8s_get", vec![s("pods"), s("ns"), s("extra")]),
        ("k8s_pods", vec![s("--all-namespaces")]),
        ("k8s_pods", vec![Value::Int(1)]),
        ("k8s_namespaces", vec![s("x")]),
        ("k8s_nodes", vec![s("x")]),
        ("k8s_events", vec![s("ns"), s("extra")]),
        ("k8s_secrets", vec![s("-A")]),
        ("k8s_top_pods", vec![s("-A")]),
        ("k8s_top_nodes", vec![s("x")]),
        ("k8s_contexts", vec![s("x")]),
        ("k8s_cluster_info", vec![s("x")]),
        // A manifest is a local file; a URL and an option are refused.
        ("k8s_apply", vec![s("--validate=false")]),
        ("k8s_apply", vec![s("https://example.invalid/app.yaml")]),
        // Delete names are checked before the destructive guard runs.
        ("k8s_delete", vec![s("pods"), s("-x")]),
        ("k8s_describe", vec![s("pods"), s("p"), s("-n")]),
        // Logs are bounded: the tail is 1 to 10000 lines.
        ("k8s_logs", vec![s("-p")]),
        ("k8s_logs", vec![s("p"), s("ns"), Value::Int(0)]),
        ("k8s_logs", vec![s("p"), s("ns"), Value::Int(10_001)]),
        // Exec takes argv, never a shell string, and never an empty argv.
        ("k8s_exec", vec![s("p"), s("id")]),
        ("k8s_exec", vec![s("p"), Value::Array(vec![])]),
        ("k8s_exec", vec![s("p"), Value::Array(vec![Value::Int(1)])]),
        ("k8s_exec", vec![s("-p"), Value::Array(vec![s("id")])]),
        // Replica counts are bounded to 0..=1000.
        ("k8s_scale", vec![s("web"), Value::Int(-1)]),
        ("k8s_scale", vec![s("web"), Value::Int(1_001)]),
        ("k8s_scale", vec![s("web"), s("3")]),
        ("k8s_scale", vec![s("--web"), Value::Int(1)]),
        ("k8s_rollout_status", vec![s("--watch")]),
        ("k8s_rollout_restart", vec![s("")]),
        ("k8s_context", vec![s("-x")]),
        ("k8s_context", vec![Value::Int(1)]),
    ];
    for (name, args) in cases {
        let msg = refused(name, args);
        assert!(
            msg.contains("E_BAD_ARG"),
            "{name} must refuse with E_BAD_ARG before kubectl runs: {msg}"
        );
    }
}

#[test]
fn piped_input_is_refused_by_every_builtin_that_takes_none() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("k8s_pods", vec![]),
        ("k8s_nodes", vec![]),
        ("k8s_contexts", vec![]),
        ("k8s_context", vec![]),
        ("k8s_cluster_info", vec![]),
        ("k8s_top_nodes", vec![]),
        ("k8s_get", vec![s("pods")]),
        ("k8s_delete", vec![s("pods"), s("web")]),
        ("k8s_exec", vec![s("web"), Value::Array(vec![s("id")])]),
    ];
    for (name, args) in cases {
        let mut env = Env::new();
        let err = call_with_input(name, args, Some(s("subject")), &mut env)
            .expect_err(&format!("{name} must refuse a pipeline subject it ignores"));
        let msg = format!("{err}");
        assert!(
            msg.contains("E_BAD_ARG") && msg.contains("pipeline input"),
            "{name}: {msg}"
        );
    }
}

#[test]
fn kubectl_spellings_are_declared_under_the_k8s_name() {
    for (alias, canonical) in [
        ("kubectl_get", "k8s_get"),
        ("kubectl_apply", "k8s_apply"),
        ("kubectl_delete", "k8s_delete"),
        ("kubectl_describe", "k8s_describe"),
        ("kubectl_logs", "k8s_logs"),
        ("kubectl_exec", "k8s_exec"),
    ] {
        let sig = signature_of(alias)
            .unwrap_or_else(|| panic!("{alias} is dispatched but has no declaration"));
        assert_eq!(sig.name, canonical, "{alias} must resolve to {canonical}");
    }
}

// ---------------------------------------------------------------------------
// Through the `ae` binary, against the dead server.
// ---------------------------------------------------------------------------

#[test]
fn the_kubeconfig_names_its_one_context() {
    let fx = Fixture::new("contexts");
    let present = kubectl_present();

    let run = ae(&fx, None, "k8s_contexts()");
    if !present {
        assert!(run.output.contains("E_TOOL_MISSING"), "{}", run.output);
        return;
    }
    assert!(run.success, "{}", run.output);
    assert!(run.output.contains("dead-ctx"), "{}", run.output);

    let run = ae(&fx, None, "k8s_context()");
    assert!(run.success, "{}", run.output);
    assert!(run.output.contains("dead-ctx"), "{}", run.output);

    let run = ae(&fx, None, "k8s_context(\"dead-ctx\")");
    assert!(run.success, "{}", run.output);

    let run = ae(&fx, None, "k8s_context(\"no-such-context\")");
    assert!(!run.success, "{}", run.output);
    assert!(run.output.contains("E_TOOL_FAILED"), "{}", run.output);
}

#[test]
fn every_cluster_call_fails_coded_against_the_dead_server() {
    let fx = Fixture::new("dead");
    let present = kubectl_present();
    let calls = [
        "k8s_get(\"pods\", \"default\")",
        "k8s_pods()",
        "k8s_services(\"default\")",
        "k8s_deployments()",
        "k8s_namespaces()",
        "k8s_nodes()",
        "k8s_events(\"default\")",
        "k8s_configmaps()",
        "k8s_secrets()",
        "k8s_ingresses()",
        "k8s_describe(\"pod\", \"web\")",
        "k8s_logs(\"web\", \"default\", 5)",
        "k8s_exec(\"web\", [\"id\"])",
        "k8s_delete(\"pod\", \"web\")",
        "k8s_scale(\"web\", 1)",
        "k8s_rollout_status(\"web\")",
        "k8s_rollout_restart(\"web\")",
        "k8s_top_pods()",
        "k8s_top_nodes()",
        "k8s_cluster_info()",
    ];
    for expr in calls {
        let run = ae(&fx, None, expr);
        if !present {
            assert!(
                run.output.contains("E_TOOL_MISSING"),
                "{expr}: {}",
                run.output
            );
            continue;
        }
        assert!(
            !run.success,
            "{expr} must fail against {DEAD_SERVER}: {}",
            run.output
        );
        assert!(
            run.output.contains("E_TOOL_FAILED"),
            "{expr} must fail coded: {}",
            run.output
        );
    }
}

#[test]
fn agent_mode_asks_for_approval_before_kubectl_is_run() {
    let fx = Fixture::new("approval");
    for expr in [
        "k8s_delete(\"pods\", \"web\")",
        "k8s_exec(\"web\", [\"id\"])",
        "k8s_rollout_restart(\"web\")",
    ] {
        let run = ae(&fx, Some("agent"), expr);
        assert!(
            !run.success && run.output.contains("E_NEEDS_APPROVAL"),
            "{expr} must need approval in agent mode: {}",
            run.output
        );
        assert!(
            !run.output.contains(DEAD_SERVER),
            "{expr} reached kubectl before approval: {}",
            run.output
        );
    }
}

#[test]
fn apply_reads_only_a_manifest_inside_the_workspace() {
    let fx = Fixture::new("apply");
    let outside = fx.root.join("outside.yaml");
    std::fs::write(&outside, MANIFEST).expect("write outside manifest");
    std::fs::write(fx.workspace.join("inside.yaml"), MANIFEST).expect("write inside manifest");

    // A manifest outside the workspace is refused by the read guard, before kubectl.
    let run = ae(
        &fx,
        None,
        &format!("k8s_apply(\"{}\")", path_literal(&outside)),
    );
    assert!(
        !run.success && run.output.contains("E_OUTSIDE_WORKSPACE"),
        "{}",
        run.output
    );
    assert!(!run.output.contains(DEAD_SERVER), "{}", run.output);

    if !kubectl_present() {
        return;
    }
    let run = ae(&fx, None, "k8s_apply(\"inside.yaml\")");
    assert!(!run.success, "{}", run.output);
    assert!(run.output.contains("E_TOOL_FAILED"), "{}", run.output);
    assert!(run.output.contains(DEAD_SERVER), "{}", run.output);
}
