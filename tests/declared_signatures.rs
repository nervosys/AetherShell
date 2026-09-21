//! A declared builtin must be checked before its body runs, and must be
//! described by its declaration rather than by its name.
//!
//! Six defects in one week were one property observed six times: a call the
//! builtin could not honour, answered with a value. `round(4.966, 2)` gave `5`;
//! `env(name, default)` discarded the default; `db_json_to_sqlite("x")` gave
//! `false` at exit 0. The calling convention carries no schema, so nothing
//! could check arity or types before the body ran.
//!
//! The discovery surface could not catch it either, because it was a second,
//! independent description. Nineteen builtins have hand-written entries; the
//! rest were generated from the **name** — `describe_builtin_name` splits on
//! `_` and title-cases the remainder. `db_json_to_sqlite` was documented as
//! "Database: json to sqlite" with an empty parameter list, and `max` as "Get
//! maximum value, `max() -> Number`" while refusing the array that implies.
//!
//! One declaration now serves both jobs. These tests assert that it does.

use aethershell::builtins::{call, call_with_input, is_dispatched};
use aethershell::env::Env;
use aethershell::signature::{signature_of, Ty, SIGNATURES};
use aethershell::value::Value;

fn err(name: &str, args: Vec<Value>) -> String {
    match call(name, args, &mut Env::new()) {
        Ok(v) => panic!("{name} accepted a call it should refuse, and answered {v:?}"),
        Err(e) => e.to_string(),
    }
}

fn ok(name: &str, args: Vec<Value>) -> Value {
    call(name, args, &mut Env::new()).unwrap_or_else(|e| panic!("{name} refused a valid call: {e}"))
}

// ── the declarations describe things that exist ─────────────────────────

#[test]
fn every_declared_builtin_is_actually_dispatched() {
    // A declaration for a name the shell does not serve would be the same
    // fiction in a new place.
    for sig in SIGNATURES {
        assert!(
            is_dispatched(sig.name),
            "{} has a declaration but is not dispatched",
            sig.name
        );
    }
}

#[test]
fn every_declaration_carries_a_worked_example() {
    // The example is the part an agent copies. A declaration without one is
    // only marginally better than a name.
    for sig in SIGNATURES {
        assert!(!sig.examples.is_empty(), "{} declares no example", sig.name);
        assert!(!sig.doc.is_empty(), "{} declares no description", sig.name);
        for (code, _) in sig.examples {
            // Under the canonical name or any declared alias -- an example for
            // `from-json` naturally reads `… | from_json | …`, which is the
            // spelling an agent writes. Both must be real: the alias is
            // asserted dispatchable below, so this cannot pass on a typo.
            let spellings = std::iter::once(sig.name).chain(sig.aliases.iter().copied());
            assert!(
                spellings.clone().any(|n| code.contains(n)),
                "{}'s example calls neither it nor any of its aliases {:?}: {code}",
                sig.name,
                sig.aliases
            );
        }
        for alias in sig.aliases {
            assert!(
                is_dispatched(alias),
                "{} declares alias `{alias}`, which is not dispatched",
                sig.name
            );
        }
    }
}

#[test]
fn the_rendered_signature_names_its_parameters() {
    // `max() -> Number` with an empty parameter list was the original defect.
    // A rendered signature must not come out empty when parameters exist.
    for sig in SIGNATURES {
        let rendered = sig.render();
        for p in sig.params {
            assert!(
                rendered.contains(p.name),
                "{}'s signature omits parameter {}: {rendered}",
                sig.name,
                p.name
            );
        }
    }
}

// ── the declaration is enforced ─────────────────────────────────────────

#[test]
fn a_wrongly_typed_argument_is_refused_by_parameter_name() {
    let e = err("round", vec![Value::Float(1.0), Value::Str("two".into())]);
    assert!(
        e.contains("digits"),
        "the refusal must name the parameter: {e}"
    );
    assert!(
        e.contains("Int"),
        "the refusal must name the expected type: {e}"
    );
}

#[test]
fn an_out_of_range_argument_is_refused() {
    // `round(x, 99)` used to be accepted and ignored.
    let e = err("round", vec![Value::Float(1.0), Value::Int(99)]);
    assert!(e.contains("0..=17"), "the refusal must name the range: {e}");
}

#[test]
fn a_missing_required_argument_is_refused_with_the_signature() {
    // This is the call that returned `false` at exit 0.
    let e = err("db_json_to_sqlite", vec![Value::Str("only-one".into())]);
    assert!(
        e.contains("db_json_to_sqlite(db: String, json: String"),
        "the refusal must show the signature, argument order included: {e}"
    );
}

#[test]
fn a_lambda_parameter_will_not_accept_a_number() {
    let mut env = Env::new();
    let e = call_with_input(
        "where",
        vec![Value::Int(5)],
        Some(Value::Array(vec![Value::Int(1)])),
        &mut env,
    )
    .expect_err("where(5) must be refused");
    assert!(e.to_string().contains("predicate"), "got: {e}");
}

#[test]
fn a_wrongly_typed_subject_is_refused() {
    let mut env = Env::new();
    let e = call_with_input(
        "sum",
        vec![],
        Some(Value::Str("not an array".into())),
        &mut env,
    )
    .expect_err("sum over a string must be refused");
    assert!(e.to_string().contains("Array"), "got: {e}");
}

// ── and the valid forms still work ──────────────────────────────────────

#[test]
fn the_calls_the_declarations_describe_all_run() {
    assert_eq!(
        ok("round", vec![Value::Float(4.966), Value::Int(2)]),
        Value::Float(4.97)
    );
    assert_eq!(ok("round", vec![Value::Float(4.5)]), Value::Int(5));
    assert_eq!(ok("max", vec![Value::Int(2), Value::Int(9)]), Value::Int(9));
    assert_eq!(
        ok(
            "max",
            vec![Value::Array(vec![
                Value::Int(1),
                Value::Int(5),
                Value::Int(3)
            ])]
        ),
        Value::Int(5)
    );
    // A subject-taking builtin called through the pipe.
    let mut env = Env::new();
    assert_eq!(
        call_with_input(
            "sum",
            vec![],
            Some(Value::Array(vec![Value::Int(1), Value::Int(2)])),
            &mut env
        )
        .expect("piped sum"),
        Value::Int(3)
    );
}

#[test]
fn an_undeclared_builtin_is_unaffected() {
    // The migration is incremental: validation must not reach builtins that
    // have not been declared, or the slice becomes a rewrite.
    assert!(
        signature_of("upper").is_none(),
        "test assumes `upper` is undeclared"
    );
    assert_eq!(
        ok("upper", vec![Value::Str("abc".into())]),
        Value::Str("ABC".into())
    );
}

// ── non-vacuity ─────────────────────────────────────────────────────────

#[test]
fn non_vacuity_validation_is_reachable_and_discriminating() {
    // If `validate` accepted everything, the refusal tests pass for the wrong
    // reason; if it refused everything, the success tests do.
    assert!(signature_of("round").is_some(), "round is not declared");
    assert!(
        call(
            "round",
            vec![Value::Float(1.0), Value::Int(2)],
            &mut Env::new()
        )
        .is_ok(),
        "a valid declared call is being refused"
    );
    assert!(
        call(
            "round",
            vec![Value::Float(1.0), Value::Int(99)],
            &mut Env::new()
        )
        .is_err(),
        "an invalid declared call is being accepted"
    );

    // And the type lattice must discriminate, or every parameter check passes.
    assert!(Ty::Int.as_str() == "Int" && Ty::Lambda.as_str() == "Lambda");
    assert!(
        call("where", vec![Value::Int(5)], &mut Env::new()).is_err(),
        "a Lambda parameter accepted an Int"
    );
}

// ── the declaration is the only description ─────────────────────────────

#[test]
fn the_catalogue_entry_comes_from_the_declaration_not_a_second_one() {
    // `map` had a hand-written catalogue entry saying `map(array: Array, fn:
    // Lambda) -> Array` while the dispatcher enforced something else. Two
    // descriptions of one builtin is the defect this work removes, so a
    // declaration must outrank the hand-written entry rather than sit beside it.
    for sig in SIGNATURES {
        let def = aethershell::agent_api::ontology_describe_json(sig.name);
        let advertised = def
            .get("signature")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("{} has no catalogue signature", sig.name));
        assert_eq!(
            advertised,
            sig.render(),
            "{}'s catalogue entry disagrees with its declaration",
            sig.name
        );
    }
}

#[test]
fn a_refusal_does_not_carry_its_signature_twice() {
    // `diagnose` is the minimal repair context. It already refuses to restate
    // what a signature says for `return_type` and `parameters`; `expected` and
    // `hint` were doing exactly that, and an expected clause naming a whole
    // signature cost as much again. This kept `diagnose` cheaper than a full
    // `ontology_describe`, which `tests/self_healing.rs` asserts.
    // Through the evaluator, because `catch` is what binds a failure to a value.
    let src = r#"diagnose(try { map() } catch e { e })"#;
    let stmts = aethershell::parser::parse_program(src).expect("parse");
    let diag = aethershell::eval::eval_program(&stmts, &mut Env::new()).expect("eval");
    let Value::Record(r) = &diag else {
        panic!("diagnose returned {diag:?}")
    };
    let sig = match r.get("signature") {
        Some(Value::Str(s)) => s.clone(),
        other => panic!("no signature in repair context: {other:?}"),
    };
    // Non-vacuity: the signature must actually be there and be the declared one.
    assert_eq!(sig, signature_of("map").expect("map declared").render());
    for field in ["expected", "hint", "message"] {
        if let Some(Value::Str(v)) = r.get(field) {
            assert!(!v.contains(&sig), "{field} restates the signature: {v}");
        }
    }
    // ...and what the repair needs is still present.
    assert!(
        matches!(r.get("got"), Some(Value::Str(_))),
        "no `got`: {r:?}"
    );
}

// ── declarations must describe the language as it is actually written ───

#[test]
fn the_corpus_working_set_is_discoverable_with_real_parameters() {
    // `benches/agentic/PREREGISTERED_E7.md` hands the AetherShell arm its
    // `ontology_describe` output as reference material, and names a poor
    // ontology as the thing that would make that arm lose for a fixable reason
    // rather than a fundamental one. These are the builtins the E1 corpus
    // actually calls; four of them (`from_json`, `group_by`, `mean`,
    // `to_string`) could not be looked up at all, because they live in the
    // dispatcher's fallback half which the catalogue never walked.
    const WORKING_SET: &[&str] = &[
        "cat",
        "from_json",
        "where",
        "map",
        "sum",
        "len",
        "group_by",
        "sort_by",
        "sort",
        "last",
        "first",
        "flatten",
        "unique",
        "any",
        "all",
        "max",
        "min",
        "mean",
        "round",
        "to_string",
        "lower",
        "contains",
    ];
    for name in WORKING_SET {
        let def = aethershell::agent_api::ontology_describe_json(name);
        assert!(
            def.get("error").is_none(),
            "{name} is called by the benchmark corpus but is not in the ontology: {def}"
        );
        let sig = def.get("signature").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            !sig.is_empty() && sig != format!("{name}() -> Value"),
            "{name}'s catalogue entry is name-derived, not declared: {sig}"
        );
        let examples = def.get("examples").and_then(|v| v.as_array());
        assert!(
            examples.is_some_and(|e| !e.is_empty()),
            "{name} has no worked example, which is the part an agent copies"
        );
    }
}

#[test]
fn a_declaration_is_found_by_every_spelling_that_dispatches() {
    // `from_json` and `from-json` are one implementation; a declaration keyed
    // on one spelling would enforce nothing for the other and would leave the
    // catalogue describing it by name-splitting.
    for (canonical, alias) in [
        ("from-json", "from_json"),
        ("group", "group_by"),
        ("avg", "mean"),
        ("str", "to_string"),
    ] {
        let a = signature_of(canonical).unwrap_or_else(|| panic!("{canonical} not declared"));
        let b = signature_of(alias).unwrap_or_else(|| panic!("{alias} does not resolve"));
        assert_eq!(
            a.name, b.name,
            "{alias} resolved to a different declaration"
        );
        assert!(
            is_dispatched(alias),
            "{alias} is declared as an alias but is not dispatched"
        );
    }
}

#[test]
fn non_vacuity_an_unrelated_name_still_resolves_to_nothing() {
    // If `signature_of` fell back to something for any input, both tests above
    // would pass without meaning anything.
    assert!(signature_of("definitely_not_a_builtin_94117").is_none());
    assert!(
        aethershell::agent_api::ontology_describe_json("definitely_not_a_builtin_94117")
            .get("error")
            .is_some(),
        "the ontology invented an entry for a name that does not exist"
    );
}
