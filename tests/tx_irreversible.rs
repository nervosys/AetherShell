//! A builtin a rollback cannot undo is refused inside a transaction.
//!
//! `tx_rollback` restores only what `tx::snapshot` recorded. Before
//! `tx::IRREVERSIBLE`, 229 implementations classified as more than a read ran
//! inside a transaction with nothing recorded (git_commit, docker_run, kill,
//! web_post, terraform_apply and the like), so a rollback answered as if it
//! had undone them. The dispatcher now refuses every listed implementation
//! while a transaction is active, before validation or the body runs.
//!
//! The list is pinned here: every implementation whose effect class is more
//! than `ReadLocal` must be refused centrally, refuse or snapshot in its own
//! body (found by a scan of its dispatch row and the function the row calls,
//! or listed in `SELF_COVERED` with where it does so), or be listed in
//! `HARMLESS` with the reason a rollback need not undo it.

use iron_shell::builtins::{call, is_dispatched, BUILTIN_LOOKUP, FALLBACK_BUILTINS};
use iron_shell::env::Env;
use iron_shell::safety::{effect_of, Effect};
use iron_shell::signature::signature_of;
use iron_shell::tx::IRREVERSIBLE;
use iron_shell::value::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

/// More than a read, but nothing a rollback should undo.
const HARMLESS: &[(&str, &[&str])] = &[
    (
        "only reads or inspects: local tools and files, cluster, container and package state, HTTP GETs, dialogs that answer a value",
        &[
            "ai-backends", "ai_backends", "ai_detect", "ai_gateway", "ai_supported_conversions",
            "ansible_inventory", "crypto_decrypt", "crypto_encrypt",
            "crypto_generate_key", "curl", "docker_compose_ps", "docker_inspect",
            "docker_logs", "docker_top", "eslint", "fetch", "fzf", "gdb_bt", "gh_repo",
            "gui_dialog_file_open", "gui_dialog_file_save", "gui_dialog_folder",
            "gui_dialog_input", "gui_dialog_message", "helm_list", "helm_repos", "helm_search",
            "helm_status", "ifconfig", "input_editor", "input_read_password", "jq_filter",
            "k8s_cluster_info", "k8s_configmaps", "k8s_contexts", "k8s_deployments",
            "k8s_describe", "k8s_events", "k8s_get", "k8s_ingresses", "k8s_logs",
            "k8s_namespaces", "k8s_nodes", "k8s_pods", "k8s_rollout_status", "k8s_secrets",
            "k8s_services", "k8s_top_nodes", "k8s_top_pods", "lnav", "marketplace_info",
            "marketplace_search", "marketplace_update", "mcp-detect", "mcp-servers",
            "mcp_client", "monitor_tcpdump", "multitail", "mypy", "net_arp", "net_bandwidth",
            "net_connections", "net_ip", "net_ports", "net_route", "net_stats",
            "pkg_changelog", "podman_logs", "rga", "ruff_check",
            "scrape", "shfmt", "skopeo_inspect", "sudo_check", "syntax_categories",
            "syntax_get", "syntax_list", "syntax_search", "terraform_output", "terraform_plan",
            "terraform_state", "terraform_validate", "trivy_image", "trivy_scan",
            "web_build_query", "web_check_url", "web_cookies", "web_decode_url",
            "web_encode_url", "web_extract_emails", "web_extract_phones", "web_headers",
            "web_html_to_markdown", "web_html_to_text", "web_json_get", "web_json_path",
            "web_parse_query", "web_parse_url", "web_robots_txt", "web_sitemap", "web_xpath",
            "vault_conversions", "vault_models", "yq",
        ],
    ),
    (
        "changes only this process's memory, which a rollback never restores, as it never restores a variable",
        &[
            "ai_local_unload", "cache_ai", "cache_clear_ai", "cd", "env_set", "env_unset", "feature_disable",
            "feature_enable", "feature_set", "set_env", "telemetry_disable", "telemetry_enable",
            "telemetry_reset",
        ],
    ),
    (
        "is the transaction stack itself, this process's memory; refusing it would strand an open transaction",
        &[
            "tx_begin", "tx_commit", "tx_rollback", "tx_rollback_to", "tx_savepoint",
        ],
    ),
    (
        "appends to the audit trail, which a rollback must not erase",
        &["audit_log"],
    ),
    (
        "makes a fresh path in the temporary directory, not a caller's file",
        &["fs_tempdir", "fs_tempfile"],
    ),
];

/// Refuse in a transaction, or snapshot what they change, deeper than the
/// scan below reads; each with where.
const SELF_COVERED: &[(&str, &str)] = &[
    ("acl_set", "refuses (acl_mount.rs)"),
    ("apt", "refuses (builtins.rs)"),
    ("at_remove", "refuses (scheduler.rs)"),
    ("at_schedule", "refuses (scheduler.rs)"),
    ("backup_file", "snapshots (builtins.rs)"),
    ("chmod", "snapshots (builtins.rs)"),
    ("cron_add", "refuses (scheduler.rs)"),
    ("cron_disable", "refuses (builtins.rs)"),
    ("cron_enable", "refuses (builtins.rs)"),
    ("cron_remove", "refuses (builtins.rs)"),
    ("db_json_to_sqlite", "snapshots (builtins.rs)"),
    ("db_kv_delete", "snapshots (builtins.rs)"),
    ("db_kv_set", "snapshots (builtins.rs)"),
    ("db_kv_store", "snapshots (builtins.rs)"),
    ("db_sqlite_create_table", "snapshots (builtins.rs)"),
    ("db_sqlite_delete", "snapshots (builtins.rs)"),
    (
        "db_sqlite_import",
        "snapshots the database file (bi_db_sqlite_exec)",
    ),
    ("db_sqlite_insert", "snapshots (builtins.rs)"),
    ("db_sqlite_update", "snapshots (builtins.rs)"),
    ("db_sqlite_vacuum", "snapshots (builtins.rs)"),
    ("entr", "refuses (tool_runs.rs)"),
    ("fs_mount", "refuses (acl_mount.rs)"),
    ("fs_unmount", "refuses (acl_mount.rs)"),
    (
        "group_add",
        "refuses (accounts.rs, accounts/windows.rs, accounts/macos.rs)",
    ),
    (
        "group_del",
        "refuses (accounts.rs, accounts/windows.rs, accounts/macos.rs)",
    ),
    (
        "group_mod",
        "refuses (accounts.rs, accounts/windows.rs, accounts/macos.rs)",
    ),
    ("iperf3_server", "refuses (tool_runs.rs)"),
    ("net_close", "refuses (tcp_sockets.rs)"),
    ("net_connect", "refuses (tcp_sockets.rs)"),
    ("net_listen", "refuses (tcp_sockets.rs)"),
    ("net_recv", "refuses (tcp_sockets.rs)"),
    ("net_send", "refuses (tcp_sockets.rs)"),
    ("nodemon", "refuses (tool_runs.rs)"),
    ("perm_set", "snapshots (builtins.rs)"),
    ("pkg_add_source", "refuses (pkg_admin.rs)"),
    ("pkg_autoremove", "refuses (pkg_admin.rs)"),
    ("pkg_clean", "refuses (pkg_admin.rs)"),
    ("pkg_hold", "refuses (pkg_admin.rs)"),
    ("pkg_remove", "refuses (builtins.rs)"),
    ("pkg_unhold", "refuses (pkg_admin.rs)"),
    ("pkg_update", "refuses (builtins.rs)"),
    ("pkg_upgrade", "refuses (builtins.rs)"),
    ("sudo_exec", "refuses (services.rs)"),
    ("svc_create", "refuses (services.rs)"),
    ("svc_delete", "refuses (services.rs)"),
    ("touch", "snapshots (builtins.rs)"),
    (
        "user_add",
        "refuses (accounts.rs, accounts/windows.rs, accounts/macos.rs)",
    ),
    (
        "user_del",
        "refuses (accounts.rs, accounts/windows.rs, accounts/macos.rs)",
    ),
    ("user_lock", "refuses (builtins.rs)"),
    (
        "user_mod",
        "refuses (accounts.rs, accounts/windows.rs, accounts/macos.rs)",
    ),
    (
        "user_passwd",
        "refuses (accounts.rs, accounts/windows.rs, accounts/macos.rs)",
    ),
    ("user_unlock", "refuses (builtins.rs)"),
    ("watchexec", "refuses (tool_runs.rs)"),
];

/// Text that refuses a transaction or records a pre-image.
const TX_MARKERS: &[&str] = &["tx::snapshot", "refuse_in_transaction(", "tx::is_active()"];

fn source() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builtins.rs"),
    )
    .expect("src/builtins.rs is readable")
}

/// `BUILTIN_DISPATCH` rows, one per line, as the other dispatch scanners read
/// them.
fn dispatch_rows(src: &str) -> Vec<String> {
    let lines: Vec<&str> = src.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.contains("BUILTIN_DISPATCH") && l.contains("&["))
        .expect("BUILTIN_DISPATCH must be findable");
    let mut rows = Vec::new();
    for line in &lines[start + 1..] {
        let t = line.trim();
        if t == "];" {
            break;
        }
        let t = t.strip_prefix("move ").unwrap_or(t);
        if t.starts_with('|') {
            rows.push(t.to_string());
        }
    }
    rows
}

/// The body of `fn name(` or `pub fn name(` in `src`, by brace matching.
fn body_of<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let at = [format!("\nfn {name}("), format!("\npub fn {name}(")]
        .iter()
        .find_map(|head| src.find(head.as_str()))?;
    let open = at + src[at..].find('{')?;
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&src[open..open + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether the row, or the function it calls, refuses or snapshots.
fn covers_itself(src: &str, row: &str) -> bool {
    let marked = |text: &str| TX_MARKERS.iter().any(|m| text.contains(m));
    marked(row)
        || row
            .split_once("bi_")
            .and_then(|(_, rest)| rest.split_once('('))
            .and_then(|(f, _)| body_of(src, &format!("bi_{f}")))
            .is_some_and(marked)
}

fn more_than_a_read(name: &str) -> bool {
    !matches!(effect_of(name), Effect::Pure | Effect::ReadLocal)
}

/// Fallback-only names (served by the `match` in `call_with_input_inner`, not
/// by `BUILTIN_LOOKUP`), grouped by the function their arm calls: one
/// implementation per group, as `implementations` groups the table.
fn fallback_implementations() -> BTreeMap<&'static str, Vec<&'static str>> {
    let mut by_callee: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    for &(name, callee) in FALLBACK_BUILTINS {
        if !BUILTIN_LOOKUP.contains_key(name) {
            by_callee.entry(callee).or_default().push(name);
        }
    }
    by_callee
}

/// Every implementation, as its names.
fn implementations() -> BTreeMap<usize, Vec<&'static str>> {
    let mut by_index: BTreeMap<usize, Vec<&'static str>> = BTreeMap::new();
    for (name, idx) in BUILTIN_LOOKUP.iter() {
        by_index.entry(*idx).or_default().push(name);
    }
    by_index
}

fn harmless() -> impl Iterator<Item = &'static str> {
    HARMLESS.iter().flat_map(|(_, names)| names.iter().copied())
}

#[test]
fn the_lists_are_sorted_dispatched_and_disjoint() {
    assert!(
        IRREVERSIBLE.windows(2).all(|w| w[0] < w[1]),
        "tx::IRREVERSIBLE must be sorted and unique"
    );
    let mut seen = BTreeSet::new();
    let covered = SELF_COVERED.iter().map(|(n, _)| *n);
    for name in IRREVERSIBLE
        .iter()
        .copied()
        .chain(harmless())
        .chain(covered)
    {
        assert!(is_dispatched(name), "{name} is not dispatched");
        assert!(seen.insert(name), "{name} is listed twice");
    }
}

#[test]
fn every_builtin_that_does_more_than_read_is_accounted_for() {
    let src = source();
    let rows = dispatch_rows(&src);
    let accounted: BTreeSet<&str> = IRREVERSIBLE
        .iter()
        .copied()
        .chain(harmless())
        .chain(SELF_COVERED.iter().map(|(n, _)| *n))
        .collect();
    let mut missing = Vec::new();
    let mut stale = Vec::new();
    for (idx, mut names) in implementations() {
        names.sort();
        let listed = names.iter().any(|n| accounted.contains(n));
        let acts = names.iter().any(|n| more_than_a_read(n));
        let row = rows.get(idx).map(String::as_str).unwrap_or("");
        if acts && !listed && !covers_itself(&src, row) {
            missing.push(names.join("/"));
        }
        if listed && !acts {
            stale.extend(names.iter().filter(|n| accounted.contains(*n)).copied());
        }
    }
    for (callee, mut names) in fallback_implementations() {
        names.sort();
        let listed = names.iter().any(|n| accounted.contains(*n));
        let acts = names.iter().any(|n| more_than_a_read(n));
        let body = body_of(&src, callee).unwrap_or("");
        let refuses_itself = TX_MARKERS.iter().any(|m| body.contains(m));
        if acts && !listed && !refuses_itself {
            missing.push(names.join("/"));
        }
        if listed && !acts {
            stale.extend(names.iter().filter(|n| accounted.contains(*n)).copied());
        }
    }
    assert!(
        missing.is_empty(),
        "these do more than read, and neither refuse in a transaction, snapshot, \
         nor are listed in tx::IRREVERSIBLE or HARMLESS here:\n{}",
        missing.join("\n")
    );
    assert!(
        stale.is_empty(),
        "listed, but now classified Pure or ReadLocal: {stale:?}"
    );
}

/// Run `f` inside a transaction in a private workspace, then roll it back.
fn in_transaction(f: impl FnOnce(&mut Env)) {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let w = std::env::temp_dir().join(format!("ae_tx_irrev_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("IRONSHELL_WORKSPACE", &w);
    std::env::set_var("IRONSHELL_SESSION_DIR", w.join("session"));
    std::env::remove_var("IRONSHELL_MODE");
    let mut env = Env::new();
    call("tx_begin", vec![], &mut env).expect("tx_begin");
    f(&mut env);
    let _ = call("tx_rollback", vec![], &mut env);
    std::env::remove_var("IRONSHELL_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
}

fn needs_an_argument(name: &str) -> bool {
    signature_of(name).is_some_and(|s| s.params.iter().any(|p| p.required))
}

fn refused(got: &anyhow::Result<Value>) -> bool {
    matches!(got, Err(e) if e.to_string().contains("E_POLICY_DENY")
        && e.to_string().contains("rollback cannot undo"))
}

#[test]
fn every_listed_builtin_is_refused_inside_a_transaction_before_it_runs() {
    // Only builtins with a required parameter are called, with none: should
    // the refusal ever go, validation still stops them at the missing
    // argument, so no body runs.
    in_transaction(|env| {
        let mut ran = Vec::new();
        let mut called = 0;
        for name in IRREVERSIBLE.iter().filter(|n| needs_an_argument(n)) {
            called += 1;
            let got = call(name, vec![], env);
            if !refused(&got) {
                ran.push(format!("{name}: {got:?}"));
            }
        }
        assert!(ran.is_empty(), "not refused:\n{}", ran.join("\n"));
        assert!(
            called > 100,
            "only {called} were called; the filter ate the check"
        );
    });
}

#[test]
fn another_spelling_is_refused_and_a_pure_builtin_still_runs() {
    let listed: BTreeSet<&str> = IRREVERSIBLE.iter().copied().collect();
    let aliases: Vec<&str> = BUILTIN_LOOKUP
        .iter()
        .filter(|(name, idx)| {
            !listed.contains(*name)
                && needs_an_argument(name)
                && IRREVERSIBLE
                    .iter()
                    .any(|l| BUILTIN_LOOKUP.get(l) == Some(*idx))
        })
        .map(|(name, _)| *name)
        .collect();
    // A fallback-only spelling of a listed arm is refused by its callee.
    let fallback_aliases: Vec<&str> = FALLBACK_BUILTINS
        .iter()
        .filter(|&&(name, callee)| {
            !listed.contains(name)
                && !BUILTIN_LOOKUP.contains_key(name)
                && needs_an_argument(name)
                && FALLBACK_BUILTINS
                    .iter()
                    .any(|&(l, c)| listed.contains(l) && c == callee)
        })
        .map(|&(name, _)| name)
        .collect();
    let aliases: Vec<&str> = aliases.into_iter().chain(fallback_aliases).collect();
    assert!(
        !aliases.is_empty(),
        "no unlisted spelling of a listed builtin found"
    );
    in_transaction(|env| {
        for alias in &aliases {
            let got = call(alias, vec![], env);
            assert!(refused(&got), "{alias}: {got:?}");
        }
        let got = call("upper", vec![Value::Str("x".into())], env);
        assert!(got.is_ok(), "a pure builtin still runs: {got:?}");
    });
    // Outside a transaction the refusal is gone: the same call fails on its
    // missing argument instead.
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let got = call(aliases[0], vec![], &mut Env::new());
    assert!(got.is_err() && !refused(&got), "{}: {got:?}", aliases[0]);
}
