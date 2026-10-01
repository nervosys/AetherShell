use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use std::{path::PathBuf, sync::Mutex};
static LOCK: Mutex<()> = Mutex::new(());
struct Fixture {
    root: PathBuf,
    vars: Vec<(&'static str, Option<std::ffi::OsString>)>,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("ae-text-contract-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let keys = [
            "NO_COLOR",
            "AETHER_MODE",
            "AETHER_AGENT",
            "AETHER_WORKSPACE",
            "AETHER_POLICY",
            "AETHER_AUDIT_LOG",
            "AETHER_AUDIT_KEY",
            "AETHER_AUDIT_KEY_FILE",
            "AETHER_AUDIT_SINK",
            "AETHER_MAX_FILES",
        ];
        let fixture = Self {
            root,
            vars: keys
                .iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        };
        for key in keys {
            std::env::remove_var(key);
        }
        std::env::set_var("AETHER_AUDIT_LOG", fixture.root.join("audit.log"));
        std::env::set_var("NO_COLOR", "1");
        std::env::set_var("AETHER_AUDIT_KEY", "11".repeat(32));
        fixture
    }
    fn path(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        while aethershell::tx::rollback().is_ok() {}
        for (key, value) in &self.vars {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        // Only recursively remove the exact absolute directory we exclusively created.
        if self
            .root
            .canonicalize()
            .is_ok_and(|resolved| resolved == self.root)
        {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}
fn s(text: &str) -> Value {
    Value::Str(text.into())
}
fn diff_record(value: Value) -> std::collections::BTreeMap<String, Value> {
    let Value::Record(record) = value else {
        panic!("expected diff record")
    };
    record
}
#[test]
fn file_diff_preserves_positional_changes_counts_and_line_equality() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let a = fixture.path("a");
    let b = fixture.path("b");
    std::fs::write(&a, "a\r\nb\r\n").unwrap();
    std::fs::write(&b, "a\nc\nd").unwrap();
    let mut env = Env::default();
    for name in ["file_diff", "diff_files"] {
        let result = diff_record(call(name, vec![s(&a), s(&b)], &mut env).unwrap());
        assert_eq!(result["file1"], s(&a));
        assert_eq!(result["file2"], s(&b));
        assert_eq!(result["lines1"], Value::Int(2));
        assert_eq!(result["lines2"], Value::Int(3));
        assert_eq!(result["diff_count"], Value::Int(2));
        assert_eq!(result["identical"], Value::Bool(false));
        let Value::Array(rows) = &result["differences"] else {
            panic!("expected differences")
        };
        assert_eq!(
            diff_record(rows[0].clone()),
            std::collections::BTreeMap::from([
                ("line".into(), Value::Int(2)),
                ("type".into(), s("changed")),
                ("old".into(), s("b")),
                ("new".into(), s("c"))
            ])
        );
        assert_eq!(
            diff_record(rows[1].clone()),
            std::collections::BTreeMap::from([
                ("line".into(), Value::Int(3)),
                ("type".into(), s("added")),
                ("new".into(), s("d"))
            ])
        );
    }
    std::fs::write(&b, "a").unwrap();
    let result = diff_record(call("file_diff", vec![s(&a), s(&b)], &mut env).unwrap());
    let Value::Array(rows) = &result["differences"] else {
        panic!("expected differences")
    };
    assert_eq!(
        diff_record(rows[0].clone()),
        std::collections::BTreeMap::from([
            ("line".into(), Value::Int(2)),
            ("type".into(), s("deleted")),
            ("old".into(), s("b"))
        ])
    );
    std::fs::write(&b, "a\nb").unwrap();
    assert_eq!(
        diff_record(call("file_diff", vec![s(&a), s(&b)], &mut env).unwrap())["identical"],
        Value::Bool(true)
    );
    std::fs::write(&a, "").unwrap();
    std::fs::write(&b, "").unwrap();
    let result = diff_record(call("file_diff", vec![s(&a), s(&b)], &mut env).unwrap());
    assert_eq!(result["lines1"], Value::Int(0));
    assert_eq!(result["differences"], Value::Array(vec![]));
}
#[test]
fn file_diff_declares_and_guards_both_actual_read_targets() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let workspace = fixture.root.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("a"), "a").unwrap();
    std::fs::write(workspace.join("b"), "b").unwrap();
    let outside = fixture.path("outside");
    std::fs::write(&outside, "secret").unwrap();
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    std::env::set_var("AETHER_MODE", "agent");
    let mut env = Env::default();
    for name in ["file_diff", "diff_files"] {
        assert_eq!(
            aethershell::safety::effect_of(name),
            aethershell::safety::Effect::ReadLocal
        );
        assert!(aethershell::safety::effect_is_declared(name));
        code(
            call(name, vec![s("a"), s(&outside)], &mut env),
            "E_OUTSIDE_WORKSPACE",
        );
        code(
            call(name, vec![s(&outside), s("b")], &mut env),
            "E_OUTSIDE_WORKSPACE",
        );
        assert_eq!(
            diff_record(call(name, vec![s("a"), s("b")], &mut env).unwrap())["diff_count"],
            Value::Int(1)
        );
    }
}
#[test]
fn file_diff_bounds_reads_and_refuses_invalid_arguments() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let mut env = Env::default();
    let good = fixture.path("good");
    std::fs::write(&good, "a").unwrap();
    let bad = fixture.path("bad");
    std::fs::write(&bad, [0xff]).unwrap();
    code(call("file_diff", vec![s(&good), s(&bad)], &mut env), "E_IO");
    code(
        call(
            "file_diff",
            vec![s(&good), s(&fixture.path("missing"))],
            &mut env,
        ),
        "E_NOT_FOUND",
    );
    code(
        call("file_diff", vec![s(&good), s(&fixture.path("."))], &mut env),
        "E_BAD_ARG",
    );
    let large = fixture.path("large");
    std::fs::File::create(&large)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    code(
        call("file_diff", vec![s(&large), s(&good)], &mut env),
        "E_BUDGET_EXCEEDED",
    );
    code(
        call("file_diff", vec![s(&good), Value::Int(1)], &mut env),
        "E_BAD_ARG",
    );
    code(call("file_diff", vec![s(&good)], &mut env), "E_BAD_ARG");
    code(
        call("file_diff", vec![s(&good), s(&good), s(&good)], &mut env),
        "E_BAD_ARG",
    );
    code(
        call_with_input(
            "file_diff",
            vec![s(&good), s(&good)],
            Some(s(&good)),
            &mut env,
        ),
        "E_BAD_ARG",
    );
}
#[test]
fn file_diff_bounds_difference_records_at_the_exact_limit() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let mut env = Env::default();
    let a = fixture.path("a");
    let b = fixture.path("b");
    std::fs::write(&a, "a\n".repeat(10000)).unwrap();
    std::fs::write(&b, "b\n".repeat(10000)).unwrap();
    let result = diff_record(call("diff_files", vec![s(&a), s(&b)], &mut env).unwrap());
    assert_eq!(result["diff_count"], Value::Int(10000));
    let Value::Array(rows) = &result["differences"] else {
        panic!("expected differences")
    };
    assert_eq!(diff_record(rows[9999].clone())["line"], Value::Int(10000));
    std::fs::write(&a, "a\n".repeat(10001)).unwrap();
    std::fs::write(&b, "b\n".repeat(10001)).unwrap();
    code(
        call("file_diff", vec![s(&a), s(&b)], &mut env),
        "E_BUDGET_EXCEEDED",
    );
}
#[test]
fn sd_uses_native_regex_and_preserves_capture_and_line_semantics() {
    let _lock = LOCK.lock().unwrap();
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for name in ["sd", "sd_replace"] {
        assert_eq!(
            call(name, vec![s("[a-z]+"), s("x"), s("abc 123 def")], &mut env).unwrap(),
            s("x 123 x")
        );
        assert_eq!(
            call_with_input(
                name,
                vec![s("(?P<word>[a-z]+)-(\\d+)"), s("${word}:$2:$$")],
                Some(s("abc-12")),
                &mut env
            )
            .unwrap(),
            s("abc:12:$")
        );
        assert_eq!(
            call(name, vec![s("a"), s("x"), s("a\r\na")], &mut env).unwrap(),
            s("x\r\nx")
        );
        assert_eq!(
            call(name, vec![s(""), s("x"), s("é")], &mut env).unwrap(),
            s("xéx")
        );
        assert_eq!(
            call(name, vec![s("missing"), s("x"), s("original")], &mut env).unwrap(),
            s("original")
        );
    }
}
#[test]
fn sd_refuses_invalid_regex_types_and_conflicting_input() {
    let _lock = LOCK.lock().unwrap();
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for name in ["sd", "sd_replace"] {
        code(
            call(name, vec![s("["), s("x"), s("a")], &mut env),
            "E_BAD_ARG",
        );
        code(call(name, vec![s("a"), s("x")], &mut env), "E_BAD_ARG");
        code(
            call(name, vec![s("a"), s("x"), Value::Int(1)], &mut env),
            "E_BAD_ARG",
        );
        code(
            call_with_input(
                name,
                vec![s("a"), s("x"), s("explicit")],
                Some(s("piped")),
                &mut env,
            ),
            "E_BAD_ARG",
        );
        code(
            call_with_input(name, vec![s("a"), s("x")], Some(Value::Int(1)), &mut env),
            "E_BAD_ARG",
        );
        code(
            call(
                name,
                vec![s("a"), s("x"), s("a"), Value::Record(Default::default())],
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
}
#[test]
fn sd_is_declared_pure_and_runs_under_strict_agent_policy() {
    let _lock = LOCK.lock().unwrap();
    let _fixture = Fixture::new();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_POLICY", "strict");
    std::env::set_var("AETHER_MAX_FILES", "0");
    let mut env = Env::default();
    for name in ["sd", "sd_replace"] {
        assert_eq!(
            aethershell::safety::effect_of(name),
            aethershell::safety::Effect::Pure
        );
        assert!(aethershell::safety::effect_is_declared(name));
        assert_eq!(
            call(name, vec![s("a+"), s("x"), s("aaa")], &mut env).unwrap(),
            s("x")
        );
    }
}
fn sed_options(global: bool, file: bool) -> Value {
    Value::Record(std::collections::BTreeMap::from([
        ("global".into(), Value::Bool(global)),
        ("file".into(), Value::Bool(file)),
    ]))
}
#[test]
fn sed_preserves_capture_expansion_and_whole_string_match_semantics() {
    let _lock = LOCK.lock().unwrap();
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for name in ["sed", "sed_replace"] {
        assert_eq!(
            call(name, vec![s("a"), s("x"), s("a\r\na")], &mut env).unwrap(),
            s("x\r\nx")
        );
        assert_eq!(
            call_with_input(
                name,
                vec![s("a"), s("x"), sed_options(false, false)],
                Some(s("a\na")),
                &mut env
            )
            .unwrap(),
            s("x\na")
        );
        assert_eq!(
            call(
                name,
                vec![s("(?P<word>[a-z]+)-(\\d+)"), s("${word}:$2:$$"), s("ab-12")],
                &mut env
            )
            .unwrap(),
            s("ab:12:$")
        );
        assert_eq!(
            call(name, vec![s("missing"), s("x"), s("original")], &mut env).unwrap(),
            s("original")
        );
        assert_eq!(
            call(name, vec![s(""), s("x"), s("é")], &mut env).unwrap(),
            s("xéx")
        );
    }
}
#[test]
fn sed_refuses_bad_options_regex_and_conflicts_without_writing() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let path = fixture.path("original");
    std::fs::write(&path, "original").unwrap();
    let mut env = Env::default();
    for options in [
        Value::Bool(true),
        Value::Record(std::collections::BTreeMap::from([(
            "file".into(),
            s("true"),
        )])),
        Value::Record(std::collections::BTreeMap::from([(
            "global".into(),
            Value::Int(1),
        )])),
        Value::Record(std::collections::BTreeMap::from([(
            "unknown".into(),
            Value::Bool(true),
        )])),
    ] {
        code(
            call(
                "sed",
                vec![s("o"), s("x"), s(&path), options.clone()],
                &mut env,
            ),
            "E_BAD_ARG",
        );
        code(
            call_with_input(
                "sed_replace",
                vec![s("o"), s("x"), options],
                Some(s(&path)),
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
    code(
        call(
            "sed",
            vec![s("["), s("x"), s(&path), sed_options(true, true)],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call_with_input(
            "sed",
            vec![s("o"), s("x"), s(&path), sed_options(true, true)],
            Some(s("other")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(call("sed", vec![s("o"), s("x")], &mut env), "E_BAD_ARG");
    code(
        call_with_input("sed", vec![s("o"), s("x")], Some(Value::Int(1)), &mut env),
        "E_BAD_ARG",
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
}
#[test]
fn sed_file_mode_resolves_paths_and_rolls_back_atomic_edits() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    std::fs::write(fixture.root.join("input"), "aba").unwrap();
    let mut env = Env::default();
    aethershell::tx::begin().unwrap();
    let result = call(
        "sed",
        vec![s("a"), s("x"), s("input"), sed_options(false, true)],
        &mut env,
    )
    .unwrap();
    assert_eq!(
        result,
        Value::Record(std::collections::BTreeMap::from([
            ("file".into(), s("input")),
            ("status".into(), s("modified"))
        ]))
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("input")).unwrap(),
        "xba"
    );
    call_with_input(
        "sed_replace",
        vec![s("a"), s("y"), sed_options(true, true)],
        Some(s("input")),
        &mut env,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("input")).unwrap(),
        "xby"
    );
    aethershell::tx::rollback().unwrap();
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("input")).unwrap(),
        "aba"
    );
    assert!(!std::fs::read_dir(&fixture.root).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".aether_tmp_")));
}
#[test]
fn sed_file_mode_jails_aliases_and_bounds_reads() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let outside = fixture.path("outside");
    std::fs::write(&outside, "original").unwrap();
    let workspace = fixture.root.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    let mut env = Env::default();
    for name in ["sed", "sed_replace"] {
        code(
            call(
                name,
                vec![s("o"), s("x"), s(&outside), sed_options(true, true)],
                &mut env,
            ),
            "E_OUTSIDE_WORKSPACE",
        );
    }
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "original");
    std::fs::write(workspace.join("bad"), [0xff]).unwrap();
    code(
        call(
            "sed",
            vec![s("x"), s("y"), s("bad"), sed_options(true, true)],
            &mut env,
        ),
        "E_IO",
    );
    code(
        call(
            "sed",
            vec![s("x"), s("y"), s("missing"), sed_options(true, true)],
            &mut env,
        ),
        "E_NOT_FOUND",
    );
    std::fs::File::create(workspace.join("large"))
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    code(
        call(
            "sed",
            vec![s("x"), s("y"), s("large"), sed_options(true, true)],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
}
#[test]
fn awk_preserves_fields_whole_lines_and_literal_filter_semantics() {
    let _lock = LOCK.lock().unwrap();
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for name in ["awk", "awk_process"] {
        assert_eq!(
            call(
                name,
                vec![s("{print $2,$1,$2,$9}"), s("a b\r\nc d\r\n")],
                &mut env
            )
            .unwrap(),
            Value::Array(vec![s("b\ta\tb"), s("d\tc\td")])
        );
        assert_eq!(
            call_with_input(
                name,
                vec![s("/a.b/{print $0, $2}")],
                Some(s("a.b x\naxb y")),
                &mut env
            )
            .unwrap(),
            Value::Array(vec![s("a.b x\tx")])
        );
        assert_eq!(
            call(name, vec![s("/keep/"), s("keep a\nskip b")], &mut env).unwrap(),
            Value::Array(vec![s("keep a")])
        );
        assert_eq!(
            call(name, vec![s("print"), s("  a  b  ")], &mut env).unwrap(),
            Value::Array(vec![s("  a  b  ")])
        );
        assert_eq!(
            call(name, vec![s("$2,$1"), s("a\u{2003}b")], &mut env).unwrap(),
            Value::Array(vec![s("b\ta")])
        );
    }
}
#[test]
fn awk_honours_optional_separators_in_direct_and_piped_forms() {
    let _lock = LOCK.lock().unwrap();
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for separator in [",", "::", ""] {
        let input = if separator.is_empty() {
            "ab".to_string()
        } else {
            format!("a{separator}b")
        };
        let options = Value::Record(std::collections::BTreeMap::from([(
            "fs".into(),
            s(separator),
        )]));
        let expected = if separator.is_empty() { s("a") } else { s("b") };
        assert_eq!(
            call(
                "awk",
                vec![s("{print $2}"), s(&input), options.clone()],
                &mut env
            )
            .unwrap(),
            Value::Array(vec![expected.clone()])
        );
        assert_eq!(
            call_with_input(
                "awk_process",
                vec![s("{print $2}"), options],
                Some(s(&input)),
                &mut env
            )
            .unwrap(),
            Value::Array(vec![expected])
        );
    }
}
#[test]
fn awk_refuses_unhandled_programs_invalid_options_and_conflicts() {
    let _lock = LOCK.lock().unwrap();
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for program in [
        "",
        "{exit}",
        "{print 1}",
        "{print $1; exit}",
        "printf",
        "/a/{}",
    ] {
        // A malformed field token is a bad argument; unsupported actions are explicit refusals.
        let expected = if program.contains("$1;") {
            "E_BAD_ARG"
        } else {
            "E_UNIMPLEMENTED"
        };
        code(call("awk", vec![s(program), s("a")], &mut env), expected);
    }
    for program in [
        "/missing",
        "{print $1",
        "print $1}",
        "{print $-1}",
        "{print $}",
        "{print $18446744073709551616}",
    ] {
        code(call("awk", vec![s(program), s("a")], &mut env), "E_BAD_ARG");
    }
    for options in [
        Value::Bool(true),
        Value::Record(std::collections::BTreeMap::from([(
            "fs".into(),
            Value::Int(1),
        )])),
        Value::Record(std::collections::BTreeMap::from([(
            "unknown".into(),
            s(","),
        )])),
    ] {
        code(
            call("awk", vec![s("print"), s("a"), options.clone()], &mut env),
            "E_BAD_ARG",
        );
        code(
            call_with_input("awk", vec![s("print"), options], Some(s("a")), &mut env),
            "E_BAD_ARG",
        );
    }
    code(
        call_with_input(
            "awk",
            vec![s("print"), s("explicit"), Value::Record(Default::default())],
            Some(s("piped")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(call("awk", vec![s("print")], &mut env), "E_BAD_ARG");
    code(
        call_with_input("awk", vec![s("print")], Some(Value::Int(1)), &mut env),
        "E_BAD_ARG",
    );
}
#[test]
fn awk_bounds_field_reference_expansion() {
    let _lock = LOCK.lock().unwrap();
    let _fixture = Fixture::new();
    let mut env = Env::default();
    let program = std::iter::repeat_n("$1", 10000)
        .collect::<Vec<_>>()
        .join(",");
    let Value::Array(result) = call("awk", vec![s(&program), s("a")], &mut env).unwrap() else {
        panic!("expected rows")
    };
    assert_eq!(
        result[0],
        s(&std::iter::repeat_n("a", 10000)
            .collect::<Vec<_>>()
            .join("\t"))
    );
    code(
        call("awk", vec![s(&(program + ",$1")), s("a")], &mut env),
        "E_BUDGET_EXCEEDED",
    );
}
#[test]
fn cut_preserves_field_order_duplicates_and_literal_delimiters() {
    let _lock = LOCK.lock().unwrap();
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for name in ["cut", "cut_columns"] {
        assert_eq!(
            call(
                name,
                vec![s(","), s("3,1,3,9"), s("a,b,c\r\nx,y\r\n")],
                &mut env
            )
            .unwrap(),
            Value::Array(vec![s("c,a,c"), s("x")])
        );
        assert_eq!(
            call_with_input(
                name,
                vec![s("::"), s(" 2-3 ")],
                Some(s("a::b::c\nx::y")),
                &mut env
            )
            .unwrap(),
            Value::Array(vec![s("b::c"), s("y")])
        );
        assert_eq!(
            call_with_input(name, vec![s(","), Value::Int(2)], Some(s("a,b")), &mut env).unwrap(),
            Value::Array(vec![s("b")])
        );
        assert_eq!(
            call(name, vec![s(""), s("2-3"), s("ab")], &mut env).unwrap(),
            Value::Array(vec![s("ab")])
        );
        assert_eq!(
            call(name, vec![s(","), s("1"), s("")], &mut env).unwrap(),
            Value::Array(vec![])
        );
    }
}
#[test]
fn cut_refuses_malformed_conflicting_and_oversized_selections() {
    let _lock = LOCK.lock().unwrap();
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for spec in [
        "",
        "0",
        "-1",
        "3-1",
        "1-2-3",
        "1,",
        "1--2",
        "+1",
        "1.0",
        "18446744073709551616",
    ] {
        code(
            call("cut", vec![s(","), s(spec), s("a,b")], &mut env),
            "E_BAD_ARG",
        );
    }
    for spec in ["1-10001", "1-10000,1"] {
        code(
            call("cut", vec![s(","), s(spec), s("a")], &mut env),
            "E_BUDGET_EXCEEDED",
        );
    }
    let result = call("cut", vec![s(","), s("1-10000"), s("a")], &mut env).unwrap();
    assert_eq!(result, Value::Array(vec![s("a")]));
    for fields in [
        Value::Int(-1),
        Value::Int(0),
        Value::Float(1.0),
        Value::Bool(true),
    ] {
        code(
            call("cut", vec![s(","), fields, s("a")], &mut env),
            "E_BAD_ARG",
        );
    }
    code(
        call_with_input(
            "cut",
            vec![s(","), s("1"), s("explicit")],
            Some(s("piped")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call_with_input("cut", vec![s(","), s("1")], Some(Value::Int(1)), &mut env),
        "E_BAD_ARG",
    );
    code(call("cut", vec![s(","), s("1")], &mut env), "E_BAD_ARG");
}
fn options(append: bool) -> Value {
    Value::Record(std::collections::BTreeMap::from([(
        "append".into(),
        Value::Bool(append),
    )]))
}
fn code(result: anyhow::Result<Value>, expected: &str) {
    let e = result.unwrap_err();
    assert!(e.to_string().contains(expected), "{e}");
}
#[test]
fn tail_preserves_both_file_orders_and_pipeline_line_semantics() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let path = fixture.path("input");
    std::fs::write(&path, "a\r\nb\r\nc\r\n").unwrap();
    let mut env = Env::default();
    assert_eq!(
        call("tail", vec![s(&path)], &mut env).unwrap(),
        s("a\nb\nc")
    );
    for args in [vec![s(&path), Value::Int(2)], vec![Value::Int(2), s(&path)]] {
        assert_eq!(call("tail", args, &mut env).unwrap(), s("b\nc"));
    }
    assert_eq!(
        call_with_input("tail", vec![Value::Int(0)], Some(s("a\nb")), &mut env).unwrap(),
        s("")
    );
    assert_eq!(
        call_with_input("tail", vec![Value::Int(1)], Some(s("a\n\nb\n")), &mut env).unwrap(),
        s("b")
    );
    assert_eq!(
        call_with_input("tail", vec![], Some(s("")), &mut env).unwrap(),
        s("")
    );
    for count in [Value::Int(-1), Value::Float(1.0), s("bad")] {
        code(
            call_with_input("tail", vec![count.clone()], Some(s("a")), &mut env),
            "E_BAD_ARG",
        );
        code(call("tail", vec![s(&path), count], &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input("tail", vec![], Some(Value::Int(1)), &mut env),
        "E_BAD_ARG",
    );
}
#[test]
fn tail_bounds_reads_and_codes_missing_or_invalid_utf8_files() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let mut env = Env::default();
    code(
        call("tail", vec![s(&fixture.path("missing"))], &mut env),
        "E_NOT_FOUND",
    );
    let path = fixture.path("bad");
    std::fs::write(&path, [0xff]).unwrap();
    code(call("tail", vec![s(&path)], &mut env), "E_IO");
    let path = fixture.path("large");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    code(call("tail", vec![s(&path)], &mut env), "E_BUDGET_EXCEEDED");
}
#[test]
fn tee_overwrites_appends_and_formats_data_in_both_forms_and_aliases() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let path = fixture.path("output");
    let mut env = Env::default();
    let rendered = Value::Int(2).to_string();
    assert_eq!(
        call("tee", vec![s(&path), s("a")], &mut env).unwrap(),
        s("a")
    );
    assert_eq!(
        call(
            "tee_output",
            vec![s(&path), Value::Int(2), options(true)],
            &mut env
        )
        .unwrap(),
        s(&rendered)
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("a{rendered}")
    );
    assert_eq!(
        call_with_input("tee", vec![s(&path), options(true)], Some(s("b")), &mut env).unwrap(),
        s("b")
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("a{rendered}b")
    );
    call_with_input("tee", vec![s(&path)], Some(s("new")), &mut env).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    assert!(!std::fs::read_dir(&fixture.root).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".aether_tmp_")));
}
#[test]
fn malformed_tee_options_are_refused_without_clobbering() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let path = fixture.path("output");
    let mut env = Env::default();
    std::fs::write(&path, "original").unwrap();
    for bad in [
        Value::Bool(true),
        Value::Record(std::collections::BTreeMap::from([(
            "append".into(),
            s("true"),
        )])),
        Value::Record(std::collections::BTreeMap::from([(
            "unknown".into(),
            Value::Bool(true),
        )])),
    ] {
        code(
            call("tee", vec![s(&path), s("bad"), bad.clone()], &mut env),
            "E_BAD_ARG",
        );
        code(
            call_with_input("tee", vec![s(&path), bad], Some(s("bad")), &mut env),
            "E_BAD_ARG",
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    }
    code(
        call_with_input(
            "tee",
            vec![s(&path), s("explicit"), options(false)],
            Some(s("piped")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
}
#[test]
fn tee_jails_new_destinations_and_resolves_relative_paths_to_workspace() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let workspace = fixture.root.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    let mut env = Env::default();
    for name in ["tee", "tee_output"] {
        code(
            call(name, vec![s(&fixture.path("outside")), s("bad")], &mut env),
            "E_OUTSIDE_WORKSPACE",
        );
        assert!(!fixture.root.join("outside").exists());
    }
    call("tee", vec![s("inside"), s("good")], &mut env).unwrap();
    assert_eq!(
        std::fs::read_to_string(workspace.join("inside")).unwrap(),
        "good"
    );
}
#[test]
fn tee_snapshots_overwrites_appends_and_new_files_for_rollback() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let path = fixture.path("existing");
    let new = fixture.path("new");
    std::fs::write(&path, "original").unwrap();
    let mut env = Env::default();
    aethershell::tx::begin().unwrap();
    call("tee", vec![s(&path), s("replacement")], &mut env).unwrap();
    call(
        "tee",
        vec![s(&path), s("appended"), options(true)],
        &mut env,
    )
    .unwrap();
    call("tee", vec![s(&new), s("created")], &mut env).unwrap();
    aethershell::tx::rollback().unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    assert!(!std::path::Path::new(&new).exists());
}

#[test]
fn head_preserves_both_file_orders_and_pipeline_line_semantics() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let path = fixture.path("input");
    std::fs::write(&path, "a\r\nb\r\nc\r\n").unwrap();
    let mut env = Env::default();
    assert_eq!(
        call("head", vec![s(&path)], &mut env).unwrap(),
        s("a\nb\nc")
    );
    for args in [vec![s(&path), Value::Int(2)], vec![Value::Int(2), s(&path)]] {
        assert_eq!(call("head", args, &mut env).unwrap(), s("a\nb"));
    }
    assert_eq!(
        call_with_input("head", vec![Value::Int(0)], Some(s("a\nb")), &mut env).unwrap(),
        s("")
    );
    assert_eq!(
        call_with_input("head", vec![Value::Int(1)], Some(s("a\n\nb\n")), &mut env).unwrap(),
        s("a")
    );
    assert_eq!(
        call_with_input("head", vec![], Some(s("")), &mut env).unwrap(),
        s("")
    );
    for count in [Value::Int(-1), Value::Float(1.0), s("bad")] {
        code(
            call_with_input("head", vec![count.clone()], Some(s("a")), &mut env),
            "E_BAD_ARG",
        );
        code(call("head", vec![s(&path), count], &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input("head", vec![], Some(Value::Int(1)), &mut env),
        "E_BAD_ARG",
    );
}
#[test]
fn head_bounds_reads_and_codes_missing_or_invalid_utf8_files() {
    let _lock = LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let mut env = Env::default();
    code(
        call("head", vec![s(&fixture.path("missing"))], &mut env),
        "E_NOT_FOUND",
    );
    let path = fixture.path("bad");
    std::fs::write(&path, [0xff]).unwrap();
    code(call("head", vec![s(&path)], &mut env), "E_IO");
    let path = fixture.path("large");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    code(call("head", vec![s(&path)], &mut env), "E_BUDGET_EXCEEDED");
}
