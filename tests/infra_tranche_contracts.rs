//! Contracts for the terraform_* and helm_* builtins.
//!
//! No test touches real infrastructure or a real cluster.
//!
//! * Terraform runs only in a fresh temporary directory whose one configuration
//!   is `output "x" { value = 1 }`: no providers, so nothing is reached. Every
//!   terraform call is made by a child `ish` whose workspace is that directory,
//!   with TF_DATA_DIR and TF_CLI_CONFIG_FILE inside it. A directory outside the
//!   workspace is never run, and the tests check that by looking for the state
//!   a run would have written there.
//! * Helm runs only against a temporary kubeconfig whose one server is
//!   https://127.0.0.1:1, with HELM_CONFIG_HOME, HELM_CACHE_HOME and
//!   HELM_DATA_HOME inside the temporary directory. No repository is added and
//!   nothing is searched on the network.
//!
//! Each child has a 30 second deadline and a closed stdin. In-process calls are
//! made only for refusals whose body returns before any process is spawned.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use iron_shell::builtins::{call, call_with_input};
use iron_shell::env::Env;
use iron_shell::signature::signature_of;
use iron_shell::value::Value;

const DEADLINE: Duration = Duration::from_secs(30);

/// The only server any kubeconfig in this file names. Nothing listens on port 1.
const DEAD_SERVER: &str = "127.0.0.1:1";

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

/// The one configuration every terraform call runs. It names no provider.
const CONFIG: &str = "output \"x\" {\n  value = 1\n}\n";

const TERRAFORM_NAMES: &[&str] = &[
    "terraform_init",
    "terraform_plan",
    "terraform_apply",
    "terraform_destroy",
    "terraform_state",
    "terraform_output",
    "terraform_workspace",
    "terraform_validate",
];

const HELM_NAMES: &[&str] = &[
    "helm_list",
    "helm_install",
    "helm_upgrade",
    "helm_uninstall",
    "helm_repos",
    "helm_search",
    "helm_status",
];

/// One test's temporary world. Removed when the test ends.
struct Fixture {
    root: PathBuf,
    kubeconfig: PathBuf,
    workspace: PathBuf,
    session: PathBuf,
    helm_home: PathBuf,
    tf_data: PathBuf,
    tf_rc: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ae_infra_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let workspace = root.join("ws");
        let session = root.join("session");
        let helm_home = root.join("helm");
        let tf_data = root.join("tf-data");
        for dir in [&workspace, &session, &helm_home, &tf_data] {
            std::fs::create_dir_all(dir).expect("create temporary directory");
        }
        std::fs::write(workspace.join("main.tf"), CONFIG).expect("write configuration");
        let kubeconfig = root.join("kubeconfig");
        std::fs::write(&kubeconfig, KUBECONFIG).expect("write temporary kubeconfig");
        let tf_rc = root.join("terraform.rc");
        std::fs::write(&tf_rc, "").expect("write empty terraform CLI config");
        Self {
            root,
            kubeconfig,
            workspace,
            session,
            helm_home,
            tf_data,
            tf_rc,
        }
    }

    /// A directory outside the workspace, holding the same configuration. A
    /// run that reached it would write a state file there.
    fn outside(&self) -> PathBuf {
        let dir = self.root.join("outside");
        std::fs::create_dir_all(&dir).expect("create outside directory");
        std::fs::write(dir.join("main.tf"), CONFIG).expect("write outside configuration");
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

/// Run `ish -c <expr>` in the fixture's workspace, with both tools pointed inside
/// the fixture. Stdout and stderr come back together.
fn ae(fx: &Fixture, mode: Option<&str>, expr: &str) -> Run {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ish"));
    cmd.current_dir(&fx.workspace)
        .args(["-c", expr])
        .env("IRONSHELL_SESSION_DIR", &fx.session)
        .env("IRONSHELL_WORKSPACE", &fx.workspace)
        .env("KUBECONFIG", &fx.kubeconfig)
        .env("HELM_CONFIG_HOME", fx.helm_home.join("config"))
        .env("HELM_CACHE_HOME", fx.helm_home.join("cache"))
        .env("HELM_DATA_HOME", fx.helm_home.join("data"))
        .env("TF_DATA_DIR", &fx.tf_data)
        .env("TF_CLI_CONFIG_FILE", &fx.tf_rc)
        .env("TF_IN_AUTOMATION", "1")
        .env("CHECKPOINT_DISABLE", "1")
        .env_remove("IRONSHELL_MODE")
        .env_remove("IRONSHELL_AGENT")
        .env_remove("IRONSHELL_POLICY")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(mode) = mode {
        cmd.env("IRONSHELL_MODE", mode);
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

/// Whether a program answers `version`-style probes on this host. Nothing
/// listed here contacts a server: the probes are local.
fn installed(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .env("CHECKPOINT_DISABLE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn terraform_present() -> bool {
    installed("terraform", &["version"])
}

fn helm_present() -> bool {
    installed("helm", &["version", "--short"])
}

/// A path written into an `ish` string literal: forward slashes, no quotes.
fn path_literal(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

/// The refusal message for a call that must not succeed. Used only for
/// refusals whose body returns before any process is spawned.
fn refused(name: &str, args: Vec<Value>) -> String {
    let mut env = Env::new();
    match call(name, args, &mut env) {
        Ok(v) => panic!("{name} accepted a call it must refuse, and answered {v:?}"),
        Err(e) => e.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Declarations.
// ---------------------------------------------------------------------------

#[test]
fn every_tranche_name_is_declared_under_its_own_name() {
    for name in TERRAFORM_NAMES.iter().chain(HELM_NAMES) {
        let sig = signature_of(name).unwrap_or_else(|| panic!("{name} has no declaration"));
        assert_eq!(sig.name, *name);
        assert!(
            sig.aliases.is_empty(),
            "{name} declares aliases it does not have"
        );
    }
}

#[test]
fn helm_repos_takes_no_arguments_and_says_so() {
    let sig = signature_of("helm_repos").expect("helm_repos is declared");
    assert!(sig.params.is_empty());
}

// ---------------------------------------------------------------------------
// In-process: refusals that return before any process is spawned.
//
// Every case here must fail validation. None may pass it: a case that did
// would run terraform or helm in the test's own directory.
// ---------------------------------------------------------------------------

#[test]
fn argument_refusals_come_before_any_terraform_or_helm_call() {
    let outside = std::env::temp_dir().join("ae_infra_never_created");
    let outside = outside.display().to_string();
    let cases: Vec<(&str, Vec<Value>, &str)> = vec![
        // Options are refused: a directory, variable file, workspace or name is never a flag.
        ("terraform_init", vec![s("-upgrade")], "E_BAD_ARG"),
        ("terraform_init", vec![s("")], "E_BAD_ARG"),
        ("terraform_init", vec![Value::Int(1)], "E_BAD_ARG"),
        ("terraform_init", vec![s("a"), s("b")], "E_BAD_ARG"),
        ("terraform_plan", vec![s("-var-file=x")], "E_BAD_ARG"),
        ("terraform_plan", vec![Value::Int(1)], "E_BAD_ARG"),
        ("terraform_plan", vec![s("a"), s("b")], "E_BAD_ARG"),
        ("terraform_apply", vec![s("-auto-approve")], "E_BAD_ARG"),
        ("terraform_apply", vec![Value::Int(1)], "E_BAD_ARG"),
        ("terraform_destroy", vec![s("-auto-approve")], "E_BAD_ARG"),
        ("terraform_destroy", vec![Value::Int(1)], "E_BAD_ARG"),
        ("terraform_destroy", vec![s("a"), s("b")], "E_BAD_ARG"),
        ("terraform_state", vec![s("-json")], "E_BAD_ARG"),
        ("terraform_output", vec![s("-json")], "E_BAD_ARG"),
        ("terraform_validate", vec![s("-json")], "E_BAD_ARG"),
        ("terraform_workspace", vec![s("-x")], "E_BAD_ARG"),
        ("terraform_workspace", vec![s("bad name")], "E_BAD_ARG"),
        ("terraform_workspace", vec![s("")], "E_BAD_ARG"),
        ("terraform_workspace", vec![Value::Int(1)], "E_BAD_ARG"),
        ("terraform_workspace", vec![s("a"), s("b")], "E_BAD_ARG"),
        // Directories outside the workspace are refused, relative traversal included.
        ("terraform_init", vec![s(&outside)], "E_OUTSIDE_WORKSPACE"),
        (
            "terraform_init",
            vec![s("../escape")],
            "E_OUTSIDE_WORKSPACE",
        ),
        (
            "terraform_destroy",
            vec![s(&outside)],
            "E_OUTSIDE_WORKSPACE",
        ),
        ("terraform_state", vec![s(&outside)], "E_OUTSIDE_WORKSPACE"),
        ("terraform_output", vec![s(&outside)], "E_OUTSIDE_WORKSPACE"),
        (
            "terraform_validate",
            vec![s(&outside)],
            "E_OUTSIDE_WORKSPACE",
        ),
        (
            "terraform_plan",
            vec![s("../outside.tfvars")],
            "E_OUTSIDE_WORKSPACE",
        ),
        ("terraform_apply", vec![s(&outside)], "E_OUTSIDE_WORKSPACE"),
        // A directory that is not there is a coded not-found, not a missing tool.
        (
            "terraform_state",
            vec![s("ae-no-such-directory-xyz")],
            "E_NOT_FOUND",
        ),
        // Helm: option-like names, bad shapes and non-scalar values are refused.
        ("helm_list", vec![Value::Int(1)], "E_BAD_ARG"),
        ("helm_list", vec![s("-A")], "E_BAD_ARG"),
        ("helm_list", vec![s("a"), s("b")], "E_BAD_ARG"),
        ("helm_install", vec![s("-x"), s("nginx")], "E_BAD_ARG"),
        ("helm_install", vec![s("web")], "E_BAD_ARG"),
        ("helm_install", vec![s("web"), s("-n")], "E_BAD_ARG"),
        (
            "helm_install",
            vec![s("web"), s("nginx"), s("-A")],
            "E_BAD_ARG",
        ),
        (
            "helm_install",
            vec![s("web"), s("nginx"), Value::Int(1)],
            "E_BAD_ARG",
        ),
        (
            "helm_install",
            vec![s("web"), s("nginx"), s("default"), s("x")],
            "E_BAD_ARG",
        ),
        (
            "helm_install",
            vec![s("web"), s("nginx"), s("default"), Value::Array(vec![])],
            "E_BAD_ARG",
        ),
        (
            "helm_install",
            vec![
                s("web"),
                s("nginx"),
                s("default"),
                Value::Record(record(&[("a", Value::Array(vec![Value::Int(1)]))])),
            ],
            "E_BAD_ARG",
        ),
        (
            "helm_install",
            vec![
                s("web"),
                s("nginx"),
                s("default"),
                Value::Record(record(&[("a", Value::Null)])),
            ],
            "E_BAD_ARG",
        ),
        (
            "helm_install",
            vec![
                s("web"),
                s("nginx"),
                s("default"),
                Value::Record(record(&[("-x", Value::Int(1))])),
            ],
            "E_BAD_ARG",
        ),
        (
            "helm_install",
            vec![
                s("web"),
                s("nginx"),
                s("default"),
                Value::Record(record(&[("a b", Value::Int(1))])),
            ],
            "E_BAD_ARG",
        ),
        (
            "helm_install",
            vec![
                s("web"),
                s("nginx"),
                s("default"),
                Value::Record(record(&[("a", s("line\nbreak"))])),
            ],
            "E_BAD_ARG",
        ),
        (
            "helm_install",
            vec![
                s("web"),
                s("nginx"),
                s("default"),
                Value::Record(BTreeMap::new()),
                s("extra"),
            ],
            "E_BAD_ARG",
        ),
        ("helm_upgrade", vec![s("web"), s("--set")], "E_BAD_ARG"),
        (
            "helm_upgrade",
            vec![
                s("web"),
                s("nginx"),
                s("default"),
                Value::Record(record(&[("a", Value::Record(BTreeMap::new()))])),
            ],
            "E_BAD_ARG",
        ),
        ("helm_uninstall", vec![s("-x")], "E_BAD_ARG"),
        ("helm_uninstall", vec![s("web"), s("-A")], "E_BAD_ARG"),
        (
            "helm_uninstall",
            vec![s("web"), s("default"), s("extra")],
            "E_BAD_ARG",
        ),
        ("helm_repos", vec![s("x")], "E_BAD_ARG"),
        ("helm_search", vec![], "E_BAD_ARG"),
        ("helm_search", vec![s("-x")], "E_BAD_ARG"),
        ("helm_search", vec![s("a"), s("b")], "E_BAD_ARG"),
        ("helm_status", vec![], "E_BAD_ARG"),
        ("helm_status", vec![s("--kubeconfig=/x")], "E_BAD_ARG"),
        ("helm_status", vec![s("web"), s("-A")], "E_BAD_ARG"),
    ];
    for (name, args, code) in cases {
        let msg = refused(name, args.clone());
        assert!(
            msg.contains(code),
            "{name} with {args:?} must refuse with {code} before any tool runs: {msg}"
        );
    }
}

fn record(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[test]
fn piped_input_is_refused_by_every_builtin_in_the_tranche() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("terraform_init", vec![]),
        ("terraform_plan", vec![]),
        ("terraform_apply", vec![]),
        ("terraform_destroy", vec![]),
        ("terraform_state", vec![]),
        ("terraform_output", vec![]),
        ("terraform_workspace", vec![]),
        ("terraform_validate", vec![]),
        ("helm_list", vec![]),
        ("helm_install", vec![s("web"), s("nginx")]),
        ("helm_upgrade", vec![s("web"), s("nginx")]),
        ("helm_uninstall", vec![s("web")]),
        ("helm_repos", vec![]),
        ("helm_search", vec![s("nginx")]),
        ("helm_status", vec![s("web")]),
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

// ---------------------------------------------------------------------------
// Through the `ish` binary: terraform in a temporary workspace only.
// ---------------------------------------------------------------------------

#[test]
fn terraform_round_trip_runs_only_in_the_temporary_workspace() {
    let fx = Fixture::new("tf_round");
    if !terraform_present() {
        let run = ae(&fx, None, "terraform_init()");
        assert!(
            !run.success && run.output.contains("E_TOOL_MISSING"),
            "{}",
            run.output
        );
        return;
    }
    let steps: Vec<(&str, &str)> = vec![
        ("terraform_init()", "successfully initialized"),
        ("terraform_validate()", "valid"),
        ("terraform_plan()", "Changes to Outputs"),
        ("terraform_apply()", "x = 1"),
        ("terraform_output()", "value: 1"),
        ("terraform_state()", "[]"),
        ("terraform_workspace()", "default"),
        ("terraform_destroy()", "Destroy complete"),
    ];
    for (expr, want) in steps {
        let run = ae(&fx, None, expr);
        assert!(
            run.success,
            "{expr} must succeed in the workspace: {}",
            run.output
        );
        assert!(
            run.output.contains(want),
            "{expr} must mention {want:?}: {}",
            run.output
        );
    }
}

#[test]
fn terraform_directories_must_lie_inside_the_workspace() {
    let fx = Fixture::new("tf_outside");
    let outside = fx.outside();
    let outside_lit = path_literal(&outside);
    let vars = format!("{outside_lit}/vars.tfvars");
    let calls = [
        format!("terraform_init(\"{outside_lit}\")"),
        format!("terraform_destroy(\"{outside_lit}\")"),
        format!("terraform_state(\"{outside_lit}\")"),
        format!("terraform_output(\"{outside_lit}\")"),
        format!("terraform_validate(\"{outside_lit}\")"),
        format!("terraform_plan(\"{vars}\")"),
        format!("terraform_apply(\"{vars}\")"),
    ];
    for expr in calls {
        let run = ae(&fx, None, &expr);
        assert!(
            !run.success && run.output.contains("E_OUTSIDE_WORKSPACE"),
            "{expr} must be refused by the workspace guard: {}",
            run.output
        );
    }
    // Had any of them run, terraform would have left state in the outside directory.
    assert!(
        !outside.join("terraform.tfstate").exists() && !outside.join(".terraform").exists(),
        "a terraform run reached the directory outside the workspace"
    );
    assert!(!outside.join("vars.tfvars").exists());
}

#[test]
fn terraform_var_files_are_read_only_from_inside_the_workspace() {
    let fx = Fixture::new("tf_vars");
    std::fs::write(fx.workspace.join("vars.tfvars"), "# no variables\n")
        .expect("write variable file");
    // A variable file that is not there is a coded not-found, before terraform runs.
    let run = ae(&fx, None, "terraform_plan(\"missing.tfvars\")");
    assert!(
        !run.success && run.output.contains("E_NOT_FOUND"),
        "{}",
        run.output
    );
    if !terraform_present() {
        let run = ae(&fx, None, "terraform_plan(\"vars.tfvars\")");
        assert!(run.output.contains("E_TOOL_MISSING"), "{}", run.output);
        return;
    }
    let run = ae(&fx, None, "terraform_init()");
    assert!(run.success, "{}", run.output);
    let run = ae(&fx, None, "terraform_plan(\"vars.tfvars\")");
    assert!(run.success, "{}", run.output);
    assert!(run.output.contains("Changes to Outputs"), "{}", run.output);
}

#[test]
fn terraform_missing_directory_is_not_found_not_a_missing_tool() {
    let fx = Fixture::new("tf_missing_dir");
    let run = ae(&fx, None, "terraform_state(\"no-such-directory\")");
    assert!(
        !run.success && run.output.contains("E_NOT_FOUND"),
        "{}",
        run.output
    );
    assert!(!run.output.contains("E_TOOL_MISSING"), "{}", run.output);
}

#[test]
fn terraform_changes_need_approval_in_agent_mode_before_terraform_runs() {
    let fx = Fixture::new("tf_approval");
    if terraform_present() {
        // Give the workspace state so a run that slipped past the gate would show.
        let run = ae(&fx, None, "terraform_apply()");
        assert!(run.success, "{}", run.output);
    }
    for expr in ["terraform_destroy()", "terraform_apply()"] {
        let run = ae(&fx, Some("agent"), expr);
        assert!(
            !run.success && run.output.contains("E_NEEDS_APPROVAL"),
            "{expr} must need approval in agent mode: {}",
            run.output
        );
        assert!(
            !run.output.contains("Destroy complete") && !run.output.contains("Apply complete"),
            "{expr} ran before approval: {}",
            run.output
        );
    }
    if terraform_present() {
        assert!(
            fx.workspace.join("terraform.tfstate").exists(),
            "the agent-mode destroy removed the state it was refused"
        );
    }
}

#[test]
fn terraform_failures_are_coded_with_the_tools_own_words() {
    let fx = Fixture::new("tf_fail");
    if !terraform_present() {
        let run = ae(&fx, None, "terraform_state()");
        assert!(run.output.contains("E_TOOL_MISSING"), "{}", run.output);
        return;
    }
    // No state has been written yet, so `state list` fails.
    let run = ae(&fx, None, "terraform_state()");
    assert!(
        !run.success && run.output.contains("E_TOOL_FAILED"),
        "{}",
        run.output
    );
    assert!(run.output.contains("No state file"), "{}", run.output);

    let run = ae(&fx, None, "terraform_workspace(\"ae-no-such-workspace\")");
    assert!(
        !run.success && run.output.contains("E_TOOL_FAILED"),
        "{}",
        run.output
    );

    // An invalid configuration is an answer: validate returns the diagnostics.
    std::fs::write(
        fx.workspace.join("bad.tf"),
        "resource \"x\" \"y\" {\n  broken = \n}\n",
    )
    .expect("write invalid configuration");
    let run = ae(&fx, None, "terraform_validate()");
    assert!(run.success, "{}", run.output);
    assert!(run.output.contains("valid"), "{}", run.output);
    assert!(run.output.contains("false"), "{}", run.output);
}

// ---------------------------------------------------------------------------
// Through the `ish` binary: helm against the dead server only.
// ---------------------------------------------------------------------------

#[test]
fn helm_cluster_calls_fail_coded_against_the_dead_server() {
    let fx = Fixture::new("helm_dead");
    let present = helm_present();
    for expr in [
        "helm_list()",
        "helm_list(\"default\")",
        "helm_status(\"web\")",
        "helm_status(\"web\", \"default\")",
        "helm_install(\"web\", \"ae-no-such-chart\")",
        "helm_upgrade(\"web\", \"ae-no-such-chart\")",
        "helm_uninstall(\"web\")",
    ] {
        let run = ae(&fx, None, expr);
        if !present {
            assert!(
                !run.success && run.output.contains("E_TOOL_MISSING"),
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
fn helm_repos_is_empty_without_repositories_and_search_stays_local() {
    let fx = Fixture::new("helm_local");
    if !helm_present() {
        let run = ae(&fx, None, "helm_repos()");
        assert!(run.output.contains("E_TOOL_MISSING"), "{}", run.output);
        return;
    }
    // A fresh helm home has no repository file: the list is empty, not a failure.
    let run = ae(&fx, None, "helm_repos()");
    assert!(run.success, "{}", run.output);
    assert!(run.output.contains("[]"), "{}", run.output);

    // The search reads only local indexes, so with none it succeeds or fails coded.
    let run = ae(&fx, None, "helm_search(\"nginx\")");
    assert!(
        run.success || run.output.contains("E_TOOL_FAILED"),
        "{}",
        run.output
    );
    assert!(!run.output.contains(DEAD_SERVER), "{}", run.output);
}

#[test]
fn helm_changes_need_approval_in_agent_mode_before_helm_runs() {
    let fx = Fixture::new("helm_approval");
    for expr in [
        "helm_install(\"web\", \"nginx\")",
        "helm_uninstall(\"web\")",
    ] {
        let run = ae(&fx, Some("agent"), expr);
        assert!(
            !run.success && run.output.contains("E_NEEDS_APPROVAL"),
            "{expr} must need approval in agent mode: {}",
            run.output
        );
        assert!(
            !run.output.contains(DEAD_SERVER),
            "{expr} reached helm before approval: {}",
            run.output
        );
    }
}

#[test]
fn helm_option_like_names_never_reach_helm() {
    let fx = Fixture::new("helm_options");
    for expr in [
        "helm_status(\"--kubeconfig=/etc/passwd\")",
        "helm_uninstall(\"--all\")",
        "helm_install(\"web\", \"--repo=https://example.invalid\")",
        "helm_search(\"--version\")",
        "helm_list(\"--all-namespaces\")",
    ] {
        let run = ae(&fx, None, expr);
        assert!(
            !run.success && run.output.contains("E_BAD_ARG"),
            "{expr} must be refused before helm: {}",
            run.output
        );
    }
}
