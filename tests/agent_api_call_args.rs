//! Tests for how the Agent API turns a `call` into code.
//!
//! Named arguments must reach the builtin in the position its declared
//! parameters give it, an unknown name must be refused with the valid names,
//! a string must stay a string whatever it contains, a parse error must be the
//! structured JSON the CLI reports, and two schema requests must be identical.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use iron_shell::agent_api::{process_request, AgentRequest, AgentResponse, SchemaFormat};
use serde_json::{json, Value as JsonValue};

/// Serialises the tests that set `IRONSHELL_SESSION_DIR`.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Holds the env lock, and points `IRONSHELL_SESSION_DIR` at a fresh directory
/// under the system temp dir for as long as it lives.
struct Sandbox {
    _guard: MutexGuard<'static, ()>,
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root =
            std::env::temp_dir().join(format!("ish-call-args-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("session")).expect("create session dir");
        std::env::set_var("IRONSHELL_SESSION_DIR", root.join("session"));
        Sandbox {
            _guard: guard,
            root,
        }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        std::env::remove_var("IRONSHELL_SESSION_DIR");
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn call(builtin: &str, args: JsonValue) -> AgentResponse {
    process_request(&AgentRequest::Call {
        builtin: builtin.to_string(),
        args,
    })
}

#[test]
fn named_args_reach_the_builtin_in_their_declared_positions() {
    let _sandbox = Sandbox::new("named");

    // `crypto_hmac(data, key, algorithm?)` is pure and returns lowercase hex.
    let named = call(
        "crypto_hmac",
        json!({"key": "secret", "data": "message", "algorithm": "sha512"}),
    );
    let positional = call("crypto_hmac", json!(["message", "secret", "sha512"]));
    assert!(named.success, "named call failed: {:?}", named.error);
    assert!(
        positional.success,
        "positional call failed: {:?}",
        positional.error
    );
    assert_eq!(named.result, positional.result);
    // sha512 hex is 128 characters, so the algorithm argument arrived.
    assert_eq!(
        named.result.as_ref().and_then(|v| v.as_str()).map(str::len),
        Some(128)
    );

    // The optional `algorithm` may be omitted when no later one is given.
    let default = call("crypto_hmac", json!({"data": "message", "key": "secret"}));
    let default_positional = call("crypto_hmac", json!(["message", "secret"]));
    assert!(default.success, "default call failed: {:?}", default.error);
    assert_eq!(default.result, default_positional.result);
    assert_ne!(
        default.result, named.result,
        "sha256 and sha512 must differ"
    );
}

#[test]
fn missing_required_named_argument_is_refused() {
    let _sandbox = Sandbox::new("missing");

    let response = call("crypto_hmac", json!({"key": "secret"}));
    assert!(!response.success);
    let error = response.error.unwrap_or_default();
    assert!(
        error.contains("missing required argument 'data'"),
        "{error}"
    );
}

#[test]
fn unknown_named_argument_is_refused_and_names_the_valid_ones() {
    let _sandbox = Sandbox::new("unknown");

    let response = call(
        "crypto_hmac",
        json!({"data": "message", "key": "secret", "hash": "sha256"}),
    );
    assert!(
        !response.success,
        "an unknown name must not be dropped and run"
    );
    let error = response.error.unwrap_or_default();
    assert!(error.contains("'hash'"), "{error}");
    for valid in ["data", "key", "algorithm"] {
        assert!(error.contains(valid), "error must name '{valid}': {error}");
    }
    assert_eq!(
        response.metadata.as_ref().and_then(|m| m.get("code")),
        Some(&json!("E_BAD_ARG"))
    );
}

#[test]
fn string_arguments_stay_strings_and_run_nothing() {
    let sandbox = Sandbox::new("inject");
    let sentinel = sandbox.root.join("sentinel.txt");
    // Escaped for an IronShell string, so that if it did run, it would write
    // the sentinel file.
    let target = sentinel.display().to_string().replace('\\', "\\\\");

    // A quote, a parenthesis and a semicolon that would close the call and
    // start a second statement.
    let quoted = format!("x\"); file_write(\"{target}\", \"ran\"); (\"");
    // An interpolation hole, which the evaluator would run if it were written
    // into a string literal unchanged.
    let interpolated = format!("${{file_write(\"{target}\", \"ran\")}}");

    for payload in [&quoted, &interpolated] {
        // `trim` returns its subject unchanged when there is no surrounding
        // whitespace, so the result is the payload itself.
        let echoed = call("trim", json!([payload]));
        assert!(echoed.success, "trim failed: {:?}", echoed.error);
        assert_eq!(echoed.result, Some(json!(payload)));

        // The same string as a named argument reaches the builtin intact.
        let named = call("crypto_hmac", json!({"data": payload, "key": "k"}));
        let positional = call("crypto_hmac", json!([payload, "k"]));
        assert!(named.success, "named call failed: {:?}", named.error);
        assert_eq!(named.result, positional.result);
    }

    assert!(!sentinel.exists(), "a string argument was run as code");
}

#[test]
fn parse_error_is_the_structured_json_without_a_prefix() {
    let _sandbox = Sandbox::new("parse");

    let response = process_request(&AgentRequest::Eval {
        code: "(((".to_string(),
    });
    assert!(!response.success);
    let error = response.error.expect("a parse error message");
    assert!(
        !error.starts_with("Parse error:"),
        "prefix on JSON: {error}"
    );
    let parsed: JsonValue =
        serde_json::from_str(&error).unwrap_or_else(|e| panic!("not JSON ({e}): {error}"));
    assert_eq!(parsed["error"]["code"], json!("E_PARSE"));
    assert!(parsed["error"]["message"].is_string());
    assert!(parsed["error"]["hint"].is_string());
}

#[test]
fn two_schema_requests_return_identical_bytes() {
    let _sandbox = Sandbox::new("schema");

    let request = AgentRequest::Schema {
        format: SchemaFormat::Claude,
    };
    let first = process_request(&request);
    let second = process_request(&request);
    assert!(first.success && second.success);
    assert_eq!(first.result_type, second.result_type);
    assert_eq!(
        serde_json::to_string_pretty(&first.result).expect("serialise"),
        serde_json::to_string_pretty(&second.result).expect("serialise"),
    );
}
