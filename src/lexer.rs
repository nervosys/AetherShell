use crate::tokens::{Span, Token, TokenKind};

#[derive(Debug)]
pub struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    pub fn all(mut self) -> anyhow::Result<Vec<Token>> {
        let mut toks = Vec::new();
        while self.pos < self.src.len() {
            let c = self.src.as_bytes()[self.pos] as char;
            match c {
                c if c.is_whitespace() => {
                    self.bump_ws();
                }
                '(' => {
                    toks.push(self.simple(TokenKind::LParen));
                    self.pos += 1;
                }
                ')' => {
                    toks.push(self.simple(TokenKind::RParen));
                    self.pos += 1;
                }
                '{' => {
                    toks.push(self.simple(TokenKind::LBrace));
                    self.pos += 1;
                }
                '}' => {
                    toks.push(self.simple(TokenKind::RBrace));
                    self.pos += 1;
                }
                '[' => {
                    toks.push(self.simple(TokenKind::LBracket));
                    self.pos += 1;
                }
                ']' => {
                    toks.push(self.simple(TokenKind::RBracket));
                    self.pos += 1;
                }
                ',' => {
                    toks.push(self.simple(TokenKind::Comma));
                    self.pos += 1;
                }
                ':' => {
                    toks.push(self.simple(TokenKind::Colon));
                    self.pos += 1;
                }
                ';' => {
                    toks.push(self.simple(TokenKind::Semicolon));
                    self.pos += 1;
                }
                '|' => {
                    if self.peek_char(1) == Some('|') {
                        toks.push(self.simple(TokenKind::OrOr));
                        self.pos += 2;
                    } else {
                        toks.push(self.simple(TokenKind::Pipe));
                        self.pos += 1;
                    }
                }
                '&' => {
                    if self.peek_char(1) == Some('&') {
                        toks.push(self.simple(TokenKind::AndAnd));
                        self.pos += 2;
                    } else {
                        self.pos += 1;
                    }
                }
                '+' => {
                    toks.push(self.simple(TokenKind::Plus));
                    self.pos += 1;
                }
                '-' => {
                    toks.push(self.simple(TokenKind::Minus));
                    self.pos += 1;
                }
                '*' => {
                    toks.push(self.simple(TokenKind::Star));
                    self.pos += 1;
                }
                '/' => {
                    if self.peek_char(1) == Some('/') {
                        self.bump_line_comment();
                    } else {
                        toks.push(self.simple(TokenKind::Slash));
                        self.pos += 1;
                    }
                }
                '%' => {
                    toks.push(self.simple(TokenKind::Percent));
                    self.pos += 1;
                }
                '!' => {
                    if self.peek_char(1) == Some('=') {
                        toks.push(self.simple(TokenKind::Ne));
                        self.pos += 2;
                    } else {
                        toks.push(self.simple(TokenKind::Bang));
                        self.pos += 1;
                    }
                }
                '=' => {
                    if self.peek_char(1) == Some('=') {
                        toks.push(self.simple(TokenKind::Eq));
                        self.pos += 2;
                    } else {
                        toks.push(self.simple(TokenKind::Assign));
                        self.pos += 1;
                    }
                }
                '>' => {
                    if self.peek_char(1) == Some('=') {
                        toks.push(self.simple(TokenKind::Gte));
                        self.pos += 2;
                    } else {
                        toks.push(self.simple(TokenKind::Gt));
                        self.pos += 1;
                    }
                }
                '<' => {
                    if self.peek_char(1) == Some('=') {
                        toks.push(self.simple(TokenKind::Lte));
                        self.pos += 2;
                    } else {
                        toks.push(self.simple(TokenKind::Lt));
                        self.pos += 1;
                    }
                }
                '"' => {
                    toks.push(self.string()?);
                }
                c if is_ident_start(c) => {
                    toks.push(self.ident_or_kw());
                }
                c if c.is_ascii_digit() => {
                    toks.push(self.number());
                }
                _ => {
                    // skip unknown char
                    self.pos += 1;
                }
            }
        }
        toks.push(Token {
            kind: TokenKind::Eof,
            span: Span {
                start: self.pos,
                end: self.pos,
            },
        });
        Ok(toks)
    }

    fn simple(&self, k: TokenKind) -> Token {
        Token {
            kind: k,
            span: Span {
                start: self.pos,
                end: self.pos + 1,
            },
        }
    }

    fn peek_char(&self, off: usize) -> Option<char> {
        self.src.as_bytes().get(self.pos + off).map(|b| *b as char)
    }

    fn bump_ws(&mut self) {
        while self.pos < self.src.len() && (self.src.as_bytes()[self.pos] as char).is_whitespace() {
            self.pos += 1;
        }
    }

    fn bump_line_comment(&mut self) {
        while self.pos < self.src.len() && (self.src.as_bytes()[self.pos] as char) != '\n' {
            self.pos += 1;
        }
    }

    fn string(&mut self) -> anyhow::Result<Token> {
        let start = self.pos;
        self.pos += 1; // consume opening quote
        let mut s = String::new();
        while self.pos < self.src.len() {
            let ch = self.src.as_bytes()[self.pos] as char;
            if ch == '"' {
                self.pos += 1; // closing quote
                break;
            }
            if ch == '\\' {
                let next = self.peek_char(1).unwrap_or('\n');
                match next {
                    'n' => {
                        s.push('\n');
                        self.pos += 2;
                    }
                    't' => {
                        s.push('\t');
                        self.pos += 2;
                    }
                    '"' => {
                        s.push('"');
                        self.pos += 2;
                    }
                    '\\' => {
                        s.push('\\');
                        self.pos += 2;
                    }
                    _ => {
                        s.push(next);
                        self.pos += 2;
                    }
                }
            } else {
                s.push(ch);
                self.pos += 1;
            }
        }
        Ok(Token {
            kind: TokenKind::Str(s),
            span: Span {
                start,
                end: self.pos,
            },
        })
    }

    fn ident_or_kw(&mut self) -> Token {
        let start = self.pos;
        let s = self.take_while(|ch| is_ident_continue(ch));
        let kind = match s.as_str() {
            "let" => TokenKind::KwLet,
            "mut" => TokenKind::KwMut,
            "fn" => TokenKind::KwFn,
            "match" => TokenKind::KwMatch,
            "if" => TokenKind::KwIf,
            "else" => TokenKind::KwElse,
            "true" => TokenKind::KwTrue,
            "false" => TokenKind::KwFalse,
            "null" => TokenKind::KwNull,
            _ => TokenKind::Ident(s),
        };
        Token {
            kind,
            span: Span {
                start,
                end: self.pos,
            },
        }
    }

    fn number(&mut self) -> Token {
        let start = self.pos;
        let s = self.take_while(|ch| ch.is_ascii_digit() || ch == '.');
        if s.contains('.') {
            let v: f64 = s.parse().unwrap_or(0.0);
            Token {
                kind: TokenKind::Float(v),
                span: Span {
                    start,
                    end: self.pos,
                },
            }
        } else {
            let v: i64 = s.parse().unwrap_or(0);
            Token {
                kind: TokenKind::Int(v),
                span: Span {
                    start,
                    end: self.pos,
                },
            }
        }
    }

    fn take_while<F: Fn(char) -> bool>(&mut self, f: F) -> String {
        let start = self.pos;
        while self.pos < self.src.len() && f(self.src.as_bytes()[self.pos] as char) {
            self.pos += 1;
        }
        self.src[start..self.pos].to_string()
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}
