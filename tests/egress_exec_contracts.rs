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
    ]
}

/// Local queries of the same builtins: an allowlist does not refuse them.
fn local_query_cases() -> Vec<(&'static str, Vec<Value>)> {
    // `helm_repos` reads only helm's own config file, but it is classed Network
    // in the effect table, so the central guard refuses it under an allowlist.
    // That is an existing over-refusal, not a leak, and is not changed here.
    vec![("terraform_workspace", vec![])]
}

/// The git builtins that contact a remote take no remote name or URL: they fetch
/// from the default remote that `.git/config` names. The central guard judges
/// their (empty) argument list, and with no host to judge it refuses them under
/// an allowlist. The refusal comes from `guard`, not `check_egress_unknown`, so
/// its message does not carry the allowlist suffix.
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

/// git_fetch, git_pull and git_push reach the remote in `.git/config`, which the
/// call does not name. Under an allowlist they are refused before git runs. The
/// refusal is "could not be determined", not an off-list host, because the
/// destination is not in the arguments.
#[test]
fn git_remote_builtins_are_refused_under_an_allowlist() {
    let _g = lock();
    let _e = Egress::new("git_remote", Some("example.com"));
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
            message.contains("could not be determined"),
            "{name}: the refusal says the destination is unknown: {message}"
        );
    }
}

/// The same git builtins with a wildcard allowlist are not refused by this check.
#[test]
fn git_remote_builtins_pass_a_wildcard_allowlist() {
    let _g = lock();
    let _e = Egress::new("git_star", Some("*"));
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
