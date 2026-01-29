//! Tests for OS tools database functionality

use aethershell::os_tools::*;

#[test]
fn test_database_initialization() {
    let db = OSToolsDatabase::new();

    // Should have tools loaded
    assert!(!db.tools.is_empty());
    println!("Loaded {} tools", db.tools.len());

    // Should have categories indexed
    assert!(!db.categories.is_empty());
    println!("Categories: {:?}", db.categories.keys().collect::<Vec<_>>());

    // Should have OS-specific tools indexed
    assert!(!db.os_specific.is_empty());
    println!(
        "Supported OS: {:?}",
        db.os_specific.keys().collect::<Vec<_>>()
    );
}

#[test]
fn test_cross_platform_tools() {
    let db = OSToolsDatabase::new();

    // Ping should be available on all platforms
    let ping = db.get_tool("ping");
    assert!(ping.is_some());
    let ping_tool = ping.unwrap();
    assert!(ping_tool.supported_os.contains(&OperatingSystem::Linux));
    assert!(ping_tool.supported_os.contains(&OperatingSystem::Windows));
    assert!(ping_tool.supported_os.contains(&OperatingSystem::MacOS));
}

#[test]
fn test_platform_specific_tools() {
    let db = OSToolsDatabase::new();

    // ls should be Unix-only
    let ls = db.get_tool("ls");
    assert!(ls.is_some());
    let ls_tool = ls.unwrap();
    assert!(ls_tool.supported_os.contains(&OperatingSystem::Linux));
    assert!(ls_tool.supported_os.contains(&OperatingSystem::MacOS));
    assert!(!ls_tool.supported_os.contains(&OperatingSystem::Windows));

    // dir should be Windows-only
    let dir = db.get_tool("dir");
    assert!(dir.is_some());
    let dir_tool = dir.unwrap();
    assert!(dir_tool.supported_os.contains(&OperatingSystem::Windows));
    assert!(!dir_tool.supported_os.contains(&OperatingSystem::Linux));
    assert!(!dir_tool.supported_os.contains(&OperatingSystem::MacOS));
}

#[test]
fn test_tool_categories() {
    let db = OSToolsDatabase::new();

    // Test file system tools
    let fs_tools = db.get_tools_by_category(&ToolCategory::FileSystem);
    assert!(!fs_tools.is_empty());
    assert!(fs_tools.iter().any(|t| t.name == "ls" || t.name == "dir"));

    // Test text processing tools
    let text_tools = db.get_tools_by_category(&ToolCategory::TextProcessing);
    assert!(!text_tools.is_empty());
    assert!(text_tools
        .iter()
        .any(|t| t.name == "grep" || t.name == "findstr"));

    // Test network tools
    let net_tools = db.get_tools_by_category(&ToolCategory::NetworkTools);
    assert!(!net_tools.is_empty());
    assert!(net_tools.iter().any(|t| t.name == "ping"));
}

#[test]
fn test_safety_levels() {
    let db = OSToolsDatabase::new();

    // Safe tools should include read-only operations
    let safe_tools = db.get_safe_tools();
    assert!(!safe_tools.is_empty());

    let ls_tool = db.get_tool("ls");
    if let Some(tool) = ls_tool {
        assert_eq!(tool.safety_level, SafetyLevel::Safe);
    }

    let ping_tool = db.get_tool("ping");
    if let Some(tool) = ping_tool {
        assert_eq!(tool.safety_level, SafetyLevel::Safe);
    }

    // Dangerous tools should include process killers
    let kill_tool = db.get_tool("kill");
    if let Some(tool) = kill_tool {
        assert_eq!(tool.safety_level, SafetyLevel::Dangerous);
    }

    let taskkill_tool = db.get_tool("taskkill");
    if let Some(tool) = taskkill_tool {
        assert_eq!(tool.safety_level, SafetyLevel::Dangerous);
    }
}

#[test]
fn test_admin_requirements() {
    let db = OSToolsDatabase::new();

    // Most tools should not require admin
    let non_admin_count = db
        .tools
        .values()
        .filter(|tool| !tool.requires_admin)
        .count();

    let admin_count = db.tools.values().filter(|tool| tool.requires_admin).count();

    println!(
        "Non-admin tools: {}, Admin tools: {}",
        non_admin_count, admin_count
    );
    assert!(non_admin_count > admin_count);

    // icacls should require admin
    let icacls_tool = db.get_tool("icacls");
    if let Some(tool) = icacls_tool {
        assert!(tool.requires_admin);
    }
}

#[test]
fn test_tool_search() {
    let db = OSToolsDatabase::new();

    // Search by name
    let list_results = db.search_tools("list");
    assert!(!list_results.is_empty());

    // Search by description
    let file_results = db.search_tools("file");
    assert!(!file_results.is_empty());

    // Search should be case-insensitive
    let case_results = db.search_tools("FILE");
    assert_eq!(file_results.len(), case_results.len());
}

#[test]
fn test_tool_recommendations() {
    let db = OSToolsDatabase::new();

    // File operations
    let file_recs = db.get_recommended_tools("I need to copy files");
    assert!(!file_recs.is_empty());
    assert!(file_recs
        .iter()
        .any(|t| t.category == ToolCategory::FileSystem));

    // Text processing
    let text_recs = db.get_recommended_tools("search for text in files");
    assert!(!text_recs.is_empty());
    assert!(text_recs
        .iter()
        .any(|t| t.category == ToolCategory::TextProcessing));

    // Network operations
    let net_recs = db.get_recommended_tools("test network connection");
    assert!(!net_recs.is_empty());
    assert!(net_recs
        .iter()
        .any(|t| t.category == ToolCategory::NetworkTools));
}

#[test]
fn test_tool_examples() {
    let db = OSToolsDatabase::new();

    // Every tool should have at least one example
    for (name, tool) in &db.tools {
        assert!(!tool.examples.is_empty(), "Tool {} has no examples", name);

        for example in &tool.examples {
            assert!(
                !example.description.is_empty(),
                "Tool {} has empty example description",
                name
            );
            assert!(
                !example.command.is_empty(),
                "Tool {} has empty example command",
                name
            );
        }
    }
}

#[test]
fn test_tool_args() {
    let db = OSToolsDatabase::new();

    // ls should have common args
    let ls_tool = db.get_tool("ls");
    if let Some(tool) = ls_tool {
        assert!(tool.common_args.contains(&"-la".to_string()));
    }

    // grep should have common args
    let grep_tool = db.get_tool("grep");
    if let Some(tool) = grep_tool {
        assert!(tool.common_args.contains(&"-i".to_string()));
        assert!(tool.common_args.contains(&"-r".to_string()));
    }
}

#[test]
fn test_os_specific_filtering() {
    let db = OSToolsDatabase::new();

    // Linux tools should include Unix commands
    let linux_tools = db.get_tools_by_os(&OperatingSystem::Linux);
    assert!(linux_tools.iter().any(|t| t.name == "ls"));
    assert!(linux_tools.iter().any(|t| t.name == "grep"));
    assert!(linux_tools.iter().any(|t| t.name == "find"));

    // Windows tools should include Windows commands
    let windows_tools = db.get_tools_by_os(&OperatingSystem::Windows);
    assert!(windows_tools.iter().any(|t| t.name == "dir"));
    assert!(windows_tools.iter().any(|t| t.name == "findstr"));
    assert!(windows_tools.iter().any(|t| t.name == "tasklist"));

    // macOS should have similar tools to Linux
    let macos_tools = db.get_tools_by_os(&OperatingSystem::MacOS);
    assert!(macos_tools.iter().any(|t| t.name == "ls"));
    assert!(macos_tools.iter().any(|t| t.name == "grep"));
}

#[test]
fn test_default_implementation() {
    let db1 = OSToolsDatabase::new();
    let db2 = OSToolsDatabase::default();

    // Both should have the same number of tools
    assert_eq!(db1.tools.len(), db2.tools.len());
}

#[test]
fn test_comprehensive_coverage() {
    let db = OSToolsDatabase::new();

    // Should have tools in all major categories
    let expected_categories = vec![
        ToolCategory::FileSystem,
        ToolCategory::TextProcessing,
        ToolCategory::NetworkTools,
        ToolCategory::SystemInfo,
        ToolCategory::ProcessManagement,
        ToolCategory::Archives,
        ToolCategory::SearchTools,
        ToolCategory::Development,
        ToolCategory::Media,
        ToolCategory::Security,
    ];

    for category in expected_categories {
        let tools = db.get_tools_by_category(&category);
        assert!(
            !tools.is_empty(),
            "No tools found for category {:?}",
            category
        );
    }

    // Should have tools for all supported operating systems
    let expected_os = vec![
        OperatingSystem::Linux,
        OperatingSystem::Windows,
        OperatingSystem::MacOS,
    ];

    for os in expected_os {
        let tools = db.get_tools_by_os(&os);
        assert!(!tools.is_empty(), "No tools found for OS {:?}", os);
    }
}
