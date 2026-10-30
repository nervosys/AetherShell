//! `--deterministic` must not be a way around redaction in agent mode.
//!
//! `run_one` returned the canonical JSON before it reached the agent renderer,
//! and only the agent renderer redacted, so `ish --agent --deterministic -c`
//! printed a record's password in the clear while `ish --agent -c` hid it.
//! Found by a reliability audit of the output paths.
//!
//! Driven through the real binary, because the precedence between the two
//! renderers lives in `run_one`.

use std::process::{Command, Stdio};

/// The `ish` binary built alongside this test.
fn ish() -> std::path::PathBuf {
    let mut p = std::env::current_exe().expect("test exe path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join(format!("ish{}", std::env::consts::EXE_SUFFIX))
}

const SECRET: &str = r#"{user: "ada", password: "hunter2-not-a-real-one"}"#;

fn run(args: &[&str], env: &[(&str, &str)]) -> String {
    let session = std::env::temp_dir().join(format!("ish-detredact-{}", std::process::id()));
    let mut cmd = Command::new(ish());
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("IRONSHELL_MODE")
        .env_remove("IRONSHELL_REDACT")
        .env_remove("IRONSHELL_DETERMINISTIC")
        .env("IRONSHELL_SESSION_DIR", &session);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run ish");
    let _ = std::fs::remove_dir_all(&session);
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn agent_output_hides_the_password_with_or_without_deterministic() {
    let plain = run(&["--agent", "-c", SECRET], &[]);
    assert!(plain.contains("ada"), "the record did not render: {plain}");
    assert!(!plain.contains("hunter2"), "agent output leaked: {plain}");

    let det = run(&["--agent", "--deterministic", "-c", SECRET], &[]);
    assert!(det.contains("ada"), "the record did not render: {det}");
    assert!(!det.contains("hunter2"), "--deterministic leaked: {det}");
}

#[test]
fn deterministic_output_is_still_canonical_and_still_full_for_humans() {
    // Redaction is an agent-path scrub; the human profile keeps full fidelity,
    // and the agent path stays canonical JSON (sorted keys).
    let human = run(&["--deterministic", "-c", SECRET], &[]);
    assert!(
        human.contains("hunter2"),
        "human output was scrubbed: {human}"
    );

    let det = run(&["--agent", "--deterministic", "-c", SECRET], &[]);
    let p = det.find("\"password\"").expect("password key present");
    let u = det.find("\"user\"").expect("user key present");
    assert!(p < u, "keys are not in canonical order: {det}");
}

#[test]
fn opting_out_of_redaction_still_works_in_deterministic_mode() {
    let det = run(
        &["--agent", "--deterministic", "-c", SECRET],
        &[("IRONSHELL_REDACT", "off")],
    );
    assert!(
        det.contains("hunter2"),
        "IRONSHELL_REDACT=off was ignored: {det}"
    );
}
