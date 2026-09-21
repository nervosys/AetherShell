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
        subject: Some(Ty::Array),
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
        subject: Some(Ty::Array),
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
        doc: "Whether a string contains a substring, or an array an element.",
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
        name: "reverse",
        category: None,
        subject_required: false,
        aliases: &[],
        subject: Some(Ty::Array),
        params: &[],
        returns: "Array",
        doc: "Reverse the order of an array.",
        examples: &[("[1, 2, 3] | reverse", r#"[3, 2, 1]"#)],
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
        doc: "Pair two arrays element by element.",
        examples: &[("zip([1, 2], [3, 4]) | len", "2"), ("[1, 2] | zip([3, 4]) | first | len", "2")],
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
        subject: Some(Ty::Array),
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
