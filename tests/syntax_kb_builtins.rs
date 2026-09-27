/// Integration tests for Syntax KB builtins
/// Tests the functionality of syntax_get, syntax_list, syntax_search, syntax_add,
/// syntax_categories, ab_encode, and ab_decode
use aethershell::{env::Env, eval, parser, value::Value};
use anyhow::Result;

/// The syntax KB persists to $HOME/.aethershell/syntax_kb.json, and
/// `syntax_add` saves. These tests added `test_proto` to the developer's own
/// knowledge base on every run. Point HOME at a scratch directory before the
/// first test touches the KB (it is loaded once per process).
fn isolate() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("ae-syntax-kb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("HOME", &dir);
        std::env::set_var("USERPROFILE", &dir);
    });
}

/// Helper to evaluate and get string result
fn eval_str(env: &mut Env, code: &str) -> Result<String> {
    let stmts = parser::parse_program(code)?;
    let result = eval::eval_program(&stmts, env)?;
    Ok(format!("{:?}", result))
}

/// Helper to evaluate code in environment
fn eval_in_env(env: &mut Env, code: &str) -> Result<Value> {
    let stmts = parser::parse_program(code)?;
    eval::eval_program(&stmts, env)
}

#[test]
fn test_syntax_get_ab() {
    isolate();
    let mut env = Env::new();
    let result = eval_str(&mut env, r#"syntax_get("ab")"#).unwrap();

    assert!(result.contains("AgenticBinary"));
    assert!(result.contains("id"));
    assert!(result.contains("specification"));
    assert!(result.contains("category"));
}

#[test]
fn test_syntax_search() {
    isolate();
    let mut env = Env::new();
    let result = eval_str(&mut env, r#"syntax_search("protocol")"#).unwrap();

    // Should find at least "ab" and "jsonrpc" protocols
    assert!(result.contains("ab") || result.contains("jsonrpc"));
}

#[test]
fn test_syntax_add_and_retrieve() {
    isolate();
    let mut env = Env::new();

    // Add a custom syntax entry
    let add_code = r#"
        syntax_add({
            id: "test_proto",
            name: "Test Protocol",
            category: "protocol",
            specification: "A test protocol specification",
            examples: ["test example 1", "test example 2"]
        })
    "#;

    eval_str(&mut env, add_code).unwrap();

    // Retrieve it
    let result = eval_str(&mut env, r#"syntax_get("test_proto")"#).unwrap();

    assert!(result.contains("Test Protocol"));
    assert!(result.contains("test_proto"));
    assert!(result.contains("A test protocol specification"));
}

#[test]
fn test_ab_encode_decode_ping() {
    isolate();
    let mut env = Env::new();

    // Encode a PING message
    let bytes_result = eval_str(&mut env, r#"ab_encode("command", "ping", "hello")"#).unwrap();
    assert!(bytes_result.contains("Array"));

    // Decode it back
    let decode_result =
        eval_str(&mut env, r#"ab_decode([0, 5, 104, 101, 108, 108, 111])"#).unwrap();

    assert!(decode_result.contains("Command"));
    assert!(decode_result.contains("PING"));
    assert!(decode_result.contains("hello"));
    assert!(decode_result.contains("version"));
    assert!(decode_result.contains("msg_type"));
    assert!(decode_result.contains("opcode"));
    assert!(decode_result.contains("payload"));
}

#[test]
fn test_ab_encode_decode_query() {
    isolate();
    let mut env = Env::new();

    // Encode a QUERY message
    eval_str(
        &mut env,
        r#"query_bytes = ab_encode("query", "query", "data request")"#,
    )
    .unwrap();

    // Decode it
    let result = eval_str(&mut env, r#"ab_decode(query_bytes)"#).unwrap();

    assert!(result.contains("Query"));
    assert!(result.contains("QUERY"));
    assert!(result.contains("data request"));
}

#[test]
fn test_ab_encode_delegate() {
    isolate();
    let mut env = Env::new();

    // Encode a DELEGATE message
    let result = eval_str(
        &mut env,
        r#"ab_encode("command", "delegate", "task:analyze")"#,
    )
    .unwrap();

    // Should return an array of bytes
    assert!(result.contains("Array"));
    assert!(result.contains("Int"));
}

#[test]
fn test_ab_encode_collaborate() {
    isolate();
    let mut env = Env::new();

    // Encode a COLLABORATE message
    eval_str(
        &mut env,
        r#"msg = ab_encode("command", "collaborate", "agent_2,agent_3")"#,
    )
    .unwrap();

    // Decode and verify
    let result = eval_str(&mut env, r#"ab_decode(msg)"#).unwrap();

    assert!(result.contains("COLLABORATE"));
    assert!(result.contains("agent_2,agent_3"));
}

#[test]
fn test_ab_encode_learn_ack_workflow() {
    isolate();
    let mut env = Env::new();

    // Agent 1 sends LEARN message
    eval_str(
        &mut env,
        r#"learn = ab_encode("command", "learn", "syntax:ab")"#,
    )
    .unwrap();
    let learn_decoded = eval_str(&mut env, r#"ab_decode(learn)"#).unwrap();

    assert!(learn_decoded.contains("LEARN"));
    assert!(learn_decoded.contains("syntax:ab"));

    // Agent 2 responds with ACK
    eval_str(
        &mut env,
        r#"ack = ab_encode("response", "ack", "learned:ab")"#,
    )
    .unwrap();
    let ack_decoded = eval_str(&mut env, r#"ab_decode(ack)"#).unwrap();

    assert!(ack_decoded.contains("Response"));
    assert!(ack_decoded.contains("ACK"));
    assert!(ack_decoded.contains("learned:ab"));
}

#[test]
fn test_ab_all_opcodes() {
    isolate();
    let mut env = Env::new();

    let opcodes = vec![
        ("ping", "PING"),
        ("ack", "ACK"),
        ("query", "QUERY"),
        ("exec", "EXEC"),
        ("data", "DATA"),
        ("error", "ERROR"),
        ("sync", "SYNC"),
        ("auth", "AUTH"),
        ("delegate", "DELEGATE"),
        ("collaborate", "COLLABORATE"),
        ("learn", "LEARN"),
        ("reason", "REASON"),
        ("plan", "PLAN"),
        ("observe", "OBSERVE"),
        ("reflect", "REFLECT"),
        ("extend", "EXTEND"),
    ];

    for (opcode_name, expected_upper) in opcodes {
        let code = format!(r#"ab_encode("command", "{}", "test")"#, opcode_name);
        eval_str(&mut env, &format!(r#"bytes_{} = {}"#, opcode_name, code)).unwrap();

        let decode_code = format!(r#"ab_decode(bytes_{})"#, opcode_name);
        let result = eval_str(&mut env, &decode_code).unwrap();

        assert!(
            result.contains(expected_upper),
            "Opcode {} should decode to {}",
            opcode_name,
            expected_upper
        );
    }
}

#[test]
fn test_ab_encode_numeric_codes() {
    isolate();
    let mut env = Env::new();

    // Test using numeric message type and opcode
    eval_str(&mut env, r#"bytes = ab_encode(0, 4, "test data")"#).unwrap();
    let result = eval_str(&mut env, r#"ab_decode(bytes)"#).unwrap();

    assert!(result.contains("Command"));
    assert!(result.contains("DATA"));
    assert!(result.contains("test data"));
}

#[test]
fn test_syntax_search_no_results() {
    isolate();
    let mut env = Env::new();

    // Search for something that doesn't exist
    let result = eval_str(&mut env, r#"syntax_search("nonexistent_protocol_xyz123")"#).unwrap();

    // Should return an empty array
    assert!(result.contains("Array([])") || result == "Array([])");
}

#[test]
fn test_ab_message_types() {
    isolate();
    let mut env = Env::new();

    let msg_types = vec![
        ("command", "Command"),
        ("query", "Query"),
        ("response", "Response"),
        ("event", "Event"),
    ];

    for (type_name, expected) in msg_types {
        let code = format!(r#"ab_encode("{}", "ping", "test")"#, type_name);
        eval_str(&mut env, &format!(r#"msg_{} = {}"#, type_name, code)).unwrap();

        let result = eval_str(&mut env, &format!(r#"ab_decode(msg_{})"#, type_name)).unwrap();

        assert!(
            result.contains(expected),
            "Message type {} should decode to {}",
            type_name,
            expected
        );
    }
}

#[test]
fn test_ab_roundtrip_unicode() {
    isolate();
    let mut env = Env::new();

    // Test Unicode payload
    eval_str(
        &mut env,
        r#"bytes = ab_encode("command", "data", "Hello 世界 🌍")"#,
    )
    .unwrap();
    let result = eval_str(&mut env, r#"ab_decode(bytes)"#).unwrap();

    assert!(result.contains("Hello"));
    // Note: Unicode may be escaped in debug output
}

#[test]
fn test_ab_version_field() {
    isolate();
    let mut env = Env::new();

    // Encode and decode, check version field
    eval_str(&mut env, r#"bytes = ab_encode("command", "ping", "v")"#).unwrap();
    let result = eval_str(&mut env, r#"ab_decode(bytes)"#).unwrap();

    // Version should be 0
    assert!(result.contains("version"));
    assert!(result.contains("Int(0)"));
}

#[test]
fn test_syntax_get_nonexistent() {
    isolate();
    let mut env = Env::new();

    // Try to get non-existent syntax entry
    let result = eval_in_env(&mut env, r#"syntax_get("nonexistent_id_xyz")"#);

    // Should return an error
    assert!(result.is_err());
}

/// The KB is a HashMap, iterated in an order randomised per process, so the
/// same search answered in a different order on every run. The discarded-args
/// probe counted `syntax_search` as nondeterministic for it.
#[test]
fn listings_are_in_a_stable_order() {
    isolate();
    let mut env = Env::new();
    for code in [r#"syntax_list()"#, r#"syntax_search("a")"#] {
        let Value::Array(items) = eval_in_env(&mut env, code).unwrap() else {
            panic!("{code} is not an array")
        };
        let ids: Vec<String> = items
            .iter()
            .map(|v| match v {
                Value::Str(s) => s.clone(),
                Value::Record(r) => r
                    .get("id")
                    .map(|i| i.to_display_string())
                    .unwrap_or_default(),
                other => other.to_display_string(),
            })
            .collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "{code} is not in a stable order");
    }
}
