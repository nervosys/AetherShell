//! Agentic AI Callable API for AetherShell
//!
//! This module provides a structured JSON-based API for AI agents to interact
//! with AetherShell without needing to generate multi-line code blocks.
//!
//! Features:
//! - Single-call builtin execution
//! - Pipeline construction and execution
//! - Language ontology/schema export
//! - Native support for OpenAI, Claude, and Gemini function calling formats
//!
//! # AI Agent Integration
//!
//! Instead of generating brittle AetherShell code:
//! ```ae
//! let files = ls(".")
//! files | where(fn(f) => f.size > 1000) | map(fn(f) => f.name)
//! ```
//!
//! AI agents can use structured JSON:
//! ```json
//! {
//!   "action": "call",
//!   "builtin": "ls",
//!   "args": { "path": "." }
//! }
//! ```

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;

use crate::env::Env;
use crate::eval::eval_program;
use crate::parser::parse_program;
use crate::value::Value;

// ============================================================================
// Core API Types
// ============================================================================

/// Agent API request - the main entry point for AI agents
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AgentRequest {
    /// Execute a single builtin function
    Call {
        builtin: String,
        #[serde(default)]
        args: JsonValue,
    },
    /// Execute a pipeline of operations
    Pipeline {
        steps: Vec<PipelineStep>,
        #[serde(default)]
        input: Option<JsonValue>,
    },
    /// Execute raw AetherShell code (escape hatch)
    Eval { code: String },
    /// Get information about a builtin
    Describe { builtin: String },
    /// List all available builtins
    ListBuiltins {
        #[serde(default)]
        category: Option<String>,
    },
    /// Get the language ontology/schema
    Schema {
        #[serde(default)]
        format: SchemaFormat,
    },
    /// Get type information
    TypeInfo {
        #[serde(default)]
        type_name: Option<String>,
    },
}

/// A single step in a pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStep {
    /// Builtin function to call
    pub builtin: String,
    /// Arguments (can be positional array or named object)
    #[serde(default)]
    pub args: JsonValue,
    /// Optional field selector (e.g., "name" to extract .name from results)
    #[serde(default)]
    pub select: Option<String>,
    /// Optional predicate for filtering (simplified expression)
    #[serde(default)]
    pub predicate: Option<String>,
}

/// Schema output format
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SchemaFormat {
    /// JSON Schema format
    #[default]
    JsonSchema,
    /// OpenAI function calling format
    OpenAI,
    /// Anthropic Claude tool use format
    Claude,
    /// Google Gemini function declaration format
    Gemini,
    /// Compact ontology format
    Ontology,
}

/// Agent API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// The result value (if success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<JsonValue>,
    /// Error message (if failure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Type of the result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_type: Option<String>,
    /// Metadata about the operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonValue>,
}

// ============================================================================
// Language Ontology Types
// ============================================================================

/// Complete language ontology for AI agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageOntology {
    /// Language metadata
    pub language: LanguageInfo,
    /// Type system
    pub types: Vec<TypeDefinition>,
    /// All builtin functions
    pub builtins: Vec<BuiltinDefinition>,
    /// Operators
    pub operators: Vec<OperatorDefinition>,
    /// Syntax patterns
    pub syntax: SyntaxPatterns,
    /// Categories of functionality
    pub categories: Vec<CategoryInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub paradigms: Vec<String>,
    pub typing: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDefinition {
    pub name: String,
    pub description: String,
    pub json_equivalent: String,
    pub examples: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<FieldDefinition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinDefinition {
    pub name: String,
    pub description: String,
    pub category: String,
    pub signature: String,
    pub parameters: Vec<ParameterDefinition>,
    pub return_type: String,
    pub examples: Vec<ExampleDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    /// Simplified JSON calling convention
    pub json_schema: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub description: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExampleDefinition {
    pub description: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorDefinition {
    pub symbol: String,
    pub name: String,
    pub description: String,
    pub precedence: u8,
    pub associativity: String,
    pub operand_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxPatterns {
    pub variable_declaration: String,
    pub function_definition: String,
    pub lambda: String,
    pub pipeline: String,
    pub record_literal: String,
    pub array_literal: String,
    pub conditional: String,
    pub pattern_matching: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryInfo {
    pub name: String,
    pub description: String,
    pub builtin_count: usize,
}

// ============================================================================
// API Implementation
// ============================================================================

/// Process an agent API request
pub fn process_request(request: &AgentRequest) -> AgentResponse {
    match request {
        AgentRequest::Call { builtin, args } => execute_call(builtin, args),
        AgentRequest::Pipeline { steps, input } => execute_pipeline(steps, input.as_ref()),
        AgentRequest::Eval { code } => execute_eval(code),
        AgentRequest::Describe { builtin } => describe_builtin(builtin),
        AgentRequest::ListBuiltins { category } => list_builtins(category.as_deref()),
        AgentRequest::Schema { format } => get_schema(format),
        AgentRequest::TypeInfo { type_name } => get_type_info(type_name.as_deref()),
    }
}

/// Execute a single builtin call
fn execute_call(builtin: &str, args: &JsonValue) -> AgentResponse {
    // Convert JSON args to AetherShell code
    let code = match args_to_code(builtin, args) {
        Ok(c) => c,
        Err(e) => {
            return AgentResponse {
                success: false,
                result: None,
                error: Some(format!("Failed to convert arguments: {}", e)),
                result_type: None,
                metadata: None,
            }
        }
    };

    execute_eval(&code)
}

/// Execute a pipeline of operations
fn execute_pipeline(steps: &[PipelineStep], input: Option<&JsonValue>) -> AgentResponse {
    if steps.is_empty() {
        return AgentResponse {
            success: false,
            result: None,
            error: Some("Pipeline must have at least one step".to_string()),
            result_type: None,
            metadata: None,
        };
    }

    // Build the pipeline code
    let mut code_parts = Vec::new();

    // Handle initial input if provided
    if let Some(input_val) = input {
        code_parts.push(json_to_ae_literal(input_val));
    }

    for step in steps {
        let step_code = match build_step_code(step) {
            Ok(c) => c,
            Err(e) => {
                return AgentResponse {
                    success: false,
                    result: None,
                    error: Some(format!("Failed to build step '{}': {}", step.builtin, e)),
                    result_type: None,
                    metadata: None,
                }
            }
        };

        if code_parts.is_empty() {
            code_parts.push(step_code);
        } else {
            code_parts.push(format!("| {}", step_code));
        }
    }

    let code = code_parts.join(" ");
    execute_eval(&code)
}

/// Execute raw AetherShell code
fn execute_eval(code: &str) -> AgentResponse {
    let mut env = Env::default();

    match parse_program(code) {
        Ok(stmts) => match eval_program(&stmts, &mut env) {
            Ok(value) => {
                let result_type = value_type_name(&value);
                AgentResponse {
                    success: true,
                    result: Some(value_to_json(&value)),
                    error: None,
                    result_type: Some(result_type),
                    metadata: Some(json!({
                        "code_executed": code
                    })),
                }
            }
            Err(e) => AgentResponse {
                success: false,
                result: None,
                error: Some(format!("{}", e)),
                result_type: None,
                metadata: Some(json!({
                    "code_attempted": code
                })),
            },
        },
        Err(e) => AgentResponse {
            success: false,
            result: None,
            error: Some(format!("Parse error: {}", e)),
            result_type: None,
            metadata: Some(json!({
                "code_attempted": code
            })),
        },
    }
}

/// Get information about a specific builtin
fn describe_builtin(name: &str) -> AgentResponse {
    let builtins = get_builtin_definitions();

    if let Some(builtin) = builtins.iter().find(|b| b.name == name) {
        AgentResponse {
            success: true,
            result: Some(serde_json::to_value(builtin).unwrap_or(JsonValue::Null)),
            error: None,
            result_type: Some("BuiltinDefinition".to_string()),
            metadata: None,
        }
    } else {
        // Check for aliases
        if let Some(builtin) = builtins.iter().find(|b| {
            b.aliases
                .as_ref()
                .map(|a| a.contains(&name.to_string()))
                .unwrap_or(false)
        }) {
            AgentResponse {
                success: true,
                result: Some(serde_json::to_value(builtin).unwrap_or(JsonValue::Null)),
                error: None,
                result_type: Some("BuiltinDefinition".to_string()),
                metadata: Some(
                    json!({"note": format!("'{}' is an alias for '{}'", name, builtin.name)}),
                ),
            }
        } else {
            AgentResponse {
                success: false,
                result: None,
                error: Some(format!("Builtin '{}' not found", name)),
                result_type: None,
                metadata: None,
            }
        }
    }
}

/// List all available builtins
fn list_builtins(category: Option<&str>) -> AgentResponse {
    let builtins = get_builtin_definitions();

    let filtered: Vec<_> = if let Some(cat) = category {
        builtins
            .into_iter()
            .filter(|b| b.category.to_lowercase() == cat.to_lowercase())
            .collect()
    } else {
        builtins
    };

    let summary: Vec<JsonValue> = filtered
        .iter()
        .map(|b| {
            json!({
                "name": b.name,
                "description": b.description,
                "category": b.category,
                "signature": b.signature
            })
        })
        .collect();

    AgentResponse {
        success: true,
        result: Some(json!({
            "count": summary.len(),
            "builtins": summary
        })),
        error: None,
        result_type: Some("BuiltinList".to_string()),
        metadata: None,
    }
}

/// Get the language schema in the requested format
fn get_schema(format: &SchemaFormat) -> AgentResponse {
    let ontology = build_language_ontology();

    let result = match format {
        SchemaFormat::JsonSchema => serde_json::to_value(&ontology).unwrap_or(JsonValue::Null),
        SchemaFormat::OpenAI => build_openai_schema(&ontology),
        SchemaFormat::Claude => build_claude_schema(&ontology),
        SchemaFormat::Gemini => build_gemini_schema(&ontology),
        SchemaFormat::Ontology => build_compact_ontology(&ontology),
    };

    AgentResponse {
        success: true,
        result: Some(result),
        error: None,
        result_type: Some(format!("{:?}Schema", format)),
        metadata: None,
    }
}

/// Get type information
fn get_type_info(type_name: Option<&str>) -> AgentResponse {
    let types = get_type_definitions();

    if let Some(name) = type_name {
        if let Some(type_def) = types
            .iter()
            .find(|t| t.name.to_lowercase() == name.to_lowercase())
        {
            AgentResponse {
                success: true,
                result: Some(serde_json::to_value(type_def).unwrap_or(JsonValue::Null)),
                error: None,
                result_type: Some("TypeDefinition".to_string()),
                metadata: None,
            }
        } else {
            AgentResponse {
                success: false,
                result: None,
                error: Some(format!("Type '{}' not found", name)),
                result_type: None,
                metadata: None,
            }
        }
    } else {
        AgentResponse {
            success: true,
            result: Some(serde_json::to_value(&types).unwrap_or(JsonValue::Null)),
            error: None,
            result_type: Some("TypeList".to_string()),
            metadata: None,
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert JSON arguments to AetherShell code
fn args_to_code(builtin: &str, args: &JsonValue) -> Result<String> {
    match args {
        JsonValue::Null => Ok(format!("{}()", builtin)),
        JsonValue::Array(arr) => {
            let arg_strs: Vec<String> = arr.iter().map(json_to_ae_literal).collect();
            Ok(format!("{}({})", builtin, arg_strs.join(", ")))
        }
        JsonValue::Object(obj) => {
            // Named arguments - convert to positional based on builtin signature
            // For now, if there's a single "input" or "value" key, use it directly
            if let Some(val) = obj.get("input").or(obj.get("value")).or(obj.get("path")) {
                Ok(format!("{}({})", builtin, json_to_ae_literal(val)))
            } else if obj.is_empty() {
                Ok(format!("{}()", builtin))
            } else {
                // Convert object to record argument
                Ok(format!("{}({})", builtin, json_to_ae_literal(args)))
            }
        }
        _ => Ok(format!("{}({})", builtin, json_to_ae_literal(args))),
    }
}

/// Build code for a single pipeline step
fn build_step_code(step: &PipelineStep) -> Result<String> {
    let mut code = args_to_code(&step.builtin, &step.args)?;

    // Handle predicate (for where/filter operations)
    if let Some(pred) = &step.predicate {
        // Parse simple predicates like "size > 1000"
        let lambda = predicate_to_lambda(pred);
        code = format!("{}({})", step.builtin, lambda);
    }

    // Handle field selection
    if let Some(field) = &step.select {
        code = format!("{} | map(fn(x) => x.{})", code, field);
    }

    Ok(code)
}

/// Convert a simple predicate to a lambda
fn predicate_to_lambda(predicate: &str) -> String {
    // Handle common patterns
    if predicate.contains(">")
        || predicate.contains("<")
        || predicate.contains("==")
        || predicate.contains("!=")
    {
        format!("fn(x) => x.{}", predicate)
    } else if predicate.starts_with('.') {
        format!("fn(x) => x{}", predicate)
    } else {
        format!("fn(x) => {}", predicate)
    }
}

/// Convert JSON value to AetherShell literal syntax
fn json_to_ae_literal(val: &JsonValue) -> String {
    match val {
        JsonValue::Null => "None".to_string(),
        JsonValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        JsonValue::Array(arr) => {
            let items: Vec<String> = arr.iter().map(json_to_ae_literal).collect();
            format!("[{}]", items.join(", "))
        }
        JsonValue::Object(obj) => {
            let fields: Vec<String> = obj
                .iter()
                .map(|(k, v)| format!("{}: {}", k, json_to_ae_literal(v)))
                .collect();
            format!("{{{}}}", fields.join(", "))
        }
    }
}

/// Convert AetherShell Value to JSON
fn value_to_json(value: &Value) -> JsonValue {
    match value {
        Value::Null => JsonValue::Null,
        Value::Bool(b) => json!(*b),
        Value::Int(i) => json!(*i),
        Value::Float(f) => json!(*f),
        Value::Str(s) => json!(s),
        Value::Uri(u) => json!({"_type": "Uri", "value": u}),
        Value::Array(arr) => {
            let items: Vec<JsonValue> = arr.iter().map(value_to_json).collect();
            JsonValue::Array(items)
        }
        Value::Record(fields) => {
            let obj: serde_json::Map<String, JsonValue> = fields
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            JsonValue::Object(obj)
        }
        Value::Table(table) => {
            json!({
                "_type": "Table",
                "schema": table.schema,
                "rows": table.rows.iter().map(|row| {
                    let obj: serde_json::Map<String, JsonValue> = row
                        .iter()
                        .map(|(k, v)| (k.clone(), value_to_json(v)))
                        .collect();
                    JsonValue::Object(obj)
                }).collect::<Vec<_>>()
            })
        }
        Value::Lambda(lambda) => {
            json!({
                "_type": "Lambda",
                "params": lambda.params,
                "body": format!("{:?}", lambda.body)
            })
        }
        Value::AsyncLambda(lambda) => {
            json!({
                "_type": "AsyncLambda",
                "params": lambda.params,
                "body": format!("{:?}", lambda.body)
            })
        }
        Value::Future(future) => {
            json!({
                "_type": "Future",
                "args": future.args.iter().map(value_to_json).collect::<Vec<_>>()
            })
        }
        Value::Error(e) => json!({
            "_type": "Error",
            "message": e
        }),
    }
}

/// Get the type name for a Value
fn value_type_name(value: &Value) -> String {
    value.type_name().to_string()
}

// ============================================================================
// Schema Builders
// ============================================================================

/// Build the complete language ontology
pub fn build_language_ontology() -> LanguageOntology {
    LanguageOntology {
        language: LanguageInfo {
            name: "AetherShell".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: "A typed functional shell with multi-modal AI capabilities".to_string(),
            paradigms: vec![
                "Functional".to_string(),
                "Pipeline-oriented".to_string(),
                "Expression-based".to_string(),
            ],
            typing: "Hindley-Milner type inference".to_string(),
        },
        types: get_type_definitions(),
        builtins: get_builtin_definitions(),
        operators: get_operator_definitions(),
        syntax: SyntaxPatterns {
            variable_declaration: "let <name> = <expr>".to_string(),
            function_definition: "fn <name>(<params>) => <expr>".to_string(),
            lambda: "fn(<params>) => <expr>".to_string(),
            pipeline: "<expr> | <func> | <func>".to_string(),
            record_literal: "{ field1: value1, field2: value2 }".to_string(),
            array_literal: "[item1, item2, item3]".to_string(),
            conditional: "if <cond> then <expr> else <expr>".to_string(),
            pattern_matching: "match <expr> { pattern => result, ... }".to_string(),
        },
        categories: get_category_info(),
    }
}

/// Get type definitions
fn get_type_definitions() -> Vec<TypeDefinition> {
    vec![
        TypeDefinition {
            name: "Int".to_string(),
            description: "64-bit signed integer".to_string(),
            json_equivalent: "number (integer)".to_string(),
            examples: vec!["42".to_string(), "-17".to_string(), "0".to_string()],
            fields: None,
        },
        TypeDefinition {
            name: "Float".to_string(),
            description: "64-bit floating point number".to_string(),
            json_equivalent: "number".to_string(),
            examples: vec!["3.14".to_string(), "-0.5".to_string(), "1e10".to_string()],
            fields: None,
        },
        TypeDefinition {
            name: "String".to_string(),
            description: "UTF-8 text string".to_string(),
            json_equivalent: "string".to_string(),
            examples: vec!["\"hello\"".to_string(), "\"multi\\nline\"".to_string()],
            fields: None,
        },
        TypeDefinition {
            name: "Bool".to_string(),
            description: "Boolean true/false".to_string(),
            json_equivalent: "boolean".to_string(),
            examples: vec!["true".to_string(), "false".to_string()],
            fields: None,
        },
        TypeDefinition {
            name: "Array".to_string(),
            description: "Ordered collection of values".to_string(),
            json_equivalent: "array".to_string(),
            examples: vec!["[1, 2, 3]".to_string(), "[\"a\", \"b\"]".to_string()],
            fields: None,
        },
        TypeDefinition {
            name: "Record".to_string(),
            description: "Key-value mapping (like JSON object)".to_string(),
            json_equivalent: "object".to_string(),
            examples: vec!["{name: \"file.txt\", size: 1024}".to_string()],
            fields: None,
        },
        TypeDefinition {
            name: "Table".to_string(),
            description: "Tabular data with headers and rows".to_string(),
            json_equivalent: "object with headers and rows arrays".to_string(),
            examples: vec!["Result of ls(), csv parsing".to_string()],
            fields: Some(vec![
                FieldDefinition {
                    name: "headers".to_string(),
                    field_type: "Array<String>".to_string(),
                    description: "Column names".to_string(),
                },
                FieldDefinition {
                    name: "rows".to_string(),
                    field_type: "Array<Array<Value>>".to_string(),
                    description: "Row data".to_string(),
                },
            ]),
        },
        TypeDefinition {
            name: "Lambda".to_string(),
            description: "Anonymous function".to_string(),
            json_equivalent: "object with params and body".to_string(),
            examples: vec![
                "fn(x) => x * 2".to_string(),
                "fn(a, b) => a + b".to_string(),
            ],
            fields: None,
        },
        TypeDefinition {
            name: "Option".to_string(),
            description: "Optional value (Some or None)".to_string(),
            json_equivalent: "value or null".to_string(),
            examples: vec!["Some(42)".to_string(), "None".to_string()],
            fields: None,
        },
    ]
}

/// Get builtin definitions from documentation
fn get_builtin_definitions() -> Vec<BuiltinDefinition> {
    // Core builtins with full definitions
    let mut builtins = vec![
        // File System
        BuiltinDefinition {
            name: "ls".to_string(),
            description: "List directory contents".to_string(),
            category: "FileSystem".to_string(),
            signature: "ls(path?: String) -> Array<Record>".to_string(),
            parameters: vec![ParameterDefinition {
                name: "path".to_string(),
                param_type: "String".to_string(),
                description: "Directory path to list".to_string(),
                required: false,
                default: Some("\".\"".to_string()),
            }],
            return_type: "Array<Record>".to_string(),
            examples: vec![
                ExampleDefinition {
                    description: "List current directory".to_string(),
                    code: "ls()".to_string(),
                    result: Some("[{name: \"file.txt\", size: 1024, ...}, ...]".to_string()),
                },
                ExampleDefinition {
                    description: "List specific directory".to_string(),
                    code: "ls(\"/home\")".to_string(),
                    result: None,
                },
            ],
            aliases: Some(vec!["dir".to_string()]),
            json_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory path"}
                }
            }),
        },
        BuiltinDefinition {
            name: "cat".to_string(),
            description: "Read file contents".to_string(),
            category: "FileSystem".to_string(),
            signature: "cat(path: String) -> String".to_string(),
            parameters: vec![ParameterDefinition {
                name: "path".to_string(),
                param_type: "String".to_string(),
                description: "File path to read".to_string(),
                required: true,
                default: None,
            }],
            return_type: "String".to_string(),
            examples: vec![ExampleDefinition {
                description: "Read a file".to_string(),
                code: "cat(\"README.md\")".to_string(),
                result: None,
            }],
            aliases: Some(vec!["read".to_string(), "read_text".to_string()]),
            json_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path to read"}
                },
                "required": ["path"]
            }),
        },
        BuiltinDefinition {
            name: "pwd".to_string(),
            description: "Get current working directory".to_string(),
            category: "FileSystem".to_string(),
            signature: "pwd() -> String".to_string(),
            parameters: vec![],
            return_type: "String".to_string(),
            examples: vec![ExampleDefinition {
                description: "Get current directory".to_string(),
                code: "pwd()".to_string(),
                result: Some("\"/home/user\"".to_string()),
            }],
            aliases: None,
            json_schema: json!({"type": "object", "properties": {}}),
        },
        BuiltinDefinition {
            name: "cd".to_string(),
            description: "Change current directory".to_string(),
            category: "FileSystem".to_string(),
            signature: "cd(path: String) -> String".to_string(),
            parameters: vec![ParameterDefinition {
                name: "path".to_string(),
                param_type: "String".to_string(),
                description: "Directory to change to".to_string(),
                required: true,
                default: None,
            }],
            return_type: "String".to_string(),
            examples: vec![ExampleDefinition {
                description: "Change to home directory".to_string(),
                code: "cd(\"~\")".to_string(),
                result: None,
            }],
            aliases: None,
            json_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory path"}
                },
                "required": ["path"]
            }),
        },
        // Functional
        BuiltinDefinition {
            name: "map".to_string(),
            description: "Transform each element of an array".to_string(),
            category: "Functional".to_string(),
            signature: "map(array: Array, fn: Lambda) -> Array".to_string(),
            parameters: vec![
                ParameterDefinition {
                    name: "array".to_string(),
                    param_type: "Array".to_string(),
                    description: "Input array (or piped input)".to_string(),
                    required: true,
                    default: None,
                },
                ParameterDefinition {
                    name: "fn".to_string(),
                    param_type: "Lambda".to_string(),
                    description: "Transformation function".to_string(),
                    required: true,
                    default: None,
                },
            ],
            return_type: "Array".to_string(),
            examples: vec![
                ExampleDefinition {
                    description: "Double each number".to_string(),
                    code: "[1, 2, 3] | map(fn(x) => x * 2)".to_string(),
                    result: Some("[2, 4, 6]".to_string()),
                },
                ExampleDefinition {
                    description: "Extract field from records".to_string(),
                    code: "ls() | map(fn(f) => f.name)".to_string(),
                    result: None,
                },
            ],
            aliases: None,
            json_schema: json!({
                "type": "object",
                "properties": {
                    "input": {"type": "array", "description": "Array to transform"},
                    "field": {"type": "string", "description": "Field to extract (shorthand for map)"}
                }
            }),
        },
        BuiltinDefinition {
            name: "where".to_string(),
            description: "Filter array elements by predicate".to_string(),
            category: "Functional".to_string(),
            signature: "where(array: Array, predicate: Lambda) -> Array".to_string(),
            parameters: vec![
                ParameterDefinition {
                    name: "array".to_string(),
                    param_type: "Array".to_string(),
                    description: "Input array".to_string(),
                    required: true,
                    default: None,
                },
                ParameterDefinition {
                    name: "predicate".to_string(),
                    param_type: "Lambda".to_string(),
                    description: "Filter function returning Bool".to_string(),
                    required: true,
                    default: None,
                },
            ],
            return_type: "Array".to_string(),
            examples: vec![ExampleDefinition {
                description: "Filter large files".to_string(),
                code: "ls() | where(fn(f) => f.size > 1000)".to_string(),
                result: None,
            }],
            aliases: Some(vec!["filter".to_string()]),
            json_schema: json!({
                "type": "object",
                "properties": {
                    "input": {"type": "array"},
                    "predicate": {"type": "string", "description": "Filter condition like 'size > 1000'"}
                }
            }),
        },
        BuiltinDefinition {
            name: "reduce".to_string(),
            description: "Reduce array to single value".to_string(),
            category: "Functional".to_string(),
            signature: "reduce(array: Array, fn: Lambda, init: Value) -> Value".to_string(),
            parameters: vec![
                ParameterDefinition {
                    name: "array".to_string(),
                    param_type: "Array".to_string(),
                    description: "Input array".to_string(),
                    required: true,
                    default: None,
                },
                ParameterDefinition {
                    name: "fn".to_string(),
                    param_type: "Lambda".to_string(),
                    description: "Reducer function (acc, item) => new_acc".to_string(),
                    required: true,
                    default: None,
                },
                ParameterDefinition {
                    name: "init".to_string(),
                    param_type: "Value".to_string(),
                    description: "Initial accumulator value".to_string(),
                    required: true,
                    default: None,
                },
            ],
            return_type: "Value".to_string(),
            examples: vec![ExampleDefinition {
                description: "Sum numbers".to_string(),
                code: "[1, 2, 3] | reduce(fn(a, b) => a + b, 0)".to_string(),
                result: Some("6".to_string()),
            }],
            aliases: Some(vec!["fold".to_string()]),
            json_schema: json!({
                "type": "object",
                "properties": {
                    "input": {"type": "array"},
                    "operation": {"type": "string", "enum": ["sum", "product", "concat", "custom"]},
                    "initial": {"description": "Initial value"}
                }
            }),
        },
        // AI
        BuiltinDefinition {
            name: "ai".to_string(),
            description: "Query AI model with a prompt".to_string(),
            category: "AI".to_string(),
            signature: "ai(prompt: String, options?: Record) -> String".to_string(),
            parameters: vec![
                ParameterDefinition {
                    name: "prompt".to_string(),
                    param_type: "String".to_string(),
                    description: "The prompt to send to the AI".to_string(),
                    required: true,
                    default: None,
                },
                ParameterDefinition {
                    name: "options".to_string(),
                    param_type: "Record".to_string(),
                    description: "Options like model, temperature".to_string(),
                    required: false,
                    default: None,
                },
            ],
            return_type: "String".to_string(),
            examples: vec![ExampleDefinition {
                description: "Ask AI a question".to_string(),
                code: "ai(\"Explain recursion\")".to_string(),
                result: None,
            }],
            aliases: Some(vec!["ask".to_string()]),
            json_schema: json!({
                "type": "object",
                "properties": {
                    "prompt": {"type": "string", "description": "AI prompt"},
                    "model": {"type": "string", "description": "Model to use"},
                    "temperature": {"type": "number", "minimum": 0, "maximum": 2}
                },
                "required": ["prompt"]
            }),
        },
        BuiltinDefinition {
            name: "agent".to_string(),
            description: "Create an autonomous AI agent with tool access".to_string(),
            category: "AI".to_string(),
            signature: "agent(goal: String, options?: Record) -> Value".to_string(),
            parameters: vec![
                ParameterDefinition {
                    name: "goal".to_string(),
                    param_type: "String".to_string(),
                    description: "The goal for the agent to accomplish".to_string(),
                    required: true,
                    default: None,
                },
                ParameterDefinition {
                    name: "options".to_string(),
                    param_type: "Record".to_string(),
                    description: "Options like tools, max_steps, dry_run".to_string(),
                    required: false,
                    default: None,
                },
            ],
            return_type: "Value".to_string(),
            examples: vec![ExampleDefinition {
                description: "Create an agent to analyze files".to_string(),
                code: "agent(\"Find large log files\", {tools: [\"ls\", \"cat\"]})".to_string(),
                result: None,
            }],
            aliases: None,
            json_schema: json!({
                "type": "object",
                "properties": {
                    "goal": {"type": "string", "description": "Agent's goal"},
                    "tools": {"type": "array", "items": {"type": "string"}},
                    "max_steps": {"type": "integer", "default": 10},
                    "dry_run": {"type": "boolean", "default": false}
                },
                "required": ["goal"]
            }),
        },
        // Aggregation
        BuiltinDefinition {
            name: "sum".to_string(),
            description: "Sum numeric values in an array".to_string(),
            category: "Aggregation".to_string(),
            signature: "sum(array: Array<Number>) -> Number".to_string(),
            parameters: vec![ParameterDefinition {
                name: "array".to_string(),
                param_type: "Array<Number>".to_string(),
                description: "Array of numbers".to_string(),
                required: true,
                default: None,
            }],
            return_type: "Number".to_string(),
            examples: vec![ExampleDefinition {
                description: "Sum numbers".to_string(),
                code: "[1, 2, 3, 4, 5] | sum()".to_string(),
                result: Some("15".to_string()),
            }],
            aliases: None,
            json_schema: json!({
                "type": "object",
                "properties": {
                    "input": {"type": "array", "items": {"type": "number"}}
                }
            }),
        },
        BuiltinDefinition {
            name: "avg".to_string(),
            description: "Calculate average of numeric values".to_string(),
            category: "Aggregation".to_string(),
            signature: "avg(array: Array<Number>) -> Float".to_string(),
            parameters: vec![ParameterDefinition {
                name: "array".to_string(),
                param_type: "Array<Number>".to_string(),
                description: "Array of numbers".to_string(),
                required: true,
                default: None,
            }],
            return_type: "Float".to_string(),
            examples: vec![ExampleDefinition {
                description: "Average of numbers".to_string(),
                code: "[1, 2, 3, 4, 5] | avg()".to_string(),
                result: Some("3.0".to_string()),
            }],
            aliases: Some(vec!["mean".to_string()]),
            json_schema: json!({
                "type": "object",
                "properties": {
                    "input": {"type": "array", "items": {"type": "number"}}
                }
            }),
        },
        BuiltinDefinition {
            name: "len".to_string(),
            description: "Get length of array or string".to_string(),
            category: "Core".to_string(),
            signature: "len(value: Array | String) -> Int".to_string(),
            parameters: vec![ParameterDefinition {
                name: "value".to_string(),
                param_type: "Array | String".to_string(),
                description: "Array or string to measure".to_string(),
                required: true,
                default: None,
            }],
            return_type: "Int".to_string(),
            examples: vec![
                ExampleDefinition {
                    description: "Array length".to_string(),
                    code: "len([1, 2, 3])".to_string(),
                    result: Some("3".to_string()),
                },
                ExampleDefinition {
                    description: "String length".to_string(),
                    code: "len(\"hello\")".to_string(),
                    result: Some("5".to_string()),
                },
            ],
            aliases: Some(vec!["length".to_string(), "count".to_string()]),
            json_schema: json!({
                "type": "object",
                "properties": {
                    "input": {"description": "Array or string"}
                }
            }),
        },
        // String operations
        BuiltinDefinition {
            name: "split".to_string(),
            description: "Split string into array".to_string(),
            category: "String".to_string(),
            signature: "split(str: String, delimiter: String) -> Array<String>".to_string(),
            parameters: vec![
                ParameterDefinition {
                    name: "str".to_string(),
                    param_type: "String".to_string(),
                    description: "String to split".to_string(),
                    required: true,
                    default: None,
                },
                ParameterDefinition {
                    name: "delimiter".to_string(),
                    param_type: "String".to_string(),
                    description: "Delimiter to split on".to_string(),
                    required: true,
                    default: None,
                },
            ],
            return_type: "Array<String>".to_string(),
            examples: vec![ExampleDefinition {
                description: "Split CSV line".to_string(),
                code: "\"a,b,c\" | split(\",\")".to_string(),
                result: Some("[\"a\", \"b\", \"c\"]".to_string()),
            }],
            aliases: None,
            json_schema: json!({
                "type": "object",
                "properties": {
                    "input": {"type": "string"},
                    "delimiter": {"type": "string"}
                },
                "required": ["delimiter"]
            }),
        },
        BuiltinDefinition {
            name: "join".to_string(),
            description: "Join array elements into string".to_string(),
            category: "String".to_string(),
            signature: "join(array: Array, delimiter: String) -> String".to_string(),
            parameters: vec![
                ParameterDefinition {
                    name: "array".to_string(),
                    param_type: "Array".to_string(),
                    description: "Array to join".to_string(),
                    required: true,
                    default: None,
                },
                ParameterDefinition {
                    name: "delimiter".to_string(),
                    param_type: "String".to_string(),
                    description: "Delimiter between elements".to_string(),
                    required: true,
                    default: None,
                },
            ],
            return_type: "String".to_string(),
            examples: vec![ExampleDefinition {
                description: "Join with comma".to_string(),
                code: "[\"a\", \"b\", \"c\"] | join(\", \")".to_string(),
                result: Some("\"a, b, c\"".to_string()),
            }],
            aliases: None,
            json_schema: json!({
                "type": "object",
                "properties": {
                    "input": {"type": "array"},
                    "delimiter": {"type": "string"}
                },
                "required": ["delimiter"]
            }),
        },
        // HTTP
        BuiltinDefinition {
            name: "http_get".to_string(),
            description: "Make HTTP GET request".to_string(),
            category: "Network".to_string(),
            signature: "http_get(url: String, options?: Record) -> Value".to_string(),
            parameters: vec![
                ParameterDefinition {
                    name: "url".to_string(),
                    param_type: "String".to_string(),
                    description: "URL to request".to_string(),
                    required: true,
                    default: None,
                },
                ParameterDefinition {
                    name: "options".to_string(),
                    param_type: "Record".to_string(),
                    description: "Headers, timeout, etc.".to_string(),
                    required: false,
                    default: None,
                },
            ],
            return_type: "Value".to_string(),
            examples: vec![ExampleDefinition {
                description: "Fetch JSON API".to_string(),
                code: "http_get(\"https://api.example.com/data\")".to_string(),
                result: None,
            }],
            aliases: Some(vec!["fetch".to_string(), "get".to_string()]),
            json_schema: json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "format": "uri"},
                    "headers": {"type": "object"}
                },
                "required": ["url"]
            }),
        },
        // MCP Tools
        BuiltinDefinition {
            name: "mcp_tools".to_string(),
            description: "List available MCP tools".to_string(),
            category: "MCP".to_string(),
            signature: "mcp_tools(options?: Record) -> Array<Record>".to_string(),
            parameters: vec![ParameterDefinition {
                name: "options".to_string(),
                param_type: "Record".to_string(),
                description: "Filter options like category".to_string(),
                required: false,
                default: None,
            }],
            return_type: "Array<Record>".to_string(),
            examples: vec![ExampleDefinition {
                description: "List all tools".to_string(),
                code: "mcp_tools()".to_string(),
                result: None,
            }],
            aliases: None,
            json_schema: json!({
                "type": "object",
                "properties": {
                    "category": {"type": "string"}
                }
            }),
        },
        BuiltinDefinition {
            name: "mcp_call".to_string(),
            description: "Execute an MCP tool".to_string(),
            category: "MCP".to_string(),
            signature: "mcp_call(tool: String, args: Record) -> Value".to_string(),
            parameters: vec![
                ParameterDefinition {
                    name: "tool".to_string(),
                    param_type: "String".to_string(),
                    description: "Tool name".to_string(),
                    required: true,
                    default: None,
                },
                ParameterDefinition {
                    name: "args".to_string(),
                    param_type: "Record".to_string(),
                    description: "Tool arguments".to_string(),
                    required: true,
                    default: None,
                },
            ],
            return_type: "Value".to_string(),
            examples: vec![ExampleDefinition {
                description: "Call git tool".to_string(),
                code: "mcp_call(\"git\", {command: \"status\"})".to_string(),
                result: None,
            }],
            aliases: None,
            json_schema: json!({
                "type": "object",
                "properties": {
                    "tool": {"type": "string"},
                    "args": {"type": "object"}
                },
                "required": ["tool", "args"]
            }),
        },
    ];

    // More builtins can be added here or discovered dynamically

    builtins
}

/// Categorize a builtin by name
fn categorize_builtin(name: &str) -> String {
    match name {
        n if n.starts_with("mcp_") => "MCP".to_string(),
        n if n.starts_with("ai") || n.starts_with("agent") || n.starts_with("swarm") => {
            "AI".to_string()
        }
        n if n.starts_with("http") || n.starts_with("fetch") => "Network".to_string(),
        n if n.starts_with("nn_") || n.starts_with("rl_") => "ML".to_string(),
        n if n.starts_with("kg_") || n.starts_with("rag_") => "Knowledge".to_string(),
        n if [
            "ls",
            "cat",
            "pwd",
            "cd",
            "mkdir",
            "rm",
            "exists",
            "read_text",
            "write",
        ]
        .contains(&n) =>
        {
            "FileSystem".to_string()
        }
        n if [
            "map", "where", "reduce", "filter", "each", "any", "all", "take", "first", "last",
        ]
        .contains(&n) =>
        {
            "Functional".to_string()
        }
        n if ["sum", "avg", "mean", "min", "max", "count", "product"].contains(&n) => {
            "Aggregation".to_string()
        }
        n if [
            "split", "join", "trim", "upper", "lower", "replace", "contains",
        ]
        .contains(&n) =>
        {
            "String".to_string()
        }
        _ => "Core".to_string(),
    }
}

/// Get operator definitions
fn get_operator_definitions() -> Vec<OperatorDefinition> {
    vec![
        OperatorDefinition {
            symbol: "|".to_string(),
            name: "pipe".to_string(),
            description: "Pipeline operator - pass left result to right function".to_string(),
            precedence: 1,
            associativity: "left".to_string(),
            operand_types: vec!["Value".to_string(), "Function".to_string()],
        },
        OperatorDefinition {
            symbol: "+".to_string(),
            name: "add".to_string(),
            description: "Addition or string concatenation".to_string(),
            precedence: 5,
            associativity: "left".to_string(),
            operand_types: vec!["Number".to_string(), "String".to_string()],
        },
        OperatorDefinition {
            symbol: "-".to_string(),
            name: "subtract".to_string(),
            description: "Subtraction".to_string(),
            precedence: 5,
            associativity: "left".to_string(),
            operand_types: vec!["Number".to_string()],
        },
        OperatorDefinition {
            symbol: "*".to_string(),
            name: "multiply".to_string(),
            description: "Multiplication".to_string(),
            precedence: 6,
            associativity: "left".to_string(),
            operand_types: vec!["Number".to_string()],
        },
        OperatorDefinition {
            symbol: "/".to_string(),
            name: "divide".to_string(),
            description: "Division".to_string(),
            precedence: 6,
            associativity: "left".to_string(),
            operand_types: vec!["Number".to_string()],
        },
        OperatorDefinition {
            symbol: "==".to_string(),
            name: "equals".to_string(),
            description: "Equality comparison".to_string(),
            precedence: 3,
            associativity: "left".to_string(),
            operand_types: vec!["Value".to_string()],
        },
        OperatorDefinition {
            symbol: "!=".to_string(),
            name: "not_equals".to_string(),
            description: "Inequality comparison".to_string(),
            precedence: 3,
            associativity: "left".to_string(),
            operand_types: vec!["Value".to_string()],
        },
        OperatorDefinition {
            symbol: ">".to_string(),
            name: "greater_than".to_string(),
            description: "Greater than comparison".to_string(),
            precedence: 4,
            associativity: "left".to_string(),
            operand_types: vec!["Number".to_string()],
        },
        OperatorDefinition {
            symbol: "<".to_string(),
            name: "less_than".to_string(),
            description: "Less than comparison".to_string(),
            precedence: 4,
            associativity: "left".to_string(),
            operand_types: vec!["Number".to_string()],
        },
        OperatorDefinition {
            symbol: "&&".to_string(),
            name: "and".to_string(),
            description: "Logical AND".to_string(),
            precedence: 2,
            associativity: "left".to_string(),
            operand_types: vec!["Bool".to_string()],
        },
        OperatorDefinition {
            symbol: "||".to_string(),
            name: "or".to_string(),
            description: "Logical OR".to_string(),
            precedence: 2,
            associativity: "left".to_string(),
            operand_types: vec!["Bool".to_string()],
        },
        OperatorDefinition {
            symbol: ".".to_string(),
            name: "field_access".to_string(),
            description: "Access record field".to_string(),
            precedence: 10,
            associativity: "left".to_string(),
            operand_types: vec!["Record".to_string(), "String".to_string()],
        },
    ]
}

/// Get category information
fn get_category_info() -> Vec<CategoryInfo> {
    let builtins = get_builtin_definitions();
    let mut categories: HashMap<String, usize> = HashMap::new();

    for b in &builtins {
        *categories.entry(b.category.clone()).or_insert(0) += 1;
    }

    let descriptions: HashMap<&str, &str> = [
        ("FileSystem", "File and directory operations"),
        (
            "Functional",
            "Higher-order functions for data transformation",
        ),
        ("Aggregation", "Statistical and aggregation functions"),
        ("String", "String manipulation functions"),
        ("Network", "HTTP and network operations"),
        ("AI", "AI model queries and agent operations"),
        ("MCP", "Model Context Protocol tools"),
        ("ML", "Machine learning and neural network operations"),
        ("Knowledge", "Knowledge graphs and RAG operations"),
        ("Core", "Core language utilities"),
    ]
    .into_iter()
    .collect();

    categories
        .into_iter()
        .map(|(name, count)| CategoryInfo {
            description: descriptions.get(name.as_str()).unwrap_or(&"").to_string(),
            name,
            builtin_count: count,
        })
        .collect()
}

// ============================================================================
// AI Provider Schema Formats
// ============================================================================

/// Build OpenAI function calling schema
fn build_openai_schema(ontology: &LanguageOntology) -> JsonValue {
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": format!("aethershell_{}", b.name),
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "openai_function_calling",
        "version": "2024-01",
        "tools": tools,
        "instructions": "Use these tools to execute AetherShell operations. Each tool corresponds to an AetherShell builtin function."
    })
}

/// Build Anthropic Claude tool use schema
fn build_claude_schema(ontology: &LanguageOntology) -> JsonValue {
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "name": format!("aethershell_{}", b.name),
                "description": b.description,
                "input_schema": b.json_schema
            })
        })
        .collect();

    json!({
        "format": "anthropic_tool_use",
        "version": "2024-01",
        "tools": tools,
        "instructions": "Use these tools to execute AetherShell operations. Results are returned as JSON."
    })
}

/// Build Google Gemini function declaration schema
fn build_gemini_schema(ontology: &LanguageOntology) -> JsonValue {
    let function_declarations: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "name": format!("aethershell_{}", b.name),
                "description": b.description,
                "parameters": b.json_schema
            })
        })
        .collect();

    json!({
        "format": "gemini_function_calling",
        "version": "v1",
        "function_declarations": function_declarations,
        "instructions": "Use these functions to execute AetherShell operations."
    })
}

/// Build compact ontology for context efficiency
fn build_compact_ontology(ontology: &LanguageOntology) -> JsonValue {
    // Compact format optimized for LLM context windows
    let builtins_compact: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "n": b.name,
                "d": b.description,
                "s": b.signature,
                "c": b.category
            })
        })
        .collect();

    json!({
        "lang": "AetherShell",
        "ver": ontology.language.version,
        "types": ["Int", "Float", "String", "Bool", "Array", "Record", "Lambda", "Table", "Option"],
        "ops": ["|", "+", "-", "*", "/", "==", "!=", ">", "<", ">=", "<=", "&&", "||", "."],
        "syntax": {
            "let": "let x = expr",
            "fn": "fn(x) => expr",
            "pipe": "a | b | c",
            "rec": "{k: v}",
            "arr": "[a, b]",
            "if": "if cond then a else b",
            "match": "match x { p => r }"
        },
        "builtins": builtins_compact
    })
}

// ============================================================================
// HTTP Server Integration
// ============================================================================

#[cfg(feature = "native")]
pub mod server {
    use super::*;
    use axum::{
        extract::State,
        http::StatusCode,
        response::IntoResponse,
        routing::{get, post},
        Json, Router,
    };
    use std::sync::Arc;
    use tower_http::cors::{Any, CorsLayer};

    /// Agent API server configuration
    #[derive(Debug, Clone)]
    pub struct AgentApiConfig {
        pub host: String,
        pub port: u16,
        pub enable_cors: bool,
    }

    impl Default for AgentApiConfig {
        fn default() -> Self {
            Self {
                host: "127.0.0.1".to_string(),
                port: 3002,
                enable_cors: true,
            }
        }
    }

    /// Start the Agent API HTTP server
    pub async fn start_agent_api_server(config: AgentApiConfig) -> Result<()> {
        let mut app = Router::new()
            // Main execution endpoint
            .route("/api/v1/execute", post(handle_execute))
            // Convenience endpoints
            .route("/api/v1/call/:builtin", post(handle_call))
            .route("/api/v1/pipeline", post(handle_pipeline))
            .route("/api/v1/eval", post(handle_eval))
            // Discovery endpoints
            .route("/api/v1/schema", get(handle_schema))
            .route("/api/v1/schema/:format", get(handle_schema_format))
            .route("/api/v1/builtins", get(handle_list_builtins))
            .route("/api/v1/builtins/:name", get(handle_describe_builtin))
            .route("/api/v1/types", get(handle_types))
            // Health check
            .route("/health", get(handle_health));

        if config.enable_cors {
            app = app.layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            );
        }

        let addr: std::net::SocketAddr =
            format!("{}:{}", config.host, config.port).parse().unwrap();

        println!("🤖 AetherShell Agent API starting on http://{}", addr);
        println!("   For AI agents: ChatGPT, Claude, Gemini supported");
        println!();
        println!("Endpoints:");
        println!("  POST /api/v1/execute          - Execute any request");
        println!("  POST /api/v1/call/:builtin    - Call a single builtin");
        println!("  POST /api/v1/pipeline         - Execute a pipeline");
        println!("  POST /api/v1/eval             - Evaluate raw code");
        println!("  GET  /api/v1/schema           - Get language ontology");
        println!("  GET  /api/v1/schema/:format   - Get schema (openai/claude/gemini)");
        println!("  GET  /api/v1/builtins         - List all builtins");
        println!("  GET  /api/v1/builtins/:name   - Describe a builtin");
        println!("  GET  /api/v1/types            - Get type information");

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    async fn handle_execute(Json(request): Json<AgentRequest>) -> impl IntoResponse {
        let response = process_request(&request);
        if response.success {
            (StatusCode::OK, Json(response))
        } else {
            (StatusCode::BAD_REQUEST, Json(response))
        }
    }

    async fn handle_call(
        axum::extract::Path(builtin): axum::extract::Path<String>,
        Json(args): Json<JsonValue>,
    ) -> impl IntoResponse {
        let request = AgentRequest::Call { builtin, args };
        let response = process_request(&request);
        if response.success {
            (StatusCode::OK, Json(response))
        } else {
            (StatusCode::BAD_REQUEST, Json(response))
        }
    }

    async fn handle_pipeline(Json(body): Json<JsonValue>) -> impl IntoResponse {
        let steps: Vec<PipelineStep> = serde_json::from_value(
            body.get("steps")
                .cloned()
                .unwrap_or(JsonValue::Array(vec![])),
        )
        .unwrap_or_default();

        let input = body.get("input").cloned();

        let request = AgentRequest::Pipeline { steps, input };
        let response = process_request(&request);
        if response.success {
            (StatusCode::OK, Json(response))
        } else {
            (StatusCode::BAD_REQUEST, Json(response))
        }
    }

    async fn handle_eval(Json(body): Json<JsonValue>) -> impl IntoResponse {
        let code = body
            .get("code")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let request = AgentRequest::Eval { code };
        let response = process_request(&request);
        if response.success {
            (StatusCode::OK, Json(response))
        } else {
            (StatusCode::BAD_REQUEST, Json(response))
        }
    }

    async fn handle_schema() -> impl IntoResponse {
        let request = AgentRequest::Schema {
            format: SchemaFormat::Ontology,
        };
        Json(process_request(&request))
    }

    async fn handle_schema_format(
        axum::extract::Path(format): axum::extract::Path<String>,
    ) -> impl IntoResponse {
        let schema_format = match format.to_lowercase().as_str() {
            "openai" => SchemaFormat::OpenAI,
            "claude" | "anthropic" => SchemaFormat::Claude,
            "gemini" | "google" => SchemaFormat::Gemini,
            "json" | "jsonschema" => SchemaFormat::JsonSchema,
            _ => SchemaFormat::Ontology,
        };

        let request = AgentRequest::Schema {
            format: schema_format,
        };
        Json(process_request(&request))
    }

    async fn handle_list_builtins(
        axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
    ) -> impl IntoResponse {
        let category = params.get("category").cloned();
        let request = AgentRequest::ListBuiltins { category };
        Json(process_request(&request))
    }

    async fn handle_describe_builtin(
        axum::extract::Path(name): axum::extract::Path<String>,
    ) -> impl IntoResponse {
        let request = AgentRequest::Describe { builtin: name };
        let response = process_request(&request);
        if response.success {
            (StatusCode::OK, Json(response))
        } else {
            (StatusCode::NOT_FOUND, Json(response))
        }
    }

    async fn handle_types() -> impl IntoResponse {
        let request = AgentRequest::TypeInfo { type_name: None };
        Json(process_request(&request))
    }

    async fn handle_health() -> impl IntoResponse {
        Json(json!({
            "status": "healthy",
            "service": "aethershell-agent-api",
            "version": env!("CARGO_PKG_VERSION"),
            "supported_agents": ["openai", "claude", "gemini"]
        }))
    }
}

// ============================================================================
// CLI Integration
// ============================================================================

/// Process a JSON request from stdin (for CLI usage)
pub fn process_json_request(json_str: &str) -> Result<String> {
    let request: AgentRequest =
        serde_json::from_str(json_str).map_err(|e| anyhow!("Invalid JSON request: {}", e))?;

    let response = process_request(&request);
    Ok(serde_json::to_string_pretty(&response)?)
}

/// Generate schema in the specified format
pub fn generate_schema(format: &str) -> Result<String> {
    let schema_format = match format.to_lowercase().as_str() {
        "openai" => SchemaFormat::OpenAI,
        "claude" | "anthropic" => SchemaFormat::Claude,
        "gemini" | "google" => SchemaFormat::Gemini,
        "json" | "jsonschema" | "full" => SchemaFormat::JsonSchema,
        "compact" | "ontology" => SchemaFormat::Ontology,
        _ => return Err(anyhow!("Unknown schema format: {}", format)),
    };

    let request = AgentRequest::Schema {
        format: schema_format,
    };
    let response = process_request(&request);

    if response.success {
        Ok(serde_json::to_string_pretty(&response.result)?)
    } else {
        Err(anyhow!(
            "Failed to generate schema: {}",
            response.error.unwrap_or_default()
        ))
    }
}
