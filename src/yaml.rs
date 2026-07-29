//! A YAML subset parser and emitter with an explicit, enforced boundary.
//!
//! # Why this exists
//!
//! `from-yaml` previously split each line on the first `:` and collected the
//! results into one flat record. That silently mis-parsed almost every real
//! YAML document: nesting collapsed, sequences vanished (a `- item` line has no
//! colon, so it was dropped), `#` comments were kept as part of the value, and
//! `---` document markers became a `{"---": ""}` entry. `to-yaml` emitted
//! values unquoted, so any string containing `: ` produced output that would
//! not read back.
//!
//! For a shell whose entire pitch is *deterministic typed output*, silently
//! returning wrong data is the worst available behavior — an agent cannot
//! detect it and will act on the corruption.
//!
//! # The contract
//!
//! This module implements the commonly-used subset of YAML and **fails loudly**
//! on everything else, naming the construct it cannot handle. Callers get
//! either correct data or an error — never quiet corruption.
//!
//! Supported:
//! - Nested block mappings (indentation-based) and block sequences (`- `)
//! - Scalars: null/`~`, booleans, integers, floats, strings
//! - Single- and double-quoted scalars, with `\n`/`\t`/`\"`/`\\` escapes
//! - `#` comments, blank lines, leading `---` and trailing `...`
//! - Flow collections that are valid JSON (`[1, 2]`, `{"a": 1}`)
//!
//! Rejected with a clear error (rather than mis-parsed):
//! - Anchors and aliases (`&anchor`, `*alias`), merge keys (`<<`)
//! - Block scalars (`|`, `>`)
//! - Explicit tags (`!!str`, `!Custom`)
//! - Multiple documents in one string
//!
//! # Example
//!
//! ```
//! use aethershell::yaml;
//! use aethershell::value::Value;
//!
//! let v = yaml::parse("name: ae\nports:\n  - 80\n  - 443\n").unwrap();
//! let Value::Record(r) = &v else { panic!("expected a record") };
//! assert_eq!(r.get("name"), Some(&Value::Str("ae".into())));
//! assert_eq!(
//!     r.get("ports"),
//!     Some(&Value::Array(vec![Value::Int(80), Value::Int(443)]))
//! );
//! ```

use crate::value::Value;
use anyhow::{anyhow, Result};
use std::collections::BTreeMap;

/// One significant line: its indentation depth and its content.
#[derive(Debug, Clone)]
struct Line {
    indent: usize,
    content: String,
    /// 1-based source line number, for error messages.
    number: usize,
}

/// Parse a YAML document into a [`Value`].
///
/// Returns an error naming the unsupported construct rather than guessing —
/// see the module docs for the supported subset.
pub fn parse(src: &str) -> Result<Value> {
    let lines = significant_lines(src)?;
    if lines.is_empty() {
        return Ok(Value::Null);
    }
    let mut pos = 0usize;
    let base = lines[0].indent;
    let value = parse_block(&lines, &mut pos, base)?;
    if pos < lines.len() {
        return Err(anyhow!(
            "line {}: unexpected content at indentation {} (expected {})",
            lines[pos].number,
            lines[pos].indent,
            base
        ));
    }
    Ok(value)
}

/// Strip blanks, comments and document markers; reject unsupported constructs.
fn significant_lines(src: &str) -> Result<Vec<Line>> {
    let mut out = Vec::new();
    let mut seen_doc_start = false;

    for (i, raw) in src.lines().enumerate() {
        let number = i + 1;
        let indent = raw.len() - raw.trim_start().len();
        let trimmed = raw.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed == "..." {
            break; // explicit end of document
        }
        if trimmed == "---" {
            if seen_doc_start || !out.is_empty() {
                return Err(anyhow!(
                    "line {number}: multiple YAML documents are not supported; \
                     split them and parse each separately"
                ));
            }
            seen_doc_start = true;
            continue;
        }

        reject_unsupported(trimmed, number)?;

        out.push(Line {
            indent,
            content: trimmed.to_string(),
            number,
        });
    }
    Ok(out)
}

/// Reject YAML features this parser does not implement, by name.
fn reject_unsupported(line: &str, number: usize) -> Result<()> {
    // Look at the value side of `key:` when there is one, else the whole line.
    let value_part: String = match split_key(line) {
        Some((_, rest)) => rest.trim().to_string(),
        None => line.trim_start_matches("- ").trim().to_string(),
    };
    let value_part = value_part.as_str();

    let unsupported = [
        ("&", "anchors (&name)"),
        ("*", "aliases (*name)"),
        ("!", "explicit tags (!!type)"),
        ("|", "block scalars (|)"),
        (">", "folded block scalars (>)"),
    ];
    for (marker, name) in unsupported {
        if value_part.starts_with(marker) {
            return Err(anyhow!(
                "line {number}: {name} are not supported by AetherShell's YAML \
                 subset. Convert the document to JSON, or simplify it."
            ));
        }
    }
    if line.starts_with("<<") {
        return Err(anyhow!(
            "line {number}: merge keys (<<) are not supported by AetherShell's \
             YAML subset."
        ));
    }
    Ok(())
}

/// Parse a block (mapping or sequence) at the given indentation.
fn parse_block(lines: &[Line], pos: &mut usize, indent: usize) -> Result<Value> {
    if *pos >= lines.len() {
        return Ok(Value::Null);
    }
    if lines[*pos].content.starts_with("- ") || lines[*pos].content == "-" {
        parse_sequence(lines, pos, indent)
    } else {
        parse_mapping(lines, pos, indent)
    }
}

fn parse_sequence(lines: &[Line], pos: &mut usize, indent: usize) -> Result<Value> {
    let mut items = Vec::new();
    while *pos < lines.len() && lines[*pos].indent == indent {
        let line = &lines[*pos];
        if !(line.content.starts_with("- ") || line.content == "-") {
            break;
        }
        let rest = line.content[1..].trim().to_string();
        *pos += 1;

        if rest.is_empty() {
            // `-` alone: the item is the nested block beneath it.
            items.push(parse_nested(lines, pos, indent)?);
        } else if let Some((key, value)) = split_key(&rest) {
            // `- key: value` — a mapping that starts on the dash line. Its
            // logical indent is where the key text begins.
            let inner_indent = indent + (line.content.len() - rest.len());
            let mut map = BTreeMap::new();
            insert_entry(&mut map, key, value, lines, pos, inner_indent, line.number)?;
            while *pos < lines.len() && lines[*pos].indent == inner_indent {
                let l = &lines[*pos];
                let Some((k, v)) = split_key(&l.content) else {
                    break;
                };
                let n = l.number;
                *pos += 1;
                insert_entry(&mut map, k, v, lines, pos, inner_indent, n)?;
            }
            items.push(Value::Record(map));
        } else {
            items.push(parse_scalar(&rest, line.number)?);
        }
    }
    Ok(Value::Array(items))
}

fn parse_mapping(lines: &[Line], pos: &mut usize, indent: usize) -> Result<Value> {
    let mut map = BTreeMap::new();
    while *pos < lines.len() && lines[*pos].indent == indent {
        let line = lines[*pos].clone();
        let Some((key, value)) = split_key(&line.content) else {
            return Err(anyhow!(
                "line {}: expected `key: value`, found {:?}",
                line.number,
                line.content
            ));
        };
        *pos += 1;
        insert_entry(&mut map, key, value, lines, pos, indent, line.number)?;
    }
    Ok(Value::Record(map))
}

/// Insert one `key: value` entry, descending into a nested block when the
/// value is empty.
fn insert_entry(
    map: &mut BTreeMap<String, Value>,
    key: String,
    value: String,
    lines: &[Line],
    pos: &mut usize,
    indent: usize,
    number: usize,
) -> Result<()> {
    let parsed = if value.trim().is_empty() {
        parse_nested(lines, pos, indent)?
    } else {
        parse_scalar(value.trim(), number)?
    };
    if map.insert(key.clone(), parsed).is_some() {
        // Duplicate keys are a YAML error, and silently keeping the last one
        // is precisely the sort of quiet data loss this module exists to stop.
        return Err(anyhow!("line {number}: duplicate key {key:?}"));
    }
    Ok(())
}

/// Parse the block nested beneath the current line, if any.
fn parse_nested(lines: &[Line], pos: &mut usize, indent: usize) -> Result<Value> {
    if *pos < lines.len() && lines[*pos].indent > indent {
        let deeper = lines[*pos].indent;
        parse_block(lines, pos, deeper)
    } else {
        // `key:` with nothing under it is an explicit null, as in YAML.
        Ok(Value::Null)
    }
}

/// Split `key: value`, respecting quotes so `"a: b": 1` works.
///
/// Returns `None` when the line has no top-level `:` separator.
fn split_key(line: &str) -> Option<(String, String)> {
    let bytes: Vec<char> = line.chars().collect();
    let mut quote: Option<char> = None;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) => {
                if c == '\\' {
                    i += 1;
                } else if c == q {
                    quote = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                } else if c == ':' {
                    // A `:` only separates when followed by space or EOL —
                    // otherwise it is part of a scalar such as `12:30` or a URL.
                    let next = bytes.get(i + 1);
                    if next.is_none() || next == Some(&' ') {
                        let key = line[..byte_index(line, i)].trim();
                        let value = line[byte_index(line, i + 1)..].trim();
                        return Some((unquote(key), value.to_string()));
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// Byte offset of the `n`th character.
fn byte_index(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map(|(b, _)| b).unwrap_or(s.len())
}

/// Strip surrounding quotes from a key, if present.
fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2
        && ((t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')))
    {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// Parse a scalar: quoted string, JSON flow collection, or plain scalar.
fn parse_scalar(raw: &str, number: usize) -> Result<Value> {
    let s = raw.trim();

    // Quoted strings keep everything inside verbatim (after escapes), so a `#`
    // or `:` inside quotes is data, not syntax.
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        return Ok(Value::Str(unescape_double(&s[1..s.len() - 1])));
    }
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        // Single quotes: only '' is an escape, for a literal quote.
        return Ok(Value::Str(s[1..s.len() - 1].replace("''", "'")));
    }

    // Flow collections: JSON is a subset of YAML flow style, so anything that
    // parses as JSON is correct here. Anything else in flow style is rejected
    // rather than guessed at.
    if s.starts_with('[') || s.starts_with('{') {
        return match serde_json::from_str::<serde_json::Value>(s) {
            Ok(j) => Ok(json_to_value(j)),
            Err(e) => Err(anyhow!(
                "line {number}: flow collection {s:?} is not valid JSON ({e}); \
                 AetherShell's YAML subset supports JSON-compatible flow style only"
            )),
        };
    }

    // Plain scalar: an unquoted ` #` starts a comment.
    let s = match s.find(" #") {
        Some(i) => s[..i].trim(),
        None => s,
    };

    Ok(plain_scalar(s))
}

/// Type a plain (unquoted) scalar the way YAML 1.2 core schema does.
fn plain_scalar(s: &str) -> Value {
    match s {
        "" | "~" | "null" | "Null" | "NULL" => Value::Null,
        "true" | "True" | "TRUE" => Value::Bool(true),
        "false" | "False" | "FALSE" => Value::Bool(false),
        _ => {
            if let Ok(i) = s.parse::<i64>() {
                Value::Int(i)
            } else if let Ok(f) = s.parse::<f64>() {
                // Reject things Rust parses but YAML does not treat as numbers,
                // e.g. "inf" alone; YAML spells it `.inf`.
                if s.chars().any(|c| c.is_ascii_digit()) {
                    Value::Float(f)
                } else {
                    Value::Str(s.to_string())
                }
            } else {
                Value::Str(s.to_string())
            }
        }
    }
}

fn unescape_double(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn json_to_value(j: serde_json::Value) -> Value {
    match j {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(Value::Int)
            .or_else(|| n.as_f64().map(Value::Float))
            .unwrap_or(Value::Null),
        serde_json::Value::String(s) => Value::Str(s),
        serde_json::Value::Array(a) => Value::Array(a.into_iter().map(json_to_value).collect()),
        serde_json::Value::Object(o) => {
            Value::Record(o.into_iter().map(|(k, v)| (k, json_to_value(v))).collect())
        }
    }
}

// ============================================================================
// EMIT
// ============================================================================

/// Render a [`Value`] as YAML.
///
/// Output is quoted wherever quoting is required, so the result reads back
/// through [`parse`] as the same value — the property the previous emitter did
/// not have.
pub fn emit(value: &Value) -> String {
    let mut out = String::new();
    emit_into(value, 0, &mut out);
    if out.is_empty() {
        out.push_str("null\n");
    }
    out
}

fn emit_into(value: &Value, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    match value {
        Value::Record(map) if map.is_empty() => out.push_str("{}\n"),
        Value::Record(map) => {
            for (k, v) in map {
                out.push_str(&pad);
                out.push_str(&quote_key(k));
                match v {
                    Value::Record(inner) if !inner.is_empty() => {
                        out.push_str(":\n");
                        emit_into(v, indent + 2, out);
                    }
                    Value::Array(items) if !items.is_empty() => {
                        out.push_str(":\n");
                        emit_into(v, indent + 2, out);
                    }
                    _ => {
                        out.push_str(": ");
                        out.push_str(&scalar_to_yaml(v));
                        out.push('\n');
                    }
                }
            }
        }
        Value::Array(items) if items.is_empty() => out.push_str("[]\n"),
        Value::Array(items) => {
            for item in items {
                out.push_str(&pad);
                match item {
                    Value::Record(_) | Value::Array(_) => {
                        // Nested collections start on the following line so the
                        // dash does not disturb their indentation.
                        out.push_str("-\n");
                        emit_into(item, indent + 2, out);
                    }
                    _ => {
                        out.push_str("- ");
                        out.push_str(&scalar_to_yaml(item));
                        out.push('\n');
                    }
                }
            }
        }
        other => {
            out.push_str(&pad);
            out.push_str(&scalar_to_yaml(other));
            out.push('\n');
        }
    }
}

fn quote_key(k: &str) -> String {
    if needs_quoting(k) {
        format!("\"{}\"", escape_double(k))
    } else {
        k.to_string()
    }
}

fn scalar_to_yaml(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.is_finite() {
                f.to_string()
            } else if f.is_nan() {
                ".nan".to_string()
            } else if *f > 0.0 {
                ".inf".to_string()
            } else {
                "-.inf".to_string()
            }
        }
        Value::Str(s) | Value::Uri(s) => {
            if needs_quoting(s) {
                format!("\"{}\"", escape_double(s))
            } else {
                s.clone()
            }
        }
        // Anything else (lambdas, futures, tables) has no YAML representation;
        // render its display form as a quoted string rather than emitting
        // something that would not read back.
        other => format!("\"{}\"", escape_double(&format!("{other:?}"))),
    }
}

/// Whether a scalar must be quoted to survive a round trip.
fn needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    // Anything that would re-parse as a different type, or that contains
    // structural characters, has to be quoted.
    if !matches!(plain_scalar(s), Value::Str(_)) {
        return true;
    }
    s.contains(':')
        || s.contains('#')
        || s.contains('\n')
        || s.contains('\t')
        || s.contains('"')
        || s.contains('\'')
        || s.starts_with([
            '-', '?', '&', '*', '!', '|', '>', '%', '@', '`', '[', '{', ',',
        ])
        || s.trim() != s
}

fn escape_double(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
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

    // ---- the failures of the previous line-splitting implementation ----

    #[test]
    fn parses_nested_mappings() {
        // The old parser flattened this into one record, losing the structure.
        let v = parse("server:\n  host: localhost\n  port: 8080\n").unwrap();
        assert_eq!(
            v,
            rec(&[(
                "server",
                rec(&[
                    ("host", Value::Str("localhost".into())),
                    ("port", Value::Int(8080)),
                ])
            )])
        );
    }

    #[test]
    fn parses_sequences() {
        // The old parser dropped these entirely: a `- item` line has no colon.
        let v = parse("ports:\n  - 80\n  - 443\n").unwrap();
        assert_eq!(
            v,
            rec(&[("ports", Value::Array(vec![Value::Int(80), Value::Int(443)]))])
        );
    }

    #[test]
    fn parses_sequence_of_mappings() {
        let v = parse("users:\n  - name: ada\n    admin: true\n  - name: bob\n    admin: false\n")
            .unwrap();
        let Value::Record(r) = &v else { panic!() };
        let Some(Value::Array(items)) = r.get("users") else {
            panic!("expected array, got {r:?}")
        };
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0],
            rec(&[
                ("name", Value::Str("ada".into())),
                ("admin", Value::Bool(true))
            ])
        );
    }

    #[test]
    fn strips_comments() {
        // The old parser kept " # the port" as part of the value.
        let v = parse("# leading\nport: 8080 # the port\n").unwrap();
        assert_eq!(v, rec(&[("port", Value::Int(8080))]));
    }

    #[test]
    fn keeps_hash_inside_quotes() {
        let v = parse(r##"color: "#ff0000""##).unwrap();
        assert_eq!(v, rec(&[("color", Value::Str("#ff0000".into()))]));
    }

    #[test]
    fn ignores_document_start_marker() {
        // The old parser turned `---` into a {"---": ""} entry.
        let v = parse("---\nname: ae\n").unwrap();
        assert_eq!(v, rec(&[("name", Value::Str("ae".into()))]));
    }

    #[test]
    fn colon_inside_a_value_is_not_a_separator() {
        let v = parse("time: 12:30\nurl: https://example.com/x\n").unwrap();
        assert_eq!(
            v,
            rec(&[
                ("time", Value::Str("12:30".into())),
                ("url", Value::Str("https://example.com/x".into())),
            ])
        );
    }

    // ---- typing ----

    #[test]
    fn types_plain_scalars() {
        let v = parse("i: 42\nf: 1.5\nt: true\nf2: False\nn: null\ne: ~\ns: hello\n").unwrap();
        assert_eq!(
            v,
            rec(&[
                ("i", Value::Int(42)),
                ("f", Value::Float(1.5)),
                ("t", Value::Bool(true)),
                ("f2", Value::Bool(false)),
                ("n", Value::Null),
                ("e", Value::Null),
                ("s", Value::Str("hello".into())),
            ])
        );
    }

    #[test]
    fn quoted_numbers_stay_strings() {
        let v = parse("version: \"1.0\"\nzip: '01234'\n").unwrap();
        assert_eq!(
            v,
            rec(&[
                ("version", Value::Str("1.0".into())),
                ("zip", Value::Str("01234".into())),
            ])
        );
    }

    #[test]
    fn handles_escapes_in_double_quotes() {
        let v = parse(r#"msg: "line1\nline2\ttabbed""#).unwrap();
        assert_eq!(
            v,
            rec(&[("msg", Value::Str("line1\nline2\ttabbed".into()))])
        );
    }

    #[test]
    fn parses_json_flow_collections() {
        let v = parse("list: [1, 2, 3]\nmap: {\"a\": 1}\n").unwrap();
        assert_eq!(
            v,
            rec(&[
                (
                    "list",
                    Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
                ),
                ("map", rec(&[("a", Value::Int(1))])),
            ])
        );
    }

    #[test]
    fn key_with_no_value_is_null() {
        let v = parse("empty:\n").unwrap();
        assert_eq!(v, rec(&[("empty", Value::Null)]));
    }

    // ---- loud failure instead of quiet corruption ----

    #[test]
    fn rejects_unsupported_constructs_by_name() {
        for (src, expect) in [
            ("a: &anchor 1\n", "anchors"),
            ("a: *alias\n", "aliases"),
            ("a: !!str 1\n", "tags"),
            ("a: |\n  block\n", "block scalars"),
            ("a: >\n  folded\n", "folded"),
            ("<<: base\n", "merge keys"),
            ("a: 1\n---\nb: 2\n", "multiple YAML documents"),
        ] {
            let err = parse(src).unwrap_err().to_string();
            assert!(
                err.contains(expect),
                "parsing {src:?} should name {expect:?}, said: {err}"
            );
        }
    }

    #[test]
    fn rejects_duplicate_keys() {
        // Silently keeping the last value is the class of bug this module exists
        // to eliminate.
        let err = parse("a: 1\na: 2\n").unwrap_err().to_string();
        assert!(err.contains("duplicate key"), "got {err}");
    }

    #[test]
    fn rejects_malformed_flow_collections() {
        let err = parse("a: [1, 2\n").unwrap_err().to_string();
        assert!(err.contains("not valid JSON"), "got {err}");
    }

    #[test]
    fn rejects_a_line_that_is_not_a_mapping_entry() {
        let err = parse("just a bare line\n").unwrap_err().to_string();
        assert!(err.contains("expected `key: value`"), "got {err}");
    }

    #[test]
    fn empty_input_is_null() {
        assert_eq!(parse("").unwrap(), Value::Null);
        assert_eq!(parse("# only a comment\n").unwrap(), Value::Null);
    }

    // ---- emit ----

    #[test]
    fn emits_nested_structures() {
        let v = rec(&[(
            "server",
            rec(&[
                ("host", Value::Str("localhost".into())),
                ("port", Value::Int(8080)),
            ]),
        )]);
        assert_eq!(emit(&v), "server:\n  host: localhost\n  port: 8080\n");
    }

    #[test]
    fn emits_sequences() {
        let v = rec(&[("ports", Value::Array(vec![Value::Int(80), Value::Int(443)]))]);
        assert_eq!(emit(&v), "ports:\n  - 80\n  - 443\n");
    }

    #[test]
    fn quotes_values_that_would_not_read_back() {
        // The old emitter wrote these bare, producing unreadable YAML.
        let v = rec(&[
            ("colon", Value::Str("a: b".into())),
            ("hash", Value::Str("#tag".into())),
            ("numeric", Value::Str("42".into())),
            ("empty", Value::Str(String::new())),
        ]);
        let out = emit(&v);
        assert!(out.contains(r#"colon: "a: b""#), "got {out}");
        assert!(out.contains(r##"hash: "#tag""##), "got {out}");
        assert!(out.contains(r#"numeric: "42""#), "got {out}");
        assert!(out.contains(r#"empty: """#), "got {out}");
    }

    #[test]
    fn round_trips() {
        // The property that matters: emit then parse yields the same value.
        let original = rec(&[
            ("name", Value::Str("ae".into())),
            ("version", Value::Str("1.6.0".into())),
            ("port", Value::Int(8080)),
            ("ratio", Value::Float(0.5)),
            ("debug", Value::Bool(false)),
            ("nothing", Value::Null),
            ("tricky", Value::Str("a: b # c".into())),
            (
                "nested",
                rec(&[("deep", Value::Array(vec![Value::Int(1), Value::Int(2)]))]),
            ),
            (
                "list",
                Value::Array(vec![Value::Str("x".into()), Value::Str("y".into())]),
            ),
        ]);
        let text = emit(&original);
        let reparsed = parse(&text).unwrap_or_else(|e| panic!("reparse failed: {e}\n---\n{text}"));
        assert_eq!(reparsed, original, "round trip changed the value:\n{text}");
    }

    #[test]
    fn round_trips_a_sequence_of_records() {
        let original = rec(&[(
            "users",
            Value::Array(vec![
                rec(&[("name", Value::Str("ada".into()))]),
                rec(&[("name", Value::Str("bob".into()))]),
            ]),
        )]);
        let text = emit(&original);
        assert_eq!(parse(&text).unwrap(), original, "\n{text}");
    }

    #[test]
    fn emits_empty_collections() {
        assert_eq!(emit(&Value::Record(BTreeMap::new())), "{}\n");
        assert_eq!(emit(&Value::Array(vec![])), "[]\n");
    }
}
