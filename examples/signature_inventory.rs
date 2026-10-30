//! Enumerate migration coverage without invoking any builtin.
use iron_shell::builtins::{BUILTIN_LOOKUP, FALLBACK_BUILTINS};
use iron_shell::signature::{signature_of, SIGNATURES};
use std::collections::BTreeMap;

fn main() {
    let mut implementations: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for (&name, &index) in BUILTIN_LOOKUP.iter() {
        implementations
            .entry(format!("indexed:{index}"))
            .or_default()
            .push(name);
    }
    for &(name, function) in FALLBACK_BUILTINS {
        if !BUILTIN_LOOKUP.contains_key(name) {
            implementations
                .entry(format!("fallback:{function}"))
                .or_default()
                .push(name);
        }
    }
    let mut missing = Vec::new();
    let mut declared_names = 0;
    let mut callable_names = 0;
    let mut covered = 0;
    for names in implementations.values_mut() {
        names.sort_unstable();
        names.dedup();
        callable_names += names.len();
        let declared = names
            .iter()
            .filter(|name| signature_of(name).is_some())
            .count();
        declared_names += declared;
        if declared == names.len() {
            covered += 1;
        } else {
            missing.push(
                names
                    .iter()
                    .filter(|name| signature_of(name).is_none())
                    .copied()
                    .collect::<Vec<_>>(),
            );
        }
    }
    missing.sort();
    println!("{} declarations; {covered}/{} implementations fully covered; {declared_names}/{callable_names} callable names covered", SIGNATURES.len(), implementations.len());
    for names in missing {
        println!("{}", names.join(", "));
    }
}
