//! Exact warning-code filtering of supplied rustc/Cargo JSON lines.
use crate::{safety, value::Value};
use anyhow::Result;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value as Json;
use std::{collections::BTreeSet, fmt};
fn bad(message: &str) -> anyhow::Error {
    safety::arg_err(format!("diag_suppress: {message}"))
}
fn bounded() -> anyhow::Error {
    safety::budget_exceeded(
        "diag_suppress",
        "report or selectors exceed limits".into(),
        "supply at most 1 MiB/25000 lines/64 KiB per line and 128 codes of at most 128 bytes",
    )
}
// Reject ambiguous duplicate JSON keys at every depth, before filtering.
struct Strict(Json);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct StrictVisitor;
        impl<'de> Visitor<'de> for StrictVisitor {
            type Value = Strict;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Strict, E> {
                Ok(Strict(Json::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Strict, E> {
                serde_json::Number::from_f64(v)
                    .map(|v| Strict(Json::Number(v)))
                    .ok_or_else(|| E::custom("invalid number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Strict, E> {
                Ok(Strict(Json::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut values = Vec::new();
                while let Some(Strict(v)) = a.next_element()? {
                    values.push(v);
                }
                Ok(Strict(Json::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = a.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom("duplicate key"));
                    }
                    let Strict(v) = a.next_value()?;
                    values.insert(key, v);
                }
                Ok(Strict(Json::Object(values)))
            }
        }
        d.deserialize_any(StrictVisitor)
    }
}
fn valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.split("::").all(|part| {
            part.bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}
fn suppress(line: &str, codes: &BTreeSet<&str>) -> Result<bool> {
    let Strict(event) = serde_json::from_str::<Strict>(line)
        .map_err(|_| bad("invalid JSON line or duplicate keys"))?;
    let object = event
        .as_object()
        .ok_or_else(|| bad("each line must be a JSON object"))?;
    let diagnostic = if let Some(reason) = object.get("reason") {
        let reason = reason
            .as_str()
            .ok_or_else(|| bad("Cargo reason must be String"))?;
        if reason != "compiler-message" {
            return Ok(false);
        }
        object
            .get("message")
            .and_then(Json::as_object)
            .ok_or_else(|| bad("compiler-message requires a diagnostic object"))?
    } else {
        if object.get("$message_type").and_then(Json::as_str) != Some("diagnostic") {
            return Err(bad("expected rustc diagnostic or Cargo event"));
        }
        object
    };
    let level = diagnostic
        .get("level")
        .and_then(Json::as_str)
        .ok_or_else(|| bad("diagnostic level must be String"))?;
    if !matches!(
        level,
        "error" | "warning" | "note" | "help" | "failure-note" | "ice"
    ) {
        return Err(bad("unknown diagnostic level"));
    }
    if diagnostic.get("message").and_then(Json::as_str).is_none() {
        return Err(bad("diagnostic message must be String"));
    }
    let code = match diagnostic.get("code") {
        Some(Json::Null) => None,
        Some(Json::Object(code)) => {
            let code = code
                .get("code")
                .and_then(Json::as_str)
                .ok_or_else(|| bad("diagnostic code must contain a code String"))?;
            if code.len() > 128 || !valid_code(code) {
                return Err(bad("invalid diagnostic code"));
            }
            Some(code)
        }
        _ => return Err(bad("diagnostic code must be null or an object")),
    };
    Ok(level == "warning" && code.is_some_and(|code| codes.contains(code)))
}
pub(super) fn run(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if args.len() != 2 || input.is_some() {
        return Err(bad("expected report and codes, no pipeline input"));
    }
    let (Value::Str(report), Value::Array(selectors)) = (&args[0], &args[1]) else {
        return Err(bad("expected report String and codes Array"));
    };
    if report.len() > 1048576 || selectors.len() > 128 {
        return Err(bounded());
    }
    if selectors.is_empty() {
        return Err(bad("codes must be nonempty"));
    }
    let mut codes = BTreeSet::new();
    for selector in selectors {
        let Value::Str(code) = selector else {
            return Err(bad("codes must be Strings"));
        };
        if code.len() > 128 {
            return Err(bounded());
        }
        if !valid_code(code) || !codes.insert(code.as_str()) {
            return Err(bad("codes must be valid and unique"));
        }
    }
    let mut output = String::with_capacity(report.len());
    for (index, line) in report.split_inclusive('\n').enumerate() {
        if index >= 25000 || line.len() > 65536 {
            return Err(bounded());
        }
        if !suppress(line, &codes)? {
            output.push_str(line);
        }
    }
    Ok(Value::Str(output))
}
