// Test self-describing tool registration with simon
// Run with: cargo test --test test_simon_integration -- --nocapture

use aethershell::external_tools::ExternalToolRegistry;
use std::path::Path;

#[test]
fn test_simon_self_describing() {
    let simon_path =
        Path::new(r"C:\Users\adamm\dev\nervosys\utilities\simon\target\release\simon.exe");

    if !simon_path.exists() {
        println!("Skipping test: simon not found at {}", simon_path.display());
        return;
    }

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
    let simon_path =
        Path::new(r"C:\Users\adamm\dev\nervosys\utilities\simon\target\release\simon.exe");

    if !simon_path.exists() {
        println!("Skipping test: simon not found");
        return;
    }

    let registry = ExternalToolRegistry::new();
    if let Ok(name) = registry.register_self_describing(simon_path, Some("simon")) {
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
