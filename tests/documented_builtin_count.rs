//! The builtin count the docs advertise must be one the code can reproduce.
//!
//! Five figures were in circulation at once -- "1,280+" (AGENTS.md, lib.rs),
//! "1,100+" (llms.txt), "1,300+" (the plugin manifest and OpenAPI spec) and
//! "1,301" (README) -- none of them tied to a computation, so each drifted on
//! its own. There are three honest numbers, and they answer different
//! questions:
//!
//! - **listed**: what `ontology_manifest()` reports, i.e. what an agent can
//!   discover. This is the one to advertise.
//! - **names**: every string the dispatcher accepts, aliases included.
//! - **implementations**: distinct functions behind those names.
//!
//! The docs quote "listed" and "names"; this pins both, rounded down to the
//! hundred with a "+", so ordinary growth does not churn every page but a
//! drop below the advertised figure fails here.

use aethershell::builtins::{BUILTIN_LOOKUP, FALLBACK_BUILTINS};
use std::collections::BTreeSet;

fn listed() -> usize {
    aethershell::agent_api::ontology_manifest_json()["total_builtins"]
        .as_u64()
        .unwrap() as usize
}

fn names() -> usize {
    let mut all: BTreeSet<&str> = BUILTIN_LOOKUP.keys().copied().collect();
    all.extend(FALLBACK_BUILTINS.iter().map(|(n, _)| *n));
    all.len()
}

fn implementations() -> usize {
    let lookup: BTreeSet<usize> = BUILTIN_LOOKUP.values().copied().collect();
    let fallback: BTreeSet<&str> = FALLBACK_BUILTINS
        .iter()
        .filter(|(n, _)| !BUILTIN_LOOKUP.contains_key(n))
        .map(|(_, target)| *target)
        .collect();
    lookup.len() + fallback.len()
}

/// "1,100+" for 1,164.
fn advertised(n: usize) -> String {
    let h = n / 100 * 100;
    format!("{},{:03}+", h / 1000, h % 1000)
}

#[test]
fn print_the_counts() {
    eprintln!(
        "listed {}  names {}  implementations {}",
        listed(),
        names(),
        implementations()
    );
}

#[test]
fn every_advertised_count_is_reproducible() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let listed = advertised(listed());
    for file in [
        "AGENTS.md",
        "llms.txt",
        "llms-full.txt",
        "README.md",
        "src/lib.rs",
        ".well-known/ai-plugin.json",
        ".well-known/openapi.yaml",
    ] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        for stale in ["1,280+", "1,300+", "1,301 builtins"] {
            assert!(
                !text.contains(stale),
                "{file} still says {stale}; the ontology lists {listed}"
            );
        }
        assert!(
            text.contains(&listed),
            "{file} should advertise {listed} builtins (what ontology_manifest lists)"
        );
    }
}
