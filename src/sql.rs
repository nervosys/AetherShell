//! In-process SQL over typed values: `sql`, `sqlite_query`, `sql_value`.
//!
//! The benchmarks' central finding (`docs/LANGUAGE_FIRST_PRINCIPLES.md` §2–3)
//! is that an agent writes a borrowed syntax right the first time far more
//! often than an invented one, and that SQL is the borrowed syntax for set and
//! aggregate work. sqlite3 won E1 outright on tokens. This puts SQL *inside*
//! the shell, over the values the shell already has, so the query is SQL and
//! the result is still a typed value with structured errors around it:
//!
//! ```text
//! ls("src") | sql("select name, size from t where size > 1000 order by size desc")
//! sql_value("issues.json", "select count(*) from issues where state = 'open'")
//! ```
//!
//! Sources:
//! - a piped array of records (or table) is the table `t`;
//! - a piped record whose fields are arrays is one table per field;
//! - a `.json`, `.jsonl`/`.ndjson` or `.csv` path is a table named after the
//!   file's stem, also reachable as `t`;
//! - any other path is an existing SQLite database, opened read-only;
//!   `":memory:"` is an empty one.
//!
//! Loaded values keep SQLite's dynamic typing: Int, Float, String and Null map
//! directly, Bool is stored as 0/1 (SQLite has no boolean, and `TRUE`/`FALSE`
//! compare as 1/0), and nested arrays and records are stored as JSON text, so
//! `user ->> 'login'` and `json_array_length(labels)` work on them.
//!
//! Every form is **read-only** and cannot reach the filesystem: an authorizer
//! admits only reads, functions and read-only pragmas, `ATTACH` is limited to
//! zero databases, and a statement SQLite reports as writing is refused.
//! `db_sqlite_exec` remains the way to change a database file. That is what
//! lets these builtins be classed `ReadLocal` instead of `Exec`: they used to
//! spawn the `sqlite3` CLI, which made them approval-gated in agent mode and
//! absent on any host without it — the absence that once zeroed E7's SQL arm.

use crate::safety::{ErrorCode, SafetyError};
use crate::value::Value;
use anyhow::Result;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::limits::Limit;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Wall-clock budget for one query. A recursive CTE can run forever; the
/// search builtins use the same 30 s.
const QUERY_BUDGET: Duration = Duration::from_secs(30);
/// Rows a result may hold before it is refused rather than materialized.
const MAX_ROWS: usize = 1_000_000;
/// Columns named per table in an error hint, so a wide table cannot make the
/// hint cost more than the retry it saves.
const HINT_COLUMNS: usize = 40;

/// A named table of rows, in the order the rows arrived.
struct Loaded {
    name: String,
    rows: Vec<BTreeMap<String, Value>>,
}

/// A structured `E_BAD_ARG` with a hint the caller chooses.
fn bad(builtin: &str, message: String, hint: String, expected: &str, got: String) -> anyhow::Error {
    anyhow::Error::new(SafetyError {
        code: ErrorCode::BadArg,
        message: format!("{builtin}: {message}"),
        builtin: builtin.to_string(),
        hint,
        approval: None,
        did_you_mean: Vec::new(),
        expected: expected.to_string(),
        got,
    })
}

/// Rows of an array whose every element is a record.
fn rows_of(builtin: &str, what: &str, items: Vec<Value>) -> Result<Vec<BTreeMap<String, Value>>> {
    items
        .into_iter()
        .enumerate()
        .map(|(i, v)| match v {
            Value::Record(r) => Ok(r),
            other => Err(bad(
                builtin,
                format!("{what}: element {i} is {}, not a record", other.type_name()),
                "SQL needs rows: pass an array of records, e.g. from from_json or ls".to_string(),
                "Array of Record",
                other.type_name().to_string(),
            )),
        })
        .collect()
}

/// Tables from a piped value.
fn tables_from_subject(builtin: &str, subject: Value) -> Result<Vec<Loaded>> {
    match subject {
        Value::Array(items) => Ok(vec![Loaded {
            name: "t".to_string(),
            rows: rows_of(builtin, "the piped array", items)?,
        }]),
        Value::Table(t) => Ok(vec![Loaded {
            name: "t".to_string(),
            rows: t.rows,
        }]),
        Value::Record(fields)
            if !fields.is_empty()
                && fields
                    .values()
                    .all(|v| matches!(v, Value::Array(_) | Value::Table(_))) =>
        {
            let mut out = Vec::new();
            for (name, v) in fields {
                let rows = match v {
                    Value::Array(items) => rows_of(builtin, &format!("field `{name}`"), items)?,
                    Value::Table(t) => t.rows,
                    _ => unreachable!("guarded above"),
                };
                out.push(Loaded { name, rows });
            }
            Ok(out)
        }
        other => Err(bad(
            builtin,
            format!("cannot query a piped {}", other.type_name()),
            "pipe an array of records (the table `t`), or a record of arrays (one table \
             per field)"
                .to_string(),
            "Array of Record, or Record of Array",
            other.type_name().to_string(),
        )),
    }
}

/// A CSV cell, typed the way `from_csv` types it.
fn csv_cell(field: &str) -> Value {
    if let Ok(i) = field.parse::<i64>() {
        Value::Int(i)
    } else if let Ok(f) = field.parse::<f64>() {
        Value::Float(f)
    } else if let Ok(b) = field.parse::<bool>() {
        Value::Bool(b)
    } else {
        Value::Str(field.to_string())
    }
}

/// The file extension, lower-cased, if the path names a tabular text format.
fn tabular_kind(path: &str) -> Option<&'static str> {
    let ext = std::path::Path::new(path)
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    match ext.as_str() {
        "json" => Some("json"),
        "jsonl" | "ndjson" => Some("jsonl"),
        "csv" => Some("csv"),
        _ => None,
    }
}

/// Load a JSON, JSONL or CSV file as a table named after its stem.
fn table_from_file(builtin: &str, path: &str, kind: &str) -> Result<Vec<Loaded>> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            crate::safety::not_found(builtin, "file", path)
        } else {
            crate::safety::fs_error(builtin, path, &e)
        }
    })?;
    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("t")
        .to_string();
    let parse_err = |line: Option<usize>, e: &dyn std::fmt::Display| {
        let at = line.map(|n| format!(" (line {n})")).unwrap_or_default();
        bad(
            builtin,
            format!("{path} is not valid {kind}{at}: {e}"),
            format!("check the file with `cat(\"{path}\") | take(5)`"),
            kind,
            e.to_string(),
        )
    };
    let tables = match kind {
        "json" => {
            let json: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| parse_err(None, &e))?;
            match Value::from_json(&json) {
                Value::Array(items) => vec![Loaded {
                    name: stem,
                    rows: rows_of(builtin, path, items)?,
                }],
                rec @ Value::Record(_) => tables_from_subject(builtin, rec)?,
                other => {
                    return Err(bad(
                        builtin,
                        format!("{path} holds a JSON {}, not rows", other.type_name()),
                        "the file must hold an array of objects, or an object of arrays"
                            .to_string(),
                        "Array of Record",
                        other.type_name().to_string(),
                    ))
                }
            }
        }
        "jsonl" => {
            let mut rows = Vec::new();
            for (i, line) in text.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                let json: serde_json::Value =
                    serde_json::from_str(line).map_err(|e| parse_err(Some(i + 1), &e))?;
                rows.push(Value::from_json(&json));
            }
            vec![Loaded {
                name: stem,
                rows: rows_of(builtin, path, rows)?,
            }]
        }
        _ => {
            let mut reader = csv::Reader::from_reader(text.as_bytes());
            let headers: Vec<String> = reader
                .headers()
                .map_err(|e| parse_err(None, &e))?
                .iter()
                .map(str::to_string)
                .collect();
            let mut rows = Vec::new();
            for (i, record) in reader.records().enumerate() {
                let record = record.map_err(|e| parse_err(Some(i + 2), &e))?;
                let mut row = BTreeMap::new();
                for (h, field) in headers.iter().zip(record.iter()) {
                    row.insert(h.clone(), csv_cell(field));
                }
                rows.push(row);
            }
            vec![Loaded { name: stem, rows }]
        }
    };
    Ok(tables)
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// A shell value as the SQLite value it is stored as.
fn to_sql(v: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as S;
    match v {
        Value::Null => S::Null,
        Value::Bool(b) => S::Integer(i64::from(*b)),
        Value::Int(i) => S::Integer(*i),
        Value::Float(f) => S::Real(*f),
        Value::Str(s) | Value::Uri(s) => S::Text(s.clone()),
        other => S::Text(other.to_json().to_string()),
    }
}

/// Create and fill each table. Columns are the union of the rows' keys, in
/// sorted order (records are sorted maps); a row without a key stores NULL.
fn load(conn: &Connection, builtin: &str, tables: &[Loaded]) -> Result<()> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| internal(builtin, &e))?;
    for t in tables {
        let mut cols: Vec<&String> = t.rows.iter().flat_map(|r| r.keys()).collect();
        cols.sort();
        cols.dedup();
        if cols.is_empty() {
            // An empty source is still a table a query can name; it has no rows
            // and so no columns to infer. One placeholder column keeps
            // `select count(*) from t` answering 0 instead of "no such table".
            tx.execute_batch(&format!("CREATE TABLE {} (\"_\")", quote_ident(&t.name)))
                .map_err(|e| internal(builtin, &e))?;
            continue;
        }
        let col_list: Vec<String> = cols.iter().map(|c| quote_ident(c)).collect();
        tx.execute_batch(&format!(
            "CREATE TABLE {} ({})",
            quote_ident(&t.name),
            col_list.join(", ")
        ))
        .map_err(|e| {
            bad(
                builtin,
                format!("cannot create table `{}`: {e}", t.name),
                "two sources or fields may share a name that differs only in case".to_string(),
                "distinct table and column names",
                e.to_string(),
            )
        })?;
        let placeholders = vec!["?"; cols.len()].join(", ");
        let mut stmt = tx
            .prepare(&format!(
                "INSERT INTO {} ({}) VALUES ({placeholders})",
                quote_ident(&t.name),
                col_list.join(", ")
            ))
            .map_err(|e| internal(builtin, &e))?;
        for row in &t.rows {
            let params: Vec<rusqlite::types::Value> = cols
                .iter()
                .map(|c| {
                    row.get(*c)
                        .map(to_sql)
                        .unwrap_or(rusqlite::types::Value::Null)
                })
                .collect();
            stmt.execute(rusqlite::params_from_iter(params))
                .map_err(|e| internal(builtin, &e))?;
        }
    }
    tx.commit().map_err(|e| internal(builtin, &e))?;
    Ok(())
}

/// A failure of the loading machinery itself, not of the caller's query.
fn internal(builtin: &str, e: &dyn std::fmt::Display) -> anyhow::Error {
    anyhow::Error::new(SafetyError {
        code: ErrorCode::BadState,
        message: format!("{builtin}: SQLite failed while loading the data: {e}"),
        builtin: builtin.to_string(),
        hint: "this is a defect in the builtin, not in the query; report it".to_string(),
        approval: None,
        did_you_mean: Vec::new(),
        expected: String::new(),
        got: e.to_string(),
    })
}

/// The tables and columns a query can name, for an error hint.
fn schema_hint(conn: &Connection) -> String {
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_schema WHERE type IN ('table','view') ORDER BY name")
        .and_then(|mut s| {
            s.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();
    if tables.is_empty() {
        return "there are no tables; pipe rows in, or name a .json/.csv file or a database"
            .to_string();
    }
    let described: Vec<String> = tables
        .iter()
        .map(|t| {
            let cols: Vec<String> = conn
                .prepare(&format!(
                    "SELECT name FROM pragma_table_info({})",
                    sql_string(t)
                ))
                .and_then(|mut s| {
                    s.query_map([], |r| r.get::<_, String>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap_or_default();
            let more = cols.len().saturating_sub(HINT_COLUMNS);
            let mut shown: Vec<String> = cols.into_iter().take(HINT_COLUMNS).collect();
            if more > 0 {
                shown.push(format!("…{more} more"));
            }
            format!("{t}({})", shown.join(", "))
        })
        .collect();
    format!("tables: {}", described.join("; "))
}

fn sql_string(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Admit reads, functions and read-only pragmas; refuse everything else.
fn read_only_authorizer(ctx: AuthContext<'_>) -> Authorization {
    match ctx.action {
        AuthAction::Select
        | AuthAction::Read { .. }
        | AuthAction::Function { .. }
        | AuthAction::Recursive => Authorization::Allow,
        AuthAction::Pragma {
            pragma_value: None, ..
        } => Authorization::Allow,
        // `pragma_table_info('t')` passes the table as the pragma's value, so
        // the rule above would refuse the one question an agent most needs
        // answered about data it cannot see. These three only describe.
        AuthAction::Pragma { pragma_name, .. }
            if matches!(pragma_name, "table_info" | "table_xinfo" | "table_list") =>
        {
            Authorization::Allow
        }
        _ => Authorization::Deny,
    }
}

/// Run one read-only statement, returning column names and rows.
fn run(builtin: &str, conn: &Connection, query: &str) -> Result<(Vec<String>, Vec<Vec<Value>>)> {
    conn.set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)
        .map_err(|e| internal(builtin, &e))?;
    conn.authorizer(Some(read_only_authorizer))
        .map_err(|e| internal(builtin, &e))?;
    let started = Instant::now();
    conn.progress_handler(
        10_000,
        Some(move || started.elapsed() > QUERY_BUDGET || crate::safety::check_deadline().is_err()),
    )
    .map_err(|e| internal(builtin, &e))?;

    let query_err = |e: rusqlite::Error| {
        let text = e.to_string();
        let (message, hint) = if text.contains("not authorized") {
            (
                format!("only reads are allowed here ({text})"),
                "sql is read-only and cannot ATTACH; use db_sqlite_exec to change a database \
                 file"
                    .to_string(),
            )
        } else if text.contains("interrupted") {
            return crate::safety::budget_exceeded(
                builtin,
                format!("the query ran past its {}s budget", QUERY_BUDGET.as_secs()),
                "narrow the query, or bound a recursive CTE with LIMIT",
            );
        } else {
            (text.clone(), schema_hint(conn))
        };
        bad(builtin, message, hint, "a valid read-only SQL query", text)
    };

    let mut stmt = conn.prepare(query).map_err(query_err)?;
    if !stmt.readonly() {
        return Err(bad(
            builtin,
            "only reads are allowed here".to_string(),
            "sql is read-only; use db_sqlite_exec to change a database file".to_string(),
            "a read-only SQL query",
            "a statement that writes".to_string(),
        ));
    }
    let columns: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
    let mut rows_out = Vec::new();
    let mut rows = stmt.query([]).map_err(query_err)?;
    while let Some(row) = rows.next().map_err(query_err)? {
        if rows_out.len() >= MAX_ROWS {
            return Err(crate::safety::budget_exceeded(
                builtin,
                format!("the result has more than {MAX_ROWS} rows"),
                "add a WHERE clause, an aggregate, or LIMIT",
            ));
        }
        let mut cells = Vec::with_capacity(columns.len());
        for i in 0..columns.len() {
            cells.push(match row.get_ref(i).map_err(query_err)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => Value::Int(n),
                ValueRef::Real(f) => Value::Float(f),
                ValueRef::Text(t) => Value::Str(String::from_utf8_lossy(t).into_owned()),
                ValueRef::Blob(b) => Value::Str(b.iter().map(|x| format!("{x:02x}")).collect()),
            });
        }
        rows_out.push(cells);
    }
    Ok((columns, rows_out))
}

/// A file as loaded: its canonical path, size and modification time. Any
/// change to the file changes the key, so a cached table is never stale.
type FileKey = (String, u64, std::time::SystemTime);

/// Files larger than this are loaded per query rather than kept.
const CACHE_MAX_FILE: u64 = 64 << 20;
/// Loaded files kept per thread.
const CACHE_ENTRIES: usize = 4;

thread_local! {
    /// In-memory databases already built from files, for a session that asks
    /// several questions of the same file (`ae mcp stdio`, `ae agent serve`,
    /// the REPL). Building one parses the file and inserts every row, which
    /// was most of a warm query's cost; the queries are read-only, so reuse
    /// is safe.
    static FILE_CACHE: std::cell::RefCell<Vec<(FileKey, Connection)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn file_key(path: &str) -> Option<FileKey> {
    let md = std::fs::metadata(path).ok()?;
    if md.len() > CACHE_MAX_FILE {
        return None;
    }
    let canonical = std::fs::canonicalize(path).ok()?;
    Some((
        canonical.to_string_lossy().into_owned(),
        md.len(),
        md.modified().ok()?,
    ))
}

fn cache_take(key: &FileKey) -> Option<Connection> {
    FILE_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let i = c.iter().position(|(k, _)| k == key)?;
        Some(c.remove(i).1)
    })
}

fn cache_put(key: FileKey, conn: Connection) {
    FILE_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        // Any older version of the same file is superseded.
        c.retain(|(k, _)| k.0 != key.0);
        c.push((key, conn));
        while c.len() > CACHE_ENTRIES {
            c.remove(0);
        }
    })
}

/// Open the connection a call's arguments describe and load its tables. The
/// key is set when the connection came from a file and may be cached.
fn connect(
    builtin: &str,
    args: &[Value],
    subject: Option<Value>,
) -> Result<(Connection, String, Option<FileKey>)> {
    let str_arg = |i: usize, what: &str| match args.get(i) {
        Some(Value::Str(s)) => Ok(s.clone()),
        other => Err(bad(
            builtin,
            format!("{what} must be a String"),
            format!("{builtin}(query) over piped rows, or {builtin}(source, query)"),
            "String",
            other
                .map(|v| v.type_name())
                .unwrap_or("nothing")
                .to_string(),
        )),
    };
    let memory = || Connection::open_in_memory().map_err(|e| internal(builtin, &e));
    match args.len() {
        1 => {
            let query = str_arg(0, "the query")?;
            let subject = subject.ok_or_else(|| {
                bad(
                    builtin,
                    "no rows to query".to_string(),
                    format!(
                        "pipe rows in (`rows | {builtin}(\"select … from t\")`), or name a \
                         source: {builtin}(\"data.json\", query)"
                    ),
                    "piped rows, or a source argument",
                    "nothing".to_string(),
                )
            })?;
            let tables = tables_from_subject(builtin, subject)?;
            let conn = memory()?;
            load(&conn, builtin, &tables)?;
            Ok((conn, query, None))
        }
        2 => {
            let source = str_arg(0, "the source")?;
            let query = str_arg(1, "the query")?;
            if let Some(kind) = tabular_kind(&source) {
                let key = file_key(&source);
                if let Some(conn) = key.as_ref().and_then(cache_take) {
                    return Ok((conn, query, key));
                }
                let tables = table_from_file(builtin, &source, kind)?;
                let conn = memory()?;
                load(&conn, builtin, &tables)?;
                if let [only] = tables.as_slice() {
                    if only.name != "t" {
                        conn.execute_batch(&format!(
                            "CREATE VIEW t AS SELECT * FROM {}",
                            quote_ident(&only.name)
                        ))
                        .map_err(|e| internal(builtin, &e))?;
                    }
                }
                return Ok((conn, query, key));
            }
            if source == ":memory:" {
                return Ok((memory()?, query, None));
            }
            if !std::path::Path::new(&source).is_file() {
                return Err(crate::safety::not_found(builtin, "database", &source));
            }
            // No SQLITE_OPEN_URI: a `file:…?mode=rwc` name must not reopen the
            // database writable.
            let conn = Connection::open_with_flags(
                &source,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|e| {
                bad(
                    builtin,
                    format!("{source} could not be opened as a SQLite database: {e}"),
                    "name a SQLite file, or a .json/.jsonl/.csv file".to_string(),
                    "a SQLite database",
                    e.to_string(),
                )
            })?;
            Ok((conn, query, None))
        }
        n => Err(bad(
            builtin,
            format!("takes 1 or 2 arguments, got {n}"),
            format!("{builtin}(query) over piped rows, or {builtin}(source, query)"),
            "1 or 2 arguments",
            n.to_string(),
        )),
    }
}

/// Connect, run, and return a file's database to the cache whether or not the
/// query succeeded -- a typo in a query should not cost the next one a reload.
fn query_once(
    builtin: &str,
    args: &[Value],
    subject: Option<Value>,
) -> Result<(Vec<String>, Vec<Vec<Value>>)> {
    let (conn, query, key) = connect(builtin, args, subject)?;
    let result = run(builtin, &conn, &query);
    if let Some(k) = key {
        cache_put(k, conn);
    }
    result
}

/// `sql` / `sqlite_query` / `db_sqlite_query`: rows as records.
pub fn sql(builtin: &str, args: Vec<Value>, subject: Option<Value>) -> Result<Value> {
    let (columns, rows) = query_once(builtin, &args, subject)?;
    Ok(Value::Array(
        rows.into_iter()
            .map(|cells| Value::Record(columns.iter().cloned().zip(cells).collect()))
            .collect(),
    ))
}

/// `sql_value`: the single cell of a one-row, one-column result.
///
/// A separate name rather than `sql` returning a scalar when the result
/// happens to be 1×1, because a result whose type depends on the data is the
/// defect `docs/TYPED_SHELL_RESPONSE.md` spent a section removing. This one
/// says what it returns, and refuses when the query did not produce it.
pub fn sql_value(args: Vec<Value>, subject: Option<Value>) -> Result<Value> {
    const NAME: &str = "sql_value";
    let (columns, mut rows) = query_once(NAME, &args, subject)?;
    if rows.len() == 1 && columns.len() == 1 {
        return Ok(rows.remove(0).remove(0));
    }
    Err(anyhow::Error::new(SafetyError {
        code: ErrorCode::BadState,
        message: format!(
            "{NAME}: the query returned {} row(s) of {} column(s), not exactly one value",
            rows.len(),
            columns.len()
        ),
        builtin: NAME.to_string(),
        hint: "select a single column and reduce to one row (an aggregate, or LIMIT 1), or use \
               sql for rows"
            .to_string(),
        approval: None,
        did_you_mean: Vec::new(),
        expected: "1 row × 1 column".to_string(),
        got: format!("{} × {}", rows.len(), columns.len()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(pairs: &[(&str, Value)]) -> Value {
        Value::Record(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    fn rows() -> Value {
        Value::Array(vec![
            rec(&[
                ("n", Value::Int(1)),
                ("s", Value::Str("a".into())),
                ("b", Value::Bool(true)),
            ]),
            rec(&[("n", Value::Int(2)), ("s", Value::Str("b".into()))]),
            rec(&[
                ("n", Value::Int(3)),
                ("s", Value::Str("c".into())),
                ("u", rec(&[("login", Value::Str("ann".into()))])),
            ]),
        ])
    }

    fn q(query: &str) -> Result<Value> {
        sql("sql", vec![Value::Str(query.into())], Some(rows()))
    }

    #[test]
    fn piped_rows_are_table_t() {
        assert_eq!(
            q("select sum(n) as total from t").unwrap(),
            Value::Array(vec![rec(&[("total", Value::Int(6))])])
        );
    }

    #[test]
    fn missing_keys_are_null_and_nested_values_are_json() {
        assert_eq!(
            sql_value(
                vec![Value::Str("select u ->> 'login' from t where n = 3".into())],
                Some(rows())
            )
            .unwrap(),
            Value::Str("ann".into())
        );
        assert_eq!(
            sql_value(
                vec![Value::Str("select count(*) from t where b is null".into())],
                Some(rows())
            )
            .unwrap(),
            Value::Int(2)
        );
    }

    #[test]
    fn sql_value_refuses_a_result_that_is_not_one_cell() {
        let e = sql_value(vec![Value::Str("select n from t".into())], Some(rows())).unwrap_err();
        let se = e.downcast_ref::<SafetyError>().expect("structured");
        assert_eq!(se.code, ErrorCode::BadState);
        assert!(
            se.message.contains("3 row(s) of 1 column(s)"),
            "{}",
            se.message
        );
    }

    #[test]
    fn a_bad_query_names_the_tables_and_columns() {
        let e = q("select nope from t").unwrap_err();
        let se = e.downcast_ref::<SafetyError>().expect("structured");
        assert_eq!(se.code, ErrorCode::BadArg);
        assert!(se.hint.contains("t(b, n, s, u)"), "{}", se.hint);
    }

    #[test]
    fn writes_and_attach_are_refused_and_touch_no_file() {
        let dir = std::env::temp_dir().join(format!("ae_sql_ro_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("escaped.db");
        let t = target.to_string_lossy().replace('\'', "''");
        for query in [
            format!("attach database '{t}' as x"),
            format!("vacuum into '{t}'"),
            "insert into t (n) values (9)".to_string(),
            "create table z (a)".to_string(),
            "select 1; drop table t".to_string(),
            "pragma journal_mode = wal".to_string(),
        ] {
            let e = q(&query).expect_err(&query);
            let se = e.downcast_ref::<SafetyError>().expect("structured");
            assert_eq!(se.code, ErrorCode::BadArg, "{query}: {}", se.message);
        }
        assert!(!target.exists(), "a query created a file on disk");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_runaway_recursive_query_is_cut_off() {
        // Not the 30 s budget itself -- the row cap, which a counting CTE hits
        // first. Either way the answer is E_BUDGET_EXCEEDED, not a hang.
        let e =
            q("with recursive c(x) as (select 1 union all select x + 1 from c) select x from c")
                .unwrap_err();
        let se = e.downcast_ref::<SafetyError>().expect("structured");
        assert_eq!(se.code, ErrorCode::BudgetExceeded, "{}", se.message);
    }

    #[test]
    fn a_cached_file_is_reloaded_when_it_changes() {
        let dir = std::env::temp_dir().join(format!("ae_sql_cache_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rows.json");
        let p = path.to_string_lossy().to_string();
        let count = || {
            sql_value(
                vec![
                    Value::Str(p.clone()),
                    Value::Str("select count(*) from rows".into()),
                ],
                None,
            )
            .unwrap()
        };
        std::fs::write(&path, r#"[{"a":1},{"a":2}]"#).unwrap();
        assert_eq!(count(), Value::Int(2));
        assert_eq!(count(), Value::Int(2), "served from the cache");
        // A different size is a different key, whatever the timestamp says.
        std::fs::write(&path, r#"[{"a":1},{"a":2},{"a":3}]"#).unwrap();
        assert_eq!(
            count(),
            Value::Int(3),
            "a changed file must not be served stale"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_json_file_is_a_table_named_after_it_and_also_t() {
        let dir = std::env::temp_dir().join(format!("ae_sql_file_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("issues.json");
        std::fs::write(
            &path,
            r#"[{"state":"open"},{"state":"closed"},{"state":"open"}]"#,
        )
        .unwrap();
        let p = path.to_string_lossy().to_string();
        for query in [
            "select count(*) from issues where state = 'open'",
            "select count(*) from t where state = 'open'",
        ] {
            assert_eq!(
                sql_value(vec![Value::Str(p.clone()), Value::Str(query.into())], None).unwrap(),
                Value::Int(2),
                "{query}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
