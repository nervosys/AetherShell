//! Tests for the Agent API module
//!
//! Tests the JSON-based API for AI agent integration.

use aethershell::agent_api::{
    build_language_ontology, generate_schema, process_json_request, process_request, AgentRequest,
    AgentResponse, PipelineStep, SchemaFormat,
};
use serde_json::json;

#[test]
fn test_call_request_simple() {
    let request = AgentRequest::Call {
        builtin: "len".to_string(),
        args: json!(["hello"]),
    };

    let response = process_request(&request);
    assert!(
        response.success,
        "Call should succeed: {:?}",
        response.error
    );
    assert_eq!(response.result, Some(json!(5)));
    assert_eq!(response.result_type, Some("Int".to_string()));
}

#[test]
fn test_call_request_with_object_args() {
    let request = AgentRequest::Call {
        builtin: "pwd".to_string(),
        args: json!({}),
    };

    let response = process_request(&request);
    assert!(
        response.success,
        "pwd() should succeed: {:?}",
        response.error
    );
    assert!(response.result_type == Some("String".to_string()));
}

#[test]
fn test_eval_request() {
    let request = AgentRequest::Eval {
        code: "1 + 2 * 3".to_string(),
    };

    let response = process_request(&request);
    assert!(
        response.success,
        "Eval should succeed: {:?}",
        response.error
    );
    assert_eq!(response.result, Some(json!(7)));
}

#[test]
fn test_eval_request_pipeline() {
    let request = AgentRequest::Eval {
        code: "[1, 2, 3] | map(fn(x) => x * 2) | sum()".to_string(),
    };

    let response = process_request(&request);
    assert!(
        response.success,
        "Pipeline eval should succeed: {:?}",
        response.error
    );
    assert_eq!(response.result, Some(json!(12)));
}

#[test]
fn test_list_builtins() {
    let request = AgentRequest::ListBuiltins { category: None };

    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    assert!(
        count > 10,
        "Should have at least 10 builtins, got {}",
        count
    );
}

#[test]
fn test_list_builtins_filtered() {
    let request = AgentRequest::ListBuiltins {
        category: Some("AI".to_string()),
    };

    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    let builtins = result.get("builtins").and_then(|v| v.as_array()).unwrap();

    for builtin in builtins {
        let cat = builtin
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(cat, "AI", "All builtins should be in AI category");
    }
}

#[test]
fn test_describe_builtin() {
    let request = AgentRequest::Describe {
        builtin: "map".to_string(),
    };

    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    assert_eq!(result.get("name").and_then(|v| v.as_str()), Some("map"));
    assert!(result.get("description").is_some());
    assert!(result.get("signature").is_some());
}

#[test]
fn test_describe_builtin_not_found() {
    let request = AgentRequest::Describe {
        builtin: "nonexistent_builtin_xyz".to_string(),
    };

    let response = process_request(&request);
    assert!(!response.success);
    assert!(response.error.is_some());
}

#[test]
fn test_schema_compact() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Ontology,
    };

    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    assert!(result.get("lang").is_some());
    assert!(result.get("ver").is_some());
    assert!(result.get("types").is_some());
    assert!(result.get("ops").is_some());
    assert!(result.get("syntax").is_some());
    assert!(result.get("builtins").is_some());
}

#[test]
fn test_schema_openai() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::OpenAI,
    };

    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    assert_eq!(
        result.get("format").and_then(|v| v.as_str()),
        Some("openai_function_calling")
    );
    assert!(result.get("tools").is_some());

    let tools = result.get("tools").and_then(|v| v.as_array()).unwrap();
    assert!(!tools.is_empty());

    // Check first tool has correct OpenAI format
    let first_tool = &tools[0];
    assert_eq!(
        first_tool.get("type").and_then(|v| v.as_str()),
        Some("function")
    );
    assert!(first_tool.get("function").is_some());
}

#[test]
fn test_schema_claude() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Claude,
    };

    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    assert_eq!(
        result.get("format").and_then(|v| v.as_str()),
        Some("anthropic_tool_use")
    );
    assert!(result.get("tools").is_some());

    let tools = result.get("tools").and_then(|v| v.as_array()).unwrap();
    assert!(!tools.is_empty());

    // Check first tool has correct Claude format (input_schema instead of parameters)
    let first_tool = &tools[0];
    assert!(first_tool.get("name").is_some());
    assert!(first_tool.get("input_schema").is_some());
}

#[test]
fn test_schema_gemini() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Gemini,
    };

    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    assert_eq!(
        result.get("format").and_then(|v| v.as_str()),
        Some("gemini_function_calling")
    );
    assert!(result.get("function_declarations").is_some());
}

#[test]
fn test_type_info() {
    let request = AgentRequest::TypeInfo { type_name: None };

    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    let types = result.as_array().unwrap();
    assert!(!types.is_empty());

    // Check for expected types
    let type_names: Vec<&str> = types
        .iter()
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
        .collect();

    assert!(type_names.contains(&"Int"));
    assert!(type_names.contains(&"String"));
    assert!(type_names.contains(&"Array"));
    assert!(type_names.contains(&"Record"));
}

#[test]
fn test_type_info_specific() {
    let request = AgentRequest::TypeInfo {
        type_name: Some("Array".to_string()),
    };

    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    assert_eq!(result.get("name").and_then(|v| v.as_str()), Some("Array"));
}

#[test]
fn test_process_json_request() {
    let json_str = r#"{"action":"eval","code":"2 + 2"}"#;
    let result = process_json_request(json_str).unwrap();
    let parsed: AgentResponse = serde_json::from_str(&result).unwrap();

    assert!(parsed.success);
    assert_eq!(parsed.result, Some(json!(4)));
}

#[test]
fn test_process_json_request_invalid() {
    let json_str = r#"{"invalid": "json structure"}"#;
    let result = process_json_request(json_str);
    assert!(result.is_err());
}

#[test]
fn test_generate_schema_formats() {
    let formats = vec!["openai", "claude", "gemini", "compact", "json"];

    for format in formats {
        let result = generate_schema(format);
        assert!(
            result.is_ok(),
            "Schema generation for {} should succeed",
            format
        );
    }
}

#[test]
fn test_generate_schema_invalid() {
    let result = generate_schema("invalid_format_xyz");
    assert!(result.is_err());
}

#[test]
fn test_language_ontology_structure() {
    let ontology = build_language_ontology();

    assert_eq!(ontology.language.name, "AetherShell");
    assert!(!ontology.types.is_empty());
    assert!(!ontology.builtins.is_empty());
    assert!(!ontology.operators.is_empty());
    assert!(!ontology.categories.is_empty());

    // Check syntax patterns are populated
    assert!(!ontology.syntax.variable_declaration.is_empty());
    assert!(!ontology.syntax.lambda.is_empty());
    assert!(!ontology.syntax.pipeline.is_empty());
}

#[test]
fn test_eval_error_handling() {
    // Using a clear syntax error that will fail parsing
    let request = AgentRequest::Eval {
        code: "let x = ".to_string(), // Incomplete expression
    };

    let response = process_request(&request);
    assert!(!response.success, "Incomplete code should fail");
    assert!(response.error.is_some());
}

#[test]
fn test_call_nonexistent_builtin() {
    let request = AgentRequest::Call {
        builtin: "nonexistent_function_xyz".to_string(),
        args: json!({}),
    };

    let response = process_request(&request);
    assert!(!response.success);
}

#[test]
fn test_result_types() {
    // Test different result types
    let test_cases = vec![
        (r#"{"action":"eval","code":"42"}"#, "Int"),
        (r#"{"action":"eval","code":"3.14"}"#, "Float"),
        (r#"{"action":"eval","code":"\"hello\""}"#, "String"),
        (r#"{"action":"eval","code":"true"}"#, "Bool"),
        (r#"{"action":"eval","code":"[1, 2, 3]"}"#, "Array"),
        (r#"{"action":"eval","code":"{x: 1, y: 2}"}"#, "Record"),
    ];

    for (json_str, expected_type) in test_cases {
        let result = process_json_request(json_str).unwrap();
        let parsed: AgentResponse = serde_json::from_str(&result).unwrap();

        assert!(
            parsed.success,
            "Eval should succeed for type {}",
            expected_type
        );
        assert_eq!(
            parsed.result_type.as_deref(),
            Some(expected_type),
            "Expected type {} but got {:?}",
            expected_type,
            parsed.result_type
        );
    }
}
