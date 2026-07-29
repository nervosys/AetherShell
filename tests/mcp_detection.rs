//! Tests for MCP Server Detection
//! Tests the auto-detection of MCP servers and integration with AI backends

use aethershell::ai::{detect_mcp_servers, McpServerInfo};

// ========== Basic MCP Detection Tests ==========

#[test]
fn test_mcp_server_info_structure() {
    let info = McpServerInfo {
        name: "test".to_string(),
        endpoint: "http://localhost:3001".to_string(),
        available: true,
        tools: vec!["read_file".to_string(), "write_file".to_string()],
    };

    assert_eq!(info.name, "test");
    assert_eq!(info.endpoint, "http://localhost:3001");
    assert!(info.available);
    assert_eq!(info.tools.len(), 2);
}

#[test]
fn test_detect_mcp_servers_returns_vec() {
    // The list may legitimately be empty (no server running), so the property
    // under test is that every entry it *does* return is well-formed.
    for s in detect_mcp_servers() {
        assert!(!s.name.is_empty(), "detected server has no name");
        assert!(!s.endpoint.is_empty(), "detected server has no endpoint");
    }
}

#[test]
fn test_detect_mcp_servers_with_no_servers() {
    // Graceful degradation: with nothing listening, detection returns rather
    // than erroring, and never reports a server as available without an
    // endpoint to reach it at.
    for s in detect_mcp_servers() {
        assert!(
            !s.available || !s.endpoint.is_empty(),
            "server {} marked available with no endpoint",
            s.name
        );
    }
}

#[test]
fn test_mcp_server_info_clone() {
    let info = McpServerInfo {
        name: "test".to_string(),
        endpoint: "http://localhost:3001".to_string(),
        available: true,
        tools: vec!["tool1".to_string()],
    };

    let cloned = info.clone();
    assert_eq!(info.name, cloned.name);
    assert_eq!(info.endpoint, cloned.endpoint);
    assert_eq!(info.available, cloned.available);
    assert_eq!(info.tools, cloned.tools);
}

// ========== MCP Detection Port Scanning Tests ==========

#[test]
fn test_mcp_detection_scans_standard_ports() {
    // Should scan ports 3001-3005 and 8080-8081. With no server running the
    // list is legitimately empty, so the property under test is that detection
    // completes and returns a well-formed list rather than panicking or hanging.
    let servers = detect_mcp_servers();
    for s in &servers {
        assert!(
            !s.endpoint.is_empty(),
            "a detected server needs an endpoint"
        );
    }
}

#[test]
fn test_mcp_detection_handles_unreachable_servers() {
    // Unreachable servers must be filtered out or reported unavailable — never
    // surfaced as usable. Reaching the end of this test is itself the
    // "did not panic / did not hang" assertion.
    for s in detect_mcp_servers() {
        if s.available {
            assert!(
                s.endpoint.starts_with("http://") || s.endpoint.starts_with("https://"),
                "available server {} has an unusable endpoint {}",
                s.name,
                s.endpoint
            );
        }
    }
}

// ========== MCP Server Properties Tests ==========

#[test]
fn test_mcp_server_has_required_fields() {
    let info = McpServerInfo {
        name: "filesystem".to_string(),
        endpoint: "http://localhost:3001".to_string(),
        available: false,
        tools: vec![],
    };

    // All fields should be accessible
    let _ = &info.name;
    let _ = &info.endpoint;
    let _ = info.available;
    let _ = &info.tools;
}

#[test]
fn test_mcp_server_tools_can_be_empty() {
    let info = McpServerInfo {
        name: "test".to_string(),
        endpoint: "http://localhost:3001".to_string(),
        available: true,
        tools: vec![],
    };

    assert!(info.tools.is_empty());
}

#[test]
fn test_mcp_server_tools_can_have_multiple_entries() {
    let tools = vec![
        "read_file".to_string(),
        "write_file".to_string(),
        "list_dir".to_string(),
        "search".to_string(),
    ];

    let info = McpServerInfo {
        name: "filesystem".to_string(),
        endpoint: "http://localhost:3001".to_string(),
        available: true,
        tools: tools.clone(),
    };

    assert_eq!(info.tools.len(), 4);
    assert_eq!(info.tools[0], "read_file");
    assert_eq!(info.tools[3], "search");
}

// ========== MCP Detection Endpoint Tests ==========

#[test]
fn test_mcp_standard_endpoint_format() {
    let info = McpServerInfo {
        name: "test".to_string(),
        endpoint: "http://localhost:3001".to_string(),
        available: true,
        tools: vec![],
    };

    assert!(info.endpoint.starts_with("http://"));
    assert!(info.endpoint.contains("localhost"));
}

#[test]
fn test_mcp_detection_checks_common_ports() {
    // Standard MCP ports: 3001-3005, 8080-8081
    let servers = detect_mcp_servers();

    // If any servers are found, they should be on standard ports
    for server in servers {
        let has_standard_port = server.endpoint.contains(":3001")
            || server.endpoint.contains(":3002")
            || server.endpoint.contains(":3003")
            || server.endpoint.contains(":3004")
            || server.endpoint.contains(":3005")
            || server.endpoint.contains(":8080")
            || server.endpoint.contains(":8081");

        assert!(
            has_standard_port,
            "Server {} is not on standard port",
            server.endpoint
        );
    }
}

// ========== MCP Detection Integration Tests ==========

#[test]
fn test_mcp_detection_integration_with_ai_backends() {
    use aethershell::ai::{detect_available_backends, detect_mcp_servers};

    let ai_backends = detect_available_backends();
    let mcp_servers = detect_mcp_servers();

    // The two detectors are independent: neither may return malformed entries
    // because the other ran. Both lists may be empty on a bare machine.
    for b in &ai_backends {
        assert!(!b.name.is_empty(), "backend name must not be blank");
    }
    for s in &mcp_servers {
        assert!(!s.name.is_empty(), "server name must not be blank");
    }
}

#[test]
fn test_mcp_detection_does_not_block_ai_detection() {
    use aethershell::ai::{auto_select_backend, detect_mcp_servers};

    // MCP detection must not interfere with AI backend selection: whatever
    // `auto_select_backend` returns on its own, it must still return after MCP
    // detection has run.
    let before = auto_select_backend();
    let _ = detect_mcp_servers();
    let after = auto_select_backend();

    assert_eq!(
        before, after,
        "MCP detection changed which AI backend gets selected"
    );
    if let Some(name) = after {
        assert!(!name.is_empty(), "selected backend must be named");
    }
}

// ========== MCP Detection Timeout Tests ==========

#[test]
fn test_mcp_detection_completes_in_reasonable_time() {
    use std::time::Instant;

    let start = Instant::now();
    let _ = detect_mcp_servers();
    let duration = start.elapsed();

    // Should complete within 20 seconds (7 endpoints * 2s timeout + overhead)
    assert!(
        duration.as_secs() < 20,
        "Detection took too long: {:?}",
        duration
    );
}

// ========== MCP Server Name Tests ==========

#[test]
fn test_mcp_server_names_are_descriptive() {
    let servers = detect_mcp_servers();

    for server in servers {
        // Names should not be empty
        assert!(!server.name.is_empty());

        // Names should be lowercase and descriptive
        let valid_names = [
            "filesystem",
            "git",
            "docker",
            "aws",
            "database",
            "custom1",
            "custom2",
        ];

        assert!(
            valid_names.contains(&server.name.as_str()),
            "Unexpected server name: {}",
            server.name
        );
    }
}

// ========== MCP Detection Resilience Tests ==========

#[test]
fn test_mcp_detection_handles_network_errors_gracefully() {
    // Detection should not panic on network errors
    let result = std::panic::catch_unwind(detect_mcp_servers);

    assert!(result.is_ok(), "MCP detection panicked on network errors");
}

#[test]
fn test_mcp_detection_returns_only_available_servers() {
    let servers = detect_mcp_servers();

    // All returned servers should be marked as available
    for server in servers {
        assert!(
            server.available,
            "Server {} marked as unavailable but included in results",
            server.name
        );
    }
}

// ========== MCP Detection Concurrent Access Tests ==========

#[test]
fn test_mcp_detection_is_thread_safe() {
    use std::thread;

    let handles: Vec<_> = (0..3).map(|_| thread::spawn(detect_mcp_servers)).collect();

    for handle in handles {
        let result = handle.join();
        assert!(result.is_ok(), "Thread panicked during MCP detection");
    }
}

// ========== MCP Detection Edge Cases ==========

#[test]
fn test_mcp_server_info_debug_format() {
    let info = McpServerInfo {
        name: "test".to_string(),
        endpoint: "http://localhost:3001".to_string(),
        available: true,
        tools: vec!["tool1".to_string()],
    };

    let debug_str = format!("{:?}", info);
    assert!(debug_str.contains("test"));
    assert!(debug_str.contains("localhost:3001"));
}

#[test]
fn test_mcp_detection_repeated_calls_are_consistent() {
    let servers1 = detect_mcp_servers();
    let servers2 = detect_mcp_servers();

    // Should return same number of servers (assuming no servers started/stopped)
    // Note: This might fail in CI if timing is different, but good for local testing
    assert_eq!(servers1.len(), servers2.len());
}
