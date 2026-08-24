//! Every `module.function` advertised in `modules.rs` must point at a builtin
//! that actually exists.
//!
//! `modules.rs` maps a user-facing name to a builtin name (`("verify",
//! "crypto_verify")`). Nothing checked that the right-hand side resolves, so
//! aliases drifted from the dispatch table and `crypto.verify(...)` failed with
//! `unknown builtin: crypto_verify` — an advertised API that cannot be called.
//!
//! The failure is *closed* (an error, and `Value::Error` is falsy), so this is a
//! correctness bug rather than a vulnerability. It still matters for the
//! security-relevant modules: `perm.acl_get` and the `sso.*` calls are in the
//! allowlist below, meaning AetherShell advertises an access-control surface it
//! does not implement. Nothing should be written that depends on those
//! returning a meaningful answer.
//!
//! The whole `rbac.*` module used to be in that list -- all seven entries, so
//! `rbac.check(...)` answered `unknown builtin` while the README documented it
//! as working. The implementations existed under their own names the whole
//! time; the aliases now point at them, and `rbac.permissions` was withdrawn
//! because nothing implements it.
//!
//! As of 2026-07-30: 919 aliases, 71 dangling; as of 2026-08-24 the allowlist
//! is 64 (the `rbac.*` seven, wired up or withdrawn). The allowlist is the debt;
//! it should only ever shrink. Wiring up or withdrawing an entry means deleting
//! its line here.

use aethershell::builtins::BUILTIN_LOOKUP;
use aethershell::modules::all_modules;
use aethershell::value::Value;

/// Builtins reachable through the fallback `match` at the end of
/// `builtins::call` but absent from `BUILTIN_LOOKUP`.
///
/// Dispatch has two layers: `BUILTIN_LOOKUP` maps a name to an index into a
/// function-pointer table, and a trailing `match` handles the rest. Only the
/// first is public, so these three resolve at runtime yet look missing to any
/// consumer that enumerates `BUILTIN_LOOKUP` — including `agent_api`'s dynamic
/// discovery, whose whole purpose is telling an agent what it may call. Worth
/// unifying; until then, they are dispatchable and must not fail this test.
const DISPATCHABLE_OUTSIDE_LOOKUP: &[&str] = &["agent", "ai", "swarm"];

/// Builtin names referenced by a module alias that no builtin implements.
///
/// These are genuinely unimplemented features, not typos — the typos (nine of
/// them, e.g. `crypto_key_generate` for `crypto_generate_key` and
/// `gui_window_close` for `gui_close_window`) were corrected rather than
/// allowlisted.
const KNOWN_UNIMPLEMENTED: &[&str] = &[
    "ai_detect",
    "audit_retention",
    "audit_stream",
    "clipboard_has_image",
    "clipboard_watch",
    "crypto_jwt_encode",
    "crypto_key_derive",
    "db_kv_close",
    "db_kv_list",
    "db_kv_open",
    "db_redis_cmd",
    "db_redis_connect",
    "db_sqlite_close",
    "db_sqlite_commit",
    "db_sqlite_rollback",
    "db_sqlite_transaction",
    "embed",
    "gui_click",
    "gui_color_at",
    "gui_double_click",
    "gui_drag",
    "gui_hotkey",
    "gui_key_down",
    "gui_key_up",
    "gui_ocr_region",
    "gui_pixel_search",
    "gui_right_click",
    "gui_scroll",
    "gui_wait_image",
    "gui_window_list",
    "input_number",
    "input_path",
    "input_text",
    "perm_acl_get",
    "perm_acl_set",
    "perm_owner_get",
    "perm_owner_set",
    "pkg_lock",
    "pkg_outdated",
    "pkg_repo_add",
    "pkg_repo_remove",
    "pkg_repos",
    "pkg_uninstall",
    "pkg_unlock",
    "sso_providers",
    "sso_refresh",
    "sso_user_info",
    "web_back",
    "web_click",
    "web_eval",
    "web_extract",
    "web_extract_all",
    "web_form_fill",
    "web_forward",
    "web_links",
    "web_navigate",
    "web_pdf",
    "web_refresh",
    "web_screenshot",
    "web_scroll",
    "web_select",
    "web_table_extract",
    "web_type",
    "web_wait",
];

/// Whether `name` can actually be dispatched by `builtins::call`.
fn resolves(name: &str) -> bool {
    BUILTIN_LOOKUP.contains_key(name) || DISPATCHABLE_OUTSIDE_LOOKUP.contains(&name)
}

/// Walk every module and yield `(module, exposed_name, builtin_name)`.
fn aliases() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for (module_name, value) in all_modules() {
        let Value::Record(fields) = value else {
            panic!("module {module_name} is not a Record");
        };
        for (exposed, target) in fields {
            if let Value::Builtin(b) = target {
                out.push((module_name.to_string(), exposed, b.name));
            }
        }
    }
    out
}

#[test]
fn no_new_dangling_module_aliases() {
    let dangling: Vec<_> = aliases()
        .into_iter()
        .filter(|(_, _, target)| !resolves(target))
        .filter(|(_, _, target)| !KNOWN_UNIMPLEMENTED.contains(&target.as_str()))
        .collect();

    assert!(
        dangling.is_empty(),
        "module alias(es) point at builtins that do not exist — calling these \
         yields `unknown builtin`:\n{}",
        dangling
            .iter()
            .map(|(m, e, t)| format!("  {m}.{e} -> {t}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The allowlist must not rot: an entry that has since been implemented should
/// be deleted, so the list keeps measuring real debt rather than accumulating
/// stale names.
#[test]
fn allowlist_contains_no_entries_that_now_exist() {
    let stale: Vec<_> = KNOWN_UNIMPLEMENTED
        .iter()
        .filter(|name| resolves(name))
        .collect();

    assert!(
        stale.is_empty(),
        "these are implemented now — remove them from KNOWN_UNIMPLEMENTED: {stale:?}"
    );
}

/// Guard the fix itself: the nine corrected aliases must keep resolving.
#[test]
fn previously_broken_aliases_now_resolve() {
    for (module, exposed) in [
        ("crypto", "verify"),
        ("crypto", "key_generate"),
        ("gui", "window_close"),
        ("gui", "window_focus"),
        ("gui", "window_maximize"),
        ("gui", "window_minimize"),
        ("gui", "window_move"),
        ("gui", "window_resize"),
        ("input", "multi_select"),
    ] {
        let found = aliases()
            .into_iter()
            .find(|(m, e, _)| m == module && e == exposed);
        let Some((_, _, target)) = found else {
            // The alias may legitimately be renamed later; that is not this
            // test's business, so only assert when it is still exposed.
            continue;
        };
        assert!(
            resolves(&target),
            "{module}.{exposed} regressed to a dangling target: {target}"
        );
    }
}
