//! A builtin that writes a file must be journaled or refused in a transaction.
//!
//! `tx_rollback` restores only what `tx::snapshot` recorded (and removes what
//! it recorded as absent). A writer that does neither leaves its change in
//! place while the transaction reports success. Before this rule, eleven
//! writers did exactly that: `file_patch`, `file_insert`, `file_move`,
//! `save_json`, `session_export`, `touch`, `git_ignore`, `platform_db_export`,
//! and the archive and gzip builtins, whose external tools write files no
//! snapshot names.
//!
//! The rule, stated over the source: a top-level function body that contains a
//! write marker must also contain a snapshot or a transaction refusal, or be in
//! `ALLOWED` with the reason it does not need one. The scan is textual, so it
//! sees direct Rust writes only; a writer that hands its work to an external
//! program (`zip`, `tar`) is covered by name in `REQUIRED_COVERED` below, not
//! by the marker scan.
//!
//! `ALLOWED` is pinned to exactly the set of uncovered writers, in both
//! directions: an entry that stops writing must be removed, and a new
//! uncovered writer must be fixed or listed with a reason.

use std::collections::BTreeSet;

/// Syntax that writes a file in the body it appears in.
const WRITE_MARKERS: &[&str] = &[
    "write_file_atomically(",
    "fs::write(",
    "fs::rename(",
    "File::create(",
    ".truncate(true)",
    ".append(true)",
];

/// Syntax that journals the write or refuses it inside a transaction.
const COVERAGE_MARKERS: &[&str] = &[
    "tx::snapshot",
    "tx::snapshot_metadata",
    "refuse_in_transaction(",
    "net_refuse_in_transaction(",
    "tx::is_active()",
];

/// Writers that are deliberately not journaled, each with the reason. This list
/// may only shrink, and it is pinned to the uncovered set by the test below.
const ALLOWED: &[(&str, &str)] = &[
    (
        "write_file_atomically",
        "the shared atomic writer; its callers snapshot the caller's file first, except monitor_store_save, which writes the shell's own store",
    ),
    (
        "bi_config_init",
        "creates the shell's own config.toml once, and never overwrites an existing one",
    ),
    (
        "monitor_store_save",
        "the shell's own monitor store under its config directory, not a caller-named file",
    ),
    (
        "bi_platform_db_init",
        "the shell's own platform store under the home directory, not a caller-named file",
    ),
    (
        "bi_platform_db_store",
        "the shell's own platform store under the home directory, not a caller-named file",
    ),
    (
        "bi_platform_db_delete",
        "the shell's own platform store under the home directory, not a caller-named file",
    ),
    (
        "bi_platform_db_import",
        "the shell's own platform store under the home directory; the caller's file is only read",
    ),
    (
        "scratch_file",
        "writes a fresh file in a new temporary directory that the call removes; no caller path is touched",
    ),
    (
        "bi_db_sqlite_export_csv",
        "unregistered (§5 item 3); tests/sql_injection.rs fires if it is dispatched",
    ),
];

/// Builtins whose coverage is pinned by name: each must contain a coverage
/// marker, whether or not the marker scan sees a direct write in it.
const REQUIRED_COVERED: &[&str] = &[
    "bi_file_write",
    "bi_file_append",
    "bi_file_patch",
    "bi_file_insert",
    "bi_file_move",
    "bi_file_copy",
    "bi_save_json",
    "bi_session_export",
    "bi_platform_db_export",
    "bi_touch",
    "bi_git_ignore",
    "bi_zip_create",
    "bi_zip_add",
    "bi_zip_extract",
    "bi_tar_extract",
    "bi_gzip_compress",
    "bi_gzip_decompress",
];

fn source() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builtins.rs");
    std::fs::read_to_string(path)
        .expect("src/builtins.rs is readable")
        .replace("\r\n", "\n")
}

/// Whether a column-0 line starts a new top-level item. Indented lines and a
/// bare closing brace stay in the item they belong to, so a function's body
/// runs to the next item and a `#[cfg(test)] mod` is its own chunk.
fn starts_item(line: &str) -> bool {
    const STARTS: &[&str] = &[
        "fn ",
        "pub fn ",
        "pub(crate) fn ",
        "#[",
        "mod ",
        "impl",
        "static ",
        "const ",
        "struct ",
        "enum ",
        "use ",
        "type ",
        "trait ",
        "pub ",
        "lazy_static!",
        "thread_local!",
        "macro_rules!",
    ];
    STARTS.iter().any(|s| line.starts_with(s))
}

fn fn_name(line: &str) -> String {
    let rest = line
        .strip_prefix("pub(crate) fn ")
        .or_else(|| line.strip_prefix("pub fn "))
        .or_else(|| line.strip_prefix("fn "));
    match rest {
        Some(r) => r
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect(),
        None => String::new(),
    }
}

/// `(name, text)` per top-level item. Items that are not functions have an
/// empty name and are skipped by the callers.
fn items(src: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in src.split_inclusive('\n') {
        if starts_item(line) {
            out.push((fn_name(line), String::new()));
        }
        if let Some((_, body)) = out.last_mut() {
            body.push_str(line);
        }
    }
    out
}

/// The code of a body without its comments: whole-line comments are dropped,
/// and a trailing `// ...` is cut where it follows whitespace, so `"https://"`
/// in a string is left alone. A marker in a comment is not a call.
fn code_only(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    for line in body.lines() {
        if line.trim_start().starts_with("//") {
            out.push('\n');
            continue;
        }
        let cut = line
            .match_indices("//")
            .find(|(i, _)| *i > 0 && line.as_bytes()[i - 1].is_ascii_whitespace())
            .map_or(line.len(), |(i, _)| i);
        out.push_str(&line[..cut]);
        out.push('\n');
    }
    out
}

fn writes(code: &str) -> bool {
    WRITE_MARKERS.iter().any(|m| code.contains(m))
}

fn covered(code: &str) -> bool {
    COVERAGE_MARKERS.iter().any(|m| code.contains(m))
}

/// Named functions that write, with whether they are covered.
fn scan(src: &str) -> (Vec<String>, usize) {
    let mut uncovered = Vec::new();
    let mut covered_count = 0usize;
    for (name, body) in items(src) {
        if name.is_empty() {
            continue;
        }
        let code = code_only(&body);
        if !writes(&code) {
            continue;
        }
        if covered(&code) {
            covered_count += 1;
        } else {
            uncovered.push(name);
        }
    }
    (uncovered, covered_count)
}

#[test]
fn every_writer_is_journaled_refused_or_listed_with_a_reason() {
    let (uncovered, covered_count) = scan(&source());
    assert!(
        covered_count >= 15,
        "only {covered_count} journaled or refusing writers were found; the scanner has \
         drifted and this test is checking almost nothing"
    );

    let found: BTreeSet<&str> = uncovered.iter().map(String::as_str).collect();
    let allowed: BTreeSet<&str> = ALLOWED.iter().map(|(n, _)| *n).collect();
    let unlisted: Vec<&&str> = found.difference(&allowed).collect();
    assert!(
        unlisted.is_empty(),
        "{} writer(s) neither snapshot nor refuse inside a transaction, so tx_rollback \
         would leave their change in place: {unlisted:?}\n\n\
         Put a `crate::tx::snapshot(&path)` before the write, call \
         `refuse_in_transaction(..)` when the written files cannot be named in advance, \
         or add the function to ALLOWED in tests/tx_coverage_ratchet.rs with the reason.",
        unlisted.len()
    );
    let stale: Vec<&&str> = allowed.difference(&found).collect();
    assert!(
        stale.is_empty(),
        "ALLOWED names functions that no longer write uncovered: {stale:?}. Remove them, \
         so the list stays exactly the set that remains."
    );
}

#[test]
fn the_allowlist_has_a_reason_and_may_only_shrink() {
    for (name, reason) in ALLOWED {
        assert!(
            !reason.trim().is_empty(),
            "{name} is allowed without a reason; the reason is the point"
        );
    }
    assert!(
        ALLOWED.len() <= 10,
        "the tx-coverage allowlist has grown to {}; it may only shrink",
        ALLOWED.len()
    );
}

#[test]
fn the_named_writers_are_covered_by_name() {
    let src = source();
    let items = items(&src);
    for name in REQUIRED_COVERED {
        let Some((_, body)) = items.iter().find(|(n, _)| n.as_str() == *name) else {
            panic!("{name} was not found by the scanner; a rename must update this list");
        };
        assert!(
            covered(&code_only(body)),
            "{name} must snapshot or refuse inside a transaction"
        );
    }
}

#[test]
fn the_scanner_catches_an_unjournaled_writer_and_passes_the_fixed_shapes() {
    let bad = "fn sample_writer(p: &str) {\n    std::fs::write(p, b\"x\").unwrap();\n}\n";
    let (uncovered, _) = scan(bad);
    assert_eq!(uncovered, vec!["sample_writer".to_string()]);

    let snapshotted = "fn sample_fixed(p: &str) {\n    crate::tx::snapshot(p); // pre-image\n    std::fs::write(p, b\"x\").unwrap();\n}\n";
    let (uncovered, covered_count) = scan(snapshotted);
    assert!(uncovered.is_empty());
    assert_eq!(covered_count, 1);

    let refused = "fn sample_refused(p: &str) -> Result<()> {\n    refuse_in_transaction(\"x\", \"why\")?;\n    fs::write(p, b\"x\")?;\n    Ok(())\n}\n";
    let (uncovered, _) = scan(refused);
    assert!(uncovered.is_empty());
}

#[test]
fn the_scanner_reads_only_top_level_functions_and_ignores_comments() {
    // A write inside a test module is not a builtin's write, and a marker in a
    // comment is not a call.
    let src = "fn sample_reader() {\n    // fs::write(p, b\"x\") would be a write\n    let _ = 1;\n}\n\n#[cfg(test)]\nmod tests {\n    fn helper() {\n        std::fs::write(\"p\", b\"x\").unwrap();\n    }\n}\n";
    let (uncovered, covered_count) = scan(src);
    assert!(uncovered.is_empty(), "{uncovered:?}");
    assert_eq!(covered_count, 0);
    assert!(
        !items(src).iter().any(|(n, _)| n == "helper"),
        "functions inside a module are not top-level items"
    );
}
