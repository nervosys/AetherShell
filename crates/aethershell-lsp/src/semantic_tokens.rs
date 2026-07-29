//! Semantic tokens for syntax highlighting
//!
//! Provides rich syntax highlighting through LSP semantic tokens

use tower_lsp::lsp_types::*;

use crate::document::DocumentStore;

/// Token types supported by the server
pub static TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::COMMENT,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::TYPE,
    SemanticTokenType::PROPERTY,
];

/// Token modifiers
pub static TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::READONLY,
    SemanticTokenModifier::STATIC,
    SemanticTokenModifier::MODIFICATION,
];

// Token type indices
const TT_KEYWORD: u32 = 0;
const TT_FUNCTION: u32 = 1;
const TT_VARIABLE: u32 = 2;
const TT_STRING: u32 = 3;
const TT_NUMBER: u32 = 4;
const TT_OPERATOR: u32 = 5;
const TT_COMMENT: u32 = 6;
const TT_PARAMETER: u32 = 7;
#[allow(dead_code)]
const TT_TYPE: u32 = 8;
const TT_PROPERTY: u32 = 9;

// Token modifier bits
const TM_DECLARATION: u32 = 1 << 0;
const TM_DEFINITION: u32 = 1 << 1;
#[allow(dead_code)]
const TM_READONLY: u32 = 1 << 2;

/// Keywords in AetherShell
const KEYWORDS: &[&str] = &["let", "mut", "fn", "match", "if", "true", "false", "null"];

/// Built-in functions
const BUILTINS: &[&str] = &[
    "print",
    "echo",
    "help",
    "type_of",
    "len",
    "map",
    "where",
    "reduce",
    "take",
    "first",
    "last",
    "any",
    "all",
    "flatten",
    "reverse",
    "sort",
    "uniq",
    "zip",
    "range",
    "slice",
    "push",
    "concat",
    "keys",
    "split",
    "join",
    "trim",
    "upper",
    "lower",
    "replace",
    "contains",
    "starts_with",
    "ends_with",
    "ls",
    "pwd",
    "cat",
    "read_text",
    "head",
    "tail",
    "find",
    "wc",
    "grep",
    "http_get",
    "ai",
    "agent",
    "swarm",
    "mcp_servers",
    "mcp_detect",
    "abs",
    "min",
    "max",
    "floor",
    "ceil",
    "round",
    "sqrt",
    "pow",
    "sin",
    "cos",
    "tan",
    "log",
    "log10",
    "Some",
    "None",
    "to_json",
    "from_json",
    "foreach",
    "clear",
];

pub fn get_semantic_tokens(store: &DocumentStore, uri: &Url) -> Vec<SemanticToken> {
    let content = match store.get_content(uri) {
        Some(c) => c,
        None => return vec![],
    };

    tokenize(&content)
}

pub fn get_semantic_tokens_range(
    store: &DocumentStore,
    uri: &Url,
    range: Range,
) -> Vec<SemanticToken> {
    let content = match store.get_content(uri) {
        Some(c) => c,
        None => return vec![],
    };

    // Get all tokens and filter by range
    let all_tokens = tokenize(&content);

    // Convert tokens to absolute positions for filtering
    let mut filtered = Vec::new();
    let mut current_line = 0u32;
    let mut current_char = 0u32;

    for token in all_tokens {
        // Calculate absolute position
        current_line += token.delta_line;
        if token.delta_line > 0 {
            current_char = token.delta_start;
        } else {
            current_char += token.delta_start;
        }

        // Check if within range
        if current_line >= range.start.line
            && current_line <= range.end.line
            && (current_line > range.start.line || current_char >= range.start.character)
            && (current_line < range.end.line || current_char + token.length <= range.end.character)
        {
            filtered.push(token);
        }
    }

    filtered
}

fn tokenize(content: &str) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_char = 0u32;

    let lines: Vec<&str> = content.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        let line_num = line_idx as u32;
        let mut char_idx = 0usize;
        let chars: Vec<char> = line.chars().collect();

        while char_idx < chars.len() {
            // Skip whitespace
            if chars[char_idx].is_whitespace() {
                char_idx += 1;
                continue;
            }

            // Check for comments
            if char_idx + 1 < chars.len() && chars[char_idx] == '/' && chars[char_idx + 1] == '/' {
                let start = char_idx;
                let length = chars.len() - char_idx;
                add_token(
                    &mut tokens,
                    line_num,
                    start as u32,
                    length as u32,
                    TT_COMMENT,
                    0,
                    &mut prev_line,
                    &mut prev_char,
                );
                break;
            }

            // Check for strings
            if chars[char_idx] == '"' {
                let start = char_idx;
                char_idx += 1;
                while char_idx < chars.len() && chars[char_idx] != '"' {
                    if chars[char_idx] == '\\' && char_idx + 1 < chars.len() {
                        char_idx += 1;
                    }
                    char_idx += 1;
                }
                if char_idx < chars.len() {
                    char_idx += 1;
                }
                let length = char_idx - start;
                add_token(
                    &mut tokens,
                    line_num,
                    start as u32,
                    length as u32,
                    TT_STRING,
                    0,
                    &mut prev_line,
                    &mut prev_char,
                );
                continue;
            }

            // Check for numbers
            if chars[char_idx].is_ascii_digit()
                || (chars[char_idx] == '-'
                    && char_idx + 1 < chars.len()
                    && chars[char_idx + 1].is_ascii_digit())
            {
                let start = char_idx;
                if chars[char_idx] == '-' {
                    char_idx += 1;
                }
                while char_idx < chars.len()
                    && (chars[char_idx].is_ascii_digit() || chars[char_idx] == '.')
                {
                    char_idx += 1;
                }
                let length = char_idx - start;
                add_token(
                    &mut tokens,
                    line_num,
                    start as u32,
                    length as u32,
                    TT_NUMBER,
                    0,
                    &mut prev_line,
                    &mut prev_char,
                );
                continue;
            }

            // Check for identifiers/keywords
            if chars[char_idx].is_alphabetic() || chars[char_idx] == '_' {
                let start = char_idx;
                while char_idx < chars.len()
                    && (chars[char_idx].is_alphanumeric() || chars[char_idx] == '_')
                {
                    char_idx += 1;
                }
                let word: String = chars[start..char_idx].iter().collect();
                let length = char_idx - start;

                let (token_type, modifiers) = classify_identifier(&word, line, start);
                add_token(
                    &mut tokens,
                    line_num,
                    start as u32,
                    length as u32,
                    token_type,
                    modifiers,
                    &mut prev_line,
                    &mut prev_char,
                );
                continue;
            }

            // Check for operators
            if is_operator(chars[char_idx]) {
                let start = char_idx;
                let mut length = 1;

                // Check for multi-character operators
                if char_idx + 1 < chars.len() {
                    let two_char: String = chars[char_idx..char_idx + 2].iter().collect();
                    if matches!(
                        two_char.as_str(),
                        "=>" | "==" | "!=" | "<=" | ">=" | "&&" | "||"
                    ) {
                        length = 2;
                    }
                }

                add_token(
                    &mut tokens,
                    line_num,
                    start as u32,
                    length as u32,
                    TT_OPERATOR,
                    0,
                    &mut prev_line,
                    &mut prev_char,
                );
                char_idx += length;
                continue;
            }

            char_idx += 1;
        }
    }

    tokens
}

fn classify_identifier(word: &str, line: &str, pos: usize) -> (u32, u32) {
    // Check for keywords
    if KEYWORDS.contains(&word) {
        return (TT_KEYWORD, 0);
    }

    // Check for builtins
    if BUILTINS.contains(&word) {
        return (TT_FUNCTION, 0);
    }

    // Check if this is a declaration (after 'let')
    let before = &line[..pos].trim_end();
    if before.ends_with("let") || before.ends_with("let mut") {
        return (TT_VARIABLE, TM_DECLARATION | TM_DEFINITION);
    }

    // Check if this is a parameter (inside fn())
    if is_parameter_context(line, pos) {
        return (TT_PARAMETER, TM_DECLARATION);
    }

    // Check if this is a property access (after .)
    if pos > 0 && line.chars().nth(pos - 1) == Some('.') {
        return (TT_PROPERTY, 0);
    }

    // Default to variable
    (TT_VARIABLE, 0)
}

fn is_parameter_context(line: &str, pos: usize) -> bool {
    let before = &line[..pos];

    // Look for 'fn(' before this position
    let mut depth = 0;
    for (i, ch) in before.chars().rev().enumerate() {
        match ch {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    // Check if preceded by 'fn'
                    let fn_check_end = before.len() - i - 1;
                    if fn_check_end >= 2 {
                        let fn_check = &before[fn_check_end - 2..fn_check_end];
                        return fn_check == "fn";
                    }
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    false
}

fn is_operator(ch: char) -> bool {
    matches!(
        ch,
        '+' | '-' | '*' | '/' | '%' | '^' | '=' | '<' | '>' | '!' | '&' | '|' | '.'
    )
}

// The LSP semantic-token encoding is positional by specification (line, char,
// length, type, modifiers, plus the running delta state), so the argument list
// mirrors the wire format rather than being decomposable into a struct.
#[allow(clippy::too_many_arguments)]
fn add_token(
    tokens: &mut Vec<SemanticToken>,
    line: u32,
    character: u32,
    length: u32,
    token_type: u32,
    modifiers: u32,
    prev_line: &mut u32,
    prev_char: &mut u32,
) {
    let delta_line = line - *prev_line;
    let delta_start = if delta_line == 0 {
        character - *prev_char
    } else {
        character
    };

    tokens.push(SemanticToken {
        delta_line,
        delta_start,
        length,
        token_type,
        token_modifiers_bitset: modifiers,
    });

    *prev_line = line;
    *prev_char = character;
}
