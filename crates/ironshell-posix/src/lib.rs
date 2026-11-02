//! A POSIX-shell subset, executed in-process: what `ish -b` runs before it
//! falls back to the transpiler.
//!
//! E6 (`benches/agentic/bashcompat.mjs`) asked how much of the shell an agent
//! actually emits `ish -b` could run. The answer was 2 of 32: the transpiler
//! produced IronShell that did not parse, and everything with a loop, a
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
//! directly -- never through a shell -- after the host's [`Gate`] admits it, so
//! agent mode gates each one. A redirection that writes a file goes through the
//! same gate as `file.write`, jail included.
//!
//! What it does not parse (functions, `case`, arithmetic, here-documents,
//! subshells, background jobs) makes [`run`] return `None` before anything has
//! executed, and the caller falls back to the transpiler exactly as before.
//!
//! This crate builds without `std`. Every file, environment, process, clock,
//! locale and stream operation goes through [`Os`], and every policy decision
//! through [`Gate`] (reached by [`Os::gate`]). The host backend lives in
//! iron-shell.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::os::{
    Captured, CopyStep, ErrorKind, FileKind, Meta, Os, OsError, Refusal, Spawn, SpawnError, Stream,
    Timestamp, WriteError,
};

pub mod os;

/// The cap on the entries one walk or one glob may gather. The typed
/// builtins (`cat`, `ls`, `glob`) use the same number.
pub const WALK_MAX_ENTRIES: usize = 1_000_000;

/// The regex engine behind `grep`, `sed` and the POSIX `test` operators:
/// `regex-automata`'s meta engine with the same syntax and byte semantics as
/// the `regex` crate's `bytes` API, which builds without `std`.
mod re {
    use alloc::vec::Vec;
    use regex_automata::meta;
    use regex_automata::util::interpolate;
    use regex_automata::util::syntax;

    #[derive(Clone)]
    pub(crate) struct Regex(meta::Regex);

    impl Regex {
        /// Compiles `pattern` with the bytes API's settings: invalid UTF-8
        /// may be matched, and an empty match may fall inside a character.
        pub(crate) fn new(pattern: &str) -> Result<Regex, ()> {
            meta::Builder::new()
                .configure(meta::Config::new().utf8_empty(false))
                .syntax(syntax::Config::new().utf8(false))
                .build(pattern)
                .map(Regex)
                .map_err(|_| ())
        }

        pub(crate) fn is_match(&self, haystack: &[u8]) -> bool {
            self.0.is_match(haystack)
        }

        /// The `(start, end)` of each match, left to right.
        pub(crate) fn find_spans(&self, haystack: &[u8]) -> Vec<(usize, usize)> {
            self.0
                .find_iter(haystack)
                .map(|m| (m.start(), m.end()))
                .collect()
        }

        /// `replace` (the first match) or `replace_all` (every match), with
        /// `$0`, `${N}` and `$$` in `rep` expanded as the `regex` crate expands
        /// them.
        pub(crate) fn replace(&self, haystack: &[u8], rep: &[u8], all: bool) -> Vec<u8> {
            let mut out = Vec::with_capacity(haystack.len());
            let mut last = 0;
            for caps in self.0.captures_iter(haystack) {
                let Some(whole) = caps.get_match() else {
                    continue;
                };
                out.extend_from_slice(&haystack[last..whole.start()]);
                interpolate::bytes(
                    rep,
                    |index, dst| {
                        if let Some(span) = caps.get_group(index) {
                            dst.extend_from_slice(&haystack[span.start..span.end]);
                        }
                    },
                    |name| name.parse::<usize>().ok(),
                    &mut out,
                );
                last = whole.end();
                if !all {
                    break;
                }
            }
            out.extend_from_slice(&haystack[last..]);
            out
        }
    }
}

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
                parts.push(Part::Lit(core::mem::take(lit), false));
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
                        parts.push(Part::Lit(core::mem::take(&mut lit), true));
                    }
                    self.dollar(parts, true)?;
                    any = true;
                }
                Some('`') => {
                    if !lit.is_empty() {
                        parts.push(Part::Lit(core::mem::take(&mut lit), true));
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
struct Input<'a> {
    data: Vec<u8>,
    pos: usize,
    /// The host's standard input, when this is the shell's own; `None` for a
    /// buffer.
    os: Option<&'a dyn Os>,
    eof: bool,
}

/// The longest line `read` takes from real stdin.
const READ_LINE_MAX: usize = 16 * 1024 * 1024;

impl<'a> Input<'a> {
    fn buf(data: Vec<u8>) -> Self {
        Input {
            data,
            pos: 0,
            os: None,
            eof: true,
        }
    }
    fn real(os: &'a dyn Os) -> Self {
        Input {
            data: Vec::new(),
            pos: 0,
            os: Some(os),
            eof: false,
        }
    }
    fn fill_all(&mut self) {
        if let Some(os) = self.os {
            if !self.eof {
                os.read_stdin_to_end(&mut self.data);
                self.eof = true;
            }
        }
    }
    fn read_all(&mut self) -> Vec<u8> {
        self.fill_all();
        let out = self.rest();
        self.pos = self.data.len();
        out
    }
    /// The unread bytes, not consumed.
    fn rest(&self) -> Vec<u8> {
        self.data[self.pos..].to_vec()
    }
    /// One line without its newline, `Ok(None)` at end of input, or `Err` for
    /// a line from the terminal or a pipe longer than [`READ_LINE_MAX`]. A
    /// line was read whole however long, and one that was not UTF-8 read as
    /// the end of input; consumed lines are now dropped, not kept.
    fn read_line(&mut self) -> Result<Option<Vec<u8>>, String> {
        loop {
            if let Some(nl) = self.data[self.pos..].iter().position(|&b| b == b'\n') {
                let line = self.data[self.pos..self.pos + nl].to_vec();
                self.pos += nl + 1;
                if self.os.is_some() {
                    self.data.drain(..self.pos);
                    self.pos = 0;
                }
                return Ok(Some(line));
            }
            if let (Some(os), false) = (self.os, self.eof) {
                // `None`: end of input, or a read error (both end the input).
                match os.read_stdin_line(READ_LINE_MAX + 1) {
                    None => self.eof = true,
                    Some(chunk) if chunk.len() > READ_LINE_MAX && !chunk.ends_with(b"\n") => {
                        self.eof = true;
                        return Err(format!("a line longer than {READ_LINE_MAX} bytes"));
                    }
                    Some(chunk) => self.data.extend_from_slice(&chunk),
                }
                continue;
            }
            if self.pos < self.data.len() {
                let line = self.data[self.pos..].to_vec();
                self.pos = self.data.len();
                return Ok(Some(line));
            }
            return Ok(None);
        }
    }
    fn is_buffered(&self) -> bool {
        self.os.is_none()
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

struct Sink<'a> {
    out: Vec<u8>,
    err_mode: ErrMode,
    err: Vec<u8>,
    /// The host, for the shell's own standard error (`ErrMode::Real`).
    os: &'a dyn Os,
}

impl<'a> Sink<'a> {
    fn new(os: &'a dyn Os, err_mode: ErrMode) -> Self {
        Sink {
            out: Vec::new(),
            err_mode,
            err: Vec::new(),
            os,
        }
    }
    fn ewrite(&mut self, b: &[u8]) {
        match self.err_mode {
            ErrMode::Real => self.os.write_stderr(b),
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

struct Shell<'a> {
    vars: BTreeMap<String, String>,
    exported: BTreeSet<String>,
    last: i32,
    /// A pathname expansion over the cap, set by `expand` (which cannot
    /// report errors itself) and taken by the command that contains it, which
    /// is then refused rather than run with part of its arguments.
    glob_refused: Option<String>,
    /// The host the shell's file, directory, environment and process
    /// operations go through.
    os: &'a dyn Os,
}

const SHELL_NAME: &str = "ish";

impl<'a> Shell<'a> {
    fn var(&self, name: &str) -> Option<String> {
        match name {
            "?" => return Some(self.last.to_string()),
            "$" => return Some(self.os.process_id().to_string()),
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
        self.os.env_var(name)
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
                match glob_expand(self.os, &text) {
                    Some(matches) if !matches.is_empty() => {
                        out.extend(matches);
                        continue;
                    }
                    Some(_) => {}
                    None => {
                        self.glob_refused.get_or_insert(format!(
                            "{SHELL_NAME}: {text}: more than {WALK_MAX} matches; nothing was expanded"
                        ));
                    }
                }
            }
            out.push(text);
        }
        out
    }

    /// The refusal for a command whose expansion went over the glob cap: the
    /// message goes to stderr, and the command does not run.
    fn glob_refusal(&mut self, sink: &mut Sink) -> Option<Flow> {
        let msg = self.glob_refused.take()?;
        sink.eline(&msg);
        self.last = 1;
        Some(Flow::Next(1))
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
            mask.extend(core::iter::repeat_n(false, value.chars().count()));
            *have = true;
            return;
        }
        let starts_ws = value.starts_with(|c: char| c.is_ascii_whitespace());
        let ends_ws = value.ends_with(|c: char| c.is_ascii_whitespace());
        let pieces: Vec<&str> = value.split_ascii_whitespace().collect();
        if pieces.is_empty() {
            if starts_ws && *have {
                fields.push((core::mem::take(cur), core::mem::take(mask)));
                *have = false;
            }
            return;
        }
        for (k, p) in pieces.iter().enumerate() {
            if (k > 0 || starts_ws) && *have {
                fields.push((core::mem::take(cur), core::mem::take(mask)));
            }
            cur.push_str(p);
            // Unquoted expansion results are subject to globbing.
            mask.extend(core::iter::repeat_n(true, p.chars().count()));
            *have = true;
        }
        if ends_ws {
            fields.push((core::mem::take(cur), core::mem::take(mask)));
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
        let operand = |sh: &mut Shell<'_>, stdin: &mut Input| sh.expand(word, stdin).join(" ");
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
                self.os.write_stderr(
                    format!(
                        "{SHELL_NAME}: command substitution uses {what}, which this shell does not run\n"
                    )
                    .as_bytes(),
                );
                self.last = 2;
                return String::new();
            }
        };
        let mut sink = Sink::new(self.os, ErrMode::Real);
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
                let mut stage = Sink::new(sink.os, sink.err_mode);
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
        // A refusal is the business of the command whose expansion set it. One
        // left by an expansion no site checks (a `case` word, a `${x:-...}`
        // operand) must not refuse this, unrelated, command.
        self.glob_refused = None;
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
                    match self.os.read_file(&path) {
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
        if let Some(flow) = self.glob_refusal(sink) {
            return flow;
        }
        let mut local = Sink::new(self.os, err_mode);
        let flow = match local_in.as_mut() {
            Some(i) => self.run_cmd_inner(cmd, i, &mut local),
            None => self.run_cmd_inner(cmd, stdin, &mut local),
        };
        if let Some((path, append)) = err_file {
            if let Err(msg) = write_file(self.os, &path, &local.err, append) {
                sink.eline(&msg);
            }
        }
        if let Some((path, append)) = out_file {
            if let Err(msg) = write_file(self.os, &path, &local.out, append) {
                sink.eline(&msg);
                self.last = 1;
                return Flow::Next(1);
            }
        } else if out_to_err {
            let out = core::mem::take(&mut local.out);
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
                if let Some(flow) = self.glob_refusal(sink) {
                    return flow;
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
        if let Some(flow) = self.glob_refusal(sink) {
            return flow;
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
                let old = self.os.current_dir().ok();
                return match self.os.set_current_dir(&target) {
                    Ok(()) => {
                        if let Some(o) = old {
                            self.set_var("OLDPWD", o);
                        }
                        if let Ok(d) = self.os.current_dir() {
                            self.set_var("PWD", d);
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
                sink.out.extend_from_slice(logical_pwd(self.os).as_bytes());
                sink.out.push(b'\n');
                return Flow::Next(0);
            }
            _ => {}
        }
        // `env` runs a command with variables set, or lists the environment.
        // It is handled here, not in `utility`, because a command it runs must
        // take the same path a `NAME=VALUE cmd` prefix takes.
        if name == "env" {
            if let Some(e) = env_args(args) {
                return self.env_builtin(e, env, stdin, sink);
            }
        }
        if env.is_empty() {
            match utility(self.os, name, args, stdin, sink) {
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
                self.os.write_stderr(
                    format!("{SHELL_NAME}: read: {a}: unsupported option\n").as_bytes(),
                );
                return 2;
            } else {
                names.push(a.clone());
            }
        }
        if names.is_empty() {
            names.push("REPLY".into());
        }
        let line = match stdin.read_line() {
            Ok(line) => line,
            Err(why) => {
                self.os
                    .write_stderr(format!("{SHELL_NAME}: read: {why}\n").as_bytes());
                return 2;
            }
        };
        let Some(line) = line else {
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

    /// `env`: with no command, prints the environment (the host's, then the
    /// shell's exports, then this command's own overrides), sorted. With a
    /// command, runs it with the overrides set, through [`Shell::external`]
    /// exactly as `NAME=VALUE cmd` is run, so the gate sees the same thing.
    fn env_builtin(
        &mut self,
        e: EnvArgs,
        env: &[(String, String)],
        stdin: &mut Input,
        sink: &mut Sink,
    ) -> Flow {
        if e.command.is_empty() {
            let mut vars: BTreeMap<String, String> = BTreeMap::new();
            if !e.clear {
                vars.extend(self.os.env_vars());
                for n in &self.exported {
                    if let Some(v) = self.vars.get(n) {
                        vars.insert(n.clone(), v.clone());
                    }
                }
            }
            for n in &e.unset {
                vars.remove(n);
            }
            for (n, v) in env.iter().chain(e.assign.iter()) {
                vars.insert(n.clone(), v.clone());
            }
            for (n, v) in &vars {
                sink.out.extend_from_slice(format!("{n}={v}\n").as_bytes());
            }
            return Flow::Next(0);
        }
        // A host Spawn can set variables but not remove them, so `-i` and `-u`
        // cannot be honoured in front of a command. Refuse, rather than run the
        // command with variables the caller asked to be gone.
        if e.clear || !e.unset.is_empty() {
            sink.eline("env: -i and -u are not supported with a command on this host");
            return Flow::Next(125);
        }
        let mut over: Vec<(String, String)> = env.to_vec();
        over.extend(e.assign);
        if over.is_empty() {
            return self.dispatch(&e.command, &[], stdin, sink);
        }
        Flow::Next(self.external(&e.command, &over, stdin, sink))
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
        let gate = self.os.gate();
        if let Err(refusal) = gate.admit_exec(&argv.join(" ")) {
            sink.eline(&refusal.message);
            return refusal.code;
        }
        // The approval above describes argv only. A child that inherits an
        // `IRONSHELL_*` variable from an assignment prefix or an export runs with
        // whatever gate that variable sets (`IRONSHELL_MODE=human ish -c 'apply(..)'`
        // is ungated), so in agent mode no such variable reaches a child.
        if gate.agent_mode() {
            let gate_var = self
                .exported
                .iter()
                .map(String::as_str)
                .chain(env.iter().map(|(n, _)| n.as_str()))
                .find(|n| n.to_ascii_uppercase().starts_with("IRONSHELL_"));
            if let Some(name) = gate_var {
                let refusal = gate.refuse_gate_variable(name);
                sink.eline(&refusal.message);
                return refusal.code;
            }
        }
        // The shell's exports first, then this command's own assignments,
        // which win; the host applies them in order.
        let mut vars: Vec<(String, String)> = Vec::new();
        for n in &self.exported {
            if let Some(v) = self.vars.get(n) {
                vars.push((n.clone(), v.clone()));
            }
        }
        vars.extend(env.iter().cloned());
        let feed = stdin.is_buffered();
        // A buffered stdin is consumed only once the child has started, as
        // it always was: a program that failed to start reads none of it.
        let spec = Spawn {
            argv: argv.to_vec(),
            env: vars,
            stderr: match sink.err_mode {
                ErrMode::Real => Stream::Inherit,
                ErrMode::Null => Stream::Null,
                _ => Stream::Piped,
            },
            stdin: feed.then(|| stdin.rest()),
        };
        let out: Captured = match self.os.run_program(spec) {
            Ok(o) => o,
            Err(SpawnError::Start(e)) if e.kind == ErrorKind::NotFound => {
                sink.eline(&format!("{SHELL_NAME}: {}: command not found", argv[0]));
                return 127;
            }
            Err(SpawnError::Start(e)) => {
                sink.eline(&format!("{SHELL_NAME}: {}: {}", argv[0], io_reason(&e)));
                return 126;
            }
            Err(SpawnError::Wait(e)) => {
                if feed {
                    stdin.read_all();
                }
                sink.eline(&format!("{SHELL_NAME}: {}: {}", argv[0], io_reason(&e)));
                return 1;
            }
        };
        if feed {
            stdin.read_all();
        }
        sink.out.extend_from_slice(&out.stdout);
        if !out.stderr.is_empty() {
            sink.ewrite(&out.stderr);
        }
        out.status.unwrap_or(128)
    }
}

/// Write a redirection's output through the host's gate, which admits it,
/// journals it and writes it (see [`Gate::guarded_write`]). A refusal is
/// printed as the gate worded it; an I/O failure is printed with the path as
/// the command gave it.
fn write_file(os: &dyn Os, path: &str, data: &[u8], append: bool) -> Result<(), String> {
    match os.gate().guarded_write(os, path, data, append) {
        Ok(()) => Ok(()),
        Err(WriteError::Refused(refusal)) => Err(refusal.message),
        Err(WriteError::Io(e)) => Err(format!("{SHELL_NAME}: {path}: {}", io_reason(&e))),
    }
}

/// The message GNU tools print for an I/O error.
fn io_reason(e: &OsError) -> String {
    use ErrorKind::*;
    match e.kind {
        NotFound => "No such file or directory".into(),
        PermissionDenied => "Permission denied".into(),
        IsADirectory => "Is a directory".into(),
        NotADirectory => "Not a directory".into(),
        AlreadyExists => "File exists".into(),
        Other => {
            if e.errno == Some(21) {
                return "Is a directory".into();
            }
            let s = &e.message;
            s.split(" (os error").next().unwrap_or(s).to_string()
        }
    }
}

/// `Path::exists` and `Path::is_dir`, through the host: both follow links,
/// and both read a failed lookup as "no".
fn exists(os: &dyn Os, path: &str) -> bool {
    os.metadata(path).is_ok()
}

fn is_dir(os: &dyn Os, path: &str) -> bool {
    os.metadata(path).is_ok_and(|m| m.kind == FileKind::Dir)
}

fn logical_pwd(os: &dyn Os) -> String {
    let phys = os.current_dir().ok();
    if let (Some(pwd), Some(phys)) = (os.env_var("PWD"), phys.as_ref()) {
        if os.canonicalize(&pwd).ok().as_deref() == os.canonicalize(phys).ok().as_deref() {
            return pwd;
        }
    }
    phys.unwrap_or_default()
}

// ════════════════════════════════════════════════════════════════════════
// Collation and globbing
// ════════════════════════════════════════════════════════════════════════

/// Whether sorting follows a natural-language locale rather than bytes. `ls`
/// and `sort` both use `strcoll`, so under `en_US.UTF-8` they do not sort by
/// byte, and output that is meant to match theirs cannot either.
fn locale_collates(os: &dyn Os) -> bool {
    let loc = ["LC_ALL", "LC_COLLATE", "LANG"]
        .iter()
        .filter_map(|k| os.env_var(k))
        .find(|v| !v.is_empty())
        .unwrap_or_default();
    if loc.is_empty() || loc == "C" || loc == "POSIX" || loc.starts_with("C.") {
        return false;
    }
    // Whether the host can load the locale: when it cannot (it was never
    // generated), `ls` and `sort` fall back to bytes, so output meant to match
    // theirs must too.
    os.locale_available(&loc)
}

/// glibc's ISO 14651 ordering for ASCII, as measured against glibc 2.41's
/// `en_US.UTF-8` (`printf … | sort`): compare letters and digits ignoring case
/// and punctuation; then lower case before upper, position by position; then
/// the full strings with punctuation ranked below letters and digits. Beyond
/// ASCII this falls back to bytes, which is where it stops being exact.
fn collate(os: &dyn Os, a: &str, b: &str) -> core::cmp::Ordering {
    collate_in(locale_collates(os), a, b)
}

fn collate_in(natural: bool, a: &str, b: &str) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    if !natural {
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
/// `None` when some stage gathers more than `WALK_MAX_ENTRIES` names: the
/// caller refuses the expansion rather than run with part of it.
fn glob_expand(os: &dyn Os, pattern: &str) -> Option<Vec<String>> {
    let (v, over) = expand_pattern(os, pattern, false, |a, b| collate(os, a, b), WALK_MAX);
    (!over).then_some(v)
}

/// Pathname expansion for the typed builtins (`glob`, `cat`, `ls`): `**`
/// matches any number of directories, and results sort by bytes, so the
/// answer does not depend on the locale the shell happens to run under.
pub fn glob_paths(os: &dyn Os, pattern: &str) -> Vec<String> {
    glob_paths_capped(os, pattern, usize::MAX).0
}

/// `glob_paths`, but it stops gathering once more than `max` distinct names
/// have turned up, and says so: `(paths, true)` then holds only what was
/// gathered before the stop, and the caller must refuse. Below the cap the
/// paths are the same, sorted and without duplicates, as `glob_paths` gives.
pub fn glob_paths_capped(os: &dyn Os, pattern: &str, max: usize) -> (Vec<String>, bool) {
    let (mut v, over) = expand_pattern(
        os,
        pattern,
        true,
        |a, b| a.as_bytes().cmp(b.as_bytes()),
        max,
    );
    if over {
        return (Vec::new(), true);
    }
    v.sort();
    v.dedup();
    (v, false)
}

/// The cap the POSIX subset applies to a glob, the same as the typed
/// builtins' walks.
const WALK_MAX: usize = WALK_MAX_ENTRIES;

/// The names one expansion stage has gathered. A stage that gathers more
/// than `max` distinct names is over, and stops as soon as it knows that.
/// Under `dedup` (for `**`, whose stages can repeat a name), duplicates are
/// removed before the count, so only distinct names count against the cap;
/// the list is kept small by settling it when it has doubled since the last
/// settle, not on every push.
struct Gather {
    out: Vec<String>,
    max: usize,
    dedup: bool,
    settle_at: usize,
    over: bool,
}

impl Gather {
    fn new(max: usize, dedup: bool) -> Self {
        Gather {
            out: Vec::new(),
            max,
            dedup,
            settle_at: max,
            over: false,
        }
    }

    fn push(&mut self, name: String) {
        if self.over {
            return;
        }
        self.out.push(name);
        if self.out.len() > self.settle_at {
            self.settle();
        }
    }

    fn settle(&mut self) {
        if self.dedup {
            self.out.sort_unstable();
            self.out.dedup();
        }
        if self.out.len() > self.max {
            self.over = true;
            self.out.clear();
        }
        self.settle_at = self.out.len().max(self.max).saturating_mul(2);
    }

    /// The stage's names, or `None` when it is over.
    fn finish(mut self) -> Option<Vec<String>> {
        self.settle();
        (!self.over).then_some(self.out)
    }
}

/// Whether a string contains glob metacharacters.
pub fn has_glob_meta(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

/// The directories beneath `dir`, depth first, not following symbolic links
/// and skipping hidden names, as bash's `globstar` does. Stops once `out` is
/// over the cap.
fn descendant_dirs(os: &dyn Os, dir: &str, out: &mut Gather) {
    let Ok(rd) = os.read_dir(if dir.is_empty() { "." } else { dir }) else {
        return;
    };
    let mut names: Vec<String> = rd
        .into_iter()
        .filter(|e| e.kind == Some(FileKind::Dir))
        .map(|e| e.name)
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    for n in names {
        if out.over {
            return;
        }
        let p = if dir.is_empty() {
            n
        } else if dir.ends_with('/') {
            format!("{dir}{n}")
        } else {
            format!("{dir}/{n}")
        };
        out.push(p.clone());
        descendant_dirs(os, &p, out);
    }
}

/// Expands `pattern` one component at a time. The second value is true when
/// some stage gathered more than `max` names; the names are then not
/// returned, and the caller must refuse.
fn expand_pattern(
    os: &dyn Os,
    pattern: &str,
    globstar: bool,
    order: impl Fn(&String, &String) -> core::cmp::Ordering,
    max: usize,
) -> (Vec<String>, bool) {
    let absolute = pattern.starts_with('/');
    let comps: Vec<&str> = pattern.split('/').filter(|c| !c.is_empty()).collect();
    let mut bases: Vec<String> = vec![if absolute { "/".into() } else { String::new() }];
    for (i, comp) in comps.iter().enumerate() {
        let last = i + 1 == comps.len();
        let mut next = Gather::new(max, globstar);
        if globstar && *comp == "**" {
            // Zero or more directories: each base, and everything below it.
            for base in &bases {
                next.push(base.clone());
                descendant_dirs(os, base, &mut next);
                if next.over {
                    return (Vec::new(), true);
                }
            }
            if last {
                // A trailing `**` also names the files in those directories.
                let dirs = next.out.clone();
                for d in dirs {
                    let dir = if d.is_empty() {
                        ".".to_string()
                    } else {
                        d.clone()
                    };
                    if let Ok(rd) = os.read_dir(&dir) {
                        for e in rd {
                            let n = e.name;
                            if !n.starts_with('.') && e.kind.is_some_and(|k| k != FileKind::Dir) {
                                next.push(if d.is_empty() { n } else { format!("{d}/{n}") });
                            }
                        }
                    }
                    if next.over {
                        return (Vec::new(), true);
                    }
                }
                next.out.retain(|p| !p.is_empty());
            }
            bases = match next.finish() {
                Some(v) => v,
                None => return (Vec::new(), true),
            };
            continue;
        }
        let has_meta = has_glob_meta(comp);
        for base in &bases {
            if next.over {
                break;
            }
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
                if (last && exists(os, &p)) || (!last && is_dir(os, &p)) {
                    next.push(p);
                }
                continue;
            }
            let dir = if base.is_empty() { "." } else { base.as_str() };
            let Ok(rd) = os.read_dir(dir) else {
                continue;
            };
            let mut names: Vec<String> = rd
                .into_iter()
                .map(|e| e.name)
                .filter(|n| !n.starts_with('.') || comp.starts_with('.'))
                .filter(|n| glob_match(comp, n))
                .collect();
            names.sort_by(&order);
            for n in names {
                let p = join(&n);
                if last || is_dir(os, &p) {
                    next.push(p);
                }
            }
        }
        bases = match next.finish() {
            Some(v) => v,
            None => return (Vec::new(), true),
        };
    }
    if pattern.ends_with('/') {
        bases = bases.into_iter().map(|b| format!("{b}/")).collect();
    }
    (bases, false)
}

// ════════════════════════════════════════════════════════════════════════
// Utilities
// ════════════════════════════════════════════════════════════════════════

/// Read a utility's inputs: each operand in turn (`-` is stdin), or stdin
/// when there are none. Missing files are reported the GNU way and skipped.
fn read_inputs(
    os: &dyn Os,
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
        match os.read_file(f) {
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
fn short_flags(args: &[String], allowed: &str) -> Option<(BTreeSet<char>, Vec<String>)> {
    let mut flags = BTreeSet::new();
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

fn utility(os: &dyn Os, name: &str, args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    match name {
        "echo" => echo(args, sink),
        "printf" => printf(args, sink),
        "test" => Util::Done(test(os, args, sink)),
        "[" => {
            if args.last().map(String::as_str) != Some("]") {
                sink.eline(&format!("{SHELL_NAME}: [: missing `]'"));
                return Util::Done(2);
            }
            Util::Done(test(os, &args[..args.len() - 1], sink))
        }
        "cat" => cat(os, args, stdin, sink),
        "head" => head_tail(os, true, args, stdin, sink),
        "tail" => head_tail(os, false, args, stdin, sink),
        "wc" => wc(os, args, stdin, sink),
        "grep" => grep(os, args, stdin, sink),
        "sort" => sort(os, args, stdin, sink),
        "uniq" => uniq(os, args, stdin, sink),
        "cut" => cut(os, args, stdin, sink),
        "sed" => sed(os, args, stdin, sink),
        "ls" => ls(os, args, sink),
        "find" => find(os, args, sink),
        "date" => date(os, args, sink),
        "rm" => rm(os, args, sink),
        "cp" => cp(os, args, sink),
        "mv" => mv(os, args, sink),
        "mkdir" => mkdir(os, args, sink),
        "tr" => tr(args, stdin, sink),
        "tee" => tee(os, args, stdin, sink),
        "touch" => touch(os, args, sink),
        "which" => which(os, args, sink),
        "expr" => expr(args, sink),
        "yes" => yes(args, sink),
        "readlink" => readlink(os, args, sink),
        "stat" => stat(os, args, sink),
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
            // The whole output is gathered before it is written, so `seq 1
            // 9999999999` grew without limit; and `i += step` overflowed
            // near i64::MAX.
            let count = if (step > 0 && first > last) || (step < 0 && first < last) {
                0
            } else {
                (last as i128 - first as i128) / step as i128 + 1
            };
            if count > SEQ_MAX as i128 {
                sink.eline(&format!(
                    "seq: {count} numbers is more than {SEQ_MAX}; nothing was printed"
                ));
                return Util::Done(1);
            }
            let mut i = first;
            for _ in 0..count {
                sink.out.extend_from_slice(format!("{i}\n").as_bytes());
                match i.checked_add(step) {
                    Some(n) => i = n,
                    None => break,
                }
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

/// The most numbers `seq` prints: its output is gathered whole before it is
/// written.
const SEQ_MAX: usize = 10_000_000;
/// The widest `printf` field, as awk's printf allows.
const PRINTF_WIDTH_MAX: usize = 4096;

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
            // A width is padding allocated up front: `%999999999999s` asked
            // for a terabyte, and a width too long to parse was read as 0.
            let digits = spec
                .trim_start_matches(['-', '0'])
                .split('.')
                .next()
                .unwrap_or("");
            let width: usize = if digits.is_empty() {
                0
            } else {
                match digits.parse::<usize>() {
                    Ok(w) if w <= PRINTF_WIDTH_MAX => w,
                    _ => {
                        sink.eline(&format!(
                            "printf: field width {digits} is more than {PRINTF_WIDTH_MAX}"
                        ));
                        return Util::Done(1);
                    }
                }
            };
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

fn test(os: &dyn Os, args: &[String], sink: &mut Sink) -> i32 {
    fn unary(os: &dyn Os, op: &str, a: &str) -> Option<bool> {
        let md = os.metadata(a);
        Some(match op {
            "-e" => md.is_ok(),
            "-f" => md.map(|m| m.kind == FileKind::File).unwrap_or(false),
            "-d" => md.map(|m| m.kind == FileKind::Dir).unwrap_or(false),
            "-s" => md.map(|m| m.len > 0).unwrap_or(false),
            "-r" | "-w" | "-x" => md.is_ok(),
            "-L" | "-h" => os
                .symlink_metadata(a)
                .map(|m| m.kind == FileKind::Symlink)
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
    fn eval(os: &dyn Os, a: &[String], sink: &mut Sink) -> Result<bool, i32> {
        // -a / -o, lowest precedence first.
        if let Some(p) = a.iter().rposition(|x| x == "-o") {
            if p > 0 && p + 1 < a.len() {
                return Ok(eval(os, &a[..p], sink)? || eval(os, &a[p + 1..], sink)?);
            }
        }
        if let Some(p) = a.iter().rposition(|x| x == "-a") {
            if p > 0 && p + 1 < a.len() {
                return Ok(eval(os, &a[..p], sink)? && eval(os, &a[p + 1..], sink)?);
            }
        }
        match a {
            [] => Ok(false),
            [x] => Ok(!x.is_empty()),
            [bang, rest @ ..] if bang == "!" => Ok(!eval(os, rest, sink)?),
            [op, x] => match unary(os, op, x) {
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
    match eval(os, args, sink) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(c) => c,
    }
}

fn cat(os: &dyn Os, args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let Some((flags, files)) = short_flags(args, "") else {
        return Util::Decline;
    };
    let _ = flags;
    let mut status = 0;
    for (name, data) in read_inputs_dirs(os, "cat", &files, stdin, sink, &mut status) {
        let _ = name;
        sink.out.extend_from_slice(&data);
    }
    Util::Done(status)
}

/// Like [`read_inputs`], but a directory operand is the GNU "Is a directory"
/// error rather than an unreadable file.
fn read_inputs_dirs(
    os: &dyn Os,
    tool: &str,
    files: &[String],
    stdin: &mut Input,
    sink: &mut Sink,
    status: &mut i32,
) -> Vec<(String, Vec<u8>)> {
    let mut ok = Vec::new();
    for f in files {
        if is_dir(os, f) {
            sink.eline(&format!("{tool}: {f}: Is a directory"));
            *status = 1;
        } else {
            ok.push(f.clone());
        }
    }
    if !files.is_empty() && ok.is_empty() {
        return Vec::new();
    }
    read_inputs(os, tool, &ok, stdin, sink, status)
}

/// `head` and `tail`: `-n N`, `-nN`, `-N`, `-c N`, and `tail -n +N`.
fn head_tail(os: &dyn Os, head: bool, args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
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
    let inputs = read_inputs_dirs(os, tool, &files, stdin, sink, &mut status);
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

fn wc(os: &dyn Os, args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
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
    let inputs = read_inputs_dirs(os, "wc", &files, stdin, sink, &mut status);
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
            .filter_map(|f| os.metadata(f).ok())
            .filter(|m| m.kind == FileKind::File)
            .map(|m| m.len)
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
pub fn bre_to_regex(p: &str) -> Option<String> {
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
                Some(o) => out.push_str(&regex_syntax::escape(&o.to_string())),
                None => out.push_str("\\\\"),
            },
            '+' | '?' | '|' | '(' | ')' | '{' | '}' => {
                out.push_str(&regex_syntax::escape(&c.to_string()))
            }
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

fn grep(os: &dyn Os, args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let mut flags = BTreeSet::new();
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
            Some(regex_syntax::escape(p))
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
    let Ok(re) = re::Regex::new(&src) else {
        return Util::Decline;
    };

    // Collect (display name, path or stdin) in GNU's order.
    let mut status_err = false;
    let mut targets: Vec<(String, Option<String>)> = Vec::new();
    if ops.is_empty() {
        if recursive {
            walk_files(os, ".", "", &mut targets, true);
        } else {
            targets.push(("(standard input)".into(), None));
        }
    } else {
        for op in &ops {
            if op == "-" {
                targets.push(("(standard input)".into(), None));
            } else if is_dir(os, op) {
                if recursive {
                    walk_files(os, op, op, &mut targets, false);
                } else if !f('s') {
                    sink.eline(&format!("grep: {op}: Is a directory"));
                }
            } else if exists(os, op) {
                targets.push((op.clone(), Some(op.clone())));
            } else {
                if !f('s') {
                    sink.eline(&format!("grep: {op}: No such file or directory"));
                }
                status_err = true;
            }
        }
    }
    if targets.len() > WALK_MAX_ENTRIES {
        sink.eline(&format!(
            "grep: more than {} files under the search path; nothing was searched",
            WALK_MAX_ENTRIES
        ));
        return Util::Done(2);
    }
    let prefix = (targets.len() > 1 || recursive || ops.len() > 1) && !f('h') || f('H');
    let mut any = false;
    for (name, path) in &targets {
        let data = match path {
            Some(p) => match os.read_file(p) {
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
                for (start, end) in re.find_spans(line) {
                    if start == end {
                        continue;
                    }
                    out.extend_from_slice(&lead);
                    out.extend_from_slice(&line[start..end]);
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
fn walk_files(
    os: &dyn Os,
    dir: &str,
    shown: &str,
    out: &mut Vec<(String, Option<String>)>,
    strip_dot: bool,
) {
    let Ok(rd) = os.read_dir(dir) else {
        return;
    };
    for e in rd {
        // One past the cap is enough for the caller to refuse the search.
        if out.len() > WALK_MAX_ENTRIES {
            return;
        }
        let name = e.name;
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
        let Some(kind) = e.kind else { continue };
        if kind == FileKind::Symlink {
            continue;
        }
        if kind == FileKind::Dir {
            walk_files(os, &path, &display, out, false);
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

fn sort(os: &dyn Os, args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let Some((flags, files)) = short_flags(args, "rnuf") else {
        return Util::Decline;
    };
    let mut status = 0;
    let mut lines: Vec<String> = Vec::new();
    for (_, d) in read_inputs(os, "sort", &files, stdin, sink, &mut status) {
        lines.extend(
            lines_of(&d)
                .iter()
                .map(|l| String::from_utf8_lossy(l).into_owned()),
        );
    }
    let numeric = flags.contains(&'n');
    let fold = flags.contains(&'f');
    let key_cmp = |a: &String, b: &String| -> core::cmp::Ordering {
        if numeric {
            numeric_prefix(a)
                .partial_cmp(&numeric_prefix(b))
                .unwrap_or(core::cmp::Ordering::Equal)
        } else if fold {
            collate(os, &a.to_uppercase(), &b.to_uppercase())
        } else {
            collate(os, a, b)
        }
    };
    lines.sort_by(|a, b| key_cmp(a, b).then_with(|| collate(os, a, b)));
    if flags.contains(&'u') {
        lines.dedup_by(|a, b| key_cmp(a, b) == core::cmp::Ordering::Equal);
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

fn uniq(os: &dyn Os, args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let Some((flags, files)) = short_flags(args, "cdui") else {
        return Util::Decline;
    };
    if files.len() > 1 {
        return Util::Decline;
    }
    let mut status = 0;
    let data = read_inputs(os, "uniq", &files, stdin, sink, &mut status)
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

fn cut(os: &dyn Os, args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
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
    for (_, data) in read_inputs(os, "cut", &files, stdin, sink, &mut status) {
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
    Re(re::Regex),
}

enum SedCmd {
    Print,
    Delete,
    Quit,
    Subst(re::Regex, Vec<u8>, bool, bool),
}

struct SedLine {
    a1: Option<Addr>,
    a2: Option<Addr>,
    cmd: SedCmd,
    active: bool,
}

/// `sed` for the everyday scripts: `Np`, `a,bp`, `$p`, `/re/p`, `Nd`, `q`,
/// and `s/re/rep/[gp]`, with `-n`, `-e` and `-E`. Anything else declines.
fn sed(os: &dyn Os, args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
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
    let to_re = |p: &str| -> Option<re::Regex> {
        let r = if ere {
            ere_to_regex(p)?
        } else {
            bre_to_regex(p)?
        };
        re::Regex::new(&r).ok()
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
    for (_, d) in read_inputs(os, "sed", &files, stdin, sink, &mut status) {
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
                    let replaced = re.replace(&pattern, rep.as_slice(), *global);
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

fn ls(os: &dyn Os, args: &[String], sink: &mut Sink) -> Util {
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
            os.symlink_metadata(op)
        } else {
            os.metadata(op)
        };
        match md {
            Ok(m) if m.kind == FileKind::Dir && !dirs_as_files => dirs.push(op.clone()),
            Ok(_) => files.push(op.clone()),
            Err(e) => {
                sink.eline(&format!("ls: cannot access '{op}': {}", io_reason(&e)));
                status = 2;
            }
        }
    }
    files.sort_by(|a, b| collate(os, a, b));
    dirs.sort_by(|a, b| collate(os, a, b));
    let headers = files.len() + dirs.len() > 1 || status != 0;
    let mut first = true;
    if !files.is_empty() {
        let entries: Vec<(String, String)> = files.iter().map(|f| (f.clone(), f.clone())).collect();
        emit_ls(os, &entries, long, false, sink);
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
        let Ok(rd) = os.read_dir(d) else {
            sink.eline(&format!(
                "ls: cannot open directory '{d}': Permission denied"
            ));
            status = 2;
            continue;
        };
        let mut names: Vec<String> = rd
            .into_iter()
            .map(|e| e.name)
            .filter(|n| all || almost || !n.starts_with('.'))
            .collect();
        if all {
            names.push(".".into());
            names.push("..".into());
        }
        names.sort_by(|a, b| collate(os, a, b));
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
        emit_ls(os, &entries, long, true, sink);
    }
    Util::Done(status)
}

/// `(shown name, path)` pairs, in order.
fn emit_ls(os: &dyn Os, entries: &[(String, String)], long: bool, total: bool, sink: &mut Sink) {
    if !long {
        for (n, _) in entries {
            sink.out.extend_from_slice(n.as_bytes());
            sink.out.push(b'\n');
        }
        return;
    }
    if os.has_unix_metadata() {
        let now = os.now();
        let mut rows = Vec::new();
        let mut blocks: u64 = 0;
        for (name, path) in entries {
            let Ok(m) = os.symlink_metadata(path) else {
                continue;
            };
            blocks += m.blocks;
            let mode = m.mode;
            let kind = match m.kind {
                FileKind::Dir => 'd',
                FileKind::Symlink => 'l',
                FileKind::BlockDevice => 'b',
                FileKind::CharDevice => 'c',
                FileKind::Fifo => 'p',
                FileKind::Socket => 's',
                FileKind::File | FileKind::Other => '-',
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
            // A time the host cannot render reads as now, as it always did.
            let mtime = Timestamp {
                secs: m.mtime,
                nanos: 0,
            };
            let mtime = if os.format_local(mtime, "%b").is_some() {
                mtime
            } else {
                now
            };
            // The age in whole seconds, truncated toward zero, and the recency
            // test on it.
            let age_ns = (now.secs as i128 - mtime.secs as i128) * 1_000_000_000
                + now.nanos as i128
                - mtime.nanos as i128;
            let age_secs = age_ns / 1_000_000_000;
            let recent = age_secs >= 0 && age_secs / 86_400 < 182;
            let date = os
                .format_local(mtime, if recent { "%b %e %H:%M" } else { "%b %e  %Y" })
                .unwrap_or_default();
            let mut shown = name.clone();
            if m.kind == FileKind::Symlink {
                if let Ok(t) = os.read_link(path) {
                    shown = format!("{name} -> {t}");
                }
            }
            rows.push((
                perms,
                m.nlink.to_string(),
                user_name(os, m.uid),
                group_name(os, m.gid),
                m.len.to_string(),
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
}

fn user_name(os: &dyn Os, uid: u32) -> String {
    os.user_name(uid).unwrap_or_else(|| uid.to_string())
}

fn group_name(os: &dyn Os, gid: u32) -> String {
    os.group_name(gid).unwrap_or_else(|| gid.to_string())
}

/// `find` with `-name`, `-iname`, `-path`, `-type`, `-maxdepth`, `-mindepth`,
/// `!`/`-not` and `-print`, visiting in directory order as GNU find does.
fn find(os: &dyn Os, args: &[String], sink: &mut Sink) -> Util {
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
    let matches = |path: &str, kind: FileKind| {
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
                Test::Type('d') => kind == FileKind::Dir,
                Test::Type('l') => kind == FileKind::Symlink,
                Test::Type(_) => kind == FileKind::File,
            };
            hit != *neg
        })
    };
    let mut status = 0;
    // Matches printed so far. One past the cap marks the walk as stopped.
    let mut count = 0usize;
    fn visit(
        path: &str,
        depth: usize,
        maxdepth: usize,
        mindepth: usize,
        matches: &dyn Fn(&str, FileKind) -> bool,
        os: &dyn Os,
        sink: &mut Sink,
        status: &mut i32,
        count: &mut usize,
    ) {
        if *count > WALK_MAX_ENTRIES {
            return;
        }
        let md = match os.symlink_metadata(path) {
            Ok(m) => m,
            Err(e) => {
                sink.eline(&format!("find: '{path}': {}", io_reason(&e)));
                *status = 1;
                return;
            }
        };
        let kind = md.kind;
        if depth >= mindepth && matches(path, kind) {
            if *count >= WALK_MAX_ENTRIES {
                *count += 1;
                sink.eline(&format!(
                    "find: stopped after {} entries; the listing is partial",
                    WALK_MAX_ENTRIES
                ));
                *status = 1;
                return;
            }
            *count += 1;
            sink.out.extend_from_slice(path.as_bytes());
            sink.out.push(b'\n');
        }
        if kind == FileKind::Dir && depth < maxdepth {
            let rd = match os.read_dir(path) {
                Ok(r) => r,
                Err(e) => {
                    sink.eline(&format!("find: '{path}': {}", io_reason(&e)));
                    *status = 1;
                    return;
                }
            };
            for e in rd {
                let name = e.name;
                let child = if path.ends_with('/') {
                    format!("{path}{name}")
                } else {
                    format!("{path}/{name}")
                };
                visit(
                    &child,
                    depth + 1,
                    maxdepth,
                    mindepth,
                    matches,
                    os,
                    sink,
                    status,
                    count,
                );
            }
        }
    }
    for s in &starts {
        visit(
            s,
            0,
            maxdepth,
            mindepth,
            &matches,
            os,
            sink,
            &mut status,
            &mut count,
        );
    }
    Util::Done(status)
}

// ════════════════════════════════════════════════════════════════════════
// File changes: rm, cp, mv, mkdir
// ════════════════════════════════════════════════════════════════════════
//
// Each one reads what it needs, then asks the Gate once per operand to make
// the change: a whole tree is one decision. A flag it does not support
// declines to the external program, which the host's guard handles as it
// always has.

/// The flags in `args`, among `allowed`, and the operands. `None` when a flag
/// is not in `allowed`.
fn split_flags<'a>(args: &'a [String], allowed: &str) -> Option<(String, Vec<&'a str>)> {
    let mut flags = String::new();
    let mut operands = Vec::new();
    let mut options = true;
    for a in args {
        if options && a == "--" {
            options = false;
        } else if options && a.len() > 1 && a.starts_with('-') {
            for c in a[1..].chars() {
                if !allowed.contains(c) {
                    return None;
                }
                flags.push(c);
            }
        } else {
            operands.push(a.as_str());
        }
    }
    Some((flags, operands))
}

/// `name` in `dir`, joined as the shell joins them.
fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// The last component of `path`, ignoring trailing slashes.
fn base_name(path: &str) -> &str {
    let t = path.trim_end_matches('/');
    t.rsplit('/').next().unwrap_or(t)
}

/// The directory part of `path`, as `dirname` gives it. Stops at `/` and `.`:
/// each is its own parent.
fn parent_of(path: &str) -> String {
    let t = path.trim_end_matches('/');
    if t.is_empty() {
        return if path.is_empty() {
            ".".into()
        } else {
            "/".into()
        };
    }
    match t.rfind('/') {
        None => ".".into(),
        Some(i) => match t[..i].trim_end_matches('/') {
            "" => "/".into(),
            p => p.into(),
        },
    }
}

/// Whether two names are one file: the same text, or the same place once
/// both resolve.
fn same_path(os: &dyn Os, a: &str, b: &str) -> bool {
    a == b
        || matches!(
            (os.canonicalize(a), os.canonicalize(b)),
            (Ok(x), Ok(y)) if x == y
        )
}

/// Removes `path`, and with `recursive` everything under it. A directory
/// without `recursive` is refused, as `rm` refuses it. A [`Gate`] runs this
/// once it has admitted the operand; nothing else should call it.
pub fn remove_tree(os: &dyn Os, path: &str, recursive: bool) -> Result<(), OsError> {
    let meta = os.symlink_metadata(path)?;
    if meta.kind != FileKind::Dir {
        return os.remove(path, false);
    }
    if !recursive {
        return Err(OsError {
            kind: ErrorKind::IsADirectory,
            errno: None,
            message: String::from("Is a directory"),
        });
    }
    for e in os.read_dir(path)? {
        remove_tree(os, &join(path, &e.name), true)?;
    }
    os.remove(path, true)
}

/// The steps that copy `from` to `to`, reading the source and writing
/// nothing. A directory needs `recursive`, and its whole tree is planned.
/// Links are followed.
pub fn copy_plan(
    os: &dyn Os,
    from: &str,
    to: &str,
    recursive: bool,
) -> Result<Vec<CopyStep>, OsError> {
    let mut plan = Vec::new();
    plan_copy(os, from, to, recursive, &mut plan)?;
    Ok(plan)
}

fn plan_copy(
    os: &dyn Os,
    from: &str,
    to: &str,
    recursive: bool,
    plan: &mut Vec<CopyStep>,
) -> Result<(), OsError> {
    if plan.len() >= WALK_MAX_ENTRIES {
        return Err(OsError {
            kind: ErrorKind::Other,
            errno: None,
            message: String::from("too many entries to copy"),
        });
    }
    if os.metadata(from)?.kind != FileKind::Dir {
        plan.push(CopyStep {
            from: from.into(),
            to: to.into(),
            dir: false,
        });
        return Ok(());
    }
    if !recursive {
        return Err(OsError {
            kind: ErrorKind::IsADirectory,
            errno: None,
            message: String::from("Is a directory"),
        });
    }
    plan.push(CopyStep {
        from: from.into(),
        to: to.into(),
        dir: true,
    });
    for e in os.read_dir(from)? {
        plan_copy(
            os,
            &join(from, &e.name),
            &join(to, &e.name),
            recursive,
            plan,
        )?;
    }
    Ok(())
}

/// Carries out a plan from [`copy_plan`]. A directory that is there already
/// is merged into, as `cp -r` does. A [`Gate`] runs this once it has admitted
/// the copy; nothing else should call it.
pub fn copy_steps(os: &dyn Os, plan: &[CopyStep]) -> Result<(), OsError> {
    for s in plan {
        if s.dir {
            if !is_dir(os, &s.to) {
                os.create_dir(&s.to)?;
            }
        } else {
            let data = os.read_file(&s.from)?;
            os.write_file(&s.to, &data, false)?;
        }
    }
    Ok(())
}

/// A usage error, which GNU follows with a pointer to `--help`.
fn usage(sink: &mut Sink, name: &str, msg: &str) -> Util {
    sink.eline(msg);
    sink.eline(&format!("Try '{name} --help' for more information."));
    Util::Done(1)
}

/// A gate's refusal is the answer to the command: its text, and its code as
/// the status.
fn refused(sink: &mut Sink, r: &Refusal) -> Util {
    sink.eline(r.message.trim_end_matches('\n'));
    Util::Done(r.code)
}

fn rm(os: &dyn Os, args: &[String], sink: &mut Sink) -> Util {
    let Some((flags, ops)) = split_flags(args, "rRf") else {
        return Util::Decline;
    };
    let recursive = flags.contains(['r', 'R']);
    let force = flags.contains('f');
    if ops.is_empty() {
        if force {
            return Util::Done(0);
        }
        return usage(sink, "rm", "rm: missing operand");
    }
    let mut status = 0;
    for p in ops {
        let meta = match os.symlink_metadata(p) {
            Ok(m) => m,
            // `-f` excuses a name that is not there, and nothing else.
            Err(e) if force && e.kind == ErrorKind::NotFound => continue,
            Err(e) => {
                sink.eline(&format!("rm: cannot remove '{p}': {}", io_reason(&e)));
                status = 1;
                continue;
            }
        };
        if meta.kind == FileKind::Dir && !recursive {
            sink.eline(&format!("rm: cannot remove '{p}': Is a directory"));
            status = 1;
            continue;
        }
        match os.gate().guarded_remove(os, p, recursive) {
            Ok(()) => {}
            Err(WriteError::Refused(r)) => return refused(sink, &r),
            Err(WriteError::Io(e)) => {
                sink.eline(&format!("rm: cannot remove '{p}': {}", io_reason(&e)));
                status = 1;
            }
        }
    }
    Util::Done(status)
}

fn cp(os: &dyn Os, args: &[String], sink: &mut Sink) -> Util {
    let Some((flags, ops)) = split_flags(args, "rR") else {
        return Util::Decline;
    };
    let recursive = flags.contains(['r', 'R']);
    match ops.len() {
        0 => return usage(sink, "cp", "cp: missing file operand"),
        1 => {
            let msg = format!("cp: missing destination file operand after '{}'", ops[0]);
            return usage(sink, "cp", &msg);
        }
        _ => {}
    }
    let (sources, dest) = ops.split_at(ops.len() - 1);
    let dest = dest[0];
    let into_dir = is_dir(os, dest);
    if sources.len() > 1 && !into_dir {
        sink.eline(&format!("cp: target '{dest}' is not a directory"));
        return Util::Done(1);
    }
    let mut status = 0;
    for src in sources {
        let target = if into_dir {
            join(dest, base_name(src))
        } else {
            dest.into()
        };
        let meta = match os.metadata(src) {
            Ok(m) => m,
            Err(e) => {
                sink.eline(&format!("cp: cannot stat '{src}': {}", io_reason(&e)));
                status = 1;
                continue;
            }
        };
        if meta.kind == FileKind::Dir && !recursive {
            sink.eline(&format!("cp: -r not specified; omitting directory '{src}'"));
            status = 1;
            continue;
        }
        if same_path(os, src, &target) {
            sink.eline(&format!("cp: '{src}' and '{target}' are the same file"));
            status = 1;
            continue;
        }
        if target.strip_prefix(src).is_some_and(|r| r.starts_with('/')) {
            let msg = format!("cp: cannot copy a directory, '{src}', into itself, '{target}'");
            sink.eline(&msg);
            status = 1;
            continue;
        }
        let plan = match copy_plan(os, src, &target, recursive) {
            Ok(plan) => plan,
            Err(e) => {
                let msg = format!("cp: cannot open '{src}' for reading: {}", io_reason(&e));
                sink.eline(&msg);
                status = 1;
                continue;
            }
        };
        match os.gate().guarded_copy(os, &plan) {
            Ok(()) => {}
            Err(WriteError::Refused(r)) => return refused(sink, &r),
            Err(WriteError::Io(e)) => {
                let what = if meta.kind == FileKind::Dir {
                    "directory"
                } else {
                    "regular file"
                };
                sink.eline(&format!(
                    "cp: cannot create {what} '{target}': {}",
                    io_reason(&e)
                ));
                status = 1;
            }
        }
    }
    Util::Done(status)
}

fn mv(os: &dyn Os, args: &[String], sink: &mut Sink) -> Util {
    // `mv` takes no flag this subset runs: any flag declines.
    let Some((_, ops)) = split_flags(args, "") else {
        return Util::Decline;
    };
    match ops.len() {
        0 => return usage(sink, "mv", "mv: missing file operand"),
        1 => {
            let msg = format!("mv: missing destination file operand after '{}'", ops[0]);
            return usage(sink, "mv", &msg);
        }
        _ => {}
    }
    let (sources, dest) = ops.split_at(ops.len() - 1);
    let dest = dest[0];
    let into_dir = is_dir(os, dest);
    if sources.len() > 1 && !into_dir {
        sink.eline(&format!("mv: target '{dest}' is not a directory"));
        return Util::Done(1);
    }
    let mut status = 0;
    for src in sources {
        if let Err(e) = os.symlink_metadata(src) {
            sink.eline(&format!("mv: cannot stat '{src}': {}", io_reason(&e)));
            status = 1;
            continue;
        }
        let target = if into_dir {
            join(dest, base_name(src))
        } else {
            dest.into()
        };
        if same_path(os, src, &target) {
            sink.eline(&format!("mv: '{src}' and '{target}' are the same file"));
            status = 1;
            continue;
        }
        match os.gate().guarded_rename(os, src, &target) {
            Ok(()) => {}
            Err(WriteError::Refused(r)) => return refused(sink, &r),
            Err(WriteError::Io(e)) => {
                let msg = format!("mv: cannot move '{src}' to '{target}': {}", io_reason(&e));
                sink.eline(&msg);
                status = 1;
            }
        }
    }
    Util::Done(status)
}

fn mkdir(os: &dyn Os, args: &[String], sink: &mut Sink) -> Util {
    let Some((flags, ops)) = split_flags(args, "p") else {
        return Util::Decline;
    };
    let parents = flags.contains('p');
    if ops.is_empty() {
        return usage(sink, "mkdir", "mkdir: missing operand");
    }
    let mut status = 0;
    for p in ops {
        let dirs = match new_dirs(os, p, parents) {
            Ok(dirs) => dirs,
            Err(why) => {
                sink.eline(&format!("mkdir: cannot create directory '{p}': {why}"));
                status = 1;
                continue;
            }
        };
        if dirs.is_empty() {
            continue;
        }
        match os.gate().guarded_create_dirs(os, &dirs) {
            Ok(()) => {}
            Err(WriteError::Refused(r)) => return refused(sink, &r),
            Err(WriteError::Io(e)) => {
                sink.eline(&format!(
                    "mkdir: cannot create directory '{p}': {}",
                    io_reason(&e)
                ));
                status = 1;
            }
        }
    }
    Util::Done(status)
}

/// The directories `mkdir` must create for `path`, highest first. Empty when
/// `parents` finds a directory there already. On failure, the reason as GNU
/// prints it.
fn new_dirs(os: &dyn Os, path: &str, parents: bool) -> Result<Vec<String>, String> {
    if let Ok(m) = os.metadata(path) {
        return if parents && m.kind == FileKind::Dir {
            Ok(Vec::new())
        } else {
            Err("File exists".into())
        };
    }
    if !parents {
        let parent = parent_of(path);
        return match os.metadata(&parent) {
            Ok(m) if m.kind == FileKind::Dir => Ok(alloc::vec![path.into()]),
            Ok(_) => Err("Not a directory".into()),
            Err(e) => Err(io_reason(&e)),
        };
    }
    let mut missing = alloc::vec![String::from(path)];
    let mut dir = parent_of(path);
    loop {
        match os.metadata(&dir) {
            Ok(m) if m.kind == FileKind::Dir => break,
            Ok(_) => return Err("Not a directory".into()),
            Err(e) => {
                let up = parent_of(&dir);
                if up == dir {
                    return Err(io_reason(&e));
                }
                missing.push(core::mem::replace(&mut dir, up));
            }
        }
    }
    missing.reverse();
    Ok(missing)
}

// ════════════════════════════════════════════════════════════════════════
// Text filters, files, search, environment and time
// ════════════════════════════════════════════════════════════════════════
//
// Each one prints what GNU coreutils prints for the flags it runs. A flag
// it does not run declines, and the external program runs instead.

/// What `yes` writes at most, in bytes. The subset's sink has no signal that
/// the reader has gone (a closed pipe), so an endless writer needs a bound.
const YES_CAP: usize = 64 * 1024 * 1024;

/// The `date` format when none is given, as GNU prints it.
const DATE_DEFAULT: &str = "%a %b %e %H:%M:%S %Z %Y";

/// Why a `tr` set was not expanded.
enum SetErr {
    /// A form this subset does not run: `[=c=]`, `[c*n]`, octal escapes, an
    /// unknown class name.
    Decline,
    /// A range whose end sorts before its start.
    Reversed(u8, u8),
}

/// One unit of a `tr` set: a byte, or a character class.
enum TrUnit {
    Byte(u8),
    Class(Vec<u8>),
}

/// The bytes of the named character class, in ascending order. `None` for a
/// class this subset does not run.
fn tr_class(name: &[u8]) -> Option<Vec<u8>> {
    let range = |a: u8, b: u8| (a..=b).collect::<Vec<u8>>();
    Some(match name {
        b"upper" => range(b'A', b'Z'),
        b"lower" => range(b'a', b'z'),
        b"digit" => range(b'0', b'9'),
        b"alpha" => [range(b'A', b'Z'), range(b'a', b'z')].concat(),
        b"alnum" => [range(b'0', b'9'), range(b'A', b'Z'), range(b'a', b'z')].concat(),
        b"space" => vec![9, 10, 11, 12, 13, 32],
        _ => return None,
    })
}

/// The unit that starts at `b[i]`, and the index after it.
fn tr_unit(b: &[u8], i: usize) -> Result<(TrUnit, usize), SetErr> {
    match b[i] {
        b'\\' => {
            let Some(&c) = b.get(i + 1) else {
                return Err(SetErr::Decline);
            };
            let v = match c {
                b'n' => b'\n',
                b't' => b'\t',
                b'r' => b'\r',
                b'\\' => b'\\',
                _ => return Err(SetErr::Decline),
            };
            Ok((TrUnit::Byte(v), i + 2))
        }
        b'[' if b.get(i + 1) == Some(&b':') => {
            match b[i + 2..].windows(2).position(|w| w == b":]") {
                Some(p) => {
                    let end = i + 2 + p;
                    match tr_class(&b[i + 2..end]) {
                        Some(class) => Ok((TrUnit::Class(class), end + 2)),
                        None => Err(SetErr::Decline),
                    }
                }
                // No closing `:]`: the bracket is an ordinary byte.
                None => Ok((TrUnit::Byte(b'['), i + 1)),
            }
        }
        b'[' if b.get(i + 1) == Some(&b'=') || b.get(i + 2) == Some(&b'*') => Err(SetErr::Decline),
        c => Ok((TrUnit::Byte(c), i + 1)),
    }
}

/// The bytes a `tr` set names, in order: ranges expanded, classes expanded,
/// escapes read.
fn tr_set(s: &str) -> Result<Vec<u8>, SetErr> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let (unit, next) = tr_unit(b, i)?;
        if let TrUnit::Byte(from) = unit {
            if next + 1 < b.len() && b[next] == b'-' {
                match tr_unit(b, next + 1)? {
                    (TrUnit::Byte(to), after) => {
                        if from > to {
                            return Err(SetErr::Reversed(from, to));
                        }
                        out.extend(from..=to);
                        i = after;
                        continue;
                    }
                    (TrUnit::Class(_), _) => return Err(SetErr::Decline),
                }
            }
        }
        match unit {
            TrUnit::Byte(x) => out.push(x),
            TrUnit::Class(c) => out.extend_from_slice(&c),
        }
        i = next;
    }
    Ok(out)
}

/// `tr [-cCds] SET1 [SET2]`, over stdin. Translates when SET2 is given without
/// `-d`; squeezes when `-s` is given, over SET2 if there is one and SET1 if not.
/// `-c` complements SET1 in ascending byte order, as GNU does.
fn tr(args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let Some((flags, sets)) = short_flags(args, "cCds") else {
        return Util::Decline;
    };
    let complement = flags.contains(&'c') || flags.contains(&'C');
    let delete = flags.contains(&'d');
    let squeeze = flags.contains(&'s');
    let two = match sets.len() {
        0 => return usage(sink, "tr", "tr: missing operand"),
        1 => false,
        2 => true,
        _ => return Util::Decline,
    };
    // `tr -d SET1 SET2` without `-s` is GNU's "extra operand": let it say so.
    if two && delete && !squeeze {
        return Util::Decline;
    }
    let translate = two && !delete;
    // One set needs exactly one of -d and -s; with neither, or both, GNU
    // asks for a second set, so that case is left to it.
    if !two && delete == squeeze {
        return Util::Decline;
    }
    let mut set1 = match tr_set(&sets[0]) {
        Ok(v) => v,
        Err(SetErr::Decline) => return Util::Decline,
        Err(SetErr::Reversed(a, b)) => {
            sink.eline(&format!(
                "tr: range-endpoints of '{}-{}' are in reverse collating sequence order",
                a as char, b as char
            ));
            return Util::Done(1);
        }
    };
    let set2 = if two {
        match tr_set(&sets[1]) {
            Ok(v) => v,
            Err(SetErr::Decline) => return Util::Decline,
            Err(SetErr::Reversed(a, b)) => {
                sink.eline(&format!(
                    "tr: range-endpoints of '{}-{}' are in reverse collating sequence order",
                    a as char, b as char
                ));
                return Util::Done(1);
            }
        }
    } else {
        Vec::new()
    };
    if translate && set2.is_empty() {
        return Util::Decline;
    }
    if complement {
        set1 = (0..=255u8).filter(|b| !set1.contains(b)).collect();
    }
    let mut map: [u8; 256] = core::array::from_fn(|i| i as u8);
    if translate {
        // SET2 is padded with its last byte, as GNU pads it; a repeated byte
        // in SET1 takes the later mapping.
        let last = set2[set2.len() - 1];
        for (k, &from) in set1.iter().enumerate() {
            map[from as usize] = set2.get(k).copied().unwrap_or(last);
        }
    }
    let mut del = [false; 256];
    if delete {
        for &b in &set1 {
            del[b as usize] = true;
        }
    }
    let mut sq = [false; 256];
    if squeeze {
        let squeeze_set: &Vec<u8> = if two { &set2 } else { &set1 };
        for &b in squeeze_set {
            sq[b as usize] = true;
        }
    }
    let data = stdin.read_all();
    let mut last: Option<u8> = None;
    for &b in &data {
        if del[b as usize] {
            continue;
        }
        let o = map[b as usize];
        if squeeze && sq[o as usize] && last == Some(o) {
            continue;
        }
        sink.out.push(o);
        last = Some(o);
    }
    Util::Done(0)
}

/// `tee [-a] FILE...`: stdin to stdout and to each file. Each file is a write
/// the gate admits, with `-a` an append.
fn tee(os: &dyn Os, args: &[String], stdin: &mut Input, sink: &mut Sink) -> Util {
    let Some((flags, files)) = short_flags(args, "a") else {
        return Util::Decline;
    };
    // GNU reads "-" as a file operand's stdin-like name in some versions; this
    // subset does not guess which.
    if files.iter().any(|f| f == "-") {
        return Util::Decline;
    }
    let append = flags.contains(&'a');
    let data = stdin.read_all();
    sink.out.extend_from_slice(&data);
    let mut status = 0;
    for f in &files {
        match os.gate().guarded_write(os, f, &data, append) {
            Ok(()) => {}
            Err(WriteError::Refused(r)) => return refused(sink, &r),
            Err(WriteError::Io(e)) => {
                sink.eline(&format!("tee: {f}: {}", io_reason(&e)));
                status = 1;
            }
        }
    }
    Util::Done(status)
}

/// `touch [-c] FILE...`. A file that is absent is created empty, through the
/// gate. A file that is there is left as it is: the [`Os`] trait has no way to
/// set a modification time, so touching an existing file succeeds and changes
/// nothing (its mtime does not move). `-c` creates nothing.
fn touch(os: &dyn Os, args: &[String], sink: &mut Sink) -> Util {
    let Some((flags, files)) = short_flags(args, "c") else {
        return Util::Decline;
    };
    if files.is_empty() {
        return usage(sink, "touch", "touch: missing file operand");
    }
    let no_create = flags.contains(&'c');
    let mut status = 0;
    for f in &files {
        match os.metadata(f) {
            Ok(_) => continue,
            Err(e) if e.kind == ErrorKind::NotFound => {
                if no_create {
                    continue;
                }
            }
            Err(e) => {
                sink.eline(&format!("touch: cannot touch '{f}': {}", io_reason(&e)));
                status = 1;
                continue;
            }
        }
        match os.gate().guarded_write(os, f, &[], false) {
            Ok(()) => {}
            Err(WriteError::Refused(r)) => return refused(sink, &r),
            Err(WriteError::Io(e)) => {
                sink.eline(&format!("touch: cannot touch '{f}': {}", io_reason(&e)));
                status = 1;
            }
        }
    }
    Util::Done(status)
}

/// Whether `cand` names a usable program: a file that is not a directory and,
/// where the host has permission bits, one of them executable. On a host
/// without them, each of `exts` (from PATHEXT) is tried after the bare name.
fn which_hit(os: &dyn Os, cand: &str, exts: &[String]) -> Option<String> {
    let mut names = alloc::vec![String::from(cand)];
    names.extend(exts.iter().map(|e| format!("{cand}{e}")));
    let unix = os.has_unix_metadata();
    names.into_iter().find(|p| {
        os.metadata(p)
            .is_ok_and(|m| m.kind != FileKind::Dir && (!unix || m.mode & 0o111 != 0))
    })
}

/// `which NAME...`: the first match for each name on PATH, one per line. A
/// name with a `/` is checked as it is. Exit 1 when any name is not found,
/// and nothing is printed for it, as GNU and debianutils `which` do.
fn which(os: &dyn Os, args: &[String], sink: &mut Sink) -> Util {
    let Some((_, names)) = short_flags(args, "") else {
        return Util::Decline;
    };
    if names.is_empty() {
        return Util::Decline;
    }
    // The host reports Windows by having no Unix metadata (`ls -l` relies on
    // the same signal). Its PATH is `;`-separated, and its programs take
    // extensions from PATHEXT.
    let unix = os.has_unix_metadata();
    let sep = if unix { ':' } else { ';' };
    let exts: Vec<String> = if unix {
        Vec::new()
    } else {
        os.env_var("PATHEXT")
            .unwrap_or_default()
            .split(';')
            .filter(|e| !e.is_empty())
            .map(String::from)
            .collect()
    };
    let path = os.env_var("PATH").unwrap_or_default();
    let mut status = 0;
    for name in &names {
        let found = if name.contains('/') {
            which_hit(os, name, &exts)
        } else {
            path.split(sep).find_map(|d| {
                let d = if d.is_empty() { "." } else { d };
                which_hit(os, &join(d, name), &exts)
            })
        };
        match found {
            Some(p) => sink.out.extend_from_slice(format!("{p}\n").as_bytes()),
            None => status = 1,
        }
    }
    Util::Done(status)
}

/// A value `expr` computes: its text. An integer is its decimal text.
type ExprVal = String;

/// Why `expr` did not compute a value.
enum ExprErr {
    /// Not a syntax this subset runs (a back-reference, a regex it cannot
    /// compile, no arguments): the external program answers.
    Decline,
    Syntax,
    NonInteger,
    DivZero,
}

/// Whether `s` is an optional `-` and one or more digits.
fn looks_int(s: &str) -> bool {
    let d = s.strip_prefix('-').unwrap_or(s);
    !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit())
}

/// `s` as an integer, when it is one that fits.
fn int_of(s: &str) -> Option<i64> {
    if looks_int(s) {
        s.parse().ok()
    } else {
        None
    }
}

/// `s` as an integer for arithmetic. Not one is an error.
fn num(s: &str) -> Result<i64, ExprErr> {
    if !looks_int(s) {
        return Err(ExprErr::NonInteger);
    }
    s.parse().map_err(|_| ExprErr::Decline)
}

/// `expr`'s null-or-zero test: the empty string, or an integer zero.
fn is_null(s: &str) -> bool {
    s.is_empty() || int_of(s) == Some(0)
}

/// `STRING : REGEX`: anchored at the start, as `expr` anchors it. With a
/// `\(...\)` in the pattern the result is its first group (empty when there
/// is no match); without one, the length of the match (0 when none).
fn regex_match(s: &str, pat: &str) -> Result<ExprVal, ExprErr> {
    let Some(conv) = bre_to_regex(pat) else {
        return Err(ExprErr::Decline);
    };
    let Ok(re) = re::Regex::new(&format!("^(?:{conv})")) else {
        return Err(ExprErr::Decline);
    };
    let bytes = s.as_bytes();
    let grouped = pat.contains("\\(");
    let Some(&(_, end)) = re.find_spans(bytes).first() else {
        return Ok(if grouped { String::new() } else { "0".into() });
    };
    if grouped {
        // Replacing the whole anchored match with `$1` leaves exactly the
        // group. The trailing `.*` only widens the match; it cannot change the
        // first match's groups.
        let Ok(re2) = re::Regex::new(&format!("^(?:{conv})(?s:.*)")) else {
            return Err(ExprErr::Decline);
        };
        let rep = re2.replace(bytes, b"$1", false);
        return Ok(String::from_utf8_lossy(&rep).into_owned());
    }
    Ok(String::from_utf8_lossy(&bytes[..end])
        .chars()
        .count()
        .to_string())
}

/// The recursive-descent parser for `expr`, with GNU's precedence, lowest
/// first: `|`, `&`, comparisons, `+ -`, `* / %`, `:`, then operands and the
/// functions `length`, `substr`, `index` and `match`.
struct Expr<'a> {
    t: &'a [String],
    i: usize,
}

impl<'a> Expr<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.t.get(self.i).map(String::as_str)
    }

    fn take(&mut self) -> Option<&'a str> {
        let v = self.peek();
        self.i += 1;
        v
    }

    /// The `n` operands after the current token are all there.
    fn has(&self, n: usize) -> bool {
        self.t.len().saturating_sub(self.i) >= n
    }

    fn or(&mut self) -> Result<ExprVal, ExprErr> {
        let mut l = self.and()?;
        while self.peek() == Some("|") {
            self.i += 1;
            let r = self.and()?;
            if is_null(&l) {
                l = r;
            }
        }
        Ok(l)
    }

    fn and(&mut self) -> Result<ExprVal, ExprErr> {
        let mut l = self.cmp()?;
        while self.peek() == Some("&") {
            self.i += 1;
            let r = self.cmp()?;
            if is_null(&l) || is_null(&r) {
                l = "0".into();
            }
        }
        Ok(l)
    }

    fn cmp(&mut self) -> Result<ExprVal, ExprErr> {
        let mut l = self.add()?;
        while let Some(op) = self
            .peek()
            .filter(|o| matches!(*o, "=" | "!=" | "<" | "<=" | ">" | ">="))
        {
            self.i += 1;
            let r = self.add()?;
            // Integers compare as numbers; anything else compares as bytes.
            let ord = match (int_of(&l), int_of(&r)) {
                (Some(a), Some(b)) => a.cmp(&b),
                _ => l.as_bytes().cmp(r.as_bytes()),
            };
            let yes = match op {
                "=" => ord.is_eq(),
                "!=" => ord.is_ne(),
                "<" => ord.is_lt(),
                "<=" => ord.is_le(),
                ">" => ord.is_gt(),
                _ => ord.is_ge(),
            };
            l = String::from(if yes { "1" } else { "0" });
        }
        Ok(l)
    }

    fn add(&mut self) -> Result<ExprVal, ExprErr> {
        let mut l = self.mul()?;
        while matches!(self.peek(), Some("+") | Some("-")) {
            let op = self.take().unwrap_or("+");
            let r = self.mul()?;
            let (a, b) = (num(&l)?, num(&r)?);
            let v = if op == "+" {
                a.checked_add(b)
            } else {
                a.checked_sub(b)
            };
            l = v.ok_or(ExprErr::Decline)?.to_string();
        }
        Ok(l)
    }

    fn mul(&mut self) -> Result<ExprVal, ExprErr> {
        let mut l = self.matching()?;
        while matches!(self.peek(), Some("*") | Some("/") | Some("%")) {
            let op = self.take().unwrap_or("*");
            let r = self.matching()?;
            let (a, b) = (num(&l)?, num(&r)?);
            let v = match op {
                "*" => a.checked_mul(b),
                _ if b == 0 => return Err(ExprErr::DivZero),
                "/" => a.checked_div(b),
                _ => a.checked_rem(b),
            };
            l = v.ok_or(ExprErr::Decline)?.to_string();
        }
        Ok(l)
    }

    fn matching(&mut self) -> Result<ExprVal, ExprErr> {
        let mut l = self.primary()?;
        while self.peek() == Some(":") {
            self.i += 1;
            let r = self.primary()?;
            l = regex_match(&l, &r)?;
        }
        Ok(l)
    }

    fn primary(&mut self) -> Result<ExprVal, ExprErr> {
        let Some(t) = self.take() else {
            return Err(ExprErr::Syntax);
        };
        match t {
            "(" => {
                let v = self.or()?;
                if self.take() != Some(")") {
                    return Err(ExprErr::Syntax);
                }
                Ok(v)
            }
            ")" => Err(ExprErr::Syntax),
            // A function with too few operands is GNU's to answer.
            "length" if self.has(1) => {
                Ok(self.take().unwrap_or_default().chars().count().to_string())
            }
            "substr" if self.has(3) => {
                let s = self.take().unwrap_or_default();
                let pos = self.take().and_then(int_of);
                let len = self.take().and_then(int_of);
                let (Some(pos), Some(len)) = (pos, len) else {
                    return Ok(String::new());
                };
                if pos < 1 || len < 1 {
                    return Ok(String::new());
                }
                let take = len as usize;
                Ok(s.chars().skip(pos as usize - 1).take(take).collect())
            }
            "index" if self.has(2) => {
                let s = self.take().unwrap_or_default();
                let set = self.take().unwrap_or_default();
                let at = s.chars().position(|c| set.contains(c)).map_or(0, |p| p + 1);
                Ok(at.to_string())
            }
            "match" if self.has(2) => {
                let s = self.take().unwrap_or_default();
                let pat = self.take().unwrap_or_default();
                regex_match(s, &pat)
            }
            _ => Ok(t.to_string()),
        }
    }
}

/// `expr EXPRESSION`. The value goes to stdout; the status is 1 when the value
/// is null or zero, 0 otherwise, and 2 on an error. Comparisons of two integers
/// are numeric; other comparisons are bytewise (GNU uses the collation).
fn expr(args: &[String], sink: &mut Sink) -> Util {
    if args.is_empty() || args[0] == "--help" || args[0] == "--version" {
        return Util::Decline;
    }
    let mut p = Expr { t: args, i: 0 };
    let res = match p.or() {
        Ok(v) if p.i == args.len() => Ok(v),
        Ok(_) => Err(ExprErr::Syntax),
        Err(e) => Err(e),
    };
    match res {
        Ok(v) => {
            let status = if is_null(&v) { 1 } else { 0 };
            sink.out.extend_from_slice(format!("{v}\n").as_bytes());
            Util::Done(status)
        }
        Err(ExprErr::Decline) => Util::Decline,
        Err(ExprErr::Syntax) => {
            sink.eline("expr: syntax error");
            Util::Done(2)
        }
        Err(ExprErr::NonInteger) => {
            sink.eline("expr: non-integer argument");
            Util::Done(2)
        }
        Err(ExprErr::DivZero) => {
            sink.eline("expr: division by zero");
            Util::Done(2)
        }
    }
}

/// Writes `line` into `out` until `out` has grown by at least `cap` bytes.
fn yes_into(out: &mut Vec<u8>, line: &[u8], cap: usize) {
    let start = out.len();
    while out.len() - start < cap {
        out.extend_from_slice(line);
    }
}

/// `yes [STRING...]`: `STRING...` (default `y`) joined by spaces, on a line,
/// until the output reaches [`YES_CAP`]. GNU never stops on its own; here the
/// bound is the only stop, because the sink cannot say the reader has gone. The
/// status is 0, which is what the pipeline it feeds reports.
fn yes(args: &[String], sink: &mut Sink) -> Util {
    if args
        .first()
        .is_some_and(|a| a == "--help" || a == "--version")
    {
        return Util::Decline;
    }
    let mut line = if args.is_empty() {
        String::from("y")
    } else {
        args.join(" ")
    };
    line.push('\n');
    yes_into(&mut sink.out, line.as_bytes(), YES_CAP);
    Util::Done(0)
}

/// `readlink [-f] FILE...`: each link's target, one per line. `-f` gives the
/// canonical path, and the last component may be missing, as GNU allows.
fn readlink(os: &dyn Os, args: &[String], sink: &mut Sink) -> Util {
    let Some((flags, files)) = short_flags(args, "f") else {
        return Util::Decline;
    };
    if files.is_empty() {
        return usage(sink, "readlink", "readlink: missing operand");
    }
    let canonical = flags.contains(&'f');
    let mut status = 0;
    for f in &files {
        let res = if canonical {
            canonical_path(os, f)
        } else {
            os.read_link(f)
        };
        match res {
            Ok(p) => sink.out.extend_from_slice(format!("{p}\n").as_bytes()),
            Err(e) => {
                sink.eline(&format!("readlink: {f}: {}", io_reason(&e)));
                status = 1;
            }
        }
    }
    Util::Done(status)
}

/// The canonical path of `path`. When only its last component is missing,
/// the parent is canonicalized and the name joined on, as `readlink -f` does.
fn canonical_path(os: &dyn Os, path: &str) -> Result<String, OsError> {
    match os.canonicalize(path) {
        Err(e) if e.kind == ErrorKind::NotFound => {
            let base = base_name(path);
            if base.is_empty() || base == "." || base == ".." {
                return Err(e);
            }
            let parent = os.canonicalize(&parent_of(path))?;
            Ok(join(&parent, base))
        }
        other => other,
    }
}

/// The GNU `stat -c` specifiers this host's [`Meta`] can answer. `%i` (the
/// inode) and the rest are refused.
const STAT_SPECS: &str = "nsFaUGYhugbf%";

/// Refuses a format with a specifier outside [`STAT_SPECS`], before any file is
/// looked at.
fn stat_check(format: &str) -> Result<(), String> {
    let mut it = format.chars();
    while let Some(c) = it.next() {
        if c != '%' {
            continue;
        }
        match it.next() {
            Some(s) if STAT_SPECS.contains(s) => {}
            Some(s) => return Err(format!("stat: unsupported format specifier '%{s}'")),
            None => return Err(String::from("stat: format ends in '%'")),
        }
    }
    Ok(())
}

/// The name `stat` gives a file type, from [`Meta`]. `None` for a kind GNU
/// would describe in a way this subset has not measured.
fn stat_kind(m: &Meta) -> Option<&'static str> {
    Some(match m.kind {
        FileKind::File if m.len == 0 => "regular empty file",
        FileKind::File => "regular file",
        FileKind::Dir => "directory",
        FileKind::Symlink => "symbolic link",
        FileKind::BlockDevice => "block special file",
        FileKind::CharDevice => "character special file",
        FileKind::Fifo => "fifo",
        FileKind::Socket => "socket",
        FileKind::Other => return None,
    })
}

/// One `stat -c` line for `m`, the metadata of `name`. Backslashes in the
/// format are not escapes here: GNU reads escapes only for `--printf`.
fn stat_render(os: &dyn Os, format: &str, name: &str, m: &Meta) -> Result<String, String> {
    let mut out = String::new();
    let mut it = format.chars();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('%') => out.push('%'),
            Some('n') => out.push_str(name),
            Some('s') => out.push_str(&m.len.to_string()),
            Some('F') => match stat_kind(m) {
                Some(k) => out.push_str(k),
                None => return Err(format!("stat: cannot describe the file type of '{name}'")),
            },
            Some('a') => out.push_str(&format!("{:o}", m.mode & 0o7777)),
            Some('U') => out.push_str(&os.user_name(m.uid).unwrap_or_else(|| m.uid.to_string())),
            Some('G') => out.push_str(&os.group_name(m.gid).unwrap_or_else(|| m.gid.to_string())),
            Some('Y') => out.push_str(&m.mtime.to_string()),
            Some('h') => out.push_str(&m.nlink.to_string()),
            Some('u') => out.push_str(&m.uid.to_string()),
            Some('g') => out.push_str(&m.gid.to_string()),
            Some('b') => out.push_str(&m.blocks.to_string()),
            Some('f') => out.push_str(&format!("{:x}", m.mode)),
            _ => {}
        }
    }
    Ok(out)
}

/// `stat -c FORMAT FILE...`. The default multi-line layout (device, inode,
/// the three times) is not reproduced, so `-c` is required. Symbolic links are
/// not followed, as GNU `stat` does not follow them without `-L`.
fn stat(os: &dyn Os, args: &[String], sink: &mut Sink) -> Util {
    let mut format: Option<String> = None;
    let mut files: Vec<&str> = Vec::new();
    let mut options = true;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if options && a == "--" {
            options = false;
        } else if options && a == "-c" {
            match args.get(i + 1) {
                Some(f) => {
                    format = Some(f.clone());
                    i += 1;
                }
                None => return Util::Decline,
            }
        } else if options && a.starts_with("--format=") {
            format = Some(a["--format=".len()..].to_string());
        } else if options && a.len() > 1 && a.starts_with('-') {
            return Util::Decline;
        } else {
            files.push(a);
        }
        i += 1;
    }
    if files.is_empty() {
        return usage(sink, "stat", "stat: missing operand");
    }
    let Some(format) = format else {
        sink.eline("stat: this host needs -c FORMAT; the default layout is not reproduced");
        return Util::Done(1);
    };
    if let Err(msg) = stat_check(&format) {
        sink.eline(&msg);
        return Util::Done(1);
    }
    let mut status = 0;
    for f in files {
        let m = match os.symlink_metadata(f) {
            Ok(m) => m,
            Err(e) => {
                sink.eline(&format!("stat: cannot stat '{f}': {}", io_reason(&e)));
                status = 1;
                continue;
            }
        };
        match stat_render(os, &format, f, &m) {
            Ok(line) => sink.out.extend_from_slice(format!("{line}\n").as_bytes()),
            Err(msg) => {
                sink.eline(&msg);
                status = 1;
            }
        }
    }
    Util::Done(status)
}

/// The operands of `env`: `-i` (or `-`), `-u NAME`, leading `NAME=VALUE`
/// operands, and the command with its arguments, if any.
struct EnvArgs {
    clear: bool,
    unset: Vec<String>,
    assign: Vec<(String, String)>,
    command: Vec<String>,
}

/// Parses `env`'s arguments. `None` for an option this subset does not run.
fn env_args(args: &[String]) -> Option<EnvArgs> {
    let mut e = EnvArgs {
        clear: false,
        unset: Vec::new(),
        assign: Vec::new(),
        command: Vec::new(),
    };
    let mut options = true;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if options && a == "--" {
            options = false;
        } else if options && (a == "-" || a == "-i") {
            e.clear = true;
        } else if options && a == "-u" {
            e.unset.push(args.get(i + 1)?.clone());
            i += 1;
        } else if options && a.starts_with("--unset=") {
            e.unset.push(a["--unset=".len()..].to_string());
        } else if options && a.len() > 1 && a.starts_with('-') {
            return None;
        } else {
            break;
        }
        i += 1;
    }
    while i < args.len() {
        match args[i].split_once('=') {
            Some((n, v)) if !n.is_empty() => e.assign.push((n.into(), v.into())),
            _ => break,
        }
        i += 1;
    }
    e.command = args[i..].to_vec();
    Some(e)
}

/// `date` with `-u` (UTC) or the local time, `+FORMAT` or GNU's default. The
/// UTC form is computed here, so it works where the host has no local
/// formatter. Local time needs the host's [`Os::format_local`], which declines
/// a format it does not accept, and then so does `date`.
fn date(os: &dyn Os, args: &[String], sink: &mut Sink) -> Util {
    let mut utc = false;
    let mut format: Option<&str> = None;
    for a in args {
        if a == "-u" || a == "--utc" {
            utc = true;
        } else if let Some(f) = a.strip_prefix('+') {
            if format.is_some() {
                return Util::Decline;
            }
            format = Some(f);
        } else {
            return Util::Decline;
        }
    }
    let at = os.now();
    let f = format.unwrap_or(DATE_DEFAULT);
    let text = if utc {
        format_utc(at, f)
    } else {
        os.format_local(at, f)
    };
    match text {
        Some(t) => {
            sink.out.extend_from_slice(format!("{t}\n").as_bytes());
            Util::Done(0)
        }
        None => Util::Decline,
    }
}

/// The fields of a UTC instant that [`render_utc`] needs.
struct Utc {
    year: i64,
    month: u32,
    day: u32,
    /// 0 is Sunday.
    wday: u32,
    /// 1-based.
    yday: u32,
    hour: u32,
    min: u32,
    sec: u32,
    epoch: i64,
}

/// Days since 1970-01-01 to (year, month, day), in the proleptic Gregorian
/// calendar (H. Hinnant's `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// The inverse of [`civil_from_days`]: (year, month, day) to days since
/// 1970-01-01.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from(if m > 2 { m - 3 } else { m + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The UTC fields of `at`. `None` for a year outside 1000..=9999, which this
/// formatter does not render.
fn utc_fields(at: Timestamp) -> Option<Utc> {
    let days = at.secs.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    if !(1000..=9999).contains(&year) {
        return None;
    }
    let of_day = at.secs.rem_euclid(86_400) as u32;
    Some(Utc {
        year,
        month,
        day,
        wday: (days + 4).rem_euclid(7) as u32,
        yday: (days - days_from_civil(year, 1, 1) + 1) as u32,
        hour: of_day / 3600,
        min: of_day % 3600 / 60,
        sec: of_day % 60,
        epoch: at.secs,
    })
}

/// `at` in UTC, rendered with a `strftime` format. Supports the conversions
/// `date` is commonly given; any other one, a trailing `%`, or a year outside
/// 1000..=9999 gives `None`, so the caller declines rather than guess.
fn format_utc(at: Timestamp, format: &str) -> Option<String> {
    let t = utc_fields(at)?;
    let mut out = String::new();
    render_utc(&t, format, &mut out)?;
    Some(out)
}

fn render_utc(t: &Utc, format: &str, out: &mut String) -> Option<()> {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const DAYS_LONG: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const MONTHS_LONG: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let h12 = if t.hour % 12 == 0 { 12 } else { t.hour % 12 };
    let mut it = format.chars();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match it.next()? {
            '%' => out.push('%'),
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'a' => out.push_str(DAYS[t.wday as usize]),
            'A' => out.push_str(DAYS_LONG[t.wday as usize]),
            'b' | 'h' => out.push_str(MONTHS[t.month as usize - 1]),
            'B' => out.push_str(MONTHS_LONG[t.month as usize - 1]),
            'C' => out.push_str(&format!("{:02}", t.year / 100)),
            'c' => render_utc(t, "%a %b %e %H:%M:%S %Y", out)?,
            'd' => out.push_str(&format!("{:02}", t.day)),
            'D' | 'x' => render_utc(t, "%m/%d/%y", out)?,
            'e' => out.push_str(&format!("{:>2}", t.day)),
            'F' => render_utc(t, "%Y-%m-%d", out)?,
            'H' => out.push_str(&format!("{:02}", t.hour)),
            'I' => out.push_str(&format!("{:02}", h12)),
            'j' => out.push_str(&format!("{:03}", t.yday)),
            'k' => out.push_str(&format!("{:>2}", t.hour)),
            'l' => out.push_str(&format!("{:>2}", h12)),
            'm' => out.push_str(&format!("{:02}", t.month)),
            'M' => out.push_str(&format!("{:02}", t.min)),
            'p' => out.push_str(if t.hour < 12 { "AM" } else { "PM" }),
            'P' => out.push_str(if t.hour < 12 { "am" } else { "pm" }),
            'r' => render_utc(t, "%I:%M:%S %p", out)?,
            'R' => render_utc(t, "%H:%M", out)?,
            'S' => out.push_str(&format!("{:02}", t.sec)),
            's' => out.push_str(&t.epoch.to_string()),
            'T' | 'X' => render_utc(t, "%H:%M:%S", out)?,
            'u' => out.push_str(&(if t.wday == 0 { 7 } else { t.wday }).to_string()),
            'w' => out.push_str(&t.wday.to_string()),
            'y' => out.push_str(&format!("{:02}", t.year % 100)),
            'Y' => out.push_str(&t.year.to_string()),
            'Z' => out.push_str("UTC"),
            'z' => out.push_str("+0000"),
            _ => return None,
        }
    }
    Some(())
}

// ════════════════════════════════════════════════════════════════════════
// Entry point
// ════════════════════════════════════════════════════════════════════════

/// Run `src` as a shell script, with the host's files, environment,
/// processes, streams and policy taken from `os`.
///
/// Returns the exit status, or `None` when the script uses something this
/// subset does not run -- decided before anything executes, so the caller can
/// fall back without a half-run script behind it.
pub fn run_on(os: &dyn Os, src: &str) -> Option<i32> {
    Session::new(os).run(src).map(|o| o.status)
}

/// The shell state a host keeps between lines: variables, exports and `$?`.
/// A host with a prompt keeps one `Session` and runs each line through
/// [`Session::run`]; [`run_on`] is a session that lives for one script.
pub struct Session<'a> {
    sh: Shell<'a>,
}

/// How one [`Session::run`] ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    /// The status of the last command that ran, or the status `exit` gave.
    pub status: i32,
    /// Whether an `exit` ran. A host that has to stop on `exit` checks this
    /// rather than guessing from the status.
    pub exited: bool,
}

impl<'a> Session<'a> {
    /// A session with no variables, no exports, and `$?` at 0.
    pub fn new(os: &'a dyn Os) -> Self {
        Session {
            sh: Shell {
                vars: BTreeMap::new(),
                exported: BTreeSet::new(),
                last: 0,
                glob_refused: None,
                os,
            },
        }
    }

    /// Runs `src` in this session, and leaves its variables and `$?` for the
    /// next call. `None` when the subset does not run `src`: decided before
    /// anything executes, so nothing has changed.
    pub fn run(&mut self, src: &str) -> Option<Outcome> {
        let list = parse(src).ok()?;
        let os = self.sh.os;
        let mut stdin = Input::real(os);
        let mut sink = Sink::new(os, ErrMode::Real);
        // An empty line keeps `$?`, as an interactive shell does.
        let mut status = self.sh.last;
        // Output reaches the terminal as each top-level command finishes, not at
        // the end -- but only here: a loop body inside a pipeline must feed the
        // next stage, not the terminal.
        for ao in &list {
            let flow = self.sh.run_and_or(ao, &mut stdin, &mut sink);
            os.write_stdout(&sink.out);
            sink.out.clear();
            match flow {
                Flow::Next(c) => {
                    status = c;
                    self.sh.last = c;
                }
                Flow::Exit(c) => {
                    self.sh.last = c;
                    return Some(Outcome {
                        status: c,
                        exited: true,
                    });
                }
                Flow::Break(_) | Flow::Continue(_) => {}
            }
        }
        Some(Outcome {
            status,
            exited: false,
        })
    }
}

/// Whether `src` is inside the subset, without running it.
pub fn supports(src: &str) -> Result<(), String> {
    parse(src).map(|_| ()).map_err(|Unsupported(w)| w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os::{DirEntry, Gate, Meta, Refusal};

    /// A test host over the real file system and environment, with no
    /// policy: every program and write is admitted. It exists so the
    /// subset's own tests can run against files. It spawns nothing (every
    /// test here is a utility the subset runs itself), and it is not a
    /// backend: the shipped host is in iron-shell, with its gate.
    struct TestHost;

    impl Os for TestHost {
        fn metadata(&self, path: &str) -> Result<Meta, OsError> {
            std::fs::metadata(path)
                .map(|m| std_meta(&m))
                .map_err(|e| std_err(&e))
        }
        fn symlink_metadata(&self, path: &str) -> Result<Meta, OsError> {
            std::fs::symlink_metadata(path)
                .map(|m| std_meta(&m))
                .map_err(|e| std_err(&e))
        }
        fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, OsError> {
            let rd = std::fs::read_dir(path).map_err(|e| std_err(&e))?;
            Ok(rd
                .filter_map(|e| e.ok())
                .map(|e| DirEntry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    kind: e.file_type().ok().map(|ft| {
                        if ft.is_dir() {
                            FileKind::Dir
                        } else if ft.is_symlink() {
                            FileKind::Symlink
                        } else {
                            FileKind::File
                        }
                    }),
                })
                .collect())
        }
        fn read_file(&self, path: &str) -> Result<Vec<u8>, OsError> {
            std::fs::read(path).map_err(|e| std_err(&e))
        }
        fn write_file(&self, path: &str, data: &[u8], append: bool) -> Result<(), OsError> {
            use std::io::Write;
            let res = if append {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .and_then(|mut f| f.write_all(data))
            } else {
                std::fs::write(path, data)
            };
            res.map_err(|e| std_err(&e))
        }
        fn remove(&self, path: &str, dir: bool) -> Result<(), OsError> {
            if dir {
                std::fs::remove_dir(path)
            } else {
                std::fs::remove_file(path)
            }
            .map_err(|e| std_err(&e))
        }
        fn create_dir(&self, path: &str) -> Result<(), OsError> {
            std::fs::create_dir(path).map_err(|e| std_err(&e))
        }
        fn rename(&self, from: &str, to: &str) -> Result<(), OsError> {
            std::fs::rename(from, to).map_err(|e| std_err(&e))
        }
        fn read_link(&self, path: &str) -> Result<String, OsError> {
            std::fs::read_link(path)
                .map(|p| p.to_string_lossy().into_owned())
                .map_err(|e| std_err(&e))
        }
        fn canonicalize(&self, path: &str) -> Result<String, OsError> {
            std::fs::canonicalize(path)
                .map(|p| p.to_string_lossy().into_owned())
                .map_err(|e| std_err(&e))
        }
        fn current_dir(&self) -> Result<String, OsError> {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .map_err(|e| std_err(&e))
        }
        fn set_current_dir(&self, path: &str) -> Result<(), OsError> {
            std::env::set_current_dir(path).map_err(|e| std_err(&e))
        }
        fn env_var(&self, name: &str) -> Option<String> {
            std::env::var(name).ok()
        }
        fn process_id(&self) -> u32 {
            std::process::id()
        }
        fn run_program(&self, _spec: Spawn) -> Result<Captured, SpawnError> {
            Err(SpawnError::Start(OsError {
                kind: ErrorKind::NotFound,
                errno: None,
                message: "no programs in the test host".into(),
            }))
        }
        fn write_stdout(&self, bytes: &[u8]) {
            use std::io::Write;
            let _ = std::io::stdout().write_all(bytes);
        }
        fn write_stderr(&self, bytes: &[u8]) {
            use std::io::Write;
            let _ = std::io::stderr().write_all(bytes);
        }
        fn read_stdin_line(&self, _limit: usize) -> Option<Vec<u8>> {
            None
        }
        fn read_stdin_to_end(&self, _out: &mut Vec<u8>) {}
        fn now(&self) -> Timestamp {
            Timestamp { secs: 0, nanos: 0 }
        }
        fn format_local(&self, _at: Timestamp, _format: &str) -> Option<String> {
            None
        }
        fn locale_available(&self, name: &str) -> bool {
            name.starts_with("en_US")
        }
        fn gate(&self) -> &dyn Gate {
            self
        }
    }

    impl Gate for TestHost {
        fn agent_mode(&self) -> bool {
            false
        }
        fn admit_exec(&self, _argv: &str) -> Result<(), Refusal> {
            Ok(())
        }
        fn guarded_write(
            &self,
            os: &dyn Os,
            path: &str,
            data: &[u8],
            append: bool,
        ) -> Result<(), WriteError> {
            os.write_file(path, data, append).map_err(WriteError::Io)
        }
        fn guarded_remove(
            &self,
            os: &dyn Os,
            path: &str,
            recursive: bool,
        ) -> Result<(), WriteError> {
            remove_tree(os, path, recursive).map_err(WriteError::Io)
        }
        fn guarded_copy(&self, os: &dyn Os, plan: &[CopyStep]) -> Result<(), WriteError> {
            copy_steps(os, plan).map_err(WriteError::Io)
        }
        fn guarded_create_dirs(&self, os: &dyn Os, dirs: &[String]) -> Result<(), WriteError> {
            dirs.iter()
                .try_for_each(|d| os.create_dir(d))
                .map_err(WriteError::Io)
        }
        fn guarded_rename(&self, os: &dyn Os, from: &str, to: &str) -> Result<(), WriteError> {
            os.rename(from, to).map_err(WriteError::Io)
        }
    }

    fn std_err(e: &std::io::Error) -> OsError {
        OsError {
            kind: match e.kind() {
                std::io::ErrorKind::NotFound => ErrorKind::NotFound,
                std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
                std::io::ErrorKind::AlreadyExists => ErrorKind::AlreadyExists,
                _ => ErrorKind::Other,
            },
            errno: e.raw_os_error(),
            message: e.to_string(),
        }
    }

    fn std_meta(m: &std::fs::Metadata) -> Meta {
        Meta {
            kind: if m.is_dir() {
                FileKind::Dir
            } else if m.file_type().is_symlink() {
                FileKind::Symlink
            } else {
                FileKind::File
            },
            len: m.len(),
            mode: 0,
            uid: 0,
            gid: 0,
            nlink: 1,
            blocks: 0,
            mtime: 0,
        }
    }

    /// Run in-process and capture stdout and the status.
    fn sh(src: &str) -> (String, i32) {
        sh_on(&TestHost, src)
    }

    /// [`sh`], against the host `os`.
    fn sh_on(os: &dyn Os, src: &str) -> (String, i32) {
        let list = parse(src).unwrap_or_else(|Unsupported(w)| panic!("{src}: {w}"));
        let mut shell = Shell {
            vars: BTreeMap::new(),
            exported: BTreeSet::new(),
            last: 0,
            glob_refused: None,
            os,
        };
        let mut stdin = Input::buf(Vec::new());
        let mut sink = Sink::new(os, ErrMode::Null);
        let c = match shell.run_list(&list, &mut stdin, &mut sink) {
            Flow::Next(c) | Flow::Exit(c) => c,
            _ => 0,
        };
        (String::from_utf8_lossy(&sink.out).into_owned(), c)
    }

    /// seq gathers its whole output and printf allocates its padding before
    /// writing, so both refuse sizes past their caps instead of growing.
    #[test]
    fn seq_and_printf_refuse_sizes_past_their_caps() {
        assert_eq!(sh("seq 3"), ("1\n2\n3\n".into(), 0));
        assert_eq!(sh("seq 5 -2 1"), ("5\n3\n1\n".into(), 0));
        assert_eq!(sh("seq 3 1"), ("".into(), 0));
        assert_eq!(sh("seq 1 99999999999"), ("".into(), 1));
        assert_eq!(
            sh("seq 9223372036854775806 9223372036854775807"),
            ("9223372036854775806\n9223372036854775807\n".into(), 0)
        );
        assert_eq!(sh("printf '%5s|' ab"), ("   ab|".into(), 0));
        assert_eq!(sh("printf '%99999999s' x"), ("".into(), 1));
        assert_eq!(sh("printf '%999999999999999999999999s' x"), ("".into(), 1));
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

    #[cfg(target_os = "linux")]
    #[test]
    fn a_locale_that_was_never_generated_sorts_by_bytes() {
        // glibc's `ls` and `sort` fall back to bytes when LANG names a locale
        // the host lacks; ae-disposable sets en_US.UTF-8 without generating it.
        // The test host loads only `en_US*` locales.
        static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("LC_ALL", "xx_NOWHERE.UTF-8");
        let unavailable = locale_collates(&TestHost);
        std::env::set_var("LC_ALL", "en_US.UTF-8");
        let available = locale_collates(&TestHost);
        std::env::remove_var("LC_ALL");
        assert!(!unavailable);
        assert!(available);
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
        // The ordering itself, independent of whether this host generated
        // en_US.UTF-8.
        v.sort_by(|a, b| collate_in(true, a, b));
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
        let found = glob_paths(&TestHost, &format!("{r}/**/*.rs"));
        let names: Vec<&str> = found
            .iter()
            .map(|p| p.rsplit('/').next().unwrap())
            .collect();
        assert_eq!(names, vec!["deep.rs", "mid.rs", "top.rs"], "{found:?}");
        // Without globstar, `**` is `*`: one level only.
        let shell = glob_expand(&TestHost, &format!("{r}/**/*.rs")).expect("under the cap");
        assert_eq!(shell.len(), 1, "{shell:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An in-memory host. Its paths exist nowhere on disk, so a utility that
    /// reached `std::fs` instead of the trait would find nothing and print
    /// something else. `calls` records which trait methods ran.
    #[derive(Default)]
    struct FakeOs {
        files: alloc::collections::BTreeMap<String, Vec<u8>>,
        dirs: alloc::collections::BTreeSet<String>,
        env: BTreeMap<String, String>,
        calls: std::cell::RefCell<Vec<&'static str>>,
    }

    impl FakeOs {
        fn note(&self, what: &'static str) {
            self.calls.borrow_mut().push(what);
        }
        fn parent(path: &str) -> String {
            match path.rsplit_once('/') {
                Some(("", _)) => "/".into(),
                Some((p, _)) => p.into(),
                None => ".".into(),
            }
        }
        fn name(path: &str) -> String {
            path.rsplit('/').next().unwrap_or(path).into()
        }
        fn missing() -> OsError {
            OsError {
                kind: ErrorKind::NotFound,
                errno: Some(2),
                message: "No such file or directory (os error 2)".into(),
            }
        }
    }

    impl Os for FakeOs {
        fn env_vars(&self) -> Vec<(String, String)> {
            self.env
                .iter()
                .map(|(n, v)| (n.clone(), v.clone()))
                .collect()
        }

        fn metadata(&self, path: &str) -> Result<Meta, OsError> {
            self.note("metadata");
            let meta = |kind, len| Meta {
                kind,
                len,
                // Executable, so `which` finds the fake programs.
                mode: 0o755,
                uid: 0,
                gid: 0,
                nlink: 1,
                blocks: 0,
                mtime: 0,
            };
            if self.dirs.contains(path) {
                return Ok(meta(FileKind::Dir, 4096));
            }
            match self.files.get(path) {
                Some(d) => Ok(meta(FileKind::File, d.len() as u64)),
                None => Err(Self::missing()),
            }
        }
        fn symlink_metadata(&self, path: &str) -> Result<Meta, OsError> {
            self.note("symlink_metadata");
            self.metadata(path)
        }
        fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, OsError> {
            self.note("read_dir");
            if !self.dirs.contains(path) {
                return Err(Self::missing());
            }
            let mut out = Vec::new();
            for d in self.dirs.iter().filter(|p| Self::parent(p) == path) {
                out.push(DirEntry {
                    name: Self::name(d),
                    kind: Some(FileKind::Dir),
                });
            }
            for f in self.files.keys().filter(|p| Self::parent(p) == path) {
                out.push(DirEntry {
                    name: Self::name(f),
                    kind: Some(FileKind::File),
                });
            }
            Ok(out)
        }
        fn read_file(&self, path: &str) -> Result<Vec<u8>, OsError> {
            self.note("read_file");
            if self.dirs.contains(path) {
                return Err(OsError {
                    kind: ErrorKind::IsADirectory,
                    errno: Some(21),
                    message: "Is a directory (os error 21)".into(),
                });
            }
            self.files.get(path).cloned().ok_or_else(Self::missing)
        }
        fn write_file(&self, _path: &str, _data: &[u8], _append: bool) -> Result<(), OsError> {
            self.note("write_file");
            Err(OsError {
                kind: ErrorKind::PermissionDenied,
                errno: Some(13),
                message: "Permission denied (os error 13)".into(),
            })
        }
        fn read_link(&self, _path: &str) -> Result<String, OsError> {
            self.note("read_link");
            Err(Self::missing())
        }
        fn canonicalize(&self, path: &str) -> Result<String, OsError> {
            self.note("canonicalize");
            if self.dirs.contains(path) || self.files.contains_key(path) {
                Ok(path.into())
            } else {
                Err(Self::missing())
            }
        }
        fn current_dir(&self) -> Result<String, OsError> {
            self.note("current_dir");
            Ok("/w".into())
        }
        fn set_current_dir(&self, _path: &str) -> Result<(), OsError> {
            self.note("set_current_dir");
            Err(Self::missing())
        }
        fn env_var(&self, name: &str) -> Option<String> {
            self.note("env_var");
            self.env.get(name).cloned()
        }
        fn process_id(&self) -> u32 {
            4242
        }
        fn run_program(&self, _spec: Spawn) -> Result<Captured, SpawnError> {
            self.note("run_program");
            Err(SpawnError::Start(Self::missing()))
        }
        fn write_stdout(&self, _bytes: &[u8]) {}
        fn write_stderr(&self, _bytes: &[u8]) {}
        fn read_stdin_line(&self, _limit: usize) -> Option<Vec<u8>> {
            None
        }
        fn read_stdin_to_end(&self, _out: &mut Vec<u8>) {}
        fn now(&self) -> Timestamp {
            Timestamp { secs: 0, nanos: 0 }
        }
        fn format_local(&self, _at: Timestamp, _format: &str) -> Option<String> {
            None
        }
    }

    /// `ls` and `cat` run through the trait: their answers come from the fake,
    /// and the calls they make are the trait's, not `std::fs`'s.
    #[test]
    fn ls_and_cat_run_through_the_os_trait() {
        let mut os = FakeOs::default();
        os.dirs.insert("/w".into());
        os.dirs.insert("/w/sub".into());
        os.files.insert("/w/a.txt".into(), b"hello\n".to_vec());
        os.files.insert("/w/b.txt".into(), b"second\n".to_vec());

        assert_eq!(
            sh_on(&os, "ls /w; cat /w/a.txt"),
            ("a.txt\nb.txt\nsub\nhello\n".into(), 0)
        );
        assert_eq!(sh_on(&os, "cat /w/missing.txt").1, 1);
        assert_eq!(sh_on(&os, "ls /nowhere").1, 2);

        let calls = os.calls.borrow();
        assert!(calls.contains(&"read_dir"), "{calls:?}");
        assert!(calls.contains(&"read_file"), "{calls:?}");
        assert!(!calls.contains(&"run_program"), "{calls:?}");
    }

    /// A writable in-memory host for the file utilities. A path maps to its
    /// contents, or to `None` for a directory. `allow` is the gate's answer:
    /// when false, the gate refuses every change. `admitted` counts the
    /// changes the gate was asked about.
    struct MemOs {
        tree: core::cell::RefCell<BTreeMap<String, Option<Vec<u8>>>>,
        allow: bool,
        admitted: core::cell::Cell<u32>,
    }

    impl MemOs {
        fn new(allow: bool, entries: &[(&str, Option<&str>)]) -> Self {
            let tree = entries
                .iter()
                .map(|(p, c)| (p.to_string(), c.map(|s| s.as_bytes().to_vec())))
                .collect();
            MemOs {
                tree: core::cell::RefCell::new(tree),
                allow,
                admitted: Default::default(),
            }
        }
        fn has(&self, path: &str) -> bool {
            self.tree.borrow().contains_key(path)
        }
        fn kind(&self, path: &str) -> Option<FileKind> {
            self.tree.borrow().get(path).map(|c| match c {
                None => FileKind::Dir,
                Some(_) => FileKind::File,
            })
        }
        fn contents(&self, path: &str) -> Option<Vec<u8>> {
            self.tree.borrow().get(path).cloned().flatten()
        }
        fn parent_is_dir(&self, path: &str) -> bool {
            self.kind(&parent_of(path)) == Some(FileKind::Dir)
        }
        fn admit(&self) -> Result<(), WriteError> {
            self.admitted.set(self.admitted.get() + 1);
            match self.allow {
                true => Ok(()),
                false => Err(WriteError::Refused(Refusal {
                    message: "refused by test gate".into(),
                    code: 7,
                })),
            }
        }
    }

    impl Os for MemOs {
        fn metadata(&self, path: &str) -> Result<Meta, OsError> {
            let kind = self.kind(path).ok_or_else(FakeOs::missing)?;
            let len = self.contents(path).map_or(0, |d| d.len() as u64);
            Ok(Meta {
                kind,
                len,
                mode: 0,
                uid: 0,
                gid: 0,
                nlink: 1,
                blocks: 0,
                mtime: 0,
            })
        }
        fn symlink_metadata(&self, path: &str) -> Result<Meta, OsError> {
            self.metadata(path)
        }
        fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, OsError> {
            if self.kind(path) != Some(FileKind::Dir) {
                return Err(FakeOs::missing());
            }
            let tree = self.tree.borrow();
            Ok(tree
                .keys()
                .filter(|k| k.as_str() != path && parent_of(k) == path)
                .map(|k| DirEntry {
                    name: base_name(k).into(),
                    kind: self.kind(k),
                })
                .collect())
        }
        fn read_file(&self, path: &str) -> Result<Vec<u8>, OsError> {
            match self.tree.borrow().get(path) {
                Some(Some(d)) => Ok(d.clone()),
                Some(None) => Err(OsError {
                    kind: ErrorKind::IsADirectory,
                    errno: Some(21),
                    message: "Is a directory (os error 21)".into(),
                }),
                None => Err(FakeOs::missing()),
            }
        }
        fn write_file(&self, path: &str, data: &[u8], append: bool) -> Result<(), OsError> {
            if self.kind(path) == Some(FileKind::Dir) {
                return Err(OsError {
                    kind: ErrorKind::IsADirectory,
                    errno: Some(21),
                    message: "Is a directory (os error 21)".into(),
                });
            }
            if !self.parent_is_dir(path) {
                return Err(FakeOs::missing());
            }
            let mut tree = self.tree.borrow_mut();
            let d = tree
                .entry(path.into())
                .or_insert_with(|| Some(Vec::new()))
                .get_or_insert_with(Vec::new);
            if append {
                d.extend_from_slice(data);
            } else {
                *d = data.to_vec();
            }
            Ok(())
        }
        fn remove(&self, path: &str, dir: bool) -> Result<(), OsError> {
            let prefix = format!("{path}/");
            let mut tree = self.tree.borrow_mut();
            let is_dir = match tree.get(path) {
                None => return Err(FakeOs::missing()),
                Some(c) => c.is_none(),
            };
            if is_dir != dir {
                let kind = if dir {
                    ErrorKind::NotADirectory
                } else {
                    ErrorKind::IsADirectory
                };
                return Err(OsError {
                    kind,
                    errno: None,
                    message: String::from("wrong kind"),
                });
            }
            if is_dir && tree.keys().any(|k| k.starts_with(&prefix)) {
                return Err(OsError {
                    kind: ErrorKind::Other,
                    errno: Some(39),
                    message: "Directory not empty (os error 39)".into(),
                });
            }
            tree.remove(path);
            Ok(())
        }
        fn create_dir(&self, path: &str) -> Result<(), OsError> {
            if self.has(path) {
                return Err(OsError {
                    kind: ErrorKind::AlreadyExists,
                    errno: Some(17),
                    message: "File exists (os error 17)".into(),
                });
            }
            if !self.parent_is_dir(path) {
                return Err(FakeOs::missing());
            }
            self.tree.borrow_mut().insert(path.into(), None);
            Ok(())
        }
        fn rename(&self, from: &str, to: &str) -> Result<(), OsError> {
            if !self.has(from) || !self.parent_is_dir(to) {
                return Err(FakeOs::missing());
            }
            let mut tree = self.tree.borrow_mut();
            let under = format!("{from}/");
            let moved: Vec<String> = tree
                .keys()
                .filter(|k| k.as_str() == from || k.starts_with(&under))
                .cloned()
                .collect();
            for k in moved {
                let v = tree.remove(&k).unwrap_or(None);
                tree.insert(format!("{to}{}", &k[from.len()..]), v);
            }
            Ok(())
        }
        fn read_link(&self, _path: &str) -> Result<String, OsError> {
            Err(FakeOs::missing())
        }
        fn canonicalize(&self, path: &str) -> Result<String, OsError> {
            if self.has(path) {
                Ok(path.into())
            } else {
                Err(FakeOs::missing())
            }
        }
        fn current_dir(&self) -> Result<String, OsError> {
            Ok("/w".into())
        }
        fn set_current_dir(&self, _path: &str) -> Result<(), OsError> {
            Err(FakeOs::missing())
        }
        fn env_var(&self, _name: &str) -> Option<String> {
            None
        }
        fn process_id(&self) -> u32 {
            1
        }
        fn run_program(&self, _spec: Spawn) -> Result<Captured, SpawnError> {
            Err(SpawnError::Start(FakeOs::missing()))
        }
        fn write_stdout(&self, _bytes: &[u8]) {}
        fn write_stderr(&self, _bytes: &[u8]) {}
        fn read_stdin_line(&self, _limit: usize) -> Option<Vec<u8>> {
            None
        }
        fn read_stdin_to_end(&self, _out: &mut Vec<u8>) {}
        fn now(&self) -> Timestamp {
            Timestamp { secs: 0, nanos: 0 }
        }
        fn format_local(&self, _at: Timestamp, _format: &str) -> Option<String> {
            None
        }
        fn gate(&self) -> &dyn Gate {
            self
        }
    }

    impl Gate for MemOs {
        fn guarded_write(
            &self,
            os: &dyn Os,
            path: &str,
            data: &[u8],
            append: bool,
        ) -> Result<(), WriteError> {
            self.admit()?;
            os.write_file(path, data, append).map_err(WriteError::Io)
        }

        fn guarded_remove(
            &self,
            os: &dyn Os,
            path: &str,
            recursive: bool,
        ) -> Result<(), WriteError> {
            self.admit()?;
            remove_tree(os, path, recursive).map_err(WriteError::Io)
        }
        fn guarded_copy(&self, os: &dyn Os, plan: &[CopyStep]) -> Result<(), WriteError> {
            self.admit()?;
            copy_steps(os, plan).map_err(WriteError::Io)
        }
        fn guarded_create_dirs(&self, os: &dyn Os, dirs: &[String]) -> Result<(), WriteError> {
            self.admit()?;
            dirs.iter()
                .try_for_each(|d| os.create_dir(d))
                .map_err(WriteError::Io)
        }
        fn guarded_rename(&self, os: &dyn Os, from: &str, to: &str) -> Result<(), WriteError> {
            self.admit()?;
            os.rename(from, to).map_err(WriteError::Io)
        }
    }

    /// [`sh_on`] that returns what the shell printed to standard error, and
    /// the status, on a host of its own.
    fn sh_err(os: &dyn Os, src: &str) -> (String, i32) {
        let list = parse(src).unwrap_or_else(|Unsupported(w)| panic!("{src}: {w}"));
        let mut shell = Shell {
            vars: BTreeMap::new(),
            exported: BTreeSet::new(),
            last: 0,
            glob_refused: None,
            os,
        };
        let mut stdin = Input::buf(Vec::new());
        let mut sink = Sink::new(os, ErrMode::Capture);
        let c = match shell.run_list(&list, &mut stdin, &mut sink) {
            Flow::Next(c) | Flow::Exit(c) => c,
            _ => 0,
        };
        (String::from_utf8_lossy(&sink.err).into_owned(), c)
    }

    #[test]
    fn rm_removes_files_and_trees_through_the_gate() {
        let os = MemOs::new(
            true,
            &[
                ("/w", None),
                ("/w/a", Some("A")),
                ("/w/d", None),
                ("/w/d/x", Some("X")),
                ("/w/d/s", None),
                ("/w/d/s/y", Some("Y")),
            ],
        );
        assert_eq!(sh_err(&os, "rm /w/a"), (String::new(), 0));
        assert!(!os.has("/w/a"));
        assert_eq!(
            sh_err(&os, "rm /w/d"),
            ("rm: cannot remove '/w/d': Is a directory\n".into(), 1)
        );
        assert!(os.has("/w/d/s/y"));
        // A tree is one decision: one admission for the whole of `rm -r`.
        let before = os.admitted.get();
        assert_eq!(sh_err(&os, "rm -rf /w/d"), (String::new(), 0));
        assert_eq!(os.admitted.get(), before + 1);
        assert!(!os.has("/w/d") && !os.has("/w/d/s/y"));
    }

    #[test]
    fn rm_error_paths_match_gnu() {
        let os = MemOs::new(true, &[("/w", None), ("/w/a", Some("A"))]);
        assert_eq!(
            sh_err(&os, "rm"),
            (
                "rm: missing operand\nTry 'rm --help' for more information.\n".into(),
                1
            )
        );
        assert_eq!(sh_err(&os, "rm -f"), (String::new(), 0));
        assert_eq!(
            sh_err(&os, "rm /w/nope"),
            (
                "rm: cannot remove '/w/nope': No such file or directory\n".into(),
                1
            )
        );
        assert_eq!(sh_err(&os, "rm -f /w/nope"), (String::new(), 0));
        // A flag the subset does not run declines to the external program,
        // which this host does not have, and the file stays.
        assert_eq!(sh_err(&os, "rm -i /w/a").1, 126);
        assert!(os.has("/w/a"));
    }

    #[test]
    fn cp_copies_files_and_trees_through_the_gate() {
        let os = MemOs::new(
            true,
            &[
                ("/w", None),
                ("/w/a", Some("A")),
                ("/w/d", None),
                ("/w/d/x", Some("X")),
                ("/w/d/s", None),
                ("/w/d/s/y", Some("Y")),
            ],
        );
        assert_eq!(sh_err(&os, "cp /w/a /w/b"), (String::new(), 0));
        assert_eq!(os.contents("/w/b"), Some(b"A".to_vec()));
        assert_eq!(sh_err(&os, "cp /w/a /w/d"), (String::new(), 0));
        assert_eq!(os.contents("/w/d/a"), Some(b"A".to_vec()));
        assert_eq!(sh_err(&os, "cp -r /w/d /w/e"), (String::new(), 0));
        assert_eq!(os.contents("/w/e/s/y"), Some(b"Y".to_vec()));
        assert_eq!(
            sh_err(&os, "cp /w/d /w/f"),
            (
                "cp: -r not specified; omitting directory '/w/d'\n".into(),
                1
            )
        );
        assert!(!os.has("/w/f"));
    }

    #[test]
    fn cp_error_paths_match_gnu() {
        let os = MemOs::new(
            true,
            &[
                ("/w", None),
                ("/w/a", Some("A")),
                ("/w/b", Some("B")),
                ("/w/d", None),
            ],
        );
        assert_eq!(
            sh_err(&os, "cp"),
            (
                "cp: missing file operand\nTry 'cp --help' for more information.\n".into(),
                1
            )
        );
        assert_eq!(
            sh_err(&os, "cp /w/a"),
            (
                "cp: missing destination file operand after '/w/a'\nTry 'cp --help' for more information.\n"
                    .into(),
                1
            )
        );
        assert_eq!(
            sh_err(&os, "cp /w/nope /w/x"),
            (
                "cp: cannot stat '/w/nope': No such file or directory\n".into(),
                1
            )
        );
        assert_eq!(
            sh_err(&os, "cp /w/a /w/b /w/c"),
            ("cp: target '/w/c' is not a directory\n".into(), 1)
        );
        assert_eq!(
            sh_err(&os, "cp /w/a /w/a"),
            ("cp: '/w/a' and '/w/a' are the same file\n".into(), 1)
        );
        assert_eq!(
            sh_err(&os, "cp -r /w/d /w/d/in"),
            (
                "cp: cannot copy a directory, '/w/d', into itself, '/w/d/in'\n".into(),
                1
            )
        );
        assert!(!os.has("/w/c") && !os.has("/w/d/in"));
        // `-a` is not one of the flags this subset runs.
        assert_eq!(sh_err(&os, "cp -a /w/a /w/q").1, 126);
    }

    #[test]
    fn mv_renames_and_moves_into_directories() {
        let os = MemOs::new(
            true,
            &[
                ("/w", None),
                ("/w/a", Some("A")),
                ("/w/d", None),
                ("/w/d/x", Some("X")),
            ],
        );
        assert_eq!(sh_err(&os, "mv /w/a /w/z"), (String::new(), 0));
        assert!(!os.has("/w/a"));
        assert_eq!(os.contents("/w/z"), Some(b"A".to_vec()));
        assert_eq!(sh_err(&os, "mv /w/z /w/d"), (String::new(), 0));
        assert_eq!(os.contents("/w/d/z"), Some(b"A".to_vec()));
        assert_eq!(sh_err(&os, "mv /w/d /w/e"), (String::new(), 0));
        assert_eq!(os.contents("/w/e/x"), Some(b"X".to_vec()));
    }

    #[test]
    fn mv_error_paths_match_gnu() {
        let os = MemOs::new(true, &[("/w", None), ("/w/a", Some("A"))]);
        assert_eq!(
            sh_err(&os, "mv"),
            (
                "mv: missing file operand\nTry 'mv --help' for more information.\n".into(),
                1
            )
        );
        assert_eq!(
            sh_err(&os, "mv /w/a"),
            (
                "mv: missing destination file operand after '/w/a'\nTry 'mv --help' for more information.\n"
                    .into(),
                1
            )
        );
        assert_eq!(
            sh_err(&os, "mv /w/nope /w/x"),
            (
                "mv: cannot stat '/w/nope': No such file or directory\n".into(),
                1
            )
        );
        assert_eq!(
            sh_err(&os, "mv /w/a /w/a"),
            ("mv: '/w/a' and '/w/a' are the same file\n".into(), 1)
        );
        assert_eq!(
            sh_err(&os, "mv /w/a /nodir/x"),
            (
                "mv: cannot move '/w/a' to '/nodir/x': No such file or directory\n".into(),
                1
            )
        );
        assert!(os.has("/w/a"));
        // `-f` is not one of the flags this subset runs.
        assert_eq!(sh_err(&os, "mv -f /w/a /w/q").1, 126);
    }

    #[test]
    fn mkdir_makes_directories_and_parents() {
        let os = MemOs::new(true, &[("/w", None), ("/w/a", Some("A"))]);
        assert_eq!(sh_err(&os, "mkdir /w/n"), (String::new(), 0));
        assert_eq!(os.kind("/w/n"), Some(FileKind::Dir));
        assert_eq!(
            sh_err(&os, "mkdir /w/n"),
            (
                "mkdir: cannot create directory '/w/n': File exists\n".into(),
                1
            )
        );
        assert_eq!(
            sh_err(&os, "mkdir /w/q/r"),
            (
                "mkdir: cannot create directory '/w/q/r': No such file or directory\n".into(),
                1
            )
        );
        assert_eq!(sh_err(&os, "mkdir -p /w/q/r"), (String::new(), 0));
        assert_eq!(os.kind("/w/q/r"), Some(FileKind::Dir));
        assert_eq!(sh_err(&os, "mkdir -p /w/n /w/q"), (String::new(), 0));
        assert_eq!(
            sh_err(&os, "mkdir -p /w/a/b"),
            (
                "mkdir: cannot create directory '/w/a/b': Not a directory\n".into(),
                1
            )
        );
        assert_eq!(
            sh_err(&os, "mkdir"),
            (
                "mkdir: missing operand\nTry 'mkdir --help' for more information.\n".into(),
                1
            )
        );
        // `-m` is not one of the flags this subset runs.
        assert_eq!(sh_err(&os, "mkdir -m 700 /w/m").1, 126);
        assert!(!os.has("/w/m"));
    }

    /// A gate that refuses every change changes nothing, and each utility
    /// prints the refusal and returns its code. This is the test that a
    /// utility which writes without asking the gate fails.
    #[test]
    fn a_gate_that_refuses_changes_nothing() {
        let os = MemOs::new(
            false,
            &[
                ("/w", None),
                ("/w/a", Some("A")),
                ("/w/d", None),
                ("/w/d/x", Some("X")),
            ],
        );
        for src in [
            "rm /w/a",
            "rm -r /w/d",
            "cp /w/a /w/b",
            "cp -r /w/d /w/e",
            "mv /w/a /w/b",
            "mkdir /w/n",
            "mkdir -p /w/p/q",
        ] {
            assert_eq!(
                sh_err(&os, src),
                ("refused by test gate\n".into(), 7),
                "{src}"
            );
        }
        assert_eq!(os.tree.borrow().len(), 4);
        assert!(os.has("/w/a") && os.has("/w/d/x"));
    }

    /// With the default gate, which refuses everything in every mode, the
    /// utilities write nothing, and the Os defaults say they are unsupported.
    #[test]
    fn a_default_gate_refuses_every_change() {
        let mut os = FakeOs::default();
        os.dirs.insert("/w".into());
        os.files.insert("/w/a.txt".into(), b"A".to_vec());
        for src in [
            "rm /w/a.txt",
            "rm -r /w",
            "cp /w/a.txt /w/b",
            "mv /w/a.txt /w/b",
            "mkdir /w/n",
            "mkdir -p /w/p/q",
        ] {
            let (err, c) = sh_err(&os, src);
            assert_eq!(c, 126, "{src}");
            assert!(err.contains("no policy gate"), "{src}: {err}");
        }
        let calls = os.calls.borrow();
        for write in [
            "remove",
            "create_dir",
            "rename",
            "write_file",
            "run_program",
        ] {
            assert!(!calls.contains(&write), "{calls:?}");
        }
        drop(calls);
        let fake = FakeOs::default();
        for e in [
            fake.remove("x", false).err(),
            fake.create_dir("x").err(),
            fake.rename("x", "y").err(),
        ] {
            assert_eq!(
                e.map(|e| e.message),
                Some("not supported by this host".into())
            );
        }
    }

    /// `tr`: each expected value is what GNU coreutils `tr` prints for the same
    /// command, `printf '...' | tr ...`, under the same name in the comment.
    #[test]
    fn tr_translates_deletes_and_squeezes_like_gnu() {
        // tr a-y b-z: h->i e->f l->m o->p.
        assert_eq!(sh("printf hello | tr a-y b-z"), ("ifmmp".into(), 0));
        // tr '[:lower:]' '[:upper:]'.
        assert_eq!(
            sh("printf 'Hello World' | tr '[:lower:]' '[:upper:]'"),
            ("HELLO WORLD".into(), 0)
        );
        // tr -d '[:digit:]'.
        assert_eq!(sh("printf 'a1b2c3' | tr -d '[:digit:]'"), ("abc".into(), 0));
        assert_eq!(sh("printf hello | tr -d l"), ("heo".into(), 0));
        // tr -s 'a-d': runs of one byte collapse.
        assert_eq!(sh("printf aabbccdd | tr -s 'a-d'"), ("abcd".into(), 0));
        assert_eq!(
            sh("printf 'hello   world' | tr -s ' '"),
            ("hello world".into(), 0)
        );
        // tr -s ' ' _: translate, then squeeze SET2.
        assert_eq!(sh("printf 'a  b' | tr -s ' ' _"), ("a_b".into(), 0));
        // tr abc x: SET2 is padded with its last byte.
        assert_eq!(sh("printf abc | tr abc x"), ("xxx".into(), 0));
        // tr -cd a-z: delete the complement of a-z.
        assert_eq!(sh("printf 'a1b!' | tr -cd a-z"), ("ab".into(), 0));
        // tr -c a-z _: every complement byte maps to _.
        assert_eq!(sh("printf 'a-b' | tr -c a-z _"), ("a_b".into(), 0));
        // tr '\n' ' ': the escape is read by tr, not by the shell.
        assert_eq!(sh("printf 'a\\nb' | tr '\\n' ' '"), ("a b".into(), 0));
        // GNU: tr z-a b => "tr: range-endpoints of 'z-a' are in reverse
        // collating sequence order", status 1.
        assert_eq!(
            sh_err(&TestHost, "printf x | tr z-a b"),
            (
                "tr: range-endpoints of 'z-a' are in reverse collating sequence order\n".into(),
                1
            )
        );
        // GNU: tr => "tr: missing operand" and the usage hint.
        assert_eq!(
            sh_err(&TestHost, "tr"),
            (
                "tr: missing operand\nTry 'tr --help' for more information.\n".into(),
                1
            )
        );
    }

    /// `tee` writes each file through the gate, and `-a` appends. A refusal
    /// writes nothing and returns the gate's status.
    #[test]
    fn tee_copies_stdin_through_the_gate() {
        let os = MemOs::new(true, &[("/w", None), ("/w/a", Some("A"))]);
        // GNU: printf hi | tee /w/t => "hi" on stdout, and the file holds "hi".
        assert_eq!(sh_on(&os, "printf hi | tee /w/t"), ("hi".into(), 0));
        assert_eq!(os.contents("/w/t"), Some(b"hi".to_vec()));
        assert_eq!(sh_on(&os, "printf more | tee -a /w/t"), ("more".into(), 0));
        assert_eq!(os.contents("/w/t"), Some(b"himore".to_vec()));
        assert_eq!(sh_on(&os, "printf new | tee /w/t"), ("new".into(), 0));
        assert_eq!(os.contents("/w/t"), Some(b"new".to_vec()));
        assert_eq!(
            sh_err(&os, "printf x | tee /nodir/t"),
            ("tee: /nodir/t: No such file or directory\n".into(), 1)
        );
        // A refusal from the gate writes nothing, and its status is the gate's.
        let refuse = MemOs::new(false, &[("/w", None)]);
        assert_eq!(
            sh_err(&refuse, "tee /w/t"),
            ("refused by test gate\n".into(), 7)
        );
        assert!(!refuse.has("/w/t"));
    }

    /// `touch` creates an absent file through the gate. An existing file is
    /// left unchanged and succeeds, since the trait cannot set a time. `-c`
    /// creates nothing.
    #[test]
    fn touch_creates_through_the_gate_and_leaves_existing_files() {
        let os = MemOs::new(true, &[("/w", None), ("/w/a", Some("A"))]);
        // GNU: touch /w/n creates an empty file.
        assert_eq!(sh_err(&os, "touch /w/n"), (String::new(), 0));
        assert_eq!(os.contents("/w/n"), Some(Vec::new()));
        assert_eq!(sh_err(&os, "touch /w/a"), (String::new(), 0));
        assert_eq!(os.contents("/w/a"), Some(b"A".to_vec()));
        // GNU: touch -c /w/q creates nothing and succeeds.
        assert_eq!(sh_err(&os, "touch -c /w/q"), (String::new(), 0));
        assert!(!os.has("/w/q"));
        // GNU: touch /nodir/x => "touch: cannot touch '/nodir/x': No such file
        // or directory", status 1.
        assert_eq!(
            sh_err(&os, "touch /nodir/x"),
            (
                "touch: cannot touch '/nodir/x': No such file or directory\n".into(),
                1
            )
        );
        assert_eq!(
            sh_err(&os, "touch"),
            (
                "touch: missing file operand\nTry 'touch --help' for more information.\n".into(),
                1
            )
        );
        // A refusal from the gate creates nothing.
        let refuse = MemOs::new(false, &[("/w", None)]);
        assert_eq!(
            sh_err(&refuse, "touch /w/n"),
            ("refused by test gate\n".into(), 7)
        );
        assert!(!refuse.has("/w/n"));
    }

    /// `which` prints the first match on PATH for each name. A name with a `/`
    /// is checked as it is. A name that is not found sets status 1.
    #[test]
    fn which_finds_the_first_executable_on_path() {
        let mut os = FakeOs::default();
        os.dirs.insert("/nope".into());
        os.dirs.insert("/usr".into());
        os.dirs.insert("/usr/bin".into());
        os.files.insert("/usr/bin/tool".into(), b"x".to_vec());
        os.env.insert("PATH".into(), "/nope:/usr/bin".into());
        // GNU: which tool => /usr/bin/tool.
        assert_eq!(sh_on(&os, "which tool"), ("/usr/bin/tool\n".into(), 0));
        assert_eq!(
            sh_on(&os, "which tool missing"),
            ("/usr/bin/tool\n".into(), 1)
        );
        assert_eq!(
            sh_on(&os, "which /usr/bin/tool"),
            ("/usr/bin/tool\n".into(), 0)
        );
        assert_eq!(sh_on(&os, "which missing"), (String::new(), 1));
    }

    /// `env` lists the environment sorted, with `-i`, `-u` and assignments
    /// applied; with a command, it runs it the way `NAME=VALUE cmd` runs.
    #[test]
    fn env_lists_and_runs_like_a_prefix_assignment() {
        let mut os = FakeOs::default();
        os.env.insert("HOME".into(), "/h".into());
        os.env.insert("PATH".into(), "/bin".into());
        assert_eq!(sh_on(&os, "env"), ("HOME=/h\nPATH=/bin\n".into(), 0));
        assert_eq!(sh_on(&os, "env -u HOME"), ("PATH=/bin\n".into(), 0));
        assert_eq!(sh_on(&os, "env -i A=1 B=2"), ("A=1\nB=2\n".into(), 0));
        assert_eq!(
            sh_on(&os, "X=2 env"),
            ("HOME=/h\nPATH=/bin\nX=2\n".into(), 0)
        );
        // FakeOs has no gate, so a program it would run is refused, and the
        // prefix form and the env form are refused alike.
        let refused = "ish: no policy gate on this host: refusing\n";
        assert_eq!(sh_err(&os, "env A=1 nosuch"), (refused.into(), 126));
        assert_eq!(sh_err(&os, "A=1 nosuch"), (refused.into(), 126));
        // -i and -u cannot be applied to a command on this host.
        assert_eq!(
            sh_err(&os, "env -i nosuch"),
            (
                "env: -i and -u are not supported with a command on this host\n".into(),
                125
            )
        );
    }

    /// `readlink -f` canonicalizes, and a missing last component is allowed,
    /// as GNU allows it. A missing directory is an error.
    #[test]
    fn readlink_canonicalizes_like_gnu() {
        let mut os = FakeOs::default();
        os.dirs.insert("/w".into());
        os.files.insert("/w/a.txt".into(), b"A".to_vec());
        assert_eq!(sh_on(&os, "readlink -f /w/a.txt"), ("/w/a.txt\n".into(), 0));
        // GNU: readlink -f /w/new => /w/new (the last component need not exist).
        assert_eq!(sh_on(&os, "readlink -f /w/new"), ("/w/new\n".into(), 0));
        assert_eq!(
            sh_err(&os, "readlink -f /nope/x"),
            ("readlink: /nope/x: No such file or directory\n".into(), 1)
        );
        assert_eq!(
            sh_err(&os, "readlink"),
            (
                "readlink: missing operand\nTry 'readlink --help' for more information.\n".into(),
                1
            )
        );
    }

    /// `stat -c`: each specifier the metadata has, GNU's text for it. `%i` is
    /// refused, since the metadata has no inode, and so is the default layout.
    #[test]
    fn stat_formats_what_the_metadata_has() {
        let mut os = FakeOs::default();
        os.dirs.insert("/w".into());
        os.files.insert("/w/a.txt".into(), b"hello\n".to_vec());
        os.files.insert("/w/empty".into(), Vec::new());
        // GNU: stat -c '%n %s %F %a %h' a.txt, for a 6-byte file, mode 755, one link.
        assert_eq!(
            sh_on(&os, "stat -c '%n %s %F %a %h' /w/a.txt"),
            ("/w/a.txt 6 regular file 755 1\n".into(), 0)
        );
        assert_eq!(
            sh_on(&os, "stat -c %F /w/empty"),
            ("regular empty file\n".into(), 0)
        );
        assert_eq!(sh_on(&os, "stat -c %F /w"), ("directory\n".into(), 0));
        // A user or group without a name prints its number; %% is a percent.
        assert_eq!(
            sh_on(&os, "stat -c '%U:%G %Y %u %%' /w/a.txt"),
            ("0:0 0 0 %\n".into(), 0)
        );
        assert_eq!(
            sh_err(&os, "stat -c %i /w/a.txt"),
            ("stat: unsupported format specifier '%i'\n".into(), 1)
        );
        assert_eq!(
            sh_err(&os, "stat /w/a.txt"),
            (
                "stat: this host needs -c FORMAT; the default layout is not reproduced\n".into(),
                1
            )
        );
        assert_eq!(
            sh_err(&os, "stat -c %s /w/nope"),
            (
                "stat: cannot stat '/w/nope': No such file or directory\n".into(),
                1
            )
        );
        assert_eq!(
            sh_err(&os, "stat"),
            (
                "stat: missing operand\nTry 'stat --help' for more information.\n".into(),
                1
            )
        );
    }

    /// `yes` writes its line until the bound. The bound is what stops it:
    /// the sink cannot say that a reader has gone.
    #[test]
    fn yes_stops_at_its_bound() {
        let mut out = Vec::new();
        yes_into(&mut out, b"y\n", 5);
        assert_eq!(out, b"y\ny\ny\n".to_vec());
        let mut out = Vec::new();
        yes_into(&mut out, b"ab\n", 0);
        assert!(out.is_empty());
    }

    /// `expr`: each value and status is GNU's for the same arguments.
    #[test]
    fn expr_computes_like_gnu() {
        assert_eq!(sh("expr 1 + 2"), ("3\n".into(), 0));
        assert_eq!(sh("expr 7 - 10"), ("-3\n".into(), 0));
        assert_eq!(sh("expr 2 + 3 '*' 4"), ("14\n".into(), 0));
        assert_eq!(sh("expr 1 + 2 '*' 3"), ("7\n".into(), 0));
        assert_eq!(sh("expr '(' 2 + 3 ')' '*' 4"), ("20\n".into(), 0));
        assert_eq!(sh("expr 7 / 2"), ("3\n".into(), 0));
        assert_eq!(sh("expr 7 % 3"), ("1\n".into(), 0));
        assert_eq!(sh("expr -7 % 3"), ("-1\n".into(), 0));
        // A null or zero value is status 1.
        assert_eq!(sh("expr 0"), ("0\n".into(), 1));
        assert_eq!(sh("expr ''"), ("\n".into(), 1));
        assert_eq!(sh("expr 1 = 1"), ("1\n".into(), 0));
        assert_eq!(sh("expr abc '!=' abc"), ("0\n".into(), 1));
        assert_eq!(sh("expr 10 '>' 9"), ("1\n".into(), 0));
        assert_eq!(sh("expr 10 '<' 9"), ("0\n".into(), 1));
        assert_eq!(sh("expr abc '<' abd"), ("1\n".into(), 0));
        assert_eq!(sh("expr 1 '|' 2"), ("1\n".into(), 0));
        assert_eq!(sh("expr 0 '|' 2"), ("2\n".into(), 0));
        assert_eq!(sh("expr 0 '&' 2"), ("0\n".into(), 1));
        assert_eq!(sh("expr 3 '&' 2"), ("3\n".into(), 0));
        assert_eq!(sh("expr length abcdef"), ("6\n".into(), 0));
        assert_eq!(sh("expr substr abcdef 2 3"), ("bcd\n".into(), 0));
        assert_eq!(sh("expr substr abcdef 0 3"), ("\n".into(), 1));
        assert_eq!(sh("expr index abcdef cf"), ("3\n".into(), 0));
        assert_eq!(sh("expr index abc xyz"), ("0\n".into(), 1));
        // Anchored: the match length, or 0; with \( \) the group.
        assert_eq!(sh("expr abcdef : abc"), ("3\n".into(), 0));
        assert_eq!(sh("expr abcdef : b"), ("0\n".into(), 1));
        assert_eq!(sh("expr abc : 'a\\(b\\)c'"), ("b\n".into(), 0));
        assert_eq!(sh("expr abc : 'x\\(b\\)'"), ("\n".into(), 1));
        // Errors: status 2, the message on stderr.
        assert_eq!(
            sh_err(&TestHost, "expr 1 / 0"),
            ("expr: division by zero\n".into(), 2)
        );
        assert_eq!(
            sh_err(&TestHost, "expr abc + 1"),
            ("expr: non-integer argument\n".into(), 2)
        );
        assert_eq!(
            sh_err(&TestHost, "expr 1 +"),
            ("expr: syntax error\n".into(), 2)
        );
        assert_eq!(
            sh_err(&TestHost, "expr 1 2"),
            ("expr: syntax error\n".into(), 2)
        );
    }

    /// `date -u` and the pure UTC formatter. Expected values are GNU's
    /// `date -u -d @SECS +FORMAT` for the same seconds.
    #[test]
    fn date_utc_matches_gnu() {
        let at = |secs: i64| Timestamp { secs, nanos: 0 };
        // GNU: date -u -d @0 => "Thu Jan  1 00:00:00 UTC 1970".
        assert_eq!(
            format_utc(at(0), DATE_DEFAULT),
            Some("Thu Jan  1 00:00:00 UTC 1970".into())
        );
        // GNU: date -u -d @1000000000 => "Sun Sep  9 01:46:40 UTC 2001".
        assert_eq!(
            format_utc(at(1_000_000_000), DATE_DEFAULT),
            Some("Sun Sep  9 01:46:40 UTC 2001".into())
        );
        // GNU: date -u -d @951782400 +'%Y-%m-%d %H:%M:%S %j %a' (a leap day).
        assert_eq!(
            format_utc(at(951_782_400), "%Y-%m-%d %H:%M:%S %j %a"),
            Some("2000-02-29 00:00:00 060 Tue".into())
        );
        // GNU: date -u -d @1700000000 +'%F %T %u %I%p'.
        assert_eq!(
            format_utc(at(1_700_000_000), "%F %T %u %I%p"),
            Some("2023-11-14 22:13:20 2 10PM".into())
        );
        // A conversion this formatter does not know, a trailing %, and a year
        // it does not render all decline.
        assert_eq!(format_utc(at(0), "%Q"), None);
        assert_eq!(format_utc(at(0), "100%"), None);
        assert_eq!(format_utc(at(1_000_000_000_000_000), "%Y"), None);
        // Through the shell, with the test host's clock at 0.
        assert_eq!(sh("date -u +%Y-%m-%d"), ("1970-01-01\n".into(), 0));
        assert_eq!(sh("date -u"), ("Thu Jan  1 00:00:00 UTC 1970\n".into(), 0));
    }

    /// A session keeps variables, exports and `$?` from one run to the next,
    /// and says whether `exit` ran. `run_on` builds a fresh session, so it
    /// keeps none of them.
    #[test]
    fn a_session_keeps_state_between_runs() {
        let mut s = Session::new(&TestHost);
        assert_eq!(
            s.run("x=5"),
            Some(Outcome {
                status: 0,
                exited: false
            })
        );
        assert_eq!(s.run("false").map(|o| o.status), Some(1));
        // An empty line keeps `$?`, as an interactive shell does.
        assert_eq!(s.run("").map(|o| o.status), Some(1));
        assert_eq!(s.run("[ $? -eq 1 ]").map(|o| o.status), Some(0));
        assert_eq!(s.run("[ \"$x\" = 5 ]").map(|o| o.status), Some(0));
        assert_eq!(
            s.run("exit 3"),
            Some(Outcome {
                status: 3,
                exited: true
            })
        );
        // A line the subset does not run changes nothing, and reports nothing.
        assert_eq!(s.run("f() { :; }"), None);
        assert_eq!(s.run("[ \"$x\" = 5 ]").map(|o| o.status), Some(0));
        assert_eq!(
            Session::new(&TestHost)
                .run("[ \"$x\" = 5 ]")
                .map(|o| o.status),
            Some(1)
        );
        assert_eq!(run_on(&TestHost, "[ \"$x\" = 5 ]"), Some(1));
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
