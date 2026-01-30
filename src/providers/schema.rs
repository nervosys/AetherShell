//! Tool Schema Generation
//!
//! Generates tool schemas in provider-specific formats from the OS ontology.
//! Each AI provider has slightly different requirements for how tools/functions
//! are defined, and this module handles the translation.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;

use super::ontology::{OSOperation, ParamType, StringFormat, PathType, DateTimeFormat, Constraint};
use super::ProviderType;

// ============================================================================
// UNIVERSAL TOOL SCHEMA
// ============================================================================

/// Provider-agnostic tool schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// Tool name (unique identifier)
    pub name: String,
    /// Tool description
    pub description: String,
    /// Input parameters schema (JSON Schema)
    pub parameters: JsonValue,
    /// Whether this tool requires confirmation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_confirmation: Option<bool>,
    /// Tool category
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Whether results should be cached
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cacheable: Option<bool>,
}

impl ToolSchema {
    /// Create a new tool schema
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            requires_confirmation: None,
            category: None,
            cacheable: None,
        }
    }

    /// Add a string parameter
    pub fn with_string_param(
        mut self,
        name: &str,
        description: &str,
        required: bool,
    ) -> Self {
        self.add_param(name, description, json!({"type": "string"}), required);
        self
    }

    /// Add an integer parameter
    pub fn with_integer_param(
        mut self,
        name: &str,
        description: &str,
        required: bool,
    ) -> Self {
        self.add_param(name, description, json!({"type": "integer"}), required);
        self
    }

    /// Add a boolean parameter
    pub fn with_boolean_param(
        mut self,
        name: &str,
        description: &str,
        required: bool,
    ) -> Self {
        self.add_param(name, description, json!({"type": "boolean"}), required);
        self
    }

    /// Add an enum parameter
    pub fn with_enum_param(
        mut self,
        name: &str,
        description: &str,
        values: Vec<&str>,
        required: bool,
    ) -> Self {
        self.add_param(
            name,
            description,
            json!({
                "type": "string",
                "enum": values
            }),
            required,
        );
        self
    }

    /// Add a parameter with a custom schema
    fn add_param(&mut self, name: &str, description: &str, mut schema: JsonValue, required: bool) {
        if let Some(obj) = schema.as_object_mut() {
            obj.insert("description".to_string(), json!(description));
        }

        if let Some(params) = self.parameters.as_object_mut() {
            if let Some(props) = params.get_mut("properties").and_then(|p| p.as_object_mut()) {
                props.insert(name.to_string(), schema);
            }
            if required {
                if let Some(req) = params.get_mut("required").and_then(|r| r.as_array_mut()) {
                    req.push(json!(name));
                }
            }
        }
    }

    /// Convert to OpenAI function format
    pub fn to_openai(&self) -> JsonValue {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters
            }
        })
    }

    /// Convert to Anthropic tool format
    pub fn to_anthropic(&self) -> JsonValue {
        json!({
            "name": self.name,
            "description": self.description,
            "input_schema": self.parameters
        })
    }

    /// Convert to Google tool format
    pub fn to_google(&self) -> JsonValue {
        json!({
            "name": self.name,
            "description": self.description,
            "parameters": self.parameters
        })
    }

    /// Convert to Cohere tool format
    pub fn to_cohere(&self) -> JsonValue {
        // Cohere uses a different parameter format
        let params = self.cohere_params();
        json!({
            "name": self.name,
            "description": self.description,
            "parameter_definitions": params
        })
    }

    /// Convert to Mistral tool format
    pub fn to_mistral(&self) -> JsonValue {
        // Mistral uses OpenAI-compatible format
        self.to_openai()
    }

    /// Convert parameters to Cohere format
    fn cohere_params(&self) -> JsonValue {
        let mut params = HashMap::new();

        if let Some(props) = self.parameters.get("properties").and_then(|p| p.as_object()) {
            let required = self.parameters.get("required")
                .and_then(|r| r.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            for (name, schema) in props {
                let desc = schema.get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                
                let param_type = schema.get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("string");

                params.insert(name.clone(), json!({
                    "description": desc,
                    "type": param_type,
                    "required": required.contains(name)
                }));
            }
        }

        json!(params)
    }

    /// Convert to provider-specific format
    pub fn to_provider_format(&self, provider: ProviderType) -> JsonValue {
        match provider {
            ProviderType::OpenAI
            | ProviderType::Azure
            | ProviderType::Groq
            | ProviderType::Together
            | ProviderType::Fireworks
            | ProviderType::DeepSeek
            | ProviderType::OpenRouter
            | ProviderType::VLLM
            | ProviderType::Ollama
            | ProviderType::TGI
            | ProviderType::LlamaCpp => self.to_openai(),
            
            ProviderType::Anthropic | ProviderType::Bedrock => self.to_anthropic(),
            ProviderType::Google => self.to_google(),
            ProviderType::Cohere => self.to_cohere(),
            ProviderType::Mistral => self.to_mistral(),
            ProviderType::XAI | ProviderType::Perplexity | ProviderType::Local => self.to_openai(),
        }
    }
}

// ============================================================================
// SCHEMA GENERATOR
// ============================================================================

/// Generates tool schemas from OS operations
pub struct SchemaGenerator {
    /// Target provider
    provider: ProviderType,
}

impl SchemaGenerator {
    /// Create a new schema generator for a provider
    pub fn new(provider: ProviderType) -> Self {
        Self { provider }
    }

    /// Generate a tool schema from an OS operation
    pub fn from_operation(&self, op: &OSOperation) -> ToolSchema {
        let mut schema = ToolSchema::new(&op.id, &op.description);
        
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &op.parameters {
            let param_schema = self.param_type_to_json(&param.param_type);
            let mut param_obj = param_schema.as_object().cloned().unwrap_or_default();
            
            param_obj.insert("description".to_string(), json!(&param.description));
            
            if let Some(default) = &param.default {
                param_obj.insert("default".to_string(), default.clone());
            }
            
            // Add examples if available
            if !param.examples.is_empty() {
                param_obj.insert("examples".to_string(), json!(param.examples));
            }

            properties.insert(param.name.clone(), JsonValue::Object(param_obj));
            
            if param.required {
                required.push(param.name.clone());
            }
        }

        schema.parameters = json!({
            "type": "object",
            "properties": properties,
            "required": required
        });

        schema.requires_confirmation = Some(op.security.requires_confirmation);
        schema.category = Some(format!("{:?}", op.domain));
        schema.cacheable = Some(op.is_idempotent);

        schema
    }

    /// Convert a ParamType to JSON Schema
    fn param_type_to_json(&self, param_type: &ParamType) -> JsonValue {
        match param_type {
            ParamType::String { format, min_length, max_length, pattern } => {
                let mut obj = json!({"type": "string"});
                if let Some(obj_mut) = obj.as_object_mut() {
                    if let Some(fmt) = format {
                        obj_mut.insert("format".to_string(), self.string_format_to_json(fmt));
                    }
                    if let Some(min) = min_length {
                        obj_mut.insert("minLength".to_string(), json!(min));
                    }
                    if let Some(max) = max_length {
                        obj_mut.insert("maxLength".to_string(), json!(max));
                    }
                    if let Some(pat) = pattern {
                        obj_mut.insert("pattern".to_string(), json!(pat));
                    }
                }
                obj
            }
            ParamType::Integer { minimum, maximum } => {
                let mut obj = json!({"type": "integer"});
                if let Some(obj_mut) = obj.as_object_mut() {
                    if let Some(min) = minimum {
                        obj_mut.insert("minimum".to_string(), json!(min));
                    }
                    if let Some(max) = maximum {
                        obj_mut.insert("maximum".to_string(), json!(max));
                    }
                }
                obj
            }
            ParamType::Number { minimum, maximum } => {
                let mut obj = json!({"type": "number"});
                if let Some(obj_mut) = obj.as_object_mut() {
                    if let Some(min) = minimum {
                        obj_mut.insert("minimum".to_string(), json!(min));
                    }
                    if let Some(max) = maximum {
                        obj_mut.insert("maximum".to_string(), json!(max));
                    }
                }
                obj
            }
            ParamType::Boolean => json!({"type": "boolean"}),
            ParamType::Array { items, min_items, max_items } => {
                let mut obj = json!({
                    "type": "array",
                    "items": self.param_type_to_json(items)
                });
                if let Some(obj_mut) = obj.as_object_mut() {
                    if let Some(min) = min_items {
                        obj_mut.insert("minItems".to_string(), json!(min));
                    }
                    if let Some(max) = max_items {
                        obj_mut.insert("maxItems".to_string(), json!(max));
                    }
                }
                obj
            }
            ParamType::Object { properties, required } => {
                let props: serde_json::Map<String, JsonValue> = properties
                    .iter()
                    .map(|(k, v)| (k.clone(), self.param_type_to_json(v)))
                    .collect();
                json!({
                    "type": "object",
                    "properties": props,
                    "required": required
                })
            }
            ParamType::Enum { values } => {
                json!({
                    "type": "string",
                    "enum": values
                })
            }
            ParamType::Path { must_exist, path_type } => {
                let mut obj = json!({
                    "type": "string",
                    "format": "path"
                });
                if let Some(obj_mut) = obj.as_object_mut() {
                    obj_mut.insert("x-must-exist".to_string(), json!(must_exist));
                    obj_mut.insert("x-path-type".to_string(), json!(format!("{:?}", path_type)));
                }
                obj
            }
            ParamType::Uri { schemes } => {
                json!({
                    "type": "string",
                    "format": "uri",
                    "x-schemes": schemes
                })
            }
            ParamType::Binary { max_size } => {
                let mut obj = json!({
                    "type": "string",
                    "contentEncoding": "base64"
                });
                if let Some(size) = max_size {
                    if let Some(obj_mut) = obj.as_object_mut() {
                        obj_mut.insert("x-max-size".to_string(), json!(size));
                    }
                }
                obj
            }
            ParamType::DateTime { format } => {
                let fmt = match format {
                    DateTimeFormat::Iso8601 => "date-time",
                    DateTimeFormat::UnixTimestamp => "unix-timestamp",
                    DateTimeFormat::Rfc2822 => "rfc2822",
                    DateTimeFormat::Custom { .. } => "custom",
                };
                json!({
                    "type": "string",
                    "format": fmt
                })
            }
            ParamType::Duration => {
                json!({
                    "type": "string",
                    "format": "duration"
                })
            }
            ParamType::OneOf { variants } => {
                json!({
                    "oneOf": variants.iter().map(|v| self.param_type_to_json(v)).collect::<Vec<_>>()
                })
            }
            ParamType::Ref { type_name } => {
                json!({
                    "$ref": format!("#/definitions/{}", type_name)
                })
            }
        }
    }

    /// Convert string format to JSON Schema format
    fn string_format_to_json(&self, format: &StringFormat) -> JsonValue {
        match format {
            StringFormat::PlainText => json!("text"),
            StringFormat::Regex => json!("regex"),
            StringFormat::Glob => json!("glob"),
            StringFormat::Json => json!("json"),
            StringFormat::Yaml => json!("yaml"),
            StringFormat::Xml => json!("xml"),
            StringFormat::Html => json!("html"),
            StringFormat::Markdown => json!("markdown"),
            StringFormat::Code { language } => {
                if let Some(lang) = language {
                    json!(format!("code:{}", lang))
                } else {
                    json!("code")
                }
            }
            StringFormat::Email => json!("email"),
            StringFormat::Hostname => json!("hostname"),
            StringFormat::IpAddress => json!("ip-address"),
            StringFormat::Uuid => json!("uuid"),
            StringFormat::Base64 => json!("base64"),
            StringFormat::Hex => json!("hex"),
        }
    }

    /// Generate schemas for all operations in a domain
    pub fn from_domain(&self, domain: &super::ontology::CapabilityDomain) -> Vec<ToolSchema> {
        super::ontology::OS_ONTOLOGY
            .by_domain(domain)
            .iter()
            .map(|op| self.from_operation(op))
            .collect()
    }

    /// Generate schemas for all operations
    pub fn all(&self) -> Vec<ToolSchema> {
        super::ontology::OS_ONTOLOGY
            .all()
            .iter()
            .map(|op| self.from_operation(op))
            .collect()
    }

    /// Generate schemas filtered by permission level
    pub fn with_max_permission(
        &self,
        max_level: super::ontology::PermissionLevel,
    ) -> Vec<ToolSchema> {
        super::ontology::OS_ONTOLOGY
            .all()
            .iter()
            .filter(|op| op.security.permission_level <= max_level)
            .map(|op| self.from_operation(op))
            .collect()
    }

    /// Generate provider-specific tool list
    pub fn to_provider_tools(&self, schemas: &[ToolSchema]) -> JsonValue {
        let tools: Vec<JsonValue> = schemas
            .iter()
            .map(|s| s.to_provider_format(self.provider.clone()))
            .collect();
        
        match self.provider {
            ProviderType::Google => {
                // Google wraps tools in a function_declarations array
                json!({
                    "function_declarations": tools
                })
            }
            _ => json!(tools),
        }
    }
}

// ============================================================================
// TOOL RESULT FORMATTING
// ============================================================================

/// Format tool results for different providers
pub struct ResultFormatter {
    provider: ProviderType,
}

impl ResultFormatter {
    pub fn new(provider: ProviderType) -> Self {
        Self { provider }
    }

    /// Format a successful tool result
    pub fn success(&self, tool_call_id: &str, result: JsonValue) -> JsonValue {
        match self.provider {
            ProviderType::OpenAI
            | ProviderType::Azure
            | ProviderType::Groq
            | ProviderType::Together
            | ProviderType::DeepSeek
            | ProviderType::OpenRouter => {
                json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": serde_json::to_string(&result).unwrap_or_default()
                })
            }
            ProviderType::Anthropic | ProviderType::Bedrock => {
                json!({
                    "type": "tool_result",
                    "tool_use_id": tool_call_id,
                    "content": serde_json::to_string(&result).unwrap_or_default()
                })
            }
            ProviderType::Google => {
                json!({
                    "functionResponse": {
                        "name": tool_call_id,
                        "response": result
                    }
                })
            }
            ProviderType::Cohere => {
                json!({
                    "call": {
                        "name": tool_call_id
                    },
                    "outputs": [result]
                })
            }
            _ => json!({
                "tool_call_id": tool_call_id,
                "result": result
            }),
        }
    }

    /// Format an error result
    pub fn error(&self, tool_call_id: &str, error: &str) -> JsonValue {
        match self.provider {
            ProviderType::Anthropic | ProviderType::Bedrock => {
                json!({
                    "type": "tool_result",
                    "tool_use_id": tool_call_id,
                    "content": error,
                    "is_error": true
                })
            }
            _ => self.success(tool_call_id, json!({
                "error": error
            })),
        }
    }
}

// ============================================================================
// BUILT-IN TOOL SCHEMAS
// ============================================================================

/// Collection of commonly used tool schemas
pub mod common_tools {
    use super::*;

    /// Read file tool
    pub fn read_file() -> ToolSchema {
        ToolSchema::new("read_file", "Read the contents of a file")
            .with_string_param("path", "Path to the file to read", true)
            .with_enum_param("encoding", "Text encoding", vec!["utf-8", "ascii", "binary"], false)
    }

    /// Write file tool
    pub fn write_file() -> ToolSchema {
        ToolSchema::new("write_file", "Write content to a file")
            .with_string_param("path", "Path to write to", true)
            .with_string_param("content", "Content to write", true)
    }

    /// List directory tool
    pub fn list_dir() -> ToolSchema {
        ToolSchema::new("list_dir", "List contents of a directory")
            .with_string_param("path", "Directory path to list", false)
            .with_boolean_param("recursive", "List recursively", false)
    }

    /// Execute command tool
    pub fn execute_command() -> ToolSchema {
        ToolSchema::new("execute_command", "Execute a shell command")
            .with_string_param("command", "The command to execute", true)
            .with_integer_param("timeout", "Timeout in seconds", false)
    }

    /// HTTP request tool
    pub fn http_request() -> ToolSchema {
        ToolSchema::new("http_request", "Make an HTTP request")
            .with_string_param("url", "Target URL", true)
            .with_enum_param("method", "HTTP method", vec!["GET", "POST", "PUT", "DELETE"], false)
            .with_string_param("body", "Request body", false)
    }

    /// Search files tool
    pub fn search_files() -> ToolSchema {
        ToolSchema::new("search_files", "Search for files by pattern or content")
            .with_string_param("pattern", "Glob pattern or search query", true)
            .with_string_param("directory", "Directory to search in", false)
            .with_boolean_param("content_search", "Search file contents", false)
    }

    /// Get all common tools
    pub fn all() -> Vec<ToolSchema> {
        vec![
            read_file(),
            write_file(),
            list_dir(),
            execute_command(),
            http_request(),
            search_files(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_schema_builder() {
        let schema = ToolSchema::new("test_tool", "A test tool")
            .with_string_param("input", "The input", true)
            .with_boolean_param("flag", "A flag", false);

        assert_eq!(schema.name, "test_tool");
        let params = schema.parameters.as_object().unwrap();
        let props = params["properties"].as_object().unwrap();
        assert!(props.contains_key("input"));
        assert!(props.contains_key("flag"));
    }

    #[test]
    fn test_openai_format() {
        let schema = ToolSchema::new("my_tool", "Does something")
            .with_string_param("arg", "An argument", true);
        
        let openai = schema.to_openai();
        assert_eq!(openai["type"], "function");
        assert_eq!(openai["function"]["name"], "my_tool");
    }

    #[test]
    fn test_anthropic_format() {
        let schema = ToolSchema::new("my_tool", "Does something")
            .with_string_param("arg", "An argument", true);
        
        let anthropic = schema.to_anthropic();
        assert_eq!(anthropic["name"], "my_tool");
        assert!(anthropic.get("input_schema").is_some());
    }

    #[test]
    fn test_common_tools() {
        let tools = common_tools::all();
        assert!(!tools.is_empty());
        assert!(tools.iter().any(|t| t.name == "read_file"));
    }
}
