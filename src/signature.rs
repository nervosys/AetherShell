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
    pub doc: &'static str,
}

/// A builtin's declared interface.
#[derive(Debug, Clone, Copy)]
pub struct Signature {
    pub name: &'static str,
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
            .map(|p| {
                if p.required {
                    format!("{}: {}", p.name, p.ty.as_str())
                } else {
                    format!("{}?: {}", p.name, p.ty.as_str())
                }
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
        doc,
    }
}

const fn opt(name: &'static str, ty: Ty, doc: &'static str) -> Param {
    Param {
        name,
        ty,
        required: false,
        range: None,
        doc,
    }
}

const fn opt_range(name: &'static str, ty: Ty, lo: i64, hi: i64, doc: &'static str) -> Param {
    Param {
        name,
        ty,
        required: false,
        range: Some((lo, hi)),
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
        subject: Some(Ty::Array),
        params: &[],
        returns: "Number",
        doc: "Total of a numeric array.",
        examples: &[("[1, 2, 3] | sum", "6")],
    },
    Signature {
        name: "len",
        subject: Some(Ty::Any),
        params: &[],
        returns: "Int",
        doc: "Number of elements in an array, characters in a string, or fields in a record.",
        examples: &[("[1, 2, 3] | len", "3"), (r#""abc" | len"#, "3")],
    },
];

/// The declaration for `name`, if it has one.
pub fn signature_of(name: &str) -> Option<&'static Signature> {
    SIGNATURES.iter().find(|s| s.name == name)
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
    if supplied > sig.params.len() {
        return Err(crate::safety::bad_arg(
            name,
            &format!("at most {} argument(s): {}", sig.params.len(), sig.render()),
            &format!("{supplied}"),
        ));
    }

    for (param, arg) in sig.params.iter().zip(args[params_start..].iter()) {
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
