//! Every `.ae` file this repository ships must parse.
//!
//! `examples/` and `lib/` are the first AetherShell most people read, and
//! `lib/` is the standard library — but nothing ever ran them, so they drifted
//! away from the language. Measured when this test was written: seven examples
//! used `/* … */` block comments, which the parser has never accepted (it takes
//! `#` and `//`), and several more used syntax the parser rejects outright.
//!
//! Parsing is the right bar here rather than execution. Running these needs API
//! keys, network access and a TUI; parsing needs nothing, is deterministic, and
//! catches the entire class of defect that had accumulated — a file that does
//! not parse cannot possibly work, whatever the environment.

use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn ae_files(dir: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(repo(dir))
        .unwrap_or_else(|e| panic!("read {dir}: {e}"))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "ae"))
        .collect();
    out.sort();
    out
}

/// Parse each file and return the ones that fail, with the first error.
fn parse_failures(files: &[PathBuf]) -> Vec<(String, String)> {
    let mut bad = Vec::new();
    for path in files {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                bad.push((path.display().to_string(), format!("unreadable: {e}")));
                continue;
            }
        };
        if let Err(e) = aethershell::parser::parse_program(&src) {
            let first = e.to_string().lines().take(2).collect::<Vec<_>>().join(" ");
            bad.push((
                path.file_name().unwrap().to_string_lossy().to_string(),
                first,
            ));
        }
    }
    bad
}

fn report(kind: &str, bad: &[(String, String)]) -> String {
    let mut s = format!("{} {kind} file(s) do not parse:\n", bad.len());
    for (f, e) in bad {
        s.push_str(&format!("  {f}\n      {e}\n"));
    }
    s
}

#[test]
fn every_example_parses() {
    let files = ae_files("examples");
    assert!(
        files.len() > 20,
        "only {} examples found — this test is looking in the wrong place",
        files.len()
    );
    let bad = parse_failures(&files);
    assert!(bad.is_empty(), "{}", report("example", &bad));
}

#[test]
fn every_stdlib_file_parses() {
    let files = ae_files("lib");
    assert!(
        !files.is_empty(),
        "no .ae files in lib/ — this test is looking in the wrong place",
    );
    let bad = parse_failures(&files);
    assert!(bad.is_empty(), "{}", report("stdlib", &bad));
}

/// The specific syntax that broke seven examples: `/* … */` is not a comment in
/// AetherShell, and a file using it fails at its first line. Asserted directly
/// so the reason stays attached to the rule.
#[test]
fn block_comments_are_not_valid_and_no_shipped_script_uses_them() {
    assert!(
        aethershell::parser::parse_program("/* nope */\n1\n").is_err(),
        "`/* */` now parses — if block comments were added, this test and the \
         comment rules in editors/vscode should learn about them together"
    );
    assert!(aethershell::parser::parse_program("# yes\n1\n").is_ok());
    assert!(aethershell::parser::parse_program("// yes\n1\n").is_ok());

    let mut offenders = Vec::new();
    for dir in ["examples", "lib"] {
        for path in ae_files(dir) {
            if std::fs::read_to_string(&path)
                .unwrap_or_default()
                .contains("/*")
            {
                offenders.push(path.display().to_string());
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these shipped scripts contain `/*`, which the parser rejects: {offenders:#?}"
    );
}
