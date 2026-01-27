//! Code analysis for go-to-definition, references, rename, formatting

use std::collections::HashMap;
use tower_lsp::lsp_types::*;

use crate::document::DocumentStore;

/// Find the definition of the symbol at the given position
pub fn goto_definition(
    store: &DocumentStore,
    uri: &Url,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    let (word, _) = store.get_word_at_position(uri, position)?;

    // Search for variable declaration
    let doc = store.get(uri)?;
    if let Some(ref ast) = doc.ast {
        for (stmt_idx, stmt) in ast.iter().enumerate() {
            if let aethershell::ast::Stmt::Let { name, .. } = stmt {
                if name == &word {
                    // Find the position of this declaration in source
                    let content = doc.content.to_string();
                    if let Some(def_range) = find_let_declaration(&content, name, stmt_idx) {
                        return Some(GotoDefinitionResponse::Scalar(Location {
                            uri: uri.clone(),
                            range: def_range,
                        }));
                    }
                }
            }
        }
    }

    None
}

/// Find all references to the symbol at the given position
pub fn find_references(
    store: &DocumentStore,
    uri: &Url,
    position: Position,
) -> Option<Vec<Location>> {
    let (word, _) = store.get_word_at_position(uri, position)?;
    let content = store.get_content(uri)?;

    let mut locations = Vec::new();

    // Find all occurrences of the identifier
    for (line_idx, line) in content.lines().enumerate() {
        let mut search_start = 0;
        while let Some(col) = line[search_start..].find(&word) {
            let actual_col = search_start + col;

            // Verify it's a complete word (not part of another identifier)
            let is_start_boundary = actual_col == 0
                || !line
                    .chars()
                    .nth(actual_col - 1)
                    .map(|c| c.is_alphanumeric() || c == '_')
                    .unwrap_or(false);

            let is_end_boundary = actual_col + word.len() >= line.len()
                || !line
                    .chars()
                    .nth(actual_col + word.len())
                    .map(|c| c.is_alphanumeric() || c == '_')
                    .unwrap_or(false);

            if is_start_boundary && is_end_boundary {
                locations.push(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: Position {
                            line: line_idx as u32,
                            character: actual_col as u32,
                        },
                        end: Position {
                            line: line_idx as u32,
                            character: (actual_col + word.len()) as u32,
                        },
                    },
                });
            }

            search_start = actual_col + word.len();
        }
    }

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}

/// Rename symbol at position
pub fn rename(
    store: &DocumentStore,
    uri: &Url,
    position: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let references = find_references(store, uri, position)?;

    let edits: Vec<TextEdit> = references
        .into_iter()
        .filter(|loc| &loc.uri == uri)
        .map(|loc| TextEdit {
            range: loc.range,
            new_text: new_name.to_string(),
        })
        .collect();

    if edits.is_empty() {
        return None;
    }

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);

    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

/// Prepare rename - check if rename is valid and return the range
pub fn prepare_rename(
    store: &DocumentStore,
    uri: &Url,
    position: Position,
) -> Option<PrepareRenameResponse> {
    let (word, range) = store.get_word_at_position(uri, position)?;

    // Don't allow renaming keywords or builtins
    const KEYWORDS: &[&str] = &["let", "mut", "fn", "match", "if", "true", "false", "null"];
    const BUILTINS: &[&str] = &[
        "print", "echo", "map", "where", "reduce", "take", "first", "last", "len", "type_of", "ls",
        "pwd", "cat", "head", "tail", "grep", "sort", "uniq", "split", "join", "trim", "upper",
        "lower", "http_get", "ai", "agent", "swarm", "foreach", "range", "keys", "any", "all",
        "flatten", "reverse", "contains", "help", "clear",
    ];

    if KEYWORDS.contains(&word.as_str()) || BUILTINS.contains(&word.as_str()) {
        return None;
    }

    Some(PrepareRenameResponse::Range(range))
}

/// Format document
pub fn format_document(store: &DocumentStore, uri: &Url) -> Option<Vec<TextEdit>> {
    let content = store.get_content(uri)?;
    let formatted = format_aethershell(&content);

    if formatted == content {
        return None;
    }

    // Return a single edit replacing the entire document
    let line_count = content.lines().count();
    let last_line = content.lines().last().unwrap_or("");

    Some(vec![TextEdit {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: line_count as u32,
                character: last_line.len() as u32,
            },
        },
        new_text: formatted,
    }])
}

fn find_let_declaration(content: &str, name: &str, _stmt_idx: usize) -> Option<Range> {
    // Simple search for "let name" or "let mut name"
    for (line_idx, line) in content.lines().enumerate() {
        // Look for let declarations
        if let Some(let_pos) = line.find("let ") {
            let after_let = &line[let_pos + 4..];

            // Check for mut
            let check_str = if after_let.starts_with("mut ") {
                &after_let[4..]
            } else {
                after_let
            };

            // Find the variable name
            let var_start = check_str
                .find(|c: char| c.is_alphanumeric() || c == '_')
                .unwrap_or(0);

            let var_end = check_str[var_start..]
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .map(|i| var_start + i)
                .unwrap_or(check_str.len());

            let var_name = &check_str[var_start..var_end];

            if var_name == name {
                let name_start = line.find(name)?;
                return Some(Range {
                    start: Position {
                        line: line_idx as u32,
                        character: name_start as u32,
                    },
                    end: Position {
                        line: line_idx as u32,
                        character: (name_start + name.len()) as u32,
                    },
                });
            }
        }
    }
    None
}

fn format_aethershell(content: &str) -> String {
    let mut result = String::new();
    let mut indent_level: usize = 0;
    let indent_str = "    "; // 4 spaces

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            result.push('\n');
            continue;
        }

        // Adjust indent for closing braces/brackets
        if trimmed.starts_with('}') || trimmed.starts_with(']') {
            indent_level = indent_level.saturating_sub(1);
        }

        // Write indented line
        for _ in 0..indent_level {
            result.push_str(indent_str);
        }
        result.push_str(trimmed);
        result.push('\n');

        // Adjust indent for opening braces/brackets
        if trimmed.ends_with('{') || trimmed.ends_with('[') {
            indent_level += 1;
        }

        // Handle inline braces (e.g., "match x {")
        if trimmed.contains('{') && !trimmed.ends_with('{') && !trimmed.contains('}') {
            indent_level += 1;
        }
    }

    result
}
