use anyhow::Result;
use std::io::{self, Write};

use crate::{
    env::Env,
    eval::eval_program,
    parser, // must expose `pub fn parse_program(&str) -> anyhow::Result<Vec<crate::ast::Stmt>>`
    value::Value,
};

/// Interactive REPL. Ctrl-D/Ctrl-Z exits.
pub fn run(env: &mut Env) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    writeln!(stdout, "Aurora REPL — type Ctrl-D to exit")?;
    stdout.flush()?;

    loop {
        // Prompt
        write!(stdout, "au> ")?;
        stdout.flush()?;

        // Read one line
        let mut line = String::new();
        let n = stdin.read_line(&mut line)?;
        if n == 0 {
            writeln!(stdout)?;
            break;
        }
        let code = line.trim();
        if code.is_empty() {
            continue;
        }

        match eval_line(env, code) {
            Ok(v) => {
                if let Some(out) = render_for_repl(&v) {
                    writeln!(stdout, "{out}")?;
                }
            }
            Err(e) => {
                writeln!(stdout, "eval error: {e}")?;
            }
        }
    }
    Ok(())
}

/// One-liner (e.g. `ae -c 'code'`)
pub fn run_one(env: &mut Env, code: &str) -> Result<i32> {
    match eval_line(env, code) {
        Ok(v) => {
            if let Some(out) = render_for_repl(&v) {
                println!("{out}");
            }
            Ok(0)
        }
        Err(e) => {
            eprintln!("eval error: {e}");
            Ok(1)
        }
    }
}

pub fn eval_line(env: &mut Env, code: &str) -> Result<Value> {
    let stmts = parser::parse_program(code)?;
    eval_program(&stmts, env)
}

/// REPL rendering:
/// - Null => print nothing
/// - Str  => print raw (no quotes), so ANSI works
/// - else => compact pretty-print
fn render_for_repl(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::Str(s) => Some(s.clone()),
        _ => Some(pp(v)),
    }
}

fn pp(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(x) => x.to_string(),
        Value::Str(s) => s.clone(),
        Value::Uri(u) => u.clone(),
        Value::Array(items) => {
            let mut s = String::new();
            s.push('[');
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&pp_item(it));
            }
            s.push(']');
            s
        }
        Value::Record(map) => {
            let mut s = String::new();
            s.push('{');
            let mut first = true;
            for (k, v) in map {
                if !first {
                    s.push_str(", ");
                }
                first = false;
                s.push_str(k);
                s.push_str(": ");
                s.push_str(&pp_item(v));
            }
            s.push('}');
            s
        }
        Value::Table(t) => format!("<Table rows={}>", t.rows.len()),
        Value::Lambda(_) => "<lambda>".into(),
    }
}

fn pp_item(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(x) => x.to_string(),
        Value::Str(s) => s.clone(),
        Value::Uri(u) => u.clone(),
        Value::Array(a) => format!("[len={}]", a.len()),
        Value::Record(_) => "{…}".into(),
        Value::Table(t) => format!("<Table rows={}>", t.rows.len()),
        Value::Lambda(_) => "<lambda>".into(),
    }
}
