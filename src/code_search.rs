//! Bounded source clone search, syntax similarity and supplied-embedding ranking.
use crate::{safety, value::Value};
use anyhow::Result;
use quote::ToTokens;
use std::collections::{BTreeMap, BTreeSet};

fn bad(name: &str, message: &str) -> anyhow::Error {
    safety::arg_err(format!("{name}: {message}"))
}
fn budget(name: &str, message: &str) -> anyhow::Error {
    safety::budget_exceeded(
        name,
        message.into(),
        "submit a smaller source unit or corpus",
    )
}
fn row(fields: Vec<(&str, Value)>) -> Value {
    Value::Record(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn text<'a>(name: &str, value: &'a Value) -> Result<&'a str> {
    match value {
        Value::Str(s) => Ok(s),
        _ => Err(bad(name, "expected String")),
    }
}
fn parse(name: &str, source: &str) -> Result<syn::File> {
    if source.len() > 65536 {
        return Err(budget(name, "source exceeds 64 KiB"));
    }
    if source.bytes().filter(u8::is_ascii_punctuation).count() > 256 {
        return Err(budget(
            name,
            "source exceeds 256 raw ASCII punctuation characters, including comments/strings",
        ));
    }
    syn::parse_file(source).map_err(|_| bad(name, "invalid Rust source"))
}
struct Function {
    document: String,
    name: String,
    syntax: String,
}
impl Function {
    fn location(&self) -> Value {
        row(vec![
            ("document", Value::Str(self.document.clone())),
            ("function", Value::Str(self.name.clone())),
        ])
    }
}
fn functions(name: &str, document: &str, source: &str) -> Result<Vec<Function>> {
    fn walk(items: Vec<syn::Item>, prefix: &str, document: &str, out: &mut Vec<Function>) {
        for item in items {
            match item {
                syn::Item::Fn(mut f) => {
                    let function = format!("{prefix}{}", f.sig.ident);
                    // Names and visibility/docs are not clone content. Parameters,
                    // types, qualifiers, generics and literal values remain exact.
                    f.sig.ident = syn::Ident::new("__ae_function", f.sig.ident.span());
                    let sig = &f.sig;
                    let body = &f.block;
                    out.push(Function {
                        document: document.into(),
                        name: function,
                        syntax: quote::quote!(#sig #body).to_string(),
                    });
                }
                syn::Item::Mod(m) => {
                    if let Some((_, items)) = m.content {
                        walk(items, &format!("{prefix}{}::", m.ident), document, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(parse(name, source)?.items, "", document, &mut out);
    let mut names = BTreeSet::new();
    for f in &out {
        if !names.insert(&f.name) {
            return Err(bad(name, "duplicate function paths in a source unit; conditional compilation is not evaluated"));
        }
    }
    Ok(out)
}
fn corpus(name: &str, value: &Value) -> Result<Vec<Function>> {
    let Value::Array(docs) = value else {
        return Err(bad(
            name,
            "corpus must be an Array of {name, source} records",
        ));
    };
    if docs.len() > 128 {
        return Err(budget(name, "corpus exceeds 128 documents"));
    }
    let mut names = BTreeSet::new();
    let mut total = 0;
    let mut out = Vec::new();
    for doc in docs {
        let Value::Record(fields) = doc else {
            return Err(bad(name, "corpus entries must be records"));
        };
        if fields.len() != 2 {
            return Err(bad(
                name,
                "source corpus entries require exactly name and source",
            ));
        }
        let id = text(
            name,
            fields
                .get("name")
                .ok_or_else(|| bad(name, "document name required"))?,
        )?;
        let source = text(
            name,
            fields
                .get("source")
                .ok_or_else(|| bad(name, "document source required"))?,
        )?;
        if id.is_empty() || id.len() > 256 || !names.insert(id) {
            return Err(bad(
                name,
                "document names must be unique nonempty Strings, at most 256 bytes",
            ));
        }
        total += source.len();
        if total > 1048576 {
            return Err(budget(name, "corpus source exceeds 1 MiB"));
        }
        out.extend(functions(name, id, source)?);
        if out.len() > 256 {
            return Err(budget(name, "corpus exceeds 256 free functions"));
        }
    }
    out.sort_by(|a, b| (&a.document, &a.name).cmp(&(&b.document, &b.name)));
    Ok(out)
}
fn limit(name: &str, value: Option<&Value>) -> Result<usize> {
    match value {
        None => Ok(10),
        Some(Value::Int(n)) if (1..=100).contains(n) => Ok(*n as usize),
        _ => Err(bad(name, "limit must be Int 1..100")),
    }
}
fn shingles(source: &str) -> BTreeSet<[char; 5]> {
    let chars: Vec<_> = source.chars().collect();
    chars
        .windows(5)
        .map(|s| [s[0], s[1], s[2], s[3], s[4]])
        .collect()
}
fn embedding(name: &str, value: &Value, dim: Option<usize>) -> Result<Vec<f64>> {
    let Value::Array(values) = value else {
        return Err(bad(name, "embedding must be a numeric Array"));
    };
    if values.is_empty() || values.len() > 4096 || dim.is_some_and(|d| d != values.len()) {
        return Err(bad(
            name,
            "embeddings must have the same dimension in 1..4096",
        ));
    }
    let mut numbers = Vec::with_capacity(values.len());
    let mut scale = 0.0_f64;
    for value in values {
        let x = match value {
            Value::Int(x) => *x as f64,
            Value::Float(x) if x.is_finite() => *x,
            _ => return Err(bad(name, "embedding elements must be finite Int or Float")),
        };
        scale = scale.max(x.abs());
        numbers.push(x);
    }
    if scale == 0.0 {
        return Err(bad(name, "zero embeddings cannot be cosine-ranked"));
    }
    for x in &mut numbers {
        *x /= scale;
    }
    let norm = numbers.iter().map(|x| x * x).sum::<f64>().sqrt();
    for x in &mut numbers {
        *x /= norm;
    }
    Ok(numbers)
}
fn semantic(name: &str, args: &[Value]) -> Result<Value> {
    let query = embedding(name, &args[0], None)?;
    let Value::Array(docs) = &args[1] else {
        return Err(bad(name, "embedding corpus must be an Array"));
    };
    if docs.len() > 1024 {
        return Err(budget(name, "embedding corpus exceeds 1024 entries"));
    }
    if docs.len() * query.len() > 1048576 {
        return Err(budget(
            name,
            "embedding comparison exceeds 1048576 components",
        ));
    }
    let k = limit(name, args.get(2))?;
    let mut names = BTreeSet::new();
    let mut scored = Vec::new();
    for doc in docs {
        let Value::Record(fields) = doc else {
            return Err(bad(name, "embedding entries must be records"));
        };
        if fields.len() != 2 {
            return Err(bad(
                name,
                "embedding entries require exactly name and embedding",
            ));
        }
        let id = text(
            name,
            fields
                .get("name")
                .ok_or_else(|| bad(name, "document name required"))?,
        )?;
        if id.is_empty() || id.len() > 256 || !names.insert(id) {
            return Err(bad(
                name,
                "document names must be unique nonempty Strings, at most 256 bytes",
            ));
        }
        let vector = embedding(
            name,
            fields
                .get("embedding")
                .ok_or_else(|| bad(name, "document embedding required"))?,
            Some(query.len()),
        )?;
        let score = query
            .iter()
            .zip(vector)
            .map(|(a, b)| a * b)
            .sum::<f64>()
            .clamp(-1.0, 1.0);
        scored.push((id, score));
    }
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    Ok(Value::Array(
        scored
            .into_iter()
            .take(k)
            .map(|(id, score)| {
                row(vec![
                    ("name", Value::Str(id.into())),
                    ("score", Value::Float(score)),
                ])
            })
            .collect(),
    ))
}
fn documentation(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|a| {
            if !a.path().is_ident("doc") {
                return None;
            }
            let syn::Meta::NameValue(value) = &a.meta else {
                return None;
            };
            let syn::Expr::Lit(value) = &value.value else {
                return None;
            };
            let syn::Lit::Str(value) = &value.lit else {
                return None;
            };
            Some(value.value().trim_start().to_owned())
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn api(name: &str, source: &str) -> Result<Value> {
    let mut rows = Vec::new();
    for item in parse(name, source)?.items {
        let (ident, kind, vis, attrs, declaration) = match &item {
            syn::Item::Fn(x) => (
                x.sig.ident.to_string(),
                "function",
                &x.vis,
                &x.attrs,
                x.sig.to_token_stream().to_string(),
            ),
            syn::Item::Struct(x) => (
                x.ident.to_string(),
                "struct",
                &x.vis,
                &x.attrs,
                item.to_token_stream().to_string(),
            ),
            syn::Item::Enum(x) => (
                x.ident.to_string(),
                "enum",
                &x.vis,
                &x.attrs,
                item.to_token_stream().to_string(),
            ),
            syn::Item::Union(x) => (
                x.ident.to_string(),
                "union",
                &x.vis,
                &x.attrs,
                item.to_token_stream().to_string(),
            ),
            syn::Item::Trait(x) => (
                x.ident.to_string(),
                "trait",
                &x.vis,
                &x.attrs,
                item.to_token_stream().to_string(),
            ),
            syn::Item::Type(x) => (
                x.ident.to_string(),
                "alias",
                &x.vis,
                &x.attrs,
                item.to_token_stream().to_string(),
            ),
            syn::Item::Const(x) => (
                x.ident.to_string(),
                "const",
                &x.vis,
                &x.attrs,
                item.to_token_stream().to_string(),
            ),
            syn::Item::Static(x) => (
                x.ident.to_string(),
                "static",
                &x.vis,
                &x.attrs,
                item.to_token_stream().to_string(),
            ),
            _ => continue,
        };
        if !matches!(vis, syn::Visibility::Public(_)) {
            continue;
        }
        rows.push((
            ident.clone(),
            kind,
            row(vec![
                ("name", Value::Str(ident)),
                ("kind", Value::Str(kind.into())),
                ("declaration", Value::Str(declaration)),
                ("documentation", Value::Str(documentation(attrs))),
            ]),
        ));
    }
    rows.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    Ok(Value::Array(
        rows.into_iter().map(|(_, _, row)| row).collect(),
    ))
}
pub(super) fn run(name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (min, max) = match name {
        "docs_api" | "search_duplicates" => (1, 1),
        _ => (2, 3),
    };
    if input.is_some() || !(min..=max).contains(&args.len()) {
        return Err(bad(
            name,
            "wrong argument count or unexpected pipeline input",
        ));
    }
    match name {
        "docs_api" => api(name, text(name, &args[0])?),
        "search_semantic" => semantic(name, &args),
        "search_duplicates" => {
            let mut groups: BTreeMap<String, Vec<Function>> = BTreeMap::new();
            for f in corpus(name, &args[0])? {
                groups.entry(f.syntax.clone()).or_default().push(f);
            }
            let mut rows = groups
                .into_values()
                .filter(|g| g.len() > 1)
                .collect::<Vec<_>>();
            rows.sort_by(|a, b| (&a[0].document, &a[0].name).cmp(&(&b[0].document, &b[0].name)));
            Ok(Value::Array(
                rows.into_iter()
                    .map(|g| {
                        row(vec![
                            ("count", Value::Int(g.len() as i64)),
                            (
                                "occurrences",
                                Value::Array(g.iter().map(Function::location).collect()),
                            ),
                        ])
                    })
                    .collect(),
            ))
        }
        "search_similar" => {
            let query = functions(name, "<query>", text(name, &args[0])?)?;
            if query.len() != 1 {
                return Err(bad(
                    name,
                    "query source must contain exactly one free function",
                ));
            }
            let query = shingles(&query[0].syntax);
            let k = limit(name, args.get(2))?;
            let mut scored = Vec::new();
            for f in corpus(name, &args[1])? {
                let candidate = shingles(&f.syntax);
                let intersection = query.intersection(&candidate).count();
                let union = query.len() + candidate.len() - intersection;
                let score = if union == 0 {
                    1.0
                } else {
                    intersection as f64 / union as f64
                };
                scored.push((f, score));
            }
            scored.sort_by(|a, b| {
                b.1.total_cmp(&a.1)
                    .then_with(|| (&a.0.document, &a.0.name).cmp(&(&b.0.document, &b.0.name)))
            });
            Ok(Value::Array(
                scored
                    .into_iter()
                    .take(k)
                    .map(|(f, score)| {
                        let Value::Record(mut fields) = f.location() else {
                            unreachable!()
                        };
                        fields.insert("score".into(), Value::Float(score));
                        Value::Record(fields)
                    })
                    .collect(),
            ))
        }
        _ => unreachable!("internal search operation"),
    }
}
