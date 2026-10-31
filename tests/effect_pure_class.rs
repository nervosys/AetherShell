//! `Pure` is a stated class, not the fall-through (docs/COMPLETION_PLAN.md).
//!
//! `safety::effect_of` answers `Pure` for any name nothing classifies, and
//! agent mode allows `Pure` without approval. So a builtin added to the dispatch
//! without a `classified_effect` arm was allowed by default. `tests/effect_snapshot`
//! does not stop that: a new name is reported as "(new)", and re-blessing it
//! writes `Pure` into the baseline without any check.
//!
//! This test closes it. Every dispatched name must be classified, or on the
//! stated-pure baseline `tests/effect_stated_pure.txt`. A new name fails until
//! someone either classifies it or adds it to that file, which is a reviewed
//! change. A listed name that gains a classification also fails, so the list
//! can only shrink.

use iron_shell::builtins::{BUILTIN_LOOKUP, FALLBACK_BUILTINS};
use iron_shell::safety::effect_is_declared;
use std::collections::BTreeSet;

/// The stated class, one name per line; `#` lines are comments.
const STATED_BASELINE: &str = include_str!("effect_stated_pure.txt");

fn stated_pure() -> Vec<&'static str> {
    STATED_BASELINE
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

fn dispatched() -> BTreeSet<&'static str> {
    let mut names: BTreeSet<&'static str> = BUILTIN_LOOKUP.keys().copied().collect();
    names.extend(FALLBACK_BUILTINS.iter().map(|(n, _)| *n));
    names
}

#[test]
fn every_dispatched_name_is_classified_or_stated_pure() {
    let listed = stated_pure();
    let missing: Vec<&str> = dispatched()
        .into_iter()
        .filter(|n| !effect_is_declared(n) && !listed.contains(n))
        .collect();
    assert!(
        missing.is_empty(),
        "{} dispatched builtin(s) are neither classified nor on tests/effect_stated_pure.txt, \
         so they fall through to Pure and agent mode allows them without approval. Classify \
         each in src/safety.rs classified_effect, or, if its body reads nothing, writes nothing \
         and spawns nothing, add it to the baseline:\n{}",
        missing.len(),
        missing.join("\n")
    );
}

#[test]
fn stated_pure_list_is_sorted_unique_and_current() {
    let listed = stated_pure();
    let mut sorted = listed.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted, listed,
        "tests/effect_stated_pure.txt must be sorted, with no duplicates"
    );

    let names = dispatched();
    let stale: Vec<&str> = listed
        .iter()
        .copied()
        .filter(|n| !names.contains(n) || effect_is_declared(n))
        .collect();
    assert!(
        stale.is_empty(),
        "tests/effect_stated_pure.txt lists names that are not dispatched or are now \
         classified; remove them so the list only shrinks:\n{}",
        stale.join("\n")
    );
}
