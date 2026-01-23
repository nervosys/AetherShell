use anyhow::{anyhow, Result};

use crate::ast::{BinOp, CfgCondition, ExportItem, Expr, ImportItem, Stmt, UnOp, Visibility};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    ColonEqual, // := walrus operator
    Dot,
    Pipe,
    Equal,
    FatArrow,
    Plus,
    Minus,
    Star,
    Caret,
    Slash,
    Percent,
    Bang,
    Lt,
    Lte,
    Gt,
    Gte,
    EqEq,
    Ne,
    AndAnd,
    OrOr,
    Fn,
    Let,
    Mut,
    Pub,
    Export,
    True,
    False,
    Null,
    Match,
    If,
    Import,
    From,
    As,
    Async, // async keyword
    Await, // await keyword
    Try,   // try keyword
    Catch, // catch keyword
    Throw, // throw keyword
    Ident,
    String,
    Int,
    Float,
    Attribute, // #[...] attribute
    Eof,
}

#[derive(Debug, Clone)]
struct Spanned {
    kind: Tok,
    text: String, // literal/ident text where relevant
    line: usize,  // 1-based line number
    col: usize,   // 1-based column number
}

pub struct Parser {
    toks: Vec<Spanned>,
    i: usize,
    // When true, allow the space-separated word-call sugar in parse_postfix.
    // This should only be enabled for top-level expressions (or explicitly
    // when parsing the RHS of a pipeline). It must be disabled when parsing
    // nested expression bodies like lambda bodies to avoid greedy consumption
    // of trailing atoms that belong to an outer call.
    allow_word_call: bool,
}

/// Parse a program, returning statements and any errors encountered.
/// Uses error recovery to report multiple errors when possible.
pub fn parse_program(src: &str) -> Result<Vec<Stmt>> {
    let toks = lex(src)?;
    let mut p = Parser {
        toks,
        i: 0,
        allow_word_call: false,
    };

    let mut stmts = Vec::new();
    let mut errors = Vec::new();

    while !p.check(Tok::Eof) {
        match p.parse_stmt() {
            Ok(s) => stmts.push(s),
            Err(e) => {
                errors.push(e);
                // Try to recover by synchronizing to a safe point
                p.synchronize();
            }
        }
    }

    // If we had errors, return them combined
    if !errors.is_empty() {
        let error_messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        return Err(anyhow!(
            "found {} error(s):\n  {}",
            errors.len(),
            error_messages.join("\n  ")
        ));
    }

    Ok(stmts)
}

/// Parse a program without error recovery (stops at first error).
/// Useful for cases where partial parsing is not desired.
pub fn parse_program_strict(src: &str) -> Result<Vec<Stmt>> {
    let toks = lex(src)?;
    let mut p = Parser {
        toks,
        i: 0,
        allow_word_call: false,
    };
    let mut stmts = Vec::new();
    while !p.check(Tok::Eof) {
        let s = p.parse_stmt()?;
        stmts.push(s);
    }
    Ok(stmts)
}

// ============ Lexer ============

fn lex(src: &str) -> Result<Vec<Spanned>> {
    let mut out = Vec::<Spanned>::new();
    let chars: Vec<char> = src.chars().collect();
    let mut pos = 0;
    let mut line = 1usize;
    let mut col = 1usize;

    // Helper to create and push a token
    macro_rules! tok {
        ($kind:expr, $text:expr) => {{
            out.push(Spanned {
                kind: $kind,
                text: $text.to_string(),
                line,
                col,
            });
        }};
    }

    // Helper: advance position and update line/col
    fn advance(chars: &[char], pos: &mut usize, line: &mut usize, col: &mut usize) -> Option<char> {
        if *pos < chars.len() {
            let c = chars[*pos];
            *pos += 1;
            if c == '\n' {
                *line += 1;
                *col = 1;
            } else {
                *col += 1;
            }
            Some(c)
        } else {
            None
        }
    }

    // Helper: peek at current char
    fn peek(chars: &[char], pos: usize) -> Option<char> {
        if pos < chars.len() {
            Some(chars[pos])
        } else {
            None
        }
    }

    // Helper: peek at next char
    fn peek_next(chars: &[char], pos: usize) -> Option<char> {
        if pos + 1 < chars.len() {
            Some(chars[pos + 1])
        } else {
            None
        }
    }

    // Helper: read number (optionally with a leading '-' already consumed)
    fn read_number(
        chars: &[char],
        pos: &mut usize,
        line: &mut usize,
        col: &mut usize,
        mut acc: String,
    ) -> (Tok, String) {
        // digits before decimal
        while let Some(ch) = peek(chars, *pos) {
            if ch.is_ascii_digit() {
                acc.push(ch);
                advance(chars, pos, line, col);
            } else {
                break;
            }
        }
        // optional fraction
        if peek(chars, *pos) == Some('.') {
            // Check it's not followed by another dot (like range operator ..)
            if peek_next(chars, *pos) != Some('.') {
                acc.push('.');
                advance(chars, pos, line, col);
                while let Some(ch) = peek(chars, *pos) {
                    if ch.is_ascii_digit() {
                        acc.push(ch);
                        advance(chars, pos, line, col);
                    } else {
                        break;
                    }
                }
                return (Tok::Float, acc);
            }
        }
        (Tok::Int, acc)
    }

    while let Some(c) = peek(&chars, pos) {
        let start_line = line;
        let start_col = col;

        match c {
            ' ' | '\t' | '\r' | '\n' => {
                advance(&chars, &mut pos, &mut line, &mut col);
            }
            '(' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                tok!(Tok::LParen, "(");
            }
            ')' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                tok!(Tok::RParen, ")");
            }
            '[' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                tok!(Tok::LBracket, "[");
            }
            ']' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                tok!(Tok::RBracket, "]");
            }
            '{' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                tok!(Tok::LBrace, "{");
            }
            '}' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                tok!(Tok::RBrace, "}");
            }
            ',' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                tok!(Tok::Comma, ",");
            }
            ':' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                if peek(&chars, pos) == Some('=') {
                    advance(&chars, &mut pos, &mut line, &mut col);
                    out.push(Spanned {
                        kind: Tok::ColonEqual,
                        text: ":=".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                } else {
                    out.push(Spanned {
                        kind: Tok::Colon,
                        text: ":".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                }
            }
            '.' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                out.push(Spanned {
                    kind: Tok::Dot,
                    text: ".".to_string(),
                    line: start_line,
                    col: start_col,
                });
            }
            '|' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                if peek(&chars, pos) == Some('|') {
                    advance(&chars, &mut pos, &mut line, &mut col);
                    out.push(Spanned {
                        kind: Tok::OrOr,
                        text: "||".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                } else {
                    out.push(Spanned {
                        kind: Tok::Pipe,
                        text: "|".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                }
            }
            '=' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                if peek(&chars, pos) == Some('>') {
                    advance(&chars, &mut pos, &mut line, &mut col);
                    out.push(Spanned {
                        kind: Tok::FatArrow,
                        text: "=>".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                } else if peek(&chars, pos) == Some('=') {
                    advance(&chars, &mut pos, &mut line, &mut col);
                    out.push(Spanned {
                        kind: Tok::EqEq,
                        text: "==".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                } else {
                    out.push(Spanned {
                        kind: Tok::Equal,
                        text: "=".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                }
            }
            '!' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                if peek(&chars, pos) == Some('=') {
                    advance(&chars, &mut pos, &mut line, &mut col);
                    out.push(Spanned {
                        kind: Tok::Ne,
                        text: "!=".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                } else {
                    out.push(Spanned {
                        kind: Tok::Bang,
                        text: "!".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                }
            }
            '<' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                if peek(&chars, pos) == Some('=') {
                    advance(&chars, &mut pos, &mut line, &mut col);
                    out.push(Spanned {
                        kind: Tok::Lte,
                        text: "<=".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                } else {
                    out.push(Spanned {
                        kind: Tok::Lt,
                        text: "<".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                }
            }
            '>' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                if peek(&chars, pos) == Some('=') {
                    advance(&chars, &mut pos, &mut line, &mut col);
                    out.push(Spanned {
                        kind: Tok::Gte,
                        text: ">=".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                } else {
                    out.push(Spanned {
                        kind: Tok::Gt,
                        text: ">".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                }
            }
            '&' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                if peek(&chars, pos) == Some('&') {
                    advance(&chars, &mut pos, &mut line, &mut col);
                    out.push(Spanned {
                        kind: Tok::AndAnd,
                        text: "&&".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                } else {
                    return Err(anyhow!(
                        "unknown character '&' at line {}, column {}",
                        start_line,
                        start_col
                    ));
                }
            }
            '+' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                out.push(Spanned {
                    kind: Tok::Plus,
                    text: "+".to_string(),
                    line: start_line,
                    col: start_col,
                });
            }
            '-' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                // negative number if followed by digit
                if let Some(d) = peek(&chars, pos) {
                    if d.is_ascii_digit() {
                        let (kind, text) =
                            read_number(&chars, &mut pos, &mut line, &mut col, "-".to_string());
                        out.push(Spanned {
                            kind,
                            text,
                            line: start_line,
                            col: start_col,
                        });
                        continue;
                    }
                }
                out.push(Spanned {
                    kind: Tok::Minus,
                    text: "-".to_string(),
                    line: start_line,
                    col: start_col,
                });
            }
            '*' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                out.push(Spanned {
                    kind: Tok::Star,
                    text: "*".to_string(),
                    line: start_line,
                    col: start_col,
                });
            }
            '^' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                out.push(Spanned {
                    kind: Tok::Caret,
                    text: "^".to_string(),
                    line: start_line,
                    col: start_col,
                });
            }
            '/' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                // Check for line comment //
                if peek(&chars, pos) == Some('/') {
                    advance(&chars, &mut pos, &mut line, &mut col); // consume second /
                                                                    // Skip until end of line
                    while let Some(ch) = peek(&chars, pos) {
                        if ch == '\n' {
                            break;
                        }
                        advance(&chars, &mut pos, &mut line, &mut col);
                    }
                    continue; // Don't push a token, just skip the comment
                }
                out.push(Spanned {
                    kind: Tok::Slash,
                    text: "/".to_string(),
                    line: start_line,
                    col: start_col,
                });
            }
            '%' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                out.push(Spanned {
                    kind: Tok::Percent,
                    text: "%".to_string(),
                    line: start_line,
                    col: start_col,
                });
            }
            '"' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                let mut s = String::new();
                while let Some(ch) = peek(&chars, pos) {
                    advance(&chars, &mut pos, &mut line, &mut col);
                    if ch == '"' {
                        break;
                    }
                    if ch == '\\' {
                        if let Some(esc) = peek(&chars, pos) {
                            advance(&chars, &mut pos, &mut line, &mut col);
                            s.push(match esc {
                                'n' => '\n',
                                'r' => '\r',
                                't' => '\t',
                                '\\' => '\\',
                                '"' => '"',
                                other => other,
                            });
                        }
                    } else {
                        s.push(ch);
                    }
                }
                out.push(Spanned {
                    kind: Tok::String,
                    text: s,
                    line: start_line,
                    col: start_col,
                });
            }
            '\'' => {
                advance(&chars, &mut pos, &mut line, &mut col);
                let mut s = String::new();
                while let Some(ch) = peek(&chars, pos) {
                    advance(&chars, &mut pos, &mut line, &mut col);
                    if ch == '\'' {
                        break;
                    }
                    s.push(ch);
                }
                out.push(Spanned {
                    kind: Tok::String,
                    text: s,
                    line: start_line,
                    col: start_col,
                });
            }
            d if d.is_ascii_digit() => {
                let (kind, text) =
                    read_number(&chars, &mut pos, &mut line, &mut col, String::new());
                out.push(Spanned {
                    kind,
                    text,
                    line: start_line,
                    col: start_col,
                });
            }
            c if is_ident_start(c) => {
                let mut s = String::new();
                while let Some(ch) = peek(&chars, pos) {
                    if is_ident_part(ch) {
                        s.push(ch);
                        advance(&chars, &mut pos, &mut line, &mut col);
                    } else {
                        break;
                    }
                }
                let kw = match s.as_str() {
                    "fn" => Tok::Fn,
                    "let" => Tok::Let,
                    "mut" => Tok::Mut,
                    "pub" => Tok::Pub,
                    "export" => Tok::Export,
                    "true" => Tok::True,
                    "false" => Tok::False,
                    "null" => Tok::Null,
                    "match" => Tok::Match,
                    "if" => Tok::If,
                    "import" => Tok::Import,
                    "from" => Tok::From,
                    "as" => Tok::As,
                    "async" => Tok::Async,
                    "await" => Tok::Await,
                    "try" => Tok::Try,
                    "catch" => Tok::Catch,
                    "throw" => Tok::Throw,
                    _ => Tok::Ident,
                };
                out.push(Spanned {
                    kind: kw,
                    text: s,
                    line: start_line,
                    col: start_col,
                });
            }
            ';' => {
                // treat semicolon as statement separator; ignore it
                advance(&chars, &mut pos, &mut line, &mut col);
            }
            '#' => {
                advance(&chars, &mut pos, &mut line, &mut col); // consume #
                                                                // Check if this is an attribute #[...]
                if peek(&chars, pos) == Some('[') {
                    advance(&chars, &mut pos, &mut line, &mut col); // consume [
                                                                    // Read the attribute content until ]
                    let mut attr = String::new();
                    let mut depth = 1;
                    while let Some(ch) = peek(&chars, pos) {
                        advance(&chars, &mut pos, &mut line, &mut col);
                        if ch == '[' {
                            depth += 1;
                            attr.push(ch);
                        } else if ch == ']' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            attr.push(ch);
                        } else {
                            attr.push(ch);
                        }
                    }
                    // Push special attribute token
                    out.push(Spanned {
                        kind: Tok::Attribute,
                        text: attr,
                        line: start_line,
                        col: start_col,
                    });
                    continue;
                }
                // Otherwise it's a shell-style line comment
                // Skip until end of line
                while let Some(ch) = peek(&chars, pos) {
                    if ch == '\n' {
                        break;
                    }
                    advance(&chars, &mut pos, &mut line, &mut col);
                }
                continue; // Don't push a token, just skip the comment
            }
            other => {
                return Err(anyhow!(
                    "unknown character '{}' at line {}, column {}",
                    other,
                    start_line,
                    start_col
                ));
            }
        }
    }

    out.push(Spanned {
        kind: Tok::Eof,
        text: String::new(),
        line,
        col,
    });
    Ok(out)
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}
fn is_ident_part(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

// ============ Parser ============

impl Parser {
    fn parse_stmt(&mut self) -> Result<Stmt> {
        // Check for #[cfg(...)] attribute
        if self.check(Tok::Attribute) {
            let attr_text = self.peek().text.clone();
            self.i += 1; // consume the attribute token

            // Parse the cfg condition from the attribute text
            let condition = self.parse_cfg_condition(&attr_text)?;

            // Parse the following statement
            let body = self.parse_stmt()?;

            return Ok(Stmt::Cfg {
                condition,
                body: Box::new(body),
            });
        }

        // Check for `pub` visibility modifier
        let visibility = if self.match_tok(Tok::Pub) {
            Visibility::Pub
        } else {
            Visibility::Private
        };

        // Check for `mut name = value` (mutable without let)
        if self.check(Tok::Mut) && self.peek_ahead(1) == Some(Tok::Ident) {
            let peek2 = self.peek_ahead(2);
            if peek2 == Some(Tok::Equal) || peek2 == Some(Tok::ColonEqual) {
                self.match_tok(Tok::Mut); // consume 'mut'
                let name = self.need_ident("expected identifier after mut")?;
                // Accept either = or :=
                if !self.match_tok(Tok::Equal) {
                    self.need(Tok::ColonEqual, "expected '=' or ':='")?;
                }
                let value = self.parse_expr()?;
                return Ok(Stmt::Let {
                    name,
                    value,
                    is_mut: true,
                    visibility,
                });
            }
        }

        // Check for `name = value` or `name := value` (simple assignment with type inference)
        if self.check(Tok::Ident) {
            let peek1 = self.peek_ahead(1);
            if peek1 == Some(Tok::Equal) || peek1 == Some(Tok::ColonEqual) {
                let name = self.need_ident("expected identifier")?;
                // Accept either = or :=
                if !self.match_tok(Tok::Equal) {
                    self.need(Tok::ColonEqual, "expected '=' or ':='")?;
                }
                let value = self.parse_expr()?;
                return Ok(Stmt::Let {
                    name,
                    value,
                    is_mut: false,
                    visibility,
                });
            }
        }

        if self.match_tok(Tok::Let) {
            // Keep let/let mut for explicit declarations
            let is_mut = self.match_tok(Tok::Mut);
            let name = self.need_ident("expected identifier after let")?;
            if self.match_tok(Tok::Colon) {
                let _ = self.need_ident("expected type after ':'")?;
            }
            // Accept either = or :=
            if !self.match_tok(Tok::Equal) {
                self.need(Tok::ColonEqual, "expected '=' or ':=' in let")?;
            }
            let value = self.parse_expr()?;
            Ok(Stmt::Let {
                name,
                value,
                is_mut,
                visibility,
            })
        } else if self.match_tok(Tok::Import) {
            // Parse import statement
            if visibility == Visibility::Pub {
                return Err(self.error_at_prev("'pub' cannot be used with import statements"));
            }
            self.parse_import()
        } else if self.match_tok(Tok::Export) {
            // Parse export statement
            if visibility == Visibility::Pub {
                return Err(self.error_at_prev("'pub' cannot be used with export statements"));
            }
            self.parse_export()
        } else {
            // Top-level statement: allow word-call sugar (e.g. `print "hi"").
            if visibility == Visibility::Pub {
                return Err(
                    self.error_at_prev("'pub' can only be used with let/assignment statements")
                );
            }
            let prev = self.allow_word_call;
            self.allow_word_call = true;
            let e = self.parse_expr()?;
            self.allow_word_call = prev;
            Ok(Stmt::Expr(e))
        }
    }

    /// Parse import statement
    /// Syntax:
    ///   import "path"
    ///   import "path" as alias
    ///   import { a, b } from "path"
    ///   import { a as x, b } from "path"
    fn parse_import(&mut self) -> Result<Stmt> {
        let mut items = Vec::new();
        let mut alias = None;

        // Check for destructuring import: import { ... } from "path"
        if self.match_tok(Tok::LBrace) {
            // Parse import items
            loop {
                if self.check(Tok::RBrace) {
                    break;
                }
                let name = self.need_ident("expected import name")?;
                let item_alias = if self.match_tok(Tok::As) {
                    Some(self.need_ident("expected alias after 'as'")?)
                } else {
                    None
                };
                items.push(ImportItem {
                    name,
                    alias: item_alias,
                });
                if !self.match_tok(Tok::Comma) {
                    break;
                }
            }
            self.need(Tok::RBrace, "expected '}' after import list")?;
            self.need(Tok::From, "expected 'from' after import list")?;
        }

        // Parse source path (string)
        let source = self.need_string("expected module path string")?;

        // Check for alias: import "path" as name
        if items.is_empty() && self.match_tok(Tok::As) {
            alias = Some(self.need_ident("expected alias after 'as'")?);
        }

        Ok(Stmt::Import {
            items,
            source,
            alias,
        })
    }

    /// Parse export statement
    /// Syntax:
    ///   export { a, b }
    ///   export { a as x, b }
    ///   export { a, b } from "path"  (re-export)
    fn parse_export(&mut self) -> Result<Stmt> {
        self.need(Tok::LBrace, "expected '{' after export")?;

        let mut items = Vec::new();

        // Parse export items
        loop {
            if self.check(Tok::RBrace) {
                break;
            }
            let name = self.need_ident("expected export name")?;
            let item_alias = if self.match_tok(Tok::As) {
                Some(self.need_ident("expected alias after 'as'")?)
            } else {
                None
            };
            items.push(ExportItem {
                name,
                alias: item_alias,
            });
            if !self.match_tok(Tok::Comma) {
                break;
            }
        }

        self.need(Tok::RBrace, "expected '}' after export list")?;

        // Check for re-export: export { ... } from "path"
        let from_source = if self.match_tok(Tok::From) {
            Some(self.need_string("expected module path string after 'from'")?)
        } else {
            None
        };

        Ok(Stmt::Export { items, from_source })
    }

    /// Parse a cfg condition from attribute text like "cfg(windows)" or "cfg(feature = \"name\")"
    fn parse_cfg_condition(&self, attr: &str) -> Result<CfgCondition> {
        let attr = attr.trim();

        // Must start with "cfg("
        if !attr.starts_with("cfg(") || !attr.ends_with(')') {
            return Err(self.error_at_prev(&format!(
                "invalid cfg attribute: expected cfg(...), got '{}'",
                attr
            )));
        }

        // Extract the inner content
        let inner = &attr[4..attr.len() - 1];
        self.parse_cfg_inner(inner.trim())
    }

    fn parse_cfg_inner(&self, content: &str) -> Result<CfgCondition> {
        let content = content.trim();

        // Check for not(...)
        if content.starts_with("not(") && content.ends_with(')') {
            let inner = &content[4..content.len() - 1];
            let cond = self.parse_cfg_inner(inner.trim())?;
            return Ok(CfgCondition::Not(Box::new(cond)));
        }

        // Check for all(...)
        if content.starts_with("all(") && content.ends_with(')') {
            let inner = &content[4..content.len() - 1];
            let conditions = self.parse_cfg_list(inner)?;
            return Ok(CfgCondition::All(conditions));
        }

        // Check for any(...)
        if content.starts_with("any(") && content.ends_with(')') {
            let inner = &content[4..content.len() - 1];
            let conditions = self.parse_cfg_list(inner)?;
            return Ok(CfgCondition::Any(conditions));
        }

        // Check for feature = "name"
        if content.starts_with("feature") {
            let rest = content["feature".len()..].trim();
            if rest.starts_with('=') {
                let value = rest[1..].trim();
                // Remove quotes
                let value = value.trim_matches('"').trim_matches('\'');
                return Ok(CfgCondition::Feature(value.to_string()));
            }
            return Err(self.error_at_prev("invalid feature cfg: expected feature = \"name\""));
        }

        // Otherwise it's a platform name
        Ok(CfgCondition::Platform(content.to_string()))
    }

    fn parse_cfg_list(&self, content: &str) -> Result<Vec<CfgCondition>> {
        let mut conditions = Vec::new();
        let mut depth = 0;
        let mut current = String::new();

        for ch in content.chars() {
            match ch {
                '(' => {
                    depth += 1;
                    current.push(ch);
                }
                ')' => {
                    depth -= 1;
                    current.push(ch);
                }
                ',' if depth == 0 => {
                    if !current.trim().is_empty() {
                        conditions.push(self.parse_cfg_inner(current.trim())?);
                    }
                    current.clear();
                }
                _ => current.push(ch),
            }
        }

        if !current.trim().is_empty() {
            conditions.push(self.parse_cfg_inner(current.trim())?);
        }

        Ok(conditions)
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_pipe()
    }

    fn parse_pipe(&mut self) -> Result<Expr> {
        let mut left = self.parse_logic_or()?;
        while self.match_tok(Tok::Pipe) {
            let right = self.parse_call_like()?;
            left = Expr::Pipe {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// Word-call support: if the rhs begins with an identifier, slurp a sequence of
    /// atoms/lambdas as arguments to that identifier.
    fn parse_call_like(&mut self) -> Result<Expr> {
        if self.check(Tok::Fn) {
            return self.parse_lambda();
        }
        if self.check(Tok::Async) {
            return self.parse_async_lambda();
        }
        let primary = self.parse_atom_expr()?;
        if let Expr::Ident(_) = primary {
            let callee = primary;
            // If the next token is a parenthesis, parse a normal call with
            // comma-separated expressions (handles e.g. map(fn(x)=>..., 0)).
            if self.check(Tok::LParen) {
                self.match_tok(Tok::LParen);
                let mut args = Vec::new();
                if !self.check(Tok::RParen) {
                    loop {
                        let a = self.parse_expr()?;
                        args.push(a);
                        if self.match_tok(Tok::Comma) {
                            continue;
                        }
                        break;
                    }
                }
                self.need(Tok::RParen, "expected ')' after arguments")?;
                return Ok(Expr::Call {
                    callee: Box::new(callee),
                    args,
                    named: Vec::new(),
                });
            }

            // Otherwise support word-call style: space-separated atoms and lambdas
            let mut args: Vec<Expr> = Vec::new();
            while self.check_any(&[
                Tok::Fn,
                Tok::Async,
                Tok::String,
                Tok::Int,
                Tok::Float,
                Tok::LBrace,
                Tok::LBracket,
                Tok::Ident,
                Tok::LParen,
                Tok::True,
                Tok::False,
                Tok::Null,
            ]) {
                if self.check(Tok::Fn) {
                    let lam = self.parse_lambda()?;
                    args.push(lam);
                    continue;
                }
                if self.check(Tok::Async) {
                    let lam = self.parse_async_lambda()?;
                    args.push(lam);
                    continue;
                }
                if self.check_any(&[Tok::Pipe, Tok::RBrace, Tok::RParen, Tok::Eof]) {
                    break;
                }
                let a = self.parse_atom_expr()?;
                args.push(a);
            }
            if args.is_empty() {
                Ok(callee)
            } else {
                Ok(Expr::Call {
                    callee: Box::new(callee),
                    args,
                    named: Vec::new(),
                })
            }
        } else {
            Ok(primary)
        }
    }

    fn parse_logic_or(&mut self) -> Result<Expr> {
        let mut e = self.parse_logic_and()?;
        while self.match_tok(Tok::OrOr) {
            let r = self.parse_logic_and()?;
            e = Expr::Binary {
                left: Box::new(e),
                op: BinOp::Or,
                right: Box::new(r),
            };
        }
        Ok(e)
    }

    fn parse_logic_and(&mut self) -> Result<Expr> {
        let mut e = self.parse_equality()?;
        while self.match_tok(Tok::AndAnd) {
            let r = self.parse_equality()?;
            e = Expr::Binary {
                left: Box::new(e),
                op: BinOp::And,
                right: Box::new(r),
            };
        }
        Ok(e)
    }

    fn parse_equality(&mut self) -> Result<Expr> {
        let mut e = self.parse_comparison()?;
        loop {
            if self.match_tok(Tok::EqEq) {
                let r = self.parse_comparison()?;
                e = Expr::Binary {
                    left: Box::new(e),
                    op: BinOp::Eq,
                    right: Box::new(r),
                };
            } else if self.match_tok(Tok::Ne) {
                let r = self.parse_comparison()?;
                e = Expr::Binary {
                    left: Box::new(e),
                    op: BinOp::Ne,
                    right: Box::new(r),
                };
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn parse_comparison(&mut self) -> Result<Expr> {
        let mut e = self.parse_term()?;
        loop {
            if self.match_tok(Tok::Lt) {
                let r = self.parse_term()?;
                e = Expr::Binary {
                    left: Box::new(e),
                    op: BinOp::Lt,
                    right: Box::new(r),
                };
            } else if self.match_tok(Tok::Lte) {
                let r = self.parse_term()?;
                e = Expr::Binary {
                    left: Box::new(e),
                    op: BinOp::Lte,
                    right: Box::new(r),
                };
            } else if self.match_tok(Tok::Gt) {
                let r = self.parse_term()?;
                e = Expr::Binary {
                    left: Box::new(e),
                    op: BinOp::Gt,
                    right: Box::new(r),
                };
            } else if self.match_tok(Tok::Gte) {
                let r = self.parse_term()?;
                e = Expr::Binary {
                    left: Box::new(e),
                    op: BinOp::Gte,
                    right: Box::new(r),
                };
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn parse_term(&mut self) -> Result<Expr> {
        let mut e = self.parse_factor()?;
        loop {
            if self.match_tok(Tok::Plus) {
                let r = self.parse_factor()?;
                e = Expr::Binary {
                    left: Box::new(e),
                    op: BinOp::Add,
                    right: Box::new(r),
                };
            } else if self.match_tok(Tok::Minus) {
                let r = self.parse_factor()?;
                e = Expr::Binary {
                    left: Box::new(e),
                    op: BinOp::Sub,
                    right: Box::new(r),
                };
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn parse_factor(&mut self) -> Result<Expr> {
        // Handle multiplicative operators and exponentiation (right-assoc)
        let mut e = self.parse_power()?;
        loop {
            if self.match_tok(Tok::Star) {
                let r = self.parse_power()?;
                e = Expr::Binary {
                    left: Box::new(e),
                    op: BinOp::Mul,
                    right: Box::new(r),
                };
            } else if self.match_tok(Tok::Slash) {
                let r = self.parse_power()?;
                e = Expr::Binary {
                    left: Box::new(e),
                    op: BinOp::Div,
                    right: Box::new(r),
                };
            } else if self.match_tok(Tok::Percent) {
                let r = self.parse_power()?;
                e = Expr::Binary {
                    left: Box::new(e),
                    op: BinOp::Rem,
                    right: Box::new(r),
                };
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn parse_power(&mut self) -> Result<Expr> {
        // Right-associative exponentiation: a ^ b ^ c -> a ^ (b ^ c)
        let mut left = self.parse_unary()?;
        if self.match_tok(Tok::Caret) {
            let right = self.parse_power()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinOp::Pow,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        if self.match_tok(Tok::Bang) {
            let e = self.parse_unary()?;
            Ok(Expr::Unary {
                op: UnOp::Not,
                expr: Box::new(e),
            })
        } else if self.match_tok(Tok::Minus) {
            let e = self.parse_unary()?;
            Ok(Expr::Unary {
                op: UnOp::Neg,
                expr: Box::new(e),
            })
        } else if self.match_tok(Tok::Await) {
            // await expr - await the result of an async expression
            let e = self.parse_unary()?;
            Ok(Expr::Await(Box::new(e)))
        } else if self.match_tok(Tok::Throw) {
            // throw expr - raise an error
            let e = self.parse_unary()?;
            Ok(Expr::Throw(Box::new(e)))
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut e = self.parse_atom_expr()?;
        loop {
            if self.match_tok(Tok::LParen) {
                let mut args = Vec::new();
                if !self.check(Tok::RParen) {
                    loop {
                        let a = self.parse_expr()?;
                        args.push(a);
                        if self.match_tok(Tok::Comma) {
                            continue;
                        }
                        break;
                    }
                }
                self.need(Tok::RParen, "expected ')' after arguments")?;
                e = Expr::Call {
                    callee: Box::new(e),
                    args,
                    named: Vec::new(),
                };
            } else if self.match_tok(Tok::Dot) {
                // Member access: obj.field
                let field = self.need_ident("expected field name after '.'")?;
                e = Expr::MemberAccess {
                    object: Box::new(e),
                    field,
                };
            } else {
                break;
            }
        }
        // Support word-call (space-separated) style for top-level identifiers
        // e.g. `print "hi"` should parse like `print("hi")`. Only enable
        // this when the parser flag `allow_word_call` is set to avoid greedily
        // consuming tokens inside nested expressions like lambda bodies.
        if self.allow_word_call {
            if let Expr::Ident(_) = e {
                let mut args: Vec<Expr> = Vec::new();
                while self.check_any(&[
                    Tok::Fn,
                    Tok::Async,
                    Tok::String,
                    Tok::Int,
                    Tok::Float,
                    Tok::LBrace,
                    Tok::LBracket,
                    Tok::Ident,
                    Tok::LParen,
                    Tok::True,
                    Tok::False,
                    Tok::Null,
                ]) {
                    if self.check(Tok::Fn) {
                        let lam = self.parse_lambda()?;
                        args.push(lam);
                        continue;
                    }
                    if self.check(Tok::Async) {
                        let lam = self.parse_async_lambda()?;
                        args.push(lam);
                        continue;
                    }
                    if self.check_any(&[Tok::Pipe, Tok::RBrace, Tok::RParen, Tok::Eof]) {
                        break;
                    }
                    let a = self.parse_atom_expr()?;
                    args.push(a);
                }
                if !args.is_empty() {
                    e = Expr::Call {
                        callee: Box::new(e),
                        args,
                        named: Vec::new(),
                    };
                }
            }
        }
        Ok(e)
    }

    fn parse_atom_expr(&mut self) -> Result<Expr> {
        if self.match_tok(Tok::LParen) {
            let e = self.parse_expr()?;
            self.need(Tok::RParen, "expected ')'")?;
            return Ok(e);
        }
        if self.match_tok(Tok::LBracket) {
            let mut items = Vec::new();
            if !self.check(Tok::RBracket) {
                loop {
                    let item = self.parse_expr()?;
                    items.push(item);
                    if self.match_tok(Tok::Comma) {
                        continue;
                    }
                    break;
                }
            }
            self.need(Tok::RBracket, "expected ']'")?;
            return Ok(Expr::Array(items));
        }
        if self.match_tok(Tok::LBrace) {
            let mut kvs = Vec::new();
            if !self.check(Tok::RBrace) {
                loop {
                    let key = self.need_ident("expected key in record")?;
                    self.need(Tok::Colon, "expected ':' after key")?;
                    let val = self.parse_expr()?;
                    kvs.push((key, val));
                    if self.match_tok(Tok::Comma) {
                        continue;
                    }
                    break;
                }
            }
            self.need(Tok::RBrace, "expected '}'")?;
            return Ok(Expr::Record(kvs));
        }
        if self.match_tok(Tok::Match) {
            return self.parse_match();
        }
        if self.match_tok(Tok::Try) {
            return self.parse_try_catch();
        }
        if self.match_tok(Tok::Fn) {
            return self.parse_lambda_after_fn(false);
        }
        if self.match_tok(Tok::Async) {
            self.need(Tok::Fn, "expected 'fn' after 'async'")?;
            return self.parse_lambda_after_fn(true);
        }
        if self.match_tok(Tok::String) {
            let s = self.prev().text.clone();
            return Ok(Expr::LitStr(s));
        }
        if self.match_tok(Tok::Float) {
            let x: f64 = self.prev().text.parse().unwrap_or(0.0);
            return Ok(Expr::LitFloat(x));
        }
        if self.match_tok(Tok::Int) {
            let n: i64 = self.prev().text.parse().unwrap_or(0);
            return Ok(Expr::LitInt(n));
        }
        if self.match_tok(Tok::True) {
            return Ok(Expr::LitBool(true));
        }
        if self.match_tok(Tok::False) {
            return Ok(Expr::LitBool(false));
        }
        if self.match_tok(Tok::Null) {
            return Ok(Expr::Null);
        }
        if self.match_tok(Tok::Ident) {
            let name = self.prev().text.clone();
            return Ok(Expr::Ident(name));
        }
        Err(self.error_at_current(&format!("unexpected token {:?}", self.peek().kind)))
    }

    fn parse_lambda(&mut self) -> Result<Expr> {
        self.need(Tok::Fn, "expected 'fn'")?;
        self.parse_lambda_after_fn(false)
    }

    /// Parse an async lambda: async fn(x) => expr
    fn parse_async_lambda(&mut self) -> Result<Expr> {
        self.need(Tok::Async, "expected 'async'")?;
        self.need(Tok::Fn, "expected 'fn' after 'async'")?;
        self.parse_lambda_after_fn(true)
    }

    fn parse_lambda_after_fn(&mut self, is_async: bool) -> Result<Expr> {
        self.need(Tok::LParen, "expected '(' after fn")?;
        let mut params = Vec::new();
        if !self.check(Tok::RParen) {
            loop {
                let p = self.need_ident("expected parameter name")?;
                params.push(p);
                if self.match_tok(Tok::Comma) {
                    continue;
                }
                break;
            }
        }
        self.need(Tok::RParen, "expected ')' after parameter list")?;
        self.need(Tok::FatArrow, "expected '=>' after parameter list")?;
        // Parse the lambda body - include pipes as part of the body expression.
        // Disable word-call sugar while parsing to avoid greedily consuming
        // trailing atoms that belong to an outer call (e.g. `fn(a,b)=> a+b 0`).
        let prev_allow = self.allow_word_call;
        self.allow_word_call = false;
        let body = self.parse_pipe()?; // Changed from parse_logic_or() to parse_pipe()
        self.allow_word_call = prev_allow;
        if is_async {
            Ok(Expr::AsyncLambda {
                params,
                body: Box::new(body),
            })
        } else {
            Ok(Expr::Lambda {
                params,
                body: Box::new(body),
            })
        }
    }

    fn parse_match(&mut self) -> Result<Expr> {
        // match expr { arms }
        // Disable word-call to prevent consuming the '{' as a record argument
        let prev_allow = self.allow_word_call;
        self.allow_word_call = false;
        let scrutinee = Box::new(self.parse_pipe()?); // parse just the scrutinee, no word-call
        self.allow_word_call = prev_allow;

        self.need(Tok::LBrace, "expected '{' after match expression")?;

        let mut arms = Vec::new();
        while !self.check(Tok::RBrace) && !self.check(Tok::Eof) {
            let arm = self.parse_match_arm()?;
            arms.push(arm);
            // Optional comma after each arm
            self.match_tok(Tok::Comma);
        }

        self.need(Tok::RBrace, "expected '}' after match arms")?;
        Ok(Expr::Match { scrutinee, arms })
    }

    /// Parse try/catch expression:
    /// try { expr } catch { handler }
    /// try { expr } catch e { handler }  (with error binding)
    fn parse_try_catch(&mut self) -> Result<Expr> {
        // 'try' already consumed
        self.need(Tok::LBrace, "expected '{' after 'try'")?;
        let try_expr = self.parse_expr()?;
        self.need(Tok::RBrace, "expected '}' after try expression")?;

        self.need(Tok::Catch, "expected 'catch' after try block")?;

        // Optional error binding: catch e { ... }
        // Check if next token is Ident and token after that is LBrace
        let catch_var = if self.check(Tok::Ident) && self.peek_ahead(1) == Some(Tok::LBrace) {
            let name = self.peek().text.clone();
            self.i += 1;
            Some(name)
        } else {
            None
        };

        self.need(Tok::LBrace, "expected '{' after 'catch'")?;
        let catch_expr = self.parse_expr()?;
        self.need(Tok::RBrace, "expected '}' after catch expression")?;

        Ok(Expr::TryCatch {
            try_expr: Box::new(try_expr),
            catch_var,
            catch_expr: Box::new(catch_expr),
        })
    }

    fn parse_match_arm(&mut self) -> Result<crate::ast::MatchArm> {
        use crate::ast::MatchArm;

        let pattern = self.parse_pattern()?;

        // Optional guard: if condition
        let guard = if self.match_tok(Tok::If) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        self.need(Tok::FatArrow, "expected '=>' in match arm")?;
        let body = Box::new(self.parse_expr()?);

        Ok(MatchArm {
            pattern,
            guard,
            body,
        })
    }

    fn parse_pattern(&mut self) -> Result<crate::ast::Pattern> {
        use crate::ast::Pattern;

        // Wildcard: _
        if self.check(Tok::Ident) && self.peek().text == "_" {
            self.i += 1;
            return Ok(Pattern::Wildcard);
        }

        // Literals
        if self.match_tok(Tok::Int) {
            let n: i64 = self.prev().text.parse().unwrap_or(0);
            return Ok(Pattern::LitInt(n));
        }
        if self.match_tok(Tok::String) {
            let s = self.prev().text.clone();
            return Ok(Pattern::LitStr(s));
        }
        if self.match_tok(Tok::True) {
            return Ok(Pattern::LitBool(true));
        }
        if self.match_tok(Tok::False) {
            return Ok(Pattern::LitBool(false));
        }
        if self.match_tok(Tok::Null) {
            return Ok(Pattern::Null);
        }

        // Array pattern: [p1, p2, ...]
        if self.match_tok(Tok::LBracket) {
            let mut patterns = Vec::new();
            if !self.check(Tok::RBracket) {
                loop {
                    patterns.push(self.parse_pattern()?);
                    if self.match_tok(Tok::Comma) {
                        continue;
                    }
                    break;
                }
            }
            self.need(Tok::RBracket, "expected ']' in array pattern")?;
            return Ok(Pattern::Array(patterns));
        }

        // Record pattern: {field1, field2} or {field1: p1, field2: p2}
        if self.match_tok(Tok::LBrace) {
            let mut fields = Vec::new();
            if !self.check(Tok::RBrace) {
                loop {
                    let key = self.need_ident("expected field name in record pattern")?;
                    let pattern = if self.match_tok(Tok::Colon) {
                        self.parse_pattern()?
                    } else {
                        // Shorthand: {x} means {x: x}
                        Pattern::Ident(key.clone())
                    };
                    fields.push((key, pattern));
                    if self.match_tok(Tok::Comma) {
                        continue;
                    }
                    break;
                }
            }
            self.need(Tok::RBrace, "expected '}' in record pattern")?;
            return Ok(Pattern::Record(fields));
        }

        // Constructor pattern: Name(args...) or just Name
        if self.check(Tok::Ident) {
            let name = self.need_ident("expected pattern")?;

            // Check if it's a constructor call with args
            if self.match_tok(Tok::LParen) {
                let mut args = Vec::new();
                if !self.check(Tok::RParen) {
                    loop {
                        args.push(self.parse_pattern()?);
                        if self.match_tok(Tok::Comma) {
                            continue;
                        }
                        break;
                    }
                }
                self.need(Tok::RParen, "expected ')' in constructor pattern")?;
                return Ok(Pattern::Constructor { name, args });
            }

            // Otherwise it's just a variable binding
            return Ok(Pattern::Ident(name));
        }

        Err(self.error_at_current(&format!(
            "unexpected token in pattern: {:?}",
            self.peek().kind
        )))
    }

    // ---- small helpers ----
    fn check(&self, k: Tok) -> bool {
        self.peek().kind == k
    }
    fn check_any(&self, ks: &[Tok]) -> bool {
        ks.iter().any(|k| self.peek().kind == *k)
    }
    fn peek_ahead(&self, offset: usize) -> Option<Tok> {
        if self.i + offset < self.toks.len() {
            Some(self.toks[self.i + offset].kind)
        } else {
            None
        }
    }
    fn match_tok(&mut self, k: Tok) -> bool {
        if self.check(k) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    /// Create an error message with line/column information at current token
    fn error_at_current(&self, msg: &str) -> anyhow::Error {
        let tok = self.peek();
        anyhow!("{} at line {}, column {}", msg, tok.line, tok.col)
    }

    /// Create an error message with line/column information at previous token
    fn error_at_prev(&self, msg: &str) -> anyhow::Error {
        let tok = self.prev();
        anyhow!("{} at line {}, column {}", msg, tok.line, tok.col)
    }

    fn need(&mut self, k: Tok, msg: &'static str) -> Result<()> {
        if self.match_tok(k) {
            Ok(())
        } else {
            Err(self.error_at_current(msg))
        }
    }
    fn need_ident(&mut self, msg: &'static str) -> Result<String> {
        if self.match_tok(Tok::Ident) || self.match_tok(Tok::String) {
            Ok(self.prev().text.clone())
        } else {
            Err(self.error_with_suggestion(msg))
        }
    }
    fn need_string(&mut self, msg: &'static str) -> Result<String> {
        if self.match_tok(Tok::String) {
            Ok(self.prev().text.clone())
        } else {
            Err(self.error_with_suggestion(msg))
        }
    }
    fn peek(&self) -> &Spanned {
        // Safety: always return last token (Eof) if index is out of bounds
        if self.i >= self.toks.len() {
            self.toks.last().expect("token list should never be empty")
        } else {
            &self.toks[self.i]
        }
    }
    fn prev(&self) -> &Spanned {
        // Safety: return first token if i is 0
        if self.i == 0 {
            &self.toks[0]
        } else {
            &self.toks[self.i - 1]
        }
    }

    /// Generate error message with possible suggestion
    fn error_with_suggestion(&self, base_msg: &str) -> anyhow::Error {
        let tok = self.peek();
        let mut msg = format!("{} at line {}, column {}", base_msg, tok.line, tok.col);

        // Add suggestion based on context
        if let Some(suggestion) = self.get_suggestion() {
            msg.push_str(&format!("\n  suggestion: {}", suggestion));
        }

        anyhow!(msg)
    }

    /// Get a suggestion based on the current parser state and token
    fn get_suggestion(&self) -> Option<String> {
        let tok = self.peek();

        // Check for common typos in keywords
        if tok.kind == Tok::Ident {
            let text = tok.text.to_lowercase();

            // Suggest 'let' for common typos
            if matches!(text.as_str(), "lte" | "elt" | "lt" | "le" | "lets" | "lett") {
                return Some(format!("did you mean 'let'? Found '{}'", tok.text));
            }

            // Suggest 'fn' for common typos
            if matches!(text.as_str(), "fun" | "func" | "function" | "fnn") {
                return Some(format!("did you mean 'fn'? Found '{}'", tok.text));
            }

            // Suggest 'match' for common typos
            if matches!(
                text.as_str(),
                "metch" | "mtch" | "swtich" | "switch" | "case"
            ) {
                return Some(format!("did you mean 'match'? Found '{}'", tok.text));
            }

            // Suggest 'import' for common typos
            if matches!(text.as_str(), "include" | "require" | "imprt" | "imoprt") {
                return Some(format!("did you mean 'import'? Found '{}'", tok.text));
            }

            // Suggest 'export' for common typos
            if matches!(text.as_str(), "exprt" | "exprot" | "exports") {
                return Some(format!("did you mean 'export'? Found '{}'", tok.text));
            }

            // Suggest 'true'/'false' for common typos
            if matches!(text.as_str(), "ture" | "treu" | "tre") {
                return Some(format!("did you mean 'true'? Found '{}'", tok.text));
            }
            if matches!(text.as_str(), "flase" | "fasle" | "fals") {
                return Some(format!("did you mean 'false'? Found '{}'", tok.text));
            }

            // Suggest 'null' for common typos
            if matches!(text.as_str(), "nil" | "none" | "nul" | "nill" | "undefined") {
                return Some(format!("did you mean 'null'? Found '{}'", tok.text));
            }

            // Suggest 'pub' for common typos
            if matches!(text.as_str(), "public" | "pbu") {
                return Some(format!("did you mean 'pub'? Found '{}'", tok.text));
            }

            // Suggest 'mut' for common typos
            if matches!(text.as_str(), "mutable" | "var" | "mtu") {
                return Some(format!("did you mean 'mut'? Found '{}'", tok.text));
            }
        }

        // Check for missing operators
        if tok.kind == Tok::Ident {
            // Previous was also ident - might be missing operator
            if self.i > 0 && self.toks[self.i - 1].kind == Tok::Ident {
                return Some(
                    "two identifiers in a row - did you forget an operator like '=' or '|'?"
                        .to_string(),
                );
            }
        }

        // Suggest closing bracket
        if tok.kind == Tok::Eof {
            // Check for unclosed delimiters
            let opens: Vec<_> = self
                .toks
                .iter()
                .filter(|t| matches!(t.kind, Tok::LParen | Tok::LBracket | Tok::LBrace))
                .collect();
            let closes: Vec<_> = self
                .toks
                .iter()
                .filter(|t| matches!(t.kind, Tok::RParen | Tok::RBracket | Tok::RBrace))
                .collect();

            if opens.len() > closes.len() {
                let last_open = opens.last().unwrap();
                let expected = match last_open.kind {
                    Tok::LParen => ")",
                    Tok::LBracket => "]",
                    Tok::LBrace => "}",
                    _ => "closing bracket",
                };
                return Some(format!(
                    "unclosed delimiter - expected '{}' to match '{}' at line {}",
                    expected, last_open.text, last_open.line
                ));
            }
        }

        None
    }

    /// Synchronize the parser state after encountering an error.
    /// This allows the parser to continue and report multiple errors.
    /// We skip tokens until we find a safe synchronization point.
    fn synchronize(&mut self) {
        // Advance past the current problematic token (if not at end)
        if self.i < self.toks.len().saturating_sub(1) {
            self.i += 1;
        }

        while !self.check(Tok::Eof) {
            // If the previous token indicates statement end, we're synchronized
            if self.i > 0 {
                let prev = &self.toks[self.i - 1];
                // After these tokens, a new statement likely begins
                if matches!(prev.kind, Tok::RBrace | Tok::RBracket | Tok::RParen) {
                    return;
                }
            }

            // These tokens often start new statements
            match self.peek().kind {
                Tok::Let
                | Tok::Fn
                | Tok::Async
                | Tok::If
                | Tok::Match
                | Tok::Import
                | Tok::Export
                | Tok::Pub => {
                    return;
                }
                // Attributes start new cfg-guarded statements
                Tok::Attribute => {
                    return;
                }
                _ => {}
            }

            // Don't advance past the last real token (Eof is always last)
            if self.i < self.toks.len().saturating_sub(1) {
                self.i += 1;
            } else {
                break;
            }
        }
    }
}
