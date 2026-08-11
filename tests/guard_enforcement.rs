//! Does a classified builtin actually *reach* the policy engine?
//!
//! `effect_of` is an advertisement; `guard` is the control. 6.0.0 classified 306
//! process-spawning builtins, which improved what the ontology told an agent
//! without changing what the shell would let one do — 305 of the 306 never
//! called a guard. This file asserts the control is now wired, and keeps the
//! self-guarding list honest against the source.

use aethershell::safety::{self, Effect};
use aethershell::value::Value;
use std::sync::Mutex;

/// `AETHER_MODE` is process-global, and these tests switch it.
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

const SOURCE: &str = include_str!("../src/builtins.rs");

/// Extract `fn bi_<name>` bodies by brace matching — the same evidence-reading
/// approach as `tests/effect_ratchet.rs`, for the same reason: a list derived
/// from names would drift from what the code does.
fn builtin_bodies() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = SOURCE.as_bytes();
    let mut search = 0usize;
    while let Some(rel) = SOURCE[search..].find("fn bi_") {
        let start = search + rel;
        search = start + 6;
        let rest = &SOURCE[start + 3..];
        let name_end = match rest.find(|c: char| !(c.is_alphanumeric() || c == '_')) {
            Some(i) => i,
            None => continue,
        };
        let builtin = match rest[..name_end].strip_prefix("bi_") {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => continue,
        };
        let brace = match SOURCE[start..].find('{') {
            Some(i) => start + i,
            None => continue,
        };
        let (mut depth, mut i, mut in_str, mut esc) = (0i32, brace, false, false);
        while i < bytes.len() {
            let c = bytes[i] as char;
            if in_str {
                if c == '\\' && !esc {
                    esc = true;
                } else {
                    if c == '"' && !esc {
                        in_str = false;
                    }
                    esc = false;
                }
            } else if c == '"' {
                in_str = true;
            } else if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            i += 1;
        }
        if depth == 0 && i > brace {
            out.push((builtin, SOURCE[brace..=i.min(bytes.len() - 1)].to_string()));
        }
    }
    out
}

/// Whether a body enforces policy for itself — by calling a `guard_*` helper,
/// or by consulting the approval system directly.
///
/// The second form matters. `apply` never calls `guard`; it gates a whole plan
/// on one plan-derived token. A detector that only looked for `guard(` left it
/// out of `SELF_GUARDED`, so the dispatcher demanded a second unrelated token
/// and broke the documented plan/apply flow.
fn enforces_policy_itself(body: &str) -> bool {
    for marker in ["guard", "is_approved", "is_token_approved"] {
        let mut idx = 0;
        while let Some(rel) = body[idx..].find(marker) {
            let at = idx + rel;
            let rest = &body[at + marker.len()..];
            let after = rest.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_');
            if after.starts_with('(') {
                return true;
            }
            idx = at + marker.len();
        }
    }
    false
}

#[test]
fn the_self_guarded_list_matches_the_source() {
    // If a builtin gains or loses its own guard, this list must move with it.
    // Otherwise the dispatcher either double-guards (charging the governor
    // twice) or skips a builtin that no longer guards itself — a silent hole.
    let actual: std::collections::BTreeSet<String> = builtin_bodies()
        .into_iter()
        .filter(|(_, body)| enforces_policy_itself(body))
        .map(|(name, _)| name)
        .collect();
    let declared: std::collections::BTreeSet<String> =
        safety::SELF_GUARDED.iter().map(|s| s.to_string()).collect();

    let missing: Vec<&String> = actual.difference(&declared).collect();
    let extra: Vec<&String> = declared.difference(&actual).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "safety::SELF_GUARDED is out of step with src/builtins.rs.\n\
         guards itself but not listed (would be guarded twice): {missing:?}\n\
         listed but no longer guards itself (would be skipped): {extra:?}"
    );
}

#[test]
fn a_builtin_that_runs_its_own_approval_flow_is_not_double_gated() {
    // Regression. `apply` gates a whole plan on one plan-derived token and
    // returns a `needs_approval` record carrying it. Central enforcement first
    // shipped detecting only `guard(`, so `apply` was gated generically as
    // `Exec`: it demanded a second, unrelated token and never reached the code
    // that hands back the plan token. A working approval flow became a dead end.
    let _g = lock();
    assert!(
        safety::SELF_GUARDED.contains(&"apply"),
        "apply enforces its own policy and must be skipped centrally"
    );

    std::env::set_var("AETHER_MODE", "agent");
    let mut env = aethershell::env::Env::new();
    let result = aethershell::builtins::call("apply", vec![Value::Array(vec![])], &mut env);
    std::env::remove_var("AETHER_MODE");

    // It must reach apply's own logic rather than being refused by the
    // dispatcher — whatever apply then decides about an empty plan.
    match result {
        Ok(_) => {}
        Err(e) => {
            let text = e.to_string();
            assert!(
                !text.contains("E_NEEDS_APPROVAL"),
                "apply must not be gated by the dispatcher: {text}"
            );
        }
    }
}

#[test]
fn the_self_guarded_list_is_sorted_and_unique() {
    let mut sorted: Vec<&str> = safety::SELF_GUARDED.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(safety::SELF_GUARDED.to_vec(), sorted);
}

#[test]
fn a_destructive_builtin_is_now_stopped_in_agent_mode() {
    // The behaviour 6.0.0 advertised but did not enforce. `git_clean` deletes
    // untracked files; before central enforcement it ran unguarded whatever its
    // effect class said.
    let _g = lock();
    assert_eq!(safety::effect_of("git_clean"), Effect::Destructive);

    std::env::set_var("AETHER_MODE", "agent");
    let denied = safety::guard_dispatch("git_clean", vec![]);
    std::env::remove_var("AETHER_MODE");

    let err = denied.expect_err("a destructive builtin must not run unguarded in agent mode");
    assert_eq!(err.code, safety::ErrorCode::NeedsApproval);
    assert!(
        err.approval.is_some(),
        "an approval path must be offered, not a flat refusal"
    );
}

#[test]
fn the_human_surface_is_unchanged() {
    // The dual-surface split: a human at a REPL is not gated by any of this.
    let _g = lock();
    std::env::remove_var("AETHER_MODE");
    std::env::remove_var("AETHER_AGENT");
    assert!(
        safety::guard_dispatch("git_clean", vec![]).is_ok(),
        "human mode stays default-allow"
    );
}

#[test]
fn read_only_builtins_are_not_gated_even_in_agent_mode() {
    // Enforcement must not tax exploration. 140 of the 306 are read-only
    // wrappers an agent leans on constantly; gating them would buy nothing.
    let _g = lock();
    std::env::set_var("AETHER_MODE", "agent");
    let results: Vec<_> = ["git_status", "pkg_list", "platform_cpu", "hw_gpu"]
        .iter()
        .map(|n| (*n, safety::guard_dispatch(n, vec![]).is_ok()))
        .collect();
    std::env::remove_var("AETHER_MODE");
    for (name, ok) in results {
        assert!(ok, "{name} is read-only and must stay ungated");
    }
}

#[test]
fn a_self_guarding_builtin_is_not_guarded_twice() {
    // `rm` guards itself with real targets and a real blast radius. The
    // dispatcher must defer to that rather than admit the same action again.
    let _g = lock();
    std::env::set_var("AETHER_MODE", "agent");
    let skipped = safety::guard_dispatch("rm", vec!["/tmp/x".into()]).is_ok();
    std::env::remove_var("AETHER_MODE");
    assert!(
        skipped,
        "a self-guarding builtin must be skipped centrally; its own call site guards it"
    );
}

#[test]
fn an_approval_token_lets_the_call_through() {
    // A gate with no key is a denial. The approval path must actually work,
    // end to end, or the friction has no release valve.
    let _g = lock();
    std::env::set_var("AETHER_MODE", "agent");
    let err =
        safety::guard_dispatch("git_clean", vec![]).expect_err("expected an approval requirement");
    let token = err.approval.as_ref().expect("descriptor").token.clone();

    safety::grant_approval(&token);
    let allowed = safety::guard_dispatch("git_clean", vec![]);
    safety::revoke_approval(&token);
    std::env::remove_var("AETHER_MODE");

    assert!(
        allowed.is_ok(),
        "the granted token must admit the same action: {allowed:?}"
    );
}

#[test]
fn enforcement_reaches_the_dispatcher_not_just_the_helper() {
    // Guarding in `guard_dispatch` proves nothing if the dispatcher never calls
    // it. Go through the real entry point.
    let _g = lock();
    std::env::set_var("AETHER_MODE", "agent");
    let mut env = aethershell::env::Env::new();
    let result = aethershell::builtins::call("git_clean", vec![Value::Bool(false)], &mut env);
    std::env::remove_var("AETHER_MODE");

    let err = result.expect_err("the dispatcher must enforce, not merely offer enforcement");
    let text = err.to_string();
    assert!(
        text.contains("approval") || text.contains("E_NEEDS_APPROVAL"),
        "expected an approval error from the dispatcher, got: {text}"
    );
}

#[test]
fn the_central_jail_catches_a_path_that_really_is_outside_the_workspace() {
    // A destructive call naming an existing path outside the jail is the case
    // the workspace root exists to stop.
    let _g = lock();
    let outside = std::env::temp_dir().join(format!("ae_jail_out_{}", std::process::id()));
    std::fs::write(&outside, "x").expect("seed");
    let workspace = std::env::temp_dir().join(format!("ae_jail_ws_{}", std::process::id()));
    std::fs::create_dir_all(&workspace).expect("ws");

    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    let result = safety::guard_dispatch("git_clean", vec![outside.to_string_lossy().into_owned()]);
    std::env::remove_var("AETHER_WORKSPACE");
    std::env::remove_var("AETHER_MODE");
    let _ = std::fs::remove_file(&outside);
    let _ = std::fs::remove_dir_all(&workspace);

    let err = result.expect_err("an existing path outside the workspace must be refused");
    assert_eq!(err.code, safety::ErrorCode::OutsideWorkspace, "got {err:?}");
}

#[test]
fn a_non_path_argument_is_never_mistaken_for_one() {
    // The failure mode that kept the jail out of the dispatcher in the first
    // place. `docker_rm`-style arguments — container names, subcommands, SQL —
    // are not paths, and judging them against a workspace root would refuse
    // legitimate calls with no workaround.
    let _g = lock();
    let workspace = std::env::temp_dir().join(format!("ae_jail_ws2_{}", std::process::id()));
    std::fs::create_dir_all(&workspace).expect("ws");

    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    // Approve so the only thing that can fail is the jail.
    let probe = safety::guard_dispatch("podman_stop", vec!["my-container".into()]);
    let token = probe
        .as_ref()
        .err()
        .and_then(|e| e.approval.as_ref())
        .map(|a| a.token.clone());
    let after = match token {
        Some(t) => {
            safety::grant_approval(&t);
            let r = safety::guard_dispatch("podman_stop", vec!["my-container".into()]);
            safety::revoke_approval(&t);
            r
        }
        None => probe,
    };
    std::env::remove_var("AETHER_WORKSPACE");
    std::env::remove_var("AETHER_MODE");
    let _ = std::fs::remove_dir_all(&workspace);

    match after {
        Ok(()) => {}
        Err(e) => assert_ne!(
            e.code,
            safety::ErrorCode::OutsideWorkspace,
            "a container name must not be judged as a path: {e:?}"
        ),
    }
}

#[test]
fn only_paths_that_exist_are_treated_as_paths() {
    // The rule is observation, not pattern-matching: a string is a path because
    // it resolves to one, not because it contains a slash.
    let real = std::env::temp_dir();
    let found = safety::existing_paths(&[
        real.to_string_lossy().into_owned(),
        "/definitely/not/here/xyzzy".into(),
        "select * from t".into(),
        "my-container".into(),
    ]);
    assert_eq!(found.len(), 1, "expected only the real path, got {found:?}");
}
