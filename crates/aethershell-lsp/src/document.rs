//! Document management for the language server

use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::lsp_types::*;

use aethershell::ast::Stmt;
use aethershell::parser::parse_program;

/// Stores document state for all open files
#[derive(Debug)]
pub struct DocumentStore {
    documents: DashMap<Url, Document>,
}

/// A single document with its content and parsed AST
#[derive(Debug)]
pub struct Document {
    pub content: Rope,
    #[allow(dead_code)]
    pub version: i32,
    pub ast: Option<Vec<Stmt>>,
    pub parse_error: Option<String>,
}

impl DocumentStore {
    pub fn new() -> Self {
        Self {
            documents: DashMap::new(),
        }
    }

    pub fn open(&self, uri: &Url, content: String, version: i32) {
        let rope = Rope::from_str(&content);
        let (ast, parse_error) = parse_document(&content);

        self.documents.insert(
            uri.clone(),
            Document {
                content: rope,
                version,
                ast,
                parse_error,
            },
        );
    }

    pub fn update(&self, uri: &Url, content: String, version: i32) {
        let rope = Rope::from_str(&content);
        let (ast, parse_error) = parse_document(&content);

        self.documents.insert(
            uri.clone(),
            Document {
                content: rope,
                version,
                ast,
                parse_error,
            },
        );
    }

    pub fn close(&self, uri: &Url) {
        self.documents.remove(uri);
    }

    pub fn get(&self, uri: &Url) -> Option<dashmap::mapref::one::Ref<'_, Url, Document>> {
        self.documents.get(uri)
    }

    pub fn get_content(&self, uri: &Url) -> Option<String> {
        self.documents.get(uri).map(|d| d.content.to_string())
    }

    pub fn get_line(&self, uri: &Url, line: u32) -> Option<String> {
        self.documents.get(uri).and_then(|d| {
            let line_idx = line as usize;
            if line_idx < d.content.len_lines() {
                Some(d.content.line(line_idx).to_string())
            } else {
                None
            }
        })
    }

    #[allow(dead_code)]
    pub fn offset_to_position(&self, uri: &Url, offset: usize) -> Option<Position> {
        self.documents.get(uri).map(|d| {
            let line = d.content.char_to_line(offset.min(d.content.len_chars()));
            let line_start = d.content.line_to_char(line);
            let character = offset.saturating_sub(line_start);
            Position {
                line: line as u32,
                character: character as u32,
            }
        })
    }

    #[allow(dead_code)]
    pub fn position_to_offset(&self, uri: &Url, position: Position) -> Option<usize> {
        self.documents.get(uri).and_then(|d| {
            let line = position.line as usize;
            if line < d.content.len_lines() {
                let line_start = d.content.line_to_char(line);
                let line_len = d.content.line(line).len_chars();
                let char_offset = (position.character as usize).min(line_len);
                Some(line_start + char_offset)
            } else {
                None
            }
        })
    }

    pub fn get_word_at_position(&self, uri: &Url, position: Position) -> Option<(String, Range)> {
        let doc = self.get(uri)?;
        let line_idx = position.line as usize;

        if line_idx >= doc.content.len_lines() {
            return None;
        }

        let line = doc.content.line(line_idx).to_string();
        let char_idx = position.character as usize;

        if char_idx > line.len() {
            return None;
        }

        // Find word boundaries
        let chars: Vec<char> = line.chars().collect();
        let mut start = char_idx;
        let mut end = char_idx;

        // Find start of word
        while start > 0 && is_word_char(chars.get(start - 1).copied()) {
            start -= 1;
        }

        // Find end of word
        while end < chars.len() && is_word_char(chars.get(end).copied()) {
            end += 1;
        }

        if start == end {
            return None;
        }

        let word: String = chars[start..end].iter().collect();
        let range = Range {
            start: Position {
                line: position.line,
                character: start as u32,
            },
            end: Position {
                line: position.line,
                character: end as u32,
            },
        };

        Some((word, range))
    }
}

fn is_word_char(ch: Option<char>) -> bool {
    ch.map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false)
}

fn parse_document(content: &str) -> (Option<Vec<Stmt>>, Option<String>) {
    match parse_program(content) {
        Ok(ast) => (Some(ast), None),
        Err(e) => (None, Some(e.to_string())),
    }
}
