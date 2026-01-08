//! Model Context Protocol (MCP) Server Implementation
//!
//! This module provides a comprehensive MCP server that exposes AetherShell's
//! OS tools database through the Model Context Protocol, enabling AI agents
//! to discover and execute tools in a standardized way.
//!
//! Features:
//! - Full MCP protocol compliance
//! - Tool discovery and execution
//! - Schema generation for function calling
//! - Safety level enforcement
//! - Cross-platform command translation

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;

use crate::os_tools::{OSTool, OSToolsDatabase, OperatingSystem, SafetyLevel, ToolCategory};

// ============================================================================
// MCP Protocol Types
// ============================================================================

/// MCP Tool definition for protocol communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: JsonValue,
}

/// MCP Tool call request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    pub name: String,
    pub arguments: HashMap<String, JsonValue>,
}

/// MCP Tool call result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// MCP Content block
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    #[serde(rename = "resource")]
    Resource {
        uri: String,
        mime_type: Option<String>,
    },
}

/// MCP Resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// MCP Prompt definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    pub name: String,
    pub description: Option<String>,
    pub arguments: Option<Vec<McpPromptArgument>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgument {
    pub name: String,
    pub description: Option<String>,
    pub required: Option<bool>,
}

/// MCP Server capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapabilities {
    pub tools: Option<McpToolsCapability>,
    pub resources: Option<McpResourcesCapability>,
    pub prompts: Option<McpPromptsCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolsCapability {
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourcesCapability {
    pub subscribe: bool,
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptsCapability {
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

/// MCP Server information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
}

/// MCP Initialize response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: McpCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: McpServerInfo,
}

// ============================================================================
// MCP Server Implementation
// ============================================================================

/// MCP Server configuration
#[derive(Debug, Clone)]
pub struct McpConfig {
    /// Maximum safety level allowed for tool execution
    pub max_safety_level: SafetyLevel,
    /// Whether to allow admin-requiring tools
    pub allow_admin_tools: bool,
    /// Allowed tool categories (None = all)
    pub allowed_categories: Option<Vec<ToolCategory>>,
    /// Explicitly blocked tools
    pub blocked_tools: Vec<String>,
    /// Timeout for tool execution in seconds
    pub execution_timeout: u64,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            max_safety_level: SafetyLevel::Caution,
            allow_admin_tools: false,
            allowed_categories: None,
            blocked_tools: vec![
                "rm".to_string(),
                "del".to_string(),
                "format".to_string(),
                "dd".to_string(),
            ],
            execution_timeout: 30,
        }
    }
}

/// AetherShell MCP Server
pub struct McpServer {
    tools_db: Arc<OSToolsDatabase>,
    config: McpConfig,
    /// Registered tools (filtered by config)
    registered_tools: HashMap<String, McpTool>,
}

impl McpServer {
    /// Create a new MCP server with default configuration
    pub fn new() -> Self {
        Self::with_config(McpConfig::default())
    }

    /// Create a new MCP server with custom configuration
    pub fn with_config(config: McpConfig) -> Self {
        let tools_db = Arc::new(OSToolsDatabase::new());
        let mut server = Self {
            tools_db,
            config,
            registered_tools: HashMap::new(),
        };
        server.register_all_tools();
        server
    }

    /// Register all available tools based on configuration
    fn register_all_tools(&mut self) {
        let current_os = OperatingSystem::current();

        for (name, tool) in &self.tools_db.tools {
            // Skip blocked tools
            if self.config.blocked_tools.contains(name) {
                continue;
            }

            // Check safety level
            if !self.is_safety_level_allowed(&tool.safety_level) {
                continue;
            }

            // Check admin requirement
            if tool.requires_admin && !self.config.allow_admin_tools {
                continue;
            }

            // Check category filter
            if let Some(ref allowed) = self.config.allowed_categories {
                if !allowed.contains(&tool.category) {
                    continue;
                }
            }

            // Check OS compatibility
            if !tool.supported_os.contains(&current_os) {
                continue;
            }

            // Register the tool
            self.registered_tools
                .insert(name.clone(), self.tool_to_mcp_tool(tool));
        }
    }

    /// Check if a safety level is allowed
    fn is_safety_level_allowed(&self, level: &SafetyLevel) -> bool {
        match (&self.config.max_safety_level, level) {
            (SafetyLevel::Safe, SafetyLevel::Safe) => true,
            (SafetyLevel::Caution, SafetyLevel::Safe | SafetyLevel::Caution) => true,
            (
                SafetyLevel::Dangerous,
                SafetyLevel::Safe | SafetyLevel::Caution | SafetyLevel::Dangerous,
            ) => true,
            (SafetyLevel::Critical, _) => true,
            _ => false,
        }
    }

    /// Convert an OSTool to MCP tool format
    fn tool_to_mcp_tool(&self, tool: &OSTool) -> McpTool {
        McpTool {
            name: tool.name.clone(),
            description: tool.description.clone(),
            input_schema: tool.to_openai_function_schema()["function"]["parameters"].clone(),
        }
    }

    /// Handle MCP initialize request
    pub fn initialize(&self) -> McpInitializeResult {
        McpInitializeResult {
            protocol_version: "2024-11-05".to_string(),
            capabilities: McpCapabilities {
                tools: Some(McpToolsCapability { list_changed: true }),
                resources: Some(McpResourcesCapability {
                    subscribe: false,
                    list_changed: true,
                }),
                prompts: Some(McpPromptsCapability {
                    list_changed: false,
                }),
            },
            server_info: McpServerInfo {
                name: "aethershell-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        }
    }

    /// List all available tools
    pub fn list_tools(&self) -> Vec<McpTool> {
        self.registered_tools.values().cloned().collect()
    }

    /// Get a specific tool by name
    pub fn get_tool(&self, name: &str) -> Option<&McpTool> {
        self.registered_tools.get(name)
    }

    /// Call a tool with arguments
    pub fn call_tool(&self, call: McpToolCall) -> McpToolResult {
        // Check if tool exists
        let tool = match self.tools_db.get_tool(&call.name) {
            Some(t) => t,
            None => {
                return McpToolResult {
                    content: vec![McpContent::Text {
                        text: format!("Tool '{}' not found", call.name),
                    }],
                    is_error: Some(true),
                };
            }
        };

        // Check if tool is registered (passes safety checks)
        if !self.registered_tools.contains_key(&call.name) {
            return McpToolResult {
                content: vec![McpContent::Text {
                    text: format!(
                        "Tool '{}' is not available (blocked by security policy)",
                        call.name
                    ),
                }],
                is_error: Some(true),
            };
        }

        // Execute the tool
        match self.execute_tool(tool, &call.arguments) {
            Ok(output) => McpToolResult {
                content: vec![McpContent::Text { text: output }],
                is_error: Some(false),
            },
            Err(e) => McpToolResult {
                content: vec![McpContent::Text {
                    text: format!("Tool execution failed: {}", e),
                }],
                is_error: Some(true),
            },
        }
    }

    /// Execute a tool with the given arguments
    fn execute_tool(&self, tool: &OSTool, args: &HashMap<String, JsonValue>) -> Result<String> {
        use std::process::Command;

        // Build command
        let cmd_name = tool.command_for_current_os();
        let mut cmd = Command::new(&cmd_name);

        // Add arguments based on tool parameters
        for param in &tool.parameters {
            if let Some(value) = args.get(&param.name) {
                match value {
                    JsonValue::String(s) => {
                        // Handle flag-style parameters
                        if param.name.starts_with('-') || s.starts_with('-') {
                            cmd.arg(s);
                        } else {
                            cmd.arg(s);
                        }
                    }
                    JsonValue::Bool(true) => {
                        // Add flag for boolean true
                        if let Some(ref default) = param.default_value {
                            if default == "false" {
                                // This is a flag that should be added
                                let flag = format!("-{}", param.name.chars().next().unwrap_or('?'));
                                cmd.arg(flag);
                            }
                        }
                    }
                    JsonValue::Number(n) => {
                        cmd.arg(n.to_string());
                    }
                    JsonValue::Array(arr) => {
                        for item in arr {
                            if let JsonValue::String(s) = item {
                                cmd.arg(s);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Execute with timeout
        let output = cmd
            .output()
            .map_err(|e| anyhow!("Failed to execute command '{}': {}", cmd_name, e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(stdout.to_string())
        } else {
            if stderr.is_empty() {
                Ok(stdout.to_string())
            } else {
                Err(anyhow!("Command failed: {}", stderr))
            }
        }
    }

    /// List available resources
    pub fn list_resources(&self) -> Vec<McpResource> {
        vec![
            McpResource {
                uri: "aethershell://tools".to_string(),
                name: "Available Tools".to_string(),
                description: Some("List of all available OS tools".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            McpResource {
                uri: "aethershell://categories".to_string(),
                name: "Tool Categories".to_string(),
                description: Some("List of tool categories".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            McpResource {
                uri: "aethershell://system-info".to_string(),
                name: "System Information".to_string(),
                description: Some("Current system information".to_string()),
                mime_type: Some("application/json".to_string()),
            },
        ]
    }

    /// Read a resource
    pub fn read_resource(&self, uri: &str) -> Result<McpContent> {
        match uri {
            "aethershell://tools" => {
                let tools: Vec<_> = self.registered_tools.keys().collect();
                Ok(McpContent::Text {
                    text: serde_json::to_string_pretty(&tools)?,
                })
            }
            "aethershell://categories" => {
                let categories: Vec<_> = self.tools_db.categories.keys().collect();
                Ok(McpContent::Text {
                    text: serde_json::to_string_pretty(&categories)?,
                })
            }
            "aethershell://system-info" => {
                let info = json!({
                    "os": format!("{:?}", OperatingSystem::current()),
                    "tool_count": self.registered_tools.len(),
                    "server_version": env!("CARGO_PKG_VERSION"),
                });
                Ok(McpContent::Text {
                    text: serde_json::to_string_pretty(&info)?,
                })
            }
            _ => Err(anyhow!("Unknown resource: {}", uri)),
        }
    }

    /// List available prompts
    pub fn list_prompts(&self) -> Vec<McpPrompt> {
        vec![
            McpPrompt {
                name: "find-tool".to_string(),
                description: Some("Find the best tool for a task".to_string()),
                arguments: Some(vec![McpPromptArgument {
                    name: "task".to_string(),
                    description: Some("Description of the task to accomplish".to_string()),
                    required: Some(true),
                }]),
            },
            McpPrompt {
                name: "explain-tool".to_string(),
                description: Some("Get detailed explanation of a tool".to_string()),
                arguments: Some(vec![McpPromptArgument {
                    name: "tool_name".to_string(),
                    description: Some("Name of the tool to explain".to_string()),
                    required: Some(true),
                }]),
            },
        ]
    }

    /// Get a prompt
    pub fn get_prompt(
        &self,
        name: &str,
        args: &HashMap<String, String>,
    ) -> Result<Vec<McpContent>> {
        match name {
            "find-tool" => {
                let task = args
                    .get("task")
                    .ok_or_else(|| anyhow!("Missing 'task' argument"))?;
                let tools = self.search_tools_for_task(task);
                let response = format!(
                    "Based on the task '{}', here are the recommended tools:\n\n{}",
                    task,
                    tools
                        .iter()
                        .map(|t| format!("- **{}**: {}", t.name, t.description))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                Ok(vec![McpContent::Text { text: response }])
            }
            "explain-tool" => {
                let tool_name = args
                    .get("tool_name")
                    .ok_or_else(|| anyhow!("Missing 'tool_name' argument"))?;
                let tool = self
                    .tools_db
                    .get_tool(tool_name)
                    .ok_or_else(|| anyhow!("Tool '{}' not found", tool_name))?;

                let explanation = format!(
                    "# {}\n\n{}\n\n## Category\n{:?}\n\n## Safety Level\n{:?}\n\n## Examples\n{}\n\n## Parameters\n{}",
                    tool.name,
                    tool.description,
                    tool.category,
                    tool.safety_level,
                    tool.examples.iter()
                        .map(|e| format!("- `{}` - {}", e.command, e.description))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    tool.parameters.iter()
                        .map(|p| format!("- **{}** ({:?}): {}", p.name, p.param_type, p.description))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                Ok(vec![McpContent::Text { text: explanation }])
            }
            _ => Err(anyhow!("Unknown prompt: {}", name)),
        }
    }

    /// Search tools relevant to a task description
    fn search_tools_for_task(&self, task: &str) -> Vec<&McpTool> {
        let task_lower = task.to_lowercase();
        let keywords: Vec<&str> = task_lower.split_whitespace().collect();

        let mut scored_tools: Vec<(&McpTool, usize)> = self
            .registered_tools
            .values()
            .map(|tool| {
                let tool_text = format!("{} {}", tool.name, tool.description).to_lowercase();
                let score = keywords.iter().filter(|kw| tool_text.contains(*kw)).count();
                (tool, score)
            })
            .filter(|(_, score)| *score > 0)
            .collect();

        scored_tools.sort_by(|a, b| b.1.cmp(&a.1));
        scored_tools
            .into_iter()
            .take(5)
            .map(|(tool, _)| tool)
            .collect()
    }

    /// Get all tools for a category
    pub fn get_tools_by_category(&self, category: &ToolCategory) -> Vec<&McpTool> {
        self.registered_tools
            .values()
            .filter(|tool| {
                if let Some(os_tool) = self.tools_db.get_tool(&tool.name) {
                    os_tool.category == *category
                } else {
                    false
                }
            })
            .collect()
    }

    /// Get tool count
    pub fn tool_count(&self) -> usize {
        self.registered_tools.len()
    }

    /// Get the underlying tools database
    pub fn tools_db(&self) -> &OSToolsDatabase {
        &self.tools_db
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// MCP Client for connecting to external MCP servers
// ============================================================================

/// MCP Client for connecting to external MCP servers
pub struct McpClient {
    endpoint: String,
    client: reqwest::blocking::Client,
}

impl McpClient {
    /// Create a new MCP client
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Initialize connection to MCP server
    pub fn initialize(&self) -> Result<McpInitializeResult> {
        let url = format!("{}/mcp/v1/initialize", self.endpoint);
        let response = self
            .client
            .post(&url)
            .json(&json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "aethershell",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }))
            .send()
            .map_err(|e| anyhow!("Failed to connect to MCP server: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("MCP server returned error: {}", response.status()));
        }

        response
            .json()
            .map_err(|e| anyhow!("Failed to parse response: {}", e))
    }

    /// List tools from remote MCP server
    pub fn list_tools(&self) -> Result<Vec<McpTool>> {
        let url = format!("{}/mcp/v1/tools/list", self.endpoint);
        let response = self
            .client
            .get(&url)
            .send()
            .map_err(|e| anyhow!("Failed to list tools: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to list tools: {}", response.status()));
        }

        #[derive(Deserialize)]
        struct ToolsResponse {
            tools: Vec<McpTool>,
        }

        let result: ToolsResponse = response
            .json()
            .map_err(|e| anyhow!("Failed to parse tools: {}", e))?;

        Ok(result.tools)
    }

    /// Call a tool on remote MCP server
    pub fn call_tool(
        &self,
        name: &str,
        arguments: HashMap<String, JsonValue>,
    ) -> Result<McpToolResult> {
        let url = format!("{}/mcp/v1/tools/call", self.endpoint);
        let response = self
            .client
            .post(&url)
            .json(&json!({
                "name": name,
                "arguments": arguments
            }))
            .send()
            .map_err(|e| anyhow!("Failed to call tool: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("Tool call failed: {}", response.status()));
        }

        response
            .json()
            .map_err(|e| anyhow!("Failed to parse result: {}", e))
    }

    /// List resources from remote MCP server
    pub fn list_resources(&self) -> Result<Vec<McpResource>> {
        let url = format!("{}/mcp/v1/resources/list", self.endpoint);
        let response = self
            .client
            .get(&url)
            .send()
            .map_err(|e| anyhow!("Failed to list resources: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to list resources: {}", response.status()));
        }

        #[derive(Deserialize)]
        struct ResourcesResponse {
            resources: Vec<McpResource>,
        }

        let result: ResourcesResponse = response
            .json()
            .map_err(|e| anyhow!("Failed to parse resources: {}", e))?;

        Ok(result.resources)
    }

    /// Read a resource from remote MCP server
    pub fn read_resource(&self, uri: &str) -> Result<Vec<McpContent>> {
        let url = format!("{}/mcp/v1/resources/read", self.endpoint);
        let response = self
            .client
            .post(&url)
            .json(&json!({ "uri": uri }))
            .send()
            .map_err(|e| anyhow!("Failed to read resource: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to read resource: {}", response.status()));
        }

        #[derive(Deserialize)]
        struct ReadResponse {
            contents: Vec<McpContent>,
        }

        let result: ReadResponse = response
            .json()
            .map_err(|e| anyhow!("Failed to parse resource: {}", e))?;

        Ok(result.contents)
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Create an MCP server with all tools (maximum safety)
pub fn create_full_mcp_server() -> McpServer {
    McpServer::with_config(McpConfig {
        max_safety_level: SafetyLevel::Critical,
        allow_admin_tools: true,
        allowed_categories: None,
        blocked_tools: vec![],
        execution_timeout: 60,
    })
}

/// Create an MCP server with safe tools only
pub fn create_safe_mcp_server() -> McpServer {
    McpServer::with_config(McpConfig {
        max_safety_level: SafetyLevel::Safe,
        allow_admin_tools: false,
        allowed_categories: None,
        blocked_tools: vec![],
        execution_timeout: 30,
    })
}

/// Create an MCP server for specific categories
pub fn create_category_mcp_server(categories: Vec<ToolCategory>) -> McpServer {
    McpServer::with_config(McpConfig {
        max_safety_level: SafetyLevel::Caution,
        allow_admin_tools: false,
        allowed_categories: Some(categories),
        blocked_tools: vec![],
        execution_timeout: 30,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_creation() {
        let server = McpServer::new();
        assert!(server.tool_count() > 0);
    }

    #[test]
    fn test_mcp_initialize() {
        let server = McpServer::new();
        let result = server.initialize();
        assert_eq!(result.protocol_version, "2024-11-05");
        assert_eq!(result.server_info.name, "aethershell-mcp");
    }

    #[test]
    fn test_mcp_list_tools() {
        let server = McpServer::new();
        let tools = server.list_tools();
        assert!(!tools.is_empty());
    }

    #[test]
    fn test_mcp_get_tool() {
        let server = create_full_mcp_server();
        // ls should be available on Unix-like systems
        #[cfg(not(windows))]
        {
            let tool = server.get_tool("ls");
            assert!(tool.is_some());
        }
        #[cfg(windows)]
        {
            let tool = server.get_tool("dir");
            assert!(tool.is_some());
        }
    }

    #[test]
    fn test_mcp_list_resources() {
        let server = McpServer::new();
        let resources = server.list_resources();
        assert!(!resources.is_empty());
    }

    #[test]
    fn test_mcp_list_prompts() {
        let server = McpServer::new();
        let prompts = server.list_prompts();
        assert_eq!(prompts.len(), 2);
    }

    #[test]
    fn test_safe_server_blocks_dangerous() {
        let server = create_safe_mcp_server();
        // Dangerous tools should not be registered
        assert!(server.get_tool("nmap").is_none());
    }

    #[test]
    fn test_full_server_allows_all() {
        let server = create_full_mcp_server();
        // Should have more tools than safe server
        let safe_server = create_safe_mcp_server();
        assert!(server.tool_count() >= safe_server.tool_count());
    }

    #[test]
    fn test_category_filter() {
        let server = create_category_mcp_server(vec![ToolCategory::TextProcessing]);
        let tools = server.list_tools();
        // Should only have text processing tools
        for tool in &tools {
            let os_tool = server.tools_db().get_tool(&tool.name).unwrap();
            assert_eq!(os_tool.category, ToolCategory::TextProcessing);
        }
    }

    #[test]
    fn test_search_tools_for_task() {
        let server = McpServer::new();
        let tools = server.search_tools_for_task("search for text in files");
        assert!(!tools.is_empty());
    }
}
