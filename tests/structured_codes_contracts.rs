//! Errors that named their code only in prose now carry it structurally.
//!
//! Before this, several sites built `anyhow!("E_BUDGET_EXCEEDED: ...")` or the
//! like. At the boundary `ensure_structured` wrapped those as `E_UNKNOWN`, so
//! the real code survived only as text inside the message, and an agent that
//! branched on `.code` saw `E_UNKNOWN` and was told to stop. Each site below is
//! now a `SafetyError` with its code, and the tests read `error.code` from the
//! JSON the error renders to, not a substring of the prose.
//!
//! The ratchet at the bottom scans `src/` and fails if any string literal that
//! begins with an `E_` code is handed to an error constructor again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use iron_shell::env::Env;
use iron_shell::value::Value;

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    iron_shell::builtins::call(name, args, &mut env)
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

/// The structured fields of a refusal, read from its rendered JSON.
struct Refusal {
    code: String,
    message: String,
    builtin: String,
    retryable: bool,
}

fn structured(e: &anyhow::Error) -> Refusal {
    let v: serde_json::Value = serde_json::from_str(&e.to_string())
        .unwrap_or_else(|_| panic!("refusal is not a structured error: {e}"));
    let field = |k: &str| v["error"][k].as_str().unwrap_or_default().to_string();
    Refusal {
        code: field("code"),
        message: field("message"),
        builtin: field("builtin"),
        retryable: v["error"]["retryable"].as_bool().unwrap_or(true),
    }
}

fn refusal_of(name: &str, args: Vec<Value>) -> Refusal {
    match call(name, args) {
        Err(e) => structured(&e),
        Ok(v) => panic!("{name} succeeded where a refusal was expected: {v:?}"),
    }
}

/// The message must carry no code prefix: the code is in `.code`, and a
/// message that still starts with `E_X:` is the old defect in a new place.
fn assert_prose_only(r: &Refusal) {
    assert!(
        !r.message.starts_with("E_") && !r.message.contains(": E_"),
        "the code is still spelled out in the message: {}",
        r.message
    );
}

#[test]
fn neural_parameter_budget_is_coded_budget_exceeded() {
    // 1000 * 1000 + 1000 biases is over the 1,000,000-parameter ceiling, and the
    // count is taken before any weight is allocated.
    let r = refusal_of(
        "nn_create",
        vec![
            s("too-big"),
            Value::Array(vec![Value::Int(1000), Value::Int(1000)]),
        ],
    );
    assert_eq!(r.code, "E_BUDGET_EXCEEDED");
    assert!(!r.retryable, "an exhausted envelope is not retryable");
    assert_eq!(r.builtin, "nn_create", "the boundary names the builtin");
    assert!(r.message.contains("1000000 parameters"), "{}", r.message);
    assert_prose_only(&r);
}

#[test]
fn oversized_encoded_network_is_coded_budget_exceeded() {
    // `value_to_nn` refuses an encoded payload over 64 MiB before decoding it.
    let mut rec = BTreeMap::new();
    rec.insert(
        "_data".to_string(),
        Value::Str("x".repeat(64 * 1024 * 1024 + 1)),
    );
    let r = refusal_of("nn_info", vec![Value::Record(rec)]);
    assert_eq!(r.code, "E_BUDGET_EXCEEDED");
    assert!(r.message.contains("64 MiB"), "{}", r.message);
    assert_prose_only(&r);
}

#[test]
fn empty_password_is_bad_arg_with_no_prose_code() {
    // Was `arg_err("E_CRYPTO_BAD_INPUT: ...")`, which the boundary filed under
    // E_BAD_ARG while the message still named a code that did not exist.
    let r = refusal_of("crypto_encrypt", vec![s("plaintext"), s("")]);
    assert_eq!(r.code, "E_BAD_ARG");
    assert!(
        !r.message.contains("E_CRYPTO_BAD_INPUT"),
        "the invented code must not leak into the message: {}",
        r.message
    );
    assert!(r.message.contains("non-empty password"), "{}", r.message);
    assert_prose_only(&r);
}

#[test]
fn fips_refusal_keeps_its_documented_code() {
    // `docs/security/CRYPTO_AND_FIPS.md` names E_FIPS_DISALLOWED, so the code is
    // a variant of its own rather than a generic policy denial.
    std::env::set_var("IRONSHELL_FIPS", "1");
    let md5 = refusal_of("md5", vec![s("abc")]);
    // `sha1` is not a builtin name; the algorithm is the second argument here.
    let sha1 = refusal_of("crypto_hash", vec![s("abc"), s("sha1")]);
    let ok = call("sha256", vec![s("abc")]);
    std::env::remove_var("IRONSHELL_FIPS");

    for r in [&md5, &sha1] {
        assert_eq!(r.code, "E_FIPS_DISALLOWED", "{}", r.message);
        assert!(!r.retryable);
        assert!(!r.builtin.is_empty(), "the boundary must name the builtin");
        assert_prose_only(r);
    }
    assert!(ok.is_ok(), "sha256 is FIPS-approved");
}

#[test]
fn database_open_failure_is_coded_io() {
    // A database under a directory that does not exist cannot be opened. The
    // failure is neither absence nor a refusal, so it is E_IO, and nothing is
    // created on disk.
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&dir).expect("tmpdir");
    let csv = dir.join("structured_codes_import.csv");
    std::fs::write(&csv, "a,b\n1,2\n").expect("write csv fixture");
    let db = "structured-codes-no-such-dir/import.db";

    let r = refusal_of(
        "db_csv_to_sqlite",
        vec![s(db), s(&csv.display().to_string())],
    );
    assert_eq!(r.code, "E_IO", "{}", r.message);
    assert!(!r.retryable);
    assert!(r.message.contains("cannot open"), "{}", r.message);
    assert_prose_only(&r);
    assert!(
        !Path::new(db).exists(),
        "a failed open must not create the database"
    );
}

#[test]
fn crypto_failed_constructor_carries_its_code() {
    let e = iron_shell::safety::crypto_failed("crypto_encrypt", "boom", "retry later");
    let r = structured(&e);
    assert_eq!(r.code, "E_CRYPTO_FAILED");
    assert!(!r.retryable);
    assert_eq!(r.builtin, "crypto_encrypt");
}

// ---------------------------------------------------------------------------
// Ratchet: no `anyhow!("E_`, `bail!("E_`, `format!("E_` or `arg_err("E_` in src/
// ---------------------------------------------------------------------------

/// Blank out comments, keeping string literals and line numbers intact.
///
/// Handles `//` line comments, nested `/* */` block comments, normal strings
/// with escapes, raw strings (`r"..."`, `r#"..."#`, with the hash count checked
/// on the closing delimiter, across lines), and char literals such as `'"'`, so
/// a quote inside a char literal does not open a string.
fn blank_comments(src: &str) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum St {
        Code,
        Str,
        Raw(usize),
    }
    let c: Vec<char> = src.chars().collect();
    let at = |i: usize| c.get(i).copied();
    let mut out = String::with_capacity(src.len());
    let mut st = St::Code;
    let mut block = 0usize;
    let mut i = 0usize;
    while i < c.len() {
        let ch = c[i];
        if block > 0 {
            if ch == '*' && at(i + 1) == Some('/') {
                block -= 1;
                out.push_str("  ");
                i += 2;
            } else if ch == '/' && at(i + 1) == Some('*') {
                block += 1;
                out.push_str("  ");
                i += 2;
            } else {
                out.push(if ch == '\n' { '\n' } else { ' ' });
                i += 1;
            }
            continue;
        }
        match st {
            St::Str => {
                out.push(ch);
                if ch == '\\' && i + 1 < c.len() {
                    out.push(c[i + 1]);
                    i += 2;
                    continue;
                }
                if ch == '"' {
                    st = St::Code;
                }
                i += 1;
            }
            St::Raw(n) => {
                // The contents of a raw string are data, not code, so they are
                // blanked: a fixture that quotes a constructor is not a call.
                if ch == '"' && (1..=n).all(|k| at(i + k) == Some('#')) {
                    out.push('"');
                    for _ in 0..n {
                        out.push('#');
                    }
                    st = St::Code;
                    i += 1 + n;
                } else {
                    out.push(if ch == '\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
            St::Code => {
                if ch == '/' && at(i + 1) == Some('/') {
                    while i < c.len() && c[i] != '\n' {
                        out.push(' ');
                        i += 1;
                    }
                } else if ch == '/' && at(i + 1) == Some('*') {
                    block = 1;
                    out.push_str("  ");
                    i += 2;
                } else if ch == 'r'
                    && !(i > 0 && (c[i - 1].is_alphanumeric() || c[i - 1] == '_'))
                    && matches!(at(i + 1), Some('"') | Some('#'))
                {
                    let mut j = i + 1;
                    while at(j) == Some('#') {
                        j += 1;
                    }
                    if at(j) == Some('"') {
                        let n = j - (i + 1);
                        out.push('r');
                        for _ in 0..n {
                            out.push('#');
                        }
                        out.push('"');
                        st = St::Raw(n);
                        i = j + 1;
                    } else {
                        out.push(ch);
                        i += 1;
                    }
                } else if ch == '"' {
                    st = St::Str;
                    out.push(ch);
                    i += 1;
                } else if ch == '\'' {
                    // A char literal: '"', '\n', '\u{..}'. A lifetime such as 'a
                    // has no closing quote, so it is left as code.
                    if at(i + 1) == Some('\\') {
                        let mut j = i + 2;
                        while j < c.len() && c[j] != '\'' && j - i < 12 {
                            j += 1;
                        }
                        if at(j) == Some('\'') {
                            out.extend(c[i..=j].iter());
                            i = j + 1;
                            continue;
                        }
                    } else if at(i + 2) == Some('\'') {
                        out.extend(c[i..=i + 2].iter());
                        i += 3;
                        continue;
                    }
                    out.push(ch);
                    i += 1;
                } else {
                    out.push(ch);
                    i += 1;
                }
            }
        }
    }
    out
}

/// Every place where an error constructor is handed a string literal that
/// begins with an `E_` code, as `(1-based line, macro)`.
fn prose_code_sites(src: &str) -> Vec<(usize, &'static str)> {
    let clean = blank_comments(src);
    let bytes = clean.as_bytes();
    let skip_ws = |mut j: usize| {
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        j
    };
    let mut hits = Vec::new();
    for name in ["anyhow!", "bail!", "format!", "arg_err"] {
        let mut from = 0;
        while let Some(pos) = clean[from..].find(name) {
            let start = from + pos;
            from = start + name.len();
            let j = skip_ws(from);
            if bytes.get(j) != Some(&b'(') {
                continue;
            }
            let j = skip_ws(j + 1);
            let is_code = bytes.get(j) == Some(&b'"')
                && bytes.get(j + 1) == Some(&b'E')
                && bytes.get(j + 2) == Some(&b'_')
                && bytes.get(j + 3).is_some_and(|b| b.is_ascii_uppercase());
            if is_code {
                let line = clean[..start].matches('\n').count() + 1;
                hits.push((line, name));
            }
        }
    }
    hits.sort_unstable();
    hits
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn scanner_sees_code_and_ignores_comments_and_raw_strings() {
    // Self-test, so the ratchet cannot pass by being blind.
    let src = r##"
fn a() -> anyhow::Error { anyhow!("E_FOO: x") }
// anyhow!("E_COMMENTED: x")
/* bail!("E_BLOCK: x") */
fn b() { let fixture = r#"anyhow!("E_RAW: x")"#; let c = '"'; }
fn d() { arg_err(
    "E_MULTI: y",
); }
fn e() -> &'static str { "E_PLAIN: no constructor here" }
"##;
    let hits = prose_code_sites(src);
    let names: Vec<&str> = hits.iter().map(|(_, n)| *n).collect();
    assert_eq!(names, vec!["anyhow!", "arg_err"], "{hits:?}");
    assert_eq!(
        hits[0].0, 2,
        "line numbers count from the start of the file"
    );
    assert_eq!(
        hits[1].0, 6,
        "a call split across lines is found at its macro"
    );
}

#[test]
fn no_src_file_hands_a_code_name_to_an_error_as_prose() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    assert!(files.len() > 10, "scanned too few files: {}", files.len());

    let mut offenders = Vec::new();
    for file in &files {
        let src = std::fs::read_to_string(file).expect("read source");
        for (line, name) in prose_code_sites(&src) {
            offenders.push(format!("{}:{line} {name}\"E_..\"", file.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "error constructors must take a message and a structured code, not an E_ prefix:\n{}",
        offenders.join("\n")
    );
}
