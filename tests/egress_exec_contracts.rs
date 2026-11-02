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
            "HOME",
            "USERPROFILE",
            // Each changes which git config is read or where the repository is.
            "GIT_DIR",
            "GIT_CONFIG",
            "GIT_CONFIG_GLOBAL",
            "GIT_CONFIG_SYSTEM",
            "GIT_CONFIG_PARAMETERS",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_NOSYSTEM",
            "XDG_CONFIG_HOME",
            "ProgramData",
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
        // An empty home, so no test reads the real ~/.ssh/config. A test that
        // needs a config points HOME at its own directory.
        std::env::set_var("HOME", &empty_dir);
        std::env::set_var("USERPROFILE", &empty_dir);
        // Git's own config is read from fake files only: the global files live under the
        // empty home, and the system files are dropped (GIT_CONFIG_NOSYSTEM, which git
        // honours), so no test reads the real /etc/gitconfig or ProgramData file.
        std::env::set_var("XDG_CONFIG_HOME", &empty_dir);
        std::env::set_var("ProgramData", &empty_dir);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
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
        // `run` pulls a missing image from the registry the reference names.
        ("docker_run", vec![s("evil.example.invalid/app:1")]),
        ("podman_run", vec![s("evil.example.invalid/app:1")]),
        ("docker_run", vec![s("alpine:3")]),
    ]
}

fn on_list_cases() -> Vec<(&'static str, Vec<Value>)> {
    vec![
        ("ssh_exec", vec![s("ops@build.example.com"), s("uptime")]),
        ("docker_pull", vec![s("registry.example.com/app:1")]),
        ("docker_run", vec![s("registry.example.com/app:1")]),
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

/// Builtins that fetch from a registry, a module proxy or a remote import, where
/// the registry is set by the project's or the tool's configuration. Found by
/// listing every Network or Exec builtin that installs, builds or runs against a
/// remote source. Under an allowlist each is refused before anything runs.
fn registry_mirror_cases() -> Vec<(&'static str, Vec<Value>)> {
    vec![
        ("uv_run", vec![s("main.py")]),
        ("cargo_build_cmd", vec![]),
        ("cargo_test_cmd", vec![]),
        ("cargo_run", vec![]),
        ("go_build", vec![]),
        ("go_test", vec![]),
        ("go_run", vec![s(".")]),
        ("deno_run", vec![s("main.ts")]),
        ("mise_use", vec![s("node@20")]),
        ("pre_commit_run", vec![]),
        ("buildah_build", vec![s("app:1")]),
        // registries.conf can redirect a docker:// registry, so it is unknown.
        ("skopeo_inspect", vec![s("docker://alpine:3")]),
        (
            "skopeo_copy",
            vec![s("dir:."), s("docker://registry.example.com/app:1")],
        ),
        ("trivy_scan", vec![]),
        ("trivy_image", vec![s("alpine:3")]),
        ("gh_repo_cmd", vec![s("list")]),
        ("gh_repo", vec![s("view"), s("owner/repo")]),
    ]
}

/// Local queries of the same builtins: an allowlist does not refuse them.
fn local_query_cases() -> Vec<(&'static str, Vec<Value>)> {
    vec![
        ("terraform_workspace", vec![]),
        // Reads helm's own repository file. Classed ReadLocal.
        ("helm_repos", vec![]),
        // A local transport is not network.
        ("skopeo_inspect", vec![s("dir:.")]),
    ]
}

/// The git builtins that contact a remote take no remote name or URL: they fetch
/// from the remote the repository's `.git/config` names. Their bodies read that
/// remote (`safety::check_git_remote_egress`).
fn git_remote_cases() -> Vec<(&'static str, Vec<Value>)> {
    vec![
        ("git_fetch", vec![]),
        ("git_pull", vec![]),
        ("git_push", vec![]),
    ]
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
        .chain(registry_mirror_cases())
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
        .chain(registry_mirror_cases())
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
    assert!(
        iron_shell::safety::check_egress_unknown("x", "unknown").is_ok(),
        "`*` allows a destination that cannot be read"
    );
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

/// Restores the process working directory when dropped. The git tests change it,
/// and every test in this file holds `LOCK`, so none sees another's directory.
struct Cwd(std::path::PathBuf);

impl Cwd {
    fn enter(dir: &std::path::Path) -> Self {
        let saved = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir).expect("enter test repo");
        Cwd(saved)
    }
}

impl Drop for Cwd {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

/// A temporary repository with the given `.git/config` and `.git/HEAD`. Removed on drop.
struct Repo {
    dir: std::path::PathBuf,
}

impl Repo {
    fn new(tag: &str, config: &str, head: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ae_git_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".git")).expect("temp git dir");
        std::fs::write(dir.join(".git").join("config"), config).expect("temp config");
        std::fs::write(dir.join(".git").join("HEAD"), head).expect("temp HEAD");
        Repo { dir }
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const MAIN: &str = "ref: refs/heads/main\n";

/// A repository whose `origin` is `url`, on `main`.
fn origin_repo(tag: &str, url: &str) -> Repo {
    Repo::new(tag, &format!("[remote \"origin\"]\n\turl = {url}\n"), MAIN)
}

/// Under an allowlist, a remote the repository's config puts off the list is
/// refused, naming the host, and nothing is spawned.
#[test]
fn git_remote_host_off_the_list_is_refused_before_git_runs() {
    let _g = lock();
    let _e = Egress::new("git_off", Some("example.com"));
    let repo = origin_repo("git_off", "https://evil.example.invalid/x.git");
    let _cwd = Cwd::enter(&repo.dir);
    for (name, args) in git_remote_cases() {
        let r = call(name, args);
        let (code, message) = egress_refusal(&r)
            .unwrap_or_else(|| panic!("{name} must be refused under an allowlist; got {r:?}"));
        assert_eq!(code, ErrorCode::PolicyDeny, "{name}");
        assert!(
            message.starts_with(&format!("{name}: egress denied")),
            "{name}: {message}"
        );
        assert!(
            message.contains("evil.example.invalid") && message.contains("not in the allowlist"),
            "{name}: the refusal names the host: {message}"
        );
    }
}

/// A remote on the list is not refused by this check (git then fails to spawn,
/// because PATH is empty, and that is not an egress refusal).
#[test]
fn git_remote_host_on_the_list_is_not_refused() {
    let _g = lock();
    let _e = Egress::new("git_on", Some("example.com"));
    let repo = origin_repo("git_on", "https://git.example.com/x.git");
    let _cwd = Cwd::enter(&repo.dir);
    for (name, args) in git_remote_cases() {
        let r = call(name, args);
        assert!(
            !refused_by_egress(&r),
            "{name} with an on-list remote must pass the egress check; got {r:?}"
        );
    }
}

/// A repository whose remote cannot be determined is refused as unknown, and the
/// refusal says why: no repository, a missing remote, an ext:: transport, an
/// include, a remote helper, or an insteadOf that cannot be read.
#[test]
fn a_repo_with_no_determinable_remote_is_refused() {
    let _g = lock();
    let _e = Egress::new("git_unknown", Some("example.com"));
    let cases: Vec<(&str, Repo, &str)> = vec![
        (
            "no-remote",
            Repo::new("git_nr", "[core]\n\tbare = false\n", MAIN),
            "[remote] section",
        ),
        (
            "ext-transport",
            origin_repo("git_ext", "ext::sh -c touch% /tmp/x"),
            "ext:: or fd::",
        ),
        (
            "include",
            Repo::new(
                "git_inc",
                "[include]\n\tpath = ../x\n[remote \"origin\"]\n\turl = https://git.example.com/x\n",
                MAIN,
            ),
            "include",
        ),
        (
            "vcs-helper",
            Repo::new(
                "git_vcs",
                "[remote \"origin\"]\n\turl = https://git.example.com/x\n\tvcs = hg\n",
                MAIN,
            ),
            "remote helper",
        ),
        (
            "detached-unreadable-head",
            Repo::new(
                "git_head",
                "[remote \"origin\"]\n\turl = https://git.example.com/x\n",
                "garbage\n",
            ),
            "HEAD",
        ),
    ];
    for (label, repo, reason) in &cases {
        let _cwd = Cwd::enter(&repo.dir);
        for (name, args) in git_remote_cases() {
            let r = call(name, args);
            let (code, message) = egress_refusal(&r).unwrap_or_else(|| {
                panic!("{name} in {label} must be refused under an allowlist; got {r:?}")
            });
            assert_eq!(code, ErrorCode::PolicyDeny, "{name} in {label}");
            assert!(
                message.contains(reason),
                "{name} in {label}: the refusal says why ({reason}): {message}"
            );
            assert!(
                message.contains("an allowlist is set, so the call is refused"),
                "{name} in {label}: {message}"
            );
        }
    }
    // Outside any repository at all.
    let outside = std::env::temp_dir().join(format!("ae_git_none_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&outside);
    let _cwd = Cwd::enter(&outside);
    let r = call("git_fetch", vec![]);
    let _ = std::fs::remove_dir_all(&outside);
    let (_, message) = egress_refusal(&r).expect("no repository is refused");
    assert!(
        message.contains("no git repository") || message.contains("worktree or submodule"),
        "{message}"
    );
}

/// `insteadOf` in the repository's config is applied, so a short name that
/// rewrites to an off-list host is refused, and `pushInsteadOf` applies to push.
#[test]
fn insteadof_in_the_repo_config_is_judged() {
    let _g = lock();
    let _e = Egress::new("git_insteadof", Some("example.com"));
    let config = "[url \"https://evil.example.invalid/\"]\n\tinsteadOf = gh:\n\
                  [url \"https://push.evil.example.invalid/\"]\n\tpushInsteadOf = https://git.example.com/\n\
                  [remote \"origin\"]\n\turl = gh:o/r\n\tpushurl = https://git.example.com/o/r\n";
    let repo = Repo::new("git_insteadof", config, MAIN);
    let _cwd = Cwd::enter(&repo.dir);
    let r = call("git_fetch", vec![]);
    let (_, message) = egress_refusal(&r).expect("insteadOf rewrites to an off-list host");
    assert!(message.contains("evil.example.invalid"), "{message}");
    // Push uses pushurl, and pushInsteadOf rewrites it to push.evil.example.invalid.
    let r = call("git_push", vec![]);
    let (_, message) = egress_refusal(&r).expect("pushInsteadOf rewrites to an off-list host");
    assert!(message.contains("push.evil.example.invalid"), "{message}");
}

/// The branch's remote is the one judged, not `origin`.
#[test]
fn the_current_branchs_remote_is_judged() {
    let _g = lock();
    let _e = Egress::new("git_branch", Some("example.com"));
    let config = "[remote \"origin\"]\n\turl = https://git.example.com/x\n\
                  [remote \"up\"]\n\turl = https://evil.example.invalid/x\n\
                  [branch \"main\"]\n\tremote = up\n";
    let repo = Repo::new("git_branch", config, MAIN);
    let _cwd = Cwd::enter(&repo.dir);
    let r = call("git_pull", vec![]);
    let (_, message) = egress_refusal(&r).expect("the branch remote is off the list");
    assert!(message.contains("evil.example.invalid"), "{message}");
}

/// Pure parser tests on strings: which host each remote form contacts.
#[test]
fn git_remote_forms_are_classified() {
    use iron_shell::safety::{git_remote_target, GitRemote};
    let host = |h: &str, ssh: bool| GitRemote::Host {
        host: h.to_string(),
        ssh,
    };
    assert_eq!(
        git_remote_target("https://github.com/o/r.git"),
        host("github.com", false)
    );
    assert_eq!(
        git_remote_target("https://user@GitHub.com:443/o/r"),
        host("github.com", false)
    );
    assert_eq!(
        git_remote_target("ssh://git@Evil.Example.invalid:2222/o/r.git"),
        host("evil.example.invalid", true)
    );
    assert_eq!(
        git_remote_target("git@build.example.com:org/repo.git"),
        host("build.example.com", true)
    );
    assert_eq!(
        git_remote_target("build.example.com:org/repo.git"),
        host("build.example.com", true)
    );
    assert_eq!(git_remote_target("file:///srv/repo.git"), GitRemote::Local);
    assert_eq!(git_remote_target("/srv/repo.git"), GitRemote::Local);
    assert_eq!(git_remote_target("../sibling"), GitRemote::Local);
    assert_eq!(git_remote_target("C:\\repos\\x"), GitRemote::Local);
    assert_eq!(git_remote_target("C:/repos/x"), GitRemote::Local);
    assert_eq!(git_remote_target("some/dir:name"), GitRemote::Local);
    assert!(matches!(
        git_remote_target("ext::sh -c touch% /tmp/x"),
        GitRemote::Unknown(_)
    ));
    assert!(matches!(git_remote_target("fd::3"), GitRemote::Unknown(_)));
    assert!(matches!(
        git_remote_target("hg::https://x.example.com/r"),
        GitRemote::Unknown(_)
    ));
    // A share is a Windows form; elsewhere this string is a relative path.
    #[cfg(windows)]
    assert!(matches!(
        git_remote_target("\\\\files.example.invalid\\share\\repo"),
        GitRemote::Unknown(_)
    ));
}

/// Pure parser tests on strings: the URL git contacts, after the repository's
/// remote is picked and `insteadOf` applied.
#[test]
fn git_contact_urls_follow_the_remote_and_insteadof() {
    use iron_shell::safety::git_contact_urls;
    let plain = "[remote \"origin\"]\n\turl = https://a.example.com/x\n";
    assert_eq!(
        git_contact_urls(&[], plain, MAIN, false),
        Ok(vec!["https://a.example.com/x".to_string()])
    );
    // A detached HEAD uses origin.
    assert_eq!(
        git_contact_urls(
            &[],
            plain,
            "0123456789abcdef0123456789abcdef01234567\n",
            false
        ),
        Ok(vec!["https://a.example.com/x".to_string()])
    );
    // insteadOf: longest matching prefix wins.
    let rules = "[url \"https://short.example.com/\"]\n\tinsteadOf = gh:\n\
                 [url \"https://long.example.com/\"]\n\tinsteadOf = gh:o/\n\
                 [remote \"origin\"]\n\turl = gh:o/r\n";
    assert_eq!(
        git_contact_urls(&[], rules, MAIN, false),
        Ok(vec!["https://long.example.com/r".to_string()])
    );
    // pushInsteadOf applies to push and takes precedence over insteadOf.
    let push = "[url \"https://push.example.com/\"]\n\tpushInsteadOf = https://a.example.com/\n\
                [url \"https://other.example.com/\"]\n\tinsteadOf = https://a.example.com/\n\
                [remote \"origin\"]\n\turl = https://a.example.com/x\n";
    assert_eq!(
        git_contact_urls(&[], push, MAIN, true),
        Ok(vec!["https://push.example.com/x".to_string()])
    );
    assert_eq!(
        git_contact_urls(&[], push, MAIN, false),
        Ok(vec!["https://other.example.com/x".to_string()])
    );
    // remote.pushDefault picks the push remote; branch.<b>.pushRemote beats it.
    let pd = "[remote \"origin\"]\n\turl = https://a.example.com/x\n\
              [remote \"fork\"]\n\turl = https://b.example.com/x\n\
              [remote]\n\tpushDefault = fork\n";
    assert_eq!(
        git_contact_urls(&[], pd, MAIN, true),
        Ok(vec!["https://b.example.com/x".to_string()])
    );
    assert_eq!(
        git_contact_urls(&[], pd, MAIN, false),
        Ok(vec!["https://a.example.com/x".to_string()])
    );
    // A remote named `.` is local.
    let local = "[branch \"main\"]\n\tremote = .\n";
    assert_eq!(git_contact_urls(&[], local, MAIN, false), Ok(Vec::new()));
    // Comments, quotes and key case.
    let quoted = "[Remote \"origin\"] # the main one\n\tURL = \"https://a.example.com/x\" ; note\n";
    assert_eq!(
        git_contact_urls(&[], quoted, MAIN, false),
        Ok(vec!["https://a.example.com/x".to_string()])
    );
    // Refusals: an include, an escape, a missing remote, no URL, a bad HEAD.
    assert!(git_contact_urls(&[], "[include]\n\tpath = x\n", MAIN, false).is_err());
    assert!(git_contact_urls(&[], "[remote \"origin\"]\n\turl = C:\\x\n", MAIN, false).is_err());
    assert!(git_contact_urls(
        &[],
        "[remote \"up\"]\n\turl = https://x.example.com/\n",
        MAIN,
        false
    )
    .is_err());
    assert!(git_contact_urls(&[], "[remote \"origin\"]\n", MAIN, false).is_err());
    assert!(git_contact_urls(&[], plain, "not a head\n", false).is_err());
}

/// The config reader keeps the entries it understands and refuses the rest.
#[test]
fn git_config_entries_parse_sections_and_keys() {
    use iron_shell::safety::git_config_entries;
    let e = git_config_entries("[Remote \"a.b\"]\n  URL=https://x/\n[core]\n bare\n").unwrap();
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].section, "remote");
    assert_eq!(e[0].subsection.as_deref(), Some("a.b"));
    assert_eq!(e[0].key, "url");
    assert_eq!(e[0].value, "https://x/");
    assert_eq!(e[1].key, "bare");
    assert_eq!(e[1].value, "true");
    assert!(
        git_config_entries("url = x\n").is_err(),
        "a key before any section"
    );
}

/// Pure: the repository's values win over the lower levels, a lower level fills
/// in a value the repository does not set, the later lower file wins, and
/// rewrites from every level are combined, the longest prefix first.
/// A Windows path in an unrelated setting must not refuse every fetch, but an
/// escape that could change a destination or hide a key still does.
#[test]
fn only_a_backslash_that_can_change_the_destination_refuses() {
    use iron_shell::safety::git_config_entries;
    assert!(git_config_entries("[http]\n\tsslCAInfo = C:\\certs\\ca.pem\n").is_ok());
    assert!(git_config_entries("[remote \"o\"]\n\turl = a\\tb\n").is_err());
    assert!(git_config_entries("[url \"x\"]\n\tinsteadOf = a\\b\n").is_err());
    assert!(git_config_entries("[http]\n\tproxy = a\\\n[remote \"o\"]\n").is_err());
}

#[test]
fn lower_levels_fill_in_values_and_rewrites_combine() {
    use iron_shell::safety::git_contact_urls;
    let global = "[url \"https://global.example.com/\"]\n\tinsteadOf = gh:\n\
                  [remote \"origin\"]\n\turl = https://evil.example.invalid/x\n";
    let repo = "[remote \"origin\"]\n\turl = https://git.example.com/x\n";
    let bare = "[core]\n\tbare = false\n";
    // The repository's url wins over the global one.
    assert_eq!(
        git_contact_urls(&[global], repo, MAIN, false),
        Ok(vec!["https://git.example.com/x".to_string()])
    );
    // With no repository remote, the global one is used.
    assert_eq!(
        git_contact_urls(&[global], bare, MAIN, false),
        Ok(vec!["https://evil.example.invalid/x".to_string()])
    );
    // A url is multi-valued, as git reads it: a lower level's urls all count, and each
    // is judged, so a system url is not hidden by a global one.
    let system = "[remote \"origin\"]\n\turl = https://system.example.com/x\n";
    assert_eq!(
        git_contact_urls(&[system, global], bare, MAIN, false),
        Ok(vec![
            "https://system.example.com/x".to_string(),
            "https://evil.example.invalid/x".to_string()
        ])
    );
    // A global rewrite applies to the repository's remote.
    let short = "[remote \"origin\"]\n\turl = gh:o/r\n";
    assert_eq!(
        git_contact_urls(&[global], short, MAIN, false),
        Ok(vec!["https://global.example.com/o/r".to_string()])
    );
    // Rewrites combine across levels: the repository's longer prefix wins.
    let long = "[url \"https://long.example.com/\"]\n\tinsteadOf = gh:o/\n\
                [remote \"origin\"]\n\turl = gh:o/r\n";
    assert_eq!(
        git_contact_urls(&[global], long, MAIN, false),
        Ok(vec!["https://long.example.com/r".to_string()])
    );
    // An include at any level refuses.
    assert!(git_contact_urls(&["[include]\n\tpath = x\n"], repo, MAIN, false).is_err());
}

/// Pure: which config files are read, in git's order, for a given environment.
/// The fake environment is a closure, so no file is touched.
#[test]
fn git_config_files_follow_git_order() {
    use iron_shell::safety::git_config_paths;
    use std::ffi::OsString;
    use std::path::PathBuf;
    let home = PathBuf::from("/fake/home");
    let with = |vars: &'static [(&'static str, &'static str)]| {
        move |name: &str| -> Option<OsString> {
            vars.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(v))
        }
    };
    // System, then ~/.config/git/config, then ~/.gitconfig.
    assert_eq!(
        git_config_paths(with(
            &[("HOME", "/fake/home"), ("ProgramData", "/fake/pd"),]
        )),
        vec![
            PathBuf::from("/etc/gitconfig"),
            PathBuf::from("/fake/pd").join("Git").join("config"),
            home.join(".config").join("git").join("config"),
            home.join(".gitconfig"),
        ]
    );
    // XDG_CONFIG_HOME replaces ~/.config, and GIT_CONFIG_NOSYSTEM=1 drops the system files.
    assert_eq!(
        git_config_paths(with(&[
            ("HOME", "/fake/home"),
            ("XDG_CONFIG_HOME", "/fake/xdg"),
            ("GIT_CONFIG_NOSYSTEM", "1"),
        ])),
        vec![
            PathBuf::from("/fake/xdg").join("git").join("config"),
            home.join(".gitconfig"),
        ]
    );
    // An empty XDG_CONFIG_HOME is unset; GIT_CONFIG_NOSYSTEM=0 keeps the system files.
    assert_eq!(
        git_config_paths(with(&[
            ("HOME", "/fake/home"),
            ("XDG_CONFIG_HOME", ""),
            ("GIT_CONFIG_NOSYSTEM", "0"),
        ]))[0],
        PathBuf::from("/etc/gitconfig")
    );
    // HOME falls back to USERPROFILE.
    assert_eq!(
        git_config_paths(with(&[("USERPROFILE", "/fake/up")])).last(),
        Some(&PathBuf::from("/fake/up").join(".gitconfig"))
    );
}

/// A global `~/.gitconfig` and `$XDG_CONFIG_HOME/git/config` are read: an
/// `insteadOf` in either rewrites the remote to an off-list host.
#[test]
fn global_config_insteadof_is_judged() {
    let _g = lock();
    let e = Egress::new("git_global", Some("example.com"));
    let repo = Repo::new("git_global", "[remote \"origin\"]\n\turl = gh:o/r\n", MAIN);
    let _cwd = Cwd::enter(&repo.dir);
    std::fs::write(
        e.empty_dir.join(".gitconfig"),
        "[url \"https://evil.example.invalid/\"]\n\tinsteadOf = gh:\n",
    )
    .expect("fake ~/.gitconfig");
    let r = call("git_fetch", vec![]);
    let (_, message) = egress_refusal(&r).expect("a ~/.gitconfig insteadOf is judged");
    assert!(message.contains("evil.example.invalid"), "{message}");
    // The same rule in the XDG file, with ~/.gitconfig gone.
    std::fs::remove_file(e.empty_dir.join(".gitconfig")).expect("remove fake ~/.gitconfig");
    std::fs::create_dir_all(e.empty_dir.join("git")).expect("fake XDG git dir");
    std::fs::write(
        e.empty_dir.join("git").join("config"),
        "[url \"https://xdg.example.invalid/\"]\n\tinsteadOf = gh:\n",
    )
    .expect("fake XDG config");
    let r = call("git_pull", vec![]);
    let (_, message) = egress_refusal(&r).expect("an XDG config insteadOf is judged");
    assert!(message.contains("xdg.example.invalid"), "{message}");
}

/// An `include` in a global file refuses as unknown, as it does in the repository.
#[test]
fn an_include_in_a_global_file_refuses_as_unknown() {
    let _g = lock();
    let e = Egress::new("git_global_inc", Some("example.com"));
    let repo = origin_repo("git_global_inc", "https://git.example.com/x.git");
    let _cwd = Cwd::enter(&repo.dir);
    std::fs::write(e.empty_dir.join(".gitconfig"), "[include]\n\tpath = ../x\n")
        .expect("fake ~/.gitconfig");
    let r = call("git_fetch", vec![]);
    let (code, message) = egress_refusal(&r).expect("an include in a global file refuses");
    assert_eq!(code, ErrorCode::PolicyDeny);
    assert!(message.contains("include"), "{message}");
}

/// A worktree's `.git` file names its gitdir: `HEAD` is read from the gitdir and
/// `config` from the common directory that the gitdir's `commondir` names.
#[test]
fn a_worktrees_remote_is_read_from_its_gitdir_and_common_dir() {
    let _g = lock();
    let _e = Egress::new("git_wt", Some("example.com"));
    let root = std::env::temp_dir().join(format!("ae_git_wt_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let common = root.join("main").join(".git");
    let gitdir = common.join("worktrees").join("wt");
    let wt = root.join("wt");
    std::fs::create_dir_all(&gitdir).expect("gitdir");
    std::fs::create_dir_all(&wt).expect("worktree");
    // The main checkout is on main, whose remote is origin (on the list). The
    // worktree is on feature, whose remote `up` is off the list. Only a HEAD read
    // from the gitdir and a config read from the commondir gives the refusal.
    std::fs::write(
        common.join("config"),
        "[remote \"origin\"]\n\turl = https://git.example.com/x\n\
         [remote \"up\"]\n\turl = https://evil.example.invalid/x\n\
         [branch \"feature\"]\n\tremote = up\n",
    )
    .expect("common config");
    std::fs::write(common.join("HEAD"), MAIN).expect("main HEAD");
    std::fs::write(gitdir.join("HEAD"), "ref: refs/heads/feature\n").expect("gitdir HEAD");
    std::fs::write(gitdir.join("commondir"), "../..\n").expect("commondir");
    std::fs::write(wt.join(".git"), format!("gitdir: {}\n", gitdir.display())).expect(".git file");
    {
        let _cwd = Cwd::enter(&wt);
        let r = call("git_pull", vec![]);
        let (_, message) =
            egress_refusal(&r).expect("the worktree's branch remote is off the list");
        assert!(message.contains("evil.example.invalid"), "{message}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// A malformed worktree pointer refuses as unknown, and the refusal says so: a
/// `.git` file with no `gitdir:`, a gitdir that does not exist, and a gitdir whose
/// `commondir` names a directory that does not exist.
#[test]
fn a_malformed_worktree_pointer_refuses_as_unknown() {
    let _g = lock();
    let _e = Egress::new("git_ptr", Some("example.com"));
    let root = std::env::temp_dir().join(format!("ae_git_ptr_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("gd")).expect("gd");
    std::fs::write(root.join("gd").join("HEAD"), MAIN).expect("gd HEAD");
    std::fs::write(root.join("gd").join("commondir"), "missing\n").expect("gd commondir");
    let cases = [
        ("garbage", "garbage\n".to_string()),
        (
            "no-gitdir",
            format!("gitdir: {}\n", root.join("nowhere").display()),
        ),
        (
            "no-commondir",
            format!("gitdir: {}\n", root.join("gd").display()),
        ),
    ];
    for (label, pointer) in cases {
        let wt = root.join(label);
        std::fs::create_dir_all(&wt).expect("worktree");
        std::fs::write(wt.join(".git"), pointer).expect(".git file");
        let _cwd = Cwd::enter(&wt);
        for (name, args) in git_remote_cases() {
            let r = call(name, args);
            let (code, message) = egress_refusal(&r)
                .unwrap_or_else(|| panic!("{name} in {label} must be refused; got {r:?}"));
            assert_eq!(code, ErrorCode::PolicyDeny, "{name} in {label}");
            assert!(message.contains("worktree"), "{name} in {label}: {message}");
        }
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// The same git builtins with a wildcard allowlist are not refused by this check.
#[test]
fn git_remote_builtins_pass_a_wildcard_allowlist() {
    let _g = lock();
    let _e = Egress::new("git_star", Some("*"));
    let repo = origin_repo("git_star", "https://evil.example.invalid/x.git");
    let _cwd = Cwd::enter(&repo.dir);
    for (name, args) in git_remote_cases() {
        let r = call(name, args);
        assert!(
            !refused_by_egress(&r),
            "{name} under `*` must pass the egress check; got {r:?}"
        );
    }
}

/// `pkg_changelog` takes a package name, which is not a host. The dispatcher used
/// to judge that name as a host, so an allowlist naming a dotted name such as
/// `python3.11` let the call through to apt, while an undotted name was refused.
/// The body now makes the destination-unknown check, so both names are refused
/// with the same reason before any package manager is detected. Only the refusal
/// is asserted, and only under an allowlist: without one the apt path would
/// really fetch a changelog.
#[test]
fn pkg_changelog_refuses_dotted_and_undotted_names_alike_under_an_allowlist() {
    let _g = lock();
    let _e = Egress::new("changelog", Some("python3.11"));
    let mut reasons = Vec::new();
    for name in ["python3.11", "bash"] {
        let r = call("pkg_changelog", vec![s(name)]);
        let (code, message) = egress_refusal(&r).unwrap_or_else(|| {
            panic!("pkg_changelog {name} must be refused under an allowlist; got {r:?}")
        });
        assert_eq!(code, ErrorCode::PolicyDeny, "pkg_changelog {name}");
        assert!(
            message.starts_with("pkg_changelog: egress denied"),
            "pkg_changelog {name}: {message}"
        );
        assert!(
            message.contains("an allowlist is set, so the call is refused"),
            "pkg_changelog {name}: {message}"
        );
        reasons.push(message.replace(name, "<package>"));
    }
    assert_eq!(
        reasons[0], reasons[1],
        "a dotted and an undotted package name must get the same refusal"
    );
}

/// A UNC string names a host, and asking the file system about it opens an SMB
/// session to that host. Under an allowlist that excludes the host, `find`
/// (which hands its string straight to `validate_read_path`) must refuse it with
/// `PolicyDeny` before any probe. `.invalid` never resolves, so a probe that got
/// through would stall for seconds; the call must return well under that.
/// `cat` and `ls` stat their argument before the path check and are not covered
/// here (see the report on the call sites).
#[cfg(windows)]
#[test]
fn a_unc_path_is_judged_by_its_host_before_anything_probes_it() {
    let _g = lock();
    let _e = Egress::new("unc", Some("example.com"));
    for (builtin, path) in [
        ("find", r"\\unreachable.invalid\share\x"),
        ("find", "//unreachable.invalid/share/x"),
        ("ls", r"\\unreachable.invalid\share\x"),
        ("cat", r"\\unreachable.invalid\share\x"),
        ("cat", r"\\unreachable.invalid\share\*.txt"),
        ("fs_glob", "//unreachable.invalid/share/*.txt"),
    ] {
        let started = std::time::Instant::now();
        let r = call(builtin, vec![s(path)]);
        let elapsed = started.elapsed();
        let (code, message) = egress_refusal(&r).unwrap_or_else(|| {
            panic!("{builtin} {path:?} must be refused by the egress check; got {r:?}")
        });
        assert_eq!(code, ErrorCode::PolicyDeny, "{builtin} {path:?}");
        assert!(
            message.contains("unreachable.invalid") && message.contains("not in the allowlist"),
            "{builtin} {path:?}: the refusal names the host: {message}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "{builtin} {path:?}: refused after {elapsed:?}; a probe reached the network"
        );
    }

    // The verbatim form of a local path names no host.
    let local = format!(
        r"\\?\{}\Cargo.toml",
        std::env::current_dir().unwrap().display()
    );
    let r = call("cat", vec![s(&local)]);
    assert!(
        egress_refusal(&r).is_none(),
        "a verbatim local path is not egress: {r:?}"
    );
}

// ---------------------------------------------------------------------------
// ssh config: the allowlist judges the host ssh will connect to, not the alias
// the caller wrote. The parser is pure, so these pass it strings. No test reads
// a real ~/.ssh/config.
// ---------------------------------------------------------------------------

use iron_shell::safety::ssh_config_destination as resolve;

#[test]
fn an_alias_resolves_to_its_hostname() {
    let cfg = "Host prod\n    HostName evil.example\n";
    assert_eq!(resolve(cfg, "prod"), Ok(Some("evil.example".to_string())));
}

#[test]
fn a_dotted_alias_resolves_to_its_hostname() {
    let cfg = "Host github.com\n    HostName evil.example\n";
    assert_eq!(
        resolve(cfg, "github.com"),
        Ok(Some("evil.example".to_string()))
    );
}

#[test]
fn a_block_without_a_hostname_judges_the_name_as_written() {
    let cfg = "Host build.example.com\n    User ops\n";
    assert_eq!(resolve(cfg, "build.example.com"), Ok(None));
}

#[test]
fn a_wildcard_host_that_matches_the_name_fails_closed() {
    let cfg = "Host *.internal\n    HostName 10.0.0.9\n";
    assert!(resolve(cfg, "db.internal").is_err());
    // A wildcard that does not match the name does not apply to it.
    assert_eq!(resolve(cfg, "db.example.com"), Ok(None));
    // `Host *` applies to every name, but only HostName, ProxyJump and
    // ProxyCommand in it change the destination.
    assert!(resolve("Host *\n    HostName evil.example\n", "prod").is_err());
}

#[test]
fn a_harmless_wildcard_block_does_not_fail_closed() {
    // The common `Host *` block changes nothing about the destination.
    assert_eq!(
        resolve("Host *\n    ServerAliveInterval 60\n", "prod"),
        Ok(None)
    );
    assert_eq!(resolve("Host *\n    User ops\n", "prod"), Ok(None));
    // A later exact block still supplies the HostName.
    assert_eq!(
        resolve(
            "Host *\n    ServerAliveInterval 60\nHost prod\n    HostName real.example\n",
            "prod"
        ),
        Ok(Some("real.example".to_string()))
    );
}

#[test]
fn a_wildcard_block_with_proxyjump_fails_closed_for_names_it_applies_to() {
    assert!(resolve("Host *\n    ProxyJump bastion\n", "prod").is_err());
    assert!(resolve("Host *\n    ProxyCommand nc %h %p\n", "prod").is_err());
}

#[test]
fn a_proxyjump_in_a_block_for_another_name_does_not_affect_this_one() {
    assert_eq!(
        resolve("Host other\n    ProxyJump bastion\n", "prod"),
        Ok(None)
    );
    assert_eq!(
        resolve(
            "Host other\n    ProxyJump bastion\nHost prod\n    HostName a.example\n",
            "prod"
        ),
        Ok(Some("a.example".to_string()))
    );
}

#[test]
fn a_negated_pattern_excludes_the_name_from_its_block() {
    let cfg = "Host !prod *\n    HostName evil.example\n";
    assert_eq!(resolve(cfg, "prod"), Ok(None));
}

#[test]
fn proxyjump_and_proxycommand_fail_closed_in_a_block_that_applies() {
    assert!(resolve(
        "Host prod\n    HostName a.example\n    ProxyJump bastion\n",
        "prod"
    )
    .is_err());
    assert!(resolve("Host prod\n    ProxyCommand nc %h %p\n", "prod").is_err());
}

#[test]
fn include_and_match_fail_closed() {
    assert!(resolve("Include config.d/*\n", "prod").is_err());
    assert!(resolve("Host prod\n    HostName a.example\nInclude extra\n", "prod").is_err());
    assert!(resolve("Match host prod\n    HostName evil.example\n", "prod").is_err());
    assert!(resolve("Host x\n    HostName a.example\nmatch all\n", "prod").is_err());
}

#[test]
fn a_name_the_file_does_not_mention_is_judged_as_written() {
    let cfg = "Host other\n    HostName evil.example\n";
    assert_eq!(resolve(cfg, "prod.example.com"), Ok(None));
    assert_eq!(resolve("", "prod.example.com"), Ok(None));
}

#[test]
fn comments_are_ignored_and_keywords_are_case_insensitive() {
    // A commented directive is not a directive.
    let cfg = "# Host prod\n# HostName evil.example\n#Include x\n#Match all\n";
    assert_eq!(resolve(cfg, "prod"), Ok(None));
    // Keywords and names are read without regard to case, and `key=value` parses.
    let cfg = "HOST PROD\n  hostNAME real.example\n";
    assert_eq!(resolve(cfg, "prod"), Ok(Some("real.example".to_string())));
    let cfg = "Host=prod\nHostName=real.example\n";
    assert_eq!(resolve(cfg, "PROD"), Ok(Some("real.example".to_string())));
    // A commented INCLUDE does not fail closed.
    assert_eq!(
        resolve("# Include x\nHost prod\n  HostName a.example\n", "prod"),
        Ok(Some("a.example".to_string()))
    );
}

#[test]
fn the_first_hostname_that_applies_wins_and_a_token_fails_closed() {
    let cfg = "Host prod\n  HostName first.example\nHost prod\n  HostName second.example\n";
    assert_eq!(resolve(cfg, "prod"), Ok(Some("first.example".to_string())));
    assert!(resolve("Host prod\n  HostName %h.example\n", "prod").is_err());
}

/// The judge, end to end, with HOME pointed at a config this test writes. The
/// alias is on the allowlist and its HostName is not, so the call must be refused
/// before ssh starts. Judging the alias instead would let it through (and then
/// fail with "not found", because PATH is empty).
#[test]
fn an_allowlisted_alias_whose_hostname_is_off_the_list_is_refused() {
    let _g = lock();
    let _e = Egress::new("alias", Some("build.example.com"));
    let home = std::env::temp_dir().join(format!("ae_ssh_alias_home_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".ssh")).expect("temp ssh dir");
    std::fs::write(
        home.join(".ssh").join("config"),
        "Host build.example.com\n    HostName evil.example\n",
    )
    .expect("temp ssh config");
    std::env::set_var("HOME", &home);
    std::env::set_var("USERPROFILE", &home);

    let r = call("ssh_exec", vec![s("ops@build.example.com"), s("uptime")]);
    let _ = std::fs::remove_dir_all(&home);
    let (code, message) = egress_refusal(&r)
        .unwrap_or_else(|| panic!("the HostName must be judged and refused; got {r:?}"));
    assert_eq!(code, ErrorCode::PolicyDeny);
    assert!(
        message.contains("evil.example"),
        "the refusal names the resolved host: {message}"
    );
}
