//! Aliases must agree about what they do.
//!
//! Dispatch is by *implementation*; classification is by *name*. `lldb` and
//! `lldb_run` are one dispatch index; `vault-convert` and `vault_convert` are
//! one match arm. `safety::effect_of` reads a literal name, so only one spelling
//! of each was ever classified — and the other read as `Pure`.
//!
//! `guard_dispatch` takes its decision from `effect_of(builtin)` with the name
//! as typed. `centrally_enforced(Pure)` is false, so the unclassified spelling
//! returned `Ok` before any policy ran, and because the audit line covers only
//! `WriteLocal`/`Network` it left no trace either. Measured when this was
//! written: **104** alias groups disagreed with themselves and **26** produced
//! different guard decisions for the same implementation.
//! `tests/alias_guard_bypass.rs` runs one of them end to end.
//!
//! This file holds every alias group — both halves of the dispatcher — to a
//! single effect, so a classification added to one spelling and not its twin
//! fails the build instead of quietly opening a door.

use aethershell::builtins::alias_groups;
use aethershell::safety::{effect_of, Effect};

/// Groups whose members disagree, with the effect each name claims.
fn disagreements() -> Vec<Vec<(&'static str, Effect)>> {
    let mut out = Vec::new();
    for names in alias_groups() {
        let labelled: Vec<(&'static str, Effect)> =
            names.iter().map(|n| (*n, effect_of(n))).collect();
        let first = labelled[0].1;
        if labelled.iter().any(|(_, e)| *e != first) {
            out.push(labelled);
        }
    }
    out
}

#[test]
fn aliases_of_one_implementation_share_one_effect() {
    let bad = disagreements();
    let report: Vec<String> = bad
        .iter()
        .map(|names| {
            let each: Vec<String> = names
                .iter()
                .map(|(n, e)| format!("{n}={}", e.as_str()))
                .collect();
            format!("  {}", each.join(" "))
        })
        .collect();

    assert!(
        bad.is_empty(),
        "{} alias group(s) disagree about their own effect. Dispatch is by \
         implementation and classification is by name, so the weakest spelling is \
         the one an agent will use: `guard_dispatch` reads `effect_of(builtin)` for \
         the name as typed, and a `Pure` spelling of a guarded builtin is not \
         gated and not even audited.\n\
         `effect_of` inherits the strictest sibling classification, so a group can \
         only reach this list when two spellings are *both* classified and \
         classified differently — which is a decision, not an oversight, and has \
         to be made in `safety::classified_effect`:\n{}",
        bad.len(),
        report.join("\n")
    );
}

#[test]
fn the_grouping_is_not_vacuous() {
    // A check on the checker: if the grouping broke there would be no groups and
    // the assertion above would pass by finding nothing.
    let groups = alias_groups();
    assert!(
        groups.len() > 20,
        "only {} alias groups found — the grouping has broken",
        groups.len()
    );
    assert!(
        groups
            .iter()
            .any(|n| n.contains(&"undo") && n.contains(&"rewind")),
        "the known fallback alias pair undo/rewind was not grouped"
    );
    assert!(
        groups
            .iter()
            .any(|n| n.contains(&"sh") && n.contains(&"shell")),
        "the known fast-table alias pair sh/shell was not grouped"
    );
}

#[test]
fn every_group_that_agrees_agrees_on_a_declared_effect() {
    // Direction check. A rule that made every group agree by *relaxing* the
    // classified spelling would satisfy the test above and undo the fix, so
    // assert that a group carrying a non-Pure effect still reports it as
    // declared rather than as a fall-through guess.
    for names in alias_groups() {
        let agreed = effect_of(names[0]);
        if agreed == Effect::Pure {
            continue;
        }
        for name in &names {
            assert!(
                aethershell::safety::effect_is_declared(name),
                "`{name}` reports {} but reads as undeclared, so an agent asking whether the label was reasoned about is told no",
                agreed.as_str()
            );
        }
    }
}
