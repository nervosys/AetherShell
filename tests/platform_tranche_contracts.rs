//! Contracts for the platform_* builtins: refusals, determinism of the pure
//! comparators, the platform store's round trip, and coded failures.
//!
//! The platform store lives at `$HOME/.ironshell/platform_db.json`. Every test that
//! writes it runs `ish` as a child process with HOME and USERPROFILE pointing at
//! a fresh temporary directory, so the user's real store is never touched. The
//! file is read back with serde_json, never through the builtin under test.
//! Each child has a 30 second deadline and a non-interactive stdin.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use iron_shell::builtins::{call, call_with_input};
use iron_shell::env::Env;
use iron_shell::value::Value;

const DEADLINE: Duration = Duration::from_secs(30);

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

fn rec(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}

fn field(v: &Value, key: &str) -> Value {
    match v {
        Value::Record(r) => r
            .get(key)
            .cloned()
            .unwrap_or_else(|| panic!("no field {key} in {v:?}")),
        other => panic!("expected a record, got {other:?}"),
    }
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

fn drain<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

struct Run {
    success: bool,
    stdout: String,
    stderr: String,
}

/// A temporary home directory for one test. The store is `.ironshell/platform_db.json`
/// under it, and `ish` runs with this as HOME, USERPROFILE and working directory.
struct Home {
    root: PathBuf,
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Home {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("ae_platform_tranche_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("session")).expect("create fixture directory");
        Self { root }
    }

    fn store(&self) -> PathBuf {
        self.root.join(".ironshell").join("platform_db.json")
    }

    fn read_store(&self) -> String {
        std::fs::read_to_string(self.store()).expect("read the platform store")
    }

    /// The store parsed by serde_json, which is not the builtin's own reader.
    fn store_object(&self) -> serde_json::Map<String, serde_json::Value> {
        serde_json::from_str::<serde_json::Value>(&self.read_store())
            .expect("the store is JSON")
            .as_object()
            .cloned()
            .expect("the store is a JSON object")
    }

    /// Run `ish -c <expr>` with this home and a hard deadline.
    fn ae(&self, expr: &str) -> Run {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ish"))
            .current_dir(&self.root)
            .env("HOME", &self.root)
            .env("USERPROFILE", &self.root)
            .env("IRONSHELL_SESSION_DIR", self.root.join("session"))
            .args(["-c", expr])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn ae");
        let out = drain(child.stdout.take());
        let err = drain(child.stderr.take());
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().expect("poll ae") {
                break status;
            }
            if started.elapsed() > DEADLINE {
                let _ = child.kill();
                let _ = child.wait();
                panic!("ae did not finish within 30 s: {expr}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        Run {
            success: status.success(),
            stdout: out.join().unwrap_or_default(),
            stderr: err.join().unwrap_or_default(),
        }
    }

    /// The value of `expr` as JSON. The call must succeed.
    fn json(&self, expr: &str) -> serde_json::Value {
        let run = self.ae(&format!("json.stringify({expr})"));
        assert!(
            run.success,
            "{expr} exited non-zero\nstdout: {}\nstderr: {}",
            run.stdout, run.stderr
        );
        serde_json::from_str(run.stdout.trim_end())
            .unwrap_or_else(|e| panic!("{expr} did not print JSON ({e}): {}", run.stdout))
    }

    /// The error code `expr` raised. A call that succeeds fails the test.
    fn refusal_code(&self, expr: &str) -> String {
        let run = self.ae(&format!(
            "try {{ {expr} }} catch e {{ json.stringify(e.error) }}"
        ));
        let printed = run.stdout.trim_end();
        let parsed: serde_json::Value = serde_json::from_str(printed).unwrap_or_else(|_| {
            panic!(
                "{expr} was not refused; it answered: {printed} {}",
                run.stderr
            )
        });
        parsed["code"]
            .as_str()
            .unwrap_or_else(|| panic!("{expr} raised no coded error: {parsed}"))
            .to_string()
    }
}

/// `path` with forward slashes, so it can sit inside an IronShell string.
fn slashed(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

// ── refusals, in process, for builtins that never touch the store ───────────

#[test]
fn comparators_and_readers_refuse_arguments_they_cannot_read() {
    let mut env = Env::new();
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("platform_require", vec![s("linux")]),
        ("platform_require", vec![Value::Int(1)]),
        ("platform_require", vec![rec(&[("os", Value::Int(42))])]),
        ("platform_require", vec![rec(&[("cargo", Value::Int(1))])]),
        ("platform_require", vec![rec(&[("cargo", s("banana"))])]),
        ("platform_require", vec![rec(&[]), rec(&[])]),
        ("platform_check_reqs", vec![s("linux")]),
        ("platform_satisfies", vec![Value::Int(1)]),
        (
            "platform_satisfies",
            vec![rec(&[("os", Value::Bool(true))])],
        ),
        ("platform_diff", vec![rec(&[])]),
        ("platform_diff", vec![s("x"), rec(&[])]),
        ("platform_diff", vec![rec(&[]), rec(&[]), rec(&[])]),
        ("platform_compatible", vec![rec(&[])]),
        // Two records with no os/arch/family used to compare equal.
        ("platform_compatible", vec![rec(&[]), rec(&[])]),
        (
            "platform_compatible",
            vec![
                rec(&[
                    ("os", s("linux")),
                    ("arch", s("x86_64")),
                    ("family", s("unix")),
                ]),
                rec(&[("os", s("linux"))]),
            ],
        ),
        ("platform_snapshot", vec![Value::Int(1)]),
        ("platform_fingerprint", vec![Value::Int(1)]),
        ("platform_memory", vec![Value::Int(1)]),
        ("platform_hardware_summary", vec![Value::Int(1)]),
    ];
    for (name, args) in cases {
        let err =
            call(name, args.clone(), &mut env).expect_err(&format!("{name} accepted {args:?}"));
        assert_eq!(code_of(&err), "E_BAD_ARG", "{name} {args:?}: {err}");
    }
}

#[test]
fn host_readers_and_comparators_refuse_piped_input() {
    let mut env = Env::new();
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("platform_require", vec![]),
        ("platform_check_reqs", vec![]),
        ("platform_satisfies", vec![]),
        ("platform_diff", vec![rec(&[]), rec(&[])]),
        (
            "platform_compatible",
            vec![
                rec(&[
                    ("os", s("linux")),
                    ("arch", s("x86_64")),
                    ("family", s("unix")),
                ]),
                rec(&[
                    ("os", s("linux")),
                    ("arch", s("x86_64")),
                    ("family", s("unix")),
                ]),
            ],
        ),
        ("platform_snapshot", vec![]),
        ("platform_fingerprint", vec![]),
        ("platform_memory", vec![]),
        ("platform_hardware_summary", vec![]),
    ];
    for (name, args) in cases {
        let err = call_with_input(name, args, Some(s("piped")), &mut env)
            .expect_err(&format!("{name} accepted piped input"));
        assert_eq!(code_of(&err), "E_BAD_ARG", "{name}: {err}");
    }
}

// ── the pure comparators on literal inputs ───────────────────────────────────

#[test]
fn comparators_answer_the_same_on_literal_inputs_every_time() {
    let mut env = Env::new();
    let linux = rec(&[
        ("os", s("linux")),
        ("arch", s("x86_64")),
        ("family", s("unix")),
    ]);
    let windows = rec(&[
        ("os", s("windows")),
        ("arch", s("x86_64")),
        ("family", s("windows")),
    ]);
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("platform_require", vec![]),
        (
            "platform_require",
            vec![rec(&[("os", s("nonexistent-os"))])],
        ),
        ("platform_satisfies", vec![]),
        (
            "platform_satisfies",
            vec![rec(&[("os", s("nonexistent-os"))])],
        ),
        (
            "platform_diff",
            vec![
                rec(&[("a", Value::Int(1)), ("b", Value::Int(2))]),
                rec(&[("a", Value::Int(3)), ("c", Value::Int(4))]),
            ],
        ),
        ("platform_compatible", vec![linux.clone(), linux.clone()]),
        ("platform_compatible", vec![linux, windows]),
    ];
    for (name, args) in cases {
        let first = call(name, args.clone(), &mut env)
            .unwrap_or_else(|e| panic!("{name} {args:?} failed: {e}"));
        for _ in 0..3 {
            let again = call(name, args.clone(), &mut env).unwrap();
            assert_eq!(again, first, "{name} {args:?} answered differently");
        }
    }
}

#[test]
fn comparators_give_the_literal_answers_their_declarations_promise() {
    let mut env = Env::new();

    let none = call("platform_require", vec![], &mut env).unwrap();
    assert_eq!(field(&none, "satisfied"), Value::Bool(true));
    assert_eq!(field(&none, "missing"), Value::Array(vec![]));

    let absent_os = rec(&[("os", s("nonexistent-os"))]);
    let unmet = call("platform_require", vec![absent_os.clone()], &mut env).unwrap();
    assert_eq!(field(&unmet, "satisfied"), Value::Bool(false));
    let Value::Array(missing) = field(&unmet, "missing") else {
        panic!("missing is not an array")
    };
    assert_eq!(missing.len(), 1);
    let Value::Str(why) = &missing[0] else {
        panic!("a missing entry is not a string")
    };
    assert!(why.starts_with("os: need nonexistent-os"), "{why}");

    // check_reqs is the same check under its own name.
    assert_eq!(
        call("platform_check_reqs", vec![absent_os.clone()], &mut env).unwrap(),
        unmet
    );

    // A tool that is not on PATH is unmet, with the reason.
    let no_tool = rec(&[("no-such-tool-aeth-platform", s(">=1.0"))]);
    let answer = call("platform_require", vec![no_tool], &mut env).unwrap();
    assert_eq!(field(&answer, "satisfied"), Value::Bool(false));
    assert!(format!("{answer:?}").contains("not found"), "{answer:?}");

    assert_eq!(
        call("platform_satisfies", vec![], &mut env).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        call("platform_satisfies", vec![absent_os], &mut env).unwrap(),
        Value::Bool(false)
    );

    let diff = call(
        "platform_diff",
        vec![
            rec(&[("a", Value::Int(1)), ("b", Value::Int(2))]),
            rec(&[("a", Value::Int(3)), ("c", Value::Int(4))]),
        ],
        &mut env,
    )
    .unwrap();
    assert_eq!(
        diff,
        rec(&[
            ("added", Value::Array(vec![s("c")])),
            ("removed", Value::Array(vec![s("b")])),
            (
                "changed",
                Value::Array(vec![rec(&[
                    ("key", s("a")),
                    ("old", Value::Int(1)),
                    ("new", Value::Int(3)),
                ])])
            ),
        ])
    );

    let same = rec(&[
        ("os", s("linux")),
        ("arch", s("x86_64")),
        ("family", s("unix")),
    ]);
    assert_eq!(
        call(
            "platform_compatible",
            vec![same.clone(), same.clone()],
            &mut env
        )
        .unwrap(),
        Value::Bool(true)
    );
    let other = rec(&[
        ("os", s("windows")),
        ("arch", s("x86_64")),
        ("family", s("windows")),
    ]);
    assert_eq!(
        call("platform_compatible", vec![same, other], &mut env).unwrap(),
        Value::Bool(false)
    );
}

// ── host readers: fingerprint stability and coded results ────────────────────

#[test]
fn the_fingerprint_names_the_machine_not_the_moment() {
    let mut env = Env::new();
    let first = call("platform_snapshot", vec![], &mut env).unwrap();
    // Timestamps have one-second resolution: wait so the clock visibly moves.
    std::thread::sleep(Duration::from_millis(1100));
    let second = call("platform_snapshot", vec![], &mut env).unwrap();
    assert_ne!(
        field(&first, "timestamp"),
        field(&second, "timestamp"),
        "the clock did not advance, so this test cannot tell"
    );
    let fp = field(&first, "fingerprint");
    assert!(matches!(fp, Value::Str(_)), "{fp:?}");
    assert_eq!(
        field(&second, "fingerprint"),
        fp,
        "the same machine gave two fingerprints"
    );
    assert_eq!(
        call("platform_fingerprint", vec![], &mut env).unwrap(),
        fp,
        "platform_fingerprint is not the snapshot's fingerprint"
    );
}

#[test]
fn memory_and_hardware_are_never_an_empty_answer() {
    let mut env = Env::new();
    match call("platform_memory", vec![], &mut env) {
        Ok(Value::Record(r)) => assert!(
            r.contains_key("total_kb") || r.contains_key("total_bytes"),
            "a memory record with no total: {r:?}"
        ),
        Ok(other) => panic!("platform_memory answered {other:?}, not a record"),
        Err(e) => assert!(
            ["E_TOOL_FAILED", "E_UNIMPLEMENTED", "E_TOOL_MISSING"].contains(&code_of(&e).as_str()),
            "platform_memory failed without a coded reason: {e}"
        ),
    }

    let hw = call("platform_hardware_summary", vec![], &mut env).unwrap();
    assert_eq!(field(&hw, "os"), s(std::env::consts::OS));
    assert_eq!(field(&hw, "arch"), s(std::env::consts::ARCH));
    if let Value::Record(r) = &hw {
        if let Some(Value::Array(unavailable)) = r.get("unavailable") {
            assert!(!unavailable.is_empty(), "an empty unavailable list");
            for entry in unavailable {
                let Value::Str(text) = entry else {
                    panic!("an unavailable entry is not a string: {entry:?}")
                };
                assert!(text.contains(": "), "no section named in: {text}");
            }
        }
    }
}

// ── the platform store, in a temporary home, read back independently ─────────

#[test]
fn the_store_round_trips_and_the_file_says_so() {
    let home = Home::new("roundtrip");

    let printed = home.json("platform_db_init()");
    let printed = printed.as_str().expect("init answers the path as a string");
    assert_eq!(
        Path::new(printed),
        home.store(),
        "init wrote somewhere else"
    );
    assert_eq!(
        home.read_store().trim(),
        "{}",
        "a new store is an empty object"
    );

    let linux = r#"{os: "linux", arch: "x86_64", family: "unix"}"#;
    let windows = r#"{os: "windows", arch: "x86_64", family: "windows"}"#;
    assert_eq!(
        home.json(&format!(r#"platform_db_store("alpha", {linux})"#)),
        serde_json::Value::Bool(true)
    );
    assert_eq!(home.store_object()["alpha"]["os"], "linux");
    assert_eq!(
        home.json(&format!(r#"platform_db_store("beta", {windows})"#)),
        serde_json::Value::Bool(true)
    );
    assert_eq!(home.store_object()["beta"]["os"], "windows");

    // Compare reports the change from the first key to the second.
    let diff = home.json(r#"platform_db_compare("alpha", "beta")"#);
    assert_eq!(diff["added"], serde_json::json!([]));
    assert_eq!(diff["removed"], serde_json::json!([]));
    let mut changed: Vec<String> = diff["changed"]
        .as_array()
        .expect("changed is an array")
        .iter()
        .map(|c| c["key"].as_str().unwrap_or("?").to_string())
        .collect();
    changed.sort();
    // The architecture is the same on both, so only the OS and its family differ.
    assert_eq!(changed, ["family", "os"]);

    let same = home.json(r#"platform_db_compare("alpha", "alpha")"#);
    assert_eq!(same["changed"], serde_json::json!([]));

    // A key that is not stored is named, not reported as an empty diff.
    assert_eq!(
        home.refusal_code(r#"platform_db_compare("alpha", "no-such-key-aeth")"#),
        "E_NOT_FOUND"
    );

    // Delete reports whether the key was there, and changes the file only then.
    assert_eq!(
        home.json(r#"platform_db_delete("alpha")"#),
        serde_json::Value::Bool(true)
    );
    let keys: Vec<String> = home.store_object().keys().cloned().collect();
    assert_eq!(keys, vec!["beta".to_string()]);
    let before = home.read_store();
    assert_eq!(
        home.json(r#"platform_db_delete("alpha")"#),
        serde_json::Value::Bool(false)
    );
    assert_eq!(home.read_store(), before, "a miss rewrote the store");

    // Import merges a JSON object from a file into the store.
    let import = home.root.join("import.json");
    std::fs::write(&import, r#"{"gamma": {"os": "linux", "arch": "x86_64"}}"#).unwrap();
    assert_eq!(
        home.json(&format!(r#"platform_db_import("{}")"#, slashed(&import))),
        serde_json::json!(2)
    );
    let keys: Vec<String> = home.store_object().keys().cloned().collect();
    assert_eq!(keys, vec!["beta".to_string(), "gamma".to_string()]);

    // Direction matters: beta is "before", gamma is "after". beta has family,
    // which gamma lacks, so family is removed.
    let diff = home.json(r#"platform_db_compare("beta", "gamma")"#);
    assert_eq!(diff["removed"], serde_json::json!(["family"]));
    assert_eq!(diff["added"], serde_json::json!([]));
}

#[test]
fn db_builtins_refuse_bad_arguments_before_touching_the_store() {
    let home = Home::new("refuse");
    home.json("platform_db_init()");
    let before = home.read_store();

    let cases = [
        "platform_db_init(1)",
        r#""piped" | platform_db_init()"#,
        "platform_db_store(42)",
        r#"platform_db_store("k", {a: 1}, 2)"#,
        r#""piped" | platform_db_store("k")"#,
        "platform_db_delete()",
        r#"platform_db_delete(1)"#,
        r#"platform_db_delete("k", "extra")"#,
        r#""piped" | platform_db_delete("k")"#,
        r#"platform_db_compare("a")"#,
        r#"platform_db_compare(1, "b")"#,
        "platform_db_import(42)",
        "platform_db_import()",
        r#"platform_db_import("not json and not a file")"#,
    ];
    for expr in cases {
        assert_eq!(home.refusal_code(expr), "E_BAD_ARG", "{expr}");
        assert_eq!(home.read_store(), before, "{expr} changed the store");
    }
}

#[test]
fn a_corrupt_store_is_refused_and_never_rewritten() {
    let home = Home::new("corrupt");
    home.json("platform_db_init()");
    let import = home.root.join("import.json");
    std::fs::write(&import, r#"{"gamma": {"os": "linux"}}"#).unwrap();
    std::fs::write(home.store(), "not json").unwrap();

    let cases = [
        r#"platform_db_store("k", {os: "linux"})"#.to_string(),
        r#"platform_db_delete("k")"#.to_string(),
        format!(r#"platform_db_import("{}")"#, slashed(&import)),
        r#"platform_db_compare("k", "k")"#.to_string(),
    ];
    for expr in cases {
        assert_eq!(home.refusal_code(&expr), "E_BAD_STATE", "{expr}");
        assert_eq!(
            home.read_store(),
            "not json",
            "{expr} rewrote the corrupt store"
        );
    }
}
