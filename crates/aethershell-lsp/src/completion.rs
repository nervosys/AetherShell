//! Autocompletion for AetherShell
//!
//! Provides intelligent completions for:
//! - Builtins (map, filter, reduce, etc.)
//! - Keywords (let, fn, match, if)
//! - Variables in scope
//! - Pipeline operators

use tower_lsp::lsp_types::*;

use crate::document::DocumentStore;

/// AetherShell builtin functions with documentation
const BUILTINS: &[(&str, &str, &str)] = &[
    // General
    ("print", "print(value)", "Print a value to stdout"),
    ("echo", "echo(value)", "Echo a value (alias for print)"),
    ("help", "help()", "Display help information"),
    ("type_of", "type_of(value)", "Get the type of a value"),
    (
        "len",
        "len(collection)",
        "Get the length of a string or array",
    ),
    // Data Pipeline
    (
        "map",
        "map(fn(x) => expr)",
        "Transform each element in an array",
    ),
    (
        "where",
        "where(fn(x) => condition)",
        "Filter elements by predicate",
    ),
    (
        "reduce",
        "reduce(fn(acc, x) => expr, initial)",
        "Reduce array to single value",
    ),
    ("take", "take(n)", "Take first n elements from array"),
    ("first", "first()", "Get the first element of an array"),
    ("last", "last()", "Get the last element of an array"),
    (
        "any",
        "any(fn(x) => condition)",
        "Check if any element matches",
    ),
    (
        "all",
        "all(fn(x) => condition)",
        "Check if all elements match",
    ),
    ("flatten", "flatten()", "Flatten nested arrays"),
    ("reverse", "reverse()", "Reverse an array"),
    ("sort", "sort()", "Sort an array"),
    ("uniq", "uniq()", "Remove duplicate elements"),
    ("zip", "zip(other_array)", "Zip two arrays together"),
    ("range", "range(start, end)", "Generate a range of integers"),
    ("slice", "slice(start, end)", "Get a slice of an array"),
    ("push", "push(element)", "Add element to array"),
    ("concat", "concat(other)", "Concatenate arrays or strings"),
    ("keys", "keys(record)", "Get keys of a record"),
    // String
    ("split", "split(delimiter)", "Split string by delimiter"),
    (
        "join",
        "join(delimiter)",
        "Join array elements with delimiter",
    ),
    ("trim", "trim()", "Trim whitespace from string"),
    ("upper", "upper()", "Convert string to uppercase"),
    ("lower", "lower()", "Convert string to lowercase"),
    (
        "replace",
        "replace(pattern, replacement)",
        "Replace occurrences in string",
    ),
    (
        "contains",
        "contains(substring)",
        "Check if string contains substring",
    ),
    (
        "starts_with",
        "starts_with(prefix)",
        "Check if string starts with prefix",
    ),
    (
        "ends_with",
        "ends_with(suffix)",
        "Check if string ends with suffix",
    ),
    // File System
    ("ls", "ls(path?)", "List directory contents"),
    ("pwd", "pwd()", "Print working directory"),
    ("cat", "cat(path)", "Read file contents"),
    ("read_text", "read_text(path)", "Read file as text"),
    ("head", "head(n?)", "Get first n lines (default 10)"),
    ("tail", "tail(n?)", "Get last n lines (default 10)"),
    ("find", "find(path, pattern)", "Find files matching pattern"),
    ("wc", "wc()", "Word/line count"),
    ("grep", "grep(pattern)", "Search for pattern in text"),
    // HTTP
    ("http_get", "http_get(url)", "Make HTTP GET request"),
    // AI & Agents
    ("ai", "ai(prompt)", "Send prompt to AI model"),
    ("agent", "agent(goal)", "Run an AI agent with a goal"),
    ("swarm", "swarm(config)", "Run a multi-agent swarm"),
    // MCP (Model Context Protocol)
    ("mcp_servers", "mcp_servers()", "List available MCP servers"),
    (
        "mcp_detect",
        "mcp_detect()",
        "Detect MCP servers on network",
    ),
    // Math
    ("abs", "abs(n)", "Absolute value"),
    ("min", "min(a, b)", "Minimum of two values"),
    ("max", "max(a, b)", "Maximum of two values"),
    ("floor", "floor(n)", "Round down"),
    ("ceil", "ceil(n)", "Round up"),
    ("round", "round(n)", "Round to nearest integer"),
    ("sqrt", "sqrt(n)", "Square root"),
    ("pow", "pow(base, exp)", "Raise to power"),
    ("sin", "sin(n)", "Sine"),
    ("cos", "cos(n)", "Cosine"),
    ("tan", "tan(n)", "Tangent"),
    ("log", "log(n)", "Natural logarithm"),
    ("log10", "log10(n)", "Base-10 logarithm"),
    // Option constructors
    ("Some", "Some(value)", "Wrap value in Some"),
    ("None", "None()", "Create None value"),
    // JSON
    ("to_json", "to_json(value)", "Convert to JSON string"),
    ("from_json", "from_json(str)", "Parse JSON string"),
    // Iteration
    (
        "foreach",
        "foreach(fn(x) => action)",
        "Execute action for each element",
    ),
];

/// AetherShell keywords
const KEYWORDS: &[(&str, &str)] = &[
    ("let", "Variable binding: let x = value"),
    ("mut", "Mutable binding: let mut x = value"),
    ("fn", "Lambda function: fn(x) => expr"),
    ("match", "Pattern matching: match value { pattern => expr }"),
    ("if", "Conditional (in guards): if condition"),
    ("true", "Boolean literal true"),
    ("false", "Boolean literal false"),
    ("null", "Null value"),
];

pub fn get_completions(
    store: &DocumentStore,
    uri: &Url,
    position: Position,
) -> Vec<CompletionItem> {
    let mut completions = Vec::new();

    // Get context at cursor position
    let context = get_completion_context(store, uri, position);

    match context {
        CompletionContext::Pipeline => {
            // After a pipe, suggest pipeline-friendly builtins
            for (name, signature, doc) in BUILTINS.iter() {
                if is_pipeline_builtin(name) {
                    completions.push(create_builtin_completion(name, signature, doc));
                }
            }
        }
        CompletionContext::DotAccess => {
            // After a dot, suggest record field access
            completions.push(CompletionItem {
                label: "field_name".to_string(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some("Record field access".to_string()),
                ..Default::default()
            });
        }
        CompletionContext::General => {
            // Add all builtins
            for (name, signature, doc) in BUILTINS.iter() {
                completions.push(create_builtin_completion(name, signature, doc));
            }

            // Add keywords
            for (keyword, doc) in KEYWORDS.iter() {
                completions.push(CompletionItem {
                    label: keyword.to_string(),
                    kind: Some(CompletionItemKind::KEYWORD),
                    detail: Some(doc.to_string()),
                    insert_text: Some(get_keyword_snippet(keyword)),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    ..Default::default()
                });
            }

            // Add variables from document
            if let Some(doc) = store.get(uri) {
                if let Some(ref ast) = doc.ast {
                    for stmt in ast {
                        if let aether_shell::ast::Stmt::Let { name, .. } = stmt {
                            completions.push(CompletionItem {
                                label: name.clone(),
                                kind: Some(CompletionItemKind::VARIABLE),
                                detail: Some("Variable".to_string()),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
    }

    completions
}

pub fn resolve_completion(mut item: CompletionItem) -> CompletionItem {
    // Add detailed documentation for builtins
    for (name, signature, doc) in BUILTINS.iter() {
        if item.label == *name {
            item.documentation = Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```aethershell\n{}\n```\n\n{}", signature, doc),
            }));
            break;
        }
    }
    item
}

pub fn get_signature_help(
    store: &DocumentStore,
    uri: &Url,
    position: Position,
) -> Option<SignatureHelp> {
    // Get the current line and find function call context
    let line = store.get_line(uri, position.line)?;
    let prefix = &line[..position.character as usize];

    // Find the function name being called
    let (func_name, param_index) = find_function_context(prefix)?;

    // Find builtin signature
    for (name, signature, doc) in BUILTINS.iter() {
        if *name == func_name {
            return Some(SignatureHelp {
                signatures: vec![SignatureInformation {
                    label: signature.to_string(),
                    documentation: Some(Documentation::String(doc.to_string())),
                    parameters: extract_parameters(signature),
                    active_parameter: Some(param_index),
                }],
                active_signature: Some(0),
                active_parameter: Some(param_index),
            });
        }
    }

    None
}

#[derive(Debug)]
enum CompletionContext {
    Pipeline,
    DotAccess,
    General,
}

fn get_completion_context(
    store: &DocumentStore,
    uri: &Url,
    position: Position,
) -> CompletionContext {
    if let Some(line) = store.get_line(uri, position.line) {
        let prefix = &line[..position.character as usize];
        let trimmed = prefix.trim_end();

        if trimmed.ends_with('|') {
            return CompletionContext::Pipeline;
        }
        if trimmed.ends_with('.') {
            return CompletionContext::DotAccess;
        }
    }
    CompletionContext::General
}

fn create_builtin_completion(name: &str, signature: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::FUNCTION),
        detail: Some(signature.to_string()),
        documentation: Some(Documentation::String(doc.to_string())),
        insert_text: Some(get_builtin_snippet(name)),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

fn get_builtin_snippet(name: &str) -> String {
    match name {
        "map" => "map(fn(${1:x}) => ${2:x})".to_string(),
        "where" => "where(fn(${1:x}) => ${2:true})".to_string(),
        "reduce" => "reduce(fn(${1:acc}, ${2:x}) => ${3:acc}, ${4:0})".to_string(),
        "foreach" => "foreach(fn(${1:x}) => ${2:print(x)})".to_string(),
        "match" => "match ${1:value} {\n\t${2:pattern} => ${3:result}\n}".to_string(),
        "agent" => "agent(\"${1:goal}\")".to_string(),
        "ai" => "ai(\"${1:prompt}\")".to_string(),
        _ => format!("{}(${{1}})", name),
    }
}

fn get_keyword_snippet(keyword: &str) -> String {
    match keyword {
        "let" => "let ${1:name} = ${2:value}".to_string(),
        "mut" => "mut".to_string(),
        "fn" => "fn(${1:x}) => ${2:x}".to_string(),
        "match" => "match ${1:value} {\n\t${2:pattern} => ${3:result}\n}".to_string(),
        _ => keyword.to_string(),
    }
}

fn is_pipeline_builtin(name: &str) -> bool {
    matches!(
        name,
        "map"
            | "where"
            | "reduce"
            | "take"
            | "first"
            | "last"
            | "any"
            | "all"
            | "flatten"
            | "reverse"
            | "sort"
            | "uniq"
            | "head"
            | "tail"
            | "grep"
            | "wc"
            | "foreach"
            | "print"
            | "split"
            | "join"
            | "trim"
            | "upper"
            | "lower"
            | "slice"
            | "push"
            | "concat"
    )
}

fn find_function_context(prefix: &str) -> Option<(String, u32)> {
    let mut depth = 0;
    let mut param_count = 0u32;
    let mut func_start = None;

    for (i, ch) in prefix.chars().rev().enumerate() {
        match ch {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    func_start = Some(prefix.len() - i - 1);
                    break;
                }
                depth -= 1;
            }
            ',' if depth == 0 => param_count += 1,
            _ => {}
        }
    }

    let start = func_start?;
    let before_paren = &prefix[..start];
    let func_name: String = before_paren
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect();

    if func_name.is_empty() {
        None
    } else {
        Some((func_name, param_count))
    }
}

fn extract_parameters(signature: &str) -> Option<Vec<ParameterInformation>> {
    // Extract parameter names from signature like "map(fn(x) => expr)"
    let start = signature.find('(')?;
    let end = signature.rfind(')')?;
    let params_str = &signature[start + 1..end];

    if params_str.is_empty() {
        return Some(vec![]);
    }

    let params: Vec<ParameterInformation> = params_str
        .split(',')
        .map(|p| ParameterInformation {
            label: ParameterLabel::Simple(p.trim().to_string()),
            documentation: None,
        })
        .collect();

    Some(params)
}
