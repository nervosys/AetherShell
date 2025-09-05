// src/builtins.rs
use anyhow::{Result, anyhow};
use std::collections::BTreeMap;

use crate::{
    env::Env,
    eval::eval_expr,
    value::{Lambda, Value},
};

// --------------- Public entry points ---------------

pub fn call(name: &str, args: Vec<Value>, env: &mut Env) -> Result<Value> {
    call_with_input(name, args, None, env)
}
/// Call a builtin with optional piped input
pub fn call_with_input(
    name: &str,
    args: Vec<Value>,
    input: Option<Value>,
    env: &mut Env,
) -> Result<Value> {
    match name {
        // General
        "help" => bi_help(),
        "call" => bi_call(args, input, env),
        "clear" => bi_clear(),
        "echo" => bi_echo(&args),
        "print" => bi_print(args, input),
        "http_get" => bi_http_get(args, input),

        // Data / pipelines
        "map" => bi_map(args, input, env),
        "where" => bi_where(args, input, env),
        "reduce" => bi_reduce(args, input, env),
        "take" => bi_take(args, input),

        // AI/Agents (thin wrappers; optional module must exist)
        "agent" => bi_agent(args, input, env),
        "swarm" => bi_swarm(args, input, env),

        _ => Err(anyhow!("unknown builtin: {}", name)),
    }
}

// --------------- Helpers: type extraction ---------------

fn expect_array<'a>(name: &str, v: &'a Value) -> Result<&'a [Value]> {
    if let Value::Array(a) = v {
        Ok(a.as_slice())
    } else {
        Err(anyhow!("{} requires array input, got {:?}", name, v))
    }
}
fn expect_int(name: &str, v: &Value) -> Result<i64> {
    match v {
        Value::Int(n) => Ok(*n),
        Value::Float(f) if f.fract() == 0.0 => Ok(*f as i64),
        _ => Err(anyhow!("{} expects integer, got {:?}", name, v)),
    }
}
fn need_lambda<'a>(v: &'a Value, name: &str) -> Result<&'a Lambda> {
    if let Value::Lambda(l) = v {
        Ok(l)
    } else {
        Err(anyhow!("{} expects lambda, got {:?}", name, v))
    }
}

/// Evaluate a lambda with N positional arguments by temporarily binding its `params`
/// in the environment, then `eval_expr` on its body. Restores env afterwards.
fn call_lambda(lam: &Lambda, args: &[Value], env: &mut Env) -> Result<Value> {
    let params = &lam.params;
    if params.len() != args.len() {
        return Err(anyhow!(
            "lambda expected {} args, got {}",
            params.len(),
            args.len()
        ));
    }

    // Save previous bindings
    let mut saved: Vec<(String, Option<Value>)> = Vec::with_capacity(params.len());
    for (i, p) in params.iter().enumerate() {
        let prev = env.get_var(p).cloned();
        saved.push((p.clone(), prev));
        env.set_var(p, args[i].clone());
    }

    // Eval and restore
    let result = eval_expr(&lam.body, env);

    for (name, prev) in saved.into_iter().rev() {
        match prev {
            Some(v) => env.set_var(&name, v),
            None => env.del_var(&name),
        }
    }
    result
}

// --------------- General builtins ---------------

fn bi_help() -> Result<Value> {
    let txt = r#"Aurora (ae) built-ins:
- help                         # this help
- clear                        # clear screen (prints ANSI)
- echo <...values>             # echo stringified values
- print <value>                # pretty-print value (returns text)

Data / pipelines:
- map    <array> <fn(x)=> expr>             # map over array
- where  <array> <fn(x)=> predicate>        # filter array
- reduce <array> <fn(a,b)=> expr> <init>    # fold array
- take   <array> <n>                        # take first n

HTTP:
- http_get <url>                # fetch URL → {{url,status,headers,body}}

AI / Agents (require ai module present):
- agent <goal> [tools...] [max_steps] [dry_run]
- swarm <json-config|record>  OR  <goal> [tools...] [max_steps] [dry_run]

Examples:
  [1,2,3] | map fn(x)=> x*2 | reduce fn(a,b)=> a+b 0
  [5,4,3,2,1] | where fn(x)=> x>2 | take 2 | print
  http_get "https://api.github.com" | print
"#;
    Ok(Value::Str(txt.to_string()))
}

fn bi_clear() -> Result<Value> {
    // CSI 2J (clear screen) + CSI H (cursor home)
    Ok(Value::Str("\u{1b}[2J\u{1b}[H".to_string()))
}

fn bi_echo(args: &[Value]) -> Result<Value> {
    let mut s = String::new();
    for (i, v) in args.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format_value_one_line(v));
    }
    Ok(Value::Str(s))
}

fn bi_print(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    // Determine the target Value (either piped input or first arg).
    let target = if let Some(v) = input {
        v
    } else {
        args.into_iter()
            .next()
            .ok_or_else(|| anyhow!("print expects a value"))?
    };

    // (Removed an unused strip_ansi helper; tests have their own)

    // Print the pretty inline display to stdout (with colors).
    let pretty =
        crate::value::pretty::display_inline(&target, &crate::value::pretty::Theme::default());
    println!("{}", pretty);

    // Helper to strip ANSI color sequences from a string
    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut it = s.chars().peekable();
        while let Some(ch) = it.next() {
            if ch == '\x1b' {
                if let Some('[') = it.peek() {
                    it.next();
                    while let Some(&c) = it.peek() {
                        it.next();
                        if c == 'm' {
                            break;
                        }
                    }
                    continue;
                }
            }
            out.push(ch);
        }
        out
    }

    // Return behavior: always return the pretty inline display (ANSI stripped)
    // as a `Str`. This keeps the REPL and tests consistent regardless of
    // whether the input was a bare string or another value.
    Ok(Value::Str(strip_ansi(&pretty)))
}

fn bi_http_get(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let url_val = if let Some(v) = input {
        v
    } else {
        args.into_iter()
            .next()
            .ok_or_else(|| anyhow!("http_get expects URL string/uri"))?
    };
    let url = match url_val {
        Value::Str(s) | Value::Uri(s) => s,
        other => return Err(anyhow!("http_get expects URL string/uri, got {:?}", other)),
    };

    // Real blocking GET using reqwest (enable "blocking" feature)
    let resp = reqwest::blocking::get(&url)?;
    let status = resp.status().as_u16() as i64;
    let mut headers_rec = BTreeMap::<String, Value>::new();
    for (k, v) in resp.headers().iter() {
        headers_rec.insert(
            k.to_string(),
            Value::Str(v.to_str().unwrap_or("").to_string()),
        );
    }
    let body_text = resp.text().unwrap_or_default();

    let mut out = BTreeMap::<String, Value>::new();
    out.insert("url".into(), Value::Str(url));
    out.insert("status".into(), Value::Int(status));
    out.insert("headers".into(), Value::Record(headers_rec));
    out.insert("body".into(), Value::Str(body_text));
    Ok(Value::Record(out))
}

fn bi_call(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    // call <name> [args...]
    let name_val = args
        .get(0)
        .cloned()
        .ok_or_else(|| anyhow!("call requires name"))?;
    let name = match name_val {
        Value::Str(s) | Value::Uri(s) => s,
        other => {
            return Err(anyhow!(
                "call: expected String builtin name, got {:?}",
                other
            ));
        }
    };
    // remaining args (after name)
    let mut rem: Vec<Value> = Vec::new();
    for v in args.into_iter().skip(1) {
        rem.push(v);
    }
    // If input was provided, pass it as pipe input to the called builtin
    call_with_input(&name, rem, input, env)
}

// --------------- Data / pipeline builtins ---------------

fn bi_map(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    // Array comes from pipe if present; else first arg. Use references to avoid moving `input`.
    let arr_val = if let Some(ref v) = input {
        v.clone()
    } else {
        args.get(0)
            .cloned()
            .ok_or_else(|| anyhow!("map requires array input"))?
    };

    let lam_val = if input.is_some() {
        args.get(0).ok_or_else(|| anyhow!("map requires lambda"))?
    } else {
        args.get(1).ok_or_else(|| anyhow!("map requires lambda"))?
    };
    let lam = need_lambda(lam_val, "map")?;

    let arr = expect_array("map", &arr_val)?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, v) in arr.iter().cloned().enumerate() {
        // clone v/acc before trying fallbacks to avoid use-after-move
        let v_clone = v.clone();
        let y = call_lambda(lam, &[v.clone(), Value::Int(i as i64)], env)
            .or_else(|_| call_lambda(lam, &[v_clone], env))?; // support fn(x,i) or fn(x)
        out.push(y);
    }
    Ok(Value::Array(out))
}

fn bi_where(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let arr_val = if let Some(ref v) = input {
        v.clone()
    } else {
        args.get(0)
            .cloned()
            .ok_or_else(|| anyhow!("where requires array input"))?
    };

    let lam_val = if input.is_some() {
        args.get(0)
            .ok_or_else(|| anyhow!("where requires lambda"))?
    } else {
        args.get(1)
            .ok_or_else(|| anyhow!("where requires lambda"))?
    };
    let lam = need_lambda(lam_val, "where")?;

    let arr = expect_array("where", &arr_val)?;
    let mut out = Vec::new();
    for (i, v) in arr.iter().cloned().enumerate() {
        let v_clone = v.clone();
        let keep_val = call_lambda(lam, &[v_clone.clone(), Value::Int(i as i64)], env)
            .or_else(|_| call_lambda(lam, &[v_clone], env))?;
        match keep_val {
            Value::Bool(true) => out.push(v),
            Value::Bool(false) => {}
            other => return Err(anyhow!("where predicate must return Bool, got {:?}", other)),
        }
    }
    Ok(Value::Array(out))
}

fn bi_reduce(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    // Cases:
    //  - piping:  [1,2,3] | reduce fn(a,b)=>... 0
    //  - direct:  reduce [1,2,3] fn(a,b)=>... 0
    // Determine arr, lambda index and init index without moving `input`
    let arr_val = if let Some(ref v) = input {
        v.clone()
    } else {
        if args.len() < 3 {
            return Err(anyhow!("reduce expects <array> <fn(a,b)=> expr> <init>"));
        }
        args[0].clone()
    };
    let (lam_idx, init_idx) = if input.is_some() {
        (0usize, 1usize)
    } else {
        (1usize, 2usize)
    };

    let lam = need_lambda(
        args.get(lam_idx)
            .ok_or_else(|| anyhow!("reduce missing lambda"))?,
        "reduce",
    )?;
    let init = args
        .get(init_idx)
        .cloned()
        .ok_or_else(|| anyhow!("reduce missing init"))?;

    let arr = expect_array("reduce", &arr_val)?;
    let mut acc = init;
    for (i, v) in arr.iter().cloned().enumerate() {
        // try fn(a,b,i) then fallback to fn(a,b); clone acc before trying fallbacks
        let acc1 = acc.clone();
        let acc2 = acc.clone();
        acc = call_lambda(lam, &[acc1, v.clone(), Value::Int(i as i64)], env)
            .or_else(|_| call_lambda(lam, &[acc2, v], env))?;
    }
    Ok(acc)
}

fn bi_take(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr_val, n_idx) = if input.is_some() {
        (input.unwrap(), 0usize)
    } else {
        if args.len() < 2 {
            return Err(anyhow!("take expects <array> <n>"));
        }
        (args[0].clone(), 1usize)
    };

    let n = expect_int(
        "take",
        args.get(n_idx).ok_or_else(|| anyhow!("take missing n"))?,
    )?;
    let arr = expect_array("take", &arr_val)?;

    let take_n = if n < 0 { 0 } else { n as usize };
    Ok(Value::Array(arr.iter().cloned().take(take_n).collect()))
}

// --------------- AI / Agents wrappers (optional) ---------------

fn bi_agent(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    // Accept either: agent "<goal>" [tools...] [max_steps] [dry_run]
    // or a record config: {goal:"...", tools:["..."], max_steps:3, dry_run:true, model_uri:"..."}
    if let Some(Value::Record(cfg)) = input {
        return agent_from_record(cfg, env);
    }
    if let Some(Value::Record(cfg)) = args.get(0) {
        return agent_from_record(cfg.clone(), env);
    }
    // positional
    let goal = match args.get(0) {
        Some(Value::Str(s)) => s.clone(),
        Some(other) => return Err(anyhow!("agent: expected String goal, got {:?}", other)),
        None => return Err(anyhow!("agent requires goal")),
    };
    let mut tools: Vec<String> = Vec::new();
    let mut max_steps: usize = 8;
    let mut dry_run = false;

    // tools: collect following strings until number/bool appears
    let mut i = 1usize;
    // If second positional arg is a JSON config string, try to parse it
    if let Some(Value::Str(s)) = args.get(1) {
        let t = s.trim();
        if t.starts_with('{') {
            // try parse JSON into a Record Value and delegate, merge goal
            if let Ok(jv) = serde_json::from_str::<serde_json::Value>(t) {
                let mut cfg_val = crate::value::Value::from_json(&jv);
                if let Value::Record(ref mut cfg) = cfg_val {
                    // if we have a goal string in args[0], ensure the record has goal
                    if let Some(Value::Str(goal_s)) = args.get(0) {
                        cfg.insert("goal".into(), Value::Str(goal_s.clone()));
                        return agent_from_record(cfg.clone(), env);
                    } else {
                        return Err(anyhow!(
                            "agent config must include goal when provided as JSON"
                        ));
                    }
                } else {
                    return Err(anyhow!("agent config must be a record"));
                }
            } else {
                return Err(anyhow!("agent config JSON parse error"));
            }
        }
    }

    while let Some(Value::Str(s)) = args.get(i) {
        tools.push(s.clone());
        i += 1;
    }
    // Also accept a single positional array as tools: agent(goal, ["ls","print"], ...)
    if tools.is_empty() {
        if let Some(Value::Array(vs)) = args.get(1) {
            for v in vs {
                if let Value::Str(s) = v {
                    tools.push(s.clone());
                } else {
                    return Err(anyhow!("agent: tools array must contain strings"));
                }
            }
            i = 2;
        }
    }
    if let Some(v) = args.get(i) {
        if let Value::Int(n) = v {
            max_steps = (*n).max(0) as usize;
            i += 1;
        }
    }
    if let Some(v) = args.get(i) {
        if let Value::Bool(b) = v {
            dry_run = *b;
        }
    }
    let tool_refs: Vec<&str> = tools.iter().map(|s| s.as_str()).collect();
    let out = crate::ai::agents::run_sync(&goal, &tool_refs, max_steps, dry_run, env)?;
    Ok(Value::Str(out))
}

fn agent_from_record(cfg: BTreeMap<String, Value>, env: &mut Env) -> Result<Value> {
    let goal = match cfg.get("goal") {
        Some(Value::Str(s)) => s.clone(),
        _ => return Err(anyhow!("agent config requires {{goal: String}}")),
    };
    let tools: Vec<String> = match cfg.get("tools") {
        Some(Value::Array(vs)) => {
            // Validate all tools are strings
            let mut out = Vec::new();
            for v in vs {
                if let Value::Str(s) = v {
                    out.push(s.clone());
                } else {
                    return Err(anyhow!("agent config tools must be array of strings"));
                }
            }
            out
        }
        _ => vec![],
    };
    let max_steps = match cfg.get("max_steps") {
        Some(Value::Int(n)) if *n >= 0 => *n as usize,
        _ => 8usize,
    };
    let dry_run = matches!(cfg.get("dry_run"), Some(Value::Bool(true)));

    let tool_refs: Vec<&str> = tools.iter().map(|s| s.as_str()).collect();
    let out = crate::ai::agents::run_sync(&goal, &tool_refs, max_steps, dry_run, env)?;
    Ok(Value::Str(out))
}

fn bi_swarm(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    // Accept {goal, tools, max_steps, dry_run} or just a goal + tools...
    if let Some(Value::Record(cfg)) = input {
        return swarm_from_record(cfg, env);
    }
    if let Some(Value::Record(cfg)) = args.get(0) {
        return swarm_from_record(cfg.clone(), env);
    }

    // positional like agent()
    let goal = match args.get(0) {
        Some(Value::Str(s)) => s.clone(),
        Some(other) => return Err(anyhow!("swarm: expected String goal, got {:?}", other)),
        None => return Err(anyhow!("swarm requires goal")),
    };
    let mut tools: Vec<String> = Vec::new();
    let mut max_steps: usize = 12;
    let mut dry_run = false;

    let mut i = 1usize;
    // If second positional arg is a JSON config string, try parse and delegate
    if let Some(Value::Str(s)) = args.get(1) {
        let t = s.trim();
        if t.starts_with('{') {
            if let Ok(jv) = serde_json::from_str::<serde_json::Value>(t) {
                let mut cfg_val = crate::value::Value::from_json(&jv);
                if let Value::Record(ref mut cfg) = cfg_val {
                    if let Some(Value::Str(goal_s)) = args.get(0) {
                        cfg.insert("goal".into(), Value::Str(goal_s.clone()));
                        return swarm_from_record(cfg.clone(), env);
                    } else {
                        return Err(anyhow!(
                            "swarm config must include goal when provided as JSON"
                        ));
                    }
                } else {
                    return Err(anyhow!("swarm config must be a record"));
                }
            } else {
                return Err(anyhow!("swarm config JSON parse error"));
            }
        }
    }
    while let Some(Value::Str(s)) = args.get(i) {
        tools.push(s.clone());
        i += 1;
    }
    // Also accept tools as a single array positional argument
    if tools.is_empty() {
        if let Some(Value::Array(vs)) = args.get(1) {
            for v in vs {
                if let Value::Str(s) = v {
                    tools.push(s.clone());
                } else {
                    return Err(anyhow!("swarm: tools array must contain strings"));
                }
            }
            i = 2;
        }
    }
    if let Some(v) = args.get(i) {
        if let Value::Int(n) = v {
            max_steps = (*n).max(0) as usize;
            i += 1;
        }
    }
    if let Some(v) = args.get(i) {
        if let Value::Bool(b) = v {
            dry_run = *b;
        }
    }

    let tool_refs: Vec<&str> = tools.iter().map(|s| s.as_str()).collect();
    // For compatibility we call agents::run_sync (single agent) from the swarm shim in ai.rs
    let out = crate::ai::agents::swarm::run_sync(&goal, &tool_refs, max_steps, dry_run, env)?;
    Ok(Value::Str(out))
}

fn swarm_from_record(cfg: BTreeMap<String, Value>, env: &mut Env) -> Result<Value> {
    let goal = match cfg.get("goal") {
        Some(Value::Str(s)) => s.clone(),
        _ => return Err(anyhow!("swarm config requires {{goal: String}}")),
    };
    let tools: Vec<String> = match cfg.get("tools") {
        Some(Value::Array(vs)) => {
            let mut out = Vec::new();
            for v in vs {
                if let Value::Str(s) = v {
                    out.push(s.clone());
                } else {
                    return Err(anyhow!("swarm config tools must be array of strings"));
                }
            }
            out
        }
        _ => vec![],
    };
    let max_steps = match cfg.get("max_steps") {
        Some(Value::Int(n)) if *n >= 0 => *n as usize,
        _ => 12usize,
    };
    let dry_run = matches!(cfg.get("dry_run"), Some(Value::Bool(true)));

    let tool_refs: Vec<&str> = tools.iter().map(|s| s.as_str()).collect();
    let out = crate::ai::agents::swarm::run_sync(&goal, &tool_refs, max_steps, dry_run, env)?;
    Ok(Value::Str(out))
}

// --------------- Formatting helpers ---------------

fn format_value_one_line(v: &Value) -> String {
    // keep this consistent with your pretty module; simplified for echo
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(x) => x.to_string(),
        Value::Str(s) => s.clone(),
        Value::Uri(s) => s.clone(),
        Value::Array(a) => format!("[len={}]", a.len()),
        Value::Record(_) => "{…}".into(),
        Value::Table(t) => format!("<Table rows={}>", t.rows.len()),
        Value::Lambda(_) => "<lambda>".into(),
    }
}
