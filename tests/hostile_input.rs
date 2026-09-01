//! The shell must not crash or hang on input it did not choose.
//!
//! An agentic shell parses whatever a model, a file or a caller hands it. The
//! bar is therefore not "handles valid programs" but "**never** aborts the
//! process, and always terminates". A parse error is a fine outcome; a
//! `SIGSEGV`, a stack overflow, or an infinite loop is not.
//!
//! Written after probing found a real one: roughly 15,000 nested parentheses
//! overflowed the native stack and killed the process —
//!
//! ```text
//! thread 'aether-eval' has overflowed its stack
//! ```
//!
//! — as did nested arrays, nested records, long prefix-operator runs, and
//! `1 + 1 + 1 …` repeated far enough. The evaluator had guarded *call* depth
//! for a long time; the parser guarded nothing, and the iterative operator
//! loops built deep trees that only overflowed later, when something else
//! walked them.
//!
//! These tests run the real binary rather than calling the parser in-process,
//! because a stack overflow aborts the whole process: in-process it would take
//! the test harness with it and report nothing useful.

use std::process::{Command, Stdio};
use std::time::Duration;

fn ae() -> std::path::PathBuf {
    let mut p = std::env::current_exe().expect("test exe");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join(format!("ae{}", std::env::consts::EXE_SUFFIX))
}

/// What happened when the shell was handed this source.
#[derive(Debug, PartialEq)]
enum Outcome {
    /// Ran or refused cleanly — either is acceptable.
    Handled,
    /// The process died on a stack overflow or a signal.
    Crashed(String),
    /// Still running when the deadline passed.
    HungOrKilled,
}

/// These tests each spawn a process and hand it a large input. Run in
/// parallel on a loaded machine they starve each other and time out, which
/// looks exactly like the hang they are meant to detect — so they run one at a
/// time. The deadline is then generous rather than tight: what is under test is
/// *termination*, not latency. Measured individually, the worst of these inputs
/// finishes in about 200 ms.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn feed(source: &str) -> Outcome {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // pid + a process-wide counter + the clock. pid alone is not enough
    // (these tests are threads of one process) and pid + clock is not
    // either: a coarse `SystemTime` lets two tests read the same
    // nanosecond and share a directory. Observed on macOS in
    // tests/audit_concurrency.rs, where one test then read another's
    // files. The counter is what makes this unique; the clock only keeps
    // the names readable.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ae_hostile_{}_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("input.ae");
    std::fs::write(&path, source).expect("write input");

    // Output goes to files, not pipes.
    //
    // Several of these inputs make the shell emit tens of thousands of error
    // lines. With `Stdio::piped()` and a poll loop that only drains after exit,
    // the child fills the pipe buffer, blocks writing, and never exits — a
    // deadlock in the *harness* that is indistinguishable from the hang it is
    // supposed to be detecting. Files have no such limit.
    let out_path = dir.join("stdout");
    let err_path = dir.join("stderr");
    let mut child = Command::new(ae())
        .arg("--deterministic")
        .arg(&path)
        .stdout(Stdio::from(
            std::fs::File::create(&out_path).expect("stdout file"),
        ))
        .stderr(Stdio::from(
            std::fs::File::create(&err_path).expect("stderr file"),
        ))
        .spawn()
        .expect("spawn ae");

    // Poll rather than wait forever: a hang is a result, not a reason to stall
    // the suite.
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let status = loop {
        match child.try_wait().expect("try_wait") {
            Some(s) => break Some(s),
            None if std::time::Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    };
    let text = format!(
        "{}{}",
        std::fs::read_to_string(&out_path).unwrap_or_default(),
        std::fs::read_to_string(&err_path).unwrap_or_default()
    );
    let _ = std::fs::remove_dir_all(&dir);

    match status {
        None => Outcome::HungOrKilled,
        Some(_) if text.contains("overflowed its stack") => {
            Outcome::Crashed("stack overflow".into())
        }
        Some(_) if text.contains("panicked at") => Outcome::Crashed(format!(
            "panic: {}",
            text.lines()
                .find(|l| l.contains("panicked at"))
                .unwrap_or_default()
        )),
        Some(_) => Outcome::Handled,
    }
}

fn assert_handled(label: &str, source: &str) {
    match feed(source) {
        Outcome::Handled => {}
        other => panic!("{label}: {other:?}"),
    }
}

// ── Nesting depth: the class that was actually crashing ──────────────────

#[test]
fn deeply_nested_brackets_do_not_crash() {
    // Well past the ~15,000 that used to abort the process, and past the
    // guard, so the guard itself is what is being exercised.
    for (label, open, close) in [("parentheses", "(", ")"), ("arrays", "[", "]")] {
        for depth in [1_000, 20_000, 60_000] {
            let src = format!("{}1{}", open.repeat(depth), close.repeat(depth));
            assert_handled(&format!("{depth} nested {label}"), &src);
        }
    }
}

#[test]
fn deeply_nested_records_do_not_crash() {
    for depth in [1_000, 20_000, 60_000] {
        let src = format!("{}1{}", "{a:".repeat(depth), "}".repeat(depth));
        assert_handled(&format!("{depth} nested records"), &src);
    }
}

/// Prefix operators recurse through a different path than bracket nesting, and
/// were still crashing after the first guard went in.
#[test]
fn long_prefix_operator_runs_do_not_crash() {
    for (label, prefix, tail) in [
        ("minus", "-", "1"),
        ("not", "!", "true"),
        ("await", "await ", "1"),
        ("throw", "throw ", "1"),
    ] {
        for depth in [1_000, 50_000] {
            let src = format!("{}{}", prefix.repeat(depth), tail);
            assert_handled(&format!("{depth} leading {label}"), &src);
        }
    }
}

/// These parse *iteratively*, so they never troubled the parser's own stack —
/// they built a tree tens of thousands deep that overflowed whatever walked it
/// next. The failure surfaced far from its cause, which is what made it worth
/// a test of its own.
#[test]
fn long_operator_and_postfix_chains_do_not_crash() {
    let n = 40_000;
    let cases = [
        ("member access", format!("x{}", ".f".repeat(n))),
        ("call chain", format!("f{}", "()".repeat(n))),
        ("pipe chain", format!("1{}", " | f".repeat(n))),
        ("addition", format!("1{}", " + 1".repeat(n))),
        ("logical and", format!("true{}", " && true".repeat(n))),
        ("comparison", format!("1{}", " < 1".repeat(n))),
    ];
    for (label, src) in cases {
        assert_handled(&format!("{n}-long {label}"), &src);
    }
}

#[test]
fn unbalanced_brackets_do_not_crash() {
    for depth in [1_000, 60_000] {
        assert_handled(&format!("{depth} unclosed parens"), &"(".repeat(depth));
        assert_handled(&format!("{depth} unopened parens"), &")".repeat(depth));
        assert_handled(&format!("{depth} unclosed arrays"), &"[".repeat(depth));
        assert_handled(&format!("{depth} unclosed braces"), &"{".repeat(depth));
    }
}

// ── Ordinary code must still work ────────────────────────────────────────

/// A depth limit that also rejects real programs would be a worse bug than the
/// crash it prevents.
#[test]
fn realistic_programs_still_run() {
    for (label, src) in [
        (
            "pipeline",
            "print([1,2,3] | map(fn(x) => x * 2) | where(fn(x) => x > 2))",
        ),
        ("arithmetic", "print(1 + 2 * 3 - 4 / 2)"),
        ("member access", "let r = {a: {b: {c: 1}}}\nprint(r.a.b.c)"),
        ("nested calls", "print(len(str(len([1,2,3]))))"),
        ("moderate nesting", "print(((((((((((1))))))))))) "),
        (
            "currying",
            "let mk = fn(a) => fn(b) => a + b\nprint(mk(1)(2))",
        ),
        ("try/catch", "print(try { throw \"x\" } catch e { e })"),
        ("match", "print(match 2 { 1 => \"one\", _ => \"other\" })"),
    ] {
        assert_handled(label, src);
    }
}

/// A hundred levels is far more than real code uses and must keep working;
/// this pins the limit as generous rather than merely present.
#[test]
fn moderately_nested_code_is_not_rejected() {
    let src = format!("print({}1{})", "(".repeat(100), ")".repeat(100));
    let out = Command::new(ae())
        .args(["--deterministic", "-c", &src])
        .output()
        .expect("run");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains('1'),
        "100 levels of nesting should evaluate, got: {text:?}"
    );
}

// ── Malformed and adversarial bytes ──────────────────────────────────────

#[test]
fn malformed_sources_do_not_crash() {
    let cases: [(&str, String); 12] = [
        ("empty", String::new()),
        ("only whitespace", "   \n\t\r\n  ".into()),
        ("lone quote", "\"".into()),
        ("unterminated string", "let a = \"abc".into()),
        ("unterminated interpolation", "\"${".into()),
        ("nul byte", "let a = 1\u{0}let b = 2".into()),
        ("control characters", (1u8..32).map(|b| b as char).collect()),
        ("lone surrogate-ish bytes", "\u{fffd}\u{fffd}".into()),
        ("rtl override", "let \u{202e}abc = 1".into()),
        ("bom then code", "\u{feff}print(1)".into()),
        (
            "very long identifier",
            format!("let {} = 1", "a".repeat(200_000)),
        ),
        (
            "very long string",
            format!("let a = \"{}\"", "x".repeat(500_000)),
        ),
    ];
    for (label, src) in cases {
        assert_handled(label, &src);
    }
}

#[test]
fn pathological_interpolation_does_not_crash() {
    for (label, src) in [
        (
            "many holes",
            format!("print(\"{}\")", "${a}".repeat(20_000)),
        ),
        (
            "nested braces in a hole",
            format!("print(\"${{{}}}\")", "{".repeat(5_000)),
        ),
        ("unclosed hole", "print(\"${a\")".to_string()),
        ("hole containing a quote", "print(\"${\"}\")".to_string()),
    ] {
        assert_handled(label, &src);
    }
}

/// Every byte of a multi-byte character as the last thing in the file, so a
/// slice at the wrong boundary would show up.
#[test]
fn truncated_utf8_in_source_does_not_crash() {
    for take in 1..=3 {
        let mut src = String::from("print(\"");
        src.push_str(&"a".repeat(80));
        let snowman = "☃";
        src.push_str(&snowman.chars().next().unwrap().to_string()[..]);
        let _ = take;
        src.push_str("\")");
        assert_handled("multibyte near a boundary", &src);
    }
}
