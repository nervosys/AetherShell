//! Two ways a builtin can turn a failed tool into a quiet answer, counted.
//!
//! 1. It runs a command and never reads the exit status, so whatever reached
//!    stdout -- often nothing -- is returned as the result. Every git builtin
//!    did this: outside a repository `git_status()` was `[]`, a clean tree.
//! 2. It returns `Bool(status.success())`, so the failure survives as `false`
//!    and the reason (stderr, the exit code) is dropped.
//!
//! Neither is always wrong. `lsof` and `grep` exit 1 for "no match"; test
//! runners and linters exit non-zero to report what they found; a `_check`
//! builtin may legitimately answer false. So this does not demand zero: it
//! pins the counts, which may only fall, and each fix is a reading of one
//! body. Lower the ceilings whenever a fix lowers the count.

const UNCHECKED_MAX: usize = 49;
const BOOL_SUCCESS_MAX: usize = 100;

fn source() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builtins.rs");
    std::fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

/// Split at every top-level `fn` so each chunk is one function body.
fn functions(src: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in src.split_inclusive('\n') {
        let head = line
            .strip_prefix("pub(crate) fn ")
            .or_else(|| line.strip_prefix("pub fn "))
            .or_else(|| line.strip_prefix("fn "));
        if let Some(rest) = head {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            out.push((name, String::new()));
        }
        if let Some((_, body)) = out.last_mut() {
            body.push_str(line);
        }
    }
    out
}

fn unchecked(src: &str) -> Vec<String> {
    functions(src)
        .into_iter()
        .filter(|(_, b)| {
            b.contains("Command::new(")
                && b.contains(".output()")
                && !b.contains(".status")
                && !b.contains("output_with_timeout")
                && !b.contains("git_succeeded(")
        })
        .map(|(n, _)| n)
        .collect()
}

fn bool_success(src: &str) -> usize {
    ["output", "o", "out"]
        .iter()
        .map(|v| {
            src.matches(&format!("Ok(Value::Bool({v}.status.success()))"))
                .count()
        })
        .sum()
}

#[test]
fn builtins_that_ignore_the_exit_status_only_decrease() {
    let found = unchecked(&source());
    assert!(
        found.len() <= UNCHECKED_MAX,
        "{} builtins run a command and never read its exit status (ceiling {UNCHECKED_MAX}). \
         Check the status in the new one:\n  {}",
        found.len(),
        found.join("\n  ")
    );
    if found.len() < UNCHECKED_MAX {
        eprintln!(
            "exit-status ratchet: {} < {UNCHECKED_MAX}; lower UNCHECKED_MAX",
            found.len()
        );
    }
}

#[test]
fn failures_reported_as_false_only_decrease() {
    let n = bool_success(&source());
    assert!(
        n <= BOOL_SUCCESS_MAX,
        "{n} sites return Bool(status.success()) (ceiling {BOOL_SUCCESS_MAX}); \
         report the failure with its stderr instead"
    );
}

/// Non-vacuity: the scanner must find the pattern it counts. A git builtin
/// with its check removed is exactly what it exists to catch.
#[test]
fn the_scanner_sees_an_unchecked_body() {
    let sample = "fn bi_x(a: Vec<Value>) -> Result<Value> {\n    \
                  let output = std::process::Command::new(\"git\").output()?;\n    \
                  Ok(Value::Str(String::from_utf8_lossy(&output.stdout).into()))\n}\n\
                  fn bi_y() -> Result<Value> {\n    \
                  let output = std::process::Command::new(\"git\").output()?;\n    \
                  git_succeeded(\"y\", &output)?;\n    Ok(Value::Null)\n}\n";
    assert_eq!(unchecked(sample), ["bi_x"]);
    assert_eq!(bool_success("Ok(Value::Bool(output.status.success()))"), 1);
    assert!(
        unchecked(&source()).len() > 10,
        "the scan found almost nothing"
    );
}
