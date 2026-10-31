//! Every category the manifest advertises can actually be listed.
//!
//! `ontology_manifest()` is the root an agent starts from: categories and
//! their counts. `ontology_describe("<category>")` is how it gets the names.
//! Four categories -- AI, Cluster, Platform and Service -- were advertised with
//! 112 builtins between them, and describing any of them returned a single
//! builtin instead of the category, because each shares its name with one
//! (`ai`, `cluster`, `platform`, `service`) and builtin lookup is
//! case-insensitive.
//!
//! The manifest's counts were right the whole time, which is why nothing
//! noticed: a check that only reads the manifest cannot see this. This walks
//! the manifest the way an agent does and asks each category for its list.

use iron_shell::agent_api::{
    ontology_describe_json, ontology_manifest_json, process_request, AgentRequest,
};
use iron_shell::modules::all_modules;
use std::collections::BTreeSet;

#[test]
fn every_advertised_category_lists_the_count_it_advertises() {
    let manifest = ontology_manifest_json();
    let cats = manifest["categories"]
        .as_array()
        .expect("manifest has a categories array");

    // Non-vacuity: a manifest with nothing in it would pass every check below.
    assert!(
        cats.len() >= 40,
        "only {} categories in the manifest",
        cats.len()
    );

    let mut short = Vec::new();
    let mut total = 0;
    for c in cats {
        let name = c["category"].as_str().expect("category name");
        let advertised = c["builtins"].as_u64().expect("category count") as usize;
        let listed = ontology_describe_json(name)["builtins"]
            .as_array()
            .map_or(0, |b| b.len());
        total += listed;
        if listed != advertised {
            short.push(format!(
                "{name}: advertised {advertised}, describe() lists {listed}"
            ));
        }
    }

    assert!(
        short.is_empty(),
        "{} categor(ies) cannot be enumerated as advertised -- an agent is told \
         these builtins exist and cannot find their names:\n{}",
        short.len(),
        short.join("\n")
    );
    assert!(
        total >= 1000,
        "only {total} builtins enumerated across all categories"
    );
}

/// The manifest's `modules` field: the namespaces an agent writes as
/// `<module>.<member>`, space-joined. It must be sorted and hold exactly the
/// names the module table registers, so the root is deterministic and every
/// name resolves.
#[test]
fn manifest_modules_are_sorted_and_match_the_module_table() {
    let manifest = ontology_manifest_json();
    let joined = manifest["modules"]
        .as_str()
        .expect("manifest has a modules string");
    let mods: Vec<&str> = joined.split(' ').collect();

    // Non-vacuity: an empty array would pass the checks below.
    assert!(!mods.is_empty(), "manifest lists no modules");

    let mut sorted = mods.clone();
    sorted.sort();
    assert_eq!(mods, sorted, "manifest modules are not sorted by name");
    assert!(
        mods.contains(&"file"),
        "`file` missing from manifest modules"
    );
    assert!(mods.contains(&"sys"), "`sys` missing from manifest modules");

    // The names the evaluator registers, deduplicated (the table can repeat one).
    let table: BTreeSet<&str> = all_modules().into_iter().map(|(name, _)| name).collect();
    let listed: BTreeSet<&str> = mods.iter().copied().collect();
    assert_eq!(
        mods.len(),
        listed.len(),
        "manifest modules contain a duplicate"
    );
    assert_eq!(
        listed, table,
        "manifest modules differ from the module table"
    );

    // A sample member really evaluates through the namespace it is listed under.
    let r = process_request(&AgentRequest::Eval {
        code: r#"str.upper("ok")"#.to_string(),
    });
    assert!(r.success, "str.upper did not evaluate: {:?}", r.error);
    assert_eq!(r.result, Some(serde_json::json!("OK")));
}

#[test]
fn a_lowercase_name_still_reaches_the_builtin_behind_a_category() {
    // The fix makes an exact, capitalised category name win. The builtin that
    // shares it must stay reachable by its own (lowercase) name.
    for (category, builtin) in [
        ("Platform", "platform"),
        ("AI", "ai"),
        ("Service", "service"),
        ("Cluster", "cluster"),
    ] {
        let c = ontology_describe_json(category);
        assert!(
            c["builtins"].as_array().is_some_and(|b| !b.is_empty()),
            "{category} should describe as a category: {c}"
        );
        let b = ontology_describe_json(builtin);
        assert_eq!(
            b["builtin"].as_str(),
            Some(builtin),
            "{builtin} should still describe as the builtin: {b}"
        );
    }
}
