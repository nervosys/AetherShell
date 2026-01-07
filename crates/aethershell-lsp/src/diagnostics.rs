//! Diagnostics for AetherShell documents
//!
//! Provides parse error reporting and type checking diagnostics

use tower_lsp::lsp_types::*;

use crate::document::DocumentStore;

/// Analyze a document and return diagnostics
pub fn analyze(store: &DocumentStore, uri: &Url) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if let Some(doc) = store.get(uri) {
        // Parse errors
        if let Some(ref error) = doc.parse_error {
            diagnostics.push(create_parse_error_diagnostic(
                error,
                &doc.content.to_string(),
            ));
        }

        // If we have a valid AST, do additional analysis
        if let Some(ref _ast) = doc.ast {
            // Type checking diagnostics could go here
            // For now, we rely on parse errors
        }
    }

    diagnostics
}

fn create_parse_error_diagnostic(error: &str, content: &str) -> Diagnostic {
    // Try to extract line/column from error message
    let (line, character) = extract_error_position(error, content);

    Diagnostic {
        range: Range {
            start: Position { line, character },
            end: Position {
                line,
                character: character + 1,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String("parse-error".to_string())),
        code_description: None,
        source: Some("aethershell".to_string()),
        message: clean_error_message(error),
        related_information: None,
        tags: None,
        data: None,
    }
}

fn extract_error_position(error: &str, content: &str) -> (u32, u32) {
    // Try to find position hints in error message
    // Common patterns: "at line X", "line X column Y", "position X"
    let error_lower = error.to_lowercase();

    // Try "line X column Y" pattern
    if let Some(line_idx) = error_lower.find("line ") {
        let after_line = &error_lower[line_idx + 5..];
        if let Some(end) = after_line.find(|c: char| !c.is_ascii_digit()) {
            if let Ok(line) = after_line[..end].parse::<u32>() {
                let mut col = 0u32;
                if let Some(col_idx) = after_line.find("column ") {
                    let after_col = &after_line[col_idx + 7..];
                    if let Some(end) = after_col.find(|c: char| !c.is_ascii_digit()) {
                        if let Ok(c) = after_col[..end].parse::<u32>() {
                            col = c.saturating_sub(1);
                        }
                    }
                }
                return (line.saturating_sub(1), col);
            }
        }
    }

    // Try to find the problematic token/character in the error
    // and locate it in the source
    if let Some(quote_start) = error.find('\'') {
        if let Some(quote_end) = error[quote_start + 1..].find('\'') {
            let token = &error[quote_start + 1..quote_start + 1 + quote_end];
            if let Some(pos) = content.find(token) {
                return offset_to_position(content, pos);
            }
        }
    }

    // Default to first line
    (0, 0)
}

fn offset_to_position(content: &str, offset: usize) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;

    for (i, ch) in content.chars().enumerate() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    (line, col)
}

fn clean_error_message(error: &str) -> String {
    // Remove anyhow context wrapping if present
    error
        .trim()
        .replace("parse error: ", "")
        .replace("Parse error: ", "")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_to_position() {
        let content = "hello\nworld\nfoo";
        assert_eq!(offset_to_position(content, 0), (0, 0));
        assert_eq!(offset_to_position(content, 5), (0, 5));
        assert_eq!(offset_to_position(content, 6), (1, 0));
        assert_eq!(offset_to_position(content, 11), (1, 5));
    }
}
