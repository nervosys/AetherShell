//! A POSIX-shell subset, executed in-process: what `ae -b` runs before it
//! falls back to the transpiler.
//!
//! E6 (`benches/agentic/bashcompat.mjs`) asked how much of the shell an agent
//! actually emits `ae -b` could run. The answer was 2 of 32: the transpiler
//! produced AetherShell that did not parse, and everything with a loop, a
//! variable or `&&` was handed to `bash -lc` -- which bypasses every safety
//! property the rest of the shell has, because the effect gate sees one `sh`
//! call instead of the commands the script ran.
//!
//! The first thing a model writes is bash (`docs/LANGUAGE_FIRST_PRINCIPLES.md`
//! §3: borrow the syntax, own the semantics). So this runs the ordinary subset
//! itself: pipelines, `&&`/`||`/`;`, variables and `export`, `$(…)`, quoting,
//! globs, redirections, `if`/`for`/`while`/`until`, `test`/`[`, and text-mode
//! versions of the utilities agents reach for, written to print what GNU
//! coreutils prints. A command it does not implement runs as that program,
//! directly -- never through a shell -- after `safety::guard_exec`, so agent
//! mode gates each one. A redirection that writes a file goes through the same
//! guard as `file.write`, jail included.
//!
//! What it does not parse (functions, `case`, arithmetic, here-documents,
//! subshells, background jobs) makes [`run`] return `None` before anything has
//! executed, and the caller falls back to the transpiler exactly as before.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::Path;

// ════════════════════════════════════════════════════════════════════════
// Syntax
// ════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
enum Part {
    /// Literal text; `true` when it came from inside quotes (no globbing, no
    /// field splitting).
    Lit(String, bool),
    /// `$name` / `${name…}`: the name, an optional operator and its word, and
    /// whether the expansion was quoted.
    Var(String, Option<(String, Vec<Part>)>, bool),
    /// `${#name}`
    Len(String),
    /// `$(…)` or backquotes, and whether quoted.
    Cmd(String, bool),
}

type Word = Vec<Part>;

#[derive(Debug, Clone)]
enum Redir {
    /// `[n]>file`, `[n]>>file`
    Out { fd: u8, append: bool, target: Word },
    /// `<file`
    In(Word),
    /// `n>&m`
    Dup { from: u8, to: u8 },
    /// `&>file`
    Both(Word),
}

#[derive(Debug, Clone)]
enum Tok {
    Word(Word),
    Pipe,
    And,
    Or,
    Semi,
    Nl,
    Redir(Redir),
}

#[derive(Debug, Clone)]
enum Cmd {
    Simple {
        assigns: Vec<(String, Word)>,
        words: Vec<Word>,
        redirs: Vec<Redir>,
    },
    If {
        branches: Vec<(List, List)>,
        otherwise: Option<List>,
        redirs: Vec<Redir>,
    },
    For {
        var: String,
        items: Vec<Word>,
        body: List,
        redirs: Vec<Redir>,
    },
    While {
        until: bool,
        cond: List,
        body: List,
        redirs: Vec<Redir>,
    },
    Group {
        body: List,
        redirs: Vec<Redir>,
    },
}

#[derive(Debug, Clone)]
struct Pipeline {
    negate: bool,
    cmds: Vec<Cmd>,
}

#[derive(Debug, Clone)]
struct AndOr {
    first: Pipeline,
    rest: Vec<(bool, Pipeline)>,
}

type List = Vec<AndOr>;

/// Why the subset declined a script. Never shown as an error: it is the signal
/// to fall back.
#[derive(Debug)]
pub struct Unsupported(pub String);

type PResult<T> = Result<T, Unsupported>;

fn unsupported<T>(what: &str) -> PResult<T> {
    Err(Unsupported(what.to_string()))
}

// ── lexer ────────────────────────────────────────────────────────────────

struct Lexer<'a> {
    s: &'a [char],
    i: usize,
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

impl<'a> Lexer<'a> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn peek_at(&self, n: usize) -> Option<char> {
        self.s.get(self.i + n).copied()
    }

    fn tokens(mut self) -> PResult<Vec<Tok>> {
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\t' | '\r' => self.i += 1,
                '\\' if self.peek_at(1) == Some('\n') => self.i += 2,
                '#' => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.i += 1;
                    }
                }
                '\n' => {
                    self.i += 1;
                    out.push(Tok::Nl);
                }
                ';' => {
                    if self.peek_at(1) == Some(';') {
                        return unsupported("case");
                    }
                    self.i += 1;
                    out.push(Tok::Semi);
                }
                '|' => {
                    if self.peek_at(1) == Some('|') {
                        self.i += 2;
                        out.push(Tok::Or);
                    } else if self.peek_at(1) == Some('&') {
                        return unsupported("|&");
                    } else {
                        self.i += 1;
                        out.push(Tok::Pipe);
                    }
                }
                '&' => {
                    if self.peek_at(1) == Some('&') {
                        self.i += 2;
                        out.push(Tok::And);
                    } else if self.peek_at(1) == Some('>') {
                        self.i += 2;
                        if self.peek() == Some('>') {
                            return unsupported("&>>");
                        }
                        let target = self.target()?;
                        out.push(Tok::Redir(Redir::Both(target)));
                    } else {
                        return unsupported("background job");
                    }
                }
                '(' | ')' => return unsupported("subshell"),
                '<' | '>' => out.push(Tok::Redir(self.redir(None)?)),
                _ => {
                    // A word; but digits immediately followed by a redirection
                    // operator are that operator's descriptor.
                    let start = self.i;
                    let mut j = self.i;
                    while self.s.get(j).is_some_and(|c| c.is_ascii_digit()) {
                        j += 1;
                    }
                    if j > start && matches!(self.s.get(j), Some('<') | Some('>')) {
                        let fd: u8 = self.s[start..j]
                            .iter()
                            .collect::<String>()
                            .parse()
                            .map_err(|_| Unsupported("descriptor".into()))?;
                        self.i = j;
                        out.push(Tok::Redir(self.redir(Some(fd))?));
                        continue;
                    }
                    out.push(Tok::Word(self.word()?));
                }
            }
        }
        Ok(out)
    }

    fn redir(&mut self, fd: Option<u8>) -> PResult<Redir> {
        let c = self.peek().unwrap_or(' ');
        self.i += 1;
        if c == '<' {
            match self.peek() {
                Some('<') => return unsupported("here-document"),
                Some('(') => return unsupported("process substitution"),
                Some('&') | Some('>') => return unsupported("<& / <>"),
                _ => {}
            }
            if fd.is_some_and(|f| f != 0) {
                return unsupported("input descriptor");
            }
            return Ok(Redir::In(self.target()?));
        }
        let fd = fd.unwrap_or(1);
        let mut append = false;
        match self.peek() {
            Some('>') => {
                append = true;
                self.i += 1;
            }
            Some('&') => {
                self.i += 1;
                let d = self.peek();
                return match d {
                    Some(d @ '0'..='9') => {
                        self.i += 1;
                        Ok(Redir::Dup {
                            from: fd,
                            to: d as u8 - b'0',
                        })
                    }
                    _ => unsupported(">&word"),
                };
            }
            Some('(') => return unsupported("process substitution"),
            Some('|') => return unsupported(">|"),
            _ => {}
        }
        if fd > 2 {
            return unsupported("descriptor above 2");
        }
        Ok(Redir::Out {
            fd,
            append,
            target: self.target()?,
        })
    }

    fn target(&mut self) -> PResult<Word> {
        while matches!(self.peek(), Some(' ') | Some('\t')) {
            self.i += 1;
        }
        match self.peek() {
            None | Some('\n') | Some(';') | Some('|') | Some('&') | Some('<') | Some('>') => {
                unsupported("redirection without a target")
            }
            _ => self.word(),
        }
    }

    /// One shell word, up to an unquoted metacharacter.
    fn word(&mut self) -> PResult<Word> {
        let mut parts: Word = Vec::new();
        let mut lit = String::new();
        let flush = |lit: &mut String, parts: &mut Word| {
            if !lit.is_empty() {
                parts.push(Part::Lit(std::mem::take(lit), false));
            }
        };
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\t' | '\r' | '\n' | ';' | '|' | '&' | '<' | '>' | '(' | ')' => break,
                '\\' => {
                    self.i += 1;
                    match self.peek() {
                        Some('\n') => self.i += 1,
                        Some(e) => {
                            flush(&mut lit, &mut parts);
                            parts.push(Part::Lit(e.to_string(), true));
                            self.i += 1;
                        }
                        None => lit.push('\\'),
                    }
                }
                '\'' => {
                    flush(&mut lit, &mut parts);
                    self.i += 1;
                    let mut q = String::new();
                    loop {
                        match self.peek() {
                            None => return unsupported("unterminated quote"),
                            Some('\'') => {
                                self.i += 1;
                                break;
                            }
                            Some(ch) => {
                                q.push(ch);
                                self.i += 1;
                            }
                        }
                    }
                    parts.push(Part::Lit(q, true));
                }
                '"' => {
                    flush(&mut lit, &mut parts);
                    self.i += 1;
                    self.double_quoted(&mut parts)?;
                }
                '$' => {
                    flush(&mut lit, &mut parts);
                    self.dollar(&mut parts, false)?;
                }
                '`' => {
                    flush(&mut lit, &mut parts);
                    let body = self.backquote()?;
                    parts.push(Part::Cmd(body, false));
                }
                _ => {
                    lit.push(c);
                    self.i += 1;
                }
            }
        }
        flush(&mut lit, &mut parts);
        if parts.is_empty() {
            return unsupported("empty word");
        }
        Ok(parts)
    }

    fn double_quoted(&mut self, parts: &mut Word) -> PResult<()> {
        let mut lit = String::new();
        // An empty "" is still an argument.
        let mut any = false;
        loop {
            match self.peek() {
                None => return unsupported("unterminated quote"),
                Some('"') => {
                    self.i += 1;
                    break;
                }
                Some('\\') => {
                    self.i += 1;
                    match self.peek() {
                        Some(e @ ('"' | '\\' | '$' | '`')) => {
                            lit.push(e);
                            self.i += 1;
                        }
                        Some('\n') => self.i += 1,
                        Some(e) => {
                            lit.push('\\');
                            lit.push(e);
                            self.i += 1;
                        }
                        None => return unsupported("unterminated quote"),
                    }
                }
                Some('$') => {
                    if !lit.is_empty() || !any {
                        parts.push(Part::Lit(std::mem::take(&mut lit), true));
                    }
                    self.dollar(parts, true)?;
                    any = true;
                }
                Some('`') => {
                    if !lit.is_empty() {
                        parts.push(Part::Lit(std::mem::take(&mut lit), true));
                    }
                    let body = self.backquote()?;
                    parts.push(Part::Cmd(body, true));
                    any = true;
                }
                Some(c) => {
                    lit.push(c);
                    self.i += 1;
                }
            }
        }
        if !lit.is_empty() || !any {
            parts.push(Part::Lit(lit, true));
        }
        Ok(())
    }

    fn backquote(&mut self) -> PResult<String> {
        self.i += 1;
        let mut body = String::new();
        loop {
            match self.peek() {
                None => return unsupported("unterminated backquote"),
                Some('`') => {
                    self.i += 1;
                    return Ok(body);
                }
                Some('\\') if matches!(self.peek_at(1), Some('`') | Some('\\') | Some('$')) => {
                    body.push(self.peek_at(1).unwrap_or('\\'));
                    self.i += 2;
                }
                Some(c) => {
                    body.push(c);
                    self.i += 1;
                }
            }
        }
    }

    /// `$…` at `self.i`.
    fn dollar(&mut self, parts: &mut Word, quoted: bool) -> PResult<()> {
        self.i += 1;
        match self.peek() {
            Some('(') => {
                if self.peek_at(1) == Some('(') {
                    return unsupported("arithmetic expansion");
                }
                self.i += 1;
                let start = self.i;
                let mut depth = 1;
                let mut in_single = false;
                let mut in_double = false;
                while let Some(c) = self.peek() {
                    match c {
                        '\\' if !in_single => self.i += 1,
                        '\'' if !in_double => in_single = !in_single,
                        '"' if !in_single => in_double = !in_double,
                        '(' if !in_single && !in_double => depth += 1,
                        ')' if !in_single && !in_double => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    self.i += 1;
                }
                if self.peek() != Some(')') {
                    return unsupported("unterminated $(");
                }
                let body: String = self.s[start..self.i].iter().collect();
                self.i += 1;
                parts.push(Part::Cmd(body, quoted));
            }
            Some('{') => {
                self.i += 1;
                if self.peek() == Some('#') && self.peek_at(1).is_some_and(is_name_start) {
                    self.i += 1;
                    let name = self.name();
                    if self.peek() != Some('}') {
                        return unsupported("${#…} form");
                    }
                    self.i += 1;
                    parts.push(Part::Len(name));
                    return Ok(());
                }
                let name = match self.peek() {
                    Some(c) if is_name_start(c) => self.name(),
                    Some(c @ ('?' | '$' | '#' | '0'..='9')) => {
                        self.i += 1;
                        c.to_string()
                    }
                    _ => return unsupported("${…} form"),
                };
                if self.peek() == Some('}') {
                    self.i += 1;
                    parts.push(Part::Var(name, None, quoted));
                    return Ok(());
                }
                let mut op = String::new();
                if self.peek() == Some(':') {
                    op.push(':');
                    self.i += 1;
                }
                match self.peek() {
                    Some(c @ ('-' | '=' | '+' | '?')) => {
                        op.push(c);
                        self.i += 1;
                    }
                    _ => return unsupported("${…} operator"),
                }
                // The operator's word, up to the matching brace.
                let start = self.i;
                let mut depth = 1;
                while let Some(c) = self.peek() {
                    match c {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    self.i += 1;
                }
                if self.peek() != Some('}') {
                    return unsupported("unterminated ${");
                }
                let inner: Vec<char> = self.s[start..self.i].to_vec();
                self.i += 1;
                let word = if inner.is_empty() {
                    vec![Part::Lit(String::new(), true)]
                } else {
                    let mut sub = Lexer { s: &inner, i: 0 };
                    let mut w = Vec::new();
                    // Treat the operand like double-quoted text when the
                    // expansion itself is quoted, else as a word.
                    if quoted {
                        let mut chars = inner.clone();
                        chars.push('"');
                        let mut q = Lexer { s: &chars, i: 0 };
                        q.double_quoted(&mut w)?;
                    } else {
                        w = sub.word()?;
                        if sub.i != inner.len() {
                            return unsupported("${…} operand");
                        }
                    }
                    w
                };
                parts.push(Part::Var(name, Some((op, word)), quoted));
            }
            Some(c) if is_name_start(c) => {
                let name = self.name();
                parts.push(Part::Var(name, None, quoted));
            }
            Some(c @ ('?' | '$' | '#' | '0'..='9' | '@' | '*' | '!' | '-')) => {
                self.i += 1;
                if matches!(c, '@' | '*' | '!' | '-') {
                    return unsupported("special parameter");
                }
                parts.push(Part::Var(c.to_string(), None, quoted));
            }
            _ => parts.push(Part::Lit("$".into(), quoted)),
        }
        Ok(())
    }

    fn name(&mut self) -> String {
        let mut n = String::new();
        while let Some(c) = self.peek() {
            if is_name_char(c) {
                n.push(c);
                self.i += 1;
            } else {
                break;
            }
        }
        n
    }
}

// ── parser ───────────────────────────────────────────────────────────────

struct Parser {
    toks: Vec<Tok>,
    i: usize,
}

/// The text of a word that is a single unquoted literal, which is what a
/// reserved word must be to count as one.
fn plain(w: &Word) -> Option<&str> {
    match w.as_slice() {
        [Part::Lit(s, false)] => Some(s),
        _ => None,
    }
}

const RESERVED: &[&str] = &[
    "if", "then", "elif", "else", "fi", "for", "in", "do", "done", "while", "until", "{", "}", "!",
    "case", "esac", "function", "select", "[[", "]]",
];

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i)
    }

    fn peek_keyword(&self) -> Option<&str> {
        match self.peek() {
            Some(Tok::Word(w)) => plain(w).filter(|s| RESERVED.contains(s)),
            _ => None,
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Some(Tok::Nl) | Some(Tok::Semi)) {
            self.i += 1;
        }
    }

    fn expect_keyword(&mut self, kw: &str) -> PResult<()> {
        self.skip_newlines();
        if self.peek_keyword() == Some(kw) {
            self.i += 1;
            Ok(())
        } else {
            unsupported(&format!("expected `{kw}`"))
        }
    }

    /// A list, stopping before any of `stop` (reserved words) or the end.
    fn list(&mut self, stop: &[&str]) -> PResult<List> {
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                None => break,
                Some(Tok::Word(_)) => {
                    if let Some(k) = self.peek_keyword() {
                        if stop.contains(&k) {
                            break;
                        }
                    }
                }
                Some(Tok::Redir(_)) => {}
                Some(_) => return unsupported("unexpected operator"),
            }
            out.push(self.and_or()?);
            match self.peek() {
                None | Some(Tok::Nl) | Some(Tok::Semi) => {}
                Some(Tok::Word(w)) if plain(w).is_some_and(|k| stop.contains(&k)) => {}
                _ => return unsupported("unexpected token after command"),
            }
        }
        Ok(out)
    }

    fn and_or(&mut self) -> PResult<AndOr> {
        let first = self.pipeline()?;
        let mut rest = Vec::new();
        loop {
            let and = match self.peek() {
                Some(Tok::And) => true,
                Some(Tok::Or) => false,
                _ => break,
            };
            self.i += 1;
            while matches!(self.peek(), Some(Tok::Nl)) {
                self.i += 1;
            }
            rest.push((and, self.pipeline()?));
        }
        Ok(AndOr { first, rest })
    }

    fn pipeline(&mut self) -> PResult<Pipeline> {
        let mut negate = false;
        if self.peek_keyword() == Some("!") {
            negate = true;
            self.i += 1;
        }
        let mut cmds = vec![self.command()?];
        while matches!(self.peek(), Some(Tok::Pipe)) {
            self.i += 1;
            while matches!(self.peek(), Some(Tok::Nl)) {
                self.i += 1;
            }
            cmds.push(self.command()?);
        }
        Ok(Pipeline { negate, cmds })
    }

    fn trailing_redirs(&mut self) -> Vec<Redir> {
        let mut r = Vec::new();
        while let Some(Tok::Redir(x)) = self.peek() {
            r.push(x.clone());
            self.i += 1;
        }
        r
    }

    fn command(&mut self) -> PResult<Cmd> {
        match self.peek_keyword() {
            Some("if") => {
                self.i += 1;
                let mut branches = Vec::new();
                let cond = self.list(&["then"])?;
                self.expect_keyword("then")?;
                let body = self.list(&["elif", "else", "fi"])?;
                branches.push((cond, body));
                let mut otherwise = None;
                loop {
                    self.skip_newlines();
                    match self.peek_keyword() {
                        Some("elif") => {
                            self.i += 1;
                            let c = self.list(&["then"])?;
                            self.expect_keyword("then")?;
                            let b = self.list(&["elif", "else", "fi"])?;
                            branches.push((c, b));
                        }
                        Some("else") => {
                            self.i += 1;
                            otherwise = Some(self.list(&["fi"])?);
                        }
                        Some("fi") => {
                            self.i += 1;
                            break;
                        }
                        _ => return unsupported("unterminated if"),
                    }
                }
                Ok(Cmd::If {
                    branches,
                    otherwise,
                    redirs: self.trailing_redirs(),
                })
            }
            Some("for") => {
                self.i += 1;
                let var = match self.peek() {
                    Some(Tok::Word(w)) => match plain(w) {
                        Some(n)
                            if n.chars().next().is_some_and(is_name_start)
                                && n.chars().all(is_name_char) =>
                        {
                            n.to_string()
                        }
                        _ => return unsupported("for variable"),
                    },
                    _ => return unsupported("for variable"),
                };
                self.i += 1;
                while matches!(self.peek(), Some(Tok::Nl)) {
                    self.i += 1;
                }
                if self.peek_keyword() != Some("in") {
                    return unsupported("for without in");
                }
                self.i += 1;
                let mut items = Vec::new();
                while let Some(Tok::Word(w)) = self.peek() {
                    if plain(w) == Some("do") {
                        break;
                    }
                    items.push(w.clone());
                    self.i += 1;
                }
                self.expect_keyword("do")?;
                let body = self.list(&["done"])?;
                self.expect_keyword("done")?;
                Ok(Cmd::For {
                    var,
                    items,
                    body,
                    redirs: self.trailing_redirs(),
                })
            }
            Some(k @ ("while" | "until")) => {
                let until = k == "until";
                self.i += 1;
                let cond = self.list(&["do"])?;
                self.expect_keyword("do")?;
                let body = self.list(&["done"])?;
                self.expect_keyword("done")?;
                Ok(Cmd::While {
                    until,
                    cond,
                    body,
                    redirs: self.trailing_redirs(),
                })
            }
            Some("{") => {
                self.i += 1;
                let body = self.list(&["}"])?;
                self.expect_keyword("}")?;
                Ok(Cmd::Group {
                    body,
                    redirs: self.trailing_redirs(),
                })
            }
            Some(k) if !matches!(k, "!") => unsupported(&format!("`{k}`")),
            _ => self.simple(),
        }
    }

    fn simple(&mut self) -> PResult<Cmd> {
        let mut assigns = Vec::new();
        let mut words: Vec<Word> = Vec::new();
        let mut redirs = Vec::new();
        loop {
            match self.peek() {
                Some(Tok::Word(w)) => {
                    if words.is_empty() {
                        if let Some(a) = assignment(w) {
                            assigns.push(a);
                            self.i += 1;
                            continue;
                        }
                        if let Some(Part::Lit(s, false)) = w.first() {
                            if s.ends_with("()") || s == "function" {
                                return unsupported("function definition");
                            }
                            if s == "[[" {
                                return unsupported("[[");
                            }
                        }
                    }
                    words.push(w.clone());
                    self.i += 1;
                }
                Some(Tok::Redir(r)) => {
                    redirs.push(r.clone());
                    self.i += 1;
                }
                _ => break,
            }
        }
        if assigns.is_empty() && words.is_empty() && redirs.is_empty() {
            return unsupported("empty command");
        }
        Ok(Cmd::Simple {
            assigns,
            words,
            redirs,
        })
    }
}

/// `NAME=value` at the start of a simple command.
fn assignment(w: &Word) -> Option<(String, Word)> {
    let Part::Lit(first, false) = w.first()? else {
        return None;
    };
    let eq = first.find('=')?;
    let name = &first[..eq];
    if name.is_empty()
        || !name.chars().next().is_some_and(is_name_start)
        || !name.chars().all(is_name_char)
    {
        return None;
    }
    if first[eq + 1..].starts_with('(') {
        return None;
    }
    let mut value: Word = Vec::new();
    let rest = &first[eq + 1..];
    if !rest.is_empty() {
        value.push(Part::Lit(rest.to_string(), false));
    }
    value.extend(w[1..].iter().cloned());
    if value.is_empty() {
        value.push(Part::Lit(String::new(), true));
    }
    Some((name.to_string(), value))
}

fn parse(src: &str) -> PResult<List> {
    let chars: Vec<char> = src.chars().collect();
    let toks = Lexer { s: &chars, i: 0 }.tokens()?;
    let mut p = Parser { toks, i: 0 };
    let list = p.list(&[])?;
    if p.i != p.toks.len() {
        return unsupported("trailing tokens");
    }
    Ok(list)
}

// ════════════════════════════════════════════════════════════════════════
// Execution
// ════════════════════════════════════════════════════════════════════════

/// A command's standard input: a buffer, or the process's own stdin read on
/// demand (so a script that never reads it does not block on a terminal).
struct Input {
    data: Vec<u8>,
    pos: usize,
    real: bool,
    eof: bool,
}

impl Input {
    fn buf(data: Vec<u8>) -> Self {
        Input {
            data,
            pos: 0,
            real: false,
            eof: true,
        }
    }
    fn real() -> Self {
        Input {
            data: Vec::new(),
            pos: 0,
            real: true,
            eof: false,
        }
    }
    fn fill_all(&mut self) {
        if self.real && !self.eof {
            let _ = std::io::stdin().read_to_end(&mut self.data);
            self.eof = true;
        }
    }
    fn read_all(&mut self) -> Vec<u8> {
        self.fill_all();
        let out = self.data[self.pos..].to_vec();
        self.pos = self.data.len();
        out
    }
    /// One line without its newline, or `None` at end of input.
    fn read_line(&mut self) -> Option<Vec<u8>> {
        loop {
            if let Some(nl) = self.data[self.pos..].iter().position(|&b| b == b'\n') {
                let line = self.data[self.pos..self.pos + nl].to_vec();
                self.pos += nl + 1;
                return Some(line);
            }
            if self.real && !self.eof {
                let mut chunk = String::new();
                match std::io::stdin().read_line(&mut chunk) {
                    Ok(0) | Err(_) => self.eof = true,
                    Ok(_) => self.data.extend_from_slice(chunk.as_bytes()),
                }
                continue;
            }
            if self.pos < self.data.len() {
                let line = self.data[self.pos..].to_vec();
                self.pos = self.data.len();
                return Some(line);
            }
            return None;
        }
    }
    fn is_buffered(&self) -> bool {
        !self.real
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ErrMode {
    Real,
    Null,
    /// Into this sink's stdout (`2>&1`).
    Merge,
    /// Captured, for `2>file`.
    Capture,
}

struct Sink {
    out: Vec<u8>,
    err_mode: ErrMode,
    err: Vec<u8>,
}

impl Sink {
    fn new(err_mode: ErrMode) -> Self {
        Sink {
            out: Vec::new(),
            err_mode,
            err: Vec::new(),
        }
    }
    fn ewrite(&mut self, b: &[u8]) {
        match self.err_mode {
            ErrMode::Real => {
                let _ = std::io::stderr().write_all(b);
            }
            ErrMode::Null => {}
            ErrMode::Merge => self.out.extend_from_slice(b),
            ErrMode::Capture => self.err.extend_from_slice(b),
        }
    }
    fn eline(&mut self, s: &str) {
        self.ewrite(format!("{s}\n").as_bytes());
    }
}

enum Flow {
    Next(i32),
    Break(u32),
    Continue(u32),
    Exit(i32),
}

/// The result of a utility that may decline its arguments, in which case the
/// real program runs instead.
enum Util {
    Done(i32),
    Decline,
}

struct Shell {
    vars: HashMap<String, String>,
    exported: HashSet<String>,
    last: i32,
}

const SHELL_NAME: &str = "ae";

impl Shell {
    fn var(&self, name: &str) -> Option<String> {
        match name {
            "?" => return Some(self.last.to_string()),
            "$" => return Some(std::process::id().to_string()),
            "#" => return Some("0".into()),
            "0" => return Some(SHELL_NAME.into()),
            _ => {}
        }
        if name.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        if let Some(v) = self.vars.get(name) {
            return Some(v.clone());
        }
        std::env::var(name).ok()
    }

    fn set_var(&mut self, name: &str, value: String) {
        self.vars.insert(name.to_string(), value);
    }

    // ── expansion ──

    /// Expand a word into fields: parameter and command substitution, field
    /// splitting of unquoted results, then pathname expansion.
    fn expand(&mut self, w: &Word, stdin: &mut Input) -> Vec<String> {
        // Each field is built as (text, glob-eligible mask per char).
        let mut fields: Vec<(String, Vec<bool>)> = Vec::new();
        let mut cur = String::new();
        let mut mask: Vec<bool> = Vec::new();
        let mut have = false;
        let tilde =
            matches!(w.first(), Some(Part::Lit(s, false)) if s == "~" || s.starts_with("~/"));

        for (idx, part) in w.iter().enumerate() {
            match part {
                Part::Lit(s, quoted) => {
                    let s = if idx == 0 && tilde {
                        let home = self.var("HOME").unwrap_or_default();
                        format!("{home}{}", &s[1..])
                    } else {
                        s.clone()
                    };
                    for c in s.chars() {
                        cur.push(c);
                        mask.push(!quoted);
                    }
                    have = true;
                }
                Part::Len(name) => {
                    let n = self.var(name).unwrap_or_default().chars().count();
                    for c in n.to_string().chars() {
                        cur.push(c);
                        mask.push(false);
                    }
                    have = true;
                }
                Part::Var(name, op, quoted) => {
                    let value = self.param(name, op, stdin);
                    self.splice(&value, *quoted, &mut fields, &mut cur, &mut mask, &mut have);
                }
                Part::Cmd(body, quoted) => {
                    let value = self.substitute(body);
                    self.splice(&value, *quoted, &mut fields, &mut cur, &mut mask, &mut have);
                }
            }
        }
        if have {
            fields.push((cur, mask));
        }
        let mut out = Vec::new();
        for (text, mask) in fields {
            let globby = text
                .chars()
                .zip(mask.iter())
                .any(|(c, &m)| m && matches!(c, '*' | '?' | '['));
            if globby {
                let matches = glob_expand(&text);
                if !matches.is_empty() {
                    out.extend(matches);
                    continue;
                }
            }
            out.push(text);
        }
        out
    }

    /// Append an expansion's value, splitting it into fields when unquoted.
    fn splice(
        &self,
        value: &str,
        quoted: bool,
        fields: &mut Vec<(String, Vec<bool>)>,
        cur: &mut String,
        mask: &mut Vec<bool>,
        have: &mut bool,
    ) {
        if quoted {
            cur.push_str(value);
            mask.extend(std::iter::repeat_n(false, value.chars().count()));
            *have = true;
            return;
        }
        let starts_ws = value.starts_with(|c: char| c.is_ascii_whitespace());
        let ends_ws = value.ends_with(|c: char| c.is_ascii_whitespace());
        let pieces: Vec<&str> = value.split_ascii_whitespace().collect();
        if pieces.is_empty() {
            if starts_ws && *have {
                fields.push((std::mem::take(cur), std::mem::take(mask)));
                *have = false;
            }
            return;
        }
        for (k, p) in pieces.iter().enumerate() {
            if (k > 0 || starts_ws) && *have {
                fields.push((std::mem::take(cur), std::mem::take(mask)));
            }
            cur.push_str(p);
            // Unquoted expansion results are subject to globbing.
            mask.extend(std::iter::repeat_n(true, p.chars().count()));
            *have = true;
        }
        if ends_ws {
            fields.push((std::mem::take(cur), std::mem::take(mask)));
            *have = false;
        }
    }

    fn param(&mut self, name: &str, op: &Option<(String, Word)>, stdin: &mut Input) -> String {
        let value = self.var(name);
        let Some((op, word)) = op else {
            return value.unwrap_or_default();
        };
        let colon = op.starts_with(':');
        let unset_or_null = match &value {
            None => true,
            Some(v) => colon && v.is_empty(),
        };
        let operand = |sh: &mut Shell, stdin: &mut Input| sh.expand(word, stdin).join(" ");
        match op.trim_start_matches(':') {
            "-" => {
                if unset_or_null {
                    operand(self, stdin)
                } else {
                    value.unwrap_or_default()
                }
            }
            "=" => {
                if unset_or_null {
                    let v = operand(self, stdin);
                    self.set_var(name, v.clone());
                    v
                } else {
                    value.unwrap_or_default()
                }
            }
            "+" => {
                if unset_or_null {
                    String::new()
                } else {
                    operand(self, stdin)
                }
            }
            _ => value.unwrap_or_default(),
        }
    }

    /// `$(…)`: run the body and take its stdout, less trailing newlines.
    fn substitute(&mut self, body: &str) -> String {
        let list = match parse(body) {
            Ok(l) => l,
            Err(Unsupported(what)) => {
                let _ = writeln!(
                    std::io::stderr(),
                    "{SHELL_NAME}: command substitution uses {what}, which this shell does not run"
                );
                self.last = 2;
                return String::new();
            }
        };
        let mut sink = Sink::new(ErrMode::Real);
        let mut input = Input::buf(Vec::new());
        let flow = self.run_list(&list, &mut input, &mut sink);
        self.last = match flow {
            Flow::Next(c) | Flow::Exit(c) => c,
            _ => 0,
        };
        let mut s = String::from_utf8_lossy(&sink.out).into_owned();
        while s.ends_with('\n') {
            s.pop();
        }
        s
    }

    // ── running ──

    fn run_list(&mut self, list: &List, stdin: &mut Input, sink: &mut Sink) -> Flow {
        let mut status = 0;
        for ao in list {
            match self.run_and_or(ao, stdin, sink) {
                Flow::Next(c) => status = c,
                other => return other,
            }
        }
        Flow::Next(status)
    }

    fn run_and_or(&mut self, ao: &AndOr, stdin: &mut Input, sink: &mut Sink) -> Flow {
        let mut status = match self.run_pipeline(&ao.first, stdin, sink) {
            Flow::Next(c) => c,
            other => return other,
        };
        for (and, p) in &ao.rest {
            if (*and && status != 0) || (!*and && status == 0) {
                continue;
            }
            status = match self.run_pipeline(p, stdin, sink) {
                Flow::Next(c) => c,
                other => return other,
            };
        }
        Flow::Next(status)
    }

    fn run_pipeline(&mut self, p: &Pipeline, stdin: &mut Input, sink: &mut Sink) -> Flow {
        let flow = if p.cmds.len() == 1 {
            self.run_cmd(&p.cmds[0], stdin, sink)
        } else {
            let mut data: Option<Vec<u8>> = None;
            let mut status = 0;
            let n = p.cmds.len();
            for (k, cmd) in p.cmds.iter().enumerate() {
                let mut local_in = match data.take() {
                    Some(d) => Input::buf(d),
                    None => Input::buf(Vec::new()),
                };
                let input: &mut Input = if k == 0 { &mut *stdin } else { &mut local_in };
                let mut stage = Sink::new(sink.err_mode);
                // Each stage of a pipeline is a subshell in bash: `exit` and
                // `break` end the stage, not the script.
                status = match self.run_cmd(cmd, input, &mut stage) {
                    Flow::Next(c) | Flow::Exit(c) => c,
                    _ => 0,
                };
                if stage.err_mode == ErrMode::Capture {
                    sink.err.extend_from_slice(&stage.err);
                }
                if k + 1 == n {
                    sink.out.extend_from_slice(&stage.out);
                } else {
                    data = Some(stage.out);
                }
            }
            Flow::Next(status)
        };
        match flow {
            Flow::Next(c) => {
                let c = if p.negate { i32::from(c == 0) } else { c };
                self.last = c;
                Flow::Next(c)
            }
            other => other,
        }
    }

    fn run_cmd(&mut self, cmd: &Cmd, stdin: &mut Input, sink: &mut Sink) -> Flow {
        let redirs = match cmd {
            Cmd::Simple { redirs, .. }
            | Cmd::If { redirs, .. }
            | Cmd::For { redirs, .. }
            | Cmd::While { redirs, .. }
            | Cmd::Group { redirs, .. } => redirs,
        };
        if redirs.is_empty() {
            return self.run_cmd_inner(cmd, stdin, sink);
        }
        // Resolve redirections into a local input and sink.
        let mut local_in: Option<Input> = None;
        let mut out_file: Option<(String, bool)> = None;
        let mut err_file: Option<(String, bool)> = None;
        let mut out_null = false;
        let mut out_to_err = false;
        let mut err_mode = sink.err_mode;
        for r in redirs {
            match r {
                Redir::In(w) => {
                    let path = self.expand(w, stdin).join(" ");
                    match std::fs::read(&path) {
                        Ok(d) => local_in = Some(Input::buf(d)),
                        Err(e) => {
                            sink.eline(&format!("{SHELL_NAME}: {path}: {}", io_reason(&e)));
                            self.last = 1;
                            return Flow::Next(1);
                        }
                    }
                }
                Redir::Out { fd, append, target } => {
                    let path = self.expand(target, stdin).join(" ");
                    if *fd == 2 {
                        if path == "/dev/null" {
                            err_mode = ErrMode::Null;
                        } else {
                            err_mode = ErrMode::Capture;
                            err_file = Some((path, *append));
                        }
                    } else if path == "/dev/null" {
                        out_null = true;
                        out_file = None;
                    } else {
                        out_null = false;
                        out_file = Some((path, *append));
                    }
                }
                Redir::Both(target) => {
                    let path = self.expand(target, stdin).join(" ");
                    if path == "/dev/null" {
                        out_null = true;
                        err_mode = ErrMode::Null;
                    } else {
                        out_file = Some((path, false));
                        err_mode = ErrMode::Merge;
                    }
                }
                Redir::Dup { from: 2, to: 1 } => err_mode = ErrMode::Merge,
                Redir::Dup { from: 1, to: 2 } => out_to_err = true,
                Redir::Dup { .. } => {}
            }
        }
        let mut local = Sink::new(err_mode);
        let flow = match local_in.as_mut() {
            Some(i) => self.run_cmd_inner(cmd, i, &mut local),
            None => self.run_cmd_inner(cmd, stdin, &mut local),
        };
        if let Some((path, append)) = err_file {
            if let Err(msg) = write_file(&path, &local.err, append) {
                sink.eline(&msg);
            }
        }
        if let Some((path, append)) = out_file {
            if let Err(msg) = write_file(&path, &local.out, append) {
                sink.eline(&msg);
                self.last = 1;
                return Flow::Next(1);
            }
        } else if out_to_err {
            let out = std::mem::take(&mut local.out);
            sink.ewrite(&out);
        } else if !out_null {
            sink.out.extend_from_slice(&local.out);
        }
        flow
    }

    fn run_cmd_inner(&mut self, cmd: &Cmd, stdin: &mut Input, sink: &mut Sink) -> Flow {
        match cmd {
            Cmd::Simple { assigns, words, .. } => self.run_simple(assigns, words, stdin, sink),
            Cmd::If {
                branches,
                otherwise,
                ..
            } => {
                for (cond, body) in branches {
                    match self.run_list(cond, stdin, sink) {
                        Flow::Next(0) => return self.run_list(body, stdin, sink),
                        Flow::Next(_) => {}
                        other => return other,
                    }
                }
                match otherwise {
                    Some(b) => self.run_list(b, stdin, sink),
                    None => Flow::Next(0),
                }
            }
            Cmd::For {
                var, items, body, ..
            } => {
                let mut values = Vec::new();
                for w in items {
                    values.extend(self.expand(w, stdin));
                }
                let mut status = 0;
                for v in values {
                    self.set_var(var, v);
                    match self.run_list(body, stdin, sink) {
                        Flow::Next(c) => status = c,
                        Flow::Break(n) => {
                            if n > 1 {
                                return Flow::Break(n - 1);
                            }
                            break;
                        }
                        Flow::Continue(n) => {
                            if n > 1 {
                                return Flow::Continue(n - 1);
                            }
                        }
                        Flow::Exit(c) => return Flow::Exit(c),
                    }
                }
                Flow::Next(status)
            }
            Cmd::While {
                until, cond, body, ..
            } => {
                let mut status = 0;
                let mut guard = 0u64;
                loop {
                    guard += 1;
                    if guard > 10_000_000 {
                        sink.eline(&format!("{SHELL_NAME}: loop ran 10,000,000 times; stopped"));
                        return Flow::Next(1);
                    }
                    let c = match self.run_list(cond, stdin, sink) {
                        Flow::Next(c) => c,
                        other => return other,
                    };
                    if (c == 0) == *until {
                        break;
                    }
                    match self.run_list(body, stdin, sink) {
                        Flow::Next(c) => status = c,
                        Flow::Break(n) => {
                            if n > 1 {
                                return Flow::Break(n - 1);
                            }
                            break;
                        }
                        Flow::Continue(n) => {
                            if n > 1 {
                                return Flow::Continue(n - 1);
                            }
                        }
                        Flow::Exit(c) => return Flow::Exit(c),
                    }
                }
                Flow::Next(status)
            }
            Cmd::Group { body, .. } => self.run_list(body, stdin, sink),
        }
    }

    fn run_simple(
        &mut self,
        assigns: &[(String, Word)],
        words: &[Word],
        stdin: &mut Input,
        sink: &mut Sink,
    ) -> Flow {
        let mut argv = Vec::new();
        for w in words {
            argv.extend(self.expand(w, stdin));
        }
        let mut env_overrides = Vec::new();
        for (name, w) in assigns {
            let v = self.expand(w, stdin).join(" ");
            if argv.is_empty() {
                self.set_var(name, v);
            } else {
                env_overrides.push((name.clone(), v));
            }
        }
        if argv.is_empty() {
            // `$(cmd)` in an assignment leaves its status; a bare assignment 0.
            let has_subst = assigns
                .iter()
                .any(|(_, w)| w.iter().any(|p| matches!(p, Part::Cmd(..))));
            let c = if has_subst { self.last } else { 0 };
            self.last = c;
            return Flow::Next(c);
        }
        let flow = self.dispatch(&argv, &env_overrides, stdin, sink);
        if let Flow::Next(c) = flow {
            self.last = c;
        }
        flow
    }

    fn dispatch(
        &mut self,
        argv: &[String],
        env: &[(String, String)],
        stdin: &mut Input,
        sink: &mut Sink,
    ) -> Flow {
        let name = argv[0].as_str();
        let args = &argv[1..];
        // Shell builtins: they act on the shell itself.
        match name {
            "true" | ":" => return Flow::Next(0),
            "false" => return Flow::Next(1),
            "exit" => {
                let c = args
                    .first()
                    .and_then(|a| a.parse().ok())
                    .unwrap_or(self.last);
                return Flow::Exit(c);
            }
            "break" | "continue" => {
                let n = args
                    .first()
                    .and_then(|a| a.parse().ok())
                    .unwrap_or(1u32)
                    .max(1);
                return if name == "break" {
                    Flow::Break(n)
                } else {
                    Flow::Continue(n)
                };
            }
            "export" => {
                for a in args {
                    let (n, v) = match a.split_once('=') {
                        Some((n, v)) => (n.to_string(), Some(v.to_string())),
                        None => (a.clone(), None),
                    };
                    if let Some(v) = v {
                        self.set_var(&n, v);
                    }
                    self.exported.insert(n);
                }
                return Flow::Next(0);
            }
            "unset" => {
                for a in args {
                    self.vars.remove(a);
                    self.exported.remove(a);
                }
                return Flow::Next(0);
            }
            "cd" => {
                let target = match args.first() {
                    Some(t) if t == "-" => self.var("OLDPWD").unwrap_or_default(),
                    Some(t) => t.clone(),
                    None => self.var("HOME").unwrap_or_default(),
                };
                let old = std::env::current_dir().ok();
                return match std::env::set_current_dir(&target) {
                    Ok(()) => {
                        if let Some(o) = old {
                            self.set_var("OLDPWD", o.to_string_lossy().into_owned());
                        }
                        if let Ok(d) = std::env::current_dir() {
                            self.set_var("PWD", d.to_string_lossy().into_owned());
                        }
                        Flow::Next(0)
                    }
                    Err(e) => {
                        sink.eline(&format!("{SHELL_NAME}: cd: {target}: {}", io_reason(&e)));
                        Flow::Next(1)
                    }
                };
            }
            "read" => return Flow::Next(self.read_builtin(args, stdin)),
            "pwd" => {
                sink.out.extend_from_slice(logical_pwd().as_bytes());
                sink.out.push(b'\n');
                return Flow::Next(0);
            }
            _ => {}
        }
        if env.is_empty() {
            match utility(name, args, stdin, sink) {
                Util::Done(c) => return Flow::Next(c),
                Util::Decline => {}
            }
        }
        Flow::Next(self.external(argv, env, stdin, sink))
    }

    fn read_builtin(&mut self, args: &[String], stdin: &mut Input) -> i32 {
        let mut raw = false;
        let mut names = Vec::new();
        for a in args {
            if a == "-r" {
                raw = true;
            } else if a.starts_with('-') {
                // -p, -t, -n …: honoured by ignoring would be a lie; say no.
                let _ = writeln!(
                    std::io::stderr(),
                    "{SHELL_NAME}: read: {a}: unsupported option"
                );
                return 2;
            } else {
                names.push(a.clone());
            }
        }
        if names.is_empty() {
            names.push("REPLY".into());
        }
        let Some(line) = stdin.read_line() else {
            for n in &names {
                self.set_var(n, String::new());
            }
            return 1;
        };
        let mut line = String::from_utf8_lossy(&line).into_owned();
        if !raw {
            let mut s = String::new();
            let mut it = line.chars();
            while let Some(c) = it.next() {
                if c == '\\' {
                    if let Some(n) = it.next() {
                        s.push(n);
                    }
                } else {
                    s.push(c);
                }
            }
            line = s;
        }
        if names.len() == 1 && names[0] == "REPLY" {
            self.set_var("REPLY", line);
            return 0;
        }
        let trimmed = line.trim_matches(|c| c == ' ' || c == '\t');
        let mut rest = trimmed;
        for (k, n) in names.iter().enumerate() {
            if k + 1 == names.len() {
                self.set_var(n, rest.to_string());
            } else {
                let (head, tail) = match rest.find([' ', '\t']) {
                    Some(p) => (&rest[..p], rest[p..].trim_start_matches([' ', '\t'])),
                    None => (rest, ""),
                };
                self.set_var(n, head.to_string());
                rest = tail;
            }
        }
        0
    }

    /// Run a program that is not implemented here: directly, with no shell in
    /// between, and only after the effect gate has admitted it.
    fn external(
        &mut self,
        argv: &[String],
        env: &[(String, String)],
        stdin: &mut Input,
        sink: &mut Sink,
    ) -> i32 {
        if let Err(e) = crate::safety::guard_exec("posix", argv.join(" ")) {
            sink.eline(&e.to_string());
            return e.code.exit_code();
        }
        let mut cmd = std::process::Command::new(&argv[0]);
        cmd.args(&argv[1..]);
        for n in &self.exported {
            if let Some(v) = self.vars.get(n) {
                cmd.env(n, v);
            }
        }
        for (n, v) in env {
            cmd.env(n, v);
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(match sink.err_mode {
            ErrMode::Real => std::process::Stdio::inherit(),
            ErrMode::Null => std::process::Stdio::null(),
            _ => std::process::Stdio::piped(),
        });
        let feed = stdin.is_buffered();
        cmd.stdin(if feed {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::inherit()
        });
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                sink.eline(&format!("{SHELL_NAME}: {}: command not found", argv[0]));
                return 127;
            }
            Err(e) => {
                sink.eline(&format!("{SHELL_NAME}: {}: {}", argv[0], io_reason(&e)));
                return 126;
            }
        };
        let writer = if feed {
            let data = stdin.read_all();
            child.stdin.take().map(|mut w| {
                std::thread::spawn(move || {
                    let _ = w.write_all(&data);
                })
            })
        } else {
            None
        };
        let out = match child.wait_with_output() {
            Ok(o) => o,
            Err(e) => {
                sink.eline(&format!("{SHELL_NAME}: {}: {}", argv[0], io_reason(&e)));
                return 1;
            }
        };
        if let Some(w) = writer {
            let _ = w.join();
        }
        sink.out.extend_from_slice(&out.stdout);
        if !out.stderr.is_empty() {
            sink.ewrite(&out.stderr);
        }
        out.status.code().unwrap_or(128)
    }
}

/// Write a redirection's output through the same guard as `file.write`.
fn write_file(path: &str, data: &[u8], append: bool) -> Result<(), String> {
    let resolved = crate::safety::resolve_path_str(path);
    crate::safety::guard(crate::safety::GuardCtx {
        builtin: "posix",
        effect: crate::safety::Effect::WriteLocal,
        what: "write",
        targets: vec![resolved.clone()],
        blast_radius: serde_json::json!({ "path": resolved }),
        reversible: true,
        fs_paths: true,
    })
    .map_err(|e| e.to_string())?;
    let res = if append {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut f| f.write_all(data))
    } else {
        std::fs::write(path, data)
    };
    res.map_err(|e| format!("{SHELL_NAME}: {path}: {}", io_reason(&e)))
}

/// The message GNU tools print for an I/O error.
fn io_reason(e: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound => "No such file or directory".into(),
        PermissionDenied => "Permission denied".into(),
        IsADirectory => "Is a directory".into(),
        NotADirectory => "Not a directory".into(),
        AlreadyExists => "File exists".into(),
        _ => {
            if e.raw_os_error() == Some(21) {
                return "Is a directory".into();
            }
            let s = e.to_string();
            s.split(" (os error").next().unwrap_or(&s).to_string()
        }
    }
}

fn logical_pwd() -> String {
    let phys = std::env::current_dir().ok();
    if let (Ok(pwd), Some(phys)) = (std::env::var("PWD"), phys.as_ref()) {
        if std::fs::canonicalize(&pwd).ok().as_deref()
            == std::fs::canonicalize(phys).ok().as_deref()
        {
            return pwd;
        }
    }
    phys.map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

// ════════════════════════════════════════════════════════════════════════
// Collation and globbing
// ════════════════════════════════════════════════════════════════════════

/// Whether sorting follows a natural-language locale rather than bytes. `ls`
/// and `sort` both use `strcoll`, so under `en_US.UTF-8` they do not sort by
/// byte, and output that is meant to match theirs cannot either.
fn locale_collates() -> bool {
    let loc = ["LC_ALL", "LC_COLLATE", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty())
        .unwrap_or_default();
    !(loc.is_empty() || loc == "C" || loc == "POSIX" || loc.starts_with("C."))
}

/// glibc's ISO 14651 ordering for ASCII, as measured against glibc 2.41's
/// `en_US.UTF-8` (`printf … | sort`): compare letters and digits ignoring case
/// and punctuation; then lower case before upper, position by position; then
/// the full strings with punctuation ranked below letters and digits. Beyond
/// ASCII this falls back to bytes, which is where it stops being exact.
fn collate(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    if !locale_collates() {
        return a.as_bytes().cmp(b.as_bytes());
    }
    let l1 = |s: &str| -> Vec<char> {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect()
    };
    let o = l1(a).cmp(&l1(b));
    if o != Ordering::Equal {
        return o;
    }
    let l3 = |s: &str| -> Vec<u8> {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .map(|c| u8::from(c.is_uppercase()))
            .collect()
    };
    let o = l3(a).cmp(&l3(b));
    if o != Ordering::Equal {
        return o;
    }
    let l4 = |s: &str| -> Vec<(u8, u32)> {
        s.chars()
            .map(|c| (u8::from(c.is_alphanumeric()), c as u32))
            .collect()
    };
    let o = l4(a).cmp(&l4(b));
    if o != Ordering::Equal {
        return o;
    }
    a.as_bytes().cmp(b.as_bytes())
}

/// `fnmatch` for `*`, `?` and `[…]` (with `!`/`^` negation and ranges).
fn glob_match(pat: &str, s: &str) -> bool {
    fn go(p: &[char], s: &[char]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some('*') => (0..=s.len()).any(|k| go(&p[1..], &s[k..])),
            Some('?') => !s.is_empty() && go(&p[1..], &s[1..]),
            Some('[') => {
                let Some(&c) = s.first() else { return false };
                let mut k = 1;
                let negate = matches!(p.get(1), Some('!') | Some('^'));
                if negate {
                    k += 1;
                }
                let mut matched = false;
                let mut first = true;
                while k < p.len() && (p[k] != ']' || first) {
                    first = false;
                    if k + 2 < p.len() && p[k + 1] == '-' && p[k + 2] != ']' {
                        if p[k] <= c && c <= p[k + 2] {
                            matched = true;
                        }
                        k += 3;
                    } else {
                        if p[k] == c {
                            matched = true;
                        }
                        k += 1;
                    }
                }
                if k >= p.len() {
                    // No closing bracket: a literal '['.
                    return c == '[' && go(&p[1..], &s[1..]);
                }
                matched != negate && go(&p[k + 1..], &s[1..])
            }
            Some('\\') if p.len() > 1 => s.first() == Some(&p[1]) && go(&p[2..], &s[1..]),
            Some(&c) => s.first() == Some(&c) && go(&p[1..], &s[1..]),
        }
    }
    let p: Vec<char> = pat.chars().collect();
    let s: Vec<char> = s.chars().collect();
    go(&p, &s)
}

/// Pathname expansion, component by component, sorted as `ls` would sort.
/// `**` is an ordinary `*` here, as in bash without `globstar`.
fn glob_expand(pattern: &str) -> Vec<String> {
    expand_pattern(pattern, false, |a, b| collate(a, b))
}

/// Pathname expansion for the typed builtins (`glob`, `cat`, `ls`): `**`
/// matches any number of directories, and results sort by bytes, so the
/// answer does not depend on the locale the shell happens to run under.
pub fn glob_paths(pattern: &str) -> Vec<String> {
    let mut v = expand_pattern(pattern, true, |a, b| a.as_bytes().cmp(b.as_bytes()));
    v.sort();
    v.dedup();
    v
}

/// Whether a string contains glob metacharacters.
pub fn has_glob_meta(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

/// The directories beneath `dir`, depth first, not following symbolic links
/// and skipping hidden names, as bash's `globstar` does.
fn descendant_dirs(dir: &str, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(if dir.is_empty() { "." } else { dir }) else {
        return;
    };
    let mut names: Vec<String> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    for n in names {
        let p = if dir.is_empty() {
            n
        } else if dir.ends_with('/') {
            format!("{dir}{n}")
        } else {
            format!("{dir}/{n}")
        };
        out.push(p.clone());
        descendant_dirs(&p, out);
    }
}

fn expand_pattern(
    pattern: &str,
    globstar: bool,
    order: impl Fn(&String, &String) -> std::cmp::Ordering,
) -> Vec<String> {
    let absolute = pattern.starts_with('/');
    let comps: Vec<&str> = pattern.split('/').filter(|c| !c.is_empty()).collect();
    let mut bases: Vec<String> = vec![if absolute { "/".into() } else { String::new() }];
    for (i, comp) in comps.iter().enumerate() {
        let last = i + 1 == comps.len();
        let mut next = Vec::new();
        if globstar && *comp == "**" {
            // Zero or more directories: each base, and everything below it.
            for base in &bases {
                next.push(base.clone());
                descendant_dirs(base, &mut next);
            }
            if last {
                // A trailing `**` also names the files in those directories.
                let dirs = next.clone();
                for d in dirs {
                    let dir = if d.is_empty() {
                        ".".to_string()
                    } else {
                        d.clone()
                    };
                    if let Ok(rd) = std::fs::read_dir(&dir) {
                        for e in rd.filter_map(|e| e.ok()) {
                            let n = e.file_name().to_string_lossy().into_owned();
                            if !n.starts_with('.')
                                && e.file_type().map(|t| !t.is_dir()).unwrap_or(false)
                            {
                                next.push(if d.is_empty() { n } else { format!("{d}/{n}") });
                            }
                        }
                    }
                }
                next.retain(|p| !p.is_empty());
            }
            bases = next;
            continue;
        }
        let has_meta = has_glob_meta(comp);
        for base in &bases {
            let join = |name: &str| {
                if base.is_empty() {
                    name.to_string()
                } else if base.ends_with('/') {
                    format!("{base}{name}")
                } else {
                    format!("{base}/{name}")
                }
            };
            if !has_meta {
                let p = join(comp);
                let path = Path::new(&p);
                if (last && path.exists()) || (!last && path.is_dir()) {
                    next.push(p);
                }
                continue;
            }
            let dir = if base.is_empty() { "." } else { base.as_str() };
            let Ok(rd) = std::fs::read_dir(dir) else {
                continue;
            };
            let mut names: Vec<String> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.starts_with('.') || comp.starts_with('.'))
                .filter(|n| glob_match(comp, n))
                .collect();
            names.sort_by(&order);
            for n in names {
                let p = join(&n);
                if last || Path::new(&p).is_dir() {
                    next.push(p);
                }
            }
        }
        bases = next;
    }
    if pattern.ends_with('/') {
        bases = bases.into_iter().map(|b| format!("{b}/")).collect();
    }
    bases
}

// ════════════════════════════════════════════════════════════════════════
// Utilities
// ════════════════════════════════════════════════════════════════════════

/// Read a utility's inputs: each operand in turn (`-` is stdin), or stdin
/// when there are none. Missing files are reported the GNU way and skipped.
fn read_inputs(
    tool: &str,
    files: &[String],
    stdin: &mut Input,
    sink: &mut Sink,
    status: &mut i32,
) -> Vec<(String, Vec<u8>)> {
    if files.is_empty() {
        return vec![("(standard input)".into(), stdin.read_all())];
    }
    let mut out = Vec::new();
    for f in files {
        if f == "-" {
            out.push(("(standard input)".into(), stdin.read_all()));
            continue;
        }
        match std::fs::read(f) {
            Ok(d) => out.push((f.clone(), d)),
            Err(e) => {
                sink.eline(&format!("{tool}: {f}: {}", io_reason(&e)));
                *status = 1;
            }
        }
    }
    out
}

fn lines_of(data: &[u8]) -> Vec<&[u8]> {
    if data.is_empty() {
        return Vec::new();
    }
    let mut v: Vec<&[u8]> = data.split(|&b| b == b'\n').collect();
    if data.ends_with(b"\n") {
        v.pop();
    }
    v
}

/// Split `-abc` style flags; returns (flags, operands) or `None` when an
/// option takes a value we do not model.
fn short_flags(args: &[String], allowed: &str) -> Option<(HashSet<char>, Vec<String>)> {
    let mut flags = HashSet::new();
    let mut ops = Vec::new();
    let mut done = false;
    for a in args {
        if !done && a == "--" {
            done = true;
        } else if !done && a.len() > 1 && a.starts_with('-') {
            for c in a[1..].chars() {
                if !allowed.contains(c) {
                    return None;
                }
                flags.insert(c);
            }
        } else {
            ops.push(a.clone());
        }
    }
    Some((flags, ops))
}

fn utility(name: &str, args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    match name {
        "echo" => echo(args, sink),
        "printf" => printf(args, sink),
        "test" => Util::Done(test(args, sink)),
        "[" => {
            if args.last().map(String::as_str) != Some("]") {
                sink.eline(&format!("{SHELL_NAME}: [: missing `]'"));
                return Util::Done(2);
            }
            Util::Done(test(&args[..args.len() - 1], sink))
        }
        "cat" => cat(args, stdin, sink),
        "head" => head_tail(true, args, stdin, sink),
        "tail" => head_tail(false, args, stdin, sink),
        "wc" => wc(args, stdin, sink),
        "grep" => grep(args, stdin, sink),
        "sort" => sort(args, stdin, sink),
        "uniq" => uniq(args, stdin, sink),
        "cut" => cut(args, stdin, sink),
        "sed" => sed(args, stdin, sink),
        "ls" => ls(args, sink),
        "find" => find(args, sink),
        "date" => date(args, sink),
        "basename" => match args {
            [p] | [p, _] => {
                let trimmed = p.trim_end_matches('/');
                let mut b = trimmed.rsplit('/').next().unwrap_or("").to_string();
                if trimmed.is_empty() && p.starts_with('/') {
                    b = "/".into();
                }
                if let [_, suffix] = args {
                    if b.len() > suffix.len() && b.ends_with(suffix.as_str()) {
                        b.truncate(b.len() - suffix.len());
                    }
                }
                sink.out.extend_from_slice(format!("{b}\n").as_bytes());
                Util::Done(0)
            }
            _ => Util::Decline,
        },
        "dirname" => match args {
            [p] => {
                let t = p.trim_end_matches('/');
                let d = match t.rfind('/') {
                    Some(0) => "/".to_string(),
                    Some(i) => t[..i].trim_end_matches('/').to_string(),
                    None => ".".to_string(),
                };
                let d = if t.is_empty() && p.starts_with('/') {
                    "/".into()
                } else {
                    d
                };
                sink.out.extend_from_slice(format!("{d}\n").as_bytes());
                Util::Done(0)
            }
            _ => Util::Decline,
        },
        "seq" => {
            let nums: Option<Vec<i64>> = args.iter().map(|a| a.parse().ok()).collect();
            let (first, step, last) = match nums.as_deref() {
                Some([l]) => (1, 1, *l),
                Some([f, l]) => (*f, 1, *l),
                Some([f, s, l]) if *s != 0 => (*f, *s, *l),
                _ => return Util::Decline,
            };
            let mut i = first;
            while (step > 0 && i <= last) || (step < 0 && i >= last) {
                sink.out.extend_from_slice(format!("{i}\n").as_bytes());
                i += step;
            }
            Util::Done(0)
        }
        _ => Util::Decline,
    }
}

fn unescape_echo(s: &str) -> (String, bool) {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('a') => out.push('\x07'),
            Some('b') => out.push('\x08'),
            Some('e') => out.push('\x1b'),
            Some('f') => out.push('\x0c'),
            Some('v') => out.push('\x0b'),
            Some('c') => return (out, true),
            Some('0') => {
                let mut v = 0u32;
                for _ in 0..3 {
                    match it.peek() {
                        Some(d @ '0'..='7') => {
                            v = v * 8 + (*d as u32 - '0' as u32);
                            it.next();
                        }
                        _ => break,
                    }
                }
                out.push(char::from_u32(v).unwrap_or('\0'));
            }
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    (out, false)
}

fn echo(args: &[String], sink: &mut Sink) -> Util {
    let mut newline = true;
    let mut escapes = false;
    let mut k = 0;
    while let Some(a) = args.get(k) {
        if a.len() > 1 && a.starts_with('-') && a[1..].chars().all(|c| matches!(c, 'n' | 'e' | 'E'))
        {
            for c in a[1..].chars() {
                match c {
                    'n' => newline = false,
                    'e' => escapes = true,
                    _ => escapes = false,
                }
            }
            k += 1;
        } else {
            break;
        }
    }
    let mut text = args[k..].join(" ");
    if escapes {
        let (t, stop) = unescape_echo(&text);
        text = t;
        if stop {
            newline = false;
        }
    }
    sink.out.extend_from_slice(text.as_bytes());
    if newline {
        sink.out.push(b'\n');
    }
    Util::Done(0)
}

fn printf(args: &[String], sink: &mut Sink) -> Util {
    let Some(fmt) = args.first() else {
        sink.eline("printf: usage: printf format [arguments]");
        return Util::Done(2);
    };
    let fmt: Vec<char> = fmt.chars().collect();
    let mut rest = args[1..].iter();
    let mut out = String::new();
    loop {
        let mut consumed = false;
        let mut i = 0;
        while i < fmt.len() {
            let c = fmt[i];
            if c == '\\' {
                let (e, _) =
                    unescape_echo(&fmt[i..(i + 2).min(fmt.len())].iter().collect::<String>());
                out.push_str(&e);
                i += 2;
                continue;
            }
            if c != '%' {
                out.push(c);
                i += 1;
                continue;
            }
            i += 1;
            if fmt.get(i) == Some(&'%') {
                out.push('%');
                i += 1;
                continue;
            }
            let mut spec = String::new();
            while let Some(&f) = fmt.get(i) {
                if f == '-' || f == '0' || f.is_ascii_digit() || f == '.' {
                    spec.push(f);
                    i += 1;
                } else {
                    break;
                }
            }
            let conv = fmt.get(i).copied().unwrap_or('s');
            i += 1;
            let arg = rest.next();
            consumed |= arg.is_some();
            let arg = arg.cloned().unwrap_or_default();
            let left = spec.starts_with('-');
            let zero = spec.trim_start_matches('-').starts_with('0');
            let width: usize = spec
                .trim_start_matches(['-', '0'])
                .split('.')
                .next()
                .and_then(|w| w.parse().ok())
                .unwrap_or(0);
            let body = match conv {
                's' | 'b' => {
                    if conv == 'b' {
                        unescape_echo(&arg).0
                    } else {
                        arg
                    }
                }
                'd' | 'i' => match arg.trim().parse::<i64>() {
                    Ok(n) => n.to_string(),
                    Err(_) if arg.is_empty() => "0".into(),
                    Err(_) => return Util::Decline,
                },
                'c' => arg.chars().next().map(String::from).unwrap_or_default(),
                _ => return Util::Decline,
            };
            let pad = width.saturating_sub(body.chars().count());
            if left {
                out.push_str(&body);
                out.push_str(&" ".repeat(pad));
            } else if zero && matches!(conv, 'd' | 'i') {
                let (sign, digits) = body
                    .strip_prefix('-')
                    .map_or(("", body.as_str()), |d| ("-", d));
                out.push_str(sign);
                out.push_str(&"0".repeat(pad));
                out.push_str(digits);
            } else {
                out.push_str(&" ".repeat(pad));
                out.push_str(&body);
            }
        }
        if !consumed || rest.len() == 0 {
            break;
        }
    }
    sink.out.extend_from_slice(out.as_bytes());
    Util::Done(0)
}

fn test(args: &[String], sink: &mut Sink) -> i32 {
    fn unary(op: &str, a: &str) -> Option<bool> {
        let md = std::fs::metadata(a);
        Some(match op {
            "-e" => md.is_ok(),
            "-f" => md.map(|m| m.is_file()).unwrap_or(false),
            "-d" => md.map(|m| m.is_dir()).unwrap_or(false),
            "-s" => md.map(|m| m.len() > 0).unwrap_or(false),
            "-r" | "-w" | "-x" => md.is_ok(),
            "-L" | "-h" => std::fs::symlink_metadata(a)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false),
            "-z" => a.is_empty(),
            "-n" => !a.is_empty(),
            _ => return None,
        })
    }
    fn binary(a: &str, op: &str, b: &str) -> Option<Result<bool, String>> {
        let num = |s: &str| {
            s.trim()
                .parse::<i64>()
                .map_err(|_| format!("{s}: integer expression expected"))
        };
        Some(match op {
            "=" | "==" => Ok(a == b),
            "!=" => Ok(a != b),
            "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" => num(a).and_then(|x| {
                num(b).map(|y| match op {
                    "-eq" => x == y,
                    "-ne" => x != y,
                    "-lt" => x < y,
                    "-le" => x <= y,
                    "-gt" => x > y,
                    _ => x >= y,
                })
            }),
            _ => return None,
        })
    }
    fn eval(a: &[String], sink: &mut Sink) -> Result<bool, i32> {
        // -a / -o, lowest precedence first.
        if let Some(p) = a.iter().rposition(|x| x == "-o") {
            if p > 0 && p + 1 < a.len() {
                return Ok(eval(&a[..p], sink)? || eval(&a[p + 1..], sink)?);
            }
        }
        if let Some(p) = a.iter().rposition(|x| x == "-a") {
            if p > 0 && p + 1 < a.len() {
                return Ok(eval(&a[..p], sink)? && eval(&a[p + 1..], sink)?);
            }
        }
        match a {
            [] => Ok(false),
            [x] => Ok(!x.is_empty()),
            [bang, rest @ ..] if bang == "!" => Ok(!eval(rest, sink)?),
            [op, x] => match unary(op, x) {
                Some(v) => Ok(v),
                None => {
                    sink.eline(&format!(
                        "{SHELL_NAME}: test: {op}: unary operator expected"
                    ));
                    Err(2)
                }
            },
            [x, op, y] => match binary(x, op, y) {
                Some(Ok(v)) => Ok(v),
                Some(Err(m)) => {
                    sink.eline(&format!("{SHELL_NAME}: test: {m}"));
                    Err(2)
                }
                None => {
                    sink.eline(&format!(
                        "{SHELL_NAME}: test: {op}: binary operator expected"
                    ));
                    Err(2)
                }
            },
            _ => {
                sink.eline(&format!("{SHELL_NAME}: test: too many arguments"));
                Err(2)
            }
        }
    }
    match eval(args, sink) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(c) => c,
    }
}

fn cat(args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let Some((flags, files)) = short_flags(args, "") else {
        return Util::Decline;
    };
    let _ = flags;
    let mut status = 0;
    for (name, data) in read_inputs_dirs("cat", &files, stdin, sink, &mut status) {
        let _ = name;
        sink.out.extend_from_slice(&data);
    }
    Util::Done(status)
}

/// Like [`read_inputs`], but a directory operand is the GNU "Is a directory"
/// error rather than an unreadable file.
fn read_inputs_dirs(
    tool: &str,
    files: &[String],
    stdin: &mut Input,
    sink: &mut Sink,
    status: &mut i32,
) -> Vec<(String, Vec<u8>)> {
    let mut ok = Vec::new();
    for f in files {
        if Path::new(f).is_dir() {
            sink.eline(&format!("{tool}: {f}: Is a directory"));
            *status = 1;
        } else {
            ok.push(f.clone());
        }
    }
    if !files.is_empty() && ok.is_empty() {
        return Vec::new();
    }
    read_inputs(tool, &ok, stdin, sink, status)
}

/// `head` and `tail`: `-n N`, `-nN`, `-N`, `-c N`, and `tail -n +N`.
fn head_tail(head: bool, args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let tool = if head { "head" } else { "tail" };
    let mut count: i64 = 10;
    let mut from_start = false;
    let mut bytes = false;
    let mut quiet = false;
    let mut files = Vec::new();
    let mut i = 0;
    let parse_n = |s: &str, from_start: &mut bool| -> Option<i64> {
        let s = if let Some(r) = s.strip_prefix('+') {
            *from_start = true;
            r
        } else {
            s
        };
        s.parse::<i64>().ok()
    };
    while i < args.len() {
        let a = &args[i];
        if a == "-n" || a == "-c" {
            bytes = a == "-c";
            let v = args.get(i + 1).and_then(|v| parse_n(v, &mut from_start));
            match v {
                Some(v) => count = v,
                None => return Util::Decline,
            }
            i += 2;
            continue;
        }
        if let Some(v) = a.strip_prefix("-n").or_else(|| a.strip_prefix("-c")) {
            if !v.is_empty() {
                bytes = a.starts_with("-c");
                match parse_n(v, &mut from_start) {
                    Some(v) => count = v,
                    None => return Util::Decline,
                }
                i += 1;
                continue;
            }
        }
        if a == "-q" {
            quiet = true;
        } else if a.len() > 1 && a.starts_with('-') && a[1..].chars().all(|c| c.is_ascii_digit()) {
            count = a[1..].parse().unwrap_or(10);
        } else if a.starts_with('-') && a != "-" {
            return Util::Decline;
        } else {
            files.push(a.clone());
        }
        i += 1;
    }
    let mut status = 0;
    let inputs = read_inputs_dirs(tool, &files, stdin, sink, &mut status);
    let many = inputs.len() > 1 && !quiet;
    for (k, (name, data)) in inputs.iter().enumerate() {
        if many {
            if k > 0 {
                sink.out.push(b'\n');
            }
            sink.out
                .extend_from_slice(format!("==> {name} <==\n").as_bytes());
        }
        if bytes {
            let n = data.len();
            let slice = if head {
                if count >= 0 {
                    &data[..(count as usize).min(n)]
                } else {
                    &data[..n.saturating_sub((-count) as usize)]
                }
            } else if from_start {
                &data[(count.max(1) as usize - 1).min(n)..]
            } else {
                &data[n.saturating_sub(count.unsigned_abs() as usize)..]
            };
            sink.out.extend_from_slice(slice);
            continue;
        }
        // Lines, keeping the file's own terminators.
        let mut starts = vec![0usize];
        for (j, &b) in data.iter().enumerate() {
            if b == b'\n' && j + 1 < data.len() {
                starts.push(j + 1);
            }
        }
        let nlines = if data.is_empty() { 0 } else { starts.len() };
        let (from, to) = if head {
            if count >= 0 {
                (0, (count as usize).min(nlines))
            } else {
                (0, nlines.saturating_sub((-count) as usize))
            }
        } else if from_start {
            ((count.max(1) as usize - 1).min(nlines), nlines)
        } else {
            (nlines.saturating_sub(count.unsigned_abs() as usize), nlines)
        };
        if from < to {
            let a = starts[from];
            let b = if to < nlines { starts[to] } else { data.len() };
            sink.out.extend_from_slice(&data[a..b]);
        }
    }
    Util::Done(status)
}

fn wc(args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let Some((flags, files)) = short_flags(args, "lwcm") else {
        return Util::Decline;
    };
    let (mut l, mut w, mut c, m) = (
        flags.contains(&'l'),
        flags.contains(&'w'),
        flags.contains(&'c'),
        flags.contains(&'m'),
    );
    if !(l || w || c || m) {
        l = true;
        w = true;
        c = true;
    }
    let mut status = 0;
    let from_stdin = files.is_empty();
    let inputs = read_inputs_dirs("wc", &files, stdin, sink, &mut status);
    let count = |d: &[u8]| -> [usize; 4] {
        let text = String::from_utf8_lossy(d);
        [
            d.iter().filter(|&&b| b == b'\n').count(),
            text.split_ascii_whitespace().count(),
            text.chars().count(),
            d.len(),
        ]
    };
    let rows: Vec<(String, [usize; 4])> =
        inputs.iter().map(|(n, d)| (n.clone(), count(d))).collect();
    let mut total = [0usize; 4];
    for (_, r) in &rows {
        for (t, v) in total.iter_mut().zip(r) {
            *t += v;
        }
    }
    let selected = |r: &[usize; 4]| {
        let mut v = Vec::new();
        if l {
            v.push(r[0]);
        }
        if w {
            v.push(r[1]);
        }
        if m {
            v.push(r[2]);
        }
        if c {
            v.push(r[3]);
        }
        v
    };
    let ncols = usize::from(l) + usize::from(w) + usize::from(c) + usize::from(m);
    // GNU pads to the width of the total byte count when every input is a
    // file, to 7 when one is a pipe, and not at all for a single number.
    let width = if ncols == 1 && rows.len() <= 1 {
        0
    } else if from_stdin || files.iter().any(|f| f == "-") {
        7
    } else {
        let sizes: u64 = files
            .iter()
            .filter_map(|f| std::fs::metadata(f).ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum();
        sizes.to_string().len().max(1)
    };
    let mut emit = |vals: Vec<usize>, name: Option<&str>| {
        let s: Vec<String> = vals.iter().map(|v| format!("{v:>width$}")).collect();
        let mut line = s.join(" ");
        if let Some(n) = name {
            line.push(' ');
            line.push_str(n);
        }
        line.push('\n');
        sink.out.extend_from_slice(line.as_bytes());
    };
    for (name, r) in &rows {
        emit(
            selected(r),
            if from_stdin {
                None
            } else {
                Some(name.as_str())
            },
        );
    }
    if rows.len() > 1 {
        emit(selected(&total), Some("total"));
    }
    Util::Done(status)
}

/// A POSIX basic regular expression as a Rust regex: in BRE, `+ ? | ( ) { }`
/// are literal and their backslashed forms are the operators.
pub(crate) fn bre_to_regex(p: &str) -> Option<String> {
    let mut out = String::new();
    let mut it = p.chars().peekable();
    let mut first = true;
    while let Some(c) = it.next() {
        match c {
            '\\' => match it.next() {
                Some(o @ ('+' | '?' | '|' | '(' | ')' | '{' | '}')) => out.push(o),
                Some(d @ '1'..='9') => {
                    let _ = d;
                    return None; // back-references
                }
                Some(o @ ('w' | 'W' | 's' | 'S' | 'b' | 'B')) => {
                    out.push('\\');
                    out.push(o);
                }
                Some('<') | Some('>') => out.push_str("\\b"),
                Some(o) => out.push_str(&regex::escape(&o.to_string())),
                None => out.push_str("\\\\"),
            },
            '+' | '?' | '|' | '(' | ')' | '{' | '}' => out.push_str(&regex::escape(&c.to_string())),
            '*' if first => out.push_str("\\*"),
            _ => out.push(c),
        }
        first = false;
    }
    Some(out)
}

/// An extended regex: already close to Rust's, but back-references are not.
fn ere_to_regex(p: &str) -> Option<String> {
    if p.contains("\\1") || p.contains("\\2") {
        return None;
    }
    Some(p.replace("\\<", "\\b").replace("\\>", "\\b"))
}

fn grep(args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let mut flags = HashSet::new();
    let mut patterns: Vec<String> = Vec::new();
    let mut ops = Vec::new();
    let mut i = 0;
    let mut done = false;
    while i < args.len() {
        let a = &args[i];
        if !done && a == "--" {
            done = true;
        } else if !done && a == "-e" {
            match args.get(i + 1) {
                Some(p) => patterns.push(p.clone()),
                None => return Util::Decline,
            }
            i += 1;
        } else if !done && a.starts_with("--") {
            return Util::Decline;
        } else if !done && a.len() > 1 && a.starts_with('-') {
            for c in a[1..].chars() {
                if !"rRnclLivEFwoqshHx".contains(c) {
                    return Util::Decline;
                }
                flags.insert(c);
            }
        } else {
            ops.push(a.clone());
        }
        i += 1;
    }
    if patterns.is_empty() {
        if ops.is_empty() {
            sink.eline("Usage: grep [OPTION]... PATTERNS [FILE]...");
            return Util::Done(2);
        }
        patterns.push(ops.remove(0));
    }
    let f = |c| flags.contains(&c);
    let recursive = f('r') || f('R');
    let mut alts = Vec::new();
    for p in patterns.iter().flat_map(|p| p.split('\n')) {
        let r = if f('F') {
            Some(regex::escape(p))
        } else if f('E') {
            ere_to_regex(p)
        } else {
            bre_to_regex(p)
        };
        let Some(mut r) = r else { return Util::Decline };
        if f('w') {
            r = format!("\\b(?:{r})\\b");
        }
        if f('x') {
            r = format!("^(?:{r})$");
        }
        alts.push(format!("(?:{r})"));
    }
    let src = format!("{}{}", if f('i') { "(?i)" } else { "" }, alts.join("|"));
    let Ok(re) = regex::bytes::Regex::new(&src) else {
        return Util::Decline;
    };

    // Collect (display name, path or stdin) in GNU's order.
    let mut status_err = false;
    let mut targets: Vec<(String, Option<String>)> = Vec::new();
    if ops.is_empty() {
        if recursive {
            walk_files(".", "", &mut targets, true);
        } else {
            targets.push(("(standard input)".into(), None));
        }
    } else {
        for op in &ops {
            let p = Path::new(op);
            if op == "-" {
                targets.push(("(standard input)".into(), None));
            } else if p.is_dir() {
                if recursive {
                    walk_files(op, op, &mut targets, false);
                } else if !f('s') {
                    sink.eline(&format!("grep: {op}: Is a directory"));
                }
            } else if p.exists() {
                targets.push((op.clone(), Some(op.clone())));
            } else {
                if !f('s') {
                    sink.eline(&format!("grep: {op}: No such file or directory"));
                }
                status_err = true;
            }
        }
    }
    let prefix = (targets.len() > 1 || recursive || ops.len() > 1) && !f('h') || f('H');
    let mut any = false;
    for (name, path) in &targets {
        let data = match path {
            Some(p) => match std::fs::read(p) {
                Ok(d) => d,
                Err(e) => {
                    if !f('s') {
                        sink.eline(&format!("grep: {name}: {}", io_reason(&e)));
                    }
                    status_err = true;
                    continue;
                }
            },
            None => stdin.read_all(),
        };
        let binary = data.iter().take(32 * 1024).any(|&b| b == 0);
        let mut count = 0usize;
        let mut out = Vec::new();
        for (n, line) in lines_of(&data).into_iter().enumerate() {
            let hit = re.is_match(line) != f('v');
            if !hit {
                continue;
            }
            count += 1;
            if f('q') || f('l') || f('L') || f('c') || binary {
                continue;
            }
            let lead = {
                let mut s = Vec::new();
                if prefix {
                    s.extend_from_slice(name.as_bytes());
                    s.push(b':');
                }
                if f('n') {
                    s.extend_from_slice(format!("{}:", n + 1).as_bytes());
                }
                s
            };
            if f('o') && !f('v') {
                for m in re.find_iter(line) {
                    if m.as_bytes().is_empty() {
                        continue;
                    }
                    out.extend_from_slice(&lead);
                    out.extend_from_slice(m.as_bytes());
                    out.push(b'\n');
                }
            } else {
                out.extend_from_slice(&lead);
                out.extend_from_slice(line);
                out.push(b'\n');
            }
        }
        if count > 0 {
            any = true;
        }
        if f('q') {
            if any {
                return Util::Done(0);
            }
            continue;
        }
        if f('l') {
            if count > 0 {
                sink.out.extend_from_slice(format!("{name}\n").as_bytes());
            }
        } else if f('L') {
            if count == 0 {
                sink.out.extend_from_slice(format!("{name}\n").as_bytes());
            }
        } else if f('c') {
            let line = if prefix {
                format!("{name}:{count}\n")
            } else {
                format!("{count}\n")
            };
            sink.out.extend_from_slice(line.as_bytes());
        } else if binary {
            if count > 0 {
                sink.eline(&format!("grep: {name}: binary file matches"));
            }
        } else {
            sink.out.extend_from_slice(&out);
        }
    }
    Util::Done(if status_err && !(any && f('q')) {
        2
    } else if any {
        0
    } else {
        1
    })
}

/// Files beneath `dir` in directory order, depth first, as GNU grep -r and
/// find visit them; symbolic links met during the walk are not followed.
fn walk_files(dir: &str, shown: &str, out: &mut Vec<(String, Option<String>)>, strip_dot: bool) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.filter_map(|e| e.ok()) {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = if dir.ends_with('/') {
            format!("{dir}{name}")
        } else {
            format!("{dir}/{name}")
        };
        let display = if strip_dot && shown.is_empty() {
            name.clone()
        } else if shown.ends_with('/') {
            format!("{shown}{name}")
        } else {
            format!("{shown}/{name}")
        };
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            walk_files(&path, &display, out, false);
        } else {
            out.push((display, Some(path)));
        }
    }
}

fn numeric_prefix(s: &str) -> f64 {
    let t = s.trim_start();
    let mut end = 0;
    for (i, c) in t.char_indices() {
        if c.is_ascii_digit() || c == '.' || (i == 0 && c == '-') {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    t[..end].parse().unwrap_or(0.0)
}

fn sort(args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let Some((flags, files)) = short_flags(args, "rnuf") else {
        return Util::Decline;
    };
    let mut status = 0;
    let mut lines: Vec<String> = Vec::new();
    for (_, d) in read_inputs("sort", &files, stdin, sink, &mut status) {
        lines.extend(
            lines_of(&d)
                .iter()
                .map(|l| String::from_utf8_lossy(l).into_owned()),
        );
    }
    let numeric = flags.contains(&'n');
    let fold = flags.contains(&'f');
    let key_cmp = |a: &String, b: &String| -> std::cmp::Ordering {
        if numeric {
            numeric_prefix(a)
                .partial_cmp(&numeric_prefix(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        } else if fold {
            collate(&a.to_uppercase(), &b.to_uppercase())
        } else {
            collate(a, b)
        }
    };
    lines.sort_by(|a, b| key_cmp(a, b).then_with(|| collate(a, b)));
    if flags.contains(&'u') {
        lines.dedup_by(|a, b| key_cmp(a, b) == std::cmp::Ordering::Equal);
    }
    if flags.contains(&'r') {
        lines.reverse();
    }
    for l in lines {
        sink.out.extend_from_slice(l.as_bytes());
        sink.out.push(b'\n');
    }
    Util::Done(status)
}

fn uniq(args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let Some((flags, files)) = short_flags(args, "cdui") else {
        return Util::Decline;
    };
    if files.len() > 1 {
        return Util::Decline;
    }
    let mut status = 0;
    let data = read_inputs("uniq", &files, stdin, sink, &mut status)
        .into_iter()
        .next()
        .map(|(_, d)| d)
        .unwrap_or_default();
    let eq = |a: &[u8], b: &[u8]| {
        if flags.contains(&'i') {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    let lines = lines_of(&data);
    let mut k = 0;
    while k < lines.len() {
        let mut j = k + 1;
        while j < lines.len() && eq(lines[j], lines[k]) {
            j += 1;
        }
        let n = j - k;
        let keep = (!flags.contains(&'d') || n > 1) && (!flags.contains(&'u') || n == 1);
        if keep {
            if flags.contains(&'c') {
                sink.out.extend_from_slice(format!("{n:>7} ").as_bytes());
            }
            sink.out.extend_from_slice(lines[k]);
            sink.out.push(b'\n');
        }
        k = j;
    }
    Util::Done(status)
}

/// A `cut` list: `1,3-5,7-` as inclusive 1-based ranges.
fn cut_list(s: &str) -> Option<Vec<(usize, usize)>> {
    let mut v = Vec::new();
    for part in s.split(',') {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (
                if a.is_empty() { 1 } else { a.parse().ok()? },
                if b.is_empty() {
                    usize::MAX
                } else {
                    b.parse().ok()?
                },
            ),
            None => {
                let n = part.parse().ok()?;
                (n, n)
            }
        };
        if a == 0 {
            return None;
        }
        v.push((a, b));
    }
    Some(v)
}

fn cut(args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let mut delim = '\t';
    let mut fields = None;
    let mut chars = None;
    let mut only_delimited = false;
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let take = |i: &mut usize, flag: &str| -> Option<String> {
            if a.len() > flag.len() {
                Some(a[flag.len()..].to_string())
            } else {
                *i += 1;
                args.get(*i).cloned()
            }
        };
        if a.starts_with("-d") {
            let d = take(&mut i, "-d");
            match d.and_then(|d| d.chars().next()) {
                Some(c) => delim = c,
                None => return Util::Decline,
            }
        } else if a.starts_with("-f") {
            fields = take(&mut i, "-f").and_then(|l| cut_list(&l));
            if fields.is_none() {
                return Util::Decline;
            }
        } else if a.starts_with("-c") || a.starts_with("-b") {
            chars = take(&mut i, "-c").and_then(|l| cut_list(&l));
            if chars.is_none() {
                return Util::Decline;
            }
        } else if a == "-s" {
            only_delimited = true;
        } else if a.starts_with('-') && a != "-" {
            return Util::Decline;
        } else {
            files.push(a.clone());
        }
        i += 1;
    }
    if fields.is_none() && chars.is_none() {
        sink.eline("cut: you must specify a list of bytes, characters, or fields");
        return Util::Done(1);
    }
    let within =
        |ranges: &[(usize, usize)], n: usize| ranges.iter().any(|&(a, b)| a <= n && n <= b);
    let mut status = 0;
    for (_, data) in read_inputs("cut", &files, stdin, sink, &mut status) {
        for line in lines_of(&data) {
            let text = String::from_utf8_lossy(line);
            if let Some(r) = &chars {
                let s: String = text
                    .chars()
                    .enumerate()
                    .filter(|(k, _)| within(r, k + 1))
                    .map(|(_, c)| c)
                    .collect();
                sink.out.extend_from_slice(s.as_bytes());
                sink.out.push(b'\n');
            } else if let Some(r) = &fields {
                if !text.contains(delim) {
                    if !only_delimited {
                        sink.out.extend_from_slice(line);
                        sink.out.push(b'\n');
                    }
                    continue;
                }
                let picked: Vec<&str> = text
                    .split(delim)
                    .enumerate()
                    .filter(|(k, _)| within(r, k + 1))
                    .map(|(_, f)| f)
                    .collect();
                sink.out
                    .extend_from_slice(picked.join(&delim.to_string()).as_bytes());
                sink.out.push(b'\n');
            }
        }
    }
    Util::Done(status)
}

#[derive(Clone)]
enum Addr {
    Line(usize),
    Last,
    Re(regex::bytes::Regex),
}

enum SedCmd {
    Print,
    Delete,
    Quit,
    Subst(regex::bytes::Regex, Vec<u8>, bool, bool),
}

struct SedLine {
    a1: Option<Addr>,
    a2: Option<Addr>,
    cmd: SedCmd,
    active: bool,
}

/// `sed` for the everyday scripts: `Np`, `a,bp`, `$p`, `/re/p`, `Nd`, `q`,
/// and `s/re/rep/[gp]`, with `-n`, `-e` and `-E`. Anything else declines.
fn sed(args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let mut quiet = false;
    let mut ere = false;
    let mut scripts = Vec::new();
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-n" => quiet = true,
            "-E" | "-r" => ere = true,
            "-e" => {
                i += 1;
                match args.get(i) {
                    Some(s) => scripts.push(s.clone()),
                    None => return Util::Decline,
                }
            }
            "-ne" | "-nE" | "-En" => {
                quiet = true;
                ere = a.contains('E');
                if a == "-ne" {
                    i += 1;
                    match args.get(i) {
                        Some(s) => scripts.push(s.clone()),
                        None => return Util::Decline,
                    }
                }
            }
            s if s.starts_with('-') && s != "-" => return Util::Decline,
            _ => {
                if scripts.is_empty() {
                    scripts.push(a.clone());
                } else {
                    files.push(a.clone());
                }
            }
        }
        i += 1;
    }
    let to_re = |p: &str| -> Option<regex::bytes::Regex> {
        let r = if ere {
            ere_to_regex(p)?
        } else {
            bre_to_regex(p)?
        };
        regex::bytes::Regex::new(&r).ok()
    };
    let mut prog: Vec<SedLine> = Vec::new();
    for script in &scripts {
        for raw in script.split(['\n', ';']) {
            let s = raw.trim();
            if s.is_empty() {
                continue;
            }
            let c: Vec<char> = s.chars().collect();
            let mut k = 0;
            let addr = |k: &mut usize| -> Result<Option<Addr>, ()> {
                match c.get(*k) {
                    Some(d) if d.is_ascii_digit() => {
                        let mut n = 0usize;
                        while let Some(d) = c.get(*k).filter(|d| d.is_ascii_digit()) {
                            n = n * 10 + (*d as usize - '0' as usize);
                            *k += 1;
                        }
                        Ok(Some(Addr::Line(n)))
                    }
                    Some('$') => {
                        *k += 1;
                        Ok(Some(Addr::Last))
                    }
                    Some('/') => {
                        let end = c[*k + 1..].iter().position(|&x| x == '/').ok_or(())?;
                        let pat: String = c[*k + 1..*k + 1 + end].iter().collect();
                        *k += end + 2;
                        Ok(Some(Addr::Re(to_re(&pat).ok_or(())?)))
                    }
                    _ => Ok(None),
                }
            };
            let Ok(a1) = addr(&mut k) else {
                return Util::Decline;
            };
            let mut a2 = None;
            if a1.is_some() && c.get(k) == Some(&',') {
                k += 1;
                match addr(&mut k) {
                    Ok(Some(a)) => a2 = Some(a),
                    _ => return Util::Decline,
                }
            }
            let rest: String = c[k..].iter().collect();
            let rest = rest.trim();
            let cmd = match rest {
                "p" => SedCmd::Print,
                "d" => SedCmd::Delete,
                "q" => SedCmd::Quit,
                r if r.starts_with('s') && r.len() > 1 => {
                    let sep = r[1..].chars().next().unwrap_or('/');
                    let body: Vec<&str> = r[1 + sep.len_utf8()..].split(sep).collect();
                    let [pat, rep, fl] = body.as_slice() else {
                        return Util::Decline;
                    };
                    if fl.chars().any(|f| !matches!(f, 'g' | 'p')) {
                        return Util::Decline;
                    }
                    let Some(re) = to_re(pat) else {
                        return Util::Decline;
                    };
                    // `&` is the match and `\N` a group: in regex's syntax
                    // `${0}` and `${N}`; a literal `$` must be escaped for it.
                    let mut r2 = Vec::new();
                    let mut it = rep.chars().peekable();
                    while let Some(ch) = it.next() {
                        match ch {
                            '&' => r2.extend_from_slice(b"${0}"),
                            '$' => r2.extend_from_slice(b"$$"),
                            '\\' => match it.next() {
                                Some(d @ '1'..='9') => {
                                    r2.extend_from_slice(format!("${{{d}}}").as_bytes())
                                }
                                Some('n') => r2.push(b'\n'),
                                Some('t') => r2.push(b'\t'),
                                Some(o) => {
                                    let mut b = [0; 4];
                                    r2.extend_from_slice(o.encode_utf8(&mut b).as_bytes());
                                }
                                None => r2.push(b'\\'),
                            },
                            o => {
                                let mut b = [0; 4];
                                r2.extend_from_slice(o.encode_utf8(&mut b).as_bytes());
                            }
                        }
                    }
                    SedCmd::Subst(re, r2, fl.contains('g'), fl.contains('p'))
                }
                _ => return Util::Decline,
            };
            prog.push(SedLine {
                a1,
                a2,
                cmd,
                active: false,
            });
        }
    }
    if prog.is_empty() {
        return Util::Decline;
    }
    let mut status = 0;
    let mut data = Vec::new();
    for (_, d) in read_inputs("sed", &files, stdin, sink, &mut status) {
        data.extend_from_slice(&d);
    }
    let lines = lines_of(&data);
    let n = lines.len();
    let hits = |a: &Addr, ln: usize, line: &[u8]| match a {
        Addr::Line(k) => *k == ln,
        Addr::Last => ln == n,
        Addr::Re(r) => r.is_match(line),
    };
    'lines: for (idx, line) in lines.iter().enumerate() {
        let ln = idx + 1;
        let mut pattern = line.to_vec();
        let mut deleted = false;
        for sl in prog.iter_mut() {
            let selected = match (&sl.a1, &sl.a2) {
                (None, _) => true,
                (Some(a), None) => hits(a, ln, &pattern),
                (Some(a), Some(b)) => {
                    if sl.active {
                        let end = match b {
                            Addr::Line(k) => ln >= *k,
                            _ => hits(b, ln, &pattern),
                        };
                        if end {
                            sl.active = false;
                        }
                        true
                    } else if hits(a, ln, &pattern) {
                        let ends_now = match b {
                            Addr::Line(k) => *k <= ln,
                            _ => false,
                        };
                        sl.active = !ends_now;
                        true
                    } else {
                        false
                    }
                }
            };
            if !selected {
                continue;
            }
            match &sl.cmd {
                SedCmd::Print => {
                    sink.out.extend_from_slice(&pattern);
                    sink.out.push(b'\n');
                }
                SedCmd::Delete => {
                    deleted = true;
                    break;
                }
                SedCmd::Quit => {
                    if !quiet {
                        sink.out.extend_from_slice(&pattern);
                        sink.out.push(b'\n');
                    }
                    break 'lines;
                }
                SedCmd::Subst(re, rep, global, print) => {
                    let replaced = if *global {
                        re.replace_all(&pattern, rep.as_slice()).into_owned()
                    } else {
                        re.replace(&pattern, rep.as_slice()).into_owned()
                    };
                    let changed = replaced != pattern;
                    pattern = replaced;
                    if changed && *print {
                        sink.out.extend_from_slice(&pattern);
                        sink.out.push(b'\n');
                    }
                }
            }
        }
        if !deleted && !quiet {
            sink.out.extend_from_slice(&pattern);
            sink.out.push(b'\n');
        }
    }
    Util::Done(status)
}

fn ls(args: &[String], sink: &mut Sink) -> Util {
    let Some((flags, mut ops)) = short_flags(args, "aAl1d") else {
        return Util::Decline;
    };
    let long = flags.contains(&'l');
    if long && !cfg!(unix) {
        return Util::Decline;
    }
    let all = flags.contains(&'a');
    let almost = flags.contains(&'A');
    let dirs_as_files = flags.contains(&'d');
    if ops.is_empty() {
        ops.push(".".into());
    }
    let mut status = 0;
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for op in &ops {
        let md = if long {
            std::fs::symlink_metadata(op)
        } else {
            std::fs::metadata(op)
        };
        match md {
            Ok(m) if m.is_dir() && !dirs_as_files => dirs.push(op.clone()),
            Ok(_) => files.push(op.clone()),
            Err(e) => {
                sink.eline(&format!("ls: cannot access '{op}': {}", io_reason(&e)));
                status = 2;
            }
        }
    }
    files.sort_by(|a, b| collate(a, b));
    dirs.sort_by(|a, b| collate(a, b));
    let headers = files.len() + dirs.len() > 1 || status != 0;
    let mut first = true;
    if !files.is_empty() {
        let entries: Vec<(String, String)> = files.iter().map(|f| (f.clone(), f.clone())).collect();
        emit_ls(&entries, long, false, sink);
        first = false;
    }
    for d in &dirs {
        if !first {
            sink.out.push(b'\n');
        }
        first = false;
        if headers {
            sink.out.extend_from_slice(format!("{d}:\n").as_bytes());
        }
        let Ok(rd) = std::fs::read_dir(d) else {
            sink.eline(&format!(
                "ls: cannot open directory '{d}': Permission denied"
            ));
            status = 2;
            continue;
        };
        let mut names: Vec<String> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| all || almost || !n.starts_with('.'))
            .collect();
        if all {
            names.push(".".into());
            names.push("..".into());
        }
        names.sort_by(|a, b| collate(a, b));
        let entries: Vec<(String, String)> = names
            .iter()
            .map(|n| {
                let p = if d.ends_with('/') {
                    format!("{d}{n}")
                } else {
                    format!("{d}/{n}")
                };
                (n.clone(), p)
            })
            .collect();
        emit_ls(&entries, long, true, sink);
    }
    Util::Done(status)
}

/// `(shown name, path)` pairs, in order.
fn emit_ls(entries: &[(String, String)], long: bool, total: bool, sink: &mut Sink) {
    if !long {
        for (n, _) in entries {
            sink.out.extend_from_slice(n.as_bytes());
            sink.out.push(b'\n');
        }
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let now = chrono::Local::now();
        let mut rows = Vec::new();
        let mut blocks: u64 = 0;
        for (name, path) in entries {
            let Ok(m) = std::fs::symlink_metadata(path) else {
                continue;
            };
            blocks += m.blocks();
            let ft = m.file_type();
            let mode = m.mode();
            let kind = if ft.is_dir() {
                'd'
            } else if ft.is_symlink() {
                'l'
            } else {
                use std::os::unix::fs::FileTypeExt;
                if ft.is_block_device() {
                    'b'
                } else if ft.is_char_device() {
                    'c'
                } else if ft.is_fifo() {
                    'p'
                } else if ft.is_socket() {
                    's'
                } else {
                    '-'
                }
            };
            let bit = |m: u32, c: char| if mode & m != 0 { c } else { '-' };
            let special =
                |x: u32, s: u32, lower: char, upper: char| match (mode & x != 0, mode & s != 0) {
                    (true, true) => lower,
                    (false, true) => upper,
                    (true, false) => 'x',
                    (false, false) => '-',
                };
            let perms: String = [
                kind,
                bit(0o400, 'r'),
                bit(0o200, 'w'),
                special(0o100, 0o4000, 's', 'S'),
                bit(0o040, 'r'),
                bit(0o020, 'w'),
                special(0o010, 0o2000, 's', 'S'),
                bit(0o004, 'r'),
                bit(0o002, 'w'),
                special(0o001, 0o1000, 't', 'T'),
            ]
            .iter()
            .collect();
            let mtime = chrono::DateTime::from_timestamp(m.mtime(), 0)
                .map(|t| t.with_timezone(&chrono::Local))
                .unwrap_or(now);
            let age = now.signed_duration_since(mtime);
            let recent = age.num_seconds() >= 0 && age.num_days() < 182;
            let date = if recent {
                mtime.format("%b %e %H:%M").to_string()
            } else {
                mtime.format("%b %e  %Y").to_string()
            };
            let mut shown = name.clone();
            if ft.is_symlink() {
                if let Ok(t) = std::fs::read_link(path) {
                    shown = format!("{name} -> {}", t.to_string_lossy());
                }
            }
            rows.push((
                perms,
                m.nlink().to_string(),
                user_name(m.uid()),
                group_name(m.gid()),
                m.size().to_string(),
                date,
                shown,
            ));
        }
        if total {
            sink.out
                .extend_from_slice(format!("total {}\n", blocks.div_ceil(2)).as_bytes());
        }
        let w = |f: fn(&(String, String, String, String, String, String, String)) -> &String| {
            rows.iter().map(|r| f(r).len()).max().unwrap_or(0)
        };
        let (wl, wu, wg, ws) = (w(|r| &r.1), w(|r| &r.2), w(|r| &r.3), w(|r| &r.4));
        for r in &rows {
            let line = format!(
                "{} {:>wl$} {:<wu$} {:<wg$} {:>ws$} {} {}\n",
                r.0, r.1, r.2, r.3, r.4, r.5, r.6
            );
            sink.out.extend_from_slice(line.as_bytes());
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (total, sink);
    }
}

#[cfg(unix)]
fn user_name(uid: u32) -> String {
    let mut buf = vec![0 as libc::c_char; 4096];
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: getpwuid_r writes into the caller's buffers and sets `result`
    // to `&pwd` or null; `pw_name` points into `buf`, which outlives the read.
    let rc = unsafe { libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut result) };
    if rc == 0 && !result.is_null() {
        // SAFETY: as above; the name is a NUL-terminated string within `buf`.
        let name = unsafe { std::ffi::CStr::from_ptr(pwd.pw_name) };
        return name.to_string_lossy().into_owned();
    }
    uid.to_string()
}

#[cfg(unix)]
fn group_name(gid: u32) -> String {
    let mut buf = vec![0 as libc::c_char; 4096];
    let mut grp: libc::group = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::group = std::ptr::null_mut();
    // SAFETY: getgrgid_r writes into the caller's buffers and sets `result`
    // to `&grp` or null; `gr_name` points into `buf`, which outlives the read.
    let rc = unsafe { libc::getgrgid_r(gid, &mut grp, buf.as_mut_ptr(), buf.len(), &mut result) };
    if rc == 0 && !result.is_null() {
        // SAFETY: as above.
        let name = unsafe { std::ffi::CStr::from_ptr(grp.gr_name) };
        return name.to_string_lossy().into_owned();
    }
    gid.to_string()
}

/// `find` with `-name`, `-iname`, `-path`, `-type`, `-maxdepth`, `-mindepth`,
/// `!`/`-not` and `-print`, visiting in directory order as GNU find does.
fn find(args: &[String], sink: &mut Sink) -> Util {
    enum Test {
        Name(String, bool),
        Path(String),
        Type(char),
    }
    let mut starts = Vec::new();
    let mut i = 0;
    while i < args.len() && !args[i].starts_with('-') && args[i] != "!" {
        starts.push(args[i].clone());
        i += 1;
    }
    if starts.is_empty() {
        starts.push(".".into());
    }
    let mut tests: Vec<(bool, Test)> = Vec::new();
    let mut maxdepth = usize::MAX;
    let mut mindepth = 0usize;
    let mut negate = false;
    while i < args.len() {
        let a = args[i].as_str();
        let val = args.get(i + 1).cloned();
        match a {
            "!" | "-not" => {
                negate = !negate;
                i += 1;
                continue;
            }
            "-print" => {}
            "-name" | "-iname" | "-path" | "-wholename" | "-type" | "-maxdepth" | "-mindepth" => {
                let Some(v) = val else { return Util::Decline };
                match a {
                    "-name" => tests.push((negate, Test::Name(v, false))),
                    "-iname" => tests.push((negate, Test::Name(v.to_lowercase(), true))),
                    "-path" | "-wholename" => tests.push((negate, Test::Path(v))),
                    "-type" => match v.as_str() {
                        "f" | "d" | "l" => {
                            tests.push((negate, Test::Type(v.chars().next().unwrap_or('f'))))
                        }
                        _ => return Util::Decline,
                    },
                    "-maxdepth" => match v.parse() {
                        Ok(n) => maxdepth = n,
                        Err(_) => return Util::Decline,
                    },
                    _ => match v.parse() {
                        Ok(n) => mindepth = n,
                        Err(_) => return Util::Decline,
                    },
                }
                i += 1;
            }
            _ => return Util::Decline,
        }
        negate = false;
        i += 1;
    }
    let matches = |path: &str, ft: &std::fs::FileType| {
        tests.iter().all(|(neg, t)| {
            let base = path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(path);
            let hit = match t {
                Test::Name(p, false) => glob_match(p, base),
                Test::Name(p, true) => glob_match(p, &base.to_lowercase()),
                Test::Path(p) => glob_match(p, path),
                Test::Type('d') => ft.is_dir(),
                Test::Type('l') => ft.is_symlink(),
                Test::Type(_) => ft.is_file(),
            };
            hit != *neg
        })
    };
    let mut status = 0;
    fn visit(
        path: &str,
        depth: usize,
        maxdepth: usize,
        mindepth: usize,
        matches: &dyn Fn(&str, &std::fs::FileType) -> bool,
        sink: &mut Sink,
        status: &mut i32,
    ) {
        let md = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) => {
                sink.eline(&format!("find: '{path}': {}", io_reason(&e)));
                *status = 1;
                return;
            }
        };
        let ft = md.file_type();
        if depth >= mindepth && matches(path, &ft) {
            sink.out.extend_from_slice(path.as_bytes());
            sink.out.push(b'\n');
        }
        if ft.is_dir() && depth < maxdepth {
            let rd = match std::fs::read_dir(path) {
                Ok(r) => r,
                Err(e) => {
                    sink.eline(&format!("find: '{path}': {}", io_reason(&e)));
                    *status = 1;
                    return;
                }
            };
            for e in rd.filter_map(|e| e.ok()) {
                let name = e.file_name().to_string_lossy().into_owned();
                let child = if path.ends_with('/') {
                    format!("{path}{name}")
                } else {
                    format!("{path}/{name}")
                };
                visit(&child, depth + 1, maxdepth, mindepth, matches, sink, status);
            }
        }
    }
    for s in &starts {
        visit(s, 0, maxdepth, mindepth, &matches, sink, &mut status);
    }
    Util::Done(status)
}

fn date(args: &[String], sink: &mut Sink) -> Util {
    match args {
        [f] if f.starts_with('+') => {
            let now = chrono::Local::now();
            let fmt = &f[1..];
            // chrono panics on a format it cannot parse; ask it first.
            use chrono::format::{Item, StrftimeItems};
            if StrftimeItems::new(fmt).any(|i| matches!(i, Item::Error)) {
                return Util::Decline;
            }
            sink.out
                .extend_from_slice(format!("{}\n", now.format(fmt)).as_bytes());
            Util::Done(0)
        }
        _ => Util::Decline,
    }
}

// ════════════════════════════════════════════════════════════════════════
// Entry point
// ════════════════════════════════════════════════════════════════════════

/// Run `src` as a shell script, writing to the real stdout and stderr.
///
/// Returns the exit status, or `None` when the script uses something this
/// subset does not run -- decided before anything executes, so the caller can
/// fall back without a half-run script behind it.
pub fn run(src: &str) -> Option<i32> {
    let list = parse(src).ok()?;
    let mut sh = Shell {
        vars: HashMap::new(),
        exported: HashSet::new(),
        last: 0,
    };
    let mut stdin = Input::real();
    let mut sink = Sink::new(ErrMode::Real);
    let mut status = 0;
    // Output reaches the terminal as each top-level command finishes, not at
    // the end -- but only here: a loop body inside a pipeline must feed the
    // next stage, not the terminal.
    for ao in &list {
        let flow = sh.run_and_or(ao, &mut stdin, &mut sink);
        let _ = std::io::stdout().write_all(&sink.out);
        let _ = std::io::stdout().flush();
        sink.out.clear();
        match flow {
            Flow::Next(c) => status = c,
            Flow::Exit(c) => return Some(c),
            Flow::Break(_) | Flow::Continue(_) => {}
        }
    }
    Some(status)
}

/// Whether `src` is inside the subset, without running it.
pub fn supports(src: &str) -> Result<(), String> {
    parse(src).map(|_| ()).map_err(|Unsupported(w)| w)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run in-process and capture stdout and the status.
    fn sh(src: &str) -> (String, i32) {
        let list = parse(src).unwrap_or_else(|Unsupported(w)| panic!("{src}: {w}"));
        let mut shell = Shell {
            vars: HashMap::new(),
            exported: HashSet::new(),
            last: 0,
        };
        let mut stdin = Input::buf(Vec::new());
        let mut sink = Sink::new(ErrMode::Null);
        let c = match shell.run_list(&list, &mut stdin, &mut sink) {
            Flow::Next(c) | Flow::Exit(c) => c,
            _ => 0,
        };
        (String::from_utf8_lossy(&sink.out).into_owned(), c)
    }

    #[test]
    fn variables_quoting_and_control_flow() {
        assert_eq!(sh("X=5; echo $X"), ("5\n".into(), 0));
        assert_eq!(sh("export FOO=bar && echo $FOO"), ("bar\n".into(), 0));
        assert_eq!(sh(r#"echo "${NOPE_AE_X:-unset}""#), ("unset\n".into(), 0));
        assert_eq!(
            sh("for i in 1 2 3; do echo $i; done"),
            ("1\n2\n3\n".into(), 0)
        );
        // A loop feeding a pipeline must feed it, not the terminal.
        assert_eq!(
            sh("for i in 1 2 3; do echo $i; done | head -1"),
            ("1\n".into(), 0)
        );
        assert_eq!(sh("false || echo recovered"), ("recovered\n".into(), 0));
        assert_eq!(sh("true && echo chained"), ("chained\n".into(), 0));
        assert_eq!(sh("false && echo no"), (String::new(), 1));
        assert_eq!(sh("! false"), (String::new(), 0));
        assert_eq!(
            sh("if [ 1 -lt 2 ]; then echo yes; else echo no; fi"),
            ("yes\n".into(), 0)
        );
        assert_eq!(
            sh("i=0; while [ $i -lt 3 ]; do echo $i; i=$(echo 3); done"),
            ("0\n".into(), 0)
        );
        assert_eq!(
            sh("echo 'a  b' \"c  d\" e\\ f"),
            ("a  b c  d e f\n".into(), 0)
        );
        assert_eq!(sh("echo $(echo one two)"), ("one two\n".into(), 0));
        assert_eq!(
            sh("X='a b'; for w in $X; do echo [$w]; done"),
            ("[a]\n[b]\n".into(), 0)
        );
        assert_eq!(
            sh("echo \"lines: $(printf 'a\\nb\\n' | wc -l)\""),
            ("lines: 2\n".into(), 0)
        );
        assert_eq!(sh("exit 3; echo no"), (String::new(), 3));
    }

    #[test]
    fn text_utilities_match_gnu() {
        let input = "b\na\nc\na\n";
        let p = |cmd: &str| sh(&format!("printf '{}' | {cmd}", input.replace('\n', "\\n")));
        assert_eq!(p("sort").0, "a\na\nb\nc\n");
        assert_eq!(p("sort -u").0, "a\nb\nc\n");
        assert_eq!(p("sort | uniq -c").0, "      2 a\n      1 b\n      1 c\n");
        assert_eq!(p("head -2").0, "b\na\n");
        assert_eq!(p("tail -n 1").0, "a\n");
        assert_eq!(p("tail -n +3").0, "c\na\n");
        assert_eq!(p("wc -l").0, "4\n");
        assert_eq!(p("grep -c a").0, "2\n");
        assert_eq!(p("grep -n c").0, "3:c\n");
        assert_eq!(p("sed -n '2,3p'").0, "a\nc\n");
        assert_eq!(p("sed 's/a/X/'").0, "b\nX\nc\nX\n");
        assert_eq!(sh("echo k=v | cut -d= -f2").0, "v\n");
        assert_eq!(sh("printf 'x y z\\n' | wc").0, "      1       3       6\n");
        assert_eq!(sh("echo abc | head -c 2").0, "ab");
        assert_eq!(sh("grep x /nonexistent/ae-x").1, 2);
        assert_eq!(p("grep zzz").1, 1);
    }

    #[test]
    fn declines_what_it_does_not_run() {
        for src in [
            "f() { echo; }",
            "case x in x) echo;; esac",
            "echo $((1+2))",
            "cat <<EOF\nx\nEOF",
            "(cd /tmp; ls)",
            "sleep 1 &",
        ] {
            assert!(supports(src).is_err(), "{src} should be declined");
        }
    }

    #[test]
    fn collation_matches_glibc_en_us() {
        // Measured against glibc 2.41 `sort` under en_US.UTF-8.
        let mut v: Vec<&str> = vec![
            "b",
            "B",
            "a",
            "A",
            "_a",
            ".a",
            "a_b",
            "ab",
            "a.b",
            "a-b",
            "A1",
            "a1",
            "a10",
            "a2",
            "Cargo.lock",
            "cargo.toml",
            "README.md",
            "build.rs",
            "_z",
            "Z",
            "z",
            "1",
            "10",
            "2",
            "a b",
            "ab_",
            "aB",
            "Ab",
        ];
        std::env::set_var("LC_COLLATE", "en_US.UTF-8");
        v.sort_by(|a, b| collate(a, b));
        std::env::remove_var("LC_COLLATE");
        assert_eq!(
            v,
            vec![
                "1",
                "10",
                "2",
                ".a",
                "_a",
                "a",
                "A",
                "a1",
                "A1",
                "a10",
                "a2",
                "a b",
                "a-b",
                "a.b",
                "a_b",
                "ab",
                "ab_",
                "aB",
                "Ab",
                "b",
                "B",
                "build.rs",
                "Cargo.lock",
                "cargo.toml",
                "README.md",
                "_z",
                "z",
                "Z"
            ]
        );
    }

    #[test]
    fn globstar_is_recursive_for_builtins_only() {
        let root = std::env::temp_dir().join(format!("ae_globstar_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        for f in ["top.rs", "a/mid.rs", "a/b/deep.rs", "a/b/deep.rsx"] {
            std::fs::write(root.join(f), "").unwrap();
        }
        let r = root.to_string_lossy().replace('\\', "/");
        let found = glob_paths(&format!("{r}/**/*.rs"));
        let names: Vec<&str> = found
            .iter()
            .map(|p| p.rsplit('/').next().unwrap())
            .collect();
        assert_eq!(names, vec!["deep.rs", "mid.rs", "top.rs"], "{found:?}");
        // Without globstar, `**` is `*`: one level only.
        let shell = glob_expand(&format!("{r}/**/*.rs"));
        assert_eq!(shell.len(), 1, "{shell:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn globs() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(!glob_match("*.rs", "main.rsx"));
        assert!(glob_match("a?c", "abc"));
        assert!(glob_match("[a-c]x", "bx"));
        assert!(!glob_match("[!a-c]x", "bx"));
    }
}
