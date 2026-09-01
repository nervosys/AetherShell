//! Every environment variable a document tells you to set must be one the
//! shell actually reads.
//!
//! `docs/security/SECURITY_COMPLIANCE.md` carried a "Production Security
//! Settings" block recommending seven variables — `AETHER_SECURITY_LEVEL`,
//! `AETHER_AUDIT_LOGGING`, `AETHER_RATE_LIMIT_REQUESTS`,
//! `AETHER_RATE_LIMIT_WINDOW`, `AETHER_COMMAND_WHITELIST`,
//! `AETHER_MAX_PROMPT_LENGTH`, `AETHER_SESSION_TIMEOUT`. The source read none
//! of them. An operator following that document would export all seven,
//! believe the shell was hardened, and have changed nothing.
//!
//! That is worse than an undocumented option. A missing document sends someone
//! to the source; a false one stops them looking. In a file headed "Security
//! Compliance", under a list of ticked boxes, it reads as an assurance.
//!
//! Only `export`/`unset` lines inside fenced shell blocks are checked — those
//! are the lines a reader copies. Prose may discuss a name freely, which is
//! what lets the document above describe the variables it used to recommend
//! without re-introducing the claim.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn markdown_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name();
            let name = name.to_string_lossy();
            if p.is_dir() {
                // Build output and dependencies are not our documents.
                if !matches!(name.as_ref(), "target" | ".git" | "node_modules") {
                    walk(&p, out);
                }
            } else if p.extension().is_some_and(|x| x == "md") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&root(), &mut out);
    out
}

/// Names that appear on an `export`/`unset` line inside a fenced shell block.
fn recommended_in(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut in_shell_block = false;
    for line in text.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("```") {
            let lang = rest.trim().to_ascii_lowercase();
            in_shell_block = if in_shell_block {
                false
            } else {
                matches!(lang.as_str(), "bash" | "sh" | "shell" | "console" | "zsh")
            };
            continue;
        }
        if !in_shell_block {
            continue;
        }
        let Some(rest) = t
            .strip_prefix("export ")
            .or_else(|| t.strip_prefix("unset "))
        else {
            continue;
        };
        for word in rest.split_whitespace() {
            let name: String = word
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
                .collect();
            if name.starts_with("AETHER_") {
                found.insert(name);
            }
        }
    }
    found
}

/// Every `"AETHER_…"` string literal in the shell's own source.
fn read_by_source() -> BTreeSet<String> {
    fn walk(dir: &Path, out: &mut String) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                if let Ok(s) = std::fs::read_to_string(&p) {
                    out.push_str(&s);
                }
            }
        }
    }
    let mut src = String::new();
    walk(&root().join("src"), &mut src);
    walk(&root().join("crates"), &mut src);

    let mut names = BTreeSet::new();
    let bytes: Vec<char> = src.chars().collect();
    let needle: Vec<char> = "AETHER_".chars().collect();
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if bytes[i..i + needle.len()] == needle[..] {
            let name: String = bytes[i..]
                .iter()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || **c == '_')
                .collect();
            names.insert(name);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    names
}

#[test]
fn documented_environment_variables_are_read_by_the_shell() {
    let known = read_by_source();
    let mut bogus: Vec<(String, String)> = Vec::new();

    for path in markdown_files() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let rel = path
            .strip_prefix(root())
            .unwrap_or(&path)
            .display()
            .to_string();
        for name in recommended_in(&text) {
            if !known.contains(&name) {
                bogus.push((rel.clone(), name));
            }
        }
    }

    assert!(
        bogus.is_empty(),
        "these documents tell a reader to set variables the shell never reads, \
         so following them changes nothing:\n{}",
        bogus
            .iter()
            .map(|(f, n)| format!("  {f}: {n}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The check on the checker.
///
/// Both halves have to work: the scanner must find names in a shell block, and
/// the source index must be populated. If either silently returned nothing the
/// test above would pass while proving nothing at all.
#[test]
fn non_vacuity_the_scanner_and_the_index_both_work() {
    let sample = "text\n\n```bash\nexport AETHER_MODE=agent\nunset AETHER_ALLOW_SH\n```\n\
                  prose naming AETHER_SECURITY_LEVEL must be ignored\n";
    let found = recommended_in(sample);
    assert!(
        found.contains("AETHER_MODE") && found.contains("AETHER_ALLOW_SH"),
        "the scanner failed to read a shell block: {found:?}"
    );
    assert!(
        !found.contains("AETHER_SECURITY_LEVEL"),
        "prose outside a shell block must not count as a recommendation"
    );

    let fake = "text\n\n```bash\nexport AETHER_NOT_A_REAL_SETTING=1\n```\n";
    assert!(
        recommended_in(fake).contains("AETHER_NOT_A_REAL_SETTING"),
        "the scanner must catch an invented name"
    );

    let known = read_by_source();
    assert!(
        known.contains("AETHER_MODE") && known.contains("AETHER_WORKSPACE"),
        "the source index is empty or wrong, so the main test proves nothing"
    );
    assert!(
        !known.contains("AETHER_SECURITY_LEVEL"),
        "AETHER_SECURITY_LEVEL is not read by the source; if this fails the \
         variable was implemented and the documentation should be restored"
    );
}
