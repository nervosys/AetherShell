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

/// Every Rust source file in the crate, read at test time.
///
/// This used to be `include_str!("../src/builtins.rs")` alone, which made a
/// whole class of effect invisible: a builtin delegating into `os_tools`,
/// `external_tools` or any other module was reading as pure because the lint
/// had never opened the file. `Command::new` appears in 6 other modules,
/// `fs::write` in 10. "No evidence" meant "no evidence in one file".
///
/// Read from disk rather than `include_str!` so a module added tomorrow is
/// covered without anyone remembering to add it here.
fn source_files() -> Vec<String> {
    fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
                if let Ok(text) = std::fs::read_to_string(&p) {
                    out.push(text);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut out,
    );
    assert!(
        out.len() > 20,
        "expected to read the crate's modules, got {}",
        out.len()
    );
    out
}

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
    // Added after asking what the marker list could still be missing rather
    // than assuming it complete. Each is written precisely, because the first
    // attempt was not and the imprecision produced confident nonsense:
    //
    // * a bare `symlink` matched the *field* `allow_symlinks` in
    //   `security::validate_safe_path`, so `ls`, `cat`, `head`, `tail` and
    //   `read_text` were all reported as creating symbolic links;
    // * a bare `OpenOptions` cannot tell a read from a write;
    // * a bare `TcpListener` matches a type name in a signature.
    //
    // A marker must name something that *performs* the effect.
    (".append(true)", "opens a file to append"),
    (".create(true)", "opens a file to create"),
    (".write(true)", "opens a file to write"),
    ("fs::set_permissions", "changes file permissions"),
    ("fs::hard_link", "creates a hard link"),
    ("symlink_file(", "creates a file symlink"),
    ("symlink_dir(", "creates a directory symlink"),
    ("fs::symlink(", "creates a symbolic link"),
    ("TcpListener::bind", "binds a listening socket"),
    ("UdpSocket::bind", "binds a datagram socket"),
    // `Command::new` catches *starting* a process; nothing caught *stopping*
    // one. `Effect::Process` resolves to Approve in agent mode, so a builtin
    // that signals a process while classified `Pure` is allowed outright --
    // the same shape of hole the original 28 sat in, one verb over.
    //
    // Written as `.kill(` and `libc::kill(` rather than a bare `kill`, which
    // would match `bi_proc_kill`, `pkill`, and the word in any identifier.
    (".kill(", "signals or terminates a process"),
    ("libc::kill(", "signals a process by pid"),
    // `.create(true)`/`.write(true)` are listed; truncation was not, and it
    // destroys content rather than adding to it.
    (".truncate(true)", "truncates a file to zero length"),
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
/// effect.
///
/// Depth was then **measured** rather than argued: 2, 3, 4 and 5 all report the
/// same zero outstanding violations, so nothing is hiding three or more hops
/// down in this codebase. It is set to 4 because that costs a fraction of a
/// second and removes the question; if a future refactor introduces deeper
/// indirection this will find it without anyone having to remember to look.
const FOLLOW_DEPTH: usize = 4;

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
fn strip_comments(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();

        // Line comment.
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }

        // Block comment (nesting allowed, as in Rust).
        if c == '/' && next == Some('*') {
            let mut depth = 1usize;
            out.push(' ');
            out.push(' ');
            i += 2;
            while i < chars.len() && depth > 0 {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                } else {
                    out.push(if chars[i] == '\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
            continue;
        }

        // Character literal: 'x' or '\n'.
        //
        // Handled explicitly because a literal quote — `'"'`, of which
        // builtins.rs contains 20 — otherwise reads as the *start of a string*
        // and blanks all the real code that follows it. That silently hid
        // genuine effects: the violation count fell from 6 to 3 and the drop
        // looked like progress. The canary tests below exist so that a scanner
        // change which blinds this lint fails instead of flattering it.
        if c == '\'' {
            let close = if next == Some('\\') {
                chars
                    .iter()
                    .skip(i + 2)
                    .position(|&x| x == '\'')
                    .map(|p| i + 2 + p)
            } else if chars.get(i + 2) == Some(&'\'') {
                Some(i + 2)
            } else {
                None
            };
            if let Some(close) = close {
                for _ in i..=close {
                    out.push(' ');
                }
                i = close + 1;
                continue;
            }
            // Otherwise a lifetime (`'a`), which is ordinary code.
            out.push(c);
            i += 1;
            continue;
        }

        // String literal: keep the quotes, blank the contents. A literal is
        // data, not code — `bi_help`'s help text contains `| join("-")`, which
        // the call scanner read as a call to a function named `join`, and since
        // `tui::distributed::join` binds a UDP socket, `help` was reported as
        // opening a datagram socket.
        if c == '"' {
            out.push('"');
            i += 1;
            let mut esc = false;
            while i < chars.len() {
                let ch = chars[i];
                if ch == '\\' && !esc {
                    esc = true;
                    out.push(' ');
                    i += 1;
                    continue;
                }
                if ch == '"' && !esc {
                    out.push('"');
                    i += 1;
                    break;
                }
                esc = false;
                out.push(if ch == '\n' { '\n' } else { ' ' });
                i += 1;
            }
            continue;
        }

        out.push(c);
        i += 1;
    }
    out
}

fn all_fn_bodies() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for file in source_files() {
        collect_fn_bodies(&strip_comments(&file), &mut out);
    }
    out
}

/// Extract every `fn <name>` body from one comment-stripped file.
fn collect_fn_bodies(source: &str, out: &mut Vec<(String, String)>) {
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
}

/// Function name → body, **excluding any name defined more than once**.
///
/// Reading the whole crate instead of one file multiplied the name collisions:
/// `join`, `new`, `run` and friends are defined in many modules. Resolving a
/// call by bare name then picks an arbitrary one, and the lint reported `help`
/// as opening a datagram socket because some unrelated `join` does. An
/// ambiguous resolution is not evidence, so ambiguous names are dropped rather
/// than guessed — a false negative the direct-evidence pass can still catch,
/// instead of a false positive that would get "fixed" by misclassifying a pure
/// builtin.
fn bodies_by_name() -> HashMap<String, String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let bodies = all_fn_bodies();
    for (name, _) in &bodies {
        *counts.entry(name.clone()).or_insert(0) += 1;
    }
    bodies
        .into_iter()
        .filter(|(name, _)| counts.get(name) == Some(&1))
        .collect()
}

/// How many function names are too ambiguous to resolve. Reported so the size
/// of that blind spot is visible rather than implied.
fn ambiguous_name_count() -> usize {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (name, _) in all_fn_bodies() {
        *counts.entry(name).or_insert(0) += 1;
    }
    counts.values().filter(|c| **c > 1).count()
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
    // Assert the property directly. Looking `json_to_value` up by name no
    // longer works — it is defined in five modules, so the ambiguity filter
    // (correctly) drops it — and a test that depends on a lookup succeeding
    // would fail for a reason unrelated to what it is checking.
    let src = "\
        // fn json_to_value(json: serde_json::Value) -> Value;\n\
        fn deletes_things() { std::fs::remove_file(p); }\n";
    let stripped = strip_comments(src);
    assert!(
        !stripped.contains("fn json_to_value"),
        "a commented-out signature must not survive stripping: {stripped}"
    );
    let mut bodies = Vec::new();
    collect_fn_bodies(&stripped, &mut bodies);
    assert_eq!(
        bodies.len(),
        1,
        "only the real function should be parsed, got {bodies:?}"
    );
    assert_eq!(bodies[0].0, "deletes_things");

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
        // Track the last two *non-whitespace* characters. Using the immediately
        // preceding character is not enough: a multi-line method chain puts a
        // newline between the dot and the name —
        //
        //     some_vec
        //         .join(", ")
        //
        // — so `join` read as a free call, and the lint reported `help` as
        // binding a datagram socket because an unrelated `join` elsewhere does.
        if !c.is_whitespace() {
            prev_prev = prev;
            prev = Some(c);
        }
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
    // The remaining blind spot, stated as a number rather than as a caveat: a
    // call whose name is defined in more than one module cannot be resolved
    // from a text scan, so delegation through it is not followed.
    println!(
        "unresolvable call names (defined more than once, so not followed): {}",
        ambiguous_name_count()
    );
    assert!(n <= baseline().len(), "the debt must never grow");
}

// ── Canaries ────────────────────────────────────────────────────────────────
//
// A lint that goes blind reports zero violations, which is indistinguishable
// from success. It happened here: adding string-literal blanking without
// handling character literals meant a literal `'"'` opened a phantom string and
// blanked the code after it. The count fell from 6 to 3 and looked like
// progress; `platform_machine_id` had simply become invisible.
//
// These assert that known-acting code is still *seen*. They fail when the
// scanner loses sight, rather than quietly agreeing with it.

#[test]
fn the_scanner_still_sees_a_process_being_constructed() {
    let all = bodies_by_name();
    let body = all
        .get("bi_platform_machine_id")
        .expect("bi_platform_machine_id should be parsed");
    assert!(
        direct_evidence(body).is_some(),
        "this builtin constructs a process; failing to see it means the scanner \
         is blind, not that the code changed"
    );
}

#[test]
fn the_scanner_still_sees_a_file_being_appended_to() {
    let all = bodies_by_name();
    let body = all
        .get("bi_git_ignore")
        .expect("bi_git_ignore should be parsed");
    assert!(
        direct_evidence(body).is_some(),
        "git_ignore opens a file with .append(true)"
    );
}

#[test]
fn a_char_literal_containing_a_quote_does_not_blind_the_scanner() {
    // The specific defect, pinned. Everything after `'"'` must still be read.
    let src = r#"
        fn probe() {
            let quote = '"';
            let _ = quote;
            std::process::Command::new("ls");
        }
    "#;
    let stripped = strip_comments(src);
    assert!(
        stripped.contains("Command::new"),
        "code after a quote character literal was blanked: {stripped}"
    );
}

#[test]
fn a_string_literal_is_not_read_as_code() {
    // The opposite direction, equally pinned.
    let src = r#"
        fn helptext() -> &'static str {
            "usage: xs | join(\"-\")"
        }
    "#;
    let stripped = strip_comments(src);
    assert!(
        !stripped.contains("join("),
        "documentation inside a string was read as a call: {stripped}"
    );
}
