//! A bounded, in-process awk subset behind `awk` and `awk_process`.
//!
//! The program is lexed and parsed once into a small tree and then run over
//! the input lines by the interpreter below. No process is started and no
//! shell is involved. Anything outside the subset is refused with a coded
//! E_BAD_ARG that names the construct, so a caller never receives a silently
//! different result.
//!
//! Limits: program 64 KiB, input 64 MiB or 10,000,000 lines, output 64 MiB,
//! a step budget checked on every evaluated node, 16 MiB per string value or
//! record, 256 MiB across all variables, and nesting 200 deep in the parser
//! and in any one expression tree (E_BUDGET_EXCEEDED). The step budget alone
//! does not bound memory: `{s = s s}` doubles a string per line in a handful
//! of steps, so sizes are bounded where values are made.

use crate::safety::{arg_err, budget_exceeded};
use anyhow::Result;
use regex::Regex;
use std::cmp::Ordering;

const MAX_PROGRAM_BYTES: usize = 64 * 1024;
const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_INPUT_LINES: usize = 10_000_000;
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_STEPS: u64 = 200_000_000;
const MAX_FIELD: usize = 1_000_000;
const MAX_PRINTF_PRECISION: usize = 100;
const MAX_VALUE_BYTES: usize = 16 * 1024 * 1024;
const MAX_VAR_BYTES: usize = 256 * 1024 * 1024;
const MAX_DEPTH: usize = 200;

const KEYWORDS: &[&str] = &[
    "BEGIN", "END", "if", "else", "while", "for", "do", "print", "printf", "return", "function",
    "getline", "next", "nextfile", "exit", "delete", "in", "break", "continue",
];
const BUILTINS: &[&str] = &["length", "substr", "index", "tolower", "toupper"];
/// Real awk builtins outside the subset; naming them gives a clearer refusal.
const OUTSIDE_BUILTINS: &[&str] = &[
    "split", "sub", "gsub", "match", "sprintf", "int", "sin", "cos", "atan2", "exp", "log", "sqrt",
    "rand", "srand", "system", "close", "fflush", "systime", "strftime",
];
/// Special variables that the subset does not implement.
const OUTSIDE_VARS: &[&str] = &[
    "RS",
    "ORS",
    "FNR",
    "FILENAME",
    "SUBSEP",
    "CONVFMT",
    "OFMT",
    "RSTART",
    "RLENGTH",
    "ENVIRON",
    "ARGC",
    "ARGV",
    "IGNORECASE",
];
const CLASSES: &[&str] = &[
    "alpha", "digit", "alnum", "upper", "lower", "space", "blank", "punct", "print", "graph",
    "cntrl", "xdigit",
];
/// Longest operators first, so the first match is the longest one.
const OPS: &[&str] = &[
    "**=", "+=", "-=", "*=", "/=", "%=", "^=", "==", "<=", ">=", "!=", "!~", "&&", "||", "++",
    "--", "**", ">>", "+", "-", "*", "/", "%", "^", "!", "<", ">", "=", "~", "?", ":", ",", ";",
    "(", ")", "{", "}", "[", "]", "|", "$",
];

fn refuse(what: &str) -> anyhow::Error {
    arg_err(format!(
        "awk: unsupported construct: {what}; the awk subset is BEGIN/END, pattern-action rules, \
         if/else, print, printf, assignment, fields, NR/NF/FS/OFS, arithmetic, comparison, \
         concatenation, and length/substr/index/tolower/toupper"
    ))
}

/// Runs `program` over the lines of `text` with field separator `fs` and
/// returns the output as lines. The caller decides how to present them.
pub fn run(program: &str, text: &str, fs: &str) -> Result<Vec<String>> {
    if program.len() > MAX_PROGRAM_BYTES {
        return Err(budget_exceeded(
            "awk",
            format!(
                "program is {} bytes; the limit is {MAX_PROGRAM_BYTES}",
                program.len()
            ),
            "shorten the program",
        ));
    }
    if text.len() > MAX_INPUT_BYTES {
        return Err(budget_exceeded(
            "awk",
            format!(
                "input is {} bytes; the limit is {MAX_INPUT_BYTES}",
                text.len()
            ),
            "read less input at once",
        ));
    }
    let prog = parse(program)?;
    let mut rt = Runtime::new(&prog, fs)?;
    for stmt in &prog.begin {
        rt.exec(stmt)?;
    }
    if !prog.rules.is_empty() || !prog.end.is_empty() {
        for line in text.lines() {
            rt.nr += 1.0;
            if rt.nr > MAX_INPUT_LINES as f64 {
                return Err(budget_exceeded(
                    "awk",
                    format!("input exceeds {MAX_INPUT_LINES} lines"),
                    "read fewer lines at once",
                ));
            }
            rt.set_record(line);
            for rule in &prog.rules {
                let selected = match &rule.pattern {
                    Some(pattern) => {
                        let value = rt.eval(pattern)?;
                        truthy(&value)
                    }
                    None => true,
                };
                if selected {
                    match &rule.action {
                        Some(stmts) => {
                            for stmt in stmts {
                                rt.exec(stmt)?;
                            }
                        }
                        None => rt.emit_record()?,
                    }
                }
            }
        }
    }
    for stmt in &prog.end {
        rt.exec(stmt)?;
    }
    Ok(split_lines(&rt.out))
}

/// Output lines: a trailing newline ends the last line and adds no empty one.
fn split_lines(out: &str) -> Vec<String> {
    if out.is_empty() {
        return Vec::new();
    }
    let body = out.strip_suffix('\n').unwrap_or(out);
    body.split('\n').map(str::to_string).collect()
}

// ---------------------------------------------------------------- lexer

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Regex(String),
    Name(String),
    Op(&'static str),
    Newline,
    Eof,
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    /// Whitespace immediately precedes this token.
    spaced: bool,
}

static EOF_TOK: Tok = Tok::Eof;

fn prev_is_operand(prev: Option<&Tok>) -> bool {
    match prev {
        Some(Tok::Num(_)) | Some(Tok::Str(_)) | Some(Tok::Regex(_)) => true,
        Some(Tok::Name(name)) => !KEYWORDS.contains(&name.as_str()),
        Some(Tok::Op(op)) => matches!(*op, ")" | "]" | "++" | "--"),
        _ => false,
    }
}

fn starts_with_at(chars: &[char], at: usize, pattern: &str) -> bool {
    pattern
        .chars()
        .enumerate()
        .all(|(k, p)| chars.get(at + k) == Some(&p))
}

fn lex(src: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = src.chars().collect();
    let mut toks: Vec<Token> = Vec::new();
    let mut i = 0;
    let mut spaced = false;
    while i < chars.len() {
        let c = chars[i];
        if c == ' ' || c == '\t' || c == '\r' {
            spaced = true;
            i += 1;
            continue;
        }
        if c == '\\' && chars.get(i + 1) == Some(&'\n') {
            spaced = true;
            i += 2;
            continue;
        }
        if c == '#' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        let tok = if c == '\n' {
            i += 1;
            Tok::Newline
        } else if c == '"' {
            i += 1;
            let mut s = String::new();
            loop {
                let Some(&d) = chars.get(i) else {
                    return Err(arg_err("awk: unterminated string"));
                };
                i += 1;
                match d {
                    '"' => break,
                    '\n' => return Err(arg_err("awk: unterminated string")),
                    '\\' => {
                        let Some(&e) = chars.get(i) else {
                            return Err(arg_err("awk: unterminated string"));
                        };
                        i += 1;
                        s.push(match e {
                            '"' => '"',
                            '\\' => '\\',
                            '/' => '/',
                            'n' => '\n',
                            't' => '\t',
                            'r' => '\r',
                            'a' => '\x07',
                            'b' => '\x08',
                            'f' => '\x0c',
                            'v' => '\x0b',
                            other => {
                                return Err(arg_err(format!(
                                    "awk: unsupported string escape \\{other}"
                                )))
                            }
                        });
                    }
                    other => s.push(other),
                }
            }
            Tok::Str(s)
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if chars.get(i) == Some(&'.') {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if matches!(chars.get(i), Some('e') | Some('E')) {
                let mut j = i + 1;
                if matches!(chars.get(j), Some('+') | Some('-')) {
                    j += 1;
                }
                if chars.get(j).is_some_and(|d| d.is_ascii_digit()) {
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                    i = j;
                }
            }
            let text: String = chars[start..i].iter().collect();
            Tok::Num(
                text.parse::<f64>()
                    .map_err(|_| arg_err("awk: malformed number"))?,
            )
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            Tok::Name(chars[start..i].iter().collect())
        } else if c == '/' && !prev_is_operand(toks.last().map(|t| &t.tok)) {
            i += 1;
            let mut s = String::new();
            loop {
                let Some(&d) = chars.get(i) else {
                    return Err(arg_err("awk: unterminated regex"));
                };
                i += 1;
                match d {
                    '/' => break,
                    '\n' => return Err(arg_err("awk: unterminated regex")),
                    '\\' => {
                        let Some(&e) = chars.get(i) else {
                            return Err(arg_err("awk: unterminated regex"));
                        };
                        i += 1;
                        if e == '\n' {
                            return Err(arg_err("awk: unterminated regex"));
                        }
                        s.push('\\');
                        s.push(e);
                    }
                    other => s.push(other),
                }
            }
            Tok::Regex(s)
        } else {
            let Some(op) = OPS.iter().copied().find(|op| starts_with_at(&chars, i, op)) else {
                return Err(arg_err(format!("awk: unexpected character {c:?}")));
            };
            i += op.chars().count();
            Tok::Op(op)
        };
        toks.push(Token { tok, spaced });
        spaced = false;
    }
    Ok(toks)
}

// ---------------------------------------------------------------- regex

#[derive(Clone, Copy, PartialEq)]
enum Prev {
    Start,
    Atom,
    Open,
    Alt,
    Quant,
    Anchor,
}

fn push_literal(out: &mut String, c: char) {
    if c.is_ascii_alphanumeric() || c == ' ' {
        out.push(c);
    } else {
        out.push_str(&regex::escape(&c.to_string()));
    }
}

fn class_char(c: char) -> String {
    if c.is_ascii_alphanumeric() {
        c.to_string()
    } else {
        format!("\\x{{{:X}}}", c as u32)
    }
}

/// Parses a bracket expression; `i` is the index just after `[`.
fn bracket(chars: &[char], i: &mut usize) -> Result<String> {
    let mut negate = false;
    if chars.get(*i) == Some(&'^') {
        negate = true;
        *i += 1;
    }
    let mut items = String::new();
    let mut first = true;
    loop {
        let Some(&c) = chars.get(*i) else {
            return Err(refuse("unterminated bracket expression"));
        };
        if c == ']' && !first {
            *i += 1;
            break;
        }
        first = false;
        if c == '\\' {
            return Err(refuse("backslash inside a bracket expression"));
        }
        if c == '[' {
            if chars.get(*i + 1) == Some(&':') {
                let rest = &chars[*i + 2..];
                let Some(end) = rest.windows(2).position(|w| w[0] == ':' && w[1] == ']') else {
                    return Err(refuse("unterminated character class"));
                };
                let name: String = rest[..end].iter().collect();
                if !CLASSES.contains(&name.as_str()) {
                    return Err(refuse(&format!("character class [:{name}:]")));
                }
                items.push_str(&format!("[:{name}:]"));
                *i += 2 + end + 2;
                continue;
            }
            if matches!(chars.get(*i + 1), Some('.') | Some('=')) {
                return Err(refuse("collating element in a bracket expression"));
            }
        }
        *i += 1;
        if chars.get(*i) == Some(&'-') && chars.get(*i + 1).is_some_and(|&h| h != ']') {
            let hi = chars[*i + 1];
            if hi == '\\' || hi == '[' {
                return Err(refuse("range endpoint in a bracket expression"));
            }
            if hi < c {
                return Err(refuse("reversed range in a bracket expression"));
            }
            items.push_str(&class_char(c));
            items.push('-');
            items.push_str(&class_char(hi));
            *i += 2;
        } else {
            items.push_str(&class_char(c));
        }
    }
    Ok(format!("[{}{}]", if negate { "^" } else { "" }, items))
}

/// Parses an interval after `{`; `start` is the index just after it.
fn interval(chars: &[char], start: usize) -> Result<(String, usize)> {
    let Some(rel) = chars[start..].iter().position(|&c| c == '}') else {
        return Err(refuse("unterminated regex interval"));
    };
    let body: String = chars[start..start + rel].iter().collect();
    let end = start + rel + 1;
    let valid = |s: &str| !s.is_empty() && s.len() <= 4 && s.bytes().all(|b| b.is_ascii_digit());
    let mut parts = body.splitn(2, ',');
    let lo = parts.next().unwrap_or("");
    if !valid(lo) {
        return Err(refuse("regex interval"));
    }
    let quant = match parts.next() {
        None => format!("{{{lo}}}"),
        Some("") => format!("{{{lo},}}"),
        Some(hi) if valid(hi) => {
            if hi.parse::<u32>().unwrap_or(0) < lo.parse::<u32>().unwrap_or(0) {
                return Err(refuse("reversed regex interval"));
            }
            format!("{{{lo},{hi}}}")
        }
        Some(_) => return Err(refuse("regex interval")),
    };
    Ok((quant, end))
}

/// Translates POSIX ERE, as awk uses it, into the `regex` crate's syntax.
/// Backreferences, gawk escapes (\y, \<, \s and the like), collating
/// elements and stacked quantifiers are refused rather than guessed at.
fn translate_ere(src: &str) -> Result<String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut prev = Prev::Start;
    let mut depth = 0usize;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        match c {
            '\\' => {
                let Some(&e) = chars.get(i) else {
                    return Err(refuse("regex ends in a backslash"));
                };
                i += 1;
                let lit = match e {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    'f' => '\x0c',
                    'v' => '\x0b',
                    'a' => '\x07',
                    e if !e.is_ascii_alphanumeric() => e,
                    e => return Err(refuse(&format!("regex escape \\{e}"))),
                };
                push_literal(&mut out, lit);
                prev = Prev::Atom;
            }
            '[' => {
                let class = bracket(&chars, &mut i)?;
                out.push_str(&class);
                prev = Prev::Atom;
            }
            '.' => {
                out.push_str("(?s:.)");
                prev = Prev::Atom;
            }
            '^' | '$' => {
                out.push(c);
                prev = Prev::Anchor;
            }
            '(' => {
                out.push_str("(?:");
                depth += 1;
                prev = Prev::Open;
            }
            ')' => {
                if depth == 0 || prev == Prev::Open {
                    return Err(refuse("unbalanced or empty regex group"));
                }
                depth -= 1;
                out.push(')');
                prev = Prev::Atom;
            }
            '|' => {
                out.push('|');
                prev = Prev::Alt;
            }
            '*' | '+' | '?' => {
                if prev != Prev::Atom {
                    return Err(refuse(&format!("regex quantifier {c} without one atom")));
                }
                out.push(c);
                prev = Prev::Quant;
            }
            '{' => {
                let (quant, next) = interval(&chars, i)?;
                if prev != Prev::Atom {
                    return Err(refuse("regex interval without one atom"));
                }
                out.push_str(&quant);
                i = next;
                prev = Prev::Quant;
            }
            other => {
                push_literal(&mut out, other);
                prev = Prev::Atom;
            }
        }
    }
    if depth != 0 {
        return Err(refuse("unbalanced regex group"));
    }
    Ok(out)
}

fn compile_regex(src: &str) -> Result<Regex> {
    let translated = translate_ere(src)?;
    Regex::new(&translated).map_err(|e| arg_err(format!("awk: invalid regex: {e}")))
}

// ---------------------------------------------------------------- AST

#[derive(Clone, Copy, Debug, PartialEq)]
enum Var {
    User(usize),
    Nr,
    Nf,
    Fs,
    Ofs,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Bin {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Builtin {
    Length,
    Substr,
    Index,
    ToLower,
    ToUpper,
}

fn builtin_arity(b: Builtin) -> (usize, usize) {
    match b {
        Builtin::Length => (0, 1),
        Builtin::Substr => (2, 3),
        Builtin::Index => (2, 2),
        Builtin::ToLower | Builtin::ToUpper => (1, 1),
    }
}

#[derive(Debug)]
enum Expr {
    Num(f64),
    Str(String),
    /// A bare /re/: matches against $0.
    MatchRecord(usize),
    Var(Var),
    Field(Box<Expr>),
    Neg(Box<Expr>),
    Plus(Box<Expr>),
    Not(Box<Expr>),
    Bin(Bin, Box<Expr>, Box<Expr>),
    Concat(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Match {
        negate: bool,
        subject: Box<Expr>,
        re: usize,
    },
    Assign {
        target: Box<Expr>,
        op: Option<Bin>,
        value: Box<Expr>,
    },
    IncDec {
        target: Box<Expr>,
        delta: f64,
        pre: bool,
    },
    Call(Builtin, Vec<Expr>),
}

#[derive(Debug)]
enum Stmt {
    Print(Vec<Expr>),
    Printf(Vec<Expr>),
    Expr(Expr),
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    Block(Vec<Stmt>),
}

struct Rule {
    pattern: Option<Expr>,
    action: Option<Vec<Stmt>>,
}

struct Program {
    begin: Vec<Stmt>,
    rules: Vec<Rule>,
    end: Vec<Stmt>,
    regexes: Vec<Regex>,
    names: Vec<String>,
}

fn is_lvalue(e: &Expr) -> bool {
    matches!(e, Expr::Var(_) | Expr::Field(_))
}

// ---------------------------------------------------------------- parser

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    /// Set while parsing print arguments: an unparenthesized `>` is a
    /// redirection, not a comparison.
    no_gt: bool,
    names: Vec<String>,
    regexes: Vec<Regex>,
    /// Current recursion depth of statements and expressions.
    depth: usize,
}

fn too_deep() -> anyhow::Error {
    budget_exceeded(
        "awk",
        format!("the program nests deeper than {MAX_DEPTH}"),
        "split the expression into assignments",
    )
}

/// The depth of an expression tree, measured without recursion so a long
/// operator chain cannot overflow the stack while it is being measured.
fn tree_depth(root: &Expr) -> usize {
    let mut deepest = 0;
    let mut stack: Vec<(&Expr, usize)> = vec![(root, 1)];
    while let Some((e, d)) = stack.pop() {
        deepest = deepest.max(d);
        let children: Vec<&Expr> = match e {
            Expr::Num(_) | Expr::Str(_) | Expr::MatchRecord(_) | Expr::Var(_) => Vec::new(),
            Expr::Field(x) | Expr::Neg(x) | Expr::Plus(x) | Expr::Not(x) => vec![x],
            Expr::Bin(_, a, b) | Expr::Concat(a, b) | Expr::And(a, b) | Expr::Or(a, b) => {
                vec![a, b]
            }
            Expr::Match { subject, .. } => vec![subject],
            Expr::Assign { target, value, .. } => vec![target, value],
            Expr::IncDec { target, .. } => vec![target],
            Expr::Call(_, args) => args.iter().collect(),
        };
        stack.extend(children.into_iter().map(|child| (child, d + 1)));
    }
    deepest
}

fn check_expr(e: &Expr) -> Result<()> {
    if tree_depth(e) > MAX_DEPTH {
        return Err(too_deep());
    }
    Ok(())
}

/// Statement nesting is already bounded by the parser's depth guard.
fn check_stmts(stmts: &[Stmt]) -> Result<()> {
    for stmt in stmts {
        match stmt {
            Stmt::Print(args) | Stmt::Printf(args) => args.iter().try_for_each(check_expr)?,
            Stmt::Expr(e) => check_expr(e)?,
            Stmt::If(cond, then, otherwise) => {
                check_expr(cond)?;
                check_stmts(then)?;
                check_stmts(otherwise)?;
            }
            Stmt::Block(body) => check_stmts(body)?,
        }
    }
    Ok(())
}

fn parse(src: &str) -> Result<Program> {
    let toks = lex(src)?;
    let mut parser = Parser {
        toks,
        pos: 0,
        no_gt: false,
        names: Vec::new(),
        regexes: Vec::new(),
        depth: 0,
    };
    parser.program()
}

impl Parser {
    fn tok(&self) -> &Tok {
        match self.toks.get(self.pos) {
            Some(t) => &t.tok,
            None => &EOF_TOK,
        }
    }

    fn is_op(&self, op: &str) -> bool {
        matches!(self.tok(), Tok::Op(o) if *o == op)
    }

    fn is_name(&self, name: &str) -> bool {
        matches!(self.tok(), Tok::Name(n) if n.as_str() == name)
    }

    fn syntax(&self, near: &str) -> anyhow::Error {
        arg_err(format!("awk: syntax error near {near}"))
    }

    fn expect_op(&mut self, op: &str) -> Result<()> {
        if self.is_op(op) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.syntax(op))
        }
    }

    fn skip_newlines(&mut self) {
        while *self.tok() == Tok::Newline {
            self.pos += 1;
        }
    }

    fn skip_terminators(&mut self) {
        while *self.tok() == Tok::Newline || self.is_op(";") {
            self.pos += 1;
        }
    }

    fn at_stmt_end(&self) -> bool {
        match self.tok() {
            Tok::Newline | Tok::Eof => true,
            Tok::Op(o) => *o == ";" || *o == "}",
            Tok::Name(n) => n.as_str() == "else",
            _ => false,
        }
    }

    fn end_simple(&self) -> Result<()> {
        if self.at_stmt_end() {
            return Ok(());
        }
        if self.is_op("|") {
            return Err(refuse("pipes (|)"));
        }
        if self.is_op("?") || self.is_op(":") {
            return Err(refuse("conditional expression ?:"));
        }
        Err(self.syntax("statement continues"))
    }

    /// Runs `f` with unparenthesized `>` treated as a comparison again.
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let saved = self.no_gt;
        self.no_gt = false;
        let result = f(self);
        self.no_gt = saved;
        result
    }

    fn intern(&mut self, name: &str) -> usize {
        match self.names.iter().position(|n| n == name) {
            Some(index) => index,
            None => {
                self.names.push(name.to_string());
                self.names.len() - 1
            }
        }
    }

    fn regex(&mut self, src: &str) -> Result<usize> {
        let re = compile_regex(src)?;
        self.regexes.push(re);
        Ok(self.regexes.len() - 1)
    }

    fn program(&mut self) -> Result<Program> {
        let mut begin = Vec::new();
        let mut end = Vec::new();
        let mut rules = Vec::new();
        let mut items = 0usize;
        loop {
            self.skip_terminators();
            if *self.tok() == Tok::Eof {
                break;
            }
            items += 1;
            if self.is_name("BEGIN") {
                self.pos += 1;
                self.skip_newlines();
                begin.extend(self.block()?);
            } else if self.is_name("END") {
                self.pos += 1;
                self.skip_newlines();
                end.extend(self.block()?);
            } else if self.is_op("{") {
                let action = self.block()?;
                rules.push(Rule {
                    pattern: None,
                    action: Some(action),
                });
            } else {
                let pattern = self.expr()?;
                if self.is_op(",") {
                    return Err(refuse("range pattern (pattern,pattern)"));
                }
                let action = if self.is_op("{") {
                    Some(self.block()?)
                } else {
                    None
                };
                if action.is_none()
                    && !matches!(self.tok(), Tok::Newline | Tok::Eof)
                    && !self.is_op(";")
                {
                    return Err(self.syntax("pattern"));
                }
                rules.push(Rule {
                    pattern: Some(pattern),
                    action,
                });
            }
        }
        if items == 0 {
            return Err(arg_err("awk: program is empty"));
        }
        check_stmts(&begin)?;
        check_stmts(&end)?;
        for rule in &rules {
            if let Some(pattern) = &rule.pattern {
                check_expr(pattern)?;
            }
            if let Some(action) = &rule.action {
                check_stmts(action)?;
            }
        }
        Ok(Program {
            begin,
            rules,
            end,
            regexes: std::mem::take(&mut self.regexes),
            names: std::mem::take(&mut self.names),
        })
    }

    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect_op("{")?;
        let mut stmts = Vec::new();
        loop {
            self.skip_terminators();
            if self.is_op("}") {
                self.pos += 1;
                return Ok(stmts);
            }
            if *self.tok() == Tok::Eof {
                return Err(arg_err("awk: missing } at end of program"));
            }
            stmts.push(self.stmt()?);
        }
    }

    /// Runs `f` one level deeper, refusing programs that nest past
    /// `MAX_DEPTH` before they can exhaust the parser's stack.
    fn deeper<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let saved = self.depth;
        self.link()?;
        let result = f(self);
        // Restores links taken by operator chains inside `f` as well.
        self.depth = saved;
        result
    }

    /// One more level of nesting: a recursive call, or one more operator in
    /// a left-associative chain, each of which deepens the tree by one.
    fn link(&mut self) -> Result<()> {
        if self.depth >= MAX_DEPTH {
            return Err(too_deep());
        }
        self.depth += 1;
        Ok(())
    }

    fn stmt(&mut self) -> Result<Stmt> {
        self.deeper(Self::stmt_inner)
    }

    fn stmt_inner(&mut self) -> Result<Stmt> {
        if self.is_op("{") {
            return Ok(Stmt::Block(self.block()?));
        }
        if self.is_name("if") {
            self.pos += 1;
            self.expect_op("(")?;
            let cond = self.nested(|p| p.expr())?;
            self.expect_op(")")?;
            self.skip_newlines();
            let then = vec![self.stmt()?];
            let save = self.pos;
            self.skip_terminators();
            let otherwise = if self.is_name("else") {
                self.pos += 1;
                self.skip_newlines();
                vec![self.stmt()?]
            } else {
                self.pos = save;
                Vec::new()
            };
            return Ok(Stmt::If(cond, then, otherwise));
        }
        if self.is_name("print") || self.is_name("printf") {
            let printf = self.is_name("printf");
            self.pos += 1;
            if self.is_op(">") || self.is_op(">>") {
                return Err(refuse("output redirection (print > file)"));
            }
            if self.is_op("|") {
                return Err(refuse("output pipes (print | command)"));
            }
            let args = if self.at_stmt_end() {
                Vec::new()
            } else {
                self.print_args()?
            };
            if self.is_op(">") || self.is_op(">>") {
                return Err(refuse("output redirection (print > file)"));
            }
            if self.is_op("|") {
                return Err(refuse("output pipes (print | command)"));
            }
            if printf && args.is_empty() {
                return Err(self.syntax("printf without a format"));
            }
            self.end_simple()?;
            return Ok(if printf {
                Stmt::Printf(args)
            } else {
                Stmt::Print(args)
            });
        }
        let e = self.expr()?;
        self.end_simple()?;
        Ok(Stmt::Expr(e))
    }

    fn print_args(&mut self) -> Result<Vec<Expr>> {
        if self.is_op("(") {
            let save = self.pos;
            self.pos += 1;
            let grouped = self.nested(|p| {
                let mut list = vec![p.expr()?];
                while p.is_op(",") {
                    p.pos += 1;
                    p.skip_newlines();
                    list.push(p.expr()?);
                }
                p.expect_op(")")?;
                Ok(list)
            });
            if let Ok(list) = grouped {
                if self.at_stmt_end() {
                    return Ok(list);
                }
            }
            self.pos = save;
        }
        let saved = self.no_gt;
        self.no_gt = true;
        let result = self.expr_list();
        self.no_gt = saved;
        result
    }

    fn expr_list(&mut self) -> Result<Vec<Expr>> {
        let mut list = vec![self.expr()?];
        while self.is_op(",") {
            self.pos += 1;
            self.skip_newlines();
            list.push(self.expr()?);
        }
        Ok(list)
    }

    fn expr(&mut self) -> Result<Expr> {
        self.deeper(Self::expr_inner)
    }

    fn expr_inner(&mut self) -> Result<Expr> {
        let lhs = self.or_expr()?;
        let op = if self.is_op("=") {
            None
        } else if self.is_op("+=") {
            Some(Bin::Add)
        } else if self.is_op("-=") {
            Some(Bin::Sub)
        } else if self.is_op("*=") {
            Some(Bin::Mul)
        } else if self.is_op("/=") {
            Some(Bin::Div)
        } else if self.is_op("%=") || self.is_op("^=") || self.is_op("**=") {
            return Err(refuse("compound assignment %=, ^= or **="));
        } else {
            return Ok(lhs);
        };
        if !is_lvalue(&lhs) {
            return Err(self.syntax("assignment to a non-variable"));
        }
        self.pos += 1;
        let value = self.expr()?;
        Ok(Expr::Assign {
            target: Box::new(lhs),
            op,
            value: Box::new(value),
        })
    }

    fn or_expr(&mut self) -> Result<Expr> {
        let mut lhs = self.and_expr()?;
        while self.is_op("||") {
            self.link()?;
            self.pos += 1;
            self.skip_newlines();
            let rhs = self.and_expr()?;
            lhs = Expr::Or(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn and_expr(&mut self) -> Result<Expr> {
        let mut lhs = self.match_expr()?;
        while self.is_op("&&") {
            self.link()?;
            self.pos += 1;
            self.skip_newlines();
            let rhs = self.match_expr()?;
            lhs = Expr::And(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn match_expr(&mut self) -> Result<Expr> {
        let mut lhs = self.cmp_expr()?;
        while self.is_op("~") || self.is_op("!~") {
            let negate = self.is_op("!~");
            self.link()?;
            self.pos += 1;
            let re = match self.tok().clone() {
                Tok::Regex(src) | Tok::Str(src) => {
                    self.pos += 1;
                    self.regex(&src)?
                }
                _ => {
                    return Err(refuse(
                        "dynamic regex; ~ and !~ take a /re/ or \"re\" literal",
                    ))
                }
            };
            lhs = Expr::Match {
                negate,
                subject: Box::new(lhs),
                re,
            };
        }
        Ok(lhs)
    }

    fn cmp_expr(&mut self) -> Result<Expr> {
        let mut lhs = self.concat_expr()?;
        loop {
            let op = match self.tok() {
                Tok::Op("<") => Bin::Lt,
                Tok::Op("<=") => Bin::Le,
                Tok::Op("==") => Bin::Eq,
                Tok::Op("!=") => Bin::Ne,
                Tok::Op(">=") => Bin::Ge,
                Tok::Op(">") if !self.no_gt => Bin::Gt,
                _ => break,
            };
            self.link()?;
            self.pos += 1;
            let rhs = self.concat_expr()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn starts_operand(&self) -> bool {
        match self.tok() {
            Tok::Num(_) | Tok::Str(_) => true,
            Tok::Name(n) => !KEYWORDS.contains(&n.as_str()),
            Tok::Op(o) => matches!(*o, "$" | "("),
            _ => false,
        }
    }

    fn concat_expr(&mut self) -> Result<Expr> {
        let mut lhs = self.additive()?;
        while self.starts_operand() {
            // Every operand consumes a token. A parse that does not would loop
            // here forever, building an ever larger tree.
            self.link()?;
            let before = self.pos;
            let rhs = self.additive()?;
            if self.pos == before {
                return Err(self.syntax("expression"));
            }
            lhs = Expr::Concat(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn additive(&mut self) -> Result<Expr> {
        let mut lhs = self.multiplicative()?;
        loop {
            let op = if self.is_op("+") {
                Bin::Add
            } else if self.is_op("-") {
                Bin::Sub
            } else {
                break;
            };
            self.link()?;
            self.pos += 1;
            let rhs = self.multiplicative()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn multiplicative(&mut self) -> Result<Expr> {
        let mut lhs = self.unary()?;
        loop {
            let op = if self.is_op("*") {
                Bin::Mul
            } else if self.is_op("/") {
                Bin::Div
            } else if self.is_op("%") {
                Bin::Mod
            } else {
                break;
            };
            self.link()?;
            self.pos += 1;
            let rhs = self.unary()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr> {
        self.deeper(Self::unary_inner)
    }

    fn unary_inner(&mut self) -> Result<Expr> {
        if self.is_op("!") {
            self.pos += 1;
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        if self.is_op("-") {
            self.pos += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.is_op("+") {
            self.pos += 1;
            return Ok(Expr::Plus(Box::new(self.unary()?)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr> {
        let e = self.primary()?;
        if is_lvalue(&e) && (self.is_op("++") || self.is_op("--")) {
            let delta = if self.is_op("++") { 1.0 } else { -1.0 };
            self.pos += 1;
            return Ok(Expr::IncDec {
                target: Box::new(e),
                delta,
                pre: false,
            });
        }
        if self.is_op("^") || self.is_op("**") {
            return Err(refuse("exponentiation (^ or **)"));
        }
        Ok(e)
    }

    fn prefix_incdec(&mut self) -> Result<Expr> {
        let delta = if self.is_op("++") { 1.0 } else { -1.0 };
        self.pos += 1;
        let target = self.primary()?;
        if !is_lvalue(&target) {
            return Err(self.syntax("increment of a non-variable"));
        }
        Ok(Expr::IncDec {
            target: Box::new(target),
            delta,
            pre: true,
        })
    }

    fn primary(&mut self) -> Result<Expr> {
        self.deeper(Self::primary_inner)
    }

    fn primary_inner(&mut self) -> Result<Expr> {
        let tok = self.tok().clone();
        match tok {
            Tok::Num(n) => {
                self.pos += 1;
                Ok(Expr::Num(n))
            }
            Tok::Str(s) => {
                self.pos += 1;
                Ok(Expr::Str(s))
            }
            Tok::Regex(src) => {
                self.pos += 1;
                Ok(Expr::MatchRecord(self.regex(&src)?))
            }
            Tok::Op("$") => {
                self.pos += 1;
                let index = if self.is_op("++") || self.is_op("--") {
                    self.prefix_incdec()?
                } else if self.is_op("-") {
                    self.pos += 1;
                    Expr::Neg(Box::new(self.primary()?))
                } else {
                    self.primary()?
                };
                Ok(Expr::Field(Box::new(index)))
            }
            Tok::Op("++") | Tok::Op("--") => self.prefix_incdec(),
            Tok::Op("(") => {
                self.pos += 1;
                let e = self.nested(|p| p.expr())?;
                if self.is_op(",") {
                    return Err(refuse("expression list outside print"));
                }
                self.expect_op(")")?;
                Ok(e)
            }
            Tok::Name(name) => self.name_expr(&name),
            _ => Err(self.syntax("expression")),
        }
    }

    fn call_args(&mut self) -> Result<Vec<Expr>> {
        self.nested(|p| {
            let mut args = Vec::new();
            if !p.is_op(")") {
                args.push(p.expr()?);
                while p.is_op(",") {
                    p.pos += 1;
                    p.skip_newlines();
                    args.push(p.expr()?);
                }
            }
            p.expect_op(")")?;
            Ok(args)
        })
    }

    fn name_expr(&mut self, name: &str) -> Result<Expr> {
        if KEYWORDS.contains(&name) {
            return Err(refuse(match name {
                "function" => "user-defined functions",
                "getline" => "getline",
                "while" | "for" | "do" => "loops (while, for, do)",
                "in" => "array membership (in)",
                "delete" => "delete",
                "exit" => "exit",
                "next" | "nextfile" => "next",
                "return" => "return",
                "break" | "continue" => "break and continue",
                other => return Err(self.syntax(other)),
            }));
        }
        if OUTSIDE_BUILTINS.contains(&name) {
            return Err(refuse(&format!("builtin function {name}")));
        }
        // Past the name: what follows decides between a call and a variable.
        self.pos += 1;
        let paren = self.is_op("(");
        let spaced_paren = paren && self.toks.get(self.pos).is_some_and(|t| t.spaced);
        if BUILTINS.contains(&name) {
            let builtin = match name {
                "length" => Builtin::Length,
                "substr" => Builtin::Substr,
                "index" => Builtin::Index,
                "tolower" => Builtin::ToLower,
                _ => Builtin::ToUpper,
            };
            if !paren {
                if builtin == Builtin::Length {
                    return Ok(Expr::Call(Builtin::Length, Vec::new()));
                }
                return Err(refuse(&format!("builtin {name} without arguments")));
            }
            self.pos += 1;
            let args = self.call_args()?;
            let (min, max) = builtin_arity(builtin);
            if args.len() < min || args.len() > max {
                return Err(arg_err(format!("awk: wrong number of arguments to {name}")));
            }
            return Ok(Expr::Call(builtin, args));
        }
        if paren && !spaced_paren {
            return Err(refuse(&format!("user-defined function {name}")));
        }
        if self.is_op("[") {
            return Err(refuse(&format!("associative array {name}[...]")));
        }
        if OUTSIDE_VARS.contains(&name) {
            return Err(refuse(&format!("variable {name}")));
        }
        let var = match name {
            "NR" => Var::Nr,
            "NF" => Var::Nf,
            "FS" => Var::Fs,
            "OFS" => Var::Ofs,
            _ => Var::User(self.intern(name)),
        };
        Ok(Expr::Var(var))
    }
}

// ---------------------------------------------------------------- values

#[derive(Clone, Debug)]
enum Val {
    Num(f64),
    Str(String),
    /// A string from input (a field): numeric when it looks like a number.
    Input(String),
    Uninit,
}

#[derive(Clone, Copy)]
enum Place {
    Var(Var),
    Field(usize),
}

enum Splitter {
    Blank,
    Char(char),
    Pattern(Regex),
}

fn flag(b: bool) -> f64 {
    if b {
        1.0
    } else {
        0.0
    }
}

fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n'
}

/// Scans a decimal number at the start of `t`; returns its value and the
/// byte length of the numeric prefix.
fn scan_number(t: &str) -> Option<(f64, usize)> {
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let frac_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        digits += i - frac_start;
    }
    if digits == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    let value: f64 = t[..i].parse().ok()?;
    Some((value, i))
}

/// The numeric value of a whole string that looks like a number, or None.
fn looks_numeric(s: &str) -> Option<f64> {
    let t = s.trim_matches(is_blank);
    match scan_number(t) {
        Some((value, len)) if len == t.len() => Some(value),
        _ => None,
    }
}

/// The leading numeric prefix of a string, as awk converts it (0 if none).
fn leading_number(s: &str) -> f64 {
    let t = s.trim_start_matches(is_blank);
    scan_number(t).map_or(0.0, |(value, _)| value)
}

fn trim_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

/// C's %g with the given precision (no flags).
fn format_g(n: f64, precision: usize) -> String {
    let p = precision.max(1);
    if n == 0.0 {
        return if n.is_sign_negative() {
            "-0".into()
        } else {
            "0".into()
        };
    }
    let scientific = format!("{:.*e}", p - 1, n);
    let (mantissa, exponent) = scientific
        .split_once('e')
        .unwrap_or((scientific.as_str(), "0"));
    let x: i32 = exponent.parse().unwrap_or(0);
    if x < -4 || x >= p as i32 {
        let sign = if x < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", trim_zeros(mantissa), x.abs())
    } else {
        let decimals = (p as i32 - 1 - x).max(0) as usize;
        trim_zeros(&format!("{:.*}", decimals, n))
    }
}

/// A number as awk converts it to a string (CONVFMT and OFMT are "%.6g").
fn number_text(n: f64) -> String {
    if n.is_nan() {
        return "nan".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "inf" } else { "-inf" }.into();
    }
    if n.fract() == 0.0 && n.abs() < 1e16 {
        return format!("{}", n as i64);
    }
    format_g(n, 6)
}

fn to_num(v: &Val) -> f64 {
    match v {
        Val::Num(n) => *n,
        Val::Str(s) | Val::Input(s) => leading_number(s),
        Val::Uninit => 0.0,
    }
}

fn to_str(v: &Val) -> String {
    match v {
        Val::Num(n) => number_text(*n),
        Val::Str(s) | Val::Input(s) => s.clone(),
        Val::Uninit => String::new(),
    }
}

fn truthy(v: &Val) -> bool {
    match v {
        Val::Num(n) => *n != 0.0,
        Val::Str(s) => !s.is_empty(),
        Val::Input(s) => match looks_numeric(s) {
            Some(n) => n != 0.0,
            None => !s.is_empty(),
        },
        Val::Uninit => false,
    }
}

fn is_numeric_operand(v: &Val) -> bool {
    match v {
        Val::Num(_) | Val::Uninit => true,
        Val::Input(s) => looks_numeric(s).is_some(),
        Val::Str(_) => false,
    }
}

/// awk comparison: numeric when both sides are numeric, else by string.
fn compare(a: &Val, b: &Val) -> Ordering {
    if is_numeric_operand(a) && is_numeric_operand(b) {
        to_num(a).partial_cmp(&to_num(b)).unwrap_or(Ordering::Equal)
    } else {
        to_str(a).cmp(&to_str(b))
    }
}

fn binary(op: Bin, a: &Val, b: &Val) -> Result<Val> {
    Ok(match op {
        Bin::Add => Val::Num(to_num(a) + to_num(b)),
        Bin::Sub => Val::Num(to_num(a) - to_num(b)),
        Bin::Mul => Val::Num(to_num(a) * to_num(b)),
        Bin::Div => {
            let d = to_num(b);
            if d == 0.0 {
                return Err(arg_err("awk: division by zero"));
            }
            Val::Num(to_num(a) / d)
        }
        Bin::Mod => {
            let d = to_num(b);
            if d == 0.0 {
                return Err(arg_err("awk: division by zero in %"));
            }
            Val::Num(to_num(a) % d)
        }
        _ => {
            let ord = compare(a, b);
            let holds = match op {
                Bin::Lt => ord == Ordering::Less,
                Bin::Le => ord != Ordering::Greater,
                Bin::Gt => ord == Ordering::Greater,
                Bin::Ge => ord != Ordering::Less,
                Bin::Eq => ord == Ordering::Equal,
                _ => ord != Ordering::Equal,
            };
            Val::Num(flag(holds))
        }
    })
}

/// awk substr: 1-based, with rounded start and length, clipped to the string.
/// substr as gawk --posix computes it, the reference this subset is tested
/// against: both numbers are truncated toward zero, a start below 1 counts
/// from 1 without shortening the length (substr("hello", -1, 3) is "hel"),
/// and the result is clipped to the string. Awks disagree on a start below 1;
/// mawk gives "hell" there.
fn substr(s: &str, start: f64, len: Option<f64>) -> String {
    let chars: Vec<char> = s.chars().collect();
    let total = chars.len() as f64;
    let start = if start.is_nan() {
        1.0
    } else {
        start.trunc().max(1.0)
    };
    if start > total {
        return String::new();
    }
    let take = match len {
        Some(n) if n.is_nan() || n.trunc() <= 0.0 => return String::new(),
        Some(n) => n.trunc(),
        None => total,
    };
    let from = start as usize - 1;
    let to = (start - 1.0 + take).min(total) as usize;
    chars[from..to].iter().collect()
}

fn index_of(s: &str, t: &str) -> usize {
    if t.is_empty() {
        return 0;
    }
    match s.find(t) {
        Some(byte) => s[..byte].chars().count() + 1,
        None => 0,
    }
}

fn char_from(n: f64) -> Result<char> {
    if n < 0.0 || n.fract() != 0.0 {
        return Err(arg_err("awk: printf %c needs a non-negative integer"));
    }
    char::from_u32(n as u32).ok_or_else(|| arg_err("awk: printf %c code point is invalid"))
}

/// printf with the supported conversions only: %s %d %i %x %c %f (with an
/// optional .N precision) and %%. Widths and other flags are refused.
fn format_printf(fmt: &str, args: &[Val]) -> Result<String> {
    let chars: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut next_arg = 0usize;
    let mut i = 0;
    while i < chars.len() {
        if out.len() > MAX_OUTPUT_BYTES {
            return Err(budget_exceeded(
                "awk",
                format!("printf output exceeds {MAX_OUTPUT_BYTES} bytes"),
                "print less output",
            ));
        }
        let c = chars[i];
        i += 1;
        if c != '%' {
            out.push(c);
            continue;
        }
        let Some(&first) = chars.get(i) else {
            out.push('%');
            break;
        };
        if first == '%' {
            out.push('%');
            i += 1;
            continue;
        }
        let mut precision = None;
        let conv = if first == '.' {
            i += 1;
            let mut p = 0usize;
            while let Some(d) = chars.get(i).and_then(|d| d.to_digit(10)) {
                p = p.saturating_mul(10).saturating_add(d as usize);
                i += 1;
            }
            if p > MAX_PRINTF_PRECISION {
                return Err(refuse("printf precision above 100"));
            }
            precision = Some(p);
            match chars.get(i) {
                Some(&x) => x,
                None => return Err(refuse("printf precision without a conversion")),
            }
        } else {
            first
        };
        if conv.is_ascii_digit() || matches!(conv, '-' | '+' | ' ' | '#' | '*') {
            return Err(refuse("printf width or flags"));
        }
        if precision.is_some() && conv != 'f' {
            return Err(refuse("printf precision on a conversion other than %f"));
        }
        i += 1;
        let arg = args.get(next_arg).cloned().unwrap_or(Val::Uninit);
        next_arg += 1;
        match conv {
            'd' | 'i' => {
                let t = to_num(&arg).trunc();
                if t.abs() < 9.0e18 {
                    out.push_str(&(t as i64).to_string());
                } else {
                    out.push_str(&format!("{t:.0}"));
                }
            }
            'x' => {
                let t = to_num(&arg).trunc();
                if !(0.0..1.8e19).contains(&t) {
                    return Err(arg_err("awk: printf %x needs an integer from 0 to 2^64-1"));
                }
                out.push_str(&format!("{:x}", t as u64));
            }
            'c' => {
                let code = match &arg {
                    Val::Num(n) => Some(*n),
                    Val::Input(s) if looks_numeric(s).is_some() => Some(to_num(&arg)),
                    _ => None,
                };
                match code {
                    Some(n) => out.push(char_from(n)?),
                    None => {
                        if let Some(ch) = to_str(&arg).chars().next() {
                            out.push(ch);
                        }
                    }
                }
            }
            's' => out.push_str(&to_str(&arg)),
            'f' => {
                let p = precision.unwrap_or(6);
                out.push_str(&format!("{:.*}", p, to_num(&arg)));
            }
            other => {
                return Err(refuse(&format!(
                    "printf conversion %{other} (supported: %s %d %i %x %c %f %%)"
                )))
            }
        }
    }
    Ok(out)
}

fn compile_splitter(fs: &str) -> Result<Splitter> {
    if fs == " " {
        return Ok(Splitter::Blank);
    }
    let mut chars = fs.chars();
    match (chars.next(), chars.next()) {
        (None, _) => Err(arg_err(
            "awk: an empty field separator is outside the subset; use a character, \" \", or a regex",
        )),
        (Some(c), None) => Ok(Splitter::Char(c)),
        _ => {
            let re = compile_regex(fs)?;
            if re.is_match("") {
                return Err(arg_err("awk: a field separator regex must not match the empty string"));
            }
            Ok(Splitter::Pattern(re))
        }
    }
}

fn checked_field(n: f64) -> Result<usize> {
    if n.is_nan() || n < 0.0 {
        return Err(arg_err("awk: negative or invalid field index"));
    }
    if n > MAX_FIELD as f64 {
        return Err(arg_err(format!("awk: field index exceeds {MAX_FIELD}")));
    }
    Ok(n as usize)
}

// ---------------------------------------------------------------- runtime

struct Runtime<'p> {
    prog: &'p Program,
    vars: Vec<Val>,
    nr: f64,
    fs: String,
    splitter: Splitter,
    ofs: String,
    record: String,
    fields: Vec<String>,
    out: String,
    steps: u64,
    /// Bytes held by user variables, bounded by `MAX_VAR_BYTES`.
    var_bytes: usize,
    /// Bytes held by partial concatenations still being evaluated.
    temp_bytes: usize,
}

fn val_len(v: &Val) -> usize {
    match v {
        Val::Str(s) | Val::Input(s) => s.len(),
        Val::Num(_) | Val::Uninit => 0,
    }
}

fn too_large(what: &str, limit: usize) -> anyhow::Error {
    budget_exceeded(
        "awk",
        format!("{what} would exceed {limit} bytes"),
        "build smaller strings",
    )
}

impl<'p> Runtime<'p> {
    fn new(prog: &'p Program, fs: &str) -> Result<Self> {
        Ok(Runtime {
            prog,
            vars: vec![Val::Uninit; prog.names.len()],
            nr: 0.0,
            fs: fs.to_string(),
            splitter: compile_splitter(fs)?,
            ofs: " ".to_string(),
            record: String::new(),
            fields: Vec::new(),
            out: String::new(),
            steps: 0,
            var_bytes: 0,
            temp_bytes: 0,
        })
    }

    fn tick(&mut self) -> Result<()> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return Err(budget_exceeded(
                "awk",
                format!("step budget of {MAX_STEPS} is exhausted"),
                "simplify the program or narrow the input",
            ));
        }
        Ok(())
    }

    fn set_record(&mut self, line: &str) {
        self.record.clear();
        self.record.push_str(line);
        self.split();
    }

    fn split(&mut self) {
        self.fields.clear();
        let record = self.record.as_str();
        match &self.splitter {
            Splitter::Blank => self.fields.extend(
                record
                    .split(is_blank)
                    .filter(|w| !w.is_empty())
                    .map(str::to_string),
            ),
            Splitter::Char(c) => {
                if !record.is_empty() {
                    self.fields.extend(record.split(*c).map(str::to_string));
                }
            }
            Splitter::Pattern(re) => {
                if !record.is_empty() {
                    self.fields.extend(re.split(record).map(str::to_string));
                }
            }
        }
    }

    fn rebuild(&mut self) -> Result<()> {
        let separators = self
            .ofs
            .len()
            .saturating_mul(self.fields.len().saturating_sub(1));
        let total = self
            .fields
            .iter()
            .fold(separators, |n, f| n.saturating_add(f.len()));
        if total > MAX_VALUE_BYTES {
            return Err(too_large("the rebuilt record", MAX_VALUE_BYTES));
        }
        self.record = self.fields.join(self.ofs.as_str());
        Ok(())
    }

    fn read(&self, place: Place) -> Val {
        match place {
            Place::Var(v) => match v {
                Var::User(i) => self.vars[i].clone(),
                Var::Nr => Val::Num(self.nr),
                Var::Nf => Val::Num(self.fields.len() as f64),
                Var::Fs => Val::Str(self.fs.clone()),
                Var::Ofs => Val::Str(self.ofs.clone()),
            },
            Place::Field(0) => Val::Input(self.record.clone()),
            Place::Field(i) => self
                .fields
                .get(i - 1)
                .map_or(Val::Uninit, |f| Val::Input(f.clone())),
        }
    }

    fn write(&mut self, place: Place, v: Val) -> Result<()> {
        match place {
            Place::Var(Var::User(i)) => {
                let held = self.var_bytes - val_len(&self.vars[i]) + val_len(&v);
                if held > MAX_VAR_BYTES {
                    return Err(too_large("the variables together", MAX_VAR_BYTES));
                }
                self.var_bytes = held;
                self.vars[i] = v;
            }
            Place::Var(Var::Nr) => self.nr = to_num(&v),
            Place::Var(Var::Nf) => {
                let n = checked_field(to_num(&v))?;
                self.fields.resize(n, String::new());
                self.rebuild()?;
            }
            Place::Var(Var::Fs) => {
                let fs = to_str(&v);
                self.splitter = compile_splitter(&fs)?;
                self.fs = fs;
            }
            Place::Var(Var::Ofs) => self.ofs = to_str(&v),
            Place::Field(0) => {
                self.record = to_str(&v);
                self.split();
            }
            Place::Field(i) => {
                if i > self.fields.len() {
                    self.fields.resize(i, String::new());
                }
                self.fields[i - 1] = to_str(&v);
                self.rebuild()?;
            }
        }
        Ok(())
    }

    fn field_index(&mut self, e: &Expr) -> Result<usize> {
        let n = to_num(&self.eval(e)?);
        checked_field(n)
    }

    fn place(&mut self, target: &Expr) -> Result<Place> {
        match target {
            Expr::Var(v) => Ok(Place::Var(*v)),
            Expr::Field(index) => Ok(Place::Field(self.field_index(index)?)),
            _ => Err(arg_err(
                "awk: assignment target must be a variable or a field",
            )),
        }
    }

    fn assign(&mut self, target: &Expr, op: Option<Bin>, value: &Expr) -> Result<Val> {
        let place = self.place(target)?;
        let rhs = self.eval(value)?;
        let v = match op {
            None => rhs,
            Some(op) => {
                let current = self.read(place);
                binary(op, &current, &rhs)?
            }
        };
        self.write(place, v.clone())?;
        Ok(v)
    }

    fn eval(&mut self, e: &Expr) -> Result<Val> {
        self.tick()?;
        Ok(match e {
            Expr::Num(n) => Val::Num(*n),
            Expr::Str(s) => Val::Str(s.clone()),
            Expr::MatchRecord(re) => {
                let hit = self.prog.regexes[*re].is_match(&self.record);
                Val::Num(flag(hit))
            }
            Expr::Var(v) => self.read(Place::Var(*v)),
            Expr::Field(index) => {
                let i = self.field_index(index)?;
                self.read(Place::Field(i))
            }
            Expr::Neg(x) => Val::Num(-to_num(&self.eval(x)?)),
            Expr::Plus(x) => Val::Num(to_num(&self.eval(x)?)),
            Expr::Not(x) => Val::Num(flag(!truthy(&self.eval(x)?))),
            Expr::Bin(op, a, b) => {
                let x = self.eval(a)?;
                let y = self.eval(b)?;
                binary(*op, &x, &y)?
            }
            Expr::Concat(a, b) => {
                let mut s = to_str(&self.eval(a)?);
                // The left side is held while the right is evaluated; nested
                // concatenations hold one each, so their total is bounded too.
                if self.temp_bytes + s.len() > MAX_VAR_BYTES {
                    return Err(too_large("pending concatenations", MAX_VAR_BYTES));
                }
                self.temp_bytes += s.len();
                let right = self.eval(b);
                self.temp_bytes -= s.len();
                let right = to_str(&right?);
                if s.len() + right.len() > MAX_VALUE_BYTES {
                    return Err(too_large("a concatenated string", MAX_VALUE_BYTES));
                }
                s.push_str(&right);
                Val::Str(s)
            }
            Expr::And(a, b) => {
                let left = truthy(&self.eval(a)?);
                Val::Num(flag(left && truthy(&self.eval(b)?)))
            }
            Expr::Or(a, b) => {
                let left = truthy(&self.eval(a)?);
                Val::Num(flag(left || truthy(&self.eval(b)?)))
            }
            Expr::Match {
                negate,
                subject,
                re,
            } => {
                let s = to_str(&self.eval(subject)?);
                let hit = self.prog.regexes[*re].is_match(&s);
                Val::Num(flag(hit != *negate))
            }
            Expr::Assign { target, op, value } => self.assign(target, *op, value)?,
            Expr::IncDec { target, delta, pre } => {
                let place = self.place(target)?;
                let old = to_num(&self.read(place));
                let new = old + delta;
                self.write(place, Val::Num(new))?;
                Val::Num(if *pre { new } else { old })
            }
            Expr::Call(b, args) => self.call(*b, args)?,
        })
    }

    fn call(&mut self, b: Builtin, args: &[Expr]) -> Result<Val> {
        Ok(match b {
            Builtin::Length => {
                let n = if args.is_empty() {
                    self.record.chars().count()
                } else {
                    to_str(&self.eval(&args[0])?).chars().count()
                };
                Val::Num(n as f64)
            }
            Builtin::Substr => {
                let s = to_str(&self.eval(&args[0])?);
                let start = to_num(&self.eval(&args[1])?);
                let len = if args.len() == 3 {
                    Some(to_num(&self.eval(&args[2])?))
                } else {
                    None
                };
                Val::Str(substr(&s, start, len))
            }
            Builtin::Index => {
                let s = to_str(&self.eval(&args[0])?);
                let t = to_str(&self.eval(&args[1])?);
                Val::Num(index_of(&s, &t) as f64)
            }
            Builtin::ToLower => Val::Str(to_str(&self.eval(&args[0])?).to_ascii_lowercase()),
            Builtin::ToUpper => Val::Str(to_str(&self.eval(&args[0])?).to_ascii_uppercase()),
        })
    }

    fn emit_raw(&mut self, text: &str) -> Result<()> {
        if self.out.len() + text.len() > MAX_OUTPUT_BYTES {
            return Err(budget_exceeded(
                "awk",
                format!("output exceeds {MAX_OUTPUT_BYTES} bytes"),
                "print less output",
            ));
        }
        self.out.push_str(text);
        Ok(())
    }

    fn emit_record(&mut self) -> Result<()> {
        let mut line = self.record.clone();
        line.push('\n');
        self.emit_raw(&line)
    }

    fn exec(&mut self, stmt: &Stmt) -> Result<()> {
        self.tick()?;
        match stmt {
            Stmt::Print(args) => {
                if args.is_empty() {
                    return self.emit_record();
                }
                let ofs = self.ofs.clone();
                let mut line = String::new();
                for (k, arg) in args.iter().enumerate() {
                    if k > 0 {
                        line.push_str(&ofs);
                    }
                    let v = self.eval(arg)?;
                    line.push_str(&to_str(&v));
                    if self.out.len() + line.len() > MAX_OUTPUT_BYTES {
                        return Err(budget_exceeded(
                            "awk",
                            format!("output exceeds {MAX_OUTPUT_BYTES} bytes"),
                            "print less output",
                        ));
                    }
                }
                line.push('\n');
                self.emit_raw(&line)
            }
            Stmt::Printf(args) => {
                let fmt = to_str(&self.eval(&args[0])?);
                let mut vals = Vec::with_capacity(args.len() - 1);
                for arg in &args[1..] {
                    vals.push(self.eval(arg)?);
                }
                let text = format_printf(&fmt, &vals)?;
                self.emit_raw(&text)
            }
            Stmt::Expr(e) => self.eval(e).map(|_| ()),
            Stmt::If(cond, then, otherwise) => {
                let branch = if truthy(&self.eval(cond)?) {
                    then
                } else {
                    otherwise
                };
                for s in branch {
                    self.exec(s)?;
                }
                Ok(())
            }
            Stmt::Block(stmts) => {
                for s in stmts {
                    self.exec(s)?;
                }
                Ok(())
            }
        }
    }
}
