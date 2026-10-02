//! Manifest script discovery. Commands are returned as data, never executed.
use crate::{safety, value::Value};
use anyhow::Result;
use serde_json::Value as Json;
use std::{collections::BTreeMap, io::Read};
const NAME: &str = "project_scripts";
fn bad(message: &str) -> anyhow::Error {
    safety::bad_state(NAME, message, "supply a valid supported manifest mapping")
}
fn entry(name: &str, command: Value, kind: &str, path: &str) -> Value {
    Value::Record(BTreeMap::from([
        ("name".into(), Value::Str(name.into())),
        ("command".into(), command),
        ("kind".into(), Value::Str(kind.into())),
        ("manifest".into(), Value::Str(path.into())),
    ]))
}
fn json_map(
    value: Option<&Json>,
    kind: &str,
    path: &str,
    tasks: bool,
    out: &mut Vec<Value>,
) -> Result<()> {
    let Some(value) = value else { return Ok(()) };
    let map = value
        .as_object()
        .ok_or_else(|| bad("scripts/tasks must be an object"))?;
    for (name, value) in map {
        let command = if tasks && value.is_object() {
            if let Some(dependencies) = value.get("dependencies") {
                let dependencies = dependencies
                    .as_array()
                    .ok_or_else(|| bad("task dependencies must be a String Array"))?;
                if dependencies.iter().any(|item| !item.is_string()) {
                    return Err(bad("task dependencies must be Strings"));
                }
            }
            match value.get("command") {
                Some(command) => Value::Str(
                    command
                        .as_str()
                        .ok_or_else(|| bad("task command must be String"))?
                        .into(),
                ),
                None if value.get("dependencies").is_some() => Value::Null,
                None => return Err(bad("task must declare a command or dependencies")),
            }
        } else {
            Value::Str(
                value
                    .as_str()
                    .ok_or_else(|| bad("script/task command must be String"))?
                    .into(),
            )
        };
        let mut result = entry(name, command, kind, path);
        if let Value::Record(fields) = &mut result {
            fields.insert("definition".into(), Value::from_json(value));
        }
        out.push(result);
    }
    Ok(())
}
fn toml_map(
    value: Option<&toml::Value>,
    kind: &str,
    path: &str,
    arrays: bool,
    out: &mut Vec<Value>,
) -> Result<()> {
    let Some(value) = value else { return Ok(()) };
    let map = value
        .as_table()
        .ok_or_else(|| bad("script/alias mapping must be a TOML table"))?;
    for (name, value) in map {
        let command = if let Some(s) = value.as_str() {
            Value::Str(s.into())
        } else if arrays {
            let a = value
                .as_array()
                .ok_or_else(|| bad("Cargo alias must be String or String Array"))?;
            if a.is_empty() {
                return Err(bad("Cargo alias Array must not be empty"));
            }
            Value::Array(
                a.iter()
                    .map(|v| {
                        v.as_str()
                            .map(|s| Value::Str(s.into()))
                            .ok_or_else(|| bad("Cargo alias elements must be String"))
                    })
                    .collect::<Result<_>>()?,
            )
        } else {
            return Err(bad("Python script target must be String"));
        };
        let mut result = entry(name, command.clone(), kind, path);
        if let Value::Record(fields) = &mut result {
            fields.insert("definition".into(), command);
        }
        out.push(result);
    }
    Ok(())
}
fn jsonc(source: &str) -> Result<String> {
    let bytes = source.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    let mut quoted = false;
    while i < bytes.len() {
        if quoted {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                quoted = false;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'"' {
            quoted = true;
            i += 1;
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
            out[i] = b' ';
            i += 1;
            while i < bytes.len() && bytes[i] != b'\n' {
                out[i] = b' ';
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    out[i] = b' ';
                    out[i + 1] = b' ';
                    i += 2;
                    closed = true;
                    break;
                }
                if !matches!(bytes[i], b'\n' | b'\r') {
                    out[i] = b' ';
                }
                i += 1;
            }
            if !closed {
                return Err(bad("unterminated JSONC comment"));
            }
            continue;
        }
        i += 1;
    }
    // Remove only trailing structural commas, leaving commas inside strings intact.
    i = 0;
    quoted = false;
    while i < out.len() {
        if quoted {
            if out[i] == b'\\' {
                i += 2;
                continue;
            }
            if out[i] == b'"' {
                quoted = false;
            }
        } else if out[i] == b'"' {
            quoted = true;
        } else if out[i] == b',' {
            let mut next = i + 1;
            while next < out.len() && out[next].is_ascii_whitespace() {
                next += 1;
            }
            let mut previous = i;
            while previous > 0 && out[previous - 1].is_ascii_whitespace() {
                previous -= 1;
            }
            let follows_value =
                previous > 0 && !matches!(out[previous - 1], b'{' | b'[' | b',' | b':');
            if follows_value && matches!(out.get(next), Some(b'}' | b']')) {
                out[i] = b' ';
            }
        }
        i += 1;
    }
    String::from_utf8(out).map_err(|_| bad("JSONC comment stripping produced invalid UTF-8"))
}
pub(super) fn guard_and_discover(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if args.len() != 1 || input.is_some() {
        return Err(safety::arg_err(
            "project_scripts requires one manifest path without pipeline input",
        ));
    }
    let Value::Str(path) = &args[0] else {
        return Err(safety::arg_err("project_scripts path must be String"));
    };
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if !matches!(ext, "json" | "jsonc" | "toml") {
        return Err(safety::arg_err(
            "project_scripts supports JSON, JSONC and TOML manifests",
        ));
    }
    let resolved = safety::resolve_path_str(path);
    let file_path = crate::security::validate_read_path(&resolved).map_err(|e| {
        if e.downcast_ref::<safety::SafetyError>().is_some() {
            return e;
        }
        if let Some(io) = e.downcast_ref::<std::io::Error>() {
            return safety::fs_error(NAME, &resolved, io);
        }
        safety::arg_err(format!("project_scripts: invalid read path: {e}"))
    })?;
    let actual = file_path
        .to_str()
        .ok_or_else(|| bad("resolved manifest path is not UTF-8"))?
        .to_owned();
    safety::guard(safety::GuardCtx {
        builtin: NAME,
        effect: safety::Effect::ReadLocal,
        what: "read manifest",
        targets: vec![actual.clone()],
        blast_radius: serde_json::json!({"path":actual}),
        reversible: true,
        fs_paths: true,
    })?;
    let meta = std::fs::metadata(&file_path).map_err(|e| safety::fs_error(NAME, &file_path, &e))?;
    if !meta.is_file() {
        return Err(safety::arg_err(
            "project_scripts requires a regular manifest file",
        ));
    }
    if meta.len() > 1048576 {
        return Err(safety::budget_exceeded(
            NAME,
            "manifest exceeds 1 MiB".into(),
            "use a smaller manifest",
        ));
    }
    let file =
        std::fs::File::open(&file_path).map_err(|e| safety::fs_error(NAME, &file_path, &e))?;
    let mut source = String::new();
    file.take(1048577)
        .read_to_string(&mut source)
        .map_err(|e| safety::fs_error(NAME, &file_path, &e))?;
    if source.len() > 1048576 {
        return Err(safety::budget_exceeded(
            NAME,
            "manifest grew beyond 1 MiB".into(),
            "use a smaller manifest",
        ));
    }
    let mut entries = Vec::new();
    match ext {
        "json" | "jsonc" => {
            let source = if ext == "jsonc" {
                jsonc(&source)?
            } else {
                source
            };
            let value: Json =
                serde_json::from_str(&source).map_err(|_| bad("invalid JSON manifest"))?;
            let object = value
                .as_object()
                .ok_or_else(|| bad("manifest root must be an object"))?;
            json_map(
                object.get("scripts"),
                "npm_script",
                &actual,
                false,
                &mut entries,
            )?;
            json_map(
                object.get("tasks"),
                "deno_task",
                &actual,
                true,
                &mut entries,
            )?;
        }
        "toml" => {
            let value: toml::Value =
                toml::from_str(&source).map_err(|_| bad("invalid TOML manifest"))?;
            if let Some(project) = value.get("project") {
                let project = project
                    .as_table()
                    .ok_or_else(|| bad("project must be a TOML table"))?;
                toml_map(
                    project.get("scripts"),
                    "python_console_script",
                    &actual,
                    false,
                    &mut entries,
                )?;
                toml_map(
                    project.get("gui-scripts"),
                    "python_gui_script",
                    &actual,
                    false,
                    &mut entries,
                )?;
            }
            toml_map(
                value.get("alias"),
                "cargo_alias",
                &actual,
                true,
                &mut entries,
            )?;
        }
        _ => unreachable!("validated extension"),
    }
    if entries.len() > 10000 {
        return Err(safety::budget_exceeded(
            NAME,
            "more than 10000 manifest scripts".into(),
            "use a smaller manifest",
        ));
    }
    fn sort_key(value: &Value) -> (&str, &str) {
        let Value::Record(fields) = value else {
            unreachable!("internal manifest entry")
        };
        let Value::Str(name) = &fields["name"] else {
            unreachable!("internal manifest name")
        };
        let Value::Str(kind) = &fields["kind"] else {
            unreachable!("internal manifest kind")
        };
        (name, kind)
    }
    entries.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
    Ok(Value::Array(entries))
}
