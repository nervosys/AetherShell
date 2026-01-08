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
    // Cloud & DevOps categories
    CloudAWS,       // AWS CLI tools
    CloudAzure,     // Azure CLI tools
    CloudGCP,       // Google Cloud tools
    Kubernetes,     // Kubernetes management
    Containers,     // Docker, Podman
    Infrastructure, // Terraform, Ansible, Pulumi
    // Data & Analytics categories
    Database,        // Database clients and tools
    DataProcessing,  // ETL, data transformation
    MachineLearning, // ML tools and frameworks
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
        db.populate_cloud_tools();
        db.populate_container_tools();
        db.populate_data_tools();
        db.populate_sysadmin_tools();
        db.populate_security_tools();
        db.populate_ai_tools();
        db.populate_media_tools();
        db.populate_api_tools();
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

    /// Populate cloud provider tools (AWS, Azure, GCP)
    fn populate_cloud_tools(&mut self) {
        // ==================== AWS Tools ====================
        self.add_tool(OSTool {
            name: "aws".to_string(),
            description: "Amazon Web Services CLI - manage AWS resources".to_string(),
            category: ToolCategory::CloudAWS,
            command: "aws".to_string(),
            common_args: vec![
                "s3".to_string(),
                "ec2".to_string(),
                "lambda".to_string(),
                "iam".to_string(),
            ],
            examples: vec![
                ToolExample {
                    description: "List S3 buckets".to_string(),
                    command: "aws s3 ls".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Describe EC2 instances".to_string(),
                    command: "aws ec2 describe-instances".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "service".to_string(),
                    description: "AWS service (s3, ec2, lambda, etc.)".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "s3".to_string(),
                        "ec2".to_string(),
                        "lambda".to_string(),
                        "iam".to_string(),
                        "rds".to_string(),
                        "ecs".to_string(),
                        "eks".to_string(),
                        "dynamodb".to_string(),
                        "sqs".to_string(),
                    ],
                },
                ToolParameter {
                    name: "command".to_string(),
                    description: "Service command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "region".to_string(),
                    description: "AWS region".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        // ==================== Azure Tools ====================
        self.add_tool(OSTool {
            name: "az".to_string(),
            description: "Azure CLI - manage Azure resources".to_string(),
            category: ToolCategory::CloudAzure,
            command: "az".to_string(),
            common_args: vec!["vm".to_string(), "storage".to_string(), "aks".to_string()],
            examples: vec![
                ToolExample {
                    description: "List resource groups".to_string(),
                    command: "az group list".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "List VMs".to_string(),
                    command: "az vm list".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "group".to_string(),
                    description: "Command group (vm, storage, aks, etc.)".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "vm".to_string(),
                        "storage".to_string(),
                        "aks".to_string(),
                        "acr".to_string(),
                        "network".to_string(),
                        "group".to_string(),
                        "webapp".to_string(),
                        "sql".to_string(),
                        "keyvault".to_string(),
                    ],
                },
                ToolParameter {
                    name: "command".to_string(),
                    description: "Command within group".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        // ==================== Google Cloud Tools ====================
        self.add_tool(OSTool {
            name: "gcloud".to_string(),
            description: "Google Cloud CLI - manage GCP resources".to_string(),
            category: ToolCategory::CloudGCP,
            command: "gcloud".to_string(),
            common_args: vec![
                "compute".to_string(),
                "storage".to_string(),
                "container".to_string(),
            ],
            examples: vec![
                ToolExample {
                    description: "List compute instances".to_string(),
                    command: "gcloud compute instances list".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "List GKE clusters".to_string(),
                    command: "gcloud container clusters list".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "group".to_string(),
                    description: "Command group".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "compute".to_string(),
                        "storage".to_string(),
                        "container".to_string(),
                        "run".to_string(),
                        "functions".to_string(),
                        "sql".to_string(),
                        "pubsub".to_string(),
                        "iam".to_string(),
                    ],
                },
                ToolParameter {
                    name: "command".to_string(),
                    description: "Command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "gsutil".to_string(),
            description: "Google Cloud Storage utility".to_string(),
            category: ToolCategory::CloudGCP,
            command: "gsutil".to_string(),
            common_args: vec![
                "ls".to_string(),
                "cp".to_string(),
                "mb".to_string(),
                "rb".to_string(),
            ],
            examples: vec![
                ToolExample {
                    description: "List buckets".to_string(),
                    command: "gsutil ls".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Copy file to bucket".to_string(),
                    command: "gsutil cp file.txt gs://bucket/".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "command".to_string(),
                    description: "gsutil command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "ls".to_string(),
                        "cp".to_string(),
                        "mv".to_string(),
                        "rm".to_string(),
                        "mb".to_string(),
                    ],
                },
                ToolParameter {
                    name: "source".to_string(),
                    description: "Source path or URI".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "destination".to_string(),
                    description: "Destination path or URI".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        // ==================== Infrastructure as Code ====================
        self.add_tool(OSTool {
            name: "terraform".to_string(),
            description: "Infrastructure as Code tool".to_string(),
            category: ToolCategory::Infrastructure,
            command: "terraform".to_string(),
            common_args: vec![
                "init".to_string(),
                "plan".to_string(),
                "apply".to_string(),
                "destroy".to_string(),
            ],
            examples: vec![
                ToolExample {
                    description: "Initialize Terraform".to_string(),
                    command: "terraform init".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Plan infrastructure changes".to_string(),
                    command: "terraform plan".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "command".to_string(),
                    description: "Terraform command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "init".to_string(),
                        "plan".to_string(),
                        "apply".to_string(),
                        "destroy".to_string(),
                        "validate".to_string(),
                        "fmt".to_string(),
                        "output".to_string(),
                        "state".to_string(),
                    ],
                },
                ToolParameter {
                    name: "auto_approve".to_string(),
                    description: "Skip interactive approval".to_string(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default_value: Some("false".to_string()),
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "ansible".to_string(),
            description: "IT automation and configuration management".to_string(),
            category: ToolCategory::Infrastructure,
            command: "ansible".to_string(),
            common_args: vec!["-i".to_string(), "-m".to_string(), "-a".to_string()],
            examples: vec![ToolExample {
                description: "Ping all hosts".to_string(),
                command: "ansible all -m ping".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "pattern".to_string(),
                    description: "Host pattern".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: Some("all".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "module".to_string(),
                    description: "Ansible module".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![
                        "ping".to_string(),
                        "shell".to_string(),
                        "copy".to_string(),
                        "file".to_string(),
                    ],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "ansible-playbook".to_string(),
            description: "Run Ansible playbooks".to_string(),
            category: ToolCategory::Infrastructure,
            command: "ansible-playbook".to_string(),
            common_args: vec!["-i".to_string(), "-e".to_string(), "--check".to_string()],
            examples: vec![ToolExample {
                description: "Run playbook".to_string(),
                command: "ansible-playbook site.yml".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "playbook".to_string(),
                    description: "Playbook file".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "inventory".to_string(),
                    description: "Inventory file".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "pulumi".to_string(),
            description: "Infrastructure as Code using real programming languages".to_string(),
            category: ToolCategory::Infrastructure,
            command: "pulumi".to_string(),
            common_args: vec![
                "up".to_string(),
                "preview".to_string(),
                "destroy".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Preview changes".to_string(),
                command: "pulumi preview".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Pulumi command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "up".to_string(),
                    "preview".to_string(),
                    "destroy".to_string(),
                    "stack".to_string(),
                ],
            }],
        });
    }

    /// Populate container and orchestration tools
    fn populate_container_tools(&mut self) {
        // ==================== Docker ====================
        self.add_tool(OSTool {
            name: "docker".to_string(),
            description: "Container runtime and management".to_string(),
            category: ToolCategory::Containers,
            command: "docker".to_string(),
            common_args: vec![
                "run".to_string(),
                "build".to_string(),
                "ps".to_string(),
                "images".to_string(),
            ],
            examples: vec![
                ToolExample {
                    description: "List running containers".to_string(),
                    command: "docker ps".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Build image".to_string(),
                    command: "docker build -t myapp .".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "command".to_string(),
                    description: "Docker command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "run".to_string(),
                        "build".to_string(),
                        "ps".to_string(),
                        "images".to_string(),
                        "pull".to_string(),
                        "push".to_string(),
                        "exec".to_string(),
                        "logs".to_string(),
                        "stop".to_string(),
                        "rm".to_string(),
                        "rmi".to_string(),
                        "network".to_string(),
                        "volume".to_string(),
                        "compose".to_string(),
                    ],
                },
                ToolParameter {
                    name: "image".to_string(),
                    description: "Image name".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "docker-compose".to_string(),
            description: "Multi-container Docker applications".to_string(),
            category: ToolCategory::Containers,
            command: "docker-compose".to_string(),
            common_args: vec![
                "up".to_string(),
                "down".to_string(),
                "build".to_string(),
                "logs".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Start services".to_string(),
                command: "docker-compose up -d".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "command".to_string(),
                    description: "Compose command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "up".to_string(),
                        "down".to_string(),
                        "build".to_string(),
                        "logs".to_string(),
                        "ps".to_string(),
                    ],
                },
                ToolParameter {
                    name: "detach".to_string(),
                    description: "Run in background".to_string(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default_value: Some("false".to_string()),
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "podman".to_string(),
            description: "Daemonless container engine".to_string(),
            category: ToolCategory::Containers,
            command: "podman".to_string(),
            common_args: vec!["run".to_string(), "build".to_string(), "ps".to_string()],
            examples: vec![ToolExample {
                description: "Run container".to_string(),
                command: "podman run -it alpine".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Podman command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "run".to_string(),
                    "build".to_string(),
                    "ps".to_string(),
                    "images".to_string(),
                ],
            }],
        });

        // ==================== Kubernetes ====================
        self.add_tool(OSTool {
            name: "kubectl".to_string(),
            description: "Kubernetes CLI".to_string(),
            category: ToolCategory::Kubernetes,
            command: "kubectl".to_string(),
            common_args: vec![
                "get".to_string(),
                "apply".to_string(),
                "delete".to_string(),
                "describe".to_string(),
            ],
            examples: vec![
                ToolExample {
                    description: "List pods".to_string(),
                    command: "kubectl get pods".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Apply manifest".to_string(),
                    command: "kubectl apply -f deployment.yaml".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "command".to_string(),
                    description: "kubectl command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "get".to_string(),
                        "apply".to_string(),
                        "delete".to_string(),
                        "describe".to_string(),
                        "logs".to_string(),
                        "exec".to_string(),
                        "port-forward".to_string(),
                        "scale".to_string(),
                    ],
                },
                ToolParameter {
                    name: "resource".to_string(),
                    description: "Resource type".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![
                        "pods".to_string(),
                        "services".to_string(),
                        "deployments".to_string(),
                        "configmaps".to_string(),
                        "secrets".to_string(),
                        "nodes".to_string(),
                    ],
                },
                ToolParameter {
                    name: "namespace".to_string(),
                    description: "Kubernetes namespace".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "helm".to_string(),
            description: "Kubernetes package manager".to_string(),
            category: ToolCategory::Kubernetes,
            command: "helm".to_string(),
            common_args: vec![
                "install".to_string(),
                "upgrade".to_string(),
                "list".to_string(),
            ],
            examples: vec![
                ToolExample {
                    description: "Install chart".to_string(),
                    command: "helm install myapp ./chart".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "List releases".to_string(),
                    command: "helm list".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "command".to_string(),
                    description: "Helm command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "install".to_string(),
                        "upgrade".to_string(),
                        "uninstall".to_string(),
                        "list".to_string(),
                        "repo".to_string(),
                        "search".to_string(),
                    ],
                },
                ToolParameter {
                    name: "release".to_string(),
                    description: "Release name".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "k9s".to_string(),
            description: "Kubernetes TUI".to_string(),
            category: ToolCategory::Kubernetes,
            command: "k9s".to_string(),
            common_args: vec!["--namespace".to_string(), "--context".to_string()],
            examples: vec![ToolExample {
                description: "Launch k9s".to_string(),
                command: "k9s".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "namespace".to_string(),
                description: "Starting namespace".to_string(),
                param_type: ParameterType::String,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "minikube".to_string(),
            description: "Local Kubernetes cluster".to_string(),
            category: ToolCategory::Kubernetes,
            command: "minikube".to_string(),
            common_args: vec![
                "start".to_string(),
                "stop".to_string(),
                "status".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Start cluster".to_string(),
                command: "minikube start".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Minikube command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "start".to_string(),
                    "stop".to_string(),
                    "delete".to_string(),
                    "status".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "kind".to_string(),
            description: "Kubernetes in Docker".to_string(),
            category: ToolCategory::Kubernetes,
            command: "kind".to_string(),
            common_args: vec![
                "create".to_string(),
                "delete".to_string(),
                "get".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Create cluster".to_string(),
                command: "kind create cluster".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "kind command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "create".to_string(),
                    "delete".to_string(),
                    "get".to_string(),
                    "load".to_string(),
                ],
            }],
        });
    }

    /// Populate database and data tools
    fn populate_data_tools(&mut self) {
        // ==================== Database Tools ====================
        self.add_tool(OSTool {
            name: "psql".to_string(),
            description: "PostgreSQL interactive terminal".to_string(),
            category: ToolCategory::Database,
            command: "psql".to_string(),
            common_args: vec!["-h".to_string(), "-U".to_string(), "-d".to_string()],
            examples: vec![ToolExample {
                description: "Connect to database".to_string(),
                command: "psql -h localhost -U postgres -d mydb".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "host".to_string(),
                    description: "Database host".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: Some("localhost".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "user".to_string(),
                    description: "Database user".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "database".to_string(),
                    description: "Database name".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "command".to_string(),
                    description: "SQL command to execute".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "mysql".to_string(),
            description: "MySQL command-line client".to_string(),
            category: ToolCategory::Database,
            command: "mysql".to_string(),
            common_args: vec!["-h".to_string(), "-u".to_string(), "-p".to_string()],
            examples: vec![ToolExample {
                description: "Connect to MySQL".to_string(),
                command: "mysql -h localhost -u root -p".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "host".to_string(),
                    description: "Database host".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: Some("localhost".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "user".to_string(),
                    description: "Database user".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "database".to_string(),
                    description: "Database name".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "mongosh".to_string(),
            description: "MongoDB Shell".to_string(),
            category: ToolCategory::Database,
            command: "mongosh".to_string(),
            common_args: vec!["--host".to_string(), "--port".to_string()],
            examples: vec![ToolExample {
                description: "Connect to MongoDB".to_string(),
                command: "mongosh mongodb://localhost:27017/mydb".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "uri".to_string(),
                description: "MongoDB connection URI".to_string(),
                param_type: ParameterType::String,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "redis-cli".to_string(),
            description: "Redis command-line interface".to_string(),
            category: ToolCategory::Database,
            command: "redis-cli".to_string(),
            common_args: vec!["-h".to_string(), "-p".to_string(), "-a".to_string()],
            examples: vec![ToolExample {
                description: "Connect to Redis".to_string(),
                command: "redis-cli -h localhost -p 6379".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "host".to_string(),
                    description: "Redis host".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: Some("localhost".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "port".to_string(),
                    description: "Redis port".to_string(),
                    param_type: ParameterType::Port,
                    required: false,
                    default_value: Some("6379".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "command".to_string(),
                    description: "Redis command".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "sqlite3".to_string(),
            description: "SQLite command-line shell".to_string(),
            category: ToolCategory::Database,
            command: "sqlite3".to_string(),
            common_args: vec!["-column".to_string(), "-header".to_string()],
            examples: vec![ToolExample {
                description: "Open database".to_string(),
                command: "sqlite3 mydb.db".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "database".to_string(),
                description: "Database file path".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        // ==================== Data Processing Tools ====================
        self.add_tool(OSTool {
            name: "csvtool".to_string(),
            description: "CSV file manipulation".to_string(),
            category: ToolCategory::DataProcessing,
            command: "csvtool".to_string(),
            common_args: vec!["col".to_string(), "head".to_string(), "join".to_string()],
            examples: vec![ToolExample {
                description: "Extract columns".to_string(),
                command: "csvtool col 1,3 data.csv".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "command".to_string(),
                    description: "csvtool command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "col".to_string(),
                        "head".to_string(),
                        "join".to_string(),
                        "cat".to_string(),
                    ],
                },
                ToolParameter {
                    name: "file".to_string(),
                    description: "CSV file".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "xsv".to_string(),
            description: "Fast CSV toolkit".to_string(),
            category: ToolCategory::DataProcessing,
            command: "xsv".to_string(),
            common_args: vec![
                "select".to_string(),
                "search".to_string(),
                "stats".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Get statistics".to_string(),
                command: "xsv stats data.csv".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "command".to_string(),
                    description: "xsv command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "select".to_string(),
                        "search".to_string(),
                        "stats".to_string(),
                        "sort".to_string(),
                    ],
                },
                ToolParameter {
                    name: "file".to_string(),
                    description: "CSV file".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "mlr".to_string(),
            description: "Miller - like awk for CSV/JSON".to_string(),
            category: ToolCategory::DataProcessing,
            command: "mlr".to_string(),
            common_args: vec!["--csv".to_string(), "--json".to_string()],
            examples: vec![ToolExample {
                description: "Filter CSV".to_string(),
                command: "mlr --csv filter '$age > 30' data.csv".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "format".to_string(),
                    description: "Input format".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: Some("--csv".to_string()),
                    enum_values: vec![
                        "--csv".to_string(),
                        "--json".to_string(),
                        "--pprint".to_string(),
                    ],
                },
                ToolParameter {
                    name: "command".to_string(),
                    description: "mlr command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
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

        // ==================== ML Tools ====================
        self.add_tool(OSTool {
            name: "python".to_string(),
            description: "Python interpreter".to_string(),
            category: ToolCategory::MachineLearning,
            command: "python".to_string(),
            common_args: vec!["-c".to_string(), "-m".to_string()],
            examples: vec![ToolExample {
                description: "Run script".to_string(),
                command: "python script.py".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: Some(CrossPlatformCommand {
                canonical: "python".to_string(),
                linux: Some("python3".to_string()),
                bsd: Some("python3".to_string()),
                macos: Some("python3".to_string()),
                windows: Some("python".to_string()),
                ios: Some("python3".to_string()),
                android: Some("python".to_string()),
                arg_mappings: HashMap::new(),
            }),
            parameters: vec![
                ToolParameter {
                    name: "script".to_string(),
                    description: "Python script to run".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "code".to_string(),
                    description: "Python code to execute".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "pip".to_string(),
            description: "Python package installer".to_string(),
            category: ToolCategory::MachineLearning,
            command: "pip".to_string(),
            common_args: vec![
                "install".to_string(),
                "list".to_string(),
                "freeze".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Install package".to_string(),
                command: "pip install numpy pandas".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: Some(CrossPlatformCommand {
                canonical: "pip".to_string(),
                linux: Some("pip3".to_string()),
                bsd: Some("pip3".to_string()),
                macos: Some("pip3".to_string()),
                windows: Some("pip".to_string()),
                ios: Some("pip3".to_string()),
                android: Some("pip".to_string()),
                arg_mappings: HashMap::new(),
            }),
            parameters: vec![
                ToolParameter {
                    name: "command".to_string(),
                    description: "pip command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "install".to_string(),
                        "uninstall".to_string(),
                        "list".to_string(),
                        "freeze".to_string(),
                    ],
                },
                ToolParameter {
                    name: "packages".to_string(),
                    description: "Package names".to_string(),
                    param_type: ParameterType::Array,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "jupyter".to_string(),
            description: "Jupyter notebook server".to_string(),
            category: ToolCategory::MachineLearning,
            command: "jupyter".to_string(),
            common_args: vec!["notebook".to_string(), "lab".to_string()],
            examples: vec![ToolExample {
                description: "Start Jupyter Lab".to_string(),
                command: "jupyter lab".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Jupyter command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: Some("lab".to_string()),
                enum_values: vec![
                    "notebook".to_string(),
                    "lab".to_string(),
                    "nbconvert".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "ollama".to_string(),
            description: "Local LLM runner".to_string(),
            category: ToolCategory::MachineLearning,
            command: "ollama".to_string(),
            common_args: vec!["run".to_string(), "pull".to_string(), "list".to_string()],
            examples: vec![
                ToolExample {
                    description: "Run model".to_string(),
                    command: "ollama run llama3".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "List models".to_string(),
                    command: "ollama list".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "command".to_string(),
                    description: "ollama command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "run".to_string(),
                        "pull".to_string(),
                        "list".to_string(),
                        "rm".to_string(),
                        "serve".to_string(),
                    ],
                },
                ToolParameter {
                    name: "model".to_string(),
                    description: "Model name".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "conda".to_string(),
            description: "Conda package and environment manager".to_string(),
            category: ToolCategory::MachineLearning,
            command: "conda".to_string(),
            common_args: vec![
                "create".to_string(),
                "activate".to_string(),
                "install".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Create environment".to_string(),
                command: "conda create -n myenv python=3.11".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "command".to_string(),
                    description: "conda command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "create".to_string(),
                        "activate".to_string(),
                        "deactivate".to_string(),
                        "install".to_string(),
                        "list".to_string(),
                        "env".to_string(),
                    ],
                },
                ToolParameter {
                    name: "name".to_string(),
                    description: "Environment name".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        // Additional ML/Data tools
        self.add_tool(OSTool {
            name: "dvc".to_string(),
            description: "Data Version Control for ML experiments".to_string(),
            category: ToolCategory::MachineLearning,
            command: "dvc".to_string(),
            common_args: vec![
                "init".to_string(),
                "add".to_string(),
                "push".to_string(),
                "pull".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Track data file".to_string(),
                command: "dvc add data/dataset.csv".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "DVC command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "init".to_string(),
                    "add".to_string(),
                    "push".to_string(),
                    "pull".to_string(),
                    "repro".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "mlflow".to_string(),
            description: "MLflow experiment tracking and model registry".to_string(),
            category: ToolCategory::MachineLearning,
            command: "mlflow".to_string(),
            common_args: vec!["ui".to_string(), "run".to_string(), "models".to_string()],
            examples: vec![ToolExample {
                description: "Start MLflow UI".to_string(),
                command: "mlflow ui".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "MLflow command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "ui".to_string(),
                    "run".to_string(),
                    "models".to_string(),
                    "server".to_string(),
                ],
            }],
        });
    }

    /// Additional system administration and DevOps tools
    fn populate_sysadmin_tools(&mut self) {
        // System monitoring tools
        self.add_tool(OSTool {
            name: "htop".to_string(),
            description: "Interactive process viewer".to_string(),
            category: ToolCategory::Monitoring,
            command: "htop".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Start htop".to_string(),
                command: "htop".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "btop".to_string(),
            description: "Modern resource monitor".to_string(),
            category: ToolCategory::Monitoring,
            command: "btop".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Start btop".to_string(),
                command: "btop".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "glances".to_string(),
            description: "Cross-platform system monitoring tool".to_string(),
            category: ToolCategory::Monitoring,
            command: "glances".to_string(),
            common_args: vec!["-w".to_string()],
            examples: vec![ToolExample {
                description: "Start web interface".to_string(),
                command: "glances -w".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "mode".to_string(),
                description: "Run mode".to_string(),
                param_type: ParameterType::String,
                required: false,
                default_value: None,
                enum_values: vec!["-w".to_string(), "-s".to_string(), "-c".to_string()],
            }],
        });

        self.add_tool(OSTool {
            name: "iotop".to_string(),
            description: "I/O monitoring tool".to_string(),
            category: ToolCategory::Monitoring,
            command: "iotop".to_string(),
            common_args: vec!["-o".to_string()],
            examples: vec![ToolExample {
                description: "Show only active I/O".to_string(),
                command: "iotop -o".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: true,
            supported_os: vec![OperatingSystem::Linux],
            cross_platform: None,
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "nethogs".to_string(),
            description: "Network bandwidth monitoring per process".to_string(),
            category: ToolCategory::Monitoring,
            command: "nethogs".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Monitor network usage".to_string(),
                command: "nethogs".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: true,
            supported_os: vec![OperatingSystem::Linux],
            cross_platform: None,
            parameters: vec![],
        });

        // Disk tools
        self.add_tool(OSTool {
            name: "ncdu".to_string(),
            description: "NCurses disk usage analyzer".to_string(),
            category: ToolCategory::FileSystem,
            command: "ncdu".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Analyze current directory".to_string(),
                command: "ncdu .".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "path".to_string(),
                description: "Directory to analyze".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: Some(".".to_string()),
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "duf".to_string(),
            description: "Modern disk usage/free utility".to_string(),
            category: ToolCategory::FileSystem,
            command: "duf".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Show disk usage".to_string(),
                command: "duf".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "dust".to_string(),
            description: "Modern du alternative".to_string(),
            category: ToolCategory::FileSystem,
            command: "dust".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Show directory sizes".to_string(),
                command: "dust".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "path".to_string(),
                description: "Directory to analyze".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: Some(".".to_string()),
                enum_values: vec![],
            }],
        });

        // Modern CLI tools
        self.add_tool(OSTool {
            name: "bat".to_string(),
            description: "Cat clone with syntax highlighting".to_string(),
            category: ToolCategory::FileSystem,
            command: "bat".to_string(),
            common_args: vec!["--style=full".to_string()],
            examples: vec![ToolExample {
                description: "View file with highlighting".to_string(),
                command: "bat file.rs".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "File to display".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "exa".to_string(),
            description: "Modern ls replacement".to_string(),
            category: ToolCategory::FileSystem,
            command: "exa".to_string(),
            common_args: vec!["-la".to_string(), "--git".to_string()],
            examples: vec![ToolExample {
                description: "List with git status".to_string(),
                command: "exa -la --git".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "path".to_string(),
                description: "Directory to list".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: Some(".".to_string()),
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "eza".to_string(),
            description: "Modern ls replacement (exa fork)".to_string(),
            category: ToolCategory::FileSystem,
            command: "eza".to_string(),
            common_args: vec![
                "-la".to_string(),
                "--git".to_string(),
                "--icons".to_string(),
            ],
            examples: vec![ToolExample {
                description: "List with icons".to_string(),
                command: "eza -la --icons".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "path".to_string(),
                description: "Directory to list".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: Some(".".to_string()),
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "fd".to_string(),
            description: "Fast find alternative".to_string(),
            category: ToolCategory::SearchTools,
            command: "fd".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Find files".to_string(),
                command: "fd '*.rs'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "pattern".to_string(),
                    description: "Search pattern".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "path".to_string(),
                    description: "Search path".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: Some(".".to_string()),
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "rg".to_string(),
            description: "Ripgrep - fast recursive grep".to_string(),
            category: ToolCategory::SearchTools,
            command: "rg".to_string(),
            common_args: vec!["--smart-case".to_string()],
            examples: vec![ToolExample {
                description: "Search in code".to_string(),
                command: "rg 'function' --type rust".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "pattern".to_string(),
                    description: "Search pattern".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "path".to_string(),
                    description: "Search path".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: Some(".".to_string()),
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "fzf".to_string(),
            description: "Fuzzy finder".to_string(),
            category: ToolCategory::SearchTools,
            command: "fzf".to_string(),
            common_args: vec!["--preview".to_string()],
            examples: vec![ToolExample {
                description: "Interactive file search".to_string(),
                command: "fzf --preview 'cat {}'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "ag".to_string(),
            description: "The Silver Searcher".to_string(),
            category: ToolCategory::SearchTools,
            command: "ag".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Search pattern".to_string(),
                command: "ag 'TODO'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "pattern".to_string(),
                description: "Search pattern".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        // JSON/YAML tools
        self.add_tool(OSTool {
            name: "jq".to_string(),
            description: "JSON processor".to_string(),
            category: ToolCategory::DataProcessing,
            command: "jq".to_string(),
            common_args: vec![".".to_string()],
            examples: vec![ToolExample {
                description: "Pretty print JSON".to_string(),
                command: "cat data.json | jq '.'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "filter".to_string(),
                    description: "jq filter expression".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: Some(".".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "file".to_string(),
                    description: "JSON file".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "yq".to_string(),
            description: "YAML processor".to_string(),
            category: ToolCategory::DataProcessing,
            command: "yq".to_string(),
            common_args: vec![".".to_string()],
            examples: vec![ToolExample {
                description: "Read YAML".to_string(),
                command: "yq '.key' file.yaml".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "expression".to_string(),
                    description: "yq expression".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: Some(".".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "file".to_string(),
                    description: "YAML file".to_string(),
                    param_type: ParameterType::Path,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        // Git tools
        self.add_tool(OSTool {
            name: "git".to_string(),
            description: "Distributed version control".to_string(),
            category: ToolCategory::Development,
            command: "git".to_string(),
            common_args: vec!["status".to_string(), "log".to_string(), "diff".to_string()],
            examples: vec![
                ToolExample {
                    description: "Check status".to_string(),
                    command: "git status".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "View log".to_string(),
                    command: "git log --oneline -10".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Git command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "status".to_string(),
                    "log".to_string(),
                    "diff".to_string(),
                    "add".to_string(),
                    "commit".to_string(),
                    "push".to_string(),
                    "pull".to_string(),
                    "branch".to_string(),
                    "checkout".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "gh".to_string(),
            description: "GitHub CLI".to_string(),
            category: ToolCategory::Development,
            command: "gh".to_string(),
            common_args: vec!["pr".to_string(), "issue".to_string(), "repo".to_string()],
            examples: vec![ToolExample {
                description: "List PRs".to_string(),
                command: "gh pr list".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "gh command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "pr".to_string(),
                    "issue".to_string(),
                    "repo".to_string(),
                    "workflow".to_string(),
                    "release".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "lazygit".to_string(),
            description: "Terminal UI for git".to_string(),
            category: ToolCategory::Development,
            command: "lazygit".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Open lazygit".to_string(),
                command: "lazygit".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "delta".to_string(),
            description: "Syntax-highlighting pager for git/diff".to_string(),
            category: ToolCategory::Development,
            command: "delta".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Use with git diff".to_string(),
                command: "git diff | delta".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![],
        });

        // HTTP/API tools
        self.add_tool(OSTool {
            name: "httpie".to_string(),
            description: "User-friendly HTTP client".to_string(),
            category: ToolCategory::WebTools,
            command: "http".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "GET request".to_string(),
                command: "http GET https://api.example.com".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
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
                        "PATCH".to_string(),
                    ],
                },
                ToolParameter {
                    name: "url".to_string(),
                    description: "URL to request".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "xh".to_string(),
            description: "Fast HTTPie alternative".to_string(),
            category: ToolCategory::WebTools,
            command: "xh".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "GET request".to_string(),
                command: "xh GET https://api.example.com".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
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
                    description: "URL to request".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        // Network diagnostics
        self.add_tool(OSTool {
            name: "mtr".to_string(),
            description: "Network diagnostic tool combining ping and traceroute".to_string(),
            category: ToolCategory::NetworkTools,
            command: "mtr".to_string(),
            common_args: vec!["--report".to_string()],
            examples: vec![ToolExample {
                description: "Network trace".to_string(),
                command: "mtr --report google.com".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
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
            name: "nmap".to_string(),
            description: "Network scanner".to_string(),
            category: ToolCategory::NetworkTools,
            command: "nmap".to_string(),
            common_args: vec!["-sV".to_string()],
            examples: vec![ToolExample {
                description: "Scan ports".to_string(),
                command: "nmap -sV localhost".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "target".to_string(),
                description: "Target host or network".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        // Text editors
        self.add_tool(OSTool {
            name: "nvim".to_string(),
            description: "Neovim text editor".to_string(),
            category: ToolCategory::Development,
            command: "nvim".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Edit file".to_string(),
                command: "nvim file.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "File to edit".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "helix".to_string(),
            description: "Helix text editor".to_string(),
            category: ToolCategory::Development,
            command: "hx".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Edit file".to_string(),
                command: "hx file.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "File to edit".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        // Compression tools
        self.add_tool(OSTool {
            name: "zstd".to_string(),
            description: "Fast compression algorithm".to_string(),
            category: ToolCategory::Archives,
            command: "zstd".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Compress file".to_string(),
                command: "zstd file.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "File to compress".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "pigz".to_string(),
            description: "Parallel gzip".to_string(),
            category: ToolCategory::Archives,
            command: "pigz".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Compress with multiple cores".to_string(),
                command: "pigz file.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "File to compress".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        // File transfer
        self.add_tool(OSTool {
            name: "rsync".to_string(),
            description: "Fast incremental file transfer".to_string(),
            category: ToolCategory::FileSystem,
            command: "rsync".to_string(),
            common_args: vec!["-avz".to_string()],
            examples: vec![ToolExample {
                description: "Sync directories".to_string(),
                command: "rsync -avz src/ dest/".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
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
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "rclone".to_string(),
            description: "Cloud storage sync".to_string(),
            category: ToolCategory::FileSystem,
            command: "rclone".to_string(),
            common_args: vec!["sync".to_string(), "copy".to_string()],
            examples: vec![ToolExample {
                description: "Sync to cloud".to_string(),
                command: "rclone sync local/ remote:bucket/".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "rclone command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "sync".to_string(),
                    "copy".to_string(),
                    "ls".to_string(),
                    "lsd".to_string(),
                    "config".to_string(),
                ],
            }],
        });

        // System tools
        self.add_tool(OSTool {
            name: "tmux".to_string(),
            description: "Terminal multiplexer".to_string(),
            category: ToolCategory::Utilities,
            command: "tmux".to_string(),
            common_args: vec!["new".to_string(), "attach".to_string()],
            examples: vec![ToolExample {
                description: "New session".to_string(),
                command: "tmux new -s main".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "tmux command".to_string(),
                param_type: ParameterType::String,
                required: false,
                default_value: None,
                enum_values: vec![
                    "new".to_string(),
                    "attach".to_string(),
                    "ls".to_string(),
                    "kill-session".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "zellij".to_string(),
            description: "Modern terminal workspace".to_string(),
            category: ToolCategory::Utilities,
            command: "zellij".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Start zellij".to_string(),
                command: "zellij".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
            cross_platform: None,
            parameters: vec![],
        });

        // Build tools
        self.add_tool(OSTool {
            name: "make".to_string(),
            description: "Build automation tool".to_string(),
            category: ToolCategory::Development,
            command: "make".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Build project".to_string(),
                command: "make".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "target".to_string(),
                description: "Make target".to_string(),
                param_type: ParameterType::String,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "cmake".to_string(),
            description: "Cross-platform build system".to_string(),
            category: ToolCategory::Development,
            command: "cmake".to_string(),
            common_args: vec![".".to_string()],
            examples: vec![ToolExample {
                description: "Configure build".to_string(),
                command: "cmake -B build".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "path".to_string(),
                description: "Source directory".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: Some(".".to_string()),
                enum_values: vec![],
            }],
        });

        // Language tools
        self.add_tool(OSTool {
            name: "cargo".to_string(),
            description: "Rust package manager".to_string(),
            category: ToolCategory::Development,
            command: "cargo".to_string(),
            common_args: vec!["build".to_string(), "test".to_string(), "run".to_string()],
            examples: vec![
                ToolExample {
                    description: "Build project".to_string(),
                    command: "cargo build --release".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Run tests".to_string(),
                    command: "cargo test".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Cargo command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "build".to_string(),
                    "test".to_string(),
                    "run".to_string(),
                    "check".to_string(),
                    "clippy".to_string(),
                    "fmt".to_string(),
                    "doc".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "npm".to_string(),
            description: "Node.js package manager".to_string(),
            category: ToolCategory::Development,
            command: "npm".to_string(),
            common_args: vec!["install".to_string(), "run".to_string()],
            examples: vec![ToolExample {
                description: "Install dependencies".to_string(),
                command: "npm install".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "npm command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "install".to_string(),
                    "run".to_string(),
                    "start".to_string(),
                    "test".to_string(),
                    "build".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "pnpm".to_string(),
            description: "Fast Node.js package manager".to_string(),
            category: ToolCategory::Development,
            command: "pnpm".to_string(),
            common_args: vec!["install".to_string(), "run".to_string()],
            examples: vec![ToolExample {
                description: "Install dependencies".to_string(),
                command: "pnpm install".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "pnpm command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "install".to_string(),
                    "run".to_string(),
                    "start".to_string(),
                    "test".to_string(),
                    "build".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "bun".to_string(),
            description: "Fast JavaScript runtime and toolkit".to_string(),
            category: ToolCategory::Development,
            command: "bun".to_string(),
            common_args: vec!["run".to_string(), "install".to_string()],
            examples: vec![ToolExample {
                description: "Run script".to_string(),
                command: "bun run script.ts".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Bun command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "run".to_string(),
                    "install".to_string(),
                    "build".to_string(),
                    "test".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "deno".to_string(),
            description: "Secure JavaScript/TypeScript runtime".to_string(),
            category: ToolCategory::Development,
            command: "deno".to_string(),
            common_args: vec!["run".to_string(), "task".to_string()],
            examples: vec![ToolExample {
                description: "Run script".to_string(),
                command: "deno run script.ts".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Deno command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "run".to_string(),
                    "task".to_string(),
                    "compile".to_string(),
                    "test".to_string(),
                    "bench".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "go".to_string(),
            description: "Go programming language".to_string(),
            category: ToolCategory::Development,
            command: "go".to_string(),
            common_args: vec!["build".to_string(), "run".to_string(), "test".to_string()],
            examples: vec![ToolExample {
                description: "Build project".to_string(),
                command: "go build".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Go command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "build".to_string(),
                    "run".to_string(),
                    "test".to_string(),
                    "mod".to_string(),
                    "get".to_string(),
                ],
            }],
        });

        // Additional language tools
        self.add_tool(OSTool {
            name: "rustc".to_string(),
            description: "Rust compiler".to_string(),
            category: ToolCategory::Development,
            command: "rustc".to_string(),
            common_args: vec!["--version".to_string()],
            examples: vec![ToolExample {
                description: "Compile file".to_string(),
                command: "rustc main.rs".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "Source file".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "rustup".to_string(),
            description: "Rust toolchain manager".to_string(),
            category: ToolCategory::Development,
            command: "rustup".to_string(),
            common_args: vec!["update".to_string(), "show".to_string()],
            examples: vec![ToolExample {
                description: "Update Rust".to_string(),
                command: "rustup update".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Rustup command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "update".to_string(),
                    "show".to_string(),
                    "default".to_string(),
                    "target".to_string(),
                    "component".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "java".to_string(),
            description: "Java runtime".to_string(),
            category: ToolCategory::Development,
            command: "java".to_string(),
            common_args: vec!["-version".to_string()],
            examples: vec![ToolExample {
                description: "Run Java app".to_string(),
                command: "java -jar app.jar".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "jar".to_string(),
                description: "JAR file to run".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "mvn".to_string(),
            description: "Maven build tool".to_string(),
            category: ToolCategory::Development,
            command: "mvn".to_string(),
            common_args: vec![
                "clean".to_string(),
                "install".to_string(),
                "package".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Build project".to_string(),
                command: "mvn clean install".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "goal".to_string(),
                description: "Maven goal".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "clean".to_string(),
                    "compile".to_string(),
                    "test".to_string(),
                    "package".to_string(),
                    "install".to_string(),
                    "deploy".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "gradle".to_string(),
            description: "Gradle build tool".to_string(),
            category: ToolCategory::Development,
            command: "gradle".to_string(),
            common_args: vec!["build".to_string(), "test".to_string()],
            examples: vec![ToolExample {
                description: "Build project".to_string(),
                command: "gradle build".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "task".to_string(),
                description: "Gradle task".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "build".to_string(),
                    "test".to_string(),
                    "clean".to_string(),
                    "assemble".to_string(),
                ],
            }],
        });
    }

    /// Security and cryptography tools
    fn populate_security_tools(&mut self) {
        self.add_tool(OSTool {
            name: "openssl".to_string(),
            description: "OpenSSL cryptographic toolkit".to_string(),
            category: ToolCategory::Security,
            command: "openssl".to_string(),
            common_args: vec!["version".to_string()],
            examples: vec![
                ToolExample {
                    description: "Generate RSA key".to_string(),
                    command: "openssl genrsa -out key.pem 2048".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Generate self-signed cert".to_string(),
                    command:
                        "openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365"
                            .to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "OpenSSL command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "genrsa".to_string(),
                    "req".to_string(),
                    "x509".to_string(),
                    "enc".to_string(),
                    "dgst".to_string(),
                    "s_client".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "ssh".to_string(),
            description: "Secure shell client".to_string(),
            category: ToolCategory::Security,
            command: "ssh".to_string(),
            common_args: vec!["-v".to_string()],
            examples: vec![ToolExample {
                description: "Connect to server".to_string(),
                command: "ssh user@hostname".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "destination".to_string(),
                description: "user@host".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "ssh-keygen".to_string(),
            description: "SSH key generator".to_string(),
            category: ToolCategory::Security,
            command: "ssh-keygen".to_string(),
            common_args: vec!["-t".to_string(), "ed25519".to_string()],
            examples: vec![ToolExample {
                description: "Generate ED25519 key".to_string(),
                command: "ssh-keygen -t ed25519 -C 'email@example.com'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "type".to_string(),
                description: "Key type".to_string(),
                param_type: ParameterType::String,
                required: false,
                default_value: Some("ed25519".to_string()),
                enum_values: vec![
                    "rsa".to_string(),
                    "ed25519".to_string(),
                    "ecdsa".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "gpg".to_string(),
            description: "GNU Privacy Guard".to_string(),
            category: ToolCategory::Cryptography,
            command: "gpg".to_string(),
            common_args: vec!["--list-keys".to_string()],
            examples: vec![
                ToolExample {
                    description: "List keys".to_string(),
                    command: "gpg --list-keys".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Encrypt file".to_string(),
                    command: "gpg -c file.txt".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "operation".to_string(),
                description: "GPG operation".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "--encrypt".to_string(),
                    "--decrypt".to_string(),
                    "--sign".to_string(),
                    "--verify".to_string(),
                    "--list-keys".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "age".to_string(),
            description: "Modern file encryption".to_string(),
            category: ToolCategory::Cryptography,
            command: "age".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Encrypt file".to_string(),
                command: "age -r recipient -o file.age file.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "File to encrypt".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "sops".to_string(),
            description: "Secrets management".to_string(),
            category: ToolCategory::Security,
            command: "sops".to_string(),
            common_args: vec!["--encrypt".to_string(), "--decrypt".to_string()],
            examples: vec![ToolExample {
                description: "Edit encrypted file".to_string(),
                command: "sops secrets.yaml".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "Secrets file".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "vault".to_string(),
            description: "HashiCorp Vault secrets management".to_string(),
            category: ToolCategory::Security,
            command: "vault".to_string(),
            common_args: vec!["status".to_string()],
            examples: vec![ToolExample {
                description: "Read secret".to_string(),
                command: "vault kv get secret/myapp".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Vault command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "status".to_string(),
                    "login".to_string(),
                    "kv".to_string(),
                    "secrets".to_string(),
                ],
            }],
        });

        // Network security tools
        self.add_tool(OSTool {
            name: "tcpdump".to_string(),
            description: "Network packet analyzer".to_string(),
            category: ToolCategory::NetworkTools,
            command: "tcpdump".to_string(),
            common_args: vec!["-i".to_string(), "any".to_string()],
            examples: vec![ToolExample {
                description: "Capture packets".to_string(),
                command: "tcpdump -i eth0 -w capture.pcap".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: true,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "interface".to_string(),
                description: "Network interface".to_string(),
                param_type: ParameterType::String,
                required: false,
                default_value: Some("any".to_string()),
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "wireshark".to_string(),
            description: "Network protocol analyzer (CLI: tshark)".to_string(),
            category: ToolCategory::NetworkTools,
            command: "tshark".to_string(),
            common_args: vec!["-i".to_string()],
            examples: vec![ToolExample {
                description: "Capture traffic".to_string(),
                command: "tshark -i eth0".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: true,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "interface".to_string(),
                description: "Network interface".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "netcat".to_string(),
            description: "Network utility (nc)".to_string(),
            category: ToolCategory::NetworkTools,
            command: "nc".to_string(),
            common_args: vec!["-v".to_string()],
            examples: vec![ToolExample {
                description: "Port check".to_string(),
                command: "nc -zv hostname 80".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "host".to_string(),
                    description: "Target host".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "port".to_string(),
                    description: "Target port".to_string(),
                    param_type: ParameterType::Integer,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "socat".to_string(),
            description: "Multipurpose relay".to_string(),
            category: ToolCategory::NetworkTools,
            command: "socat".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "TCP proxy".to_string(),
                command: "socat TCP-LISTEN:8080,fork TCP:localhost:80".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![],
        });

        // System administration
        self.add_tool(OSTool {
            name: "systemctl".to_string(),
            description: "Systemd service manager".to_string(),
            category: ToolCategory::SystemInfo,
            command: "systemctl".to_string(),
            common_args: vec!["status".to_string()],
            examples: vec![
                ToolExample {
                    description: "Service status".to_string(),
                    command: "systemctl status nginx".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "List services".to_string(),
                    command: "systemctl list-units --type=service".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "command".to_string(),
                    description: "systemctl command".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![
                        "status".to_string(),
                        "start".to_string(),
                        "stop".to_string(),
                        "restart".to_string(),
                        "enable".to_string(),
                        "disable".to_string(),
                        "list-units".to_string(),
                    ],
                },
                ToolParameter {
                    name: "service".to_string(),
                    description: "Service name".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "journalctl".to_string(),
            description: "Systemd journal viewer".to_string(),
            category: ToolCategory::SystemInfo,
            command: "journalctl".to_string(),
            common_args: vec!["-f".to_string(), "-u".to_string()],
            examples: vec![ToolExample {
                description: "Follow logs".to_string(),
                command: "journalctl -f -u nginx".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "unit".to_string(),
                description: "Service unit".to_string(),
                param_type: ParameterType::String,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "dmesg".to_string(),
            description: "Kernel ring buffer".to_string(),
            category: ToolCategory::SystemInfo,
            command: "dmesg".to_string(),
            common_args: vec!["-T".to_string()],
            examples: vec![ToolExample {
                description: "View kernel messages".to_string(),
                command: "dmesg -T | tail -50".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::BSD],
            cross_platform: None,
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "lsof".to_string(),
            description: "List open files".to_string(),
            category: ToolCategory::SystemInfo,
            command: "lsof".to_string(),
            common_args: vec!["-i".to_string()],
            examples: vec![ToolExample {
                description: "List network connections".to_string(),
                command: "lsof -i :80".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "strace".to_string(),
            description: "System call tracer".to_string(),
            category: ToolCategory::Development,
            command: "strace".to_string(),
            common_args: vec!["-f".to_string()],
            examples: vec![ToolExample {
                description: "Trace process".to_string(),
                command: "strace -f -p 1234".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: true,
            supported_os: vec![OperatingSystem::Linux],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "pid".to_string(),
                description: "Process ID".to_string(),
                param_type: ParameterType::Integer,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "perf".to_string(),
            description: "Linux profiling tool".to_string(),
            category: ToolCategory::Development,
            command: "perf".to_string(),
            common_args: vec!["stat".to_string(), "record".to_string()],
            examples: vec![ToolExample {
                description: "Profile command".to_string(),
                command: "perf stat ./myprogram".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Perf command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "stat".to_string(),
                    "record".to_string(),
                    "report".to_string(),
                    "top".to_string(),
                ],
            }],
        });

        // Virtualization
        self.add_tool(OSTool {
            name: "vagrant".to_string(),
            description: "Development environment manager".to_string(),
            category: ToolCategory::Infrastructure,
            command: "vagrant".to_string(),
            common_args: vec!["up".to_string(), "ssh".to_string()],
            examples: vec![ToolExample {
                description: "Start VM".to_string(),
                command: "vagrant up".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Vagrant command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "up".to_string(),
                    "halt".to_string(),
                    "destroy".to_string(),
                    "ssh".to_string(),
                    "status".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "packer".to_string(),
            description: "Machine image builder".to_string(),
            category: ToolCategory::Infrastructure,
            command: "packer".to_string(),
            common_args: vec!["build".to_string(), "validate".to_string()],
            examples: vec![ToolExample {
                description: "Build image".to_string(),
                command: "packer build template.pkr.hcl".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Packer command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "build".to_string(),
                    "validate".to_string(),
                    "fmt".to_string(),
                    "init".to_string(),
                ],
            }],
        });

        // Text processing
        self.add_tool(OSTool {
            name: "sed".to_string(),
            description: "Stream editor".to_string(),
            category: ToolCategory::TextProcessing,
            command: "sed".to_string(),
            common_args: vec!["-i".to_string()],
            examples: vec![ToolExample {
                description: "Replace text".to_string(),
                command: "sed 's/old/new/g' file.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "expression".to_string(),
                    description: "Sed expression".to_string(),
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
            ],
        });

        self.add_tool(OSTool {
            name: "awk".to_string(),
            description: "Pattern scanning and processing".to_string(),
            category: ToolCategory::TextProcessing,
            command: "awk".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Print column".to_string(),
                command: "awk '{print $1}' file.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "program".to_string(),
                description: "AWK program".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "tr".to_string(),
            description: "Translate characters".to_string(),
            category: ToolCategory::TextProcessing,
            command: "tr".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Lowercase to uppercase".to_string(),
                command: "echo 'hello' | tr 'a-z' 'A-Z'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "set1".to_string(),
                    description: "Characters to replace".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "set2".to_string(),
                    description: "Replacement characters".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "cut".to_string(),
            description: "Remove sections from lines".to_string(),
            category: ToolCategory::TextProcessing,
            command: "cut".to_string(),
            common_args: vec!["-d".to_string(), "-f".to_string()],
            examples: vec![ToolExample {
                description: "Extract field".to_string(),
                command: "cut -d',' -f1 data.csv".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "delimiter".to_string(),
                    description: "Field delimiter".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: Some("\t".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "fields".to_string(),
                    description: "Field numbers".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "sort".to_string(),
            description: "Sort lines".to_string(),
            category: ToolCategory::TextProcessing,
            command: "sort".to_string(),
            common_args: vec!["-n".to_string(), "-r".to_string(), "-u".to_string()],
            examples: vec![ToolExample {
                description: "Sort numerically".to_string(),
                command: "sort -n numbers.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "Input file".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "uniq".to_string(),
            description: "Report or filter repeated lines".to_string(),
            category: ToolCategory::TextProcessing,
            command: "uniq".to_string(),
            common_args: vec!["-c".to_string()],
            examples: vec![ToolExample {
                description: "Count occurrences".to_string(),
                command: "sort file.txt | uniq -c".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "wc".to_string(),
            description: "Word, line, character count".to_string(),
            category: ToolCategory::TextProcessing,
            command: "wc".to_string(),
            common_args: vec!["-l".to_string(), "-w".to_string(), "-c".to_string()],
            examples: vec![ToolExample {
                description: "Count lines".to_string(),
                command: "wc -l file.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "Input file".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "head".to_string(),
            description: "Output first part of files".to_string(),
            category: ToolCategory::TextProcessing,
            command: "head".to_string(),
            common_args: vec!["-n".to_string()],
            examples: vec![ToolExample {
                description: "First 20 lines".to_string(),
                command: "head -n 20 file.txt".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "lines".to_string(),
                description: "Number of lines".to_string(),
                param_type: ParameterType::Integer,
                required: false,
                default_value: Some("10".to_string()),
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "tail".to_string(),
            description: "Output last part of files".to_string(),
            category: ToolCategory::TextProcessing,
            command: "tail".to_string(),
            common_args: vec!["-n".to_string(), "-f".to_string()],
            examples: vec![ToolExample {
                description: "Follow log file".to_string(),
                command: "tail -f /var/log/syslog".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "lines".to_string(),
                description: "Number of lines".to_string(),
                param_type: ParameterType::Integer,
                required: false,
                default_value: Some("10".to_string()),
                enum_values: vec![],
            }],
        });

        // Archive tools
        self.add_tool(OSTool {
            name: "7z".to_string(),
            description: "7-Zip archiver".to_string(),
            category: ToolCategory::Archives,
            command: "7z".to_string(),
            common_args: vec!["a".to_string(), "x".to_string()],
            examples: vec![ToolExample {
                description: "Create archive".to_string(),
                command: "7z a archive.7z files/".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "7z command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "a".to_string(),
                    "x".to_string(),
                    "l".to_string(),
                    "t".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "unzip".to_string(),
            description: "Extract ZIP archives".to_string(),
            category: ToolCategory::Archives,
            command: "unzip".to_string(),
            common_args: vec!["-l".to_string()],
            examples: vec![ToolExample {
                description: "Extract archive".to_string(),
                command: "unzip archive.zip".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "ZIP file".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        // Process tools
        self.add_tool(OSTool {
            name: "pgrep".to_string(),
            description: "Find processes by name".to_string(),
            category: ToolCategory::ProcessManagement,
            command: "pgrep".to_string(),
            common_args: vec!["-l".to_string()],
            examples: vec![ToolExample {
                description: "Find process".to_string(),
                command: "pgrep -l nginx".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "pattern".to_string(),
                description: "Process name pattern".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "pkill".to_string(),
            description: "Kill processes by name".to_string(),
            category: ToolCategory::ProcessManagement,
            command: "pkill".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Kill process".to_string(),
                command: "pkill -9 nginx".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Dangerous,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "pattern".to_string(),
                description: "Process name pattern".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        // Misc utilities
        self.add_tool(OSTool {
            name: "watch".to_string(),
            description: "Execute program periodically".to_string(),
            category: ToolCategory::Utilities,
            command: "watch".to_string(),
            common_args: vec!["-n".to_string(), "1".to_string()],
            examples: vec![ToolExample {
                description: "Watch directory".to_string(),
                command: "watch -n 1 ls -la".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "interval".to_string(),
                    description: "Refresh interval (seconds)".to_string(),
                    param_type: ParameterType::Integer,
                    required: false,
                    default_value: Some("2".to_string()),
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "command".to_string(),
                    description: "Command to run".to_string(),
                    param_type: ParameterType::String,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "xargs".to_string(),
            description: "Build and execute commands from input".to_string(),
            category: ToolCategory::Utilities,
            command: "xargs".to_string(),
            common_args: vec!["-I".to_string(), "{}".to_string()],
            examples: vec![ToolExample {
                description: "Delete files".to_string(),
                command: "find . -name '*.tmp' | xargs rm".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "tee".to_string(),
            description: "Read from stdin and write to stdout and files".to_string(),
            category: ToolCategory::Utilities,
            command: "tee".to_string(),
            common_args: vec!["-a".to_string()],
            examples: vec![ToolExample {
                description: "Log output".to_string(),
                command: "command | tee output.log".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "Output file".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "time".to_string(),
            description: "Time command execution".to_string(),
            category: ToolCategory::Utilities,
            command: "time".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Measure time".to_string(),
                command: "time ./script.sh".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::BSD,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Command to time".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "hyperfine".to_string(),
            description: "Command-line benchmarking tool".to_string(),
            category: ToolCategory::Development,
            command: "hyperfine".to_string(),
            common_args: vec!["--warmup".to_string(), "3".to_string()],
            examples: vec![ToolExample {
                description: "Benchmark commands".to_string(),
                command: "hyperfine 'fd .' 'find .'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Command to benchmark".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "tokei".to_string(),
            description: "Code statistics".to_string(),
            category: ToolCategory::Development,
            command: "tokei".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Count lines of code".to_string(),
                command: "tokei .".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "path".to_string(),
                description: "Directory to analyze".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: Some(".".to_string()),
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "difft".to_string(),
            description: "Structural diff tool".to_string(),
            category: ToolCategory::Development,
            command: "difft".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Compare files".to_string(),
                command: "difft file1.rs file2.rs".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "file1".to_string(),
                    description: "First file".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "file2".to_string(),
                    description: "Second file".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });
    }

    /// AI/ML and data science tools
    fn populate_ai_tools(&mut self) {
        self.add_tool(OSTool {
            name: "huggingface-cli".to_string(),
            description: "Hugging Face Hub CLI".to_string(),
            category: ToolCategory::MachineLearning,
            command: "huggingface-cli".to_string(),
            common_args: vec!["download".to_string(), "login".to_string()],
            examples: vec![ToolExample {
                description: "Download model".to_string(),
                command: "huggingface-cli download meta-llama/Llama-2-7b".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "CLI command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "download".to_string(),
                    "login".to_string(),
                    "upload".to_string(),
                    "repo".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "transformers-cli".to_string(),
            description: "Transformers library CLI".to_string(),
            category: ToolCategory::MachineLearning,
            command: "transformers-cli".to_string(),
            common_args: vec!["convert".to_string()],
            examples: vec![ToolExample {
                description: "Convert model".to_string(),
                command: "transformers-cli convert --model_type bert".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "llm".to_string(),
            description: "LLM CLI tool by Simon Willison".to_string(),
            category: ToolCategory::MachineLearning,
            command: "llm".to_string(),
            common_args: vec!["chat".to_string(), "prompt".to_string()],
            examples: vec![ToolExample {
                description: "Chat with model".to_string(),
                command: "llm chat -m gpt-4".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "model".to_string(),
                description: "Model to use".to_string(),
                param_type: ParameterType::String,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "vllm".to_string(),
            description: "vLLM inference server".to_string(),
            category: ToolCategory::MachineLearning,
            command: "vllm".to_string(),
            common_args: vec!["serve".to_string()],
            examples: vec![ToolExample {
                description: "Start server".to_string(),
                command: "vllm serve meta-llama/Llama-2-7b-hf".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "model".to_string(),
                description: "Model to serve".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "llamacpp".to_string(),
            description: "llama.cpp inference".to_string(),
            category: ToolCategory::MachineLearning,
            command: "llama-cli".to_string(),
            common_args: vec!["-m".to_string()],
            examples: vec![ToolExample {
                description: "Run inference".to_string(),
                command: "llama-cli -m model.gguf -p 'Hello'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "model".to_string(),
                description: "Model file path".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "whisper".to_string(),
            description: "OpenAI Whisper speech recognition".to_string(),
            category: ToolCategory::MachineLearning,
            command: "whisper".to_string(),
            common_args: vec!["--model".to_string(), "base".to_string()],
            examples: vec![ToolExample {
                description: "Transcribe audio".to_string(),
                command: "whisper audio.mp3 --model base".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "file".to_string(),
                    description: "Audio file".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "model".to_string(),
                    description: "Model size".to_string(),
                    param_type: ParameterType::String,
                    required: false,
                    default_value: Some("base".to_string()),
                    enum_values: vec![
                        "tiny".to_string(),
                        "base".to_string(),
                        "small".to_string(),
                        "medium".to_string(),
                        "large".to_string(),
                    ],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "stable-diffusion".to_string(),
            description: "Stable Diffusion image generation".to_string(),
            category: ToolCategory::MachineLearning,
            command: "sd".to_string(),
            common_args: vec!["--prompt".to_string()],
            examples: vec![ToolExample {
                description: "Generate image".to_string(),
                command: "sd --prompt 'a cat in space'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "prompt".to_string(),
                description: "Image prompt".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "tensorboard".to_string(),
            description: "TensorFlow visualization toolkit".to_string(),
            category: ToolCategory::MachineLearning,
            command: "tensorboard".to_string(),
            common_args: vec!["--logdir".to_string()],
            examples: vec![ToolExample {
                description: "Start TensorBoard".to_string(),
                command: "tensorboard --logdir=./logs".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "logdir".to_string(),
                description: "Log directory".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "wandb".to_string(),
            description: "Weights & Biases ML tracking".to_string(),
            category: ToolCategory::MachineLearning,
            command: "wandb".to_string(),
            common_args: vec!["login".to_string(), "sync".to_string()],
            examples: vec![ToolExample {
                description: "Login to W&B".to_string(),
                command: "wandb login".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "W&B command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "login".to_string(),
                    "sync".to_string(),
                    "artifact".to_string(),
                    "sweep".to_string(),
                ],
            }],
        });

        // Data science tools
        self.add_tool(OSTool {
            name: "duckdb".to_string(),
            description: "DuckDB analytical database".to_string(),
            category: ToolCategory::Database,
            command: "duckdb".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Query CSV".to_string(),
                command: "duckdb -c \"SELECT * FROM 'data.csv' LIMIT 10\"".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "database".to_string(),
                description: "Database file".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "clickhouse".to_string(),
            description: "ClickHouse column-oriented database".to_string(),
            category: ToolCategory::Database,
            command: "clickhouse-client".to_string(),
            common_args: vec!["--query".to_string()],
            examples: vec![ToolExample {
                description: "Run query".to_string(),
                command: "clickhouse-client --query 'SELECT 1'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "query".to_string(),
                description: "SQL query".to_string(),
                param_type: ParameterType::String,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "datasette".to_string(),
            description: "SQLite data exploration tool".to_string(),
            category: ToolCategory::Database,
            command: "datasette".to_string(),
            common_args: vec!["serve".to_string()],
            examples: vec![ToolExample {
                description: "Serve database".to_string(),
                command: "datasette serve data.db".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "database".to_string(),
                description: "SQLite database".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "litecli".to_string(),
            description: "SQLite CLI with autocomplete".to_string(),
            category: ToolCategory::Database,
            command: "litecli".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Open database".to_string(),
                command: "litecli data.db".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "database".to_string(),
                description: "SQLite database".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "pgcli".to_string(),
            description: "PostgreSQL CLI with autocomplete".to_string(),
            category: ToolCategory::Database,
            command: "pgcli".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Connect to database".to_string(),
                command: "pgcli postgres://user:pass@localhost/db".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "connection".to_string(),
                description: "Connection string".to_string(),
                param_type: ParameterType::String,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "mycli".to_string(),
            description: "MySQL CLI with autocomplete".to_string(),
            category: ToolCategory::Database,
            command: "mycli".to_string(),
            common_args: vec!["-u".to_string()],
            examples: vec![ToolExample {
                description: "Connect to MySQL".to_string(),
                command: "mycli -u root -h localhost".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "user".to_string(),
                description: "Username".to_string(),
                param_type: ParameterType::String,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        // Data processing
        self.add_tool(OSTool {
            name: "dasel".to_string(),
            description: "Query and modify JSON/YAML/TOML/CSV/XML".to_string(),
            category: ToolCategory::DataProcessing,
            command: "dasel".to_string(),
            common_args: vec!["-f".to_string()],
            examples: vec![ToolExample {
                description: "Query JSON".to_string(),
                command: "dasel -f config.json '.database.host'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "Input file".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "fx".to_string(),
            description: "Interactive JSON viewer".to_string(),
            category: ToolCategory::DataProcessing,
            command: "fx".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "View JSON".to_string(),
                command: "cat data.json | fx".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "gron".to_string(),
            description: "Make JSON greppable".to_string(),
            category: ToolCategory::DataProcessing,
            command: "gron".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Flatten JSON".to_string(),
                command: "gron data.json | grep 'name'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "JSON file".to_string(),
                param_type: ParameterType::Path,
                required: false,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "pup".to_string(),
            description: "HTML parsing tool".to_string(),
            category: ToolCategory::DataProcessing,
            command: "pup".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Extract links".to_string(),
                command: "curl -s example.com | pup 'a attr{href}'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "selector".to_string(),
                description: "CSS selector".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "htmlq".to_string(),
            description: "Like jq but for HTML".to_string(),
            category: ToolCategory::DataProcessing,
            command: "htmlq".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Extract text".to_string(),
                command: "curl -s example.com | htmlq 'title'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "selector".to_string(),
                description: "CSS selector".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });
    }

    /// Media and image processing tools
    fn populate_media_tools(&mut self) {
        self.add_tool(OSTool {
            name: "ffmpeg".to_string(),
            description: "Multimedia framework".to_string(),
            category: ToolCategory::Media,
            command: "ffmpeg".to_string(),
            common_args: vec!["-i".to_string()],
            examples: vec![
                ToolExample {
                    description: "Convert video".to_string(),
                    command: "ffmpeg -i input.mp4 output.webm".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Extract audio".to_string(),
                    command: "ffmpeg -i video.mp4 -vn audio.mp3".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "input".to_string(),
                    description: "Input file".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "output".to_string(),
                    description: "Output file".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "ffprobe".to_string(),
            description: "Multimedia stream analyzer".to_string(),
            category: ToolCategory::Media,
            command: "ffprobe".to_string(),
            common_args: vec![
                "-v".to_string(),
                "quiet".to_string(),
                "-print_format".to_string(),
                "json".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Get video info".to_string(),
                command: "ffprobe -v quiet -print_format json -show_format video.mp4".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "Media file".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "imagemagick".to_string(),
            description: "Image manipulation".to_string(),
            category: ToolCategory::Media,
            command: "convert".to_string(),
            common_args: vec![],
            examples: vec![
                ToolExample {
                    description: "Resize image".to_string(),
                    command: "convert input.jpg -resize 50% output.jpg".to_string(),
                    expected_output: None,
                },
                ToolExample {
                    description: "Convert format".to_string(),
                    command: "convert image.png image.jpg".to_string(),
                    expected_output: None,
                },
            ],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "input".to_string(),
                    description: "Input image".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "output".to_string(),
                    description: "Output image".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "exiftool".to_string(),
            description: "Read/write EXIF metadata".to_string(),
            category: ToolCategory::Media,
            command: "exiftool".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "View metadata".to_string(),
                command: "exiftool photo.jpg".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "Media file".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "yt-dlp".to_string(),
            description: "Video downloader".to_string(),
            category: ToolCategory::Media,
            command: "yt-dlp".to_string(),
            common_args: vec!["--format".to_string(), "best".to_string()],
            examples: vec![ToolExample {
                description: "Download video".to_string(),
                command: "yt-dlp 'https://youtube.com/watch?v=...'".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "url".to_string(),
                description: "Video URL".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "sox".to_string(),
            description: "Sound eXchange audio processor".to_string(),
            category: ToolCategory::Media,
            command: "sox".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Convert audio".to_string(),
                command: "sox input.wav output.mp3".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "input".to_string(),
                    description: "Input audio".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "output".to_string(),
                    description: "Output audio".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });

        self.add_tool(OSTool {
            name: "gifsicle".to_string(),
            description: "GIF manipulation tool".to_string(),
            category: ToolCategory::Media,
            command: "gifsicle".to_string(),
            common_args: vec!["--optimize".to_string()],
            examples: vec![ToolExample {
                description: "Optimize GIF".to_string(),
                command: "gifsicle --optimize=3 input.gif -o output.gif".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "file".to_string(),
                description: "GIF file".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "pdftk".to_string(),
            description: "PDF toolkit".to_string(),
            category: ToolCategory::Media,
            command: "pdftk".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Merge PDFs".to_string(),
                command: "pdftk file1.pdf file2.pdf cat output merged.pdf".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "input".to_string(),
                description: "Input PDF".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "pandoc".to_string(),
            description: "Universal document converter".to_string(),
            category: ToolCategory::Media,
            command: "pandoc".to_string(),
            common_args: vec!["-o".to_string()],
            examples: vec![ToolExample {
                description: "Convert Markdown to PDF".to_string(),
                command: "pandoc document.md -o document.pdf".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![
                ToolParameter {
                    name: "input".to_string(),
                    description: "Input file".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
                ToolParameter {
                    name: "output".to_string(),
                    description: "Output file".to_string(),
                    param_type: ParameterType::Path,
                    required: true,
                    default_value: None,
                    enum_values: vec![],
                },
            ],
        });
    }

    /// Additional web and API tools
    fn populate_api_tools(&mut self) {
        self.add_tool(OSTool {
            name: "grpcurl".to_string(),
            description: "gRPC command line tool".to_string(),
            category: ToolCategory::WebTools,
            command: "grpcurl".to_string(),
            common_args: vec!["-plaintext".to_string()],
            examples: vec![ToolExample {
                description: "List services".to_string(),
                command: "grpcurl -plaintext localhost:50051 list".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "address".to_string(),
                description: "Server address".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "websocat".to_string(),
            description: "WebSocket client".to_string(),
            category: ToolCategory::WebTools,
            command: "websocat".to_string(),
            common_args: vec![],
            examples: vec![ToolExample {
                description: "Connect to WebSocket".to_string(),
                command: "websocat ws://localhost:8080/ws".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "url".to_string(),
                description: "WebSocket URL".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "hey".to_string(),
            description: "HTTP load generator".to_string(),
            category: ToolCategory::WebTools,
            command: "hey".to_string(),
            common_args: vec![
                "-n".to_string(),
                "200".to_string(),
                "-c".to_string(),
                "50".to_string(),
            ],
            examples: vec![ToolExample {
                description: "Load test".to_string(),
                command: "hey -n 200 -c 50 http://localhost:8080".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "url".to_string(),
                description: "Target URL".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "wrk".to_string(),
            description: "HTTP benchmarking tool".to_string(),
            category: ToolCategory::WebTools,
            command: "wrk".to_string(),
            common_args: vec!["-t12".to_string(), "-c400".to_string(), "-d30s".to_string()],
            examples: vec![ToolExample {
                description: "Benchmark".to_string(),
                command: "wrk -t12 -c400 -d30s http://localhost:8080".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![OperatingSystem::Linux, OperatingSystem::MacOS],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "url".to_string(),
                description: "Target URL".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "vegeta".to_string(),
            description: "HTTP load testing tool".to_string(),
            category: ToolCategory::WebTools,
            command: "vegeta".to_string(),
            common_args: vec!["attack".to_string()],
            examples: vec![ToolExample {
                description: "Attack endpoint".to_string(),
                command: "echo 'GET http://localhost' | vegeta attack -duration=5s | vegeta report"
                    .to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![],
        });

        self.add_tool(OSTool {
            name: "postman".to_string(),
            description: "Postman CLI (newman)".to_string(),
            category: ToolCategory::WebTools,
            command: "newman".to_string(),
            common_args: vec!["run".to_string()],
            examples: vec![ToolExample {
                description: "Run collection".to_string(),
                command: "newman run collection.json".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "collection".to_string(),
                description: "Collection file".to_string(),
                param_type: ParameterType::Path,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "ngrok".to_string(),
            description: "Secure tunnel to localhost".to_string(),
            category: ToolCategory::WebTools,
            command: "ngrok".to_string(),
            common_args: vec!["http".to_string()],
            examples: vec![ToolExample {
                description: "Expose local server".to_string(),
                command: "ngrok http 8080".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "port".to_string(),
                description: "Local port".to_string(),
                param_type: ParameterType::Integer,
                required: true,
                default_value: None,
                enum_values: vec![],
            }],
        });

        self.add_tool(OSTool {
            name: "cloudflared".to_string(),
            description: "Cloudflare Tunnel client".to_string(),
            category: ToolCategory::WebTools,
            command: "cloudflared".to_string(),
            common_args: vec!["tunnel".to_string()],
            examples: vec![ToolExample {
                description: "Quick tunnel".to_string(),
                command: "cloudflared tunnel --url http://localhost:8080".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Caution,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "command".to_string(),
                description: "Cloudflared command".to_string(),
                param_type: ParameterType::String,
                required: true,
                default_value: None,
                enum_values: vec![
                    "tunnel".to_string(),
                    "access".to_string(),
                    "service".to_string(),
                ],
            }],
        });

        self.add_tool(OSTool {
            name: "mkcert".to_string(),
            description: "Local HTTPS certificates".to_string(),
            category: ToolCategory::Security,
            command: "mkcert".to_string(),
            common_args: vec!["-install".to_string()],
            examples: vec![ToolExample {
                description: "Create cert".to_string(),
                command: "mkcert localhost 127.0.0.1 ::1".to_string(),
                expected_output: None,
            }],
            safety_level: SafetyLevel::Safe,
            requires_admin: false,
            supported_os: vec![
                OperatingSystem::Linux,
                OperatingSystem::MacOS,
                OperatingSystem::Windows,
            ],
            cross_platform: None,
            parameters: vec![ToolParameter {
                name: "domains".to_string(),
                description: "Domain names".to_string(),
                param_type: ParameterType::String,
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
