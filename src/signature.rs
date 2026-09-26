//! Declared builtin signatures, enforced at dispatch and read by the ontology.
//!
//! # Why this exists
//!
//! Six defects surfaced in one week of benchmarking, and they were one design
//! property observed six times:
//!
//! | Call | Answered |
//! | --- | --- |
//! | `round(4.966, 2)` | `5` — the digits argument was accepted and discarded |
//! | `mean \| round(2)` | `2` — the digit count was taken as the subject |
//! | `env(name, default)` | the default was accepted and discarded |
//! | `echo $HOME` (compat) | `null`, exit 0 |
//! | `1 / 0`, `1 % 0` | `inf` at exit 0; a panic |
//! | `db_json_to_sqlite("x")` | `false`, exit 0 |
//!
//! Every one is a call the builtin could not honour, answered with a value
//! instead of a refusal. The cause is the calling convention: a builtin is
//! `fn(Vec<Value>, Option<Value>) -> Result<Value>`, which carries no schema,
//! so nothing can check arity or types before the body runs and the body is
//! free to ignore what it was handed.
//!
//! # And why the ontology could not catch it
//!
//! The discovery surface was a *second*, independent description. Nineteen
//! builtins have hand-written definitions in `agent_api`; the rest are
//! generated from the builtin's **name** — `describe_builtin_name` splits on
//! `_`, treats the first word as a module and title-cases the rest. So
//! `db_json_to_sqlite` is documented as "Database: json to sqlite" with an
//! empty parameter list, and `max` was documented as "Get maximum value,
//! `max() -> Number`" while refusing the array that description implies.
//!
//! An agent asking the shell what a function does was being told what its name
//! looks like. That is name-based reasoning, and it produced exactly the class
//! of bug it always produces.
//!
//! # The fix
//!
//! One declaration per builtin, used for both jobs. [`validate`] runs at
//! dispatch, so a call that does not match is refused before the body sees it;
//! the same declaration supplies the ontology, so the catalogue cannot describe
//! a function that does not exist.
//!
//! This is a **vertical slice**, not a migration. Builtins without a
//! declaration dispatch exactly as they did before, so nothing regresses;
//! `SIGNATURES` currently covers the ones that failed above plus a core of
//! everyday verbs. `tests/declared_signatures.rs` asserts that every declared
//! builtin is real, that its ontology entry comes from the declaration rather
//! than from its name, and that violating a declaration is refused.

use crate::value::Value;

/// The shape a parameter or a pipeline subject accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Any,
    Int,
    Numeric,
    Str,
    Bool,
    Array,
    Record,
    Lambda,
    /// An array, or a string treated as its lines.
    ///
    /// `Numeric` is the precedent: a union in the lattice because the
    /// builtins genuinely accept either. `sort`, `unique` and `uniq` all
    /// split a piped string on newlines on purpose -- `cat f | sort` is
    /// the oldest idiom in the shell -- and their own error text already
    /// said "input must be an array or string". Declaring them `Array`
    /// deleted the text form, and it took a harness comparing every
    /// declaration against its own body to notice.
    Sequence,
}

impl Ty {
    /// The name used in a signature string and in a refusal.
    pub fn as_str(self) -> &'static str {
        match self {
            Ty::Any => "Any",
            Ty::Int => "Int",
            Ty::Numeric => "Number",
            Ty::Str => "String",
            Ty::Bool => "Bool",
            Ty::Array => "Array",
            Ty::Record => "Record",
            Ty::Lambda => "Lambda",
            Ty::Sequence => "Array | String",
        }
    }

    fn accepts(self, v: &Value) -> bool {
        match self {
            Ty::Any => true,
            Ty::Int => matches!(v, Value::Int(_)),
            Ty::Numeric => matches!(v, Value::Int(_) | Value::Float(_)),
            Ty::Str => matches!(v, Value::Str(_) | Value::Uri(_)),
            Ty::Bool => matches!(v, Value::Bool(_)),
            Ty::Array => matches!(v, Value::Array(_)),
            Ty::Record => matches!(v, Value::Record(_) | Value::Table(_)),
            Ty::Lambda => matches!(v, Value::Lambda(_) | Value::AsyncLambda(_)),
            Ty::Sequence => matches!(v, Value::Array(_) | Value::Str(_) | Value::Uri(_)),
        }
    }
}

/// One declared parameter.
#[derive(Debug, Clone, Copy)]
pub struct Param {
    pub name: &'static str,
    pub ty: Ty,
    pub required: bool,
    /// Inclusive bounds for an `Int`, when the builtin has them. `round`'s
    /// digits argument is the reason this exists: it silently accepted 99.
    pub range: Option<(i64, i64)>,
    /// This parameter absorbs all remaining arguments. Only meaningful on the
    /// last one. `pick("name", "size", "modified")` is why: a fixed parameter
    /// list cannot describe it, and declaring a fixed arity would have removed
    /// the form E2's corpus uses.
    pub variadic: bool,
    pub doc: &'static str,
}

/// A builtin's declared interface.
#[derive(Debug, Clone, Copy)]
pub struct Signature {
    pub name: &'static str,
    /// The catalogue category, when the name-derived one is wrong.
    ///
    /// `categorize_builtin` works from the name, so the array `zip` was filed
    /// under **Archive** and `uniq`/`sort` under **FileSystem**. Categories are
    /// how an agent browses (`ontology_describe("Array")` lists a category), so
    /// a wrong one does not just mislabel a builtin, it hides it from the list
    /// it belongs in. `None` keeps the derived category.
    pub category: Option<&'static str>,
    /// This builtin only works with a piped subject; the direct-call form is
    /// not supported.
    ///
    /// `uniq([1, 1, 2])` failed with `E_UNKNOWN` and "uniq: no input provided"
    /// -- the one code an agent is told not to reason about, for a condition
    /// that is entirely knowable before the body runs.
    pub subject_required: bool,
    /// Other spellings that dispatch to the same implementation.
    ///
    /// Needed because a builtin is reachable under several names and an agent
    /// may write any of them: `from_json`/`from-json`, `group_by`/`group`,
    /// `mean`/`avg`, `to_string`/`str`. A declaration keyed on one spelling
    /// would enforce nothing for the others and would leave the catalogue
    /// describing them by name-splitting.
    pub aliases: &'static [&'static str],
    /// What the pipeline supplies, for a builtin that takes a subject.
    pub subject: Option<Ty>,
    pub params: &'static [Param],
    pub returns: &'static str,
    pub doc: &'static str,
    /// Worked examples: the code, and what it evaluates to. These are asserted
    /// to be non-empty for every declaration, because an example is the part an
    /// agent copies.
    pub examples: &'static [(&'static str, &'static str)],
}

impl Signature {
    /// The signature as an agent reads it: `round(digits: Int) -> Number`.
    pub fn render(&self) -> String {
        let params = self
            .params
            .iter()
            .map(|p| match (p.required, p.variadic) {
                (_, true) => format!("{}...: {}", p.name, p.ty.as_str()),
                (true, false) => format!("{}: {}", p.name, p.ty.as_str()),
                (false, false) => format!("{}?: {}", p.name, p.ty.as_str()),
            })
            .collect::<Vec<_>>()
            .join(", ");
        match self.subject {
            Some(s) => format!(
                "{} | {}({}) -> {}",
                s.as_str(),
                self.name,
                params,
                self.returns
            ),
            None => format!("{}({}) -> {}", self.name, params, self.returns),
        }
    }
}

const fn req(name: &'static str, ty: Ty, doc: &'static str) -> Param {
    Param {
        name,
        ty,
        required: true,
        range: None,
        variadic: false,
        doc,
    }
}

const fn opt(name: &'static str, ty: Ty, doc: &'static str) -> Param {
    Param {
        name,
        ty,
        required: false,
        range: None,
        variadic: false,
        doc,
    }
}

const fn opt_range(name: &'static str, ty: Ty, lo: i64, hi: i64, doc: &'static str) -> Param {
    Param {
        name,
        ty,
        required: false,
        range: Some((lo, hi)),
        variadic: false,
        doc,
    }
}

/// A parameter that absorbs every remaining argument.
const fn rest(name: &'static str, ty: Ty, doc: &'static str) -> Param {
    Param {
        name,
        ty,
        required: true,
        range: None,
        variadic: true,
        doc,
    }
}

/// The declared set.
///
/// Deliberately small. Every entry here is either a builtin that answered a
/// call it could not honour, or one of the everyday verbs the benchmark corpus
/// actually reaches for. Growing this list is the migration; the mechanism does
/// not depend on its size.
pub static SIGNATURES: &[Signature] = &[
    Signature {
        name: "round",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Numeric),
        params: &[opt_range(
            "digits",
            Ty::Int,
            0,
            17,
            "decimal places; 0 returns an Int",
        )],
        returns: "Number",
        doc: "Round a number, optionally to a number of decimal places.",
        examples: &[
            ("round(4.966, 2)", "4.97"),
            ("4.966 | round(2)", "4.97"),
            ("round(4.5)", "5"),
        ],
    },
    Signature {
        name: "max",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[opt(
            "values",
            Ty::Any,
            "an array to aggregate, or two numbers",
        )],
        returns: "Number",
        doc: "Largest element of an array, or the larger of two numbers.",
        examples: &[("[1, 5, 3] | max", "5"), ("max(2, 9)", "9")],
    },
    Signature {
        name: "min",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[opt(
            "values",
            Ty::Any,
            "an array to aggregate, or two numbers",
        )],
        returns: "Number",
        doc: "Smallest element of an array, or the smaller of two numbers.",
        examples: &[("[1, 5, 3] | min", "1"), ("min(2, 9)", "2")],
    },
    Signature {
        name: "env",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[
            req("name", Ty::Str, "environment variable to read"),
            opt("default", Ty::Any, "returned when the variable is unset"),
        ],
        returns: "String",
        doc: "Read an environment variable, with an optional default when unset.",
        examples: &[
            (r#"env("HOME")"#, "/home/me"),
            (r#"env("NOPE", "fallback")"#, "fallback"),
        ],
    },
    Signature {
        name: "db_json_to_sqlite",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[
            req(
                "db",
                Ty::Str,
                "database to write — NOTE: the database comes first",
            ),
            req("json", Ty::Str, "JSON file to import"),
            opt("table", Ty::Str, "table name; defaults to \"imported\""),
        ],
        returns: "Bool",
        doc: "Import a JSON array of objects into a SQLite table.",
        examples: &[(
            r#"db_json_to_sqlite("issues.db", "issues.json", "issues")"#,
            "true",
        )],
    },
    Signature {
        name: "where",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[req(
            "predicate",
            Ty::Lambda,
            "fn(element) -> Bool, or fn(element, index)",
        )],
        returns: "Array",
        doc: "Keep the elements for which the predicate returns true.",
        examples: &[
            ("[1, 2, 3] | where(fn(x) => x > 1)", "[2, 3]"),
            // The subject may also be passed directly, which is the form the
            // hand-written catalogue entry used to advertise.
            ("where([1, 2, 3], fn(x) => x > 1)", "[2, 3]"),
        ],
    },
    Signature {
        name: "map",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[req(
            "transform",
            Ty::Lambda,
            "fn(element) -> Any, or fn(element, index)",
        )],
        returns: "Array",
        doc: "Apply a transform to every element.",
        examples: &[
            ("[1, 2, 3] | map(fn(x) => x * 2)", "[2, 4, 6]"),
            ("map([1, 2, 3], fn(x) => x * 2)", "[2, 4, 6]"),
        ],
    },
    Signature {
        name: "sum",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[],
        returns: "Number",
        doc: "Total of a numeric array.",
        examples: &[("[1, 2, 3] | sum", "6")],
    },
    Signature {
        name: "len",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Any),
        params: &[],
        returns: "Int",
        doc: "Number of elements in an array, characters in a string, or fields in a record.",
        examples: &[("[1, 2, 3] | len", "3"), (r#""abc" | len"#, "3")],
    },
    // ── the E1 corpus's working set ─────────────────────────────────────
    //
    // Everything below is a builtin the benchmark corpus actually calls. They
    // were all described by name-splitting -- `from-json` as "From-json" with
    // no parameters, `sort_by` as "Sort by" with none, `str` as "Str" -- and
    // four of them (`from_json`, `group_by`, `mean`, `to_string`) could not be
    // looked up at all, because they live in the dispatcher's fallback half
    // which the catalogue never walked.
    //
    // `benches/agentic/PREREGISTERED_E7.md` names this as the thing that would
    // make the AetherShell arm lose for a fixable reason rather than a
    // fundamental one, so it lands before that experiment runs.
    Signature {
        name: "from-json",
        category: None,
        subject_required: false,
        aliases: &["from_json"],
        subject: Some(Ty::Str),
        params: &[],
        returns: "Any",
        doc: "Parse a JSON string into typed values.",
        examples: &[
            (r#"cat("issues.json") | from_json | len"#, "500"),
            (r#""[1, 2]" | from_json"#, "[1, 2]"),
        ],
    },
    Signature {
        name: "group",
        category: None,
        subject_required: false,
        aliases: &["group_by", "group-object", "Group-Object"],
        subject: Some(Ty::Array),
        params: &[req("key", Ty::Str, "field name to group on")],
        returns: "Array",
        doc: "Group records by a field, returning {Name, Count, Group} records.",
        examples: &[(
            r#"[{user: "a"}, {user: "a"}, {user: "b"}] | group_by("user") | len"#,
            "2",
        )],
    },
    Signature {
        name: "avg",
        category: None,
        subject_required: false,
        aliases: &["mean"],
        subject: Some(Ty::Array),
        params: &[],
        returns: "Float",
        doc: "Arithmetic mean of a numeric array.",
        examples: &[
            ("[1, 2, 3, 4] | mean", "2.5"),
            ("[1, 2] | avg | round(2)", "1.5"),
        ],
    },
    Signature {
        name: "str",
        category: None,
        subject_required: false,
        aliases: &["to_string"],
        subject: Some(Ty::Any),
        params: &[],
        returns: "String",
        doc: "Render any value as a string.",
        examples: &[(r#"to_string(42) + "!""#, r#""42!""#)],
    },
    Signature {
        name: "sort_by",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        // `key` is deliberately `Any` and optional. It takes a field name or a
        // lambda, `"desc"` may follow it, and calling it with no key at all
        // must reach the builtin's own error, which names both accepted forms
        // -- a better message than a declaration can give, and one
        // `tests/sort_by_key.rs` asserts. Declaring `key: String, required`
        // rejected the lambda form and replaced that message with a worse one.
        params: &[
            opt(
                "key",
                Ty::Any,
                "field name (String), or fn(record) -> Any",
            ),
            opt("order", Ty::Str, r#""desc" for descending; default ascending"#),
        ],
        returns: "Array",
        doc: "Sort an array of records by a field name or a lambda.",
        examples: &[
            (r#"[{n: 2}, {n: 1}] | sort_by("n") | first | fn(r) => r.n"#, "1"),
            (r#"[{n: 1}, {n: 2}] | sort_by("n", "desc") | first | fn(r) => r.n"#, "2"),
            ("[{n: 2}, {n: 1}] | sort_by(fn(r) => r.n)", "[{n: 1}, {n: 2}]"),
        ],
    },
    Signature {
        name: "sort",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Sequence),
        params: &[],
        returns: "Array",
        doc: "Sort an array ascending by natural ordering.",
        examples: &[("[3, 1, 2] | sort", "[1, 2, 3]")],
    },
    Signature {
        name: "first",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        // `first(n)` returns the first n as an array; bare `first` returns the
        // element. Declaring it parameterless refused `ls(…) | first(5)`, which
        // is the form E2's corpus uses -- and the Rust suite did not notice,
        // because nothing in it calls `first` with a count.
        params: &[opt(
            "count",
            Ty::Int,
            "how many to return, as an array; omit for the single element",
        )],
        returns: "Any",
        doc: "First element of an array, or the first `count` elements.",
        examples: &[("[1, 2, 3] | first", "1"), ("[1, 2, 3] | first(2)", "[1, 2]")],
    },
    Signature {
        name: "last",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[opt(
            "count",
            Ty::Int,
            "how many to return, as an array; omit for the single element",
        )],
        returns: "Any",
        doc: "Last element of an array, or the last `count` elements.",
        examples: &[("[1, 2, 3] | last", "3"), ("[1, 2, 3] | last(2)", "[2, 3]")],
    },
    Signature {
        name: "flatten",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[],
        returns: "Array",
        doc: "Flatten one level of nesting.",
        examples: &[("[[1, 2], [3]] | flatten", "[1, 2, 3]")],
    },
    Signature {
        name: "unique",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Sequence),
        params: &[],
        returns: "Array",
        doc: "Remove duplicate elements, preserving first-seen order.",
        examples: &[("[1, 2, 1, 3] | unique", "[1, 2, 3]")],
    },
    Signature {
        name: "any",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        // The subject is NOT listed as a parameter: `validate` already shifts
        // past it for the direct-call form `any(array, predicate)`. Listing it
        // too counted it twice and refused the corpus's own `any(r.labels,
        // fn(l) => ...)` -- caught by running E1, not by the type checker.
        // Optional: `any([false, true])` over an array of booleans is a
        // documented form with its own tests. Declaring the predicate required
        // refused it -- caught by `tests/builtin_consistent_syntax.rs`, which
        // is what a suite is for.
        params: &[opt(
            "predicate",
            Ty::Lambda,
            "fn(element) -> Bool; omit to test the elements themselves",
        )],
        returns: "Bool",
        doc: "True when the predicate holds for at least one element, or when               any element is itself true.",
        examples: &[
            (r#"any(["a", "b"], fn(l) => l == "b")"#, "true"),
            ("[1, 2] | any(fn(x) => x > 1)", "true"),
            ("any([false, true, false])", "true"),
        ],
    },
    Signature {
        name: "all",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[opt(
            "predicate",
            Ty::Lambda,
            "fn(element) -> Bool; omit to test the elements themselves",
        )],
        returns: "Bool",
        doc: "True when the predicate holds for every element, or when every               element is itself true.",
        examples: &[
            ("[2, 3] | all(fn(x) => x > 1)", "true"),
            ("all([true, false, true])", "false"),
        ],
    },
    Signature {
        name: "contains",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Any),
        params: &[req("needle", Ty::Any, "substring or element to look for")],
        returns: "Bool",
        // Strings only. This said "or an array an element" until
        // `contains([1, 2, 3], 2)` was actually run: it answers
        // `E_BAD_ARG: expected a string, got Array`. Exactly the kind of
        // plausible-but-wrong sentence that name-derived documentation
        // produces, written here by hand instead.
        doc: "Whether a string contains a substring. Strings only.",
        examples: &[(r#"contains("security fix", "security")"#, "true")],
    },
    Signature {
        name: "lower",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Str),
        params: &[],
        returns: "String",
        doc: "Convert a string to lowercase.",
        examples: &[(r#"lower("ABC")"#, r#""abc""#)],
    },
    Signature {
        name: "ends_with",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Str),
        params: &[req("suffix", Ty::Str, "text the string must end with")],
        returns: "Bool",
        doc: "Whether a string ends with the given suffix.",
        examples: &[(r#"ends_with("main.rs", ".rs")"#, "true"), (r#""main.rs" | ends_with(".rs")"#, "true")],
    },
    Signature {
        name: "starts_with",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Str),
        params: &[req("prefix", Ty::Str, "text the string must begin with")],
        returns: "Bool",
        doc: "Whether a string starts with the given prefix.",
        examples: &[(r#"starts_with("version = 1", "version")"#, "true")],
    },
    Signature {
        name: "split",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Str),
        params: &[req("delimiter", Ty::Str, "separator to split on; required")],
        returns: "Array",
        doc: "Split a string on a delimiter.",
        examples: &[(r#"split("a,b,c", ",")"#, r#"["a", "b", "c"]"#), (r#""a,b,c" | split(",") | len"#, "3")],
    },
    Signature {
        name: "upper",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Str),
        params: &[],
        returns: "String",
        doc: "Convert a string to uppercase.",
        examples: &[(r#"upper("ab")"#, r#""AB""#)],
    },
    Signature {
        name: "trim",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Str),
        params: &[],
        returns: "String",
        doc: "Remove leading and trailing whitespace.",
        examples: &[(r#"trim("  x  ")"#, r#""x""#)],
    },
    Signature {
        name: "replace",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Str),
        params: &[req("from", Ty::Str, "text to find"), req("to", Ty::Str, "replacement")],
        returns: "String",
        doc: "Replace every occurrence of one substring with another.",
        examples: &[(r#"replace("aXa", "X", "Y")"#, r#""aYa""#)],
    },
    Signature {
        name: "join",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[req("separator", Ty::Str, "text placed between elements")],
        returns: "String",
        doc: "Join an array into a string with a separator.",
        examples: &[(r#"join(["a", "b"], "-")"#, r#""a-b""#), (r#"["a", "b"] | join("-")"#, r#""a-b""#)],
    },
    Signature {
        name: "keys",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Record),
        params: &[],
        returns: "Array",
        doc: "The field names of a record.",
        examples: &[(r#"keys({a: 1, b: 2}) | len"#, "2")],
    },
    Signature {
        name: "values",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Record),
        params: &[],
        returns: "Array",
        doc: "The field values of a record.",
        examples: &[(r#"values({a: 1, b: 2}) | sum"#, "3")],
    },
    Signature {
        // `Any`, not `Array`: it reverses strings too, and declaring it
        // `Array` refused `"abc" | reverse`. That shipped in 5a54fc3 and
        // survived because the only string test used the function-call
        // form, which was not type-checked at the time.
        name: "reverse",
        category: Some("Array"),
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Any),
        params: &[],
        returns: "Any",
        doc: "Reverse an array or a string.",
        examples: &[
            ("[1, 2, 3] | reverse", r#"[3, 2, 1]"#),
            (r#""abc" | reverse"#, r#""cba""#),
            (r#"reverse("abc")"#, r#""cba""#),
        ],
    },
    Signature {
        name: "take",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[req("n", Ty::Int, "how many elements to keep")],
        returns: "Array",
        doc: "The first n elements of an array.",
        examples: &[("[1, 2, 3, 4] | take(2) | len", "2")],
    },
    Signature {
        // A declared String subject turns `[1,2,3] | head` from an uncoded
        // E_UNKNOWN ("head: input must be a string") into an E_BAD_ARG naming
        // the expected type -- the taxonomy says E_UNKNOWN is the one code an
        // agent must not reason about.
        name: "head",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Str),
        params: &[opt("n", Ty::Int, "how many leading lines; default 10")],
        returns: "String",
        doc: "The first lines of a string.",
        examples: &[
            (r#""abc" | head"#, r#""abc""#),
            (r#""a\nb\nc" | head(2) | split("\n") | len"#, "2"),
        ],
    },
    Signature {
        name: "to-json",
        category: None,
        subject_required: false,
        aliases: &["to_json"],
        subject: Some(Ty::Any),
        params: &[],
        returns: "String",
        doc: "Render a value as JSON text.",
        examples: &[(r#"[1, 2] | to_json"#, r#""[1,2]""#)],
    },
    Signature {
        name: "ls",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[opt("path", Ty::Str, "directory to list; defaults to the working directory")],
        returns: "Array",
        doc: "List a directory as records with name, path, size and modified.",
        examples: &[
            (r#"(ls("src") | len) > 0"#, "true"),
            (r#"(ls() | len) > 0"#, "true"),
        ],
    },
    Signature {
        name: "fs_walk",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[req("path", Ty::Str, "directory to walk recursively")],
        returns: "Array",
        doc: "Every file beneath a directory, recursively.",
        examples: &[(r#"(fs_walk("tests") | len) > 0"#, "true")],
    },
    Signature {
        // Variadic: `pick("name", "size", "modified")` is the form E2's
        // corpus uses, and a fixed arity would have removed it.
        name: "pick",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[rest("fields", Ty::Str, "field names to keep")],
        returns: "Array",
        doc: "Keep only the named fields of each record.",
        examples: &[(r#"[{a: 1, b: 2}] | pick("a") | first | keys | len"#, "1")],
    },
    Signature {
        // Categorised by hand: `categorize_builtin` read the name and filed
        // this under Archive, next to the compression builtins.
        name: "zip",
        category: Some("Array"),
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[req("other", Ty::Array, "array to pair with, element by element")],
        returns: "Array",
        doc: "Pair two arrays element by element, stopping at the shorter one.",
        examples: &[
            ("zip([1, 2], [3, 4]) | len", "2"),
            ("[1, 2] | zip([3, 4]) | first | len", "2"),
            ("zip([1, 2, 3], [4]) | len", "1"),
        ],
    },
    Signature {
        // `subject_required`: `uniq([1, 1, 2])` failed with E_UNKNOWN and
        // "uniq: no input provided". The direct form is genuinely
        // unsupported, and saying so in the declaration makes the refusal
        // coded and the requirement visible in the signature.
        name: "uniq",
        category: Some("Array"),
        subject_required: true,
        aliases: &[],
        subject: Some(Ty::Sequence),
        params: &[],
        returns: "Array",
        doc: "Remove duplicate elements. Pipeline only.",
        examples: &[("[1, 1, 2] | uniq | len", "2")],
    },
    Signature {
        name: "abs",
        category: Some("Math"),
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Numeric),
        params: &[],
        returns: "Number",
        doc: "Absolute value of a number.",
        examples: &[("abs(-2)", "2")],
    },
    Signature {
        name: "sqrt",
        category: Some("Math"),
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Numeric),
        params: &[],
        returns: "Number",
        doc: "Square root of a number.",
        examples: &[("sqrt(16)", "4")],
    },
    Signature {
        name: "pow",
        category: Some("Math"),
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Numeric),
        params: &[req("exponent", Ty::Numeric, "the power to raise to")],
        returns: "Number",
        doc: "Raise a number to a power.",
        examples: &[("pow(2, 3)", "8")],
    },
    Signature {
        name: "floor",
        category: Some("Math"),
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Numeric),
        params: &[],
        returns: "Int",
        doc: "Largest integer not greater than a number.",
        examples: &[("floor(2.7)", "2")],
    },
    Signature {
        name: "ceil",
        category: Some("Math"),
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Numeric),
        params: &[],
        returns: "Int",
        doc: "Smallest integer not less than a number.",
        examples: &[("ceil(2.1)", "3")],
    },
    Signature {
        name: "range",
        category: Some("Array"),
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[req("start_or_count", Ty::Int, "count when alone, start when an end follows"), opt("end", Ty::Int, "exclusive upper bound")],
        returns: "Array",
        doc: "A range of integers: range(n) is 0..n, range(a, b) is a..b.",
        examples: &[("range(3) | len", "3"), ("range(1, 4) | first", "1")],
    },
    Signature {
        name: "typeof",
        category: Some("Core"),
        subject_required: false,
        aliases: &["type_of"],
        subject: Some(Ty::Any),
        params: &[],
        returns: "String",
        doc: "The type name of a value.",
        examples: &[(r#"type_of(42)"#, r#""Int""#), (r#"typeof("x")"#, r#""String""#)],
    },
    Signature {
        name: "grep",
        category: Some("Text"),
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[req("pattern", Ty::Str, "text or regex to search for"), opt("path", Ty::Str, "file to search; omit to search the piped text")],
        returns: "Array",
        doc: "Lines matching a pattern.",
        examples: &[(r#"grep("fn main", "src/main.rs") | len"#, "1"), (r#"(grep("fn", "src/main.rs") | len) > 0"#, "true")],
    },
    Signature {
        name: "find",
        category: Some("FileSystem"),
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[req("path", Ty::Str, "directory to search"), opt("pattern", Ty::Str, "glob such as *.rs; omit for every file")],
        returns: "Array",
        doc: "Files under a directory, optionally matching a glob.",
        examples: &[(r#"(find("src", "*.rs") | len) > 0"#, "true"), (r#"(find("src") | len) > 0"#, "true")],
    },
    Signature {
        // The design document names this a core verb, and the hybrid arm of E1
        // is built on it. It was described as `sql() -> Value`, "Sql", with no
        // parameters -- and it answered three calls it could not honour:
        //
        //   sqlite_query(":memory:")                       exit 0, no output
        //   sqlite_query()                                  exit 0, no output
        //   sqlite_query(db, "SELECT ? AS n", [7])          exit 0, n = null
        //
        // The first two are the worst kind: an agent asks for data, and gets
        // silence and success. The third accepts bind parameters and discards
        // them, which is `round(4.966, 2)` returning 5 in a different costume.
        // Declaring two required parameters and no more refuses all three.
        //
        // Bind parameters are genuinely unsupported; that is now visible in the
        // signature instead of being discovered when a query returns nulls.
        name: "sql",
        category: Some("Database"),
        subject_required: false,
        aliases: &["sqlite_query", "db_sqlite_query"],
        subject: None,
        params: &[
            req(
                "database",
                Ty::Str,
                "path to the SQLite file, or \":memory:\" for a scratch database",
            ),
            req("query", Ty::Str, "SQL to execute; bind parameters are not supported"),
        ],
        returns: "Array",
        doc: "Run a SQL query against a SQLite database, returning rows as records.",
        examples: &[
            (
                r#"sqlite_query(":memory:", "SELECT 1 AS n") | first | fn(r) => r.n"#,
                "1",
            ),
            (r#"sql(":memory:", "SELECT 2 AS n") | len"#, "1"),
        ],
    },
    Signature {
        // Works on arrays and on strings, so the subject is `Any` -- narrowing
        // it to Array would have deleted `"abcdef" | slice(1, 3)`.
        name: "slice",
        category: Some("Array"),
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Any),
        params: &[
            req("start", Ty::Int, "first index to keep, counting from zero"),
            opt("end", Ty::Int, "index to stop before; omit to run to the end"),
        ],
        returns: "Any",
        doc: "A sub-range of an array or a string.",
        examples: &[
            ("[1, 2, 3, 4, 5] | slice(1, 3) | len", "2"),
            ("[1, 2, 3, 4, 5] | slice(2) | len", "3"),
            (r#""abcdef" | slice(1, 3)"#, r#""bc""#),
        ],
    },
    Signature {
        // `subject: None` on purpose. With a piped subject `cat` is a
        // passthrough -- `"Cargo.toml" | cat` returns the string, it does not
        // read the file -- so the path is a parameter, not a subject. Declaring
        // a subject would have made `cat("Cargo.toml")` treat the path as the
        // subject and then complain the path was missing.
        //
        // `path` is optional for the same reason: required would refuse the
        // passthrough form. That leaves `cat()` still answering E_UNKNOWN ("no
        // file specified"), which a declaration cannot reach -- the contract is
        // "a piped subject OR a path" and this model cannot say that. Two of
        // the three defects go, and the third is written down rather than
        // implied:
        //
        //   cat({unexpected: true})        was E_UNKNOWN, now E_BAD_ARG
        //   cat("Cargo.toml", "README.md") silently ignored the second file
        //   cat()                          still E_UNKNOWN; needs a body change
        name: "cat",
        category: Some("FileSystem"),
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[opt(
            "path",
            Ty::Str,
            "file to read; omit when the content arrives through the pipe",
        )],
        returns: "String",
        doc: "Read a file as text, or pass piped text through unchanged.",
        examples: &[
            (r#"(cat("Cargo.toml") | len) > 0"#, "true"),
            (r#""passthrough" | cat"#, r#""passthrough""#),
        ],
    },
    // ---- System information -------------------------------------------
    //
    // Fourteen builtins that take no arguments at all -- their bodies bind
    // `_args`, so the compiler proves it -- and which therefore accepted and
    // discarded anything they were handed. `sys_os("a", 1, false)` answered
    // "linux" as happily as `sys_os()`. That is the `round(4.966, 2) -> 5`
    // defect, and `benches/agentic/discarded-args.mjs` measured it across
    // 369 of 393 comparable builtins.
    //
    // Declaring a parameterless builtin carries none of the risk that sank
    // `last(5)` and `"abc" | reverse`: there is no type to get too narrow,
    // because there is no argument. The declaration says only "none", which
    // the body already enforces by ignoring them.
    //
    // Each `returns` and `doc` is read off what the builtin actually
    // returned, never off its name -- `sys_cpu_count` says 24 where
    // `sys_cpu_info` reports 12 cores, so one counts hyperthreads and the
    // other does not, and only running them says which is which.
    //
    // The examples are `typeof(x())` rather than a pinned value, because a
    // hostname is not the same on two machines. That keeps every example
    // executable by `tests/declared_signatures.rs` instead of joining the
    // two-name `NEEDS_THE_WORLD` exemption, and it checks the `returns`
    // field rather than merely decorating it.
    Signature {
        name: "sys_arch",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The CPU architecture this shell is running on.",
        examples: &[("typeof(sys_arch())", "String")],
    },
    Signature {
        name: "sys_boot_time",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "When the machine last booted.",
        examples: &[("typeof(sys_boot_time())", "String")],
    },
    Signature {
        name: "sys_cpu_count",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Int",
        doc: "The number of logical CPUs, hyperthreads included.",
        examples: &[("typeof(sys_cpu_count())", "Int")],
    },
    Signature {
        name: "sys_cpu_info",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "The CPU's model name, physical core count and clock speed in MHz.",
        examples: &[("typeof(sys_cpu_info())", "Record")],
    },
    Signature {
        name: "sys_disk_info",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every mounted filesystem: device, mount point and free space.",
        examples: &[("typeof(sys_disk_info())", "Array")],
    },
    Signature {
        name: "sys_groups",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every group on this system, with its gid and members.",
        examples: &[("typeof(sys_groups())", "Array")],
    },
    Signature {
        name: "sys_info",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "A summary of the host: OS, OS family, architecture and distribution name.",
        examples: &[("typeof(sys_info())", "Record")],
    },
    Signature {
        name: "sys_kernel",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The running kernel's release string.",
        examples: &[("typeof(sys_kernel())", "String")],
    },
    Signature {
        name: "sys_load_avg",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "The 1, 5 and 15 minute load averages.",
        examples: &[("typeof(sys_load_avg())", "Record")],
    },
    Signature {
        name: "sys_locale",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The active locale.",
        examples: &[("typeof(sys_locale())", "String")],
    },
    Signature {
        name: "sys_os",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The operating system name.",
        examples: &[("typeof(sys_os())", "String")],
    },
    Signature {
        name: "sys_swap_info",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Swap totals in bytes: total, used and free.",
        examples: &[("typeof(sys_swap_info())", "Record")],
    },
    Signature {
        name: "sys_timezone",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The system's IANA time zone name.",
        examples: &[("typeof(sys_timezone())", "String")],
    },
    Signature {
        name: "sys_user_info",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "The current user's name, home directory, login shell and domain.",
        examples: &[("typeof(sys_user_info())", "Record")],
    },
    // ---- Project introspection ------------------------------------------
    //
    // The second tranche of parameterless declarations, on the same footing
    // as the system group above: every body binds `_args`, so the compiler
    // proves there is no argument to declare too narrowly.
    //
    // Every `returns` and `doc` was read off what the builtin produced when
    // run against this repository, never off its name. `project_size`
    // reports 24,297,412 -- bytes, not lines, which the name does not say
    // and `describe_builtin_name` would have guessed wrong. `project_loc`
    // is the line count, and it reported 0 for every project on every
    // platform until this commit.
    Signature {
        name: "project_config_files",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "The build and package manifests found at the project root.",
        examples: &[("typeof(project_config_files())", "Array")],
    },
    Signature {
        name: "project_dependencies",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "The project's declared runtime dependencies.",
        examples: &[("typeof(project_dependencies())", "Array")],
    },
    Signature {
        name: "project_dev_dependencies",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "The project's declared development dependencies.",
        examples: &[("typeof(project_dev_dependencies())", "Array")],
    },
    Signature {
        name: "project_entry_points",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "The source files the build treats as entry points.",
        examples: &[("typeof(project_entry_points())", "Array")],
    },
    Signature {
        name: "project_gitignore",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The contents of the .gitignore file, verbatim.",
        examples: &[("typeof(project_gitignore())", "String")],
    },
    Signature {
        name: "project_languages",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every programming language detected among the project files.",
        examples: &[("typeof(project_languages())", "Array")],
    },
    Signature {
        name: "project_license",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The contents of the LICENSE file, verbatim.",
        examples: &[("typeof(project_license())", "String")],
    },
    Signature {
        name: "project_loc",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Int",
        doc: "Total source lines, skipping build output and vendored trees.",
        examples: &[("typeof(project_loc())", "Int")],
    },
    Signature {
        name: "project_name",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The project name from its manifest.",
        examples: &[("typeof(project_name())", "String")],
    },
    Signature {
        name: "project_readme",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The contents of the README, verbatim.",
        examples: &[("typeof(project_readme())", "String")],
    },
    Signature {
        name: "project_root",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The absolute path of the project root.",
        examples: &[("typeof(project_root())", "String")],
    },
    Signature {
        name: "project_size",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Int",
        doc: "Total size of the project tree in bytes.",
        examples: &[("typeof(project_size())", "Int")],
    },
    Signature {
        name: "project_structure",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every path in the project tree.",
        examples: &[("typeof(project_structure())", "Array")],
    },
    Signature {
        name: "project_test_files",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every test file in the project.",
        examples: &[("typeof(project_test_files())", "Array")],
    },
    Signature {
        name: "project_type",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The build system in use, such as \"rust\", \"node\" or \"python\".",
        examples: &[("typeof(project_type())", "String")],
    },
    Signature {
        name: "project_version",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The project version from its manifest.",
        examples: &[("typeof(project_version())", "String")],
    },
    // ---- Read-only status and inspection ---------------------------------
    //
    // Third tranche of parameterless declarations. Only read-only builtins
    // are here: gathering the evidence means RUNNING each one, and
    // `telemetry_enable`, `governor_reset`, `session_start`, `fs_mount`,
    // `diag_fix` and `refactor_remove_unused` change the machine, so they
    // are left for a setting where that is safe to do.
    //
    // `now` and `time` are declared identically because they measurably
    // are the same thing: both return Unix seconds, one second apart on
    // consecutive calls. The ontology should say so rather than imply two
    // different clocks.
    //
    // `capabilities` stays undeclared: it is a Linux concept, and an
    // executable example has to run on every platform CI has. The
    // parameterless `search_*` builtins were held back for the same reason
    // until they stopped shelling out to `grep`/`find` (which Windows may
    // lack, or resolve to System32\find.exe); they walk in-process now and
    // are declared below.
    //
    // `env_venv` returned "" when no virtualenv was active. That was first
    // documented rather than changed; it now returns null, because null is
    // what every other unset-variable lookup in the shell returns. It is a
    // breaking change, and belongs in the release notes as one.
    Signature {
        name: "git_branches",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every branch in the repository.",
        examples: &[("typeof(git_branches())", "Array")],
    },
    Signature {
        name: "git_remote",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The configured remotes, as `git remote -v` prints them.",
        examples: &[("typeof(git_remote())", "String")],
    },
    Signature {
        name: "git_root",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The absolute path of the repository root.",
        examples: &[("typeof(git_root())", "String")],
    },
    Signature {
        name: "git_stash_list",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every stash entry, newest first.",
        examples: &[("typeof(git_stash_list())", "Array")],
    },
    Signature {
        name: "git_tags",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every tag in the repository.",
        examples: &[("typeof(git_tags())", "Array")],
    },
    Signature {
        name: "env_container",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Bool",
        doc: "Whether this shell is running inside a container.",
        examples: &[("typeof(env_container())", "Bool")],
    },
    Signature {
        name: "env_detect",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "The host OS, OS family and architecture.",
        examples: &[("typeof(env_detect())", "Record")],
    },
    Signature {
        name: "env_java",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The installed Java version string.",
        examples: &[("typeof(env_java())", "String")],
    },
    Signature {
        name: "env_node",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The installed Node.js version string.",
        examples: &[("typeof(env_node())", "String")],
    },
    Signature {
        name: "env_rust",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The installed Rust compiler version string.",
        examples: &[("typeof(env_rust())", "String")],
    },
    Signature {
        name: "env_shell",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The path of the user's login shell.",
        examples: &[("typeof(env_shell())", "String")],
    },
    Signature {
        name: "env_venv",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String | Null",
        doc: "The active Python virtualenv path, or null when there is none.",
        examples: &[(
            r#"env_venv() == null || typeof(env_venv()) == "String""#,
            "true",
        )],
    },
    Signature {
        name: "a2a_agents",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every agent registered with a2a.register.",
        examples: &[("typeof(a2a_agents())", "Array")],
    },
    Signature {
        name: "a2a_discover",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Agents discovered on the local network.",
        examples: &[("typeof(a2a_discover())", "Array")],
    },
    Signature {
        name: "a2a_status",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Whether the agent channel is active, and its agent and pending-message counts.",
        examples: &[("typeof(a2a_status())", "Record")],
    },
    Signature {
        name: "telemetry_status",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Whether telemetry is enabled, how many events are held, and where they are stored.",
        examples: &[("typeof(telemetry_status())", "Record")],
    },
    Signature {
        name: "governor_status",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "The active resource limits and how much of each has been used.",
        examples: &[("typeof(governor_status())", "Record")],
    },
    Signature {
        name: "safety_status",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "The current mode, policy, workspace, governor state and audit log path.",
        examples: &[("typeof(safety_status())", "Record")],
    },
    Signature {
        name: "tx_status",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Whether a filesystem transaction is active, and its nesting depth.",
        examples: &[("typeof(tx_status())", "Record")],
    },

    Signature {
        name: "config",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "The effective configuration, after files and environment are merged.",
        examples: &[("typeof(config())", "Record")],
    },
    Signature {
        name: "now",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Int",
        doc: "The current Unix time in seconds.",
        examples: &[("typeof(now())", "Int")],
    },
    Signature {
        name: "time",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Int",
        doc: "The current Unix time in seconds.",
        examples: &[("typeof(time())", "Int")],
    },
    Signature {
        name: "audit_stats",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Audit entry counts by severity.",
        examples: &[("typeof(audit_stats())", "Record")],
    },
    Signature {
        name: "cron_list",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every scheduled job visible to this user.",
        examples: &[("typeof(cron_list())", "Array")],
    },
    Signature {
        name: "group_list",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every group on this system, with its gid and members.",
        examples: &[("typeof(group_list())", "Array")],
    },
    Signature {
        name: "user_list",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every user account, with uid, gid, home and shell.",
        examples: &[("typeof(user_list())", "Array")],
    },
    Signature {
        name: "workspace_list",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every configured workspace.",
        examples: &[("typeof(workspace_list())", "Array")],
    },
    Signature {
        name: "repl_sessions",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every open REPL session.",
        examples: &[("typeof(repl_sessions())", "Array")],
    },
    Signature {
        name: "marketplace_list",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every plugin available in the configured marketplaces.",
        examples: &[("typeof(marketplace_list())", "Array")],
    },
    Signature {
        name: "startup_list",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every startup service, with its enabled state.",
        examples: &[("typeof(startup_list())", "Array")],
    },
    Signature {
        name: "search_fixmes",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every FIXME comment under the working directory, as path:line:text. Skips dependency and build trees.",
        examples: &[("typeof(search_fixmes())", "Array")],
    },
    Signature {
        name: "search_todos",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every TODO comment under the working directory, as path:line:text. Skips dependency and build trees.",
        examples: &[("typeof(search_todos())", "Array")],
    },
    Signature {
        name: "search_recent",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Files under the working directory modified in the last day. Skips dependency and build trees.",
        examples: &[("typeof(search_recent())", "Array")],
    },
    // Fourth tranche of parameterless declarations. Chosen from the
    // discarded-args list by reading bodies: every one binds `_args` or never
    // reads `args`, is classified Pure or ReadLocal, and changes nothing.
    // Each type below was measured with `typeof(x())` on Linux and on
    // Windows and was the same on both; builtins whose type differed
    // (platform_cpu_freq, platform_libcpp, platform_ssl_version) or whose
    // answer depends on the working directory (docs_*, diag_config) are left
    // out until that is fixed.
    Signature {
        name: "None",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "The Option variant for no value: the record {_tag: \"None\"}.",
        examples: &[("typeof(None())", "Record")],
    },
    Signature {
        name: "cloud_instances",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Cloud instances registered in this session, as records.",
        examples: &[("typeof(cloud_instances())", "Array")],
    },
    Signature {
        name: "cloud_regions",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "A fixed list of well-known cloud regions, as {code, name} records.",
        examples: &[("typeof(cloud_regions())", "Array")],
    },
    Signature {
        name: "config_path",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Where the shell keeps its files: config file and directory, cache, data, history, init script and plugins.",
        examples: &[("typeof(config_path())", "Record")],
    },
    Signature {
        name: "df",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Mounted filesystems with size, used and available space.",
        examples: &[("typeof(df())", "Array")],
    },
    Signature {
        name: "features",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "The runtime feature flags currently enabled, by name.",
        examples: &[("typeof(features())", "Array")],
    },
    Signature {
        name: "finetune_list",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Fine-tuning jobs started in this session, as records.",
        examples: &[("typeof(finetune_list())", "Array")],
    },
    Signature {
        name: "handles",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every live result handle with its shape and size, without the data.",
        examples: &[("typeof(handles())", "Array")],
    },
    Signature {
        name: "hostname",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "This machine's hostname.",
        examples: &[("typeof(hostname())", "String")],
    },
    Signature {
        name: "hw_disk",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Disks and their usage; the same answer as df().",
        examples: &[("typeof(hw_disk())", "Array")],
    },
    Signature {
        name: "hw_network",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Network interfaces; the same answer as net_interfaces().",
        examples: &[("typeof(hw_network())", "Array")],
    },
    Signature {
        name: "is_bsd",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Bool",
        doc: "Whether the shell is running on a BSD.",
        examples: &[("typeof(is_bsd())", "Bool")],
    },
    Signature {
        name: "is_linux",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Bool",
        doc: "Whether the shell is running on Linux (Android excluded).",
        examples: &[("typeof(is_linux())", "Bool")],
    },
    Signature {
        name: "is_macos",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Bool",
        doc: "Whether the shell is running on macOS.",
        examples: &[("typeof(is_macos())", "Bool")],
    },
    Signature {
        name: "is_unix",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Bool",
        doc: "Whether the shell is running on a Unix-like system.",
        examples: &[("typeof(is_unix())", "Bool")],
    },
    Signature {
        name: "is_windows",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Bool",
        doc: "Whether the shell is running on Windows.",
        examples: &[("typeof(is_windows())", "Bool")],
    },
    Signature {
        name: "list_roles",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Every RBAC role defined in this session, as records.",
        examples: &[("typeof(list_roles())", "Array")],
    },
    Signature {
        name: "lscpu",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "CPU information; the same answer as sys_cpu_info().",
        examples: &[("typeof(lscpu())", "Record")],
    },
    Signature {
        name: "pkg_sources",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "The system package manager's configured sources (apt sources on Debian-family Linux); empty where there are none to read.",
        examples: &[("typeof(pkg_sources())", "Array")],
    },
    Signature {
        name: "platform",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The platform name: windows, linux, macos, bsd, ios or android.",
        examples: &[("typeof(platform())", "String")],
    },
    Signature {
        name: "platform_arch",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The CPU architecture the shell was built for, such as x86_64 or aarch64.",
        examples: &[("typeof(platform_arch())", "String")],
    },
    Signature {
        name: "platform_build",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The operating system's build identifier.",
        examples: &[("typeof(platform_build())", "String")],
    },
    Signature {
        name: "platform_build_systems",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Installed build tools (make, cmake, bazel, ...) with their versions.",
        examples: &[("typeof(platform_build_systems())", "Record")],
    },
    Signature {
        name: "platform_capabilities",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "What this host allows: administrator rights, sudo, a GUI, the network, and similar flags.",
        examples: &[("typeof(platform_capabilities())", "Record")],
    },
    Signature {
        name: "platform_cloud_clis",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Installed cloud CLIs with their versions.",
        examples: &[("typeof(platform_cloud_clis())", "Record")],
    },
    Signature {
        name: "platform_compilers",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Installed compilers with their versions.",
        examples: &[("typeof(platform_compilers())", "Record")],
    },
    Signature {
        name: "platform_containers",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Installed container and virtual-machine tools with their versions.",
        examples: &[("typeof(platform_containers())", "Record")],
    },
    Signature {
        name: "platform_cpu",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "CPU model and core counts.",
        examples: &[("typeof(platform_cpu())", "Record")],
    },
    Signature {
        name: "platform_cpu_count",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Int",
        doc: "Logical CPUs available to this process.",
        examples: &[("typeof(platform_cpu_count())", "Int")],
    },
    Signature {
        name: "platform_cuda_version",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record | Null",
        doc: "The CUDA toolkit version reported by nvcc; null where nvcc is absent.",
        examples: &[(
            r#"platform_cuda_version() == null || typeof(platform_cuda_version()) == "Record""#,
            "true",
        )],
    },
    Signature {
        name: "platform_databases",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Installed database clients and servers with their versions.",
        examples: &[("typeof(platform_databases())", "Record")],
    },
    Signature {
        name: "platform_db_list",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "The names of the platform snapshots saved with platform_db_store().",
        examples: &[("typeof(platform_db_list())", "Array")],
    },
    Signature {
        name: "platform_detect_tools",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Which of a fixed set of common developer tools are on PATH.",
        examples: &[("typeof(platform_detect_tools())", "Array")],
    },
    Signature {
        name: "platform_disks",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Disks with size, free space and filesystem.",
        examples: &[("typeof(platform_disks())", "Array")],
    },
    Signature {
        name: "platform_gpu_memory",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record | Null",
        doc: "GPU memory totals from nvidia-smi; null without an NVIDIA GPU.",
        examples: &[(
            r#"platform_gpu_memory() == null || typeof(platform_gpu_memory()) == "Record""#,
            "true",
        )],
    },
    Signature {
        name: "platform_gpus",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "GPUs from nvidia-smi with memory, driver and utilisation; empty without an NVIDIA GPU.",
        examples: &[("typeof(platform_gpus())", "Array")],
    },
    Signature {
        name: "platform_has_admin",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Bool",
        doc: "Whether the shell runs with administrator (root) rights.",
        examples: &[("typeof(platform_has_admin())", "Bool")],
    },
    Signature {
        name: "platform_has_container",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Bool",
        doc: "Whether docker or podman is on PATH.",
        examples: &[("typeof(platform_has_container())", "Bool")],
    },
    Signature {
        name: "platform_has_gui",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Bool",
        doc: "Whether a graphical display is available.",
        examples: &[("typeof(platform_has_gui())", "Bool")],
    },
    Signature {
        name: "platform_has_network",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Bool",
        doc: "Whether a loopback socket can be bound. It does not test internet access.",
        examples: &[("typeof(platform_has_network())", "Bool")],
    },
    Signature {
        name: "platform_has_sudo",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Bool",
        doc: "Whether sudo is on PATH.",
        examples: &[("typeof(platform_has_sudo())", "Bool")],
    },
    Signature {
        name: "platform_hostname",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "This machine's hostname, from the hostname command.",
        examples: &[("typeof(platform_hostname())", "String")],
    },
    Signature {
        name: "platform_iac_tools",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Installed infrastructure-as-code and orchestration tools with their versions.",
        examples: &[("typeof(platform_iac_tools())", "Record")],
    },
    Signature {
        name: "platform_kernel",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The kernel version.",
        examples: &[("typeof(platform_kernel())", "String")],
    },
    Signature {
        name: "platform_libc",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The C library in use and its version.",
        examples: &[("typeof(platform_libc())", "String")],
    },
    Signature {
        name: "platform_libs",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Common development libraries (openssl, zlib, ...) that pkg-config can find, with versions.",
        examples: &[("typeof(platform_libs())", "Record")],
    },
    Signature {
        name: "platform_line_ending",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "This platform's line ending: CRLF on Windows, LF elsewhere.",
        examples: &[("typeof(platform_line_ending())", "String")],
    },
    Signature {
        name: "platform_linters",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Installed linters and formatters with their versions.",
        examples: &[("typeof(platform_linters())", "Record")],
    },
    Signature {
        name: "platform_machine_id",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "A stable per-machine identifier from the operating system.",
        examples: &[("typeof(platform_machine_id())", "String")],
    },
    Signature {
        name: "platform_memory_total",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Int",
        doc: "Total physical memory, in bytes.",
        examples: &[("typeof(platform_memory_total())", "Int")],
    },
    Signature {
        name: "platform_network_interfaces",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Network interfaces with name, MAC address and speed where known.",
        examples: &[("typeof(platform_network_interfaces())", "Array")],
    },
    Signature {
        name: "platform_os_version",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The operating system's version string.",
        examples: &[("typeof(platform_os_version())", "String")],
    },
    Signature {
        name: "platform_path_sep",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "This platform's path separator: a backslash on Windows, / elsewhere.",
        examples: &[("typeof(platform_path_sep())", "String")],
    },
    Signature {
        name: "platform_pkg_lang",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Installed language package managers (pip, npm, cargo, ...) with their versions.",
        examples: &[("typeof(platform_pkg_lang())", "Record")],
    },
    Signature {
        name: "platform_pkg_managers",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "Installed system package managers.",
        examples: &[("typeof(platform_pkg_managers())", "Array")],
    },
    Signature {
        name: "platform_runtimes",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Installed language runtimes with their versions.",
        examples: &[("typeof(platform_runtimes())", "Record")],
    },
    Signature {
        name: "platform_shell_type",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The shell this process was started from: bash, zsh, powershell, cmd, ...",
        examples: &[("typeof(platform_shell_type())", "String")],
    },
    Signature {
        name: "platform_system_libs",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "System library versions, such as the C library.",
        examples: &[("typeof(platform_system_libs())", "Record")],
    },
    Signature {
        name: "platform_tool_versions",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Versions of every tool the platform_* builtins know, keyed group.tool. Probing never reaches the network.",
        examples: &[("typeof(platform_tool_versions())", "Record")],
    },
    Signature {
        name: "platform_vcs",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "Installed version-control tools with their versions.",
        examples: &[("typeof(platform_vcs())", "Record")],
    },
    Signature {
        name: "plugin_categories",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "The plugin categories the plugin registry knows.",
        examples: &[("typeof(plugin_categories())", "Array")],
    },
    Signature {
        name: "pwd",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The current working directory.",
        examples: &[("typeof(pwd())", "String")],
    },
    Signature {
        name: "session_id",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Int",
        doc: "The current session counter; 0 before session_start().",
        examples: &[("typeof(session_id())", "Int")],
    },
    Signature {
        name: "sso_info",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "SSO configuration status and the number of active sessions.",
        examples: &[("typeof(sso_info())", "Record")],
    },
    Signature {
        name: "svc_list",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "System services with their state.",
        examples: &[("typeof(svc_list())", "Array")],
    },
    Signature {
        name: "themes",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Array",
        doc: "The available colour themes, by name.",
        examples: &[("typeof(themes())", "Array")],
    },
    Signature {
        name: "uptime",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Int",
        doc: "Seconds since the machine booted.",
        examples: &[("typeof(uptime())", "Int")],
    },
    Signature {
        name: "whoami",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The name of the user this shell runs as.",
        examples: &[("typeof(whoami())", "String")],
    },
    // Held back from the fourth tranche because their type differed by
    // operating system; declared once that was fixed. `platform_ssl_version`
    // was already consistent: null means no OpenSSL, as elsewhere.
    Signature {
        name: "platform_cpu_freq",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Float | Null",
        doc: "CPU clock speed in MHz, or null where the OS reports none (Apple Silicon, many ARM and virtual CPUs).",
        examples: &[(r#"platform_cpu_freq() == null || typeof(platform_cpu_freq()) == "Float""#, "true")],
    },
    Signature {
        name: "platform_libcpp",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "Record",
        doc: "The C++ standard library: {name, version, abi}, with version and abi null where not reported.",
        examples: &[(r#"typeof(platform_libcpp().name)"#, "String")],
    },
    Signature {
        name: "platform_ssl_version",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String | Null",
        doc: "The OpenSSL version, from openssl or pkg-config; null where OpenSSL is not installed.",
        examples: &[(r#"platform_ssl_version() == null || typeof(platform_ssl_version()) == "String""#, "true")],
    },
    // The rest of the benchmark working set (plan item 1b starts here):
    // every builtin the E1/E2 corpora or the E7 prompts use is now declared.
    // `each` and `echo` are in the E7 cheatsheet; `git_branch` is E2's t4.
    Signature {
        name: "each",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[req(
            "action",
            Ty::Lambda,
            "fn(element) -> Any; run for its effect, result discarded",
        )],
        returns: "Array",
        doc: "Run an action on every element for its effect and return the array unchanged. An error in the action stops the loop and is reported.",
        examples: &[
            ("[1, 2, 3] | each(fn(x) => x * 2)", "[1, 2, 3]"),
            ("each([1, 2, 3], fn(x) => x)", "[1, 2, 3]"),
        ],
    },
    Signature {
        name: "echo",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[Param {
            name: "values",
            ty: Ty::Any,
            required: false,
            range: None,
            variadic: true,
            doc: "values to print, space-separated; with none, the piped value",
        }],
        returns: "String",
        doc: "Render values on one line, separated by spaces. With no arguments, renders the piped value.",
        examples: &[
            (r#"echo("a", 1, true)"#, "a 1 true"),
            (r#""hello" | echo()"#, "hello"),
        ],
    },
    Signature {
        name: "git_branch",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: None,
        params: &[],
        returns: "String",
        doc: "The current branch name; an empty string on a detached HEAD. Outside a repository, E_BAD_STATE.",
        examples: &[("typeof(git_branch())", "String")],
    },
];

/// The declaration for `name`, if it has one.
pub fn signature_of(name: &str) -> Option<&'static Signature> {
    SIGNATURES
        .iter()
        .find(|s| s.name == name)
        .or_else(|| SIGNATURES.iter().find(|s| s.aliases.contains(&name)))
}

/// Check a call against its declaration, if it has one.
///
/// Returns `Ok(())` for every undeclared builtin, so migration is incremental
/// and nothing that works today changes behaviour.
///
/// The subject is checked only when the call came through a pipeline: a
/// builtin with a declared subject may also be called with the subject as its
/// first argument, and both forms are in use.
pub fn validate(name: &str, args: &[Value], input: Option<&Value>) -> anyhow::Result<()> {
    let Some(sig) = signature_of(name) else {
        return Ok(());
    };

    // A pipeline-only builtin says so before the body has to.
    if sig.subject_required && input.is_none() {
        return Err(crate::safety::bad_arg(
            name,
            &format!("a piped subject: {}", sig.render()),
            "a direct call",
        ));
    }

    // A declared subject must match when one was piped in.
    if let (Some(want), Some(got)) = (sig.subject, input) {
        if !want.accepts(got) {
            return Err(crate::safety::bad_arg(
                name,
                &format!("{} as the piped subject: {}", want.as_str(), sig.render()),
                got.type_name(),
            ));
        }
    }

    // In a pipeline the subject is the input, so the arguments are the
    // parameters. Called directly, a subject-taking builtin may receive the
    // subject as its first argument instead, and the parameters shift by one.
    let params_start = usize::from(input.is_none() && sig.subject.is_some() && !args.is_empty());
    let supplied = args.len().saturating_sub(params_start);

    // When the subject arrives as the first argument rather than through the
    // pipe, it still has a declared type and still has to match. Checking it
    // only in pipeline position left `unique({unexpected: true})` to fail in
    // the body with an uncoded `E_UNKNOWN`, which
    // `tests/uncoded_failure_census.rs` found by sweeping rather than by
    // someone happening to try it.
    //
    // Restricted to builtins that declare no parameters, where the first
    // argument can only be the subject. With parameters present it is
    // genuinely ambiguous: `max(2, 9)` takes two numbers and `[1, 5] | max`
    // takes an array, so `2` is a parameter there, not a malformed subject.
    // Checking it unconditionally refused `max(2, 9)` -- caught immediately by
    // the declaration tests, which is the argument for having written them.
    // Resolving that ambiguity properly is overload resolution, and this is
    // not the place for it.
    if params_start == 1 && sig.params.is_empty() {
        if let (Some(want), Some(got)) = (sig.subject, args.first()) {
            if !want.accepts(got) {
                return Err(crate::safety::bad_arg(
                    name,
                    &format!("{} as the subject: {}", want.as_str(), sig.render()),
                    got.type_name(),
                ));
            }
        }
    }

    let required = sig.params.iter().filter(|p| p.required).count();
    if supplied < required {
        // `expected` is the signature alone, with no "— missing x" tail. The
        // tail read well and cost twice: `diagnose` carries the signature in
        // its own field, generated from this same declaration, so a restated
        // copy inside `expected` is the duplication `bi_diagnose` already
        // refuses for `return_type` and `parameters`. The count says what the
        // tail said, in four tokens.
        return Err(crate::safety::bad_arg(
            name,
            &sig.render(),
            &format!("{supplied} of {required} required"),
        ));
    }
    // A trailing variadic parameter absorbs everything after it, so there is no
    // upper bound to check.
    let variadic = sig.params.last().is_some_and(|p| p.variadic);
    if !variadic && supplied > sig.params.len() {
        return Err(crate::safety::bad_arg(
            name,
            &format!("at most {} argument(s): {}", sig.params.len(), sig.render()),
            &format!("{supplied}"),
        ));
    }

    // Each supplied argument is checked against its own parameter, and every
    // argument past the last against the variadic one.
    let rest = args[params_start..].iter().enumerate().map(|(i, a)| {
        let p = sig
            .params
            .get(i)
            .or_else(|| sig.params.last().filter(|p| p.variadic));
        (p, a)
    });
    for (param, arg) in rest.filter_map(|(p, a)| p.map(|p| (p, a))) {
        if !param.ty.accepts(arg) {
            return Err(crate::safety::bad_arg(
                name,
                &format!("{}: {} ({})", param.name, param.ty.as_str(), param.doc),
                arg.type_name(),
            ));
        }
        if let (Some((lo, hi)), Value::Int(n)) = (param.range, arg) {
            if *n < lo || *n > hi {
                return Err(crate::safety::bad_arg(
                    name,
                    &format!(
                        "{}: {} in {lo}..={hi} ({})",
                        param.name,
                        param.ty.as_str(),
                        param.doc
                    ),
                    &format!("{n}"),
                ));
            }
        }
    }

    Ok(())
}
