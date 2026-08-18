//! The kv store built its SQL with `format!`, so a key could rewrite the query.
//!
//! Both of these were demonstrated against a real database before the fix, not
//! reasoned about:
//!
//! ```text
//! db_kv_get(db, "x' OR '1'='1")             -> returned another key's value
//! db_kv_delete(db, "z'; DELETE FROM kv; --") -> emptied the table (2 rows -> 0)
//! ```
//!
//! The builtins shell out to the `sqlite3` CLI with the SQL as one argument, so
//! there is no command injection and no bound parameter to use -- the statement
//! is a string by the time it leaves the process. `safety::sql_literal` is what
//! makes interpolating into it safe.

use aethershell::env::Env;
use aethershell::value::Value;

fn call(name: &str, args: Vec<Value>) -> Value {
    let mut env = Env::new();
    aethershell::builtins::call(name, args, &mut env).unwrap_or(Value::Null)
}

/// Whether the `sqlite3` CLI is on PATH.
///
/// The db builtins shell out to it, so without it every call returns Null and
/// the end-to-end assertions below compare against a database that was never
/// written. That is exactly how these tests failed on CI's Windows runner
/// while passing locally: two of them "found no injection" because nothing had
/// happened at all -- the same shape of green-but-meaningless result this
/// repository keeps getting bitten by.
///
/// So: skip, and say so out loud. `the_quoting_helper_doubles_quotes_and_refuses_nul`
/// is pure and still runs everywhere, which keeps the fix pinned on every
/// platform even where the end-to-end tests cannot run.
fn sqlite_available() -> bool {
    std::process::Command::new("sqlite3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn require_sqlite(test: &str) -> bool {
    if sqlite_available() {
        return true;
    }
    eprintln!("SKIP: {test} — the `sqlite3` CLI is not on PATH, so the db builtins cannot run");
    false
}

fn db(tag: &str) -> String {
    let p = std::env::temp_dir().join(format!("ae_sqli_{tag}.db"));
    let _ = std::fs::remove_file(&p);
    p.to_string_lossy().to_string()
}

fn count(path: &str) -> i64 {
    match call(
        "db_sqlite_query",
        vec![
            Value::Str(path.to_string()),
            Value::Str("SELECT count(*) AS c FROM kv".to_string()),
        ],
    ) {
        Value::Array(rows) => match rows.first() {
            Some(Value::Record(r)) => match r.get("c") {
                Some(Value::Int(n)) => *n,
                Some(Value::Str(s)) => s.parse().unwrap_or(-1),
                _ => -1,
            },
            _ => 0,
        },
        _ => -1,
    }
}

fn seed(path: &str) {
    call(
        "db_kv_set",
        vec![
            Value::Str(path.to_string()),
            Value::Str("a".into()),
            Value::Str("1".into()),
        ],
    );
    call(
        "db_kv_set",
        vec![
            Value::Str(path.to_string()),
            Value::Str("b".into()),
            Value::Str("2".into()),
        ],
    );
}

#[test]
fn a_crafted_key_cannot_read_another_keys_value() {
    if !require_sqlite("a_crafted_key_cannot_read_another_keys_value") {
        return;
    }
    let path = db("read");
    seed(&path);
    let got = call(
        "db_kv_get",
        vec![Value::Str(path.clone()), Value::Str("x' OR '1'='1".into())],
    );
    assert_eq!(
        got,
        Value::Null,
        "the injected predicate matched a row: {got:?}"
    );
}

#[test]
fn a_crafted_key_cannot_append_a_second_statement() {
    if !require_sqlite("a_crafted_key_cannot_append_a_second_statement") {
        return;
    }
    let path = db("chain");
    seed(&path);
    assert_eq!(count(&path), 2, "seed failed");
    call(
        "db_kv_delete",
        vec![
            Value::Str(path.clone()),
            Value::Str("zzz'; DELETE FROM kv; --".into()),
        ],
    );
    assert_eq!(
        count(&path),
        2,
        "the chained DELETE executed -- statement separation is reachable again"
    );
}

#[test]
fn an_apostrophe_in_a_real_key_still_round_trips() {
    if !require_sqlite("an_apostrophe_in_a_real_key_still_round_trips") {
        return;
    }
    // The fix must escape, not reject. Refusing every quote would be a smaller
    // hole and a broken key/value store.
    let path = db("apos");
    call(
        "db_kv_set",
        vec![
            Value::Str(path.clone()),
            Value::Str("it's".into()),
            Value::Str("value with ' quote".into()),
        ],
    );
    assert_eq!(
        call(
            "db_kv_get",
            vec![Value::Str(path.clone()), Value::Str("it's".into())]
        ),
        Value::Str("value with ' quote".into())
    );
}

#[test]
fn the_quoting_helper_doubles_quotes_and_refuses_nul() {
    use aethershell::safety::{sql_identifier, sql_literal};
    assert_eq!(sql_literal("t", "plain").unwrap(), "'plain'");
    assert_eq!(sql_literal("t", "it's").unwrap(), "'it''s'");
    assert_eq!(
        sql_literal("t", "' OR '1'='1").unwrap(),
        "''' OR ''1''=''1'"
    );
    assert!(
        sql_literal("t", "a\0b").is_err(),
        "a NUL truncates the statement the CLI receives and must be refused"
    );

    assert!(sql_identifier("t", "kv").is_ok());
    assert!(sql_identifier("t", "my_table1").is_ok());
    for bad in ["kv; DROP TABLE kv", "1abc", "", "kv'", "a b"] {
        assert!(
            sql_identifier("t", bad).is_err(),
            "{bad:?} must be refused as an identifier"
        );
    }
}
