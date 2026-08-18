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

    /// The name of the compact-mode meta-tool that invokes any builtin by name.
    pub const META_INVOKE_TOOL: &'static str = "aether";

    /// Whether the MCP tool surface is in compact discovery mode (the default).
    /// `AETHER_MCP_TOOLS=all` (or `full`) restores the flat per-builtin listing.
    fn tools_compact_mode() -> bool {
        match std::env::var("AETHER_MCP_TOOLS") {
            Ok(v) => !(v.eq_ignore_ascii_case("all") || v.eq_ignore_ascii_case("full")),
            Err(_) => true,
        }
    }

    /// Expose AetherShell's builtins to an MCP agent. By **default** this is the
    /// *compact discovery surface* — three meta-tools instead of ~1k individual
    /// tool definitions, which otherwise cost ~26k tokens of standing context every
    /// session (measured: `examples/standing_context.rs`). The agent indexes the
    /// catalog via `ontology_manifest`, expands a slice with `ontology_describe`,
    /// and invokes any builtin through the `aether` meta-tool — all routed through
    /// [`Self::call_builtin`], so effect policy / approval / jail / audit apply
    /// unchanged. Set `AETHER_MCP_TOOLS=all` for the flat, per-builtin `x-effect`
    /// listing (every operation's danger level visible up front).
    pub fn list_builtin_tools(&self) -> Vec<McpTool> {
        if Self::tools_compact_mode() {
            Self::compact_discovery_tools()
        } else {
            Self::full_builtin_tools()
        }
    }

    /// The compact discovery surface (default): `ontology_manifest` +
    /// `ontology_describe` (progressive disclosure of the catalog) and the `aether`
    /// invoke meta-tool. Effect classes remain discoverable per-category in the
    /// manifest and per-builtin via describe; enforcement is unchanged on call.
    fn compact_discovery_tools() -> Vec<McpTool> {
        vec![
            McpTool {
                name: "ontology_manifest".to_string(),
                description: "List AetherShell's builtin categories (name, count, effect classes) — the compact index of the full toolset. Call with no args, then ontology_describe(\"<category>\") to expand a slice.".to_string(),
                input_schema: serde_json::json!({ "type": "object", "properties": {} }),
            },
            McpTool {
                name: "ontology_describe".to_string(),
                description: "Expand one slice of the ontology: a category name -> its builtins (name, signature, effect); or a builtin name -> full detail (params, examples, effect class). Arg: a single query string.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "args": { "type": "array", "description": "[query]: a category or builtin name" }
                    },
                }),
            },
            McpTool {
                name: Self::META_INVOKE_TOOL.to_string(),
                description: "Invoke any AetherShell builtin by name (discover names via ontology_manifest / ontology_describe, and check a builtin's effect class there before invoking a destructive one). Runs under the same effect policy / approval / workspace jail / audit as every builtin.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "builtin name to invoke" },
                        "args": { "type": "array", "description": "positional arguments" }
                    },
                    "required": ["name"],
                }),
            },
        ]
    }

    /// The flat per-builtin listing (`AETHER_MCP_TOOLS=all`): every builtin as its
    /// own MCP tool, annotated with its safety effect class under `x-effect`.
    fn full_builtin_tools() -> Vec<McpTool> {
        crate::agent_api::builtin_tool_specs()
            .into_iter()
            .filter_map(|spec| {
                let name = spec.get("name").and_then(|v| v.as_str())?.to_string();
                let desc = spec
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let sig = spec.get("signature").and_then(|v| v.as_str()).unwrap_or("");
                let effect = spec
                    .get("effect")
                    .and_then(|v| v.as_str())
                    .unwrap_or("pure");
                // The proven return shape, when there is one. Lets an agent
                // compose against the result without a discovery call first.
                let returns = spec.get("returns").and_then(|v| v.as_str());
                let mut input_schema = serde_json::json!({
                    "type": "object",
                    "properties": {
                        "args": { "type": "array", "description": "positional arguments" }
                    },
                    "x-effect": effect,
                });
                if let (Some(r), Some(obj)) = (returns, input_schema.as_object_mut()) {
                    obj.insert("x-returns".to_string(), serde_json::json!(r));
                }
                Some(McpTool {
                    name,
                    description: format!("{} | {}", sig, desc),
                    input_schema,
                })
            })
            .collect()
    }

    /// Invoke an AetherShell builtin as an MCP tool. `args` is a JSON array of
    /// positional arguments. The call goes through `builtins::call`, so guarded
    /// builtins enforce policy/approval/jail and emit audit entries; a refusal
    /// is returned as an error result carrying the structured `E_*` code.
    pub fn call_builtin(&self, name: &str, args: &JsonValue) -> McpToolResult {
        let values: Vec<crate::value::Value> = match args {
            JsonValue::Array(a) => a.iter().map(crate::value::Value::from_json).collect(),
            JsonValue::Null => vec![],
            other => vec![crate::value::Value::from_json(other)],
        };
        let mut env = crate::env::Env::new();
        match crate::builtins::call(name, values, &mut env) {
            Ok(v) => {
                // Agent mode mirrors the CLI/HTTP default: tabular results render
                // as compact AECON (MCP content is plain text, so no JSON
                // re-escaping cost). Scalars keep their display form.
                let text = if crate::safety::current_mode() == crate::safety::Mode::Agent
                    && matches!(
                        v,
                        crate::value::Value::Array(_) | crate::value::Value::Table(_)
                    ) {
                    let budget = std::env::var("AE_TOKEN_BUDGET")
                        .ok()
                        .and_then(|s| s.parse::<usize>().ok())
                        .filter(|m| *m > 0);
                    crate::builtins::render_agent(&v, budget)
                        .unwrap_or_else(|| v.to_display_string())
                } else {
                    v.to_display_string()
                };
                McpToolResult {
                    content: vec![McpContent::Text { text }],
                    is_error: Some(false),
                }
            }
            Err(e) => McpToolResult {
                content: vec![McpContent::Text {
                    text: format!("{}", e),
                }],
                is_error: Some(true),
            },
        }
    }

    /// Route an MCP `tools/call` by tool name. The compact-mode `aether` meta-tool
    /// unwraps `{name, args}` and invokes that builtin; any other name is treated as
    /// a builtin invoked with its `args` array. Every path goes through
    /// [`Self::call_builtin`], so effect policy / approval / jail / audit apply
    /// uniformly whether the agent is in compact or full mode.
    pub fn route_tool_call(&self, name: &str, arguments: &JsonValue) -> McpToolResult {
        if name == Self::META_INVOKE_TOOL {
            let inner = arguments.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if inner.is_empty() {
                return McpToolResult {
                    content: vec![McpContent::Text {
                        text: "aether: missing 'name' (the builtin to invoke)".to_string(),
                    }],
                    is_error: Some(true),
                };
            }
            let args = arguments
                .get("args")
                .cloned()
                .unwrap_or_else(|| JsonValue::Array(vec![]));
            return self.call_builtin(inner, &args);
        }
        let args = arguments
            .get("args")
            .cloned()
            .unwrap_or_else(|| arguments.clone());
        self.call_builtin(name, &args)
    }

    /// Run a strict **JSON-RPC 2.0 over stdio** MCP server (the canonical MCP
    /// transport, complementing the HTTP server). Reads newline-delimited requests
    /// from stdin and dispatches:
    ///
    /// - `initialize` → protocol/capabilities/serverInfo;
    /// - `tools/list` → the builtin tool surface ([`Self::list_builtin_tools`]);
    /// - `tools/call` → routes through [`Self::call_builtin`], so policy / jail /
    ///   approval / audit apply exactly as on the REPL and HTTP paths;
    /// - `ping` → `{}`.
    ///
    /// Writes one JSON-RPC response line per request to stdout. Notifications
    /// (messages with no `id`, e.g. `notifications/initialized`) get no reply.
    /// Loops until stdin EOF. Runs in whatever safety mode the process is in, so
    /// `ae --agent mcp stdio` serves with the agent default-deny policy active.
    pub fn serve_stdio(&self) -> std::io::Result<()> {
        use std::io::BufRead;
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        let mut handle = stdin.lock();
        let mut buf = String::new();
        loop {
            buf.clear();
            if handle.read_line(&mut buf)? == 0 {
                break; // EOF
            }
            let line = buf.trim();
            if line.is_empty() {
                continue;
            }
            let req: JsonValue = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(e) => {
                    Self::write_message(
                        &mut out,
                        &Self::rpc_error(JsonValue::Null, -32700, &format!("parse error: {}", e)),
                    )?;
                    continue;
                }
            };
            if let Some(response) = self.handle_rpc(&req) {
                Self::write_message(&mut out, &response)?;
            }
        }
        Ok(())
    }

    /// Dispatch one JSON-RPC request, returning the response (or `None` for a
    /// notification, which gets no reply). Pure w.r.t. I/O so it is unit-testable.
    pub fn handle_rpc(&self, req: &JsonValue) -> Option<JsonValue> {
        // Notifications carry no `id` → no response.
        let id = match req.get("id") {
            Some(v) if !v.is_null() => v.clone(),
            _ => return None,
        };
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let response = match method {
            "initialize" => Self::rpc_ok(id, self.stdio_initialize()),
            "tools/list" => Self::rpc_ok(
                id,
                serde_json::json!({
                    "tools": serde_json::to_value(self.list_builtin_tools())
                        .unwrap_or_else(|_| serde_json::json!([])),
                }),
            ),
            "tools/call" => match self.stdio_tools_call(req.get("params")) {
                Ok(result) => Self::rpc_ok(id, result),
                Err((code, msg)) => Self::rpc_error(id, code, &msg),
            },
            "ping" => Self::rpc_ok(id, serde_json::json!({})),
            other => Self::rpc_error(id, -32601, &format!("method not found: {}", other)),
        };
        Some(response)
    }

    fn stdio_initialize(&self) -> JsonValue {
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "aethershell", "version": env!("CARGO_PKG_VERSION") },
        })
    }

    /// Handle a `tools/call`: extract the tool name and positional args
    /// (`arguments.args` if present, else the whole `arguments`), route through
    /// `call_builtin`, and serialize the [`McpToolResult`].
    fn stdio_tools_call(&self, params: Option<&JsonValue>) -> Result<JsonValue, (i64, String)> {
        let params = params.ok_or((-32602, "missing params".to_string()))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or((-32602, "missing tool name".to_string()))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| JsonValue::Array(vec![]));
        let result = self.route_tool_call(name, &arguments);
        serde_json::to_value(result).map_err(|e| (-32603, format!("serialize result: {}", e)))
    }

    fn rpc_ok(id: JsonValue, result: JsonValue) -> JsonValue {
        serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result })
    }

    fn rpc_error(id: JsonValue, code: i64, message: &str) -> JsonValue {
        serde_json::json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
    }

    fn write_message(out: &mut impl std::io::Write, msg: &JsonValue) -> std::io::Result<()> {
        let s = serde_json::to_string(msg).unwrap_or_else(|_| "{}".to_string());
        writeln!(out, "{}", s)?;
        out.flush()
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
// MCP HTTP Server (for `ae mcp serve`)
// ============================================================================

#[cfg(feature = "native")]
pub mod server {
    use super::*;
    use axum::{
        extract::State,
        http::{header, StatusCode},
        response::IntoResponse,
        routing::{get, post},
        Json, Router,
    };
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tower_http::cors::{Any, CorsLayer};

    /// MCP HTTP Server configuration
    #[derive(Debug, Clone)]
    pub struct McpServerConfig {
        pub host: String,
        pub port: u16,
        pub enable_cors: bool,
        /// Bearer token required on every route except `/health`.
        ///
        /// `None` mints one at startup and prints it -- the server is never
        /// reachable without a credential, matching `agent_api`.
        pub auth_token: Option<String>,
        pub safety_level: SafetyLevel,
        pub allow_admin: bool,
    }

    impl Default for McpServerConfig {
        fn default() -> Self {
            Self {
                host: "127.0.0.1".to_string(),
                port: 3001,
                // Off by default. `allow_origin(Any)` on a server that executes
                // builtins means any page the user visits can drive it; a
                // library caller taking Default should not opt into that
                // silently.
                enable_cors: false,
                auth_token: None,
                safety_level: SafetyLevel::Caution,
                allow_admin: false,
            }
        }
    }

    /// Shared state for the HTTP server
    struct AppState {
        mcp: RwLock<McpServer>,
    }

    /// Start the MCP HTTP server
    /// Mint a bearer token, matching `agent_api`'s construction.
    fn generate_mcp_token() -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        let bytes: [u8; 32] = rand::random();
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Constant-time comparison, so a token cannot be recovered a byte at a
    /// time by timing the response.
    fn mcp_tokens_match(presented: &str, expected: &str) -> bool {
        let (a, b) = (presented.as_bytes(), expected.as_bytes());
        if a.len() != b.len() {
            return false;
        }
        let mut diff = 0u8;
        for (x, y) in a.iter().zip(b.iter()) {
            diff |= x ^ y;
        }
        diff == 0
    }

    pub async fn start_mcp_server(config: McpServerConfig) -> Result<()> {
        let mcp_config = McpConfig {
            max_safety_level: config.safety_level.clone(),
            allow_admin_tools: config.allow_admin,
            allowed_categories: None,
            blocked_tools: vec![],
            execution_timeout: 30,
        };

        let mcp = McpServer::with_config(mcp_config);
        let state = Arc::new(AppState {
            mcp: RwLock::new(mcp),
        });

        let mut app = Router::new()
            // MCP Protocol endpoints
            .route("/mcp/v1/initialize", post(handle_initialize))
            .route("/mcp/v1/tools", get(handle_list_tools))
            .route("/mcp/v1/builtins", get(handle_list_builtin_tools))
            .route("/mcp/v1/tools/:name/execute", post(handle_call_tool))
            .route("/mcp/v1/resources", get(handle_list_resources))
            .route("/mcp/v1/resources/:uri", get(handle_read_resource))
            .route("/mcp/v1/prompts", get(handle_list_prompts))
            .route("/mcp/v1/prompts/:name", post(handle_get_prompt))
            // Health check
            .route("/health", get(handle_health))
            // Info endpoint
            .route("/", get(handle_info))
            .with_state(state);

        // Authentication, applied before CORS so it wraps every route.
        //
        // This server had none, and `POST /mcp/v1/tools/:name/execute` runs
        // builtins. With `--cors` that was demonstrably exploitable: a page on
        // https://evil.example POSTed here and read `C:/Windows/win.ini` off
        // the disk, cross-origin, unauthenticated. `agent_api` carries a
        // comment explaining that its own `allow_origin(Any)` is only tolerable
        // *because* a bearer token is required -- the reasoning was right there
        // and this server did not have the token.
        //
        // Loopback binding is not a defence: the browser making the request is
        // on the same machine, which is the entire point of the CORS control.
        let token = Arc::new(match config.auth_token.clone() {
            Some(t) if !t.trim().is_empty() => t,
            _ => {
                let t = generate_mcp_token();
                println!("MCP bearer token: {t}");
                println!(
                    "Send it as `Authorization: Bearer <token>` on every route except /health."
                );
                t
            }
        });
        let auth_token = Arc::clone(&token);
        app = app.layer(axum::middleware::from_fn(
            move |req: axum::extract::Request, next: axum::middleware::Next| {
                let expected = Arc::clone(&auth_token);
                async move {
                    if req.uri().path() == "/health" {
                        return next.run(req).await;
                    }
                    let presented = req
                        .headers()
                        .get(header::AUTHORIZATION)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.strip_prefix("Bearer "))
                        .map(str::trim)
                        .unwrap_or("");
                    if mcp_tokens_match(presented, &expected) {
                        next.run(req).await
                    } else {
                        (
                            StatusCode::UNAUTHORIZED,
                            [(header::WWW_AUTHENTICATE, "Bearer")],
                            Json(serde_json::json!({
                                "error": "unauthorized",
                                "detail": "Send `Authorization: Bearer <token>`.                                     The token is printed when the server starts.",
                            })),
                        )
                            .into_response()
                    }
                }
            },
        ));

        if config.enable_cors {
            app = app.layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            );
        }

        let addr: SocketAddr = format!("{}:{}", config.host, config.port)
            .parse()
            .map_err(|e| anyhow!("Invalid address: {}", e))?;

        println!("🚀 AetherShell MCP Server starting on http://{}", addr);
        println!("   Protocol: MCP 2024-11-05");
        println!("   Safety level: {:?}", config.safety_level);
        println!();
        println!("Endpoints:");
        println!("  POST /mcp/v1/initialize     - Initialize MCP session");
        println!("  GET  /mcp/v1/tools          - List available tools");
        println!("  POST /mcp/v1/tools/:name    - Execute a tool");
        println!("  GET  /mcp/v1/resources      - List resources");
        println!("  GET  /mcp/v1/prompts        - List prompts");
        println!("  GET  /health                - Health check");
        println!();

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    // Handler implementations

    async fn handle_info() -> impl IntoResponse {
        Json(json!({
            "name": "aethershell-mcp",
            "version": env!("CARGO_PKG_VERSION"),
            "protocol": "MCP 2024-11-05",
            "description": "AetherShell Model Context Protocol Server"
        }))
    }

    async fn handle_health() -> impl IntoResponse {
        Json(json!({
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION")
        }))
    }

    async fn handle_initialize(State(state): State<Arc<AppState>>) -> impl IntoResponse {
        let mcp = state.mcp.read().await;
        Json(mcp.initialize())
    }

    async fn handle_list_tools(State(state): State<Arc<AppState>>) -> impl IntoResponse {
        let mcp = state.mcp.read().await;
        Json(json!({
            "tools": mcp.list_tools()
        }))
    }

    /// Discover AetherShell builtins as MCP tools (effect-tagged). Kept separate
    /// from `/tools` so the OS-tool list stays small; builtins are executed via
    /// the same `/tools/:name/execute` route (it falls back to builtins).
    async fn handle_list_builtin_tools(State(state): State<Arc<AppState>>) -> impl IntoResponse {
        let mcp = state.mcp.read().await;
        Json(json!({
            "tools": mcp.list_builtin_tools()
        }))
    }

    #[derive(Deserialize)]
    struct ToolCallRequest {
        arguments: HashMap<String, JsonValue>,
    }

    async fn handle_call_tool(
        State(state): State<Arc<AppState>>,
        axum::extract::Path(name): axum::extract::Path<String>,
        Json(payload): Json<ToolCallRequest>,
    ) -> impl IntoResponse {
        let mcp = state.mcp.read().await;
        // If `name` is a registered OS tool, run it; otherwise treat it as an
        // AetherShell builtin and route through the safety-guarded dispatch
        // (effect policy, approval, jail, audit all apply). Builtin args come
        // from the conventional `args` array in the request.
        let result = if mcp.get_tool(&name).is_some() {
            mcp.call_tool(McpToolCall {
                name,
                arguments: payload.arguments,
            })
        } else {
            let arguments = serde_json::json!(payload.arguments);
            mcp.route_tool_call(&name, &arguments)
        };

        if result.is_error.unwrap_or(false) {
            (StatusCode::BAD_REQUEST, Json(result))
        } else {
            (StatusCode::OK, Json(result))
        }
    }

    async fn handle_list_resources(State(state): State<Arc<AppState>>) -> impl IntoResponse {
        let mcp = state.mcp.read().await;
        Json(json!({
            "resources": mcp.list_resources()
        }))
    }

    async fn handle_read_resource(
        State(state): State<Arc<AppState>>,
        axum::extract::Path(uri): axum::extract::Path<String>,
    ) -> impl IntoResponse {
        let mcp = state.mcp.read().await;
        // URL decode the URI
        let decoded_uri = urlencoding::decode(&uri)
            .map(|s| s.into_owned())
            .unwrap_or(uri);

        match mcp.read_resource(&decoded_uri) {
            Ok(content) => (StatusCode::OK, Json(json!({ "content": content }))),
            Err(e) => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": e.to_string() })),
            ),
        }
    }

    async fn handle_list_prompts(State(state): State<Arc<AppState>>) -> impl IntoResponse {
        let mcp = state.mcp.read().await;
        Json(json!({
            "prompts": mcp.list_prompts()
        }))
    }

    #[derive(Deserialize)]
    struct PromptRequest {
        arguments: HashMap<String, String>,
    }

    async fn handle_get_prompt(
        State(state): State<Arc<AppState>>,
        axum::extract::Path(name): axum::extract::Path<String>,
        Json(payload): Json<PromptRequest>,
    ) -> impl IntoResponse {
        let mcp = state.mcp.read().await;
        match mcp.get_prompt(&name, &payload.arguments) {
            Ok(content) => (StatusCode::OK, Json(json!({ "messages": content }))),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": e.to_string() })),
            ),
        }
    }
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
