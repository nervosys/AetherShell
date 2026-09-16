//! `ErrorCode`'s own documentation makes a promise, and two failures broke it.
//!
//! > Every uncoded failure lands here rather than escaping as bare prose, so
//! > *every* failure is branchable on `.code`.
//!
//! Measuring ten induced failures against bash, PowerShell and nushell
//! (`benches/agentic/errors.mjs`) found the exceptions:
//!
//!   * **Parse errors carried no code at all.** They arrived as
//!     `error: found 1 error(s): unexpected token Eof at line 1, column 27`.
//!     It was the one failure in ten where AetherShell had nothing for an
//!     agent to switch on, and nushell — which codes all ten — was ahead of us
//!     because of it.
//!   * **The `sh()` refusal was filed under `E_UNKNOWN`**, with the hint
//!     "failed without a specific error code; inspect the message rather than
//!     retrying the same call". That is backwards for the most
//!     security-relevant refusal the shell makes: the cause is known exactly,
//!     and `E_UNKNOWN` is the one code an agent is told not to reason about.
//!
//! `ParseError` deliberately lives in `parser` rather than in `safety`:
//! `safety` is `#[cfg(feature = "native")]` and the parser is not, so building
//! the coded error out of `SafetyError` would make the shell's error contract
//! differ between the native and wasm builds. It renders the same JSON, so an
//! agent cannot tell them apart.

use aethershell::parser::{parse_program, ParseError};

/// Environment mutation is process-global, so the two tests that set
/// AETHER_ALLOW_SH must not overlap. A file-local mutex is the right scope:
/// other test binaries are separate processes and cannot see this one's
/// environment. Serialising with `--test-threads=1` instead would weaken every
/// other test in the file for the sake of two.
#[cfg(feature = "native")]
static SH_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn parse_failure(src: &str) -> anyhow::Error {
    parse_program(src).expect_err("this source must not parse")
}

// ── parse errors ────────────────────────────────────────────────────────

#[test]
fn a_parse_failure_is_a_parse_error_not_bare_prose() {
    let e = parse_failure("[1,2,3] | map(fn(x) => x *");
    assert!(
        e.downcast_ref::<ParseError>().is_some(),
        "a parse failure must be downcastable to ParseError, got: {e}"
    );
}

#[test]
fn a_lexer_failure_carries_the_code_too() {
    // The first version of this fix wrapped only the parser's own diagnostics.
    // `lex` runs before them and returns early, so `let x = @` still escaped as
    // bare prose — "unknown character '@' at line 1, column 9" — and the path
    // that most obviously needs a code did not have one. This test is how that
    // was noticed, about four minutes after the fix was declared finished.
    let e = parse_failure("let x = @");
    let pe = e
        .downcast_ref::<ParseError>()
        .unwrap_or_else(|| panic!("a lexer failure escaped uncoded: {e}"));
    assert!(
        pe.message.contains("unknown character"),
        "the lexer's diagnostic was lost: {}",
        pe.message
    );
}

#[test]
fn the_rendered_form_is_the_same_json_shape_every_other_failure_uses() {
    let text = parse_failure("let x = @").to_string();
    for fragment in [
        r#""code":"E_PARSE""#,
        r#""retryable":true"#,
        r#""message":"#,
        r#""hint":"#,
    ] {
        assert!(
            text.contains(fragment),
            "the structured form is missing {fragment}: {text}"
        );
    }
}

#[test]
fn the_message_survives_into_the_structured_form() {
    // The diagnostics are what make the error actionable; wrapping must not
    // swallow the line and column the parser worked out.
    let text = parse_failure("let a = 1\nlet b = 2\nlet c = @").to_string();
    assert!(text.contains("line 3"), "the line number was lost: {text}");
    assert!(text.contains("column"), "the column was lost: {text}");
}

#[test]
fn the_json_stays_parseable_when_the_message_contains_quotes_and_newlines() {
    // Multiple diagnostics are joined with newlines, and a diagnostic quotes
    // the offending character. Both must be escaped or the structured form an
    // agent parses is malformed.
    //
    // (An unterminated string literal is *not* a useful case here: the lexer
    // runs it to end of input, so `let a = "oops` parses successfully as a
    // multi-line string.)
    let text = parse_failure("let a = @\nlet b = @\nlet c = @").to_string();
    assert!(
        !text.contains('\n'),
        "a raw newline escaped into the JSON: {text}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("not valid JSON: {e}\n{text}"));
    assert_eq!(parsed["error"]["code"], "E_PARSE");
    assert!(
        parsed["error"]["message"]
            .as_str()
            .is_some_and(|m| !m.is_empty()),
        "the message came through empty: {text}"
    );
}

#[test]
fn a_parse_error_is_retryable_because_rewriting_the_source_can_fix_it() {
    let parsed: serde_json::Value =
        serde_json::from_str(&parse_failure("fn (").to_string()).expect("valid JSON");
    assert_eq!(parsed["error"]["retryable"], true);
    assert!(
        parsed["error"]["hint"]
            .as_str()
            .is_some_and(|h| h.contains("nothing ran")),
        "the hint should say the source never reached the evaluator"
    );
}

// ── the sh() gate ───────────────────────────────────────────────────────

#[cfg(feature = "native")]
#[test]
fn the_sh_refusal_is_a_policy_denial_not_an_unknown_failure() {
    use aethershell::safety::SafetyError;

    let _guard = SH_ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("AETHER_ALLOW_SH");

    let e = aethershell::security::validate_sh_allowed()
        .expect_err("sh must be refused with AETHER_ALLOW_SH unset");
    let se = e
        .downcast_ref::<SafetyError>()
        .unwrap_or_else(|| panic!("the sh refusal is not structured: {e}"));

    assert_eq!(se.code.as_str(), "E_POLICY_DENY", "refusal: {}", se.message);
    assert!(
        !se.code.retryable(),
        "a policy refusal must not invite a retry"
    );
    assert!(
        se.hint.contains("AETHER_ALLOW_SH"),
        "the hint must name the switch that changes the answer: {}",
        se.hint
    );
    assert!(
        !se.hint.contains("without a specific error code"),
        "still falling through to the generic E_UNKNOWN hint: {}",
        se.hint
    );
}

#[cfg(feature = "native")]
#[test]
fn the_sh_gate_still_opens_when_it_is_told_to() {
    let _guard = SH_ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("AETHER_ALLOW_SH", "true");
    let allowed = aethershell::security::validate_sh_allowed().is_ok();
    std::env::remove_var("AETHER_ALLOW_SH");
    assert!(allowed, "AETHER_ALLOW_SH=true must still enable sh()");
}

// ── non-vacuity ─────────────────────────────────────────────────────────

#[test]
fn non_vacuity_valid_source_parses_and_the_downcast_discriminates() {
    // If `parse_program` failed on everything, every test above passes for the
    // wrong reason.
    assert!(
        parse_program("[1, 2, 3] | map(fn(x) => x * 2)").is_ok(),
        "valid source no longer parses; the failure tests prove nothing"
    );

    // And the downcast must not match an unrelated error, or "is it a
    // ParseError" is answering yes to everything.
    let unrelated = anyhow::anyhow!("not a parse failure");
    assert!(
        unrelated.downcast_ref::<ParseError>().is_none(),
        "the downcast matches errors that are not parse failures"
    );
}
