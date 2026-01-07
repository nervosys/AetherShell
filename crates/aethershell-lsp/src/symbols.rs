//! Document symbols for outline view

use tower_lsp::lsp_types::*;

use crate::document::DocumentStore;

pub fn get_document_symbols(store: &DocumentStore, uri: &Url) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();

    let doc = match store.get(uri) {
        Some(d) => d,
        None => return symbols,
    };

    let content = doc.content.to_string();

    if let Some(ref ast) = doc.ast {
        for stmt in ast {
            if let aether_shell::ast::Stmt::Let {
                name,
                value,
                is_mut,
            } = stmt
            {
                let kind = classify_value_kind(value);
                let detail = if *is_mut {
                    Some("mutable".to_string())
                } else {
                    None
                };

                // Find the position of this declaration
                if let Some(range) = find_symbol_range(&content, name) {
                    #[allow(deprecated)]
                    symbols.push(DocumentSymbol {
                        name: name.clone(),
                        detail,
                        kind,
                        tags: None,
                        deprecated: None,
                        range,
                        selection_range: range,
                        children: None,
                    });
                }
            }
        }
    }

    symbols
}

fn classify_value_kind(expr: &aether_shell::ast::Expr) -> SymbolKind {
    match expr {
        aether_shell::ast::Expr::Lambda { .. } => SymbolKind::FUNCTION,
        aether_shell::ast::Expr::Array(_) => SymbolKind::ARRAY,
        aether_shell::ast::Expr::Record(_) => SymbolKind::STRUCT,
        aether_shell::ast::Expr::LitStr(_) => SymbolKind::STRING,
        aether_shell::ast::Expr::LitInt(_) | aether_shell::ast::Expr::LitFloat(_) => {
            SymbolKind::NUMBER
        }
        aether_shell::ast::Expr::LitBool(_) => SymbolKind::BOOLEAN,
        _ => SymbolKind::VARIABLE,
    }
}

fn find_symbol_range(content: &str, name: &str) -> Option<Range> {
    // Find "let name" or "let mut name"
    for (line_idx, line) in content.lines().enumerate() {
        if let Some(pos) = line.find(&format!("let {}", name)) {
            let name_start = pos + 4; // "let " length
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
        if let Some(pos) = line.find(&format!("let mut {}", name)) {
            let name_start = pos + 8; // "let mut " length
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
    None
}
