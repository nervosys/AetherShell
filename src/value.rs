//! Runtime value model for Aether Shell.
//!
//! - Strong, ergonomic `Value` with helpers (`type_name`, accessors).
//! - Pretty-printing with color via `crossterm` (tables, records, arrays).
//! - A minimal `Uri` type + parser (scheme validation; not limited to HTTP).
//! - JSON interop helpers.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ast::Expr;

/// A very small URI type (not tied to just URLs).
///
/// This is intentionally permissive but checks:
/// - a scheme that begins with an alphabetic char and continues with
/// - alphanumerics, '+', '-', or '.'
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Uri {
    pub raw: String,
    pub scheme: String,
}

#[allow(dead_code)]
impl Uri {
    /// Returns `Some(Uri)` if `s` parses with a valid scheme (RFC3986-ish).
    pub fn parse(s: &str) -> Option<Self> {
        // scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ":"
        let (scheme, _rest) = s.split_once(':')?;

        // must exist
        let mut chars = scheme.chars();
        let first = chars.next()?;
        if !first.is_ascii_alphabetic() {
            return None;
        }
        if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
            return None;
        }
        Some(Self {
            raw: s.to_string(),
            scheme: scheme.to_string(),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// A lambda closure (AST-captured).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lambda {
    pub params: Vec<String>,
    pub body: Box<Expr>,
}

/// An async lambda closure (AST-captured).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AsyncLambda {
    pub params: Vec<String>,
    pub body: Box<Expr>,
}

/// A future value representing an async computation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Future {
    /// The async lambda to execute
    pub lambda: AsyncLambda,
    /// Captured arguments
    pub args: Vec<Value>,
}

/// A simple table representation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Table {
    pub rows: Vec<BTreeMap<String, Value>>,
    pub schema: Vec<String>,
}

/// A reference to a builtin function (for module system)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuiltinRef {
    pub name: String,
}

/// The dynamic value space of the shell.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Uri(String),
    Array(Vec<Value>),
    Record(BTreeMap<String, Value>),
    Table(Table),
    Lambda(Lambda),
    /// An async lambda (not yet called)
    AsyncLambda(AsyncLambda),
    /// A future value that can be awaited
    Future(Future),
    /// An error value for try/catch handling
    Error(String),
    /// A reference to a builtin function
    Builtin(BuiltinRef),
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "Null",
            Value::Bool(_) => "Bool",
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Str(_) => "String",
            Value::Uri(_) => "Uri",
            Value::Array(_) => "Array",
            Value::Record(_) => "Record",
            Value::Table(_) => "Table",
            Value::Lambda(_) => "Lambda",
            Value::AsyncLambda(_) => "AsyncLambda",
            Value::Future(_) => "Future",
            Value::Error(_) => "Error",
            Value::Builtin(_) => "Builtin",
        }
    }

    // ----------- typed accessors (Result) -----------

    pub fn as_bool(&self) -> anyhow::Result<bool> {
        if let Value::Bool(b) = self {
            Ok(*b)
        } else {
            Err(anyhow::anyhow!("expected Bool"))
        }
    }
    pub fn as_int(&self) -> anyhow::Result<i64> {
        if let Value::Int(n) = self {
            Ok(*n)
        } else {
            Err(anyhow::anyhow!("expected Int"))
        }
    }
    pub fn as_float(&self) -> anyhow::Result<f64> {
        if let Value::Float(x) = self {
            Ok(*x)
        } else {
            Err(anyhow::anyhow!("expected Float"))
        }
    }
    pub fn as_str(&self) -> anyhow::Result<&str> {
        if let Value::Str(s) = self {
            Ok(s)
        } else {
            Err(anyhow::anyhow!("expected String"))
        }
    }
    pub fn as_uri(&self) -> anyhow::Result<&str> {
        if let Value::Uri(s) = self {
            Ok(s)
        } else {
            Err(anyhow::anyhow!("expected Uri"))
        }
    }
    pub fn as_array(&self) -> anyhow::Result<&[Value]> {
        if let Value::Array(v) = self {
            Ok(v)
        } else {
            Err(anyhow::anyhow!("expected Array"))
        }
    }
    pub fn as_record(&self) -> anyhow::Result<&BTreeMap<String, Value>> {
        if let Value::Record(m) = self {
            Ok(m)
        } else {
            Err(anyhow::anyhow!("expected Record"))
        }
    }
    pub fn as_table(&self) -> anyhow::Result<&Table> {
        if let Value::Table(t) = self {
            Ok(t)
        } else {
            Err(anyhow::anyhow!("expected Table"))
        }
    }

    /// Convert a Value to a display string (without quotes for Str values)
    pub fn to_display_string(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::Uri(u) => u.clone(),
            Value::Int(n) => n.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| v.to_display_string()).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Record(rec) => {
                let items: Vec<String> = rec
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.to_display_string()))
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
            Value::Table(t) => format!("<Table rows={}>", t.rows.len()),
            Value::Lambda(_) => "<lambda>".to_string(),
            Value::AsyncLambda(_) => "<async lambda>".to_string(),
            Value::Future(_) => "<future>".to_string(),
            Value::Error(msg) => format!("Error: {}", msg),
            Value::Builtin(b) => format!("<builtin:{}>", b.name),
        }
    }

    // ----------- JSON interop -----------

    pub fn from_json(v: &serde_json::Value) -> Self {
        use serde_json::Value as J;
        match v {
            J::Null => Value::Null,
            J::Bool(b) => Value::Bool(*b),
            J::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Int(i)
                } else if let Some(f) = n.as_f64() {
                    Value::Float(f)
                } else {
                    Value::Null
                }
            }
            J::String(s) => Value::Str(s.clone()),
            J::Array(a) => Value::Array(a.iter().map(Self::from_json).collect()),
            J::Object(m) => {
                let mut rec = BTreeMap::new();
                for (k, v) in m {
                    rec.insert(k.clone(), Self::from_json(v));
                }
                Value::Record(rec)
            }
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => json!(b),
            Value::Int(n) => json!(n),
            Value::Float(x) => json!(x),
            Value::Str(s) => json!(s),
            Value::Uri(u) => json!(u),
            Value::Array(a) => serde_json::Value::Array(a.iter().map(|v| v.to_json()).collect()),
            Value::Record(m) => {
                let mut obj = serde_json::Map::new();
                for (k, v) in m {
                    obj.insert(k.clone(), v.to_json());
                }
                serde_json::Value::Object(obj)
            }
            Value::Table(t) => {
                let mut rows = Vec::new();
                for r in &t.rows {
                    let mut obj = serde_json::Map::new();
                    for (k, v) in r {
                        obj.insert(k.clone(), v.to_json());
                    }
                    rows.push(serde_json::Value::Object(obj));
                }
                serde_json::json!({ "schema": t.schema, "rows": rows })
            }
            Value::Lambda(_) => json!("<lambda>"),
            Value::AsyncLambda(_) => json!("<async lambda>"),
            Value::Future(_) => json!("<future>"),
            Value::Error(msg) => json!({"error": msg}),
            Value::Builtin(b) => json!({"_type": "Builtin", "name": b.name}),
        }
    }
}

// ----------- Pretty printing (crossterm) - native only -----------

#[cfg(feature = "native")]
pub mod pretty {
    use super::*;
    use crossterm::style::{StyledContent, Stylize};

    #[derive(Debug, Clone)]
    pub struct Theme {
        pub key: fn(&str) -> StyledContent<String>,
        pub string: fn(&str) -> StyledContent<String>,
        pub number: fn(&str) -> StyledContent<String>,
        pub boolean: fn(&str) -> StyledContent<String>,
        pub null: fn(&str) -> StyledContent<String>,
        pub uri: fn(&str) -> StyledContent<String>,
        pub header: fn(&str) -> StyledContent<String>,
        pub dim: fn(&str) -> StyledContent<String>,
    }

    impl Theme {
        /// A theme that emits no escape sequences at all.
        ///
        /// `ContentStyle::default()` sets no attributes, so crossterm writes
        /// the content unchanged. Used whenever the destination is not a
        /// terminal, so that `print(x) | grep` and `$(...)` capture see the
        /// same characters a human does.
        pub fn plain() -> Self {
            Self {
                key: |s| s.to_string().stylize(),
                string: |s| s.to_string().stylize(),
                number: |s| s.to_string().stylize(),
                boolean: |s| s.to_string().stylize(),
                null: |s| s.to_string().stylize(),
                uri: |s| s.to_string().stylize(),
                header: |s| s.to_string().stylize(),
                dim: |s| s.to_string().stylize(),
            }
        }

        /// The default theme when the destination can show colour, the plain
        /// one otherwise.
        pub fn for_stdout() -> Self {
            if crate::config::colors_enabled() {
                Self::default()
            } else {
                Self::plain()
            }
        }
    }

    impl Default for Theme {
        fn default() -> Self {
            Self {
                key: |s| s.to_string().cyan(),
                string: |s| s.to_string().green(),
                number: |s| s.to_string().blue(),
                boolean: |s| s.to_string().magenta(),
                null: |s| s.to_string().dark_grey(),
                uri: |s| s.to_string().yellow(),
                header: |s| s.to_string().bold().underlined(),
                dim: |s| s.to_string().dark_grey(),
            }
        }
    }

    /// Write a `Value` to any `fmt::Write`, using colors.
    pub fn fmt_value<W: fmt::Write>(w: &mut W, v: &Value, theme: &Theme) -> fmt::Result {
        match v {
            Value::Null => write!(w, "{}", (theme.null)("null")),
            Value::Bool(b) => write!(w, "{}", (theme.boolean)(&b.to_string())),
            Value::Int(n) => write!(w, "{}", (theme.number)(&n.to_string())),
            Value::Float(x) => write!(w, "{}", (theme.number)(&x.to_string())),
            Value::Str(s) => write!(w, "\"{}\"", (theme.string)(s)),
            Value::Uri(u) => write!(w, "{}", (theme.uri)(u)),
            Value::Array(a) => {
                write!(w, "[")?;
                for (i, el) in a.iter().enumerate() {
                    if i > 0 {
                        write!(w, ", ")?;
                    }
                    fmt_value(w, el, theme)?;
                }
                write!(w, "]")
            }
            Value::Record(m) => {
                write!(w, "{{")?;
                let mut first = true;
                for (k, v) in m {
                    if !first {
                        write!(w, ", ")?;
                    } else {
                        first = false;
                    }
                    write!(w, "{}: ", (theme.key)(k))?;
                    fmt_value(w, v, theme)?;
                }
                write!(w, "}}")
            }
            Value::Table(t) => fmt_table(w, t, theme),
            Value::Lambda(_) => write!(w, "{}", (theme.dim)("<lambda>")),
            Value::AsyncLambda(_) => write!(w, "{}", (theme.dim)("<async lambda>")),
            Value::Future(_) => write!(w, "{}", (theme.dim)("<future>")),
            Value::Error(msg) => write!(w, "Error: {}", msg),
            Value::Builtin(b) => write!(w, "{}", (theme.dim)(&format!("<builtin:{}>", b.name))),
        }
    }

    fn fmt_table<W: fmt::Write>(w: &mut W, t: &Table, theme: &Theme) -> fmt::Result {
        if t.schema.is_empty() {
            return write!(w, "{}", (theme.dim)("<Table rows=0 cols=0>"));
        }

        // Compute column widths from schema + a sample of rows
        let mut widths: Vec<usize> = t.schema.iter().map(|s| s.len()).collect();
        let sample = t.rows.iter().take(50);
        for r in sample {
            for (i, col) in t.schema.iter().enumerate() {
                let cell = r.get(col).map(summarize).unwrap_or_default();
                widths[i] = widths[i].max(cell.len());
            }
        }

        // Header
        for (i, col) in t.schema.iter().enumerate() {
            if i > 0 {
                write!(w, " ")?;
            }
            let h = (theme.header)(col).to_string();
            write!(w, "{h:width$}", width = widths[i])?;
        }
        writeln!(w)?;

        // Rows
        for r in &t.rows {
            for (i, col) in t.schema.iter().enumerate() {
                if i > 0 {
                    write!(w, " ")?;
                }
                let cell = r
                    .get(col)
                    .map(|v| display_inline(v, theme))
                    .unwrap_or_else(|| (theme.dim)("-").to_string());
                write!(w, "{cell:width$}", width = widths[i])?;
            }
            writeln!(w)?;
        }
        Ok(())
    }

    fn summarize(v: &Value) -> String {
        match v {
            Value::Null => "null".into(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(x) => x.to_string(),
            Value::Str(s) => truncate(s, 40),
            Value::Uri(u) => truncate(u, 48),
            Value::Array(a) => format!("[{}]", a.len()),
            Value::Record(_) => "{…}".into(),
            Value::Table(t) => format!("<Table rows={}>", t.rows.len()),
            Value::Lambda(_) => "<lambda>".into(),
            Value::AsyncLambda(_) => "<async lambda>".into(),
            Value::Future(_) => "<future>".into(),
            Value::Error(msg) => format!("Error: {}", truncate(msg, 30)),
            Value::Builtin(b) => format!("<builtin:{}>", b.name),
        }
    }

    /// Render a value in full, with no length cap.
    ///
    /// `display_inline` truncates at 80 characters, which is right for the
    /// compact diagnostics `debug` and `trace` emit and wrong for `print`:
    /// `print` is how a script produces output, and it was silently discarding
    /// everything past the 80th character of any string.
    pub fn display_full(v: &Value, theme: &Theme) -> String {
        let mut buf = String::new();
        let _ = fmt_value(&mut buf, v, theme);
        buf
    }

    pub fn display_inline(v: &Value, theme: &Theme) -> String {
        let mut buf = String::new();
        let _ = fmt_value(&mut buf, v, theme);
        truncate(&buf, 80)
    }

    /// Truncate to at most `n` *characters*.
    ///
    /// This used to slice `&s[..n]` on a byte index, which panics whenever byte
    /// `n` lands inside a multi-byte character:
    ///
    /// ```text
    /// end byte index 80 is not a char boundary; it is inside '─'
    /// ```
    ///
    /// Five shipped examples crashed the shell that way — any box-drawing rule,
    /// accented word or emoji long enough to reach the limit did it. Counting
    /// characters also makes the limit mean what it says: 80 bytes of CJK is
    /// 26 glyphs, not 80.
    fn truncate(s: &str, n: usize) -> String {
        if s.chars().count() <= n {
            s.to_string()
        } else {
            let cut: String = s.chars().take(n).collect();
            format!("{}…", cut)
        }
    }
}

// ----------- Minimal Display -----------

#[cfg(feature = "native")]
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::value::pretty::{fmt_value, Theme};
        let theme = Theme::default();
        fmt_value(f, self, &theme)
    }
}

#[cfg(not(feature = "native"))]
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(x) => write!(f, "{}", x),
            Value::Str(s) => write!(f, "\"{}\"", s),
            Value::Uri(u) => write!(f, "{}", u),
            Value::Array(a) => {
                write!(f, "[")?;
                for (i, el) in a.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", el)?;
                }
                write!(f, "]")
            }
            Value::Record(m) => {
                write!(f, "{{")?;
                let mut first = true;
                for (k, v) in m {
                    if !first {
                        write!(f, ", ")?;
                    } else {
                        first = false;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Table(t) => write!(f, "<Table rows={} cols={}>", t.rows.len(), t.schema.len()),
            Value::Lambda(_) => write!(f, "<lambda>"),
            Value::AsyncLambda(_) => write!(f, "<async lambda>"),
            Value::Future(_) => write!(f, "<future>"),
            Value::Error(msg) => write!(f, "Error: {}", msg),
            // Mirrors the native renderer, which prints `<builtin:name>`.
            // Omitting this arm broke the wasm32 build outright.
            Value::Builtin(b) => write!(f, "<builtin:{}>", b.name),
        }
    }
}
