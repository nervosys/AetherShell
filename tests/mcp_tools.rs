//! Test suite for MCP tool use functionality
//!
//! Tests the Model Context Protocol implementation including:
//! - Tool listing and filtering
//! - Tool execution via call_tool
//! - Resources and prompts
//! - Category and safety level filtering

use aethershell::mcp::{McpConfig, McpServer, McpToolCall};
use aethershell::os_tools::{OSToolsDatabase, SafetyLevel, ToolCategory};
use std::collections::HashMap;
use std::sync::Mutex;

/// Serializes tests that mutate the process-global `AETHER_MODE` env var, so they
/// can't clear each other's mode mid-call when run in parallel.
static ENV_LOCK: Mutex<()> = Mutex::new(());

// ============================================================================
// Server Creation Tests
// ============================================================================

#[test]
fn test_mcp_server_default_creation() {
    let server = McpServer::new();
    let info = server.initialize();

    assert_eq!(info.server_info.name, "aethershell-mcp");
    assert_eq!(info.protocol_version, "2024-11-05");
    assert!(server.tool_count() > 0, "Server should have tools");
}

#[test]
fn test_mcp_server_with_config() {
    let config = McpConfig {
        max_safety_level: SafetyLevel::Safe,
        allow_admin_tools: false,
        allowed_categories: None,
        blocked_tools: vec!["nmap".to_string()],
        execution_timeout: 30,
    };
    let server = McpServer::with_config(config);
    let tools = server.list_tools();

    // Should not contain blocked tools
    assert!(!tools.iter().any(|t| t.name == "nmap"));
}

#[test]
fn test_mcp_server_default_config() {
    let server = McpServer::default();
    let tools = server.list_tools();

    // Default config blocks dangerous tools
    assert!(!tools.iter().any(|t| t.name == "rm"));
    assert!(!tools.iter().any(|t| t.name == "dd"));
}

// ============================================================================
// Tool Listing Tests
// ============================================================================

#[test]
fn test_list_tools_returns_tools() {
    let server = McpServer::new();
    let tools = server.list_tools();

    assert!(!tools.is_empty(), "Tool list should not be empty");
    assert!(tools.len() > 50, "Should have substantial number of tools");
}

#[test]
fn test_list_tools_have_required_fields() {
    let server = McpServer::new();
    let tools = server.list_tools();

    for tool in tools.iter().take(10) {
        assert!(!tool.name.is_empty(), "Tool name should not be empty");
        assert!(
            !tool.description.is_empty(),
            "Tool description should not be empty"
        );
        assert!(
            tool.input_schema.is_object(),
            "Tool input_schema should be a JSON object"
        );
    }
}

#[test]
fn test_get_tool_by_name() {
    let server = McpServer::new();

    // Test getting a known tool
    let git_tool = server.get_tool("git");
    assert!(git_tool.is_some(), "Should find 'git' tool");

    let git = git_tool.unwrap();
    assert_eq!(git.name, "git");
}

#[test]
fn test_get_nonexistent_tool() {
    let server = McpServer::new();

    let tool = server.get_tool("nonexistent_tool_xyz");
    assert!(tool.is_none(), "Should not find nonexistent tool");
}

// ============================================================================
// Tool Filtering Tests
// ============================================================================

#[test]
fn test_filter_tools_by_category() {
    let server = McpServer::new();

    let dev_tools = server.get_tools_by_category(&ToolCategory::Development);
    assert!(!dev_tools.is_empty(), "Should have development tools");

    // Known dev tools should be present
    let tool_names: Vec<&str> = dev_tools.iter().map(|t| t.name.as_str()).collect();
    assert!(
        tool_names.contains(&"cargo") || tool_names.contains(&"npm") || tool_names.contains(&"git"),
        "Development category should contain common dev tools"
    );
}

#[test]
fn test_filter_tools_by_multiple_categories() {
    let server = McpServer::new();

    let fs_tools = server.get_tools_by_category(&ToolCategory::FileSystem);
    let net_tools = server.get_tools_by_category(&ToolCategory::NetworkTools);
    let dev_tools = server.get_tools_by_category(&ToolCategory::Development);

    // Each category should have tools
    assert!(!fs_tools.is_empty(), "Should have filesystem tools");
    assert!(!net_tools.is_empty(), "Should have network tools");
    assert!(!dev_tools.is_empty(), "Should have development tools");

    // Categories should be different
    let total = fs_tools.len() + net_tools.len() + dev_tools.len();
    let all_tools = server.list_tools();
    assert!(
        total <= all_tools.len(),
        "Category totals should not exceed total tools"
    );
}

// ============================================================================
// Tool Execution Tests
// ============================================================================

#[test]
fn test_call_tool_basic() {
    let server = McpServer::new();

    // Call a simple tool that should work on all systems
    let call = McpToolCall {
        name: "git".to_string(),
        arguments: {
            let mut args = HashMap::new();
            args.insert(
                "command".to_string(),
                serde_json::Value::String("--version".to_string()),
            );
            args
        },
    };
    let result = server.call_tool(call);

    // Should return a result
    assert!(!result.content.is_empty(), "Result should have content");
}

#[test]
fn test_call_tool_returns_content() {
    let server = McpServer::new();

    let call = McpToolCall {
        name: "git".to_string(),
        arguments: {
            let mut args = HashMap::new();
            args.insert(
                "command".to_string(),
                serde_json::Value::String("--version".to_string()),
            );
            args
        },
    };
    let result = server.call_tool(call);

    // Result should have content
    assert!(!result.content.is_empty(), "Should have content");
}

#[test]
fn test_call_nonexistent_tool() {
    let server = McpServer::new();

    let call = McpToolCall {
        name: "nonexistent_tool_xyz".to_string(),
        arguments: HashMap::new(),
    };
    let result = server.call_tool(call);

    // Should return error
    assert!(
        result.is_error.unwrap_or(false),
        "Calling nonexistent tool should return error"
    );
}

#[test]
fn test_call_blocked_tool() {
    let server = McpServer::new();

    // rm is blocked by default
    let call = McpToolCall {
        name: "rm".to_string(),
        arguments: HashMap::new(),
    };
    let result = server.call_tool(call);

    // Should return error (blocked by security policy)
    assert!(
        result.is_error.unwrap_or(false),
        "Calling blocked tool should return error"
    );
}

// ============================================================================
// Resources Tests
// ============================================================================

#[test]
fn test_list_resources() {
    let server = McpServer::new();
    let resources = server.list_resources();

    assert!(!resources.is_empty(), "Should have resources");
}

#[test]
fn test_resources_have_required_fields() {
    let server = McpServer::new();
    let resources = server.list_resources();

    for resource in &resources {
        assert!(!resource.uri.is_empty(), "Resource URI should not be empty");
        assert!(
            !resource.name.is_empty(),
            "Resource name should not be empty"
        );
    }
}

#[test]
fn test_read_resource_tools() {
    let server = McpServer::new();

    let content = server.read_resource("aethershell://tools");
    assert!(content.is_ok(), "Should read tools resource");
}

#[test]
fn test_read_resource_categories() {
    let server = McpServer::new();

    let content = server.read_resource("aethershell://categories");
    assert!(content.is_ok(), "Should read categories resource");
}

#[test]
fn test_read_resource_system_info() {
    let server = McpServer::new();

    let content = server.read_resource("aethershell://system-info");
    assert!(content.is_ok(), "Should read system-info resource");
}

#[test]
fn test_read_nonexistent_resource() {
    let server = McpServer::new();

    let content = server.read_resource("aethershell://nonexistent");
    assert!(content.is_err(), "Should not find nonexistent resource");
}

// ============================================================================
// Prompts Tests
// ============================================================================

#[test]
fn test_list_prompts() {
    let server = McpServer::new();
    let prompts = server.list_prompts();

    assert!(!prompts.is_empty(), "Should have prompts");
}

#[test]
fn test_prompts_have_required_fields() {
    let server = McpServer::new();
    let prompts = server.list_prompts();

    for prompt in &prompts {
        assert!(!prompt.name.is_empty(), "Prompt name should not be empty");
    }
}

#[test]
fn test_get_prompt_find_tool() {
    let server = McpServer::new();

    let mut args = HashMap::new();
    args.insert("task".to_string(), "compress files".to_string());

    let prompt = server.get_prompt("find-tool", &args);
    assert!(prompt.is_ok(), "Should get find-tool prompt");
}

#[test]
fn test_get_prompt_explain_tool() {
    let server = McpServer::new();

    let mut args = HashMap::new();
    args.insert("tool_name".to_string(), "git".to_string());

    let prompt = server.get_prompt("explain-tool", &args);
    assert!(prompt.is_ok(), "Should get explain-tool prompt");
}

#[test]
fn test_get_prompt_missing_args() {
    let server = McpServer::new();

    let prompt = server.get_prompt("find-tool", &HashMap::new());
    assert!(prompt.is_err(), "Should fail without required args");
}

// ============================================================================
// OS Tools Integration Tests
// ============================================================================

#[test]
fn test_os_tools_integration() {
    let os_tools = OSToolsDatabase::new();

    // Should have tools
    assert!(
        !os_tools.tools.is_empty(),
        "OSToolsDatabase should have tools"
    );

    // Should have categories
    assert!(!os_tools.categories.is_empty(), "Should have categories");
}

#[test]
fn test_os_tools_get_by_name() {
    let os_tools = OSToolsDatabase::new();

    let git = os_tools.get_tool("git");
    assert!(git.is_some(), "Should find git tool");
}

#[test]
fn test_os_tools_search() {
    let os_tools = OSToolsDatabase::new();

    let results = os_tools.search_tools("kubernetes");
    assert!(!results.is_empty(), "Should find kubernetes-related tools");
}

#[test]
fn test_os_tools_by_category() {
    let os_tools = OSToolsDatabase::new();

    let dev_tools = os_tools.get_tools_by_category(&ToolCategory::Development);
    assert!(!dev_tools.is_empty(), "Should have development tools");
}

// ============================================================================
// Safety Level Tests
// ============================================================================

#[test]
fn test_safety_levels_exist() {
    // Test that safety levels are distinct
    let safe = SafetyLevel::Safe;
    let caution = SafetyLevel::Caution;
    let dangerous = SafetyLevel::Dangerous;
    let critical = SafetyLevel::Critical;

    assert!(safe != caution);
    assert!(caution != dangerous);
    assert!(dangerous != critical);
}

#[test]
fn test_safe_config_restricts_tools() {
    let safe_config = McpConfig {
        max_safety_level: SafetyLevel::Safe,
        allow_admin_tools: false,
        allowed_categories: None,
        blocked_tools: vec![],
        execution_timeout: 30,
    };
    let safe_server = McpServer::with_config(safe_config);
    let safe_tools = safe_server.list_tools();

    let full_config = McpConfig {
        max_safety_level: SafetyLevel::Critical,
        allow_admin_tools: true,
        allowed_categories: None,
        blocked_tools: vec![],
        execution_timeout: 30,
    };
    let full_server = McpServer::with_config(full_config);
    let full_tools = full_server.list_tools();

    assert!(
        safe_tools.len() <= full_tools.len(),
        "Safe config ({}) should have <= tools than full config ({})",
        safe_tools.len(),
        full_tools.len()
    );
}

#[test]
fn test_blocked_tools_config() {
    let config = McpConfig {
        max_safety_level: SafetyLevel::Critical,
        allow_admin_tools: true,
        allowed_categories: None,
        blocked_tools: vec!["git".to_string(), "cargo".to_string()],
        execution_timeout: 30,
    };
    let server = McpServer::with_config(config);
    let tools = server.list_tools();

    // Blocked tools should not be present
    assert!(
        !tools.iter().any(|t| t.name == "git"),
        "git should be blocked"
    );
    assert!(
        !tools.iter().any(|t| t.name == "cargo"),
        "cargo should be blocked"
    );
}

#[test]
fn test_category_restriction_config() {
    let config = McpConfig {
        max_safety_level: SafetyLevel::Critical,
        allow_admin_tools: true,
        allowed_categories: Some(vec![ToolCategory::Development]),
        blocked_tools: vec![],
        execution_timeout: 30,
    };
    let server = McpServer::with_config(config);
    let tools = server.list_tools();

    // Should only have development tools
    assert!(!tools.is_empty(), "Should have some tools");

    // All tools should be in development category
    for tool in &tools {
        let os_tool = server.tools_db().get_tool(&tool.name);
        if let Some(t) = os_tool {
            assert_eq!(
                t.category,
                ToolCategory::Development,
                "Tool {} should be in Development category",
                tool.name
            );
        }
    }
}

// ============================================================================
// Tool Count and Category Tests
// ============================================================================

#[test]
fn test_tool_count_consistency() {
    let server = McpServer::new();
    let tools = server.list_tools();

    assert_eq!(
        server.tool_count(),
        tools.len(),
        "Tool count should match actual tool count"
    );
}

#[test]
fn test_expected_categories_present() {
    let server = McpServer::new();

    let expected_categories = [
        ToolCategory::FileSystem,
        ToolCategory::Development,
        ToolCategory::NetworkTools,
        ToolCategory::Security,
        ToolCategory::Containers,
        ToolCategory::MachineLearning,
    ];

    for category in expected_categories {
        let cat_tools = server.get_tools_by_category(&category);
        assert!(
            !cat_tools.is_empty(),
            "Should have tools in category: {:?}",
            category
        );
    }
}

#[test]
fn test_common_tools_available() {
    let server = McpServer::new();

    let common_tools = ["git", "curl", "jq", "docker", "npm", "cargo"];

    for tool_name in common_tools {
        let tool = server.get_tool(tool_name);
        assert!(
            tool.is_some(),
            "Common tool '{}' should be available",
            tool_name
        );
    }
}

// ============================================================================
// Edge Cases and Robustness Tests
// ============================================================================

#[test]
fn test_get_tool_case_sensitivity() {
    let server = McpServer::new();

    // Tool names should be case-sensitive (lowercase)
    let git_lower = server.get_tool("git");
    let git_upper = server.get_tool("GIT");

    assert!(git_lower.is_some(), "Should find 'git' with lowercase");
    assert!(git_upper.is_none(), "Should not find 'GIT' with uppercase");
}

#[test]
fn test_empty_tool_call_arguments() {
    let server = McpServer::new();

    // Some tools work without arguments
    let call = McpToolCall {
        name: "git".to_string(),
        arguments: HashMap::new(),
    };
    let result = server.call_tool(call);

    // Should return some result (might be error or success)
    assert!(!result.content.is_empty(), "Should have some response");
}

#[test]
fn test_tools_db_accessor() {
    let server = McpServer::new();

    let db = server.tools_db();
    assert!(!db.tools.is_empty(), "Tools DB should have tools");
}

#[test]
fn test_initialize_returns_correct_info() {
    let server = McpServer::new();
    let info = server.initialize();

    assert_eq!(info.protocol_version, "2024-11-05");
    assert!(info.capabilities.tools.is_some());
    assert!(info.capabilities.resources.is_some());
    assert!(info.capabilities.prompts.is_some());
}

#[test]
fn test_concurrent_server_access() {
    use std::thread;

    let handles: Vec<_> = (0..4)
        .map(|_| {
            thread::spawn(|| {
                let server = McpServer::new();
                let tools = server.list_tools();
                tools.len()
            })
        })
        .collect();

    let counts: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // All threads should get same tool count
    assert!(
        counts.iter().all(|&c| c == counts[0]),
        "Concurrent access should be consistent"
    );
}

// ============================================================================
// AetherShell builtins as MCP tools (effect-tagged, safety-guarded)
// ============================================================================

fn builtin_text(res: &aethershell::mcp::McpToolResult) -> String {
    match res.content.first() {
        Some(aethershell::mcp::McpContent::Text { text }) => text.clone(),
        other => panic!("expected text content, got {other:?}"),
    }
}

#[test]
fn builtin_tools_listed_with_effect_metadata() {
    let server = McpServer::new();
    let tools = server.list_builtin_tools();
    assert!(!tools.is_empty(), "builtins exposed as MCP tools");

    let canonical = tools
        .iter()
        .find(|t| t.name == "canonical")
        .expect("canonical exposed as a tool");
    assert!(
        canonical
            .input_schema
            .get("x-effect")
            .and_then(|v| v.as_str())
            .is_some(),
        "every tool carries an x-effect annotation"
    );

    // A destructive builtin is advertised as such before any call.
    let del = tools
        .iter()
        .find(|t| t.name == "db_sqlite_delete")
        .expect("db_sqlite_delete exposed");
    assert_eq!(
        del.input_schema.get("x-effect").and_then(|v| v.as_str()),
        Some("destructive")
    );
}

#[test]
fn call_builtin_executes_a_pure_builtin() {
    let server = McpServer::new();
    let res = server.call_builtin("canonical", &serde_json::json!([42]));
    assert_eq!(res.is_error, Some(false));
    assert_eq!(builtin_text(&res), "42");
}

#[test]
fn call_builtin_gates_destructive_in_agent_mode() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("AETHER_MODE", "agent");
    let server = McpServer::new();
    let res = server.call_builtin("db_sqlite_delete", &serde_json::json!(["x.db", "t", "1=1"]));
    std::env::remove_var("AETHER_MODE");

    assert_eq!(res.is_error, Some(true), "destructive call is gated");
    assert!(
        builtin_text(&res).contains("E_NEEDS_APPROVAL"),
        "refusal carries the structured code: {}",
        builtin_text(&res)
    );
}

#[test]
fn call_builtin_renders_tabular_as_aecon_in_agent_mode() {
    // In agent mode an array-of-records result comes back as compact AECON text
    // (header once) rather than the verbose per-row display — mirroring the CLI.
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("AETHER_MODE", "agent");
    let server = McpServer::new();
    let res = server.call_builtin(
        "pick",
        &serde_json::json!([
            [{"name": "a", "size": 1, "kind": "x"}, {"name": "b", "size": 2, "kind": "x"}],
            "name",
            "size"
        ]),
    );
    let text = builtin_text(&res);
    std::env::remove_var("AETHER_MODE");

    assert_eq!(res.is_error, Some(false), "pick succeeds: {text}");
    assert!(text.starts_with("name\tsize"), "rendered as AECON: {text}");
    assert!(
        text.contains("\na\t1") && text.contains("\nb\t2"),
        "rows present: {text}"
    );
}
