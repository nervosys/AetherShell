//! `web_xpath`: a lenient HTML parser and a subset of XPath 1.0 over it.
//!
//! Pure: no network, no file access, no dependency. The parser accepts any
//! markup; the evaluator refuses only expressions outside the subset, with
//! `Refusal::Expression` naming the part it could not handle. Results are text:
//! an element is its outer HTML from the source, an attribute its decoded
//! value, a text node its decoded text.
//!
//! Supported XPath: absolute and relative location paths, `//`, `.`, `..`, the
//! axes child, descendant, descendant-or-self, self, parent and attribute (in
//! abbreviated form or spelled out), the node tests name, `*`, `text()`,
//! `node()`, `@name` and `@*`, predicates (position, comparisons with = != < <=
//! > >=, existence, `and`, `or`, `not()`, `contains()`, `starts-with()`,
//! `normalize-space()`, `position()`, `last()`, `true()`, `false()`), and
//! top-level union `|`.

/// Largest HTML a call accepts, in bytes.
pub const MAX_HTML_BYTES: usize = 16 * 1024 * 1024;
/// Deepest element nesting the parser builds.
pub const MAX_DEPTH: usize = 512;
/// Deepest nesting of parentheses, predicates and function arguments.
const MAX_EXPR_DEPTH: usize = 64;
/// Most nodes one evaluation may visit along its axes. `//*//*` visits each
/// node once per ancestor and a path in a predicate once per candidate, so
/// without a bound a large page could take quadratic time and memory.
pub const MAX_WORK: usize = 20_000_000;

/// Why a call was refused. The wrapper in `builtins.rs` turns these into
/// `E_BAD_ARG` or `E_BUDGET_EXCEEDED`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The expression, or a part of it, is outside the supported subset.
    Expression { got: String },
    /// Element nesting exceeds `MAX_DEPTH`.
    Depth,
    /// More results than the caller allows.
    TooManyResults { limit: usize },
    /// The evaluation would visit more than `MAX_WORK` nodes.
    Work,
}

// ── document ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Root,
    Element,
    Text,
}

struct Node {
    kind: Kind,
    /// Lowercase tag name; elements only.
    name: String,
    /// Lowercase name and decoded value, in source order, first duplicate kept.
    attrs: Vec<(String, String)>,
    /// Decoded text; text nodes only.
    text: String,
    parent: usize,
    children: Vec<usize>,
    /// Byte span of the outer HTML in the source.
    start: usize,
    end: usize,
}

impl Node {
    fn new(kind: Kind, parent: usize, start: usize) -> Node {
        Node {
            kind,
            name: String::new(),
            attrs: Vec::new(),
            text: String::new(),
            parent,
            children: Vec::new(),
            start,
            end: start,
        }
    }
}

/// A parsed document. Node indices are in document order: a node's index is
/// assigned when its opening tag or text run is reached, so index order is
/// source order. Index 0 is the document node.
pub struct Document<'a> {
    src: &'a str,
    nodes: Vec<Node>,
    /// Nodes visited so far; past `MAX_WORK` the axes return nothing and the
    /// evaluation is refused.
    work: std::cell::Cell<usize>,
}

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];
/// Elements whose content is raw text up to the matching close tag.
const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style", "textarea", "title"];

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn skip_space(bytes: &[u8], mut p: usize) -> usize {
    while p < bytes.len() && is_space(bytes[p]) {
        p += 1;
    }
    p
}

/// End of a tag or attribute name starting at `from`.
fn tag_name_end(bytes: &[u8], from: usize) -> usize {
    let mut p = from;
    while p < bytes.len() && !is_space(bytes[p]) && !matches!(bytes[p], b'/' | b'>') {
        p += 1;
    }
    p
}

fn find_from(src: &str, from: usize, pat: &str) -> Option<usize> {
    src.get(from..)?.find(pat).map(|i| from + i)
}

/// Decode the entities `&amp; &lt; &gt; &quot; &apos; &nbsp;` and numeric
/// `&#NN;` / `&#xHH;`. Anything else is kept verbatim.
fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        match decode_entity(after) {
            Some((ch, used)) => {
                out.push(ch);
                rest = &after[used..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The character an entity body (the text after `&`) names, and how many bytes
/// of `s` it used including the `;`.
fn decode_entity(s: &str) -> Option<(char, usize)> {
    let mut end = s.len().min(33);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    let semi = s[..end].find(';')?;
    let body = &s[..semi];
    let ch = if let Some(num) = body.strip_prefix('#') {
        let digits = num.strip_prefix(['x', 'X']);
        let value = match digits {
            Some(hex) if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) => {
                u32::from_str_radix(hex, 16).ok()?
            }
            Some(_) => return None,
            None if !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()) => {
                num.parse().ok()?
            }
            None => return None,
        };
        char::from_u32(value)?
    } else {
        match body {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            "nbsp" => '\u{a0}',
            _ => return None,
        }
    };
    Some((ch, semi + 1))
}

fn push_child(nodes: &mut Vec<Node>, parent: usize, mut node: Node) -> usize {
    node.parent = parent;
    let id = nodes.len();
    nodes.push(node);
    nodes[parent].children.push(id);
    id
}

/// Attach buffered text to `parent` as one text node.
fn flush_text(nodes: &mut Vec<Node>, parent: usize, pending: &mut String) {
    if pending.is_empty() {
        return;
    }
    let mut node = Node::new(Kind::Text, parent, 0);
    node.text = std::mem::take(pending);
    push_child(nodes, parent, node);
}

/// Attributes from `from` up to the end of an opening tag. Returns the
/// attributes, the byte after the tag, and whether it ended in `/>`.
fn parse_attributes(src: &str, from: usize) -> (Vec<(String, String)>, usize, bool) {
    let b = src.as_bytes();
    let n = b.len();
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut p = from;
    loop {
        p = skip_space(b, p);
        if p >= n {
            return (attrs, n, false);
        }
        match b[p] {
            b'>' => return (attrs, p + 1, false),
            b'/' if b.get(p + 1) == Some(&b'>') => return (attrs, p + 2, true),
            b'/' | b'=' => {
                p += 1;
                continue;
            }
            _ => {}
        }
        let start = p;
        while p < n && !is_space(b[p]) && !matches!(b[p], b'=' | b'>' | b'/') {
            p += 1;
        }
        let name = src[start..p].to_ascii_lowercase();
        let after_name = skip_space(b, p);
        let value = if after_name < n && b[after_name] == b'=' {
            p = skip_space(b, after_name + 1);
            if p < n && (b[p] == b'"' || b[p] == b'\'') {
                let quote = b[p];
                let vs = p + 1;
                let ve = b[vs..]
                    .iter()
                    .position(|&c| c == quote)
                    .map_or(n, |i| vs + i);
                p = (ve + 1).min(n);
                decode(&src[vs..ve])
            } else {
                let vs = p;
                while p < n && !is_space(b[p]) && b[p] != b'>' {
                    p += 1;
                }
                decode(&src[vs..p])
            }
        } else {
            String::new()
        };
        if !attrs.iter().any(|(k, _)| *k == name) {
            attrs.push((name, value));
        }
    }
}

/// The `</name` that closes a raw-text element, searched ASCII-case-insensitively
/// in `lower` (a lowercase copy of `src`, the same length). Returns the start of
/// the close tag and the byte after its `>`.
fn raw_close(src: &str, lower: &str, from: usize, name: &str) -> Option<(usize, usize)> {
    let needle = format!("</{name}");
    let bytes = lower.as_bytes();
    let mut at = from;
    while let Some(rel) = lower.get(at..)?.find(&needle) {
        let hit = at + rel;
        let after = hit + needle.len();
        let ends_name = match bytes.get(after) {
            None => true,
            Some(&c) => is_space(c) || c == b'/' || c == b'>',
        };
        if ends_name {
            let close_end = find_from(src, after, ">").map_or(src.len(), |e| e + 1);
            return Some((hit, close_end));
        }
        at = hit + 1;
    }
    None
}

/// Parse markup leniently. Comments, doctypes and processing instructions are
/// skipped; CDATA is text; unmatched close tags are ignored; elements still
/// open at the end are closed there.
pub fn parse(src: &str) -> Result<Document<'_>, Refusal> {
    let bytes = src.as_bytes();
    let n = bytes.len();
    let lower = src.to_ascii_lowercase();
    let mut nodes = vec![Node {
        end: n,
        ..Node::new(Kind::Root, 0, 0)
    }];
    // Open elements, document node first.
    let mut stack: Vec<usize> = vec![0];
    let mut pending = String::new();
    let mut pos = 0;
    while pos < n {
        let top = stack.last().copied().unwrap_or(0);
        if bytes[pos] != b'<' {
            let next = src[pos..].find('<').map_or(n, |i| pos + i);
            pending.push_str(&decode(&src[pos..next]));
            pos = next;
            continue;
        }
        let after = bytes.get(pos + 1).copied();
        if src[pos..].starts_with("<!--") {
            pos = find_from(src, pos + 4, "-->").map_or(n, |e| e + 3);
        } else if lower[pos..].starts_with("<![cdata[") {
            let body = pos + 9;
            let (end, next) = match find_from(src, body, "]]>") {
                Some(e) => (e, e + 3),
                None => (n, n),
            };
            pending.push_str(&src[body..end]);
            pos = next;
        } else if matches!(after, Some(b'!') | Some(b'?')) {
            pos = find_from(src, pos + 2, ">").map_or(n, |e| e + 1);
        } else if after == Some(b'/') {
            let name_start = pos + 2;
            let name_end = tag_name_end(bytes, name_start);
            if name_end == name_start {
                pending.push('<');
                pos += 1;
                continue;
            }
            flush_text(&mut nodes, top, &mut pending);
            let close_end = find_from(src, name_end, ">").map_or(n, |e| e + 1);
            let name = &lower[name_start..name_end];
            let open = stack
                .iter()
                .rposition(|&id| id != 0 && nodes[id].name == name);
            if let Some(at) = open {
                while stack.len() > at + 1 {
                    if let Some(id) = stack.pop() {
                        nodes[id].end = pos;
                    }
                }
                if let Some(id) = stack.pop() {
                    nodes[id].end = close_end;
                }
            }
            pos = close_end;
        } else if matches!(after, Some(c) if c.is_ascii_alphabetic()) {
            flush_text(&mut nodes, top, &mut pending);
            let name_start = pos + 1;
            let name_end = tag_name_end(bytes, name_start);
            let name = lower[name_start..name_end].to_string();
            let (attrs, tag_end, self_closing) = parse_attributes(src, name_end);
            let mut element = Node::new(Kind::Element, top, pos);
            element.name = name.clone();
            element.attrs = attrs;
            element.end = tag_end;
            let id = push_child(&mut nodes, top, element);
            if self_closing || VOID_ELEMENTS.contains(&name.as_str()) {
                pos = tag_end;
            } else if RAW_TEXT_ELEMENTS.contains(&name.as_str()) {
                let (content_end, next) = raw_close(src, &lower, tag_end, &name).unwrap_or((n, n));
                if content_end > tag_end {
                    let raw = &src[tag_end..content_end];
                    let mut text = Node::new(Kind::Text, id, tag_end);
                    text.text = if name == "textarea" || name == "title" {
                        decode(raw)
                    } else {
                        raw.to_string()
                    };
                    push_child(&mut nodes, id, text);
                }
                nodes[id].end = next;
                pos = next;
            } else {
                if stack.len() > MAX_DEPTH {
                    return Err(Refusal::Depth);
                }
                stack.push(id);
                pos = tag_end;
            }
        } else {
            pending.push('<');
            pos += 1;
        }
    }
    let top = stack.last().copied().unwrap_or(0);
    flush_text(&mut nodes, top, &mut pending);
    while let Some(id) = stack.pop() {
        if id != 0 {
            nodes[id].end = n;
        }
    }
    Ok(Document {
        src,
        nodes,
        work: std::cell::Cell::new(0),
    })
}

// ── XPath: tokens ──────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Slash,
    DSlash,
    Pipe,
    LBrack,
    RBrack,
    LParen,
    RParen,
    At,
    Star,
    Dot,
    DDot,
    ColonColon,
    Comma,
    Cmp(Cmp),
    Str(String),
    Num(f64),
    Name(String),
    /// Any character or string the grammar does not use; named in a refusal.
    Other(String),
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Slash => "/".into(),
        Tok::DSlash => "//".into(),
        Tok::Pipe => "|".into(),
        Tok::LBrack => "[".into(),
        Tok::RBrack => "]".into(),
        Tok::LParen => "(".into(),
        Tok::RParen => ")".into(),
        Tok::At => "@".into(),
        Tok::Star => "*".into(),
        Tok::Dot => ".".into(),
        Tok::DDot => "..".into(),
        Tok::ColonColon => "::".into(),
        Tok::Comma => ",".into(),
        Tok::Cmp(op) => match op {
            Cmp::Eq => "=",
            Cmp::Ne => "!=",
            Cmp::Lt => "<",
            Cmp::Le => "<=",
            Cmp::Gt => ">",
            Cmp::Ge => ">=",
        }
        .into(),
        Tok::Str(s) => format!("'{s}'"),
        Tok::Num(n) => num_text(*n),
        Tok::Name(s) | Tok::Other(s) => s.clone(),
    }
}

fn tokenize(expr: &str) -> Vec<Tok> {
    let b = expr.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let c = b[i];
        if is_space(c) {
            i += 1;
            continue;
        }
        let next = b.get(i + 1).copied();
        let (tok, len) = match c {
            b'/' if next == Some(b'/') => (Tok::DSlash, 2),
            b'/' => (Tok::Slash, 1),
            b'|' => (Tok::Pipe, 1),
            b'[' => (Tok::LBrack, 1),
            b']' => (Tok::RBrack, 1),
            b'(' => (Tok::LParen, 1),
            b')' => (Tok::RParen, 1),
            b'@' => (Tok::At, 1),
            b'*' => (Tok::Star, 1),
            b',' => (Tok::Comma, 1),
            b'=' => (Tok::Cmp(Cmp::Eq), 1),
            b'!' if next == Some(b'=') => (Tok::Cmp(Cmp::Ne), 2),
            b'<' if next == Some(b'=') => (Tok::Cmp(Cmp::Le), 2),
            b'<' => (Tok::Cmp(Cmp::Lt), 1),
            b'>' if next == Some(b'=') => (Tok::Cmp(Cmp::Ge), 2),
            b'>' => (Tok::Cmp(Cmp::Gt), 1),
            b'.' if next == Some(b'.') => (Tok::DDot, 2),
            b'.' if matches!(next, Some(d) if d.is_ascii_digit()) => number_token(b, i),
            b'.' => (Tok::Dot, 1),
            b':' if next == Some(b':') => (Tok::ColonColon, 2),
            b'"' | b'\'' => match expr[i + 1..].find(c as char) {
                Some(j) => (Tok::Str(expr[i + 1..i + 1 + j].to_string()), j + 2),
                None => (Tok::Other("unterminated string literal".into()), n - i),
            },
            d if d.is_ascii_digit() => number_token(b, i),
            a if a.is_ascii_alphabetic() || a == b'_' => {
                let mut j = i;
                while j < n && (b[j].is_ascii_alphanumeric() || matches!(b[j], b'-' | b'_' | b'.'))
                {
                    j += 1;
                }
                (Tok::Name(expr[i..j].to_string()), j - i)
            }
            _ => {
                let ch = expr[i..]
                    .chars()
                    .next()
                    .map_or(String::new(), |c| c.to_string());
                let len = ch.len().max(1);
                (Tok::Other(ch), len)
            }
        };
        out.push(tok);
        i += len;
    }
    out
}

/// A number starting at `i`: digits with an optional fraction.
fn number_token(b: &[u8], i: usize) -> (Tok, usize) {
    let n = b.len();
    let mut j = i;
    while j < n && b[j].is_ascii_digit() {
        j += 1;
    }
    if j < n && b[j] == b'.' {
        j += 1;
        while j < n && b[j].is_ascii_digit() {
            j += 1;
        }
    }
    let text = String::from_utf8_lossy(&b[i..j]).into_owned();
    match text.parse::<f64>() {
        Ok(v) => (Tok::Num(v), j - i),
        Err(_) => (Tok::Other(text), j - i),
    }
}

fn num_text(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

// ── XPath: syntax ──────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    Child,
    Descendant,
    DescendantOrSelf,
    SelfOnly,
    Parent,
    Attribute,
}

#[derive(Clone, Debug, PartialEq)]
enum Test {
    Node,
    Text,
    /// `*`: elements, or attributes on the attribute axis.
    Any,
    /// A name, lowercase.
    Name(String),
}

#[derive(Debug, PartialEq)]
struct Step {
    axis: Axis,
    test: Test,
    preds: Vec<Expr>,
}

#[derive(Debug, PartialEq)]
struct Path {
    absolute: bool,
    steps: Vec<Step>,
}

#[derive(Debug, PartialEq)]
enum Operand {
    Str(String),
    Num(f64),
    Position,
    Last,
    Path(Path),
    /// `normalize-space(x)`; `None` means the context node.
    Normalize(Option<Box<Operand>>),
}

#[derive(Debug, PartialEq)]
enum Expr {
    Operand(Operand),
    Cmp(Operand, Cmp, Operand),
    And(Vec<Expr>),
    Or(Vec<Expr>),
    Not(Box<Expr>),
    Contains(Operand, Operand),
    StartsWith(Operand, Operand),
    Const(bool),
}

fn axis_named(name: &str) -> Option<Axis> {
    Some(match name {
        "child" => Axis::Child,
        "descendant" => Axis::Descendant,
        "descendant-or-self" => Axis::DescendantOrSelf,
        "self" => Axis::SelfOnly,
        "parent" => Axis::Parent,
        "attribute" => Axis::Attribute,
        _ => return None,
    })
}

fn descendant_or_self() -> Step {
    Step {
        axis: Axis::DescendantOrSelf,
        test: Test::Node,
        preds: Vec::new(),
    }
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn peek_at(&self, k: usize) -> Option<&Tok> {
        self.toks.get(self.pos + k)
    }

    fn fail<T>(&self) -> Result<T, Refusal> {
        let got = self
            .toks
            .get(self.pos)
            .map_or_else(|| "end of expression".to_string(), describe);
        Err(Refusal::Expression { got })
    }

    fn expect(&mut self, want: &Tok) -> Result<(), Refusal> {
        if self.toks.get(self.pos) == Some(want) {
            self.pos += 1;
            Ok(())
        } else {
            self.fail()
        }
    }

    fn deeper(&mut self) -> Result<(), Refusal> {
        self.depth += 1;
        if self.depth > MAX_EXPR_DEPTH {
            return Err(Refusal::Expression {
                got: "an expression nested too deeply".into(),
            });
        }
        Ok(())
    }

    fn shallower(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn at_keyword(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Tok::Name(n)) if n.as_str() == kw)
    }

    fn starts_step(&self) -> bool {
        matches!(
            self.peek(),
            Some(Tok::Name(_) | Tok::Star | Tok::At | Tok::Dot | Tok::DDot)
        )
    }

    /// A location path: `/`, `//`, or a relative path, with its steps.
    fn path(&mut self) -> Result<Path, Refusal> {
        let mut steps = Vec::new();
        let mut absolute = false;
        match self.peek() {
            Some(Tok::Slash) => {
                self.pos += 1;
                absolute = true;
                // A lone `/` is the document node.
                if !self.starts_step() {
                    return Ok(Path { absolute, steps });
                }
            }
            Some(Tok::DSlash) => {
                self.pos += 1;
                absolute = true;
                steps.push(descendant_or_self());
            }
            _ => {}
        }
        steps.push(self.step()?);
        loop {
            match self.peek() {
                Some(Tok::Slash) => self.pos += 1,
                Some(Tok::DSlash) => {
                    self.pos += 1;
                    steps.push(descendant_or_self());
                }
                _ => break,
            }
            steps.push(self.step()?);
        }
        Ok(Path { absolute, steps })
    }

    /// The node test after an axis, `@`, or on its own.
    fn node_test(&mut self) -> Result<Test, Refusal> {
        match self.toks.get(self.pos).cloned() {
            Some(Tok::Star) => {
                self.pos += 1;
                Ok(Test::Any)
            }
            Some(Tok::Name(name)) => {
                self.pos += 1;
                if self.peek() == Some(&Tok::LParen) {
                    let test = match name.as_str() {
                        "text" => Test::Text,
                        "node" => Test::Node,
                        _ => {
                            return Err(Refusal::Expression {
                                got: format!("{name}()"),
                            })
                        }
                    };
                    self.pos += 1;
                    self.expect(&Tok::RParen)?;
                    Ok(test)
                } else {
                    Ok(Test::Name(name.to_ascii_lowercase()))
                }
            }
            _ => self.fail(),
        }
    }

    /// One step with its predicates.
    fn step(&mut self) -> Result<Step, Refusal> {
        let (axis, test) = match self.toks.get(self.pos).cloned() {
            Some(Tok::Dot) => {
                self.pos += 1;
                (Axis::SelfOnly, Test::Node)
            }
            Some(Tok::DDot) => {
                self.pos += 1;
                (Axis::Parent, Test::Node)
            }
            Some(Tok::At) => {
                self.pos += 1;
                (Axis::Attribute, self.node_test()?)
            }
            Some(Tok::Star) => {
                self.pos += 1;
                (Axis::Child, Test::Any)
            }
            Some(Tok::Name(name)) if self.peek_at(1) == Some(&Tok::ColonColon) => {
                let Some(axis) = axis_named(&name) else {
                    return Err(Refusal::Expression {
                        got: format!("{name}::"),
                    });
                };
                self.pos += 2;
                (axis, self.node_test()?)
            }
            Some(Tok::Name(_)) => (Axis::Child, self.node_test()?),
            other => {
                let got = other
                    .as_ref()
                    .map_or_else(|| "end of expression".to_string(), describe);
                return Err(Refusal::Expression { got });
            }
        };
        let mut preds = Vec::new();
        while self.peek() == Some(&Tok::LBrack) {
            self.pos += 1;
            preds.push(self.predicate()?);
        }
        Ok(Step { axis, test, preds })
    }

    fn predicate(&mut self) -> Result<Expr, Refusal> {
        self.deeper()?;
        let expr = self.or_expr()?;
        self.shallower();
        self.expect(&Tok::RBrack)?;
        Ok(expr)
    }

    fn or_expr(&mut self) -> Result<Expr, Refusal> {
        let first = self.and_expr()?;
        if !self.at_keyword("or") {
            return Ok(first);
        }
        let mut parts = vec![first];
        while self.at_keyword("or") {
            self.pos += 1;
            parts.push(self.and_expr()?);
        }
        Ok(Expr::Or(parts))
    }

    fn and_expr(&mut self) -> Result<Expr, Refusal> {
        let first = self.cmp_expr()?;
        if !self.at_keyword("and") {
            return Ok(first);
        }
        let mut parts = vec![first];
        while self.at_keyword("and") {
            self.pos += 1;
            parts.push(self.cmp_expr()?);
        }
        Ok(Expr::And(parts))
    }

    fn cmp_expr(&mut self) -> Result<Expr, Refusal> {
        let left = self.primary()?;
        let op = match self.peek() {
            Some(Tok::Cmp(op)) => *op,
            _ => return Ok(left),
        };
        self.pos += 1;
        let right = self.primary()?;
        match (left, right) {
            (Expr::Operand(l), Expr::Operand(r)) => {
                if matches!(l, Operand::Path(_)) && matches!(r, Operand::Path(_)) {
                    return Err(Refusal::Expression {
                        got: "a comparison of two node sets".into(),
                    });
                }
                Ok(Expr::Cmp(l, op, r))
            }
            _ => Err(Refusal::Expression {
                got: "a boolean compared with =".into(),
            }),
        }
    }

    fn primary(&mut self) -> Result<Expr, Refusal> {
        match self.toks.get(self.pos).cloned() {
            Some(Tok::Str(s)) => {
                self.pos += 1;
                Ok(Expr::Operand(Operand::Str(s)))
            }
            Some(Tok::Num(n)) => {
                self.pos += 1;
                Ok(Expr::Operand(Operand::Num(n)))
            }
            Some(Tok::LParen) => {
                self.pos += 1;
                self.deeper()?;
                let expr = self.or_expr()?;
                self.shallower();
                self.expect(&Tok::RParen)?;
                Ok(expr)
            }
            Some(Tok::Name(name))
                if self.peek_at(1) == Some(&Tok::LParen)
                    && !matches!(name.as_str(), "text" | "node") =>
            {
                self.pos += 2;
                self.function(&name)
            }
            _ => Ok(Expr::Operand(Operand::Path(self.path()?))),
        }
    }

    /// An operand that is used as a string or number, not as a boolean.
    fn operand_arg(&mut self) -> Result<Operand, Refusal> {
        match self.primary()? {
            Expr::Operand(op) => Ok(op),
            _ => Err(Refusal::Expression {
                got: "a boolean used as a string".into(),
            }),
        }
    }

    /// A function call after its name and `(`.
    fn function(&mut self, name: &str) -> Result<Expr, Refusal> {
        self.deeper()?;
        let expr = match name {
            "not" => Expr::Not(Box::new(self.or_expr()?)),
            "true" => Expr::Const(true),
            "false" => Expr::Const(false),
            "position" => Expr::Operand(Operand::Position),
            "last" => Expr::Operand(Operand::Last),
            "contains" | "starts-with" => {
                let a = self.operand_arg()?;
                self.expect(&Tok::Comma)?;
                let b = self.operand_arg()?;
                if name == "contains" {
                    Expr::Contains(a, b)
                } else {
                    Expr::StartsWith(a, b)
                }
            }
            "normalize-space" => {
                if self.peek() == Some(&Tok::RParen) {
                    Expr::Operand(Operand::Normalize(None))
                } else {
                    Expr::Operand(Operand::Normalize(Some(Box::new(self.operand_arg()?))))
                }
            }
            _ => {
                return Err(Refusal::Expression {
                    got: format!("{name}()"),
                })
            }
        };
        self.expect(&Tok::RParen)?;
        self.shallower();
        Ok(expr)
    }
}

/// Parse a whole expression: one or more location paths joined by `|`.
fn parse_expression(expr: &str) -> Result<Vec<Path>, Refusal> {
    let toks = tokenize(expr);
    if toks.is_empty() {
        return Err(Refusal::Expression {
            got: "empty expression".into(),
        });
    }
    let mut p = Parser {
        toks,
        pos: 0,
        depth: 0,
    };
    let mut paths = vec![p.path()?];
    while p.peek() == Some(&Tok::Pipe) {
        p.pos += 1;
        paths.push(p.path()?);
    }
    if p.pos < p.toks.len() {
        return p.fail();
    }
    Ok(paths)
}

// ── evaluation ─────────────────────────────────────────────────────────

/// A node of the result: an element or text node, or an attribute of a node.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Item {
    Node(usize),
    Attr(usize, usize),
}

impl Item {
    fn order(self) -> (usize, usize, usize) {
        match self {
            Item::Node(i) => (i, 0, 0),
            // An element's attributes come after it and before its children.
            Item::Attr(i, k) => (i, 1, k),
        }
    }
}

fn document_order(items: &mut Vec<Item>) {
    items.sort_by_key(|it| it.order());
    items.dedup();
}

/// A value an operand or predicate produces.
enum Val {
    Num(f64),
    Str(String),
    Bool(bool),
    Set(Vec<Item>),
}

#[derive(Clone, Copy)]
struct Ctx {
    item: Item,
    pos: usize,
    size: usize,
}

enum Atom {
    Num(f64),
    Str(String),
}

fn normalize_space(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Document<'_> {
    fn axis(&self, from: Item, axis: Axis, out: &mut Vec<Item>) {
        if self.work.get() > MAX_WORK {
            return;
        }
        let before = out.len();
        self.axis_unbudgeted(from, axis, out);
        self.work.set(self.work.get() + (out.len() - before));
    }

    fn axis_unbudgeted(&self, from: Item, axis: Axis, out: &mut Vec<Item>) {
        match axis {
            Axis::SelfOnly => out.push(from),
            Axis::DescendantOrSelf => {
                out.push(from);
                self.descendants(from, out);
            }
            Axis::Descendant => self.descendants(from, out),
            Axis::Child => {
                if let Item::Node(i) = from {
                    out.extend(self.nodes[i].children.iter().map(|&c| Item::Node(c)));
                }
            }
            Axis::Parent => match from {
                Item::Attr(i, _) => out.push(Item::Node(i)),
                Item::Node(0) => {}
                Item::Node(i) => out.push(Item::Node(self.nodes[i].parent)),
            },
            Axis::Attribute => {
                if let Item::Node(i) = from {
                    if self.nodes[i].kind == Kind::Element {
                        out.extend((0..self.nodes[i].attrs.len()).map(|k| Item::Attr(i, k)));
                    }
                }
            }
        }
    }

    /// Strict descendants of `from` in document order.
    fn descendants(&self, from: Item, out: &mut Vec<Item>) {
        let Item::Node(i) = from else { return };
        let mut stack: Vec<usize> = self.nodes[i].children.iter().rev().copied().collect();
        while let Some(c) = stack.pop() {
            if out.len() > MAX_WORK {
                return;
            }
            out.push(Item::Node(c));
            stack.extend(self.nodes[c].children.iter().rev().copied());
        }
    }

    fn matches(&self, item: Item, test: &Test) -> bool {
        match (item, test) {
            (_, Test::Node) => true,
            (Item::Node(i), Test::Text) => self.nodes[i].kind == Kind::Text,
            (Item::Node(i), Test::Any) => self.nodes[i].kind == Kind::Element,
            (Item::Attr(..), Test::Any) => true,
            (Item::Node(i), Test::Name(n)) => {
                self.nodes[i].kind == Kind::Element && self.nodes[i].name == *n
            }
            (Item::Attr(i, k), Test::Name(n)) => self.nodes[i].attrs[k].0 == *n,
            (Item::Attr(..), Test::Text) => false,
        }
    }

    /// The XPath string-value: every descendant text, for an element.
    fn string_value(&self, item: Item) -> String {
        match item {
            Item::Attr(i, k) => self.nodes[i].attrs[k].1.clone(),
            Item::Node(i) => {
                let mut out = String::new();
                let mut stack = vec![i];
                while let Some(id) = stack.pop() {
                    let node = &self.nodes[id];
                    if node.kind == Kind::Text {
                        out.push_str(&node.text);
                    }
                    stack.extend(node.children.iter().rev().copied());
                }
                out
            }
        }
    }

    /// The output text for one result item.
    fn render(&self, item: Item) -> String {
        match item {
            Item::Node(i) => {
                let node = &self.nodes[i];
                match node.kind {
                    Kind::Element => self.src[node.start..node.end].to_string(),
                    Kind::Text => node.text.clone(),
                    Kind::Root => String::new(),
                }
            }
            Item::Attr(i, k) => self.nodes[i].attrs[k].1.clone(),
        }
    }
}

fn eval_path(doc: &Document<'_>, path: &Path, ctx: Item) -> Vec<Item> {
    let mut set = vec![if path.absolute { Item::Node(0) } else { ctx }];
    for step in &path.steps {
        let mut next = Vec::new();
        for &from in &set {
            next.extend(eval_step(doc, step, from));
        }
        document_order(&mut next);
        set = next;
    }
    set
}

/// One step from one context node. Predicates number the nodes in axis order,
/// so `child::li[1]` is the first `li` child of its parent.
fn eval_step(doc: &Document<'_>, step: &Step, from: Item) -> Vec<Item> {
    let mut cands = Vec::new();
    doc.axis(from, step.axis, &mut cands);
    cands.retain(|&it| doc.matches(it, &step.test));
    for pred in &step.preds {
        let size = cands.len();
        let mut kept = Vec::new();
        for (k, &it) in cands.iter().enumerate() {
            let cx = Ctx {
                item: it,
                pos: k + 1,
                size,
            };
            if truthy(doc, pred, cx) {
                kept.push(it);
            }
        }
        cands = kept;
    }
    cands
}

fn truthy(doc: &Document<'_>, expr: &Expr, cx: Ctx) -> bool {
    match eval(doc, expr, cx) {
        Val::Num(n) => n == cx.pos as f64,
        Val::Bool(b) => b,
        Val::Str(s) => !s.is_empty(),
        Val::Set(items) => !items.is_empty(),
    }
}

fn eval(doc: &Document<'_>, expr: &Expr, cx: Ctx) -> Val {
    match expr {
        Expr::Operand(op) => eval_operand(doc, op, cx),
        Expr::Cmp(a, op, b) => {
            let left = eval_operand(doc, a, cx);
            let right = eval_operand(doc, b, cx);
            Val::Bool(compare(doc, left, *op, right))
        }
        Expr::And(parts) => Val::Bool(parts.iter().all(|p| truthy(doc, p, cx))),
        Expr::Or(parts) => Val::Bool(parts.iter().any(|p| truthy(doc, p, cx))),
        Expr::Not(inner) => Val::Bool(!truthy(doc, inner, cx)),
        Expr::Contains(a, b) => {
            let a = string_of(doc, eval_operand(doc, a, cx));
            let b = string_of(doc, eval_operand(doc, b, cx));
            Val::Bool(a.contains(&b))
        }
        Expr::StartsWith(a, b) => {
            let a = string_of(doc, eval_operand(doc, a, cx));
            let b = string_of(doc, eval_operand(doc, b, cx));
            Val::Bool(a.starts_with(&b))
        }
        Expr::Const(v) => Val::Bool(*v),
    }
}

fn eval_operand(doc: &Document<'_>, op: &Operand, cx: Ctx) -> Val {
    match op {
        Operand::Str(s) => Val::Str(s.clone()),
        Operand::Num(n) => Val::Num(*n),
        Operand::Position => Val::Num(cx.pos as f64),
        Operand::Last => Val::Num(cx.size as f64),
        Operand::Path(p) => Val::Set(eval_path(doc, p, cx.item)),
        Operand::Normalize(None) => Val::Str(normalize_space(&doc.string_value(cx.item))),
        Operand::Normalize(Some(inner)) => {
            let text = string_of(doc, eval_operand(doc, inner, cx));
            Val::Str(normalize_space(&text))
        }
    }
}

/// The string a value converts to. A node set's string is its first node's.
fn string_of(doc: &Document<'_>, v: Val) -> String {
    match v {
        Val::Str(s) => s,
        Val::Num(n) => num_text(n),
        Val::Bool(b) => b.to_string(),
        Val::Set(items) => items
            .first()
            .map(|&it| doc.string_value(it))
            .unwrap_or_default(),
    }
}

/// Comparison with XPath's existential rule: true when some pair of atoms
/// (one from each side, a node set contributing each node's string-value)
/// satisfies `op`. An empty node set compares false.
fn compare(doc: &Document<'_>, a: Val, op: Cmp, b: Val) -> bool {
    let xs = atoms(doc, a);
    let ys = atoms(doc, b);
    xs.iter().any(|x| ys.iter().any(|y| atom_cmp(x, op, y)))
}

fn atoms(doc: &Document<'_>, v: Val) -> Vec<Atom> {
    match v {
        Val::Num(n) => vec![Atom::Num(n)],
        Val::Str(s) => vec![Atom::Str(s)],
        Val::Bool(b) => vec![Atom::Str(b.to_string())],
        Val::Set(items) => items
            .iter()
            .map(|&it| Atom::Str(doc.string_value(it)))
            .collect(),
    }
}

fn atom_to_number(a: &Atom) -> f64 {
    match a {
        Atom::Num(n) => *n,
        Atom::Str(s) => s.trim().parse().unwrap_or(f64::NAN),
    }
}

fn atom_cmp(x: &Atom, op: Cmp, y: &Atom) -> bool {
    match (x, y, op) {
        (Atom::Str(a), Atom::Str(b), Cmp::Eq) => a == b,
        (Atom::Str(a), Atom::Str(b), Cmp::Ne) => a != b,
        _ => {
            let (a, b) = (atom_to_number(x), atom_to_number(y));
            match op {
                Cmp::Eq => a == b,
                Cmp::Ne => a != b,
                Cmp::Lt => a < b,
                Cmp::Le => a <= b,
                Cmp::Gt => a > b,
                Cmp::Ge => a >= b,
            }
        }
    }
}

/// Run `expression` against `html` and return the rendered results in document
/// order, without duplicates. More than `max_results` results is a refusal, not
/// a prefix.
pub fn evaluate(html: &str, expression: &str, max_results: usize) -> Result<Vec<String>, Refusal> {
    let paths = parse_expression(expression)?;
    let doc = parse(html)?;
    let mut items = Vec::new();
    for path in &paths {
        items.extend(eval_path(&doc, path, Item::Node(0)));
    }
    if doc.work.get() > MAX_WORK {
        return Err(Refusal::Work);
    }
    document_order(&mut items);
    if items.contains(&Item::Node(0)) {
        return Err(Refusal::Expression {
            got: "the document node".into(),
        });
    }
    if items.len() > max_results {
        return Err(Refusal::TooManyResults { limit: max_results });
    }
    Ok(items.iter().map(|&it| doc.render(it)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(html: &str, expr: &str) -> Result<Vec<String>, Refusal> {
        evaluate(html, expr, 1000)
    }

    fn ok(html: &str, expr: &str) -> Vec<String> {
        run(html, expr).unwrap_or_else(|e| panic!("{expr}: {e:?}"))
    }

    fn refused(expr: &str) -> String {
        match run("<p>x</p>", expr) {
            Err(Refusal::Expression { got }) => got,
            other => panic!("{expr}: expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn element_and_attribute_matches() {
        assert_eq!(
            ok("<ul><li>a</li><li>b</li></ul>", "//li"),
            ["<li>a</li>", "<li>b</li>"]
        );
        assert_eq!(ok("<a href='/x'>y</a>", "//a/@href"), ["/x"]);
        assert_eq!(ok("<p>a<b>b</b>c</p>", "//p/text()"), ["a", "c"]);
    }

    #[test]
    fn positional_and_last() {
        let html = "<ul><li>a</li><li>b</li><li>c</li></ul>";
        assert_eq!(ok(html, "//li[2]"), ["<li>b</li>"]);
        assert_eq!(ok(html, "//li[last()]"), ["<li>c</li>"]);
        assert_eq!(ok(html, "//li[position() < 3]").len(), 2);
    }

    #[test]
    fn descendant_position_is_per_parent() {
        let html = "<div><ul><li>1</li><li>2</li></ul><ol><li>3</li></ol></div>";
        assert_eq!(ok(html, "//li[1]"), ["<li>1</li>", "<li>3</li>"]);
    }

    #[test]
    fn unsupported_parts_are_named() {
        assert_eq!(refused("count(//a)"), "count()");
        assert_eq!(refused("//a/following-sibling::b"), "following-sibling::");
        assert_eq!(refused("$x"), "$");
        assert_eq!(refused(""), "empty expression");
        assert_eq!(refused("(//li)[1]"), "(");
    }

    #[test]
    fn entities_decode_in_text_and_attributes() {
        assert_eq!(
            ok(
                "<p title='a&amp;b'>&lt;&#65;&#x42;&bogus;</p>",
                "//p/@title"
            ),
            ["a&b"]
        );
        assert_eq!(
            ok("<p>&lt;&#65;&#x42;&bogus;</p>", "//p/text()"),
            ["<AB&bogus;"]
        );
    }

    #[test]
    fn script_content_is_not_markup() {
        let html = "<div><script>if (a<b) {}</script><p>x</p></div>";
        assert_eq!(
            ok(html, "//*"),
            [
                "<div><script>if (a<b) {}</script><p>x</p></div>",
                "<script>if (a<b) {}</script>",
                "<p>x</p>",
            ]
        );
    }

    #[test]
    fn depth_limit_is_a_budget_refusal() {
        let html = "<p>".repeat(MAX_DEPTH + 2);
        assert_eq!(run(&html, "//p"), Err(Refusal::Depth));
    }

    #[test]
    fn quadratic_work_is_a_budget_refusal() {
        // 500 nested divs around 2,000 spans. `/div//span` visits each node
        // once; `//div//span` once per enclosing div (about 2M); the same
        // path in a predicate runs once per span, 2,000 times that.
        let html = format!(
            "{}{}{}",
            "<div>".repeat(500),
            "<span>s</span>".repeat(2000),
            "</div>".repeat(500)
        );
        assert_eq!(
            evaluate(&html, "/div//span", 5000).map(|v| v.len()),
            Ok(2000)
        );
        assert_eq!(
            evaluate(&html, "//div//span", 5000).map(|v| v.len()),
            Ok(2000)
        );
        assert_eq!(
            evaluate(&html, "//span[//*//span]", 5000),
            Err(Refusal::Work)
        );
    }

    #[test]
    fn result_cap_refuses_rather_than_truncating() {
        assert_eq!(
            evaluate("<i></i><i></i><i></i>", "//i", 2),
            Err(Refusal::TooManyResults { limit: 2 })
        );
    }
}
