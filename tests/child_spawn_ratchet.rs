//! Every process the shell starts is built by `safety::child_command`, which
//! strips the shell's own `IRONSHELL_*` settings and, in agent mode, the
//! credentials. A raw constructor call anywhere in `src/` would skip that.
//!
//! This ratchet counts the raw `Command::new(` text in each file under `src/`
//! and compares it with `tests/child_spawn_allowlist.txt`. The count is raw
//! text, comments included, because the source scanners here do not skip them.
//! A file above its allowance fails; a file below it fails until the allowance
//! is lowered, so the list can only shrink.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const RAW_CALL: &str = "Command::new(";
const ALLOWLIST: &str = "tests/child_spawn_allowlist.txt";

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("src is readable") {
        let path = entry.expect("a src entry").path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The allowance per file, from `path: count: reason` lines.
fn parse_allowlist(text: &str) -> Result<BTreeMap<String, usize>, String> {
    let mut map = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, ':');
        let path = parts.next().unwrap_or("").trim();
        let count = parts.next().unwrap_or("").trim();
        let reason = parts.next().unwrap_or("").trim();
        let n: usize = count
            .parse()
            .map_err(|_| format!("line {}: the count '{count}' is not a number", i + 1))?;
        if path.is_empty() || reason.is_empty() {
            return Err(format!("line {}: needs `path: count: reason`", i + 1));
        }
        if map.insert(path.to_string(), n).is_some() {
            return Err(format!("line {}: {path} is listed twice", i + 1));
        }
    }
    Ok(map)
}

#[test]
fn every_raw_constructor_in_src_is_on_the_allowlist() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("src");
    let allow_text =
        std::fs::read_to_string(manifest.join(ALLOWLIST)).expect("the child spawn allowlist reads");
    let allowed = parse_allowlist(&allow_text).expect("the allowlist parses");

    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for file in &files {
        let rel = file
            .strip_prefix(manifest)
            .expect("a file under the manifest")
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(file).expect("a src file reads");
        let n = text.matches(RAW_CALL).count();
        if n > 0 {
            counts.insert(rel, n);
        }
    }

    let mut problems = Vec::new();
    for (rel, n) in &counts {
        let allow = allowed.get(rel).copied().unwrap_or(0);
        if *n > allow {
            problems.push(format!(
                "{rel}: {n} raw `{RAW_CALL}` calls, allowed {allow}; spawn through safety::child_command instead"
            ));
        } else if *n < allow {
            problems.push(format!(
                "{rel}: {n} raw calls, allowance is {allow}; lower it in {ALLOWLIST}"
            ));
        }
    }
    for (rel, allow) in &allowed {
        if !counts.contains_key(rel) && *allow > 0 {
            problems.push(format!(
                "{rel}: allowed {allow} but has no raw calls; remove it from {ALLOWLIST}"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "raw child spawns in src:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_allowlist_parser_rejects_a_line_without_a_reason() {
    assert!(parse_allowlist("src/x.rs: 1:").is_err());
    assert!(parse_allowlist("src/x.rs: one: reason").is_err());
    let ok = parse_allowlist("# comment\n\nsrc/x.rs: 3: why: because").expect("parses");
    assert_eq!(ok.get("src/x.rs"), Some(&3));
}
