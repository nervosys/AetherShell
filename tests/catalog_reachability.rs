//! Everything the ontology advertises must be callable.
//!
//! An agent discovers what this shell can do by reading the catalog. A builtin
//! described there but absent from the dispatcher is worse than one that is
//! missing from both: the agent has been told it exists, will call it, and gets
//! `E_UNKNOWN_BUILTIN` for its trouble.
//!
//! That was not hypothetical. `rm` was classified `Destructive` by `effect_of`
//! and never registered, so the safety layer guarded a name no caller could
//! reach and the shell could not delete a file. A sweep afterwards found 168
//! `bi_*` implementations unreachable by name -- `vm_*`, `wsl_*`, `virsh_*`,
//! `firewall_*` and friends. Measured, not assumed: **none of the 168 is
//! advertised**, so they are unused code rather than broken promises, and
//! registering them is a product decision left open.
//!
//! This test is what keeps that gap at zero.

use aethershell::agent_api::{ontology_describe_json, ontology_manifest_json};
use aethershell::builtins::BUILTIN_LOOKUP;
use serde_json::Value as J;

/// Names the dispatcher serves from its fallback `match` rather than from
/// `BUILTIN_LOOKUP`, so absence from the table is not absence from the shell.
///
/// Kept explicit and short. Anything added here is a claim that the name is
/// reachable by some other route -- verify it before adding, because an
/// unchecked entry turns this test into decoration.
const SERVED_BY_FALLBACK: &[&str] = &[
    // Option constructors, capitalised.
    "Some",
    "None",
    // PowerShell-style cmdlets.
    "Get-Files",
    "Get-Content",
    "Select-Object",
    "Where-Object",
    "ForEach-Object",
    "Sort-Object",
    "Group-Object",
    "Measure-Object",
    // Nushell-style data commands.
    "from-json",
    "to-json",
    "from-csv",
    "to-csv",
    "from-yaml",
    "to-yaml",
    "from_json",
    "to_json",
    "from_csv",
    "to_csv",
    "from_yaml",
    "to_yaml",
    "columns",
    "describe",
];

/// Every builtin name the ontology describes, walked category by category --
/// the same route an agent takes.
fn advertised_builtins() -> Vec<String> {
    let manifest = ontology_manifest_json();
    let mut out = Vec::new();
    if let Some(J::Array(cats)) = manifest.get("categories") {
        for c in cats {
            let name = c
                .get("category")
                .and_then(|v| v.as_str())
                .or_else(|| c.as_str());
            let Some(cat) = name else { continue };
            let listing = ontology_describe_json(cat);
            if let Some(J::Array(bs)) = listing.get("builtins") {
                for b in bs {
                    if let Some(n) = b.get("name").and_then(|v| v.as_str()) {
                        out.push(n.to_string());
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn reachable(name: &str) -> bool {
    BUILTIN_LOOKUP.contains_key(name)
        || SERVED_BY_FALLBACK.contains(&name)
        // The catalog carries a few display-cased names whose lookup entry is
        // lowercase; treat a case-insensitive hit as reachable, since the
        // dispatcher resolves them.
        || BUILTIN_LOOKUP
            .keys()
            .any(|k| k.eq_ignore_ascii_case(name))
}

#[test]
fn the_catalog_advertises_nothing_the_dispatcher_cannot_serve() {
    let advertised = advertised_builtins();
    assert!(
        advertised.len() > 500,
        "only {} builtins enumerated — the walk is broken, and a walk that \
         finds nothing proves nothing",
        advertised.len()
    );

    let unreachable: Vec<&String> = advertised.iter().filter(|n| !reachable(n)).collect();
    assert!(
        unreachable.is_empty(),
        "{} advertised builtin(s) cannot be called: {:?}\n\
         Either register them or stop advertising them — an agent reads this \
         catalog and believes it.",
        unreachable.len(),
        unreachable.iter().take(20).collect::<Vec<_>>()
    );
}

#[test]
fn the_reachability_check_can_actually_fail() {
    // The assertion above passes when the catalog is honest and also when the
    // check is broken. This pins the difference: a name nothing serves must be
    // reported unreachable.
    assert!(
        !reachable("definitely_not_a_builtin_xyzzy"),
        "the reachability check accepts a name that does not exist, so its \
         clean result above means nothing"
    );
    assert!(
        reachable("upper"),
        "a plainly registered builtin must resolve"
    );
    assert!(
        reachable("Some"),
        "a fallback-served name must resolve, or the allow-list is not wired"
    );
}

#[test]
fn the_destructive_builtins_are_reachable_and_declared() {
    // The specific shape of the `rm` bug: `effect_of` classifies a name the
    // dispatcher does not have, so policy appears to govern an operation no
    // caller can invoke. Coverage that governs nothing reads as coverage.
    use aethershell::safety::{effect_is_declared, effect_of, Effect};
    for name in BUILTIN_LOOKUP.keys() {
        if effect_of(name) == Effect::Destructive {
            assert!(
                effect_is_declared(name),
                "{name} is Destructive by fall-through, which cannot happen"
            );
        }
    }
    for name in ["rm", "rmdir"] {
        assert!(
            BUILTIN_LOOKUP.contains_key(name),
            "{name} is classified Destructive but absent from the dispatcher"
        );
    }
}
