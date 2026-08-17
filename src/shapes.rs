//! Return shapes: what a builtin gives back, advertised before it is called.
//!
//! The agent-facing surface has always described *inputs* (`json_schema`) and,
//! since the effect work, *danger* (`x-effect`). It has never described the
//! **result**. With ~1,300 builtins an agent meeting an unfamiliar one has no
//! choice but to run it to discover the shape, read the output, and only then
//! write the pipeline it wanted — a wasted round-trip on the common case, and
//! the reason a typed pipeline language still gets driven like a text shell.
//!
//! A declared shape turns exploration into composition: knowing `git_status`
//! returns `array<record{path:str,staged:bool,…}>` is enough to write
//! `git_status() | where(fn(f) => f.staged) | select("path")` correctly, first
//! try, without the intermediate rows ever entering the context window.
//!
//! # Only proven shapes are advertised
//!
//! Declaring a shape from a builtin's *name* is precisely the reasoning that
//! produced 28 misclassified effects and then 306 more (see `safety::effect_of`
//! and `tests/effect_ratchet.rs`). So this module refuses to guess: every entry
//! in [`DECLARED`] must be reproduced by actually calling the builtin in
//! `tests/return_shapes.rs`, and that test fails both on a declared shape with
//! no proof and on a proof that disagrees with what was declared.
//!
//! The consequence is that [`DECLARED`] is *small and true* rather than large
//! and aspirational. It grows by adding a probe, not by adding a claim.

use crate::value::Value;

/// Fields listed before a record shape is elided. A shape is a hint for
/// composition, not a schema dump; past a dozen fields it costs more tokens
/// than the round-trip it saves.
const MAX_FIELDS: usize = 12;

/// The shape of a value as actually observed, in the compact notation used by
/// [`DECLARED`].
///
/// Scalars render as their type name. An array renders as `array<T>` when every
/// element agrees and `array<any>` when they do not — a ragged array is a real
/// property of the result, so it is reported rather than smoothed over. An
/// empty array is `array<any>`: nothing was observed, and claiming an element
/// type from zero elements would be a guess.
pub fn observe(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "bool".to_string(),
        Value::Int(_) => "int".to_string(),
        Value::Float(_) => "float".to_string(),
        Value::Str(_) => "str".to_string(),
        Value::Uri(_) => "uri".to_string(),
        Value::Table(_) => "table".to_string(),
        Value::Array(items) => {
            let mut it = items.iter().map(observe);
            match it.next() {
                None => "array<any>".to_string(),
                Some(first) => {
                    if it.all(|s| s == first) {
                        format!("array<{first}>")
                    } else {
                        "array<any>".to_string()
                    }
                }
            }
        }
        Value::Record(map) => {
            let mut parts = Vec::new();
            for (k, val) in map.iter().take(MAX_FIELDS) {
                parts.push(format!("{k}:{}", observe(val)));
            }
            let more = map.len().saturating_sub(MAX_FIELDS);
            if more > 0 {
                parts.push(format!("+{more}"));
            }
            format!("record{{{}}}", parts.join(","))
        }
        // Everything else is a value an agent cannot usefully destructure.
        _ => "any".to_string(),
    }
}

/// The advertised return shape of a builtin, or `None` when none has been
/// proven. `None` means "not established", never "returns nothing" — the
/// absence of a claim is deliberate and is not evidence about the builtin.
pub fn shape_of(name: &str) -> Option<&'static str> {
    DECLARED
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, shape)| *shape)
}

/// How many builtins currently carry a proven shape. Surfaced so the coverage
/// is visible rather than assumed — the same reason `effect_coverage` reports
/// its fall-through count.
pub fn declared_count() -> usize {
    DECLARED.len()
}

/// Builtin → return shape, every entry proven by `tests/return_shapes.rs`.
///
/// Sorted by name. To add one, add a probe to that test; a claim without a
/// probe fails the build.
/// The shape notation understands one variable, `T`, bound to the element type
/// of the first argument.
///
/// This exists because refusing to describe `first`, `values`, `unique` and
/// `reverse` left the most-used combinators in the language undocumented — and
/// their shapes are not unknown, only *relative*. `first` is not "some type";
/// it is exactly the element type of what you passed it. Saying `T` says that,
/// and it stays honest: a claim that can be checked against two probes with
/// different element types, which is how the fixed-shape entries are checked.
///
/// `sum` is still absent, and deliberately. It yields `int` for integers and
/// `float` for floats — that is not the argument's element type, it is a
/// promotion rule this notation cannot express. Better silent than approximate.
pub const ELEMENT_VAR: &str = "T";

/// Builtins whose result shape is relative to their first argument.
///
/// Proven the same way as [`DECLARED`]: `tests/return_shapes.rs` calls each with
/// two different element types and checks the *relationship* holds, rather than
/// checking a single concrete answer that would only reflect the test's data.
pub const POLYMORPHIC: &[(&str, &str)] = &[
    ("first", "T"),
    ("last", "T"),
    ("reverse", "array<T>"),
    ("take", "array<T>"),
    ("unique", "array<T>"),
    ("values", "array<T>"),
];

/// The advertised shape of a polymorphic builtin, in terms of `T`.
pub fn polymorphic_shape_of(name: &str) -> Option<&'static str> {
    POLYMORPHIC
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| *s)
}

/// Substitute the element type into a polymorphic shape:
/// `array<T>` with `T = int` becomes `array<int>`.
pub fn instantiate(shape: &str, element: &str) -> String {
    shape.replace(ELEMENT_VAR, element)
}

/// The element type of a value, i.e. what `T` binds to.
pub fn element_of(v: &Value) -> Option<String> {
    match v {
        Value::Array(items) => {
            let mut it = items.iter().map(observe);
            let first = it.next()?;
            if it.all(|s| s == first) {
                Some(first)
            } else {
                None
            }
        }
        Value::Record(m) => {
            let mut it = m.values().map(observe);
            let first = it.next()?;
            if it.all(|s| s == first) {
                Some(first)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// A concrete example of a field's value, where the *type* is not enough to
/// write a correct predicate.
///
/// Found by using the shell as an agent. `ls` declares `ext:str`, which is
/// true, and reading it I wrote `where(fn(f) => f.ext == "rs")` — which matched
/// nothing, because the value is `".rs"` with a leading dot. The filter did not
/// error; it returned an empty set, which is the worst possible failure for an
/// agent because it is a plausible answer. A type tells you how to *hold* a
/// value; an example tells you how to *compare* it.
///
/// Only fields whose format is genuinely surprising belong here. A field that
/// looks like what it is does not need an example, and padding this table would
/// spend the tokens the shapes exist to save.
/// `ls().path` is platform-shaped, so a single literal would be a lie on every
/// platform but the one it was captured on. Windows returns a verbatim path;
/// POSIX returns a POSIX one. An agent filtering on this field needs the form
/// its *own* host produces, not the form the example's author happened to have.
#[cfg(windows)]
pub const LS_PATH_EXAMPLE: &str = "\\\\?\\C:\\...\\src\\agent.rs";
/// See [`LS_PATH_EXAMPLE`] (windows).
#[cfg(not(windows))]
pub const LS_PATH_EXAMPLE: &str = "/.../src/agent.rs";

pub const FIELD_EXAMPLES: &[(&str, &str, &str)] = &[
    ("ls", "ext", ".rs"),
    ("ls", "path", LS_PATH_EXAMPLE),
    ("ls", "modified", "1770766991"),
];

/// Example values for a builtin's fields, for inclusion beside its shape.
pub fn field_examples(name: &str) -> Vec<(&'static str, &'static str)> {
    FIELD_EXAMPLES
        .iter()
        .filter(|(b, _, _)| *b == name)
        .map(|(_, field, example)| (*field, *example))
        .collect()
}

/// Note what remains *absent*. `sum` is probed and refused; see [`ELEMENT_VAR`].
pub const DECLARED: &[(&str, &str)] = &[
    ("aecon", "str"),
    ("keys", "array<str>"),
    ("len", "int"),
    // The element shape describes a *populated* listing; an empty directory
    // yields `array<any>`, since nothing was observed to describe.
    (
        "ls",
        "array<record{ext:str,is_dir:bool,modified:int,name:str,path:str,size:int}>",
    ),
    (
        "ontology_manifest",
        "record{categories:array<record{builtins:int,category:str,effects:array<str>}>,effect_legend:array<str>,hint:str,ontology:str,total_builtins:int}",
    ),
    ("pwd", "str"),
    ("range", "array<int>"),
    ("split", "array<str>"),
    ("tokens", "int"),
    ("type_of", "str"),
    ("upper", "str"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn rec(pairs: &[(&str, Value)]) -> Value {
        let mut m = BTreeMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v.clone());
        }
        Value::Record(m)
    }

    #[test]
    fn scalars_observe_as_their_type() {
        assert_eq!(observe(&Value::Int(1)), "int");
        assert_eq!(observe(&Value::Str("a".into())), "str");
        assert_eq!(observe(&Value::Bool(true)), "bool");
        assert_eq!(observe(&Value::Null), "null");
    }

    #[test]
    fn a_uniform_array_reports_its_element_type() {
        let v = Value::Array(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(observe(&v), "array<int>");
    }

    #[test]
    fn a_ragged_array_is_reported_as_ragged_not_smoothed_over() {
        // Claiming `array<int>` here would be a shape the agent could not rely
        // on — raggedness is a real property of the result.
        let v = Value::Array(vec![Value::Int(1), Value::Str("a".into())]);
        assert_eq!(observe(&v), "array<any>");
    }

    #[test]
    fn an_empty_array_claims_nothing_about_its_elements() {
        assert_eq!(observe(&Value::Array(vec![])), "array<any>");
    }

    #[test]
    fn records_list_fields_with_their_types() {
        let v = rec(&[("b", Value::Str("x".into())), ("a", Value::Int(1))]);
        // BTreeMap ordering makes the rendering stable, which is what lets a
        // declared shape be compared for equality at all.
        assert_eq!(observe(&v), "record{a:int,b:str}");
    }

    #[test]
    fn nested_records_nest() {
        let v = rec(&[("inner", rec(&[("n", Value::Int(1))]))]);
        assert_eq!(observe(&v), "record{inner:record{n:int}}");
    }

    #[test]
    fn a_wide_record_is_elided_with_a_count_rather_than_truncated_silently() {
        let mut m = BTreeMap::new();
        for i in 0..(MAX_FIELDS + 3) {
            m.insert(format!("f{i:02}"), Value::Int(i as i64));
        }
        let s = observe(&Value::Record(m));
        assert!(s.ends_with("+3}"), "expected an elision count, got {s}");
    }

    #[test]
    fn shape_of_is_silent_where_nothing_is_proven() {
        assert_eq!(shape_of("pwd"), Some("str"));
        // Polymorphic: proven *not* to have a fixed shape, so nothing is claimed.
        assert_eq!(shape_of("first"), None);
        assert_eq!(shape_of("no_such_builtin"), None);
    }
}
