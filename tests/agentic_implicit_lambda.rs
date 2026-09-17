//! The implicit parameter must bind every `~` in an expression, not the first.
//!
//! `ae -a` exists to save tokens. It could not be used on the expressions that
//! spend them, because the desugaring built its lambda at the first `~` it
//! reached and then ran to the end of the body:
//!
//! ```text
//! w(~.is_pr)                        where(fn(__) => __.is_pr)              ok
//! w(!~.is_pr && ~.state=="open")    where(!fn(__) => __.is_pr && …)        broken
//!                                   error: where: expected a lambda, got Bool
//! w(contains(lower(~.title), "x"))  where(contains(lower(fn(__) => …)))    broken
//!                                   error: lower: expected a string, got Lambda
//! ```
//!
//! One reference worked and two did not, which ruled out six of the ten queries
//! in `benches/agentic/` -- the six with compound predicates. The token-
//! minimised syntax was unusable on the shape that spends the tokens, and
//! nothing caught it because nothing had ever benchmarked `-a`.
//!
//! The lambda is now lifted to the argument of the lambda-taking builtin, with
//! every `~.` in it bound to the same parameter.

use aethershell::transpile::agentic::transpile_agentic_to_ae;

/// Transpile the stage in pipeline position and return just the stage.
///
/// A bare `w(...)` at statement start is not pipeline position and does not
/// desugar; every realistic use of an implicit parameter is a pipeline stage.
fn stage(src: &str) -> String {
    let full = t(&format!("xs|{src}"));
    match full.split_once('|') {
        Some((_, rhs)) => rhs.trim().to_string(),
        None => full,
    }
}

fn t(src: &str) -> String {
    transpile_agentic_to_ae(src)
        .unwrap_or_else(|e| panic!("transpile {src}: {e}"))
        .trim()
        .to_string()
}

#[test]
fn one_reference_desugars_exactly_as_before() {
    // The case that already worked must be untouched, or the fix is a rewrite
    // rather than a widening.
    // `w(~.is_pr)` keeps the caller's own parentheses around the body
    // (`fn(__) => (__.is_pr)`), which is semantically identical and costs the
    // agent nothing: the transpiled source is internal, and what is token-
    // counted is the agentic text the agent wrote. Assert the binding, not the
    // spelling.
    let one = stage("w(~.is_pr)");
    assert!(
        one.contains("where(fn(__) =>") && one.contains("__.is_pr"),
        "got: {one}"
    );
    assert_eq!(one.matches("fn(__)").count(), 1, "got: {one}");
    let cmp = stage(r#"w(~.state=="closed")"#);
    assert!(
        cmp.contains("where(fn(__) =>")
            && cmp.contains(r#"__.state"#)
            && cmp.contains(r#""closed""#),
        "got: {cmp}"
    );
    assert_eq!(cmp.matches("fn(__)").count(), 1, "got: {cmp}");
}

#[test]
fn two_references_share_one_parameter() {
    // The defect: this produced `where(!fn(__) => …)`.
    let out = stage(r#"w(!~.is_pr&&~.state=="open")"#);
    assert!(
        out.starts_with("where(fn(__) =>") || out.contains("where(fn(__) =>"),
        "the lambda must wrap the whole predicate, got: {out}"
    );
    assert_eq!(
        out.matches("fn(__)").count(),
        1,
        "two references must share one lambda, not make two: {out}"
    );
    assert!(
        !out.contains("!fn(__)"),
        "the negation is still outside the lambda: {out}"
    );
}

#[test]
fn a_reference_nested_in_a_call_is_bound_too() {
    // The second shape of the defect: `lower(fn(__) => …)`.
    let out = stage(r#"w(contains(lower(~.title+" "+~.body),"security"))"#);
    assert_eq!(out.matches("fn(__)").count(), 1, "got: {out}");
    assert!(
        !out.contains("lower(fn(__)"),
        "a nested reference still builds its own lambda: {out}"
    );
    assert!(
        out.contains("__.title") && out.contains("__.body"),
        "got: {out}"
    );
}

#[test]
fn an_explicit_parameter_is_left_alone() {
    // `~x:body` names its parameter and must not be hoisted.
    let out = stage("w~x:x.is_pr");
    assert!(
        out.contains("fn(x)") && !out.contains("fn(__)"),
        "the explicit-parameter form was rewritten: {out}"
    );
}

#[test]
fn a_tilde_inside_a_string_is_not_a_parameter() {
    // The substitution runs over source text, so it has to respect literals.
    let out = stage(r#"w(~.name=="~.odd")"#);
    assert!(
        out.contains(r#""~.odd""#),
        "the literal was rewritten: {out}"
    );
    assert_eq!(
        out.matches("__").count(),
        2,
        "expected one fn(__) and one __.name: {out}"
    );
}

#[test]
fn an_expression_with_no_implicit_parameter_is_unchanged() {
    // The hoist must return None and leave the old path alone.
    let out = stage(r#"w(fn(r) => r.is_pr)"#);
    assert!(
        out.contains("fn(r) => r.is_pr"),
        "an explicit lambda was disturbed: {out}"
    );
    assert!(!out.contains("fn(__)"), "a parameter was invented: {out}");
}

#[test]
fn the_bare_dot_form_still_works() {
    // `w.size>1k` is sugar for `w~.size>1k` and is the measured token lever.
    let out = stage("w.is_pr");
    assert!(out.contains("where(fn(__) => __.is_pr)"), "got: {out}");
}

// ── non-vacuity ─────────────────────────────────────────────────────────

#[test]
fn non_vacuity_the_transpiler_is_producing_aethershell() {
    // If `transpile_agentic_to_ae` echoed its input, several assertions above
    // would hold for the wrong reason.
    let out = stage("w(~.is_pr)");
    assert!(out.contains("where"), "the builtin was not expanded: {out}");
    assert_ne!(out.trim(), "w(~.is_pr)", "the input came back unchanged");

    // And the broken emission must now be impossible.
    let compound = stage(r#"w(!~.is_pr&&~.state=="open")"#);
    assert!(
        !compound.contains("!fn(__)"),
        "the first-reference-only desugaring is back: {compound}"
    );
}
