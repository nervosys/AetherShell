//! Contracts for the line-based prompts: input_form, input_table and
//! input_autocomplete.
//!
//! The binary is driven as a user would drive it: `ish -c '<expr>'` with the
//! answers piped on stdin and stdin then closed. Every child has a hard
//! deadline, so a prompt that waits for more input than the test supplies
//! fails the test instead of hanging it.
//!
//! Argument refusals fail before the first prompt, so they are checked
//! in-process, where no stdin is read.

use iron_shell::builtins::{call, call_with_input};
use iron_shell::env::Env;
use iron_shell::value::Value;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Longest a child may run before the test kills it and fails.
const CHILD_DEADLINE: Duration = Duration::from_secs(30);

/// Runs `ish -c expr` with `answers` as its stdin, then closes stdin.
/// Returns (stdout, stderr), both trimmed. Output goes to files, so a child
/// that writes a lot cannot block on a full pipe.
fn run_ae(expr: &str, answers: &str) -> (String, String) {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir: PathBuf = std::env::temp_dir().join(format!("ae_forms_{}_{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let stdout_path = dir.join("stdout.txt");
    let stderr_path = dir.join("stderr.txt");

    let mut child = Command::new(env!("CARGO_BIN_EXE_ish"))
        .arg("-c")
        .arg(expr)
        .env("IRONSHELL_SESSION_DIR", dir.join("session"))
        .stdin(Stdio::piped())
        .stdout(std::fs::File::create(&stdout_path).expect("stdout file"))
        .stderr(std::fs::File::create(&stderr_path).expect("stderr file"))
        .spawn()
        .expect("spawn ae");

    {
        let mut stdin = child.stdin.take().expect("child stdin");
        // A child that exits before reading everything closes the pipe; the
        // write error is the expected outcome then, not a test failure.
        let _ = stdin.write_all(answers.as_bytes());
        // Dropping stdin here closes it, so a read past the answers is EOF.
    }

    let started = Instant::now();
    loop {
        match child.try_wait().expect("poll ae") {
            Some(_) => break,
            None if started.elapsed() > CHILD_DEADLINE => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("ish -c {expr:?} ran past {CHILD_DEADLINE:?} with answers {answers:?}");
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }

    let stdout = std::fs::read_to_string(&stdout_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    (stdout.trim().to_string(), stderr.trim().to_string())
}

fn rec(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

/// The coded error an in-process refusal carries, e.g. `E_BAD_ARG`.
fn code_of(e: &anyhow::Error) -> String {
    let text = e.to_string();
    match text.find("E_") {
        Some(i) => text[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {text}"),
    }
}

fn direct(name: &str, args: Vec<Value>) -> anyhow::Error {
    let mut env = Env::new();
    match call(name, args, &mut env) {
        Ok(v) => panic!("{name} accepted a call it should refuse, and answered {v:?}"),
        Err(e) => e,
    }
}

// ---------------------------------------------------------------------------
// input_form
// ---------------------------------------------------------------------------

#[test]
fn form_takes_the_default_on_an_empty_answer() {
    let (out, _) = run_ae(
        r#"input_form([{name: "city", default: "Oslo"}]).city"#,
        "\n",
    );
    assert_eq!(out, "Oslo");
}

#[test]
fn form_reprompts_a_required_field_then_takes_the_answer() {
    let (out, err) = run_ae(
        r#"input_form([{name: "id", label: "Ticket", required: true}]).id"#,
        "\n\n42\n",
    );
    assert_eq!(out, "42");
    // The trailing space of the last prompt is trimmed away with the output.
    assert_eq!(err.matches("Ticket:").count(), 3, "stderr: {err:?}");
}

#[test]
fn form_refuses_a_required_field_left_empty_three_times() {
    let (out, err) = run_ae(
        r#"(try { input_form([{name: "id", required: true}]).id } catch e { e.error.code })"#,
        "\n\n\n",
    );
    assert_eq!(out, "E_BAD_ARG", "stderr: {err}");
    assert!(
        err.contains("id"),
        "the refusal should name the field: {err}"
    );
}

#[test]
fn form_answers_every_field_and_optional_empty_is_empty_string() {
    let (out, _) = run_ae(
        r#"input_form([{name: "host"}, {name: "port", default: "22"}, {name: "user"}]).port"#,
        "example.org\n\n\n",
    );
    assert_eq!(out, "22");
}

#[test]
fn form_eof_mid_form_is_the_no_ui_coded_error() {
    let (out, err) = run_ae(
        r#"(try { input_form([{name: "one", required: true}, {name: "two", required: true}]) } catch e { e.error.code })"#,
        "first\n",
    );
    assert_eq!(out, "E_NO_UI", "stderr: {err}");
}

// ---------------------------------------------------------------------------
// input_table
// ---------------------------------------------------------------------------

#[test]
fn table_chosen_row_two_is_the_full_original_row() {
    let (out, err) = run_ae(
        r#"input_table([{a: 1, b: "x"}, {a: 2, b: "y", c: true}]).c"#,
        "2\n",
    );
    assert_eq!(out, "true", "stderr: {err}");
    assert!(
        err.contains("| c"),
        "the table lists the union of keys: {err}"
    );
}

#[test]
fn table_row_answer_keeps_every_key_of_the_row() {
    let (out, _) = run_ae(
        r#"input_table([{a: 1, b: "x"}, {a: 2, b: "y", c: true}]) | keys | len"#,
        "2\n",
    );
    assert!(out.contains('3'), "expected three keys, got {out}");
}

#[test]
fn table_refuses_after_three_out_of_range_replies() {
    let (out, err) = run_ae(
        r#"(try { input_table([{a: 1}, {a: 2}]) } catch e { e.error.code })"#,
        "0\n9\nzz\n",
    );
    assert_eq!(out, "E_BAD_ARG", "stderr: {err}");
}

#[test]
fn table_custom_prompt_is_the_question_asked() {
    let (out, err) = run_ae(r#"input_table([{a: 1}], "Which? ").a"#, "1\n");
    assert_eq!(out, "1");
    assert!(err.contains("Which?"), "stderr: {err}");
}

// ---------------------------------------------------------------------------
// input_autocomplete
// ---------------------------------------------------------------------------

const FRUIT: &str = r#"["apple", "apricot", "banana"]"#;

#[test]
fn autocomplete_exact_match_wins() {
    let (out, _) = run_ae(
        &format!(r#"input_autocomplete("Fruit: ", {FRUIT})"#),
        "apple\n",
    );
    assert_eq!(out, "apple");
}

#[test]
fn autocomplete_unique_prefix_ignores_case() {
    let (out, _) = run_ae(
        &format!(r#"input_autocomplete("Fruit: ", {FRUIT})"#),
        "BAN\n",
    );
    assert_eq!(out, "banana");
}

#[test]
fn autocomplete_ambiguous_prefix_then_refined_answers() {
    let (out, err) = run_ae(
        &format!(r#"input_autocomplete("Fruit: ", {FRUIT})"#),
        "ap\napr\n",
    );
    assert_eq!(out, "apricot", "stderr: {err}");
    assert!(
        err.contains("2 matches"),
        "the ambiguous prefix lists its matches: {err}"
    );
}

#[test]
fn autocomplete_refuses_after_five_answers_that_match_nothing() {
    let (out, err) = run_ae(
        &format!(
            r#"(try {{ input_autocomplete("Fruit: ", {FRUIT}) }} catch e {{ e.error.code }})"#
        ),
        "x\nx\nx\nx\nx\n",
    );
    assert_eq!(out, "E_BAD_ARG", "stderr: {err}");
    assert!(err.contains("no match"), "stderr: {err}");
}

// ---------------------------------------------------------------------------
// Argument refusals: in-process, before any prompt is printed or read.
// ---------------------------------------------------------------------------

#[test]
fn form_argument_shapes_refuse_as_bad_arg() {
    let field = |pairs: &[(&str, Value)]| rec(pairs);
    let cases: Vec<Vec<Value>> = vec![
        vec![],                                                  // no argument
        vec![Value::Array(vec![])],                              // zero fields
        vec![s("name")],                                         // not an array
        vec![Value::Array(vec![s("name")])],                     // field not a record
        vec![Value::Array(vec![field(&[])])],                    // no name
        vec![Value::Array(vec![field(&[("name", s("1bad"))])])], // name pattern
        vec![Value::Array(vec![field(&[("name", s(""))])])],     // empty name
        vec![Value::Array(vec![field(&[("name", Value::Int(3))])])],
        vec![Value::Array(vec![
            field(&[("name", s("a"))]),
            field(&[("name", s("a"))]),
        ])], // duplicate name
        vec![Value::Array(vec![field(&[
            ("name", s("a")),
            ("hint", s("x")),
        ])])], // unknown key
        vec![Value::Array(vec![field(&[
            ("name", s("a")),
            ("label", Value::Int(1)),
        ])])],
        vec![Value::Array(vec![field(&[
            ("name", s("a")),
            ("required", s("yes")),
        ])])],
        vec![Value::Array(vec![field(&[
            ("name", s("a")),
            ("default", s(&"x".repeat(1025))),
        ])])], // default over 1 KiB
        vec![Value::Array(
            (0..51)
                .map(|i| field(&[("name", s(&format!("f{i}")))]))
                .collect(),
        )], // 51 fields
        vec![Value::Array(vec![]), s("extra")], // surplus argument
    ];
    for args in cases {
        let e = direct("input_form", args.clone());
        assert_eq!(code_of(&e), "E_BAD_ARG", "input_form({args:?}) -> {e}");
    }
}

#[test]
fn form_refuses_piped_input() {
    let mut env = Env::new();
    let args = vec![Value::Array(vec![rec(&[("name", s("a"))])])];
    let r = call_with_input("input_form", args, Some(s("piped")), &mut env);
    let e = r.expect_err("piped input must be refused");
    assert_eq!(code_of(&e), "E_BAD_ARG");
}

#[test]
fn table_argument_shapes_refuse_as_bad_arg() {
    let cases: Vec<Vec<Value>> = vec![
        vec![],                                  // no argument
        vec![s("rows")],                         // not an array
        vec![Value::Array(vec![])],              // zero rows
        vec![Value::Array(vec![Value::Int(1)])], // row not a record
        vec![
            Value::Array(vec![rec(&[("a", Value::Int(1))])]),
            Value::Int(2),
        ], // prompt not a string
        vec![
            Value::Array(vec![rec(&[("a", Value::Int(1))])]),
            s("p"),
            s("extra"),
        ], // surplus
        vec![Value::Array(
            (0..1001).map(|i| rec(&[("n", Value::Int(i))])).collect(),
        )], // 1001 rows
    ];
    for args in cases {
        let e = direct("input_table", args.clone());
        assert_eq!(code_of(&e), "E_BAD_ARG", "input_table({args:?}) -> {e}");
    }
}

#[test]
fn table_refuses_piped_input() {
    let mut env = Env::new();
    let args = vec![Value::Array(vec![rec(&[("a", Value::Int(1))])])];
    let e = call_with_input("input_table", args, Some(s("piped")), &mut env)
        .expect_err("piped input must be refused");
    assert_eq!(code_of(&e), "E_BAD_ARG");
}

#[test]
fn autocomplete_argument_shapes_refuse_as_bad_arg() {
    let choices = |v: Vec<Value>| Value::Array(v);
    let cases: Vec<Vec<Value>> = vec![
        vec![s("p")],                                      // missing choices
        vec![Value::Int(1), choices(vec![s("a")])],        // prompt not a string
        vec![s("p"), s("a")],                              // choices not an array
        vec![s("p"), choices(vec![])],                     // zero choices
        vec![s("p"), choices(vec![Value::Int(1)])],        // choice not a string
        vec![s("p"), choices(vec![s("")])],                // empty choice
        vec![s("p"), choices(vec![s(&"x".repeat(1025))])], // choice over 1 KiB
        vec![s("p"), choices(vec![s("a"), s("a")])],       // duplicate
        vec![s("p"), choices(vec![s("a")]), s("extra")],   // surplus
        vec![
            s("p"),
            choices((0..10001).map(|i| s(&format!("c{i}"))).collect()),
        ], // too many
    ];
    for args in cases {
        let e = direct("input_autocomplete", args.clone());
        assert_eq!(
            code_of(&e),
            "E_BAD_ARG",
            "input_autocomplete({args:?}) -> {e}"
        );
    }
}

#[test]
fn autocomplete_refuses_piped_input() {
    let mut env = Env::new();
    let args = vec![s("p"), Value::Array(vec![s("a")])];
    let e = call_with_input("input_autocomplete", args, Some(s("piped")), &mut env)
        .expect_err("piped input must be refused");
    assert_eq!(code_of(&e), "E_BAD_ARG");
}
