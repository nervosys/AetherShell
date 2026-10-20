//! Contracts for the ansible_*, cloud_*, act, buildah, skopeo, trivy, rsync,
//! rustup_update, pipx_install, pkg_changelog, gh, glab and exec_remote builtins.
//!
//! No test here may reach a real cloud, registry, GitHub, GitLab or remote host,
//! install a package, or update a toolchain.
//!
//! - Refusals are argument checks that return before any process starts. They
//!   run in-process, and each one is asserted to be a refusal, not a call.
//! - A call that would start one of these tools runs in a child `ae` process
//!   whose PATH is an empty directory. The tool is then not found, so the call
//!   must fail as E_TOOL_MISSING. Nothing is started, and nothing is contacted.
//! - Cloud and remote-execution calls with valid arguments must answer
//!   E_UNIMPLEMENTED: no provider or transport is wired in, so nothing is done.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use aethershell::builtins::{call_with_input, is_dispatched};
use aethershell::env::Env;
use aethershell::signature::signature_of;
use aethershell::value::Value;

const DEADLINE: Duration = Duration::from_secs(30);

/// Every builtin this tranche declares, with the spellings it answers to.
const TRANCHE: &[(&str, &str)] = &[
    ("ansible_galaxy", "ansible_galaxy"),
    ("ansible_inventory", "ansible_inventory"),
    ("ansible_playbook", "ansible_playbook"),
    ("ansible_vault", "ansible_vault"),
    ("cloud_deploy", "cloud_deploy"),
    ("cloud_instance_connect", "cloud_instance_connect"),
    ("cloud_instance_create", "cloud_instance_create"),
    ("cloud_instance_destroy", "cloud_instance_destroy"),
    ("cloud_instance_status", "cloud_instance_status"),
    ("act_run", "act"),
    ("buildah_build", "buildah_build"),
    ("skopeo_copy", "skopeo_copy"),
    ("skopeo_inspect", "skopeo_inspect"),
    ("trivy_image", "trivy_image"),
    ("trivy_scan", "trivy_scan"),
    ("rsync_sync", "rsync"),
    ("rustup_update", "rustup_update"),
    ("pipx_install", "pipx_install"),
    ("pkg_changelog", "pkg_changelog"),
    ("gh_issue", "gh_issue"),
    ("gh_pr", "gh_pr"),
    ("gh_repo_cmd", "gh_repo"),
    ("glab_issue", "glab_issue"),
    ("glab_mr", "glab_mr"),
    ("remote_exec", "exec_remote"),
];

/// The workspace every test in this process works in. It is fresh, holds only
/// the fixtures written here, and is the process directory.
fn workspace() -> PathBuf {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("ae_ops_tranche_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create workspace");
        std::fs::write(
            root.join("site.yml"),
            "- hosts: localhost
",
        )
        .expect("write playbook");
        std::fs::write(
            root.join("inv.ini"),
            "[all]
localhost
",
        )
        .expect("write inventory");
        std::fs::write(
            root.join("vault.yml"),
            "$ANSIBLE_VAULT;1.1;AES256
",
        )
        .expect("write vault");
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(
            root.join("payload.txt"),
            "payload
",
        )
        .expect("write payload");
        std::env::set_var("AETHER_SESSION_DIR", root.join("session"));
        std::env::set_var("AETHER_WORKSPACE", &root);
        for var in ["AETHER_MODE", "AETHER_AGENT", "AETHER_POLICY"] {
            std::env::remove_var(var);
        }
        std::env::set_current_dir(&root).expect("enter workspace");
        root
    })
    .clone()
}

fn env() -> Env {
    workspace();
    Env::new()
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn record(pairs: &[(&str, Value)]) -> Value {
    let mut map = BTreeMap::new();
    for (key, value) in pairs {
        map.insert((*key).to_string(), value.clone());
    }
    Value::Record(map)
}

/// The error a call produced. A call that answers is a bug in this file, because
/// every call the refusal tests make must stop before a process starts.
fn refusal(name: &str, args: Vec<Value>) -> String {
    match call_with_input(name, args, None, &mut env()) {
        Ok(value) => panic!("{name} answered {value:?} for a call it must refuse"),
        Err(e) => e.to_string(),
    }
}

fn assert_refused(name: &str, args: Vec<Value>, code: &str) {
    let message = refusal(name, args.clone());
    assert!(
        message.contains(code),
        "{name} {args:?} was not refused as {code}: {message}"
    );
}

// ---------------------------------------------------------------------------
// Declarations and dispatch
// ---------------------------------------------------------------------------

#[test]
fn every_tranche_builtin_is_dispatched_and_declared() {
    for (canonical, spelling) in TRANCHE {
        assert!(is_dispatched(canonical), "{canonical} is not dispatched");
        assert!(is_dispatched(spelling), "{spelling} is not dispatched");
        let sig =
            signature_of(canonical).unwrap_or_else(|| panic!("{canonical} has no declaration"));
        assert_eq!(sig.name, *canonical);
        if spelling != canonical {
            assert!(
                sig.aliases.contains(spelling),
                "{spelling} is not recorded as an alias of {canonical}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Option-like operands and pipeline input
// ---------------------------------------------------------------------------

#[test]
fn an_option_like_operand_is_refused_before_any_process_starts() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("ansible_playbook", vec![s("-x")]),
        ("ansible_inventory", vec![s("-x")]),
        ("ansible_galaxy", vec![s("-x")]),
        ("ansible_vault", vec![s("view"), s("-x")]),
        ("ansible_vault", vec![s("-x"), s("vault.yml")]),
        ("cloud_deploy", vec![s("-x")]),
        ("cloud_instance_create", vec![s("-x")]),
        ("cloud_instance_destroy", vec![s("-x")]),
        ("cloud_instance_status", vec![s("-x")]),
        ("cloud_instance_connect", vec![s("-x")]),
        ("act_run", vec![s("-x")]),
        ("buildah_build", vec![s("-x")]),
        ("skopeo_copy", vec![s("-x"), s("docker://a")]),
        ("skopeo_inspect", vec![s("-x")]),
        ("trivy_scan", vec![s("-x")]),
        ("trivy_image", vec![s("-x")]),
        ("rsync_sync", vec![s("-x"), s("y")]),
        ("rsync_sync", vec![s("a"), s("-x")]),
        ("pipx_install", vec![s("-x")]),
        ("pkg_changelog", vec![s("-x")]),
        ("gh_pr", vec![s("-x")]),
        ("gh_issue", vec![s("-x")]),
        ("gh_repo_cmd", vec![s("-x")]),
        ("glab_mr", vec![s("-x")]),
        ("glab_issue", vec![s("-x")]),
        ("remote_exec", vec![s("-x"), s("ls")]),
    ];
    for (name, args) in cases {
        assert_refused(name, args, "E_BAD_ARG");
    }
}

#[test]
fn a_piped_subject_is_refused_by_every_tranche_builtin() {
    for (canonical, _) in TRANCHE {
        if *canonical == "remote_exec" {
            continue; // accepts a piped Record; a piped String is refused below
        }
        let message = match call_with_input(canonical, vec![], Some(s("x")), &mut env()) {
            Ok(value) => panic!("{canonical} accepted a piped subject: {value:?}"),
            Err(e) => e.to_string(),
        };
        assert!(
            message.contains("E_BAD_ARG"),
            "{canonical} did not refuse a piped subject as E_BAD_ARG: {message}"
        );
    }
    let message = match call_with_input("remote_exec", vec![], Some(s("x")), &mut env()) {
        Ok(value) => panic!("remote_exec accepted a piped String: {value:?}"),
        Err(e) => e.to_string(),
    };
    assert!(message.contains("E_BAD_ARG"), "{message}");
}

// ---------------------------------------------------------------------------
// Surplus arguments and unknown options are refused, never dropped
// ---------------------------------------------------------------------------

#[test]
fn surplus_arguments_are_refused_not_dropped() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        (
            "ansible_playbook",
            vec![s("site.yml"), s("inv.ini"), record(&[]), s("x")],
        ),
        ("ansible_inventory", vec![s("inv.ini"), s("x")]),
        ("ansible_galaxy", vec![s("a"), s("b")]),
        ("ansible_vault", vec![s("view"), s("vault.yml"), s("x")]),
        ("cloud_deploy", vec![s("a"), record(&[]), s("x")]),
        ("cloud_instance_create", vec![s("a"), record(&[]), s("x")]),
        ("cloud_instance_destroy", vec![s("a"), s("b")]),
        ("cloud_instance_status", vec![s("a"), s("b")]),
        ("cloud_instance_connect", vec![s("a"), s("b")]),
        ("act_run", vec![s("push"), s("x")]),
        ("buildah_build", vec![s("app:1"), s("."), s("x")]),
        (
            "skopeo_copy",
            vec![s("docker://a"), s("docker://b"), s("x")],
        ),
        ("skopeo_inspect", vec![s("docker://a"), s("x")]),
        ("trivy_scan", vec![s("."), s("x")]),
        ("trivy_image", vec![s("alpine:3"), s("x")]),
        ("rsync_sync", vec![s("a"), s("b"), record(&[]), s("x")]),
        ("pipx_install", vec![s("black"), s("ruff")]),
        ("pkg_changelog", vec![s("bash"), s("x")]),
        ("rustup_update", vec![s("stable")]),
        ("remote_exec", vec![s("n1"), s("ls"), s("x")]),
    ];
    for (name, args) in cases {
        let message = refusal(name, args.clone());
        assert!(
            message.contains("E_BAD_ARG"),
            "{name} {args:?} did not refuse its surplus argument as E_BAD_ARG: {message}"
        );
    }
}

#[test]
fn unknown_or_mistyped_options_are_refused() {
    assert_refused(
        "cloud_deploy",
        vec![s("app"), record(&[("bogus", s("x"))])],
        "E_BAD_ARG",
    );
    assert_refused(
        "cloud_instance_create",
        vec![s("web"), record(&[("type", Value::Int(1))])],
        "E_BAD_ARG",
    );
    assert_refused(
        "rsync_sync",
        vec![s("a"), s("b"), record(&[("bogus", Value::Bool(true))])],
        "E_BAD_ARG",
    );
    assert_refused(
        "rsync_sync",
        vec![s("a"), s("b"), record(&[("archive", s("yes"))])],
        "E_BAD_ARG",
    );
    assert_refused(
        "rsync_sync",
        vec![s("a"), s("b"), record(&[("exclude", s("-e sh"))])],
        "E_BAD_ARG",
    );
    // A variable name must be an identifier, and a nested value is not turned
    // into text. The playbook and inventory exist, so the refusal is the
    // extra-vars check, which runs before any process starts.
    assert_refused(
        "ansible_playbook",
        vec![s("site.yml"), s("inv.ini"), record(&[("bad-key", s("x"))])],
        "E_BAD_ARG",
    );
    assert_refused(
        "ansible_playbook",
        vec![
            s("site.yml"),
            s("inv.ini"),
            record(&[("nested", Value::Array(vec![]))]),
        ],
        "E_BAD_ARG",
    );
}

// ---------------------------------------------------------------------------
// Paths and remote specs
// ---------------------------------------------------------------------------

#[test]
fn a_local_path_outside_the_workspace_is_refused() {
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("ansible_playbook", vec![s("../outside.yml")]),
        ("ansible_inventory", vec![s("../inv.ini")]),
        ("ansible_vault", vec![s("view"), s("../secret.yml")]),
        ("buildah_build", vec![s("app:1"), s("../elsewhere")]),
        ("skopeo_copy", vec![s("dir:../x"), s("docker://a")]),
        ("skopeo_inspect", vec![s("oci:../elsewhere")]),
        ("trivy_scan", vec![s("../x")]),
        ("rsync_sync", vec![s("../x"), s("payload.txt")]),
    ];
    for (name, args) in cases {
        assert_refused(name, args, "E_OUTSIDE_WORKSPACE");
    }
}

#[test]
fn a_url_is_not_a_local_path_and_a_remote_host_must_be_plain() {
    assert_refused(
        "buildah_build",
        vec![s("app:1"), s("https://example.invalid/r.git")],
        "E_BAD_ARG",
    );
    assert_refused(
        "rsync_sync",
        vec![s("rsync://example.invalid/mod"), s("payload.txt")],
        "E_BAD_ARG",
    );
    // A host that begins with '-' would be read by ssh as an option.
    assert_refused(
        "rsync_sync",
        vec![s("user@-oProxyCommand=x:/tmp"), s("payload.txt")],
        "E_BAD_ARG",
    );
    assert_refused("rsync_sync", vec![s("-e"), s("x")], "E_BAD_ARG");
    assert_refused(
        "skopeo_copy",
        vec![s("alpine"), s("docker://a")],
        "E_BAD_ARG",
    );
    assert_refused("skopeo_inspect", vec![s("docker://-x")], "E_BAD_ARG");
    assert_refused("skopeo_inspect", vec![s("docker://a b")], "E_BAD_ARG");
    assert_refused("trivy_image", vec![s("a b")], "E_BAD_ARG");
    assert_refused("ansible_galaxy", vec![s("file:./role")], "E_BAD_ARG");
    assert_refused("ansible_galaxy", vec![s("link:./role")], "E_BAD_ARG");
    assert_refused("pipx_install", vec![s("file:./pkg")], "E_BAD_ARG");
    // An inline host list is not an inventory that exists.
    assert_refused("ansible_inventory", vec![s("host1,")], "E_NOT_FOUND");
}

// ---------------------------------------------------------------------------
// Forge calls: verbs are allow-listed, and editor or browser flags are refused
// ---------------------------------------------------------------------------

#[test]
fn forge_verbs_outside_the_list_are_refused() {
    assert_refused("gh_pr", vec![s("merge")], "E_BAD_ARG");
    assert_refused("gh_pr", vec![s("review")], "E_BAD_ARG");
    assert_refused("gh_pr", vec![s("checkout")], "E_BAD_ARG");
    assert_refused("gh_issue", vec![s("delete")], "E_BAD_ARG");
    assert_refused("gh_repo_cmd", vec![s("create")], "E_BAD_ARG");
    assert_refused("gh_repo_cmd", vec![s("delete")], "E_BAD_ARG");
    assert_refused("glab_mr", vec![s("merge")], "E_BAD_ARG");
    assert_refused("glab_mr", vec![s("approve")], "E_BAD_ARG");
    assert_refused("glab_issue", vec![s("delete")], "E_BAD_ARG");
}

#[test]
fn an_editor_or_browser_flag_and_a_non_string_flag_are_refused() {
    assert_refused("gh_pr", vec![s("list"), s("--web")], "E_BAD_ARG");
    assert_refused("gh_issue", vec![s("create"), s("--editor")], "E_BAD_ARG");
    assert_refused("gh_issue", vec![s("create"), s("-e")], "E_BAD_ARG");
    assert_refused(
        "glab_mr",
        vec![s("list"), s("--browser=firefox")],
        "E_BAD_ARG",
    );
    assert_refused("glab_issue", vec![s("list"), Value::Int(5)], "E_BAD_ARG");
}

// ---------------------------------------------------------------------------
// Cloud and remote execution: nothing is fabricated
// ---------------------------------------------------------------------------

#[test]
fn a_valid_cloud_call_is_not_implemented_and_creates_nothing() {
    for (name, args) in [
        ("cloud_deploy", vec![s("app")]),
        (
            "cloud_instance_create",
            vec![s("web"), record(&[("region", s("us-east-1"))])],
        ),
        ("cloud_instance_status", vec![s("i-1")]),
        ("cloud_instance_connect", vec![s("i-1")]),
        ("cloud_instance_destroy", vec![s("i-1")]),
    ] {
        assert_refused(name, args, "E_UNIMPLEMENTED");
    }
    let listed = call_with_input("cloud_instances", vec![], None, &mut env())
        .expect("cloud_instances answers");
    assert_eq!(
        listed,
        Value::Array(vec![]),
        "a refused cloud call left an instance behind"
    );
}

#[test]
fn a_registered_node_is_refused_as_unimplemented_and_an_unknown_one_is_not_found() {
    let mut env = env();
    aethershell::builtins::call(
        "cluster_add_node",
        vec![s("ops-tranche-node"), s("10.0.0.9:8080")],
        &mut env,
    )
    .expect("register a node");
    let known = call_with_input(
        "remote_exec",
        vec![s("ops-tranche-node"), s("ls -la")],
        None,
        &mut env,
    )
    .expect_err("remote_exec must not answer for a registered node");
    assert!(known.to_string().contains("E_UNIMPLEMENTED"), "{known}");

    let unknown = call_with_input(
        "remote_exec",
        vec![s("ops-no-such-node"), s("ls")],
        None,
        &mut env,
    )
    .expect_err("an unknown node is an error");
    assert!(unknown.to_string().contains("E_NOT_FOUND"), "{unknown}");
}

#[test]
fn remote_exec_checks_its_command_before_the_node() {
    assert_refused("remote_exec", vec![s("n1"), s("")], "E_BAD_ARG");
    assert_refused("remote_exec", vec![s("n1"), s("   ")], "E_BAD_ARG");
    let oversized = "x".repeat(8193);
    assert_refused("remote_exec", vec![s("n1"), s(&oversized)], "E_BAD_ARG");
    assert_refused(
        "remote_exec",
        vec![record(&[
            ("node_id", s("n1")),
            ("command", s("ls")),
            ("extra", s("x")),
        ])],
        "E_BAD_ARG",
    );
}

// ---------------------------------------------------------------------------
// Real calls where the tool is absent: E_TOOL_MISSING, never a contact
// ---------------------------------------------------------------------------

/// Run `ae -c <expr>` in the workspace with an empty PATH. The expression's
/// stdout is returned, trimmed of quotes.
fn ae_with_empty_path(expr: &str) -> String {
    let root = workspace();
    let empty = root.join("empty-path");
    std::fs::create_dir_all(&empty).expect("create empty PATH directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_ae"))
        .current_dir(&root)
        .env("PATH", &empty)
        .env("AETHER_SESSION_DIR", root.join("session"))
        .env("AETHER_WORKSPACE", &root)
        .env_remove("AETHER_MODE")
        .env_remove("AETHER_AGENT")
        .env_remove("AETHER_POLICY")
        .args(["-c", expr])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ae");
    let mut out = child.stdout.take().expect("stdout");
    let mut err = child.stderr.take().expect("stderr");
    let out_h = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = std::io::Read::read_to_end(&mut out, &mut b);
        b
    });
    let err_h = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = std::io::Read::read_to_end(&mut err, &mut b);
        b
    });
    let started = Instant::now();
    loop {
        if child.try_wait().expect("poll ae").is_some() {
            break;
        }
        if started.elapsed() > DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            panic!("ae did not finish within 30 s: {expr}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let stdout = String::from_utf8_lossy(&out_h.join().unwrap_or_default()).into_owned();
    let _stderr = err_h.join().unwrap_or_default();
    stdout.trim().trim_matches('"').to_string()
}

#[test]
fn an_absent_tool_is_coded_as_tool_missing_for_every_external_call() {
    let calls: Vec<&str> = vec![
        r#"ansible_playbook("site.yml")"#,
        r#"ansible_inventory("inv.ini")"#,
        r#"ansible_galaxy("geerlingguy.docker")"#,
        r#"ansible_vault("view", "vault.yml")"#,
        r#"act_run("push")"#,
        r#"buildah_build("app:1")"#,
        r#"skopeo_copy("dir:src", "dir:src")"#,
        r#"skopeo_inspect("docker://example.invalid/app:1")"#,
        r#"trivy_image("alpine:3")"#,
        r#"trivy_scan("src")"#,
        r#"rsync_sync("payload.txt", "src")"#,
        r#"rustup_update()"#,
        r#"pipx_install("black")"#,
        r#"gh_pr("list")"#,
        r#"gh_issue("list")"#,
        r#"gh_repo("view")"#,
        r#"glab_mr("list")"#,
        r#"glab_issue("list")"#,
    ];
    for call in calls {
        let code = ae_with_empty_path(&format!(r#"(try {{ {call} }} catch e {{ e.error.code }})"#));
        assert_eq!(
            code, "E_TOOL_MISSING",
            "{call} answered {code:?} with no tool on the PATH; it must be a coded E_TOOL_MISSING"
        );
    }
}
