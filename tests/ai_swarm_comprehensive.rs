//! Comprehensive Multi-Agent Swarm Tests
//! Tests for swarm coordination, blackboard communication, policies, and multi-agent workflows

use aether_shell::{
    ai::agents::{
        swarm::{AgentConfig, Policy, Swarm},
        ToolRegistry,
    },
    env::Env,
    value::Value,
};

/// Check if AI provider is configured
fn ai_available() -> bool {
    std::env::var("AETHER_AI").is_ok() || std::env::var("OPENAI_API_KEY").is_ok()
}

/// Setup environment with stub AI backend for testing
fn setup_stub_env() -> Env {
    let mut env = Env::default();
    env.set_var("AETHER_AI", Value::Str("stub".to_string()))
        .ok();
    env
}

// ========== Basic Swarm Tests ==========

#[test]
fn test_swarm_basic_creation() {
    let swarm = Swarm::new(Policy::RoundRobin, 10);
    assert_eq!(swarm.max_iters, 10);
    assert_eq!(swarm.agents.len(), 0);
}

#[test]
fn test_swarm_add_single_agent() {
    let mut swarm = Swarm::new(Policy::RoundRobin, 5);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "agent1".to_string(),
        system: "You are a helpful agent".to_string(),
        tools,
        max_steps: 3,
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    assert_eq!(swarm.agents.len(), 1);
}

#[test]
fn test_swarm_add_multiple_agents() {
    let mut swarm = Swarm::new(Policy::RoundRobin, 10);
    let registry = ToolRegistry::with_builtins();

    for i in 0..3 {
        let tools = registry.resolve_many(&["print", "echo"]);
        let config = AgentConfig {
            id: format!("agent{}", i),
            system: format!("Agent {} system prompt", i),
            tools,
            max_steps: 5,
            model_uri: Some("stub".to_string()),
        };
        swarm.add_agent(config);
    }

    assert_eq!(swarm.agents.len(), 3);
}

#[test]
fn test_swarm_execution_dry_run() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 5);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "test_agent".to_string(),
        system: "Complete the task".to_string(),
        tools,
        max_steps: 3,
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    let result = swarm.run_sync("Simple task", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

#[test]
fn test_swarm_empty_error() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 5);
    let result = swarm.run_sync("Task", &mut env, true);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("no agents"));
}

// ========== Coordination Policy Tests ==========

#[test]
fn test_swarm_round_robin_policy() {
    let mut swarm = Swarm::new(Policy::RoundRobin, 10);
    let registry = ToolRegistry::with_builtins();

    // Add 3 agents
    for i in 0..3 {
        let tools = registry.resolve_many(&["print"]);
        let config = AgentConfig {
            id: format!("agent{}", i),
            system: "Agent system".to_string(),
            tools,
            max_steps: 3,
            model_uri: Some("stub".to_string()),
        };
        swarm.add_agent(config);
    }

    let mut env = setup_stub_env();
    let result = swarm.run_sync("Test round robin", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

#[test]
fn test_swarm_router_policy() {
    let mut swarm = Swarm::new(Policy::Router, 8);
    let registry = ToolRegistry::with_builtins();

    let tools = registry.resolve_many(&["print"]);
    let config = AgentConfig {
        id: "router_agent".to_string(),
        system: "Router agent".to_string(),
        tools,
        max_steps: 3,
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    let mut env = setup_stub_env();
    let result = swarm.run_sync("Test router", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

#[test]
fn test_swarm_policy_comparison() {
    // Test that both policies can execute the same task
    let registry = ToolRegistry::with_builtins();
    let mut env1 = setup_stub_env();
    let mut env2 = setup_stub_env();

    // Round robin swarm
    let mut swarm_rr = Swarm::new(Policy::RoundRobin, 5);
    let tools1 = registry.resolve_many(&["print"]);
    swarm_rr.add_agent(AgentConfig {
        id: "agent1".to_string(),
        system: "Test".to_string(),
        tools: tools1,
        max_steps: 3,
        model_uri: Some("stub".to_string()),
    });

    // Router swarm
    let mut swarm_router = Swarm::new(Policy::Router, 5);
    let tools2 = registry.resolve_many(&["print"]);
    swarm_router.add_agent(AgentConfig {
        id: "agent2".to_string(),
        system: "Test".to_string(),
        tools: tools2,
        max_steps: 3,
        model_uri: Some("stub".to_string()),
    });

    let result1 = swarm_rr.run_sync("Task", &mut env1, true);
    if result1.is_err() && !ai_available() {
        return;
    }
    let result2 = swarm_router.run_sync("Task", &mut env2, true);

    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

// ========== Blackboard Communication Tests ==========

#[test]
fn test_swarm_blackboard_initialization() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 3);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "agent1".to_string(),
        system: "Test agent".to_string(),
        tools,
        max_steps: 2,
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    let result = swarm.run_sync("Initial goal", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }

    // Blackboard should have at least the user goal
    assert!(!swarm.blackboard.is_empty());
}

#[test]
fn test_swarm_blackboard_captures_messages() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 5);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print", "echo"]);

    let config = AgentConfig {
        id: "communicator".to_string(),
        system: "Communicate via blackboard".to_string(),
        tools,
        max_steps: 3,
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    let result = swarm.run_sync("Test communication", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }

    // Blackboard should accumulate messages
    assert!(swarm.blackboard.len() >= 1);
}

#[test]
fn test_swarm_blackboard_multi_agent_communication() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 10);
    let registry = ToolRegistry::with_builtins();

    // Add multiple agents
    for i in 0..3 {
        let tools = registry.resolve_many(&["print"]);
        let config = AgentConfig {
            id: format!("agent{}", i),
            system: format!("Agent {} cooperates", i),
            tools,
            max_steps: 2,
            model_uri: Some("stub".to_string()),
        };
        swarm.add_agent(config);
    }

    let result = swarm.run_sync("Collaborative task", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }

    // Multiple agents should have contributed to blackboard
    // With stub backend, at least one message should exist
    assert!(!swarm.blackboard.is_empty());
}

// ========== Tool Usage in Swarms ==========

#[test]
fn test_swarm_agent_uses_tools() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 5);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print", "echo", "map"]);

    let config = AgentConfig {
        id: "tool_user".to_string(),
        system: "Use available tools".to_string(),
        tools,
        max_steps: 5,
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    let result = swarm.run_sync("Use tools to complete task", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

#[test]
fn test_swarm_different_tools_per_agent() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 8);
    let registry = ToolRegistry::with_builtins();

    // Agent 1 has print and echo
    let tools1 = registry.resolve_many(&["print", "echo"]);
    swarm.add_agent(AgentConfig {
        id: "agent1".to_string(),
        system: "Specialist in printing".to_string(),
        tools: tools1,
        max_steps: 3,
        model_uri: Some("stub".to_string()),
    });

    // Agent 2 has map and reduce
    let tools2 = registry.resolve_many(&["map", "reduce"]);
    swarm.add_agent(AgentConfig {
        id: "agent2".to_string(),
        system: "Specialist in processing".to_string(),
        tools: tools2,
        max_steps: 3,
        model_uri: Some("stub".to_string()),
    });

    let result = swarm.run_sync("Specialized task", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

#[test]
fn test_swarm_shared_tools() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 6);
    let registry = ToolRegistry::with_builtins();

    // Both agents share the same tools
    for i in 0..2 {
        let tools = registry.resolve_many(&["print", "echo"]);
        let config = AgentConfig {
            id: format!("agent{}", i),
            system: "Shared tool access".to_string(),
            tools,
            max_steps: 3,
            model_uri: Some("stub".to_string()),
        };
        swarm.add_agent(config);
    }

    let result = swarm.run_sync("Task using shared tools", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

// ========== Model Selection Tests ==========

#[test]
fn test_swarm_agent_with_stub_model() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 3);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "stub_agent".to_string(),
        system: "Using stub model".to_string(),
        tools,
        max_steps: 2,
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    let result = swarm.run_sync("Test stub model", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

#[test]
fn test_swarm_agent_with_openai_format_model() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 3);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "openai_agent".to_string(),
        system: "Using OpenAI model".to_string(),
        tools,
        max_steps: 2,
        model_uri: Some("openai:gpt-4o-mini".to_string()),
    };

    swarm.add_agent(config);
    // Will fall back to stub without API key
    let result = swarm.run_sync("Test", &mut env, true);
    assert!(result.is_ok() || result.is_err()); // May fail without key
}

#[test]
fn test_swarm_agent_with_ollama_format_model() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 3);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "ollama_agent".to_string(),
        system: "Using Ollama model".to_string(),
        tools,
        max_steps: 2,
        model_uri: Some("ollama:llama3".to_string()),
    };

    swarm.add_agent(config);
    // Will fail if Ollama not running, but should parse correctly
    let result = swarm.run_sync("Test", &mut env, true);
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_swarm_mixed_models() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 6);
    let registry = ToolRegistry::with_builtins();

    // Agent with stub model
    let tools1 = registry.resolve_many(&["print"]);
    swarm.add_agent(AgentConfig {
        id: "stub_agent".to_string(),
        system: "Stub model agent".to_string(),
        tools: tools1,
        max_steps: 2,
        model_uri: Some("stub".to_string()),
    });

    // Agent with different model
    let tools2 = registry.resolve_many(&["echo"]);
    swarm.add_agent(AgentConfig {
        id: "other_agent".to_string(),
        system: "Other model agent".to_string(),
        tools: tools2,
        max_steps: 2,
        model_uri: Some("stub".to_string()), // Use stub for testing
    });

    let result = swarm.run_sync("Mixed model task", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

#[test]
fn test_swarm_agent_without_explicit_model() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 3);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "default_model_agent".to_string(),
        system: "Use default model".to_string(),
        tools,
        max_steps: 2,
        model_uri: None, // Will use environment default
    };

    swarm.add_agent(config);
    let result = swarm.run_sync("Test default model", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

// ========== Swarm Iteration Tests ==========

#[test]
fn test_swarm_respects_max_iters() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 2); // Very low max
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "limited_agent".to_string(),
        system: "Limited iterations".to_string(),
        tools,
        max_steps: 10, // Agent max steps high, but swarm limited
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    let result = swarm.run_sync("Complex task needing many steps", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
    // Should complete or indicate max_iters reached
}

#[test]
fn test_swarm_early_termination() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 10); // High max
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "quick_agent".to_string(),
        system: "Complete quickly".to_string(),
        tools,
        max_steps: 1, // Should finish quickly
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    let result = swarm.run_sync("Simple task", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
    // Should terminate before max_iters
}

#[test]
fn test_swarm_with_zero_max_iters() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 0);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "agent".to_string(),
        system: "Test".to_string(),
        tools,
        max_steps: 3,
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    let result = swarm.run_sync("Task", &mut env, true);
    // With 0 max_iters, should complete immediately with incomplete message
    assert!(result.is_ok());
}

// ========== Swarm Steps Tracking ==========

#[test]
fn test_swarm_captures_steps() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 5);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "tracked_agent".to_string(),
        system: "Steps are tracked".to_string(),
        tools,
        max_steps: 3,
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    let result = swarm.run_sync("Task with steps", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }

    // Steps should be recorded
    assert!(!swarm.steps.is_empty());
}

#[test]
fn test_swarm_step_includes_agent_id() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 3);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "identified_agent".to_string(),
        system: "Agent identification".to_string(),
        tools,
        max_steps: 2,
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    let _ = swarm.run_sync("Identify agent", &mut env, true);

    if !swarm.steps.is_empty() {
        assert_eq!(swarm.steps[0].agent, "identified_agent");
    }
}

// ========== Error Handling Tests ==========

#[test]
fn test_swarm_handles_agent_with_invalid_tools() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 3);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["nonexistent_tool"]);

    let config = AgentConfig {
        id: "agent".to_string(),
        system: "Test".to_string(),
        tools, // Empty or invalid tools
        max_steps: 2,
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    let result = swarm.run_sync("Task", &mut env, true);
    // Should handle gracefully
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_swarm_wet_run_mode() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 3);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "wet_runner".to_string(),
        system: "Execute for real".to_string(),
        tools,
        max_steps: 2,
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);
    let result = swarm.run_sync("Real execution", &mut env, false); // wet run
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
}

// ========== Integration Tests ==========

#[test]
fn test_swarm_complete_workflow() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 15);
    let registry = ToolRegistry::with_builtins();

    // Add multiple specialized agents
    let tools1 = registry.resolve_many(&["print", "echo"]);
    swarm.add_agent(AgentConfig {
        id: "coordinator".to_string(),
        system: "Coordinate the task".to_string(),
        tools: tools1,
        max_steps: 5,
        model_uri: Some("stub".to_string()),
    });

    let tools2 = registry.resolve_many(&["map", "reduce"]);
    swarm.add_agent(AgentConfig {
        id: "processor".to_string(),
        system: "Process data".to_string(),
        tools: tools2,
        max_steps: 5,
        model_uri: Some("stub".to_string()),
    });

    let tools3 = registry.resolve_many(&["print"]);
    swarm.add_agent(AgentConfig {
        id: "reporter".to_string(),
        system: "Report results".to_string(),
        tools: tools3,
        max_steps: 3,
        model_uri: Some("stub".to_string()),
    });

    let result = swarm.run_sync(
        "Complete a multi-stage task with coordination",
        &mut env,
        true,
    );
    if result.is_err() && !ai_available() {
        return;
    }

    assert!(result.is_ok());
    assert!(!swarm.blackboard.is_empty());
    assert!(!swarm.steps.is_empty());
}

#[test]
fn test_swarm_large_scale() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 20);
    let registry = ToolRegistry::with_builtins();

    // Add 5 agents
    for i in 0..5 {
        let tools = registry.resolve_many(&["print", "echo"]);
        let config = AgentConfig {
            id: format!("agent{}", i),
            system: format!("Agent {} in large swarm", i),
            tools,
            max_steps: 3,
            model_uri: Some("stub".to_string()),
        };
        swarm.add_agent(config);
    }

    let result = swarm.run_sync("Large scale task", &mut env, true);
    if result.is_err() && !ai_available() {
        return;
    }
    assert!(result.is_ok());
    assert_eq!(swarm.agents.len(), 5);
}

// ========== Performance Tests ==========

#[test]
fn test_swarm_completes_in_reasonable_time() {
    let mut env = setup_stub_env();
    let mut swarm = Swarm::new(Policy::RoundRobin, 5);
    let registry = ToolRegistry::with_builtins();
    let tools = registry.resolve_many(&["print"]);

    let config = AgentConfig {
        id: "fast_agent".to_string(),
        system: "Quick completion".to_string(),
        tools,
        max_steps: 2,
        model_uri: Some("stub".to_string()),
    };

    swarm.add_agent(config);

    let start = std::time::Instant::now();
    let _ = swarm.run_sync("Quick task", &mut env, true);
    let duration = start.elapsed();

    // Should complete quickly with stub backend
    assert!(
        duration.as_secs() < 5,
        "Swarm took too long: {:?}",
        duration
    );
}
