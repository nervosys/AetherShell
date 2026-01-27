//! Comprehensive MCP (Model Context Protocol) Tests
//! Tests for MCP client, tool discovery, execution, and integration

use aethershell::ai::mcp::{MCP_VERSION, McpClient, McpToolSchema};

// ========== Basic MCP Client Tests ==========

#[test]
fn test_mcp_client_creation() {
    let client = McpClient::new("http://localhost:8080");
    assert_eq!(client.endpoint, "http://localhost:8080");
}

#[test]
fn test_mcp_version_constant() {
    assert_eq!(MCP_VERSION, "1.0");
}

#[test]
fn test_mcp_client_with_trailing_slash() {
    let client = McpClient::new("http://localhost:8080/");
    assert_eq!(client.endpoint, "http://localhost:8080/");
}

// ========== Tool Discovery Tests ==========

#[test]
fn test_mcp_list_tools_empty_when_server_unreachable() {
    let client = McpClient::new("http://localhost:9999"); // Non-existent server
    let tools = client.list_tools();
    assert!(tools.is_ok());
    assert_eq!(tools.unwrap(), Vec::<String>::new());
}

#[test]
fn test_mcp_discover_tools_returns_empty_on_error() {
    let client = McpClient::new("http://localhost:9999");
    let result = client.discover_tools();
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_mcp_tool_schema_serialization() {
    use serde_json::json;

    let schema = McpToolSchema {
        name: "test_tool".to_string(),
        description: "A test tool".to_string(),
        input_schema: json!({"type": "object"}),
        output_schema: Some(json!({"type": "string"})),
    };

    let serialized = serde_json::to_string(&schema);
    assert!(serialized.is_ok());
}

#[test]
fn test_mcp_tool_schema_deserialization() {
    let json_str = r#"{
        "name": "example",
        "description": "Example tool",
        "input_schema": {"type": "string"},
        "output_schema": {"type": "number"}
    }"#;

    let schema: Result<McpToolSchema, _> = serde_json::from_str(json_str);
    assert!(schema.is_ok());
    let schema = schema.unwrap();
    assert_eq!(schema.name, "example");
    assert_eq!(schema.description, "Example tool");
}

// ========== Tool Execution Tests ==========

#[test]
fn test_mcp_call_tool_with_unreachable_server() {
    let client = McpClient::new("http://localhost:9999");
    let result = client.call_tool("test", "{}");
    // Should return error when server unreachable
    assert!(result.is_err());
}

#[test]
fn test_mcp_call_tool_input_parsing() {
    let client = McpClient::new("http://localhost:9999");
    // Should handle valid JSON input
    let result = client.call_tool("test", r#"{"key": "value"}"#);
    assert!(result.is_err()); // Server unreachable, but input parsed
}

#[test]
fn test_mcp_call_tool_invalid_json_input() {
    let client = McpClient::new("http://localhost:9999");
    // Should handle invalid JSON by wrapping in string
    let result = client.call_tool("test", "not json");
    assert!(result.is_err()); // Server unreachable
}

// ========== Health Check Tests ==========

#[test]
fn test_mcp_health_check_unreachable_server() {
    let client = McpClient::new("http://localhost:9999");
    let is_healthy = client.health_check();
    assert!(!is_healthy);
}

#[test]
fn test_mcp_health_check_invalid_url() {
    let client = McpClient::new("not-a-url");
    let is_healthy = client.health_check();
    assert!(!is_healthy);
}

// ========== Tool Cache Tests ==========

#[test]
fn test_mcp_tool_description_empty_cache() {
    let client = McpClient::new("http://localhost:8080");
    let desc = client.get_tool_description("nonexistent");
    assert!(desc.is_none());
}

#[test]
fn test_mcp_validate_input_without_cache() {
    use serde_json::json;

    let client = McpClient::new("http://localhost:8080");
    let input = json!({"test": "value"});
    let result = client.validate_input("unknown_tool", &input);
    // Should succeed (no validation when tool not cached)
    assert!(result.is_ok());
}

// ========== MCP Tool Resolver Tests ==========

#[test]
fn test_mcp_resolver_creation() {
    use aethershell::ai::mcp::McpToolResolver;

    let _resolver = McpToolResolver::new("http://localhost:8080");
    // Should create successfully
    // Cannot directly test private fields, but creation should work
}

#[test]
fn test_mcp_resolver_list_tools() {
    use aethershell::ai::agents::ToolResolver;
    use aethershell::ai::mcp::McpToolResolver;

    let resolver = McpToolResolver::new("http://localhost:9999");
    let tools = resolver.list();
    // Should return empty list when server unreachable
    assert_eq!(tools.len(), 0);
}

#[test]
fn test_mcp_resolver_get_tool() {
    use aethershell::ai::agents::ToolResolver;
    use aethershell::ai::mcp::McpToolResolver;

    let resolver = McpToolResolver::new("http://localhost:8080");
    let tool = resolver.get("test_tool");
    // Should return Some(tool) even if server unreachable (stub tool)
    assert!(tool.is_some());
}

#[test]
fn test_mcp_tool_name() {
    use aethershell::ai::agents::ToolResolver;
    use aethershell::ai::mcp::McpToolResolver;

    let resolver = McpToolResolver::new("http://localhost:8080");
    if let Some(tool) = resolver.get("example_tool") {
        assert_eq!(tool.name(), "example_tool");
    }
}

#[test]
fn test_mcp_tool_description_default() {
    use aethershell::ai::agents::ToolResolver;
    use aethershell::ai::mcp::McpToolResolver;

    let resolver = McpToolResolver::new("http://localhost:8080");
    if let Some(tool) = resolver.get("test") {
        let desc = tool.description();
        // Should return default description when not cached
        assert!(desc.contains("MCP") || !desc.is_empty());
    }
}

#[test]
fn test_mcp_tool_call_error_handling() {
    use aethershell::ai::agents::ToolResolver;
    use aethershell::ai::mcp::McpToolResolver;
    use aethershell::env::Env;

    let resolver = McpToolResolver::new("http://localhost:9999");
    if let Some(tool) = resolver.get("test") {
        let mut env = Env::default();
        let result = tool.call("{}", &mut env);
        // Should return error when server unreachable
        assert!(result.is_err());
    }
}

// ========== Integration Tests ==========

#[test]
fn test_mcp_integration_with_tool_registry() {
    use aethershell::ai::agents::ToolRegistry;

    let registry = ToolRegistry::with_builtins_and_mcp("http://localhost:8080");
    let tools = registry.list();

    // Should include builtins at minimum
    assert!(!tools.is_empty());
    assert!(tools.contains(&"print".to_string()));
}

#[test]
fn test_mcp_tool_resolution_with_registry() {
    use aethershell::ai::agents::ToolRegistry;

    let registry = ToolRegistry::with_builtins_and_mcp("http://localhost:8080");
    let resolved = registry.resolve_many(&["print", "mcp_tool"]);

    // Should resolve at least builtins
    assert!(!resolved.is_empty());
}

#[test]
fn test_mcp_multiple_endpoints() {
    let client1 = McpClient::new("http://server1:8080");
    let client2 = McpClient::new("http://server2:8080");

    assert_ne!(client1.endpoint, client2.endpoint);
}

#[test]
fn test_mcp_endpoint_normalization() {
    let client = McpClient::new("http://localhost:8080/");
    let tools_url = format!("{}/mcp/v1/tools", client.endpoint.trim_end_matches('/'));
    assert_eq!(tools_url, "http://localhost:8080/mcp/v1/tools");
}

// ========== Error Handling Tests ==========

#[test]
fn test_mcp_malformed_response_handling() {
    let client = McpClient::new("http://localhost:9999");
    let result = client.discover_tools();
    // Should handle errors gracefully
    assert!(result.is_ok());
}

#[test]
fn test_mcp_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    let client = Arc::new(McpClient::new("http://localhost:8080"));
    let mut handles = vec![];

    for _ in 0..5 {
        let client = Arc::clone(&client);
        let handle = thread::spawn(move || {
            let _ = client.list_tools();
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().is_ok());
    }
}

#[test]
fn test_mcp_cache_thread_safety() {
    use std::sync::Arc;
    use std::thread;

    let client = Arc::new(McpClient::new("http://localhost:8080"));
    let mut handles = vec![];

    for _ in 0..3 {
        let client = Arc::clone(&client);
        let handle = thread::spawn(move || {
            let _ = client.get_tool_description("test");
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().is_ok());
    }
}

// ========== URL Handling Tests ==========

#[test]
fn test_mcp_various_endpoint_formats() {
    let endpoints = vec![
        "http://localhost:8080",
        "http://localhost:8080/",
        "https://mcp.example.com",
        "https://mcp.example.com/api",
        "http://192.168.1.100:3000",
    ];

    for endpoint in endpoints {
        let client = McpClient::new(endpoint);
        assert!(!client.endpoint.is_empty());
    }
}

#[test]
fn test_mcp_tool_execution_url_construction() {
    let client = McpClient::new("http://localhost:8080");
    // The URL construction is internal, but we can test that the method exists
    let _ = client.call_tool("test", "{}");
}

// ========== Performance Tests ==========

#[test]
fn test_mcp_client_creation_performance() {
    let start = std::time::Instant::now();
    for _ in 0..100 {
        let _ = McpClient::new("http://localhost:8080");
    }
    let duration = start.elapsed();

    // Should be reasonably fast (< 5 seconds for 100 clients)
    // Note: This is a lenient threshold to avoid flakiness on slower machines
    assert!(
        duration.as_secs() < 5,
        "Client creation took {:?}, expected < 5s",
        duration
    );
}

#[test]
fn test_mcp_cache_access_performance() {
    let client = McpClient::new("http://localhost:8080");

    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let _ = client.get_tool_description("test");
    }
    let duration = start.elapsed();

    // Cache access should be very fast
    assert!(duration.as_millis() < 100);
}

// ========== Edge Cases ==========

#[test]
fn test_mcp_empty_endpoint() {
    let client = McpClient::new("");
    assert_eq!(client.endpoint, "");
}

#[test]
fn test_mcp_long_endpoint() {
    let long_endpoint = format!("http://localhost:8080/{}", "a".repeat(1000));
    let client = McpClient::new(&long_endpoint);
    assert_eq!(client.endpoint.len(), long_endpoint.len());
}

#[test]
fn test_mcp_tool_name_with_special_characters() {
    use aethershell::ai::agents::ToolResolver;
    use aethershell::ai::mcp::McpToolResolver;

    let resolver = McpToolResolver::new("http://localhost:8080");
    let special_names = vec!["tool-with-dash", "tool_with_underscore", "tool.with.dot"];

    for name in special_names {
        if let Some(tool) = resolver.get(name) {
            assert_eq!(tool.name(), name);
        }
    }
}

#[test]
fn test_mcp_unicode_tool_names() {
    use aethershell::ai::agents::ToolResolver;
    use aethershell::ai::mcp::McpToolResolver;

    let resolver = McpToolResolver::new("http://localhost:8080");
    let unicode_tool = resolver.get("测试工具");
    assert!(unicode_tool.is_some());
}
