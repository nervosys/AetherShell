//! Whether a builtin's effect was *decided* or merely *defaulted*.
//!
//! `effect_of` falls through to `Effect::Pure`, which is the right default for
//! policy and the wrong one for disclosure: it renders "nobody has classified
//! this" and "this was read and found harmless" as the same string. An agent
//! choosing how much to trust `x-effect: pure` cannot tell them apart, and the
//! difference is exactly the one that matters to it.

use iron_shell::builtins::BUILTIN_LOOKUP;
use iron_shell::safety::{effect_is_declared, effect_of, Effect};

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
fn reviewed_pure_builtins_are_distinguished_from_the_fallthrough() {
    // Native sd substitution was reviewed as Pure when its external process
    // and file-writing classification were removed. Both aliases must disclose
    // that finding; other Pure results still require the undeclared flag.
    let mut declared_pure: Vec<&str> = BUILTIN_LOOKUP
        .keys()
        .filter(|n| effect_of(n) == Effect::Pure && effect_is_declared(n))
        .copied()
        .collect();
    declared_pure.sort_unstable();
    assert_eq!(
        declared_pure,
        ["sd", "sd_replace"],
        "update the reviewed Pure inventory when classifying another builtin"
    );
}

// ── The disclosure convention ───────────────────────────────────────────────

#[test]
fn the_ontology_flags_only_the_undeclared_and_absence_means_declared() {
    // `effect_declared` is emitted only when false. That is a deliberate
    // asymmetry -- it halves the cost on the full-detail dump (+4.7% instead
    // of +9.2%, measured) and only one direction is actionable. But it means
    // *absence carries meaning*, which is exactly the kind of convention that
    // rots into "the field is missing, so who knows" without a test.
    use iron_shell::agent_api::ontology_describe_json;

    let unclassified = ontology_describe_json("upper");
    assert_eq!(unclassified["effect"], "pure");
    assert_eq!(
        unclassified["effect_declared"], false,
        "an unclassified builtin must carry the flag, or agents cannot tell \
         a default from a finding"
    );

    for name in ["sh", "ls", "cat", "sd", "sd_replace"] {
        let classified = ontology_describe_json(name);
        if matches!(name, "sd" | "sd_replace") {
            assert_eq!(classified["effect"], "pure");
        }
        assert!(
            classified.get("effect_declared").is_none(),
            "{name} is classified, so the flag must be absent -- emitting it \
             as `true` is the cost this convention exists to avoid"
        );
    }
}

#[test]
fn the_readers_reclassified_from_pure_report_read_local() {
    // The 55 found by body evidence. Spot-checked here so a future edit to the
    // match cannot quietly return them to `pure` -- which would restore the
    // claim that `ls` is referentially transparent.
    use iron_shell::safety::{effect_of, Effect};
    for name in [
        "ls", "cat", "head", "tail", "grep", "wc", "fs_stat", "env_var",
    ] {
        assert_eq!(
            effect_of(name),
            Effect::ReadLocal,
            "{name} observes local state and must not advertise as Pure"
        );
    }
}
