//! OS Abstraction Ontology
//!
//! A formal specification of operating system capabilities that AI agents can use.
//! This ontology provides a standardized vocabulary for describing OS operations
//! across different platforms, enabling any AI model to interact with the system
//! in a consistent, secure, and predictable manner.
//!
//! ## Design Principles
//!
//! 1. **Platform Independence** - Abstract operations work across Linux, macOS, Windows, etc.
//! 2. **Security First** - All operations have explicit safety levels and permission requirements
//! 3. **Composability** - Operations can be chained and combined
//! 4. **Observability** - All operations produce structured outputs for AI understanding
//! 5. **Idempotency** - Where possible, operations can be safely retried

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;

// ============================================================================
// CAPABILITY DOMAINS
// ============================================================================

/// Top-level capability domains representing major OS subsystems
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityDomain {
    /// File system operations (read, write, delete, navigate)
    FileSystem,
    /// Process management (spawn, kill, monitor)
    Process,
    /// Network operations (connect, listen, transfer)
    Network,
    /// System information (hardware, software, resources)
    System,
    /// User and permissions management
    Security,
    /// Environment variables and configuration
    Environment,
    /// Shell and command execution
    Shell,
    /// Package and software management
    Package,
    /// Container and virtualization
    Container,
    /// Cloud service integrations
    Cloud,
    /// Database operations
    Database,
    /// AI and ML operations
    AI,
    /// Multimedia (images, audio, video)
    Media,
    /// Web and HTTP operations
    Web,
    /// Cryptographic operations
    Crypto,
    /// Time and scheduling
    Time,
    /// Inter-process communication
    IPC,
    /// Device and hardware access
    Device,
    /// Logging and observability
    Observability,
}

impl CapabilityDomain {
    /// Get all domains
    pub fn all() -> Vec<Self> {
        vec![
            Self::FileSystem,
            Self::Process,
            Self::Network,
            Self::System,
            Self::Security,
            Self::Environment,
            Self::Shell,
            Self::Package,
            Self::Container,
            Self::Cloud,
            Self::Database,
            Self::AI,
            Self::Media,
            Self::Web,
            Self::Crypto,
            Self::Time,
            Self::IPC,
            Self::Device,
            Self::Observability,
        ]
    }

    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::FileSystem => "File and directory operations",
            Self::Process => "Process lifecycle management",
            Self::Network => "Network connectivity and communication",
            Self::System => "System information and resources",
            Self::Security => "Security, permissions, and authentication",
            Self::Environment => "Environment variables and configuration",
            Self::Shell => "Shell command execution",
            Self::Package => "Package and dependency management",
            Self::Container => "Container and virtualization operations",
            Self::Cloud => "Cloud service integrations",
            Self::Database => "Database connections and queries",
            Self::AI => "AI model and inference operations",
            Self::Media => "Multimedia processing",
            Self::Web => "HTTP and web operations",
            Self::Crypto => "Cryptographic operations",
            Self::Time => "Time, dates, and scheduling",
            Self::IPC => "Inter-process communication",
            Self::Device => "Hardware and device access",
            Self::Observability => "Logging, metrics, and tracing",
        }
    }
}

// ============================================================================
// PLATFORM SUPPORT
// ============================================================================

/// Platform categories that operations can support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportedPlatform {
    /// All platforms supported
    Universal,
    /// Desktop operating systems (Windows, macOS, Linux)
    Desktop,
    /// Mobile operating systems (iOS, Android)
    Mobile,
    /// Unix-like systems (macOS, Linux, BSD)
    Unix,
    /// Microsoft Windows
    Windows,
    /// Apple macOS
    MacOS,
    /// Linux distributions
    Linux,
    /// BSD variants
    BSD,
    /// Apple iOS
    IOS,
    /// Google Android
    Android,
}

impl SupportedPlatform {
    /// Check if this platform category includes a specific platform
    pub fn includes(&self, platform: SupportedPlatform) -> bool {
        match self {
            Self::Universal => true,
            Self::Desktop => matches!(
                platform,
                Self::Windows | Self::MacOS | Self::Linux | Self::Desktop
            ),
            Self::Mobile => matches!(platform, Self::IOS | Self::Android | Self::Mobile),
            Self::Unix => matches!(platform, Self::MacOS | Self::Linux | Self::BSD | Self::Unix),
            _ => *self == platform,
        }
    }

    /// Expand meta-platforms to concrete platforms
    pub fn concrete_platforms(&self) -> Vec<Self> {
        match self {
            Self::Universal => vec![
                Self::Windows,
                Self::MacOS,
                Self::Linux,
                Self::BSD,
                Self::IOS,
                Self::Android,
            ],
            Self::Desktop => vec![Self::Windows, Self::MacOS, Self::Linux],
            Self::Mobile => vec![Self::IOS, Self::Android],
            Self::Unix => vec![Self::MacOS, Self::Linux, Self::BSD],
            _ => vec![*self],
        }
    }
}

// ============================================================================
// OPERATION DEFINITIONS
// ============================================================================

/// An operation that can be performed on the operating system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OSOperation {
    /// Unique identifier for the operation
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Detailed description for AI understanding
    pub description: String,
    /// The capability domain this operation belongs to
    pub domain: CapabilityDomain,
    /// Input parameters
    pub parameters: Vec<OperationParameter>,
    /// Return type specification
    pub returns: ReturnType,
    /// Security and safety requirements
    pub security: SecurityRequirements,
    /// Example invocations
    pub examples: Vec<OperationExample>,
    /// Related operations
    pub related: Vec<String>,
    /// Semantic tags for categorization
    pub tags: Vec<String>,
    /// Whether this operation modifies system state
    pub is_mutating: bool,
    /// Whether this operation is idempotent
    pub is_idempotent: bool,
    /// Estimated execution time category
    pub execution_time: ExecutionTime,
    /// Platforms that support this operation
    pub supported_platforms: Option<Vec<SupportedPlatform>>,
    /// Platform-specific notes (key: platform name, value: note)
    pub platform_notes: HashMap<String, String>,
}

/// Parameter definition with rich typing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationParameter {
    /// Parameter name
    pub name: String,
    /// Description for AI understanding
    pub description: String,
    /// Type specification
    pub param_type: ParamType,
    /// Whether this parameter is required
    pub required: bool,
    /// Default value if not provided
    pub default: Option<JsonValue>,
    /// Validation constraints
    pub constraints: Vec<Constraint>,
    /// Example values
    pub examples: Vec<JsonValue>,
}

/// Rich type system for parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParamType {
    /// String type with optional format
    String {
        format: Option<StringFormat>,
        min_length: Option<usize>,
        max_length: Option<usize>,
        pattern: Option<String>,
    },
    /// Integer type
    Integer {
        minimum: Option<i64>,
        maximum: Option<i64>,
    },
    /// Floating point number
    Number {
        minimum: Option<f64>,
        maximum: Option<f64>,
    },
    /// Boolean value
    Boolean,
    /// Array of items
    Array {
        items: Box<ParamType>,
        min_items: Option<usize>,
        max_items: Option<usize>,
    },
    /// Key-value object
    Object {
        properties: HashMap<String, ParamType>,
        required: Vec<String>,
    },
    /// Enumeration of allowed values
    Enum { values: Vec<String> },
    /// File system path
    Path {
        must_exist: bool,
        path_type: PathType,
    },
    /// URL/URI
    Uri { schemes: Vec<String> },
    /// Binary data (base64 encoded)
    Binary { max_size: Option<usize> },
    /// Date/time value
    DateTime { format: DateTimeFormat },
    /// Duration value
    Duration,
    /// Union of multiple types
    OneOf { variants: Vec<ParamType> },
    /// Reference to another type
    Ref { type_name: String },
}

/// String format specifiers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StringFormat {
    PlainText,
    Regex,
    Glob,
    Json,
    Yaml,
    Xml,
    Html,
    Markdown,
    Code { language: Option<String> },
    Email,
    Hostname,
    IpAddress,
    Uuid,
    Base64,
    Hex,
}

/// Path type specifier
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathType {
    Any,
    File,
    Directory,
    Symlink,
    Executable,
}

/// Date/time format specifier
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateTimeFormat {
    Iso8601,
    UnixTimestamp,
    Rfc2822,
    Custom { pattern: String },
}

/// Validation constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Constraint {
    /// Must match regex pattern
    Pattern { regex: String },
    /// Must be one of the allowed values
    AllowedValues { values: Vec<JsonValue> },
    /// Must not be one of the forbidden values
    ForbiddenValues { values: Vec<JsonValue> },
    /// Must satisfy a custom validation
    Custom { name: String, message: String },
    /// Path must be within allowed directories
    PathWithin { allowed: Vec<String> },
    /// Must not exceed rate limit
    RateLimit { requests_per_minute: u32 },
    /// Must not exceed size limit
    SizeLimit { max_bytes: u64 },
}

/// Return type specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnType {
    /// The type of the return value
    pub value_type: ParamType,
    /// Description of the return value
    pub description: String,
    /// Possible error types
    pub errors: Vec<ErrorType>,
}

/// Error type specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorType {
    /// Error code
    pub code: String,
    /// Error description
    pub description: String,
    /// Whether this error is recoverable
    pub recoverable: bool,
    /// Suggested remediation
    pub remediation: Option<String>,
}

/// Security requirements for an operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRequirements {
    /// Minimum permission level required
    pub permission_level: PermissionLevel,
    /// Required capabilities
    pub required_capabilities: Vec<String>,
    /// Whether this operation requires user confirmation
    pub requires_confirmation: bool,
    /// Whether this operation is audited
    pub audited: bool,
    /// Rate limiting configuration
    pub rate_limit: Option<RateLimit>,
    /// Sandbox restrictions
    pub sandbox: SandboxConfig,
}

/// Permission levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    /// No special permissions needed
    None,
    /// Read-only access to non-sensitive data
    ReadPublic,
    /// Read access to user data
    ReadUser,
    /// Write access to user data
    WriteUser,
    /// Read access to system data
    ReadSystem,
    /// Write access to system data
    WriteSystem,
    /// Administrative/root access
    Admin,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    /// Maximum requests per time window
    pub max_requests: u32,
    /// Time window in seconds
    pub window_seconds: u32,
    /// Whether to queue excess requests
    pub queue_excess: bool,
}

/// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Whether to run in sandboxed environment
    pub enabled: bool,
    /// Allowed directories
    pub allowed_paths: Vec<String>,
    /// Allowed network access
    pub network_access: NetworkAccess,
    /// Resource limits
    pub resource_limits: ResourceLimits,
}

/// Network access configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkAccess {
    None,
    LocalhostOnly,
    AllowList { hosts: Vec<String> },
    Full,
}

/// Resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_memory_mb: Option<u64>,
    pub max_cpu_seconds: Option<u64>,
    pub max_file_size_mb: Option<u64>,
    pub max_open_files: Option<u32>,
}

/// Operation example
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationExample {
    /// Description of what this example demonstrates
    pub description: String,
    /// Input parameters
    pub input: HashMap<String, JsonValue>,
    /// Expected output (optional)
    pub expected_output: Option<JsonValue>,
    /// Natural language description of the use case
    pub use_case: String,
}

/// Execution time category
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTime {
    /// Instant (< 10ms)
    Instant,
    /// Fast (10ms - 100ms)
    Fast,
    /// Normal (100ms - 1s)
    Normal,
    /// Slow (1s - 10s)
    Slow,
    /// Long running (> 10s)
    LongRunning,
    /// Variable/unpredictable
    Variable,
}

// ============================================================================
// ONTOLOGY REGISTRY
// ============================================================================

/// Registry of all OS operations
pub struct OSOperationRegistry {
    operations: HashMap<String, OSOperation>,
    by_domain: HashMap<CapabilityDomain, Vec<String>>,
    by_tag: HashMap<String, Vec<String>>,
}

impl Default for OSOperationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl OSOperationRegistry {
    /// Create a new registry with built-in operations
    pub fn new() -> Self {
        let mut registry = Self {
            operations: HashMap::new(),
            by_domain: HashMap::new(),
            by_tag: HashMap::new(),
        };

        // Register all built-in operations
        registry.register_filesystem_operations();
        registry.register_process_operations();
        registry.register_network_operations();
        registry.register_system_operations();
        registry.register_environment_operations();
        registry.register_shell_operations();
        registry.register_web_operations();
        registry.register_crypto_operations();
        registry.register_ai_operations();

        registry
    }

    /// Register an operation
    pub fn register(&mut self, op: OSOperation) {
        let id = op.id.clone();
        let domain = op.domain.clone();
        let tags = op.tags.clone();

        self.operations.insert(id.clone(), op);

        self.by_domain.entry(domain).or_default().push(id.clone());

        for tag in tags {
            self.by_tag.entry(tag).or_default().push(id.clone());
        }
    }

    /// Get an operation by ID
    pub fn get(&self, id: &str) -> Option<&OSOperation> {
        self.operations.get(id)
    }

    /// Get all operations in a domain
    pub fn by_domain(&self, domain: &CapabilityDomain) -> Vec<&OSOperation> {
        self.by_domain
            .get(domain)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.operations.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all operations with a tag
    pub fn by_tag(&self, tag: &str) -> Vec<&OSOperation> {
        self.by_tag
            .get(tag)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.operations.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Search operations by query
    pub fn search(&self, query: &str) -> Vec<&OSOperation> {
        let query_lower = query.to_lowercase();
        self.operations
            .values()
            .filter(|op| {
                op.name.to_lowercase().contains(&query_lower)
                    || op.description.to_lowercase().contains(&query_lower)
                    || op
                        .tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }

    /// Get all operations
    pub fn all(&self) -> Vec<&OSOperation> {
        self.operations.values().collect()
    }

    /// Export the ontology as JSON Schema
    pub fn to_json_schema(&self) -> JsonValue {
        let operations: Vec<JsonValue> = self
            .operations
            .values()
            .map(|op| {
                json!({
                    "id": op.id,
                    "name": op.name,
                    "description": op.description,
                    "domain": op.domain,
                    "parameters": op.parameters,
                    "returns": op.returns,
                    "security": op.security,
                    "is_mutating": op.is_mutating,
                    "is_idempotent": op.is_idempotent,
                })
            })
            .collect();

        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "AetherShell OS Ontology",
            "description": "Operating system abstraction layer for AI agents",
            "version": "1.0.0",
            "domains": CapabilityDomain::all(),
            "operations": operations
        })
    }

    // ========================================================================
    // BUILT-IN OPERATION REGISTRATIONS
    // ========================================================================

    fn register_filesystem_operations(&mut self) {
        // read_file
        self.register(OSOperation {
            id: "fs.read_file".to_string(),
            name: "Read File".to_string(),
            description: "Read the contents of a file. Supports text and binary modes.".to_string(),
            domain: CapabilityDomain::FileSystem,
            parameters: vec![
                OperationParameter {
                    name: "path".to_string(),
                    description: "Path to the file to read".to_string(),
                    param_type: ParamType::Path {
                        must_exist: true,
                        path_type: PathType::File,
                    },
                    required: true,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!("/home/user/document.txt"), json!("./config.json")],
                },
                OperationParameter {
                    name: "encoding".to_string(),
                    description: "Text encoding (utf-8, ascii, binary)".to_string(),
                    param_type: ParamType::Enum {
                        values: vec![
                            "utf-8".to_string(),
                            "ascii".to_string(),
                            "binary".to_string(),
                        ],
                    },
                    required: false,
                    default: Some(json!("utf-8")),
                    constraints: vec![],
                    examples: vec![],
                },
                OperationParameter {
                    name: "max_bytes".to_string(),
                    description: "Maximum bytes to read (for large files)".to_string(),
                    param_type: ParamType::Integer {
                        minimum: Some(0),
                        maximum: Some(100_000_000),
                    },
                    required: false,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!(1024), json!(1000000)],
                },
            ],
            returns: ReturnType {
                value_type: ParamType::Object {
                    properties: HashMap::from([
                        (
                            "content".to_string(),
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                        (
                            "size".to_string(),
                            ParamType::Integer {
                                minimum: Some(0),
                                maximum: None,
                            },
                        ),
                        (
                            "encoding".to_string(),
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                    ]),
                    required: vec!["content".to_string()],
                },
                description: "File content and metadata".to_string(),
                errors: vec![
                    ErrorType {
                        code: "FILE_NOT_FOUND".to_string(),
                        description: "The specified file does not exist".to_string(),
                        recoverable: false,
                        remediation: Some("Check the path and try again".to_string()),
                    },
                    ErrorType {
                        code: "PERMISSION_DENIED".to_string(),
                        description: "Insufficient permissions to read the file".to_string(),
                        recoverable: false,
                        remediation: Some(
                            "Check file permissions or request elevated access".to_string(),
                        ),
                    },
                ],
            },
            security: SecurityRequirements {
                permission_level: PermissionLevel::ReadUser,
                required_capabilities: vec!["fs:read".to_string()],
                requires_confirmation: false,
                audited: true,
                rate_limit: Some(RateLimit {
                    max_requests: 100,
                    window_seconds: 60,
                    queue_excess: true,
                }),
                sandbox: SandboxConfig {
                    enabled: true,
                    allowed_paths: vec!["$HOME".to_string(), "$WORKSPACE".to_string()],
                    network_access: NetworkAccess::None,
                    resource_limits: ResourceLimits {
                        max_memory_mb: Some(256),
                        max_cpu_seconds: Some(10),
                        max_file_size_mb: Some(100),
                        max_open_files: Some(10),
                    },
                },
            },
            examples: vec![OperationExample {
                description: "Read a configuration file".to_string(),
                input: HashMap::from([("path".to_string(), json!("./config.json"))]),
                expected_output: Some(json!({
                    "content": "{\"key\": \"value\"}",
                    "size": 18,
                    "encoding": "utf-8"
                })),
                use_case: "Loading application configuration".to_string(),
            }],
            related: vec!["fs.write_file".to_string(), "fs.list_dir".to_string()],
            tags: vec!["file".to_string(), "read".to_string(), "io".to_string()],
            is_mutating: false,
            is_idempotent: true,
            execution_time: ExecutionTime::Fast,
            supported_platforms: Some(vec![SupportedPlatform::Universal]),
            platform_notes: HashMap::new(),
        });

        // write_file
        self.register(OSOperation {
            id: "fs.write_file".to_string(),
            name: "Write File".to_string(),
            description: "Write content to a file. Creates the file if it doesn't exist."
                .to_string(),
            domain: CapabilityDomain::FileSystem,
            parameters: vec![
                OperationParameter {
                    name: "path".to_string(),
                    description: "Path to write to".to_string(),
                    param_type: ParamType::Path {
                        must_exist: false,
                        path_type: PathType::File,
                    },
                    required: true,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!("./output.txt")],
                },
                OperationParameter {
                    name: "content".to_string(),
                    description: "Content to write".to_string(),
                    param_type: ParamType::String {
                        format: None,
                        min_length: None,
                        max_length: None,
                        pattern: None,
                    },
                    required: true,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!("Hello, World!")],
                },
                OperationParameter {
                    name: "mode".to_string(),
                    description: "Write mode".to_string(),
                    param_type: ParamType::Enum {
                        values: vec![
                            "overwrite".to_string(),
                            "append".to_string(),
                            "create_new".to_string(),
                        ],
                    },
                    required: false,
                    default: Some(json!("overwrite")),
                    constraints: vec![],
                    examples: vec![],
                },
            ],
            returns: ReturnType {
                value_type: ParamType::Object {
                    properties: HashMap::from([
                        (
                            "bytes_written".to_string(),
                            ParamType::Integer {
                                minimum: Some(0),
                                maximum: None,
                            },
                        ),
                        (
                            "path".to_string(),
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                    ]),
                    required: vec!["bytes_written".to_string()],
                },
                description: "Write operation result".to_string(),
                errors: vec![ErrorType {
                    code: "PERMISSION_DENIED".to_string(),
                    description: "Cannot write to this location".to_string(),
                    recoverable: false,
                    remediation: None,
                }],
            },
            security: SecurityRequirements {
                permission_level: PermissionLevel::WriteUser,
                required_capabilities: vec!["fs:write".to_string()],
                requires_confirmation: true,
                audited: true,
                rate_limit: Some(RateLimit {
                    max_requests: 50,
                    window_seconds: 60,
                    queue_excess: false,
                }),
                sandbox: SandboxConfig {
                    enabled: true,
                    allowed_paths: vec!["$HOME".to_string(), "$WORKSPACE".to_string()],
                    network_access: NetworkAccess::None,
                    resource_limits: ResourceLimits {
                        max_memory_mb: Some(256),
                        max_cpu_seconds: Some(30),
                        max_file_size_mb: Some(100),
                        max_open_files: Some(10),
                    },
                },
            },
            examples: vec![],
            related: vec!["fs.read_file".to_string()],
            tags: vec!["file".to_string(), "write".to_string(), "io".to_string()],
            is_mutating: true,
            is_idempotent: false,
            execution_time: ExecutionTime::Fast,
            supported_platforms: Some(vec![SupportedPlatform::Universal]),
            platform_notes: HashMap::new(),
        });

        // list_dir
        self.register(OSOperation {
            id: "fs.list_dir".to_string(),
            name: "List Directory".to_string(),
            description: "List contents of a directory with optional filtering and recursion."
                .to_string(),
            domain: CapabilityDomain::FileSystem,
            parameters: vec![
                OperationParameter {
                    name: "path".to_string(),
                    description: "Directory path to list".to_string(),
                    param_type: ParamType::Path {
                        must_exist: true,
                        path_type: PathType::Directory,
                    },
                    required: false,
                    default: Some(json!(".")),
                    constraints: vec![],
                    examples: vec![json!("."), json!("/home/user")],
                },
                OperationParameter {
                    name: "recursive".to_string(),
                    description: "List contents recursively".to_string(),
                    param_type: ParamType::Boolean,
                    required: false,
                    default: Some(json!(false)),
                    constraints: vec![],
                    examples: vec![],
                },
                OperationParameter {
                    name: "pattern".to_string(),
                    description: "Glob pattern to filter results".to_string(),
                    param_type: ParamType::String {
                        format: Some(StringFormat::Glob),
                        min_length: None,
                        max_length: None,
                        pattern: None,
                    },
                    required: false,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!("*.txt"), json!("**/*.rs")],
                },
                OperationParameter {
                    name: "include_hidden".to_string(),
                    description: "Include hidden files (starting with .)".to_string(),
                    param_type: ParamType::Boolean,
                    required: false,
                    default: Some(json!(false)),
                    constraints: vec![],
                    examples: vec![],
                },
            ],
            returns: ReturnType {
                value_type: ParamType::Array {
                    items: Box::new(ParamType::Object {
                        properties: HashMap::from([
                            (
                                "name".to_string(),
                                ParamType::String {
                                    format: None,
                                    min_length: None,
                                    max_length: None,
                                    pattern: None,
                                },
                            ),
                            (
                                "path".to_string(),
                                ParamType::String {
                                    format: None,
                                    min_length: None,
                                    max_length: None,
                                    pattern: None,
                                },
                            ),
                            (
                                "type".to_string(),
                                ParamType::Enum {
                                    values: vec![
                                        "file".to_string(),
                                        "directory".to_string(),
                                        "symlink".to_string(),
                                    ],
                                },
                            ),
                            (
                                "size".to_string(),
                                ParamType::Integer {
                                    minimum: Some(0),
                                    maximum: None,
                                },
                            ),
                            (
                                "modified".to_string(),
                                ParamType::DateTime {
                                    format: DateTimeFormat::Iso8601,
                                },
                            ),
                        ]),
                        required: vec!["name".to_string(), "path".to_string(), "type".to_string()],
                    }),
                    min_items: None,
                    max_items: Some(10000),
                },
                description: "List of directory entries".to_string(),
                errors: vec![],
            },
            security: SecurityRequirements {
                permission_level: PermissionLevel::ReadUser,
                required_capabilities: vec!["fs:read".to_string()],
                requires_confirmation: false,
                audited: true,
                rate_limit: None,
                sandbox: SandboxConfig {
                    enabled: true,
                    allowed_paths: vec!["$HOME".to_string(), "$WORKSPACE".to_string()],
                    network_access: NetworkAccess::None,
                    resource_limits: ResourceLimits {
                        max_memory_mb: Some(128),
                        max_cpu_seconds: Some(30),
                        max_file_size_mb: None,
                        max_open_files: Some(100),
                    },
                },
            },
            examples: vec![],
            related: vec!["fs.read_file".to_string(), "fs.search".to_string()],
            tags: vec![
                "directory".to_string(),
                "list".to_string(),
                "browse".to_string(),
            ],
            is_mutating: false,
            is_idempotent: true,
            execution_time: ExecutionTime::Fast,
            supported_platforms: Some(vec![SupportedPlatform::Universal]),
            platform_notes: HashMap::new(),
        });
    }

    fn register_process_operations(&mut self) {
        // process.list
        self.register(OSOperation {
            id: "process.list".to_string(),
            name: "List Processes".to_string(),
            description: "List running processes on the system.".to_string(),
            domain: CapabilityDomain::Process,
            parameters: vec![OperationParameter {
                name: "filter".to_string(),
                description: "Filter by process name pattern".to_string(),
                param_type: ParamType::String {
                    format: Some(StringFormat::Glob),
                    min_length: None,
                    max_length: None,
                    pattern: None,
                },
                required: false,
                default: None,
                constraints: vec![],
                examples: vec![json!("python*"), json!("node")],
            }],
            returns: ReturnType {
                value_type: ParamType::Array {
                    items: Box::new(ParamType::Object {
                        properties: HashMap::from([
                            (
                                "pid".to_string(),
                                ParamType::Integer {
                                    minimum: Some(1),
                                    maximum: None,
                                },
                            ),
                            (
                                "name".to_string(),
                                ParamType::String {
                                    format: None,
                                    min_length: None,
                                    max_length: None,
                                    pattern: None,
                                },
                            ),
                            (
                                "cpu_percent".to_string(),
                                ParamType::Number {
                                    minimum: Some(0.0),
                                    maximum: Some(100.0),
                                },
                            ),
                            (
                                "memory_mb".to_string(),
                                ParamType::Number {
                                    minimum: Some(0.0),
                                    maximum: None,
                                },
                            ),
                        ]),
                        required: vec!["pid".to_string(), "name".to_string()],
                    }),
                    min_items: None,
                    max_items: None,
                },
                description: "List of running processes".to_string(),
                errors: vec![],
            },
            security: SecurityRequirements {
                permission_level: PermissionLevel::ReadSystem,
                required_capabilities: vec!["process:read".to_string()],
                requires_confirmation: false,
                audited: true,
                rate_limit: Some(RateLimit {
                    max_requests: 30,
                    window_seconds: 60,
                    queue_excess: false,
                }),
                sandbox: SandboxConfig {
                    enabled: false,
                    allowed_paths: vec![],
                    network_access: NetworkAccess::None,
                    resource_limits: ResourceLimits {
                        max_memory_mb: Some(64),
                        max_cpu_seconds: Some(5),
                        max_file_size_mb: None,
                        max_open_files: None,
                    },
                },
            },
            examples: vec![],
            related: vec!["process.kill".to_string()],
            tags: vec!["process".to_string(), "monitor".to_string()],
            is_mutating: false,
            is_idempotent: true,
            execution_time: ExecutionTime::Fast,
            supported_platforms: Some(vec![SupportedPlatform::Universal]),
            platform_notes: HashMap::new(),
        });

        // process.spawn
        self.register(OSOperation {
            id: "process.spawn".to_string(),
            name: "Spawn Process".to_string(),
            description: "Start a new process.".to_string(),
            domain: CapabilityDomain::Process,
            parameters: vec![
                OperationParameter {
                    name: "command".to_string(),
                    description: "Command to execute".to_string(),
                    param_type: ParamType::String {
                        format: None,
                        min_length: Some(1),
                        max_length: None,
                        pattern: None,
                    },
                    required: true,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!("python script.py"), json!("ls -la")],
                },
                OperationParameter {
                    name: "args".to_string(),
                    description: "Command arguments".to_string(),
                    param_type: ParamType::Array {
                        items: Box::new(ParamType::String {
                            format: None,
                            min_length: None,
                            max_length: None,
                            pattern: None,
                        }),
                        min_items: None,
                        max_items: Some(100),
                    },
                    required: false,
                    default: Some(json!([])),
                    constraints: vec![],
                    examples: vec![],
                },
                OperationParameter {
                    name: "cwd".to_string(),
                    description: "Working directory".to_string(),
                    param_type: ParamType::Path {
                        must_exist: true,
                        path_type: PathType::Directory,
                    },
                    required: false,
                    default: None,
                    constraints: vec![],
                    examples: vec![],
                },
                OperationParameter {
                    name: "env".to_string(),
                    description: "Environment variables".to_string(),
                    param_type: ParamType::Object {
                        properties: HashMap::new(),
                        required: vec![],
                    },
                    required: false,
                    default: None,
                    constraints: vec![],
                    examples: vec![],
                },
                OperationParameter {
                    name: "capture_output".to_string(),
                    description: "Capture stdout/stderr".to_string(),
                    param_type: ParamType::Boolean,
                    required: false,
                    default: Some(json!(true)),
                    constraints: vec![],
                    examples: vec![],
                },
                OperationParameter {
                    name: "timeout_seconds".to_string(),
                    description: "Maximum execution time".to_string(),
                    param_type: ParamType::Integer {
                        minimum: Some(1),
                        maximum: Some(3600),
                    },
                    required: false,
                    default: Some(json!(60)),
                    constraints: vec![],
                    examples: vec![],
                },
            ],
            returns: ReturnType {
                value_type: ParamType::Object {
                    properties: HashMap::from([
                        (
                            "exit_code".to_string(),
                            ParamType::Integer {
                                minimum: None,
                                maximum: None,
                            },
                        ),
                        (
                            "stdout".to_string(),
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                        (
                            "stderr".to_string(),
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                        (
                            "duration_ms".to_string(),
                            ParamType::Integer {
                                minimum: Some(0),
                                maximum: None,
                            },
                        ),
                    ]),
                    required: vec!["exit_code".to_string()],
                },
                description: "Process execution result".to_string(),
                errors: vec![
                    ErrorType {
                        code: "COMMAND_NOT_FOUND".to_string(),
                        description: "The command was not found".to_string(),
                        recoverable: false,
                        remediation: Some(
                            "Check command spelling or install the required software".to_string(),
                        ),
                    },
                    ErrorType {
                        code: "TIMEOUT".to_string(),
                        description: "Process exceeded timeout".to_string(),
                        recoverable: true,
                        remediation: Some("Increase timeout or optimize the command".to_string()),
                    },
                ],
            },
            security: SecurityRequirements {
                permission_level: PermissionLevel::WriteSystem,
                required_capabilities: vec!["process:spawn".to_string()],
                requires_confirmation: true,
                audited: true,
                rate_limit: Some(RateLimit {
                    max_requests: 10,
                    window_seconds: 60,
                    queue_excess: false,
                }),
                sandbox: SandboxConfig {
                    enabled: true,
                    allowed_paths: vec!["$WORKSPACE".to_string()],
                    network_access: NetworkAccess::LocalhostOnly,
                    resource_limits: ResourceLimits {
                        max_memory_mb: Some(512),
                        max_cpu_seconds: Some(60),
                        max_file_size_mb: Some(100),
                        max_open_files: Some(50),
                    },
                },
            },
            examples: vec![],
            related: vec!["process.list".to_string(), "process.kill".to_string()],
            tags: vec![
                "process".to_string(),
                "execute".to_string(),
                "shell".to_string(),
            ],
            is_mutating: true,
            is_idempotent: false,
            execution_time: ExecutionTime::Variable,
            supported_platforms: Some(vec![SupportedPlatform::Desktop]),
            platform_notes: {
                let mut notes = HashMap::new();
                notes.insert(
                    "ios".to_string(),
                    "Requires jailbreak for arbitrary process spawn".to_string(),
                );
                notes.insert(
                    "android".to_string(),
                    "Available via Termux or root".to_string(),
                );
                notes
            },
        });
    }

    fn register_network_operations(&mut self) {
        // network.http_request
        self.register(OSOperation {
            id: "network.http_request".to_string(),
            name: "HTTP Request".to_string(),
            description: "Make an HTTP request to a URL.".to_string(),
            domain: CapabilityDomain::Network,
            parameters: vec![
                OperationParameter {
                    name: "url".to_string(),
                    description: "Target URL".to_string(),
                    param_type: ParamType::Uri {
                        schemes: vec!["http".to_string(), "https".to_string()],
                    },
                    required: true,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!("https://api.example.com/data")],
                },
                OperationParameter {
                    name: "method".to_string(),
                    description: "HTTP method".to_string(),
                    param_type: ParamType::Enum {
                        values: vec![
                            "GET".to_string(),
                            "POST".to_string(),
                            "PUT".to_string(),
                            "DELETE".to_string(),
                            "PATCH".to_string(),
                            "HEAD".to_string(),
                        ],
                    },
                    required: false,
                    default: Some(json!("GET")),
                    constraints: vec![],
                    examples: vec![],
                },
                OperationParameter {
                    name: "headers".to_string(),
                    description: "HTTP headers".to_string(),
                    param_type: ParamType::Object {
                        properties: HashMap::new(),
                        required: vec![],
                    },
                    required: false,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!({"Content-Type": "application/json"})],
                },
                OperationParameter {
                    name: "body".to_string(),
                    description: "Request body".to_string(),
                    param_type: ParamType::OneOf {
                        variants: vec![
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                            ParamType::Object {
                                properties: HashMap::new(),
                                required: vec![],
                            },
                        ],
                    },
                    required: false,
                    default: None,
                    constraints: vec![],
                    examples: vec![],
                },
                OperationParameter {
                    name: "timeout_seconds".to_string(),
                    description: "Request timeout".to_string(),
                    param_type: ParamType::Integer {
                        minimum: Some(1),
                        maximum: Some(300),
                    },
                    required: false,
                    default: Some(json!(30)),
                    constraints: vec![],
                    examples: vec![],
                },
            ],
            returns: ReturnType {
                value_type: ParamType::Object {
                    properties: HashMap::from([
                        (
                            "status".to_string(),
                            ParamType::Integer {
                                minimum: Some(100),
                                maximum: Some(599),
                            },
                        ),
                        (
                            "headers".to_string(),
                            ParamType::Object {
                                properties: HashMap::new(),
                                required: vec![],
                            },
                        ),
                        (
                            "body".to_string(),
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                        (
                            "duration_ms".to_string(),
                            ParamType::Integer {
                                minimum: Some(0),
                                maximum: None,
                            },
                        ),
                    ]),
                    required: vec!["status".to_string()],
                },
                description: "HTTP response".to_string(),
                errors: vec![
                    ErrorType {
                        code: "CONNECTION_REFUSED".to_string(),
                        description: "Could not connect to server".to_string(),
                        recoverable: true,
                        remediation: Some("Check URL and network connectivity".to_string()),
                    },
                    ErrorType {
                        code: "TIMEOUT".to_string(),
                        description: "Request timed out".to_string(),
                        recoverable: true,
                        remediation: Some("Increase timeout or try again later".to_string()),
                    },
                ],
            },
            security: SecurityRequirements {
                permission_level: PermissionLevel::WriteUser,
                required_capabilities: vec!["network:http".to_string()],
                requires_confirmation: false,
                audited: true,
                rate_limit: Some(RateLimit {
                    max_requests: 60,
                    window_seconds: 60,
                    queue_excess: true,
                }),
                sandbox: SandboxConfig {
                    enabled: true,
                    allowed_paths: vec![],
                    network_access: NetworkAccess::AllowList {
                        hosts: vec!["*".to_string()], // Configured per-deployment
                    },
                    resource_limits: ResourceLimits {
                        max_memory_mb: Some(128),
                        max_cpu_seconds: Some(30),
                        max_file_size_mb: None,
                        max_open_files: None,
                    },
                },
            },
            examples: vec![],
            related: vec!["network.download".to_string()],
            tags: vec!["http".to_string(), "api".to_string(), "web".to_string()],
            is_mutating: false,
            is_idempotent: true,
            execution_time: ExecutionTime::Variable,
            supported_platforms: Some(vec![SupportedPlatform::Universal]),
            platform_notes: HashMap::new(),
        });
    }

    fn register_system_operations(&mut self) {
        // system.info
        self.register(OSOperation {
            id: "system.info".to_string(),
            name: "System Information".to_string(),
            description: "Get detailed system information including OS, hardware, and resources."
                .to_string(),
            domain: CapabilityDomain::System,
            parameters: vec![],
            returns: ReturnType {
                value_type: ParamType::Object {
                    properties: HashMap::from([
                        (
                            "os".to_string(),
                            ParamType::Object {
                                properties: HashMap::from([
                                    (
                                        "name".to_string(),
                                        ParamType::String {
                                            format: None,
                                            min_length: None,
                                            max_length: None,
                                            pattern: None,
                                        },
                                    ),
                                    (
                                        "version".to_string(),
                                        ParamType::String {
                                            format: None,
                                            min_length: None,
                                            max_length: None,
                                            pattern: None,
                                        },
                                    ),
                                    (
                                        "arch".to_string(),
                                        ParamType::String {
                                            format: None,
                                            min_length: None,
                                            max_length: None,
                                            pattern: None,
                                        },
                                    ),
                                ]),
                                required: vec!["name".to_string()],
                            },
                        ),
                        (
                            "cpu".to_string(),
                            ParamType::Object {
                                properties: HashMap::from([
                                    (
                                        "cores".to_string(),
                                        ParamType::Integer {
                                            minimum: Some(1),
                                            maximum: None,
                                        },
                                    ),
                                    (
                                        "model".to_string(),
                                        ParamType::String {
                                            format: None,
                                            min_length: None,
                                            max_length: None,
                                            pattern: None,
                                        },
                                    ),
                                ]),
                                required: vec![],
                            },
                        ),
                        (
                            "memory".to_string(),
                            ParamType::Object {
                                properties: HashMap::from([
                                    (
                                        "total_mb".to_string(),
                                        ParamType::Integer {
                                            minimum: Some(0),
                                            maximum: None,
                                        },
                                    ),
                                    (
                                        "available_mb".to_string(),
                                        ParamType::Integer {
                                            minimum: Some(0),
                                            maximum: None,
                                        },
                                    ),
                                ]),
                                required: vec![],
                            },
                        ),
                    ]),
                    required: vec!["os".to_string()],
                },
                description: "System information".to_string(),
                errors: vec![],
            },
            security: SecurityRequirements {
                permission_level: PermissionLevel::ReadPublic,
                required_capabilities: vec!["system:read".to_string()],
                requires_confirmation: false,
                audited: false,
                rate_limit: None,
                sandbox: SandboxConfig {
                    enabled: false,
                    allowed_paths: vec![],
                    network_access: NetworkAccess::None,
                    resource_limits: ResourceLimits {
                        max_memory_mb: Some(32),
                        max_cpu_seconds: Some(5),
                        max_file_size_mb: None,
                        max_open_files: None,
                    },
                },
            },
            examples: vec![],
            related: vec![],
            tags: vec!["system".to_string(), "info".to_string()],
            is_mutating: false,
            is_idempotent: true,
            execution_time: ExecutionTime::Instant,
            supported_platforms: Some(vec![SupportedPlatform::Universal]),
            platform_notes: HashMap::new(),
        });
    }

    fn register_environment_operations(&mut self) {
        // env.get
        self.register(OSOperation {
            id: "env.get".to_string(),
            name: "Get Environment Variable".to_string(),
            description: "Get the value of an environment variable.".to_string(),
            domain: CapabilityDomain::Environment,
            parameters: vec![OperationParameter {
                name: "name".to_string(),
                description: "Variable name".to_string(),
                param_type: ParamType::String {
                    format: None,
                    min_length: Some(1),
                    max_length: Some(256),
                    pattern: Some("^[A-Za-z_][A-Za-z0-9_]*$".to_string()),
                },
                required: true,
                default: None,
                constraints: vec![],
                examples: vec![json!("HOME"), json!("PATH")],
            }],
            returns: ReturnType {
                value_type: ParamType::OneOf {
                    variants: vec![
                        ParamType::String {
                            format: None,
                            min_length: None,
                            max_length: None,
                            pattern: None,
                        },
                        ParamType::Object {
                            properties: HashMap::new(),
                            required: vec![],
                        }, // null case
                    ],
                },
                description: "Variable value or null".to_string(),
                errors: vec![],
            },
            security: SecurityRequirements {
                permission_level: PermissionLevel::ReadPublic,
                required_capabilities: vec!["env:read".to_string()],
                requires_confirmation: false,
                audited: true,
                rate_limit: None,
                sandbox: SandboxConfig {
                    enabled: false,
                    allowed_paths: vec![],
                    network_access: NetworkAccess::None,
                    resource_limits: ResourceLimits {
                        max_memory_mb: None,
                        max_cpu_seconds: None,
                        max_file_size_mb: None,
                        max_open_files: None,
                    },
                },
            },
            examples: vec![],
            related: vec!["env.set".to_string(), "env.list".to_string()],
            tags: vec!["environment".to_string(), "config".to_string()],
            is_mutating: false,
            is_idempotent: true,
            execution_time: ExecutionTime::Instant,
            supported_platforms: Some(vec![SupportedPlatform::Universal]),
            platform_notes: HashMap::new(),
        });
    }

    fn register_shell_operations(&mut self) {
        // shell.execute
        self.register(OSOperation {
            id: "shell.execute".to_string(),
            name: "Execute Shell Command".to_string(),
            description: "Execute a shell command and return the output.".to_string(),
            domain: CapabilityDomain::Shell,
            parameters: vec![
                OperationParameter {
                    name: "command".to_string(),
                    description: "Shell command to execute".to_string(),
                    param_type: ParamType::String {
                        format: Some(StringFormat::Code {
                            language: Some("bash".to_string()),
                        }),
                        min_length: Some(1),
                        max_length: Some(10000),
                        pattern: None,
                    },
                    required: true,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!("ls -la"), json!("echo 'Hello'")],
                },
                OperationParameter {
                    name: "shell".to_string(),
                    description: "Shell to use".to_string(),
                    param_type: ParamType::Enum {
                        values: vec![
                            "bash".to_string(),
                            "sh".to_string(),
                            "zsh".to_string(),
                            "fish".to_string(),
                            "powershell".to_string(),
                            "cmd".to_string(),
                        ],
                    },
                    required: false,
                    default: None, // Auto-detect
                    constraints: vec![],
                    examples: vec![],
                },
            ],
            returns: ReturnType {
                value_type: ParamType::Object {
                    properties: HashMap::from([
                        (
                            "exit_code".to_string(),
                            ParamType::Integer {
                                minimum: None,
                                maximum: None,
                            },
                        ),
                        (
                            "stdout".to_string(),
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                        (
                            "stderr".to_string(),
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                    ]),
                    required: vec!["exit_code".to_string()],
                },
                description: "Command execution result".to_string(),
                errors: vec![],
            },
            security: SecurityRequirements {
                permission_level: PermissionLevel::WriteSystem,
                required_capabilities: vec!["shell:execute".to_string()],
                requires_confirmation: true,
                audited: true,
                rate_limit: Some(RateLimit {
                    max_requests: 20,
                    window_seconds: 60,
                    queue_excess: false,
                }),
                sandbox: SandboxConfig {
                    enabled: true,
                    allowed_paths: vec!["$WORKSPACE".to_string()],
                    network_access: NetworkAccess::LocalhostOnly,
                    resource_limits: ResourceLimits {
                        max_memory_mb: Some(1024),
                        max_cpu_seconds: Some(120),
                        max_file_size_mb: Some(100),
                        max_open_files: Some(100),
                    },
                },
            },
            examples: vec![],
            related: vec!["process.spawn".to_string()],
            tags: vec![
                "shell".to_string(),
                "command".to_string(),
                "execute".to_string(),
            ],
            is_mutating: true,
            is_idempotent: false,
            execution_time: ExecutionTime::Variable,
            supported_platforms: Some(vec![SupportedPlatform::Universal]),
            platform_notes: HashMap::new(),
        });
    }

    fn register_web_operations(&mut self) {
        // web.scrape
        self.register(OSOperation {
            id: "web.scrape".to_string(),
            name: "Web Scrape".to_string(),
            description: "Fetch and parse content from a web page.".to_string(),
            domain: CapabilityDomain::Web,
            parameters: vec![
                OperationParameter {
                    name: "url".to_string(),
                    description: "URL to scrape".to_string(),
                    param_type: ParamType::Uri {
                        schemes: vec!["http".to_string(), "https".to_string()],
                    },
                    required: true,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!("https://example.com")],
                },
                OperationParameter {
                    name: "selector".to_string(),
                    description: "CSS selector to extract".to_string(),
                    param_type: ParamType::String {
                        format: None,
                        min_length: None,
                        max_length: None,
                        pattern: None,
                    },
                    required: false,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!("article p"), json!("#main-content")],
                },
                OperationParameter {
                    name: "format".to_string(),
                    description: "Output format".to_string(),
                    param_type: ParamType::Enum {
                        values: vec![
                            "text".to_string(),
                            "html".to_string(),
                            "markdown".to_string(),
                        ],
                    },
                    required: false,
                    default: Some(json!("text")),
                    constraints: vec![],
                    examples: vec![],
                },
            ],
            returns: ReturnType {
                value_type: ParamType::Object {
                    properties: HashMap::from([
                        (
                            "content".to_string(),
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                        (
                            "title".to_string(),
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                        (
                            "links".to_string(),
                            ParamType::Array {
                                items: Box::new(ParamType::String {
                                    format: None,
                                    min_length: None,
                                    max_length: None,
                                    pattern: None,
                                }),
                                min_items: None,
                                max_items: None,
                            },
                        ),
                    ]),
                    required: vec!["content".to_string()],
                },
                description: "Scraped content".to_string(),
                errors: vec![],
            },
            security: SecurityRequirements {
                permission_level: PermissionLevel::WriteUser,
                required_capabilities: vec!["web:scrape".to_string()],
                requires_confirmation: false,
                audited: true,
                rate_limit: Some(RateLimit {
                    max_requests: 30,
                    window_seconds: 60,
                    queue_excess: true,
                }),
                sandbox: SandboxConfig {
                    enabled: true,
                    allowed_paths: vec![],
                    network_access: NetworkAccess::Full,
                    resource_limits: ResourceLimits {
                        max_memory_mb: Some(256),
                        max_cpu_seconds: Some(30),
                        max_file_size_mb: None,
                        max_open_files: None,
                    },
                },
            },
            examples: vec![],
            related: vec!["network.http_request".to_string()],
            tags: vec!["web".to_string(), "scrape".to_string(), "html".to_string()],
            is_mutating: false,
            is_idempotent: true,
            execution_time: ExecutionTime::Normal,
            supported_platforms: Some(vec![SupportedPlatform::Universal]),
            platform_notes: HashMap::new(),
        });
    }

    fn register_crypto_operations(&mut self) {
        // crypto.hash
        self.register(OSOperation {
            id: "crypto.hash".to_string(),
            name: "Compute Hash".to_string(),
            description: "Compute a cryptographic hash of data.".to_string(),
            domain: CapabilityDomain::Crypto,
            parameters: vec![
                OperationParameter {
                    name: "data".to_string(),
                    description: "Data to hash".to_string(),
                    param_type: ParamType::String {
                        format: None,
                        min_length: None,
                        max_length: None,
                        pattern: None,
                    },
                    required: true,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!("Hello, World!")],
                },
                OperationParameter {
                    name: "algorithm".to_string(),
                    description: "Hash algorithm".to_string(),
                    param_type: ParamType::Enum {
                        values: vec![
                            "sha256".to_string(),
                            "sha384".to_string(),
                            "sha512".to_string(),
                            "sha3-256".to_string(),
                            "blake3".to_string(),
                            "md5".to_string(),
                        ],
                    },
                    required: false,
                    default: Some(json!("sha256")),
                    constraints: vec![],
                    examples: vec![],
                },
            ],
            returns: ReturnType {
                value_type: ParamType::Object {
                    properties: HashMap::from([
                        (
                            "hash".to_string(),
                            ParamType::String {
                                format: Some(StringFormat::Hex),
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                        (
                            "algorithm".to_string(),
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                    ]),
                    required: vec!["hash".to_string()],
                },
                description: "Hash result".to_string(),
                errors: vec![],
            },
            security: SecurityRequirements {
                permission_level: PermissionLevel::None,
                required_capabilities: vec![],
                requires_confirmation: false,
                audited: false,
                rate_limit: None,
                sandbox: SandboxConfig {
                    enabled: false,
                    allowed_paths: vec![],
                    network_access: NetworkAccess::None,
                    resource_limits: ResourceLimits {
                        max_memory_mb: Some(128),
                        max_cpu_seconds: Some(10),
                        max_file_size_mb: None,
                        max_open_files: None,
                    },
                },
            },
            examples: vec![],
            related: vec![],
            tags: vec!["crypto".to_string(), "hash".to_string()],
            is_mutating: false,
            is_idempotent: true,
            execution_time: ExecutionTime::Instant,
            supported_platforms: Some(vec![SupportedPlatform::Universal]),
            platform_notes: HashMap::new(),
        });
    }

    fn register_ai_operations(&mut self) {
        // ai.complete
        self.register(OSOperation {
            id: "ai.complete".to_string(),
            name: "AI Completion".to_string(),
            description: "Generate text completion using an AI model.".to_string(),
            domain: CapabilityDomain::AI,
            parameters: vec![
                OperationParameter {
                    name: "prompt".to_string(),
                    description: "The prompt to complete".to_string(),
                    param_type: ParamType::String {
                        format: None,
                        min_length: Some(1),
                        max_length: Some(128000),
                        pattern: None,
                    },
                    required: true,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!("Explain quantum computing in simple terms.")],
                },
                OperationParameter {
                    name: "model".to_string(),
                    description: "Model URI (e.g., openai:gpt-4o)".to_string(),
                    param_type: ParamType::String {
                        format: None,
                        min_length: None,
                        max_length: None,
                        pattern: Some("^[a-z]+:.+$".to_string()),
                    },
                    required: false,
                    default: None, // Uses default from config
                    constraints: vec![],
                    examples: vec![
                        json!("openai:gpt-4o"),
                        json!("anthropic:claude-3-5-sonnet"),
                        json!("ollama:llama3"),
                    ],
                },
                OperationParameter {
                    name: "temperature".to_string(),
                    description: "Sampling temperature (0-2)".to_string(),
                    param_type: ParamType::Number {
                        minimum: Some(0.0),
                        maximum: Some(2.0),
                    },
                    required: false,
                    default: Some(json!(0.7)),
                    constraints: vec![],
                    examples: vec![],
                },
                OperationParameter {
                    name: "max_tokens".to_string(),
                    description: "Maximum tokens to generate".to_string(),
                    param_type: ParamType::Integer {
                        minimum: Some(1),
                        maximum: Some(128000),
                    },
                    required: false,
                    default: Some(json!(4096)),
                    constraints: vec![],
                    examples: vec![],
                },
                OperationParameter {
                    name: "system".to_string(),
                    description: "System prompt".to_string(),
                    param_type: ParamType::String {
                        format: None,
                        min_length: None,
                        max_length: Some(32000),
                        pattern: None,
                    },
                    required: false,
                    default: None,
                    constraints: vec![],
                    examples: vec![json!("You are a helpful assistant.")],
                },
            ],
            returns: ReturnType {
                value_type: ParamType::Object {
                    properties: HashMap::from([
                        (
                            "text".to_string(),
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                        (
                            "model".to_string(),
                            ParamType::String {
                                format: None,
                                min_length: None,
                                max_length: None,
                                pattern: None,
                            },
                        ),
                        (
                            "usage".to_string(),
                            ParamType::Object {
                                properties: HashMap::from([
                                    (
                                        "prompt_tokens".to_string(),
                                        ParamType::Integer {
                                            minimum: Some(0),
                                            maximum: None,
                                        },
                                    ),
                                    (
                                        "completion_tokens".to_string(),
                                        ParamType::Integer {
                                            minimum: Some(0),
                                            maximum: None,
                                        },
                                    ),
                                ]),
                                required: vec![],
                            },
                        ),
                    ]),
                    required: vec!["text".to_string()],
                },
                description: "AI completion result".to_string(),
                errors: vec![
                    ErrorType {
                        code: "RATE_LIMITED".to_string(),
                        description: "API rate limit exceeded".to_string(),
                        recoverable: true,
                        remediation: Some("Wait and retry".to_string()),
                    },
                    ErrorType {
                        code: "INVALID_MODEL".to_string(),
                        description: "Model not found or not accessible".to_string(),
                        recoverable: false,
                        remediation: Some("Check model URI and API key".to_string()),
                    },
                ],
            },
            security: SecurityRequirements {
                permission_level: PermissionLevel::WriteUser,
                required_capabilities: vec!["ai:complete".to_string()],
                requires_confirmation: false,
                audited: true,
                rate_limit: Some(RateLimit {
                    max_requests: 60,
                    window_seconds: 60,
                    queue_excess: true,
                }),
                sandbox: SandboxConfig {
                    enabled: false,
                    allowed_paths: vec![],
                    network_access: NetworkAccess::Full,
                    resource_limits: ResourceLimits {
                        max_memory_mb: None,
                        max_cpu_seconds: None,
                        max_file_size_mb: None,
                        max_open_files: None,
                    },
                },
            },
            examples: vec![],
            related: vec!["ai.chat".to_string(), "ai.embed".to_string()],
            tags: vec![
                "ai".to_string(),
                "llm".to_string(),
                "completion".to_string(),
            ],
            is_mutating: false,
            is_idempotent: false, // Non-deterministic
            execution_time: ExecutionTime::Slow,
            supported_platforms: Some(vec![SupportedPlatform::Universal]),
            platform_notes: HashMap::new(),
        });
    }
}

/// Global ontology registry instance
lazy_static::lazy_static! {
    pub static ref OS_ONTOLOGY: OSOperationRegistry = OSOperationRegistry::new();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ontology_registration() {
        let registry = OSOperationRegistry::new();
        assert!(registry.get("fs.read_file").is_some());
        assert!(registry.get("fs.write_file").is_some());
        assert!(registry.get("process.list").is_some());
        assert!(registry.get("network.http_request").is_some());
    }

    #[test]
    fn test_domain_query() {
        let registry = OSOperationRegistry::new();
        let fs_ops = registry.by_domain(&CapabilityDomain::FileSystem);
        assert!(!fs_ops.is_empty());
    }

    #[test]
    fn test_search() {
        let registry = OSOperationRegistry::new();
        let results = registry.search("file");
        assert!(!results.is_empty());
    }
    #[test]
    fn test_platform_support() {
        let registry = OSOperationRegistry::new();

        // Check that fs.read_file has platform support
        let fs_read = registry.get("fs.read_file").unwrap();
        assert!(fs_read.supported_platforms.is_some());
        let platforms = fs_read.supported_platforms.as_ref().unwrap();
        assert!(platforms.contains(&SupportedPlatform::Universal));

        // Check process.spawn has mobile limitations
        let proc_spawn = registry.get("process.spawn").unwrap();
        assert!(proc_spawn.platform_notes.contains_key("ios"));
        assert!(proc_spawn.platform_notes.contains_key("android"));
    }

    #[test]
    fn test_platform_includes() {
        // Universal includes all platforms
        assert!(SupportedPlatform::Universal.includes(SupportedPlatform::Windows));
        assert!(SupportedPlatform::Universal.includes(SupportedPlatform::MacOS));
        assert!(SupportedPlatform::Universal.includes(SupportedPlatform::Linux));
        assert!(SupportedPlatform::Universal.includes(SupportedPlatform::IOS));
        assert!(SupportedPlatform::Universal.includes(SupportedPlatform::Android));

        // Desktop includes Windows, MacOS, Linux
        assert!(SupportedPlatform::Desktop.includes(SupportedPlatform::Windows));
        assert!(SupportedPlatform::Desktop.includes(SupportedPlatform::MacOS));
        assert!(SupportedPlatform::Desktop.includes(SupportedPlatform::Linux));
        assert!(!SupportedPlatform::Desktop.includes(SupportedPlatform::IOS));
        assert!(!SupportedPlatform::Desktop.includes(SupportedPlatform::Android));

        // Mobile includes iOS, Android
        assert!(SupportedPlatform::Mobile.includes(SupportedPlatform::IOS));
        assert!(SupportedPlatform::Mobile.includes(SupportedPlatform::Android));
        assert!(!SupportedPlatform::Mobile.includes(SupportedPlatform::Windows));

        // Unix includes MacOS, Linux, BSD but not Windows
        assert!(SupportedPlatform::Unix.includes(SupportedPlatform::MacOS));
        assert!(SupportedPlatform::Unix.includes(SupportedPlatform::Linux));
        assert!(SupportedPlatform::Unix.includes(SupportedPlatform::BSD));
        assert!(!SupportedPlatform::Unix.includes(SupportedPlatform::Windows));
    }

    #[test]
    fn test_concrete_platforms() {
        // Universal expands to all platforms
        let universal = SupportedPlatform::Universal.concrete_platforms();
        assert!(universal.contains(&SupportedPlatform::Windows));
        assert!(universal.contains(&SupportedPlatform::MacOS));
        assert!(universal.contains(&SupportedPlatform::Linux));
        assert!(universal.contains(&SupportedPlatform::BSD));
        assert!(universal.contains(&SupportedPlatform::IOS));
        assert!(universal.contains(&SupportedPlatform::Android));

        // Desktop expands to Windows, MacOS, Linux
        let desktop = SupportedPlatform::Desktop.concrete_platforms();
        assert_eq!(desktop.len(), 3);

        // Mobile expands to iOS, Android
        let mobile = SupportedPlatform::Mobile.concrete_platforms();
        assert_eq!(mobile.len(), 2);

        // Concrete platform returns itself
        let windows = SupportedPlatform::Windows.concrete_platforms();
        assert_eq!(windows.len(), 1);
        assert!(windows.contains(&SupportedPlatform::Windows));
    }
}
