//! Native OS tools database for AI agents
//!
//! This module provides a comprehensive database of native operating system tools
//! that can be used by AI agents across Linux, Windows, and macOS platforms.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OperatingSystem {
    Linux,
    Windows,
    MacOS,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    FileSystem,
    TextProcessing,
    NetworkTools,
    SystemInfo,
    ProcessManagement,
    Archives,
    SearchTools,
    Monitoring,
    Development,
    Media,
    Security,
    Utilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OSTool {
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub command: String,
    pub common_args: Vec<String>,
    pub examples: Vec<ToolExample>,
    pub safety_level: SafetyLevel,
    pub requires_admin: bool,
    pub supported_os: Vec<OperatingSystem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExample {
    pub description: String,
    pub command: String,
    pub expected_output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SafetyLevel {
    Safe,      // Read-only operations, cannot cause harm
    Caution,   // Can modify files but limited scope
    Dangerous, // Can cause system-wide changes
    Critical,  // Can cause irreversible damage
}

#[derive(Debug, Clone)]
pub struct OSToolsDatabase {
    pub tools: HashMap<String, OSTool>,
    pub categories: HashMap<ToolCategory, Vec<String>>,
    pub os_specific: HashMap<OperatingSystem, Vec<String>>,
}

impl OSToolsDatabase {
    pub fn new() -> Self {
        let mut db = OSToolsDatabase {
            tools: HashMap::new(),
            categories: HashMap::new(),
            os_specific: HashMap::new(),
        };

        db.populate_tools();
        db.build_indices();
        db
    }

    fn populate_tools(&mut self) {
        // File System Tools
        self.add_tool(OSTool {
            name: "ls".to_string(),
            description: "List directory contents".to_string(),
            category: ToolCategory::FileSystem,
            command: "ls".to_string(),
            common_args: vec!["-la".to_string(), "-lh".to_string(), "-R".to_string()],
            examples: vec![ToolExample {
                description: "List files with details".to_string(),
                command: "ls -la".to_string(),
                expected_output: Some(
                    "drwxr-xr-x 2 user user 4096 Sep  5 10:30 Documents".to_string(),
                ),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
        });

        self.add_tool(OSTool {
            name: "dir".to_string(),
            description: "Display directory contents (Windows)".to_string(),
            category: ToolCategory::FileSystem,
            command: "dir".to_string(),
            common_args: vec!["/A".to_string(), "/S".to_string(), "/Q".to_string()],
            examples: vec![ToolExample {
                description: "List files with attributes".to_string(),
                command: "dir /A".to_string(),
                expected_output: Some("Directory of C:\\Users\\user\\Documents".to_string()),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Windows],
        });

        self.add_tool(OSTool {
            name: "cp".to_string(),
            description: "Copy files and directories".to_string(),
            category: ToolCategory::FileSystem,
            command: "cp".to_string(),
            common_args: vec!["-r".to_string(), "-v".to_string(), "-i".to_string()],
            examples: vec![ToolExample {
                description: "Copy file with confirmation".to_string(),
                command: "cp -i source.txt dest.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
        });

        self.add_tool(OSTool {
            name: "copy".to_string(),
            description: "Copy files (Windows)".to_string(),
            category: ToolCategory::FileSystem,
            command: "copy".to_string(),
            common_args: vec!["/Y".to_string(), "/V".to_string()],
            examples: vec![ToolExample {
                description: "Copy file with verification".to_string(),
                command: "copy /V source.txt dest.txt".to_string(),
                expected_output: Some("1 file(s) copied.".to_string()),
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Windows],
        });

        // Text Processing Tools
        self.add_tool(OSTool {
            name: "grep".to_string(),
            description: "Search text patterns in files".to_string(),
            category: ToolCategory::TextProcessing,
            command: "grep".to_string(),
            common_args: vec![
                "-i".to_string(),
                "-n".to_string(),
                "-r".to_string(),
                "-v".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Search for pattern case-insensitively".to_string(),
                command: "grep -i 'error' logfile.txt".to_string(),
                expected_output: Some("line 42: Error occurred in function".to_string()),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
        });

        self.add_tool(OSTool {
            name: "findstr".to_string(),
            description: "Search for strings in files (Windows)".to_string(),
            category: ToolCategory::TextProcessing,
            command: "findstr".to_string(),
            common_args: vec!["/I".to_string(), "/N".to_string(), "/S".to_string()],
            examples: vec![ToolExample {
                description: "Search for pattern case-insensitively".to_string(),
                command: "findstr /I \"error\" logfile.txt".to_string(),
                expected_output: Some("42:Error occurred in function".to_string()),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Windows],
        });

        self.add_tool(OSTool {
            name: "sed".to_string(),
            description: "Stream editor for filtering and transforming text".to_string(),
            category: ToolCategory::TextProcessing,
            command: "sed".to_string(),
            common_args: vec!["-i".to_string(), "-e".to_string(), "-n".to_string()],
            examples: vec![ToolExample {
                description: "Replace text in file".to_string(),
                command: "sed 's/old/new/g' file.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
        });

        self.add_tool(OSTool {
            name: "awk".to_string(),
            description: "Pattern scanning and processing language".to_string(),
            category: ToolCategory::TextProcessing,
            command: "awk".to_string(),
            common_args: vec!["-F".to_string(), "-v".to_string()],
            examples: vec![ToolExample {
                description: "Print specific columns".to_string(),
                command: "awk '{print $1, $3}' file.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
        });

        // Network Tools
        self.add_tool(OSTool {
            name: "ping".to_string(),
            description: "Test network connectivity".to_string(),
            category: ToolCategory::NetworkTools,
            command: "ping".to_string(),
            common_args: vec!["-c".to_string(), "-i".to_string(), "-W".to_string()],
            examples: vec![ToolExample {
                description: "Ping host 4 times".to_string(),
                command: "ping -c 4 google.com".to_string(),
                expected_output: Some(
                    "PING google.com (172.217.164.110): 56 data bytes".to_string(),
                ),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
        });

        self.add_tool(OSTool {
            name: "curl".to_string(),
            description: "Transfer data from or to servers".to_string(),
            category: ToolCategory::NetworkTools,
            command: "curl".to_string(),
            common_args: vec![
                "-X".to_string(),
                "-H".to_string(),
                "-d".to_string(),
                "-o".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Download file".to_string(),
                command: "curl -o output.html https://example.com".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
        });

        self.add_tool(OSTool {
            name: "netstat".to_string(),
            description: "Display network connections and statistics".to_string(),
            category: ToolCategory::NetworkTools,
            command: "netstat".to_string(),
            common_args: vec!["-a".to_string(), "-n".to_string(), "-p".to_string()],
            examples: vec![ToolExample {
                description: "Show all listening ports".to_string(),
                command: "netstat -an".to_string(),
                expected_output: Some("Active Internet connections".to_string()),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
        });

        // System Information Tools
        self.add_tool(OSTool {
            name: "top".to_string(),
            description: "Display running processes".to_string(),
            category: ToolCategory::SystemInfo,
            command: "top".to_string(),
            common_args: vec!["-n".to_string(), "-p".to_string()],
            examples: vec![ToolExample {
                description: "Show top processes once".to_string(),
                command: "top -n 1".to_string(),
                expected_output: Some("Tasks: 125 total, 1 running, 124 sleeping".to_string()),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
        });

        self.add_tool(OSTool {
            name: "tasklist".to_string(),
            description: "Display running processes (Windows)".to_string(),
            category: ToolCategory::SystemInfo,
            command: "tasklist".to_string(),
            common_args: vec!["/FI".to_string(), "/FO".to_string()],
            examples: vec![ToolExample {
                description: "List all running processes".to_string(),
                command: "tasklist".to_string(),
                expected_output: Some(
                    "Image Name                     PID Session Name        Session#    Mem Usage"
                        .to_string(),
                ),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Windows],
        });

        self.add_tool(OSTool {
            name: "df".to_string(),
            description: "Display filesystem disk space usage".to_string(),
            category: ToolCategory::SystemInfo,
            command: "df".to_string(),
            common_args: vec!["-h".to_string(), "-T".to_string()],
            examples: vec![ToolExample {
                description: "Show disk usage in human readable format".to_string(),
                command: "df -h".to_string(),
                expected_output: Some(
                    "Filesystem      Size  Used Avail Use% Mounted on".to_string(),
                ),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
        });

        self.add_tool(OSTool {
            name: "wmic".to_string(),
            description: "Windows Management Instrumentation Command-line".to_string(),
            category: ToolCategory::SystemInfo,
            command: "wmic".to_string(),
            common_args: vec![
                "logicaldisk".to_string(),
                "process".to_string(),
                "service".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Show disk information".to_string(),
                command: "wmic logicaldisk get size,freespace,caption".to_string(),
                expected_output: Some("Caption  FreeSpace    Size".to_string()),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Windows],
        });

        // Process Management Tools
        self.add_tool(OSTool {
            name: "kill".to_string(),
            description: "Terminate processes by PID".to_string(),
            category: ToolCategory::ProcessManagement,
            command: "kill".to_string(),
            common_args: vec!["-9".to_string(), "-TERM".to_string()],
            examples: vec![ToolExample {
                description: "Gracefully terminate process".to_string(),
                command: "kill 1234".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
        });

        self.add_tool(OSTool {
            name: "taskkill".to_string(),
            description: "Terminate processes (Windows)".to_string(),
            category: ToolCategory::ProcessManagement,
            command: "taskkill".to_string(),
            common_args: vec!["/PID".to_string(), "/IM".to_string(), "/F".to_string()],
            examples: vec![ToolExample {
                description: "Force terminate process by name".to_string(),
                command: "taskkill /IM notepad.exe /F".to_string(),
                expected_output: Some(
                    "SUCCESS: The process \"notepad.exe\" with PID 1234 has been terminated."
                        .to_string(),
                ),
            }],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Windows],
        });

        // Archive Tools
        self.add_tool(OSTool {
            name: "tar".to_string(),
            description: "Archive files and directories".to_string(),
            category: ToolCategory::Archives,
            command: "tar".to_string(),
            common_args: vec!["-czf".to_string(), "-xzf".to_string(), "-tzf".to_string()],
            examples: vec![ToolExample {
                description: "Create gzipped archive".to_string(),
                command: "tar -czf archive.tar.gz directory/".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
        });

        self.add_tool(OSTool {
            name: "zip".to_string(),
            description: "Create ZIP archives".to_string(),
            category: ToolCategory::Archives,
            command: "zip".to_string(),
            common_args: vec!["-r".to_string(), "-9".to_string()],
            examples: vec![ToolExample {
                description: "Create ZIP archive recursively".to_string(),
                command: "zip -r archive.zip directory/".to_string(),
                expected_output: Some("adding: directory/ (stored 0%)".to_string()),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
        });

        // Search Tools
        self.add_tool(OSTool {
            name: "find".to_string(),
            description: "Search for files and directories".to_string(),
            category: ToolCategory::SearchTools,
            command: "find".to_string(),
            common_args: vec![
                "-name".to_string(),
                "-type".to_string(),
                "-size".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Find files by name pattern".to_string(),
                command: "find /home -name '*.txt'".to_string(),
                expected_output: Some("/home/user/document.txt".to_string()),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
        });

        self.add_tool(OSTool {
            name: "where".to_string(),
            description: "Locate files (Windows)".to_string(),
            category: ToolCategory::SearchTools,
            command: "where".to_string(),
            common_args: vec!["/R".to_string()],
            examples: vec![ToolExample {
                description: "Find executable in PATH".to_string(),
                command: "where python".to_string(),
                expected_output: Some("C:\\Python39\\python.exe".to_string()),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Windows],
        });

        // Development Tools
        self.add_tool(OSTool {
            name: "git".to_string(),
            description: "Distributed version control system".to_string(),
            category: ToolCategory::Development,
            command: "git".to_string(),
            common_args: vec!["status".to_string(), "log".to_string(), "diff".to_string()],
            examples: vec![ToolExample {
                description: "Check repository status".to_string(),
                command: "git status".to_string(),
                expected_output: Some(
                    "On branch main\nnothing to commit, working tree clean".to_string(),
                ),
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
        });

        // Media Tools
        self.add_tool(OSTool {
            name: "ffmpeg".to_string(),
            description: "Multimedia framework for processing audio/video".to_string(),
            category: ToolCategory::Media,
            command: "ffmpeg".to_string(),
            common_args: vec!["-i".to_string(), "-c".to_string(), "-f".to_string()],
            examples: vec![ToolExample {
                description: "Convert video format".to_string(),
                command: "ffmpeg -i input.avi output.mp4".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
        });

        // Security Tools
        self.add_tool(OSTool {
            name: "chmod".to_string(),
            description: "Change file permissions".to_string(),
            category: ToolCategory::Security,
            command: "chmod".to_string(),
            common_args: vec!["-R".to_string(), "755".to_string(), "644".to_string()],
            examples: vec![ToolExample {
                description: "Make file executable".to_string(),
                command: "chmod +x script.sh".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
        });

        self.add_tool(OSTool {
            name: "icacls".to_string(),
            description: "Display or modify access control lists (Windows)".to_string(),
            category: ToolCategory::Security,
            command: "icacls".to_string(),
            common_args: vec![
                "/grant".to_string(),
                "/deny".to_string(),
                "/remove".to_string(),
            ],
            examples: vec![ToolExample {
                description: "View file permissions".to_string(),
                command: "icacls file.txt".to_string(),
                expected_output: Some("file.txt BUILTIN\\Users:(I)(RX)".to_string()),
            }],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: true,
            supported_os: vec![OperatingSystem::Windows],
        });
    }

    fn add_tool(&mut self, tool: OSTool) {
        self.tools.insert(tool.name.clone(), tool);
    }

    fn build_indices(&mut self) {
        // Build category index
        for (name, tool) in &self.tools {
            self.categories
                .entry(tool.category.clone())
                .or_insert_with(Vec::new)
                .push(name.clone());
        }

        // Build OS-specific index
        for (name, tool) in &self.tools {
            for os in &tool.supported_os {
                self.os_specific
                    .entry(os.clone())
                    .or_insert_with(Vec::new)
                    .push(name.clone());
            }
        }
    }

    pub fn get_tool(&self, name: &str) -> Option<&OSTool> {
        self.tools.get(name)
    }

    pub fn get_tools_by_category(&self, category: &ToolCategory) -> Vec<&OSTool> {
        if let Some(tool_names) = self.categories.get(category) {
            tool_names
                .iter()
                .filter_map(|name| self.tools.get(name))
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn get_tools_by_os(&self, os: &OperatingSystem) -> Vec<&OSTool> {
        if let Some(tool_names) = self.os_specific.get(os) {
            tool_names
                .iter()
                .filter_map(|name| self.tools.get(name))
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn get_safe_tools(&self) -> Vec<&OSTool> {
        self.tools
            .values()
            .filter(|tool| tool.safety_level == SafetyLevel::Safe)
            .collect()
    }

    pub fn search_tools(&self, query: &str) -> Vec<&OSTool> {
        let query_lower = query.to_lowercase();
        self.tools
            .values()
            .filter(|tool| {
                tool.name.to_lowercase().contains(&query_lower)
                    || tool.description.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    pub fn get_recommended_tools(&self, task_description: &str) -> Vec<&OSTool> {
        let task_lower = task_description.to_lowercase();
        let mut recommendations = Vec::new();

        // Simple keyword-based recommendations
        if task_lower.contains("file") || task_lower.contains("directory") {
            recommendations.extend(self.get_tools_by_category(&ToolCategory::FileSystem));
        }
        if task_lower.contains("text") || task_lower.contains("search") {
            recommendations.extend(self.get_tools_by_category(&ToolCategory::TextProcessing));
        }
        if task_lower.contains("network") || task_lower.contains("connection") {
            recommendations.extend(self.get_tools_by_category(&ToolCategory::NetworkTools));
        }
        if task_lower.contains("process") || task_lower.contains("running") {
            recommendations.extend(self.get_tools_by_category(&ToolCategory::ProcessManagement));
        }

        // Remove duplicates and return
        recommendations.sort_by_key(|tool| &tool.name);
        recommendations.dedup_by_key(|tool| &tool.name);
        recommendations
    }
}

impl Default for OSToolsDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_creation() {
        let db = OSToolsDatabase::new();
        assert!(!db.tools.is_empty());
        assert!(!db.categories.is_empty());
        assert!(!db.os_specific.is_empty());
    }

    #[test]
    fn test_get_tool() {
        let db = OSToolsDatabase::new();
        let ls_tool = db.get_tool("ls");
        assert!(ls_tool.is_some());
        assert_eq!(ls_tool.unwrap().name, "ls");
    }

    #[test]
    fn test_get_tools_by_category() {
        let db = OSToolsDatabase::new();
        let fs_tools = db.get_tools_by_category(&ToolCategory::FileSystem);
        assert!(!fs_tools.is_empty());
    }

    #[test]
    fn test_get_tools_by_os() {
        let db = OSToolsDatabase::new();
        let linux_tools = db.get_tools_by_os(&OperatingSystem::Linux);
        let windows_tools = db.get_tools_by_os(&OperatingSystem::Windows);

        assert!(!linux_tools.is_empty());
        assert!(!windows_tools.is_empty());

        // Verify some tools are OS-specific
        assert!(linux_tools.iter().any(|t| t.name == "ls"));
        assert!(windows_tools.iter().any(|t| t.name == "dir"));
    }

    #[test]
    fn test_get_safe_tools() {
        let db = OSToolsDatabase::new();
        let safe_tools = db.get_safe_tools();
        assert!(!safe_tools.is_empty());

        for tool in safe_tools {
            assert_eq!(tool.safety_level, SafetyLevel::Safe);
        }
    }

    #[test]
    fn test_search_tools() {
        let db = OSToolsDatabase::new();
        let search_results = db.search_tools("list");
        assert!(!search_results.is_empty());
    }

    #[test]
    fn test_get_recommended_tools() {
        let db = OSToolsDatabase::new();
        let recommendations = db.get_recommended_tools("I need to find and copy files");
        assert!(!recommendations.is_empty());
    }

    #[test]
    fn test_tool_safety_levels() {
        let db = OSToolsDatabase::new();

        // Check that dangerous tools are marked appropriately
        let kill_tool = db.get_tool("kill");
        if let Some(tool) = kill_tool {
            assert_eq!(tool.safety_level, SafetyLevel::Dangerous);
        }

        let ls_tool = db.get_tool("ls");
        if let Some(tool) = ls_tool {
            assert_eq!(tool.safety_level, SafetyLevel::Safe);
        }
    }

    #[test]
    fn test_admin_requirements() {
        let db = OSToolsDatabase::new();

        // Most tools should not require admin
        let non_admin_tools: Vec<_> = db
            .tools
            .values()
            .filter(|tool| !tool.requires_admin)
            .collect();

        assert!(!non_admin_tools.is_empty());

        // Some tools should require admin
        let admin_tools: Vec<_> = db
            .tools
            .values()
            .filter(|tool| tool.requires_admin)
            .collect();

        // At least one tool should require admin (like icacls)
        assert!(!admin_tools.is_empty());
    }
}
