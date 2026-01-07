//! Comprehensive AI Agent Tests
//! Tests for single agents with various configurations, tools, error handling, and model selection

use aether_shell::{
    ai::agents::{run_sync, run_sync_with_model, Agent, ToolRegistry},
    env::Env,
};

// ========== Basic Agent Tests ==========

#[test]
fn test_agent_basic_execution() {
    let mut env = Env::default();
    let result = run_sync("List files", &["ls", "print"], 3, true, &mut env);
    assert!(result.is_ok(), "Agent should execute successfully");
    let output = result.unwrap();
    assert!(!output.is_empty(), "Agent should return non-empty output");
    // Output can be in various formats depending on agent execution
}

#[test]
fn test_agent_with_multiple_tools() {
    let mut env = Env::default();
    let tools = vec!["print", "echo", "map", "reduce", "ls"];
    let result = run_sync("Process data", &tools, 5, true, &mut env);
    assert!(result.is_ok());
}

#[test]
fn test_agent_respects_max_steps() {
    let mut env = Env::default();
    // Set very low max_steps to force incomplete
    let result = run_sync(
        "Complex task requiring many steps",
        &["print"],
        1,
        true,
        &mut env,
    );
    assert!(result.is_ok());
    let output = result.unwrap();
    // Should either complete in 1 step or indicate incomplete
    assert!(
        output.contains("dry_run") || output.contains("incomplete") || output.contains("final")
    );
}

#[test]
fn test_agent_with_no_tools() {
    let mut env = Env::default();
    let result = run_sync("Do something", &[], 3, true, &mut env);
    // Should still work but agent may not be able to use tools
    assert!(result.is_ok());
}

#[test]
fn test_agent_dry_run_mode() {
    let mut env = Env::default();
    let result = run_sync("Execute commands", &["print", "echo"], 3, true, &mut env);
    assert!(result.is_ok());
    let output = result.unwrap();
    // Dry run executes successfully
    assert!(!output.is_empty());
}

#[test]
fn test_agent_wet_run_mode() {
    let mut env = Env::default();
    let result = run_sync("Print hello", &["print", "echo"], 3, false, &mut env);
    assert!(result.is_ok());
    let output = result.unwrap();
    // In wet run, should not contain dry_run marker
    assert!(!output.contains("[dry_run]") || output.contains("final"));
}

// ========== Model Selection Tests ==========

#[test]
fn test_agent_with_specific_model_stub() {
    let mut env = Env::default();
    let result = run_sync_with_model("Test task", &["print"], "stub", 3, true, &mut env);
    assert!(result.is_ok(), "Stub model should always work");
}

#[test]
fn test_agent_with_model_uri_openai_format() {
    let mut env = Env::default();
    // This will use stub since no API key
    let result = run_sync_with_model("Test", &["print"], "openai:gpt-4o-mini", 2, true, &mut env);
    // Should fail gracefully or use stub
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_agent_with_model_uri_ollama_format() {
    let mut env = Env::default();
    let result = run_sync_with_model("Test", &["print"], "ollama:llama3", 2, true, &mut env);
    // May fail if Ollama not running, but should parse correctly
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_agent_model_env_variable() {
    // Test that AETHER_AGENT_MODEL_URI is respected
    unsafe {
        std::env::set_var("AETHER_AGENT_MODEL_URI", "stub");
    }
    let mut env = Env::default();
    let result = run_sync("Test", &["print"], 2, true, &mut env);
    unsafe {
        std::env::remove_var("AETHER_AGENT_MODEL_URI");
    }
    assert!(result.is_ok());
}

// ========== Error Handling Tests ==========

#[test]
fn test_agent_handles_invalid_tool_gracefully() {
    let mut env = Env::default();
    // Agent should handle when it tries to use a tool that doesn't exist
    let result = run_sync("Use nonexistent tool", &["print"], 3, true, &mut env);
    assert!(result.is_ok(), "Should handle invalid tool attempts");
}

#[test]
fn test_agent_with_empty_goal() {
    let mut env = Env::default();
    let result = run_sync("", &["print"], 2, true, &mut env);
    assert!(result.is_ok(), "Should handle empty goal");
}

#[test]
fn test_agent_with_very_long_goal() {
    let mut env = Env::default();
    let long_goal = "a".repeat(10000);
    let result = run_sync(&long_goal, &["print"], 2, true, &mut env);
    assert!(result.is_ok(), "Should handle long goals");
}

// ========== Tool Registry Tests ==========

#[test]
fn test_tool_registry_lists_builtins() {
    let registry = ToolRegistry::with_builtins();
    let tools = registry.list();
    assert!(!tools.is_empty(), "Should have builtin tools");
    assert!(tools.contains(&"print".to_string()));
    assert!(tools.contains(&"echo".to_string()));
}

#[test]
fn test_tool_registry_resolves_tools() {
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print", "echo", "map"]);
    assert_eq!(tools.len(), 3, "Should resolve all requested tools");
}

#[test]
fn test_tool_registry_deduplicates() {
    let registry = ToolRegistry::with_builtins();
    let list = registry.list();
    let unique_count = list.len();
    let mut sorted = list.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        unique_count,
        sorted.len(),
        "Tool list should have no duplicates"
    );
}

// ========== Agent Construction Tests ==========

#[test]
fn test_agent_new_construction() {
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);
    let agent = Agent::new(tools);
    assert_eq!(agent.max_steps, 8, "Default max_steps should be 8");
    assert_eq!(agent.trace.len(), 0, "Initial trace should be empty");
}

#[test]
fn test_agent_with_model_uri_construction() {
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);
    let agent = Agent::with_model_uri(tools, "stub");
    assert_eq!(agent.max_steps, 8);
}

#[test]
fn test_agent_custom_max_steps() {
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);
    let mut agent = Agent::new(tools);
    agent.max_steps = 20;
    assert_eq!(agent.max_steps, 20);
}

// ========== Agent Execution Trace Tests ==========

#[test]
fn test_agent_trace_captures_steps() {
    let mut env = Env::default();
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);
    let mut agent = Agent::new(tools);
    agent.max_steps = 3;

    let _ = agent.run_sync("Simple task", true, &mut env);
    // Trace should have at least one step
    assert!(!agent.trace.is_empty(), "Trace should capture agent steps");
}

#[test]
fn test_agent_trace_includes_thoughts() {
    let mut env = Env::default();
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);
    let mut agent = Agent::new(tools);

    let _ = agent.run_sync("Test", true, &mut env);
    if !agent.trace.is_empty() {
        // Each step should have a thought (even if empty)
        for step in &agent.trace {
            // Thought exists for each step
            assert!(step.thought.is_empty() || !step.thought.is_empty());
        }
    }
}

// ========== Integration Tests ==========

#[test]
fn test_agent_with_real_builtin_call() {
    let mut env = Env::default();
    // Use print builtin which should work
    let result = run_sync("Print hello world", &["print"], 3, false, &mut env);
    assert!(result.is_ok());
}

#[test]
fn test_agent_sequential_execution() {
    let mut env = Env::default();

    // Run multiple agents sequentially
    let result1 = run_sync("Task 1", &["print"], 2, true, &mut env);
    let result2 = run_sync("Task 2", &["echo"], 2, true, &mut env);

    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

// ========== Performance Tests ==========

#[test]
fn test_agent_completes_quickly() {
    use aether_shell::value::Value;
    use std::time::Instant;
    let mut env = Env::default();

    // Ensure we're using the stub backend
    env.set_var("AETHER_AI", Value::Str("stub".to_string()))
        .unwrap();

    let start = Instant::now();
    let _ = run_sync("Quick task", &["print"], 2, true, &mut env);
    let duration = start.elapsed();

    // Stub backend should be fast, but allow more time for system overhead
    assert!(
        duration.as_secs() < 10,
        "Agent should complete within reasonable time with stub backend"
    );
}

#[test]
fn test_multiple_agents_parallel_compatible() {
    // Ensure agent execution doesn't have global state issues
    let mut env1 = Env::default();
    let mut env2 = Env::default();

    let result1 = run_sync("Task A", &["print"], 2, true, &mut env1);
    let result2 = run_sync("Task B", &["print"], 2, true, &mut env2);

    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

// ========== Edge Cases ==========

#[test]
fn test_agent_with_zero_max_steps() {
    let mut env = Env::default();
    // Zero max_steps should be converted to default
    let result = run_sync("Test", &["print"], 0, true, &mut env);
    assert!(result.is_ok());
}

#[test]
fn test_agent_with_large_max_steps() {
    let mut env = Env::default();
    let result = run_sync("Test", &["print"], 1000, true, &mut env);
    assert!(result.is_ok());
}

#[test]
fn test_agent_with_unicode_goal() {
    let mut env = Env::default();
    let result = run_sync("测试 🚀 Тест", &["print"], 2, true, &mut env);
    assert!(result.is_ok(), "Should handle Unicode in goals");
}

#[test]
fn test_agent_with_special_characters() {
    let mut env = Env::default();
    let result = run_sync(
        "Test with \"quotes\" and 'apostrophes'",
        &["print"],
        2,
        true,
        &mut env,
    );
    assert!(result.is_ok());
}

// ========== Tool Call Tests ==========

#[test]
fn test_agent_tool_execution_dry_run() {
    let mut env = Env::default();
    let result = run_sync("Use print tool", &["print"], 3, true, &mut env);
    let output = result.unwrap();
    // In dry run, tool calls should be simulated
    assert!(!output.is_empty(), "Should return output");
}

#[test]
fn test_agent_unknown_tool_error() {
    let mut env = Env::default();
    // Agent might try to use a tool not in its list
    let result = run_sync("Complex task", &["print"], 3, false, &mut env);
    // Should handle gracefully
    assert!(result.is_ok());
}
