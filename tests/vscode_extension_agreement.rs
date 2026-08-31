//! The VS Code extension must describe the language that exists.
//!
//! The extension is a second, hand-maintained description of AetherShell:
//! a TextMate grammar naming keywords and builtins, and a `package.json`
//! pointing at files on disk. Nothing linked it to the shell, so it drifted in
//! exactly the way `AGENTS.md` had — advertising things that were never there.
//!
//! Measured when this test was written:
//!
//!   * 21 of 86 builtin tokens in the grammar were not callable. Some
//!     (`sin`, `cos`, `parse_json`, `substring`) existed nowhere; others
//!     (`log`, `merge`, `exec`, `download`) are module *members* — `git.log`,
//!     `ssh.exec` — which the `member-access` rule already scopes, so matching
//!     them as bare words highlighted ordinary variables named `log` or `set`.
//!   * 11 of the 19 real keywords were missing, `else` among them.
//!   * `package.json` set the `.ae` file icon to `./icons/ae-light.svg` and
//!     `./icons/ae-dark.svg`. Neither has ever existed in this repository —
//!     `git log --diff-filter=D` finds no deletion — so the icon had never
//!     worked in a published build.
//!
//! These are cosmetic next to a jail escape, and they are the same *kind* of
//! defect: a description that nobody checked against the thing described.

use aethershell::builtins::{BUILTIN_LOOKUP, FALLBACK_BUILTINS};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn ext_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("editors/vscode")
}

fn read_json(rel: &str) -> serde_json::Value {
    let p = ext_dir().join(rel);
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
}

/// Every name the dispatcher answers to, both halves of it — the same union
/// `tests/effect_snapshot.rs` pins, for the same reason: the fallback `match`
/// arms are as callable as the lookup table.
fn dispatcher_names() -> HashSet<String> {
    let mut s: HashSet<String> = BUILTIN_LOOKUP.keys().map(|k| k.to_string()).collect();
    s.extend(FALLBACK_BUILTINS.iter().map(|(n, _)| n.to_string()));
    s
}

/// Pull the alternatives out of a `\b(a|b|c)\b` TextMate match.
fn alternatives(pattern: &str) -> Vec<String> {
    let start = match pattern.find('(') {
        Some(i) => i + 1,
        None => return Vec::new(),
    };
    let end = match pattern.rfind(')') {
        Some(i) => i,
        None => return Vec::new(),
    };
    pattern[start..end]
        .split('|')
        .map(|s| s.to_string())
        .collect()
}

fn grammar_group(section: &str) -> Vec<String> {
    let g = read_json("syntaxes/aethershell.tmLanguage.json");
    let pats = g["repository"][section]["patterns"]
        .as_array()
        .unwrap_or_else(|| panic!("grammar has no repository.{section}.patterns"));
    pats.iter()
        .filter_map(|p| p["match"].as_str())
        .flat_map(alternatives)
        .collect()
}

/// The language's keywords, from the match arms in `src/parser.rs`. Pinned
/// rather than scraped: scraping string literals out of the parser produces
/// false members (`"from"` also appears in error text), and a checker that
/// fails for the wrong reason gets disabled. If you add a keyword to the
/// parser, add it here — this list is the checklist that makes the grammar
/// follow.
const LANGUAGE_KEYWORDS: &[&str] = &[
    "as", "async", "await", "catch", "else", "export", "false", "fn", "from", "if", "import",
    "let", "match", "mut", "null", "pub", "throw", "true", "try",
];

#[test]
fn the_grammar_only_highlights_builtins_that_exist() {
    let known = dispatcher_names();
    let tokens = grammar_group("builtins");
    assert!(
        tokens.len() > 40,
        "only {} builtin tokens found — the grammar's shape changed and this test is \
         no longer reading it",
        tokens.len()
    );

    let phantom: Vec<&String> = tokens.iter().filter(|t| !known.contains(*t)).collect();
    assert!(
        phantom.is_empty(),
        "the grammar scopes {} name(s) as builtins that the dispatcher does not \
         answer to: {:?}\n\nA name reachable only as a module member (`git.log`) is \
         covered by the member-access rule; matching it as a bare word highlights \
         every ordinary variable of that name.",
        phantom.len(),
        phantom
    );
}

#[test]
fn the_grammar_highlights_every_keyword_the_parser_accepts() {
    let scoped: HashSet<String> = grammar_group("keywords").into_iter().collect();

    let missing: Vec<&&str> = LANGUAGE_KEYWORDS
        .iter()
        .filter(|k| !scoped.contains(**k))
        .collect();
    assert!(
        missing.is_empty(),
        "these keywords are accepted by the parser but not highlighted: {missing:?}"
    );

    let known: HashSet<&str> = LANGUAGE_KEYWORDS.iter().copied().collect();
    let invented: Vec<&String> = scoped
        .iter()
        .filter(|k| !known.contains(k.as_str()))
        .collect();
    assert!(
        invented.is_empty(),
        "the grammar highlights {invented:?} as keywords, which the parser does not \
         treat as such — users see a word turn colour and infer a feature that is not there"
    );
}

/// Catches the class of bug that left the `.ae` file icon pointing at two
/// files that never existed: `vsce` packages what `package.json` names, and a
/// missing asset is silent at runtime.
#[test]
fn every_file_package_json_points_at_exists() {
    let pkg = read_json("package.json");
    let text = serde_json::to_string(&pkg).expect("re-serialize");

    // Every "./..." string in the manifest is a path into the extension root.
    let mut refs: Vec<String> = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '"' {
            if let Some(end) = bytes[i + 1..].iter().position(|c| *c == '"') {
                let s: String = bytes[i + 1..i + 1 + end].iter().collect();
                if s.starts_with("./") {
                    refs.push(s);
                }
                i += end + 2;
                continue;
            }
        }
        i += 1;
    }
    assert!(
        refs.len() >= 5,
        "found only {} relative paths in package.json — the manifest's shape changed \
         and this test is no longer reading it",
        refs.len()
    );

    // `./out/**` is build output, absent until `npm run compile`.
    let missing: Vec<&String> = refs
        .iter()
        .filter(|r| !r.starts_with("./out/"))
        .filter(|r| !ext_dir().join(r.trim_start_matches("./")).exists())
        .collect();
    assert!(
        missing.is_empty(),
        "package.json points at {} file(s) that do not exist: {:?}",
        missing.len(),
        missing
    );
}

/// The extension is versioned independently of the shell, so this does not
/// demand they match — only that the manifest carries a parseable version, so
/// a release cannot ship one that the marketplace rejects.
#[test]
fn the_extension_version_is_wellformed() {
    let pkg = read_json("package.json");
    let v = pkg["version"].as_str().expect("package.json has a version");
    let parts: Vec<&str> = v.split('.').collect();
    assert_eq!(parts.len(), 3, "version {v:?} is not major.minor.patch");
    for p in parts {
        assert!(
            p.parse::<u32>().is_ok(),
            "version {v:?} has a non-numeric component"
        );
    }
}
