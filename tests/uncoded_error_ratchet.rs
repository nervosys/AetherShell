//! Uncoded errors, counted.
//!
//! An `anyhow!(` or `bail!(` that carries no `SafetyError` reaches the agent
//! API as `E_UNKNOWN`: the caller learns that something failed, and nothing
//! about whose fault it was or what to change. A caller-input error is coded
//! with `crate::safety::arg_err` (or `bad_arg`, `budget_exceeded`). What stays
//! uncoded is deliberate: lock and poisoning faults,
//! protocol and tool-output faults, environment faults such
//! as "no container runtime", and the `reserved builtin slot` placeholders,
//! which are excluded from the count below. The three placeholders written
//! over several lines do not contain that phrase on their `anyhow!(` line, so
//! they are counted.
//!
//! The count is the one `grep` gives:
//!
//!   grep -E "anyhow!\(|bail!\(" src/builtins.rs | grep -vc "reserved builtin slot"
//!
//! It was 109 before the caller-input conversions and 89 after them. The
//! ceiling may only fall: lower `UNCODED_MAX` whenever a site is coded, and do
//! not raise it to admit a new uncoded error; code the error instead.

const UNCODED_MAX: usize = 89;

fn count_uncoded(src: &str) -> usize {
    src.lines()
        .filter(|l| {
            (l.contains("anyhow!(") || l.contains("bail!(")) && !l.contains("reserved builtin slot")
        })
        .count()
}

#[test]
fn uncoded_errors_only_fall() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builtins.rs");
    let src = std::fs::read_to_string(path).unwrap();
    let found = count_uncoded(&src);
    assert!(
        found <= UNCODED_MAX,
        "src/builtins.rs has {found} uncoded anyhow!/bail! errors, ceiling is {UNCODED_MAX}: \
         code the new error with crate::safety::arg_err (or bad_arg / budget_exceeded) \
         instead of anyhow!, which reaches callers as E_UNKNOWN"
    );
}

#[test]
fn counter_finds_a_sample() {
    let sample =
        "    return Err(anyhow!(\"x\"));\n    bail!(\"y\");\n    // reserved builtin slot\n";
    assert_eq!(count_uncoded(sample), 2);
}
