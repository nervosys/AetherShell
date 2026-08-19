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

// ── A model-driven strategy ─────────────────────────────────────────────────
//
// The mechanical strategy is a *floor*: it substitutes the first `did_you_mean`
// candidate and declines everything else, which is why the wrong-argument
// corpus scores 0. Declining is correct for it — inventing a value is not
// repairing from the error — but it leaves the ceiling unmeasured.
//
// A model can propose a value. What follows is that strategy, kept honest in
// two ways: it is given *only* the error facts and the builtin's declared
// signature (the same material an agent has), and it is scored by the same
// harness, by actually re-running the call.

/// Ask a model for a corrected call, given the failure and the signature.
///
/// The prompt demands a strict JSON object so the reply is parseable rather
/// than prose to be pattern-matched — the failure mode this whole module
/// exists to measure is a repair that *looks* right.
fn model_repair_prompt(c: &Case, facts: &ErrorFacts) -> String {
    let signature = {
        let d = aethershell::agent_api::ontology_describe_json(&c.name);
        d.get("signature")
            .and_then(|v| v.as_str())
            .unwrap_or("(unknown)")
            .to_string()
    };
    format!(
        "A shell builtin call failed. Propose a corrected call.\n\n\
         builtin: {name}\n\
         signature: {signature}\n\
         error code: {code}\n\
         suggestions: {suggestions:?}\n\n\
         Reply with ONLY a JSON object of the form \
         {{\"name\": \"<builtin>\", \"args\": [<json values>]}}. \
         No prose, no code fences.",
        name = c.name,
        signature = signature,
        code = facts.code,
        suggestions = facts.suggestions,
    )
}

/// Turn a model reply into a case. Returns `None` on anything unparseable,
/// which the harness scores as "no repair proposed" — a strategy that cannot
/// be understood has not repaired anything.
fn parse_model_reply(reply: &str) -> Option<Case> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    let json: serde_json::Value = serde_json::from_str(reply.get(start..=end)?).ok()?;
    let name = json.get("name")?.as_str()?.to_string();
    let args = json
        .get("args")?
        .as_array()?
        .iter()
        .map(json_to_value)
        .collect::<Vec<_>>();
    Some(Case { name, args })
}

fn json_to_value(j: &serde_json::Value) -> Value {
    match j {
        serde_json::Value::String(s) => Value::Str(s.clone()),
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(Value::Int)
            .or_else(|| n.as_f64().map(Value::Float))
            .unwrap_or(Value::Null),
        serde_json::Value::Array(a) => Value::Array(a.iter().map(json_to_value).collect()),
        serde_json::Value::Object(o) => Value::Record(
            o.iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect(),
        ),
        serde_json::Value::Null => Value::Null,
    }
}

/// The strategy, parameterised over "ask something" so it can be scored
/// without a model in CI and with one when a model is configured.
fn repair_with<F>(c: &Case, facts: &ErrorFacts, ask: F) -> Option<Case>
where
    F: Fn(&str) -> Option<String>,
{
    parse_model_reply(&ask(&model_repair_prompt(c, facts))?)
}

#[test]
fn the_model_strategy_repairs_what_the_mechanical_one_declines() {
    // Plumbing proven without a model: a deterministic stand-in that returns
    // the corrected call. This asserts the *strategy* works -- prompt in, case
    // out, harness re-runs it and it succeeds -- on exactly the corpus where
    // the mechanical strategy scores 0.
    //
    // It does not claim a real model would answer this well. That is the live
    // test below, which skips loudly when no backend is configured rather than
    // reporting a number nobody measured.
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("AETHER_MODE");

    let corpus = vec![case("pick", vec![Value::Int(1)]), case("tokens", vec![])];

    let oracle = |prompt: &str| -> Option<String> {
        // Answers from the signature in the prompt, the way a model would.
        if prompt.contains("builtin: pick") {
            Some(r#"{"name":"pick","args":[[{"a":1}],"a"]}"#.to_string())
        } else if prompt.contains("builtin: tokens") {
            Some(r#"{"name":"tokens","args":["hello"]}"#.to_string())
        } else {
            None
        }
    };

    let report = assess_repair(&corpus, attempt, |c, f| repair_with(c, f, oracle));

    assert_eq!(
        report.failed_first,
        corpus.len(),
        "the corpus must fail first"
    );
    assert_eq!(
        report.repaired,
        corpus.len(),
        "a strategy that can supply an argument should repair what the          mechanical one declines; got {report:?}"
    );
}

#[test]
fn an_unparseable_model_reply_counts_as_no_repair_not_as_success() {
    // The dishonest failure this guards: a strategy that returns *something*
    // and gets scored as a repair. Prose, fences and empty replies must all
    // land in `no_repair_proposed`.
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("AETHER_MODE");

    let corpus = vec![case("tokens", vec![])];
    for reply in ["I think you meant tokens(\"hello\")", "", "```json\n```"] {
        let report = assess_repair(&corpus, attempt, |c, f| {
            repair_with(c, f, |_| Some(reply.to_string()))
        });
        assert_eq!(
            report.repaired, 0,
            "reply {reply:?} must not score as a repair"
        );
        assert_eq!(report.no_repair_proposed, 1);
    }
}

#[test]
fn a_configured_model_is_scored_on_the_same_corpus() {
    // The live half. Skips loudly rather than passing silently: a repair rate
    // reported without a model is a number nobody measured.
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("AETHER_MODE");

    if aethershell::ai::complete_sync_router("reply with OK").is_err() {
        eprintln!(
            "SKIP: no AI backend configured (set AETHER_AI, or start IronGate); the model repair rate is unmeasured, not zero"
        );
        return;
    }

    let corpus = vec![
        case("pick", vec![Value::Int(1)]),
        case("tokens", vec![]),
        case("aecon_decode", vec![Value::Int(7)]),
        case("ontology_describe", vec![Value::Bool(true)]),
    ];
    let report = assess_repair(&corpus, attempt, |c, f| {
        repair_with(c, f, |p| aethershell::ai::complete_sync_router(p).ok())
    });

    eprintln!(
        "model repair rate on wrong-argument corpus: {}/{} repaired          (mechanical scores 0 here by design)",
        report.repaired, report.failed_first
    );
    // Deliberately not asserted as a threshold. Pinning a model's score turns a
    // measurement into a flaky test; the number is reported so it can be read.
    assert_eq!(
        report.dead_ends, 0,
        "every failure must still arrive with a branchable code"
    );
}
