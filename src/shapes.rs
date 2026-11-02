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
        Value::Array(items) => match items.split_first() {
            None => "array<any>".to_string(),
            Some((first, rest)) => {
                // The first element's shape is rendered once; every other row is
                // compared against it without rendering its own string.
                let first = observe(first);
                if rest.iter().all(|item| shape_is(item, &first)) {
                    format!("array<{first}>")
                } else {
                    "array<any>".to_string()
                }
            }
        },
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

/// Whether `observe(v) == expected`, without rendering `v`'s shape to a string.
///
/// `observe` is the reference: this walks the same fragments, in the same order,
/// that `observe` would concatenate, and checks each against `expected` as it
/// goes. Comparing the emitted text (rather than the value's structure) keeps the
/// answer identical even where two different structures render the same — a key
/// such as `a:int,b` can make one record look like two fields. Nested arrays are
/// the one case rendered whole, since their element agreement needs `observe`.
fn shape_is(v: &Value, expected: &str) -> bool {
    let want = expected.as_bytes();
    let mut pos = 0usize;
    let matched = emit_fragments(v, &mut |frag: &str| {
        let end = pos + frag.len();
        if want.get(pos..end) == Some(frag.as_bytes()) {
            pos = end;
            true
        } else {
            false
        }
    });
    matched && pos == want.len()
}

/// Feed each fragment of `observe(v)`, in order, to `sink`. Stops and returns
/// `false` as soon as `sink` refuses one.
fn emit_fragments<F: FnMut(&str) -> bool>(v: &Value, sink: &mut F) -> bool {
    match v {
        Value::Null => sink("null"),
        Value::Bool(_) => sink("bool"),
        Value::Int(_) => sink("int"),
        Value::Float(_) => sink("float"),
        Value::Str(_) => sink("str"),
        Value::Uri(_) => sink("uri"),
        Value::Table(_) => sink("table"),
        Value::Array(_) => sink(&observe(v)),
        Value::Record(map) => {
            if !sink("record{") {
                return false;
            }
            for (i, (k, val)) in map.iter().take(MAX_FIELDS).enumerate() {
                if i > 0 && !sink(",") {
                    return false;
                }
                if !sink(k.as_str()) || !sink(":") || !emit_fragments(val, sink) {
                    return false;
                }
            }
            // `more > 0` only when at least MAX_FIELDS fields were emitted above,
            // so the elision marker always follows a comma.
            let more = map.len().saturating_sub(MAX_FIELDS);
            if more > 0 {
                let marker = format!("+{more}");
                if !sink(",") || !sink(marker.as_str()) {
                    return false;
                }
            }
            sink("}")
        }
        _ => sink("any"),
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
        "record{categories:array<record{builtins:int,category:str,effects:array<str>}>,effect_legend:array<str>,hint:str,modules:str,ontology:str,total_builtins:int}",
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

    /// The pre-optimisation `observe`, kept verbatim as the oracle. The fast path
    /// must agree with it on every value, so it is not allowed to drift.
    fn observe_reference(v: &Value) -> String {
        match v {
            Value::Null => "null".to_string(),
            Value::Bool(_) => "bool".to_string(),
            Value::Int(_) => "int".to_string(),
            Value::Float(_) => "float".to_string(),
            Value::Str(_) => "str".to_string(),
            Value::Uri(_) => "uri".to_string(),
            Value::Table(_) => "table".to_string(),
            Value::Array(items) => {
                let mut it = items.iter().map(observe_reference);
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
                    parts.push(format!("{k}:{}", observe_reference(val)));
                }
                let more = map.len().saturating_sub(MAX_FIELDS);
                if more > 0 {
                    parts.push(format!("+{more}"));
                }
                format!("record{{{}}}", parts.join(","))
            }
            _ => "any".to_string(),
        }
    }

    fn int_row(id: i64) -> Value {
        rec(&[("id", Value::Int(id)), ("name", Value::Str("row".into()))])
    }

    fn wide(extra: usize, last: Value) -> Value {
        let mut m = BTreeMap::new();
        for i in 0..(MAX_FIELDS + extra) {
            m.insert(format!("f{i:02}"), Value::Int(i as i64));
        }
        m.insert(format!("f{:02}", MAX_FIELDS + extra - 1), last);
        Value::Record(m)
    }

    fn varied_values() -> Vec<Value> {
        let table = Value::Table(crate::value::Table {
            rows: vec![],
            schema: vec!["a".into()],
        });
        let mut differing_last: Vec<Value> = (0..1000).map(int_row).collect();
        differing_last.push(rec(&[
            ("id", Value::Str("x".into())),
            ("name", Value::Str("row".into())),
        ]));
        let mut extra_field_last: Vec<Value> = (0..1000).map(int_row).collect();
        extra_field_last.push(rec(&[
            ("id", Value::Int(1)),
            ("name", Value::Str("row".into())),
            ("extra", Value::Bool(true)),
        ]));
        vec![
            Value::Null,
            Value::Bool(false),
            Value::Int(-7),
            Value::Float(1.5),
            Value::Str("s".into()),
            Value::Uri("https://example.com".into()),
            table.clone(),
            Value::Array(vec![]),
            Value::Array(vec![table.clone(), table.clone()]),
            Value::Array(vec![Value::Int(1), Value::Int(2)]),
            Value::Array(vec![Value::Int(1), Value::Str("a".into())]),
            Value::Array(vec![Value::Null, Value::Null]),
            Value::Array(vec![
                Value::Array(vec![]),
                Value::Array(vec![Value::Int(1)]),
            ]),
            Value::Array(vec![
                Value::Array(vec![Value::Int(1)]),
                Value::Array(vec![Value::Int(2)]),
            ]),
            Value::Array(vec![
                Value::Array(vec![Value::Int(1)]),
                Value::Array(vec![Value::Str("x".into())]),
            ]),
            rec(&[]),
            rec(&[("n", Value::Null), ("s", Value::Str("".into()))]),
            rec(&[("inner", rec(&[("deep", Value::Array(vec![int_row(1)]))]))]),
            // A ragged array of records, and records that differ only in value.
            Value::Array(vec![int_row(1), rec(&[("id", Value::Int(2))])]),
            Value::Array((0..50).map(int_row).collect()),
            Value::Array(differing_last),
            Value::Array(extra_field_last),
            // Two records whose rendering collides: one key `a:int,b` of type str
            // renders exactly like the two fields `a:int` and `b:str`.
            Value::Array(vec![
                rec(&[("a:int,b", Value::Str("x".into()))]),
                rec(&[("a", Value::Int(1)), ("b", Value::Str("x".into()))]),
            ]),
            // A field beyond the 12-field window does not change the shape.
            Value::Array(vec![
                wide(2, Value::Int(9)),
                wide(2, Value::Str("hidden".into())),
            ]),
            // A difference inside the window does: `f00` is a str in the second row.
            Value::Array(vec![wide(2, Value::Int(9)), {
                let mut m = BTreeMap::new();
                for i in 0..(MAX_FIELDS + 2) {
                    let val = if i == 0 {
                        Value::Str("x".into())
                    } else {
                        Value::Int(i as i64)
                    };
                    m.insert(format!("f{i:02}"), val);
                }
                Value::Record(m)
            }]),
            // A wide record whose elided count differs.
            Value::Array(vec![wide(0, Value::Int(0)), wide(3, Value::Int(0))]),
        ]
    }

    #[test]
    fn observe_matches_the_reference_on_varied_values() {
        for v in varied_values() {
            assert_eq!(observe(&v), observe_reference(&v), "value {v:?}");
        }
    }

    #[test]
    fn a_differing_row_at_the_very_end_is_not_skipped() {
        let mut rows: Vec<Value> = (0..100_000).map(int_row).collect();
        assert_eq!(
            observe(&Value::Array(rows.clone())),
            "array<record{id:int,name:str}>"
        );
        rows.push(rec(&[
            ("id", Value::Str("x".into())),
            ("name", Value::Str("row".into())),
        ]));
        assert_eq!(observe(&Value::Array(rows)), "array<any>");
    }

    /// Small deterministic generator, so a failure is reproducible from its seed.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    fn gen_key(g: &mut Lcg) -> String {
        // Characters that appear in the rendering, so collisions are reachable.
        const ALPHABET: &[&str] = &["a", "b", "i", "n", "t", ":", ",", "{", "}", "+"];
        let len = 1 + g.below(3);
        (0..len)
            .map(|_| ALPHABET[g.below(ALPHABET.len() as u64) as usize])
            .collect()
    }

    fn gen_value(g: &mut Lcg, depth: u32) -> Value {
        let kinds = if depth == 0 { 7 } else { 9 };
        match g.below(kinds) {
            0 => Value::Null,
            1 => Value::Bool(g.below(2) == 0),
            2 => Value::Int(g.below(100) as i64),
            3 => Value::Float(0.5),
            4 => Value::Str(gen_key(g)),
            5 => Value::Uri("u".into()),
            6 => Value::Table(crate::value::Table {
                rows: vec![],
                schema: vec![],
            }),
            7 => {
                // Uniform arrays are the interesting case: most rows share a
                // prototype, and an occasional one differs, at any position.
                let proto = gen_value(g, depth - 1);
                let len = g.below(6);
                let items = (0..len)
                    .map(|_| {
                        if g.below(5) == 0 {
                            gen_value(g, depth - 1)
                        } else {
                            proto.clone()
                        }
                    })
                    .collect();
                Value::Array(items)
            }
            _ => {
                let mut m = BTreeMap::new();
                for _ in 0..g.below(16) {
                    m.insert(gen_key(g), gen_value(g, depth - 1));
                }
                Value::Record(m)
            }
        }
    }

    #[test]
    fn observe_matches_the_reference_on_generated_values() {
        let mut g = Lcg(0x5eed);
        for _ in 0..4000 {
            let v = gen_value(&mut g, 3);
            assert_eq!(observe(&v), observe_reference(&v), "value {v:?}");
        }
    }

    #[test]
    fn the_comparator_agrees_with_string_equality_on_generated_pairs() {
        // The fast path's real claim: `shape_is(v, observe(w))` is exactly the
        // reference's `observe(v) == observe(w)`, for both answers.
        let mut g = Lcg(0xfeed);
        let mut equal = 0;
        for _ in 0..20_000 {
            let v = gen_value(&mut g, 2);
            let w = gen_value(&mut g, 2);
            let expected = observe_reference(&v) == observe_reference(&w);
            assert_eq!(shape_is(&v, &observe(&w)), expected, "v {v:?} w {w:?}");
            equal += usize::from(expected);
        }
        assert!(equal > 0, "the generator never produced equal shapes");
    }
}
