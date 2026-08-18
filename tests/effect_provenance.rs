//! Whether a builtin's effect was *decided* or merely *defaulted*.
//!
//! `effect_of` falls through to `Effect::Pure`, which is the right default for
//! policy and the wrong one for disclosure: it renders "nobody has classified
//! this" and "this was read and found harmless" as the same string. An agent
//! choosing how much to trust `x-effect: pure` cannot tell them apart, and the
//! difference is exactly the one that matters to it.

use aethershell::builtins::BUILTIN_LOOKUP;
use aethershell::safety::{effect_is_declared, effect_of, Effect};

#[test]
fn a_declared_effect_and_the_fallthrough_are_distinguishable() {
    // `rm` is classified; a name nothing matches is not. Without this the two
    // are the same `Pure`-shaped answer.
    assert!(effect_is_declared("rm"), "rm is explicitly classified");
    assert!(
        !effect_is_declared("definitely_not_a_builtin_xyzzy"),
        "an unknown name must report as undeclared, not as declared-pure"
    );
}

#[test]
fn every_effecting_class_is_declared_rather_than_defaulted() {
    // The fall-through can only ever produce `Pure`. So anything reported as
    // acting must have come from a rule — if this ever fails, the fall-through
    // has been changed to something that acts, and every unclassified builtin
    // silently became dangerous (or silently gated).
    for name in BUILTIN_LOOKUP.keys() {
        let eff = effect_of(name);
        if eff != Effect::Pure {
            assert!(
                effect_is_declared(name),
                "{name} reports {eff:?} but is not declared — the fall-through now acts"
            );
        }
    }
}

#[test]
fn the_undeclared_population_is_reported_not_hidden() {
    // Not an assertion about the number: it is debt, and pinning it would just
    // make the number a thing to satisfy. Printing it keeps it visible, which
    // is the property that was missing when 28 misclassifications accumulated.
    let total = BUILTIN_LOOKUP.len();
    let declared = BUILTIN_LOOKUP
        .keys()
        .filter(|n| effect_is_declared(n))
        .count();
    eprintln!(
        "effect provenance: {declared}/{total} declared, {} fall through to Pure ({:.0}%)",
        total - declared,
        100.0 * (total - declared) as f64 / total as f64
    );
    assert!(
        declared > 0,
        "no builtin is declared — the table is not wired"
    );
}

#[test]
fn today_every_pure_is_the_fallthrough_and_none_is_a_finding() {
    // Written expecting the opposite, and corrected by the result: the match
    // has no `Pure` arm at all. So `x-effect: pure` does not mean "read and
    // found harmless" for *any* builtin today -- it means "not classified",
    // uniformly, for all 694 of them.
    //
    // Worth pinning rather than fixing quietly. If someone later classifies a
    // builtin as genuinely Pure, this test fails and asks them to say so --
    // at which point `pure` carries two meanings again and
    // `effect_is_declared` becomes the only way to separate them.
    let declared_pure: Vec<&str> = BUILTIN_LOOKUP
        .keys()
        .filter(|n| effect_of(n) == Effect::Pure && effect_is_declared(n))
        .copied()
        .collect();
    assert!(
        declared_pure.is_empty(),
        "{} builtin(s) are now explicitly classified Pure: {:?}. This is an          improvement, not a regression -- update this test to name them, and          note that `x-effect: pure` now means two different things which only          `x-effect-declared` separates.",
        declared_pure.len(),
        &declared_pure[..declared_pure.len().min(10)]
    );
}
