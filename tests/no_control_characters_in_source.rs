//! No source file may contain a stray control character.
//!
//! Edits to this repository are often made by scripts, and a script that
//! writes Rust through a shell heredoc loses backslashes: `"\\b"` arrives as
//! `"\b"`, which the scripting language then reads as a BACKSPACE (U+0008).
//! It happened again while fixing `search_symbols` -- a regex meant to end in
//! a word boundary ended in a backspace instead -- and it compiled, because a
//! control character inside a string literal is perfectly legal Rust. The
//! regex would simply never have matched. An earlier occurrence put U+0001
//! into `.github/workflows/ci.yml`, which at least failed to parse.
//!
//! Tab, line feed and carriage return are the only C0 characters with any
//! business in a text file here.

use std::path::Path;

fn scan(dir: &Path, found: &mut Vec<String>, files: &mut usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan(&path, found, files);
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "rs" | "toml" | "yml" | "yaml" | "md" | "mjs" | "ae") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        *files += 1;
        for (i, line) in text.lines().enumerate() {
            if let Some(c) = line
                .chars()
                .find(|&c| (c as u32) < 0x20 && c != '\t' && c != '\r')
            {
                found.push(format!("{}:{}: U+{:04X}", path.display(), i + 1, c as u32));
            }
        }
    }
}

#[test]
fn source_files_contain_no_stray_control_characters() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found = Vec::new();
    let mut files = 0;
    for dir in ["src", "tests", "benches", "docs", ".github"] {
        scan(&root.join(dir), &mut found, &mut files);
    }

    // Non-vacuity: a scan that reads nothing finds nothing.
    assert!(
        files > 100,
        "only {files} files scanned; the walk is not reaching the source tree"
    );

    assert!(
        found.is_empty(),
        "{} stray control character(s). Almost certainly a backslash lost in a \
         scripted edit -- `\\b` becoming a backspace -- rather than anything \
         meant:\n{}",
        found.len(),
        found.join("\n")
    );
}
