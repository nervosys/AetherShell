//! Tests for the Agent API module
//!
//! Tests the JSON-based API for AI agent integration.

use aethershell::agent_api::{
    build_language_ontology, generate_schema, process_json_request, process_request, AgentRequest,
    AgentResponse, SchemaFormat,
};
use serde_json::json;

// ============================================================================
// Basic API Tests
// ============================================================================

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

// ============================================================================
// Comprehensive Schema Format Tests
// ============================================================================

/// Helper to validate common schema structure
fn validate_schema_base(result: &serde_json::Value, expected_format: &str) {
    assert_eq!(
        result.get("format").and_then(|v| v.as_str()),
        Some(expected_format),
        "Format should be '{}'",
        expected_format
    );
}

/// Helper to validate OpenAI-compatible tool format
fn validate_openai_tools(tools: &[serde_json::Value]) {
    assert!(!tools.is_empty(), "Tools array should not be empty");
    for tool in tools {
        assert_eq!(
            tool.get("type").and_then(|v| v.as_str()),
            Some("function"),
            "Tool type should be 'function'"
        );
        let func = tool
            .get("function")
            .expect("Tool should have function field");
        assert!(func.get("name").is_some(), "Function should have name");
        assert!(
            func.get("description").is_some(),
            "Function should have description"
        );
        assert!(
            func.get("parameters").is_some(),
            "Function should have parameters"
        );
    }
}

// --- OpenAI Family Tests ---

#[test]
fn test_schema_openai_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::OpenAI,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "openai_function_calling");

    // Check compatible_models includes GPT-5
    let models = result
        .get("compatible_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();
    assert!(model_names.contains(&"gpt-5"), "Should include gpt-5");
    assert!(model_names.contains(&"o3"), "Should include o3");

    let tools = result.get("tools").and_then(|v| v.as_array()).unwrap();
    validate_openai_tools(tools);
}

#[test]
fn test_schema_azure_openai_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::AzureOpenAI,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "azure_openai_function_calling");
    assert!(
        result.get("api_version").is_some(),
        "Should have api_version"
    );
    assert!(
        result.get("compatible_deployments").is_some(),
        "Should have compatible_deployments"
    );

    let tools = result.get("tools").and_then(|v| v.as_array()).unwrap();
    validate_openai_tools(tools);
}

// --- Anthropic Tests ---

#[test]
fn test_schema_claude_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Claude,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "anthropic_tool_use");

    // Check for Claude 4.5 models
    let models = result
        .get("compatible_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();
    assert!(
        model_names.iter().any(|m| m.contains("4.5")),
        "Should include Claude 4.5 models"
    );

    let tools = result.get("tools").and_then(|v| v.as_array()).unwrap();
    assert!(!tools.is_empty());

    // Claude uses input_schema instead of parameters
    let first_tool = &tools[0];
    assert!(first_tool.get("name").is_some());
    assert!(
        first_tool.get("input_schema").is_some(),
        "Claude tools should use input_schema"
    );
}

// --- Google Tests ---

#[test]
fn test_schema_gemini_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Gemini,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "gemini_function_calling");

    // Check for Gemini 2.5 models
    let models = result
        .get("compatible_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();
    assert!(
        model_names.iter().any(|m| m.contains("2.5")),
        "Should include Gemini 2.5 models"
    );

    // Gemini uses function_declarations
    assert!(
        result.get("function_declarations").is_some(),
        "Should have function_declarations"
    );
}

// --- Meta Tests ---

#[test]
fn test_schema_llama_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Llama,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "llama_function_calling");

    // Check for Llama 4 models
    let models = result
        .get("compatible_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();
    assert!(
        model_names.iter().any(|m| m.contains("llama-4")),
        "Should include Llama 4 models"
    );

    assert!(
        result.get("system_prompt").is_some(),
        "Llama schema should have system_prompt"
    );

    let tools = result.get("tools").and_then(|v| v.as_array()).unwrap();
    validate_openai_tools(tools);
}

// --- Mistral Tests ---

#[test]
fn test_schema_mistral_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Mistral,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "mistral_function_calling");

    // Check for latest Mistral models
    let models = result
        .get("compatible_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();
    assert!(
        model_names.iter().any(|m| m.contains("2501")),
        "Should include 2501 models"
    );

    assert!(
        result.get("tool_choice").is_some(),
        "Mistral should have tool_choice"
    );
}

// --- Cohere Tests ---

#[test]
fn test_schema_cohere_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Cohere,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "cohere_tools");

    // Check for Command A
    let models = result
        .get("compatible_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();
    assert!(
        model_names.contains(&"command-a"),
        "Should include command-a"
    );

    // Cohere uses parameter_definitions format
    let tools = result.get("tools").and_then(|v| v.as_array()).unwrap();
    assert!(!tools.is_empty());
    let first_tool = &tools[0];
    assert!(
        first_tool.get("parameter_definitions").is_some(),
        "Cohere tools should use parameter_definitions"
    );
}

// --- xAI Tests ---

#[test]
fn test_schema_grok_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Grok,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "grok_function_calling");

    // Check for Grok 3 models
    let models = result
        .get("compatible_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();
    assert!(model_names.contains(&"grok-3"), "Should include grok-3");
}

// --- DeepSeek Tests ---

#[test]
fn test_schema_deepseek_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::DeepSeek,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "deepseek_function_calling");

    // Check for reasoning support
    assert_eq!(
        result.get("reasoning_support").and_then(|v| v.as_bool()),
        Some(true),
        "DeepSeek should have reasoning_support: true"
    );

    // Check for R1 models
    let models = result
        .get("compatible_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();
    assert!(
        model_names.iter().any(|m| m.contains("r1")),
        "Should include R1 models"
    );
}

// --- AWS Bedrock Tests ---

#[test]
fn test_schema_bedrock_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Bedrock,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "bedrock_converse");

    // Bedrock uses toolConfig structure
    let tool_config = result.get("toolConfig").expect("Should have toolConfig");
    let tools = tool_config.get("tools").and_then(|v| v.as_array()).unwrap();
    assert!(!tools.is_empty());

    // Bedrock tools use toolSpec format
    let first_tool = &tools[0];
    assert!(
        first_tool.get("toolSpec").is_some(),
        "Bedrock tools should use toolSpec"
    );
}

// --- Alibaba Qwen Tests ---

#[test]
fn test_schema_qwen_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Qwen,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "qwen_function_calling");

    // Check for Qwen 3 models
    let models = result
        .get("compatible_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();
    assert!(
        model_names.iter().any(|m| m.contains("qwen3")),
        "Should include Qwen 3 models"
    );
}

// --- Chinese AI Provider Tests ---

#[test]
fn test_schema_kimi_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Kimi,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "kimi_function_calling");
    assert!(result.get("api_base").is_some(), "Should have api_base");

    // Check for Kimi K2
    let models = result
        .get("compatible_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();
    assert!(
        model_names
            .iter()
            .any(|m| m.contains("k2") || m.contains("v2")),
        "Should include latest Kimi models"
    );
}

#[test]
fn test_schema_yi_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Yi,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "yi_function_calling");
    assert_eq!(
        result.get("api_base").and_then(|v| v.as_str()),
        Some("https://api.01.ai/v1")
    );
}

#[test]
fn test_schema_glm_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::GLM,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "glm_function_calling");

    // Check for GLM-5
    let models = result
        .get("compatible_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();
    assert!(
        model_names.iter().any(|m| m.contains("glm-5")),
        "Should include GLM-5 models"
    );
}

// --- Additional SOTA Provider Tests ---

#[test]
fn test_schema_reka_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Reka,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "reka_function_calling");
    assert_eq!(
        result.get("multimodal").and_then(|v| v.as_bool()),
        Some(true),
        "Reka should be multimodal"
    );
}

#[test]
fn test_schema_ai21_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::AI21,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "ai21_function_calling");

    // Check for Jamba 2
    let models = result
        .get("compatible_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();
    assert!(
        model_names.iter().any(|m| m.contains("jamba-2")),
        "Should include Jamba 2 models"
    );
}

#[test]
fn test_schema_perplexity_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Perplexity,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "perplexity_function_calling");
    assert_eq!(
        result.get("online_search").and_then(|v| v.as_bool()),
        Some(true),
        "Perplexity should have online_search: true"
    );
}

// --- Inference Platform Tests ---

#[test]
fn test_schema_together_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Together,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "together_function_calling");
    assert_eq!(
        result.get("api_base").and_then(|v| v.as_str()),
        Some("https://api.together.xyz/v1")
    );

    // Together uses tool_capable_models
    assert!(result.get("tool_capable_models").is_some());
}

#[test]
fn test_schema_groq_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Groq,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "groq_function_calling");
    assert_eq!(
        result.get("ultra_low_latency").and_then(|v| v.as_bool()),
        Some(true),
        "Groq should have ultra_low_latency: true"
    );
}

#[test]
fn test_schema_fireworks_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Fireworks,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "fireworks_function_calling");
    assert_eq!(
        result.get("fast_inference").and_then(|v| v.as_bool()),
        Some(true),
        "Fireworks should have fast_inference: true"
    );
}

// --- Local/Self-hosted Tests ---

#[test]
fn test_schema_ollama_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::Ollama,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "ollama_tools");
    assert!(result.get("endpoint").is_some(), "Should have endpoint");

    // Check for Llama 4 in Ollama
    let models = result
        .get("compatible_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();
    assert!(
        model_names.iter().any(|m| m.contains("llama4")),
        "Should include Llama 4 models"
    );
}

#[test]
fn test_schema_vllm_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::VLLM,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "vllm_openai_compatible");
    assert!(result.get("endpoint").is_some());
    assert!(result.get("tool_choice_modes").is_some());
}

#[test]
fn test_schema_huggingface_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::HuggingFace,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "huggingface_tgi");
    assert!(result.get("compatible_endpoints").is_some());
    assert!(result.get("recommended_models").is_some());
}

// --- Multi-Provider Tests ---

#[test]
fn test_schema_openrouter_structure() {
    let request = AgentRequest::Schema {
        format: SchemaFormat::OpenRouter,
    };
    let response = process_request(&request);
    assert!(response.success);

    let result = response.result.unwrap();
    validate_schema_base(&result, "openrouter_unified");
    assert_eq!(
        result.get("api_base").and_then(|v| v.as_str()),
        Some("https://openrouter.ai/api/v1")
    );

    // OpenRouter should have diverse model list
    let models = result
        .get("tool_capable_models")
        .and_then(|v| v.as_array())
        .unwrap();
    let model_names: Vec<&str> = models.iter().filter_map(|m| m.as_str()).collect();

    // Check for models from multiple providers
    assert!(
        model_names.iter().any(|m| m.starts_with("openai/")),
        "Should have OpenAI models"
    );
    assert!(
        model_names.iter().any(|m| m.starts_with("anthropic/")),
        "Should have Anthropic models"
    );
    assert!(
        model_names.iter().any(|m| m.starts_with("google/")),
        "Should have Google models"
    );
}

// --- Schema Generation CLI Tests ---

#[test]
fn test_generate_schema_all_formats() {
    let formats = vec![
        "openai",
        "azure",
        "claude",
        "gemini",
        "llama",
        "mistral",
        "cohere",
        "grok",
        "deepseek",
        "bedrock",
        "qwen",
        "kimi",
        "yi",
        "glm",
        "reka",
        "ai21",
        "perplexity",
        "together",
        "groq",
        "fireworks",
        "ollama",
        "vllm",
        "huggingface",
        "openrouter",
        "json",
        "compact",
    ];

    for format in formats {
        let result = generate_schema(format);
        assert!(
            result.is_ok(),
            "Schema generation for '{}' should succeed: {:?}",
            format,
            result.err()
        );

        // Verify it's valid JSON
        let json_str = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .expect(&format!("Schema for '{}' should be valid JSON", format));
        assert!(parsed.is_object(), "Schema should be a JSON object");
    }
}

#[test]
fn test_generate_schema_aliases() {
    // Test format aliases
    let alias_pairs = vec![
        ("gpt", "openai"),
        ("chatgpt", "openai"),
        ("anthropic", "claude"),
        ("google", "gemini"),
        ("meta", "llama"),
        ("xai", "grok"),
        ("aws", "bedrock"),
        ("alibaba", "qwen"),
        ("moonshot", "kimi"),
        ("01ai", "yi"),
        ("chatglm", "glm"),
        ("zhipu", "glm"),
        ("jamba", "ai21"),
        ("sonar", "perplexity"),
        ("hf", "huggingface"),
        ("tgi", "huggingface"),
    ];

    for (alias, canonical) in alias_pairs {
        let alias_result = generate_schema(alias);
        let canonical_result = generate_schema(canonical);

        assert!(alias_result.is_ok(), "Alias '{}' should work", alias);
        assert!(
            canonical_result.is_ok(),
            "Canonical '{}' should work",
            canonical
        );

        // Both should produce the same format field
        let alias_json: serde_json::Value = serde_json::from_str(&alias_result.unwrap()).unwrap();
        let canonical_json: serde_json::Value =
            serde_json::from_str(&canonical_result.unwrap()).unwrap();

        assert_eq!(
            alias_json.get("format"),
            canonical_json.get("format"),
            "Alias '{}' should produce same format as '{}'",
            alias,
            canonical
        );
    }
}
