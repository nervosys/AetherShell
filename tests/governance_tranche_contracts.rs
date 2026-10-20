//! Contracts for the rbac, audit, sso, compliance, plugin and marketplace
//! builtins: the fail-open, fabricated and silently-ignored behaviour found while
//! reading their bodies, each pinned by a test.
//!
//! Nothing here contacts the network. A marketplace check either refuses before
//! any request is made, or runs in agent mode with the network budget at zero,
//! where the governor refuses it before the request. All state (the audit log,
//! the workspace, the caches and the config directory) lives under a fresh temp
//! directory; no test touches the real user data.

use aethershell::builtins::{call, call_with_input};
use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());

/// Variables that change what these builtins do. Cleared before each test.
const ENV_KEYS: &[&str] = &[
    "AETHER_MODE",
    "AETHER_WORKSPACE",
    "AETHER_AUDIT_LOG",
    "AETHER_AUDIT_KEY",
    "AETHER_AUDIT_KEY_FILE",
    "AETHER_MAX_NET",
    "AETHER_POLICY",
    "AETHER_NET_ALLOW",
    "AETHER_PRINCIPAL",
    "AETHER_REGISTRY_TOKEN",
    "AETHERSHELL_TOKEN",
];

/// Exclusive use of the process environment, and a fresh scratch root.
struct World {
    _lock: MutexGuard<'static, ()>,
    root: PathBuf,
}

fn world(tag: &str) -> World {
    let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("ae_gov_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("ws")).expect("scratch workspace");
    for k in ENV_KEYS {
        std::env::remove_var(k);
    }
    for (k, sub) in [
        ("AETHER_DATA_HOME", "data"),
        ("AETHER_CONFIG_HOME", "config"),
        ("AETHER_CACHE_HOME", "cache"),
        ("HOME", "home"),
        ("XDG_CACHE_HOME", "cache"),
        ("LOCALAPPDATA", "local"),
        ("APPDATA", "roaming"),
    ] {
        std::env::set_var(k, root.join(sub));
    }
    std::env::set_var("AETHER_WORKSPACE", root.join("ws"));
    aethershell::safety::set_principal(None);
    World { _lock: lock, root }
}

/// Agent mode for the rest of the test, inside the scratch workspace.
fn agent(w: &World) {
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", w.root.join("ws"));
}

fn call_v(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let mut env = Env::new();
    call(name, args, &mut env).map_err(|e| e.to_string())
}

fn piped(name: &str, args: Vec<Value>, input: Value) -> Result<Value, String> {
    let mut env = Env::new();
    call_with_input(name, args, Some(input), &mut env).map_err(|e| e.to_string())
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

fn field(v: &Value, k: &str) -> Option<Value> {
    match v {
        Value::Record(r) => r.get(k).cloned(),
        _ => None,
    }
}

fn record(pairs: &[(&str, Value)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert((*k).to_string(), v.clone());
    }
    Value::Record(m)
}

/// The call must be refused, and the refusal must carry `code`.
fn refused_with(r: Result<Value, String>, code: &str) {
    match r {
        Ok(v) => panic!("expected {code}, but the call answered {v:?}"),
        Err(e) => assert!(e.contains(code), "expected {code}, got: {e}"),
    }
}

fn manifest(path: &Path, body: &str) -> String {
    std::fs::write(path, body).expect("write manifest");
    path.to_string_lossy().to_string()
}

// ---------------------------------------------------------------- RBAC

#[test]
fn rbac_grant_refuses_malformed_arguments() {
    let _w = world("grant_args");
    refused_with(
        call_v("rbac_grant", vec![s(""), s("effect:destructive")]),
        "E_BAD_ARG",
    );
    refused_with(
        call_v("rbac_grant", vec![s("gov-user"), s("no-colon")]),
        "E_BAD_ARG",
    );
    refused_with(
        call_v("rbac_grant", vec![s("gov-user"), s("a:b c")]),
        "E_BAD_ARG",
    );
    refused_with(
        call_v("rbac_grant", vec![Value::Int(7), s("effect:destructive")]),
        "E_BAD_ARG",
    );
}

#[test]
fn rbac_grant_says_when_nothing_changed() {
    let _w = world("grant_changed");
    let first = call_v(
        "rbac_grant",
        vec![s("gov-grant-user"), s("effect:destructive")],
    )
    .expect("first grant");
    assert_eq!(field(&first, "changed"), Some(Value::Bool(true)));
    let again = call_v(
        "rbac_grant",
        vec![s("gov-grant-user"), s("effect:destructive")],
    )
    .expect("repeated grant");
    assert_eq!(
        field(&again, "changed"),
        Some(Value::Bool(false)),
        "a repeated grant changed nothing and must not say it did"
    );
}

#[test]
fn rbac_principal_will_not_assume_a_password_account_by_name() {
    let _w = world("principal_pw");
    call_v("rbac_register", vec![s("gov-pw"), s("a-long-password")]).expect("register");
    // Naming the account used to make the shell act as it, with no proof.
    refused_with(call_v("rbac_principal", vec![s("gov-pw")]), "E_POLICY_DENY");
    assert_eq!(call_v("rbac_principal", vec![]), Ok(Value::Null));
}

#[test]
fn rbac_login_failures_are_coded_and_leave_no_principal() {
    let _w = world("login");
    call_v(
        "rbac_register",
        vec![s("gov-login"), s("correct-horse-battery")],
    )
    .expect("register");
    refused_with(
        call_v("rbac_login", vec![s("gov-login"), s("wrong-password")]),
        "E_POLICY_DENY",
    );
    refused_with(
        call_v("rbac_login", vec![s("gov-no-such-user"), s("anything")]),
        "E_POLICY_DENY",
    );
    assert_eq!(call_v("rbac_principal", vec![]), Ok(Value::Null));

    let ok = call_v(
        "rbac_login",
        vec![s("gov-login"), s("correct-horse-battery")],
    )
    .expect("right password");
    assert_eq!(field(&ok, "user"), Some(s("gov-login")));
    assert!(matches!(
        call_v("rbac_principal", vec![]),
        Ok(Value::Str(_))
    ));
    assert_eq!(call_v("rbac_logout", vec![]), Ok(Value::Bool(true)));
}

#[test]
fn rbac_register_refuses_a_name_already_taken() {
    let _w = world("register_taken");
    call_v("rbac_register", vec![s("gov-taken"), s("first-password")]).expect("first");
    refused_with(
        call_v("rbac_register", vec![s("gov-taken"), s("second-password")]),
        "E_BAD_ARG",
    );
    refused_with(call_v("rbac_register", vec![s(""), s("pw")]), "E_BAD_ARG");
}

#[test]
fn the_privileged_rbac_builtins_are_refused_in_agent_mode() {
    let w = world("agent_rbac");
    agent(&w);
    refused_with(
        call_v("rbac_grant", vec![s("gov-agent-target"), s("effect:*")]),
        "E_POLICY_DENY",
    );
    refused_with(
        call_v("rbac_principal", vec![s("gov-agent-target")]),
        "E_POLICY_DENY",
    );
    refused_with(
        call_v(
            "rbac_register",
            vec![s("gov-agent-reg"), s("long-enough-pw")],
        ),
        "E_POLICY_DENY",
    );
    assert_eq!(
        call_v("rbac_principal", vec![]),
        Ok(Value::Null),
        "a refused principal change must not take effect"
    );
}

#[test]
fn check_permission_refuses_non_strings_and_is_false_without_a_role() {
    let _w = world("check_perm");
    refused_with(
        call_v("check_permission", vec![Value::Int(1), s("r"), s("a")]),
        "E_BAD_ARG",
    );
    refused_with(
        call_v("check_permission", vec![s(""), s("r"), s("a")]),
        "E_BAD_ARG",
    );
    assert_eq!(
        call_v("check_permission", vec![s("gov-nobody"), s("r"), s("a")]),
        Ok(Value::Bool(false))
    );
    assert_eq!(
        call_v("has_permission", vec![s("gov-nobody"), s("r"), s("a")]),
        Ok(Value::Bool(false))
    );
}

#[test]
fn a_repeated_role_grant_is_unchanged_and_deleting_a_role_takes_it_back() {
    let _w = world("roles");
    let perms = Value::Array(vec![record(&[
        ("resource", s("gov-res")),
        ("actions", Value::Array(vec![s("read")])),
    ])]);
    call_v("role_create", vec![s("gov-role"), perms.clone()]).expect("create");
    let granted = call_v("role_grant", vec![s("gov-holder"), s("gov-role")]).expect("grant");
    assert_eq!(field(&granted, "status"), Some(s("granted")));
    let again = call_v("role_grant", vec![s("gov-holder"), s("gov-role")]).expect("regrant");
    assert_eq!(field(&again, "status"), Some(s("unchanged")));
    assert_eq!(
        call_v(
            "check_permission",
            vec![s("gov-holder"), s("gov-res"), s("read")]
        ),
        Ok(Value::Bool(true))
    );

    let deleted = call_v("role_delete", vec![s("gov-role")]).expect("delete");
    assert_eq!(field(&deleted, "users_unassigned"), Some(Value::Int(1)));
    assert_eq!(
        call_v(
            "check_permission",
            vec![s("gov-holder"), s("gov-res"), s("read")]
        ),
        Ok(Value::Bool(false))
    );

    // Recreating the role must not give its old holder access back.
    call_v("role_create", vec![s("gov-role"), perms]).expect("recreate");
    assert_eq!(
        call_v(
            "check_permission",
            vec![s("gov-holder"), s("gov-res"), s("read")]
        ),
        Ok(Value::Bool(false)),
        "a deleted role resurrected its old grant"
    );
}

#[test]
fn revoking_deleting_or_granting_what_is_not_there_is_not_found() {
    let _w = world("roles_missing");
    refused_with(
        call_v("role_revoke", vec![s("gov-nobody"), s("gov-missing-role")]),
        "E_NOT_FOUND",
    );
    refused_with(
        call_v("role_delete", vec![s("gov-missing-role")]),
        "E_NOT_FOUND",
    );
    refused_with(
        call_v("role_grant", vec![s("gov-holder2"), s("gov-missing-role")]),
        "E_NOT_FOUND",
    );
    refused_with(
        call_v("role_grant", vec![Value::Int(1), s("gov-missing-role")]),
        "E_BAD_ARG",
    );
}

// ---------------------------------------------------------------- audit

#[test]
fn audit_log_refuses_bad_severity_empty_action_and_control_characters() {
    let _w = world("audit_args");
    refused_with(
        call_v("audit_log", vec![s("a"), s("r"), s("d"), s("bogus")]),
        "E_BAD_ARG",
    );
    refused_with(
        call_v("audit_log", vec![s("line\nbreak"), s("r")]),
        "E_BAD_ARG",
    );
    refused_with(call_v("audit_log", vec![s("a"), s("r\tx")]), "E_BAD_ARG");
    refused_with(call_v("audit_log", vec![s(""), s("r")]), "E_BAD_ARG");
}

#[test]
fn audit_log_assertions_reach_the_chained_log_and_tampering_is_caught() {
    let w = world("audit_chain");
    let log = w.root.join("gov-audit.log");
    std::env::set_var("AETHER_AUDIT_LOG", &log);

    let logged = call_v(
        "audit_log",
        vec![
            s("gov-action"),
            s("gov-resource"),
            s("detail text"),
            s("warning"),
        ],
    )
    .expect("audit_log");
    assert_eq!(field(&logged, "status"), Some(s("logged")));

    let text = std::fs::read_to_string(&log).expect("chained log written");
    assert!(
        text.contains("\"decision\":\"asserted\"") && text.contains("gov-action"),
        "the assertion must be in the chained log, labelled as an assertion: {text}"
    );

    let verdict = call_v("audit_verify", vec![s(&log.to_string_lossy())]).expect("verify");
    assert_eq!(
        field(&verdict, "valid"),
        Some(Value::Bool(true)),
        "{verdict:?}"
    );
    assert_eq!(field(&verdict, "keyed"), Some(Value::Bool(false)));

    // A one-character edit to an entry must be seen.
    std::fs::write(&log, text.replace("gov-action", "gov-actiOn")).expect("tamper");
    let after = call_v("audit_verify", vec![s(&log.to_string_lossy())]).expect("verify");
    assert_eq!(field(&after, "valid"), Some(Value::Bool(false)));
    let why = match field(&after, "error") {
        Some(Value::Str(e)) => e,
        other => panic!("a broken chain must say why: {other:?}"),
    };
    assert!(
        why.contains("tampered") || why.contains("mismatch"),
        "{why}"
    );
    std::env::remove_var("AETHER_AUDIT_LOG");
}

#[test]
fn audit_verify_is_not_valid_for_a_missing_or_empty_log() {
    let w = world("audit_verify_missing");
    let missing = w.root.join("no-such.log");
    let verdict = call_v("audit_verify", vec![s(&missing.to_string_lossy())]).expect("answers");
    assert_eq!(field(&verdict, "valid"), Some(Value::Bool(false)));
    assert!(matches!(field(&verdict, "error"), Some(Value::Str(_))));

    let empty = w.root.join("empty.log");
    std::fs::write(&empty, "").expect("empty log");
    let verdict = call_v("audit_verify", vec![s(&empty.to_string_lossy())]).expect("answers");
    assert_eq!(
        field(&verdict, "valid"),
        Some(Value::Bool(false)),
        "an empty log attests to nothing; a truncated log must not read as valid"
    );

    refused_with(call_v("audit_verify", vec![Value::Int(7)]), "E_BAD_ARG");
}

#[test]
fn audit_tail_refuses_bad_counts_and_tells_absent_from_unreadable() {
    let w = world("audit_tail");
    refused_with(call_v("audit_tail", vec![Value::Int(-1)]), "E_BAD_ARG");
    refused_with(call_v("audit_tail", vec![Value::Int(10001)]), "E_BAD_ARG");
    refused_with(call_v("audit_tail", vec![s("5")]), "E_BAD_ARG");
    // No active log in human mode: that is a state, not an empty history.
    refused_with(call_v("audit_tail", vec![]), "E_BAD_STATE");

    let log = w.root.join("tail.log");
    std::env::set_var("AETHER_AUDIT_LOG", &log);
    assert_eq!(
        call_v("audit_tail", vec![Value::Int(5)]),
        Ok(Value::Array(Vec::new())),
        "a log not yet written holds nothing"
    );
    call_v("audit_log", vec![s("gov-tail-action"), s("gov-res")]).expect("entry");
    match call_v("audit_tail", vec![Value::Int(5)]).expect("tail") {
        Value::Array(entries) => assert_eq!(entries.len(), 1),
        other => panic!("expected an array, got {other:?}"),
    }
    std::env::remove_var("AETHER_AUDIT_LOG");
}

#[test]
fn audit_query_refuses_a_filter_it_cannot_apply() {
    let _w = world("audit_query");
    refused_with(call_v("audit_query", vec![s("login")]), "E_BAD_ARG");
    refused_with(
        call_v("audit_query", vec![record(&[("user", s("x"))])]),
        "E_BAD_ARG",
    );
    assert!(matches!(
        call_v("audit_query", vec![record(&[("action", s("gov-action"))])]),
        Ok(Value::Array(_))
    ));
}

// ---------------------------------------------------------------- SSO

#[test]
fn sso_auth_is_unimplemented_and_issues_no_token() {
    let _w = world("sso_auth");
    call_v(
        "sso_init",
        vec![s("oidc"), s("gov-app"), s("https://idp.example.com")],
    )
    .expect("configure");
    refused_with(
        call_v("sso_auth", vec![s("any-callback")]),
        "E_UNIMPLEMENTED",
    );
    refused_with(piped("sso_auth", vec![], s("any")), "E_UNIMPLEMENTED");
    assert_eq!(
        field(
            &call_v("sso_validate", vec![s("sso_token_anything")]).expect("validate"),
            "valid"
        ),
        Some(Value::Bool(false))
    );
}

#[test]
fn sso_init_refuses_an_unknown_provider_a_remote_plain_issuer_and_no_client() {
    let _w = world("sso_init");
    refused_with(
        call_v(
            "sso_init",
            vec![s("saml2"), s("app"), s("https://idp.example.com")],
        ),
        "E_BAD_ARG",
    );
    refused_with(
        call_v(
            "sso_init",
            vec![s("oidc"), s("app"), s("http://idp.example.com")],
        ),
        "E_BAD_ARG",
    );
    refused_with(
        call_v(
            "sso_init",
            vec![s("oidc"), s(""), s("https://idp.example.com")],
        ),
        "E_BAD_ARG",
    );
    let local = call_v(
        "sso_init",
        vec![s("oidc"), s("app"), s("http://localhost:8080")],
    )
    .expect("loopback issuer is allowed");
    assert_eq!(field(&local, "status"), Some(s("initialized")));
}

// ---------------------------------------------------------------- compliance

#[test]
fn compliance_is_never_compliant_while_encryption_is_unverified() {
    let _w = world("compliance_unverified");
    for r in [
        call_v("compliance_check", vec![s("GDPR")]).expect("check"),
        call_v("compliance_report", vec![s("HIPAA")]).expect("report"),
    ] {
        assert_eq!(field(&r, "compliant"), Some(Value::Bool(false)));
        match field(&r, "unverified") {
            Some(Value::Array(v)) => assert!(
                v.contains(&s("data_encryption")),
                "encryption at rest must be reported as unverified, not passed"
            ),
            other => panic!("expected an unverified list, got {other:?}"),
        }
    }
    refused_with(
        call_v("compliance_check", vec![s("NOT-A-STANDARD")]),
        "E_BAD_ARG",
    );
}

#[test]
fn compliance_scope_is_all_or_refused_direct_and_piped() {
    let _w = world("compliance_scope");
    refused_with(
        call_v("compliance_check", vec![s("GDPR"), s("one-subset")]),
        "E_BAD_ARG",
    );
    let all = call_v("compliance_check", vec![s("GDPR"), s("all")]).expect("scope all");
    assert_eq!(field(&all, "scope"), Some(s("all")));
    // A piped standard puts the scope in the first argument. It used to be
    // read from the second, so a piped scope was silently ignored.
    refused_with(
        piped("compliance_check", vec![s("one-subset")], s("GDPR")),
        "E_BAD_ARG",
    );
    let piped_all = piped("compliance_check", vec![s("all")], s("GDPR")).expect("piped");
    assert_eq!(field(&piped_all, "scope"), Some(s("all")));
}

// ---------------------------------------------------------------- plugins

#[test]
fn plugin_load_registers_metadata_only_and_refuses_builtins_it_cannot_run() {
    let w = world("plugin_load");
    let meta = manifest(
        &w.root.join("meta.plugin.toml"),
        "[plugin]\nid = \"gov-plugin\"\nname = \"Gov\"\nversion = \"1.0.0\"\n",
    );
    let loaded = call_v("plugin_load", vec![s(&meta)]).expect("metadata-only manifest");
    assert_eq!(field(&loaded, "status"), Some(s("registered")));
    refused_with(call_v("plugin_load", vec![s(&meta)]), "E_BAD_STATE");
    let unloaded = call_v("plugin_unload", vec![s("gov-plugin")]).expect("unload");
    assert_eq!(field(&unloaded, "status"), Some(s("unloaded")));

    let code = manifest(
        &w.root.join("code.plugin.toml"),
        "[plugin]\nid = \"gov-plugin-code\"\n\n[builtins]\nf = \"fn(x) => x\"\n",
    );
    refused_with(call_v("plugin_load", vec![s(&code)]), "E_UNIMPLEMENTED");
}

#[test]
fn plugin_load_refuses_option_like_and_path_like_ids_and_oversized_manifests() {
    let w = world("plugin_ids");
    for (i, id) in ["--evil", "../escape", "a/b", ""].iter().enumerate() {
        let p = manifest(
            &w.root.join(format!("bad{i}.plugin.toml")),
            &format!("[plugin]\nid = \"{id}\"\n"),
        );
        refused_with(call_v("plugin_load", vec![s(&p)]), "E_BAD_ARG");
    }
    let big = w.root.join("big.plugin.toml");
    let body = format!(
        "[plugin]\nid = \"gov-big\"\n# {}\n",
        "x".repeat(2 * 1024 * 1024)
    );
    let big = manifest(&big, &body);
    refused_with(call_v("plugin_load", vec![s(&big)]), "E_BAD_ARG");
}

#[test]
fn plugin_builtins_cannot_be_unloaded_and_unknown_plugins_are_not_found() {
    let _w = world("plugin_states");
    refused_with(
        call_v("plugin_unload", vec![s("builtin.json")]),
        "E_POLICY_DENY",
    );
    refused_with(
        call_v("plugin_unload", vec![s("gov-no-such-plugin")]),
        "E_NOT_FOUND",
    );
    refused_with(
        call_v("plugin_enable", vec![s("gov-no-such-plugin")]),
        "E_NOT_FOUND",
    );
    refused_with(
        call_v("plugin_disable", vec![s("gov-no-such-plugin")]),
        "E_NOT_FOUND",
    );
}

// ---------------------------------------------------------------- marketplace

fn package(name: &str, version: &str) -> Value {
    record(&[
        ("name", s(name)),
        ("version", s(version)),
        ("description", s("a test package")),
    ])
}

#[test]
fn publish_without_a_path_is_recorded_locally_and_says_so() {
    let _w = world("publish_local");
    let r = call_v("marketplace_publish", vec![package("gov-tool", "1.0.0")]).expect("publish");
    assert_eq!(field(&r, "status"), Some(s("recorded_locally")));
    assert_eq!(field(&r, "target"), Some(s("local")));
}

#[test]
fn publish_refuses_unknown_fields_wrong_types_and_bad_names() {
    let _w = world("publish_args");
    let unknown = record(&[
        ("name", s("gov-tool")),
        ("version", s("1.0.0")),
        ("bogus", Value::Int(1)),
    ]);
    refused_with(call_v("marketplace_publish", vec![unknown]), "E_BAD_ARG");
    let wrong = record(&[
        ("name", s("gov-tool")),
        ("version", s("1.0.0")),
        ("description", Value::Int(3)),
    ]);
    refused_with(call_v("marketplace_publish", vec![wrong]), "E_BAD_ARG");
    refused_with(
        call_v("marketplace_publish", vec![package("gov-tool", "1.0/../x")]),
        "E_BAD_ARG",
    );
}

#[test]
fn publish_of_a_file_outside_the_workspace_is_refused_before_any_request() {
    let w = world("publish_outside");
    let outside = w.root.join("outside");
    std::fs::create_dir_all(&outside).expect("outside dir");
    let file = outside.join("pkg.tar.gz");
    std::fs::write(&file, b"not really an archive").expect("file");
    let opts = record(&[
        ("name", s("gov-tool")),
        ("version", s("1.0.0")),
        ("path", s(&file.to_string_lossy())),
    ]);
    match call_v("marketplace_publish", vec![opts]) {
        Ok(v) => panic!("a file outside the workspace was published: {v:?}"),
        Err(e) => assert!(e.contains("outside the workspace"), "{e}"),
    }
}

#[test]
fn network_marketplace_calls_are_refused_by_the_budget_in_agent_mode() {
    let w = world("net_budget");
    agent(&w);
    std::env::set_var("AETHER_MAX_NET", "0");
    refused_with(
        call_v("marketplace_search", vec![s("gov")]),
        "E_BUDGET_EXCEEDED",
    );
    refused_with(
        call_v("marketplace_info", vec![s("gov-tool")]),
        "E_BUDGET_EXCEEDED",
    );
    let inside = w.root.join("ws").join("pkg.tar.gz");
    std::fs::write(&inside, b"placeholder").expect("inside file");
    let opts = record(&[
        ("name", s("gov-tool")),
        ("version", s("1.0.0")),
        ("path", s(&inside.to_string_lossy())),
    ]);
    refused_with(
        call_v("marketplace_publish", vec![opts]),
        "E_BUDGET_EXCEEDED",
    );
    // Nothing is installed, so an update has nothing to ask the registry about.
    assert_eq!(
        call_v("marketplace_update", vec![]),
        Ok(Value::Array(Vec::new()))
    );
    std::env::remove_var("AETHER_MAX_NET");
}

#[test]
fn marketplace_names_are_single_safe_segments_and_refused_before_any_request() {
    let _w = world("names");
    refused_with(
        call_v("marketplace_info", vec![s("../escape")]),
        "E_BAD_ARG",
    );
    refused_with(call_v("marketplace_install", vec![s("a/b")]), "E_BAD_ARG");
    refused_with(
        call_v("marketplace_install", vec![s("gov-tool"), s("1/../../x")]),
        "E_BAD_ARG",
    );
    refused_with(call_v("marketplace_uninstall", vec![s("-rf")]), "E_BAD_ARG");
    refused_with(
        call_v("marketplace_rate", vec![s(""), Value::Int(3)]),
        "E_BAD_ARG",
    );
}

#[test]
fn search_refuses_a_query_of_the_wrong_shape_before_any_request() {
    let _w = world("search_shape");
    refused_with(
        call_v("marketplace_search", vec![Value::Int(42)]),
        "E_BAD_ARG",
    );
    refused_with(
        call_v("marketplace_search", vec![record(&[("bogus", s("x"))])]),
        "E_BAD_ARG",
    );
}

#[test]
fn rate_and_uninstall_follow_the_local_catalogue() {
    let _w = world("rate");
    call_v(
        "marketplace_publish",
        vec![package("gov-rate-tool", "1.0.0")],
    )
    .expect("publish");
    refused_with(
        call_v("marketplace_uninstall", vec![s("gov-rate-tool")]),
        "E_BAD_STATE",
    );
    refused_with(
        call_v("marketplace_rate", vec![s("gov-rate-tool"), Value::Int(9)]),
        "E_BAD_ARG",
    );
    let rated = call_v("marketplace_rate", vec![s("gov-rate-tool"), Value::Int(4)]).expect("rate");
    assert_eq!(field(&rated, "scope"), Some(s("local_cache")));
    refused_with(
        call_v(
            "marketplace_rate",
            vec![s("gov-missing-tool"), Value::Int(4)],
        ),
        "E_NOT_FOUND",
    );
    refused_with(
        call_v("marketplace_uninstall", vec![s("gov-missing-tool")]),
        "E_NOT_FOUND",
    );
}

#[test]
fn update_checks_its_arguments_and_installed_state_before_any_request() {
    let _w = world("update");
    call_v(
        "marketplace_publish",
        vec![package("gov-upd-tool", "1.0.0")],
    )
    .expect("publish");
    refused_with(
        call_v("marketplace_update", vec![s("gov-upd-tool")]),
        "E_BAD_STATE",
    );
    refused_with(
        call_v("marketplace_update", vec![s("gov-missing-tool")]),
        "E_NOT_FOUND",
    );
    refused_with(
        call_v("marketplace_update", vec![Value::Int(3)]),
        "E_BAD_ARG",
    );
    assert_eq!(
        call_v("marketplace_update", vec![]),
        Ok(Value::Array(Vec::new()))
    );
}

// ---------------------------------------------------------------- registry client

#[test]
fn registry_download_refuses_a_name_that_is_not_one_path_segment() {
    let _w = world("registry_names");
    let client = aethershell::packages::RegistryClient::default();
    for (name, version) in [
        ("../escape", "1.0.0"),
        ("/abs", "1.0.0"),
        ("ok", "1/2"),
        ("", "1"),
    ] {
        let err = client
            .download_package(name, version)
            .expect_err("an unsafe segment must be refused before any request");
        assert!(err.to_string().contains("safe path segment"), "{err}");
    }
}

#[test]
fn registry_archives_with_links_or_escaping_entries_are_refused() {
    let w = world("registry_tar");
    let dest = w.root.join("unpack");
    std::fs::create_dir_all(&dest).expect("dest");

    // A symlink entry: it can point anywhere, so the whole archive is refused.
    let link_path = w.root.join("link.tar.gz");
    {
        let gz = flate2::write::GzEncoder::new(
            std::fs::File::create(&link_path).expect("create"),
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(gz);
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_size(0);
        h.set_path("pkg/link").expect("path");
        h.set_link_name("/").expect("link");
        h.set_cksum();
        builder.append(&h, std::io::empty()).expect("append link");
        builder
            .into_inner()
            .expect("finish")
            .finish()
            .expect("gzip");
    }
    let client = aethershell::packages::RegistryClient::default();
    let err = client
        .extract_tarball(&link_path, &dest)
        .expect_err("a link entry must be refused");
    assert!(err.to_string().contains("link entry"), "{err}");

    // An entry that would leave the destination.
    let escape_path = w.root.join("escape.tar.gz");
    {
        let gz = flate2::write::GzEncoder::new(
            std::fs::File::create(&escape_path).expect("create"),
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(gz);
        let data = b"escaped";
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        // Written as raw bytes: the header builder would not let us say `..`.
        let raw = h.as_mut_bytes();
        let name = b"../escaped.txt";
        raw[..name.len()].copy_from_slice(name);
        h.set_cksum();
        builder.append(&h, &data[..]).expect("append escape");
        builder
            .into_inner()
            .expect("finish")
            .finish()
            .expect("gzip");
    }
    assert!(client.extract_tarball(&escape_path, &dest).is_err());
    assert!(
        !w.root.join("escaped.txt").exists() && !dest.join("..").join("escaped.txt").exists(),
        "an entry escaped the destination"
    );
}
