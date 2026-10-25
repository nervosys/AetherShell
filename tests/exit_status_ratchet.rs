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
//! builtin may legitimately answer false. So the second count is pinned, and
//! may only fall, each fix a reading of one body; lower its ceiling whenever a
//! fix lowers it. The first reached zero on 2026-10-24, so it is now a rule: a
//! tool whose non-zero exit is an answer reads the status and says so.
//!
//! `bounded_cmd_output` is `.output()` with a deadline and leaves the exit
//! status to its caller just as `.output()` did, so it counts as a spawn here.
//! When the probes moved onto it the ceiling was 49 and this scanner, which
//! still looked only for `.output()`, found 5 and its non-vacuity check
//! failed; counting both forms, it finds 38.
//!
//! `probe_text` is the checked form of a probe: it runs `bounded_cmd_output`
//! and refuses a non-zero exit (through `probe_stdout`). A builtin that calls
//! it has no `bounded_cmd_output(` in its own body, so it is not counted here.
//!
//! A body that hands its `Output` to a helper which reads the status (see
//! `status_checkers`) is checked too. Counting those as unchecked had held
//! the ceiling at 26 when only two bodies dropped the status: the twenty gui
//! builtins, `cron_list`, `sudo_check`, `at_list` and `crypto_key_generate`
//! all check theirs one call away. `platform_snapshot` and
//! `platform_system_libs`, the last two, then had their status read.
//!
//! The second ceiling stood at 64 long after the sites it counted were fixed;
//! read again on 2026-10-25 it found two. `proc_exists` answered `kill -0`,
//! whose EPERM made every other user's process "gone", and now asks in
//! process. `crypto_cert_verify` is left: openssl's non-zero exit for a
//! certificate that does not verify is the answer, and a missing or
//! unreadable file is refused before it runs.

const BOOL_SUCCESS_MAX: usize = 1;

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

fn signature(body: &str) -> &str {
    body.split('{').next().unwrap_or("")
}

/// Functions that take a finished `Output` and read its status: handing the
/// output to one of these is checking it. `parse_at_queue`,
/// `crypto_key_output`, `parse_cron_listing` and the gui answer helpers are
/// such helpers, and their callers were counted as unchecked because the
/// check was one call away.
fn status_checkers(fns: &[(String, String)]) -> Vec<String> {
    fns.iter()
        .filter(|(_, b)| signature(b).contains("std::process::Output") && b.contains(".status"))
        .map(|(n, _)| n.clone())
        .collect()
}

fn spawns(body: &str) -> bool {
    body.contains("Command::new(")
        && (body.contains(".output()") || body.contains("bounded_cmd_output("))
}

fn unchecked(src: &str) -> Vec<String> {
    let fns = functions(src);
    let checkers = status_checkers(&fns);
    fns.into_iter()
        .filter(|(_, b)| {
            spawns(b)
                && !b.contains(".status")
                && !b.contains("output_with_timeout")
                && !b.contains("git_succeeded(")
                // A runner that returns the `Output` (`gui_powershell`) leaves
                // the status to its caller, as `bounded_cmd_output` does.
                && !signature(b).contains("-> Result<std::process::Output>")
                && !checkers.iter().any(|c| b.contains(&format!("{c}(")))
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
fn no_builtin_ignores_the_exit_status() {
    let found = unchecked(&source());
    assert!(
        found.is_empty(),
        "{} builtins run a command and never read its exit status. \
         Check the status in each:\n  {}",
        found.len(),
        found.join("\n  ")
    );
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
                  git_succeeded(\"y\", &output)?;\n    Ok(Value::Null)\n}\n\
                  fn bi_z() -> Result<Value> {\n    \
                  let cmd = std::process::Command::new(\"uname\");\n    \
                  let output = bounded_cmd_output(\"z\", \"uname\", cmd, 30)?;\n    \
                  Ok(Value::Str(String::from_utf8_lossy(&output.stdout).into()))\n}\n\
                  fn read_listing(output: std::process::Output) -> Result<Value> {\n    \
                  if !output.status.success() { return Err(failed()); }\n    Ok(Value::Null)\n}\n\
                  fn bi_w() -> Result<Value> {\n    \
                  let cmd = std::process::Command::new(\"atq\");\n    \
                  let output = bounded_cmd_output(\"w\", \"atq\", cmd, 30)?;\n    \
                  read_listing(output)\n}\n\
                  fn read_text(text: &str) -> bool {\n    text.contains(\".status\")\n}\n\
                  fn bi_v() -> Result<Value> {\n    \
                  let output = std::process::Command::new(\"atq\").output()?;\n    \
                  read_text(&String::from_utf8_lossy(&output.stdout));\n    Ok(Value::Null)\n}\n";
    // bi_w hands its output to a helper that reads the status; bi_v calls a
    // helper too, but one that never sees an `Output`, so it stays counted.
    assert_eq!(unchecked(sample), ["bi_x", "bi_z", "bi_v"]);
    assert_eq!(bool_success("Ok(Value::Bool(output.status.success()))"), 1);
    // The count is near zero now, so the guard against a blind scanner is on
    // what it reads, not on what it flags: it must still see the spawning
    // bodies and the helpers that check for them.
    let fns = functions(&source());
    assert!(
        fns.iter().filter(|(_, b)| spawns(b)).count() > 50,
        "the scan found almost no builtin that runs a command"
    );
    let checkers = status_checkers(&fns);
    for helper in ["parse_at_queue", "crypto_key_output", "parse_cron_listing"] {
        assert!(
            checkers.iter().any(|c| c == helper),
            "{helper} reads the status of the Output it is given"
        );
    }
}
