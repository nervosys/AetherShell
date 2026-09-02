//! A builtin the book tells you to call must be one the shell has.
//!
//! Publishing the book turned its contents from private notes into
//! instructions, and the audit that followed found several calls that do not
//! exist: `agent_reset` for clearing an agent's memory (`unknown builtin:
//! agent_reset`), and `workflow_templates` among sixteen `workflow_*` names
//! declared in `src/workflows.rs` by a `workflow_builtins()` whose only caller
//! is its own unit test.
//!
//! Those are worse than a gap. A reader who follows a chapter and gets
//! `unknown builtin` assumes they typed it wrong, or that their install is
//! broken; the documentation is the last thing anyone suspects.
//!
//! Scope is deliberately narrow, because precision matters more than reach
//! here. Only identifiers containing an underscore, in command position,
//! inside an ````aethershell```` fence are checked — `agent_reset`,
//! `tool_exec`, `workflow_create`. A bare word like `map` or `person` is
//! ambiguous (it is as likely a variable in the example), and a rule that
//! fires on those would be turned off rather than obeyed. Names an example
//! defines for itself are exempt.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Every string literal in the Rust sources. A builtin is dispatched by name,
/// so a name the sources never quote is a name the shell cannot answer to.
fn names_quoted_by_source() -> BTreeSet<String> {
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
                    out.push('\n');
                }
            }
        }
    }
    let mut src = String::new();
    walk(&root().join("src"), &mut src);
    walk(&root().join("crates"), &mut src);

    // Scan for the shape `"ident"` at every position rather than tracking
    // which quotes open and close a literal. A stateful reader loses phase on
    // the first `'"'` char literal or `r#"..."#` string in 200k lines of Rust
    // and then reports code between two unrelated quotes as a literal -- the
    // first version of this did exactly that, and claimed `tool_list` was not
    // a builtin.
    let mut out = BTreeSet::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' {
            let mut j = i + 1;
            while j < chars.len()
                && (chars[j].is_ascii_alphanumeric() || chars[j] == '_' || chars[j] == '-')
            {
                j += 1;
            }
            if j > i + 1 && j < chars.len() && chars[j] == '"' {
                out.insert(chars[i + 1..j].iter().collect::<String>());
            }
        }
        i += 1;
    }
    out
}

/// The underscored identifiers a block calls, and the ones it defines itself.
fn calls_and_definitions(block: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut calls = BTreeSet::new();
    let mut defined = BTreeSet::new();
    for raw in block.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        // `let name = ...` and `fn name(...)` introduce a name for this example.
        for kw in ["let ", "fn "] {
            if let Some(rest) = line.strip_prefix(kw) {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    defined.insert(name);
                }
            }
        }
        for seg in line.split('|') {
            let seg = seg.trim();
            let word: String = seg
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
                .collect();
            let followed_by_call = seg[word.len()..]
                .chars()
                .next()
                .is_some_and(|c| c == '(' || c == ' ' || c == '"');
            if word.contains('_') && !word.starts_with('_') && followed_by_call {
                calls.insert(word);
            }
        }
    }
    (calls, defined)
}

fn book_blocks() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "md") {
                out.push(p);
            }
        }
    }
    let src = root().join("docs").join("book").join("src");
    let mut files = Vec::new();
    walk(&src, &mut files);

    let mut blocks = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        let rel = f
            .strip_prefix(&src)
            .unwrap_or(&f)
            .to_string_lossy()
            .replace('\\', "/");
        let mut current: Option<String> = None;
        for line in text.lines() {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix("```") {
                match current.take() {
                    Some(body) => blocks.push((rel.clone(), body)),
                    None => {
                        let lang = rest.trim().to_ascii_lowercase();
                        if lang == "aethershell" || lang == "ae" {
                            current = Some(String::new());
                        }
                    }
                }
                continue;
            }
            if let Some(body) = current.as_mut() {
                body.push_str(line);
                body.push('\n');
            }
        }
    }
    blocks
}

#[test]
fn every_underscored_call_in_the_book_names_something_real() {
    let known = names_quoted_by_source();
    let mut bad: Vec<String> = Vec::new();
    for (file, block) in book_blocks() {
        let (calls, defined) = calls_and_definitions(&block);
        for name in calls {
            if defined.contains(&name) || known.contains(&name) {
                continue;
            }
            bad.push(format!("{file}: {name}"));
        }
    }
    assert!(
        bad.is_empty(),
        "the book calls builtins the source never names, so a reader following \
         it gets `unknown builtin`:\n  {}",
        bad.join("\n  ")
    );
}

#[test]
fn non_vacuity_the_scanner_finds_calls_and_the_index_is_populated() {
    let known = names_quoted_by_source();
    assert!(
        known.contains("tool_exec") && known.contains("agent_with_mcp"),
        "the source index missed known builtins, so the test above proves nothing"
    );
    assert!(
        !known.contains("agent_reset"),
        "agent_reset is now named by the source. If it became a builtin, the \
         documentation for clearing an agent's memory should be restored."
    );

    let blocks = book_blocks();
    assert!(
        blocks.len() > 50,
        "found only {} aethershell blocks in the book; the test above would \
         pass vacuously",
        blocks.len()
    );

    // The scanner must see a call, exempt a locally defined name, and ignore a
    // bare word with no underscore.
    let (calls, defined) = calls_and_definitions(
        "let my_helper = fn(x) => x\nmy_helper(1)\ntool_exec(\"git\")\nmap [1,2]\n",
    );
    assert!(
        calls.contains("tool_exec"),
        "the scanner failed to see a call: {calls:?}"
    );
    assert!(
        defined.contains("my_helper"),
        "the scanner failed to see a definition: {defined:?}"
    );
    assert!(
        !calls.contains("map"),
        "bare words are out of scope and must not be flagged"
    );

    // And it must catch an invented name.
    let (invented, _) = calls_and_definitions("agent_reset(coder)\n");
    assert!(
        invented.contains("agent_reset"),
        "the scanner must catch an invented underscored builtin"
    );
}
