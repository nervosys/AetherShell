use iron_shell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use std::collections::BTreeMap;
fn s(value: &str) -> Value {
    Value::Str(value.into())
}
fn record(fields: &[(&str, Value)]) -> Value {
    Value::Record(
        fields
            .iter()
            .map(|(name, value)| ((*name).into(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}
fn source(name: &str, text: &str) -> Value {
    record(&[("name", s(name)), ("source", s(text))])
}
fn vector(name: &str, values: &[f64]) -> Value {
    record(&[("name", s(name)), ("embedding", numbers(values))])
}
fn numbers(values: &[f64]) -> Value {
    Value::Array(values.iter().map(|x| Value::Float(*x)).collect())
}
fn run(name: &str, args: Vec<Value>) -> Value {
    call(name, args, &mut Env::default()).unwrap()
}
fn array(value: &Value) -> &[Value] {
    let Value::Array(value) = value else {
        panic!("expected Array: {value:?}")
    };
    value
}
fn fields(value: &Value) -> &BTreeMap<String, Value> {
    let Value::Record(value) = value else {
        panic!("expected Record: {value:?}")
    };
    value
}
fn error(name: &str, args: Vec<Value>, code: &str) {
    let err = call(name, args, &mut Env::default()).unwrap_err();
    assert!(err.to_string().contains(code), "{err}");
}
#[test]
fn clones_use_real_rust_syntax_preserve_literals_types_and_parameter_names() {
    let corpus = Value::Array(vec![
        source(
            "z.rs",
            "pub fn second(x: i64) -> i64 { /*different format*/ x + 1 }",
        ),
        source(
            "a.rs",
            "mod inner { fn first(x:i64)->i64{x+1} } fn unique(x:i64)->i64{x+2}",
        ),
        source(
            "t.rs",
            "fn different(x: u64) -> u64 {x+1} fn renamed(y:i64)->i64{y+1}",
        ),
    ]);
    let groups = run("search_duplicates", vec![corpus]);
    assert_eq!(array(&groups).len(), 1);
    let group = fields(&array(&groups)[0]);
    assert_eq!(group["count"], Value::Int(2));
    let locations = array(&group["occurrences"]);
    assert_eq!(fields(&locations[0])["document"], s("a.rs"));
    assert_eq!(fields(&locations[0])["function"], s("inner::first"));
    assert_eq!(fields(&locations[1])["function"], s("second"));
    // Whitespace in literals is data and must survive token normalization.
    let corpus = Value::Array(vec![source(
        "s.rs",
        r#"fn a()-> &'static str {"a b"} fn b()-> &'static str {"a  b"}"#,
    )]);
    assert!(array(&run("search_duplicates", vec![corpus])).is_empty());
    assert!(array(&run(
        "search_duplicates",
        vec![Value::Array(vec![source("x", "struct X;")])]
    ))
    .is_empty());
}
#[test]
fn similarity_ranks_normalized_functions_and_breaks_ties_by_location() {
    let corpus = Value::Array(vec![
        source("z", "fn renamed(x:i64)->i64{x+1}"),
        source("a", "fn same(x:i64)->i64{x+1}"),
        source("b", "fn different(x:i64)->i64{x*17}"),
    ]);
    let result = run(
        "search_similar",
        vec![s("fn query(x:i64)->i64{x+1}"), corpus.clone()],
    );
    let rows = array(&result);
    assert_eq!(rows.len(), 3);
    assert_eq!(fields(&rows[0])["document"], s("a"));
    assert_eq!(fields(&rows[1])["document"], s("z"));
    assert_eq!(fields(&rows[0])["score"], Value::Float(1.0));
    let Value::Float(last) = fields(&rows[2])["score"] else {
        panic!()
    };
    assert!((0.0..1.0).contains(&last));
    let one = run(
        "search_similar",
        vec![s("fn query(x:i64)->i64{x+1}"), corpus, Value::Int(1)],
    );
    assert_eq!(array(&one).len(), 1);
}
#[test]
fn embedding_search_matches_known_cosines_extreme_scales_and_ties() {
    let corpus = Value::Array(vec![
        vector("negative", &[-3.0, -4.0]),
        vector("horizontal", &[1.0, 0.0]),
        vector("vertical", &[0.0, 1.0]),
        vector("z_same", &[3.0, 4.0]),
        vector("a_same", &[6.0, 8.0]),
    ]);
    let result = run(
        "search_semantic",
        vec![numbers(&[3.0, 4.0]), corpus.clone()],
    );
    let rows = array(&result);
    assert_eq!(rows.len(), 5);
    let expected = [
        ("a_same", 1.0),
        ("z_same", 1.0),
        ("vertical", 0.8),
        ("horizontal", 0.6),
        ("negative", -1.0),
    ];
    for (row, (name, score)) in rows.iter().zip(expected) {
        assert_eq!(fields(row)["name"], s(name));
        let Value::Float(actual) = fields(row)["score"] else {
            panic!()
        };
        assert!((actual - score).abs() < 1e-12, "{actual} != {score}");
    }
    assert_eq!(
        array(&run(
            "search_semantic",
            vec![numbers(&[3.0, 4.0]), corpus, Value::Int(2)]
        ))
        .len(),
        2
    );
    for scale in [f64::MAX, 1e-300, f64::from_bits(1)] {
        let result = run(
            "search_semantic",
            vec![
                numbers(&[scale, scale]),
                Value::Array(vec![vector("small", &[1e-300, 1e-300])]),
            ],
        );
        let Value::Float(score) = fields(&array(&result)[0])["score"] else {
            panic!()
        };
        assert!((score - 1.0).abs() < 1e-12);
    }
    let result = run(
        "search_semantic",
        vec![
            Value::Array(vec![Value::Int(1), Value::Int(0)]),
            Value::Array(vec![vector("match", &[1.0, 0.0])]),
        ],
    );
    assert_eq!(fields(&array(&result)[0])["score"], Value::Float(1.0));
}
#[test]
fn api_documentation_extracts_public_declarations_and_literal_docs_without_execution() {
    let source = r#"
/// Alpha
/// Beta
pub async fn work<T>(x: T) -> T { panic!("must not execute") }
pub struct Visible { pub field: i64, hidden: bool }
pub trait Behavior { fn act(&self); }
pub const LIMIT: i64 = 4;
pub type Alias = i64;
fn hidden() {}
pub(crate) fn internal() {}
mod private { pub fn nested() {} }
"#;
    let result = run("docs_api", vec![s(source)]);
    let rows = array(&result);
    assert_eq!(rows.len(), 5);
    assert_eq!(fields(&rows[0])["name"], s("Alias"));
    let work = rows
        .iter()
        .find(|v| fields(v)["name"] == s("work"))
        .unwrap();
    assert_eq!(fields(work)["documentation"], s("Alpha\nBeta"));
    let Value::Str(declaration) = &fields(work)["declaration"] else {
        panic!()
    };
    assert!(declaration.contains("async fn work"));
    assert!(!declaration.contains("panic"));
    assert!(rows.iter().all(
        |r| !matches!(&fields(r)["name"],Value::Str(n) if n=="hidden"||n=="internal"||n=="nested")
    ));
    assert!(array(&run("docs_api", vec![s("fn private() {}")])).is_empty());
}
#[test]
fn search_and_api_contracts_refuse_invalid_calls_without_partial_results() {
    for (name, args) in [
        ("docs_api", vec![s("")]),
        ("search_duplicates", vec![Value::Array(vec![])]),
        ("search_similar", vec![s("fn q(){}"), Value::Array(vec![])]),
        (
            "search_semantic",
            vec![numbers(&[1.0]), Value::Array(vec![])],
        ),
    ] {
        error(name, vec![], "E_BAD_ARG");
        let mut surplus = args.clone();
        surplus.extend([Value::Int(1), Value::Int(1)]);
        error(name, surplus, "E_BAD_ARG");
        let e = call_with_input(name, args, Some(s("discarded")), &mut Env::default()).unwrap_err();
        assert!(e.to_string().contains("E_BAD_ARG"), "{e}");
    }
    error("docs_api", vec![s("pub fn {")], "E_BAD_ARG");
    error("docs_api", vec![s(&"x".repeat(65537))], "E_BUDGET_EXCEEDED");
    error(
        "docs_api",
        vec![s(&format!("//{}", "!".repeat(257)))],
        "E_BUDGET_EXCEEDED",
    );
    error(
        "search_similar",
        vec![s("struct Q;"), Value::Array(vec![])],
        "E_BAD_ARG",
    );
    error(
        "search_similar",
        vec![s("fn q(){} fn q2(){}"), Value::Array(vec![])],
        "E_BAD_ARG",
    );
    for name in ["search_duplicates", "search_similar"] {
        for corpus in [
            Value::Array(vec![source("x", "fn ok() {}"), source("bad", "fn {")]),
            Value::Array(vec![source("x", "fn a() {}"), source("x", "fn b() {}")]),
            Value::Array(vec![source("x", "fn same() {} fn same() {}")]),
            Value::Array(vec![record(&[
                ("name", s("x")),
                ("source", s("fn a() {}")),
                ("ignored", Value::Int(1)),
            ])]),
            Value::Array(vec![source("x", "fn a() {}"); 129]),
        ] {
            let mut args = if name == "search_similar" {
                vec![s("fn q() {}")]
            } else {
                vec![]
            };
            args.push(corpus.clone());
            error(
                name,
                args,
                if array(&corpus).len() == 129 {
                    "E_BUDGET_EXCEEDED"
                } else {
                    "E_BAD_ARG"
                },
            );
        }
    }
    for query in [
        numbers(&[0.0, 0.0]),
        numbers(&[f64::NAN]),
        numbers(&[f64::INFINITY]),
        Value::Array(vec![s("1")]),
        Value::Array(vec![]),
    ] {
        error(
            "search_semantic",
            vec![query, Value::Array(vec![])],
            "E_BAD_ARG",
        );
    }
    for corpus in [
        Value::Array(vec![vector("bad", &[1.0, 0.0])]),
        Value::Array(vec![vector("zero", &[0.0])]),
        Value::Array(vec![vector("x", &[1.0]), vector("x", &[1.0])]),
        Value::Array(vec![vector("ok", &[1.0]), vector("bad", &[f64::NAN])]),
    ] {
        error(
            "search_semantic",
            vec![numbers(&[1.0]), corpus, Value::Int(1)],
            "E_BAD_ARG",
        );
    }
    for k in [0, 101] {
        error(
            "search_semantic",
            vec![numbers(&[1.0]), Value::Array(vec![]), Value::Int(k)],
            "E_BAD_ARG",
        );
    }
    error(
        "search_semantic",
        vec![
            numbers(&[1.0]),
            Value::Array(vec![vector("x", &[1.0]); 1025]),
        ],
        "E_BUDGET_EXCEEDED",
    );
    let bytes = Value::Array(
        (0..128)
            .map(|i| source(&i.to_string(), &format!("//{}", "a".repeat(8192))))
            .collect(),
    );
    error("search_duplicates", vec![bytes], "E_BUDGET_EXCEEDED");
    let many = Value::Array(
        (0..86)
            .map(|i| source(&i.to_string(), "fn a(){} fn b(){} fn c(){}"))
            .collect(),
    );
    error("search_duplicates", vec![many], "E_BUDGET_EXCEEDED");
    error(
        "search_semantic",
        vec![numbers(&vec![1.0; 4097]), Value::Array(vec![])],
        "E_BAD_ARG",
    );
    let work = Value::Array(
        (0..257)
            .map(|i| vector(&i.to_string(), &vec![1.0; 4096]))
            .collect(),
    );
    error(
        "search_semantic",
        vec![numbers(&vec![1.0; 4096]), work],
        "E_BUDGET_EXCEEDED",
    );
}
