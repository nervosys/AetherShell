use anyhow::{Result, anyhow};

use crate::ast::{BinOp, Expr, Stmt, UnOp};

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
    True,
    False,
    Null,
    Match,
    If,
    Ident,
    String,
    Int,
    Float,
    Eof,
}

#[derive(Debug, Clone)]
struct Spanned {
    kind: Tok,
    text: String, // literal/ident text where relevant
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

pub fn parse_program(src: &str) -> Result<Vec<Stmt>> {
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

fn push_tok(out: &mut Vec<Spanned>, kind: Tok, text: &str) {
    out.push(Spanned {
        kind,
        text: text.to_string(),
    });
}

fn lex(src: &str) -> Result<Vec<Spanned>> {
    use std::iter::Peekable;
    use std::str::Chars;

    let mut out = Vec::<Spanned>::new();
    let mut it = src.chars().peekable();

    // helper: read number (optionally with a leading '-' already consumed)
    fn read_number(it: &mut Peekable<Chars<'_>>, mut acc: String) -> (Tok, String) {
        // digits before decimal
        while let Some(&ch) = it.peek() {
            if ch.is_ascii_digit() {
                acc.push(ch);
                it.next();
            } else {
                break;
            }
        }
        // optional fraction
        if let Some(&'.') = it.peek() {
            acc.push('.');
            it.next();
            while let Some(&ch) = it.peek() {
                if ch.is_ascii_digit() {
                    acc.push(ch);
                    it.next();
                } else {
                    break;
                }
            }
            return (Tok::Float, acc);
        }
        (Tok::Int, acc)
    }

    while let Some(&c) = it.peek() {
        match c {
            ' ' | '\t' | '\r' | '\n' => {
                it.next();
            }
            '(' => {
                it.next();
                push_tok(&mut out, Tok::LParen, "(");
            }
            ')' => {
                it.next();
                push_tok(&mut out, Tok::RParen, ")");
            }
            '[' => {
                it.next();
                push_tok(&mut out, Tok::LBracket, "[");
            }
            ']' => {
                it.next();
                push_tok(&mut out, Tok::RBracket, "]");
            }
            '{' => {
                it.next();
                push_tok(&mut out, Tok::LBrace, "{");
            }
            '}' => {
                it.next();
                push_tok(&mut out, Tok::RBrace, "}");
            }
            ',' => {
                it.next();
                push_tok(&mut out, Tok::Comma, ",");
            }
            ':' => {
                it.next();
                push_tok(&mut out, Tok::Colon, ":");
            }
            '|' => {
                it.next();
                if it.peek() == Some(&'|') {
                    it.next();
                    push_tok(&mut out, Tok::OrOr, "||");
                } else {
                    push_tok(&mut out, Tok::Pipe, "|");
                }
            }
            '=' => {
                it.next();
                if it.peek() == Some(&'>') {
                    it.next();
                    push_tok(&mut out, Tok::FatArrow, "=>");
                } else if it.peek() == Some(&'=') {
                    it.next();
                    push_tok(&mut out, Tok::EqEq, "==");
                } else {
                    push_tok(&mut out, Tok::Equal, "=");
                }
            }
            '!' => {
                it.next();
                if it.peek() == Some(&'=') {
                    it.next();
                    push_tok(&mut out, Tok::Ne, "!=");
                } else {
                    push_tok(&mut out, Tok::Bang, "!");
                }
            }
            '<' => {
                it.next();
                if it.peek() == Some(&'=') {
                    it.next();
                    push_tok(&mut out, Tok::Lte, "<=");
                } else {
                    push_tok(&mut out, Tok::Lt, "<");
                }
            }
            '>' => {
                it.next();
                if it.peek() == Some(&'=') {
                    it.next();
                    push_tok(&mut out, Tok::Gte, ">=");
                } else {
                    push_tok(&mut out, Tok::Gt, ">");
                }
            }
            '&' => {
                it.next();
                if it.peek() == Some(&'&') {
                    it.next();
                    push_tok(&mut out, Tok::AndAnd, "&&");
                } else {
                    return Err(anyhow!("unknown character: &"));
                }
            }
            '+' => {
                it.next();
                push_tok(&mut out, Tok::Plus, "+");
            }
            '-' => {
                it.next();
                // negative number if followed by digit
                if let Some(&d) = it.peek() {
                    if d.is_ascii_digit() {
                        let (kind, text) = read_number(&mut it, "-".to_string());
                        out.push(Spanned { kind, text });
                        continue;
                    }
                }
                push_tok(&mut out, Tok::Minus, "-");
            }
            '*' => {
                it.next();
                push_tok(&mut out, Tok::Star, "*");
            }
            '^' => {
                it.next();
                push_tok(&mut out, Tok::Caret, "^");
            }
            '/' => {
                it.next();
                push_tok(&mut out, Tok::Slash, "/");
            }
            '%' => {
                it.next();
                push_tok(&mut out, Tok::Percent, "%");
            }
            '"' => {
                it.next();
                let mut s = String::new();
                while let Some(ch) = it.next() {
                    if ch == '"' {
                        break;
                    }
                    if ch == '\\' {
                        if let Some(esc) = it.next() {
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
                });
            }
            '\'' => {
                it.next();
                let mut s = String::new();
                while let Some(ch) = it.next() {
                    if ch == '\'' {
                        break;
                    }
                    s.push(ch);
                }
                out.push(Spanned {
                    kind: Tok::String,
                    text: s,
                });
            }
            d if d.is_ascii_digit() => {
                let (kind, text) = read_number(&mut it, String::new());
                out.push(Spanned { kind, text });
            }
            c if is_ident_start(c) => {
                let mut s = String::new();
                while let Some(&ch) = it.peek() {
                    if is_ident_part(ch) {
                        s.push(ch);
                        it.next();
                    } else {
                        break;
                    }
                }
                let kw = match s.as_str() {
                    "fn" => Tok::Fn,
                    "let" => Tok::Let,
                    "mut" => Tok::Mut,
                    "true" => Tok::True,
                    "false" => Tok::False,
                    "null" => Tok::Null,
                    "match" => Tok::Match,
                    "if" => Tok::If,
                    _ => Tok::Ident,
                };
                out.push(Spanned { kind: kw, text: s });
            }
            ';' => {
                // treat semicolon as statement separator; ignore it
                it.next();
            }
            other => {
                return Err(anyhow!("unknown character: {}", other));
            }
        }
    }

    out.push(Spanned {
        kind: Tok::Eof,
        text: String::new(),
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
        if self.match_tok(Tok::Let) {
            let is_mut = self.match_tok(Tok::Mut);
            let name = self.need_ident("expected identifier after let")?;
            if self.match_tok(Tok::Colon) {
                let _ = self.need_ident("expected type after ':'")?;
            }
            self.need(Tok::Equal, "expected '=' in let")?;
            let value = self.parse_expr()?;
            Ok(Stmt::Let {
                name,
                value,
                is_mut,
            })
        } else {
            // Top-level statement: allow word-call sugar (e.g. `print "hi"").
            let prev = self.allow_word_call;
            self.allow_word_call = true;
            let e = self.parse_expr()?;
            self.allow_word_call = prev;
            Ok(Stmt::Expr(e))
        }
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
        if self.match_tok(Tok::Fn) {
            return self.parse_lambda_after_fn();
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
        Err(anyhow!("unexpected token {:?}", self.peek().kind))
    }

    fn parse_lambda(&mut self) -> Result<Expr> {
        self.need(Tok::Fn, "expected 'fn'")?;
        self.parse_lambda_after_fn()
    }

    fn parse_lambda_after_fn(&mut self) -> Result<Expr> {
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
        // Parse the lambda body without allowing top-level pipes so a following
        // `|` is treated as the outer pipeline, not part of the lambda body.
        // Also disable the word-call sugar while parsing the lambda body to
        // avoid greedily consuming trailing atoms that belong to an outer
        // call (e.g. `fn(a,b)=> a+b 0`). Restore the previous flag afterwards.
        let prev_allow = self.allow_word_call;
        self.allow_word_call = false;
        let body = self.parse_logic_or()?;
        self.allow_word_call = prev_allow;
        Ok(Expr::Lambda {
            params,
            body: Box::new(body),
        })
    }

    // ---- small helpers ----
    fn check(&self, k: Tok) -> bool {
        self.peek().kind == k
    }
    fn check_any(&self, ks: &[Tok]) -> bool {
        ks.iter().any(|k| self.peek().kind == *k)
    }
    fn match_tok(&mut self, k: Tok) -> bool {
        if self.check(k) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn need(&mut self, k: Tok, msg: &'static str) -> Result<()> {
        if self.match_tok(k) {
            Ok(())
        } else {
            Err(anyhow!(msg))
        }
    }
    fn need_ident(&mut self, msg: &'static str) -> Result<String> {
        if self.match_tok(Tok::Ident) || self.match_tok(Tok::String) {
            Ok(self.prev().text.clone())
        } else {
            Err(anyhow!(msg))
        }
    }
    fn peek(&self) -> &Spanned {
        &self.toks[self.i]
    }
    fn prev(&self) -> &Spanned {
        &self.toks[self.i - 1]
    }
}
