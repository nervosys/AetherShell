//! Printing a long non-ASCII value must not crash the shell.
//!
//! `value::pretty::truncate` sliced `&s[..80]` on a *byte* index. Any string
//! long enough to be truncated whose 80th byte fell inside a multi-byte
//! character panicked the evaluator:
//!
//! ```text
//! thread 'aether-eval' panicked at src/value.rs:470:
//! end byte index 80 is not a char boundary; it is inside '─' (bytes 79..82)
//! ```
//!
//! Five shipped examples did exactly that, because a row of box-drawing
//! characters is the most natural way to draw a banner. Any accented prose or
//! emoji of the same length did it too.
//!
//! These run the real binary: a panic inside the library would abort the test
//! process rather than fail an assertion, and the exit status is the signal.

use std::process::{Command, Stdio};

fn ae() -> std::path::PathBuf {
    let mut p = std::env::current_exe().expect("test exe");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join(format!("ae{}", std::env::consts::EXE_SUFFIX))
}

fn run(code: &str) -> (bool, String) {
    let out = Command::new(ae())
        .args(["-c", code])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run ae");
    let merged = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), merged)
}

fn assert_no_panic(label: &str, code: &str) {
    let (_, out) = run(code);
    assert!(
        !out.contains("panicked"),
        "{label} panicked the shell:\n{out}"
    );
    assert!(
        !out.contains("char boundary"),
        "{label} hit a char-boundary slice:\n{out}"
    );
}

/// The exact shape from the examples: a rule of box-drawing characters whose
/// length crosses the truncation limit.
#[test]
fn a_long_box_drawing_rule_does_not_panic() {
    for ch in ['─', '═', '━', '·'] {
        let rule: String = std::iter::repeat_n(ch, 60).collect();
        assert_no_panic(
            &format!("a 60-char rule of {ch:?}"),
            &format!("print(\"{rule}\")"),
        );
    }
}

/// Every byte length around the limit, so an off-by-one cannot hide: a
/// three-byte character is placed so that the boundary falls at each of its
/// bytes in turn.
#[test]
fn multibyte_characters_at_every_offset_around_the_limit() {
    for pad in 70..90 {
        let s = format!("{}★ tail", "a".repeat(pad));
        assert_no_panic(
            &format!("{pad} ASCII then a 3-byte char"),
            &format!("print(\"{s}\")"),
        );
    }
}

#[test]
fn emoji_and_cjk_do_not_panic() {
    let emoji: String = std::iter::repeat_n('🎨', 40).collect();
    assert_no_panic("40 emoji", &format!("print(\"{emoji}\")"));

    let cjk: String = std::iter::repeat_n('漢', 50).collect();
    assert_no_panic("50 CJK characters", &format!("print(\"{cjk}\")"));
}

#[test]
fn long_non_ascii_inside_containers_does_not_panic() {
    let rule: String = std::iter::repeat_n('═', 50).collect();
    assert_no_panic(
        "array of rules",
        &format!("print([\"{rule}\", \"{rule}\"])"),
    );
    assert_no_panic(
        "record with a rule",
        &format!("print({{banner: \"{rule}\"}})"),
    );
}

/// Truncation should count characters, not bytes — otherwise the limit silently
/// means a third as much text in any non-Latin script.
#[test]
fn truncation_counts_characters_not_bytes() {
    // 50 CJK characters is 150 bytes but well under an 80-character limit, so
    // nothing should be elided.
    let cjk: String = std::iter::repeat_n('漢', 50).collect();
    let (ok, out) = run(&format!("print(\"{cjk}\")"));
    assert!(ok, "printing 50 CJK characters failed: {out}");
    assert!(
        !out.contains('…'),
        "50 characters were truncated as though they were 150: {out}"
    );
}
