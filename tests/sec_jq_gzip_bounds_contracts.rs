//! Bounds for three builtins that used to run without one:
//!
//! - `gpg_list_keys` (through `sec_run_cmd`): a deadline and an output cap.
//! - `db_json_query` (its `jq` run): a 30 s deadline and a 16 MiB output cap.
//! - `gzip_decompress`: a cap on the decompressed bytes, with no partial file
//!   left behind on any failure.
//!
//! The ordinary tests run everywhere: refusals, the cap on a gzip bomb, and a
//! deadline on a child that prints nothing. A test that needs jq runs only when
//! `jq --version` succeeds on this machine; otherwise it prints a note and
//! checks the coded E_TOOL_MISSING instead.

use aethershell::env::Env;
use aethershell::value::Value;
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    aethershell::builtins::call(name, args, &mut env)
}

fn call_piped(name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let mut env = Env::new();
    aethershell::builtins::call_with_input(name, args, Some(input), &mut env)
}

fn code_of(e: &anyhow::Error) -> String {
    let s = e.to_string();
    match s.find("E_") {
        Some(i) => s[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {s}"),
    }
}

fn refused(name: &str, args: Vec<Value>, want: &str) {
    match call(name, args) {
        Ok(v) => panic!("{name} should have been refused with {want}, got {v:?}"),
        Err(e) => assert_eq!(code_of(&e), want, "{name}: {e}"),
    }
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

/// A scratch directory unique to this process and test name.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("ae_sec_jq_gzip_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn s_path(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

fn gzip_bytes(data: &[u8]) -> Vec<u8> {
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    enc.write_all(data).expect("compress");
    enc.finish().expect("finish")
}

/// Whether `program --version` runs and exits 0 on this machine.
fn tool_present(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|st| st.success())
        .unwrap_or(false)
}

// ── gzip_decompress ─────────────────────────────────────────────────────────

#[test]
fn gzip_bomb_past_the_cap_is_refused_and_leaves_no_output() {
    let dir = Scratch::new("bomb");
    // 4 MiB of zeros compresses to a few kilobytes: the bomb shape.
    let bomb = dir.path("bomb.gz");
    std::fs::write(&bomb, gzip_bytes(&vec![0u8; 4 * 1024 * 1024])).unwrap();
    let out = dir.path("bomb");
    let bomb_s = s_path(&bomb);

    // The hook runs gzip_decompress with a 1 MiB cap, so the bomb is 4x over it.
    let err = aethershell::builtins::__test_gzip_decompress_capped(&bomb_s, 1024 * 1024)
        .expect_err("a bomb past the cap must be refused");
    assert_eq!(code_of(&err), "E_BUDGET_EXCEEDED", "{err}");
    assert!(
        !out.exists(),
        "the partial output must be removed on refusal"
    );
}

#[test]
fn gzip_output_exactly_at_the_cap_is_kept() {
    let dir = Scratch::new("boundary");
    let data = vec![b'x'; 1000];
    let gz = dir.path("b.txt.gz");
    std::fs::write(&gz, gzip_bytes(&data)).unwrap();
    let gz_s = s_path(&gz);

    // A cap one byte short of the output refuses; a cap of exactly the output size keeps it.
    let err = aethershell::builtins::__test_gzip_decompress_capped(&gz_s, 999)
        .expect_err("999 bytes must not hold 1000");
    assert_eq!(code_of(&err), "E_BUDGET_EXCEEDED", "{err}");
    assert!(!dir.path("b.txt").exists());

    aethershell::builtins::__test_gzip_decompress_capped(&gz_s, 1000)
        .expect("an output of exactly the cap is allowed");
    assert_eq!(std::fs::read(dir.path("b.txt")).unwrap(), data);
}

#[test]
fn gzip_truncated_stream_fails_coded_and_leaves_no_partial_file() {
    let dir = Scratch::new("truncated");
    let full = gzip_bytes(
        &(0..200_000u32)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<u8>>(),
    );
    let gz = dir.path("cut.bin.gz");
    // Keep the header and part of the body: output is written, then the stream ends early.
    std::fs::write(&gz, &full[..full.len() / 2]).unwrap();
    let out = dir.path("cut.bin");

    refused("gzip_decompress", vec![s(&s_path(&gz))], "E_BAD_ARG");
    assert!(
        !out.exists(),
        "a failed decompression must not leave its output"
    );
}

#[test]
fn gzip_round_trip_through_the_builtins_is_unchanged() {
    let dir = Scratch::new("roundtrip");
    let plain = dir.path("notes.txt");
    let text: String = (0..500).map(|i| format!("line {i}\n")).collect();
    std::fs::write(&plain, &text).unwrap();

    let gz = call("gzip_compress", vec![s(&s_path(&plain))]).expect("compress");
    assert!(matches!(gz, Value::Bool(true)));
    assert!(dir.path("notes.txt.gz").exists());

    // gzip_decompress writes the file back beside the archive, so the original goes first.
    std::fs::remove_file(&plain).unwrap();
    let done = call(
        "gzip_decompress",
        vec![s(&s_path(&dir.path("notes.txt.gz")))],
    )
    .expect("decompress");
    assert!(matches!(done, Value::Bool(true)));
    assert_eq!(std::fs::read_to_string(&plain).unwrap(), text);
}

#[test]
fn gzip_decompress_refuses_an_existing_output_and_a_non_gz_name() {
    let dir = Scratch::new("refuse");
    let gz = dir.path("keep.txt.gz");
    std::fs::write(&gz, gzip_bytes(b"payload")).unwrap();
    std::fs::write(dir.path("keep.txt"), "already here").unwrap();

    refused("gzip_decompress", vec![s(&s_path(&gz))], "E_BAD_STATE");
    assert_eq!(
        std::fs::read_to_string(dir.path("keep.txt")).unwrap(),
        "already here",
        "an existing output is never overwritten"
    );
    refused(
        "gzip_decompress",
        vec![s(&s_path(&dir.path("plain.txt")))],
        "E_BAD_ARG",
    );
    refused("gzip_decompress", vec![Value::Int(3)], "E_BAD_ARG");
}

// ── sec_run_cmd, through gpg_list_keys ─────────────────────────────────────

#[test]
fn gpg_list_keys_refuses_arguments_and_piped_input() {
    refused("gpg_list_keys", vec![s("--homedir")], "E_BAD_ARG");
    let err = call_piped("gpg_list_keys", vec![], s("x")).expect_err("piped input must be refused");
    assert_eq!(code_of(&err), "E_BAD_ARG", "{err}");
}

#[test]
fn sec_run_cmd_kills_a_silent_child_at_its_deadline() {
    // Each program sleeps for 30 s. A 2 s deadline must end the call well before that.
    let (program, args): (&str, Vec<String>) = if cfg!(windows) {
        ("ping", vec!["-n".into(), "30".into(), "127.0.0.1".into()])
    } else {
        ("sleep", vec!["30".into()])
    };
    let started = Instant::now();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let _ = tx.send(aethershell::builtins::__test_sec_run_cmd(program, &refs, 2));
    });
    // The hard bound: the call must answer within 60 s even if the kill were broken.
    let answer = rx
        .recv_timeout(Duration::from_secs(60))
        .expect("sec_run_cmd did not return within 60 s");
    let err = answer.expect_err("a child past its deadline must be refused");
    assert_eq!(code_of(&err), "E_BUDGET_EXCEEDED", "{err}");
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "the deadline was not enforced: {:?}",
        started.elapsed()
    );
}

// ── db_json_query (jq) ──────────────────────────────────────────────────────

#[test]
fn db_json_query_refuses_option_like_arguments_and_bad_types() {
    refused("db_json_query", vec![s("-f"), s(".")], "E_BAD_ARG");
    refused(
        "db_json_query",
        vec![s("x.json"), s("--from-file")],
        "E_BAD_ARG",
    );
    refused("db_json_query", vec![Value::Int(1)], "E_BAD_ARG");
}

#[test]
fn db_json_query_without_jq_is_tool_missing() {
    if tool_present("jq") {
        println!("note: jq is installed; the E_TOOL_MISSING check is skipped here");
        return;
    }
    let dir = Scratch::new("nojq");
    let json = dir.path("a.json");
    std::fs::write(&json, "{\"a\": 1}").unwrap();
    refused(
        "db_json_query",
        vec![s(&s_path(&json)), s(".a")],
        "E_TOOL_MISSING",
    );
}

#[test]
fn db_json_query_runs_jq_over_the_file() {
    if !tool_present("jq") {
        println!("note: jq is not on PATH; skipping the real db_json_query run");
        return;
    }
    let dir = Scratch::new("jq_real");
    let json = dir.path("a.json");
    std::fs::write(&json, "{\"a\": {\"b\": 7}}").unwrap();
    match call("db_json_query", vec![s(&s_path(&json)), s(".a.b")]).expect("jq run") {
        Value::Int(n) => assert_eq!(n, 7),
        other => panic!("expected 7, got {other:?}"),
    }
    // A filter jq rejects is a coded tool failure, never the whole document.
    refused(
        "db_json_query",
        vec![s(&s_path(&json)), s(".a[")],
        "E_TOOL_FAILED",
    );
}

#[test]
fn db_json_query_refuses_output_past_the_cap() {
    if !tool_present("jq") {
        println!("note: jq is not on PATH; skipping the output-cap run");
        return;
    }
    // A 17 MiB string: `.` echoes it, which is past the 16 MiB cap.
    let dir = Scratch::new("jq_cap");
    let json = dir.path("big.json");
    let mut body = Vec::with_capacity(17 * 1024 * 1024 + 2);
    body.push(b'"');
    body.resize(body.len() + 17 * 1024 * 1024, b'a');
    body.push(b'"');
    std::fs::write(&json, &body).unwrap();
    refused(
        "db_json_query",
        vec![s(&s_path(&json)), s(".")],
        "E_BUDGET_EXCEEDED",
    );
}
