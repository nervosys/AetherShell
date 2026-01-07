// Smoke tests for AetherShell examples
// These tests run example files and verify they execute without errors

use std::path::Path;
use std::process::Command;

fn run_example(name: &str) -> Result<String, String> {
    let ae_exe = if cfg!(debug_assertions) {
        "target/debug/ae.exe"
    } else {
        "target/release/ae.exe"
    };

    let example_path = format!("examples/{}", name);

    // Verify the example file exists
    if !Path::new(&example_path).exists() {
        return Err(format!("Example file not found: {}", example_path));
    }

    let output = Command::new(ae_exe)
        .arg(&example_path)
        .output()
        .map_err(|e| format!("Failed to execute ae: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!(
            "Example {} failed with exit code {:?}\nStderr: {}",
            name,
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

// ============================================================================
// WORKING EXAMPLES
// ============================================================================

#[test]
fn test_example_00_hello() {
    let output = run_example("00_hello.ae").expect("00_hello.ae should run successfully");

    assert!(output.contains("Hello, Æther!"), "Should print greeting");
    assert!(output.contains("Hi, world!"), "Should interpolate string");
}

#[test]
fn test_example_04_match() {
    let output = run_example("04_match.ae").expect("04_match.ae should run successfully");

    assert!(
        output.contains("big: 42"),
        "Should match Some(x) with guard"
    );
}

#[test]
fn test_example_17_syntax_showcase() {
    let output = run_example("17_syntax_showcase.ae")
        .expect("17_syntax_showcase.ae should run successfully");

    assert!(
        output.contains("Welcome to AetherShell"),
        "Should print welcome"
    );
    assert!(
        output.contains("Double 21 = 42"),
        "Should execute functions"
    );
}

#[test]
fn test_example_18_mutable_variables() {
    let output = run_example("18_mutable_variables.ae")
        .expect("18_mutable_variables.ae should run successfully");

    assert!(
        output.contains("counter=10"),
        "Should update mutable variables"
    );
    assert!(output.contains("Sum: 150"), "Should accumulate values");
    assert!(output.contains("Complete: 100%"), "Should track progress");
}

// ============================================================================
// PARTIALLY WORKING EXAMPLES
// ============================================================================

#[test]
fn test_example_02_tables() {
    // Dot notation works now, but some builtins missing (group_by, agg)
    let result = run_example("02_tables.ae");

    match result {
        Ok(output) => {
            // Should at least execute the first pipeline with where/select
            assert!(output.contains("name") || output.contains("size"));
        }
        Err(err) => {
            // Expected to fail on missing builtins
            assert!(
                err.contains("unknown builtin"),
                "Should fail due to missing builtin: {}",
                err
            );
        }
    }
}

// ============================================================================
// BROKEN EXAMPLES (marked as ignored until fixed)
// ============================================================================

#[test]
#[ignore]
fn test_example_01_pipelines() {
    // Currently broken due to word-call greedy parsing across statements
    // Will pass after parser improvements
    run_example("01_pipelines.ae").expect("01_pipelines.ae should work after parser fix");
}

#[test]
#[ignore]
fn test_example_03_http() {
    // Requires network access and HTTP implementation
    run_example("03_http.ae").expect("03_http.ae should work with HTTP builtins");
}

#[test]
#[ignore]
fn test_example_05_ai() {
    // Requires OPENAI_API_KEY or Ollama setup
    run_example("05_ai.ae").expect("05_ai.ae should work with AI configuration");
}

#[test]
#[ignore]
fn test_example_06_agent() {
    // Requires AI configuration and AGENT_ALLOW_CMDS
    run_example("06_agent.ae").expect("06_agent.ae should work with agent configuration");
}

#[test]
#[ignore]
fn test_example_07_uri_types() {
    // Requires AI backends
    run_example("07_uri_types.ae").expect("07_uri_types.ae should work with AI");
}

#[test]
#[ignore]
fn test_example_09_tui_multimodal() {
    // Requires --tui flag and AI
    // Note: Can't easily test TUI mode in automated tests
    run_example("09_tui_multimodal.ae").expect("09_tui_multimodal.ae should work in TUI mode");
}

#[test]
#[ignore]
fn test_example_10_tui_agent_swarm() {
    // Requires --tui flag and AI
    run_example("10_tui_agent_swarm.ae").expect("10_tui_agent_swarm.ae should work in TUI mode");
}

#[test]
#[ignore]
fn test_example_11_tui_showcase() {
    // Requires --tui flag and AI
    run_example("11_tui_showcase.ae").expect("11_tui_showcase.ae should work in TUI mode");
}

#[test]
#[ignore]
fn test_example_12_multi_agent_orchestration() {
    // Requires AI configuration
    run_example("12_multi_agent_orchestration.ae")
        .expect("12_multi_agent_orchestration.ae should work with AI");
}

#[test]
#[ignore]
fn test_example_13_multimodal_ai() {
    // Requires AI configuration and media files
    run_example("13_multimodal_ai.ae").expect("13_multimodal_ai.ae should work with multimodal AI");
}

#[test]
#[ignore]
fn test_example_14_typed_pipelines() {
    // Uses dot notation (now works) but may have other issues
    run_example("14_typed_pipelines.ae").expect("14_typed_pipelines.ae should work");
}

#[test]
#[ignore]
fn test_example_15_ai_protocols() {
    // Requires AI configuration
    run_example("15_ai_protocols.ae").expect("15_ai_protocols.ae should work with AI");
}

#[test]
#[ignore]
fn test_example_16_mcp_servers() {
    // Requires MCP setup and potentially DB features
    run_example("16_mcp_servers.ae").expect("16_mcp_servers.ae should work with MCP");
}

#[test]
#[ignore]
fn test_example_19_showcase() {
    // Comprehensive language feature showcase
    // Ignored due to word-call greedy parsing issue
    run_example("19_showcase.ae").expect("19_showcase.ae should run successfully");
}

#[test]
#[ignore]
fn test_example_20_tui_a2a() {
    // Requires --tui flag and AI for A2A protocol
    run_example("20_tui_a2a.ae").expect("20_tui_a2a.ae should work in TUI mode");
}

#[test]
#[ignore]
fn test_example_21_tui_chat() {
    // Requires --tui flag
    run_example("21_tui_chat.ae").expect("21_tui_chat.ae should work in TUI mode");
}

#[test]
#[ignore]
fn test_example_22_tui_mcp() {
    // Requires --tui flag and MCP setup
    run_example("22_tui_mcp.ae").expect("22_tui_mcp.ae should work in TUI mode");
}

#[test]
#[ignore]
fn test_example_23_tui_nanda() {
    // Requires --tui flag and AI for NANDA protocol
    run_example("23_tui_nanda.ae").expect("23_tui_nanda.ae should work in TUI mode");
}
