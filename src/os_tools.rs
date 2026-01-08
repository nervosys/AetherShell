//! Native OS tools database for AI agents
//!
//! This module provides a comprehensive database of native operating system tools
//! that can be used by AI agents across Linux, BSD, macOS, Windows, iOS, and Android.
//! Includes cross-platform command translation for seamless operation across OSes.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;

/// Supported operating systems for tool execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OperatingSystem {
    Linux,
    BSD, // FreeBSD, OpenBSD, NetBSD
    MacOS,
    Windows,
    iOS,     // Limited shell access (jailbroken/development)
    Android, // Via Termux or ADB shell
}

impl OperatingSystem {
    /// Detect the current operating system at runtime
    pub fn current() -> Self {
        #[cfg(target_os = "linux")]
        {
            // Check if we're on Android
            if std::path::Path::new("/system/build.prop").exists() {
                return OperatingSystem::Android;
            }
            OperatingSystem::Linux
        }
        #[cfg(target_os = "macos")]
        {
            OperatingSystem::MacOS
        }
        #[cfg(target_os = "ios")]
        {
            OperatingSystem::iOS
        }
        #[cfg(target_os = "windows")]
        {
            OperatingSystem::Windows
        }
        #[cfg(target_os = "freebsd")]
        {
            OperatingSystem::BSD
        }
        #[cfg(target_os = "openbsd")]
        {
            OperatingSystem::BSD
        }
        #[cfg(target_os = "netbsd")]
        {
            OperatingSystem::BSD
        }
        #[cfg(target_os = "android")]
        {
            OperatingSystem::Android
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "ios",
            target_os = "windows",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "android"
        )))]
        {
            OperatingSystem::Linux // Default fallback
        }
    }

    /// Check if this OS is Unix-like (shares common tools)
    pub fn is_unix_like(&self) -> bool {
        matches!(
            self,
            OperatingSystem::Linux
                | OperatingSystem::BSD
                | OperatingSystem::MacOS
                | OperatingSystem::iOS
                | OperatingSystem::Android
        )
    }

    /// Check if this OS supports full shell access
    pub fn has_full_shell(&self) -> bool {
        matches!(
            self,
            OperatingSystem::Linux
                | OperatingSystem::BSD
                | OperatingSystem::MacOS
                | OperatingSystem::Windows
        )
    }
}

/// Tool categories for organization and filtering
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
    // New categories for expanded functionality
    WebTools,       // HTTP clients, API testing, web scraping
    CyberSecurity,  // Penetration testing, vulnerability scanning
    Reconnaissance, // Information gathering, OSINT
    Forensics,      // Memory analysis, disk forensics
    Cryptography,   // Encryption, hashing, certificates
}

/// Cross-platform command mapping for seamless OS translation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossPlatformCommand {
    /// Canonical name used in the database
    pub canonical: String,
    /// Command on Linux
    pub linux: Option<String>,
    /// Command on BSD
    pub bsd: Option<String>,
    /// Command on macOS
    pub macos: Option<String>,
    /// Command on Windows
    pub windows: Option<String>,
    /// Command on iOS (via ssh/jailbreak)
    pub ios: Option<String>,
    /// Command on Android (via Termux/ADB)
    pub android: Option<String>,
    /// Argument translations (e.g., "-la" -> "/A" for ls->dir)
    pub arg_mappings: HashMap<String, HashMap<String, String>>,
}

impl CrossPlatformCommand {
    /// Get the command for a specific OS
    pub fn for_os(&self, os: &OperatingSystem) -> Option<&String> {
        match os {
            OperatingSystem::Linux => self.linux.as_ref(),
            OperatingSystem::BSD => self.bsd.as_ref().or(self.linux.as_ref()),
            OperatingSystem::MacOS => self.macos.as_ref().or(self.linux.as_ref()),
            OperatingSystem::Windows => self.windows.as_ref(),
            OperatingSystem::iOS => self.ios.as_ref().or(self.macos.as_ref()),
            OperatingSystem::Android => self.android.as_ref().or(self.linux.as_ref()),
        }
    }

    /// Translate arguments from canonical form to OS-specific form
    pub fn translate_args(&self, args: &[String], os: &OperatingSystem) -> Vec<String> {
        let os_key = match os {
            OperatingSystem::Linux => "linux",
            OperatingSystem::BSD => "bsd",
            OperatingSystem::MacOS => "macos",
            OperatingSystem::Windows => "windows",
            OperatingSystem::iOS => "ios",
            OperatingSystem::Android => "android",
        };

        args.iter()
            .map(|arg| {
                self.arg_mappings
                    .get(arg)
                    .and_then(|m| m.get(os_key))
                    .cloned()
                    .unwrap_or_else(|| arg.clone())
            })
            .collect()
    }
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
    /// Cross-platform command equivalents (optional)
    #[serde(default)]
    pub cross_platform: Option<CrossPlatformCommand>,
    /// Parameter definitions for function calling schemas
    #[serde(default)]
    pub parameters: Vec<ToolParameter>,
}

/// Parameter definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub description: String,
    pub param_type: ParameterType,
    pub required: bool,
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub enum_values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterType {
    String,
    Integer,
    Boolean,
    Array,
    Path,   // File/directory path
    Url,    // URL/URI
    IpAddr, // IP address
    Port,   // Port number
}

impl OSTool {
    /// Generate OpenAI-compatible function calling schema
    pub fn to_openai_function_schema(&self) -> JsonValue {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &self.parameters {
            let param_schema = match param.param_type {
                ParameterType::String | ParameterType::Path | ParameterType::Url => {
                    if param.enum_values.is_empty() {
                        json!({"type": "string", "description": param.description})
                    } else {
                        json!({
                            "type": "string",
                            "description": param.description,
                            "enum": param.enum_values
                        })
                    }
                }
                ParameterType::Integer | ParameterType::Port => {
                    json!({"type": "integer", "description": param.description})
                }
                ParameterType::Boolean => {
                    json!({"type": "boolean", "description": param.description})
                }
                ParameterType::Array => {
                    json!({
                        "type": "array",
                        "items": {"type": "string"},
                        "description": param.description
                    })
                }
                ParameterType::IpAddr => {
                    json!({
                        "type": "string",
                        "description": param.description,
                        "pattern": "^(?:[0-9]{1,3}\\.){3}[0-9]{1,3}$|^(?:[a-fA-F0-9]{1,4}:){7}[a-fA-F0-9]{1,4}$"
                    })
                }
            };

            properties.insert(param.name.clone(), param_schema);

            if param.required {
                required.push(param.name.clone());
            }
        }

        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": {
                    "type": "object",
                    "properties": properties,
                    "required": required
                }
            }
        })
    }

    /// Get the appropriate command for the current OS
    pub fn command_for_current_os(&self) -> String {
        let current_os = OperatingSystem::current();
        self.command_for_os(&current_os)
    }

    /// Get the appropriate command for a specific OS
    pub fn command_for_os(&self, os: &OperatingSystem) -> String {
        if let Some(ref xplat) = self.cross_platform {
            xplat
                .for_os(os)
                .cloned()
                .unwrap_or_else(|| self.command.clone())
        } else {
            self.command.clone()
        }
    }
}

impl Default for OSTool {
    fn default() -> Self {
        OSTool {
            name: String::new(),
            description: String::new(),
            category: ToolCategory::Utilities,
            command: String::new(),
            common_args: vec![],
            examples: vec![],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![],
            cross_platform: None,
            parameters: vec![],
        }
    }
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
    /// Cross-platform command mappings for translation
    pub cross_platform_map: HashMap<String, CrossPlatformCommand>,
}

impl OSToolsDatabase {
    pub fn new() -> Self {
        let mut db = OSToolsDatabase {
            tools: HashMap::new(),
            categories: HashMap::new(),
            os_specific: HashMap::new(),
            cross_platform_map: HashMap::new(),
        };

        db.populate_cross_platform_mappings();
        db.populate_tools();
        db.populate_network_tools();
        db.populate_web_tools();
        db.populate_cyber_tools();
        db.build_indices();
        db
    }

    /// Initialize cross-platform command mappings
    fn populate_cross_platform_mappings(&mut self) {
        // List directory contents
        self.cross_platform_map.insert(
            "list_dir".to_string(),
            CrossPlatformCommand {
                canonical: "list_dir".to_string(),
                linux: Some("ls".to_string()),
                bsd: Some("ls".to_string()),
                macos: Some("ls".to_string()),
                windows: Some("dir".to_string()),
                ios: Some("ls".to_string()),
                android: Some("ls".to_string()),
                arg_mappings: {
                    let mut m = HashMap::new();
                    let mut la = HashMap::new();
                    la.insert("windows".to_string(), "/A".to_string());
                    m.insert("-la".to_string(), la);
                    let mut r = HashMap::new();
                    r.insert("windows".to_string(), "/S".to_string());
                    m.insert("-R".to_string(), r);
                    m
                },
            },
        );

        // Copy files
        self.cross_platform_map.insert(
            "copy_file".to_string(),
            CrossPlatformCommand {
                canonical: "copy_file".to_string(),
                linux: Some("cp".to_string()),
                bsd: Some("cp".to_string()),
                macos: Some("cp".to_string()),
                windows: Some("copy".to_string()),
                ios: Some("cp".to_string()),
                android: Some("cp".to_string()),
                arg_mappings: {
                    let mut m = HashMap::new();
                    let mut r = HashMap::new();
                    r.insert("windows".to_string(), "/E".to_string());
                    m.insert("-r".to_string(), r);
                    m
                },
            },
        );

        // Move/rename files
        self.cross_platform_map.insert(
            "move_file".to_string(),
            CrossPlatformCommand {
                canonical: "move_file".to_string(),
                linux: Some("mv".to_string()),
                bsd: Some("mv".to_string()),
                macos: Some("mv".to_string()),
                windows: Some("move".to_string()),
                ios: Some("mv".to_string()),
                android: Some("mv".to_string()),
                arg_mappings: HashMap::new(),
            },
        );

        // Remove files
        self.cross_platform_map.insert(
            "remove_file".to_string(),
            CrossPlatformCommand {
                canonical: "remove_file".to_string(),
                linux: Some("rm".to_string()),
                bsd: Some("rm".to_string()),
                macos: Some("rm".to_string()),
                windows: Some("del".to_string()),
                ios: Some("rm".to_string()),
                android: Some("rm".to_string()),
                arg_mappings: {
                    let mut m = HashMap::new();
                    let mut rf = HashMap::new();
                    rf.insert("windows".to_string(), "/F /Q".to_string());
                    m.insert("-rf".to_string(), rf);
                    m
                },
            },
        );

        // Text search
        self.cross_platform_map.insert(
            "text_search".to_string(),
            CrossPlatformCommand {
                canonical: "text_search".to_string(),
                linux: Some("grep".to_string()),
                bsd: Some("grep".to_string()),
                macos: Some("grep".to_string()),
                windows: Some("findstr".to_string()),
                ios: Some("grep".to_string()),
                android: Some("grep".to_string()),
                arg_mappings: {
                    let mut m = HashMap::new();
                    let mut i = HashMap::new();
                    i.insert("windows".to_string(), "/I".to_string());
                    m.insert("-i".to_string(), i);
                    let mut r = HashMap::new();
                    r.insert("windows".to_string(), "/S".to_string());
                    m.insert("-r".to_string(), r);
                    m
                },
            },
        );

        // Process list
        self.cross_platform_map.insert(
            "process_list".to_string(),
            CrossPlatformCommand {
                canonical: "process_list".to_string(),
                linux: Some("ps".to_string()),
                bsd: Some("ps".to_string()),
                macos: Some("ps".to_string()),
                windows: Some("tasklist".to_string()),
                ios: Some("ps".to_string()),
                android: Some("ps".to_string()),
                arg_mappings: {
                    let mut m = HashMap::new();
                    let mut aux = HashMap::new();
                    aux.insert("windows".to_string(), "/V".to_string());
                    m.insert("aux".to_string(), aux);
                    m
                },
            },
        );

        // Kill process
        self.cross_platform_map.insert(
            "kill_process".to_string(),
            CrossPlatformCommand {
                canonical: "kill_process".to_string(),
                linux: Some("kill".to_string()),
                bsd: Some("kill".to_string()),
                macos: Some("kill".to_string()),
                windows: Some("taskkill".to_string()),
                ios: Some("kill".to_string()),
                android: Some("kill".to_string()),
                arg_mappings: {
                    let mut m = HashMap::new();
                    let mut nine = HashMap::new();
                    nine.insert("windows".to_string(), "/F".to_string());
                    m.insert("-9".to_string(), nine);
                    m
                },
            },
        );

        // Network config
        self.cross_platform_map.insert(
            "network_config".to_string(),
            CrossPlatformCommand {
                canonical: "network_config".to_string(),
                linux: Some("ip".to_string()),
                bsd: Some("ifconfig".to_string()),
                macos: Some("ifconfig".to_string()),
                windows: Some("ipconfig".to_string()),
                ios: Some("ifconfig".to_string()),
                android: Some("ip".to_string()),
                arg_mappings: {
                    let mut m = HashMap::new();
                    let mut addr = HashMap::new();
                    addr.insert("bsd".to_string(), "-a".to_string());
                    addr.insert("macos".to_string(), "-a".to_string());
                    addr.insert("windows".to_string(), "/all".to_string());
                    m.insert("addr".to_string(), addr);
                    m
                },
            },
        );

        // Network connections
        self.cross_platform_map.insert(
            "network_connections".to_string(),
            CrossPlatformCommand {
                canonical: "network_connections".to_string(),
                linux: Some("ss".to_string()),
                bsd: Some("netstat".to_string()),
                macos: Some("netstat".to_string()),
                windows: Some("netstat".to_string()),
                ios: Some("netstat".to_string()),
                android: Some("ss".to_string()),
                arg_mappings: {
                    let mut m = HashMap::new();
                    let mut tulpn = HashMap::new();
                    tulpn.insert("bsd".to_string(), "-an".to_string());
                    tulpn.insert("macos".to_string(), "-an".to_string());
                    tulpn.insert("windows".to_string(), "-an".to_string());
                    m.insert("-tulpn".to_string(), tulpn);
                    m
                },
            },
        );
    }

    fn populate_tools(&mut self) {
        // ==================== File System Tools ====================
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
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::iOS,
                OperatingSystem::Android,
            ],
            cross_platform: self.cross_platform_map.get("list_dir").cloned(),
            parameters: vec![
                ToolParameter {
                    name: "path".to_string(),
                    description: "Directory path to list".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: Some(".".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "all".to_string(),
                    description: "Show hidden files".to_string(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default_value: Some("false".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "long".to_string(),
                    description: "Use long listing format".to_string(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default_value: Some("false".to_string()),
                    enum_values: vec![],
                },
            ],
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
            cross_platform: self.cross_platform_map.get("list_dir").cloned(),
            parameters: vec![ToolParameter {
                name: "path".to_string(),
                description: "Directory path to list".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: Some(".".to_string()),
                enum_values: vec![],
            }],
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
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::iOS,
                OperatingSystem::Android,
            ],
            cross_platform: self.cross_platform_map.get("copy_file").cloned(),
            parameters: vec![
                ToolParameter {
                    name: "source".to_string(),
                    description: "Source file or directory".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "destination".to_string(),
                    description: "Destination path".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "recursive".to_string(),
                    description: "Copy directories recursively".to_string(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default_value: Some("false".to_string()),
                    enum_values: vec![],
                },
            ],
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
            cross_platform: self.cross_platform_map.get("copy_file").cloned(),
            parameters: vec![],
        });

        // ==================== Text Processing Tools ====================
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
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::iOS,
                OperatingSystem::Android,
            ],
            cross_platform: self.cross_platform_map.get("text_search").cloned(),
            parameters: vec![
                ToolParameter {
                    name: "pattern".to_string(),
                    description: "Search pattern (regex)".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "file".to_string(),
                    description: "File to search".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "ignore_case".to_string(),
                    description: "Case-insensitive search".to_string(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default_value: Some("false".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "recursive".to_string(),
                    description: "Search recursively".to_string(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default_value: Some("false".to_string()),
                    enum_values: vec![],
                },
            ],
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
            cross_platform: self.cross_platform_map.get("text_search").cloned(),
            parameters: vec![],
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
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "expression".to_string(),
                    description: "sed expression (e.g., 's/old/new/g')".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "file".to_string(),
                    description: "File to process".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "in_place".to_string(),
                    description: "Edit file in place".to_string(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default_value: Some("false".to_string()),
                    enum_values: vec![],
                },
            ],
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
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "program".to_string(),
                    description: "AWK program".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "file".to_string(),
                    description: "Input file".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "field_separator".to_string(),
                    description: "Field separator character".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "jq".to_string(),
            description: "Command-line JSON processor".to_string(),
            category: ToolCategory::TextProcessing,
            command: "jq".to_string(),
            common_args: vec!["-r".to_string(), "-c".to_string(), ".".to_string()],
            examples: vec![ToolExample {
                description: "Extract field from JSON".to_string(),
                command: "jq '.name' data.json".to_string(),
                expected_output: Some("\"John\"".to_string()),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "filter".to_string(),
                    description: "jq filter expression".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: Some(".".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "file".to_string(),
                    description: "JSON file to process".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "raw".to_string(),
                    description: "Output raw strings".to_string(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default_value: Some("false".to_string()),
                    enum_values: vec![],
                },
            ],
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
        });
    }

    fn add_tool(&mut self, tool: OSTool) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Populate network-focused tools
    fn populate_network_tools(&mut self) {
        // ==================== Network Tools ====================

        self.add_tool(OSTool {
            name: "curl".to_string(),
            description: "Transfer data using various protocols (HTTP, FTP, etc.)".to_string(),
            category: ToolCategory::NetworkTools,
            command: "curl".to_string(),
            common_args: vec!["-X".to_string(), "-H".to_string(), "-d".to_string(), "-o".to_string(), "-s".to_string()],
            examples: vec![
                ToolExample {
                    description: "GET request".to_string(),
                    command: "curl https://api.example.com/data".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "POST JSON data".to_string(),
                    command: "curl -X POST -H 'Content-Type: application/json' -d '{\"key\":\"value\"}' https://api.example.com".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "url".to_string(),
                    description: "Target URL".to_string(),
                    param_type: ParameterType::Url,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "method".to_string(),
                    description: "HTTP method".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: Some("GET".to_string()),
                    enum_values: vec!["GET".to_string(), "POST".to_string(), "PUT".to_string(), "DELETE".to_string(), "PATCH".to_string(), "HEAD".to_string()],
                },
                ToolParameter {
                    name: "headers".to_string(),
                    description: "HTTP headers".to_string(),
                    param_type: ParameterType::Array,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "data".to_string(),
                    description: "Request body data".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "output".to_string(),
                    description: "Output file path".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "wget".to_string(),
            description: "Non-interactive network downloader".to_string(),
            category: ToolCategory::NetworkTools,
            command: "wget".to_string(),
            common_args: vec![
                "-O".to_string(),
                "-q".to_string(),
                "-r".to_string(),
                "--no-check-certificate".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Download file".to_string(),
                command: "wget -O output.html https://example.com".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "url".to_string(),
                    description: "URL to download".to_string(),
                    param_type: ParameterType::Url,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "output".to_string(),
                    description: "Output filename".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "ssh".to_string(),
            description: "OpenSSH secure shell client".to_string(),
            category: ToolCategory::NetworkTools,
            command: "ssh".to_string(),
            common_args: vec![
                "-p".to_string(),
                "-i".to_string(),
                "-L".to_string(),
                "-R".to_string(),
            ],
            examples: vec![
                ToolExample {
                    description: "Connect to remote host".to_string(),
                    command: "ssh user@hostname".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Port forwarding".to_string(),
                    command: "ssh -L 8080:localhost:80 user@hostname".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::iOS,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "host".to_string(),
                    description: "Remote host (user@hostname)".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "port".to_string(),
                    description: "SSH port".to_string(),
                    param_type: ParameterType::Port,
                    required: false,
                    default_value: Some("22".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "identity_file".to_string(),
                    description: "Private key file".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "scp".to_string(),
            description: "Secure copy over SSH".to_string(),
            category: ToolCategory::NetworkTools,
            command: "scp".to_string(),
            common_args: vec!["-r".to_string(), "-P".to_string(), "-i".to_string()],
            examples: vec![ToolExample {
                description: "Copy file to remote".to_string(),
                command: "scp file.txt user@host:/path/".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "source".to_string(),
                    description: "Source file/directory".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "destination".to_string(),
                    description: "Destination (user@host:/path)".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "rsync".to_string(),
            description: "Fast, versatile file copying tool".to_string(),
            category: ToolCategory::NetworkTools,
            command: "rsync".to_string(),
            common_args: vec![
                "-avz".to_string(),
                "--progress".to_string(),
                "--delete".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Sync directories".to_string(),
                command: "rsync -avz /source/ user@host:/dest/".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "source".to_string(),
                    description: "Source path".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "destination".to_string(),
                    description: "Destination path".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "nc".to_string(),
            description: "Netcat - TCP/UDP connections and listeners".to_string(),
            category: ToolCategory::NetworkTools,
            command: "nc".to_string(),
            common_args: vec![
                "-l".to_string(),
                "-p".to_string(),
                "-v".to_string(),
                "-z".to_string(),
            ],
            examples: vec![
                ToolExample {
                    description: "Listen on port".to_string(),
                    command: "nc -l -p 8080".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Port scan".to_string(),
                    command: "nc -zv host 20-30".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "host".to_string(),
                    description: "Target host".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "port".to_string(),
                    description: "Port number or range".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "listen".to_string(),
                    description: "Listen mode".to_string(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default_value: Some("false".to_string()),
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "nmap".to_string(),
            description: "Network exploration and security auditing tool".to_string(),
            category: ToolCategory::Reconnaissance,
            command: "nmap".to_string(),
            common_args: vec![
                "-sS".to_string(),
                "-sV".to_string(),
                "-O".to_string(),
                "-A".to_string(),
                "-p".to_string(),
            ],
            examples: vec![
                ToolExample {
                    description: "Quick scan".to_string(),
                    command: "nmap -sV 192.168.1.1".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Full port scan".to_string(),
                    command: "nmap -p- -sV -O target.com".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: true,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "target".to_string(),
                    description: "Target host/network".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "ports".to_string(),
                    description: "Port specification".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "scan_type".to_string(),
                    description: "Scan technique".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: Some("-sS".to_string()),
                    enum_values: vec![
                        "-sS".to_string(),
                        "-sT".to_string(),
                        "-sU".to_string(),
                        "-sV".to_string(),
                        "-sA".to_string(),
                    ],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "dig".to_string(),
            description: "DNS lookup utility".to_string(),
            category: ToolCategory::NetworkTools,
            command: "dig".to_string(),
            common_args: vec![
                "+short".to_string(),
                "ANY".to_string(),
                "MX".to_string(),
                "NS".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Query A record".to_string(),
                command: "dig example.com".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "domain".to_string(),
                    description: "Domain to query".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "record_type".to_string(),
                    description: "DNS record type".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: Some("A".to_string()),
                    enum_values: vec![
                        "A".to_string(),
                        "AAAA".to_string(),
                        "MX".to_string(),
                        "NS".to_string(),
                        "TXT".to_string(),
                        "CNAME".to_string(),
                        "SOA".to_string(),
                        "ANY".to_string(),
                    ],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "nslookup".to_string(),
            description: "Query DNS servers".to_string(),
            category: ToolCategory::NetworkTools,
            command: "nslookup".to_string(),
            common_args: vec!["-type=".to_string()],
            examples: vec![ToolExample {
                description: "Lookup domain".to_string(),
                command: "nslookup example.com".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "domain".to_string(),
                description: "Domain to query".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "traceroute".to_string(),
            description: "Trace packet route to host".to_string(),
            category: ToolCategory::NetworkTools,
            command: "traceroute".to_string(),
            common_args: vec!["-n".to_string(), "-m".to_string()],
            examples: vec![ToolExample {
                description: "Trace route".to_string(),
                command: "traceroute google.com".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Android,
            ],
            cross_platform: Some(CrossPlatformCommand {
                canonical: "traceroute".to_string(),
                linux: Some("traceroute".to_string()),
                bsd: Some("traceroute".to_string()),
                macos: Some("traceroute".to_string()),
                windows: Some("tracert".to_string()),
                ios: Some("traceroute".to_string()),
                android: Some("traceroute".to_string()),
                arg_mappings: HashMap::new(),
            }),
            parameters: vec![ToolParameter {
                name: "host".to_string(),
                description: "Target host".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "tcpdump".to_string(),
            description: "Packet analyzer".to_string(),
            category: ToolCategory::NetworkTools,
            command: "tcpdump".to_string(),
            common_args: vec![
                "-i".to_string(),
                "-w".to_string(),
                "-r".to_string(),
                "-n".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Capture on interface".to_string(),
                command: "tcpdump -i eth0 -w capture.pcap".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: true,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "interface".to_string(),
                    description: "Network interface".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "filter".to_string(),
                    description: "BPF filter expression".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "output".to_string(),
                    description: "Output pcap file".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "ss".to_string(),
            description: "Socket statistics".to_string(),
            category: ToolCategory::NetworkTools,
            command: "ss".to_string(),
            common_args: vec!["-tulpn".to_string(), "-a".to_string(), "-s".to_string()],
            examples: vec![ToolExample {
                description: "Show listening ports".to_string(),
                command: "ss -tulpn".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::Android],
            cross_platform: self.cross_platform_map.get("network_connections").cloned(),
            parameters: vec![
                ToolParameter {
                    name: "tcp".to_string(),
                    description: "Show TCP sockets".to_string(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default_value: Some("false".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "udp".to_string(),
                    description: "Show UDP sockets".to_string(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default_value: Some("false".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "listening".to_string(),
                    description: "Show listening sockets".to_string(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default_value: Some("false".to_string()),
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "ip".to_string(),
            description: "Show/manipulate routing, network devices, interfaces".to_string(),
            category: ToolCategory::NetworkTools,
            command: "ip".to_string(),
            common_args: vec!["addr".to_string(), "link".to_string(), "route".to_string()],
            examples: vec![
                ToolExample {
                    description: "Show addresses".to_string(),
                    command: "ip addr".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Show routes".to_string(),
                    command: "ip route".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::Android],
            cross_platform: self.cross_platform_map.get("network_config").cloned(),
            parameters: vec![ToolParameter {
                name: "object".to_string(),
                description: "Object to manage".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: Some("addr".to_string()),
                enum_values: vec![
                    "addr".to_string(),
                    "link".to_string(),
                    "route".to_string(),
                    "neigh".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "ifconfig".to_string(),
            description: "Configure network interfaces".to_string(),
            category: ToolCategory::NetworkTools,
            command: "ifconfig".to_string(),
            common_args: vec!["-a".to_string()],
            examples: vec![ToolExample {
                description: "Show all interfaces".to_string(),
                command: "ifconfig -a".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::iOS,
            ],
            cross_platform: self.cross_platform_map.get("network_config").cloned(),
            parameters: vec![ToolParameter {
                name: "interface".to_string(),
                description: "Network interface".to_string(),
                param_type: ParameterType::String,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "ipconfig".to_string(),
            description: "Display network configuration (Windows)".to_string(),
            category: ToolCategory::NetworkTools,
            command: "ipconfig".to_string(),
            common_args: vec![
                "/all".to_string(),
                "/release".to_string(),
                "/renew".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Show all configuration".to_string(),
                command: "ipconfig /all".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Windows],
            cross_platform: self.cross_platform_map.get("network_config").cloned(),
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "iptables".to_string(),
            description: "IPv4 packet filtering and NAT".to_string(),
            category: ToolCategory::NetworkTools,
            command: "iptables".to_string(),
            common_args: vec![
                "-L".to_string(),
                "-A".to_string(),
                "-D".to_string(),
                "-F".to_string(),
            ],
            examples: vec![
                ToolExample {
                    description: "List rules".to_string(),
                    command: "iptables -L -n -v".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Block IP".to_string(),
                    command: "iptables -A INPUT -s 10.0.0.1 -j DROP".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Critical,
            requires_admin: true,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::Android],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "chain".to_string(),
                    description: "Chain name".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![
                        "INPUT".to_string(),
                        "OUTPUT".to_string(),
                        "FORWARD".to_string(),
                    ],
                },
                ToolParameter {
                    name: "action".to_string(),
                    description: "Action to perform".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: Some("-L".to_string()),
                    enum_values: vec![
                        "-L".to_string(),
                        "-A".to_string(),
                        "-D".to_string(),
                        "-I".to_string(),
                        "-F".to_string(),
                    ],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "nft".to_string(),
            description: "nftables packet filtering (replaces iptables)".to_string(),
            category: ToolCategory::NetworkTools,
            command: "nft".to_string(),
            common_args: vec!["list".to_string(), "add".to_string(), "delete".to_string()],
            examples: vec![ToolExample {
                description: "List ruleset".to_string(),
                command: "nft list ruleset".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Critical,
            requires_admin: true,
            supported_os: vec![OperatingSystem::Linux],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "nft command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "list".to_string(),
                    "add".to_string(),
                    "delete".to_string(),
                    "flush".to_string(),
                ],
            }],
        });
    }

    /// Populate web-focused tools
    fn populate_web_tools(&mut self) {
        self.add_tool(OSTool {
            name: "httpie".to_string(),
            description: "User-friendly HTTP client".to_string(),
            category: ToolCategory::WebTools,
            command: "http".to_string(),
            common_args: vec!["GET".to_string(), "POST".to_string(), "--json".to_string()],
            examples: vec![ToolExample {
                description: "GET request".to_string(),
                command: "http GET https://api.example.com".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "method".to_string(),
                    description: "HTTP method".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: Some("GET".to_string()),
                    enum_values: vec![
                        "GET".to_string(),
                        "POST".to_string(),
                        "PUT".to_string(),
                        "DELETE".to_string(),
                    ],
                },
                ToolParameter {
                    name: "url".to_string(),
                    description: "Target URL".to_string(),
                    param_type: ParameterType::Url,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "openssl".to_string(),
            description: "OpenSSL cryptography toolkit".to_string(),
            category: ToolCategory::Cryptography,
            command: "openssl".to_string(),
            common_args: vec![
                "s_client".to_string(),
                "x509".to_string(),
                "req".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Check SSL certificate".to_string(),
                command: "openssl s_client -connect example.com:443".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "OpenSSL subcommand".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "s_client".to_string(),
                    "x509".to_string(),
                    "req".to_string(),
                    "genrsa".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "nikto".to_string(),
            description: "Web server vulnerability scanner".to_string(),
            category: ToolCategory::CyberSecurity,
            command: "nikto".to_string(),
            common_args: vec!["-h".to_string(), "-p".to_string(), "-ssl".to_string()],
            examples: vec![ToolExample {
                description: "Scan web server".to_string(),
                command: "nikto -h https://target.com".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "host".to_string(),
                description: "Target host".to_string(),
                param_type: ParameterType::Url,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "gobuster".to_string(),
            description: "Directory/DNS busting tool".to_string(),
            category: ToolCategory::Reconnaissance,
            command: "gobuster".to_string(),
            common_args: vec![
                "dir".to_string(),
                "dns".to_string(),
                "-u".to_string(),
                "-w".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Directory brute force".to_string(),
                command: "gobuster dir -u https://target.com -w wordlist.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "mode".to_string(),
                    description: "Scanning mode".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: Some("dir".to_string()),
                    enum_values: vec!["dir".to_string(), "dns".to_string(), "vhost".to_string()],
                },
                ToolParameter {
                    name: "url".to_string(),
                    description: "Target URL".to_string(),
                    param_type: ParameterType::Url,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "wordlist".to_string(),
                    description: "Wordlist file".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "sqlmap".to_string(),
            description: "Automatic SQL injection tool".to_string(),
            category: ToolCategory::CyberSecurity,
            command: "sqlmap".to_string(),
            common_args: vec![
                "-u".to_string(),
                "--dbs".to_string(),
                "--tables".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Test for SQL injection".to_string(),
                command: "sqlmap -u \"https://target.com/page?id=1\" --dbs".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Critical,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "url".to_string(),
                description: "Target URL with parameter".to_string(),
                param_type: ParameterType::Url,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });
    }

    /// Populate cyber security and forensics tools
    fn populate_cyber_tools(&mut self) {
        self.add_tool(OSTool {
            name: "hashcat".to_string(),
            description: "Advanced password recovery".to_string(),
            category: ToolCategory::CyberSecurity,
            command: "hashcat".to_string(),
            common_args: vec!["-m".to_string(), "-a".to_string(), "-o".to_string()],
            examples: vec![ToolExample {
                description: "Dictionary attack on MD5".to_string(),
                command: "hashcat -m 0 -a 0 hash.txt wordlist.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "hash_type".to_string(),
                    description: "Hash type (-m)".to_string(),
                    param_type: ParameterType::Integer,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "hash_file".to_string(),
                    description: "File containing hashes".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "john".to_string(),
            description: "John the Ripper password cracker".to_string(),
            category: ToolCategory::CyberSecurity,
            command: "john".to_string(),
            common_args: vec![
                "--wordlist=".to_string(),
                "--format=".to_string(),
                "--show".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Crack with wordlist".to_string(),
                command: "john --wordlist=rockyou.txt hashes.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "wordlist".to_string(),
                    description: "Wordlist file".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "hash_file".to_string(),
                    description: "File with password hashes".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "hydra".to_string(),
            description: "Network logon cracker".to_string(),
            category: ToolCategory::CyberSecurity,
            command: "hydra".to_string(),
            common_args: vec![
                "-l".to_string(),
                "-L".to_string(),
                "-p".to_string(),
                "-P".to_string(),
            ],
            examples: vec![ToolExample {
                description: "SSH brute force".to_string(),
                command: "hydra -l admin -P passwords.txt ssh://target.com".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Critical,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "login".to_string(),
                    description: "Login name".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "target".to_string(),
                    description: "Target (protocol://host)".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "volatility".to_string(),
            description: "Memory forensics framework".to_string(),
            category: ToolCategory::Forensics,
            command: "volatility".to_string(),
            common_args: vec![
                "-f".to_string(),
                "--profile=".to_string(),
                "pslist".to_string(),
            ],
            examples: vec![ToolExample {
                description: "List processes from memory dump".to_string(),
                command: "volatility -f memory.dmp --profile=Win10x64 pslist".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "file".to_string(),
                    description: "Memory dump file".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "profile".to_string(),
                    description: "OS profile".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "binwalk".to_string(),
            description: "Firmware analysis tool".to_string(),
            category: ToolCategory::Forensics,
            command: "binwalk".to_string(),
            common_args: vec!["-e".to_string(), "-M".to_string(), "-B".to_string()],
            examples: vec![ToolExample {
                description: "Extract embedded files".to_string(),
                command: "binwalk -e firmware.bin".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "File to analyze".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "strings".to_string(),
            description: "Extract printable strings from files".to_string(),
            category: ToolCategory::Forensics,
            command: "strings".to_string(),
            common_args: vec!["-n".to_string(), "-e".to_string()],
            examples: vec![ToolExample {
                description: "Find strings in binary".to_string(),
                command: "strings -n 10 binary.exe".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "file".to_string(),
                    description: "File to analyze".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "min_length".to_string(),
                    description: "Minimum string length".to_string(),
                    param_type: ParameterType::Integer,
                    required: false,
                    default_value: Some("4".to_string()),
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "strace".to_string(),
            description: "Trace system calls and signals".to_string(),
            category: ToolCategory::Forensics,
            command: "strace".to_string(),
            common_args: vec!["-p".to_string(), "-f".to_string(), "-e".to_string()],
            examples: vec![ToolExample {
                description: "Trace process".to_string(),
                command: "strace -p 1234".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: true,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::Android],
            cross_platform: Some(CrossPlatformCommand {
                canonical: "strace".to_string(),
                linux: Some("strace".to_string()),
                bsd: Some("truss".to_string()),
                macos: Some("dtruss".to_string()),
                windows: None,
                ios: Some("dtruss".to_string()),
                android: Some("strace".to_string()),
                arg_mappings: HashMap::new(),
            }),
            parameters: vec![ToolParameter {
                name: "pid".to_string(),
                description: "Process ID to trace".to_string(),
                param_type: ParameterType::Integer,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "gdb".to_string(),
            description: "GNU debugger".to_string(),
            category: ToolCategory::Development,
            command: "gdb".to_string(),
            common_args: vec!["-q".to_string(), "-x".to_string(), "-p".to_string()],
            examples: vec![ToolExample {
                description: "Debug program".to_string(),
                command: "gdb ./program".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::Android,
            ],
            cross_platform: Some(CrossPlatformCommand {
                canonical: "gdb".to_string(),
                linux: Some("gdb".to_string()),
                bsd: Some("gdb".to_string()),
                macos: Some("lldb".to_string()),
                windows: Some("gdb".to_string()),
                ios: Some("lldb".to_string()),
                android: Some("gdb".to_string()),
                arg_mappings: HashMap::new(),
            }),
            parameters: vec![ToolParameter {
                name: "program".to_string(),
                description: "Program to debug".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "gpg".to_string(),
            description: "GNU Privacy Guard encryption".to_string(),
            category: ToolCategory::Cryptography,
            command: "gpg".to_string(),
            common_args: vec![
                "--encrypt".to_string(),
                "--decrypt".to_string(),
                "--sign".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Encrypt file".to_string(),
                command: "gpg --encrypt --recipient user@example.com file.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::Android,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "operation".to_string(),
                    description: "GPG operation".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "--encrypt".to_string(),
                        "--decrypt".to_string(),
                        "--sign".to_string(),
                    ],
                },
                ToolParameter {
                    name: "file".to_string(),
                    description: "Input file".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "sha256sum".to_string(),
            description: "Compute SHA-256 hash".to_string(),
            category: ToolCategory::Cryptography,
            command: "sha256sum".to_string(),
            common_args: vec!["-c".to_string()],
            examples: vec![ToolExample {
                description: "Hash file".to_string(),
                command: "sha256sum file.txt".to_string(),
                expected_output: Some("a1b2c3... file.txt".to_string()),
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::BSD,
                OperatingSystem::MacOS,
                OperatingSystem::Android,
            ],
            cross_platform: Some(CrossPlatformCommand {
                canonical: "sha256sum".to_string(),
                linux: Some("sha256sum".to_string()),
                bsd: Some("sha256".to_string()),
                macos: Some("shasum -a 256".to_string()),
                windows: Some("certutil -hashfile".to_string()),
                ios: Some("shasum -a 256".to_string()),
                android: Some("sha256sum".to_string()),
                arg_mappings: HashMap::new(),
            }),
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "File to hash".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });
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

    /// Get all tools as OpenAI function calling schemas
    pub fn to_openai_function_schemas(&self) -> Vec<JsonValue> {
        self.tools
            .values()
            .map(|t| t.to_openai_function_schema())
            .collect()
    }

    /// Get schemas for tools in a specific category
    pub fn get_category_schemas(&self, category: &ToolCategory) -> Vec<JsonValue> {
        self.get_tools_by_category(category)
            .iter()
            .map(|t| t.to_openai_function_schema())
            .collect()
    }

    /// Get the cross-platform equivalent command for the current OS
    pub fn get_cross_platform_command(&self, canonical: &str) -> Option<String> {
        let current_os = OperatingSystem::current();
        self.cross_platform_map
            .get(canonical)
            .and_then(|cmd| cmd.for_os(&current_os).cloned())
    }

    /// Translate a command from one OS to another
    pub fn translate_command(
        &self,
        canonical: &str,
        args: &[String],
        target_os: &OperatingSystem,
    ) -> Option<(String, Vec<String>)> {
        self.cross_platform_map.get(canonical).and_then(|cmd| {
            cmd.for_os(target_os).map(|command| {
                let translated_args = cmd.translate_args(args, target_os);
                (command.clone(), translated_args)
            })
        })
    }
}

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub tool_name: String,
    pub command_executed: String,
    pub execution_time_ms: u64,
}

impl ToolExecutionResult {
    /// Convert to JSON for function calling response
    pub fn to_json(&self) -> JsonValue {
        json!({
            "success": self.success,
            "stdout": self.stdout,
            "stderr": self.stderr,
            "exit_code": self.exit_code,
            "tool_name": self.tool_name,
            "command_executed": self.command_executed,
            "execution_time_ms": self.execution_time_ms
        })
    }
}

/// Execute a tool with the given arguments
pub fn execute_tool(
    db: &OSToolsDatabase,
    tool_name: &str,
    args: &[String],
) -> Result<ToolExecutionResult, String> {
    let tool = db
        .get_tool(tool_name)
        .ok_or_else(|| format!("Tool '{}' not found", tool_name))?;

    // Check OS compatibility
    let current_os = OperatingSystem::current();
    if !tool.supported_os.contains(&current_os) {
        return Err(format!(
            "Tool '{}' is not supported on {:?}",
            tool_name, current_os
        ));
    }

    // Get the appropriate command for the current OS
    let command = tool.command_for_current_os();

    // Build the full command with arguments
    let full_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let start_time = std::time::Instant::now();

    // Execute the command
    let output = std::process::Command::new(&command)
        .args(&full_args)
        .output()
        .map_err(|e| format!("Failed to execute '{}': {}", command, e))?;

    let execution_time = start_time.elapsed().as_millis() as u64;

    let command_executed = if args.is_empty() {
        command.clone()
    } else {
        format!("{} {}", command, args.join(" "))
    };

    Ok(ToolExecutionResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
        tool_name: tool_name.to_string(),
        command_executed,
        execution_time_ms: execution_time,
    })
}

/// Execute a tool with safety checks
pub fn execute_tool_safe(
    db: &OSToolsDatabase,
    tool_name: &str,
    args: &[String],
    allow_dangerous: bool,
) -> Result<ToolExecutionResult, String> {
    let tool = db
        .get_tool(tool_name)
        .ok_or_else(|| format!("Tool '{}' not found", tool_name))?;

    // Check safety level
    match tool.safety_level {
        SafetyLevel::Safe | SafetyLevel::Caution => {}
        SafetyLevel::Dangerous | SafetyLevel::Critical => {
            if !allow_dangerous {
                return Err(format!(
                    "Tool '{}' has safety level {:?}. Set allow_dangerous=true to execute.",
                    tool_name, tool.safety_level
                ));
            }
        }
    }

    // Check admin requirements
    if tool.requires_admin {
        // On Unix, check if running as root
        #[cfg(unix)]
        {
            if unsafe { libc::geteuid() } != 0 {
                return Err(format!(
                    "Tool '{}' requires administrator privileges",
                    tool_name
                ));
            }
        }
        // On Windows, this is more complex - we just warn
        #[cfg(windows)]
        {
            // Windows admin check would require additional logic
            // For now, we proceed with a warning in stderr
        }
    }

    execute_tool(db, tool_name, args)
}

/// Execute a cross-platform command using the canonical name
pub fn execute_cross_platform(
    db: &OSToolsDatabase,
    canonical_name: &str,
    args: &[String],
) -> Result<ToolExecutionResult, String> {
    let current_os = OperatingSystem::current();

    let (command, translated_args) = db
        .translate_command(canonical_name, args, &current_os)
        .ok_or_else(|| format!("No cross-platform mapping for '{}'", canonical_name))?;

    let full_args: Vec<&str> = translated_args.iter().map(|s| s.as_str()).collect();

    let start_time = std::time::Instant::now();

    let output = std::process::Command::new(&command)
        .args(&full_args)
        .output()
        .map_err(|e| format!("Failed to execute '{}': {}", command, e))?;

    let execution_time = start_time.elapsed().as_millis() as u64;

    let command_executed = if translated_args.is_empty() {
        command.clone()
    } else {
        format!("{} {}", command, translated_args.join(" "))
    };

    Ok(ToolExecutionResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
        tool_name: canonical_name.to_string(),
        command_executed,
        execution_time_ms: execution_time,
    })
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
