//! The dispatcher's second half, held to the same standard as its first.
//!
//! `builtins::BUILTIN_LOOKUP` is a table: it can be asked what it serves and
//! which names share an implementation. The fallback `match` in
//! `call_with_input_inner` can do neither, so `builtins::FALLBACK_BUILTINS`
//! mirrors it by hand. A hand-written mirror is exactly the drift the "catalog
//! is a promise" invariant exists to prevent — unless it is checked
//! mechanically, which is what this file does: it parses the arms out of
//! `src/builtins.rs` and demands equality, names *and* the function each name
//! reaches.
//!
//! Everything else that reasons about the fallback half — the effect ratchet,
//! the alias-agreement invariant, `eval::is_effect_free` — reads the const and
//! trusts this test to have earned that. Without it, adding an arm silently
//! shrinks the set of names every safety analysis knows about, and nothing goes
//! red.

use aethershell::builtins::{is_dispatched, BUILTIN_LOOKUP, FALLBACK_BUILTINS};
use std::collections::BTreeSet;

/// Read from disk rather than `include_str!` so the test reports on the working
/// tree, not on whatever was compiled in.
fn builtins_source() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/builtins.rs");
    std::fs::read_to_string(path).expect("src/builtins.rs is readable")
}

/// The body of the fallback `match name { .. }` in `call_with_input_inner`.
///
/// Anchored on the function rather than on line numbers, which move.
fn fallback_match_body(src: &str) -> &str {
    let fn_at = src
        .find("fn call_with_input_inner(")
        .expect("call_with_input_inner exists");
    let match_at = fn_at
        + src[fn_at..]
            .find("match name {")
            .expect("the fallback match exists");
    let open = match_at + "match name {".len();
    let mut depth = 1usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..open + i];
                }
            }
            _ => {}
        }
    }
    panic!("the fallback match is unbalanced");
}

/// Every arm as (name, function-it-calls), one entry per literal in the pattern.
///
/// Deliberately literal-only: an arm whose pattern this cannot read is invisible
/// here, which is why `an_arm_shape_this_cannot_read_is_reported` exists.
fn arm_pairs(body: &str) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('"') {
            continue;
        }
        let Some(arrow) = trimmed.find("=>") else {
            continue;
        };
        let callee: String = trimmed[arrow + 2..]
            .trim_start()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if callee.is_empty() {
            continue;
        }
        let mut rest = &trimmed[..arrow];
        while let Some(o) = rest.find('"') {
            let after = &rest[o + 1..];
            let Some(c) = after.find('"') else { break };
            out.insert((after[..c].to_string(), callee.clone()));
            rest = &after[c + 1..];
        }
    }
    out
}

#[test]
fn the_mirror_matches_the_match() {
    let src = builtins_source();
    let parsed = arm_pairs(fallback_match_body(&src));
    let declared: BTreeSet<(String, String)> = FALLBACK_BUILTINS
        .iter()
        .map(|(n, f)| (n.to_string(), f.to_string()))
        .collect();

    let missing: Vec<_> = parsed.difference(&declared).cloned().collect();
    let extra: Vec<_> = declared.difference(&parsed).cloned().collect();

    assert!(
        missing.is_empty() && extra.is_empty(),
        "builtins::FALLBACK_BUILTINS no longer mirrors the fallback match.\n\
         Served by the match but not listed (agents can call these; every safety \
         analysis is blind to them): {missing:?}\n\
         Listed but no longer served, or listed against the wrong function (an \
         alias group built from a stale pairing puts names in the wrong \
         equivalence class, which is how a guard goes missing): {extra:?}"
    );
}

#[test]
fn the_parser_finds_a_plausible_number_of_arms() {
    // A check on the checker: if the anchors above drift, `arm_pairs` returns an
    // empty set and `the_mirror_matches_the_match` would only pass once
    // FALLBACK_BUILTINS was emptied too. This makes that failure loud.
    let src = builtins_source();
    let parsed = arm_pairs(fallback_match_body(&src));
    assert!(
        parsed.len() > 80,
        "only {} fallback arms parsed — the anchors have drifted",
        parsed.len()
    );
}

#[test]
fn an_arm_shape_this_cannot_read_is_reported() {
    // `arm_pairs` reads literal patterns only. A guarded or binding arm
    // (`n if n.starts_with(..)`, `other =>`) would serve names it cannot see, and
    // the mirror would be quietly incomplete rather than wrong. The fallback has
    // exactly one such arm today — the `_ =>` that reports an unknown builtin,
    // which serves nothing.
    let src = builtins_source();
    let body = fallback_match_body(&src);
    let unreadable: Vec<&str> = body
        .lines()
        .map(str::trim_start)
        .filter(|l| l.contains("=>"))
        .filter(|l| !l.starts_with('"') && !l.starts_with("_ =>") && !l.starts_with("//"))
        .collect();
    assert!(
        unreadable.is_empty(),
        "the fallback match grew an arm whose names this test cannot read, so \
         FALLBACK_BUILTINS cannot be trusted to be complete:\n{unreadable:#?}"
    );
}

#[test]
fn the_mirror_is_sorted_by_name() {
    // `is_dispatched` binary-searches it.
    let names: Vec<&str> = FALLBACK_BUILTINS.iter().map(|(n, _)| *n).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(
        names, sorted,
        "FALLBACK_BUILTINS must stay sorted by name — is_dispatched binary-searches it"
    );
}

#[test]
fn is_dispatched_answers_for_both_halves_and_only_those() {
    assert!(is_dispatched("map"), "a BUILTIN_LOOKUP name");
    assert!(is_dispatched("from_json"), "a fallback-only name");
    assert!(
        !is_dispatched("no_such_builtin_anywhere"),
        "an unknown name must read as unknown, never as harmless"
    );
    for (name, _) in FALLBACK_BUILTINS {
        assert!(is_dispatched(name), "{name} is served but reads as unknown");
    }
    for name in BUILTIN_LOOKUP.keys() {
        assert!(is_dispatched(name), "{name} is served but reads as unknown");
    }
}

#[test]
fn report_the_arms_that_are_shadowed() {
    // Not a failure: a name in both halves is served by the fast table, so its
    // fallback arm is dead code. Worth naming so it does not accumulate.
    let shadowed: Vec<&str> = FALLBACK_BUILTINS
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| BUILTIN_LOOKUP.contains_key(n))
        .collect();
    println!(
        "fallback arms shadowed by BUILTIN_LOOKUP ({}): {shadowed:?}",
        shadowed.len()
    );
    assert!(
        shadowed.len() <= 3,
        "more fallback arms are now unreachable than before ({}): {shadowed:?}",
        shadowed.len()
    );
}
