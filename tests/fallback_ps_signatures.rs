//! The PowerShell-style pipeline builtins served by the fallback dispatcher:
//! foreach-object, get-content, get-files, measure-object, select-object,
//! sort-object and where-object, under every spelling that dispatches to them.
//!
//! Each of these once took an argument it could not use and ignored it, or
//! answered a wrong shape with a plausible value. Every test here pins one of
//! those: real behaviour on literal arrays and records, a refusal for each
//! argument the body does not accept, and a coded refusal for a piped subject
//! of the wrong shape. The file-system tests build their own directories under
//! the temp dir and never read the working directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use aethershell::builtins::{call, call_with_input, is_dispatched};
use aethershell::env::Env;
use aethershell::eval::eval_program;
use aethershell::parser::parse_program;
use aethershell::value::Value;

/// An environment whose undo journal is this test process's own; see
/// `tests/declared_signatures.rs`.
fn env() -> Env {
    static ISOLATED: std::sync::Once = std::sync::Once::new();
    ISOLATED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae_fb_ps_{}", std::process::id()));
        std::env::set_var("AETHER_SESSION_DIR", dir);
    });
    Env::new()
}

const FOREACH: &[&str] = &["foreach-object", "ForEach-Object", "foreach"];
const SELECT: &[&str] = &["select-object", "Select-Object", "select"];
const MEASURE: &[&str] = &["measure-object", "Measure-Object", "measure"];
const SORT: &[&str] = &["sort-object", "Sort-Object"];
const WHERE: &[&str] = &["where-object", "Where-Object"];
const GET_CONTENT: &[&str] = &["get-content", "Get-Content"];
const GET_FILES: &[&str] = &["get-files", "Get-Files"];

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

fn ints(xs: &[i64]) -> Value {
    Value::Array(xs.iter().map(|x| Value::Int(*x)).collect())
}

fn record(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<String, Value>>(),
    )
}

fn lambda(code: &str) -> Value {
    let stmts = parse_program(code).expect("the lambda parses");
    eval_program(&stmts, &mut env()).expect("the lambda evaluates")
}

fn piped(name: &str, args: Vec<Value>, input: Value) -> Result<Value, String> {
    call_with_input(name, args, Some(input), &mut env()).map_err(|e| e.to_string())
}

fn direct(name: &str, args: Vec<Value>) -> Result<Value, String> {
    call(name, args, &mut env()).map_err(|e| e.to_string())
}

fn field(v: &Value, key: &str) -> Value {
    match v {
        Value::Record(r) => r
            .get(key)
            .cloned()
            .unwrap_or_else(|| panic!("no field {key} in {v:?}")),
        other => panic!("expected a record, got {other:?}"),
    }
}

/// The refusal a call produced; a call that succeeded fails the test.
fn refusal(result: Result<Value, String>, what: &str) -> String {
    match result {
        Ok(v) => panic!("{what} was accepted and answered {v:?}"),
        Err(e) => e,
    }
}

fn assert_refused(result: Result<Value, String>, what: &str) {
    let msg = refusal(result, what);
    assert!(
        msg.contains("E_BAD_ARG"),
        "{what} should be refused with E_BAD_ARG, got: {msg}"
    );
}

fn names_of(v: &Value) -> Vec<String> {
    let Value::Array(items) = v else {
        panic!("expected an array of file records, got {v:?}");
    };
    let mut names: Vec<String> = items
        .iter()
        .map(|item| match field(item, "Name") {
            Value::Str(s) => s,
            other => panic!("Name is not a string: {other:?}"),
        })
        .collect();
    names.sort();
    names
}

/// A directory under the temp dir that exists for the test and is removed after.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("ae_fb_ps_{}_{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch directory");
        Scratch(dir)
    }

    fn path(&self) -> String {
        self.0.display().to_string()
    }

    fn join(&self, rel: &str) -> String {
        self.0.join(rel).display().to_string()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn every_spelling_is_dispatched() {
    for name in FOREACH
        .iter()
        .chain(SELECT)
        .chain(MEASURE)
        .chain(SORT)
        .chain(WHERE)
        .chain(GET_CONTENT)
        .chain(GET_FILES)
    {
        assert!(is_dispatched(name), "{name} is not dispatched");
    }
}

#[test]
fn foreach_object_maps_under_every_spelling() {
    for name in FOREACH {
        let doubled = piped(name, vec![lambda("fn(x) => x * 2")], ints(&[1, 2, 3]))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(doubled, ints(&[2, 4, 6]), "{name}");

        // The direct form takes the array first, as map does.
        let direct_form = direct(name, vec![ints(&[1, 2, 3]), lambda("fn(x) => x * 2")])
            .unwrap_or_else(|e| panic!("{name} direct: {e}"));
        assert_eq!(direct_form, ints(&[2, 4, 6]), "{name} direct");

        // A surplus argument is refused, not ignored.
        assert_refused(
            piped(name, vec![lambda("fn(x) => x"), Value::Int(1)], ints(&[1])),
            name,
        );
        // So is a transform that is not a lambda.
        assert_refused(piped(name, vec![Value::Int(1)], ints(&[1])), name);
    }
}

#[test]
fn select_object_keeps_named_properties_under_every_spelling() {
    for name in SELECT {
        let rows = Value::Array(vec![
            record(&[("a", Value::Int(1)), ("b", Value::Int(2))]),
            record(&[("a", Value::Int(3))]),
        ]);
        let picked =
            piped(name, vec![text("a")], rows.clone()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            picked,
            Value::Array(vec![
                record(&[("a", Value::Int(1))]),
                record(&[("a", Value::Int(3))]),
            ]),
            "{name}"
        );

        // With no property named, the input comes back unchanged.
        assert_eq!(piped(name, vec![], rows.clone()).unwrap(), rows, "{name}");

        // Property names are strings; the variadic parameter has no surplus.
        assert_refused(piped(name, vec![Value::Int(1)], rows.clone()), name);
        // A piped element that is not a record is refused, not passed through.
        assert_refused(piped(name, vec![text("a")], ints(&[1, 2])), name);
        assert_refused(piped(name, vec![text("a")], Value::Int(1)), name);
        // No piped subject at all.
        assert_refused(direct(name, vec![text("a")]), name);
    }
}

#[test]
fn measure_object_counts_and_summarises_under_every_spelling() {
    for name in MEASURE {
        let counted =
            piped(name, vec![], ints(&[1, 2, 3])).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(field(&counted, "Count"), Value::Int(3), "{name}");

        let rows = Value::Array(vec![
            record(&[("n", Value::Int(2))]),
            record(&[("n", Value::Int(5))]),
            record(&[("s", text("x"))]),
        ]);
        let stats =
            piped(name, vec![text("n")], rows.clone()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(field(&stats, "Count"), Value::Int(3), "{name}");
        assert_eq!(field(&stats, "Sum"), Value::Float(7.0), "{name}");
        assert_eq!(field(&stats, "Maximum"), Value::Float(5.0), "{name}");

        // A piped value that is not an array counts as one element, and the
        // property is read from it: it used to be ignored.
        let single = piped(name, vec![text("n")], record(&[("n", Value::Int(4))]))
            .unwrap_or_else(|e| panic!("{name} single: {e}"));
        assert_eq!(field(&single, "Count"), Value::Int(1), "{name}");
        assert_eq!(field(&single, "Sum"), Value::Float(4.0), "{name}");

        assert_refused(piped(name, vec![Value::Int(5)], rows.clone()), name);
        // A second argument is surplus.
        assert_refused(piped(name, vec![text("n"), text("s")], rows), name);
        // It needs a piped subject.
        assert_refused(direct(name, vec![ints(&[1])]), name);
    }
}

#[test]
fn sort_object_orders_under_every_spelling() {
    for name in SORT {
        assert_eq!(
            piped(name, vec![], ints(&[3, 1, 2])).unwrap_or_else(|e| panic!("{name}: {e}")),
            ints(&[1, 2, 3]),
            "{name}"
        );

        let rows = Value::Array(vec![
            record(&[("n", Value::Int(2))]),
            record(&[("n", Value::Int(1))]),
        ]);
        assert_eq!(
            piped(name, vec![text("n")], rows.clone()).unwrap(),
            Value::Array(vec![
                record(&[("n", Value::Int(1))]),
                record(&[("n", Value::Int(2))]),
            ]),
            "{name}"
        );

        assert_refused(piped(name, vec![text("n"), text("m")], rows.clone()), name);
        assert_refused(piped(name, vec![Value::Int(1)], rows.clone()), name);
        // A scalar piped in used to come back unsorted and unrefused.
        assert_refused(piped(name, vec![], Value::Int(3)), name);
        // Sorting by a property needs records; the property was ignored before.
        assert_refused(piped(name, vec![text("n")], ints(&[1, 2])), name);
        assert_refused(direct(name, vec![ints(&[3, 1, 2])]), name);
    }
}

#[test]
fn where_object_filters_under_every_spelling() {
    for name in WHERE {
        let kept = piped(name, vec![lambda("fn(x) => x > 1")], ints(&[1, 2, 3]))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(kept, ints(&[2, 3]), "{name}");

        let rows = Value::Array(vec![
            record(&[("n", Value::Int(1))]),
            record(&[("n", Value::Int(2))]),
            record(&[("n", Value::Int(3))]),
        ]);
        let greater = piped(
            name,
            vec![text("n"), text("-gt"), Value::Int(1)],
            rows.clone(),
        )
        .unwrap_or_else(|e| panic!("{name} -gt: {e}"));
        assert_eq!(
            greater,
            Value::Array(vec![
                record(&[("n", Value::Int(2))]),
                record(&[("n", Value::Int(3))]),
            ]),
            "{name}"
        );

        let titles = Value::Array(vec![
            record(&[("t", text("xabx"))]),
            record(&[("t", text("q"))]),
        ]);
        let like = piped(name, vec![text("t"), text("-like"), text("ab")], titles)
            .unwrap_or_else(|e| panic!("{name} -like: {e}"));
        assert_eq!(
            like,
            Value::Array(vec![record(&[("t", text("xabx"))])]),
            "{name}"
        );

        // A surplus argument after the lambda is refused.
        assert_refused(
            piped(
                name,
                vec![lambda("fn(x) => x > 1"), Value::Int(1)],
                ints(&[1, 2]),
            ),
            name,
        );
        // The property form takes exactly three arguments.
        assert_refused(
            piped(
                name,
                vec![text("n"), text("-eq"), Value::Int(1), Value::Int(9)],
                rows.clone(),
            ),
            name,
        );
        assert_refused(
            piped(name, vec![text("n"), text("-eq")], rows.clone()),
            name,
        );
        // No filter, or a filter of the wrong type.
        assert_refused(piped(name, vec![], ints(&[1])), name);
        assert_refused(piped(name, vec![Value::Int(1)], ints(&[1])), name);
        // An unknown operator is refused even when nothing would be compared.
        assert_refused(
            piped(
                name,
                vec![text("n"), text("-zz"), Value::Int(1)],
                Value::Array(vec![]),
            ),
            name,
        );
        // -like needs a string target.
        assert_refused(
            piped(
                name,
                vec![text("t"), text("-like"), Value::Int(1)],
                rows.clone(),
            ),
            name,
        );
        // A piped record is not an array; it used to be returned unfiltered.
        assert_refused(
            piped(
                name,
                vec![text("n"), text("-eq"), Value::Int(1)],
                record(&[("n", Value::Int(1))]),
            ),
            name,
        );
        // It needs a piped subject.
        assert_refused(
            direct(name, vec![ints(&[1]), lambda("fn(x) => x > 1")]),
            name,
        );
    }
}

#[test]
fn get_content_reads_one_path_under_every_spelling() {
    let scratch = Scratch::new("content");
    let file = scratch.join("lines.txt");
    fs::write(&file, "one\ntwo\n").expect("write the fixture");
    let path = text(&file);

    for name in GET_CONTENT {
        let lines = Value::Array(vec![text("one"), text("two")]);
        assert_eq!(
            direct(name, vec![path.clone()]).unwrap_or_else(|e| panic!("{name}: {e}")),
            lines,
            "{name}"
        );
        assert_eq!(
            piped(name, vec![], path.clone()).unwrap(),
            lines,
            "{name} piped"
        );

        assert_refused(direct(name, vec![]), name);
        // A second argument is surplus.
        assert_refused(direct(name, vec![path.clone(), text("x")]), name);
        // The path comes from the pipe or the arguments, not both.
        assert_refused(piped(name, vec![path.clone()], path.clone()), name);
        assert_refused(direct(name, vec![Value::Int(1)]), name);
        assert_refused(piped(name, vec![], Value::Int(1)), name);

        let missing = text(&scratch.join("absent.txt"));
        let msg = refusal(direct(name, vec![missing]), name);
        assert!(
            msg.contains("E_NOT_FOUND"),
            "{name}: a missing file is E_NOT_FOUND, got {msg}"
        );
    }
}

#[test]
fn get_files_lists_under_every_spelling() {
    let scratch = Scratch::new("files");
    fs::write(scratch.join("a.txt"), "a").expect("write a");
    fs::create_dir_all(scratch.join("sub")).expect("make sub");
    fs::write(scratch.0.join("sub").join("b.txt"), "b").expect("write b");
    let dir = text(&scratch.path());

    for name in GET_FILES {
        let top = direct(name, vec![dir.clone()]).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            names_of(&top),
            vec!["a.txt".to_string(), "sub".to_string()],
            "{name}"
        );

        let deep = direct(name, vec![dir.clone(), text("-Recurse")])
            .unwrap_or_else(|e| panic!("{name} -Recurse: {e}"));
        assert_eq!(
            names_of(&deep),
            vec!["a.txt".to_string(), "b.txt".to_string(), "sub".to_string()],
            "{name} -Recurse"
        );

        let filtered = direct(name, vec![dir.clone(), text("-Filter"), text("a")])
            .unwrap_or_else(|e| panic!("{name} -Filter: {e}"));
        assert_eq!(
            names_of(&filtered),
            vec!["a.txt".to_string()],
            "{name} -Filter"
        );

        assert_eq!(
            names_of(&piped(name, vec![], dir.clone()).unwrap()),
            names_of(&top),
            "{name} piped"
        );

        // Anything outside the grammar is refused before the directory is read.
        assert_refused(direct(name, vec![text("-Nope")]), name);
        // A second path is refused, not dropped.
        assert_refused(direct(name, vec![dir.clone(), text("x")]), name);
        // A piped path and an argument path together.
        assert_refused(piped(name, vec![dir.clone()], dir.clone()), name);
        assert_refused(direct(name, vec![Value::Int(1)]), name);
        // -Filter without a value, or with one that is not a string.
        assert_refused(direct(name, vec![dir.clone(), text("-Filter")]), name);
        assert_refused(
            direct(name, vec![dir.clone(), text("-Filter"), Value::Int(1)]),
            name,
        );
        assert_refused(piped(name, vec![], Value::Int(1)), name);

        let missing = text(&scratch.join("absent"));
        let msg = refusal(direct(name, vec![missing]), name);
        assert!(
            msg.contains("E_NOT_FOUND"),
            "{name}: a missing directory is E_NOT_FOUND, got {msg}"
        );
    }
}
