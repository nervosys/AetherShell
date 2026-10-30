//! Contracts for the bounded awk subset behind `awk` and `awk_process`.
//!
//! The ordinary tests run everywhere: they check the output of programs whose
//! results are derived by hand, the coded refusals for constructs outside the
//! subset, and the size and output budgets.
//!
//! The oracle test is `#[ignore]`d and runs only in the disposable distro, via
//! scripts/disposable_linux_tests.sh. It runs every corpus case through
//! `gawk --posix` (argv, no shell, input on stdin, LC_ALL=C) and through the
//! builtin, and requires identical stdout. Any difference is a bug in the
//! subset, because gawk --posix is the reference here.

#[cfg(target_os = "linux")]
#[path = "support/disposable.rs"]
mod disposable;

use iron_shell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use std::collections::BTreeMap;
#[cfg(target_os = "linux")]
use std::io::Write;
#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn with_fs(fs: &str) -> Value {
    Value::Record(BTreeMap::from([("fs".to_string(), s(fs))]))
}

fn lines(expected: &[&str]) -> Value {
    Value::Array(expected.iter().map(|line| s(line)).collect())
}

fn awk(program: &str, text: &str) -> anyhow::Result<Value> {
    let mut env = Env::default();
    call("awk_process", vec![s(program), s(text)], &mut env)
}

fn awk_fs(program: &str, text: &str, fs: &str) -> anyhow::Result<Value> {
    let mut env = Env::default();
    call(
        "awk_process",
        vec![s(program), s(text), with_fs(fs)],
        &mut env,
    )
}

fn code(result: anyhow::Result<Value>, expected: &str) {
    let e = result.unwrap_err();
    assert!(e.to_string().contains(expected), "{e}");
}

#[test]
fn awk_prints_fields_joined_by_the_output_separator() {
    assert_eq!(
        awk("{print $2, $1}", "a b\nc d\n").unwrap(),
        lines(&["b a", "d c"])
    );
    assert_eq!(
        awk("{print $1, NF}", "  a   b  \n\tc\n").unwrap(),
        lines(&["a 2", "c 1"])
    );
    assert_eq!(awk("{print NF}", "\n").unwrap(), lines(&["0"]));
}

#[test]
fn awk_splits_on_the_given_separator_literally_and_keeps_empty_fields() {
    assert_eq!(
        awk_fs("{print $2}", "x,y,z\n,,\nq\n", ",").unwrap(),
        lines(&["y", "", ""])
    );
    assert_eq!(
        awk_fs("{print \"[\" $2 \"]\"}", "a::b", ":").unwrap(),
        lines(&["[]"])
    );
    assert_eq!(
        awk_fs("{print NF}", "a::b:\n:\n", ":").unwrap(),
        lines(&["4", "2"])
    );
}

#[test]
fn awk_runs_begin_and_end_with_printf_averages() {
    assert_eq!(
        awk(
            r#"BEGIN{s=0} {s+=$1; n++} END{printf("%.2f\n", s/n)}"#,
            "1\n2\n4\n"
        )
        .unwrap(),
        lines(&["2.33"])
    );
    assert_eq!(
        awk("END{print NR, $0}", "x y\nlast line\n").unwrap(),
        lines(&["2 last line"])
    );
    assert_eq!(
        awk("BEGIN{FS=\":\"} {print $2}", "a:b\nc:d\n").unwrap(),
        lines(&["b", "d"])
    );
}

#[test]
fn awk_compares_strings_and_strnums_the_way_awk_does() {
    // String constants compare as strings; input fields that look numeric compare as numbers.
    assert_eq!(
        awk(r#"BEGIN{print ("10" < "9"), (10 < 9)}"#, "").unwrap(),
        lines(&["1 0"])
    );
    assert_eq!(awk("{print ($1 < $2)}", "10 9\n").unwrap(), lines(&["0"]));
    assert_eq!(
        awk("$1 == \"10\" {print \"eq\"}", "10\n10.0\n").unwrap(),
        lines(&["eq"])
    );
    assert_eq!(
        awk("$1 == 10 {print \"num\"}", "10\n10.0\n1e1\n").unwrap(),
        lines(&["num", "num", "num"])
    );
}

#[test]
fn awk_regex_patterns_use_ere_anchors_classes_and_alternation() {
    assert_eq!(
        awk("/^[a-c]+$/", "abc\nabd\nab\n").unwrap(),
        lines(&["abc", "ab"])
    );
    assert_eq!(
        awk("/x|1/", "xyz\n123\nabc\n").unwrap(),
        lines(&["xyz", "123"])
    );
    assert_eq!(
        awk("/[[:digit:]]+/ {print $0}", "ab1\nxyz\n22\n").unwrap(),
        lines(&["ab1", "22"])
    );
    assert_eq!(
        awk("/a.c/ {print \"dot\"}", "abc\na-c\nac\n").unwrap(),
        lines(&["dot", "dot"])
    );
    assert_eq!(awk("/\\./", "a.b\nab\n").unwrap(), lines(&["a.b"]));
}

#[test]
fn awk_string_functions_follow_awk_edge_cases() {
    assert_eq!(
        awk(
            r#"BEGIN{print substr("hello",2,3), substr("hello",0), substr("hello",-1,3), substr("hello",4,100), index("hello","ll"), index("hello","z"), length("hello"), length()}"#,
            ""
        )
        .unwrap(),
        lines(&["ell hello hel lo 3 0 5 0"])
    );
    assert_eq!(
        awk("{print length(), length($1)}", "abc de\n\n").unwrap(),
        lines(&["6 3", "0 0"])
    );
    assert_eq!(
        awk("BEGIN{print toupper(\"abC1\"), tolower(\"ABc\")}", "").unwrap(),
        lines(&["ABC1 abc"])
    );
}

#[test]
fn awk_uninitialized_values_and_concatenation_precedence() {
    assert_eq!(
        awk(r#"BEGIN{print x+0, "[" x "]", length(x)}"#, "").unwrap(),
        lines(&["0 [] 0"])
    );
    // Concatenation binds looser than +, tighter than comparison.
    assert_eq!(
        awk(r#"BEGIN{print "a" "b" == "ab", 1 2 + 3, 2 " " 3 * 4}"#, "").unwrap(),
        lines(&["1 15 2 12"])
    );
}

#[test]
fn awk_arithmetic_assignment_and_increments() {
    assert_eq!(
        awk(
            "BEGIN{x=5; x+=2; x*=3; x-=1; x/=2; print x, x++, ++x, x--}",
            ""
        )
        .unwrap(),
        lines(&["10 10 12 12"])
    );
    assert_eq!(
        awk("BEGIN{print 7 % 3, -7 % 3, 7 / 2, 2 * 3 - 1, -2 + 5}", "").unwrap(),
        lines(&["1 -1 3.5 5 3"])
    );
    assert_eq!(
        awk("BEGIN{x = \"3x\"; print x + 1, \"3x\" * 2}", "").unwrap(),
        lines(&["4 6"])
    );
}

#[test]
fn awk_printf_supports_the_fixed_conversion_set() {
    assert_eq!(
        awk(
            r#"BEGIN{printf("%d|%i|%x|%c|%s|%.1f|%%\n", 42.9, -3.7, 255, 65, "s", 2.26)}"#,
            ""
        )
        .unwrap(),
        lines(&["42|-3|ff|A|s|2.3|%"])
    );
    assert_eq!(
        awk(r#"BEGIN{print 1e6, 1234567, 0.1+0.2, 100/3, 1/4}"#, "").unwrap(),
        lines(&["1000000 1234567 0.3 33.3333 0.25"])
    );
}

#[test]
fn awk_if_else_and_field_and_nf_assignment_rebuild_the_record() {
    assert_eq!(
        awk(
            "{ if ($1 > 5) print \"big\"; else print \"small\" }",
            "7\n3\n"
        )
        .unwrap(),
        lines(&["big", "small"])
    );
    assert_eq!(
        awk("{ $2 = \"X\"; print }", "a b c\none\n").unwrap(),
        lines(&["a X c", "one X"])
    );
    assert_eq!(
        awk("{ NF = 2; print }", "a b c d\n").unwrap(),
        lines(&["a b"])
    );
    assert_eq!(
        awk("{ print $NF, $(NF-1) }", "a b c\nonly\n").unwrap(),
        lines(&["c b", "only only"])
    );
}

#[test]
fn awk_logical_and_negated_patterns() {
    assert_eq!(awk("/a/ && /b/", "ab\na\nb\n").unwrap(), lines(&["ab"]));
    assert_eq!(awk("!/x/", "x\ny\n").unwrap(), lines(&["y"]));
    assert_eq!(
        awk("$1 ~ /^[0-9]+$/ {print \"num\", $1}", "12\nab\n3x\n").unwrap(),
        lines(&["num 12"])
    );
    assert_eq!(awk("$1 !~ /^a/", "ab\nbc\n").unwrap(), lines(&["bc"]));
}

#[test]
fn awk_pipeline_form_takes_the_text_from_the_pipe() {
    let mut env = Env::default();
    assert_eq!(
        call_with_input(
            "awk_process",
            vec![s("{print $2}"), with_fs(",")],
            Some(s("a,b\nc,d")),
            &mut env
        )
        .unwrap(),
        lines(&["b", "d"])
    );
    code(
        call_with_input(
            "awk_process",
            vec![s("{print}"), with_fs(","), s("extra")],
            Some(s("a,b")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
}

#[test]
fn awk_refuses_constructs_outside_the_subset_by_name() {
    let cases: &[(&str, &str)] = &[
        ("BEGIN{getline}", "getline"),
        ("{print > \"f\"}", "redirection"),
        ("{print | \"cat\"}", "pipe"),
        ("{x = split(\"a b\", a)}", "split"),
        ("{x = system(\"ls\")}", "system"),
        ("{print sprintf(\"%d\", 1)}", "sprintf"),
        ("{a[1] = 2}", "associative array"),
        ("{while (x) y}", "while"),
        ("function f(x) { return x }", "function"),
        ("{print foo(1)}", "user-defined function foo"),
        ("{x = 1 ? 2 : 3}", "conditional"),
        ("BEGIN{print 2^3}", "exponentiation"),
        ("{x = (1, 2)}", "expression list"),
        ("{printf(\"%5d\", 1)}", "width"),
        ("{printf(\"%e\", 1)}", "%e"),
        ("$1 ~ x", "dynamic regex"),
        ("1,2 {print}", "range pattern"),
        ("{print \"a\\qb\"}", "string escape"),
        ("{print 1/0}", "division by zero"),
        ("{print $-1}", "field index"),
        ("{print}}", "syntax"),
    ];
    for (program, named) in cases {
        let result = awk(program, "a\n");
        let e = match result {
            Ok(value) => panic!("{program:?} was accepted: {value:?}"),
            Err(e) => e,
        };
        let text = e.to_string();
        assert!(text.contains("E_BAD_ARG"), "{program:?}: {text}");
        assert!(
            text.contains(named),
            "{program:?} should name {named:?}: {text}"
        );
    }
}

#[test]
fn awk_refuses_an_empty_field_separator_and_empty_programs() {
    code(awk_fs("{print}", "a", ""), "E_BAD_ARG");
    code(awk("", "a"), "E_BAD_ARG");
}

#[test]
fn awk_empty_begin_block_is_a_valid_program_with_no_output() {
    assert_eq!(awk("BEGIN{}", "x\n").unwrap(), Value::Array(vec![]));
}

#[test]
fn awk_refuses_programs_over_64_kib() {
    let program = format!("BEGIN{{x=0}}{}", "x=1;".repeat(20_000));
    code(awk(&program, ""), "E_BUDGET_EXCEEDED");
}

#[test]
fn awk_stops_at_the_output_budget() {
    let input = "a\n".repeat(2_000_000);
    let program = format!("{{print \"{}\"}}", "0123456789".repeat(5));
    code(awk(&program, &input), "E_BUDGET_EXCEEDED");
}

#[test]
fn awk_stops_at_the_step_budget() {
    let input = "a\n".repeat(60_000);
    let program = format!("{{{}}}", "x++;".repeat(2_000));
    code(awk(&program, &input), "E_BUDGET_EXCEEDED");
}

/// A name used as a variable must be consumed by the parser. When it was not,
/// `BEGIN{x=5}` re-read `x` as a concatenation operand forever and the test
/// binary grew past 80 GB before the machine failed.
#[test]
fn awk_variables_parse_and_terminate() {
    assert_eq!(awk("BEGIN{x=5; print x}", "").unwrap(), lines(&["5"]));
    assert_eq!(
        awk("{n = NF} END{print n, NR}", "a b c\nd\n").unwrap(),
        lines(&["1 2"])
    );
    assert_eq!(awk("{print length}", "abc\n").unwrap(), lines(&["3"]));
    assert_eq!(
        awk("BEGIN{print length(\"abcd\")}", "").unwrap(),
        lines(&["4"])
    );
}

/// Doubling a string once per line takes few steps but exponential memory.
#[test]
fn awk_bounds_string_growth_that_the_step_budget_does_not() {
    let input = "x\n".repeat(64);
    code(
        awk("BEGIN{s=\"ab\"} {s = s s} END{print length(s)}", &input),
        "E_BUDGET_EXCEEDED",
    );
    let mut env = Env::default();
    code(
        call(
            "awk_process",
            vec![
                s("BEGIN{OFS=\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"; $1000000 = 1; print}"),
                s(""),
            ],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
}

#[test]
fn awk_refuses_programs_nested_past_the_depth_limit() {
    let parens = format!("BEGIN{{print {}1{}}}", "(".repeat(5_000), ")".repeat(5_000));
    code(awk(&parens, ""), "E_BUDGET_EXCEEDED");
    let negations = format!("BEGIN{{print {}1}}", "!".repeat(5_000));
    code(awk(&negations, ""), "E_BUDGET_EXCEEDED");
    let chain = format!("BEGIN{{print 1{}}}", " 1".repeat(5_000));
    code(awk(&chain, ""), "E_BUDGET_EXCEEDED");
    let blocks = format!("BEGIN{}{}", "{".repeat(5_000), "}".repeat(5_000));
    code(awk(&blocks, ""), "E_BUDGET_EXCEEDED");
}

/// (program, field separator passed as -F and options.fs, input). An empty
/// separator means the program sets FS itself, or uses the default.
#[cfg(target_os = "linux")]
const CORPUS: &[(&str, &str, &str)] = &[
    ("{print $2, $1}", "", "a b\nc d\n"),
    ("{print $1}", "", "  leading spaces  here\n\tTab first\n"),
    ("{print NF}", "", "  a  b  \n\n x\n"),
    ("{print NR \": \" $0}", "", "one\ntwo\nthree\n"),
    ("{print $2}", ",", "x,y,z\n,,\nq\n"),
    ("{print $1 \"|\" $3}", ":", "root:x:0\nbin:x:1:2:extra\n"),
    ("{print NF}", ":", "a::b:\n:\n"),
    ("BEGIN{s=0} {s+=$1} END{printf(\"%.2f\\n\", s/NR)}", "", "1\n2\n4\n"),
    ("BEGIN{s=0} {s+=$2} END{print s, NR}", "", "a 1.5\nb 2.25\n"),
    ("{ if ($1 < $2) print \"lt\"; else print \"ge\" }", "", "10 9\n9 10\n"),
    ("BEGIN{print (\"10\" < \"9\"), (10 < 9)}", "", ""),
    ("$1 == \"10\" {print \"eq\"}", "", "10\n10.0\n"),
    ("$1 == 10 {print \"num\"}", "", "10\n10.0\n1e1\n"),
    ("/^[a-c]+$/", "", "abc\nabd\nab\n\nc\n"),
    ("/x|1/", "", "xyz\n123\nabc\n1x\n"),
    ("/^$/ {print \"empty\"}", "", "a\n\nb\n"),
    ("/a.c/ {print \"dot\"}", "", "abc\na-c\nac\n"),
    ("/[[:digit:]]+/ {print $0}", "", "ab1\nxyz\n22\n"),
    ("/[^0-9]/", "", "123\n45a\n"),
    ("/a{2}/", "", "a\naa\naaa\n"),
    ("/^(ab|cd)+$/", "", "abcd\nabce\n"),
    ("/\\./", "", "a.b\nab\n"),
    ("BEGIN{print substr(\"hello\",2,3), substr(\"hello\",0), substr(\"hello\",-1,3), substr(\"hello\",4,100)}", "", ""),
    ("BEGIN{print index(\"hello\",\"ll\"), index(\"hello\",\"z\"), index(\"\",\"a\"), length(\"hello\"), length(\"\")}", "", ""),
    ("{print length(), length($1)}", "", "abc de\n\n"),
    ("{print substr($0, 2)}", "", "abcdef\nx\n"),
    ("BEGIN{print substr(\"hello\",0,2), substr(\"hello\",1.5,2), substr(\"hello\",2.6,2), substr(\"hello\",2,1.5), substr(\"hello\",3,0.5), substr(\"hello\",-3,5), substr(\"hello\",2,-1), substr(\"hello\",6), substr(\"hello\",0,1)}", "", ""),
    ("{print substr($0, 3, 2)}", "", "abcdef\nab\n"),
    ("{print index($0, \"b\")}", "", "abc\nxyz\n"),
    ("BEGIN{print toupper(\"abC1\"), tolower(\"ABc\")}", "", ""),
    ("BEGIN{print x+0, \"[\" x \"]\", length(x)}", "", ""),
    ("BEGIN{print \"a\" \"b\" == \"ab\", 1 2 + 3, 2 \" \" 3 * 4}", "", ""),
    ("BEGIN{x=5; x+=2; x*=3; x-=1; x/=2; print x, x++, ++x, x--}", "", ""),
    ("BEGIN{print 7 % 3, -7 % 3, 7 / 2, 2 * 3 - 1, -2 + 5}", "", ""),
    ("{$2 = \"X\"; print}", "", "a b c\none\n"),
    ("{NF = 2; print}", "", "a b c d\n"),
    ("{$5 = \"e\"; print; print NF}", "", "a b\n"),
    ("END{print NR, $0}", "", "x y\nlast line\n"),
    ("{ print $NF }", "", "a b c\nonly\n"),
    ("{ print $(NF-1) }", "", "a b c\nx y\n"),
    ("BEGIN{FS=\":\"} {print $2}", "", "a:b\nc:d\n"),
    ("BEGIN{OFS=\"-\"} {print $1, $2}", "", "a b\n"),
    ("{ if ($1 > 5) print \"big\"; else if ($1 > 2) print \"mid\"; else print \"small\" }", "", "7\n3\n1\n"),
    ("{ printf(\"%s=%d;\", $1, $2) } END { printf(\"\\n\") }", "", "a 1\nb 2.9\n"),
    ("{ printf(\"%c%c\\n\", 72, $1) }", "", "i\n"),
    ("BEGIN { printf(\"%x %%\\n\", 255) }", "", ""),
    ("BEGIN{printf(\"%.3f|%.0f|%f\\n\", 3.14159, 2.5, 1)}", "", ""),
    ("BEGIN{print 1e6, 1234567, 0.1+0.2, 100/3, 1/4}", "", ""),
    ("BEGIN{print 1e-5, 123456789.5}", "", ""),
    ("BEGIN { x = \"3x\"; print x + 1, \"3x\" * 2 }", "", ""),
    ("{ if ($2 == \"\") print \"nil\"; else print \"has\" }", "", "a\nb c\n"),
    ("BEGIN { if (!(3 > 2)) print \"no\"; else print \"yes\" }", "", ""),
    ("!/x/", "", "x\ny\n"),
    ("/a/ && /b/", "", "ab\na\nb\n"),
    ("/a/ || /z/", "", "a\nb\nz\nc\n"),
    ("$1 ~ /^[0-9]+$/ {print \"num\", $1}", "", "12\nab\n3x\n"),
    ("$1 !~ /^a/", "", "ab\nbc\n"),
    ("{print $2}", "", "a\tb c\n"),
    ("END { print \"n=\" NR }", "", ""),
    ("NR == 2", "", "a\nb\nc\n"),
    ("{ total += $1 } END { print total+0, total \"\" }", "", ""),
    ("BEGIN{}", "", "x\n"),
];

/// Runs `program` through gawk --posix, as argv with no shell.
#[cfg(target_os = "linux")]
fn gawk_posix(program: &str, fs: &str, input: &str) -> String {
    let mut cmd = Command::new("gawk");
    cmd.arg("--posix");
    if !fs.is_empty() {
        cmd.arg("-F").arg(fs);
    }
    cmd.arg(program)
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .expect("gawk must be installed in the disposable distro");
    if let Some(mut stdin) = child.stdin.take() {
        // gawk may exit without reading (BEGIN-only programs); a broken pipe is fine.
        let _ = stdin.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().expect("wait for gawk");
    assert!(
        out.status.success(),
        "gawk --posix rejected {program:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("gawk output is UTF-8")
}

/// Runs the same case through the builtin and rebuilds its stdout text.
#[cfg(target_os = "linux")]
fn ours(program: &str, fs: &str, input: &str) -> Result<String, String> {
    let mut env = Env::default();
    let mut args = vec![s(program), s(input)];
    if !fs.is_empty() {
        args.push(with_fs(fs));
    }
    match call("awk_process", args, &mut env) {
        Ok(Value::Array(rows)) => {
            let mut text = String::new();
            for row in rows {
                match row {
                    Value::Str(line) => {
                        text.push_str(&line);
                        text.push('\n');
                    }
                    other => return Err(format!("non-string output line {other:?}")),
                }
            }
            Ok(text)
        }
        Ok(other) => Err(format!("not an array: {other:?}")),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn awk_subset_matches_gawk_posix_on_corpus() {
    disposable::require();
    assert!(CORPUS.len() >= 40, "corpus has {} cases", CORPUS.len());
    let mut failures = Vec::new();
    for (program, fs, input) in CORPUS {
        let expected = gawk_posix(program, fs, input);
        match ours(program, fs, input) {
            Ok(actual) if actual == expected => {}
            Ok(actual) => failures.push(format!(
                "{program:?} fs={fs:?} input={input:?}\n  gawk: {expected:?}\n  ours: {actual:?}"
            )),
            Err(e) => failures.push(format!("{program:?} fs={fs:?}: {e}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from gawk --posix:\n{}",
        failures.len(),
        CORPUS.len(),
        failures.join("\n")
    );
}
