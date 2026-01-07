// src/builtins.rs
use anyhow::{anyhow, Context, Result};
use serde_json;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use walkdir::WalkDir;

use crate::{
    env::Env,
    eval::eval_expr,
    security::{
        check_file_size_limit, check_rate_limit, create_secure_http_client, validate_ai_prompt,
        validate_http_url, validate_read_path,
    },
    shell_features::{Parameter, ParameterSet, ParameterType, PipelineInputType},
    value::{Lambda, Value},
};
use std::time::Duration;

// Fast builtin lookup using hash map
lazy_static::lazy_static! {
    static ref BUILTIN_LOOKUP: HashMap<&'static str, usize> = {
        let mut map = HashMap::new();

        // General functions
        map.insert("help", 0);
        map.insert("call", 1);
        map.insert("clear", 2);
        map.insert("echo", 3);
        map.insert("print", 4);
        map.insert("http_get", 5);

        // Option constructors
        map.insert("Some", 6);
        map.insert("None", 7);

        // File system
        map.insert("ls", 8);
        map.insert("list", 8); // alias
        map.insert("pwd", 9);
        map.insert("cat", 10);
        map.insert("read_text", 11);
        map.insert("head", 12);
        map.insert("tail", 13);
        map.insert("find", 14);
        map.insert("sort", 15);
        map.insert("uniq", 16);
        map.insert("wc", 17);
        map.insert("grep", 18);

        // Data/pipelines
        map.insert("map", 19);
        map.insert("where", 20);
        map.insert("reduce", 21);
        map.insert("take", 22);
        map.insert("keys", 23);
        map.insert("len", 24);
        map.insert("length", 24); // alias
        map.insert("type_of", 25);
        map.insert("typeof", 25); // alias

        // Array functions
        map.insert("first", 26);
        map.insert("last", 27);
        map.insert("any", 28);
        map.insert("all", 29);

        // MCP functions
        map.insert("mcp_servers", 30);
        map.insert("mcp-servers", 30); // alias
        map.insert("mcp_detect", 31);
        map.insert("mcp-detect", 31); // alias
        map.insert("mcp_cache_clear", 32);
        map.insert("mcp-cache-clear", 32); // alias
        map.insert("mcp_cache_status", 33);
        map.insert("mcp-cache-status", 33); // alias

        // String functions
        map.insert("split", 34);
        map.insert("join", 35);
        map.insert("trim", 36);
        map.insert("upper", 37);
        map.insert("lower", 38);
        map.insert("replace", 39);
        map.insert("contains", 40);
        map.insert("starts_with", 41);
        map.insert("ends_with", 42);

        // Array functions (extended)
        map.insert("flatten", 43);
        map.insert("reverse", 44);
        map.insert("slice", 45);
        map.insert("range", 46);
        map.insert("zip", 47);
        map.insert("push", 48);
        map.insert("concat", 49);

        // Math functions
        map.insert("abs", 50);
        map.insert("min", 51);
        map.insert("max", 52);
        map.insert("floor", 53);
        map.insert("ceil", 54);
        map.insert("round", 55);
        map.insert("sqrt", 56);
        map.insert("pow", 57);

        // Utility functions
        map.insert("exit", 58);
        map.insert("env", 59);
        map.insert("set_env", 60);
        map.insert("sleep", 61);
        map.insert("time", 62);
        map.insert("json_parse", 63);
        map.insert("json_stringify", 64);

        // Syntax Knowledge Base functions
        map.insert("syntax_get", 65);
        map.insert("syntax_list", 66);
        map.insert("syntax_search", 67);
        map.insert("syntax_add", 68);
        map.insert("syntax_categories", 69);
        map.insert("ab_encode", 70);
        map.insert("ab_decode", 71);

        map
    };
}

// Fast dispatch table for builtin functions
static BUILTIN_DISPATCH: &[fn(Vec<Value>, Option<Value>, &mut Env) -> Result<Value>] = &[
    // 0-9: General and file system functions
    |_, _, _| bi_help(),
    |args, input, env| bi_call(args, input, env),
    |_, _, _| bi_clear(),
    |args, _, _| bi_echo(&args),
    |args, input, _| bi_print(args, input),
    |args, input, _| bi_http_get(args, input),
    |args, _, _| bi_some(args),
    |_, _, _| bi_none(),
    |args, input, _| bi_ls(args, input),
    |_, _, _| bi_pwd(),
    // 10-19: File operations and data functions
    |args, input, _| bi_cat(args, input),
    |args, input, _| bi_read_text(args, input),
    |args, input, _| bi_head(args, input),
    |args, input, _| bi_tail(args, input),
    |args, input, _| bi_find(args, input),
    |args, input, _| bi_sort(args, input),
    |args, input, _| bi_uniq(args, input),
    |args, input, _| bi_wc(args, input),
    |args, input, _| bi_grep(args, input),
    |args, input, env| bi_map(args, input, env),
    // 20-29: Pipeline and array functions
    |args, input, env| bi_where(args, input, env),
    |args, input, env| bi_reduce(args, input, env),
    |args, input, _| bi_take(args, input),
    |args, input, _| bi_keys(args, input),
    |args, input, _| bi_len(args, input),
    |args, input, _| bi_type_of(args, input),
    |args, input, _| bi_first(args, input),
    |args, input, _| bi_last(args, input),
    |args, input, env| bi_any(args, input, env),
    |args, input, env| bi_all(args, input, env),
    // 30-33: MCP functions
    |args, input, _| bi_mcp_servers(args, input),
    |args, input, _| bi_mcp_detect(args, input),
    |args, input, _| bi_mcp_cache_clear(args, input),
    |args, input, _| bi_mcp_cache_status(args, input),
    // 34-42: String functions
    |args, input, _| bi_split(args, input),
    |args, input, _| bi_join(args, input),
    |args, input, _| bi_trim(args, input),
    |args, input, _| bi_upper(args, input),
    |args, input, _| bi_lower(args, input),
    |args, input, _| bi_replace(args, input),
    |args, input, _| bi_contains(args, input),
    |args, input, _| bi_starts_with(args, input),
    |args, input, _| bi_ends_with(args, input),
    // 43-49: Array functions (extended)
    |args, input, _| bi_flatten(args, input),
    |args, input, _| bi_reverse(args, input),
    |args, input, _| bi_slice(args, input),
    |args, input, _| bi_range(args, input),
    |args, input, _| bi_zip(args, input),
    |args, input, _| bi_push(args, input),
    |args, input, _| bi_concat(args, input),
    // 50-57: Math functions
    |args, input, _| bi_abs(args, input),
    |args, input, _| bi_min(args, input),
    |args, input, _| bi_max(args, input),
    |args, input, _| bi_floor(args, input),
    |args, input, _| bi_ceil(args, input),
    |args, input, _| bi_round(args, input),
    |args, input, _| bi_sqrt(args, input),
    |args, input, _| bi_pow(args, input),
    // 58-64: Utility functions
    |args, input, _| bi_exit(args, input),
    |args, input, _| bi_env(args, input),
    |args, input, _| bi_set_env(args, input),
    |args, input, _| bi_sleep(args, input),
    |args, input, _| bi_time(args, input),
    |args, input, _| bi_json_parse(args, input),
    |args, input, _| bi_json_stringify(args, input),
    // 65-71: Syntax Knowledge Base functions
    |args, input, _| bi_syntax_get(args, input),
    |args, input, _| bi_syntax_list(args, input),
    |args, input, _| bi_syntax_search(args, input),
    |args, input, _| bi_syntax_add(args, input),
    |args, input, _| bi_syntax_categories(args, input),
    |args, input, _| bi_ab_encode(args, input),
    |args, input, _| bi_ab_decode(args, input),
];

fn fast_builtin_lookup(
    name: &str,
    args: Vec<Value>,
    input: Option<Value>,
    env: &mut Env,
) -> Option<Result<Value>> {
    if let Some(&index) = BUILTIN_LOOKUP.get(name) {
        if index < BUILTIN_DISPATCH.len() {
            Some(BUILTIN_DISPATCH[index](args, input, env))
        } else {
            None
        }
    } else {
        None
    }
}

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
    // Try fast lookup first
    if let Some(result) = fast_builtin_lookup(name, args.clone(), input.clone(), env) {
        return result;
    }

    // Fall back to the comprehensive match for functions not in fast lookup
    match name {
        // PowerShell-style cmdlets (not in fast lookup)
        "Get-Files" | "get-files" => bi_get_files(args, input),
        "Get-Content" | "get-content" => bi_get_content(args, input),
        "Select-Object" | "select-object" | "select" => bi_select_object(args, input),
        "Where-Object" | "where-object" => bi_where_object(args, input, env),
        "ForEach-Object" | "foreach-object" | "foreach" => bi_foreach_object(args, input, env),
        "Sort-Object" | "sort-object" => bi_sort_object(args, input),
        "Group-Object" | "group-object" | "group" | "group_by" => bi_group_object(args, input),
        "Measure-Object" | "measure-object" | "measure" => bi_measure_object(args, input),

        // Nushell-style data commands (not in fast lookup)
        "from-json" | "from_json" => bi_from_json(args, input),
        "to-json" | "to_json" => bi_to_json(args, input),
        "from-csv" | "from_csv" => bi_from_csv(args, input),
        "to-csv" | "to_csv" => bi_to_csv(args, input),
        "from-yaml" | "from_yaml" => bi_from_yaml(args, input),
        "to-yaml" | "to_yaml" => bi_to_yaml(args, input),
        "columns" => bi_columns(args, input),
        "describe" => bi_describe(args, input),

        // AI-enhanced commands (not in fast lookup)
        "ai-suggest" | "suggest" => bi_ai_suggest(args, input, env),
        "ai-explain" | "explain" => bi_ai_explain(args, input, env),
        "ai-complete" | "complete" => bi_ai_complete(args, input, env),
        "ai-fix" | "fix" => bi_ai_fix(args, input, env),

        // AI/Agents (not in fast lookup)
        "agent" => bi_agent(args, input, env),
        "swarm" => bi_swarm(args, input, env),

        // AI Backend Detection (not in fast lookup)
        "ai_backends" | "ai-backends" => bi_ai_backends(args, input),
        "ai_detect" | "ai-detect" => bi_ai_detect(args, input),

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

fn expect_string<'a>(name: &str, v: &'a Value) -> Result<&'a str> {
    if let Value::Str(s) = v {
        Ok(s.as_str())
    } else {
        Err(anyhow!("{} requires string input, got {:?}", name, v))
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

    // Save and clear pipe input to prevent it from leaking into lambda evaluation
    let saved_pipe = env.input().cloned();
    env.set_input(None);

    // Save previous bindings
    let mut saved: Vec<(String, Option<Value>)> = Vec::with_capacity(params.len());
    for (i, p) in params.iter().enumerate() {
        let prev = env.get_var(p).cloned();
        saved.push((p.clone(), prev));
        env.set_var_unchecked(p, args[i].clone());
    }

    // Eval and restore
    let result = eval_expr(&lam.body, env);

    for (name, prev) in saved.into_iter().rev() {
        match prev {
            Some(v) => env.set_var_unchecked(&name, v),
            None => env.del_var(&name),
        }
    }

    // Restore pipe input
    match saved_pipe {
        Some(v) => env.set_input(Some(v)),
        None => env.set_input(None),
    }

    result
}

// --------------- General builtins ---------------

fn bi_help() -> Result<Value> {
    let txt = r#"Æther (æ) built-ins:
- help                         # this help
- clear                        # clear screen (prints ANSI)
- echo <...values>             # echo stringified values
- print <value>                # pretty-print value (returns text)

Data / pipelines:
- map    <array> <fn(x)=> expr>             # map over array
- where  <array> <fn(x)=> predicate>        # filter array
- reduce <array> <fn(x,y)=> expr> <init>    # fold array
- take   <array> <n>                        # take first n elements
- first  <array>                            # get first element
- last   <array>                            # get last element
- any    <array> [fn(x)=> pred]             # check if any match
- all    <array> [fn(x)=> pred]             # check if all match
- keys   <record>                           # get record keys
- len    <array|record|string>              # get length
- type_of <value>                           # get type name

String functions:
- split      <string> <delimiter>           # split into array
- join       <array> <delimiter>            # join into string
- trim       <string>                       # remove whitespace
- upper      <string>                       # to uppercase
- lower      <string>                       # to lowercase
- replace    <string> <old> <new>           # replace substring
- contains   <string> <substring>           # check if contains
- starts_with <string> <prefix>             # check prefix
- ends_with  <string> <suffix>              # check suffix

Array functions:
- flatten    <array>                        # flatten nested arrays
- reverse    <array|string>                 # reverse order
- slice      <array|string> <start> [end]   # extract slice
- range      <end>  OR  <start> <end> [step] # generate range
- zip        <array1> <array2>              # zip into pairs
- push       <array> <item>                 # append item
- concat     <array1> <array2>              # concatenate
- sort       <array>                        # sort array
- uniq       <array>                        # unique values

Math functions:
- abs        <number>                       # absolute value
- min        <a> <b>                        # minimum
- max        <a> <b>                        # maximum
- floor      <number>                       # round down
- ceil       <number>                       # round up
- round      <number>                       # round nearest
- sqrt       <number>                       # square root
- pow        <base> <exponent>              # power

Utility functions:
- exit       [code]                         # exit program
- env        <key>                          # get env variable
- set_env    <key> <value>                  # set env variable
- sleep      <seconds>                      # sleep duration
- time       ()                             # current timestamp
- json_parse <json_string>                  # parse JSON
- json_stringify <value>                    # to JSON string

File system:
- ls         [path]                         # list directory
- pwd        ()                             # current directory
- cat        <file>                         # read file
- read_text  <file>                         # read file (alias)
- head       <file> [n]                     # first n lines
- tail       <file> [n]                     # last n lines
- find       <path> [pattern]               # find files
- grep       <file|array> <pattern>         # search lines
- wc         <file|array>                   # count lines

HTTP:
- http_get <url>                # fetch URL → {url,status,headers,body}

AI Backend Detection:
- ai_backends                   # list available AI backends with details
- ai_detect                     # auto-select best available AI backend

MCP Server Integration:
- mcp_servers                   # list available MCP servers and tools
- mcp_detect [endpoint]         # find specific MCP server or first available

Syntax Knowledge Base:
- syntax_get      <id>                      # get syntax entry by ID
- syntax_list     [category]                # list all syntax IDs (optional filter)
- syntax_search   <query>                   # search entries by keyword
- syntax_add      <record>                  # add new syntax entry
- syntax_categories ()                      # list all categories
- ab_encode       <msg_type> <opcode> <payload>  # encode AgenticBinary message
- ab_decode       <bytes>                   # decode AgenticBinary message

AI / Agents (require ai module present):
- agent <goal> [tools...] [max_steps] [dry_run]
- swarm <json-config|record>  OR  <goal> [tools...] [max_steps] [dry_run]

Examples:
  # Pipelines
  [1,2,3] | map(fn(x)=> x*2) | reduce(fn(a,b)=> a+b, 0)
  [5,4,3,2,1] | where(fn(x)=> x>2) | take(2) | print
  
  # String operations
  "hello,world" | split(",") | map(fn(s)=> s | upper()) | join("-")
  
  # Array operations
  range(10) | where(fn(x)=> x % 2 == 0) | print
  [[1,2],[3,4]] | flatten() | reverse()
  
  # Math
  range(1, 11) | map(fn(x)=> pow(x, 2)) | reduce(fn(a,b)=> a+b, 0)
  
  # JSON
  {name: "Alice", age: 30} | json_stringify() | json_parse()
  
AI & MCP Integration:
  model = ai_detect()            # auto-detect AI backend
  server = mcp_detect()          # find MCP server
  agent("task", model, server.tools)  # use both together
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

    // SECURITY FIX (MED-008): Validate URL to prevent SSRF
    let validated_url = validate_http_url(&url).context("http_get: URL validation failed")?;

    // SECURITY FIX (HIGH-005): Use secure HTTP client
    let client = create_secure_http_client().context("Failed to create HTTP client")?;
    let resp = client.get(&validated_url).send()?;
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
    out.insert("url".into(), Value::Str(validated_url));
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
    let (arr_val, n_idx) = if let Some(input_val) = input {
        // SECURITY: Replace .unwrap() with explicit match (CVSS 7.1)
        (input_val, 0usize)
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

fn bi_keys(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if let Some(input_val) = input {
        input_val
    } else if !args.is_empty() {
        args[0].clone()
    } else {
        return Err(anyhow!("keys: requires a Record as input or argument"));
    };

    match val {
        Value::Record(map) => {
            let keys: Vec<Value> = map.keys().map(|k| Value::Str(k.clone())).collect();
            Ok(Value::Array(keys))
        }
        _ => Err(anyhow!("keys: requires a Record, got {:?}", val)),
    }
}

fn bi_len(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if let Some(input_val) = input {
        input_val
    } else if !args.is_empty() {
        args[0].clone()
    } else {
        return Err(anyhow!("len: requires input or argument"));
    };

    let length = match val {
        Value::Array(ref v) => v.len(),
        Value::Record(ref m) => m.len(),
        Value::Str(ref s) => s.len(),
        _ => {
            return Err(anyhow!(
                "len: requires Array, Record, or String, got {:?}",
                val
            ));
        }
    };

    Ok(Value::Int(length as i64))
}

fn bi_type_of(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if let Some(input_val) = input {
        input_val
    } else if !args.is_empty() {
        args[0].clone()
    } else {
        return Err(anyhow!("type_of: requires input or argument"));
    };

    let type_str = match val {
        Value::Null => "Null",
        Value::Bool(_) => "Bool",
        Value::Int(_) => "Int",
        Value::Float(_) => "Float",
        Value::Str(_) => "String",
        Value::Uri(_) => "Uri",
        Value::Array(_) => "Array",
        Value::Record(_) => "Record",
        Value::Table(_) => "Table",
        Value::Lambda(_) => "Lambda",
    };

    Ok(Value::Str(type_str.to_string()))
}

// --------------- AI / Agents wrappers (optional) ---------------

fn bi_agent(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    // SECURITY: Rate limit agent calls (CVSS 7.8)
    check_rate_limit("bi_agent", 10, Duration::from_secs(60))
        .context("Agent rate limit exceeded")?;

    // Accept either: agent "<goal>" [tools...] [max_steps] [dry_run]
    // or a record config: {goal:"...", tools:["..."], max_steps:3, dry_run:true, model_uri:"..."}
    if let Some(Value::Record(cfg)) = input {
        return agent_from_record(cfg, env);
    }
    if let Some(Value::Record(cfg)) = args.get(0) {
        return agent_from_record(cfg.clone(), env);
    }
    // positional
    let goal_str = match args.get(0) {
        Some(Value::Str(s)) => s.clone(),
        Some(other) => return Err(anyhow!("agent: expected String goal, got {:?}", other)),
        None => return Err(anyhow!("agent requires goal")),
    };

    // SECURITY: Validate AI prompt for injection (CVSS 7.8)
    let goal = validate_ai_prompt(&goal_str).context("agent: goal validation failed")?;
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

// --------------- File system commands ---------------

fn bi_ls(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let path_str = if args.is_empty() {
        ".".to_string()
    } else {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("ls: path must be a string")),
        }
    };

    // SECURITY: Validate path to prevent traversal attacks (CVSS 8.2)
    let validated_path = validate_read_path(&path_str).context("ls: path validation failed")?;

    let entries = fs::read_dir(&validated_path)
        .with_context(|| format!("ls: failed to read directory: {:?}", validated_path))?;
    let mut files = Vec::new();

    for entry in entries {
        let entry = entry.with_context(|| "ls: failed to read directory entry")?;
        let metadata = entry
            .metadata()
            .with_context(|| "ls: failed to read file metadata")?;
        let path = entry.path();
        let name = path
            .file_name()
            .ok_or_else(|| anyhow!("ls: invalid filename"))?
            .to_string_lossy()
            .to_string();

        let mut record = BTreeMap::new();
        record.insert("name".to_string(), Value::Str(name));
        record.insert(
            "path".to_string(),
            Value::Str(path.to_string_lossy().to_string()),
        );
        record.insert("is_dir".to_string(), Value::Bool(metadata.is_dir()));
        record.insert("size".to_string(), Value::Int(metadata.len() as i64));

        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                record.insert(
                    "modified".to_string(),
                    Value::Int(duration.as_secs() as i64),
                );
            }
        }

        files.push(Value::Record(record));
    }

    Ok(Value::Array(files))
}

fn bi_pwd() -> Result<Value> {
    let current_dir = std::env::current_dir()?;
    Ok(Value::Str(current_dir.to_string_lossy().to_string()))
}

fn bi_cat(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if let Some(input) = input {
        return Ok(input);
    }

    if args.is_empty() {
        return Err(anyhow!("cat: no file specified"));
    }

    let path_str = match &args[0] {
        Value::Str(s) => s,
        _ => return Err(anyhow!("cat: path must be a string")),
    };

    // SECURITY: Validate path to prevent traversal attacks (CVSS 8.2)
    let validated_path = validate_read_path(path_str).context("cat: path validation failed")?;

    // SECURITY FIX (MED-001): Check file size before reading
    let metadata = fs::metadata(&validated_path)
        .with_context(|| format!("cat: failed to read file metadata: {:?}", validated_path))?;
    check_file_size_limit(metadata.len()).context("cat: file too large")?;

    let content = fs::read_to_string(&validated_path)
        .with_context(|| format!("cat: failed to read file: {:?}", validated_path))?;
    Ok(Value::Str(content))
}

fn bi_read_text(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("read_text: no file specified"));
    }

    let path_str = match &args[0] {
        Value::Str(s) => s,
        _ => return Err(anyhow!("read_text: path must be a string")),
    };

    // SECURITY: Validate path to prevent traversal attacks
    let validated_path =
        validate_read_path(path_str).context("read_text: path validation failed")?;

    // SECURITY FIX (MED-001): Check file size before reading
    let metadata = fs::metadata(&validated_path).with_context(|| {
        format!(
            "read_text: failed to read file metadata: {:?}",
            validated_path
        )
    })?;
    check_file_size_limit(metadata.len()).context("read_text: file too large")?;

    let content = fs::read_to_string(&validated_path)
        .with_context(|| format!("read_text: failed to read file: {:?}", validated_path))?;

    Ok(Value::Str(content))
}

fn bi_head(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    // Determine line count and content source
    let (lines_count, content) = if let Some(input) = input {
        // Input from pipeline
        let count = if !args.is_empty() {
            match &args[0] {
                Value::Int(n) => *n as usize,
                _ => 10,
            }
        } else {
            10
        };
        let content = match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("head: input must be a string")),
        };
        (count, content)
    } else {
        // No pipeline input, read from file
        match args.len() {
            0 => return Err(anyhow!("head: no input provided")),
            1 => {
                // Single argument: could be file path or line count
                match &args[0] {
                    Value::Str(path_str) => {
                        // SECURITY: Validate path to prevent traversal attacks (CVSS 8.2)
                        let validated_path =
                            validate_read_path(path_str).context("head: path validation failed")?;
                        let content = fs::read_to_string(&validated_path).with_context(|| {
                            format!("head: failed to read file: {:?}", validated_path)
                        })?;
                        (10, content)
                    }
                    Value::Int(_) => return Err(anyhow!("head: no file specified")),
                    _ => return Err(anyhow!("head: invalid argument")),
                }
            }
            2 => {
                // Two arguments: could be (file, count) or (count, file)
                // Try file first, then count
                match (&args[0], &args[1]) {
                    (Value::Str(path_str), Value::Int(n)) => {
                        // SECURITY: Validate path to prevent traversal attacks
                        let validated_path =
                            validate_read_path(path_str).context("head: path validation failed")?;
                        let content = fs::read_to_string(&validated_path).with_context(|| {
                            format!("head: failed to read file: {:?}", validated_path)
                        })?;
                        (*n as usize, content)
                    }
                    (Value::Int(n), Value::Str(path_str)) => {
                        // SECURITY: Validate path to prevent traversal attacks
                        let validated_path =
                            validate_read_path(path_str).context("head: path validation failed")?;
                        let content = fs::read_to_string(&validated_path).with_context(|| {
                            format!("head: failed to read file: {:?}", validated_path)
                        })?;
                        (*n as usize, content)
                    }
                    _ => {
                        return Err(anyhow!(
                            "head: invalid arguments - need file path and line count"
                        ));
                    }
                }
            }
            _ => return Err(anyhow!("head: too many arguments")),
        }
    };

    let lines: Vec<&str> = content.lines().take(lines_count).collect();
    Ok(Value::Str(lines.join("\n")))
}

fn bi_tail(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    // Determine line count and content source
    let (lines_count, content) = if let Some(input) = input {
        // Input from pipeline
        let count = if !args.is_empty() {
            match &args[0] {
                Value::Int(n) => *n as usize,
                _ => 10,
            }
        } else {
            10
        };
        let content = match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("tail: input must be a string")),
        };
        (count, content)
    } else {
        // No pipeline input, read from file
        match args.len() {
            0 => return Err(anyhow!("tail: no input provided")),
            1 => {
                // Single argument: could be file path or line count
                match &args[0] {
                    Value::Str(path_str) => {
                        // SECURITY: Validate path to prevent traversal attacks (CVSS 8.2)
                        let validated_path =
                            validate_read_path(path_str).context("tail: path validation failed")?;
                        let content = fs::read_to_string(&validated_path).with_context(|| {
                            format!("tail: failed to read file: {:?}", validated_path)
                        })?;
                        (10, content)
                    }
                    Value::Int(_) => return Err(anyhow!("tail: no file specified")),
                    _ => return Err(anyhow!("tail: invalid argument")),
                }
            }
            2 => {
                // Two arguments: could be (file, count) or (count, file)
                match (&args[0], &args[1]) {
                    (Value::Str(path_str), Value::Int(n)) => {
                        // SECURITY: Validate path to prevent traversal attacks
                        let validated_path =
                            validate_read_path(path_str).context("tail: path validation failed")?;
                        let content = fs::read_to_string(&validated_path).with_context(|| {
                            format!("tail: failed to read file: {:?}", validated_path)
                        })?;
                        (*n as usize, content)
                    }
                    (Value::Int(n), Value::Str(path_str)) => {
                        // SECURITY: Validate path to prevent traversal attacks
                        let validated_path =
                            validate_read_path(path_str).context("tail: path validation failed")?;
                        let content = fs::read_to_string(&validated_path).with_context(|| {
                            format!("tail: failed to read file: {:?}", validated_path)
                        })?;
                        (*n as usize, content)
                    }
                    _ => {
                        return Err(anyhow!(
                            "tail: invalid arguments - need file path and line count"
                        ));
                    }
                }
            }
            _ => return Err(anyhow!("tail: too many arguments")),
        }
    };

    let lines: Vec<&str> = content.lines().collect();
    let start = if lines.len() > lines_count {
        lines.len() - lines_count
    } else {
        0
    };
    let tail_lines = &lines[start..];
    Ok(Value::Str(tail_lines.join("\n")))
}

fn bi_find(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let (path_str, pattern) = if args.len() >= 2 {
        match (&args[0], &args[1]) {
            (Value::Str(p), Value::Str(pat)) => (p.clone(), Some(pat.clone())),
            _ => return Err(anyhow!("find: path and pattern must be strings")),
        }
    } else if args.len() == 1 {
        match &args[0] {
            Value::Str(p) => (p.clone(), None),
            _ => return Err(anyhow!("find: path must be a string")),
        }
    } else {
        (".".to_string(), None)
    };

    // SECURITY: Validate path to prevent traversal attacks (CVSS 8.2)
    let validated_path = validate_read_path(&path_str).context("find: path validation failed")?;

    let mut results = Vec::new();

    for entry in WalkDir::new(&validated_path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path_str = entry.path().to_string_lossy().to_string();

        if let Some(ref pattern) = pattern {
            // Simple glob matching: support * for any characters
            let filename = entry.file_name().to_string_lossy();
            if glob_match(pattern, &filename) {
                results.push(Value::Str(path_str));
            }
        } else {
            results.push(Value::Str(path_str));
        }
    }

    Ok(Value::Array(results))
}

// Simple glob matcher for basic patterns like *.ext
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern.starts_with("*.") {
        let ext = &pattern[2..];
        return text.ends_with(&format!(".{}", ext));
    }

    if pattern.ends_with("*") {
        let prefix = &pattern[..pattern.len() - 1];
        return text.starts_with(prefix);
    }

    // Exact match if no wildcards
    pattern == text
}

fn bi_sort(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let reverse = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-r" || s == "--reverse"));

    let array = if let Some(input) = input {
        match input {
            Value::Array(arr) => arr,
            Value::Str(s) => {
                let lines: Vec<Value> =
                    s.lines().map(|line| Value::Str(line.to_string())).collect();
                lines
            }
            _ => return Err(anyhow!("sort: input must be an array or string")),
        }
    } else {
        return Err(anyhow!("sort: no input provided"));
    };

    let mut sorted = array;
    sorted.sort_by(|a, b| {
        let cmp = match (a, b) {
            (Value::Str(s1), Value::Str(s2)) => s1.cmp(s2),
            (Value::Int(i1), Value::Int(i2)) => i1.cmp(i2),
            (Value::Float(f1), Value::Float(f2)) => {
                f1.partial_cmp(f2).unwrap_or(std::cmp::Ordering::Equal)
            }
            _ => std::cmp::Ordering::Equal,
        };
        if reverse {
            cmp.reverse()
        } else {
            cmp
        }
    });

    Ok(Value::Array(sorted))
}

fn bi_uniq(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let _count = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-c" || s == "--count"));

    let array = if let Some(input) = input {
        match input {
            Value::Array(arr) => arr,
            Value::Str(s) => {
                let lines: Vec<Value> =
                    s.lines().map(|line| Value::Str(line.to_string())).collect();
                lines
            }
            _ => return Err(anyhow!("uniq: input must be an array or string")),
        }
    } else {
        return Err(anyhow!("uniq: no input provided"));
    };

    let mut unique = Vec::new();
    let mut last: Option<&Value> = None;

    for item in &array {
        if last.map_or(true, |l| l != item) {
            unique.push(item.clone());
            last = Some(item);
        }
    }

    Ok(Value::Array(unique))
}

fn bi_wc(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let lines_only = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-l" || s == "--lines"));
    let words_only = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-w" || s == "--words"));
    let chars_only = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-c" || s == "--chars"));

    let content = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("wc: input must be a string")),
        }
    } else if !args.is_empty() {
        let path = match &args[0] {
            Value::Str(s) if !s.starts_with('-') => s,
            _ => return Err(anyhow!("wc: no input provided")),
        };
        fs::read_to_string(path)?
    } else {
        return Err(anyhow!("wc: no input provided"));
    };

    let line_count = content.lines().count();
    let word_count = content.split_whitespace().count();
    let char_count = content.chars().count();

    let mut result = BTreeMap::new();

    if lines_only {
        result.insert("lines".to_string(), Value::Int(line_count as i64));
    } else if words_only {
        result.insert("words".to_string(), Value::Int(word_count as i64));
    } else if chars_only {
        result.insert("chars".to_string(), Value::Int(char_count as i64));
    } else {
        result.insert("lines".to_string(), Value::Int(line_count as i64));
        result.insert("words".to_string(), Value::Int(word_count as i64));
        result.insert("chars".to_string(), Value::Int(char_count as i64));
    }

    Ok(Value::Record(result))
}

fn bi_grep(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("grep: pattern required"));
    }

    let pattern = match &args[0] {
        Value::Str(s) => s,
        _ => return Err(anyhow!("grep: pattern must be a string")),
    };

    let case_insensitive = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-i" || s == "--ignore-case"));
    let invert = args
        .iter()
        .any(|arg| matches!(arg, Value::Str(s) if s == "-v" || s == "--invert-match"));

    let content = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            Value::Array(arr) => {
                let lines: Vec<String> = arr
                    .iter()
                    .filter_map(|v| {
                        if let Value::Str(s) = v {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                lines.join("\n")
            }
            _ => return Err(anyhow!("grep: input must be a string or array")),
        }
    } else if args.len() > 1 {
        let path = match &args[1] {
            Value::Str(s) if !s.starts_with('-') => s,
            _ => return Err(anyhow!("grep: no input provided")),
        };
        fs::read_to_string(path)?
    } else {
        return Err(anyhow!("grep: no input provided"));
    };

    let matching_lines: Vec<Value> = content
        .lines()
        .filter(|line| {
            let matches = if case_insensitive {
                line.to_lowercase().contains(&pattern.to_lowercase())
            } else {
                line.contains(pattern)
            };
            if invert {
                !matches
            } else {
                matches
            }
        })
        .map(|line| Value::Str(line.to_string()))
        .collect();

    Ok(Value::Array(matching_lines))
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

// Helper function to convert Value to string representation
fn value_to_string(value: &Value) -> String {
    match value {
        Value::Str(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Uri(s) => s.clone(),
        Value::Array(a) => format!("[len={}]", a.len()),
        Value::Record(_) => "{…}".into(),
        Value::Table(t) => format!("<Table rows={}>", t.rows.len()),
        Value::Lambda(_) => "<lambda>".into(),
        Value::Null => "<null>".into(),
    }
}

// =============== PowerShell-style Parameter Binding ===============

/// PowerShell-style parameter binding result
#[derive(Debug)]
pub struct BoundParameters {
    pub named: BTreeMap<String, Value>,
    pub positional: Vec<Value>,
    pub pipeline_input: Option<Value>,
}

/// Bind parameters according to PowerShell conventions
fn bind_parameters(
    parameter_set: &ParameterSet,
    args: Vec<Value>,
    input: Option<Value>,
) -> Result<BoundParameters> {
    let mut bound = BoundParameters {
        named: BTreeMap::new(),
        positional: Vec::new(),
        pipeline_input: input,
    };

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        // Check if this is a named parameter (starts with -)
        if let Value::Str(s) = arg {
            if s.starts_with('-') {
                let param_name = s.trim_start_matches('-');

                // Find matching parameter definition
                if let Some(param) = parameter_set
                    .parameters
                    .iter()
                    .find(|p| p.name == param_name || p.aliases.contains(&param_name.to_string()))
                {
                    match param.parameter_type {
                        ParameterType::Switch => {
                            // Switch parameters don't take values
                            bound.named.insert(param.name.clone(), Value::Bool(true));
                            i += 1;
                        }
                        _ => {
                            // Other parameters take the next argument as value
                            if i + 1 < args.len() {
                                let value =
                                    validate_parameter_type(&args[i + 1], &param.parameter_type)?;
                                bound.named.insert(param.name.clone(), value);
                                i += 2;
                            } else {
                                return Err(anyhow!("Parameter '{}' requires a value", param.name));
                            }
                        }
                    }
                } else {
                    return Err(anyhow!("Unknown parameter: {}", param_name));
                }
            } else {
                // Positional parameter
                bound.positional.push(arg.clone());
                i += 1;
            }
        } else {
            // Positional parameter
            bound.positional.push(arg.clone());
            i += 1;
        }
    }

    // Validate required parameters
    for param in &parameter_set.parameters {
        if param.required && !bound.named.contains_key(&param.name) {
            // Check if it's satisfied by positional parameter
            if let Some(pos) = param.position {
                if (pos as usize) >= bound.positional.len() {
                    return Err(anyhow!("Required parameter '{}' not provided", param.name));
                }
            } else {
                return Err(anyhow!("Required parameter '{}' not provided", param.name));
            }
        }
    }

    Ok(bound)
}

/// Validate that a value matches the expected parameter type
fn validate_parameter_type(value: &Value, param_type: &ParameterType) -> Result<Value> {
    match (value, param_type) {
        (Value::Str(_), ParameterType::String) => Ok(value.clone()),
        (Value::Int(_), ParameterType::Int) => Ok(value.clone()),
        (Value::Float(_), ParameterType::Float) => Ok(value.clone()),
        (Value::Bool(_), ParameterType::Bool) => Ok(value.clone()),
        (Value::Array(_), ParameterType::Array) => Ok(value.clone()),
        (Value::Record(_), ParameterType::Record) => Ok(value.clone()),

        // Type coercion
        (Value::Str(s), ParameterType::Int) => s
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| anyhow!("Cannot convert '{}' to integer", s)),
        (Value::Str(s), ParameterType::Float) => s
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| anyhow!("Cannot convert '{}' to float", s)),
        (Value::Str(s), ParameterType::Bool) => match s.to_lowercase().as_str() {
            "true" | "yes" | "1" => Ok(Value::Bool(true)),
            "false" | "no" | "0" => Ok(Value::Bool(false)),
            _ => Err(anyhow!("Cannot convert '{}' to boolean", s)),
        },
        (Value::Int(i), ParameterType::Float) => Ok(Value::Float(*i as f64)),

        _ => Err(anyhow!(
            "Parameter type mismatch: expected {:?}, got {:?}",
            param_type,
            value
        )),
    }
}

// =============== PowerShell-style Cmdlets ===============

/// Get-Files: PowerShell-style file listing with rich metadata and proper parameter binding
fn bi_get_files(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    // Define parameter set
    let parameter_set = ParameterSet {
        name: "Get-Files".to_string(),
        parameters: vec![
            Parameter {
                name: "Path".to_string(),
                aliases: vec!["p".to_string()],
                parameter_type: ParameterType::String,
                required: false,
                position: Some(0),
                help_text: "The path to list files from".to_string(),
            },
            Parameter {
                name: "Recurse".to_string(),
                aliases: vec!["r".to_string()],
                parameter_type: ParameterType::Switch,
                required: false,
                position: None,
                help_text: "Recursively list files in subdirectories".to_string(),
            },
            Parameter {
                name: "Filter".to_string(),
                aliases: vec!["f".to_string()],
                parameter_type: ParameterType::String,
                required: false,
                position: None,
                help_text: "Filter files by pattern".to_string(),
            },
        ],
        pipeline_input: Some(PipelineInputType::ByValue(ParameterType::String)),
    };

    // Bind parameters
    let bound = bind_parameters(&parameter_set, args, input)?;

    // Extract parameters
    let path = if let Some(pipeline_path) = &bound.pipeline_input {
        match pipeline_path {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("Get-Files: pipeline input must be a path string")),
        }
    } else if let Some(path_arg) = bound.named.get("Path") {
        match path_arg {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("Get-Files: Path parameter must be a string")),
        }
    } else if !bound.positional.is_empty() {
        match &bound.positional[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("Get-Files: path must be a string")),
        }
    } else {
        ".".to_string()
    };

    let recurse = bound.named.get("Recurse").is_some();
    let filter = bound.named.get("Filter").and_then(|v| {
        if let Value::Str(s) = v {
            Some(s.clone())
        } else {
            None
        }
    });

    let mut files = Vec::new();

    if recurse {
        // Recursive directory traversal
        for entry in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
            if let Ok(metadata) = entry.metadata() {
                let path_str = entry.path().to_string_lossy().to_string();
                let name = entry.file_name().to_string_lossy().to_string();

                // Apply filter if specified
                if let Some(ref pattern) = filter {
                    if !name.contains(pattern) {
                        continue;
                    }
                }

                let mut file_obj = BTreeMap::new();
                file_obj.insert("Name".to_string(), Value::Str(name));
                file_obj.insert("FullName".to_string(), Value::Str(path_str));
                file_obj.insert("IsDirectory".to_string(), Value::Bool(metadata.is_dir()));
                file_obj.insert("Length".to_string(), Value::Int(metadata.len() as i64));

                if let Ok(modified) = metadata.modified() {
                    if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        file_obj.insert(
                            "LastWriteTime".to_string(),
                            Value::Int(duration.as_secs() as i64),
                        );
                    }
                }

                files.push(Value::Record(file_obj));
            }
        }
    } else {
        // Non-recursive directory listing
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let path_str = entry.path().to_string_lossy().to_string();
            let name = entry.file_name().to_string_lossy().to_string();

            // Apply filter if specified
            if let Some(ref pattern) = filter {
                if !name.contains(pattern) {
                    continue;
                }
            }

            let mut file_obj = BTreeMap::new();
            file_obj.insert("Name".to_string(), Value::Str(name));
            file_obj.insert("FullName".to_string(), Value::Str(path_str));
            file_obj.insert("IsDirectory".to_string(), Value::Bool(metadata.is_dir()));
            file_obj.insert("Length".to_string(), Value::Int(metadata.len() as i64));

            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                    file_obj.insert(
                        "LastWriteTime".to_string(),
                        Value::Int(duration.as_secs() as i64),
                    );
                }
            }

            files.push(Value::Record(file_obj));
        }
    }

    Ok(Value::Array(files))
}

/// Get-Content: PowerShell-style file content reading
fn bi_get_content(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let path = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("Get-Content: input must be a path string")),
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("Get-Content: path must be a string")),
        }
    } else {
        return Err(anyhow!("Get-Content: no path provided"));
    };

    let content = fs::read_to_string(&path)?;

    // Return as array of lines (PowerShell style)
    let lines: Vec<Value> = content
        .lines()
        .map(|line| Value::Str(line.to_string()))
        .collect();

    Ok(Value::Array(lines))
}

/// Select-Object: PowerShell-style property selection
fn bi_select_object(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("Select-Object: requires pipeline input"))?;

    if args.is_empty() {
        return Ok(input);
    }

    match input {
        Value::Array(arr) => {
            let mut results = Vec::new();
            for item in arr {
                results.push(select_properties(&item, &args)?);
            }
            Ok(Value::Array(results))
        }
        _ => Ok(select_properties(&input, &args)?),
    }
}

fn select_properties(value: &Value, properties: &[Value]) -> Result<Value> {
    match value {
        Value::Record(record) => {
            let mut selected = BTreeMap::new();
            for prop in properties {
                if let Value::Str(prop_name) = prop {
                    if let Some(val) = record.get(prop_name) {
                        selected.insert(prop_name.clone(), val.clone());
                    }
                }
            }
            Ok(Value::Record(selected))
        }
        _ => Ok(value.clone()),
    }
}

/// Where-Object: PowerShell-style filtering with enhanced syntax
fn bi_where_object(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("Where-Object: requires pipeline input"))?;

    if args.is_empty() {
        return Err(anyhow!("Where-Object: requires filter condition"));
    }

    // Support both lambda and property-based filtering
    match &args[0] {
        Value::Lambda(_lambda) => {
            // Use existing where logic
            bi_where(args, Some(input), env)
        }
        Value::Str(property) => {
            // PowerShell-style property filtering: where Status -eq "Running"
            if args.len() >= 3 {
                filter_by_property(&input, property, &args[1], &args[2])
            } else {
                Err(anyhow!(
                    "Where-Object: property filtering requires property, operator, and value"
                ))
            }
        }
        _ => Err(anyhow!("Where-Object: invalid filter condition")),
    }
}

fn filter_by_property(
    input: &Value,
    property: &str,
    operator: &Value,
    target: &Value,
) -> Result<Value> {
    let op_str = match operator {
        Value::Str(s) => s.as_str(),
        _ => return Err(anyhow!("Where-Object: operator must be a string")),
    };

    match input {
        Value::Array(arr) => {
            let mut results = Vec::new();
            for item in arr {
                if let Value::Record(record) = item {
                    if let Some(value) = record.get(property) {
                        let matches = match op_str {
                            "-eq" | "==" => value == target,
                            "-ne" | "!=" => value != target,
                            "-gt" | ">" => {
                                compare_values(value, target, std::cmp::Ordering::Greater)
                            }
                            "-lt" | "<" => compare_values(value, target, std::cmp::Ordering::Less),
                            "-ge" | ">=" => {
                                compare_values(value, target, std::cmp::Ordering::Greater)
                                    || value == target
                            }
                            "-le" | "<=" => {
                                compare_values(value, target, std::cmp::Ordering::Less)
                                    || value == target
                            }
                            "-like" => match (value, target) {
                                (Value::Str(v), Value::Str(t)) => v.contains(t),
                                _ => false,
                            },
                            _ => {
                                return Err(anyhow!(
                                    "Where-Object: unsupported operator '{}'",
                                    op_str
                                ));
                            }
                        };

                        if matches {
                            results.push(item.clone());
                        }
                    }
                }
            }
            Ok(Value::Array(results))
        }
        _ => Ok(input.clone()),
    }
}

fn compare_values(a: &Value, b: &Value, expected: std::cmp::Ordering) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y) == expected,
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y) == Some(expected),
        (Value::Int(x), Value::Float(y)) => (*x as f64).partial_cmp(y) == Some(expected),
        (Value::Float(x), Value::Int(y)) => x.partial_cmp(&(*y as f64)) == Some(expected),
        (Value::Str(x), Value::Str(y)) => x.cmp(y) == expected,
        _ => false,
    }
}

/// ForEach-Object: PowerShell-style iteration
fn bi_foreach_object(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    // This is essentially the same as map, but with PowerShell naming
    bi_map(args, input, env)
}

/// Sort-Object: PowerShell-style sorting with property support
fn bi_sort_object(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("Sort-Object: requires pipeline input"))?;

    match input {
        Value::Array(mut arr) => {
            if args.is_empty() {
                // Sort by value
                arr.sort_by(|a, b| compare_values_for_sort(a, b));
            } else if let Value::Str(property) = &args[0] {
                // Sort by property
                arr.sort_by(|a, b| {
                    let a_val = get_property_value(a, property);
                    let b_val = get_property_value(b, property);
                    compare_values_for_sort(&a_val, &b_val)
                });
            }
            Ok(Value::Array(arr))
        }
        _ => Ok(input),
    }
}

fn get_property_value(value: &Value, property: &str) -> Value {
    match value {
        Value::Record(record) => record
            .get(property)
            .cloned()
            .unwrap_or(Value::Str("".to_string())),
        _ => value.clone(),
    }
}

fn compare_values_for_sort(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Int(x), Value::Float(y)) => (*x as f64)
            .partial_cmp(y)
            .unwrap_or(std::cmp::Ordering::Equal),
        (Value::Float(x), Value::Int(y)) => x
            .partial_cmp(&(*y as f64))
            .unwrap_or(std::cmp::Ordering::Equal),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => std::cmp::Ordering::Equal,
    }
}

/// Group-Object: PowerShell-style grouping
fn bi_group_object(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("Group-Object: requires pipeline input"))?;

    if args.is_empty() {
        return Err(anyhow!("Group-Object: requires property name"));
    }

    let property = match &args[0] {
        Value::Str(s) => s,
        _ => return Err(anyhow!("Group-Object: property name must be a string")),
    };

    match input {
        Value::Array(arr) => {
            let mut groups = BTreeMap::new();

            for item in arr {
                let key = get_property_value(&item, property);
                let key_str = value_to_string(&key);
                groups.entry(key_str).or_insert_with(Vec::new).push(item);
            }

            let mut result = Vec::new();
            for (key, items) in groups {
                let mut group = BTreeMap::new();
                group.insert("Name".to_string(), Value::Str(key));
                group.insert("Count".to_string(), Value::Int(items.len() as i64));
                group.insert("Group".to_string(), Value::Array(items));
                result.push(Value::Record(group));
            }

            Ok(Value::Array(result))
        }
        _ => Err(anyhow!("Group-Object: input must be an array")),
    }
}

/// Measure-Object: PowerShell-style measurement and statistics
fn bi_measure_object(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("Measure-Object: requires pipeline input"))?;

    match input {
        Value::Array(arr) => {
            let mut result = BTreeMap::new();
            result.insert("Count".to_string(), Value::Int(arr.len() as i64));

            // If property specified, calculate statistics
            if !args.is_empty() {
                if let Value::Str(property) = &args[0] {
                    let values: Vec<f64> = arr
                        .iter()
                        .filter_map(|item| {
                            let prop_val = get_property_value(item, property);
                            match prop_val {
                                Value::Int(i) => Some(i as f64),
                                Value::Float(f) => Some(f),
                                _ => None,
                            }
                        })
                        .collect();

                    if !values.is_empty() {
                        let sum: f64 = values.iter().sum();
                        let avg = sum / values.len() as f64;
                        let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
                        let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));

                        result.insert("Sum".to_string(), Value::Float(sum));
                        result.insert("Average".to_string(), Value::Float(avg));
                        result.insert("Minimum".to_string(), Value::Float(min));
                        result.insert("Maximum".to_string(), Value::Float(max));
                    }
                }
            }

            Ok(Value::Record(result))
        }
        _ => {
            let mut result = BTreeMap::new();
            result.insert("Count".to_string(), Value::Int(1));
            Ok(Value::Record(result))
        }
    }
}

// =============== Nushell-style Data Commands ===============

/// from-json: Parse JSON into structured data
fn bi_from_json(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let json_str = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("from-json: input must be a JSON string")),
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("from-json: argument must be a JSON string")),
        }
    } else {
        return Err(anyhow!("from-json: no JSON input provided"));
    };

    let parsed: serde_json::Value = serde_json::from_str(&json_str)?;
    Ok(json_to_value(parsed))
}

/// to-json: Convert structured data to JSON
fn bi_to_json(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let value = input.ok_or_else(|| anyhow!("to-json: requires pipeline input"))?;

    let pretty = args
        .get(0)
        .and_then(|arg| match arg {
            Value::Bool(b) => Some(*b),
            Value::Str(s) => Some(s == "pretty" || s == "true"),
            _ => None,
        })
        .unwrap_or(false);

    let json_val = value_to_json(value);
    let json_str = if pretty {
        serde_json::to_string_pretty(&json_val)?
    } else {
        serde_json::to_string(&json_val)?
    };

    Ok(Value::Str(json_str))
}

fn json_to_value(json: serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Str("null".to_string()),
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Str(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::Str(s),
        serde_json::Value::Array(arr) => Value::Array(arr.into_iter().map(json_to_value).collect()),
        serde_json::Value::Object(obj) => {
            let mut map = BTreeMap::new();
            for (k, v) in obj {
                map.insert(k, json_to_value(v));
            }
            Value::Record(map)
        }
    }
}

fn value_to_json(value: Value) -> serde_json::Value {
    match value {
        Value::Str(s) => serde_json::Value::String(s),
        Value::Int(i) => serde_json::Value::Number(serde_json::Number::from(i)),
        Value::Float(f) => serde_json::Value::Number(
            serde_json::Number::from_f64(f).unwrap_or_else(|| serde_json::Number::from(0)),
        ),
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::Array(arr) => serde_json::Value::Array(arr.into_iter().map(value_to_json).collect()),
        Value::Record(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k, value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::String(value_to_string(&value)),
    }
}

/// from-csv: Parse CSV into structured data
fn bi_from_csv(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let csv_str = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("from-csv: input must be a CSV string")),
        }
    } else {
        return Err(anyhow!("from-csv: requires CSV input"));
    };

    let mut reader = csv::Reader::from_reader(csv_str.as_bytes());
    let headers: Vec<String> = reader.headers()?.iter().map(|h| h.to_string()).collect();

    let mut results = Vec::new();
    for record in reader.records() {
        let record = record?;
        let mut row = BTreeMap::new();

        for (i, field) in record.iter().enumerate() {
            if let Some(header) = headers.get(i) {
                // Try to parse as number, otherwise keep as string
                let value = if let Ok(int_val) = field.parse::<i64>() {
                    Value::Int(int_val)
                } else if let Ok(float_val) = field.parse::<f64>() {
                    Value::Float(float_val)
                } else if let Ok(bool_val) = field.parse::<bool>() {
                    Value::Bool(bool_val)
                } else {
                    Value::Str(field.to_string())
                };
                row.insert(header.clone(), value);
            }
        }
        results.push(Value::Record(row));
    }

    Ok(Value::Array(results))
}

/// to-csv: Convert structured data to CSV
fn bi_to_csv(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("to-csv: requires pipeline input"))?;

    match input {
        Value::Array(arr) => {
            if arr.is_empty() {
                return Ok(Value::Str(String::new()));
            }

            // Get headers from first record
            let headers = if let Value::Record(first_record) = &arr[0] {
                first_record.keys().cloned().collect::<Vec<_>>()
            } else {
                return Err(anyhow!("to-csv: input must be array of records"));
            };

            let mut csv_output = String::new();

            // Write headers
            csv_output.push_str(&headers.join(","));
            csv_output.push('\n');

            // Write data rows
            for item in arr {
                if let Value::Record(record) = item {
                    let row: Vec<String> = headers
                        .iter()
                        .map(|header| {
                            record
                                .get(header)
                                .map(|v| csv_escape(&value_to_string(v)))
                                .unwrap_or_default()
                        })
                        .collect();
                    csv_output.push_str(&row.join(","));
                    csv_output.push('\n');
                }
            }

            Ok(Value::Str(csv_output))
        }
        _ => Err(anyhow!("to-csv: input must be an array of records")),
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// from-yaml: Parse YAML into structured data (simplified)
fn bi_from_yaml(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    // Simplified YAML parsing - in a real implementation, use serde_yaml
    let yaml_str = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("from-yaml: input must be a YAML string")),
        }
    } else {
        return Err(anyhow!("from-yaml: requires YAML input"));
    };

    // For now, treat simple key-value pairs
    let mut result = BTreeMap::new();
    for line in yaml_str.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim();

            let parsed_value = if let Ok(int_val) = value.parse::<i64>() {
                Value::Int(int_val)
            } else if let Ok(float_val) = value.parse::<f64>() {
                Value::Float(float_val)
            } else if let Ok(bool_val) = value.parse::<bool>() {
                Value::Bool(bool_val)
            } else {
                Value::Str(value.to_string())
            };

            result.insert(key, parsed_value);
        }
    }

    Ok(Value::Record(result))
}

/// to-yaml: Convert structured data to YAML (simplified)
fn bi_to_yaml(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("to-yaml: requires pipeline input"))?;

    match input {
        Value::Record(record) => {
            let mut yaml_output = String::new();
            for (key, value) in record {
                yaml_output.push_str(&format!("{}: {}\n", key, value_to_string(&value)));
            }
            Ok(Value::Str(yaml_output))
        }
        _ => Ok(Value::Str(format!("value: {}\n", value_to_string(&input)))),
    }
}

/// columns: Get column names from structured data
fn bi_columns(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("columns: requires pipeline input"))?;

    match input {
        Value::Array(arr) => {
            if let Some(Value::Record(first)) = arr.first() {
                let columns: Vec<Value> = first.keys().map(|k| Value::Str(k.clone())).collect();
                Ok(Value::Array(columns))
            } else {
                Ok(Value::Array(Vec::new()))
            }
        }
        Value::Record(record) => {
            let columns: Vec<Value> = record.keys().map(|k| Value::Str(k.clone())).collect();
            Ok(Value::Array(columns))
        }
        _ => Ok(Value::Array(Vec::new())),
    }
}

/// describe: Get type and structure information about data
fn bi_describe(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input = input.ok_or_else(|| anyhow!("describe: requires pipeline input"))?;

    let mut description = BTreeMap::new();

    match &input {
        Value::Str(s) => {
            description.insert("type".to_string(), Value::Str("string".to_string()));
            description.insert("length".to_string(), Value::Int(s.len() as i64));
        }
        Value::Int(_) => {
            description.insert("type".to_string(), Value::Str("integer".to_string()));
        }
        Value::Float(_) => {
            description.insert("type".to_string(), Value::Str("float".to_string()));
        }
        Value::Bool(_) => {
            description.insert("type".to_string(), Value::Str("boolean".to_string()));
        }
        Value::Array(arr) => {
            description.insert("type".to_string(), Value::Str("array".to_string()));
            description.insert("length".to_string(), Value::Int(arr.len() as i64));

            if !arr.is_empty() {
                let first_type = match &arr[0] {
                    Value::Str(_) => "string",
                    Value::Int(_) => "integer",
                    Value::Float(_) => "float",
                    Value::Bool(_) => "boolean",
                    Value::Array(_) => "array",
                    Value::Record(_) => "record",
                    _ => "unknown",
                };
                description.insert(
                    "element_type".to_string(),
                    Value::Str(first_type.to_string()),
                );
            }
        }
        Value::Record(record) => {
            description.insert("type".to_string(), Value::Str("record".to_string()));
            description.insert("fields".to_string(), Value::Int(record.len() as i64));

            let field_names: Vec<Value> = record.keys().map(|k| Value::Str(k.clone())).collect();
            description.insert("field_names".to_string(), Value::Array(field_names));
        }
        _ => {
            description.insert("type".to_string(), Value::Str("unknown".to_string()));
        }
    }

    Ok(Value::Record(description))
}

// =============== AI-Enhanced Commands ===============

/// ai-suggest: Get AI-powered command suggestions
fn bi_ai_suggest(args: Vec<Value>, input: Option<Value>, _env: &mut Env) -> Result<Value> {
    let query = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("ai-suggest: input must be a query string")),
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("ai-suggest: query must be a string")),
        }
    } else {
        return Err(anyhow!("ai-suggest: requires query"));
    };

    // For now, provide rule-based suggestions
    let suggestions = get_command_suggestions(&query);
    Ok(Value::Array(suggestions))
}

fn get_command_suggestions(query: &str) -> Vec<Value> {
    let query_lower = query.to_lowercase();
    let mut suggestions = Vec::new();

    if query_lower.contains("list") || query_lower.contains("files") {
        suggestions.push(Value::Str("ls or Get-Files for listing files".to_string()));
        suggestions.push(Value::Str(
            "find . \"*.ext\" for finding specific files".to_string(),
        ));
    }

    if query_lower.contains("read") || query_lower.contains("content") {
        suggestions.push(Value::Str(
            "cat filename or Get-Content filename".to_string(),
        ));
        suggestions.push(Value::Str("head filename for first few lines".to_string()));
    }

    if query_lower.contains("filter") || query_lower.contains("search") {
        suggestions.push(Value::Str("grep \"pattern\" for text search".to_string()));
        suggestions.push(Value::Str(
            "where fn(x) => condition for filtering".to_string(),
        ));
        suggestions.push(Value::Str("Where-Object property -eq value".to_string()));
    }

    if query_lower.contains("sort") {
        suggestions.push(Value::Str("sort for basic sorting".to_string()));
        suggestions.push(Value::Str(
            "Sort-Object property for property-based sorting".to_string(),
        ));
    }

    if query_lower.contains("json") {
        suggestions.push(Value::Str("from-json for parsing JSON".to_string()));
        suggestions.push(Value::Str("to-json for converting to JSON".to_string()));
    }

    if suggestions.is_empty() {
        suggestions.push(Value::Str(
            "Try: ls, cat, grep, sort, map, where".to_string(),
        ));
        suggestions.push(Value::Str(
            "Use 'help' for complete command list".to_string(),
        ));
    }

    suggestions
}

/// ai-explain: Get AI-powered explanations of commands or errors
fn bi_ai_explain(args: Vec<Value>, input: Option<Value>, _env: &mut Env) -> Result<Value> {
    let subject = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("ai-explain: input must be a string to explain")),
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("ai-explain: subject must be a string")),
        }
    } else {
        return Err(anyhow!("ai-explain: requires something to explain"));
    };

    let explanation = generate_explanation(&subject);
    Ok(Value::Str(explanation))
}

fn generate_explanation(subject: &str) -> String {
    let subject_lower = subject.to_lowercase();

    if subject_lower.contains("error") || subject_lower.contains("failed") {
        format!(
            "🔍 Error Analysis: {}\n\n\
            Common causes:\n\
            • Check file paths and permissions\n\
            • Verify command syntax\n\
            • Ensure required arguments are provided\n\
            • Try 'help command_name' for usage info\n\n\
            💡 Use 'ai-suggest \"how to...\"' for alternative approaches",
            subject
        )
    } else if subject_lower.starts_with("ls") || subject_lower.contains("get-files") {
        "📁 ls / Get-Files: Lists files and directories\n\
        • ls . - list current directory\n\
        • ls path - list specific directory\n\
        • Get-Files returns rich objects with metadata\n\
        • Pipe to 'where' or 'select' for filtering"
            .to_string()
    } else if subject_lower.starts_with("cat") || subject_lower.contains("get-content") {
        "📄 cat / Get-Content: Reads file contents\n\
        • cat filename - display entire file\n\
        • Get-Content returns array of lines\n\
        • Pipe to 'head' or 'tail' to limit output\n\
        • Use with grep for searching"
            .to_string()
    } else if subject_lower.contains("pipe") || subject_lower.contains("|") {
        "🔗 Pipelines: Connect commands together\n\
        • data | command - pass data to next command\n\
        • Supports structured data (objects, arrays)\n\
        • Each command processes and transforms data\n\
        • Example: ls . | where fn(f) => !f.is_dir | head 5"
            .to_string()
    } else {
        format!(
            "💭 About: {}\n\n\
            This appears to be a command or concept in AetherShell.\n\
            • Try running it to see what happens\n\
            • Use 'help' for general assistance\n\
            • Check syntax with similar commands\n\
            • Use 'ai-suggest' for alternative approaches",
            subject
        )
    }
}

/// ai-complete: Get AI-powered command completion
fn bi_ai_complete(args: Vec<Value>, input: Option<Value>, _env: &mut Env) -> Result<Value> {
    let partial = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => {
                return Err(anyhow!(
                    "ai-complete: input must be a partial command string"
                ));
            }
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("ai-complete: partial command must be a string")),
        }
    } else {
        return Err(anyhow!("ai-complete: requires partial command"));
    };

    let completions = get_smart_completions(&partial);
    Ok(Value::Array(completions))
}

fn get_smart_completions(partial: &str) -> Vec<Value> {
    let mut completions = Vec::new();

    let builtins = [
        "ls",
        "cat",
        "head",
        "tail",
        "grep",
        "find",
        "sort",
        "uniq",
        "wc",
        "pwd",
        "map",
        "where",
        "reduce",
        "take",
        "print",
        "echo",
        "Get-Files",
        "Get-Content",
        "Select-Object",
        "Where-Object",
        "Sort-Object",
        "from-json",
        "to-json",
        "from-csv",
        "to-csv",
        "describe",
        "columns",
        "ai-suggest",
        "ai-explain",
        "ai-complete",
        "ai-fix",
    ];

    // Basic command completion
    for builtin in &builtins {
        if builtin.to_lowercase().starts_with(&partial.to_lowercase()) {
            completions.push(Value::Str(builtin.to_string()));
        }
    }

    // Pipeline completions
    if partial.ends_with("| ") {
        for cmd in &["map", "where", "select", "sort", "head", "tail", "grep"] {
            completions.push(Value::Str(format!("{}{}", partial, cmd)));
        }
    }

    // File path completions (simplified)
    if partial.contains("\"") || partial.contains("/") || partial.contains("\\") {
        completions.push(Value::Str("./".to_string()));
        completions.push(Value::Str("../".to_string()));
        completions.push(Value::Str("README.md".to_string()));
    }

    completions
}

/// ai-fix: Get AI-powered error fixes and suggestions
fn bi_ai_fix(args: Vec<Value>, input: Option<Value>, _env: &mut Env) -> Result<Value> {
    let error_msg = if let Some(input) = input {
        match input {
            Value::Str(s) => s,
            _ => return Err(anyhow!("ai-fix: input must be an error message string")),
        }
    } else if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(anyhow!("ai-fix: error message must be a string")),
        }
    } else {
        return Err(anyhow!("ai-fix: requires error message"));
    };

    let fix_suggestion = generate_fix_suggestion(&error_msg);
    Ok(Value::Str(fix_suggestion))
}

fn generate_fix_suggestion(error: &str) -> String {
    let error_lower = error.to_lowercase();

    if error_lower.contains("unknown builtin") {
        "🔧 Fix: Command not found\n\
        1. Check spelling: ls, cat, grep, etc.\n\
        2. Try similar commands: ai-suggest \"what command...\"\n\
        3. Use 'help' to see available commands\n\
        4. For PowerShell users: Get-Files instead of Get-ChildItem"
            .to_string()
    } else if error_lower.contains("requires array input") {
        "🔧 Fix: Type mismatch\n\
        1. Wrap single value in array: [value] | command\n\
        2. Use array-producing commands first: ls | command\n\
        3. Check if input data is structured correctly\n\
        4. Try 'describe' to see data type"
            .to_string()
    } else if error_lower.contains("no such file") || error_lower.contains("file not found") {
        "🔧 Fix: File not found\n\
        1. Check current directory: pwd\n\
        2. List available files: ls or Get-Files\n\
        3. Use absolute path: /full/path/to/file\n\
        4. Search for file: find . \"filename*\""
            .to_string()
    } else if error_lower.contains("permission denied") {
        "🔧 Fix: Permission denied\n\
        1. Check file permissions: ls -la (on Unix)\n\
        2. Run with appropriate privileges\n\
        3. Verify file ownership\n\
        4. Use different file location"
            .to_string()
    } else if error_lower.contains("syntax error") || error_lower.contains("parse") {
        "🔧 Fix: Syntax error\n\
        1. Check command syntax: help command_name\n\
        2. Verify parentheses and quotes are balanced\n\
        3. Use proper pipeline syntax: cmd1 | cmd2\n\
        4. Try simpler version first"
            .to_string()
    } else {
        format!(
            "🔧 General Fix Suggestions:\n\
            Error: {}\n\n\
            1. Check command spelling and syntax\n\
            2. Verify input data types with 'describe'\n\
            3. Use 'help' for command documentation\n\
            4. Try 'ai-suggest' for alternative approaches\n\
            5. Break complex commands into simpler steps",
            error
        )
    }
}

// ============ Option Type Constructors ============

/// Some(value) - Creates an Option variant containing a value
/// Returns: Record with _tag="Some" and _value=<arg>
fn bi_some(args: Vec<Value>) -> Result<Value> {
    if args.len() != 1 {
        return Err(anyhow!(
            "Some expects exactly 1 argument, got {}",
            args.len()
        ));
    }

    let mut record = BTreeMap::new();
    record.insert("_tag".to_string(), Value::Str("Some".to_string()));
    record.insert("_value".to_string(), args[0].clone());

    Ok(Value::Record(record))
}

/// None - Creates an Option variant representing no value
/// Returns: Record with _tag="None"
fn bi_none() -> Result<Value> {
    let mut record = BTreeMap::new();
    record.insert("_tag".to_string(), Value::Str("None".to_string()));

    Ok(Value::Record(record))
}

// ============ AI Backend Detection ============

/// ai_backends() - Detect and list all available AI backends
/// Returns: Array of records with backend information
///
/// Example:
///   ai_backends() | select(["name", "available", "models"])
fn bi_ai_backends(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let backends = crate::ai::detect_available_backends();

    let records: Vec<Value> = backends
        .into_iter()
        .map(|backend| {
            let mut record = BTreeMap::new();
            record.insert("name".to_string(), Value::Str(backend.name));
            record.insert(
                "provider".to_string(),
                Value::Str(format!("{:?}", backend.provider)),
            );
            record.insert("endpoint".to_string(), Value::Str(backend.endpoint));
            record.insert("available".to_string(), Value::Bool(backend.available));

            let models: Vec<Value> = backend.models.into_iter().map(Value::Str).collect();
            record.insert("models".to_string(), Value::Array(models));

            Value::Record(record)
        })
        .collect();

    Ok(Value::Array(records))
}

/// ai_detect() - Automatically detect and return the best available backend
/// Returns: String with the model URI (e.g., "ollama:llama3" or "vllm:model")
///
/// Example:
///   let backend = ai_detect()
///   ai(backend, "Hello!")
fn bi_ai_detect(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    match crate::ai::auto_select_backend() {
        Some(uri) => Ok(Value::Str(uri)),
        None => Ok(Value::Str("stub".to_string())),
    }
}

// ========== MCP Server Detection Functions ==========

/// mcp_servers() - Detect all available MCP servers
/// Returns: Array of records with MCP server information
///
/// Each record contains:
///   - name: String - Server name (e.g., "filesystem", "git")
///   - endpoint: String - Server endpoint URL
///   - available: Bool - Whether server is reachable
///   - tools: Array[String] - List of available tool names
///
/// Example:
///   let servers = mcp_servers()
///   servers | foreach(fn(s) => print(s.name + ": " + len(s.tools) + " tools"))
fn bi_mcp_servers(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let servers = crate::ai::detect_mcp_servers();

    let records: Vec<Value> = servers
        .into_iter()
        .map(|server| {
            let mut record = BTreeMap::new();
            record.insert("name".to_string(), Value::Str(server.name));
            record.insert("endpoint".to_string(), Value::Str(server.endpoint));
            record.insert("available".to_string(), Value::Bool(server.available));

            let tools: Vec<Value> = server.tools.into_iter().map(Value::Str).collect();
            record.insert("tools".to_string(), Value::Array(tools));

            Value::Record(record)
        })
        .collect();

    Ok(Value::Array(records))
}

/// mcp_detect(endpoint?) - Detect MCP server at specific endpoint or find first available
/// Args:
///   - endpoint (optional): String - Specific endpoint to check
/// Returns: Record with MCP server info, or null if not found
///
/// Example:
///   let fs_server = mcp_detect("http://localhost:3001")
///   let any_server = mcp_detect()
fn bi_mcp_detect(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if !args.is_empty() {
        // Check specific endpoint
        if let Value::Str(endpoint) = &args[0] {
            let servers = crate::ai::detect_mcp_servers();
            if let Some(server) = servers.iter().find(|s| &s.endpoint == endpoint) {
                let mut record = BTreeMap::new();
                record.insert("name".to_string(), Value::Str(server.name.clone()));
                record.insert("endpoint".to_string(), Value::Str(server.endpoint.clone()));
                record.insert("available".to_string(), Value::Bool(server.available));

                let tools: Vec<Value> =
                    server.tools.iter().map(|t| Value::Str(t.clone())).collect();
                record.insert("tools".to_string(), Value::Array(tools));

                return Ok(Value::Record(record));
            }
        }
        Ok(Value::Null)
    } else {
        // Return first available MCP server
        let servers = crate::ai::detect_mcp_servers();
        if let Some(server) = servers.first() {
            let mut record = BTreeMap::new();
            record.insert("name".to_string(), Value::Str(server.name.clone()));
            record.insert("endpoint".to_string(), Value::Str(server.endpoint.clone()));
            record.insert("available".to_string(), Value::Bool(server.available));

            let tools: Vec<Value> = server.tools.iter().map(|t| Value::Str(t.clone())).collect();
            record.insert("tools".to_string(), Value::Array(tools));

            Ok(Value::Record(record))
        } else {
            Ok(Value::Null)
        }
    }
}

/// mcp_cache_clear() - Clear the MCP detection cache
/// Returns: Bool - true if cache was cleared successfully
///
/// Example:
///   mcp_cache_clear()
///   let servers = mcp_servers()  # This will re-detect servers
fn bi_mcp_cache_clear(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let _ = crate::ai::clear_mcp_cache();
    Ok(Value::Bool(true))
}

/// mcp_cache_status() - Get MCP cache status information
/// Returns: Record with cache status details
///
/// Example:
///   let status = mcp_cache_status()
///   print("Cache hit rate: " + status.hit_rate)
fn bi_mcp_cache_status(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let mut record = BTreeMap::new();

    // For now, just return basic status - we could extend this later with hit/miss counters
    record.insert("enabled".to_string(), Value::Bool(true));
    record.insert("ttl_seconds".to_string(), Value::Int(30)); // Default TTL

    // Check if cache has data
    let has_cached_data = {
        match crate::ai::MCP_CACHE.lock() {
            Ok(cache_guard) => cache_guard.is_some(),
            Err(_) => {
                tracing::warn!("Failed to acquire MCP cache lock for status check");
                false
            }
        }
    };
    record.insert("has_cached_data".to_string(), Value::Bool(has_cached_data));

    Ok(Value::Record(record))
}

/// first(n?) - Get first n elements from array input (default: 1)
/// Args:
///   - n (optional): Int - Number of elements to take (default: 1)
/// Returns: Array with first n elements, or Value if n=1
///
/// Example:
///   [1,2,3,4,5] | first()      # Returns 1
///   [1,2,3,4,5] | first(3)     # Returns [1,2,3]
fn bi_first(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr_val, count_arg) = if let Some(inp) = input {
        // Piped: input is array, args[0] is optional count
        (inp, args.get(0))
    } else if !args.is_empty() {
        // Not piped: args[0] is array, args[1] is optional count
        (args[0].clone(), args.get(1))
    } else {
        return Err(anyhow!("first requires array input"));
    };

    let arr = expect_array("first", &arr_val)?;

    let count = if let Some(c_arg) = count_arg {
        expect_int("first", c_arg)?
    } else {
        1
    };

    if count <= 0 {
        return Ok(Value::Array(vec![]));
    }

    // Fast path for single element (most common case)
    if count == 1 {
        return if arr.is_empty() {
            Ok(Value::Array(vec![]))
        } else {
            Ok(arr[0].clone())
        };
    }

    // Use iterator for all other cases (proven to be fastest)
    let result: Vec<Value> = arr.iter().take(count as usize).cloned().collect();
    Ok(Value::Array(result))
}

/// last(n?) - Get last n elements from array input (default: 1)
/// Args:
///   - n (optional): Int - Number of elements to take (default: 1)
/// Returns: Array with last n elements, or Value if n=1
///
/// Example:
///   [1,2,3,4,5] | last()       # Returns 5
///   [1,2,3,4,5] | last(3)      # Returns [3,4,5]
fn bi_last(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr_val, count_arg) = if let Some(inp) = input {
        // Piped: input is array, args[0] is optional count
        (inp, args.get(0))
    } else if !args.is_empty() {
        // Not piped: args[0] is array, args[1] is optional count
        (args[0].clone(), args.get(1))
    } else {
        return Err(anyhow!("last requires array input"));
    };

    let arr = expect_array("last", &arr_val)?;

    let count = if let Some(c_arg) = count_arg {
        expect_int("last", c_arg)?
    } else {
        1
    };

    if count <= 0 {
        return Ok(Value::Array(vec![]));
    }

    // Fast path for single element (most common case)
    if count == 1 {
        return if arr.is_empty() {
            Ok(Value::Array(vec![]))
        } else {
            Ok(arr[arr.len() - 1].clone())
        };
    }

    // Use slice for all other cases (proven to be fastest)
    let start_idx = if arr.len() > count as usize {
        arr.len() - count as usize
    } else {
        0
    };

    let result: Vec<Value> = arr[start_idx..].to_vec();
    Ok(Value::Array(result))
}

/// any(predicate?) - Check if any element matches predicate (or any truthy if no predicate)
/// Args:
///   - predicate (optional): Lambda - Function to test each element
/// Returns: Bool - true if any element matches
///
/// Example:
///   [1,2,3] | any(fn(x) => x > 2)     # Returns true
///   [false, true, false] | any()      # Returns true
fn bi_any(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let input_val = input.unwrap_or(Value::Array(vec![]));
    let arr = expect_array("any", &input_val)?;

    if args.is_empty() {
        // No predicate - check for any truthy values
        for val in arr {
            if is_truthy(val) {
                return Ok(Value::Bool(true));
            }
        }
        Ok(Value::Bool(false))
    } else {
        // Use predicate function
        let predicate = need_lambda(&args[0], "any")?;

        for val in arr {
            let result = call_lambda(predicate, &[val.clone()], env)?;
            if is_truthy(&result) {
                return Ok(Value::Bool(true));
            }
        }
        Ok(Value::Bool(false))
    }
}

/// all(predicate?) - Check if all elements match predicate (or all truthy if no predicate)
/// Args:
///   - predicate (optional): Lambda - Function to test each element
/// Returns: Bool - true if all elements match
///
/// Example:
///   [1,2,3] | all(fn(x) => x > 0)     # Returns true
///   [true, true, false] | all()       # Returns false
fn bi_all(args: Vec<Value>, input: Option<Value>, env: &mut Env) -> Result<Value> {
    let input_val = input.unwrap_or(Value::Array(vec![]));
    let arr = expect_array("all", &input_val)?;

    if args.is_empty() {
        // No predicate - check if all values are truthy
        for val in arr {
            if !is_truthy(val) {
                return Ok(Value::Bool(false));
            }
        }
        Ok(Value::Bool(true))
    } else {
        // Use predicate function
        let predicate = need_lambda(&args[0], "all")?;

        for val in arr {
            let result = call_lambda(predicate, &[val.clone()], env)?;
            if !is_truthy(&result) {
                return Ok(Value::Bool(false));
            }
        }
        Ok(Value::Bool(true))
    }
}

/// Helper function to determine truthiness of a Value
fn is_truthy(val: &Value) -> bool {
    match val {
        Value::Bool(b) => *b,
        Value::Int(i) => *i != 0,
        Value::Float(f) => *f != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Record(r) => !r.is_empty(),
        Value::Null => false,
        Value::Lambda(_) => true,              // Functions are truthy
        Value::Uri(_) => true,                 // URIs are truthy if they exist
        Value::Table(t) => !t.rows.is_empty(), // Tables are truthy if they have data
    }
}

// ==================== String Functions ====================

/// split(delimiter) - Split string into array by delimiter
/// Args:
///   - delimiter: String - The delimiter to split on
/// Returns: Array - Array of string parts
///
/// Example:
///   "a,b,c" | split(",")     # Returns ["a", "b", "c"]
fn bi_split(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (text_val, delim_arg) = if let Some(inp) = input {
        // Piped: input is string, args[0] is delimiter
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("split requires delimiter argument"))?,
        )
    } else {
        // Not piped: args[0] is string, args[1] is delimiter
        (
            args.get(0)
                .ok_or_else(|| anyhow!("split requires string input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("split requires delimiter argument"))?,
        )
    };

    let text = expect_string("split", &text_val)?;
    let delimiter = expect_string("split", delim_arg)?;

    let parts: Vec<Value> = text
        .split(delimiter)
        .map(|s| Value::Str(s.to_string()))
        .collect();

    Ok(Value::Array(parts))
}

/// join(delimiter) - Join array elements into string with delimiter
/// Args:
///   - delimiter: String - The delimiter to join with
/// Returns: String - Joined string
///
/// Example:
///   ["a", "b", "c"] | join(",")     # Returns "a,b,c"
fn bi_join(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr_val, delim_arg) = if let Some(inp) = input {
        // Piped: input is array, args[0] is delimiter
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("join requires delimiter argument"))?,
        )
    } else {
        // Not piped: args[0] is array, args[1] is delimiter
        (
            args.get(0)
                .ok_or_else(|| anyhow!("join requires array input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("join requires delimiter argument"))?,
        )
    };

    let arr = expect_array("join", &arr_val)?;
    let delimiter = expect_string("join", delim_arg)?;

    let strings: Result<Vec<String>> = arr
        .iter()
        .map(|v| match v {
            Value::Str(s) => Ok(s.clone()),
            Value::Int(i) => Ok(i.to_string()),
            Value::Float(f) => Ok(f.to_string()),
            Value::Bool(b) => Ok(b.to_string()),
            _ => Err(anyhow!("join requires array of strings or numbers")),
        })
        .collect();

    Ok(Value::Str(strings?.join(delimiter)))
}

/// trim() - Remove leading and trailing whitespace from string
/// Returns: String - Trimmed string
///
/// Example:
///   "  hello  " | trim()     # Returns "hello"
fn bi_trim(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("trim requires input string"))?;
    let text = expect_string("trim", &input_val)?;
    Ok(Value::Str(text.trim().to_string()))
}

/// upper() - Convert string to uppercase
/// Returns: String - Uppercase string
///
/// Example:
///   "hello" | upper()     # Returns "HELLO"
fn bi_upper(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("upper requires input string"))?;
    let text = expect_string("upper", &input_val)?;
    Ok(Value::Str(text.to_uppercase()))
}

/// lower() - Convert string to lowercase
/// Returns: String - Lowercase string
///
/// Example:
///   "HELLO" | lower()     # Returns "hello"
fn bi_lower(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("lower requires input string"))?;
    let text = expect_string("lower", &input_val)?;
    Ok(Value::Str(text.to_lowercase()))
}

/// replace(old, new) - Replace all occurrences of substring
/// Args:
///   - old: String - Substring to replace
///   - new: String - Replacement substring
/// Returns: String - String with replacements
///
/// Example:
///   "hello world" | replace("world", "universe")     # Returns "hello universe"
fn bi_replace(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (input_val, old_arg, new_arg) = if let Some(inp) = input {
        // Piped: input is string, args[0] is old, args[1] is new
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("replace requires old substring argument"))?,
            args.get(1)
                .ok_or_else(|| anyhow!("replace requires new substring argument"))?,
        )
    } else {
        // Not piped: args[0] is string, args[1] is old, args[2] is new
        (
            args.get(0)
                .ok_or_else(|| anyhow!("replace requires string input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("replace requires old substring argument"))?,
            args.get(2)
                .ok_or_else(|| anyhow!("replace requires new substring argument"))?,
        )
    };

    let text = expect_string("replace", &input_val)?;
    let old = expect_string("replace", old_arg)?;
    let new = expect_string("replace", new_arg)?;

    Ok(Value::Str(text.replace(old, new)))
}

/// contains(substring) - Check if string contains substring
/// Args:
///   - substring: String - Substring to search for
/// Returns: Bool - true if substring found
///
/// Example:
///   "hello world" | contains("world")     # Returns true
fn bi_contains(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (input_val, substring_arg) = if let Some(inp) = input {
        // Piped: input is string, args[0] is substring
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("contains requires substring argument"))?,
        )
    } else {
        // Not piped: args[0] is string, args[1] is substring
        (
            args.get(0)
                .ok_or_else(|| anyhow!("contains requires string input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("contains requires substring argument"))?,
        )
    };

    let text = expect_string("contains", &input_val)?;
    let substring = expect_string("contains", substring_arg)?;
    Ok(Value::Bool(text.contains(substring)))
}

/// starts_with(prefix) - Check if string starts with prefix
/// Args:
///   - prefix: String - Prefix to check
/// Returns: Bool - true if string starts with prefix
///
/// Example:
///   "hello world" | starts_with("hello")     # Returns true
fn bi_starts_with(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (input_val, prefix_arg) = if let Some(inp) = input {
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("starts_with requires prefix argument"))?,
        )
    } else {
        (
            args.get(0)
                .ok_or_else(|| anyhow!("starts_with requires string input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("starts_with requires prefix argument"))?,
        )
    };

    let text = expect_string("starts_with", &input_val)?;
    let prefix = expect_string("starts_with", prefix_arg)?;
    Ok(Value::Bool(text.starts_with(prefix)))
}

/// ends_with(suffix) - Check if string ends with suffix
/// Args:
///   - suffix: String - Suffix to check
/// Returns: Bool - true if string ends with suffix
///
/// Example:
///   "hello world" | ends_with("world")     # Returns true
fn bi_ends_with(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (input_val, suffix_arg) = if let Some(inp) = input {
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("ends_with requires suffix argument"))?,
        )
    } else {
        (
            args.get(0)
                .ok_or_else(|| anyhow!("ends_with requires string input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("ends_with requires suffix argument"))?,
        )
    };

    let text = expect_string("ends_with", &input_val)?;
    let suffix = expect_string("ends_with", suffix_arg)?;
    Ok(Value::Bool(text.ends_with(suffix)))
}

// ==================== Array Functions (Extended) ====================

/// flatten() - Flatten nested arrays one level deep
/// Returns: Array - Flattened array
///
/// Example:
///   [[1,2],[3,4],[5]] | flatten()     # Returns [1,2,3,4,5]
fn bi_flatten(_args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input_val = input.ok_or_else(|| anyhow!("flatten requires input array"))?;
    let arr = expect_array("flatten", &input_val)?;

    let mut result = Vec::new();
    for val in arr {
        match val {
            Value::Array(inner) => result.extend(inner.clone()),
            other => result.push(other.clone()),
        }
    }

    Ok(Value::Array(result))
}

/// reverse() - Reverse array or string
/// Returns: Array or String - Reversed input
///
/// Example:
///   [1,2,3,4,5] | reverse()     # Returns [5,4,3,2,1]
fn bi_reverse(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("reverse requires input"))?;

    match input_val {
        Value::Array(mut arr) => {
            arr.reverse();
            Ok(Value::Array(arr))
        }
        Value::Str(s) => Ok(Value::Str(s.chars().rev().collect())),
        _ => Err(anyhow!("reverse requires array or string input")),
    }
}

/// slice(start, end?) - Extract slice of array or string
/// Args:
///   - start: Int - Start index (inclusive)
///   - end: Int (optional) - End index (exclusive)
/// Returns: Array or String - Sliced input
///
/// Example:
///   [1,2,3,4,5] | slice(1, 3)     # Returns [2,3]
fn bi_slice(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let input_val = input.ok_or_else(|| anyhow!("slice requires input"))?;

    if args.is_empty() {
        return Err(anyhow!("slice requires at least start index"));
    }

    let start = expect_int("slice", &args[0])? as usize;

    match input_val {
        Value::Array(arr) => {
            let end = if args.len() > 1 {
                expect_int("slice", &args[1])? as usize
            } else {
                arr.len()
            };

            let end = end.min(arr.len());
            let start = start.min(end);

            Ok(Value::Array(arr[start..end].to_vec()))
        }
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            let end = if args.len() > 1 {
                expect_int("slice", &args[1])? as usize
            } else {
                chars.len()
            };

            let end = end.min(chars.len());
            let start = start.min(end);

            Ok(Value::Str(chars[start..end].iter().collect()))
        }
        _ => Err(anyhow!("slice requires array or string input")),
    }
}

/// range(start, end?, step?) - Generate array of integers
/// Args:
///   - start: Int - Start value (or end if only one arg)
///   - end: Int (optional) - End value (exclusive)
///   - step: Int (optional) - Step size (default 1)
/// Returns: Array - Array of integers
///
/// Example:
///   range(5)           # Returns [0,1,2,3,4]
///   range(2, 7)        # Returns [2,3,4,5,6]
///   range(0, 10, 2)    # Returns [0,2,4,6,8]
fn bi_range(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("range requires at least one argument"));
    }

    let (start, end, step) = if args.len() == 1 {
        // range(n) -> 0..n
        (0, expect_int("range", &args[0])?, 1)
    } else if args.len() == 2 {
        // range(start, end) -> start..end
        (
            expect_int("range", &args[0])?,
            expect_int("range", &args[1])?,
            1,
        )
    } else {
        // range(start, end, step)
        (
            expect_int("range", &args[0])?,
            expect_int("range", &args[1])?,
            expect_int("range", &args[2])?,
        )
    };

    if step == 0 {
        return Err(anyhow!("range step cannot be zero"));
    }

    let mut result = Vec::new();
    let mut current = start;

    if step > 0 {
        while current < end {
            result.push(Value::Int(current));
            current += step;
        }
    } else {
        while current > end {
            result.push(Value::Int(current));
            current += step;
        }
    }

    Ok(Value::Array(result))
}

/// zip(array2) - Zip two arrays into array of pairs
/// Args:
///   - array2: Array - Second array to zip with
/// Returns: Array - Array of [item1, item2] pairs
///
/// Example:
///   [1,2,3] | zip(["a","b","c"])     # Returns [[1,"a"],[2,"b"],[3,"c"]]
fn bi_zip(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr1_val, arr2_val) = if let Some(inp) = input {
        // Piped: input is first array, args[0] is second array
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("zip requires second array argument"))?
                .clone(),
        )
    } else {
        // Not piped: args[0] is first array, args[1] is second array
        (
            args.get(0)
                .ok_or_else(|| anyhow!("zip requires two array arguments"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("zip requires two array arguments"))?
                .clone(),
        )
    };

    let arr1 = expect_array("zip", &arr1_val)?;
    let arr2 = expect_array("zip", &arr2_val)?;
    let mut result = Vec::new();

    for (a, b) in arr1.iter().zip(arr2.iter()) {
        result.push(Value::Array(vec![a.clone(), b.clone()]));
    }

    Ok(Value::Array(result))
}

/// push(item) - Add item to end of array
/// Args:
///   - item: Any - Item to add
/// Returns: Array - Array with item added
///
/// Example:
///   [1,2,3] | push(4)     # Returns [1,2,3,4]
fn bi_push(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr_val, item) = if let Some(inp) = input {
        // Piped: input is array, args[0] is item
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("push requires item argument"))?,
        )
    } else {
        // Not piped: args[0] is array, args[1] is item
        (
            args.get(0)
                .ok_or_else(|| anyhow!("push requires array input"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("push requires item argument"))?,
        )
    };

    let mut arr = expect_array("push", &arr_val)?.to_vec();
    arr.push(item.clone());
    Ok(Value::Array(arr))
}

/// concat(array2) - Concatenate two arrays
/// Args:
///   - array2: Array - Array to concatenate
/// Returns: Array - Combined array
///
/// Example:
///   [1,2,3] | concat([4,5,6])     # Returns [1,2,3,4,5,6]
fn bi_concat(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let (arr1_val, arr2_val) = if let Some(inp) = input {
        // Piped: input is first array, args[0] is second array
        (
            inp,
            args.get(0)
                .ok_or_else(|| anyhow!("concat requires second array argument"))?
                .clone(),
        )
    } else {
        // Not piped: args[0] is first array, args[1] is second array
        (
            args.get(0)
                .ok_or_else(|| anyhow!("concat requires two array arguments"))?
                .clone(),
            args.get(1)
                .ok_or_else(|| anyhow!("concat requires two array arguments"))?
                .clone(),
        )
    };

    let mut arr1 = expect_array("concat", &arr1_val)?.to_vec();
    let arr2 = expect_array("concat", &arr2_val)?;
    arr1.extend(arr2.iter().cloned());

    Ok(Value::Array(arr1))
}

// ==================== Math Functions ====================

/// abs(number?) - Get absolute value
/// Args:
///   - number: Number (optional if piped)
/// Returns: Number - Absolute value
///
/// Example:
///   abs(-5)     # Returns 5
fn bi_abs(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if args.is_empty() {
        input.ok_or_else(|| anyhow!("abs requires number argument"))?
    } else {
        args[0].clone()
    };

    match val {
        Value::Int(n) => Ok(Value::Int(n.abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        _ => Err(anyhow!("abs requires numeric input")),
    }
}

/// min(a, b) - Get minimum of two numbers
/// Args:
///   - a: Number
///   - b: Number
/// Returns: Number - Minimum value
///
/// Example:
///   min(3, 7)     # Returns 3
fn bi_min(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.len() < 2 {
        return Err(anyhow!("min requires two arguments"));
    }

    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.min(b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.min(*b))),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).min(*b))),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.min(*b as f64))),
        _ => Err(anyhow!("min requires numeric arguments")),
    }
}

/// max(a, b) - Get maximum of two numbers
/// Args:
///   - a: Number
///   - b: Number
/// Returns: Number - Maximum value
///
/// Example:
///   max(3, 7)     # Returns 7
fn bi_max(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.len() < 2 {
        return Err(anyhow!("max requires two arguments"));
    }

    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.max(b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.max(*b))),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).max(*b))),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.max(*b as f64))),
        _ => Err(anyhow!("max requires numeric arguments")),
    }
}

/// floor(number) - Round down to nearest integer
/// Args:
///   - number: Number
/// Returns: Int - Rounded down value
///
/// Example:
///   floor(3.7)     # Returns 3
fn bi_floor(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if args.is_empty() {
        input.ok_or_else(|| anyhow!("floor requires number argument"))?
    } else {
        args[0].clone()
    };

    match val {
        Value::Int(n) => Ok(Value::Int(n)),
        Value::Float(f) => Ok(Value::Int(f.floor() as i64)),
        _ => Err(anyhow!("floor requires numeric input")),
    }
}

/// ceil(number) - Round up to nearest integer
/// Args:
///   - number: Number
/// Returns: Int - Rounded up value
///
/// Example:
///   ceil(3.2)     # Returns 4
fn bi_ceil(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if args.is_empty() {
        input.ok_or_else(|| anyhow!("ceil requires number argument"))?
    } else {
        args[0].clone()
    };

    match val {
        Value::Int(n) => Ok(Value::Int(n)),
        Value::Float(f) => Ok(Value::Int(f.ceil() as i64)),
        _ => Err(anyhow!("ceil requires numeric input")),
    }
}

/// round(number) - Round to nearest integer
/// Args:
///   - number: Number
/// Returns: Int - Rounded value
///
/// Example:
///   round(3.5)     # Returns 4
fn bi_round(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if args.is_empty() {
        input.ok_or_else(|| anyhow!("round requires number argument"))?
    } else {
        args[0].clone()
    };

    match val {
        Value::Int(n) => Ok(Value::Int(n)),
        Value::Float(f) => Ok(Value::Int(f.round() as i64)),
        _ => Err(anyhow!("round requires numeric input")),
    }
}

/// sqrt(number) - Calculate square root
/// Args:
///   - number: Number
/// Returns: Float - Square root
///
/// Example:
///   sqrt(16)     # Returns 4.0
fn bi_sqrt(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if args.is_empty() {
        input.ok_or_else(|| anyhow!("sqrt requires number argument"))?
    } else {
        args[0].clone()
    };

    let num = match val {
        Value::Int(n) => n as f64,
        Value::Float(f) => f,
        _ => return Err(anyhow!("sqrt requires numeric input")),
    };

    if num < 0.0 {
        return Err(anyhow!("sqrt requires non-negative number"));
    }

    Ok(Value::Float(num.sqrt()))
}

/// pow(base, exponent) - Calculate power
/// Args:
///   - base: Number
///   - exponent: Number
/// Returns: Number - base ^ exponent
///
/// Example:
///   pow(2, 8)     # Returns 256
fn bi_pow(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.len() < 2 {
        return Err(anyhow!("pow requires two arguments: base and exponent"));
    }

    let base = match &args[0] {
        Value::Int(n) => *n as f64,
        Value::Float(f) => *f,
        _ => return Err(anyhow!("pow requires numeric base")),
    };

    let exp = match &args[1] {
        Value::Int(n) => *n as f64,
        Value::Float(f) => *f,
        _ => return Err(anyhow!("pow requires numeric exponent")),
    };

    let result = base.powf(exp);

    // Return Int if result is a whole number and both inputs were Int
    if matches!((&args[0], &args[1]), (Value::Int(_), Value::Int(_))) && result.fract() == 0.0 {
        Ok(Value::Int(result as i64))
    } else {
        Ok(Value::Float(result))
    }
}

// ==================== Utility Functions ====================

/// exit(code?) - Exit the program
/// Args:
///   - code: Int (optional) - Exit code (default 0)
/// Returns: Never returns
///
/// Example:
///   exit(0)     # Exit with success
fn bi_exit(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let code = if args.is_empty() {
        0
    } else {
        expect_int("exit", &args[0])?
    };

    std::process::exit(code as i32);
}

/// env(key) - Get environment variable
/// Args:
///   - key: String - Environment variable name
/// Returns: String or Null - Variable value or null if not set
///
/// Example:
///   env("HOME")     # Returns home directory path
fn bi_env(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("env requires variable name argument"));
    }

    let key = expect_string("env", &args[0])?;
    match std::env::var(key) {
        Ok(val) => Ok(Value::Str(val)),
        Err(_) => Ok(Value::Null),
    }
}

/// set_env(key, value) - Set environment variable
/// Args:
///   - key: String - Environment variable name
///   - value: String - Value to set
/// Returns: Null
///
/// Example:
///   set_env("MY_VAR", "value")
fn bi_set_env(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.len() < 2 {
        return Err(anyhow!("set_env requires key and value arguments"));
    }

    let key = expect_string("set_env", &args[0])?;
    let value = expect_string("set_env", &args[1])?;

    std::env::set_var(key, value);
    Ok(Value::Null)
}

/// sleep(seconds) - Sleep for specified duration
/// Args:
///   - seconds: Number - Duration to sleep
/// Returns: Null
///
/// Example:
///   sleep(2)     # Sleep for 2 seconds
fn bi_sleep(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(anyhow!("sleep requires duration argument"));
    }

    let seconds = match &args[0] {
        Value::Int(n) => *n as f64,
        Value::Float(f) => *f,
        _ => return Err(anyhow!("sleep requires numeric duration")),
    };

    if seconds < 0.0 {
        return Err(anyhow!("sleep duration must be non-negative"));
    }

    std::thread::sleep(Duration::from_secs_f64(seconds));
    Ok(Value::Null)
}

/// time() - Get current Unix timestamp
/// Returns: Int - Current Unix timestamp in seconds
///
/// Example:
///   time()     # Returns 1699401234
fn bi_time(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| anyhow!("Failed to get system time: {}", e))?
        .as_secs();

    Ok(Value::Int(timestamp as i64))
}

/// json_parse(json_string?) - Parse JSON string into value
/// Args:
///   - json_string: String (optional if piped)
/// Returns: Any - Parsed JSON value
///
/// Example:
///   json_parse("{\"a\":1}")     # Returns {a: 1}
fn bi_json_parse(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let json_str = if args.is_empty() {
        let input_val = input.ok_or_else(|| anyhow!("json_parse requires JSON string"))?;
        expect_string("json_parse", &input_val)?.to_string()
    } else {
        expect_string("json_parse", &args[0])?.to_string()
    };

    let json_val: serde_json::Value =
        serde_json::from_str(&json_str).context("Failed to parse JSON")?;

    Ok(json_to_value(json_val))
}

/// json_stringify(value?) - Convert value to JSON string
/// Args:
///   - value: Any (optional if piped)
/// Returns: String - JSON representation
///
/// Example:
///   {a: 1} | json_stringify()     # Returns "{\"a\":1}"
fn bi_json_stringify(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let val = if args.is_empty() {
        input.ok_or_else(|| anyhow!("json_stringify requires value"))?
    } else {
        args[0].clone()
    };

    let json_val = value_to_json(val);
    let json_str = serde_json::to_string(&json_val).context("Failed to stringify to JSON")?;

    Ok(Value::Str(json_str))
}

// ==================== Syntax Knowledge Base Functions ====================

use crate::syntax_kb::{AgenticBinary, SyntaxCategory, SyntaxEntry, SyntaxKB};
use std::sync::{Mutex, OnceLock};

// Global syntax knowledge base
static SYNTAX_KB: OnceLock<Mutex<SyntaxKB>> = OnceLock::new();

fn get_syntax_kb() -> &'static Mutex<SyntaxKB> {
    SYNTAX_KB.get_or_init(|| {
        // Try to load from ~/.aethershell/syntax_kb.json
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        let storage_path = std::path::PathBuf::from(home)
            .join(".aethershell")
            .join("syntax_kb.json");

        // Create directory if it doesn't exist
        if let Some(parent) = storage_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let kb = SyntaxKB::with_storage(storage_path).unwrap_or_else(|_| SyntaxKB::new());
        Mutex::new(kb)
    })
}

/// syntax_get(id) - Get syntax entry by ID
/// Args:
///   - id: String - Syntax ID
/// Returns: Record - Syntax entry details
///
/// Example:
///   syntax_get("ab")  # Returns AgenticBinary protocol details
fn bi_syntax_get(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let id_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("syntax_get requires syntax ID"))?;
    let id = expect_string("syntax_get", &id_val)?;

    let kb = get_syntax_kb().lock().unwrap();
    let entry = kb
        .get(id)
        .ok_or_else(|| anyhow!("Syntax '{}' not found", id))?;

    // Convert to Value::Record
    let mut fields = std::collections::BTreeMap::new();
    fields.insert("id".to_string(), Value::Str(entry.id.clone()));
    fields.insert("name".to_string(), Value::Str(entry.name.clone()));
    fields.insert(
        "category".to_string(),
        Value::Str(format!("{:?}", entry.category)),
    );
    fields.insert(
        "specification".to_string(),
        Value::Str(entry.specification.clone()),
    );
    fields.insert(
        "examples".to_string(),
        Value::Array(
            entry
                .examples
                .iter()
                .map(|s| Value::Str(s.clone()))
                .collect(),
        ),
    );

    if let Some(enc) = &entry.binary_encoding {
        let mut enc_fields = std::collections::BTreeMap::new();
        enc_fields.insert("name".to_string(), Value::Str(enc.name.clone()));
        enc_fields.insert("bit_layout".to_string(), Value::Str(enc.bit_layout.clone()));
        fields.insert("binary_encoding".to_string(), Value::Record(enc_fields));
    }

    Ok(Value::Record(fields))
}

/// syntax_list(category?) - List all syntax entries or by category
/// Args:
///   - category: String (optional) - Category to filter by
/// Returns: Array - List of syntax IDs
///
/// Example:
///   syntax_list()           # Returns all syntax IDs
///   syntax_list("protocol") # Returns protocol syntax IDs
fn bi_syntax_list(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let kb = get_syntax_kb().lock().unwrap();

    let ids = if let Some(cat_val) = input.or_else(|| args.get(0).cloned()) {
        let category_str = expect_string("syntax_list", &cat_val)?;
        let category = match category_str.to_lowercase().as_str() {
            "protocol" => SyntaxCategory::Protocol,
            "language" => SyntaxCategory::Language,
            "encoding" => SyntaxCategory::Encoding,
            "command" => SyntaxCategory::Command,
            "query" => SyntaxCategory::Query,
            other => SyntaxCategory::Custom(other.to_string()),
        };

        kb.list_by_category(&category)
            .iter()
            .map(|e| Value::Str(e.id.clone()))
            .collect()
    } else {
        kb.list_all_ids()
            .iter()
            .map(|id| Value::Str(id.clone()))
            .collect()
    };

    Ok(Value::Array(ids))
}

/// syntax_search(query) - Search syntax entries by keyword
/// Args:
///   - query: String - Search query
/// Returns: Array - Matching syntax IDs
///
/// Example:
///   syntax_search("binary")  # Finds entries with "binary" in name/spec
fn bi_syntax_search(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let query_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("syntax_search requires query string"))?;
    let query = expect_string("syntax_search", &query_val)?;

    let kb = get_syntax_kb().lock().unwrap();
    let results: Vec<Value> = kb
        .search(query)
        .iter()
        .map(|e| Value::Str(e.id.clone()))
        .collect();

    Ok(Value::Array(results))
}

/// syntax_add(entry_record) - Add a new syntax entry
/// Args:
///   - entry_record: Record - Syntax entry with id, name, category, specification
/// Returns: Bool - Success status
///
/// Example:
///   syntax_add({id: "mysyntax", name: "My Syntax", category: "custom", specification: "..."})
fn bi_syntax_add(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let entry_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("syntax_add requires entry record"))?;

    let record = match entry_val {
        Value::Record(r) => r,
        _ => return Err(anyhow!("syntax_add requires Record type")),
    };

    // Extract required fields
    let id = match record.get("id") {
        Some(Value::Str(s)) => s.clone(),
        _ => return Err(anyhow!("syntax_add requires 'id' field")),
    };

    let name = match record.get("name") {
        Some(Value::Str(s)) => s.clone(),
        _ => return Err(anyhow!("syntax_add requires 'name' field")),
    };

    let category_str = match record.get("category") {
        Some(Value::Str(s)) => s.clone(),
        _ => "custom".to_string(),
    };

    let category = match category_str.to_lowercase().as_str() {
        "protocol" => SyntaxCategory::Protocol,
        "language" => SyntaxCategory::Language,
        "encoding" => SyntaxCategory::Encoding,
        "command" => SyntaxCategory::Command,
        "query" => SyntaxCategory::Query,
        other => SyntaxCategory::Custom(other.to_string()),
    };

    let specification = match record.get("specification") {
        Some(Value::Str(s)) => s.clone(),
        _ => return Err(anyhow!("syntax_add requires 'specification' field")),
    };

    let examples = match record.get("examples") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| match v {
                Value::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };

    let entry = SyntaxEntry {
        id,
        name,
        category,
        specification,
        examples,
        binary_encoding: None,
        metadata: std::collections::HashMap::new(),
    };

    let mut kb = get_syntax_kb().lock().unwrap();
    kb.add_entry(entry)?;

    Ok(Value::Bool(true))
}

/// syntax_categories() - List all available syntax categories
/// Returns: Array - List of category names
///
/// Example:
///   syntax_categories()  # Returns ["protocol", "language", ...]
fn bi_syntax_categories(_args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    let kb = get_syntax_kb().lock().unwrap();
    let categories: Vec<Value> = kb
        .list_categories()
        .iter()
        .map(|c| Value::Str(c.clone()))
        .collect();

    Ok(Value::Array(categories))
}

/// ab_encode(msg_type, opcode, payload) - Encode message using AgenticBinary protocol
/// Args:
///   - msg_type: String or Int - Message type (Command/Query/Response/Event or 0-3)
///   - opcode: String or Int - Opcode name or number (0-15)
///   - payload: String - Payload data
/// Returns: Array - Encoded bytes
///
/// Example:
///   ab_encode("Query", "DATA", "hello")  # Encodes AgenticBinary message
///   ab_encode(1, 4, "hello")             # Same using numeric codes
fn bi_ab_encode(args: Vec<Value>, _input: Option<Value>) -> Result<Value> {
    if args.len() < 3 {
        return Err(anyhow!(
            "ab_encode requires 3 arguments: msg_type, opcode, payload"
        ));
    }

    // Parse message type
    let msg_type = match &args[0] {
        Value::Int(i) => *i as u8,
        Value::Str(s) => match s.to_lowercase().as_str() {
            "command" | "cmd" => 0,
            "query" => 1,
            "response" | "resp" => 2,
            "event" => 3,
            _ => return Err(anyhow!("Invalid message type: {}", s)),
        },
        _ => return Err(anyhow!("msg_type must be String or Int")),
    };

    // Parse opcode
    let opcode = match &args[1] {
        Value::Int(i) => *i as u8,
        Value::Str(s) => match s.to_uppercase().as_str() {
            "PING" => 0x0,
            "ACK" => 0x1,
            "QUERY" => 0x2,
            "EXEC" => 0x3,
            "DATA" => 0x4,
            "ERROR" => 0x5,
            "SYNC" => 0x6,
            "AUTH" => 0x7,
            "DELEGATE" => 0x8,
            "COLLABORATE" => 0x9,
            "LEARN" => 0xA,
            "REASON" => 0xB,
            "PLAN" => 0xC,
            "OBSERVE" => 0xD,
            "REFLECT" => 0xE,
            "EXTEND" => 0xF,
            _ => return Err(anyhow!("Invalid opcode: {}", s)),
        },
        _ => return Err(anyhow!("opcode must be String or Int")),
    };

    // Get payload
    let payload = expect_string("ab_encode", &args[2])?;

    // Encode using AgenticBinary (version 0)
    let encoded = AgenticBinary::encode(0, msg_type, opcode, payload.as_bytes())?;

    // Convert to Value::Array of Int values
    let result: Vec<Value> = encoded.iter().map(|&b| Value::Int(b as i64)).collect();

    Ok(Value::Array(result))
}

/// ab_decode(bytes) - Decode AgenticBinary message
/// Args:
///   - bytes: Array - Byte array to decode
/// Returns: Record - Decoded message with version, msg_type, opcode, payload
///
/// Example:
///   ab_decode([20, 5, 104, 101, 108, 108, 111])  # Decodes to record
fn bi_ab_decode(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    let bytes_val = input
        .or_else(|| args.get(0).cloned())
        .ok_or_else(|| anyhow!("ab_decode requires byte array"))?;

    let bytes_arr = expect_array("ab_decode", &bytes_val)?;
    let bytes: Vec<u8> = bytes_arr
        .iter()
        .map(|v| match v {
            Value::Int(i) => Ok(*i as u8),
            _ => Err(anyhow!("Byte array must contain integers")),
        })
        .collect::<Result<Vec<u8>>>()?;

    let (version, msg_type, opcode, payload) = AgenticBinary::decode(&bytes)?;

    let mut result = std::collections::BTreeMap::new();
    result.insert("version".to_string(), Value::Int(version as i64));
    result.insert(
        "msg_type".to_string(),
        Value::Str(AgenticBinary::msg_type_name(msg_type).to_string()),
    );
    result.insert("msg_type_code".to_string(), Value::Int(msg_type as i64));
    result.insert(
        "opcode".to_string(),
        Value::Str(AgenticBinary::opcode_name(opcode).to_string()),
    );
    result.insert("opcode_code".to_string(), Value::Int(opcode as i64));
    result.insert(
        "payload".to_string(),
        Value::Str(String::from_utf8_lossy(&payload).to_string()),
    );
    result.insert(
        "payload_bytes".to_string(),
        Value::Array(payload.iter().map(|&b| Value::Int(b as i64)).collect()),
    );

    Ok(Value::Record(result))
}
