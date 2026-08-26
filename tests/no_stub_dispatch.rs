//! An advertised builtin must not resolve to a placeholder.
//!
//! `BUILTIN_DISPATCH` carries reserved slots written as `|_, _, _|
//! Ok(Value::Null)`, so the table's indices stay stable while families are
//! filled in. That is fine until a name is registered into one.
//!
//! `mkdir`, `mkdirp` and `file_mkdir` were all registered at index 532. The
//! comment above the reserved run said `533-539`; the first placeholder was at
//! 532. So `mkdir` returned `Ok(Value::Null)` for every input and created
//! nothing, while `bi_file_mkdir` sat in `builtins.rs` fully written, correct,
//! and referenced by nothing.
//!
//! **A silent no-op is worse than a missing builtin.** An unknown name fails
//! loudly and the caller tries something else; this one returns a success value,
//! so a script "succeeds" and the directory is not there. It was found only
//! because a *jail* test expected a refusal and got `Ok(Null)` instead — nothing
//! was looking for it directly.
//!
//! `tests/catalog_reachability.rs` could not see it: it asks whether an
//! advertised name dispatches, and this one dispatched. The question it did not
//! ask is whether it dispatches to anything.
//!
//! This asks that. It reads `BUILTIN_DISPATCH` as text, finds the placeholder
//! rows by their shape, and requires that no name in `BUILTIN_LOOKUP` point at
//! one.

use aethershell::builtins::BUILTIN_LOOKUP;
use std::collections::{BTreeMap, BTreeSet};

fn source() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builtins.rs");
    std::fs::read_to_string(path).expect("src/builtins.rs is readable")
}

/// The dispatch table as (index → what the row calls), where `None` marks a
/// placeholder row.
///
/// Counting rows in order is what gives the index, so **a row shape this does
/// not recognise shifts every later index**. That is not hypothetical: the first
/// version counted only lines beginning with `|`, and missed
///
/// ```text
///     bi_try_repair,                                  // 1140
/// ```
///
/// a bare function reference rather than a closure. Every index after it came
/// out one too low, which made the last row look out of range and made nine
/// correct registrations — `plan_diff`, `rm`, `rmdir`, `touch`, `cd`, and the
/// four `rbac_*` — look as though each called its neighbour's implementation.
/// The report was one edit away from "fixing" all nine.
///
/// What stopped it was noticing that `rm` calling `bi_rmdir` would have failed
/// `tests/filesystem_removal.rs` on the first run. A scanner's output is a claim,
/// and a claim that contradicts a passing test is the scanner's problem first.
/// Hence `the_row_count_matches_the_dispatcher` below, which pins the total
/// against a known-good index rather than trusting the walk.
fn dispatch_rows(src: &str) -> Vec<Option<String>> {
    let lines: Vec<&str> = src.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.contains("BUILTIN_DISPATCH") && l.contains("&["))
        .expect("BUILTIN_DISPATCH must be findable");
    let mut rows = Vec::new();
    for line in &lines[start + 1..] {
        let t = line.trim();
        if t == "];" {
            break;
        }
        if t.starts_with('|') {
            // `|args, input, _| bi_something(args, input),`
            rows.push(
                t.split_once("| ")
                    .and_then(|(_, rest)| rest.split_once('('))
                    .map(|(name, _)| name.trim().to_string())
                    .filter(|n| n.starts_with("bi_")),
            );
        } else if t.starts_with("bi_") && t.contains(',') {
            // A bare function reference: `bi_try_repair,`
            rows.push(Some(t.split(',').next().unwrap_or(t).trim().to_string()));
        }
    }
    rows
}

#[test]
fn no_advertised_name_dispatches_to_a_placeholder() {
    let rows = dispatch_rows(&source());
    assert!(
        rows.len() > 900,
        "only {} dispatch rows parsed; the scanner has drifted and this test is \
         checking almost nothing",
        rows.len()
    );

    let mut by_index: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
    for (name, index) in BUILTIN_LOOKUP.iter() {
        by_index.entry(*index).or_default().push(name);
    }

    let mut offenders = Vec::new();
    for (index, names) in &by_index {
        match rows.get(*index) {
            None => offenders.push(format!(
                "  index {index} is past the end of the table: {names:?}"
            )),
            Some(None) => {
                let mut n = names.clone();
                n.sort_unstable();
                offenders.push(format!("  index {index} is a placeholder: {n:?}"));
            }
            Some(Some(_)) => {}
        }
    }

    assert!(
        offenders.is_empty(),
        "{} dispatch index/indices are advertised but resolve to a placeholder \
         row (`|_, _, _| Ok(Value::Null)`).\n\n\
         Calling one of these names returns a success value and does nothing — \
         which is worse than an unknown builtin, because the caller has no way to \
         tell. `mkdir`, `mkdirp` and `file_mkdir` shipped this way: registered at \
         532 while the reserved range was documented as starting at 533, with \
         `bi_file_mkdir` written and never referenced.\n\n\
         Either wire the implementation into that row, or remove the name from \
         `BUILTIN_LOOKUP`.\n\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

#[test]
fn the_scanner_still_sees_placeholders_and_real_rows() {
    // A check on the checker in both directions. If the row parser stopped
    // recognising placeholders, the test above would pass by finding none; if it
    // stopped recognising real rows, it would flag everything.
    let rows = dispatch_rows(&source());
    let placeholders = rows.iter().filter(|r| r.is_none()).count();
    let real = rows.iter().filter(|r| r.is_some()).count();
    assert!(
        placeholders > 0,
        "no placeholder rows found — the reserved slots are how the table keeps \
         its indices stable, so their absence means the parser has drifted"
    );
    assert!(
        real > 900,
        "only {real} rows resolve to a `bi_*` function; the parser has drifted"
    );
    // And the row that motivated this file must now be a real one.
    assert_eq!(
        rows.get(532).and_then(|r| r.clone()).as_deref(),
        Some("bi_file_mkdir"),
        "index 532 is `mkdir`/`mkdirp`/`file_mkdir` and must call bi_file_mkdir"
    );
}

#[test]
fn every_dispatch_index_that_is_advertised_is_within_the_table() {
    // The other way an index can be wrong. `call_with_input_inner` bounds-checks
    // before indexing, so an out-of-range entry falls through to the second half
    // of the dispatcher and then to "unknown builtin" — a name that looks
    // registered and is not.
    let rows = dispatch_rows(&source());
    let over: BTreeSet<&str> = BUILTIN_LOOKUP
        .iter()
        .filter(|(_, i)| **i >= rows.len())
        .map(|(n, _)| *n)
        .collect();
    assert!(
        over.is_empty(),
        "these names are registered at an index past the end of BUILTIN_DISPATCH: \
         {over:?}"
    );
}

#[test]
fn the_row_count_matches_the_dispatcher() {
    // The check that would have caught the parser bug immediately: pin the walk
    // against indices whose contents are known independently, at both ends of
    // the table. If a row shape stops being recognised, these move.
    let rows = dispatch_rows(&source());
    assert_eq!(
        rows.len(),
        1150,
        "BUILTIN_DISPATCH row count changed. If that is intentional, update this \
         number *and* check that every `map.insert` index still names the row it \
         meant to — inserting a row shifts every index after it."
    );
    // `bi_try_repair` is the bare-function row the first parser missed.
    assert_eq!(
        rows.get(1140).and_then(|r| r.clone()).as_deref(),
        Some("bi_try_repair"),
        "the bare function reference must still be counted as a row"
    );
    // And the last row, which the miscount made look out of range.
    assert_eq!(
        rows.get(1149).and_then(|r| r.clone()).as_deref(),
        Some("bi_rbac_session"),
        "`rbac_session` is registered at 1149 and that row must exist"
    );
}
