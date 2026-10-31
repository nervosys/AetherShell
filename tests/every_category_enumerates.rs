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
use iron_shell::value::Value;
use std::collections::BTreeMap;

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

/// The manifest's `modules` field: the `<module>.<member>` namespaces an agent
/// writes. Each count must be the member count of the module table the
/// evaluator registers, and the list must be sorted so the root is deterministic.
#[test]
fn manifest_modules_are_sorted_and_counted_from_the_module_table() {
    let manifest = ontology_manifest_json();
    let mods = manifest["modules"]
        .as_object()
        .expect("manifest has a modules object");

    // Non-vacuity: an empty object would pass the checks below.
    assert!(!mods.is_empty(), "manifest lists no modules");

    let names: Vec<&str> = mods.keys().map(String::as_str).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "manifest modules are not sorted by name");
    assert!(
        names.contains(&"file"),
        "`file` missing from manifest modules"
    );
    assert!(
        names.contains(&"sys"),
        "`sys` missing from manifest modules"
    );

    // The table the evaluator registers, counted the same way it is stored.
    let table: BTreeMap<&str, usize> = all_modules()
        .into_iter()
        .map(|(name, module)| match module {
            Value::Record(members) => (name, members.len()),
            other => panic!("module {name} is not a record: {other:?}"),
        })
        .collect();
    assert_eq!(
        mods.len(),
        table.len(),
        "manifest lists {} modules, the module table registers {}",
        mods.len(),
        table.len()
    );
    for (name, count) in mods {
        let members = count.as_u64().expect("module member count") as usize;
        let expected = table
            .get(name.as_str())
            .copied()
            .unwrap_or_else(|| panic!("manifest lists {name}, which the table does not register"));
        assert_eq!(
            members, expected,
            "manifest says {name} has {members} members; the module table has {expected}"
        );
    }

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
