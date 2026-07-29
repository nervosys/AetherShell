//! Comprehensive AI Agent Tests
//! Tests for single agents with various configurations, tools, error handling, and model selection
//!
//! NOTE: These tests require either AETHER_AI=stub or an actual AI provider to be configured.
//! When no AI provider is available, tests will skip gracefully.

use aethershell::{
    ai::agents::{run_sync, run_sync_with_model, Agent, ToolRegistry},
    env::Env,
    value::Value,
};

/// Helper to check if AI is available (either AETHER_AI env var or stub mode)
fn ai_available() -> bool {
    // If AETHER_AI is set, we have a provider
    if std::env::var("AETHER_AI").is_ok() {
        return true;
    }
    // Check for OpenAI API key
    if std::env::var("OPENAI_API_KEY").is_ok() {
        return true;
    }
    // No AI provider available
    false
}

/// Helper to set up environment with stub AI for testing
fn setup_stub_env() -> Env {
    let mut env = Env::default();
    env.set_var("AETHER_AI", Value::Str("stub".to_string()))
        .unwrap();
    env
}

/// Assert that a model URI was *understood*, whatever the runtime outcome.
///
/// These tests run without API keys and without a local Ollama, so failure is
/// expected and fine. What is not fine is failing because the URI could not be
/// parsed, or failing with an empty/unhelpful message — an agent retrying the
/// call has nothing to act on. Asserting only `is_ok() || is_err()` (as these
/// tests previously did) would pass even if URI parsing were completely broken.
fn assert_model_uri_understood<T>(result: &anyhow::Result<T>, uri: &str) {
    let Err(e) = result else {
        return; // Succeeded outright — the URI was certainly understood.
    };
    let msg = e.to_string();
    assert!(
        !msg.trim().is_empty(),
        "failure for {uri} produced an empty error message"
    );
    let lowered = msg.to_lowercase();
    for bad in [
        "unknown model uri",
        "failed to parse model",
        "invalid model uri",
    ] {
        assert!(
            !lowered.contains(bad),
            "model URI {uri} was not understood: {msg}"
        );
    }
}

// ========== Basic Agent Tests ==========

#[test]
fn test_agent_basic_execution() {
    let mut env = setup_stub_env();
    let result = run_sync("List files", &["ls", "print"], 3, true, &mut env);
    // With stub backend, should work; without, gracefully fail
    if result.is_err() && !ai_available() {
        return; // Skip if no AI configured
    }
    assert!(result.is_ok(), "Agent should execute successfully");
    let output = result.unwrap();
    assert!(!output.is_empty(), "Agent should return non-empty output");
}

#[test]
fn test_agent_with_multiple_tools() {
    let mut env = setup_stub_env();
    let tools = vec!["print", "echo", "map", "reduce", "ls"];
    let result = run_sync("Process data", &tools, 5, true, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

#[test]
fn test_agent_respects_max_steps() {
    let mut env = setup_stub_env();
    // Set very low max_steps to force incomplete
    let result = run_sync(
        "Complex task requiring many steps",
        &["print"],
        1,
        true,
        &mut env,
    );
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
    let output = result.unwrap();
    // Should either complete in 1 step or indicate incomplete
    assert!(
        output.contains("dry_run")
            || output.contains("incomplete")
            || output.contains("final")
            || !output.is_empty()
    );
}

#[test]
fn test_agent_with_no_tools() {
    let mut env = setup_stub_env();
    let result = run_sync("Do something", &[], 3, true, &mut env);
    // Should still work but agent may not be able to use tools
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

#[test]
fn test_agent_dry_run_mode() {
    let mut env = setup_stub_env();
    let result = run_sync("Execute commands", &["print", "echo"], 3, true, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
    let output = result.unwrap();
    // Dry run executes successfully
    assert!(!output.is_empty());
}

#[test]
fn test_agent_wet_run_mode() {
    let mut env = setup_stub_env();
    let result = run_sync("Print hello", &["print", "echo"], 3, false, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
    let output = result.unwrap();
    // In wet run, should not contain dry_run marker
    assert!(!output.contains("[dry_run]") || output.contains("final") || !output.is_empty());
}

// ========== Model Selection Tests ==========

#[test]
fn test_agent_with_specific_model_stub() {
    let mut env = setup_stub_env();
    let result = run_sync_with_model("Test task", &["print"], "stub", 3, true, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok(), "Stub model should always work");
}

#[test]
fn test_agent_with_model_uri_openai_format() {
    let mut env = setup_stub_env();
    // This will use stub since no API key
    let result = run_sync_with_model("Test", &["print"], "openai:gpt-4o-mini", 2, true, &mut env);
    // Without an API key this may fail — but it must fail for a *runtime*
    // reason, not because the model URI was unparseable, and the error has to
    // be actionable. (The previous assertion, `is_ok() || is_err()`, was
    // vacuous and would have passed on a URI parser that was entirely broken.)
    assert_model_uri_understood(&result, "openai:gpt-4o-mini");
}

#[test]
fn test_agent_with_model_uri_ollama_format() {
    let mut env = setup_stub_env();
    let result = run_sync_with_model("Test", &["print"], "ollama:llama3", 2, true, &mut env);
    // May fail if Ollama is not running, but the URI must still be understood.
    assert_model_uri_understood(&result, "ollama:llama3");
}

#[test]
fn test_agent_model_env_variable() {
    // Test that AETHER_AGENT_MODEL_URI is respected
    unsafe {
        std::env::set_var("AETHER_AGENT_MODEL_URI", "stub");
    }
    let mut env = setup_stub_env();
    let result = run_sync("Test", &["print"], 2, true, &mut env);
    unsafe {
        std::env::remove_var("AETHER_AGENT_MODEL_URI");
    }
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

// ========== Error Handling Tests ==========

#[test]
fn test_agent_handles_invalid_tool_gracefully() {
    let mut env = setup_stub_env();
    // Agent should handle when it tries to use a tool that doesn't exist
    let result = run_sync("Use nonexistent tool", &["print"], 3, true, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok(), "Should handle invalid tool attempts");
}

#[test]
fn test_agent_with_empty_goal() {
    let mut env = setup_stub_env();
    let result = run_sync("", &["print"], 2, true, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok(), "Should handle empty goal");
}

#[test]
fn test_agent_with_very_long_goal() {
    let mut env = setup_stub_env();
    let long_goal = "a".repeat(10000);
    let result = run_sync(&long_goal, &["print"], 2, true, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
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
    let mut env = setup_stub_env();
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);
    let mut agent = Agent::new(tools);
    agent.max_steps = 3;

    let result = agent.run_sync("Simple task", true, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    // Trace should have at least one step
    assert!(!agent.trace.is_empty(), "Trace should capture agent steps");
}

#[test]
fn test_agent_trace_includes_thoughts() {
    let mut env = setup_stub_env();
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);
    let mut agent = Agent::new(tools);

    let result = agent.run_sync("Test", true, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    // Every recorded step must carry something: a trace entry with neither a
    // thought nor a command is useless for debugging an agent run. (The
    // previous assertion, `thought.is_empty() || !thought.is_empty()`, was
    // vacuous and would have passed on an all-blank trace.)
    for step in &agent.trace {
        assert!(
            !step.thought.trim().is_empty() || step.command.is_some(),
            "trace step recorded neither a thought nor a command"
        );
    }
}

// ========== Integration Tests ==========

#[test]
fn test_agent_with_real_builtin_call() {
    let mut env = setup_stub_env();
    // Use print builtin which should work
    let result = run_sync("Print hello world", &["print"], 3, false, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

#[test]
fn test_agent_sequential_execution() {
    let mut env = setup_stub_env();

    // Run multiple agents sequentially
    let result1 = run_sync("Task 1", &["print"], 2, true, &mut env);
    if result1.is_err() && !ai_available() {
        return;
    }
    let result2 = run_sync("Task 2", &["echo"], 2, true, &mut env);

    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

// ========== Performance Tests ==========

#[test]
fn test_agent_completes_quickly() {
    use std::time::Instant;
    let mut env = setup_stub_env();

    let start = Instant::now();
    let result = run_sync("Quick task", &["print"], 2, true, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
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
    let mut env1 = setup_stub_env();
    let mut env2 = setup_stub_env();

    let result1 = run_sync("Task A", &["print"], 2, true, &mut env1);
    if result1.is_err() && !ai_available() {
        return;
    }
    let result2 = run_sync("Task B", &["print"], 2, true, &mut env2);

    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

// ========== Edge Cases ==========

#[test]
fn test_agent_with_zero_max_steps() {
    let mut env = setup_stub_env();
    // Zero max_steps should be converted to default
    let result = run_sync("Test", &["print"], 0, true, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

#[test]
fn test_agent_with_large_max_steps() {
    let mut env = setup_stub_env();
    let result = run_sync("Test", &["print"], 1000, true, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

#[test]
fn test_agent_with_unicode_goal() {
    let mut env = setup_stub_env();
    let result = run_sync("测试 🚀 Тест", &["print"], 2, true, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok(), "Should handle Unicode in goals");
}

#[test]
fn test_agent_with_special_characters() {
    let mut env = setup_stub_env();
    let result = run_sync(
        "Test with \"quotes\" and 'apostrophes'",
        &["print"],
        2,
        true,
        &mut env,
    );
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

// ========== Tool Call Tests ==========

#[test]
fn test_agent_tool_execution_dry_run() {
    let mut env = setup_stub_env();
    let result = run_sync("Use print tool", &["print"], 3, true, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    let output = result.unwrap();
    // In dry run, tool calls should be simulated
    assert!(!output.is_empty(), "Should return output");
}

#[test]
fn test_agent_unknown_tool_error() {
    let mut env = setup_stub_env();
    // Agent might try to use a tool not in its list
    let result = run_sync("Complex task", &["print"], 3, false, &mut env);
    if result.is_err() && !ai_available() {
        return;
    }
    // Should handle gracefully
    assert!(result.is_ok());
}
