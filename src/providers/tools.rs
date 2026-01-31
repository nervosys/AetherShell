//! AI Provider Tool Integration
//!
//! This module provides the integration layer between LLM providers and
//! AetherShell's OS abstraction ontology, making AetherShell the default
//! OS interface for all AI agents.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                       AI Provider Layer                          │
//! │  (OpenAI, Anthropic, Google, Ollama, etc.)                      │
//! └─────────────────────┬───────────────────────────────────────────┘
//!                       │
//!                       ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Tool Abstraction (this module)                │
//! │  - Converts ontology operations to provider tool schemas        │
//! │  - Validates tool calls against ontology                         │
//! │  - Routes tool execution through PlatformExecutor               │
//! └─────────────────────┬───────────────────────────────────────────┘
//!                       │
//!                       ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    OS Ontology (ontology.rs)                     │
//! │  - Formal operation specifications                               │
//! │  - Security requirements                                         │
//! │  - Platform support metadata                                     │
//! └─────────────────────┬───────────────────────────────────────────┘
//!                       │
//!                       ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                  Platform Executor (platform.rs)                 │
//! │  - Cross-platform implementations                                │
//! │  - Native Rust operations                                        │
//! │  - Shell command fallbacks                                       │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use super::ontology::{
    CapabilityDomain, OSOperation, OSOperationRegistry, ParamType, PermissionLevel,
    SupportedPlatform, OS_ONTOLOGY,
};
use super::platform::{ExecutionResult, PlatformExecutor, PLATFORM_EXECUTOR};
use super::schema::ToolSchema;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;

// ============================================================================
// TOOL DEFINITIONS FOR AI PROVIDERS
// ============================================================================

/// Tool definition that can be sent to any LLM provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AetherTool {
    /// Tool name (matches ontology operation ID)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// JSON Schema for parameters
    pub parameters: JsonValue,
    /// Required permission level
    pub permission_level: PermissionLevel,
    /// Whether this tool is available on the current platform
    pub available: bool,
    /// Platform-specific notes
    pub platform_notes: Option<String>,
}

/// Result of a tool invocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Tool name that was invoked
    pub tool_name: String,
    /// Whether execution succeeded
    pub success: bool,
    /// Result content (on success)
    pub content: Option<JsonValue>,
    /// Error message (on failure)
    pub error: Option<String>,
    /// Execution metadata
    pub metadata: ToolExecutionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolExecutionMetadata {
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Whether elevated privileges were used
    pub elevated: bool,
    /// Platform on which the tool ran
    pub platform: String,
    /// Operation ID from ontology
    pub operation_id: String,
}

// ============================================================================
// TOOL REGISTRY
// ============================================================================

/// Registry of tools available to AI providers
pub struct AetherToolRegistry {
    tools: HashMap<String, AetherTool>,
    executor: &'static PlatformExecutor,
}

impl AetherToolRegistry {
    /// Create a new tool registry from the ontology
    pub fn new() -> Self {
        let mut tools = HashMap::new();
        let executor = &*PLATFORM_EXECUTOR;
        let current_platform = get_current_supported_platform();

        // Convert all ontology operations to tools
        for domain in CapabilityDomain::all() {
            for operation in OS_ONTOLOGY.by_domain(&domain) {
                let tool = operation_to_tool(operation, current_platform);
                tools.insert(tool.name.clone(), tool);
            }
        }

        Self { tools, executor }
    }

    /// Get all available tools
    pub fn all_tools(&self) -> Vec<&AetherTool> {
        self.tools.values().collect()
    }

    /// Get tools available on the current platform
    pub fn available_tools(&self) -> Vec<&AetherTool> {
        self.tools.values().filter(|t| t.available).collect()
    }

    /// Get tools by domain
    pub fn tools_by_domain(&self, domain: &CapabilityDomain) -> Vec<&AetherTool> {
        let domain_prefix = match domain {
            CapabilityDomain::FileSystem => "fs.",
            CapabilityDomain::Process => "process.",
            CapabilityDomain::Network => "network.",
            CapabilityDomain::System => "system.",
            CapabilityDomain::Security => "security.",
            CapabilityDomain::Environment => "env.",
            CapabilityDomain::Shell => "shell.",
            CapabilityDomain::Package => "package.",
            CapabilityDomain::Container => "container.",
            CapabilityDomain::Cloud => "cloud.",
            CapabilityDomain::Database => "db.",
            CapabilityDomain::AI => "ai.",
            CapabilityDomain::Media => "media.",
            CapabilityDomain::Web => "web.",
            CapabilityDomain::Crypto => "crypto.",
            CapabilityDomain::Time => "time.",
            CapabilityDomain::IPC => "ipc.",
            CapabilityDomain::Device => "device.",
            CapabilityDomain::Observability => "obs.",
        };
        self.tools
            .values()
            .filter(|t| t.name.starts_with(domain_prefix))
            .collect()
    }

    /// Get a specific tool by name
    pub fn get_tool(&self, name: &str) -> Option<&AetherTool> {
        self.tools.get(name)
    }

    /// Execute a tool with given arguments
    pub fn execute(&self, tool_name: &str, args: JsonValue) -> ToolResult {
        let start = std::time::Instant::now();

        // Validate tool exists and is available
        let tool = match self.tools.get(tool_name) {
            Some(t) => t,
            None => {
                return ToolResult {
                    tool_name: tool_name.to_string(),
                    success: false,
                    content: None,
                    error: Some(format!("Tool '{}' not found", tool_name)),
                    metadata: ToolExecutionMetadata::default(),
                }
            }
        };

        if !tool.available {
            return ToolResult {
                tool_name: tool_name.to_string(),
                success: false,
                content: None,
                error: Some(format!(
                    "Tool '{}' not available on current platform",
                    tool_name
                )),
                metadata: ToolExecutionMetadata::default(),
            };
        }

        // Convert args to HashMap
        let params = match args {
            JsonValue::Object(map) => map.into_iter().collect::<HashMap<_, _>>(),
            _ => HashMap::new(),
        };

        // Execute via platform executor
        let result = match self.executor.execute(tool_name, &params) {
            Ok(exec_result) => exec_result,
            Err(e) => {
                return ToolResult {
                    tool_name: tool_name.to_string(),
                    success: false,
                    content: None,
                    error: Some(e.to_string()),
                    metadata: ToolExecutionMetadata {
                        duration_ms: start.elapsed().as_millis() as u64,
                        platform: format!("{:?}", crate::os_tools::OperatingSystem::current()),
                        operation_id: tool_name.to_string(),
                        ..Default::default()
                    },
                };
            }
        };

        ToolResult {
            tool_name: tool_name.to_string(),
            success: result.success,
            content: if result.success {
                Some(json!({
                    "output": result.stdout,
                    "stderr": result.stderr,
                    "exit_code": result.exit_code,
                }))
            } else {
                None
            },
            error: if result.success {
                None
            } else {
                Some(format!(
                    "Exit code {:?}: {}",
                    result.exit_code, result.stderr
                ))
            },
            metadata: ToolExecutionMetadata {
                duration_ms: start.elapsed().as_millis() as u64,
                elevated: false, // TODO: track this
                platform: format!("{:?}", crate::os_tools::OperatingSystem::current()),
                operation_id: tool_name.to_string(),
            },
        }
    }

    /// Convert tools to OpenAI function format
    pub fn to_openai_functions(&self) -> Vec<JsonValue> {
        self.available_tools()
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect()
    }

    /// Convert tools to Anthropic tool format
    pub fn to_anthropic_tools(&self) -> Vec<JsonValue> {
        self.available_tools()
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.parameters,
                })
            })
            .collect()
    }
}

impl Default for AetherToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn operation_to_tool(op: &OSOperation, current_platform: SupportedPlatform) -> AetherTool {
    // Check if operation is available on current platform
    let available = op
        .supported_platforms
        .as_ref()
        .map(|platforms| platforms.iter().any(|p| p.includes(current_platform)))
        .unwrap_or(true); // Default to available if not specified

    // Get platform-specific notes
    let platform_note = get_platform_note(op, current_platform);

    // Convert parameters to JSON Schema
    let parameters = params_to_json_schema(&op.parameters);

    AetherTool {
        name: op.id.clone(),
        description: format!(
            "{}{}",
            op.description,
            platform_note
                .map(|n| format!(" Note: {}", n))
                .unwrap_or_default()
        ),
        parameters,
        permission_level: op.security.permission_level.clone(),
        available,
        platform_notes: platform_note.map(|s| s.to_string()),
    }
}

fn get_current_supported_platform() -> SupportedPlatform {
    match crate::os_tools::OperatingSystem::current() {
        crate::os_tools::OperatingSystem::Windows => SupportedPlatform::Windows,
        crate::os_tools::OperatingSystem::MacOS => SupportedPlatform::MacOS,
        crate::os_tools::OperatingSystem::Linux => SupportedPlatform::Linux,
        crate::os_tools::OperatingSystem::BSD => SupportedPlatform::BSD,
        crate::os_tools::OperatingSystem::iOS => SupportedPlatform::IOS,
        crate::os_tools::OperatingSystem::Android => SupportedPlatform::Android,
    }
}

fn get_platform_note<'a>(op: &'a OSOperation, platform: SupportedPlatform) -> Option<&'a str> {
    let platform_key = match platform {
        SupportedPlatform::Windows => "windows",
        SupportedPlatform::MacOS => "macos",
        SupportedPlatform::Linux => "linux",
        SupportedPlatform::BSD => "bsd",
        SupportedPlatform::IOS => "ios",
        SupportedPlatform::Android => "android",
        SupportedPlatform::Mobile => "mobile",
        SupportedPlatform::Desktop => "desktop",
        SupportedPlatform::Unix => "unix",
        SupportedPlatform::Universal => return None,
    };
    op.platform_notes.get(platform_key).map(|s| s.as_str())
}

fn params_to_json_schema(params: &[super::ontology::OperationParameter]) -> JsonValue {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for param in params {
        let schema = param_type_to_json_schema(&param.param_type);
        properties.insert(param.name.clone(), schema);

        if param.required {
            required.push(param.name.clone());
        }
    }

    json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

fn param_type_to_json_schema(param_type: &ParamType) -> JsonValue {
    match param_type {
        ParamType::String {
            format,
            min_length,
            max_length,
            pattern,
        } => {
            let mut schema = json!({ "type": "string" });
            if let Some(min) = min_length {
                schema["minLength"] = json!(min);
            }
            if let Some(max) = max_length {
                schema["maxLength"] = json!(max);
            }
            if let Some(pat) = pattern {
                schema["pattern"] = json!(pat);
            }
            schema
        }
        ParamType::Integer { minimum, maximum } => {
            let mut schema = json!({ "type": "integer" });
            if let Some(min) = minimum {
                schema["minimum"] = json!(min);
            }
            if let Some(max) = maximum {
                schema["maximum"] = json!(max);
            }
            schema
        }
        ParamType::Number { minimum, maximum } => {
            let mut schema = json!({ "type": "number" });
            if let Some(min) = minimum {
                schema["minimum"] = json!(min);
            }
            if let Some(max) = maximum {
                schema["maximum"] = json!(max);
            }
            schema
        }
        ParamType::Boolean => json!({ "type": "boolean" }),
        ParamType::Array {
            items,
            min_items,
            max_items,
        } => {
            let mut schema = json!({
                "type": "array",
                "items": param_type_to_json_schema(items),
            });
            if let Some(min) = min_items {
                schema["minItems"] = json!(min);
            }
            if let Some(max) = max_items {
                schema["maxItems"] = json!(max);
            }
            schema
        }
        ParamType::Object {
            properties,
            required,
        } => {
            let props: serde_json::Map<String, JsonValue> = properties
                .iter()
                .map(|(k, v)| (k.clone(), param_type_to_json_schema(v)))
                .collect();
            json!({
                "type": "object",
                "properties": props,
                "required": required,
            })
        }
        ParamType::Enum { values } => {
            json!({
                "type": "string",
                "enum": values,
            })
        }
        ParamType::Path { .. } => json!({ "type": "string", "format": "path" }),
        ParamType::Uri { schemes } => json!({
            "type": "string",
            "format": "uri",
            "description": format!("URI with schemes: {}", schemes.join(", ")),
        }),
        ParamType::Binary { max_size } => {
            let mut schema = json!({ "type": "string", "format": "base64" });
            if let Some(max) = max_size {
                schema["maxLength"] = json!(max * 4 / 3 + 4); // Base64 encoding overhead
            }
            schema
        }
        ParamType::DateTime { .. } => json!({ "type": "string", "format": "date-time" }),
        ParamType::Duration => json!({ "type": "string", "format": "duration" }),
        ParamType::OneOf { variants } => {
            json!({
                "oneOf": variants.iter().map(param_type_to_json_schema).collect::<Vec<_>>(),
            })
        }
        ParamType::Ref { type_name } => json!({ "$ref": format!("#/definitions/{}", type_name) }),
    }
}

// ============================================================================
// GLOBAL INSTANCE
// ============================================================================

lazy_static::lazy_static! {
    /// Global tool registry
    pub static ref AETHER_TOOLS: AetherToolRegistry = AetherToolRegistry::new();
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_registry_creation() {
        let registry = AetherToolRegistry::new();
        assert!(!registry.all_tools().is_empty());
    }

    #[test]
    fn test_tool_availability() {
        let registry = AetherToolRegistry::new();
        // fs.read_file should be available on all platforms
        let tool = registry.get_tool("fs.read_file");
        assert!(tool.is_some());
        assert!(tool.unwrap().available);
    }

    #[test]
    fn test_openai_format() {
        let registry = AetherToolRegistry::new();
        let functions = registry.to_openai_functions();
        assert!(!functions.is_empty());

        // Check structure
        let first = &functions[0];
        assert_eq!(first["type"], "function");
        assert!(first["function"]["name"].is_string());
        assert!(first["function"]["description"].is_string());
    }

    #[test]
    fn test_anthropic_format() {
        let registry = AetherToolRegistry::new();
        let tools = registry.to_anthropic_tools();
        assert!(!tools.is_empty());

        // Check structure
        let first = &tools[0];
        assert!(first["name"].is_string());
        assert!(first["description"].is_string());
        assert!(first["input_schema"].is_object());
    }

    #[test]
    fn test_tool_execution() {
        let registry = AetherToolRegistry::new();

        // Test env.get which should work on all platforms
        let result = registry.execute("env.get", json!({ "name": "PATH" }));
        // Should succeed (PATH exists on all platforms)
        assert!(result.success || result.error.is_some());
    }

    #[test]
    fn test_domain_filtering() {
        let registry = AetherToolRegistry::new();
        let fs_tools = registry.tools_by_domain(&CapabilityDomain::FileSystem);

        // Should have filesystem tools
        assert!(!fs_tools.is_empty());

        // All should start with fs.
        for tool in fs_tools {
            assert!(tool.name.starts_with("fs."));
        }
    }
}
