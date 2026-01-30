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
    /// OpenAI function calling format (GPT-4, GPT-4o, o1, o3)
    OpenAI,
    /// Anthropic Claude tool use format (Claude 3.5 Sonnet, Claude 3 Opus)
    Claude,
    /// Google Gemini function declaration format (Gemini 1.5, Gemini 2.0)
    Gemini,
    /// Meta Llama format (Llama 3.1, 3.2, 3.3 via Ollama/vLLM/Together)
    Llama,
    /// Mistral AI function calling format (Mistral Large, Codestral)
    Mistral,
    /// Cohere Command format (Command R+, Command R)
    Cohere,
    /// xAI Grok format (Grok-2)
    Grok,
    /// DeepSeek format (DeepSeek-V3, DeepSeek-R1)
    DeepSeek,
    /// Amazon Bedrock format (Claude, Llama, Titan via Bedrock)
    Bedrock,
    /// Azure OpenAI format (Azure-hosted OpenAI models)
    AzureOpenAI,
    /// Alibaba Qwen format (Qwen 2.5)
    Qwen,
    /// Ollama local format (any model via Ollama)
    Ollama,
    /// vLLM format (high-performance inference)
    VLLM,
    /// HuggingFace TGI format (Text Generation Inference)
    HuggingFace,
    /// OpenRouter format (unified API for multiple providers)
    OpenRouter,
    /// Moonshot AI Kimi format (Kimi, Moonshot)
    Kimi,
    /// 01.AI Yi format (Yi-Large, Yi-Lightning)
    Yi,
    /// Zhipu AI GLM format (ChatGLM, GLM-4)
    GLM,
    /// Reka AI format (Reka Core, Reka Flash)
    Reka,
    /// AI21 Labs format (Jamba, Jurassic)
    AI21,
    /// Perplexity AI format (sonar models)
    Perplexity,
    /// Together AI format (hosted inference)
    Together,
    /// Groq format (fast inference - distinct from xAI Grok)
    Groq,
    /// Fireworks AI format (fast inference)
    Fireworks,
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
        SchemaFormat::Llama => build_llama_schema(&ontology),
        SchemaFormat::Mistral => build_mistral_schema(&ontology),
        SchemaFormat::Cohere => build_cohere_schema(&ontology),
        SchemaFormat::Grok => build_grok_schema(&ontology),
        SchemaFormat::DeepSeek => build_deepseek_schema(&ontology),
        SchemaFormat::Bedrock => build_bedrock_schema(&ontology),
        SchemaFormat::AzureOpenAI => build_azure_openai_schema(&ontology),
        SchemaFormat::Qwen => build_qwen_schema(&ontology),
        SchemaFormat::Ollama => build_ollama_schema(&ontology),
        SchemaFormat::VLLM => build_vllm_schema(&ontology),
        SchemaFormat::HuggingFace => build_huggingface_schema(&ontology),
        SchemaFormat::OpenRouter => build_openrouter_schema(&ontology),
        SchemaFormat::Kimi => build_kimi_schema(&ontology),
        SchemaFormat::Yi => build_yi_schema(&ontology),
        SchemaFormat::GLM => build_glm_schema(&ontology),
        SchemaFormat::Reka => build_reka_schema(&ontology),
        SchemaFormat::AI21 => build_ai21_schema(&ontology),
        SchemaFormat::Perplexity => build_perplexity_schema(&ontology),
        SchemaFormat::Together => build_together_schema(&ontology),
        SchemaFormat::Groq => build_groq_schema(&ontology),
        SchemaFormat::Fireworks => build_fireworks_schema(&ontology),
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
        "version": "2025-01",
        "compatible_models": [
            "gpt-5", "gpt-5-mini", "gpt-5-turbo",
            "gpt-4o", "gpt-4o-mini", "gpt-4-turbo",
            "o1", "o1-mini", "o1-preview",
            "o3", "o3-mini", "o4-mini"
        ],
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
        "version": "2025-01",
        "compatible_models": [
            "claude-4.5-opus", "claude-4.5-sonnet", "claude-4.5-haiku",
            "claude-4-opus", "claude-4-sonnet", "claude-4-haiku",
            "claude-3.5-sonnet-v2", "claude-3.5-haiku",
            "claude-3-opus", "claude-3-sonnet", "claude-3-haiku"
        ],
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
        "version": "v2",
        "compatible_models": [
            "gemini-2.5-pro", "gemini-2.5-flash",
            "gemini-2.0-flash", "gemini-2.0-flash-thinking",
            "gemini-1.5-pro", "gemini-1.5-flash"
        ],
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

/// Build Meta Llama function calling schema
fn build_llama_schema(ontology: &LanguageOntology) -> JsonValue {
    // Llama 3.1+ uses a tool format similar to OpenAI but with slight differences
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
        "format": "llama_function_calling",
        "version": "4.0",
        "compatible_models": [
            "llama-4-maverick-17b", "llama-4-maverick-70b", "llama-4-maverick-405b",
            "llama-4-scout-17b", "llama-4-scout-109b",
            "llama-3.3-70b", "llama-3.2-90b-vision", "llama-3.2-11b-vision",
            "llama-3.1-405b", "llama-3.1-70b", "llama-3.1-8b"
        ],
        "tools": tools,
        "system_prompt": "You are an assistant with access to AetherShell tools. Use them to help users with shell operations, data processing, and AI tasks.",
        "instructions": "Call tools using JSON format. Tool names are prefixed with 'aethershell_'."
    })
}

/// Build Mistral AI function calling schema  
fn build_mistral_schema(ontology: &LanguageOntology) -> JsonValue {
    // Mistral uses OpenAI-compatible tool format
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "mistral_function_calling",
        "version": "v2",
        "compatible_models": [
            "mistral-large-2501", "mistral-large-latest",
            "mistral-medium-2501", "mistral-small-2501",
            "codestral-2501", "codestral-latest",
            "pixtral-large-2501", "pixtral-12b",
            "ministral-8b", "ministral-3b"
        ],
        "tools": tools,
        "tool_choice": "auto",
        "instructions": "Use these tools to execute AetherShell operations. Results are returned as JSON."
    })
}

/// Build Cohere Command R function calling schema
fn build_cohere_schema(ontology: &LanguageOntology) -> JsonValue {
    // Cohere uses a distinct tool format with parameter_definitions
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            // Convert JSON schema to Cohere's parameter_definitions format
            let params = b.json_schema.get("properties")
                .and_then(|p| p.as_object())
                .map(|props| {
                    props.iter().map(|(name, schema)| {
                        json!({
                            "name": name,
                            "description": schema.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                            "type": schema.get("type").and_then(|t| t.as_str()).unwrap_or("string"),
                            "required": b.json_schema.get("required")
                                .and_then(|r| r.as_array())
                                .map(|arr| arr.iter().any(|v| v.as_str() == Some(name)))
                                .unwrap_or(false)
                        })
                    }).collect::<Vec<_>>()
                })
                .unwrap_or_default();

            json!({
                "name": b.name,
                "description": b.description,
                "parameter_definitions": params
            })
        })
        .collect();

    json!({
        "format": "cohere_tools",
        "version": "v3",
        "compatible_models": [
            "command-a", "command-a-03-2025",
            "command-r-plus", "command-r-plus-08-2024",
            "command-r", "command-r7b",
            "command-nightly"
        ],
        "tools": tools,
        "instructions": "Use these tools to execute AetherShell operations. Call tools by name with appropriate parameters."
    })
}

/// Build xAI Grok function calling schema
fn build_grok_schema(ontology: &LanguageOntology) -> JsonValue {
    // Grok uses OpenAI-compatible format
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "grok_function_calling",
        "version": "v2",
        "compatible_models": [
            "grok-3", "grok-3-mini", "grok-3-fast",
            "grok-2", "grok-2-mini", "grok-2-vision",
            "grok-2-1212"
        ],
        "tools": tools,
        "instructions": "Use these tools to execute AetherShell operations. Results are returned as JSON."
    })
}

/// Build DeepSeek function calling schema
fn build_deepseek_schema(ontology: &LanguageOntology) -> JsonValue {
    // DeepSeek uses OpenAI-compatible format with some enhancements
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "deepseek_function_calling",
        "version": "v4",
        "compatible_models": [
            "deepseek-chat", "deepseek-reasoner",
            "deepseek-v3", "deepseek-v3-0324",
            "deepseek-r1", "deepseek-r1-0528",
            "deepseek-r1-distill-llama-70b", "deepseek-r1-distill-qwen-32b",
            "deepseek-coder-v2", "deepseek-coder-v2.5"
        ],
        "tools": tools,
        "reasoning_support": true,
        "instructions": "Use these tools to execute AetherShell operations. For complex tasks, use chain-of-thought reasoning."
    })
}

/// Build AWS Bedrock converse API schema
fn build_bedrock_schema(ontology: &LanguageOntology) -> JsonValue {
    // Bedrock Converse API tool format
    let tool_config: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "toolSpec": {
                    "name": b.name,
                    "description": b.description,
                    "inputSchema": {
                        "json": b.json_schema
                    }
                }
            })
        })
        .collect();

    json!({
        "format": "bedrock_converse",
        "version": "v2",
        "compatible_models": [
            "anthropic.claude-4-5-opus-20250115-v1:0",
            "anthropic.claude-4-5-sonnet-20250115-v1:0",
            "anthropic.claude-4-5-haiku-20250115-v1:0",
            "anthropic.claude-3-5-sonnet-20241022-v2:0",
            "anthropic.claude-3-5-haiku-20241022-v1:0",
            "anthropic.claude-3-opus-20240229-v1:0",
            "amazon.nova-pro-v1:0",
            "amazon.nova-lite-v1:0",
            "amazon.nova-micro-v1:0",
            "meta.llama3-3-70b-instruct-v1:0",
            "meta.llama3-1-405b-instruct-v1:0",
            "mistral.mistral-large-2411-v1:0"
        ],
        "toolConfig": {
            "tools": tool_config
        },
        "instructions": "Use toolUse blocks in your response to invoke AetherShell tools."
    })
}

/// Build Azure OpenAI function calling schema
fn build_azure_openai_schema(ontology: &LanguageOntology) -> JsonValue {
    // Azure OpenAI uses same format as OpenAI
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "azure_openai_function_calling",
        "version": "2025-01-01-preview",
        "compatible_deployments": [
            "gpt-5", "gpt-5-mini", "gpt-5-turbo",
            "gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "gpt-4", "gpt-35-turbo",
            "o1", "o1-preview", "o1-mini", "o3-mini", "o4-mini"
        ],
        "tools": tools,
        "api_version": "2025-01-01-preview",
        "instructions": "Use these tools via Azure OpenAI API. Same format as OpenAI but with Azure endpoint."
    })
}

/// Build Alibaba Qwen function calling schema
fn build_qwen_schema(ontology: &LanguageOntology) -> JsonValue {
    // Qwen uses OpenAI-compatible format
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "qwen_function_calling",
        "version": "v3",
        "compatible_models": [
            "qwen3-235b-a22b", "qwen3-32b", "qwen3-14b", "qwen3-8b", "qwen3-4b",
            "qwen2.5-max", "qwen2.5-plus", "qwen2.5-turbo",
            "qwen2.5-72b-instruct", "qwen2.5-coder-32b-instruct",
            "qwq-32b", "qvq-72b-preview"
        ],
        "tools": tools,
        "instructions": "Use these tools to execute AetherShell operations. Compatible with DashScope API."
    })
}

/// Build Ollama local model schema
fn build_ollama_schema(ontology: &LanguageOntology) -> JsonValue {
    // Ollama tool format (similar to OpenAI)
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "ollama_tools",
        "version": "v0.5",
        "compatible_models": [
            "llama4:scout-17b", "llama4:maverick-17b",
            "llama3.3:70b", "llama3.2:3b", "llama3.1:8b", "llama3.1:70b",
            "qwen3:32b", "qwen3:8b", "qwen2.5:72b", "qwen2.5-coder:32b",
            "mistral-large:123b", "codestral:22b",
            "deepseek-r1:70b", "deepseek-r1:32b", "deepseek-r1:14b", "deepseek-r1:7b",
            "gemma3:27b", "phi4:14b", "command-a:111b"
        ],
        "tools": tools,
        "endpoint": "http://localhost:11434/api/chat",
        "instructions": "Use these tools with Ollama's chat API. Requires tool-capable model."
    })
}

/// Build vLLM OpenAI-compatible schema
fn build_vllm_schema(ontology: &LanguageOntology) -> JsonValue {
    // vLLM uses OpenAI-compatible format
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "vllm_openai_compatible",
        "version": "v1",
        "compatible_models": ["Any tool-capable model served via vLLM"],
        "tools": tools,
        "endpoint": "/v1/chat/completions",
        "tool_choice_modes": ["auto", "required", "none", {"type": "function", "function": {"name": "specific_tool"}}],
        "instructions": "Use OpenAI-compatible format with vLLM server. Ensure model supports function calling."
    })
}

/// Build HuggingFace Inference Endpoints schema
fn build_huggingface_schema(ontology: &LanguageOntology) -> JsonValue {
    // HuggingFace Text Generation Inference tool format
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "huggingface_tgi",
        "version": "v3",
        "compatible_endpoints": ["inference-endpoints", "text-generation-inference", "transformers"],
        "recommended_models": [
            "meta-llama/Llama-4-Maverick-17B-128E-Instruct",
            "meta-llama/Llama-3.3-70B-Instruct",
            "Qwen/Qwen3-235B-A22B",
            "Qwen/Qwen2.5-72B-Instruct",
            "mistralai/Mistral-Large-Instruct-2501",
            "deepseek-ai/DeepSeek-V3",
            "deepseek-ai/DeepSeek-R1",
            "google/gemma-3-27b-it"
        ],
        "tools": tools,
        "instructions": "Use with HuggingFace Inference Endpoints or local TGI deployment."
    })
}

/// Build OpenRouter unified schema
fn build_openrouter_schema(ontology: &LanguageOntology) -> JsonValue {
    // OpenRouter uses OpenAI-compatible format and routes to multiple providers
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "openrouter_unified",
        "version": "v2",
        "tool_capable_models": [
            "openai/gpt-5", "openai/gpt-5-mini", "openai/gpt-4o", "openai/gpt-4o-mini", "openai/o3", "openai/o4-mini",
            "anthropic/claude-4.5-opus", "anthropic/claude-4.5-sonnet", "anthropic/claude-4.5-haiku",
            "anthropic/claude-3.5-sonnet", "anthropic/claude-3-opus",
            "google/gemini-2.5-pro", "google/gemini-2.0-flash",
            "meta-llama/llama-4-maverick-70b", "meta-llama/llama-3.3-70b-instruct",
            "mistralai/mistral-large-2501", "mistralai/codestral-latest",
            "deepseek/deepseek-v3", "deepseek/deepseek-r1",
            "qwen/qwen-2.5-72b-instruct", "qwen/qwq-32b-preview",
            "x-ai/grok-3", "cohere/command-a"
        ],
        "tools": tools,
        "api_base": "https://openrouter.ai/api/v1",
        "instructions": "Use OpenAI-compatible format. OpenRouter will route to the specified model provider."
    })
}

/// Build Moonshot AI Kimi function calling schema
fn build_kimi_schema(ontology: &LanguageOntology) -> JsonValue {
    // Kimi uses OpenAI-compatible format
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "kimi_function_calling",
        "version": "v2",
        "compatible_models": [
            "kimi-latest", "kimi-k2",
            "moonshot-v1-128k", "moonshot-v1-32k", "moonshot-v1-8k",
            "moonshot-v2-128k"
        ],
        "tools": tools,
        "api_base": "https://api.moonshot.cn/v1",
        "instructions": "Use OpenAI-compatible format with Moonshot API. Kimi excels at long-context tasks."
    })
}

/// Build 01.AI Yi function calling schema
fn build_yi_schema(ontology: &LanguageOntology) -> JsonValue {
    // Yi uses OpenAI-compatible format
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "yi_function_calling",
        "version": "v2",
        "compatible_models": [
            "yi-large-fc", "yi-large-turbo", "yi-large-preview",
            "yi-lightning", "yi-lightning-lite",
            "yi-medium-200k", "yi-vision-v2",
            "yi-coder-9b", "yi-coder-1.5b"
        ],
        "tools": tools,
        "api_base": "https://api.01.ai/v1",
        "instructions": "Use OpenAI-compatible format with 01.AI API. Yi models support function calling."
    })
}

/// Build Zhipu AI GLM function calling schema
fn build_glm_schema(ontology: &LanguageOntology) -> JsonValue {
    // GLM/ChatGLM uses OpenAI-compatible format with some extensions
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "glm_function_calling",
        "version": "v5",
        "compatible_models": [
            "glm-5", "glm-5-plus", "glm-5-air",
            "glm-4-alltools", "glm-4-plus", "glm-4-air", "glm-4-airx", "glm-4-flash",
            "glm-4v-plus", "glm-4v-flash",
            "codegeex-4", "cogvideo-x"
        ],
        "tools": tools,
        "api_base": "https://open.bigmodel.cn/api/paas/v4",
        "instructions": "Use OpenAI-compatible format with Zhipu API. GLM-5 supports advanced tools and code generation."
    })
}

/// Build Reka AI function calling schema
fn build_reka_schema(ontology: &LanguageOntology) -> JsonValue {
    // Reka uses OpenAI-compatible format
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "reka_function_calling",
        "version": "v2",
        "compatible_models": [
            "reka-core-20250115", "reka-core",
            "reka-flash-20250115", "reka-flash",
            "reka-edge-20250115", "reka-edge",
            "reka-vibe"
        ],
        "tools": tools,
        "api_base": "https://api.reka.ai/v1",
        "multimodal": true,
        "instructions": "Use OpenAI-compatible format with Reka API. Reka models are natively multimodal."
    })
}

/// Build AI21 Labs function calling schema
fn build_ai21_schema(ontology: &LanguageOntology) -> JsonValue {
    // AI21 uses OpenAI-compatible format for Jamba
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "ai21_function_calling",
        "version": "v2",
        "compatible_models": [
            "jamba-2-large", "jamba-2-mini",
            "jamba-1.5-large", "jamba-1.5-mini",
            "jamba-instruct"
        ],
        "tools": tools,
        "api_base": "https://api.ai21.com/studio/v1",
        "instructions": "Use OpenAI-compatible format with AI21 API. Jamba 2 models have 256K context and hybrid architecture."
    })
}

/// Build Perplexity AI function calling schema
fn build_perplexity_schema(ontology: &LanguageOntology) -> JsonValue {
    // Perplexity uses OpenAI-compatible format
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "perplexity_function_calling",
        "version": "v2",
        "compatible_models": [
            "sonar-pro", "sonar",
            "sonar-reasoning-pro", "sonar-reasoning",
            "sonar-deep-research",
            "r1-1776"
        ],
        "tools": tools,
        "api_base": "https://api.perplexity.ai",
        "online_search": true,
        "instructions": "Use OpenAI-compatible format with Perplexity API. Sonar models have built-in web search."
    })
}

/// Build Together AI function calling schema
fn build_together_schema(ontology: &LanguageOntology) -> JsonValue {
    // Together uses OpenAI-compatible format
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "together_function_calling",
        "version": "v2",
        "tool_capable_models": [
            "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8",
            "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            "meta-llama/Meta-Llama-3.1-405B-Instruct-Turbo",
            "Qwen/Qwen3-235B-A22B-fp8-tput",
            "Qwen/Qwen2.5-72B-Instruct-Turbo",
            "deepseek-ai/DeepSeek-R1",
            "deepseek-ai/DeepSeek-V3",
            "mistralai/Mistral-Large-2501-Instruct-FP8",
            "google/gemma-3-27b-it"
        ],
        "tools": tools,
        "api_base": "https://api.together.xyz/v1",
        "instructions": "Use OpenAI-compatible format with Together API. Supports many open-source models."
    })
}

/// Build Groq function calling schema (distinct from xAI Grok)
fn build_groq_schema(ontology: &LanguageOntology) -> JsonValue {
    // Groq uses OpenAI-compatible format with fast inference
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "groq_function_calling",
        "version": "v2",
        "compatible_models": [
            "llama-4-scout-17b-16e-instruct",
            "llama-3.3-70b-versatile", "llama-3.3-70b-specdec",
            "llama-3.1-70b-versatile", "llama-3.1-8b-instant",
            "qwen-qwq-32b", "deepseek-r1-distill-llama-70b",
            "mixtral-8x7b-32768", "gemma2-9b-it"
        ],
        "tools": tools,
        "api_base": "https://api.groq.com/openai/v1",
        "ultra_low_latency": true,
        "instructions": "Use OpenAI-compatible format with Groq API. Ultra-fast inference on LPU hardware."
    })
}

/// Build Fireworks AI function calling schema
fn build_fireworks_schema(ontology: &LanguageOntology) -> JsonValue {
    // Fireworks uses OpenAI-compatible format
    let tools: Vec<JsonValue> = ontology
        .builtins
        .iter()
        .map(|b| {
            json!({
                "type": "function",
                "function": {
                    "name": b.name,
                    "description": b.description,
                    "parameters": b.json_schema
                }
            })
        })
        .collect();

    json!({
        "format": "fireworks_function_calling",
        "version": "v2",
        "tool_capable_models": [
            "accounts/fireworks/models/llama4-maverick-instruct-basic",
            "accounts/fireworks/models/llama-v3p3-70b-instruct",
            "accounts/fireworks/models/llama-v3p1-405b-instruct",
            "accounts/fireworks/models/qwen3-235b-a22b",
            "accounts/fireworks/models/qwen2p5-72b-instruct",
            "accounts/fireworks/models/deepseek-r1",
            "accounts/fireworks/models/deepseek-v3",
            "accounts/fireworks/models/mistral-large-2501-instruct"
        ],
        "tools": tools,
        "api_base": "https://api.fireworks.ai/inference/v1",
        "fast_inference": true,
        "instructions": "Use OpenAI-compatible format with Fireworks API. Optimized for fast model inference."
    })
}

// ============================================================================
// HTTP Server Integration
// ============================================================================

#[cfg(feature = "native")]
pub mod server {
    use super::*;
    use axum::{
        body::Body,
        extract::ws::{Message, WebSocket, WebSocketUpgrade},
        http::{header, StatusCode},
        response::{IntoResponse, Response},
        routing::{get, post},
        Json, Router,
    };
    use std::collections::HashMap;
    use std::convert::Infallible;
    use std::sync::Arc;
    use tokio::sync::{broadcast, mpsc, RwLock};
    use tower_http::cors::{Any, CorsLayer};

    // ========================================================================
    // WebSocket Types
    // ========================================================================

    /// WebSocket message from client
    #[derive(Debug, Clone, serde::Deserialize)]
    #[serde(tag = "type")]
    pub enum WsClientMessage {
        /// Execute a request
        #[serde(rename = "execute")]
        Execute { id: String, request: AgentRequest },
        /// Subscribe to a channel
        #[serde(rename = "subscribe")]
        Subscribe { channel: String },
        /// Unsubscribe from a channel
        #[serde(rename = "unsubscribe")]
        Unsubscribe { channel: String },
        /// Ping for keepalive
        #[serde(rename = "ping")]
        Ping { id: Option<String> },
        /// Register as an agent
        #[serde(rename = "register")]
        Register { agent_id: String, capabilities: Vec<String> },
        /// Send message to another agent
        #[serde(rename = "agent_message")]
        AgentMessage { to: String, payload: JsonValue },
        /// Broadcast to all agents
        #[serde(rename = "broadcast")]
        Broadcast { channel: String, payload: JsonValue },
    }

    /// WebSocket message to client
    #[derive(Debug, Clone, serde::Serialize)]
    #[serde(tag = "type")]
    pub enum WsServerMessage {
        /// Response to an execute request
        #[serde(rename = "response")]
        Response { id: String, response: AgentResponse },
        /// Streaming event (progress, data, etc.)
        #[serde(rename = "stream")]
        Stream { id: String, event: StreamEvent },
        /// Channel message (from subscription)
        #[serde(rename = "channel")]
        Channel { channel: String, payload: JsonValue },
        /// Pong response
        #[serde(rename = "pong")]
        Pong { id: Option<String>, timestamp: u64 },
        /// Error message
        #[serde(rename = "error")]
        Error { id: Option<String>, message: String },
        /// Agent message received
        #[serde(rename = "agent_message")]
        AgentMessage { from: String, payload: JsonValue },
        /// Agent registered confirmation
        #[serde(rename = "registered")]
        Registered { agent_id: String },
        /// List of connected agents
        #[serde(rename = "agents")]
        Agents { agents: Vec<AgentInfo> },
    }

    /// Information about a connected agent
    #[derive(Debug, Clone, serde::Serialize)]
    pub struct AgentInfo {
        pub id: String,
        pub capabilities: Vec<String>,
        pub connected_at: u64,
    }

    // ========================================================================
    // Agent Orchestration State
    // ========================================================================

    /// Shared state for agent orchestration
    #[derive(Debug)]
    pub struct OrchestratorState {
        /// Connected agents by ID
        agents: RwLock<HashMap<String, AgentConnection>>,
        /// Broadcast channels for pub/sub
        channels: RwLock<HashMap<String, broadcast::Sender<JsonValue>>>,
        /// Task queue for distributed work
        task_queue: RwLock<Vec<Task>>,
        /// Workflow definitions
        workflows: RwLock<HashMap<String, Workflow>>,
    }

    #[derive(Debug)]
    struct AgentConnection {
        info: AgentInfo,
        sender: mpsc::Sender<WsServerMessage>,
    }

    /// A task in the orchestration queue
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct Task {
        pub id: String,
        pub name: String,
        pub payload: JsonValue,
        pub status: TaskStatus,
        pub assigned_to: Option<String>,
        pub created_at: u64,
        pub priority: i32,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
    pub enum TaskStatus {
        Pending,
        Assigned,
        Running,
        Completed,
        Failed,
    }

    /// A workflow definition
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct Workflow {
        pub id: String,
        pub name: String,
        pub steps: Vec<WorkflowStep>,
        pub current_step: usize,
        pub status: WorkflowStatus,
        pub context: JsonValue,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct WorkflowStep {
        pub name: String,
        pub agent_capability: Option<String>,
        pub request: AgentRequest,
        pub on_success: Option<String>,
        pub on_failure: Option<String>,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
    pub enum WorkflowStatus {
        Pending,
        Running,
        Paused,
        Completed,
        Failed,
    }

    impl OrchestratorState {
        pub fn new() -> Self {
            Self {
                agents: RwLock::new(HashMap::new()),
                channels: RwLock::new(HashMap::new()),
                task_queue: RwLock::new(Vec::new()),
                workflows: RwLock::new(HashMap::new()),
            }
        }

        pub async fn register_agent(
            &self,
            agent_id: String,
            capabilities: Vec<String>,
            sender: mpsc::Sender<WsServerMessage>,
        ) {
            let info = AgentInfo {
                id: agent_id.clone(),
                capabilities,
                connected_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };
            let conn = AgentConnection { info, sender };
            self.agents.write().await.insert(agent_id, conn);
        }

        pub async fn unregister_agent(&self, agent_id: &str) {
            self.agents.write().await.remove(agent_id);
        }

        pub async fn get_agents(&self) -> Vec<AgentInfo> {
            self.agents
                .read()
                .await
                .values()
                .map(|c| c.info.clone())
                .collect()
        }

        pub async fn send_to_agent(&self, agent_id: &str, message: WsServerMessage) -> bool {
            if let Some(conn) = self.agents.read().await.get(agent_id) {
                conn.sender.send(message).await.is_ok()
            } else {
                false
            }
        }

        pub async fn broadcast_to_channel(&self, channel: &str, payload: JsonValue) {
            let channels = self.channels.read().await;
            if let Some(tx) = channels.get(channel) {
                let _ = tx.send(payload);
            }
        }

        pub async fn subscribe_to_channel(&self, channel: &str) -> broadcast::Receiver<JsonValue> {
            let mut channels = self.channels.write().await;
            if let Some(tx) = channels.get(channel) {
                tx.subscribe()
            } else {
                let (tx, rx) = broadcast::channel(100);
                channels.insert(channel.to_string(), tx);
                rx
            }
        }

        pub async fn add_task(&self, task: Task) {
            self.task_queue.write().await.push(task);
        }

        pub async fn get_pending_tasks(&self) -> Vec<Task> {
            self.task_queue
                .read()
                .await
                .iter()
                .filter(|t| t.status == TaskStatus::Pending)
                .cloned()
                .collect()
        }

        pub async fn claim_task(&self, task_id: &str, agent_id: &str) -> Option<Task> {
            let mut queue = self.task_queue.write().await;
            if let Some(task) = queue.iter_mut().find(|t| t.id == task_id && t.status == TaskStatus::Pending) {
                task.status = TaskStatus::Assigned;
                task.assigned_to = Some(agent_id.to_string());
                Some(task.clone())
            } else {
                None
            }
        }

        pub async fn complete_task(&self, task_id: &str, success: bool) {
            let mut queue = self.task_queue.write().await;
            if let Some(task) = queue.iter_mut().find(|t| t.id == task_id) {
                task.status = if success { TaskStatus::Completed } else { TaskStatus::Failed };
            }
        }

        pub async fn add_workflow(&self, workflow: Workflow) {
            self.workflows.write().await.insert(workflow.id.clone(), workflow);
        }

        pub async fn get_workflow(&self, workflow_id: &str) -> Option<Workflow> {
            self.workflows.read().await.get(workflow_id).cloned()
        }
    }

    impl Default for OrchestratorState {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Server-Sent Event for streaming responses
    #[derive(Debug, Clone, serde::Serialize)]
    pub struct StreamEvent {
        /// Event type: "start", "progress", "data", "complete", "error"
        pub event: String,
        /// Event data (JSON value)
        pub data: JsonValue,
        /// Optional event ID for reconnection
        #[serde(skip_serializing_if = "Option::is_none")]
        pub id: Option<String>,
    }

    impl StreamEvent {
        pub fn start(message: &str) -> Self {
            Self {
                event: "start".to_string(),
                data: json!({ "message": message }),
                id: None,
            }
        }

        pub fn progress(current: usize, total: usize, message: &str) -> Self {
            Self {
                event: "progress".to_string(),
                data: json!({
                    "current": current,
                    "total": total,
                    "percentage": if total > 0 { (current as f64 / total as f64) * 100.0 } else { 0.0 },
                    "message": message
                }),
                id: None,
            }
        }

        pub fn data(result: JsonValue, result_type: Option<&str>) -> Self {
            Self {
                event: "data".to_string(),
                data: json!({
                    "result": result,
                    "result_type": result_type
                }),
                id: None,
            }
        }

        pub fn complete(result: JsonValue, result_type: Option<&str>) -> Self {
            Self {
                event: "complete".to_string(),
                data: json!({
                    "success": true,
                    "result": result,
                    "result_type": result_type
                }),
                id: None,
            }
        }

        pub fn error(message: &str) -> Self {
            Self {
                event: "error".to_string(),
                data: json!({
                    "success": false,
                    "error": message
                }),
                id: None,
            }
        }

        /// Format as SSE text
        pub fn to_sse(&self) -> String {
            let mut output = String::new();
            output.push_str(&format!("event: {}\n", self.event));
            if let Some(ref id) = self.id {
                output.push_str(&format!("id: {}\n", id));
            }
            output.push_str(&format!("data: {}\n\n", serde_json::to_string(&self.data).unwrap_or_default()));
            output
        }
    }

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
        // Create shared orchestrator state
        let state = Arc::new(OrchestratorState::new());

        let mut app = Router::new()
            // Main execution endpoint
            .route("/api/v1/execute", post(handle_execute))
            // Convenience endpoints
            .route("/api/v1/call/:builtin", post(handle_call))
            .route("/api/v1/pipeline", post(handle_pipeline))
            .route("/api/v1/eval", post(handle_eval))
            // Streaming endpoints (SSE)
            .route("/api/v1/stream/execute", post(handle_stream_execute))
            .route("/api/v1/stream/pipeline", post(handle_stream_pipeline))
            .route("/api/v1/stream/eval", post(handle_stream_eval))
            // WebSocket endpoint for real-time bidirectional communication
            .route("/api/v1/ws", get({
                let state = Arc::clone(&state);
                move |ws| handle_websocket(ws, state)
            }))
            // Orchestration endpoints
            .route("/api/v1/orchestration/agents", get({
                let state = Arc::clone(&state);
                move || handle_list_agents(state)
            }))
            .route("/api/v1/orchestration/tasks", get({
                let state = Arc::clone(&state);
                move || handle_list_tasks(state)
            }))
            .route("/api/v1/orchestration/tasks", post({
                let state = Arc::clone(&state);
                move |body| handle_create_task(body, state)
            }))
            .route("/api/v1/orchestration/workflows", post({
                let state = Arc::clone(&state);
                move |body| handle_create_workflow(body, state)
            }))
            .route("/api/v1/orchestration/workflows/:id", get({
                let state = Arc::clone(&state);
                move |path| handle_get_workflow(path, state)
            }))
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
        println!("   Supports: OpenAI, Claude, Gemini, Llama, Mistral, Cohere, Grok, DeepSeek,");
        println!("             Bedrock, Azure, Qwen, Ollama, vLLM, HuggingFace, OpenRouter,");
        println!("             Kimi, Yi, GLM, Reka, AI21, Perplexity, Together, Groq, Fireworks");
        println!();
        println!("Endpoints:");
        println!("  POST /api/v1/execute          - Execute any request");
        println!("  POST /api/v1/call/:builtin    - Call a single builtin");
        println!("  POST /api/v1/pipeline         - Execute a pipeline");
        println!("  POST /api/v1/eval             - Evaluate raw code");
        println!("  POST /api/v1/stream/execute   - Stream execution (SSE)");
        println!("  POST /api/v1/stream/pipeline  - Stream pipeline (SSE)");
        println!("  POST /api/v1/stream/eval      - Stream eval (SSE)");
        println!("  GET  /api/v1/ws               - WebSocket (real-time bidirectional)");
        println!("  GET  /api/v1/orchestration/*  - Agent orchestration APIs");
        println!("  GET  /api/v1/schema           - Get language ontology");
        println!("  GET  /api/v1/schema/:format   - Get schema for AI provider");
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

    // ========================================================================
    // Streaming Handlers (Server-Sent Events)
    // ========================================================================

    /// Stream execution of a generic request
    async fn handle_stream_execute(Json(request): Json<AgentRequest>) -> impl IntoResponse {
        create_sse_response(async move {
            let response = process_request(&request);
            vec![
                StreamEvent::start("Processing request..."),
                if response.success {
                    StreamEvent::complete(
                        response.result.unwrap_or(JsonValue::Null),
                        response.result_type.as_deref(),
                    )
                } else {
                    StreamEvent::error(&response.error.unwrap_or_else(|| "Unknown error".to_string()))
                },
            ]
        })
    }

    /// Stream pipeline execution with progress updates
    async fn handle_stream_pipeline(Json(body): Json<JsonValue>) -> impl IntoResponse {
        let steps: Vec<PipelineStep> = serde_json::from_value(
            body.get("steps")
                .cloned()
                .unwrap_or(JsonValue::Array(vec![])),
        )
        .unwrap_or_default();

        let input = body.get("input").cloned();
        let total_steps = steps.len();

        create_sse_response(async move {
            let mut events = vec![StreamEvent::start(&format!("Starting pipeline with {} steps", total_steps))];

            if steps.is_empty() {
                events.push(StreamEvent::error("Pipeline must have at least one step"));
                return events;
            }

            // Process pipeline using existing infrastructure
            let request = AgentRequest::Pipeline { 
                steps: steps.clone(), 
                input: input.clone() 
            };
            
            // Emit progress for each step (simulated since actual execution is atomic)
            for (i, step) in steps.iter().enumerate() {
                events.push(StreamEvent::progress(
                    i + 1,
                    total_steps,
                    &format!("Processing step {}/{}: {}", i + 1, total_steps, step.builtin),
                ));
            }

            // Execute the full pipeline
            let response = process_request(&request);

            if response.success {
                events.push(StreamEvent::complete(
                    response.result.unwrap_or(JsonValue::Null),
                    response.result_type.as_deref(),
                ));
            } else {
                events.push(StreamEvent::error(
                    &response.error.unwrap_or_else(|| "Unknown error".to_string())
                ));
            }

            events
        })
    }

    /// Stream code evaluation
    async fn handle_stream_eval(Json(body): Json<JsonValue>) -> impl IntoResponse {
        let code = body
            .get("code")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        create_sse_response(async move {
            let mut events = vec![StreamEvent::start("Evaluating code...")];

            let request = AgentRequest::Eval { code };
            let response = process_request(&request);

            if response.success {
                events.push(StreamEvent::complete(
                    response.result.unwrap_or(JsonValue::Null),
                    response.result_type.as_deref(),
                ));
            } else {
                events.push(StreamEvent::error(
                    &response.error.unwrap_or_else(|| "Unknown error".to_string())
                ));
            }
            events
        })
    }

    /// Helper to create SSE response from events
    fn create_sse_response<F>(event_generator: F) -> impl IntoResponse
    where
        F: std::future::Future<Output = Vec<StreamEvent>> + Send + 'static,
    {
        let stream = async_stream::stream! {
            let events = event_generator.await;
            for event in events {
                yield Ok::<_, Infallible>(event.to_sse());
            }
        };

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header("X-Accel-Buffering", "no")
            .body(Body::from_stream(stream))
            .unwrap()
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
            // OpenAI family
            "openai" | "gpt" | "chatgpt" => SchemaFormat::OpenAI,
            "azure" | "azure_openai" | "azureopenai" => SchemaFormat::AzureOpenAI,

            // Anthropic
            "claude" | "anthropic" => SchemaFormat::Claude,

            // Google
            "gemini" | "google" => SchemaFormat::Gemini,

            // Meta
            "llama" | "meta" | "llama3" => SchemaFormat::Llama,

            // Mistral
            "mistral" | "codestral" | "pixtral" => SchemaFormat::Mistral,

            // Cohere
            "cohere" | "command" | "command-r" => SchemaFormat::Cohere,

            // xAI
            "grok" | "xai" => SchemaFormat::Grok,

            // DeepSeek
            "deepseek" | "deepseek-r1" => SchemaFormat::DeepSeek,

            // AWS
            "bedrock" | "aws" | "amazon" => SchemaFormat::Bedrock,

            // Alibaba
            "qwen" | "alibaba" | "dashscope" => SchemaFormat::Qwen,

            // Local/Self-hosted
            "ollama" => SchemaFormat::Ollama,
            "vllm" => SchemaFormat::VLLM,

            // Platforms
            "huggingface" | "hf" | "tgi" => SchemaFormat::HuggingFace,
            "openrouter" => SchemaFormat::OpenRouter,

            // Chinese AI providers
            "kimi" | "moonshot" => SchemaFormat::Kimi,
            "yi" | "01ai" | "lingyiwanwu" => SchemaFormat::Yi,
            "glm" | "chatglm" | "zhipu" => SchemaFormat::GLM,

            // Additional SOTA providers
            "reka" => SchemaFormat::Reka,
            "ai21" | "jamba" | "jurassic" => SchemaFormat::AI21,
            "perplexity" | "sonar" => SchemaFormat::Perplexity,
            "together" | "together-ai" => SchemaFormat::Together,
            "groq" => SchemaFormat::Groq,
            "fireworks" | "fireworks-ai" => SchemaFormat::Fireworks,

            // Standard formats
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
            "features": {
                "websocket": true,
                "sse_streaming": true,
                "orchestration": true
            },
            "supported_agents": [
                "openai", "azure_openai", "claude", "gemini",
                "llama", "mistral", "cohere", "grok", "deepseek",
                "bedrock", "qwen", "ollama", "vllm", "huggingface", "openrouter",
                "kimi", "yi", "glm", "reka", "ai21", "perplexity", "together", "groq", "fireworks"
            ],
            "schema_formats": {
                "openai_family": ["openai", "gpt", "chatgpt", "azure", "azure_openai"],
                "anthropic": ["claude", "anthropic"],
                "google": ["gemini", "google"],
                "meta": ["llama", "meta", "llama3"],
                "mistral": ["mistral", "codestral", "pixtral"],
                "cohere": ["cohere", "command", "command-r"],
                "xai": ["grok", "xai"],
                "deepseek": ["deepseek", "deepseek-r1"],
                "cloud": ["bedrock", "aws", "amazon"],
                "alibaba": ["qwen", "alibaba", "dashscope"],
                "chinese_ai": ["kimi", "moonshot", "yi", "01ai", "glm", "chatglm", "zhipu"],
                "inference_platforms": ["together", "together-ai", "groq", "fireworks", "fireworks-ai"],
                "additional_sota": ["reka", "ai21", "jamba", "perplexity", "sonar"],
                "local": ["ollama", "vllm"],
                "platforms": ["huggingface", "hf", "tgi", "openrouter"],
                "standard": ["json", "jsonschema", "ontology", "compact"]
            }
        }))
    }

    // ========================================================================
    // WebSocket Handler
    // ========================================================================

    async fn handle_websocket(
        ws: WebSocketUpgrade,
        state: Arc<OrchestratorState>,
    ) -> impl IntoResponse {
        ws.on_upgrade(move |socket| handle_websocket_connection(socket, state))
    }

    async fn handle_websocket_connection(socket: WebSocket, state: Arc<OrchestratorState>) {
        use futures_util::{SinkExt, StreamExt};

        let (mut sender, mut receiver) = socket.split();
        let (tx, mut rx) = mpsc::channel::<WsServerMessage>(100);
        let mut agent_id: Option<String> = None;

        // Task for sending messages to the client
        let send_task = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Ok(json) = serde_json::to_string(&msg) {
                    if sender.send(Message::Text(json)).await.is_err() {
                        break;
                    }
                }
            }
        });

        // Process incoming messages
        while let Some(result) = receiver.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    if let Ok(msg) = serde_json::from_str::<WsClientMessage>(&text) {
                        let response = match msg {
                            WsClientMessage::Execute { id, request } => {
                                let response = process_request(&request);
                                Some(WsServerMessage::Response { id, response })
                            }
                            WsClientMessage::Ping { id } => {
                                let timestamp = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs();
                                Some(WsServerMessage::Pong { id, timestamp })
                            }
                            WsClientMessage::Register { agent_id: aid, capabilities } => {
                                agent_id = Some(aid.clone());
                                state.register_agent(aid.clone(), capabilities, tx.clone()).await;
                                Some(WsServerMessage::Registered { agent_id: aid })
                            }
                            WsClientMessage::AgentMessage { to, payload } => {
                                if let Some(ref from) = agent_id {
                                    let msg = WsServerMessage::AgentMessage {
                                        from: from.clone(),
                                        payload,
                                    };
                                    state.send_to_agent(&to, msg).await;
                                }
                                None
                            }
                            WsClientMessage::Broadcast { channel, payload } => {
                                state.broadcast_to_channel(&channel, payload).await;
                                None
                            }
                            WsClientMessage::Subscribe { channel } => {
                                let mut rx = state.subscribe_to_channel(&channel).await;
                                let tx_clone = tx.clone();
                                let channel_clone = channel.clone();
                                tokio::spawn(async move {
                                    while let Ok(payload) = rx.recv().await {
                                        let msg = WsServerMessage::Channel {
                                            channel: channel_clone.clone(),
                                            payload,
                                        };
                                        if tx_clone.send(msg).await.is_err() {
                                            break;
                                        }
                                    }
                                });
                                None
                            }
                            WsClientMessage::Unsubscribe { channel: _ } => {
                                // Channel subscriptions are dropped when the task ends
                                None
                            }
                        };

                        if let Some(response) = response {
                            if tx.send(response).await.is_err() {
                                break;
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }

        // Clean up
        if let Some(aid) = agent_id {
            state.unregister_agent(&aid).await;
        }
        send_task.abort();
    }

    // ========================================================================
    // Orchestration Handlers
    // ========================================================================

    async fn handle_list_agents(state: Arc<OrchestratorState>) -> impl IntoResponse {
        let agents = state.get_agents().await;
        Json(json!({
            "success": true,
            "agents": agents
        }))
    }

    async fn handle_list_tasks(state: Arc<OrchestratorState>) -> impl IntoResponse {
        let tasks = state.get_pending_tasks().await;
        Json(json!({
            "success": true,
            "tasks": tasks
        }))
    }

    async fn handle_create_task(
        Json(body): Json<JsonValue>,
        state: Arc<OrchestratorState>,
    ) -> impl IntoResponse {
        let task = Task {
            id: uuid::Uuid::new_v4().to_string(),
            name: body.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed").to_string(),
            payload: body.get("payload").cloned().unwrap_or(JsonValue::Null),
            status: TaskStatus::Pending,
            assigned_to: None,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            priority: body.get("priority").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        };
        let task_id = task.id.clone();
        state.add_task(task).await;

        (StatusCode::CREATED, Json(json!({
            "success": true,
            "task_id": task_id
        })))
    }

    async fn handle_create_workflow(
        Json(body): Json<JsonValue>,
        state: Arc<OrchestratorState>,
    ) -> impl IntoResponse {
        let steps: Vec<WorkflowStep> = body
            .get("steps")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let workflow = Workflow {
            id: uuid::Uuid::new_v4().to_string(),
            name: body.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed").to_string(),
            steps,
            current_step: 0,
            status: WorkflowStatus::Pending,
            context: body.get("context").cloned().unwrap_or(JsonValue::Object(Default::default())),
        };
        let workflow_id = workflow.id.clone();
        state.add_workflow(workflow).await;

        (StatusCode::CREATED, Json(json!({
            "success": true,
            "workflow_id": workflow_id
        })))
    }

    async fn handle_get_workflow(
        axum::extract::Path(workflow_id): axum::extract::Path<String>,
        state: Arc<OrchestratorState>,
    ) -> impl IntoResponse {
        if let Some(workflow) = state.get_workflow(&workflow_id).await {
            (StatusCode::OK, Json(json!({
                "success": true,
                "workflow": workflow
            })))
        } else {
            (StatusCode::NOT_FOUND, Json(json!({
                "success": false,
                "error": "Workflow not found"
            })))
        }
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
        // OpenAI family
        "openai" | "gpt" | "chatgpt" => SchemaFormat::OpenAI,
        "azure" | "azure_openai" | "azureopenai" => SchemaFormat::AzureOpenAI,
        
        // Anthropic
        "claude" | "anthropic" => SchemaFormat::Claude,
        
        // Google
        "gemini" | "google" => SchemaFormat::Gemini,
        
        // Meta
        "llama" | "meta" | "llama3" => SchemaFormat::Llama,
        
        // Mistral
        "mistral" | "codestral" | "pixtral" => SchemaFormat::Mistral,
        
        // Cohere
        "cohere" | "command" | "command-r" => SchemaFormat::Cohere,
        
        // xAI
        "grok" | "xai" => SchemaFormat::Grok,
        
        // DeepSeek
        "deepseek" | "deepseek-r1" => SchemaFormat::DeepSeek,
        
        // AWS
        "bedrock" | "aws" | "amazon" => SchemaFormat::Bedrock,
        
        // Alibaba
        "qwen" | "alibaba" | "dashscope" => SchemaFormat::Qwen,
        
        // Local/Self-hosted
        "ollama" => SchemaFormat::Ollama,
        "vllm" => SchemaFormat::VLLM,
        
        // Platforms
        "huggingface" | "hf" | "tgi" => SchemaFormat::HuggingFace,
        "openrouter" => SchemaFormat::OpenRouter,
        
        // Chinese AI providers
        "kimi" | "moonshot" => SchemaFormat::Kimi,
        "yi" | "01ai" | "lingyiwanwu" => SchemaFormat::Yi,
        "glm" | "chatglm" | "zhipu" => SchemaFormat::GLM,
        
        // Additional SOTA providers
        "reka" => SchemaFormat::Reka,
        "ai21" | "jamba" | "jurassic" => SchemaFormat::AI21,
        "perplexity" | "sonar" => SchemaFormat::Perplexity,
        "together" | "together-ai" => SchemaFormat::Together,
        "groq" => SchemaFormat::Groq,
        "fireworks" | "fireworks-ai" => SchemaFormat::Fireworks,
        
        // Standard formats
        "json" | "jsonschema" | "full" => SchemaFormat::JsonSchema,
        "compact" | "ontology" => SchemaFormat::Ontology,
        
        _ => return Err(anyhow!(
            "Unknown schema format: '{}'. Supported: openai, claude, gemini, llama, mistral, cohere, grok, deepseek, bedrock, azure, qwen, ollama, vllm, huggingface, openrouter, kimi, yi, glm, reka, ai21, perplexity, together, groq, fireworks, json, ontology",
            format
        )),
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
