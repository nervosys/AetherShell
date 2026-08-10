//! Measures AetherShell's **repair rate**: of the calls that fail, what fraction
//! does a purely mechanical agent fix using nothing but the structured error?
//!
//! This is the number §11 of `docs/AGENTIC_FIRST_DESIGN.md` left as a placeholder
//! (`≥X%` of failed agent calls repaired without human input). It was never filled
//! in because it was never measured — the design *inferred* self-correction from
//! the presence of structured errors. This test measures it instead.
//!
//! The repair strategy here is deliberately dumb: no model, no heuristics beyond
//! what the error record literally contains. That makes the result a **floor** —
//! what the error surface alone is worth, before any intelligence is applied. A
//! model-driven strategy should do better; if it does not, the errors are the
//! problem.

use aethershell::value::Value;
use agentic_eval::repair::{assess_repair, Attempt, ErrorFacts};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone)]
struct Case {
    name: String,
    args: Vec<Value>,
}

fn case(name: &str, args: Vec<Value>) -> Case {
    Case {
        name: name.to_string(),
        args,
    }
}

fn s(x: &str) -> Value {
    Value::Str(x.to_string())
}

/// Run one call and reduce its failure to the facts an agent would actually see.
fn attempt(c: &Case) -> Attempt {
    let mut env = aethershell::env::Env::new();
    match aethershell::builtins::call(&c.name, c.args.clone(), &mut env) {
        Ok(_) => Attempt::Ok,
        Err(e) => match e.downcast_ref::<aethershell::safety::SafetyError>() {
            Some(se) => Attempt::Failed(ErrorFacts {
                code: se.code.as_str().to_string(),
                retryable: se.code.retryable(),
                suggestions: se.did_you_mean.clone(),
            }),
            // No structured error at all — the dead end the boundary net exists
            // to eliminate. Scored honestly rather than excluded.
            None => Attempt::Failed(ErrorFacts::uncoded()),
        },
    }
}

/// The mechanical strategy: if the error named a replacement, take the first one.
/// Otherwise decline — inventing an argument value is not repairing *from the
/// error*, and pretending otherwise would inflate the score.
fn repair_from_error(c: &Case, facts: &ErrorFacts) -> Option<Case> {
    let name = facts.suggestions.first()?;
    Some(case(name, c.args.clone()))
}

/// A corpus of realistic agent mistakes: names an LLM plausibly emits for a shell
/// it half-remembers. Every entry must fail on the first attempt — a corpus that
/// quietly succeeds would report a vacuous rate.
///
/// The **name is the only defect**: each row's arguments are valid for the builtin
/// it is a misspelling of. That isolation is the point — if the arguments were also
/// wrong, a corrected name would still fail and the harness would score a perfectly
/// good suggestion as misleading, measuring the corpus rather than the product.
fn misspelling_corpus() -> Vec<Case> {
    let row = || {
        let mut r = std::collections::BTreeMap::new();
        r.insert("a".to_string(), Value::Int(1));
        r.insert("b".to_string(), Value::Int(2));
        Value::Record(r)
    };
    vec![
        case("piick", vec![Value::Array(vec![row()]), s("a")]),
        case("digets", vec![Value::Array(vec![row()])]),
        case("aeconn", vec![Value::Array(vec![row()])]),
        case("tokns", vec![s("hello")]),
        case("ontology_manifst", vec![]),
        case("canonicial", vec![Value::Int(1)]),
        case("safety_stats", vec![]),
        case("governer_status", vec![]),
    ]
}

#[test]
fn the_repair_rate_on_misspelled_calls_is_measured_not_assumed() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("AETHER_MODE");

    let corpus = misspelling_corpus();
    let report = assess_repair(&corpus, attempt, repair_from_error);

    println!(
        "repair rate: {}/{} = {:.0}%  (actionable {:.0}%, misleading {}, dead ends {}, \
         declined {})",
        report.repaired,
        report.failed_first,
        report.repair_rate * 100.0,
        report.actionable_rate * 100.0,
        report.misleading,
        report.dead_ends,
        report.no_repair_proposed,
    );

    assert_eq!(
        report.failed_first,
        corpus.len(),
        "every case in this corpus must actually fail, or the rate is vacuous"
    );
    assert_eq!(
        report.dead_ends, 0,
        "a misspelled name must never be an uncoded dead end"
    );
    assert_eq!(
        report.misleading, 0,
        "a suggestion that does not fix the call is worse than no suggestion: \
         it costs the agent a whole extra round trip to learn nothing"
    );
    // The floor this locks in. Mechanical, model-free repair of a plausible
    // misspelling should be the common case, not the lucky one.
    assert!(
        report.repair_rate >= 0.75,
        "mechanical repair rate {:.0}% is below the 75% floor",
        report.repair_rate * 100.0
    );
}

/// The honest other half: failures the error surface *cannot* mechanically fix.
/// A wrong-typed argument is diagnosable but not repairable without deciding what
/// the value should have been — that is a model's job, not the error's. This test
/// exists so the headline rate is never mistaken for "all failures are repairable".
#[test]
fn wrong_argument_failures_are_actionable_but_not_mechanically_repairable() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("AETHER_MODE");

    let corpus = vec![
        case("pick", vec![Value::Int(1)]),
        case("tokens", vec![]),
        case("aecon_decode", vec![Value::Int(7)]),
        case("ontology_describe", vec![Value::Bool(true)]),
    ];
    let report = assess_repair(&corpus, attempt, repair_from_error);

    assert_eq!(report.failed_first, corpus.len());
    assert_eq!(
        report.dead_ends, 0,
        "a bad argument must still arrive with a branchable code"
    );
    assert_eq!(
        report.actionable_rate, 1.0,
        "every wrong-argument failure should be actionable"
    );
    assert_eq!(
        report.repaired, 0,
        "the mechanical strategy must decline rather than guess a value"
    );
    assert_eq!(report.no_repair_proposed, corpus.len());
}

/// The boundary net's contract: across a mixed corpus of name errors, argument
/// errors and genuine runtime failures, **every** failure carries a stable code.
/// Before the net, the runtime-failure rows here were bare prose an agent could
/// only pattern-match on.
///
/// Note this asserts *coded*, not *retryable*. A genuine runtime failure gets
/// `E_UNKNOWN` with `retryable: false` — branchable, and correctly telling the
/// agent to stop rather than re-run the same call. Conflating the two would
/// score a correct refusal as a defect.
#[test]
fn every_failure_in_a_mixed_corpus_carries_a_code() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("AETHER_MODE");

    let mut corpus = misspelling_corpus();
    corpus.extend([
        case("pick", vec![Value::Int(1)]),
        case("tokens", vec![]),
        // Genuine runtime failures — these are the ones that used to be prose.
        case("cat", vec![s("definitely_no_such_file_ae.txt")]),
        case("file_read_lines", vec![s("definitely_no_such_file_ae.txt")]),
        case("json_parse", vec![s("{not valid json")]),
    ]);

    let mut failures = 0usize;
    let mut uncoded: Vec<String> = Vec::new();
    let mut codes: std::collections::BTreeMap<String, usize> = Default::default();
    for c in &corpus {
        if let Attempt::Failed(facts) = attempt(c) {
            failures += 1;
            if facts.code.is_empty() {
                uncoded.push(c.name.clone());
            } else {
                *codes.entry(facts.code.clone()).or_default() += 1;
            }
        }
    }

    assert_eq!(
        failures,
        corpus.len(),
        "every case in this corpus must actually fail"
    );
    assert!(
        uncoded.is_empty(),
        "{} of {failures} failures carried no code ({uncoded:?}) — every failure \
         must be branchable",
        uncoded.len()
    );

    let report = assess_repair(&corpus, attempt, repair_from_error);
    println!(
        "mixed corpus: {failures} failures, 0 uncoded, codes {codes:?}, \
         {} repaired mechanically",
        report.repaired
    );
}
