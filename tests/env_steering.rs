//! Agent mode may not steer a child program, its loader, its configuration or
//! the destination of its data through the process environment (AS-2026-16,
//! -21, -22).
//!
//! Process-global state (`IRONSHELL_MODE`, the environment itself) is shared by
//! every test in this binary, so each test takes `LOCK` and restores what it
//! changed through a guard, including when an assertion fails.

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

/// Sets one variable (or removes it) and puts back the previous value on drop.
struct Set {
    name: String,
    prev: Option<OsString>,
}

impl Set {
    fn to(name: &str, value: Option<&str>) -> Self {
        let prev = std::env::var_os(name);
        match value {
            Some(v) => std::env::set_var(name, v),
            None => std::env::remove_var(name),
        }
        Set {
            name: name.to_string(),
            prev,
        }
    }

    /// Changes nothing now; restores whatever the variable holds on drop. Used
    /// around a builtin that is expected to change the variable, so a failure
    /// cannot leave the test process's `PATH` (or any other name) altered.
    fn restoring(name: &str) -> Self {
        Set {
            name: name.to_string(),
            prev: std::env::var_os(name),
        }
    }
}

impl Drop for Set {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(&self.name, v),
            None => std::env::remove_var(&self.name),
        }
    }
}

/// A fresh scratch directory, used as the session and workspace root so no test
/// writes into the repository or the user's own state.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ae_env_steer_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// The names the agent must not change, in mixed case, as Windows would see them.
const STEERING: &[&str] = &[
    "PATH",
    "GIT_CONFIG_GLOBAL",
    "IRONGATE_URL",
    "http_proxy",
    "LD_PRELOAD",
];

#[test]
fn agent_mode_refuses_steering_names_through_every_setter() {
    let _g = lock();
    let dir = scratch("refuse");
    let _mode = Set::to("IRONSHELL_MODE", Some("agent"));
    let _session = Set::to("IRONSHELL_SESSION_DIR", dir.join("state").to_str());
    for name in STEERING {
        let _restore = Set::restoring(name);
        let before = std::env::var_os(name);
        let attempts = [
            ("env_set", vec![s(name), s("/ae_steer_probe")]),
            ("set_env", vec![s(name), s("/ae_steer_probe")]),
            ("env_unset", vec![s(name)]),
        ];
        for (builtin, args) in attempts {
            let err = call(builtin, args)
                .expect_err(&format!("{builtin}({name}) must be refused in agent mode"));
            assert!(
                err.to_string().contains("E_POLICY_DENY"),
                "{builtin}({name}) was refused, but not as a policy denial: {err}"
            );
            assert_eq!(
                std::env::var_os(name),
                before,
                "{builtin}({name}) changed the variable despite the refusal"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn agent_mode_still_sets_and_unsets_ordinary_names() {
    let _g = lock();
    let dir = scratch("ordinary");
    let _mode = Set::to("IRONSHELL_MODE", Some("agent"));
    let _session = Set::to("IRONSHELL_SESSION_DIR", dir.join("state").to_str());
    let _restore = Set::to("MY_VAR", None);

    assert_eq!(
        call("env_set", vec![s("MY_VAR"), s("hello")]).expect("env_set MY_VAR"),
        Value::Bool(true)
    );
    assert_eq!(std::env::var("MY_VAR").as_deref(), Ok("hello"));

    call("set_env", vec![s("MY_VAR"), s("again")]).expect("set_env MY_VAR");
    assert_eq!(std::env::var("MY_VAR").as_deref(), Ok("again"));

    assert_eq!(
        call("env_unset", vec![s("MY_VAR")]).expect("env_unset MY_VAR"),
        Value::Bool(true)
    );
    assert!(std::env::var_os("MY_VAR").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn human_mode_sets_steering_names_as_before() {
    let _g = lock();
    let _mode = Set::to("IRONSHELL_MODE", None);
    for name in STEERING {
        let _restore = Set::restoring(name);
        assert_eq!(
            call("env_set", vec![s(name), s("/ae_human_value")]).expect("env_set"),
            Value::Bool(true),
            "{name}: a human may still set it"
        );
        assert_eq!(std::env::var(name).as_deref(), Ok("/ae_human_value"));
        call("env_unset", vec![s(name)]).expect("env_unset");
        assert!(std::env::var_os(name).is_none(), "{name} was not unset");
    }
}

#[test]
fn proc_env_of_this_process_redacts_secret_names_in_agent_mode() {
    let _g = lock();
    let name = "AE_STEER_PROBE_API_KEY";
    let secret = "sk-steer-probe-value-0123456789";
    let _var = Set::to(name, Some(secret));
    let _redact = Set::to("IRONSHELL_REDACT", None);
    let _secrets = Set::to("IRONSHELL_SECRETS", None);
    let own = Value::Int(i64::from(std::process::id()));

    {
        let _mode = Set::to("IRONSHELL_MODE", Some("agent"));
        let rec = match call("proc_env", vec![own.clone()]).expect("proc_env(own pid)") {
            Value::Record(m) => m,
            other => panic!("proc_env answered {other:?}, not a record"),
        };
        assert_eq!(
            rec.get(name),
            Some(&s(&format!("[REDACTED:{name}]"))),
            "a secret-named variable of the own process must be redacted in agent mode"
        );
        assert!(
            rec.values().all(|v| v != &s(secret)),
            "the secret value appeared in proc_env's answer"
        );
    }

    {
        let _mode = Set::to("IRONSHELL_MODE", None);
        let rec = match call("proc_env", vec![own]).expect("proc_env(own pid)") {
            Value::Record(m) => m,
            other => panic!("proc_env answered {other:?}, not a record"),
        };
        assert_eq!(rec.get(name), Some(&s(secret)), "human mode is unchanged");
    }
}

#[test]
fn an_ai_call_names_its_gateway_in_the_approval() {
    let _g = lock();
    let dir = scratch("ai_dest");
    let _mode = Set::to("IRONSHELL_MODE", Some("agent"));
    let _session = Set::to("IRONSHELL_SESSION_DIR", dir.join("state").to_str());
    let _net = Set::to("IRONSHELL_NET_ALLOW", None);
    let _approve = Set::to("IRONSHELL_APPROVE", None);
    let _approve_all = Set::to("IRONSHELL_APPROVE_ALL", None);
    let _router = Set::to("IRONSHELL_AI_ROUTER", None);
    let _provider = Set::to("IRONSHELL_AI", None);
    let _gate = Set::to("IRONGATE_URL", Some("http://gate.example:7700/v1"));

    // Agent mode with no egress allowlist asks before any model call.
    let err = iron_shell::safety::guard_dispatch("ai", &[s("hello")])
        .expect_err("ai needs approval in agent mode");
    let approval = err.approval.expect("the refusal carries the descriptor");
    assert_eq!(
        approval.blast_radius["destination"],
        serde_json::json!("http://gate.example:7700/v1"),
        "the descriptor (and so the token) must name the gateway the key goes to"
    );

    // Naming another provider takes the call elsewhere, so no gateway is named.
    let _other = Set::to("IRONSHELL_AI", Some("openai"));
    assert_eq!(iron_shell::ai::ai_gate_destination(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_shell_assignment_or_export_of_a_steering_name_is_refused_in_agent_mode() {
    let _g = lock();
    let dir = scratch("posix_agent");
    let _mode = Set::to("IRONSHELL_MODE", Some("agent"));
    let _session = Set::to("IRONSHELL_SESSION_DIR", dir.join("state").to_str());
    let _workspace = Set::to("IRONSHELL_WORKSPACE", dir.to_str());
    let _path = Set::restoring("PATH");
    let _gitcfg = Set::restoring("GIT_CONFIG_GLOBAL");
    let before = std::env::var_os("PATH");
    let scripts = [
        "export PATH=/ae_steer_probe",
        "PATH=/ae_steer_probe",
        "PATH=/ae_steer_probe ae_no_such_program_xyz",
        "export GIT_CONFIG_GLOBAL=/ae_steer_probe",
        "read PATH",
        "for PATH in a; do :; done",
        "GIT_CONFIG_GLOBAL=/ae_steer_probe ae_no_such_program_xyz",
    ];
    for script in scripts {
        let code = iron_shell::posix::run(script);
        assert_ne!(code, Some(0), "`{script}` was not refused (exit {code:?})");
    }
    assert_eq!(
        std::env::var_os("PATH"),
        before,
        "a refused script changed PATH"
    );
    assert!(std::env::var_os("GIT_CONFIG_GLOBAL").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Runs `ish -b -c <script>` in `dir`, with the agent mode set (or cleared) for
/// that child only. Returns (stdout, stderr, exit code).
fn ish_bash(mode: Option<&str>, script: &str, dir: &Path) -> (String, String, i32) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ish"));
    cmd.args(["-b", "-c", script])
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .env("IRONSHELL_SESSION_DIR", dir.join("state"))
        .env("IRONSHELL_WORKSPACE", dir);
    match mode {
        Some(m) => cmd.env("IRONSHELL_MODE", m),
        None => cmd.env_remove("IRONSHELL_MODE"),
    };
    let out = cmd.output().expect("run ish");
    (
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn ish_bash_export_of_path_is_refused_in_agent_mode() {
    let _g = lock();
    let dir = scratch("ish_bash");
    let script = r#"export PATH=/ae_steer_probe; printf "%s" "$PATH""#;

    let (out, err, _) = ish_bash(Some("agent"), script, &dir);
    assert!(
        !out.contains("ae_steer_probe"),
        "agent mode let `export PATH` take effect: {out:?}"
    );
    assert!(
        err.contains("PATH") && err.contains("may not"),
        "the refusal was not reported: {err:?}"
    );

    // Control: the same script changes PATH for a human, so the refusal above
    // is the agent rule and not a broken script.
    let (out, _, _) = ish_bash(None, script, &dir);
    assert_eq!(out, "/ae_steer_probe", "human mode export did not apply");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Git runs a repository's own `textconv` driver when it prints a diff, unless
/// `--no-textconv` is given. The repository here is a scratch one; its driver
/// writes a marker file, which must not appear.
#[test]
fn git_patch_builtins_do_not_run_a_repository_textconv_driver() {
    let _g = lock();
    let dir = scratch("textconv");
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).expect("repo dir");
    let marker = dir.join("textconv_ran.marker");
    let marker_sh = marker.to_string_lossy().replace('\\', "/");

    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["-c", "user.name=ae", "-c", "user.email=ae@example.invalid"])
            .args(args)
            .output()
            .expect("git must be installed to run this test");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&["init", "-q"]);
    std::fs::write(repo.join("file.txt"), "one\n").expect("write");
    std::fs::write(repo.join(".gitattributes"), "*.txt diff=evil\n").expect("attrs");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "first"]);
    git(&[
        "config",
        "diff.evil.textconv",
        &format!("sh -c 'echo ran > {marker_sh}'"),
    ]);
    std::fs::write(repo.join("file.txt"), "two\n").expect("write");
    git(&["commit", "-q", "-am", "second"]);
    std::fs::write(repo.join("file.txt"), "three\n").expect("write");

    let _mode = Set::to("IRONSHELL_MODE", Some("agent"));
    let _session = Set::to("IRONSHELL_SESSION_DIR", dir.join("state").to_str());
    let _workspace = Set::to("IRONSHELL_WORKSPACE", repo.to_str());
    // The git builtins act on the process's working directory, so enter the
    // scratch repository for the calls and return on drop.
    struct Cwd(PathBuf);
    impl Drop for Cwd {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }
    let _cwd = Cwd(std::env::current_dir().expect("cwd"));
    std::env::set_current_dir(&repo).expect("enter scratch repo");

    call("git_diff", vec![]).expect("git_diff");
    assert!(
        !marker.exists(),
        "git_diff ran the repository's textconv driver"
    );

    call("git_show", vec![]).expect("git_show");
    assert!(
        !marker.exists(),
        "git_show ran the repository's textconv driver"
    );
    drop(_cwd);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn posix_export_of_a_steering_name_applies_in_human_mode() {
    let _g = lock();
    let dir = scratch("posix_human");
    let _mode = Set::to("IRONSHELL_MODE", None);
    let _session = Set::to("IRONSHELL_SESSION_DIR", dir.join("state").to_str());
    let _path = Set::restoring("PATH");
    // The same assignment that is refused in agent mode runs in human mode.
    assert_eq!(iron_shell::posix::run("export PATH=$PATH"), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}
