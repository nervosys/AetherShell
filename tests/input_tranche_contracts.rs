//! Contracts for the input_* builtins that read one answer from the terminal:
//! input_read_line, input_read_password, input_confirm, input_select,
//! input_multiselect, input_number, input_date and input_editor.
//!
//! Argument and pipeline refusals are checked in-process, where a refusal that
//! reads stdin would block the test, so each of those calls must refuse before
//! it reads. Answers are checked by driving the real `ae` binary with `-c`, the
//! answers piped on stdin and stdin then closed. Every child has a hard
//! deadline of 30 seconds, so a prompt that waits for more input than the test
//! supplies fails the test instead of hanging it.
//!
//! input_editor is never launched here: without a terminal it refuses with
//! E_NO_UI before it runs anything, and that is the only path tested.

use aethershell::builtins::{call, call_with_input};
use aethershell::env::Env;
use aethershell::value::Value;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Longest a child may run before the test kills it and fails.
const CHILD_DEADLINE: Duration = Duration::from_secs(30);

/// What the child finds on stdin.
enum Input {
    /// These bytes, then end of input.
    Piped(&'static str),
    /// End of input at once.
    Eof,
}

struct Run {
    stdout: String,
    stderr: String,
    success: bool,
}

/// Runs `ae -c expr` with `input` on stdin, under the deadline.
fn run_ae(expr: &str, input: Input, tag: &str) -> Run {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("ae_input_tranche_{}_{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let stdout_path = dir.join("stdout.txt");
    let stderr_path = dir.join("stderr.txt");

    let mut child = Command::new(env!("CARGO_BIN_EXE_ae"))
        .arg("-c")
        .arg(expr)
        .env("AETHER_SESSION_DIR", dir.join("session"))
        .stdin(match input {
            Input::Piped(_) => Stdio::piped(),
            Input::Eof => Stdio::null(),
        })
        .stdout(std::fs::File::create(&stdout_path).expect("stdout file"))
        .stderr(std::fs::File::create(&stderr_path).expect("stderr file"))
        .spawn()
        .expect("spawn ae");

    if let Input::Piped(answers) = input {
        let mut stdin = child.stdin.take().expect("child stdin");
        // A child that exits before reading everything closes the pipe; the
        // write error is the expected outcome then, not a test failure.
        let _ = stdin.write_all(answers.as_bytes());
        // Dropping stdin here closes it, so a read past the answers is EOF.
    }

    let started = Instant::now();
    let status = loop {
        match child.try_wait().expect("poll ae") {
            Some(status) => break status,
            None if started.elapsed() > CHILD_DEADLINE => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("ae -c {expr:?} ran past {CHILD_DEADLINE:?}");
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    Run {
        stdout: std::fs::read_to_string(&stdout_path)
            .unwrap_or_default()
            .trim_end()
            .to_string(),
        stderr: std::fs::read_to_string(&stderr_path).unwrap_or_default(),
        success: status.success(),
    }
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

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

/// A direct call that must refuse; it answers the coded error.
fn direct(name: &str, args: Vec<Value>) -> anyhow::Error {
    let mut env = Env::new();
    match call(name, args, &mut env) {
        Ok(v) => panic!("{name} accepted a call it should refuse, and answered {v:?}"),
        Err(e) => e,
    }
}

/// A piped call that must refuse, which it does before reading anything.
fn piped(name: &str, args: Vec<Value>) -> anyhow::Error {
    let mut env = Env::new();
    match call_with_input(name, args, Some(s("piped")), &mut env) {
        Ok(v) => panic!("{name} accepted piped input and answered {v:?}"),
        Err(e) => e,
    }
}

/// The coded refusal of each builtin's argument checks, all in-process.
#[test]
fn argument_refusals_are_coded_for_every_input_builtin() {
    let optional_one: &[&str] = &[
        "input_read_line",
        "input_read_password",
        "input_confirm",
        "input_number",
        "input_date",
    ];
    for name in optional_one {
        // Wrong type, and surplus arguments.
        let e = direct(name, vec![Value::Int(0)]);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{name}(0): {e}");
        let e = direct(name, vec![s("a"), s("b")]);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{name}(a, b): {e}");
        // Piped input is refused, not ignored.
        let e = piped(name, vec![]);
        assert_eq!(code_of(&e), "E_BAD_ARG", "piped {name}: {e}");
    }

    let needs_both: &[&str] = &["input_select", "input_multiselect"];
    for name in needs_both {
        let e = direct(name, vec![s("p")]);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{name}(prompt): {e}");
        let e = direct(name, vec![Value::Int(0), Value::Array(vec![s("a")])]);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{name}(0, options): {e}");
        let e = direct(name, vec![s("p"), Value::Array(vec![])]);
        assert_eq!(code_of(&e), "E_BAD_ARG", "{name} with no options: {e}");
        let e = direct(name, vec![s("p"), s("a")]);
        assert_eq!(
            code_of(&e),
            "E_BAD_ARG",
            "{name} with a String options: {e}"
        );
        let e = piped(name, vec![s("p"), Value::Array(vec![s("a")])]);
        assert_eq!(code_of(&e), "E_BAD_ARG", "piped {name}: {e}");
    }

    // Options: a Record, an empty String, over 1000 entries.
    let record = Value::Record(std::collections::BTreeMap::from([(
        "a".to_string(),
        Value::Int(1),
    )]));
    let e = direct("input_select", vec![s("p"), Value::Array(vec![record])]);
    assert_eq!(code_of(&e), "E_BAD_ARG", "a Record option: {e}");
    let e = direct("input_select", vec![s("p"), Value::Array(vec![s("")])]);
    assert_eq!(code_of(&e), "E_BAD_ARG", "an empty option: {e}");
    let many: Vec<Value> = (0..1001).map(Value::Int).collect();
    let e = direct("input_select", vec![s("p"), Value::Array(many.clone())]);
    assert_eq!(code_of(&e), "E_BAD_ARG", "1001 options: {e}");
    let e = direct("input_multiselect", vec![s("p"), Value::Array(many)]);
    assert_eq!(code_of(&e), "E_BAD_ARG", "1001 options, multi: {e}");

    let e = direct("input_editor", vec![Value::Int(0)]);
    assert_eq!(code_of(&e), "E_BAD_ARG", "input_editor(0): {e}");
    let e = direct("input_editor", vec![s("a"), s("b")]);
    assert_eq!(code_of(&e), "E_BAD_ARG", "input_editor(a, b): {e}");
    let e = piped("input_editor", vec![]);
    assert_eq!(code_of(&e), "E_BAD_ARG", "piped input_editor: {e}");
}

#[test]
fn argument_refusals_never_read_stdin() {
    // Stdin is at end of input, so a read would fail with E_NO_UI instead of
    // E_BAD_ARG; an E_BAD_ARG here proves the refusal came first.
    let cases = [
        r#"(try { input_read_line(0) } catch e { e.error.code })"#,
        r#"(try { "x" | input_read_line() } catch e { e.error.code })"#,
        r#"(try { input_read_password("a", "b") } catch e { e.error.code })"#,
        r#"(try { input_confirm(0) } catch e { e.error.code })"#,
        r#"(try { input_select("Pick", []) } catch e { e.error.code })"#,
        r#"(try { input_select(0, ["a"]) } catch e { e.error.code })"#,
        r#"(try { input_multiselect("Pick", [{a: 1}]) } catch e { e.error.code })"#,
        r#"(try { input_number("a", "b") } catch e { e.error.code })"#,
        r#"(try { input_date(0) } catch e { e.error.code })"#,
        r#"(try { input_editor(0) } catch e { e.error.code })"#,
        r#"(try { input_editor("a", "b") } catch e { e.error.code })"#,
    ];
    for expr in cases {
        let r = run_ae(expr, Input::Eof, "refusals");
        assert!(r.success, "`{expr}` stderr: {}", r.stderr);
        assert_eq!(r.stdout, "E_BAD_ARG", "`{expr}`");
    }
}

#[test]
fn argument_refusal_comes_before_the_no_terminal_refusal() {
    // With piped stdin there is no terminal, so input_editor's E_NO_UI would
    // answer if the argument check came later. It must not.
    let r = run_ae(
        r#"(try { input_editor(0) } catch e { e.error.code })"#,
        Input::Piped("x\n"),
        "editor_args_first",
    );
    assert!(r.success, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, "E_BAD_ARG");
}

#[test]
fn input_editor_without_a_terminal_is_no_ui_and_runs_nothing() {
    for input in [Input::Eof, Input::Piped("typed\n")] {
        let r = run_ae(
            r#"(try { input_editor("draft") } catch e { e.error.code })"#,
            input,
            "editor_no_tty",
        );
        assert!(r.success, "stderr: {}", r.stderr);
        assert_eq!(r.stdout, "E_NO_UI");
    }
}

#[test]
fn read_line_answers_the_piped_line_as_typed() {
    let r = run_ae(
        r#"json.stringify(input_read_line(""))"#,
        Input::Piped("  spaced out  \r\n"),
        "read_line_ok",
    );
    assert!(r.success, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, r#""  spaced out  ""#);
}

#[test]
fn read_line_closed_stdin_is_no_ui() {
    let r = run_ae(
        r#"(try { input_read_line("Q? ") } catch e { e.error.code })"#,
        Input::Eof,
        "read_line_eof",
    );
    assert!(r.success, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, "E_NO_UI");
}

#[test]
fn read_password_answers_the_piped_line_keeping_every_character() {
    let r = run_ae(
        r#"json.stringify(input_read_password(""))"#,
        Input::Piped(" s3cret \n"),
        "password_ok",
    );
    assert!(r.success, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, r#"" s3cret ""#);
}

#[test]
fn read_password_closed_stdin_is_no_ui() {
    let r = run_ae(
        r#"(try { input_read_password("") } catch e { e.error.code })"#,
        Input::Eof,
        "password_eof",
    );
    assert!(r.success, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, "E_NO_UI");
}

#[test]
fn confirm_answers_yes_and_no_forms() {
    let yes = run_ae(r#"input_confirm("")"#, Input::Piped("YES\n"), "confirm_yes");
    assert!(yes.success, "stderr: {}", yes.stderr);
    assert_eq!(yes.stdout, "true");

    let no = run_ae(r#"input_confirm("")"#, Input::Piped("n\n"), "confirm_no");
    assert!(no.success, "stderr: {}", no.stderr);
    assert_eq!(no.stdout, "false");

    let default_no = run_ae(r#"input_confirm("")"#, Input::Piped("\n"), "confirm_empty");
    assert!(default_no.success, "stderr: {}", default_no.stderr);
    assert_eq!(default_no.stdout, "false");
}

#[test]
fn confirm_asks_again_then_refuses_an_unclear_answer() {
    let again = run_ae(
        r#"input_confirm("")"#,
        Input::Piped("maybe\ny\n"),
        "confirm_again",
    );
    assert!(again.success, "stderr: {}", again.stderr);
    assert_eq!(again.stdout, "true");

    let refused = run_ae(
        r#"(try { input_confirm("") } catch e { e.error.code })"#,
        Input::Piped("maybe\nmaybe\nmaybe\n"),
        "confirm_refused",
    );
    assert!(refused.success, "stderr: {}", refused.stderr);
    assert_eq!(refused.stdout, "E_BAD_ARG");
}

#[test]
fn confirm_closed_stdin_is_no_ui() {
    let r = run_ae(
        r#"(try { input_confirm("") } catch e { e.error.code })"#,
        Input::Eof,
        "confirm_eof",
    );
    assert!(r.success, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, "E_NO_UI");
}

#[test]
fn select_answers_the_chosen_option_as_given() {
    let r = run_ae(
        r#"json.stringify(input_select("Pick", ["alpha", "beta"]))"#,
        Input::Piped("2\n"),
        "select_ok",
    );
    assert!(r.success, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, r#""beta""#);

    let typed = run_ae(
        r#"json.stringify(input_select("", [10, 20]))"#,
        Input::Piped("1\n"),
        "select_int",
    );
    assert!(typed.success, "stderr: {}", typed.stderr);
    assert_eq!(typed.stdout, "10");
}

#[test]
fn select_asks_again_then_refuses_invalid_choices() {
    let again = run_ae(
        r#"json.stringify(input_select("", ["alpha", "beta"]))"#,
        Input::Piped("9\nx\n1\n"),
        "select_again",
    );
    assert!(again.success, "stderr: {}", again.stderr);
    assert_eq!(again.stdout, r#""alpha""#);

    let refused = run_ae(
        r#"(try { input_select("", ["alpha", "beta"]) } catch e { e.error.code })"#,
        Input::Piped("0\nx\n9\n"),
        "select_refused",
    );
    assert!(refused.success, "stderr: {}", refused.stderr);
    assert_eq!(refused.stdout, "E_BAD_ARG", "must not answer null");
}

#[test]
fn select_closed_stdin_is_no_ui() {
    let r = run_ae(
        r#"(try { input_select("", ["a"]) } catch e { e.error.code })"#,
        Input::Eof,
        "select_eof",
    );
    assert!(r.success, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, "E_NO_UI");
}

#[test]
fn multiselect_answers_the_chosen_options_in_order() {
    let r = run_ae(
        r#"json.stringify(input_multiselect("Pick", ["alpha", "beta", "gamma"]))"#,
        Input::Piped("3, 1\n"),
        "multi_ok",
    );
    assert!(r.success, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, r#"["gamma","alpha"]"#);

    let repeats = run_ae(
        r#"json.stringify(input_multiselect("", ["alpha", "beta"]))"#,
        Input::Piped("2,2\n"),
        "multi_repeat",
    );
    assert!(repeats.success, "stderr: {}", repeats.stderr);
    assert_eq!(repeats.stdout, r#"["beta"]"#);

    let none = run_ae(
        r#"json.stringify(input_multiselect("", ["alpha"]))"#,
        Input::Piped("\n"),
        "multi_none",
    );
    assert!(none.success, "stderr: {}", none.stderr);
    assert_eq!(none.stdout, "[]");
}

#[test]
fn multiselect_asks_again_then_refuses_an_invalid_answer() {
    let again = run_ae(
        r#"json.stringify(input_multiselect("", ["alpha", "beta"]))"#,
        Input::Piped("x\n1\n"),
        "multi_again",
    );
    assert!(again.success, "stderr: {}", again.stderr);
    assert_eq!(again.stdout, r#"["alpha"]"#);

    let refused = run_ae(
        r#"(try { input_multiselect("", ["alpha", "beta"]) } catch e { e.error.code })"#,
        Input::Piped("1,9\n0\n1,,2\n"),
        "multi_refused",
    );
    assert!(refused.success, "stderr: {}", refused.stderr);
    assert_eq!(refused.stdout, "E_BAD_ARG", "must not answer []");
}

#[test]
fn multiselect_closed_stdin_is_no_ui() {
    let r = run_ae(
        r#"(try { input_multiselect("", ["a"]) } catch e { e.error.code })"#,
        Input::Eof,
        "multi_eof",
    );
    assert!(r.success, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, "E_NO_UI");
}

#[test]
fn number_answers_an_int_or_a_float() {
    let int = run_ae(r#"input_number("")"#, Input::Piped("42\n"), "number_int");
    assert!(int.success, "stderr: {}", int.stderr);
    assert_eq!(int.stdout, "42");

    let float = run_ae(r#"input_number("")"#, Input::Piped("2.5\n"), "number_float");
    assert!(float.success, "stderr: {}", float.stderr);
    assert_eq!(float.stdout, "2.5");
}

#[test]
fn number_asks_again_then_refuses_text_and_non_finite_values() {
    let again = run_ae(
        r#"input_number("")"#,
        Input::Piped("inf\nNaN\n7\n"),
        "number_again",
    );
    assert!(again.success, "stderr: {}", again.stderr);
    assert_eq!(again.stdout, "7");

    let refused = run_ae(
        r#"(try { input_number("") } catch e { e.error.code })"#,
        Input::Piped("abc\n-1e400\nNaN\n"),
        "number_refused",
    );
    assert!(refused.success, "stderr: {}", refused.stderr);
    assert_eq!(refused.stdout, "E_BAD_ARG", "must not answer null");
}

#[test]
fn number_closed_stdin_is_no_ui() {
    let r = run_ae(
        r#"(try { input_number("") } catch e { e.error.code })"#,
        Input::Eof,
        "number_eof",
    );
    assert!(r.success, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, "E_NO_UI");
}

#[test]
fn date_answers_a_real_calendar_date() {
    let r = run_ae(r#"input_date("")"#, Input::Piped("2028-02-29\n"), "date_ok");
    assert!(r.success, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, "2028-02-29");
}

#[test]
fn date_asks_again_then_refuses_what_is_not_a_date() {
    let again = run_ae(
        r#"input_date("")"#,
        Input::Piped("2026-1-5\n2026-02-03\n"),
        "date_again",
    );
    assert!(again.success, "stderr: {}", again.stderr);
    assert_eq!(again.stdout, "2026-02-03");

    let refused = run_ae(
        r#"(try { input_date("") } catch e { e.error.code })"#,
        Input::Piped("2026-02-30\n2026-13-01\nlast week\n"),
        "date_refused",
    );
    assert!(refused.success, "stderr: {}", refused.stderr);
    assert_eq!(refused.stdout, "E_BAD_ARG");
}

#[test]
fn date_closed_stdin_is_no_ui() {
    let r = run_ae(
        r#"(try { input_date("") } catch e { e.error.code })"#,
        Input::Eof,
        "date_eof",
    );
    assert!(r.success, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, "E_NO_UI");
}
