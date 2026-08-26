//! SQL built by string interpolation, run by a CLI that executes many statements.
//!
//! `safety::sql_literal` and `safety::sql_identifier` exist and are careful:
//! literals get their quotes doubled and a NUL refused; identifiers are refused
//! outright unless they are plainly `[A-Za-z_][A-Za-z0-9_]*`, because an
//! identifier cannot be passed as a quoted value. Several `db_sqlite_*` builtins
//! do not call either, and interpolate caller-controlled strings straight into a
//! statement handed to the `sqlite3` CLI — which runs every statement in the
//! string, separated by `;`.
//!
//! Two of them, `db_sqlite_export_csv` and `db_sqlite_create`, are not named in
//! `safety::classified_effect` at all, so `effect_of` returns `Pure` and
//! `guard_dispatch` lets them through without a decision or an audit line. A
//! "read-only export" that can drop a table is the same shape as the `rm` bug
//! this repo keeps citing: policy that reads as coverage while governing
//! nothing.
//!
//! These tests demonstrate rather than argue. Each one writes a real database,
//! runs the builtin the way an agent would, and then checks the database for the
//! effect — the value the builtin returned is not evidence, because the
//! injected statement's whole point is that it happens off to the side.
//!
//! They are `#[ignore]`d only if `sqlite3` is missing, which is checked at
//! runtime rather than assumed.

use aethershell::env::Env;
use aethershell::value::Value;
use std::path::PathBuf;
use std::sync::Mutex;

static ENV: Mutex<()> = Mutex::new(());

fn have_sqlite3() -> bool {
    std::process::Command::new("sqlite3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let mut env = Env::new();
    aethershell::builtins::call(name, args, &mut env).map_err(|e| e.to_string())
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

/// A fresh database with one table `victim` holding one row, plus a workspace
/// that agent mode will accept.
fn fixture(tag: &str) -> PathBuf {
    let ws = std::env::temp_dir().join("ae_sqli");
    std::fs::create_dir_all(&ws).unwrap();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &ws);
    std::env::remove_var("AETHER_POLICY");
    aethershell::safety::set_principal(None);

    let db = ws.join(format!("{tag}.sqlite"));
    let _ = std::fs::remove_file(&db);
    let out = std::process::Command::new("sqlite3")
        .arg(&db)
        .arg("CREATE TABLE victim(id INTEGER); INSERT INTO victim VALUES (1);")
        .output()
        .expect("seed the database");
    assert!(out.status.success(), "fixture setup failed");
    db
}

/// Does `victim` still exist in `db`?
fn victim_survives(db: &PathBuf) -> bool {
    let out = std::process::Command::new("sqlite3")
        .arg(db)
        .arg("SELECT name FROM sqlite_master WHERE type='table' AND name='victim';")
        .output()
        .expect("query sqlite_master");
    String::from_utf8_lossy(&out.stdout).contains("victim")
}

/// `db_sqlite_create` and `db_sqlite_export_csv` are **not dispatched** — they
/// are part of the unregistered set (§5 item 3 of docs/HANDOFF.md).
///
/// An injection test against an unreachable name passes because nothing ran,
/// which is a blind check. The first draft of this file did exactly that and
/// reported two clean results it had not earned; only `db_sqlite_insert` was
/// reachable, and it was the one that failed.
fn dispatched(name: &str) -> bool {
    aethershell::builtins::is_dispatched(name)
}

#[test]
fn the_unreachable_cases_are_still_unreachable() {
    // If either becomes registered, its injection surface ships with it —
    // `db_sqlite_create` still interpolates the column *type* as written. This
    // failing is the reminder to validate before exposing.
    for name in ["db_sqlite_create", "db_sqlite_export_csv"] {
        assert!(
            !dispatched(name),
            "`{name}` is now reachable. Before shipping it, check that every \
             identifier it interpolates goes through `safety::sql_identifier`; \
             `db_sqlite_create` still passes the column type through unchecked, \
             which is noted at the call site."
        );
    }
}

#[test]
fn the_injectable_builtin_is_actually_reachable() {
    // The counterweight to the test above: this file would be worthless if the
    // one case it proves were unreachable too.
    assert!(
        dispatched("db_sqlite_insert"),
        "db_sqlite_insert must be dispatched for its injection test to mean anything"
    );
    assert!(dispatched("db_sqlite_update"));
    assert!(dispatched("db_sqlite_drop_table"));
}

#[test]
fn export_csv_does_not_execute_a_second_statement() {
    if !have_sqlite3() {
        eprintln!("skipping: sqlite3 not on PATH");
        return;
    }
    // Unreachable today; the assertion below documents the shape rather than
    // proving a live path. `the_unreachable_cases_are_still_unreachable` is what
    // makes that explicit instead of silently vacuous.
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let db = fixture("export");
    assert!(
        victim_survives(&db),
        "fixture must start with the table present"
    );

    // `table_or_query` is interpolated into `SELECT * FROM {}` whenever it does
    // not begin with SELECT. The sqlite3 CLI runs every statement in the string.
    let injected = "victim; DROP TABLE victim; --";
    let _ = call(
        "db_sqlite_export_csv",
        vec![s(&db.to_string_lossy()), s(injected)],
    );

    assert!(
        victim_survives(&db),
        "`db_sqlite_export_csv` executed an injected `DROP TABLE`. The second \
         argument is interpolated into `SELECT * FROM {{}}` without \
         `safety::sql_identifier`, and the sqlite3 CLI runs every `;`-separated \
         statement it is given. The builtin is also absent from \
         `safety::classified_effect`, so `effect_of` is `Pure` and \
         `guard_dispatch` neither gated nor audited the call — an export that \
         drops tables, with no approval prompt in agent mode."
    );
}

#[test]
fn create_does_not_execute_a_second_statement() {
    if !have_sqlite3() {
        eprintln!("skipping: sqlite3 not on PATH");
        return;
    }
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let db = fixture("create");
    assert!(victim_survives(&db));

    // The table name goes into `CREATE TABLE IF NOT EXISTS {} (...)` unchecked.
    let mut cols = std::collections::BTreeMap::new();
    cols.insert("id".to_string(), s("INTEGER"));
    let injected = "t1(x); DROP TABLE victim; --";
    let _ = call(
        "db_sqlite_create",
        vec![s(&db.to_string_lossy()), s(injected), Value::Record(cols)],
    );

    assert!(
        victim_survives(&db),
        "`db_sqlite_create` executed an injected `DROP TABLE`: the table name is \
         interpolated without `safety::sql_identifier`"
    );
}

#[test]
fn insert_does_not_execute_a_second_statement_through_a_column_name() {
    if !have_sqlite3() {
        eprintln!("skipping: sqlite3 not on PATH");
        return;
    }
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let db = fixture("insert");
    assert!(victim_survives(&db));

    // Values are escaped (quotes doubled inline). Column names are not — and they
    // come from a caller-supplied record, so they are just as controlled.
    let mut rec = std::collections::BTreeMap::new();
    rec.insert(
        "id) VALUES (1); DROP TABLE victim; --".to_string(),
        Value::Int(1),
    );
    let _ = call(
        "db_sqlite_insert",
        vec![s(&db.to_string_lossy()), s("victim"), Value::Record(rec)],
    );

    assert!(
        victim_survives(&db),
        "`db_sqlite_insert` executed an injected `DROP TABLE` through a column \
         name. The record's keys are interpolated into `INSERT INTO t ({{}})` \
         without `safety::sql_identifier`, even though its *values* are escaped — \
         so the escaping present makes the gap easier to miss, not smaller."
    );
}

#[test]
fn a_value_containing_a_quote_still_round_trips() {
    // The counterweight. Refusing injection must not mean refusing apostrophes:
    // if this breaks, the fix has been applied to literals rather than to
    // identifiers, which is the wrong half.
    if !have_sqlite3() {
        eprintln!("skipping: sqlite3 not on PATH");
        return;
    }
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let db = fixture("quote");

    let out = std::process::Command::new("sqlite3")
        .arg(&db)
        .arg("CREATE TABLE names(n TEXT);")
        .output()
        .expect("create");
    assert!(out.status.success());

    let mut rec = std::collections::BTreeMap::new();
    rec.insert("n".to_string(), s("O'Brien"));
    call(
        "db_sqlite_insert",
        vec![s(&db.to_string_lossy()), s("names"), Value::Record(rec)],
    )
    .expect("a legitimate insert with an apostrophe must succeed");

    let read = std::process::Command::new("sqlite3")
        .arg(&db)
        .arg("SELECT n FROM names;")
        .output()
        .expect("read back");
    assert!(
        String::from_utf8_lossy(&read.stdout).contains("O'Brien"),
        "an ordinary apostrophe must survive the round trip"
    );
}

#[test]
fn update_does_not_execute_a_second_statement_through_a_column_name() {
    if !have_sqlite3() {
        eprintln!("skipping: sqlite3 not on PATH");
        return;
    }
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let db = fixture("update");
    assert!(victim_survives(&db));
    assert!(dispatched("db_sqlite_update"));

    // `db_sqlite_update` shares the shape: values escaped, keys not. Its WHERE
    // clause is SQL by contract, so the key is the part that should not be.
    let mut rec = std::collections::BTreeMap::new();
    rec.insert("id = 1; DROP TABLE victim; --".to_string(), Value::Int(2));
    let _ = call(
        "db_sqlite_update",
        vec![
            s(&db.to_string_lossy()),
            s("victim"),
            Value::Record(rec),
            s("1=1"),
        ],
    );

    assert!(
        victim_survives(&db),
        "`db_sqlite_update` executed an injected `DROP TABLE` through a column name"
    );
}

#[test]
fn a_nul_in_a_value_is_refused_rather_than_silently_truncating() {
    // The hand-rolled `replace('\'', "''")` these builtins used agreed with
    // `safety::sql_literal` about quotes and inherited none of its other care.
    // sqlite3 reads its SQL as a C string, so a NUL truncates the statement and
    // the row lands short — a wrong value written silently.
    if !have_sqlite3() {
        eprintln!("skipping: sqlite3 not on PATH");
        return;
    }
    let _l = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let db = fixture("nul");
    let out = std::process::Command::new("sqlite3")
        .arg(&db)
        .arg("CREATE TABLE names(n TEXT);")
        .output()
        .expect("create");
    assert!(out.status.success());

    let mut rec = std::collections::BTreeMap::new();
    rec.insert("n".to_string(), s("before\u{0}after"));
    let err = call(
        "db_sqlite_insert",
        vec![s(&db.to_string_lossy()), s("names"), Value::Record(rec)],
    )
    .expect_err("a NUL in a value must be refused, not truncated");
    assert!(
        err.contains("NUL"),
        "the refusal should say what was wrong with it: {err}"
    );
}
