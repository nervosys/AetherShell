//! Effect ratchet — evidence from the code, not from the name.
//!
//! `tests/effect_coverage.rs` audits builtin *names* that advertise a side effect.
//! That found 28 misclassified builtins, but a name-based check is exactly the
//! reasoning that produced the misclassifications in the first place: four builtins
//! were tagged as executing purely because they were called `*_exec`, when their
//! bodies execute nothing.
//!
//! This lint reads the other direction, from the body. If a builtin's function
//! literally constructs a process, writes a file, or opens a socket, it cannot be
//! `Effect::Pure` — whatever it is called. The audit was a snapshot; this is the
//! ratchet that stops the next builtin from silently defaulting to `Pure`, which is
//! how all 28 got there.
//!
//! It is deliberately a *lower bound*: a builtin that delegates its side effect to a
//! helper is not detected here. Missing one of those is a false negative the name
//! lint may still catch; a false positive would be a broken build, so the evidence
//! markers stay narrow and the exemptions carry reasons.

use aethershell::builtins::BUILTIN_LOOKUP;
use aethershell::safety::{effect_of, Effect};

const SOURCE: &str = include_str!("../src/builtins.rs");

/// Syntax that *performs* an effect rather than describing one. Each marker is
/// something an implementation must actually call — no nouns, no type names that
/// might appear in a doc comment or a match arm.
const EVIDENCE: &[(&str, &str)] = &[
    ("Command::new", "constructs an OS process"),
    ("fs::write", "writes a file"),
    ("fs::remove_file", "deletes a file"),
    ("fs::remove_dir", "deletes a directory"),
    ("fs::create_dir", "creates a directory"),
    ("fs::copy", "copies a file"),
    ("fs::rename", "renames a path"),
    ("File::create", "creates a file"),
    ("TcpStream::connect", "opens a socket"),
    ("reqwest::", "makes an HTTP request"),
];

/// The violations that already existed when this lint was written (2026-08-11).
///
/// 306 builtins — overwhelmingly wrappers around external developer tooling
/// (`pytest_run`, `eslint_check`, `go_build`, `skopeo_copy`) — construct an OS
/// process while `effect_of` returns `Pure`. Reclassifying all of them is a
/// behavioural change with real UX consequences in agent mode, and belongs to the
/// maintainer, not to this lint. See the same open question about the permissive
/// `Pure` fall-through in docs/AGENTIC_FIRST_DESIGN.md §12.
///
/// So this file is a *ratchet*, not a gate: the baseline may only shrink. A newly
/// added builtin that acts while classified `Pure` fails the build, which is the
/// mechanism that was missing when the 28 name-detected misclassifications
/// accumulated. Entries are removed as they are classified; none may be added.
const BASELINE: &str = include_str!("effect_ratchet_baseline.txt");

fn baseline() -> std::collections::HashSet<&'static str> {
    BASELINE
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

/// Extract each `fn bi_<name>` body from the source by brace matching, ignoring
/// braces inside string literals and comments well enough for this purpose.
fn builtin_bodies() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = SOURCE.as_bytes();
    let mut search = 0usize;
    while let Some(rel) = SOURCE[search..].find("fn bi_") {
        let start = search + rel;
        search = start + 6;
        let rest = &SOURCE[start + 3..];
        let name_end = match rest.find(|c: char| !(c.is_alphanumeric() || c == '_')) {
            Some(i) => i,
            None => continue,
        };
        let fn_name = &rest[..name_end];
        let builtin = match fn_name.strip_prefix("bi_") {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => continue,
        };
        // Body starts at the first `{` after the signature.
        let brace = match SOURCE[start..].find('{') {
            Some(i) => start + i,
            None => continue,
        };
        let mut depth = 0i32;
        let mut i = brace;
        let mut in_str = false;
        let mut prev_escape = false;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if in_str {
                if c == '\\' && !prev_escape {
                    prev_escape = true;
                } else {
                    if c == '"' && !prev_escape {
                        in_str = false;
                    }
                    prev_escape = false;
                }
            } else if c == '"' {
                in_str = true;
            } else if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            i += 1;
        }
        if depth == 0 && i > brace {
            out.push((builtin, SOURCE[brace..=i.min(bytes.len() - 1)].to_string()));
        }
    }
    out
}

#[test]
fn the_parser_finds_a_plausible_number_of_builtin_bodies() {
    // A guard on the lint itself: if the extraction silently broke, every other
    // assertion here would pass vacuously.
    let bodies = builtin_bodies();
    assert!(
        bodies.len() > 800,
        "expected to parse most builtin bodies, got {}",
        bodies.len()
    );
    let (_, body) = bodies
        .iter()
        .find(|(n, _)| n == "aecon_decode")
        .expect("a known builtin should be parsed");
    assert!(
        body.starts_with('{') && body.ends_with('}'),
        "brace matched"
    );
}

/// Every registered builtin that acts while classified `Pure`, with the evidence.
fn current_violations() -> Vec<(String, &'static str, &'static str)> {
    let mut out = Vec::new();
    for (name, body) in builtin_bodies() {
        // Only names the dispatcher actually exposes; helpers named bi_* that are
        // not registered cannot be reached by an agent.
        if !BUILTIN_LOOKUP.contains_key(name.as_str()) {
            continue;
        }
        if effect_of(&name) != Effect::Pure {
            continue;
        }
        if let Some((marker, why)) = EVIDENCE.iter().find(|(m, _)| body.contains(m)) {
            out.push((name, *marker, *why));
        }
    }
    out
}

#[test]
fn no_new_builtin_acts_while_classified_pure() {
    let base = baseline();
    let fresh: Vec<String> = current_violations()
        .into_iter()
        .filter(|(name, _, _)| !base.contains(name.as_str()))
        .map(|(name, marker, why)| format!("  {name}: {why} (`{marker}`) but effect_of = Pure"))
        .collect();

    assert!(
        fresh.is_empty(),
        "{} builtin(s) added since the baseline act but are classified Pure.\n\
         Classify them in `safety::effect_of` — do not add them to \
         tests/effect_ratchet_baseline.txt, which may only shrink:\n{}",
        fresh.len(),
        fresh.join("\n")
    );
}

#[test]
fn the_baseline_only_shrinks() {
    // A name that no longer violates must be deleted from the baseline, so the file
    // always states the true remaining debt rather than drifting into fiction.
    let current: std::collections::HashSet<String> = current_violations()
        .into_iter()
        .map(|(n, _, _)| n)
        .collect();
    let fixed: Vec<&str> = baseline()
        .into_iter()
        .filter(|n| !current.contains(*n))
        .collect();
    assert!(
        fixed.is_empty(),
        "{} baseline entr(ies) no longer violate — delete them from \
         tests/effect_ratchet_baseline.txt:\n  {}",
        fixed.len(),
        fixed.join("\n  ")
    );
}

#[test]
fn report_the_outstanding_debt() {
    // Not a gate — a number that should be visible in the log, so the debt cannot
    // quietly become permanent.
    let n = current_violations().len();
    println!("effect ratchet: {n} builtin(s) act while classified Pure (baseline 306)");
    assert!(n <= baseline().len(), "the debt must never grow");
}
