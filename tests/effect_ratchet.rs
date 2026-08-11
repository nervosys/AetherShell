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
use std::collections::{HashMap, HashSet};

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

/// The violations outstanding, and **now empty**.
///
/// This lint was written (2026-08-11) against 306 builtins — overwhelmingly wrappers
/// around external developer tooling — that construct an OS process while `effect_of`
/// returned `Pure`. All 306 were classified the same day from the argv their bodies
/// actually build, so the baseline holds nothing.
///
/// The file stays because it is a *ratchet*, not a snapshot: it may only shrink, and
/// a newly added builtin that acts while classified `Pure` fails the build. That is
/// the mechanism that was missing when the original 28 name-detected
/// misclassifications accumulated unnoticed.
const BASELINE: &str = include_str!("effect_ratchet_baseline.txt");

fn baseline() -> std::collections::HashSet<&'static str> {
    BASELINE
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// How far calls are followed when looking for evidence.
///
/// One level was not enough: a builtin that calls `cloud_run_cmd_json`, which
/// calls `cloud_run_cmd`, which constructs the process, is two hops from its own
/// effect. Depth 2 covers the helper-of-a-helper pattern this file actually
/// uses; deeper costs analysis time for diminishing returns, and the limit is
/// stated here rather than left implicit so the remaining blind spot is known.
const FOLLOW_DEPTH: usize = 2;

/// Extract every `fn <name>` body from the source by brace matching — not just
/// `bi_*`.
///
/// The helpers matter as much as the builtins. Reading only `bi_*` bodies is why
/// 209 builtins that spawn processes through `cloud_run_cmd`, `sec_run_cmd` and
/// friends were invisible to this lint while it reported a clean baseline.
/// A copy of the source with comments blanked to spaces, preserving every byte
/// offset so slices taken against it line up with the original.
///
/// Without this the scan matches `fn ` inside a comment. A commented-out
/// signature (`// fn json_to_value(...)`) then binds to the *next* real
/// function's body — in this file, `bi_rm`, which deletes files. The map is
/// keyed by name, so that phantom silently replaced the genuine
/// `json_to_value`, and the lint reported `json_parse` as a builtin that
/// deletes files. Comments are not code and must not be read as code.
fn source_without_comments() -> String {
    let mut out = String::with_capacity(SOURCE.len());
    let mut chars = SOURCE.char_indices().peekable();
    let (mut in_line, mut in_block, mut in_str, mut esc) = (false, false, false, false);
    while let Some((_, c)) = chars.next() {
        let next = chars.peek().map(|(_, c)| *c);
        if in_line {
            if c == '\n' {
                in_line = false;
                out.push(c);
            } else {
                out.push(' ');
            }
        } else if in_block {
            if c == '*' && next == Some('/') {
                in_block = false;
                out.push(' ');
                out.push(' ');
                chars.next();
            } else {
                out.push(if c == '\n' { '\n' } else { ' ' });
            }
        } else if in_str {
            out.push(c);
            if c == '\\' && !esc {
                esc = true;
            } else {
                if c == '"' && !esc {
                    in_str = false;
                }
                esc = false;
            }
        } else if c == '/' && next == Some('/') {
            in_line = true;
            out.push(' ');
        } else if c == '/' && next == Some('*') {
            in_block = true;
            out.push(' ');
        } else {
            if c == '"' {
                in_str = true;
            }
            out.push(c);
        }
    }
    out
}

fn all_fn_bodies() -> Vec<(String, String)> {
    let source = source_without_comments();
    let source: &str = &source;
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut search = 0usize;
    while let Some(rel) = source[search..].find("fn ") {
        let start = search + rel;
        search = start + 3;
        let rest = &source[start + 3..];
        let name_end = match rest.find(|c: char| !(c.is_alphanumeric() || c == '_')) {
            Some(i) => i,
            None => continue,
        };
        let fn_name = rest[..name_end].to_string();
        if fn_name.is_empty() {
            continue;
        }
        // Body starts at the first `{` after the signature.
        let brace = match source[start..].find('{') {
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
            out.push((fn_name, source[brace..=i.min(bytes.len() - 1)].to_string()));
        }
    }
    out
}

fn bodies_by_name() -> HashMap<String, String> {
    all_fn_bodies().into_iter().collect()
}

#[test]
fn the_parser_finds_a_plausible_number_of_builtin_bodies() {
    // A guard on the lint itself: if the extraction silently broke, every other
    // assertion here would pass vacuously.
    let all = bodies_by_name();
    let builtins = all.keys().filter(|n| n.starts_with("bi_")).count();
    assert!(
        builtins > 800,
        "expected to parse most builtin bodies, got {builtins}"
    );
    // And the helpers, which are the point of following delegation at all.
    assert!(
        all.len() > builtins,
        "expected helper functions to be parsed too, got {} total vs {builtins} builtins",
        all.len()
    );
    let body = all
        .get("bi_aecon_decode")
        .expect("a known builtin should be parsed");
    assert!(
        body.starts_with('{') && body.ends_with('}'),
        "brace matched"
    );
}

#[test]
fn the_lint_does_not_read_comments_as_code() {
    // Regression, and a check on the checker. `// fn json_to_value(...)` appears
    // as a comment at one point in builtins.rs; a scan that matched it bound the
    // name to the *next* real function's body — `bi_rm`, which deletes files —
    // and the map silently replaced the genuine `json_to_value` with it. The
    // lint then reported `json_parse` as a builtin that deletes files, which
    // would have been "fixed" by misclassifying a pure function.
    let all = bodies_by_name();
    let body = all
        .get("json_to_value")
        .expect("json_to_value should be parsed");
    assert!(
        direct_evidence(body).is_none(),
        "json_to_value is pure data conversion; evidence here means a comment \
         was parsed as code: {}",
        &body[..body.len().min(120)]
    );

    let flagged: Vec<String> = current_violations()
        .into_iter()
        .map(|(n, _, _)| n)
        .filter(|n| n == "json_parse" || n == "jq_query")
        .collect();
    assert!(
        flagged.is_empty(),
        "pure JSON builtins must not be reported as acting: {flagged:?}"
    );
}

/// Evidence that this body acts, directly.
fn direct_evidence(body: &str) -> Option<(&'static str, &'static str)> {
    EVIDENCE
        .iter()
        .find(|(m, _)| body.contains(m))
        .map(|(m, why)| (*m, *why))
}

/// Evidence that this body acts *through a helper*, following calls up to
/// `depth` levels.
///
/// A builtin that hands its side effect to `cloud_run_cmd` is exactly as
/// dangerous as one that spawns the process itself; only the lint could tell
/// the difference, which made the difference a blind spot rather than a fact.
fn delegated_evidence(
    body: &str,
    all: &HashMap<String, String>,
    depth: usize,
    seen: &mut HashSet<String>,
) -> Option<(&'static str, String)> {
    if depth == 0 {
        return None;
    }
    for callee in called_names(body) {
        if !seen.insert(callee.clone()) {
            continue;
        }
        let Some(cb) = all.get(&callee) else { continue };
        if let Some((marker, why)) = direct_evidence(cb) {
            return Some((marker, format!("{callee}() {why}")));
        }
        if let Some((marker, chain)) = delegated_evidence(cb, all, depth - 1, seen) {
            return Some((marker, format!("{callee}() → {chain}")));
        }
    }
    None
}

/// Calls to **free functions defined in this file**.
///
/// Two mistakes are deliberately excluded, both found by checking a result
/// instead of believing it:
///
/// * An identifier preceded by `.` or `::` is a method or an associated
///   function, not a free call. Without this, `BTreeMap::new()` reads as a call
///   to a free function named `new` — and since some `fn new` in this file does
///   construct a process, *every* function that builds a map appeared to
///   delegate a side effect. `json_parse` was reported as spawning a process on
///   exactly that basis.
/// * Byte indexing. The file contains `…` and `→` in comments, and slicing a
///   `&str` mid-character panics, so this walks `char_indices`.
fn called_names(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut ident_start: Option<usize> = None;
    let mut prev: Option<char> = None;
    let mut prev_prev: Option<char> = None;
    for (i, c) in body.char_indices() {
        let is_ident = c.is_alphanumeric() || c == '_';
        match (ident_start, is_ident) {
            (None, true) if c.is_alphabetic() || c == '_' => {
                // Qualified (`Type::f`, `x.f`) — not a free call in this file.
                let qualified = prev == Some('.') || (prev == Some(':') && prev_prev == Some(':'));
                if !qualified {
                    ident_start = Some(i);
                }
            }
            (Some(start), false) => {
                if c == '(' {
                    out.push(body[start..i].to_string());
                }
                ident_start = None;
            }
            _ => {}
        }
        prev_prev = prev;
        prev = Some(c);
    }
    out
}

/// Every registered builtin that acts while classified `Pure`, with the
/// evidence — whether it acts itself or delegates.
fn current_violations() -> Vec<(String, &'static str, String)> {
    let all = bodies_by_name();
    let mut out = Vec::new();
    for (fn_name, body) in all.iter() {
        let Some(name) = fn_name.strip_prefix("bi_") else {
            continue;
        };
        // Only names the dispatcher actually exposes; helpers named bi_* that are
        // not registered cannot be reached by an agent.
        if name.is_empty() || !BUILTIN_LOOKUP.contains_key(name) {
            continue;
        }
        if effect_of(name) != Effect::Pure {
            continue;
        }
        if let Some((marker, why)) = direct_evidence(body) {
            out.push((name.to_string(), marker, why.to_string()));
            continue;
        }
        let mut seen = HashSet::new();
        // Do not follow into itself.
        seen.insert(fn_name.clone());
        if let Some((marker, chain)) = delegated_evidence(body, &all, FOLLOW_DEPTH, &mut seen) {
            out.push((name.to_string(), marker, format!("delegates: {chain}")));
        }
    }
    out.sort();
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
    println!("effect ratchet: {n} builtin(s) act while classified Pure (baseline 0)");
    assert!(n <= baseline().len(), "the debt must never grow");
}
