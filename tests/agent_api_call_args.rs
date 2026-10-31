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

/// Runs `steps` as a `pipeline` request, over `input` when one is given.
fn pipeline(steps: JsonValue, input: Option<JsonValue>) -> AgentResponse {
    process_request(&AgentRequest::Pipeline {
        steps: serde_json::from_value(steps).expect("pipeline steps"),
        input,
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
fn a_pipeline_select_must_be_a_field_name() {
    let _sandbox = Sandbox::new("select-name");
    let input = Some(json!([{"name": "a"}]));

    for bad in ["name key", "name)", "a.b", "1st", "x-y"] {
        let response = pipeline(
            json!([{"builtin": "where", "predicate": "true", "select": bad}]),
            input.clone(),
        );
        assert!(!response.success, "select {bad:?} must be refused");
        assert_eq!(response.code.as_deref(), Some("E_BAD_ARG"));
        let error = response.error.unwrap_or_default();
        assert!(error.contains(bad), "error must name {bad:?}: {error}");
    }

    let named = pipeline(
        json!([{"builtin": "where", "predicate": "true", "select": "name"}]),
        input,
    );
    assert!(named.success, "select name failed: {:?}", named.error);
    assert_eq!(named.result, Some(json!(["a"])));
}

#[test]
fn a_pipeline_predicate_must_be_one_expression() {
    let _sandbox = Sandbox::new("predicate-one");
    let input = Some(json!([1, 2]));

    // Closes the call, adds a stage, and leaves a call open for the wrapper.
    let response = pipeline(
        json!([{"builtin": "where", "predicate": "true) | map(fn(x) => 1"}]),
        input.clone(),
    );
    assert!(!response.success, "a two-stage predicate must be refused");
    assert_eq!(response.code.as_deref(), Some("E_BAD_ARG"));

    // A second statement after the expression is refused too.
    let response = pipeline(
        json!([{"builtin": "where", "predicate": "true; 1"}]),
        input.clone(),
    );
    assert!(
        !response.success,
        "a two-statement predicate must be refused"
    );

    // The documented form, `size > 1000`, is one expression and still works.
    let plain = pipeline(
        json!([{"builtin": "where", "predicate": "size > 1000"}]),
        Some(json!([{"size": 1}, {"size": 2000}])),
    );
    assert!(
        plain.success,
        "a comparison predicate failed: {:?}",
        plain.error
    );
    assert_eq!(plain.result, Some(json!([{"size": 2000}])));
}

#[test]
fn select_and_predicate_attacks_run_nothing() {
    let sandbox = Sandbox::new("pipeline-inject");
    let marker = sandbox.root.join("marker.txt");
    // Escaped for an IronShell string, as the test above does.
    let target = marker.display().to_string().replace('\\', "\\\\");
    let input = Some(json!([{"name": "a", "size": 5}]));

    // Positive control: the same write, written as code, does run. Without it,
    // a refused attack would look the same as one that never could have run.
    let direct = process_request(&AgentRequest::Eval {
        code: format!("file_write(\"{target}\", \"ran\")"),
    });
    assert!(direct.success, "control write failed: {:?}", direct.error);
    assert!(marker.exists(), "control write did not create the marker");
    std::fs::remove_file(&marker).expect("remove control marker");

    // Each payload closes the step's call, maps a write over the records, and
    // reopens a call so that the wrapper still closes the program.
    let effect = format!("map(fn(x) => file_write(\"{target}\", \"ran\"))");
    let select_attack = format!("name) | {effect} | map(fn(x) => x.name");
    let predicate_attack = format!("size > 0) | {effect} | where(fn(x) => size > 0");

    let by_select = pipeline(
        json!([{"builtin": "where", "predicate": "true", "select": select_attack}]),
        input.clone(),
    );
    assert!(!by_select.success, "a select attack was accepted");
    assert_eq!(by_select.code.as_deref(), Some("E_BAD_ARG"));

    let by_predicate = pipeline(
        json!([{"builtin": "where", "predicate": predicate_attack}]),
        input,
    );
    assert!(!by_predicate.success, "a predicate attack was accepted");
    assert_eq!(by_predicate.code.as_deref(), Some("E_BAD_ARG"));

    assert!(!marker.exists(), "a select or predicate was run as code");
}

#[test]
fn code_and_hint_come_from_the_structured_error_only_when_there_is_one() {
    let _sandbox = Sandbox::new("code-hint");

    // A parse error is structured, so its code and hint are lifted out of it,
    // and its error text is unchanged.
    let parse = process_request(&AgentRequest::Eval {
        code: "(((".to_string(),
    });
    let wire = serde_json::to_value(&parse).expect("serialise");
    assert_eq!(wire["code"], json!("E_PARSE"));
    assert!(wire["hint"].is_string(), "{wire}");
    assert!(wire["error"].is_string(), "{wire}");

    // A refused select is structured too, and its hint says what was wanted.
    let select = pipeline(
        json!([{"builtin": "where", "predicate": "true", "select": "a b"}]),
        Some(json!([{"a": 1}])),
    );
    assert_eq!(select.code.as_deref(), Some("E_BAD_ARG"));
    assert!(select.hint.as_deref().is_some_and(|h| !h.is_empty()));

    // A success has neither field on the wire.
    let ok = call("len", json!(["hello"]));
    let wire = serde_json::to_value(&ok).expect("serialise");
    assert!(
        wire.get("code").is_none() && wire.get("hint").is_none(),
        "{wire}"
    );

    // A plain-text failure has no code to report.
    let plain = process_request(&AgentRequest::Pipeline {
        steps: Vec::new(),
        input: None,
    });
    assert!(!plain.success);
    assert_eq!(plain.code, None);
    assert_eq!(plain.hint, None);
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
        module: None,
        q: None,
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

/// A schema request for the compact ontology, with the given filter.
fn schema(module: Option<&str>, q: Option<&str>) -> AgentResponse {
    process_request(&AgentRequest::Schema {
        format: SchemaFormat::Ontology,
        module: module.map(str::to_string),
        q: q.map(str::to_string),
    })
}

/// The names of the builtins in a compact-ontology answer.
fn builtin_names(response: &AgentResponse) -> Vec<String> {
    let result = response.result.as_ref().expect("a schema result");
    result["builtins"]
        .as_array()
        .expect("a builtins array")
        .iter()
        .map(|b| b["n"].as_str().expect("a name").to_string())
        .collect()
}

#[test]
fn a_module_filter_shrinks_every_provider_format() {
    let _sandbox = Sandbox::new("schema-filter-shrinks");

    let whole = process_request(&AgentRequest::Schema {
        format: SchemaFormat::Claude,
        module: None,
        q: None,
    });
    let filtered = process_request(&AgentRequest::Schema {
        format: SchemaFormat::Claude,
        module: Some("file".to_string()),
        q: None,
    });
    assert!(whole.success && filtered.success);
    let whole_tools = whole.result.as_ref().unwrap()["tools"]
        .as_array()
        .unwrap()
        .len();
    let file_tools = filtered.result.as_ref().unwrap()["tools"]
        .as_array()
        .unwrap()
        .len();
    assert!(file_tools > 0, "the file module has builtins");
    assert!(
        file_tools * 10 < whole_tools,
        "the file module is a small part of the catalogue: {file_tools} of {whole_tools}"
    );
    let whole_bytes = serde_json::to_string(&whole.result).unwrap().len();
    let file_bytes = serde_json::to_string(&filtered.result).unwrap().len();
    assert!(file_bytes < whole_bytes, "{file_bytes} vs {whole_bytes}");
}

#[test]
fn a_query_keeps_only_builtins_whose_name_or_description_match() {
    let _sandbox = Sandbox::new("schema-filter-query");

    let whole = builtin_names(&schema(None, None));
    let matched = schema(None, Some("READ"));
    assert!(matched.success);
    let names = builtin_names(&matched);
    assert!(
        !names.is_empty() && names.len() < whole.len() / 2,
        "{}",
        names.len()
    );
    for entry in matched.result.as_ref().unwrap()["builtins"]
        .as_array()
        .unwrap()
    {
        let text = format!(
            "{} {}",
            entry["n"].as_str().unwrap(),
            entry["d"].as_str().unwrap()
        )
        .to_lowercase();
        assert!(text.contains("read"), "{text}");
    }
}

#[test]
fn an_unknown_module_is_refused_and_names_the_modules_it_might_mean() {
    let _sandbox = Sandbox::new("schema-filter-unknown");

    let refused = schema(Some("fle"), None);
    assert!(!refused.success, "an unknown module must be refused");
    assert!(refused.result.is_none());
    assert_eq!(refused.code.as_deref(), Some("E_BAD_ARG"));
    let metadata = refused.metadata.as_ref().expect("metadata");
    assert_eq!(metadata["code"], json!("E_BAD_ARG"));
    assert!(
        metadata["did_you_mean"]
            .as_array()
            .expect("candidates")
            .iter()
            .any(|m| m.as_str() == Some("file")),
        "{metadata}"
    );
    assert!(refused.error.unwrap().contains("Unknown module 'fle'"));
}

#[test]
fn a_filtered_and_an_unfiltered_request_do_not_share_a_cache_entry() {
    let _sandbox = Sandbox::new("schema-filter-cache");

    // The unfiltered answer is asked first, so it is the one a collision
    // would serve to the filtered request.
    let whole = builtin_names(&schema(None, None));
    let file = builtin_names(&schema(Some("file"), None));
    let net = builtin_names(&schema(Some("net"), None));
    assert!(file.len() < whole.len() && net.len() < whole.len());
    assert_ne!(file, net, "two modules must not share an answer");

    // And the unfiltered answer is still whole after the filtered ones.
    assert_eq!(builtin_names(&schema(None, None)), whole);
    assert_eq!(builtin_names(&schema(Some("file"), None)), file);
}

#[test]
fn an_empty_filter_is_the_whole_catalogue() {
    let _sandbox = Sandbox::new("schema-filter-empty");

    let whole = serde_json::to_string(&schema(None, None).result).unwrap();
    let blank = serde_json::to_string(&schema(Some("  "), Some("")).result).unwrap();
    assert_eq!(whole, blank);
}
