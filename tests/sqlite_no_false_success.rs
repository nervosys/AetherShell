//! A sqlite builtin may not answer true, or create a file, for work it did not do.
//!
//! The refusals happen before sqlite3 is spawned, so they need no sqlite3 on the
//! host. Only the nested-value round trip reads a real database back.

use iron_shell::builtins;
use iron_shell::env::Env;
use iron_shell::safety;
use iron_shell::value::Value;
use std::path::PathBuf;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ish_sqlite_fs_{}_{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn s(p: &std::path::Path) -> Value {
    Value::Str(p.to_string_lossy().into_owned())
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

fn code(e: &anyhow::Error) -> String {
    safety::code_of(e).as_str().to_string()
}

fn have_sqlite3() -> bool {
    std::process::Command::new("sqlite3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn backup_of_an_absent_database_is_not_found_and_creates_nothing() {
    let dir = scratch("backup_absent");
    let source = dir.join("absent.db");
    let copy = dir.join("copy.db");
    let err = call("db_sqlite_backup", vec![s(&source), s(&copy)])
        .expect_err("there is nothing to back up");
    assert_eq!(code(&err), "E_NOT_FOUND", "{err:#}");
    assert!(!source.exists(), "the source must not be created");
    assert!(!copy.exists(), "no empty backup may be written");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn backup_of_an_in_memory_database_is_refused() {
    let dir = scratch("backup_memory");
    let copy = dir.join("copy.db");
    let err = call(
        "db_sqlite_backup",
        vec![Value::Str(":memory:".into()), s(&copy)],
    )
    .expect_err(":memory: is empty in a new sqlite3 process");
    assert_eq!(code(&err), "E_BAD_ARG", "{err:#}");
    assert!(!copy.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn csv_export_of_an_absent_database_does_not_create_it() {
    let dir = scratch("csv_absent");
    let source = dir.join("absent.db");
    let err = call("db_sqlite_to_csv", vec![s(&source), Value::Str("t".into())])
        .expect_err("there is nothing to export");
    assert_eq!(code(&err), "E_NOT_FOUND", "{err:#}");
    assert!(!source.exists(), "a read must not create the database");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn json_import_refuses_what_it_cannot_store_before_writing_anything() {
    let dir = scratch("json_refused");
    for (tag, body) in [
        ("not_json", "{ this is not json"),
        ("not_array", r#"{"a": 1}"#),
        ("first_scalar", r#"[1, {"a": 1}]"#),
        ("later_scalar", r#"[{"a": 1}, 2, {"a": 3}]"#),
        ("extra_key", r#"[{"a": 1}, {"a": 2, "b": 3}]"#),
    ] {
        let json = dir.join(format!("{tag}.json"));
        let db = dir.join(format!("{tag}.db"));
        std::fs::write(&json, body).expect("json");
        let err = call("db_json_to_sqlite", vec![s(&db), s(&json)])
            .expect_err(&format!("{tag} must be refused"));
        assert_eq!(code(&err), "E_BAD_ARG", "{tag}: {err:#}");
        assert!(!db.exists(), "{tag}: nothing may be written");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn json_import_of_an_empty_array_reports_nothing_imported() {
    let dir = scratch("json_empty");
    let json = dir.join("empty.json");
    let db = dir.join("empty.db");
    std::fs::write(&json, "[]").expect("json");
    let got = call("db_json_to_sqlite", vec![s(&db), s(&json)]).expect("not an error");
    assert_eq!(got, Value::Bool(false));
    assert!(!db.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn json_import_keeps_nested_values_as_json_text() {
    if !have_sqlite3() {
        eprintln!("skipping: sqlite3 not on PATH");
        return;
    }
    let dir = scratch("json_nested");
    let json = dir.join("nested.json");
    let db = dir.join("nested.db");
    std::fs::write(
        &json,
        r#"[{"id": 1, "tags": ["x", "y"], "gone": null}, {"id": 2}]"#,
    )
    .expect("json");
    let got = call(
        "db_json_to_sqlite",
        vec![s(&db), s(&json), Value::Str("t".into())],
    )
    .expect("import");
    assert_eq!(got, Value::Bool(true));

    let read = std::process::Command::new("sqlite3")
        .arg(&db)
        .arg("SELECT id, tags, gone IS NULL FROM t ORDER BY id;")
        .output()
        .expect("read back");
    let text = String::from_utf8_lossy(&read.stdout).replace('\r', "");
    let rows: Vec<&str> = text.lines().collect();
    assert_eq!(rows, [r#"1|["x","y"]|1"#, "2||1"], "{text:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
