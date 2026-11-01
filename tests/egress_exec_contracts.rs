//! `Exec`-class builtins that reach the network through a program must obey the
//! egress allowlist (`IRONSHELL_NET_ALLOW`).
//!
//! The allowlist is enforced in `safety::guard` for `Effect::Network` builtins
//! only. `ssh_exec`, `docker_pull`, `npm_install` and the rest are classified
//! `Exec` (they run a program), so the same guard never looked at their
//! destinations. Each of them now calls `safety::check_egress` with the host it
//! names, or `safety::check_egress_unknown` when the destination is not in the
//! arguments. Both are no-ops without an allowlist.
//!
//! What these tests pin, for each builtin:
//! - an off-list destination is refused with `E_POLICY_DENY` and the egress
//!   detail, before any program runs;
//! - an on-list destination is not refused by this check;
//! - with no allowlist, this check refuses nothing.
//!
//! No test reaches a real host. The child's `PATH` is an empty directory, so a
//! call that gets past the check fails with "not found" instead of starting
//! `ssh` or `docker`. Human mode is used so the body's own check is what
//! answers, not the dispatcher's approval prompt (which runs first in agent mode).

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::safety::{ErrorCode, SafetyError};
use iron_shell::value::Value;
use std::ffi::OsString;
use std::sync::Mutex;

/// The environment variables these tests set are process-global.
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sets the allowlist and an empty `PATH` for one test, and restores both on drop.
struct Egress {
    saved: Vec<(&'static str, Option<OsString>)>,
    empty_dir: std::path::PathBuf,
    audit_log: std::path::PathBuf,
}

impl Egress {
    fn new(tag: &str, allow: Option<&str>) -> Self {
        let names = [
            "IRONSHELL_NET_ALLOW",
            "IRONSHELL_MODE",
            "IRONSHELL_AGENT",
            "IRONSHELL_POLICY",
            "PATH",
            "IRONSHELL_AUDIT_LOG",
        ];
        let saved = names
            .iter()
            .map(|n| (*n, std::env::var_os(n)))
            .collect::<Vec<_>>();
        let empty_dir =
            std::env::temp_dir().join(format!("ae_egress_nopath_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&empty_dir).expect("empty PATH dir");
        for n in ["IRONSHELL_MODE", "IRONSHELL_AGENT", "IRONSHELL_POLICY"] {
            std::env::remove_var(n);
        }
        match allow {
            Some(a) => std::env::set_var("IRONSHELL_NET_ALLOW", a),
            None => std::env::remove_var("IRONSHELL_NET_ALLOW"),
        }
        std::env::set_var("PATH", &empty_dir);
        let audit_log = std::env::temp_dir().join(format!(
            "ae_egress_audit_{tag}_{}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&audit_log);
        std::env::set_var("IRONSHELL_AUDIT_LOG", &audit_log);
        Self {
            saved,
            empty_dir,
            audit_log,
        }
    }
}

impl Drop for Egress {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }
        let _ = std::fs::remove_dir_all(&self.empty_dir);
        let _ = std::fs::remove_file(&self.audit_log);
    }
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

/// Did the egress check refuse this call? Only that refusal counts: a missing
/// `ssh` or `docker` is not evidence either way.
fn refused_by_egress(r: &anyhow::Result<Value>) -> bool {
    match r {
        Ok(_) => false,
        Err(e) => format!("{e:#}").contains("egress denied"),
    }
}

/// The structured refusal, if the error is one: code and message.
fn egress_refusal(r: &anyhow::Result<Value>) -> Option<(ErrorCode, String)> {
    let e = r.as_ref().err()?;
    let se = e.downcast_ref::<SafetyError>()?;
    Some((se.code, se.message.clone()))
}

/// Each case: builtin name, its arguments, and the reason it is refused.
fn off_list_cases() -> Vec<(&'static str, Vec<Value>)> {
    vec![
        // The host argument is the destination.
        (
            "ssh_exec",
            vec![s("ops@bastion.example.invalid"), s("uptime")],
        ),
        // Both hosts are judged: ssh_host is connected to, remote_host is reached through it.
        (
            "ssh_tunnel",
            vec![
                Value::Int(18080),
                s("db.example.com"),
                Value::Int(5432),
                s("bastion.example.invalid"),
            ],
        ),
        (
            "ssh_tunnel",
            vec![
                Value::Int(18081),
                s("evil.example.invalid"),
                Value::Int(5432),
                s("bastion.example.com"),
            ],
        ),
        // The registry is the first component of the reference.
        ("docker_pull", vec![s("evil.example.invalid/app:1")]),
        ("podman_pull", vec![s("evil.example.invalid/app:1")]),
        ("docker_push", vec![s("evil.example.invalid/app:1")]),
        // No registry named means Docker Hub, which is not on the list.
        ("docker_pull", vec![s("alpine:3")]),
    ]
}

fn on_list_cases() -> Vec<(&'static str, Vec<Value>)> {
    vec![
        ("ssh_exec", vec![s("ops@build.example.com"), s("uptime")]),
        ("docker_pull", vec![s("registry.example.com/app:1")]),
    ]
}

/// Builtins that cannot name their destination in the arguments. Under an
/// allowlist they fail closed, and the refusal says why.
fn fail_closed_cases() -> Vec<(&'static str, Vec<Value>)> {
    vec![
        ("npm_install", vec![s("left-pad")]),
        ("terraform_plan", vec![]),
        ("docker_build", vec![s("app:1")]),
        ("helm_install", vec![s("release"), s("some-chart")]),
        // The backend and providers come from the configuration, not the arguments.
        ("terraform_state", vec![]),
        ("terraform_output", vec![]),
        ("terraform_workspace", vec![s("prod")]),
    ]
}

/// Package-manager builtins that fetch from the network. Only the refusal is
/// tested here, and only under an allowlist: the refusal happens before any
/// spawn. The apt half runs `/usr/bin/<tool>` by absolute path, so with no
/// allowlist these would really run apt, and they are not called without one.
fn pkg_fail_closed_cases() -> Vec<(&'static str, Vec<Value>)> {
    vec![
        ("pkg_install", vec![Value::Array(vec![s("git")])]),
        ("pkg_update", vec![]),
        ("pkg_upgrade", vec![]),
    ]
}

/// Local queries of the same builtins: an allowlist does not refuse them.
fn local_query_cases() -> Vec<(&'static str, Vec<Value>)> {
    vec![("terraform_workspace", vec![])]
}

#[test]
fn off_list_destinations_are_refused_before_anything_runs() {
    let _g = lock();
    let _e = Egress::new("off", Some("example.com"));
    for (name, args) in off_list_cases() {
        let r = call(name, args.clone());
        let (code, message) = egress_refusal(&r).unwrap_or_else(|| {
            panic!("{name} with an off-list destination must be refused by the egress check; got {r:?}")
        });
        assert_eq!(
            code,
            ErrorCode::PolicyDeny,
            "{name}: the egress refusal uses the existing E_POLICY_DENY code"
        );
        assert!(
            message.starts_with(&format!("{name}: egress denied")),
            "{name}: the refusal names the builtin: {message}"
        );
        assert!(
            message.contains("not in the allowlist") || message.contains("could not be determined"),
            "{name}: the refusal says why: {message}"
        );
    }
}

#[test]
fn on_list_destinations_are_not_refused_by_the_egress_check() {
    let _g = lock();
    let _e = Egress::new("on", Some("example.com"));
    for (name, args) in on_list_cases() {
        let r = call(name, args);
        assert!(
            !refused_by_egress(&r),
            "{name} with an on-list destination must pass the egress check; got {r:?}"
        );
    }
}

#[test]
fn no_allowlist_means_no_refusal_by_this_check() {
    let _g = lock();
    let _e = Egress::new("none", None);
    for (name, args) in off_list_cases()
        .into_iter()
        .chain(on_list_cases())
        .chain(fail_closed_cases())
    {
        let r = call(name, args);
        assert!(
            !refused_by_egress(&r),
            "{name} with no IRONSHELL_NET_ALLOW must not be refused by the egress check; got {r:?}"
        );
    }
}

#[test]
fn undeterminable_destinations_fail_closed_under_an_allowlist_and_say_why() {
    let _g = lock();
    let _e = Egress::new("closed", Some("example.com"));
    for (name, args) in fail_closed_cases()
        .into_iter()
        .chain(pkg_fail_closed_cases())
    {
        let r = call(name, args);
        let (code, message) = egress_refusal(&r)
            .unwrap_or_else(|| panic!("{name} must fail closed under an allowlist; got {r:?}"));
        assert_eq!(code, ErrorCode::PolicyDeny, "{name}");
        assert!(
            message.contains("an allowlist is set, so the call is refused"),
            "{name}: the refusal says an allowlist is why: {message}"
        );
    }
}

#[test]
fn a_wildcard_allowlist_passes_every_destination() {
    let _g = lock();
    let _e = Egress::new("star", Some("*"));
    for (name, args) in off_list_cases() {
        let r = call(name, args);
        assert!(
            !refused_by_egress(&r),
            "{name} under `*` must pass the egress check; got {r:?}"
        );
    }
}

#[test]
fn local_queries_are_not_refused_by_an_allowlist() {
    let _g = lock();
    let _e = Egress::new("local", Some("example.com"));
    for (name, args) in local_query_cases() {
        let r = call(name, args);
        assert!(
            !refused_by_egress(&r),
            "{name} without a remote backend reference is local and must not be refused; got {r:?}"
        );
    }
}

/// `pkg_search` and `pkg_info` go online only on Windows, where they run
/// `winget search` and `winget show`. The apt side reads the local index.
#[cfg(windows)]
#[test]
fn winget_queries_are_refused_under_an_allowlist() {
    let _g = lock();
    let _e = Egress::new("winget_q", Some("example.com"));
    for (name, args) in [
        ("pkg_search", vec![s("git")]),
        ("pkg_info", vec![s("Git.Git")]),
    ] {
        let r = call(name, args);
        let (code, message) = egress_refusal(&r).unwrap_or_else(|| {
            panic!("{name} (winget) must be refused under an allowlist; got {r:?}")
        });
        assert_eq!(code, ErrorCode::PolicyDeny, "{name}");
        assert!(
            message.starts_with(&format!("{name}: egress denied")),
            "{name}: {message}"
        );
    }
}

/// On the apt side `pkg_search` and `pkg_info` read the local index, so an
/// allowlist does not refuse them. Not run on Windows.
#[cfg(not(windows))]
#[test]
fn apt_queries_are_local_and_not_refused_by_an_allowlist() {
    let _g = lock();
    let _e = Egress::new("apt_q", Some("example.com"));
    for (name, args) in [("pkg_search", vec![s("git")]), ("pkg_info", vec![s("git")])] {
        let r = call(name, args);
        assert!(
            !refused_by_egress(&r),
            "{name} reads the local apt index and must not be refused; got {r:?}"
        );
    }
}

/// `pkg_add_source` with `update` runs `apt-get update` (a fetch), so an
/// allowlist refuses it before the source file is written. Without `update`
/// the file is written locally and this check does not apply.
#[test]
fn add_source_with_update_is_refused_before_the_file_is_written() {
    let _g = lock();
    let _e = Egress::new("add_src", Some("example.com"));
    let mut opts = std::collections::BTreeMap::new();
    opts.insert("update".to_string(), Value::Bool(true));
    let r = call(
        "pkg_add_source",
        vec![
            s("ironshell-egress-test"),
            s("deb https://deb.example.invalid/debian trixie main"),
            Value::Record(opts),
        ],
    );
    let (code, message) = egress_refusal(&r)
        .unwrap_or_else(|| panic!("pkg_add_source with update must be refused; got {r:?}"));
    assert_eq!(code, ErrorCode::PolicyDeny);
    assert!(
        message.starts_with("pkg_add_source: egress denied"),
        "{message}"
    );
}
