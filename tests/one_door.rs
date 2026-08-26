//! `guard_dispatch` is the only door — and `pub fn bi_*` is a window.
//!
//! `builtins::call_with_input_inner` calls `safety::guard_dispatch` before either
//! half of the dispatcher runs, and its comment says it is "the one place every
//! builtin passes through". Everything this crate does about safety rests on that
//! sentence: `effect_of`'s classifications, the alias inheritance, the privilege
//! taxonomy, the audit log. If a second way in exists, all of it describes rather
//! than constrains.
//!
//! It is a *comment*. Comments go stale — this session found the roadmap naming
//! deleted functions as blockers for two sessions running. So the claim is
//! checked here instead.
//!
//! The window is real: 128 `bi_*` functions are `pub`, so any caller inside the
//! crate — or any consumer of the published library — can invoke the
//! implementation directly and never meet `guard_dispatch`. Nothing in `src/`
//! does that today (asserted below), and the tests that *do* call `bi_rm` /
//! `bi_rmdir` directly are testing those builtins' **own** guards, which is
//! exactly the property that makes a direct call safe.
//!
//! Hence the invariant, which is narrower and truer than "nothing is pub":
//!
//! > A `pub` builtin whose effect is centrally enforced must guard itself.
//!
//! `Process`/`Destructive`/`Exec`/`Privileged` are the classes `guard_dispatch`
//! actually gates (`centrally_enforced`). For any other class it returns `Ok`
//! before deciding anything, so a direct call skips nothing that would have
//! happened. For those four, skipping the central gate means skipping the only
//! gate — unless the body has its own, which is what `safety::SELF_GUARDED`
//! records.

use aethershell::builtins::{BUILTIN_LOOKUP, FALLBACK_BUILTINS};
use aethershell::safety::{effect_of, Effect, SELF_GUARDED};
use std::collections::BTreeSet;

fn builtins_source() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/builtins.rs");
    std::fs::read_to_string(path).expect("src/builtins.rs is readable")
}

/// The classes `safety::centrally_enforced` gates. Mirrored rather than called
/// because the function is private; `the_enforced_set_is_still_these_four` below
/// keeps the mirror honest.
fn centrally_enforced(e: Effect) -> bool {
    matches!(
        e,
        Effect::Process | Effect::Destructive | Effect::Exec | Effect::Privileged
    )
}

/// Every `pub fn bi_*` in `src/builtins.rs`.
fn public_builtin_fns(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in src.lines() {
        if let Some(rest) = line.strip_prefix("pub fn bi_") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                out.insert(format!("bi_{name}"));
            }
        }
    }
    out
}

/// The dispatcher names a given `bi_*` function serves, across both halves.
fn names_served_by(fn_name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(stripped) = fn_name.strip_prefix("bi_") {
        if BUILTIN_LOOKUP.contains_key(stripped) {
            out.push(stripped.to_string());
        }
    }
    for (name, callee) in FALLBACK_BUILTINS {
        if *callee == fn_name {
            out.push((*name).to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn a_publicly_callable_builtin_that_is_centrally_gated_must_guard_itself() {
    let src = builtins_source();
    let mut holes: Vec<String> = Vec::new();

    for fn_name in public_builtin_fns(&src) {
        for name in names_served_by(&fn_name) {
            let effect = effect_of(&name);
            if !centrally_enforced(effect) {
                continue;
            }
            if SELF_GUARDED.contains(&name.as_str()) {
                continue;
            }
            holes.push(format!(
                "  {name} ({fn_name}) is {} and `pub` but not self-guarded",
                effect.as_str()
            ));
        }
    }

    assert!(
        holes.is_empty(),
        "{} builtin(s) can be reached without `guard_dispatch` and have no guard \
         of their own. `pub fn bi_*` is callable directly — by anything in this \
         crate, and by any consumer of the published library — which skips the \
         only gate their effect class has.\n\
         Fix by dropping the `pub` (preferred: the compiler then enforces the \
         single door), or by guarding the body and adding the name to \
         `safety::SELF_GUARDED`. Do not add it to SELF_GUARDED without the \
         guard — that list means 'the body already decided', and an entry \
         without one silently disables the central gate as well:\n{}",
        holes.len(),
        holes.join("\n")
    );
}

#[test]
fn nothing_in_the_crate_reaches_a_builtin_body_directly() {
    // The other half. Even a `pub` implementation is harmless while every caller
    // goes through the door; this is what would change first if one did not.
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut offenders: Vec<String> = Vec::new();

    fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
                // `builtins.rs` is where the bodies live and where the dispatcher
                // legitimately names them.
                if p.file_name().and_then(|s| s.to_str()) == Some("builtins.rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&p) else {
                    continue;
                };
                for (i, line) in text.lines().enumerate() {
                    if line.trim_start().starts_with("//") {
                        continue;
                    }
                    if line.contains("builtins::bi_") {
                        out.push(format!("{}:{}: {}", p.display(), i + 1, line.trim()));
                    }
                }
            }
        }
    }
    walk(std::path::Path::new(dir), &mut offenders);

    assert!(
        offenders.is_empty(),
        "a builtin implementation is called directly, bypassing \
         `guard_dispatch`:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_gate_still_comes_before_both_halves_of_the_dispatcher() {
    // Ordering, not merely presence. `guard_dispatch` deciding *after* the
    // builtin ran would satisfy a grep and gate nothing.
    let src = builtins_source();
    let start = src
        .find("fn call_with_input_inner(")
        .expect("call_with_input_inner exists");
    let body = &src[start..];

    let guard = body
        .find("guard_dispatch(")
        .expect("call_with_input_inner must call guard_dispatch");
    let fast = body
        .find("fast_builtin_lookup(")
        .expect("the fast half is dispatched here");
    let fallback = body
        .find("match name {")
        .expect("the fallback half is dispatched here");

    assert!(
        guard < fast && guard < fallback,
        "guard_dispatch no longer runs before the dispatcher. Policy that is \
         consulted after the action is a log, not a gate"
    );
}

#[test]
fn the_dispatch_table_is_only_reachable_through_the_gate() {
    // `BUILTIN_DISPATCH[i](..)` is a third way in if anyone indexes it elsewhere.
    let src = builtins_source();
    let uses: Vec<usize> = src
        .match_indices("BUILTIN_DISPATCH[")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        uses.len(),
        1,
        "BUILTIN_DISPATCH is indexed in {} places; it must be reachable only \
         from `fast_builtin_lookup`, which `call_with_input_inner` calls after \
         the gate",
        uses.len()
    );

    let lookup_fn = src
        .find("fn fast_builtin_lookup(")
        .expect("fast_builtin_lookup exists");
    let lookup_end = src[lookup_fn..]
        .find("\n}\n")
        .map(|o| lookup_fn + o)
        .unwrap_or(src.len());
    assert!(
        uses[0] > lookup_fn && uses[0] < lookup_end,
        "the one index into BUILTIN_DISPATCH is outside fast_builtin_lookup"
    );
}

#[test]
fn the_enforced_set_is_still_these_four() {
    // `centrally_enforced` above is a mirror of a private function. If the real
    // one gains a class, this test's first assertion silently narrows and stops
    // finding holes — so pin the mirror to observable behaviour: these four
    // classes are the ones a guard decision can refuse.
    for e in [
        Effect::Process,
        Effect::Destructive,
        Effect::Exec,
        Effect::Privileged,
    ] {
        assert!(centrally_enforced(e), "{} must be gated", e.as_str());
    }
    for e in [
        Effect::Pure,
        Effect::ReadLocal,
        Effect::WriteLocal,
        Effect::Network,
    ] {
        assert!(
            !centrally_enforced(e),
            "{} is now centrally enforced — widen the mirror in this file, or \
             `a_publicly_callable_builtin_that_is_centrally_gated_must_guard_itself` \
             is checking a smaller set than the guard actually gates",
            e.as_str()
        );
    }
}
