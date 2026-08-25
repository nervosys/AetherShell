//! A golden record of every builtin's declared effect.
//!
//! `effect_of` is a single large `match`. Refactoring it — or adding an arm
//! that accidentally shadows an earlier one — can change what a thousand
//! builtins advertise to agents without any test noticing, because each
//! individual answer still looks plausible. This pins all of them at once.
//!
//! Regenerate deliberately with `AETHER_BLESS_EFFECTS=1 cargo test --test
//! effect_snapshot`, and read the diff before committing it: a line moving from
//! `Exec` to `Pure` is a builtin becoming invisible to policy.

use aethershell::builtins::{BUILTIN_LOOKUP, FALLBACK_BUILTINS};
use aethershell::safety::effect_of;

const SNAPSHOT: &str = "tests/effect_snapshot.txt";

/// Both halves of the dispatcher.
///
/// This pinned `BUILTIN_LOOKUP` alone until the fallback `match` acquired a
/// mirror it could be read from. Those ~113 names are as callable as any other,
/// and they are exactly the ones whose classification was silently missing, so
/// leaving them out of the golden record left out the part that had already gone
/// wrong.
fn current() -> String {
    let mut names: Vec<&str> = BUILTIN_LOOKUP.keys().copied().collect();
    names.extend(FALLBACK_BUILTINS.iter().map(|(n, _)| *n));
    names.sort_unstable();
    names.dedup();
    let mut out = String::new();
    for n in names {
        out.push_str(&format!("{n}\t{:?}\n", effect_of(n)));
    }
    out
}

#[test]
fn every_builtins_effect_is_unchanged() {
    let now = current();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT);

    if std::env::var("AETHER_BLESS_EFFECTS").is_ok() || !path.exists() {
        std::fs::write(&path, &now).expect("write snapshot");
        eprintln!("blessed {} entries into {SNAPSHOT}", now.lines().count());
        return;
    }

    let before = std::fs::read_to_string(&path).expect("read snapshot");
    if before == now {
        return;
    }

    let old: std::collections::HashMap<&str, &str> =
        before.lines().filter_map(|l| l.split_once('\t')).collect();
    let new: std::collections::HashMap<&str, &str> =
        now.lines().filter_map(|l| l.split_once('\t')).collect();

    let mut changed = Vec::new();
    for (name, eff) in &new {
        match old.get(name) {
            Some(prev) if prev != eff => changed.push(format!("  {name}: {prev} -> {eff}")),
            None => changed.push(format!("  {name}: (new) -> {eff}")),
            _ => {}
        }
    }
    for name in old.keys() {
        if !new.contains_key(name) {
            changed.push(format!("  {name}: removed"));
        }
    }

    assert!(
        changed.is_empty(),
        "{} builtin effect(s) changed:\n{}\n\nIf intended, re-bless with \
         AETHER_BLESS_EFFECTS=1 and read the diff first.",
        changed.len(),
        changed.join("\n")
    );
}
