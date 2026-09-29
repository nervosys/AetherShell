//! Every call the agent-facing material names must exist.
//!
//! Found 2026-09-28 by pointing `explain_effects` at them: 21 of the 141
//! agentic abbreviations (`F.d`, `H.u`, `gh.p`, …) expanded to functions that
//! were never implemented, and 14 of the 116 module calls in `AGENTS.md`
//! (`gh.pr_list()`, `node.version()`, `platform.os()`, …) named functions no
//! module has. An agent reading either was handed a name that fails. The
//! static analyser resolves a call exactly as evaluation does, so an
//! unresolved name here is a name that would not run.

use aethershell::explain::explain;
use aethershell::value::Value;

fn unresolved(calls: &[String]) -> Vec<String> {
    let code: String = calls.iter().map(|c| format!("{c}(1)\n")).collect();
    let report = explain(&code).expect("the generated calls parse");
    let Value::Record(r) = report else {
        panic!("explain returns a record")
    };
    match r.get("unresolved") {
        Some(Value::Array(v)) => v
            .iter()
            .filter_map(|x| match x {
                Value::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => panic!("explain reports unresolved calls"),
    }
}

fn sorted_unique(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

#[test]
fn every_agentic_abbreviation_expands_to_a_real_function() {
    let src = std::fs::read_to_string("src/transpile/agentic.rs").expect("read agentic.rs");
    let re = regex::Regex::new(r#"m\.insert\("[a-z0-9_]+\.[a-z]+", "([a-z0-9_]+\.[a-z0-9_]+)"\)"#)
        .unwrap();
    let targets = sorted_unique(re.captures_iter(&src).map(|c| c[1].to_string()).collect());
    assert!(
        targets.len() > 100,
        "the scanner found only {} targets",
        targets.len()
    );
    let missing = unresolved(&targets);
    assert!(
        missing.is_empty(),
        "agentic abbreviations expand to functions that do not exist: {missing:?}"
    );
}

#[test]
fn every_module_call_in_agents_md_exists() {
    let doc = std::fs::read_to_string("AGENTS.md").expect("read AGENTS.md");
    let re = regex::Regex::new(r"`([a-z][a-z0-9_]*\.[a-z][a-z0-9_]*)\(").unwrap();
    let calls = sorted_unique(re.captures_iter(&doc).map(|c| c[1].to_string()).collect());
    assert!(
        calls.len() > 50,
        "the scanner found only {} calls",
        calls.len()
    );
    let missing = unresolved(&calls);
    assert!(
        missing.is_empty(),
        "AGENTS.md documents calls that do not exist: {missing:?}"
    );
}

#[test]
fn the_check_can_fail() {
    // Non-vacuity: a name that does not exist is reported.
    assert_eq!(
        unresolved(&["gh.pr_list".to_string()]),
        vec!["gh.pr_list".to_string()]
    );
}
