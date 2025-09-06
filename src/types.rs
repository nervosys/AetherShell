// Shared semantic types for Aether's checker & public APIs.
use std::collections::BTreeMap;

/// The Aether semantic type lattice used by the typechecker and (optionally) by
/// runtime-facing surfaces (e.g., schema hints for tables).
///
/// Keep this file lightweight and dependency-free so tests can import
/// `aether_shell::types::Type` without pulling in other modules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// Unknown or unconstrained type. Unifies with anything.
    Any,

    // Scalars
    Null,
    Bool,
    Int,
    Float,
    String,
    /// RFC 3986-ish URI (not limited to http/https).
    Uri,

    // Collections
    Array(Box<Type>),
    /// Heterogeneous record with string keys.
    Record(BTreeMap<String, Type>),

    /// Table with a (possibly partial) schema: column name -> type.
    /// An empty map represents an unknown/unspecified schema.
    Table(BTreeMap<String, Type>),

    /// Lambda with positional parameter types and a return type.
    /// During inference, parameters may be `Type::Any`.
    Lambda(Vec<Type>, Box<Type>),
}

impl Type {
    /// A short human-readable name for diagnostics.
    pub fn name(&self) -> String {
        use Type::*;
        match self {
            Any => "Any".into(),
            Null => "Null".into(),
            Bool => "Bool".into(),
            Int => "Int".into(),
            Float => "Float".into(),
            String => "String".into(),
            Uri => "Uri".into(),
            Array(t) => format!("Array<{}>", t.name()),
            Record(fields) => {
                // Show up to a few fields for brevity.
                let mut parts: Vec<std::string::String> = fields
                    .iter()
                    .take(4)
                    .map(|(k, v)| format!("{k}: {}", v.name()))
                    .collect();
                if fields.len() > 4 {
                    parts.push("…".into());
                }
                format!("Record{{{}}}", parts.join(", "))
            }
            Table(cols) => {
                let mut parts: Vec<std::string::String> = cols
                    .iter()
                    .take(4)
                    .map(|(k, v)| format!("{k}: {}", v.name()))
                    .collect();
                if cols.len() > 4 {
                    parts.push("…".into());
                }
                format!("Table{{{}}}", parts.join(", "))
            }
            Lambda(params, ret) => {
                let ps = params
                    .iter()
                    .map(|t| t.name())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("fn({}) -> {}", ps, ret.name())
            }
        }
    }

    /// Convenience predicate for numeric types.
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Int | Type::Float)
    }
}
