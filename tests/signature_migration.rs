use iron_shell::{
    builtins::{call, call_with_input},
    env::Env,
    eval::eval_program,
    parser::parse_program,
    value::Value,
};

struct FileFixture {
    root: std::path::PathBuf,
    directory: std::path::PathBuf,
}

#[test]
fn sleep_refuses_panicking_durations_and_surplus_arguments() {
    let mut env = Env::new();
    for duration in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, f64::MAX] {
        let error = call("sleep", vec![Value::Float(duration)], &mut env).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
    let error = call("sleep", vec![Value::Int(0), Value::Int(0)], &mut env).unwrap_err();
    assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    assert_eq!(
        call("sleep", vec![Value::Int(0)], &mut env).unwrap(),
        Value::Null
    );
}

#[test]
fn code_scanners_keep_line_numbers_and_refuse_discarded_options() {
    let mut env = Env::new();
    let file = Value::Str("tests/fixtures/signature-code.txt".into());
    for name in [
        "code_parse",
        "code_symbols",
        "code_outline",
        "code_imports",
        "code_dependencies",
        "code_exports",
        "code_comments",
        "code_todos",
    ] {
        let error = call(
            name,
            vec![file.clone(), Value::Str("rust".into())],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{name}: {error}");
        let fixture = FileFixture::new();
        let error = call(name, vec![fixture.path("absent.rs")], &mut env).unwrap_err();
        assert!(error.to_string().contains("E_NOT_FOUND"), "{name}: {error}");
    }
    let symbols = call("code_symbols", vec![file.clone()], &mut env).unwrap();
    assert_eq!(
        call("code_outline", vec![file.clone()], &mut env).unwrap(),
        symbols
    );
    let Value::Array(symbols) = symbols else {
        panic!("expected symbols")
    };
    let lines: Vec<_> = symbols
        .iter()
        .map(|s| match s {
            Value::Record(r) => r["line"].clone(),
            _ => panic!("expected symbol record"),
        })
        .collect();
    assert_eq!(lines, vec![Value::Int(3), Value::Int(5), Value::Int(7)]);
    assert_eq!(
        call("code_imports", vec![file.clone()], &mut env).unwrap(),
        call("code_dependencies", vec![file.clone()], &mut env).unwrap()
    );
    let Value::Record(parsed) = call("code_parse", vec![file], &mut env).unwrap() else {
        panic!("expected file metadata")
    };
    assert_eq!(
        parsed["size"],
        Value::Int(
            std::fs::metadata("tests/fixtures/signature-code.txt")
                .unwrap()
                .len() as i64
        )
    );
}

#[test]
fn memory_queries_return_complete_byte_records_and_refuse_options() {
    for (name, fields) in [
        ("sys_mem_info", ["total", "free", "available"]),
        ("sys_swap_info", ["total", "free", "used"]),
    ] {
        let mut env = Env::new();
        let error = call(name, vec![Value::Str("megabytes".into())], &mut env).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
        let value = call(name, vec![], &mut env);
        if !sysinfo::IS_SUPPORTED_SYSTEM {
            assert!(value.unwrap_err().to_string().contains("E_UNIMPLEMENTED"));
            continue;
        }
        let Value::Record(record) = value.unwrap() else {
            panic!("expected {name} record")
        };
        assert_eq!(record.len(), 3);
        let total = match record["total"] {
            Value::Int(n) => n,
            _ => panic!("expected bytes"),
        };
        for field in fields {
            let Value::Int(bytes) = record[field] else {
                panic!("expected {field} bytes")
            };
            assert!(
                bytes >= 0 && bytes <= total,
                "{name}.{field}={bytes}, total={total}"
            );
        }
        if name == "sys_swap_info" {
            let (Value::Int(free), Value::Int(used)) = (&record["free"], &record["used"]) else {
                unreachable!()
            };
            assert_eq!(free + used, total);
        } else {
            assert!(total > 0);
        }
    }
}

#[test]
fn csv_import_preserves_quoted_fields_and_supports_both_table_forms() {
    let fixture = FileFixture::new();
    let source = fixture.directory.join("source ' quoted.csv");
    let mut writer = csv::Writer::from_path(&source).unwrap();
    writer.write_record(["select", "note"]).unwrap();
    writer
        .write_record(["Ada's, λ", " leading\n\"quoted\" trailing "])
        .unwrap();
    writer.flush().unwrap();
    drop(writer);
    let mut env = Env::new();
    for (file, table, explicit) in [
        ("default ' db.sqlite", "imported", false),
        ("named.db", "people", true),
    ] {
        let mut args = vec![fixture.path(file), fixture.path("source ' quoted.csv")];
        if explicit {
            args.push(Value::Str(table.into()));
        }
        assert_eq!(
            call("db_csv_to_sqlite", args, &mut env).unwrap(),
            Value::Bool(true)
        );
        let db = rusqlite::Connection::open(fixture.directory.join(file)).unwrap();
        let row: (String, String) = db
            .query_row(&format!("SELECT \"select\", note FROM {table}"), [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(
            row,
            ("Ada's, λ".into(), " leading\n\"quoted\" trailing ".into())
        );
    }
}

#[test]
fn csv_import_validates_before_creating_destination() {
    let fixture = FileFixture::new();
    let mut env = Env::new();
    for content in [
        "",
        "name,age\nAda\n",
        "name,NAME\nAda,42\n",
        "bad-name\nAda\n",
    ] {
        std::fs::write(fixture.directory.join("bad.csv"), content).unwrap();
        let error = call(
            "db_csv_to_sqlite",
            vec![fixture.path("absent.db"), fixture.path("bad.csv")],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
        assert!(!fixture.directory.join("absent.db").exists());
    }
    let error = call(
        "db_csv_to_sqlite",
        vec![fixture.path("absent.db"), fixture.path("missing.csv")],
        &mut env,
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_NOT_FOUND"), "{error}");
    assert!(!fixture.directory.join("absent.db").exists());
}

#[test]
fn csv_import_rolls_back_all_rows_after_constraint_failure() {
    let fixture = FileFixture::new();
    let destination = fixture.directory.join("existing.db");
    let db = rusqlite::Connection::open(&destination).unwrap();
    db.execute_batch("CREATE TABLE people(name TEXT, age INTEGER CHECK(age >= 0)); INSERT INTO people VALUES('original', 10)").unwrap();
    drop(db);
    std::fs::write(
        fixture.directory.join("rows.csv"),
        "name,age\nvalid,42\ninvalid,-1\n",
    )
    .unwrap();
    let error = call(
        "db_csv_to_sqlite",
        vec![
            fixture.path("existing.db"),
            fixture.path("rows.csv"),
            Value::Str("people".into()),
        ],
        &mut Env::new(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    let db = rusqlite::Connection::open(destination).unwrap();
    let count: i64 = db
        .query_row("SELECT count(*) FROM people", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn sqlite_table_export_is_typed_and_does_not_create_missing_databases() {
    let fixture = FileFixture::new();
    let mut env = Env::new();
    let error = call(
        "db_sqlite_to_json",
        vec![fixture.path("missing.db"), Value::Str("example".into())],
        &mut env,
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_NOT_FOUND"), "{error}");
    assert!(!fixture.directory.join("missing.db").exists());
    let rows = call(
        "db_sqlite_to_json",
        vec![
            Value::Str("tests/fixtures/sample.db".into()),
            Value::Str("people".into()),
        ],
        &mut env,
    )
    .unwrap();
    let Value::Array(rows) = rows else {
        panic!("expected typed records")
    };
    assert_eq!(rows.len(), 2);
    let Value::Record(first) = &rows[0] else {
        panic!("expected record")
    };
    assert_eq!(first["age"], Value::Int(36));
}

#[test]
fn sql_import_rejects_missing_files_and_dot_commands_before_creating_a_database() {
    let fixture = FileFixture::new();
    let mut env = Env::new();
    let error = call(
        "db_sqlite_import",
        vec![fixture.path("absent.db"), fixture.path("missing.sql")],
        &mut env,
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_NOT_FOUND"), "{error}");
    std::fs::write(
        fixture.directory.join("unsafe.sql"),
        ".shell echo should-not-run",
    )
    .unwrap();
    let error = call(
        "db_sqlite_import",
        vec![fixture.path("absent.db"), fixture.path("unsafe.sql")],
        &mut env,
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    assert!(!fixture.directory.join("absent.db").exists());
}

#[test]
fn sqlite_csv_export_validates_destinations_and_codes_tool_and_write_failures() {
    let fixture = FileFixture::new();
    let mut env = Env::new();
    let source = Value::Str("tests/fixtures/sample.db".into());
    for destination in [Value::Int(1), Value::Uri("file:///ignored".into())] {
        let error = call(
            "db_sqlite_to_csv",
            vec![source.clone(), Value::Str("people".into()), destination],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
    let csv = call(
        "db_sqlite_to_csv",
        vec![
            source.clone(),
            Value::Str("SELECT name, age FROM people ORDER BY name".into()),
        ],
        &mut env,
    );
    if let Err(error) = &csv {
        assert!(error.to_string().contains("E_TOOL_MISSING"), "{error}");
        return;
    }
    let Value::Str(csv) = csv.unwrap() else {
        panic!("expected CSV text")
    };
    let mut reader = csv::Reader::from_reader(csv.as_bytes());
    let rows = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(&rows[0][0], "ada");
    let destination = fixture.path("people.csv");
    assert_eq!(
        call(
            "db_sqlite_to_csv",
            vec![
                source.clone(),
                Value::Str("people".into()),
                destination.clone()
            ],
            &mut env
        )
        .unwrap(),
        destination
    );
    assert!(fixture.directory.join("people.csv").is_file());
    let error = call(
        "db_sqlite_to_csv",
        vec![source.clone(), Value::Str("missing_table".into())],
        &mut env,
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_TOOL_FAILED"), "{error}");
    let error = call(
        "db_sqlite_to_csv",
        vec![
            source,
            Value::Str("people".into()),
            fixture.path("missing/people.csv"),
        ],
        &mut env,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("E_NOT_FOUND") || error.to_string().contains("E_IO"),
        "{error}"
    );
}

#[test]
fn sql_file_import_executes_statements_and_surplus_arguments_are_refused() {
    let fixture = FileFixture::new();
    std::fs::write(
        fixture.directory.join("schema.sql"),
        "CREATE TABLE example(value INTEGER); INSERT INTO example VALUES(42);",
    )
    .unwrap();
    let mut env = Env::new();
    let result = call(
        "db_sqlite_import",
        vec![fixture.path("imported.db"), fixture.path("schema.sql")],
        &mut env,
    );
    if let Err(error) = &result {
        assert!(error.to_string().contains("E_TOOL_MISSING"), "{error}");
        return;
    }
    assert_eq!(result.unwrap(), Value::Bool(true));
    let db = rusqlite::Connection::open(fixture.directory.join("imported.db")).unwrap();
    assert_eq!(
        db.query_row("SELECT value FROM example", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        42
    );
    drop(db);
    let error = call(
        "db_sqlite_import",
        vec![
            fixture.path("absent.db"),
            fixture.path("schema.sql"),
            Value::Bool(true),
        ],
        &mut env,
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    assert!(!fixture.directory.join("absent.db").exists());
}

#[test]
fn sqlite_record_cells_refuse_unsupported_types_before_opening_a_database() {
    let fixture = FileFixture::new();
    let mut env = Env::new();
    for value in [
        Value::Array(vec![Value::Int(1)]),
        Value::Record(Default::default()),
        Value::Float(f64::NAN),
        Value::Float(f64::INFINITY),
    ] {
        let record = Value::Record(std::collections::BTreeMap::from([("value".into(), value)]));
        for name in ["db_sqlite_insert", "db_sqlite_update"] {
            let mut args = vec![
                fixture.path("absent.db"),
                Value::Str("example".into()),
                record.clone(),
            ];
            if name == "db_sqlite_update" {
                args.push(Value::Str("1=1".into()));
            }
            let error = call(name, args, &mut env).unwrap_err();
            assert!(error.to_string().contains("E_BAD_ARG"), "{name}: {error}");
            assert!(!fixture.directory.join("absent.db").exists());
        }
    }
}

#[test]
fn sqlite_mutation_shapes_refuse_invalid_or_empty_arguments_before_writing() {
    let fixture = FileFixture::new();
    let mut env = Env::new();
    let empty = Value::Record(Default::default());
    let rows = [
        (
            "db_sqlite_insert",
            vec![
                fixture.path("absent.db"),
                Value::Str("example".into()),
                empty.clone(),
            ],
        ),
        (
            "db_sqlite_update",
            vec![
                fixture.path("absent.db"),
                Value::Str("example".into()),
                empty.clone(),
                Value::Str("1=1".into()),
            ],
        ),
        (
            "db_sqlite_create_table",
            vec![
                fixture.path("absent.db"),
                Value::Str("example".into()),
                empty,
            ],
        ),
        (
            "db_sqlite_create_table",
            vec![
                fixture.path("absent.db"),
                Value::Str("example".into()),
                Value::Str(" ".into()),
            ],
        ),
        (
            "db_sqlite_create_table",
            vec![
                fixture.path("absent.db"),
                Value::Str("example".into()),
                Value::Record(std::collections::BTreeMap::from([(
                    "value".into(),
                    Value::Int(4),
                )])),
            ],
        ),
        (
            "db_sqlite_delete",
            vec![
                fixture.path("absent.db"),
                Value::Str("example".into()),
                Value::Int(1),
            ],
        ),
        (
            "db_sqlite_delete",
            vec![
                fixture.path("absent.db"),
                Value::Str("example".into()),
                Value::Str(" ".into()),
            ],
        ),
        (
            "db_sqlite_drop_table",
            vec![
                fixture.path("absent.db"),
                Value::Str("example".into()),
                Value::Bool(true),
            ],
        ),
    ];
    for (name, args) in rows {
        let error = call(name, args, &mut env).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{name}: {error}");
        assert!(!fixture.directory.join("absent.db").exists());
    }
}

#[test]
fn sqlite_mutations_preserve_scalar_cells_and_default_delete_is_a_noop() {
    let fixture = FileFixture::new();
    let mut env = Env::new();
    let created = call(
        "db_sqlite_create_table",
        vec![
            fixture.path("rows.db"),
            Value::Str("example".into()),
            Value::Str("text TEXT, number INTEGER, fraction REAL, flag INTEGER, empty TEXT".into()),
        ],
        &mut env,
    );
    if let Err(error) = &created {
        assert!(error.to_string().contains("E_TOOL_MISSING"), "{error}");
        return;
    }
    assert_eq!(created.unwrap(), Value::Bool(true));
    let record = Value::Record(std::collections::BTreeMap::from([
        ("text".into(), Value::Str("O'Brien 世界".into())),
        ("number".into(), Value::Int(42)),
        ("fraction".into(), Value::Float(1.5)),
        ("flag".into(), Value::Bool(true)),
        ("empty".into(), Value::Null),
    ]));
    assert_eq!(
        call(
            "db_sqlite_insert",
            vec![
                fixture.path("rows.db"),
                Value::Str("example".into()),
                record
            ],
            &mut env
        )
        .unwrap(),
        Value::Bool(true)
    );
    let db = rusqlite::Connection::open(fixture.directory.join("rows.db")).unwrap();
    let row = db
        .query_row(
            "SELECT text, number, fraction, flag, empty FROM example",
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, f64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(row, ("O'Brien 世界".into(), 42, 1.5, 1, None));
    drop(db);
    assert_eq!(
        call(
            "db_sqlite_delete",
            vec![fixture.path("rows.db"), Value::Str("example".into())],
            &mut env
        )
        .unwrap(),
        Value::Bool(true)
    );
    let db = rusqlite::Connection::open(fixture.directory.join("rows.db")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM example", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(db);
    let update = Value::Record(std::collections::BTreeMap::from([(
        "number".into(),
        Value::Int(43),
    )]));
    call(
        "db_sqlite_update",
        vec![
            fixture.path("rows.db"),
            Value::Str("example".into()),
            update,
            Value::Str("number = 42".into()),
        ],
        &mut env,
    )
    .unwrap();
    let db = rusqlite::Connection::open(fixture.directory.join("rows.db")).unwrap();
    assert_eq!(
        db.query_row("SELECT number FROM example", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        43
    );
    drop(db);
    call(
        "db_sqlite_delete",
        vec![
            fixture.path("rows.db"),
            Value::Str("example".into()),
            Value::Str("number = 43".into()),
        ],
        &mut env,
    )
    .unwrap();
    let db = rusqlite::Connection::open(fixture.directory.join("rows.db")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM example", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(db);
    call(
        "db_sqlite_drop_table",
        vec![fixture.path("rows.db"), Value::Str("example".into())],
        &mut env,
    )
    .unwrap();
    let db = rusqlite::Connection::open(fixture.directory.join("rows.db")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'example'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    drop(db);
}

#[test]
fn sqlite_mutations_refuse_extra_arguments_before_creating_a_database() {
    let fixture = FileFixture::new();
    let mut env = Env::new();
    for name in ["db_sqlite_exec", "sqlite_exec", "db_kv_set", "db_kv_delete"] {
        let args = match name {
            "db_sqlite_exec" | "sqlite_exec" => vec![
                fixture.path("absent.db"),
                Value::Str("CREATE TABLE example(value TEXT)".into()),
                Value::Bool(true),
            ],
            "db_kv_set" => vec![
                fixture.path("absent.db"),
                Value::Str("key".into()),
                Value::Str("value".into()),
                Value::Bool(true),
            ],
            _ => vec![
                fixture.path("absent.db"),
                Value::Str("key".into()),
                Value::Bool(true),
            ],
        };
        let error = call(name, args, &mut env).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{name}: {error}");
        assert!(!fixture.directory.join("absent.db").exists());
    }
}

#[test]
fn csv_loading_preserves_quoted_fields_and_refuses_ragged_rows() {
    let fixture = FileFixture::new();
    let path = fixture.directory.join("quoted.csv");
    std::fs::write(
        &path,
        "name,note,age\nAda,\"hello, world\",42\nGrace,\"two\nlines\",85\n",
    )
    .unwrap();
    let mut env = Env::new();
    for name in ["db_csv_query", "db_csv_to_json"] {
        let Value::Array(rows) = call(name, vec![fixture.path("quoted.csv")], &mut env).unwrap()
        else {
            panic!("expected records")
        };
        let Value::Record(first) = &rows[0] else {
            panic!("expected record")
        };
        assert_eq!(first["note"], Value::Str("hello, world".into()));
        assert_eq!(first["age"], Value::Str("42".into()));
        let Value::Record(second) = &rows[1] else {
            panic!("expected record")
        };
        assert_eq!(second["note"], Value::Str("two\nlines".into()));
        let error = call(
            name,
            vec![fixture.path("quoted.csv"), Value::Str("SELECT *".into())],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
    std::fs::write(&path, "a,b\n1,2,3\n").unwrap();
    let error = call("db_csv_query", vec![fixture.path("quoted.csv")], &mut env).unwrap_err();
    assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    std::fs::write(&path, "").unwrap();
    assert_eq!(
        call("db_csv_query", vec![fixture.path("quoted.csv")], &mut env).unwrap(),
        Value::Array(vec![])
    );
}

#[test]
fn key_value_literals_are_validated_before_store_creation() {
    let fixture = FileFixture::new();
    let mut env = Env::new();
    for (key, value) in [("bad\0key", "value"), ("key", "bad\0value")] {
        let error = call(
            "db_kv_set",
            vec![
                fixture.path("absent.db"),
                Value::Str(key.into()),
                Value::Str(value.into()),
            ],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
        assert!(!fixture.directory.join("absent.db").exists());
    }
    for name in ["db_kv_set", "db_kv_delete"] {
        let error = call(
            name,
            vec![
                Value::Int(3),
                Value::Str("key".into()),
                Value::Str("value".into()),
            ],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn key_value_writes_refuse_views_and_quoted_keys_round_trip() {
    let fixture = FileFixture::new();
    let db = rusqlite::Connection::open(fixture.directory.join("view.db")).unwrap();
    db.execute_batch("CREATE VIEW kv AS SELECT 'x' AS key, 'y' AS value;")
        .unwrap();
    drop(db);
    let mut env = Env::new();
    let error = call(
        "db_kv_set",
        vec![
            fixture.path("view.db"),
            Value::Str("key".into()),
            Value::Str("value".into()),
        ],
        &mut env,
    )
    .unwrap_err();
    if error.to_string().contains("E_TOOL_MISSING") {
        return;
    }
    assert!(error.to_string().contains("E_TOOL_FAILED"), "{error}");
    assert!(error.to_string().contains("cannot modify kv"), "{error}");
    let key = Value::Str("key'; DROP TABLE kv; --".into());
    assert_eq!(
        call(
            "db_kv_set",
            vec![
                fixture.path("store.db"),
                key.clone(),
                Value::Str("hello'世界".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Bool(true)
    );
    let db = rusqlite::Connection::open(fixture.directory.join("store.db")).unwrap();
    assert_eq!(
        db.query_row("SELECT value FROM kv", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "hello'世界"
    );
    drop(db);
    assert_eq!(
        call(
            "db_kv_delete",
            vec![fixture.path("store.db"), key],
            &mut env
        )
        .unwrap(),
        Value::Bool(true)
    );
    let db = rusqlite::Connection::open(fixture.directory.join("store.db")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM kv", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(db);
}

#[test]
fn temporary_directories_are_unique_under_concurrent_creation() {
    temporary_paths_are_unique("fs_tempdir");
}

#[test]
fn temporary_files_are_unique_under_concurrent_creation() {
    temporary_paths_are_unique("fs_tempfile");
}

fn temporary_paths_are_unique(name: &'static str) {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let paths = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let barrier = barrier.clone();
                scope.spawn(move || {
                    let mut env = Env::new();
                    barrier.wait();
                    (0..16)
                        .map(|_| {
                            let Value::Str(path) =
                                call(name, vec![Value::Str("ae_unique_test".into())], &mut env)
                                    .unwrap()
                            else {
                                panic!("expected path")
                            };
                            std::path::PathBuf::from(path)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    let root = std::env::temp_dir().canonicalize().unwrap();
    let unique: std::collections::BTreeSet<_> = paths.iter().collect();
    for path in &unique {
        assert_eq!(path.parent().unwrap().canonicalize().unwrap(), root);
        if path.is_dir() {
            std::fs::remove_dir(path).unwrap();
        } else {
            std::fs::remove_file(path).unwrap();
        }
    }
    assert_eq!(paths.len(), unique.len());
    for prefix in [
        Value::Str("../outside".into()),
        Value::Str("C:outside".into()),
        Value::Uri("file:///outside".into()),
    ] {
        let error = call(name, vec![prefix], &mut Env::new()).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"));
    }
}

#[test]
fn sqlite_schema_treats_table_names_as_literals() {
    let fixture = FileFixture::new();
    let path = fixture.directory.join("schema.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TABLE people(name TEXT, age INTEGER); CREATE TABLE other(secret TEXT);",
    )
    .unwrap();
    drop(db);
    let mut env = Env::new();
    let path = Value::Str(path.to_string_lossy().into_owned());
    assert_eq!(
        call(
            "db_sqlite_schema",
            vec![
                path.clone(),
                Value::Str("people'); SELECT * FROM other; --".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Array(vec![])
    );
    let Value::Array(columns) = call(
        "db_sqlite_schema",
        vec![path.clone(), Value::Str("people".into())],
        &mut env,
    )
    .unwrap() else {
        panic!("expected column metadata")
    };
    assert_eq!(columns.len(), 2);
    for name in ["db_sqlite_schema", "db_sqlite_backup"] {
        let error = call(name, vec![path.clone(), Value::Int(1)], &mut env).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn sqlite_backup_failures_are_coded_and_success_preserves_data() {
    let fixture = FileFixture::new();
    let source = fixture.directory.join("source.db");
    let db = rusqlite::Connection::open(&source).unwrap();
    db.execute_batch("CREATE TABLE sample(value INTEGER); INSERT INTO sample VALUES(42);")
        .unwrap();
    drop(db);
    let mut env = Env::new();
    let result = call(
        "db_sqlite_backup",
        vec![fixture.path("source.db"), fixture.path("copy.db")],
        &mut env,
    );
    if let Err(error) = &result {
        assert!(error.to_string().contains("E_TOOL_MISSING"), "{error}");
        return;
    }
    assert_eq!(result.unwrap(), Value::Bool(true));
    let copy = rusqlite::Connection::open(fixture.directory.join("copy.db")).unwrap();
    assert_eq!(
        copy.query_row("SELECT value FROM sample", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        42
    );
    drop(copy);
    let error = call(
        "db_sqlite_backup",
        vec![fixture.path("source.db"), fixture.path("missing/copy.db")],
        &mut env,
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_TOOL_FAILED"), "{error}");
}

impl FileFixture {
    fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap();
        let directory = root.join(format!("ae_signature_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        Self { root, directory }
    }
    fn path(&self, name: &str) -> Value {
        Value::Str(self.directory.join(name).to_string_lossy().into_owned())
    }
}

impl Drop for FileFixture {
    fn drop(&mut self) {
        assert_eq!(self.directory.parent(), Some(self.root.as_path()));
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn concurrent_file_writes_do_not_share_a_temporary_file() {
    let fixture = FileFixture::new();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    std::thread::scope(|scope| {
        for index in 0..8 {
            let path = fixture.path(&format!("writer-{index}"));
            let barrier = barrier.clone();
            scope.spawn(move || {
                let mut env = Env::new();
                let text = format!("writer-{index}:").repeat(8192);
                barrier.wait();
                for _ in 0..12 {
                    call(
                        "file_write",
                        vec![path.clone(), Value::Str(text.clone())],
                        &mut env,
                    )
                    .unwrap();
                    let Value::Str(filename) = &path else {
                        unreachable!()
                    };
                    assert_eq!(std::fs::read_to_string(filename).unwrap(), text);
                }
            });
        }
    });
    assert_eq!(std::fs::read_dir(&fixture.directory).unwrap().count(), 8);
}

#[test]
fn failed_atomic_write_is_coded_and_removes_its_temporary_file() {
    let fixture = FileFixture::new();
    let destination = fixture.directory.join("directory");
    std::fs::create_dir(&destination).unwrap();
    let error = call(
        "file_write",
        vec![fixture.path("directory"), Value::Str("x".into())],
        &mut Env::new(),
    )
    .unwrap_err();
    let structured: serde_json::Value = serde_json::from_str(&error.to_string()).unwrap();
    assert!(
        matches!(
            structured["error"]["code"].as_str(),
            Some("E_IO" | "E_OS_DENIED")
        ),
        "{error}"
    );
    assert_eq!(std::fs::read_dir(&fixture.directory).unwrap().count(), 1);
}

#[test]
fn malformed_patches_are_refused_before_any_file_is_changed() {
    let fixture = FileFixture::new();
    std::fs::write(fixture.directory.join("x"), "hello").unwrap();
    let patch = Value::Record(std::collections::BTreeMap::from([
        ("find".into(), Value::Str("hello".into())),
        ("replace".into(), Value::Str("changed".into())),
    ]));
    let error = call(
        "file_patch",
        vec![
            fixture.path("x"),
            Value::Array(vec![patch, Value::Bool(false)]),
        ],
        &mut Env::new(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    assert_eq!(
        std::fs::read_to_string(fixture.directory.join("x")).unwrap(),
        "hello"
    );
}

#[test]
fn missing_unicode_patch_text_does_not_panic_or_corrupt_the_file() {
    let fixture = FileFixture::new();
    std::fs::write(fixture.directory.join("x"), "hello").unwrap();
    let result = call(
        "file_replace",
        vec![
            fixture.path("x"),
            Value::Str("界".repeat(50)),
            Value::Str("changed".into()),
        ],
        &mut Env::new(),
    )
    .unwrap();
    let Value::Record(result) = result else {
        panic!("expected patch report")
    };
    assert_eq!(result["success"], Value::Bool(false));
    assert_eq!(
        std::fs::read_to_string(fixture.directory.join("x")).unwrap(),
        "hello"
    );
}

#[test]
fn telemetry_filters_refuse_wrong_types_and_unsupported_fields() {
    for (key, value) in [
        ("category", Value::Int(1)),
        ("since", Value::Str("yesterday".into())),
    ] {
        let options = Value::Record(std::collections::BTreeMap::from([(key.into(), value)]));
        let error = call("telemetry_report", vec![options], &mut Env::new()).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn metadata_options_do_not_silently_turn_wrong_shapes_into_defaults() {
    let cases = [
        ("tool_schema", Value::Uri("https://example.com".into())),
        (
            "telemetry_report",
            Value::Table(iron_shell::value::Table {
                rows: vec![],
                schema: vec![],
            }),
        ),
    ];
    for (name, value) in cases {
        let error = call(name, vec![value], &mut Env::new()).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn random_generators_honor_lengths_and_validate_character_sets() {
    let mut env = Env::new();
    assert_eq!(
        call("crypto_random_bytes", vec![Value::Int(0)], &mut env).unwrap(),
        Value::Str(String::new())
    );
    assert_eq!(
        call(
            "crypto_random_string",
            vec![Value::Int(300), Value::Str("界".into())],
            &mut env
        )
        .unwrap(),
        Value::Str("界".repeat(300))
    );
    // More than 256 distinct characters previously caused the Windows rejection
    // sampler to loop forever and Unix to return an empty string.
    let charset: String = (0x400..0x600).filter_map(char::from_u32).collect();
    let Value::Str(text) = call(
        "crypto_random_string",
        vec![Value::Int(300), Value::Str(charset.clone())],
        &mut env,
    )
    .unwrap() else {
        panic!("expected string")
    };
    assert_eq!(text.chars().count(), 300);
    assert!(text.chars().all(|c| charset.contains(c)));
    for (name, args) in [
        ("crypto_random_bytes", vec![Value::Int(-1)]),
        ("crypto_random_bytes", vec![Value::Int(i64::MAX)]),
        (
            "crypto_random_string",
            vec![Value::Int(2), Value::Str(String::new())],
        ),
    ] {
        let error = call(name, args, &mut env).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
    let Value::Str(uuid) = call("uuid", vec![], &mut env).unwrap() else {
        panic!("expected UUID")
    };
    assert_eq!(uuid::Uuid::parse_str(&uuid).unwrap().get_version_num(), 4);
}

#[test]
fn agentic_binary_refuses_truncating_codes_and_bytes() {
    let mut env = Env::new();
    for (message, opcode) in [(-1, 0), (256, 0), (0, -1), (0, 256)] {
        let error = call(
            "ab_encode",
            vec![
                Value::Int(message),
                Value::Int(opcode),
                Value::Str("x".into()),
            ],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
    for bytes in [
        vec![],
        vec![Value::Int(256)],
        vec![Value::Int(-1)],
        vec![Value::Int(20), Value::Int(5)],
    ] {
        let error = call("ab_decode", vec![Value::Array(bytes)], &mut env).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn agentic_binary_refuses_overflowing_payload_lengths_without_panicking() {
    for last in [1, 2, 127] {
        let mut bytes = vec![Value::Int(20)];
        bytes.extend(vec![Value::Int(255); 9]);
        bytes.push(Value::Int(last));
        let error = call("ab_decode", vec![Value::Array(bytes)], &mut Env::new()).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn product_overflow_and_non_numeric_cells_are_coded_refusals() {
    for values in [
        vec![Value::Int(i64::MAX), Value::Int(2)],
        vec![Value::Str("x".into())],
    ] {
        let error = call("product", vec![Value::Array(values)], &mut Env::new()).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn sort_key_errors_are_propagated_instead_of_becoming_null() {
    let program = parse_program("[1,2] | sort_by(fn(x) => no_such_key(x))").unwrap();
    let error = eval_program(&program, &mut Env::new()).unwrap_err();
    assert!(error.to_string().contains("no_such_key"), "{error}");
}

#[test]
fn newly_declared_aliases_refuse_discarded_arguments() {
    for name in [
        "from_csv",
        "from-csv",
        "from_yaml",
        "from-yaml",
        "to_yaml",
        "to-yaml",
        "product",
        "concat",
        "reduce",
        "Some",
    ] {
        let args = vec![Value::Null; 8];
        let error = call(name, args, &mut Env::new()).unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{name}: {error}");
    }
}

#[test]
fn url_decoding_round_trips_utf8_and_refuses_malformed_data() {
    let mut env = Env::new();
    let text = Value::Str("é 世界".into());
    let encoded = call("web_encode_url", vec![text.clone()], &mut env).unwrap();
    assert_eq!(
        call("web_decode_url", vec![encoded], &mut env).unwrap(),
        text
    );
    for malformed in ["%", "%G0", "%FF"] {
        let error = call(
            "web_decode_url",
            vec![Value::Str(malformed.into())],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn extraction_keeps_a_token_at_the_end_of_input() {
    let mut env = Env::new();
    for (name, text, expected) in [
        ("web_extract_emails", "a@example.com", "a@example.com"),
        ("web_extract_phones", "5551234567", "5551234567"),
    ] {
        assert_eq!(
            call(name, vec![Value::Str(text.into())], &mut env).unwrap(),
            Value::Array(vec![Value::Str(expected.into())])
        );
    }
}

#[test]
fn malformed_json_indexes_do_not_silently_select_zero() {
    let mut env = Env::new();
    for path in ["$[bad]", "$[0", "$[-1]", "$[0]]"] {
        let error = call(
            "web_json_path",
            vec![Value::Array(vec![Value::Int(7)]), Value::Str(path.into())],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn field_previews_do_not_slice_through_a_unicode_character() {
    let mut env = Env::new();
    let text = "界".repeat(60);
    let value = Value::Record(std::collections::BTreeMap::from([(
        "text".into(),
        Value::Str(text),
    )]));
    let Value::Array(fields) = call("type_fields", vec![value], &mut env).unwrap() else {
        panic!("expected fields")
    };
    let Value::Record(field) = &fields[0] else {
        panic!("expected field record")
    };
    let Value::Str(preview) = &field["preview"] else {
        panic!("expected preview")
    };
    assert_eq!(preview.matches('界').count(), 50);
}

#[test]
fn hmac_matches_reference_vectors_and_refuses_invalid_algorithms() {
    let mut env = Env::new();
    assert_eq!(
        call(
            "crypto_hmac",
            vec![
                Value::Str("The quick brown fox jumps over the lazy dog".into()),
                Value::Str("key".into()),
                Value::Str("md5".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Str("80070713463e7749b90c2dc24911e275".into())
    );
    assert_eq!(call("crypto_hmac", vec![Value::Str("Unicode message".into()), Value::Str("KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK".into()), Value::Str("md5".into())], &mut env).unwrap(), Value::Str("1d83714680314ccd9d530f50e8953641".into()));
    assert_eq!(
        call(
            "crypto_hmac",
            vec![
                Value::Str("".into()),
                Value::Str("".into()),
                Value::Str("md5".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Str("74e6f7298a9c2d168935f58c001bad88".into())
    );
    assert_eq!(
        call(
            "crypto_hmac",
            vec![
                Value::Str("The quick brown fox jumps over the lazy dog".into()),
                Value::Str("key".into()),
                Value::Str("sha1".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Str("de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9".into())
    );
    assert_eq!(call("crypto_hmac", vec![Value::Str("Unicode message".into()), Value::Str("KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK".into()), Value::Str("sha1".into())], &mut env).unwrap(), Value::Str("65bd75815d9009090a16197d00b5c95d19982a0e".into()));
    assert_eq!(
        call(
            "crypto_hmac",
            vec![
                Value::Str("".into()),
                Value::Str("".into()),
                Value::Str("sha1".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Str("fbdb1d1b18aa6c08324b7d64b71fb76370690e1d".into())
    );
    assert_eq!(
        call(
            "crypto_hmac",
            vec![
                Value::Str("The quick brown fox jumps over the lazy dog".into()),
                Value::Str("key".into()),
                Value::Str("sha256".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Str("f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8".into())
    );
    assert_eq!(call("crypto_hmac", vec![Value::Str("Unicode message".into()), Value::Str("KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK".into()), Value::Str("sha256".into())], &mut env).unwrap(), Value::Str("ac7de0f744aa272d3f8ba6c8f4be985197401dc443e722b995a2a115cbe8e5e4".into()));
    assert_eq!(
        call(
            "crypto_hmac",
            vec![
                Value::Str("".into()),
                Value::Str("".into()),
                Value::Str("sha256".into())
            ],
            &mut env
        )
        .unwrap(),
        Value::Str("b613679a0814d9ec772f95d778c35fc5ff1697c493715653c6c712144292c5ad".into())
    );
    assert_eq!(call("crypto_hmac", vec![Value::Str("The quick brown fox jumps over the lazy dog".into()), Value::Str("key".into()), Value::Str("sha384".into())], &mut env).unwrap(), Value::Str("d7f4727e2c0b39ae0f1e40cc96f60242d5b7801841cea6fc592c5d3e1ae50700582a96cf35e1e554995fe4e03381c237".into()));
    assert_eq!(call("crypto_hmac", vec![Value::Str("Unicode message".into()), Value::Str("KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK".into()), Value::Str("sha384".into())], &mut env).unwrap(), Value::Str("1ffc837d4db60727c4ebcd6d6beae29a3578acbfb88c661e9b7a9de93526a6287531e51ff43fde78354817e723270718".into()));
    assert_eq!(call("crypto_hmac", vec![Value::Str("".into()), Value::Str("".into()), Value::Str("sha384".into())], &mut env).unwrap(), Value::Str("6c1f2ee938fad2e24bd91298474382ca218c75db3d83e114b3d4367776d14d3551289e75e8209cd4b792302840234adc".into()));
    assert_eq!(call("crypto_hmac", vec![Value::Str("The quick brown fox jumps over the lazy dog".into()), Value::Str("key".into()), Value::Str("sha512".into())], &mut env).unwrap(), Value::Str("b42af09057bac1e2d41708e48a902e09b5ff7f12ab428a4fe86653c73dd248fb82f948a549f7b791a5b41915ee4d1ec3935357e4e2317250d0372afa2ebeeb3a".into()));
    assert_eq!(call("crypto_hmac", vec![Value::Str("Unicode message".into()), Value::Str("KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK".into()), Value::Str("sha512".into())], &mut env).unwrap(), Value::Str("6bf902db15bf2dda9004b0a5e525dd9839cb9ad614d07cda6146d3e29688798c67229e86af510ed96a6dd709fc23c88302ad13d853c1d62454830517db159e11".into()));
    assert_eq!(call("crypto_hmac", vec![Value::Str("".into()), Value::Str("".into()), Value::Str("sha512".into())], &mut env).unwrap(), Value::Str("b936cee86c9f87aa5d3c6f2e84cb5a4239a5fe50480a6ec66b70ab5b1f4ac6730c6c515421b327ec1d69402e53dfb49ad7381eb067b338fd7b0cb22247225d47".into()));
    for algorithm in [Value::Str("unknown".into()), Value::Int(3)] {
        let error = call(
            "crypto_hmac",
            vec![
                Value::Str("message".into()),
                Value::Str("key".into()),
                algorithm,
            ],
            &mut env,
        )
        .unwrap_err();
        assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
    }
}

#[test]
fn authenticated_crypto_refuses_discarded_options_before_running() {
    let mut env = Env::new();
    for name in ["crypto_encrypt", "crypto_decrypt"] {
        for args in [
            vec![
                Value::Str("message".into()),
                Value::Str("password".into()),
                Value::Str("aes-cbc".into()),
            ],
            vec![Value::Str("message".into()), Value::Int(1)],
            vec![Value::Str("message".into())],
        ] {
            let error = call(name, args, &mut env).unwrap_err();
            assert!(error.to_string().contains("E_BAD_ARG"), "{name}: {error}");
        }
    }
}

#[test]
fn malformed_csv_text_is_a_coded_bad_argument() {
    // A row with more fields than the header was a bare csv::Error, so it
    // reached the caller uncoded, as E_UNKNOWN.
    let error = call_with_input(
        "from-csv",
        vec![],
        Some(Value::Str("a,b\n1,2,3\n".into())),
        &mut Env::new(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("E_BAD_ARG"), "{error}");
}
