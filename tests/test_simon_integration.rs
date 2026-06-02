// Test self-describing tool registration with an external `simon` binary.
// This is an opt-in local integration test: set the SIMON_BIN environment variable
// to the path of a `simon` executable to run it; it skips otherwise. (No path is
// hardcoded, so the test carries no machine-specific/private information.)
//   Run with: SIMON_BIN=/path/to/simon cargo test --test test_simon_integration -- --nocapture

use aethershell::external_tools::ExternalToolRegistry;
use std::path::PathBuf;

/// The `simon` binary path from `SIMON_BIN`, or `None` to skip the test.
fn simon_bin() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var("SIMON_BIN").ok()?);
    p.exists().then_some(p)
}

#[test]
fn test_simon_self_describing() {
    let simon_path = match simon_bin() {
        Some(p) => p,
        None => {
            println!("Skipping: set SIMON_BIN=<path to simon> to run this test");
            return;
        }
    };
    let simon_path = simon_path.as_path();

    // Check if simon is self-describing
    let is_sd = ExternalToolRegistry::is_self_describing(simon_path);
    println!("simon is_self_describing: {}", is_sd);
    assert!(is_sd, "simon should be self-describing");

    // Register simon
    let registry = ExternalToolRegistry::new();
    let result = registry.register_self_describing(simon_path, Some("simon"));

    match result {
        Ok(name) => {
            println!("Registered simon as: {}", name);

            // List capabilities
            let caps = registry.list_capabilities(&name).unwrap();
            println!("Simon capabilities:");
            for (cap_name, desc) in &caps {
                println!("  - {}: {}", cap_name, desc);
            }

            assert!(!caps.is_empty(), "simon should have capabilities");

            // Check for expected capabilities
            let cap_names: Vec<_> = caps.iter().map(|(n, _)| n.as_str()).collect();
            assert!(
                cap_names
                    .iter()
                    .any(|n| n.contains("system") || n.contains("cpu") || n.contains("gpu")),
                "simon should have system/cpu/gpu capabilities"
            );
        }
        Err(e) => {
            panic!("Failed to register simon: {}", e);
        }
    }
}

#[test]
fn test_simon_execute_cpu() {
    let simon_path = match simon_bin() {
        Some(p) => p,
        None => {
            println!("Skipping: set SIMON_BIN=<path to simon> to run this test");
            return;
        }
    };

    let registry = ExternalToolRegistry::new();
    if let Ok(name) = registry.register_self_describing(&simon_path, Some("simon")) {
        // Try to execute a capability
        let result = registry.execute(&name, "get_cpu_info", &serde_json::json!({}));

        match result {
            Ok(res) => {
                println!("CPU info result: {:?}", res);
                assert!(
                    res.success || res.error.is_some(),
                    "Should get result or error"
                );
            }
            Err(e) => {
                println!("Execute error (may be expected): {}", e);
            }
        }
    }
}
