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

use crate::builtins::BUILTIN_LOOKUP;
use crate::env::Env;
use crate::eval::eval_program;
use crate::marketplace::{RegistryClient, SearchQuery, SortBy};
use crate::modules::all_modules;
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
    /// JSON-LD semantic web format
    JsonLD,
    /// OWL/RDF Turtle format
    OwlTurtle,
    /// SHACL validation shapes
    SHACL,
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
    /// Ontology schema version (semver)
    pub ontology_version: String,
    /// Language metadata
    pub language: LanguageInfo,
    /// Type system
    pub types: Vec<TypeDefinition>,
    /// All builtin functions
    pub builtins: Vec<BuiltinDefinition>,
    /// Module definitions with their function mappings
    pub modules: Vec<ModuleDefinition>,
    /// Operators
    pub operators: Vec<OperatorDefinition>,
    /// Syntax patterns
    pub syntax: SyntaxPatterns,
    /// Categories of functionality
    pub categories: Vec<CategoryInfo>,
    /// OS operations ontology (platform-abstracted capabilities)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_ontology: Option<JsonValue>,
    /// CLI tool wrappers with install detection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_tools: Option<JsonValue>,
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

/// Module definition for the ontology — represents a namespace like `sys`, `net`, `file`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDefinition {
    /// Module name (e.g., "sys", "net", "file")
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Functions exposed by this module
    pub functions: Vec<ModuleFunctionDef>,
}

/// A function within a module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleFunctionDef {
    /// Public method name (e.g., "hostname" in `sys.hostname()`)
    pub method: String,
    /// Internal builtin name it maps to
    pub builtin: String,
    /// Calling syntax
    pub syntax: String,
}

// ============================================================================
// API Implementation
// ============================================================================

/// Process an agent API request
pub fn process_request(request: &AgentRequest) -> AgentResponse {
    let mut resp = dispatch_request(request);
    annotate_token_accounting(request, &mut resp);
    resp
}

fn dispatch_request(request: &AgentRequest) -> AgentResponse {
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

/// Attach estimated `token_accounting { tokens_in, tokens_out, tokens_total }`
/// to the response metadata (§6.1), so agents/supervisors get a budget signal
/// per call without changing the response struct's required fields.
fn annotate_token_accounting(request: &AgentRequest, resp: &mut AgentResponse) {
    let tokens_in =
        crate::builtins::est_token_count(&serde_json::to_string(request).unwrap_or_default());
    let out_str = match (&resp.result, &resp.error) {
        (Some(r), _) => serde_json::to_string(r).unwrap_or_default(),
        (None, Some(e)) => e.clone(),
        _ => String::new(),
    };
    let tokens_out = crate::builtins::est_token_count(&out_str);
    let acct = json!({
        "tokens_in": tokens_in,
        "tokens_out": tokens_out,
        "tokens_total": tokens_in + tokens_out,
        "estimated": true,
    });
    resp.metadata = Some(match resp.metadata.take() {
        Some(JsonValue::Object(mut m)) => {
            m.insert("token_accounting".to_string(), acct);
            JsonValue::Object(m)
        }
        Some(other) => json!({ "info": other, "token_accounting": acct }),
        None => json!({ "token_accounting": acct }),
    });
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
                // Agent mode mirrors the CLI default: tabular results (Array/Table,
                // where keys would otherwise repeat per row) are returned as compact
                // AECON text instead of verbose JSON, paged to AE_TOKEN_BUDGET when
                // set. Scalars/records/strings stay native JSON (a bare number must
                // not become a quoted string). Non-agent consumers are unchanged.
                if crate::safety::current_mode() == crate::safety::Mode::Agent
                    && matches!(value, Value::Array(_) | Value::Table(_))
                {
                    let budget = std::env::var("AE_TOKEN_BUDGET")
                        .ok()
                        .and_then(|s| s.parse::<usize>().ok())
                        .filter(|m| *m > 0);
                    if let Some(text) = crate::builtins::render_agent(&value, budget) {
                        return AgentResponse {
                            success: true,
                            result: Some(JsonValue::String(text)),
                            error: None,
                            result_type: Some("aecon".to_string()),
                            metadata: Some(json!({ "code_executed": code, "render": "aecon" })),
                        };
                    }
                }
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
        SchemaFormat::JsonLD => crate::providers::ontology::OS_ONTOLOGY.to_json_ld(),
        SchemaFormat::OwlTurtle => {
            json!({"format": "text/turtle", "content": crate::providers::ontology::OS_ONTOLOGY.to_owl_turtle()})
        }
        SchemaFormat::SHACL => crate::providers::ontology::OS_ONTOLOGY.to_shacl(),
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
        Value::Builtin(b) => json!({
            "_type": "Builtin",
            "name": b.name
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
        ontology_version: "1.0.0".to_string(),
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
        builtins: get_all_builtin_definitions(),
        modules: get_module_definitions(),
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
        os_ontology: Some(crate::providers::ontology::OS_ONTOLOGY.to_json_schema()),
        cli_tools: Some(crate::providers::ontology::CLI_TOOL_REGISTRY.to_json_schema()),
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

/// Annotate each builtin definition with its safety effect class (`pure`,
/// `read_local`, `write_local`, `destructive`, `process`, `network`, `exec`,
/// `privileged`) under the `x-effect` key of its `json_schema`, so an agent can
/// see an operation's danger level from the ontology *before* calling it.
/// See `crate::safety::effect_of`.
///
/// Also attaches `x-returns` — the builtin's proven return shape — where one
/// exists. Together these say what a call *costs* and what it *yields* before
/// it is made, which is what lets an agent compose a pipeline instead of
/// running a call to discover the shape and then writing the real one.
///
/// `x-returns` is absent far more often than present, and deliberately so:
/// `crate::shapes::DECLARED` carries only shapes reproduced by calling the
/// builtin, so an absent key means "not established", never "returns nothing".
fn annotate_effects(mut defs: Vec<BuiltinDefinition>) -> Vec<BuiltinDefinition> {
    for d in &mut defs {
        let eff = crate::safety::effect_of(&d.name).as_str();
        if let JsonValue::Object(ref mut m) = d.json_schema {
            m.insert("x-effect".to_string(), json!(eff));
            if let Some(shape) = crate::shapes::shape_of(&d.name) {
                m.insert("x-returns".to_string(), json!(shape));
            }
        }
    }
    defs
}

/// Get builtin definitions from documentation
fn get_builtin_definitions() -> Vec<BuiltinDefinition> {
    // Core builtins with full definitions
    let builtins = vec![
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

    annotate_effects(builtins)
}

/// Categorize a builtin by name using comprehensive prefix and name matching
fn categorize_builtin(name: &str) -> String {
    match name {
        // AI & Agents
        n if n.starts_with("ai") || n.starts_with("agent") || n.starts_with("swarm") => {
            "AI".to_string()
        }
        // MCP Protocol
        n if n.starts_with("mcp_") => "MCP".to_string(),
        // A2A / A2UI / NANDA protocols
        n if n.starts_with("a2a_") => "A2A".to_string(),
        n if n.starts_with("a2ui_") => "A2UI".to_string(),
        n if n.starts_with("nanda_") => "NANDA".to_string(),
        // ML / Neural Nets / Evolution / RL
        n if n.starts_with("nn_") => "NeuralNet".to_string(),
        n if n.starts_with("evo_") => "Evolution".to_string(),
        n if n.starts_with("rl_") => "ReinforcementLearning".to_string(),
        n if n.starts_with("kg_")
            || n.starts_with("rag_")
            || n.starts_with("semantic_cache")
            || n.starts_with("fine_tune") =>
        {
            "Knowledge".to_string()
        }
        // Network & HTTP
        n if n.starts_with("http_") || n.starts_with("net_") || n == "http_get" || n == "fetch" => {
            "Network".to_string()
        }
        // System info
        n if n.starts_with("sys_") || n == "whoami" || n == "hostname" || n == "uptime" => {
            "System".to_string()
        }
        // Process management
        n if n.starts_with("proc_") || n.starts_with("ps") || n == "kill" || n == "killall" => {
            "Process".to_string()
        }
        // File system
        n if n.starts_with("file_")
            || n.starts_with("fs_")
            || [
                "ls",
                "cat",
                "pwd",
                "cd",
                "mkdir",
                "rm",
                "mv",
                "cp",
                "find",
                "head",
                "tail",
                "wc",
                "sort",
                "uniq",
                "grep",
                "touch",
                "chmod",
                "chown",
                "stat",
                "glob",
                "tree",
                "exists",
                "read_text",
                "write",
                "list",
                "dir",
            ]
            .contains(&n) =>
        {
            "FileSystem".to_string()
        }
        // Services & daemons
        n if n.starts_with("svc_") || n.starts_with("service_") || n.starts_with("systemctl") => {
            "Service".to_string()
        }
        // Cron/scheduling
        n if n.starts_with("cron_") || n.starts_with("at_") => "Scheduling".to_string(),
        // Archive/compression
        n if n.starts_with("archive_")
            || n.starts_with("zip")
            || n.starts_with("tar")
            || n.starts_with("gzip")
            || n.starts_with("bzip2")
            || n.starts_with("xz") =>
        {
            "Archive".to_string()
        }
        // User/permission management
        n if n.starts_with("user_") || n.starts_with("perm_") || n.starts_with("group_") => {
            "UserManagement".to_string()
        }
        // Package management
        n if n.starts_with("pkg_") => "Package".to_string(),
        // Hardware/device
        n if n.starts_with("hw_") || n.starts_with("device_") => "Hardware".to_string(),
        // GUI automation
        n if n.starts_with("gui_")
            || n.starts_with("screen")
            || n.starts_with("click")
            || n.starts_with("type_text") =>
        {
            "GUI".to_string()
        }
        // Web automation
        n if n.starts_with("web_") || n.starts_with("browser_") => "Web".to_string(),
        // Clipboard & input
        n if n.starts_with("clip_") || n.starts_with("input_") => "Input".to_string(),
        // Database
        n if n.starts_with("db_") || n.starts_with("sqlite_") || n.starts_with("sql_") => {
            "Database".to_string()
        }
        // Security & Crypto
        n if n.starts_with("crypto_")
            || n.starts_with("hash")
            || n.starts_with("encrypt")
            || n.starts_with("decrypt")
            || n.starts_with("sign")
            || n.starts_with("verify")
            || n.starts_with("ssl_") =>
        {
            "Crypto".to_string()
        }
        // Containers
        n if n.starts_with("docker_")
            || n.starts_with("podman_")
            || n.starts_with("container_")
            || n.starts_with("lxc_") =>
        {
            "Container".to_string()
        }
        // Kubernetes
        n if n.starts_with("k8s_") || n.starts_with("helm_") || n.starts_with("kubectl_") => {
            "Kubernetes".to_string()
        }
        // VM/Hypervisor
        n if n.starts_with("vm_")
            || n.starts_with("hyperv_")
            || n.starts_with("virsh_")
            || n.starts_with("qemu_")
            || n.starts_with("wsl_") =>
        {
            "Virtualization".to_string()
        }
        // Cloud/IaC
        n if n.starts_with("terraform_")
            || n.starts_with("ansible_")
            || n.starts_with("pulumi_")
            || n.starts_with("vagrant_")
            || n.starts_with("packer_")
            || n.starts_with("cloud_") =>
        {
            "Cloud".to_string()
        }
        // Remote access
        n if n.starts_with("ssh_")
            || n.starts_with("scp_")
            || n.starts_with("rsync_")
            || n.starts_with("rdp_") =>
        {
            "RemoteAccess".to_string()
        }
        // Security
        n if n.starts_with("firewall_")
            || n.starts_with("selinux_")
            || n.starts_with("apparmor_") =>
        {
            "Security".to_string()
        }
        // Monitoring
        n if n.starts_with("monitor_")
            || n.starts_with("perf_")
            || n.starts_with("netstat_")
            || n.starts_with("watch_") =>
        {
            "Monitoring".to_string()
        }
        // Enterprise
        n if n.starts_with("rbac_") => "RBAC".to_string(),
        n if n.starts_with("audit_") => "Audit".to_string(),
        n if n.starts_with("sso_") => "SSO".to_string(),
        // Cluster/distributed
        n if n.starts_with("cluster_") || n.starts_with("job_") => "Cluster".to_string(),
        // Cloud & Scale
        n if n.starts_with("repl_") => "REPL".to_string(),
        n if n.starts_with("workspace_") => "Workspace".to_string(),
        n if n.starts_with("marketplace_") => "Marketplace".to_string(),
        n if n.starts_with("telemetry_") => "Telemetry".to_string(),
        // Git/code/dev tools
        n if n.starts_with("git_") => "Git".to_string(),
        n if n.starts_with("code_") => "Code".to_string(),
        n if n.starts_with("project_") => "Project".to_string(),
        n if n.starts_with("search_") => "Search".to_string(),
        n if n.starts_with("test_") => "Testing".to_string(),
        n if n.starts_with("diag_") => "Diagnostics".to_string(),
        n if n.starts_with("refactor_") => "Refactoring".to_string(),
        n if n.starts_with("session_") => "Session".to_string(),
        n if n.starts_with("docs_") || n.starts_with("doc_") => "Documentation".to_string(),
        n if n.starts_with("devenv_") => "DevEnvironment".to_string(),
        n if n.starts_with("platform_") => "Platform".to_string(),
        // Math
        n if n.starts_with("math_")
            || [
                "sqrt", "pow", "abs", "ceil", "floor", "round", "sin", "cos", "tan", "log", "exp",
                "pi", "e",
            ]
            .contains(&n) =>
        {
            "Math".to_string()
        }
        // String
        n if n.starts_with("str_")
            || [
                "split",
                "join",
                "trim",
                "upper",
                "lower",
                "replace",
                "contains",
                "starts_with",
                "ends_with",
                "pad_left",
                "pad_right",
                "repeat",
                "reverse",
                "slice",
                "format",
                "to_upper",
                "to_lower",
                "capitalize",
                "char_at",
                "substr",
                "str_replace",
            ]
            .contains(&n) =>
        {
            "String".to_string()
        }
        // Array/collection
        n if n.starts_with("arr_")
            || [
                "range",
                "flatten",
                "zip",
                "unique",
                "sort_by",
                "group_by",
                "chunk",
                "window",
                "scan",
                "zip_with",
                "interleave",
                "partition",
            ]
            .contains(&n) =>
        {
            "Array".to_string()
        }
        // JSON
        n if n.starts_with("json_") || n == "to_json" || n == "from_json" => "JSON".to_string(),
        // Shell
        n if n.starts_with("shell_") || n == "exec" || n == "source" || n == "eval_bash" => {
            "Shell".to_string()
        }
        // Functional
        n if [
            "map", "where", "reduce", "filter", "each", "any", "all", "take", "first", "last",
            "keys", "values", "select", "reject", "flat_map", "fold",
        ]
        .contains(&n) =>
        {
            "Functional".to_string()
        }
        // Aggregation
        n if [
            "sum", "avg", "mean", "min", "max", "count", "product", "len",
        ]
        .contains(&n) =>
        {
            "Aggregation".to_string()
        }
        // Core utilities
        _ => "Core".to_string(),
    }
}

/// Generate a human-readable description from a builtin name
fn describe_builtin_name(name: &str) -> String {
    // Convert snake_case to "Title Case Description"
    let words: Vec<&str> = name.split('_').collect();
    if words.len() <= 1 {
        // Single word builtins
        return match name {
            "ls" => "List directory contents".to_string(),
            "cat" => "Read file contents".to_string(),
            "pwd" => "Print working directory".to_string(),
            "cd" => "Change directory".to_string(),
            "rm" => "Remove file or directory".to_string(),
            "mv" => "Move or rename file".to_string(),
            "cp" => "Copy file or directory".to_string(),
            "grep" => "Search for pattern in text".to_string(),
            "sort" => "Sort lines or array elements".to_string(),
            "uniq" => "Remove duplicate elements".to_string(),
            "wc" => "Count lines, words, and bytes".to_string(),
            "head" => "Show first N lines".to_string(),
            "tail" => "Show last N lines".to_string(),
            "find" => "Find files by pattern".to_string(),
            "mkdir" => "Create directory".to_string(),
            "touch" => "Create empty file or update timestamp".to_string(),
            "echo" => "Print value to output".to_string(),
            "print" => "Print formatted output".to_string(),
            "help" => "Show available commands and help".to_string(),
            "clear" => "Clear terminal screen".to_string(),
            "map" => "Transform each element with a function".to_string(),
            "where" | "filter" => "Filter elements by predicate".to_string(),
            "reduce" => "Reduce array to single value".to_string(),
            "sum" => "Sum numeric values".to_string(),
            "avg" => "Calculate average of numeric values".to_string(),
            "len" => "Get length of array or string".to_string(),
            "keys" => "Get keys of a record".to_string(),
            "values" => "Get values of a record".to_string(),
            "first" => "Get first element".to_string(),
            "last" => "Get last element".to_string(),
            "any" => "Check if any element matches".to_string(),
            "all" => "Check if all elements match".to_string(),
            "take" => "Take first N elements".to_string(),
            "each" => "Execute function for each element".to_string(),
            "split" => "Split string by delimiter".to_string(),
            "join" => "Join array elements with delimiter".to_string(),
            "trim" => "Remove whitespace from string".to_string(),
            "upper" => "Convert string to uppercase".to_string(),
            "lower" => "Convert string to lowercase".to_string(),
            "contains" => "Check if string contains substring".to_string(),
            "replace" => "Replace occurrences in string".to_string(),
            "min" => "Get minimum value".to_string(),
            "max" => "Get maximum value".to_string(),
            "count" => "Count elements".to_string(),
            "flatten" => "Flatten nested arrays".to_string(),
            "range" => "Generate a range of numbers".to_string(),
            "unique" => "Remove duplicate elements".to_string(),
            "reverse" => "Reverse array or string".to_string(),
            "select" => "Select fields from records".to_string(),
            "reject" => "Remove fields from records".to_string(),
            "call" => "Call a named builtin dynamically".to_string(),
            "type" => "Get the type of a value".to_string(),
            "whoami" => "Get current username".to_string(),
            "hostname" => "Get system hostname".to_string(),
            "uptime" => "Get system uptime".to_string(),
            "kill" => "Terminate a process".to_string(),
            "exec" => "Execute external command".to_string(),
            "fetch" => "Fetch URL contents".to_string(),
            "glob" => "Match files by glob pattern".to_string(),
            _ => capitalize_first(name).to_string(),
        };
    }

    // Multi-word: use module prefix as context
    let prefix = words[0];
    let rest: Vec<&str> = words[1..].to_vec();
    let action = rest.join(" ");

    match prefix {
        "sys" => format!("System: {}", action),
        "proc" => format!("Process: {}", action),
        "net" => format!("Network: {}", action),
        "http" => format!("HTTP: {}", action),
        "file" => format!("File: {}", action),
        "fs" => format!("Filesystem: {}", action),
        "gui" => format!("GUI: {}", action),
        "web" => format!("Web: {}", action),
        "db" | "sqlite" => format!("Database: {}", action),
        "crypto" => format!("Crypto: {}", action),
        "svc" | "service" => format!("Service: {}", action),
        "cron" => format!("Schedule: {}", action),
        "archive" => format!("Archive: {}", action),
        "user" => format!("User: {}", action),
        "perm" => format!("Permission: {}", action),
        "pkg" => format!("Package: {}", action),
        "hw" => format!("Hardware: {}", action),
        "clip" => format!("Clipboard: {}", action),
        "input" => format!("Input: {}", action),
        "docker" => format!("Docker: {}", action),
        "podman" => format!("Podman: {}", action),
        "container" => format!("Container: {}", action),
        "k8s" | "kubectl" => format!("Kubernetes: {}", action),
        "helm" => format!("Helm: {}", action),
        "vm" => format!("VM: {}", action),
        "hyperv" => format!("Hyper-V: {}", action),
        "virsh" => format!("Libvirt: {}", action),
        "qemu" => format!("QEMU: {}", action),
        "wsl" => format!("WSL: {}", action),
        "lxc" => format!("LXC: {}", action),
        "terraform" => format!("Terraform: {}", action),
        "ansible" => format!("Ansible: {}", action),
        "pulumi" => format!("Pulumi: {}", action),
        "vagrant" => format!("Vagrant: {}", action),
        "packer" => format!("Packer: {}", action),
        "ssh" => format!("SSH: {}", action),
        "scp" => format!("SCP: {}", action),
        "rsync" => format!("Rsync: {}", action),
        "rdp" => format!("RDP: {}", action),
        "firewall" => format!("Firewall: {}", action),
        "selinux" => format!("SELinux: {}", action),
        "apparmor" => format!("AppArmor: {}", action),
        "ssl" => format!("SSL/TLS: {}", action),
        "monitor" => format!("Monitor: {}", action),
        "perf" => format!("Performance: {}", action),
        "git" => format!("Git: {}", action),
        "code" => format!("Code: {}", action),
        "project" => format!("Project: {}", action),
        "search" => format!("Search: {}", action),
        "test" => format!("Test: {}", action),
        "diag" => format!("Diagnostics: {}", action),
        "refactor" => format!("Refactor: {}", action),
        "session" => format!("Session: {}", action),
        "docs" | "doc" => format!("Documentation: {}", action),
        "devenv" => format!("Dev environment: {}", action),
        "platform" => format!("Platform: {}", action),
        "math" => format!("Math: {}", action),
        "str" => format!("String: {}", action),
        "arr" => format!("Array: {}", action),
        "json" => format!("JSON: {}", action),
        "shell" => format!("Shell: {}", action),
        "mcp" => format!("MCP: {}", action),
        "a2a" => format!("Agent-to-Agent: {}", action),
        "a2ui" => format!("Agent-to-UI: {}", action),
        "nanda" => format!("NANDA: {}", action),
        "nn" => format!("Neural net: {}", action),
        "evo" => format!("Evolution: {}", action),
        "rl" => format!("Reinforcement learning: {}", action),
        "rbac" => format!("RBAC: {}", action),
        "audit" => format!("Audit: {}", action),
        "sso" => format!("SSO: {}", action),
        "cluster" => format!("Cluster: {}", action),
        "cloud" => format!("Cloud: {}", action),
        "repl" => format!("REPL: {}", action),
        "workspace" => format!("Workspace: {}", action),
        "marketplace" => format!("Marketplace: {}", action),
        "telemetry" => format!("Telemetry: {}", action),
        _ => {
            let full = words.join(" ");
            capitalize_first(&full)
        }
    }
}

/// Capitalize the first letter of a string
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Infer a basic return type from the builtin name
fn infer_return_type(name: &str) -> String {
    match name {
        // Known return types for common builtins
        n if n == "ls" || n == "find" || n == "glob" || n == "tree" => "Array<Record>".to_string(),
        n if n == "cat"
            || n == "pwd"
            || n == "whoami"
            || n == "hostname"
            || n == "echo"
            || n == "print" =>
        {
            "String".to_string()
        }
        n if n == "len" || n == "count" || n == "wc" => "Int".to_string(),
        n if n == "sum" || n == "avg" || n == "min" || n == "max" || n == "product" => {
            "Number".to_string()
        }
        n if n == "any" || n == "all" || n == "exists" || n == "contains" => "Bool".to_string(),
        n if n == "first" || n == "last" || n == "reduce" => "Value".to_string(),
        n if n == "map"
            || n == "where"
            || n == "filter"
            || n == "take"
            || n == "sort"
            || n == "uniq"
            || n == "unique"
            || n == "flatten"
            || n == "range"
            || n == "reverse"
            || n == "select"
            || n == "reject"
            || n == "keys"
            || n == "values"
            || n == "each" =>
        {
            "Array".to_string()
        }
        n if n == "split" => "Array<String>".to_string(),
        n if n == "join"
            || n == "trim"
            || n == "upper"
            || n == "lower"
            || n == "replace"
            || n == "format" =>
        {
            "String".to_string()
        }
        // Module-prefix patterns
        n if n.ends_with("_list")
            || n.ends_with("_all")
            || n.ends_with("_search")
            || n.ends_with("_query") =>
        {
            "Array<Record>".to_string()
        }
        n if n.ends_with("_info")
            || n.ends_with("_status")
            || n.ends_with("_config")
            || n.ends_with("_stats")
            || n.ends_with("_details") =>
        {
            "Record".to_string()
        }
        n if n.ends_with("_count")
            || n.ends_with("_size")
            || n.ends_with("_pid")
            || n.ends_with("_port") =>
        {
            "Int".to_string()
        }
        n if n.ends_with("_exists")
            || n.ends_with("_check")
            || n.ends_with("_validate")
            || n.ends_with("_enabled")
            || n.ends_with("_available") =>
        {
            "Bool".to_string()
        }
        n if n.ends_with("_read")
            || n.ends_with("_get")
            || n.ends_with("_name")
            || n.ends_with("_version")
            || n.ends_with("_path")
            || n.ends_with("_hostname") =>
        {
            "String".to_string()
        }
        // Prefix patterns for structured output
        n if n.starts_with("sys_") || n.starts_with("hw_") || n.starts_with("platform_") => {
            "Record".to_string()
        }
        n if n.starts_with("proc_") && n.ends_with("s") => "Array<Record>".to_string(),
        n if n.starts_with("net_") => "Record".to_string(),
        _ => "Value".to_string(),
    }
}

/// Generate all builtin definitions dynamically from BUILTIN_LOOKUP.
/// Enriched hand-coded definitions for core builtins are preserved;
/// all remaining builtins get auto-generated schemas.
fn get_all_builtin_definitions() -> Vec<BuiltinDefinition> {
    // Start with hand-coded enriched definitions
    let enriched = get_builtin_definitions();
    let enriched_names: std::collections::HashSet<String> =
        enriched.iter().map(|b| b.name.clone()).collect();

    // Build reverse lookup: index → Vec<name> to detect aliases
    let mut index_to_names: HashMap<usize, Vec<String>> = HashMap::new();
    for (name, &idx) in BUILTIN_LOOKUP.iter() {
        index_to_names
            .entry(idx)
            .or_default()
            .push(name.to_string());
    }

    // Determine the "canonical" name for each index (shortest non-alias name)
    let mut canonical: HashMap<usize, String> = HashMap::new();
    for (idx, names) in &index_to_names {
        let mut sorted = names.clone();
        sorted.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
        canonical.insert(*idx, sorted[0].clone());
    }

    // Build final list
    let mut all = enriched;
    let mut seen_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Mark enriched indices as seen
    for name in &enriched_names {
        if let Some(&idx) = BUILTIN_LOOKUP.get(name.as_str()) {
            seen_indices.insert(idx);
        }
    }

    // Generate definitions for all remaining builtins
    for (idx, primary_name) in &canonical {
        if seen_indices.contains(idx) {
            continue;
        }
        seen_indices.insert(*idx);

        let category = categorize_builtin(primary_name);
        let description = describe_builtin_name(primary_name);
        let return_type = infer_return_type(primary_name);

        // Collect aliases (other names mapping to same index)
        let aliases: Vec<String> = index_to_names
            .get(idx)
            .map(|names| {
                names
                    .iter()
                    .filter(|n| *n != primary_name)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        let signature = format!("{}() -> {}", primary_name, return_type);

        all.push(BuiltinDefinition {
            name: primary_name.clone(),
            description,
            category,
            signature,
            parameters: vec![],
            return_type,
            examples: vec![],
            aliases: if aliases.is_empty() {
                None
            } else {
                Some(aliases)
            },
            json_schema: json!({
                "type": "object",
                "properties": {}
            }),
        });
    }

    // Sort by category then name for consistent output
    all.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));
    annotate_effects(all)
}

/// The effect class an annotated definition carries (from its `json_schema`).
fn def_effect(d: &BuiltinDefinition) -> &str {
    d.json_schema
        .get("x-effect")
        .and_then(|v| v.as_str())
        .unwrap_or("pure")
}

/// Progressive ontology disclosure (§5.4): a compact root manifest — one entry
/// per category with builtin count, the effect classes present, and a few
/// sample names — plus the effect legend and a usage hint. Agents load this
/// cheaply, then `ontology_describe` only the slice they need, instead of
/// carrying the full multi-thousand-token ontology.
pub fn ontology_manifest_json() -> JsonValue {
    use std::collections::{BTreeMap, BTreeSet};
    let defs = get_all_builtin_definitions();
    // Keep the root deliberately tiny: per category, just the count and the
    // effect classes present. Names/signatures live in ontology_describe so the
    // root stays a cheap index (the standing-context win).
    let mut by_cat: BTreeMap<String, (usize, BTreeSet<String>)> = BTreeMap::new();
    for d in &defs {
        let entry = by_cat
            .entry(d.category.clone())
            .or_insert((0, BTreeSet::new()));
        entry.0 += 1;
        entry.1.insert(def_effect(d).to_string());
    }
    let categories: Vec<JsonValue> = by_cat
        .into_iter()
        .map(|(cat, (count, effects))| {
            json!({
                "category": cat,
                "builtins": count,
                "effects": effects.into_iter().collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "ontology": "manifest",
        "total_builtins": defs.len(),
        "categories": categories,
        "effect_legend": [
            "pure", "read_local", "write_local", "destructive",
            "process", "network", "exec", "privileged"
        ],
        "hint": "ontology_describe(\"<category>\") lists a category's builtins; ontology_describe(\"<builtin>\") returns full detail"
    })
}

/// Flat tool specs for every builtin — `{name, description, signature, effect}` —
/// for exposing builtins as MCP tools annotated with their safety effect class.
pub fn builtin_tool_specs() -> Vec<JsonValue> {
    get_all_builtin_definitions()
        .iter()
        .map(|d| {
            json!({
                "name": d.name,
                "description": d.description,
                "signature": d.signature,
                "effect": def_effect(d),
                // Absent unless proven — see `crate::shapes`.
                "returns": crate::shapes::shape_of(&d.name),
            })
        })
        .collect()
}

/// Expand one slice of the ontology: a builtin name → full definition (with
/// effect, params, examples); a category name → its builtins (name, signature,
/// effect). Case-insensitive. Returns an `{error, hint}` record when unknown.
pub fn ontology_describe_json(query: &str) -> JsonValue {
    let defs = get_all_builtin_definitions();
    let q = query.trim();

    if let Some(d) = defs.iter().find(|d| d.name.eq_ignore_ascii_case(q)) {
        let params: Vec<JsonValue> = d
            .parameters
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "type": p.param_type,
                    "required": p.required,
                    "description": p.description,
                })
            })
            .collect();
        let examples: Vec<JsonValue> = d
            .examples
            .iter()
            .map(|e| json!({ "code": e.code, "description": e.description }))
            .collect();
        return json!({
            "builtin": d.name,
            "category": d.category,
            "signature": d.signature,
            "effect": def_effect(d),
            "return_type": d.return_type,
            "description": d.description,
            "parameters": params,
            "examples": examples,
            "aliases": d.aliases,
        });
    }

    let in_cat: Vec<JsonValue> = defs
        .iter()
        .filter(|d| d.category.eq_ignore_ascii_case(q))
        .map(|d| json!({ "name": d.name, "signature": d.signature, "effect": def_effect(d) }))
        .collect();
    if !in_cat.is_empty() {
        return json!({ "category": q, "builtins": in_cat });
    }

    json!({
        "error": format!("'{}' is not a known category or builtin", q),
        "hint": "call ontology_manifest() to see categories"
    })
}

/// Module descriptions for the ontology
fn module_description(name: &str) -> &str {
    match name {
        "sys" => "System information — hostname, OS, CPU, memory, uptime",
        "proc" => "Process management — list, kill, monitor processes",
        "fs" => "Filesystem operations — mount, disk, symlink",
        "file" => "File I/O — read, write, append, copy, move, edit",
        "net" => "Network — ping, DNS, interfaces, connections",
        "http" => "HTTP client — get, post, put, delete, request",
        "gui" => "GUI automation — screenshot, click, type, window control",
        "web" => "Web automation — browser control, scraping",
        "crypto" => "Cryptography — hash, encrypt, decrypt, sign, UUID",
        "db" => "Database — SQLite queries, connections",
        "svc" => "Service management — start, stop, status, enable",
        "cron" => "Scheduling — cron jobs, scheduled tasks",
        "archive" => "Compression — zip, tar, gzip, extract",
        "user" => "User management — create, delete, modify users",
        "perm" => "Permissions — chmod, chown, ACLs",
        "pkg" => "Package management — install, remove, search, update",
        "hw" => "Hardware info — CPU, GPU, disk, memory, battery",
        "clip" => "Clipboard — copy, paste, history",
        "input" => "User input — prompt, confirm, select",
        "ai" => "AI queries — multi-provider LLM completion",
        "agent" => "Agent framework — autonomous agents with tools",
        "math" => "Mathematics — sqrt, pow, trig, constants",
        "str" => "String manipulation — upper, lower, split, join, replace",
        "arr" => "Array operations — range, flatten, unique, chunk",
        "json" => "JSON — parse, stringify, query, patch",
        "mcp" => "Model Context Protocol — tool discovery and invocation",
        "shell" => "Shell operations — exec, source, environment",
        "a2ui" => "Agent-to-UI — notifications, progress, confirmations",
        "a2a" => "Agent-to-Agent — inter-agent messaging",
        "nanda" => "NANDA consensus protocol",
        "git" => "Git operations — status, commit, push, pull, branch",
        "code" => "Code intelligence — analysis, completion, generation",
        "project" => "Project management — scaffold, dependencies",
        "search" => "Code search — find symbols, references, definitions",
        "test" => "Testing — run, discover, analyze test results",
        "diag" => "Diagnostics — linting, type checking, profiling",
        "refactor" => "Refactoring — rename, extract, inline, organize",
        "session" => "Session management — save, restore, history",
        "docs" => "Documentation — generate, browse, search docs",
        "devenv" => "Dev environment — setup, tools, configuration",
        "platform" => "Platform detection — OS, arch, features, libraries",
        "rbac" => "Role-based access control — roles, permissions, grants",
        "audit" => "Audit logging — log, query, export events",
        "sso" => "Single sign-on — OIDC/SAML authentication",
        "cluster" => "Distributed computing — cluster, job scheduling",
        "nn" => "Neural networks — create, train, predict",
        "evo" => "Evolutionary algorithms — population, evolve, fitness",
        "rl" => "Reinforcement learning — Q-learning, DQN, policy",
        "monitor" => "System monitoring — metrics, alerts, dashboards",
        "cloud" => "Cloud platform — deploy, instances, regions",
        "repl" => "Remote REPL — serve, connect, broadcast via WebSocket",
        "workspace" => "Team workspaces — create, share agents, collaborate",
        "marketplace" => "Plugin marketplace — publish, search, install",
        "telemetry" => "Usage telemetry — opt-in analytics and reporting",
        "docker" => "Docker — containers, images, compose",
        "podman" => "Podman — rootless containers",
        "container" => "Container abstraction — runtime-agnostic container ops",
        "k8s" => "Kubernetes — pods, services, deployments",
        "helm" => "Helm — chart management and releases",
        "vm" => "Virtual machines — create, start, stop, snapshot",
        "hyperv" => "Hyper-V — Windows VM management",
        "virsh" => "Libvirt — Linux VM management",
        "wsl" => "WSL — Windows Subsystem for Linux",
        _ => "Module functions",
    }
}

/// Get module definitions from the module system for the ontology
fn get_module_definitions() -> Vec<ModuleDefinition> {
    let all = all_modules();
    let mut modules = Vec::new();

    for (name, value) in &all {
        if let Value::Record(map) = value {
            let mut functions = Vec::new();
            for (method, _builtin_val) in map {
                // Extract the builtin name from the Value::Builtin
                let builtin_name = if let Value::Builtin(bref) = _builtin_val {
                    bref.name.clone()
                } else {
                    method.clone()
                };
                functions.push(ModuleFunctionDef {
                    method: method.clone(),
                    builtin: builtin_name.clone(),
                    syntax: format!("{}.{}()", name, method),
                });
            }
            functions.sort_by(|a, b| a.method.cmp(&b.method));

            modules.push(ModuleDefinition {
                name: name.to_string(),
                description: module_description(name).to_string(),
                functions,
            });
        }
    }

    // Deduplicate modules (e.g., "platform" appears 3 times in all_modules)
    modules.sort_by(|a, b| a.name.cmp(&b.name));
    modules.dedup_by(|a, b| a.name == b.name);

    modules
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
    let builtins = get_all_builtin_definitions();
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
        ("Array", "Array and collection operations"),
        ("Network", "HTTP and network operations"),
        ("AI", "AI model queries and agent operations"),
        ("MCP", "Model Context Protocol tools"),
        ("NeuralNet", "Neural network creation and training"),
        ("Evolution", "Evolutionary algorithm operations"),
        ("ReinforcementLearning", "Reinforcement learning agents"),
        ("Knowledge", "Knowledge graphs and RAG operations"),
        ("System", "System info — hostname, OS, CPU, memory"),
        ("Process", "Process management — list, kill, monitor"),
        ("Service", "Service/daemon management"),
        ("Database", "Database queries and connections"),
        ("Crypto", "Cryptography, hashing, and encryption"),
        ("Container", "Docker/Podman container management"),
        ("Kubernetes", "Kubernetes cluster operations"),
        ("Virtualization", "VM and hypervisor management"),
        ("Cloud", "Cloud infrastructure and IaC"),
        ("RemoteAccess", "SSH, SCP, and remote connectivity"),
        ("Security", "Firewall, SELinux, AppArmor"),
        ("Monitoring", "System monitoring and performance"),
        ("GUI", "GUI automation — screenshots, clicks"),
        ("Web", "Web automation and browser control"),
        ("Archive", "Compression and archive operations"),
        ("Scheduling", "Cron jobs and scheduled tasks"),
        ("UserManagement", "User and group management"),
        ("Package", "Package management"),
        ("Hardware", "Hardware info and device access"),
        ("Input", "Clipboard and user input"),
        ("JSON", "JSON parsing and manipulation"),
        ("Math", "Mathematical operations and constants"),
        ("Shell", "Shell operations and command execution"),
        ("Git", "Git version control operations"),
        ("Code", "Code intelligence and analysis"),
        ("Project", "Project scaffolding and management"),
        ("Search", "Code search and symbol lookup"),
        ("Testing", "Test execution and analysis"),
        ("Diagnostics", "Linting, profiling, type checking"),
        ("Refactoring", "Code refactoring operations"),
        ("Session", "Session save/restore and history"),
        ("Documentation", "Documentation generation and browsing"),
        ("DevEnvironment", "Development environment setup"),
        ("Platform", "Platform detection and capabilities"),
        ("A2A", "Agent-to-Agent messaging protocol"),
        ("A2UI", "Agent-to-UI notification protocol"),
        ("NANDA", "NANDA consensus protocol"),
        ("RBAC", "Role-based access control"),
        ("Audit", "Audit logging and compliance"),
        ("SSO", "Single sign-on authentication"),
        ("Cluster", "Distributed computing and job scheduling"),
        ("REPL", "Remote REPL and WebSocket sessions"),
        ("Workspace", "Team workspace management"),
        ("Marketplace", "Plugin marketplace"),
        ("Telemetry", "Usage telemetry and analytics"),
        ("Core", "Core language utilities and builtins"),
    ]
    .into_iter()
    .collect();

    let mut result: Vec<CategoryInfo> = categories
        .into_iter()
        .map(|(name, count)| CategoryInfo {
            description: descriptions.get(name.as_str()).unwrap_or(&"").to_string(),
            name,
            builtin_count: count,
        })
        .collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

// ============================================================================
// AI Provider Schema Formats
// ============================================================================

use crate::providers::schema::{builtins_to_tools, ToolFormat};

/// Convert ontology builtins to tool list in the specified format with optional name prefix
fn ontology_tools(ontology: &LanguageOntology, format: ToolFormat, prefix: &str) -> Vec<JsonValue> {
    builtins_to_tools(
        ontology
            .builtins
            .iter()
            .map(|b| (b.name.as_str(), b.description.as_str(), &b.json_schema)),
        format,
        prefix,
    )
}

/// Build OpenAI function calling schema
fn build_openai_schema(ontology: &LanguageOntology) -> JsonValue {
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "aethershell_");

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
    let tools = ontology_tools(ontology, ToolFormat::Anthropic, "aethershell_");

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
    let function_declarations = ontology_tools(ontology, ToolFormat::Google, "aethershell_");

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
            let mut entry = json!({
                "n": b.name,
                "d": b.description,
                "s": b.signature,
                "c": b.category
            });
            if let Some(aliases) = &b.aliases {
                if !aliases.is_empty() {
                    entry["a"] = json!(aliases);
                }
            }
            entry
        })
        .collect();

    let modules_compact: Vec<JsonValue> = ontology
        .modules
        .iter()
        .map(|m| {
            let fns: Vec<String> = m.functions.iter().map(|f| f.method.clone()).collect();
            json!({
                "n": m.name,
                "d": m.description,
                "fns": fns
            })
        })
        .collect();

    json!({
        "lang": "AetherShell",
        "ver": ontology.language.version,
        "ontology_ver": ontology.ontology_version,
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
        "modules": modules_compact,
        "builtins": builtins_compact,
        "stats": {
            "total_builtins": ontology.builtins.len(),
            "total_modules": ontology.modules.len(),
            "categories": ontology.categories.len()
        },
        "os_ontology": ontology.os_ontology,
        "cli_tools": ontology.cli_tools,
    })
}

/// Build Meta Llama function calling schema
fn build_llama_schema(ontology: &LanguageOntology) -> JsonValue {
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "aethershell_");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::Cohere, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    // Bedrock uses Anthropic-style input_schema wrapped in toolSpec/inputSchema/json
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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    let tools = ontology_tools(ontology, ToolFormat::OpenAI, "");

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
    use std::time::Duration;
    use tokio::sync::{broadcast, mpsc, RwLock};
    use tower_http::cors::{Any, CorsLayer};
    use tower_http::timeout::TimeoutLayer;

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
        Register {
            agent_id: String,
            capabilities: Vec<String>,
        },
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
        /// Marketplace registry client
        marketplace: RwLock<Option<RegistryClient>>,
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
            let marketplace_client = RegistryClient::new(None).ok();
            Self {
                agents: RwLock::new(HashMap::new()),
                channels: RwLock::new(HashMap::new()),
                task_queue: RwLock::new(Vec::new()),
                workflows: RwLock::new(HashMap::new()),
                marketplace: RwLock::new(marketplace_client),
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
            if let Some(task) = queue
                .iter_mut()
                .find(|t| t.id == task_id && t.status == TaskStatus::Pending)
            {
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
                task.status = if success {
                    TaskStatus::Completed
                } else {
                    TaskStatus::Failed
                };
            }
        }

        pub async fn add_workflow(&self, workflow: Workflow) {
            self.workflows
                .write()
                .await
                .insert(workflow.id.clone(), workflow);
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

        /// A chunk of an array/table result, streamed so clients consume rows
        /// incrementally and can early-stop without buffering the whole result.
        pub fn chunk(seq: usize, rows: &[JsonValue], total: usize) -> Self {
            Self {
                event: "chunk".to_string(),
                data: json!({ "seq": seq, "rows": rows, "total": total }),
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
            output.push_str(&format!(
                "data: {}\n\n",
                serde_json::to_string(&self.data).unwrap_or_default()
            ));
            output
        }
    }

    /// Agent API server configuration
    #[derive(Debug, Clone)]
    pub struct AgentApiConfig {
        pub host: String,
        pub port: u16,
        pub enable_cors: bool,
        /// Bearer token required on every route except `/health`.
        ///
        /// `None` means "generate one at startup and print it", which is the
        /// safe default. It is deliberately not possible to configure the server
        /// with no authentication at all: `/api/v1/eval` evaluates arbitrary
        /// AetherShell code, so an unauthenticated listener is remote code
        /// execution for anyone who can reach the port — including, when CORS is
        /// enabled, any web page the user happens to visit while the server is
        /// running.
        pub auth_token: Option<String>,
        /// Deadline for a single request/response route, in seconds.
        ///
        /// Without one, an authenticated caller can hold a worker indefinitely
        /// — `/api/v1/eval` evaluates arbitrary code, so an infinite loop is a
        /// one-line request. Enough of those and the server stops answering.
        ///
        /// This does *not* apply to the SSE `/api/v1/stream/*` routes or the
        /// WebSocket, which are long-lived by design; see `build_router`.
        ///
        /// `0` disables the deadline, which reintroduces the exhaustion above.
        /// It exists because a legitimately long evaluation is a real use case,
        /// but prefer raising the value over disabling it.
        pub request_timeout_secs: u64,
    }

    impl Default for AgentApiConfig {
        fn default() -> Self {
            Self {
                host: "127.0.0.1".to_string(),
                port: 3002,
                enable_cors: true,
                auth_token: None,
                // Generous enough that ordinary evaluation is unaffected, short
                // enough that a wedged request frees its worker the same minute.
                request_timeout_secs: 300,
            }
        }
    }

    /// Mint a bearer token, matching `auth::generate_token`'s construction.
    fn generate_api_token() -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        let bytes: [u8; 32] = rand::random();
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Constant-time comparison, so a token cannot be recovered a byte at a time
    /// by timing the response.
    fn tokens_match(presented: &str, expected: &str) -> bool {
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

    /// Start the Agent API HTTP server
    pub async fn start_agent_api_server(config: AgentApiConfig) -> Result<()> {
        // Create shared orchestrator state
        let state = Arc::new(OrchestratorState::new());

        // Long-lived routes, kept out of the timed router below. An SSE stream
        // and a WebSocket are *supposed* to stay open, so a per-request deadline
        // would sever them at the deadline rather than protect anything.
        let long_lived = Router::new()
            // Streaming endpoints (SSE)
            .route("/api/v1/stream/execute", post(handle_stream_execute))
            .route("/api/v1/stream/pipeline", post(handle_stream_pipeline))
            .route("/api/v1/stream/eval", post(handle_stream_eval))
            // WebSocket endpoint for real-time bidirectional communication
            .route(
                "/api/v1/ws",
                get({
                    let state = Arc::clone(&state);
                    move |ws| handle_websocket(ws, state)
                }),
            );

        let mut app = Router::new()
            // Main execution endpoint
            .route("/api/v1/execute", post(handle_execute))
            // Convenience endpoints
            .route("/api/v1/call/:builtin", post(handle_call))
            .route("/api/v1/pipeline", post(handle_pipeline))
            .route("/api/v1/eval", post(handle_eval))
            // Orchestration endpoints
            .route(
                "/api/v1/orchestration/agents",
                get({
                    let state = Arc::clone(&state);
                    move || handle_list_agents(state)
                }),
            )
            .route(
                "/api/v1/orchestration/tasks",
                get({
                    let state = Arc::clone(&state);
                    move || handle_list_tasks(state)
                }),
            )
            .route(
                "/api/v1/orchestration/tasks",
                post({
                    let state = Arc::clone(&state);
                    move |body| handle_create_task(body, state)
                }),
            )
            .route(
                "/api/v1/orchestration/workflows",
                post({
                    let state = Arc::clone(&state);
                    move |body| handle_create_workflow(body, state)
                }),
            )
            .route(
                "/api/v1/orchestration/workflows/:id",
                get({
                    let state = Arc::clone(&state);
                    move |path| handle_get_workflow(path, state)
                }),
            )
            .route(
                "/api/v1/orchestration/workflows/:id/cancel",
                post({
                    let state = Arc::clone(&state);
                    move |path| handle_cancel_workflow(path, state)
                }),
            )
            .route(
                "/api/v1/orchestration/workflows",
                get({
                    let state = Arc::clone(&state);
                    move || handle_list_workflows(state)
                }),
            )
            .route(
                "/api/v1/orchestration/metrics",
                get({
                    let state = Arc::clone(&state);
                    move || handle_orchestration_metrics(state)
                }),
            )
            // Marketplace endpoints
            .route(
                "/api/v1/marketplace/publish",
                post({
                    let state = Arc::clone(&state);
                    move |body| handle_publish_agent(body, state)
                }),
            )
            .route(
                "/api/v1/marketplace/search",
                get({
                    let state = Arc::clone(&state);
                    move |params| handle_marketplace_search(params, state)
                }),
            )
            .route(
                "/api/v1/marketplace/agents",
                get({
                    let state = Arc::clone(&state);
                    move || handle_marketplace_list(state)
                }),
            )
            .route(
                "/api/v1/marketplace/install",
                post({
                    let state = Arc::clone(&state);
                    move |body| handle_marketplace_install(body, state)
                }),
            )
            .route(
                "/api/v1/marketplace/uninstall",
                post({
                    let state = Arc::clone(&state);
                    move |body| handle_marketplace_uninstall(body, state)
                }),
            )
            // Discovery endpoints
            .route("/api/v1/schema", get(handle_schema))
            .route("/api/v1/schema/:format", get(handle_schema_format))
            .route("/api/v1/builtins", get(handle_list_builtins))
            .route("/api/v1/builtins/:name", get(handle_describe_builtin))
            .route("/api/v1/types", get(handle_types))
            // Health check
            .route("/health", get(handle_health));

        // Give the interpreter a slightly tighter budget than the HTTP layer,
        // so evaluation stops itself first and the caller gets an explanation
        // rather than a bare 408 over a thread that is still running. The
        // `max(1)` keeps a 1-second configured timeout from rounding down to
        // zero, which reads as "no deadline" and would silently disable this.
        EVAL_DEADLINE_SECS.store(
            match config.request_timeout_secs {
                0 => 0,
                secs => (secs * 9 / 10).max(1),
            },
            std::sync::atomic::Ordering::Relaxed,
        );

        // The deadline goes on before the merge, so it wraps only the routes
        // added above — `Router::layer` applies to what is already registered.
        if config.request_timeout_secs > 0 {
            app = app.layer(TimeoutLayer::new(Duration::from_secs(
                config.request_timeout_secs,
            )));
        }
        let mut app = app.merge(long_lived);

        // Authentication. Applied before CORS below so that it wraps every
        // route; `/health` is exempted inside the middleware so liveness probes
        // keep working without a credential.
        let token = Arc::new(match config.auth_token.clone() {
            Some(t) if !t.trim().is_empty() => t,
            _ => generate_api_token(),
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

                    if tokens_match(presented, &expected) {
                        next.run(req).await
                    } else {
                        (
                            StatusCode::UNAUTHORIZED,
                            [(header::WWW_AUTHENTICATE, "Bearer")],
                            Json(serde_json::json!({
                                "error": "unauthorized",
                                "detail": "Send `Authorization: Bearer <token>`. \
                                           The token is printed when the server starts.",
                            })),
                        )
                            .into_response()
                    }
                }
            },
        ));

        if config.enable_cors {
            // `allow_origin(Any)` tells every browser that any web page may talk
            // to this server. That is only tolerable because the bearer token
            // above is required and a cross-origin page cannot read it: without
            // authentication this combination let any site the user visited POST
            // to /api/v1/eval on their loopback interface and execute code.
            app = app.layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            );
        }

        // `SocketAddr` parses literal addresses only — `localhost:3002` is a
        // perfectly reasonable thing to put in a config file and does *not*
        // parse. Report it instead of panicking the server at startup.
        let bind = format!("{}:{}", config.host, config.port);
        let addr: std::net::SocketAddr = bind.parse().map_err(|e| {
            anyhow::anyhow!(
                "invalid bind address {bind:?}: {e}. \
                 Use a literal IP such as 127.0.0.1 or 0.0.0.0, not a hostname."
            )
        })?;

        if !addr.ip().is_loopback() {
            println!(
                "⚠  Binding {} — this API evaluates arbitrary code, so it is now\n\
                 ⚠  reachable by anything that can route to this host. The bearer\n\
                 ⚠  token below is the only thing standing in front of it.",
                addr.ip()
            );
        }

        println!("🤖 AetherShell Agent API starting on http://{}", addr);
        if config.auth_token.is_none() {
            println!("   Auth token (generated): {}", token);
        } else {
            println!("   Auth token: (from configuration)");
        }
        println!("   Send it as: Authorization: Bearer <token>");
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
        println!("  POST /api/v1/marketplace/*    - Agent marketplace APIs");
        println!("  GET  /api/v1/schema           - Get language ontology");
        println!("  GET  /api/v1/schema/:format   - Get schema for AI provider");
        println!("  GET  /api/v1/builtins         - List all builtins");
        println!("  GET  /api/v1/builtins/:name   - Describe a builtin");
        println!("  GET  /api/v1/types            - Get type information");

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    /// Evaluation deadline for request handlers, in seconds; 0 means none.
    ///
    /// A process-level value rather than router state because these routes are
    /// registered as bare functions and there is one server per process. Set
    /// once in `start_agent_api_server`, read per request.
    static EVAL_DEADLINE_SECS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    /// The evaluation deadline for the running server, if any.
    ///
    /// Deliberately shorter than the HTTP timeout: the interpreter should stop
    /// on its own *before* the connection is torn out from under it, so the
    /// caller gets a real error explaining what happened rather than a bare
    /// 408 and a thread that keeps working.
    fn eval_deadline() -> Option<std::time::Duration> {
        match EVAL_DEADLINE_SECS.load(std::sync::atomic::Ordering::Relaxed) {
            0 => None,
            secs => Some(std::time::Duration::from_secs(secs)),
        }
    }

    /// Run a request on the blocking pool and shape the response.
    ///
    /// `process_request` is synchronous and can run arbitrary evaluated code,
    /// so calling it directly from an `async fn` pins an async worker for the
    /// whole evaluation. That is not merely a throughput concern: it also made
    /// `TimeoutLayer` decorative, because tower races the deadline against the
    /// inner future *within the same poll*. If the inner call never yields, the
    /// timeout branch is never reached and the deadline cannot fire — measured,
    /// not assumed: before this change a 1-second deadline did not interrupt a
    /// 20-second request at all.
    ///
    /// What this bounds and what it does not. Moving the work here lets the
    /// deadline fire, frees the async worker, and returns 408 to the caller.
    /// Dropping a `spawn_blocking` handle does not cancel the closure, so the
    /// HTTP-level timeout alone would leave the evaluation running on a
    /// blocking-pool thread until it returned by itself.
    ///
    /// `safety::enter_deadline` closes most of that: the interpreter checks it
    /// in `eval_expr`, so runaway recursion and large computations stop rather
    /// than leak a thread. The guard restores the previous value on drop, which
    /// matters here specifically — these threads are pooled and reused, so a
    /// deadline left set would make the *next* request on that thread fail
    /// immediately.
    ///
    /// It still cannot interrupt a builtin already blocked in a syscall
    /// (`sleep`, a subprocess wait, a network read), because those never return
    /// to the interpreter to be asked. That is the remaining gap, and it is
    /// narrower than before rather than closed.
    async fn run_blocking(
        request: AgentRequest,
        deadline: Option<std::time::Duration>,
    ) -> (StatusCode, Json<AgentResponse>) {
        match tokio::task::spawn_blocking(move || {
            let _guard = deadline.map(crate::safety::enter_deadline);
            process_request(&request)
        })
        .await
        {
            Ok(response) => {
                if response.success {
                    (StatusCode::OK, Json(response))
                } else {
                    (StatusCode::BAD_REQUEST, Json(response))
                }
            }
            // The closure panicked. Report it rather than propagating, so one
            // bad request cannot take down the listener.
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AgentResponse {
                    success: false,
                    result: None,
                    error: Some(format!("request handler failed: {e}")),
                    result_type: None,
                    metadata: None,
                }),
            ),
        }
    }

    async fn handle_execute(Json(request): Json<AgentRequest>) -> impl IntoResponse {
        run_blocking(request, eval_deadline()).await
    }

    async fn handle_call(
        axum::extract::Path(builtin): axum::extract::Path<String>,
        Json(args): Json<JsonValue>,
    ) -> impl IntoResponse {
        run_blocking(AgentRequest::Call { builtin, args }, eval_deadline()).await
    }

    async fn handle_pipeline(Json(body): Json<JsonValue>) -> impl IntoResponse {
        let steps: Vec<PipelineStep> = serde_json::from_value(
            body.get("steps")
                .cloned()
                .unwrap_or(JsonValue::Array(vec![])),
        )
        .unwrap_or_default();

        let input = body.get("input").cloned();

        run_blocking(AgentRequest::Pipeline { steps, input }, eval_deadline()).await
    }

    async fn handle_eval(Json(body): Json<JsonValue>) -> impl IntoResponse {
        let code = body
            .get("code")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        run_blocking(AgentRequest::Eval { code }, eval_deadline()).await
    }

    // ========================================================================
    // Streaming Handlers (Server-Sent Events)
    // ========================================================================

    /// Stream execution of a generic request
    async fn handle_stream_execute(Json(request): Json<AgentRequest>) -> impl IntoResponse {
        create_sse_response(async move {
            let response = process_request(&request);
            stream_events_from_response(response, 50)
        })
    }

    /// Build streaming events from a response. A large array result is split into
    /// ordered `chunk` events (default 50 rows) so a client consumes incrementally
    /// and can early-stop instead of buffering the whole result; smaller results
    /// and non-arrays emit a single `complete` event; failures emit `error`.
    pub fn stream_events_from_response(
        response: AgentResponse,
        chunk_rows: usize,
    ) -> Vec<StreamEvent> {
        let mut events = vec![StreamEvent::start("Processing request...")];
        if !response.success {
            events.push(StreamEvent::error(
                &response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
            return events;
        }
        let result = response.result.unwrap_or(JsonValue::Null);
        match &result {
            JsonValue::Array(rows) if rows.len() > chunk_rows.max(1) => {
                let n = chunk_rows.max(1);
                let total = rows.len();
                for (seq, chunk) in rows.chunks(n).enumerate() {
                    events.push(StreamEvent::chunk(seq, chunk, total));
                }
                events.push(StreamEvent::complete(
                    json!({ "streamed_rows": total }),
                    response.result_type.as_deref(),
                ));
            }
            _ => events.push(StreamEvent::complete(
                result,
                response.result_type.as_deref(),
            )),
        }
        events
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
            let mut events = vec![StreamEvent::start(&format!(
                "Starting pipeline with {} steps",
                total_steps
            ))];

            if steps.is_empty() {
                events.push(StreamEvent::error("Pipeline must have at least one step"));
                return events;
            }

            // Process pipeline using existing infrastructure
            let request = AgentRequest::Pipeline {
                steps: steps.clone(),
                input: input.clone(),
            };

            // Emit progress for each step (simulated since actual execution is atomic)
            for (i, step) in steps.iter().enumerate() {
                events.push(StreamEvent::progress(
                    i + 1,
                    total_steps,
                    &format!(
                        "Processing step {}/{}: {}",
                        i + 1,
                        total_steps,
                        step.builtin
                    ),
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
                    &response
                        .error
                        .unwrap_or_else(|| "Unknown error".to_string()),
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

            // Stream-evaluate: results are produced incrementally (element-by-element
            // for a streamable `source | map/where/…` pipeline) rather than the whole
            // value being materialized first, then chunked for the wire.
            let mut env = crate::env::Env::new();
            let mut items: Vec<JsonValue> = Vec::new();
            let result = {
                let mut emit = |v: crate::value::Value| items.push(v.to_json());
                crate::eval::eval_stream(&code, &mut env, &mut emit)
            };
            match result {
                Ok(n) => {
                    for (seq, chunk) in items.chunks(50).enumerate() {
                        events.push(StreamEvent::chunk(seq, chunk, n));
                    }
                    events.push(StreamEvent::complete(
                        json!({ "streamed_items": n }),
                        Some("stream"),
                    ));
                }
                Err(e) => events.push(StreamEvent::error(&format!("{}", e))),
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
                            WsClientMessage::Register {
                                agent_id: aid,
                                capabilities,
                            } => {
                                agent_id = Some(aid.clone());
                                state
                                    .register_agent(aid.clone(), capabilities.clone(), tx.clone())
                                    .await;

                                // Broadcast agent connection to subscribers
                                state
                                    .broadcast_to_channel(
                                        "agents",
                                        json!({
                                            "type": "agent_connected",
                                            "agent": {
                                                "id": aid,
                                                "capabilities": capabilities,
                                                "status": "online",
                                                "connectedAt": std::time::SystemTime::now()
                                                    .duration_since(std::time::UNIX_EPOCH)
                                                    .unwrap()
                                                    .as_millis() as u64
                                            }
                                        }),
                                    )
                                    .await;

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
        if let Some(ref aid) = agent_id {
            state
                .broadcast_to_channel(
                    "agents",
                    json!({
                        "type": "agent_disconnected",
                        "agentId": aid
                    }),
                )
                .await;
            state.unregister_agent(aid).await;
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
            name: body
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unnamed")
                .to_string(),
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

        (
            StatusCode::CREATED,
            Json(json!({
                "success": true,
                "task_id": task_id
            })),
        )
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
            name: body
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unnamed")
                .to_string(),
            steps,
            current_step: 0,
            status: WorkflowStatus::Pending,
            context: body
                .get("context")
                .cloned()
                .unwrap_or(JsonValue::Object(Default::default())),
        };
        let workflow_id = workflow.id.clone();
        let workflow_name = workflow.name.clone();
        let total_steps = workflow.steps.len();
        state.add_workflow(workflow).await;

        // Broadcast workflow creation
        state
            .broadcast_to_channel(
                "workflows",
                json!({
                    "type": "workflow_created",
                    "workflow": {
                        "id": workflow_id,
                        "name": workflow_name,
                        "status": "pending",
                        "currentStep": 0,
                        "totalSteps": total_steps,
                        "startedAt": std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64,
                        "steps": []
                    }
                }),
            )
            .await;

        (
            StatusCode::CREATED,
            Json(json!({
                "success": true,
                "workflow_id": workflow_id
            })),
        )
    }

    async fn handle_get_workflow(
        axum::extract::Path(workflow_id): axum::extract::Path<String>,
        state: Arc<OrchestratorState>,
    ) -> impl IntoResponse {
        if let Some(workflow) = state.get_workflow(&workflow_id).await {
            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "workflow": workflow
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "success": false,
                    "error": "Workflow not found"
                })),
            )
        }
    }

    async fn handle_cancel_workflow(
        axum::extract::Path(workflow_id): axum::extract::Path<String>,
        state: Arc<OrchestratorState>,
    ) -> impl IntoResponse {
        if let Some(mut workflow) = state.get_workflow(&workflow_id).await {
            workflow.status = WorkflowStatus::Failed;
            state.add_workflow(workflow).await;

            // Broadcast cancellation
            state
                .broadcast_to_channel(
                    "workflows",
                    json!({
                        "type": "workflow_update",
                        "workflow": {
                            "id": workflow_id,
                            "status": "cancelled"
                        }
                    }),
                )
                .await;

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "message": "Workflow cancelled"
                })),
            )
        } else {
            (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "success": false,
                    "error": "Workflow not found"
                })),
            )
        }
    }

    async fn handle_list_workflows(state: Arc<OrchestratorState>) -> impl IntoResponse {
        let workflows = state.workflows.read().await;
        let list: Vec<_> = workflows.values().cloned().collect();
        Json(json!({
            "success": true,
            "workflows": list
        }))
    }

    async fn handle_orchestration_metrics(state: Arc<OrchestratorState>) -> impl IntoResponse {
        let agents = state.get_agents().await;
        let workflows = state.workflows.read().await;
        let tasks = state.task_queue.read().await;

        let running_workflows = workflows
            .values()
            .filter(|w| w.status == WorkflowStatus::Running)
            .count();
        let completed_workflows = workflows
            .values()
            .filter(|w| w.status == WorkflowStatus::Completed)
            .count();
        let failed_workflows = workflows
            .values()
            .filter(|w| w.status == WorkflowStatus::Failed)
            .count();
        let pending_tasks = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .count();

        Json(json!({
            "success": true,
            "metrics": {
                "agents": {
                    "total": agents.len(),
                    "online": agents.len(),
                },
                "workflows": {
                    "total": workflows.len(),
                    "running": running_workflows,
                    "completed": completed_workflows,
                    "failed": failed_workflows,
                },
                "tasks": {
                    "total": tasks.len(),
                    "pending": pending_tasks,
                },
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }
        }))
    }

    async fn handle_publish_agent(
        Json(body): Json<JsonValue>,
        state: Arc<OrchestratorState>,
    ) -> impl IntoResponse {
        let agent_name = body
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed")
            .to_string();

        // Store in orchestrator state as a published agent
        state
            .broadcast_to_channel(
                "agents",
                json!({
                    "type": "agent_published",
                    "agent": {
                        "name": agent_name,
                        "description": body.get("description"),
                        "systemPrompt": body.get("systemPrompt"),
                        "tools": body.get("tools"),
                        "model": body.get("model"),
                    }
                }),
            )
            .await;

        (
            StatusCode::CREATED,
            Json(json!({
                "success": true,
                "message": format!("Agent '{}' published successfully", agent_name)
            })),
        )
    }

    async fn handle_marketplace_search(
        axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
        state: Arc<OrchestratorState>,
    ) -> impl IntoResponse {
        let query_str = params.get("q").cloned().unwrap_or_default();
        let category = params.get("category").cloned();
        let sort = params.get("sort").map(|s| match s.as_str() {
            "stars" => SortBy::Stars,
            "recent" => SortBy::Recent,
            "name" => SortBy::Name,
            _ => SortBy::Downloads,
        });

        // Try registry search first
        let marketplace = state.marketplace.read().await;
        if let Some(client) = marketplace.as_ref() {
            let search_query = SearchQuery {
                query: if query_str.is_empty() {
                    None
                } else {
                    Some(query_str.clone())
                },
                category: category.clone(),
                keyword: None,
                author: None,
                sort_by: sort,
                page: params.get("page").and_then(|p| p.parse().ok()),
                per_page: params.get("per_page").and_then(|p| p.parse().ok()),
            };

            match client.search(search_query).await {
                Ok(results) => {
                    let agents: Vec<JsonValue> = results
                        .results
                        .iter()
                        .map(|a| {
                            json!({
                                "id": a.manifest.name,
                                "name": a.manifest.display_name,
                                "description": a.manifest.description,
                                "author": a.manifest.author.name,
                                "version": a.manifest.version,
                                "downloads": a.downloads,
                                "stars": a.stars,
                                "forks": a.forks,
                                "tags": a.manifest.keywords,
                                "updatedAt": a.updated_at * 1000,
                                "verified": a.verified,
                            })
                        })
                        .collect();

                    return Json(json!({
                        "success": true,
                        "query": query_str,
                        "category": category,
                        "agents": agents,
                        "total": results.total,
                        "page": results.page,
                        "per_page": results.per_page,
                    }));
                }
                Err(_) => {
                    // Fall through to local-only response
                }
            }
        }

        // Fallback: return installed agents that match the query
        let installed_agents = get_installed_as_json(&state).await;
        let filtered: Vec<JsonValue> = if query_str.is_empty() {
            installed_agents.clone()
        } else {
            let q = query_str.to_lowercase();
            installed_agents
                .iter()
                .filter(|a| {
                    a.get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&q)
                        || a.get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&q)
                })
                .cloned()
                .collect()
        };

        Json(json!({
            "success": true,
            "query": query_str,
            "category": category,
            "agents": filtered,
            "total": filtered.len(),
            "page": 1,
            "per_page": 20,
            "source": "local",
        }))
    }

    async fn handle_marketplace_list(state: Arc<OrchestratorState>) -> impl IntoResponse {
        let installed_agents = get_installed_as_json(&state).await;

        Json(json!({
            "success": true,
            "agents": installed_agents,
            "total": installed_agents.len(),
        }))
    }

    async fn handle_marketplace_install(
        Json(body): Json<JsonValue>,
        state: Arc<OrchestratorState>,
    ) -> impl IntoResponse {
        let name = match body.get("name").and_then(|n| n.as_str()) {
            Some(n) => n.to_string(),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "success": false,
                        "error": "Missing required field: name"
                    })),
                )
            }
        };
        let version = body
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut marketplace = state.marketplace.write().await;
        let client = match marketplace.as_mut() {
            Some(c) => c,
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({
                        "success": false,
                        "error": "Marketplace client not available"
                    })),
                )
            }
        };

        match client.install(&name, version.as_deref()).await {
            Ok(installed) => {
                // Broadcast install event
                state
                    .broadcast_to_channel(
                        "agents",
                        json!({
                            "type": "agent_installed",
                            "agent": {
                                "name": installed.manifest.name,
                                "version": installed.manifest.version,
                                "description": installed.manifest.description,
                            }
                        }),
                    )
                    .await;

                (
                    StatusCode::OK,
                    Json(json!({
                        "success": true,
                        "message": format!("Installed {} v{}", installed.manifest.display_name, installed.manifest.version),
                        "agent": {
                            "name": installed.manifest.name,
                            "version": installed.manifest.version,
                            "path": installed.path.to_string_lossy(),
                        }
                    })),
                )
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "error": format!("Failed to install {}: {}", name, e)
                })),
            ),
        }
    }

    async fn handle_marketplace_uninstall(
        Json(body): Json<JsonValue>,
        state: Arc<OrchestratorState>,
    ) -> impl IntoResponse {
        let name = match body.get("name").and_then(|n| n.as_str()) {
            Some(n) => n.to_string(),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "success": false,
                        "error": "Missing required field: name"
                    })),
                )
            }
        };

        let mut marketplace = state.marketplace.write().await;
        let client = match marketplace.as_mut() {
            Some(c) => c,
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({
                        "success": false,
                        "error": "Marketplace client not available"
                    })),
                )
            }
        };

        match client.uninstall(&name) {
            Ok(()) => {
                state
                    .broadcast_to_channel(
                        "agents",
                        json!({
                            "type": "agent_uninstalled",
                            "name": name,
                        }),
                    )
                    .await;

                (
                    StatusCode::OK,
                    Json(json!({
                        "success": true,
                        "message": format!("Uninstalled {}", name)
                    })),
                )
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "error": format!("Failed to uninstall {}: {}", name, e)
                })),
            ),
        }
    }

    /// Helper: convert installed agents to JSON array
    async fn get_installed_as_json(state: &OrchestratorState) -> Vec<JsonValue> {
        let marketplace = state.marketplace.read().await;
        match marketplace.as_ref() {
            Some(client) => client
                .list_installed()
                .iter()
                .map(|a| {
                    json!({
                        "id": a.manifest.name,
                        "name": a.manifest.display_name,
                        "description": a.manifest.description,
                        "author": a.manifest.author.name,
                        "version": a.manifest.version,
                        "downloads": 0,
                        "stars": 0,
                        "forks": 0,
                        "tags": a.manifest.keywords,
                        "updatedAt": a.installed_at * 1000,
                        "verified": false,
                        "installed": true,
                    })
                })
                .collect(),
            None => vec![],
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
        "jsonld" | "json-ld" | "linked_data" => SchemaFormat::JsonLD,
        "owl" | "rdf" | "turtle" | "owl_turtle" => SchemaFormat::OwlTurtle,
        "shacl" | "shapes" => SchemaFormat::SHACL,

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
