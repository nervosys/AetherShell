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
