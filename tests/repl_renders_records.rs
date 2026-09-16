//! An array of records is the most common thing a shell produces. It rendered
//! as nothing.
//!
//! ```text
//! $ ae -c 'ls("src") | pick("name", "size")'
//! [{…}, {…}, {…}, {…}, … forty-five of them]
//! ```
//!
//! That is the flagship example from the README and the book, and the output
//! contains no filenames and no sizes. `pp_item` rendered every nested
//! `Record` as `{…}` unconditionally, one level too early.
//!
//! It was not a token budget, though it looked like one: `[{a: 1}, {a: 2}]`
//! did it too, with two tiny records and no budget in sight. Volume is already
//! handled by `budget_value` before the value reaches the renderer, so eliding
//! here bought nothing and cost the answer.
//!
//! How it was found is the uncomfortable part. A benchmark of this repository
//! measured AetherShell's output for that exact command at 316 bytes against
//! bash's 2,650 and reported a 7.9x token advantage. The 316 bytes were forty-
//! five copies of `{…}`. The harness had no oracle for those tasks -- it
//! checked only that the exit status was zero and the output non-empty -- so a
//! result containing none of the requested data scored as a win. The lesson is
//! in `benches/agentic/README.md`: an output-size comparison is meaningless
//! unless something asserts that both outputs contain the answer.
//!
//! These tests drive the real binary, because the defect was in how the binary
//! prints and nothing below that boundary could see it.

use std::process::Command;

fn ae(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_ae"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run ae");
    assert!(
        out.status.success(),
        "ae {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn an_array_of_records_shows_its_fields() {
    assert_eq!(ae(&["-c", "[{a: 1}, {a: 2}]"]), "[{a: 1}, {a: 2}]");
}

#[test]
fn the_flagship_listing_contains_the_data_it_was_asked_for() {
    // The command from the README. It must name a real file and a real size,
    // not a placeholder.
    let out = ae(&[
        "-c",
        r#"ls("src") | where(fn(f) => f.name == "lib.rs") | pick("name", "size")"#,
    ]);
    assert!(
        out.contains("lib.rs"),
        "the listing does not contain the filename it selected: {out}"
    );
    assert!(
        out.chars().any(|c| c.is_ascii_digit()),
        "the listing contains no size: {out}"
    );
    assert!(
        !out.contains('…'),
        "the record was elided to a placeholder again: {out}"
    );
}

#[test]
fn the_documented_array_summary_is_unchanged() {
    // `docs/book/src/ai/tools.md` shows `supported_os: [len=5]`, and the book
    // is checked against the shell. Fixing record rendering must not change
    // how an array nested in a record prints.
    assert_eq!(
        ae(&["-c", r#"from_json("{\"a\":1,\"b\":[1,2,3]}")"#]),
        "{a: 1, b: [len=3]}"
    );
}

#[test]
fn nesting_stays_bounded_at_two_levels() {
    // Rendering records inline must not make one line of REPL output
    // unbounded: past depth two, collections summarise again.
    assert_eq!(ae(&["-c", "[{a: {b: {c: 1}}}]"]), "[{a: {fields=1}}]");
    assert_eq!(ae(&["-c", "[{a: [1, 2, 3]}]"]), "[{a: [len=3]}]");
}

#[test]
fn scalars_and_flat_arrays_are_untouched() {
    assert_eq!(ae(&["-c", "[1, 2, 3]"]), "[1, 2, 3]");
    assert_eq!(ae(&["-c", "42"]), "42");
    assert_eq!(ae(&["-c", r#""hello""#]), "hello");
    assert_eq!(ae(&["-c", "{a: 1, b: 2}"]), "{a: 1, b: 2}");
}

#[test]
fn non_vacuity_the_binary_under_test_is_the_one_that_was_built() {
    // If `ae` silently failed or printed nothing, every assertion that checks
    // for an absence would pass.
    let v = ae(&["--version"]);
    assert!(v.starts_with("ae "), "unexpected version banner: {v}");
    assert_eq!(ae(&["-c", "1 + 1"]), "2", "the binary is not evaluating");
    // And the elision marker must still be producible, or "no … in the output"
    // is not evidence of anything.
    assert!(
        ae(&["-c", "[{a: {b: 1}}]"]).contains("fields="),
        "depth-two summarisation is gone; the bound is not being applied"
    );
}
