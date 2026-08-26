//! Every value interpolated into a SQL string must be validated or escaped.
//!
//! `safety::sql_identifier` and `safety::sql_literal` exist and are careful.
//! Whether they are *called* was, until this file, a matter of remembering:
//! `db_sqlite_delete` and `db_sqlite_count` called them, `db_sqlite_insert`,
//! `db_sqlite_update` and `db_json_to_sqlite` did not, and the gap survived
//! however many readings because the code around it looked careful — the
//! *values* were escaped inline, so a reader sees quote-doubling and stops.
//!
//! Compare `safety::ps_quote`, which solves the same problem structurally: it
//! returns a `PsLiteral` newtype whose surrounding quotes are *included*, so a
//! missed call site shows up as broken output rather than as silence. It is used
//! 72 times and has not drifted. The SQL helpers return a plain `String`, so
//! nothing distinguishes a validated identifier from any other string at the
//! interpolation site.
//!
//! This is the substitute for that: a source lint. Find every `format!` whose
//! literal looks like SQL, and require each interpolated argument to be either a
//! call to one of the helpers, something built from them, or a name on the
//! allowlist below with a reason. An unrecognised argument fails the build.
//!
//! It is deliberately a *lower bound* on rigour — it reads syntax, not meaning,
//! so a helper called on the wrong string still passes. It catches the failure
//! that actually happened three times: nobody called the helper at all.

use std::collections::BTreeSet;

/// Names that may be interpolated into SQL without passing through a helper,
/// each because it is not a value being smuggled in.
///
/// This list may only shrink. Adding to it means asserting that a
/// caller-controlled string reaching SQL unvalidated is *correct*, which is a
/// claim that needs a reason next to it.
const ALLOWED: &[(&str, &str)] = &[
    // SQL by contract: these builtins exist to take a WHERE clause, and their
    // doc comments say so. Validating it would defeat the feature.
    (
        "where_clause",
        "the builtin's purpose is to accept a WHERE clause",
    ),
    ("w", "the bound WHERE clause in db_sqlite_count"),
    // Built from already-validated pieces, immediately above the format!.
    (
        "columns.join(\", \")",
        "each column validated by sql_identifier",
    ),
    (
        "cols.join(\", \")",
        "each column validated by sql_identifier",
    ),
    (
        "vals.join(\", \")",
        "each value rendered by sql_literal/sql_value",
    ),
    (
        "sets.join(\", \")",
        "each assignment built from sql_identifier + sql_value",
    ),
    (
        "col_defs.join(\", \")",
        "each column validated by sql_identifier",
    ),
    // A caller-supplied column *definition* string, which is SQL by contract in
    // the same way a WHERE clause is; the sibling branch takes a record instead.
    ("s", "db_sqlite_create's raw column-definition branch"),
];

fn builtins_source() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/builtins.rs");
    std::fs::read_to_string(path).expect("src/builtins.rs is readable")
}

/// Does this format-string literal look like SQL?
fn looks_like_sql(fmt: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "SELECT ",
        "INSERT ",
        "UPDATE ",
        "DELETE ",
        "CREATE TABLE",
        "DROP TABLE",
        "ALTER TABLE",
    ];
    KEYWORDS.iter().any(|k| fmt.contains(k))
}

/// Is this interpolated argument expression acceptable?
fn argument_is_safe(arg: &str) -> bool {
    let a = arg.trim().trim_start_matches('&').trim();
    a.contains("sql_identifier(")
        || a.contains("sql_literal(")
        || a.contains("sql_value(")
        || ALLOWED.iter().any(|(name, _)| *name == a)
}

/// Every `format!(...)` in the source whose literal looks like SQL, as
/// (literal, argument-expressions).
///
/// Arguments are split on top-level commas so that a nested call like
/// `sql_identifier("x", &t)` stays in one piece.
fn sql_format_sites(src: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut search = 0usize;
    while let Some(rel) = src[search..].find("format!(") {
        let open = search + rel + "format!(".len();
        // Balance to the closing paren of the format! call.
        let (mut depth, mut i, mut in_str, mut esc) = (1i32, open, false, false);
        while i < src.len() && depth > 0 {
            let c = src.as_bytes()[i] as char;
            if in_str {
                if esc {
                    esc = false;
                } else if c == '\\' {
                    esc = true;
                } else if c == '"' {
                    in_str = false;
                }
            } else {
                match c {
                    '"' => in_str = true,
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
            }
            i += 1;
        }
        let inner = &src[open..i.saturating_sub(1)];
        search = i.max(search + 1);

        // The literal is the first "..." in the call.
        let Some(q0) = inner.find('"') else { continue };
        let mut j = q0 + 1;
        let (mut esc2, mut end) = (false, None);
        while j < inner.len() {
            let c = inner.as_bytes()[j] as char;
            if esc2 {
                esc2 = false;
            } else if c == '\\' {
                esc2 = true;
            } else if c == '"' {
                end = Some(j);
                break;
            }
            j += 1;
        }
        let Some(q1) = end else { continue };
        let fmt = inner[q0 + 1..q1].to_string();
        if !looks_like_sql(&fmt) {
            continue;
        }

        // Split the remaining arguments on top-level commas.
        let rest = &inner[q1 + 1..];
        let mut args = Vec::new();
        let (mut d, mut cur, mut s_in, mut s_esc) = (0i32, String::new(), false, false);
        for c in rest.chars() {
            if s_in {
                cur.push(c);
                if s_esc {
                    s_esc = false;
                } else if c == '\\' {
                    s_esc = true;
                } else if c == '"' {
                    s_in = false;
                }
                continue;
            }
            match c {
                '"' => {
                    s_in = true;
                    cur.push(c);
                }
                '(' | '[' | '{' => {
                    d += 1;
                    cur.push(c);
                }
                ')' | ']' | '}' => {
                    d -= 1;
                    cur.push(c);
                }
                ',' if d == 0 => {
                    if !cur.trim().is_empty() {
                        args.push(cur.trim().to_string());
                    }
                    cur = String::new();
                }
                _ => cur.push(c),
            }
        }
        if !cur.trim().is_empty() {
            args.push(cur.trim().to_string());
        }
        out.push((fmt, args));
    }
    out
}

#[test]
fn every_sql_interpolation_is_validated_or_escaped() {
    let src = builtins_source();
    let mut bad: Vec<String> = Vec::new();

    for (fmt, args) in sql_format_sites(&src) {
        for arg in &args {
            if !argument_is_safe(arg) {
                bad.push(format!("  {fmt:?} <- {arg}"));
            }
        }
    }

    assert!(
        bad.is_empty(),
        "{} value(s) are interpolated into SQL without `safety::sql_identifier`, \
         `sql_literal` or `sql_value`.\n\
         The sqlite3 CLI runs every `;`-separated statement it is handed, so an \
         unvalidated identifier is remote code execution against the database — \
         this exact gap was live in `db_sqlite_insert`, `db_sqlite_update` and \
         `db_json_to_sqlite`.\n\
         Wrap it, or — if the value is SQL by contract, as a WHERE clause is — add \
         it to ALLOWED in this file *with the reason*:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn the_scanner_finds_the_sql_sites_it_should() {
    // A check on the checker. If the parser breaks, the lint above passes by
    // finding nothing — the failure mode this repo keeps naming.
    let src = builtins_source();
    let sites = sql_format_sites(&src);
    assert!(
        sites.len() >= 12,
        "only {} SQL format! sites parsed; the scanner has drifted and the lint \
         above is checking almost nothing",
        sites.len()
    );
    assert!(
        sites.iter().any(|(f, _)| f.contains("INSERT INTO")),
        "the INSERT sites should be among them"
    );
    assert!(
        sites.iter().any(|(f, _)| f.contains("DROP TABLE")),
        "the DROP sites should be among them"
    );
}

#[test]
fn the_scanner_would_catch_a_raw_interpolation() {
    // Check on the checker, other direction: prove it rejects what it is for,
    // rather than trusting that it would.
    assert!(!argument_is_safe("table_name"));
    assert!(!argument_is_safe("&table"));
    assert!(!argument_is_safe("user_input.clone()"));
    assert!(argument_is_safe(
        "crate::safety::sql_identifier(\"b\", &table)?"
    ));
    assert!(argument_is_safe("sql_value(\"b\", v)?"));
    assert!(argument_is_safe("where_clause"));
}

#[test]
fn the_allowlist_has_a_reason_for_every_entry_and_no_duplicates() {
    let mut seen = BTreeSet::new();
    for (name, reason) in ALLOWED {
        assert!(
            !reason.trim().is_empty(),
            "{name} is allowed without a reason; the reason is the point"
        );
        assert!(seen.insert(*name), "{name} is listed twice");
    }
    // It may only shrink. If this number goes up, something unvalidated was
    // waved through rather than fixed.
    assert!(
        ALLOWED.len() <= 8,
        "the SQL interpolation allowlist has grown to {}; it may only shrink",
        ALLOWED.len()
    );
}
