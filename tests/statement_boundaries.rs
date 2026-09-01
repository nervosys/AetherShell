//! A statement must not swallow the statement after it.
//!
//! AetherShell supports word-calls: `print "hi"` parses as `print("hi")`. The
//! parser has two places that slurp space-separated arguments, and only one of
//! them checked that the arguments were on the same line as the callee. So a
//! bare identifier statement consumed whatever followed it:
//!
//! ```text
//! let hi = 1
//! hi
//! print("second")
//! ```
//!
//! parsed as `hi(print, "second")` and failed with `unknown builtin: hi`. The
//! silent form was worse than the loud one: when the following call had a
//! single argument the program simply did something else, and when it had two
//! the error was `expected ')'` pointing at a comma on a line the author had
//! no reason to suspect.

use aethershell::parser::parse_program;

fn parses(src: &str) -> bool {
    parse_program(src).is_ok()
}

/// How many top-level statements the parser found. The bug showed up here as a
/// count that was too low — two source statements collapsing into one.
fn statement_count(src: &str) -> usize {
    parse_program(src).map(|s| s.len()).unwrap_or(0)
}

#[test]
fn a_bare_identifier_does_not_consume_the_next_statement() {
    let src = "let hi = 1\nhi\nprint(\"second\")\n";
    assert!(parses(src), "should parse: {src:?}");
    assert_eq!(
        statement_count(src),
        3,
        "expected three statements; a lower count means `hi` swallowed the \
         `print` on the following line"
    );
}

#[test]
fn a_word_call_does_not_consume_the_next_statement() {
    // Two arguments on the next line is the shape that produced the confusing
    // "expected ')'" at the comma.
    let src = "print \"hi\"\ntype_of(\"x\", [1])\n";
    assert!(parses(src), "should parse: {src:?}");
    assert_eq!(statement_count(src), 2);
}

#[test]
fn a_word_call_still_takes_arguments_on_its_own_line() {
    // The fix must not disable word-calls, which is the whole point of the
    // syntax.
    assert_eq!(statement_count("print \"hello\"\n"), 1);
    assert_eq!(statement_count("print \"a\" \"b\"\n"), 1);
    assert!(parses("print \"hi\" | type_of\n"));
}

#[test]
fn consecutive_word_calls_stay_separate() {
    let src = "print \"one\"\nprint \"two\"\nprint \"three\"\n";
    assert_eq!(
        statement_count(src),
        3,
        "consecutive word-calls collapsed into fewer statements"
    );
}

#[test]
fn a_word_call_argument_may_not_come_from_the_next_line() {
    // `print` on its own is a complete statement; the string below belongs to
    // the next one. Whatever the parser does with this, it must not treat the
    // two lines as a single call.
    let src = "print\n\"orphan\"\n";
    assert_eq!(
        statement_count(src),
        2,
        "the string on line 2 was absorbed as an argument to line 1"
    );
}
