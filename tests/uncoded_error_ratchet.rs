//! Uncoded errors, counted.
//!
//! An `anyhow!(` or `bail!(` that carries no `SafetyError` reaches the agent
//! API as `E_UNKNOWN`: the caller learns that something failed, and nothing
//! about whose fault it was or what to change. A caller-input error is coded
//! with `crate::safety::arg_err` (or `bad_arg`, `budget_exceeded`). What stays
//! uncoded is deliberate: protocol and tool-output faults, environment
//! faults such
//! as "no container runtime", and the `reserved builtin slot` placeholders,
//! which are excluded from the count below. The three placeholders written
//! over several lines do not contain that phrase on their `anyhow!(` line, so
//! they are counted.
//!
//! The count is the one `grep` gives:
//!
//!   grep -E "anyhow!\(|bail!\(" src/builtins.rs | grep -vc "reserved builtin slot"
//!
//! It was 109 before the caller-input conversions, 89 after them, and 87 after
//! the coded-errors pass (the container runtime, provider default, save_json
//! and ls sites), then 86 once apply passed tx::begin's coded error through,
//! then 8 once the lock-poisoning sites moved to
//! `crate::safety::lock_poisoned` (E_BAD_STATE), then 0 once the reserved
//! slots, the clock, JSON serialization, the two A2UI protocol faults and the
//! marketplace lock were coded. The ceiling may only fall:
//! lower `UNCODED_MAX` whenever a site is coded, and do not raise it to admit
//! a new uncoded error; code the error instead.

const UNCODED_MAX: usize = 0;

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
        // The ceiling is 0, so "only falls" is "stays at none".
        found == UNCODED_MAX,
        "src/builtins.rs has {found} uncoded anyhow!/bail! errors, ceiling is {UNCODED_MAX}: \
         code the new error with crate::safety::arg_err (or bad_arg / budget_exceeded) \
         instead of anyhow!, which reaches callers as E_UNKNOWN"
    );
}

/// Per-module ceilings, counted by the same `count_uncoded` over the whole file,
/// `#[cfg(test)]` module included (neither file has an `anyhow!(` in its tests,
/// so nothing is excluded). Each ceiling is the exact count after the coding pass.
///
/// What remains uncoded, and why:
///
/// `src/packages.rs` (5):
/// - `PackageRegistry::resolve` and the `pkg_info` pair in `packages::builtins`
///   are not called from any builtin (`builtins.rs` has its own `bi_pkg_info`).
///   They reach only Rust callers.
/// - `parse_version_req` and `version_satisfies` are likewise called by no
///   builtin.
///
/// `src/workflows.rs` (5), all engine faults that no caller can repair:
/// - two `workflow ... panicked` join errors and the `parallel branch panicked`
///   error, where a spawned task panicked;
/// - `event stream closed`, where the engine's own broadcast channel errored;
/// - `prefixed_error`'s fallback for a non-`SafetyError` inner error, which
///   cannot be coded without knowing what it is.
///
/// `src/mcp.rs` (5): the HTTP MCP server's replies to a remote client
/// (unknown resource, tool or prompt, a missing prompt argument), which go back
/// over the protocol, not to a builtin's caller.
///
/// `src/workflow_builtins.rs`, `src/plugins.rs` and `src/marketplace.rs` have
/// none left.
///
/// The ceilings may only fall. Code a new error instead of raising one.
const MODULE_CEILINGS: &[(&str, usize)] = &[
    ("src/packages.rs", 5),
    ("src/workflows.rs", 5),
    ("src/workflow_builtins.rs", 0),
    ("src/mcp.rs", 5),
    ("src/plugins.rs", 0),
    ("src/marketplace.rs", 0),
];

#[test]
fn module_uncoded_errors_only_fall() {
    for (path, ceiling) in MODULE_CEILINGS {
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        let src = std::fs::read_to_string(&file).unwrap();
        let found = count_uncoded(&src);
        assert!(
            found <= *ceiling,
            "{path} has {found} uncoded anyhow!/bail! errors, ceiling is {ceiling}: \
             code the new error with crate::safety (arg_err, bad_arg, not_found, \
             bad_state, policy_deny, budget_exceeded) instead of anyhow!, which \
             reaches callers as E_UNKNOWN"
        );
    }
}

#[test]
fn counter_finds_a_sample() {
    let sample =
        "    return Err(anyhow!(\"x\"));\n    bail!(\"y\");\n    // reserved builtin slot\n";
    assert_eq!(count_uncoded(sample), 2);
}
