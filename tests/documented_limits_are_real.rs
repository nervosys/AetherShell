//! A number the documentation quotes must be the number the code uses.
//!
//! The other documentation ratchets check *names* — that a builtin the docs
//! tell you to call exists, that an environment variable they tell you to
//! export is read. Neither looks at prose, and prose is where the security
//! claims live: a 10 MB output limit, a 512 MB memory limit, a 4,000-character
//! prompt cap, 198 tools with 14 of them Dangerous.
//!
//! Those were checked by hand once, against the running binary, when the AI
//! chapters were rewritten. A hand check holds for a day. A constant changes,
//! a tool joins the catalogue, and the document goes on quoting the old figure
//! with nothing to notice — which is how the 512 MB memory limit came to be
//! stated without the qualification that Windows enforces no memory limit at
//! all.
//!
//! Two kinds of claim are pinned here:
//!
//!   * **Catalogue counts** are computed from the library rather than compared
//!     against a literal. `OSToolsDatabase::new()` is built and its tools
//!     counted by safety level, so adding a `Dangerous` tool fails this until
//!     the chapter is updated.
//!   * **Limits** are read out of the source text, because the constants are
//!     private to their modules. Each is asserted to have been found before it
//!     is compared, so a renamed constant fails loudly instead of silently
//!     matching nothing.

use aethershell::os_tools::{OSToolsDatabase, SafetyLevel};
use std::path::{Path, PathBuf};

const SWARMS: &str = "docs/book/src/ai/swarms.md";
const TOOLS: &str = "docs/book/src/ai/tools.md";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read_doc(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// The document with every run of whitespace collapsed to a single space.
/// Prose is hard-wrapped, so a claim can be split across two lines and a raw
/// `contains` would miss it.
fn prose(rel: &str) -> String {
    read_doc(rel)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The Rust sources, concatenated. The limits live in private consts, so the
/// text is the only place a test outside the crate can read them from.
fn source_text() -> String {
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
    let mut s = String::new();
    walk(&root().join("src"), &mut s);
    s
}

/// The right-hand side of `const <name>: <ty> = <value>;`, trimmed.
fn const_value(src: &str, name: &str) -> String {
    let needle = format!("const {name}:");
    let at = src.find(&needle).unwrap_or_else(|| {
        panic!("no const named {name} in src/. Renamed? This test is now blind.")
    });
    let rest = &src[at..];
    let eq = rest.find('=').expect("const has no initialiser");
    let semi = rest.find(';').expect("const has no terminator");
    rest[eq + 1..semi].trim().to_string()
}

#[test]
fn the_tool_catalogue_counts_match_the_chapter() {
    let db = OSToolsDatabase::new();
    let total = db.tools.len();
    let count = |lvl: SafetyLevel| {
        db.tools
            .values()
            .filter(|t| std::mem::discriminant(&t.safety_level) == std::mem::discriminant(&lvl))
            .count()
    };
    let safe = count(SafetyLevel::Safe);
    let caution = count(SafetyLevel::Caution);
    let dangerous = count(SafetyLevel::Dangerous);
    let critical = count(SafetyLevel::Critical);

    assert_eq!(
        safe + caution + dangerous + critical,
        total,
        "a safety level exists that this test does not count"
    );

    let chapter = prose(TOOLS);
    for (figure, what) in [
        (total, "tools in the catalogue"),
        (safe, "Safe tools"),
        (caution, "Caution tools"),
        (dangerous, "Dangerous tools"),
        (critical, "Critical tools"),
    ] {
        assert!(
            chapter.contains(&figure.to_string()),
            "ai/tools.md never mentions {figure}, the real number of {what}. \
             The catalogue changed and the chapter did not: {total} total, \
             {safe} Safe, {caution} Caution, {dangerous} Dangerous, \
             {critical} Critical."
        );
    }
}

#[test]
fn the_documented_limits_match_the_constants() {
    let src = source_text();
    let swarms = prose(SWARMS);

    // (constant, the literal it must still hold, the claim the chapter makes)
    for (name, expected, claim) in [
        ("MAX_PROMPT_LENGTH", "4000", "4,000 characters"),
        ("MAX_NEWLINES", "50", "50 newlines"),
        ("MAX_OUTPUT_SIZE_BYTES", "10 * 1024 * 1024", "10 MB"),
        ("MAX_EXECUTION_TIMEOUT_SECS", "30", "30 seconds"),
    ] {
        let actual = const_value(&src, name);
        assert_eq!(
            actual, expected,
            "{name} is now {actual}, but ai/swarms.md still promises {claim}"
        );
        assert!(
            swarms.contains(claim),
            "ai/swarms.md no longer states {claim}, so {name} = {expected} is \
             undocumented or was reworded. Re-point this check at the new text."
        );
    }

    // The memory cap is a setrlimit field rather than a named constant, and it
    // is the one limit that does not apply everywhere.
    assert!(
        src.contains("rlim_cur: 512 * 1024 * 1024"),
        "the 512 MB rlimit is gone from src/, but the docs still promise it"
    );
    assert!(
        swarms.contains("512 MB") && swarms.contains("Linux and macOS only"),
        "the 512 MB memory cap must stay documented as Unix-only. The Windows \
         configure_sandbox is a no-op, so an unqualified claim is false there."
    );
    assert!(
        src.contains("#[cfg(target_os = \"windows\")]"),
        "the Windows sandbox stub is gone; re-check whether the qualification \
         above is still the right one"
    );
}

#[test]
fn the_documented_rate_limits_match_the_calls() {
    let src = source_text();
    for (label, call) in [
        (
            "agent calls",
            "check_rate_limit(\"bi_agent\", 10, Duration::from_secs(60))",
        ),
        (
            "agent plans",
            "check_rate_limit(\"agent_plan\", 10, Duration::from_secs(60))",
        ),
        (
            "agent executions",
            "check_rate_limit(\"agent_execute\", 5, Duration::from_secs(60))",
        ),
    ] {
        assert!(
            src.contains(call),
            "the {label} rate limit is no longer {call}, but ai/swarms.md still \
             quotes the old figures"
        );
    }

    let swarms = prose(SWARMS);
    for claim in [
        "10 agent calls per minute",
        "10 plans and 5 executions per minute",
    ] {
        assert!(
            swarms.contains(claim),
            "ai/swarms.md no longer states {claim}, which this test exists to pin"
        );
    }
}

#[test]
fn non_vacuity_the_readers_both_work() {
    let src = source_text();
    assert!(
        src.len() > 1_000_000,
        "the source reader returned {} bytes; every contains check above would \
         pass or fail for the wrong reason",
        src.len()
    );
    assert_eq!(
        const_value(&src, "MAX_EXECUTION_TIMEOUT_SECS"),
        "30",
        "the const reader is broken, so the limit checks prove nothing"
    );

    let db = OSToolsDatabase::new();
    assert!(
        db.tools.len() > 100,
        "the tool database holds only {} entries; the count check would be \
         comparing noise",
        db.tools.len()
    );
    assert!(
        db.tools
            .values()
            .any(|t| matches!(t.safety_level, SafetyLevel::Critical)),
        "no Critical tool in the catalogue, so that count is trivially zero"
    );

    // Line-wrapped prose must survive normalisation, and a figure that is not
    // there must still be reported as absent.
    let wrapped = prose(SWARMS);
    assert!(
        !wrapped.contains("  "),
        "prose() left a double space, so wrapped claims can still be missed"
    );
    assert!(
        !wrapped.contains("31337"),
        "the doc reader is returning something unexpected"
    );
}
