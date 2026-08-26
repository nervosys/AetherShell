//! A caller value in a positional slot must not be readable as an option.
//!
//! `safety::reject_option_like` has existed since the `tar
//! --use-compress-program` finding. What kept going wrong was not the helper but
//! its coverage: it was called at three of eight `sqlite3` spawn sites, then at
//! none of the fourteen `curl` sites, then at none of the seventeen `git` sites.
//! Each time it was found by reading, and each time the reading had to be
//! repeated because nothing recorded the answer.
//!
//! This records it. The rule is scoped to the tools where a leading `-` buys
//! **code execution**, because that is a claim that can be checked rather than
//! asserted:
//!
//! | tool            | the option                                    |
//! |-----------------|-----------------------------------------------|
//! | `git`           | `--upload-pack`, `--exec-path`, `-c <cfg>`    |
//! | `ssh`/`scp`/`sftp` | `-o ProxyCommand=…`, `-S <program>`        |
//! | `tar`           | `--use-compress-program=…`                    |
//! | `zip`/`unzip`   | `-TT <cmd>`                                   |
//! | `curl`          | `-K <file>` — a config that can set `output`  |
//! | `wget`          | `--use-askpass=…`                             |
//! | `openssl`       | `-engine <so>`                                |
//! | `find`          | `-exec … ;`                                   |
//!
//! Tools outside that set are out of scope on purpose. A leading `-` reaching
//! `ps` or `uname` is a bad argument, not a foothold, and a rule that flagged
//! every one of the 372 sites carrying a value would be ignored rather than
//! obeyed. Narrow and enforced beats broad and waived.
//!
//! The allowlist below is the other half, and its entries are *decisions*: a slot
//! that is an option **by contract** (`git reset --hard`), or one where the value
//! is consumed as another option's argument and a leading `-` is legitimate data
//! (a commit message, a password, a `find -name` pattern). Guarding those would
//! break correct calls with no workaround, which is the failure mode
//! `reject_option_like`'s own doc warns about.

use std::collections::BTreeSet;

/// Tools where a caller-controlled leading `-` reaches an option that can run a
/// program. This list may only grow.
const RISKY_TOOLS: &[&str] = &[
    "git", "ssh", "scp", "sftp", "rsync", "tar", "zip", "unzip", "find", "wget", "curl", "openssl",
    "sqlite3",
];

/// Spawn sites that pass a caller value to a risky tool without
/// `reject_option_like`, each because guarding it would be wrong.
///
/// This list may only shrink, and every entry names a slot, not a builtin, so
/// "it was already on the list" cannot cover a second unguarded argument added
/// later.
const ALLOWED: &[(&str, &str)] = &[
    (
        "bi_git_status",
        "the value is `.current_dir()`, not an argv slot — nothing parses it as an option",
    ),
    (
        "bi_git_diff",
        "the value is `.current_dir()`, not an argv slot",
    ),
    (
        "bi_git_diff_staged",
        "the value is `.current_dir()`, not an argv slot",
    ),
    (
        "bi_git_commit",
        "the message is the argument of `-m`, which consumes it; messages legitimately start with '-'",
    ),
    (
        "bi_git_reset",
        "the mode IS an option by contract — it defaults to `--mixed` and callers pass `--hard`",
    ),
    (
        "bi_session_checkpoint",
        "the name is the argument of `git stash push -m`, which consumes it",
    ),
    (
        "bi_search_files",
        "the pattern is the argument of `find -name`, which consumes it; glob patterns may start with '-'",
    ),
    (
        "bi_search_by_size",
        "the size IS an option-shaped value by contract — it defaults to `+1M` and `-1M` is the documented spelling for 'smaller than'",
    ),
    (
        "bi_crypto_password_hash",
        "a password may legitimately start with '-', and `openssl passwd` has no option that runs a program",
    ),
    (
        "bi_crypto_hmac",
        "an HMAC key is opaque bytes and may start with '-'; it is the argument of `-hmac`, which consumes it",
    ),
    (
        "bi_git_clean",
        "`flag` is a local bound to one of two source literals (\"-n\"/\"-f\") from a bool argument, not caller text — the one place 'bare identifier' over-approximates",
    ),
    (
        "bi_curl_exec",
        "the builtin's purpose is to pass curl arguments through; it is Exec-classified and gated as such",
    ),
];

fn source() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builtins.rs");
    std::fs::read_to_string(path).expect("src/builtins.rs is readable")
}

/// Every spawn of a risky tool, as (tool, enclosing fn, body-so-far, arg window).
fn risky_spawn_sites(src: &str) -> Vec<(String, String, String, String)> {
    let lines: Vec<&str> = src.lines().collect();
    let mut fn_starts: Vec<(usize, String)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if l.starts_with("fn ") || l.starts_with("pub fn ") {
            let name = l
                .trim_start_matches("pub ")
                .trim_start_matches("fn ")
                .split('(')
                .next()
                .unwrap_or("")
                .to_string();
            fn_starts.push((i, name));
        }
    }
    let enclosing = |i: usize| -> (usize, String) {
        let mut best = (0usize, "<top>".to_string());
        for (j, n) in &fn_starts {
            if *j <= i {
                best = (*j, n.clone());
            } else {
                break;
            }
        }
        best
    };

    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with("//") || t.starts_with('*') {
            continue;
        }
        let Some(tool) = RISKY_TOOLS
            .iter()
            .find(|p| t.contains(&format!("Command::new(\"{p}\")")))
        else {
            continue;
        };
        let (start, name) = enclosing(i);
        let window = lines[i..(i + 10).min(lines.len())].join("\n");
        out.push((tool.to_string(), name, lines[start..=i].join("\n"), window));
    }
    out
}

/// Does this spawn hand the tool an argument whose **first character** the
/// caller controls?
///
/// That is the precise question, and getting it wrong in the loose direction was
/// this file's first draft: it looked for `&` anywhere in a ten-line window and
/// reported eighteen sites, seventeen of them literals such as `["stash",
/// "list"]` followed two lines later by `&output.stdout`. A rule that cries wolf
/// on `git tag -l` gets an allowlist entry per false positive, and then it is a
/// list of excuses rather than a control.
///
/// So: only a *bare identifier* argument counts — `&host`, `archive`,
/// `&branch.clone()`. Everything else cannot begin with a caller's `-`:
///
/// * a string literal is written in the source;
/// * `&format!("-{}", n)` produces a constant prefix — the `-` is the tool's own
///   syntax, written here, and `n` cannot reach the front of the string;
/// * `&bits.to_string()` is a number.
///
/// This is deliberately a lower bound: an expression like
/// `&path.display().to_string()` would slip through. It catches the shape that
/// actually occurred every time — a caller's string handed straight over.
fn passes_a_value(window: &str) -> bool {
    // Only the spawn statement itself, up to its terminating `;`. The window
    // extends further so the enclosing-fn walk has context; the argument list
    // does not.
    let stmt = window.split_once(";\n").map(|(a, _)| a).unwrap_or(window);
    let stmt = stmt.split_once(";\r\n").map(|(a, _)| a).unwrap_or(stmt);

    let mut args: Vec<String> = Vec::new();
    let mut rest = stmt;
    while let Some(at) = rest.find(".arg") {
        let after = &rest[at..];
        let Some(open) = after.find('(') else { break };
        let mut depth = 0i32;
        let mut end = None;
        for (k, c) in after[open..].char_indices() {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + k);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        let inner = after[open + 1..end].trim().trim_start_matches('[').trim();
        for part in inner.split(',') {
            let p = part.trim();
            if !p.is_empty() {
                args.push(p.to_string());
            }
        }
        rest = &after[end..];
    }

    args.iter().any(|a| {
        let a = a.trim().trim_matches(|c| c == '[' || c == ']').trim();
        let a = a.trim_start_matches('&').trim();
        let a = a.strip_suffix(".clone()").unwrap_or(a);
        !a.is_empty()
            && a.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
            && a.chars().all(|c| c.is_alphanumeric() || c == '_')
    })
}

#[test]
fn every_risky_spawn_guards_its_positional_values() {
    let src = source();
    let sites = risky_spawn_sites(&src);
    assert!(
        sites.len() >= 50,
        "only {} risky spawn sites parsed; the scanner has drifted and this test is \
         checking almost nothing",
        sites.len()
    );

    let mut offenders: BTreeSet<String> = BTreeSet::new();
    let mut checked = 0usize;
    for (tool, name, body, window) in &sites {
        if !passes_a_value(window) {
            continue;
        }
        checked += 1;
        // `guard_network` calls `reject_option_like` on the URL, so a site that
        // goes through it is covered by the one door rather than at the site.
        if body.contains("reject_option_like") || body.contains("guard_network") {
            continue;
        }
        if ALLOWED.iter().any(|(n, _)| n == name) {
            continue;
        }
        offenders.insert(format!("  {tool:8} {name}"));
    }

    assert!(
        checked >= 25,
        "only {checked} risky spawn sites carry a value; the value test has drifted"
    );
    assert!(
        offenders.is_empty(),
        "{} spawn site(s) hand a caller value to a tool that can be made to run a \
         program through an option, without `safety::reject_option_like`.\n\n\
         A leading `-` in a path, host or ref is parsed as an option: \
         `ssh -oProxyCommand=…`, `git --upload-pack=…`, `tar \
         --use-compress-program=…`, `curl -K<config>`.\n\n\
         Guard it, or — if the slot is an option by contract, or the value is \
         consumed as another option's argument — add the function to ALLOWED in \
         this file *with the reason*.\n\n{}",
        offenders.len(),
        offenders.into_iter().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn the_scanner_finds_the_tools_it_claims_to() {
    // A check on the checker. If the walk breaks, the rule above passes by
    // finding nothing.
    let sites = risky_spawn_sites(&source());
    let tools: BTreeSet<&str> = sites.iter().map(|(t, _, _, _)| t.as_str()).collect();
    for expected in [
        "git", "ssh", "scp", "sftp", "tar", "unzip", "curl", "sqlite3",
    ] {
        assert!(
            tools.contains(expected),
            "{expected} spawn sites should have been found, got {tools:?}"
        );
    }
    let names: BTreeSet<&str> = sites.iter().map(|(_, n, _, _)| n.as_str()).collect();
    for expected in ["bi_git_checkout", "bi_ssh_exec", "bi_tar_extract"] {
        assert!(
            names.contains(expected),
            "{expected} should be among the parsed sites"
        );
    }
}

#[test]
fn the_value_test_tells_a_literal_from_a_caller_value() {
    assert!(passes_a_value(r#".args(["checkout", &branch])"#));
    assert!(passes_a_value(r#"cmd.arg(&host).arg(&command);"#));
    assert!(!passes_a_value(r#".args(["stash", "pop"])"#));
    assert!(!passes_a_value(r#".args(["fetch"]).output()?;"#));
    // The seventeen false positives the loose version produced, in one line
    // each. A constant prefix means the caller cannot reach the front of the
    // string, so the leading `-` is the tool's syntax rather than their input.
    assert!(!passes_a_value(
        r#".args(["log", &format!("-{}", count), "--oneline"])"#
    ));
    assert!(!passes_a_value(
        r#".args([".", "-name", &format!("*.{}", ext)])"#
    ));
    assert!(!passes_a_value(r#".args(["genrsa", &bits.to_string()])"#));
    // And the shape that made it cry wolf: a literal argv, then a borrow of
    // the *result* two lines down.
    assert!(!passes_a_value(
        "        .args([\"tag\", \"-l\"])
        .output()?;
    let t = String::from_utf8_lossy(&output.stdout);"
    ));
}

#[test]
fn the_allowlist_has_a_reason_for_every_entry_and_no_duplicates() {
    let mut seen = BTreeSet::new();
    for (name, reason) in ALLOWED {
        assert!(
            !reason.trim().is_empty(),
            "{name} is allowed without a reason; the reason is the point"
        );
        assert!(
            reason.len() > 25,
            "{name}'s reason is too short to be one: {reason:?}"
        );
        assert!(seen.insert(*name), "{name} is listed twice");
    }
    assert!(
        ALLOWED.len() <= 12,
        "the option-injection allowlist has grown to {}; it may only shrink",
        ALLOWED.len()
    );
}
