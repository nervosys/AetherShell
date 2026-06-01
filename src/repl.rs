use anyhow::Result;
use crossterm::style::{Color, Stylize};
use std::io::{self, Write};

use crate::{
    config::{get_config, Theme},
    env::Env,
    eval::eval_program,
    parser, // must expose `pub fn parse_program(&str) -> anyhow::Result<Vec<crate::ast::Stmt>>`
    value::Value,
};

/// Parse a hex color string to crossterm Color
fn parse_hex_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Color::Rgb { r, g, b };
        }
    }
    Color::White // Fallback
}

/// Get the current theme colors
fn get_theme_colors() -> crate::config::CustomColors {
    let config = get_config();
    if config.colors.theme == "custom" {
        config.colors.custom.clone()
    } else {
        Theme::from_str(&config.colors.theme).colors()
    }
}

/// Interactive REPL. Ctrl-D exits or type 'exit'/'quit'.
pub fn run(env: &mut Env) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let config = get_config();

    // Show banner if enabled
    if config.shell.show_banner {
        if config.colors.enabled {
            writeln!(
                stdout,
                "{}",
                "Æther REPL — type 'exit', 'quit', or Ctrl-D to exit".dark_grey()
            )?;
        } else {
            writeln!(
                stdout,
                "Æther REPL — type 'exit', 'quit', or Ctrl-D to exit"
            )?;
        }
        stdout.flush()?;
    }

    loop {
        // Prompt: æ❯ with colors if enabled
        if config.colors.enabled {
            write!(stdout, "{}{} ", "æ".cyan(), "❯".dark_grey())?;
        } else {
            write!(stdout, "æ> ")?;
        }
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

        // Handle exit commands
        if code == "exit" || code == "quit" {
            break;
        }

        match eval_line(env, code) {
            Ok(v) => {
                if let Some(out) = render_for_repl(&v) {
                    writeln!(stdout, "{out}")?;
                }
            }
            Err(e) => {
                if config.colors.enabled {
                    writeln!(stdout, "{} {e}", "error:".red().bold())?;
                } else {
                    writeln!(stdout, "error: {e}")?;
                }
            }
        }
    }
    Ok(())
}

/// One-liner (e.g. `ae -c 'code'`)
pub fn run_one(env: &mut Env, code: &str) -> Result<i32> {
    let config = get_config();
    match eval_line(env, code) {
        Ok(v) => {
            let budget = std::env::var("AE_TOKEN_BUDGET")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|m| *m > 0);
            // Deterministic mode (`--deterministic` / AE_DETERMINISTIC) takes
            // precedence over every other renderer: canonical, byte-stable JSON for
            // snapshot tests / caching / diffs. The whole value is emitted (budget
            // is intentionally not applied — reproducibility wants the full result).
            let deterministic = std::env::var("AE_DETERMINISTIC")
                .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            if deterministic {
                if let Some(out) = crate::builtins::render_canonical(&v) {
                    println!("{out}");
                }
                return Ok(0);
            }
            // Agent mode renders results as compact, deterministic AECON by
            // default (keys once, the structural levers, no ANSI) — the token
            // savings happen automatically instead of requiring an explicit
            // `| aecon`. The human REPL keeps its colorized pretty-printer.
            if crate::safety::current_mode() == crate::safety::Mode::Agent {
                if let Some(out) = crate::builtins::render_agent(&v, budget) {
                    println!("{out}");
                }
                return Ok(0);
            }
            // Human path: apply an output token budget when AE_TOKEN_BUDGET is set
            // (e.g. via `--budget`), then pretty-print.
            let v = match budget {
                Some(max) => crate::builtins::budget_value(&v, max, 0),
                None => v,
            };
            if let Some(out) = render_for_repl(&v) {
                println!("{out}");
            }
            Ok(0)
        }
        Err(e) => {
            print_eval_error(&e, config.colors.enabled);
            Ok(1)
        }
    }
}

/// Render an uncaught evaluation error.
///
/// A [`crate::safety::SafetyError`]'s `Display` is JSON (so agents can branch on
/// it). For a **human** at the REPL that's noise, so we unpack it into legible
/// prose — `error[CODE]: message`, a `hint:` line, and (for an approvable action)
/// the exact re-run incantation. **Agent mode keeps the raw JSON** so the
/// structured `code`/`hint`/`approval` survive for programmatic self-correction.
/// Non-safety errors print as plain prose in both modes.
fn print_eval_error(e: &anyhow::Error, color: bool) {
    use crate::safety::{current_mode, Mode, SafetyError};

    if current_mode() == Mode::Agent {
        // Structured form is what an agent reads — emit it verbatim.
        eprintln!("{e}");
        return;
    }

    if let Some(se) = e.downcast_ref::<SafetyError>() {
        let code = se.code.as_str();
        if color {
            eprintln!(
                "{}{}{} {}",
                "error[".red().bold(),
                code.red().bold(),
                "]:".red().bold(),
                se.message
            );
            if !se.hint.is_empty() {
                eprintln!("  {} {}", "hint:".yellow().bold(), se.hint);
            }
            if let Some(a) = &se.approval {
                eprintln!(
                    "  {} re-run with AETHER_APPROVE={}  (or call approve(\"{}\"))",
                    "approve:".cyan().bold(),
                    a.token,
                    a.token
                );
            }
        } else {
            eprintln!("error[{code}]: {}", se.message);
            if !se.hint.is_empty() {
                eprintln!("  hint: {}", se.hint);
            }
            if let Some(a) = &se.approval {
                eprintln!(
                    "  approve: re-run with AETHER_APPROVE={}  (or call approve(\"{}\"))",
                    a.token, a.token
                );
            }
        }
        return;
    }

    if color {
        eprintln!("{} {e}", "error:".red().bold());
    } else {
        eprintln!("error: {e}");
    }
}

pub fn eval_line(env: &mut Env, code: &str) -> Result<Value> {
    let stmts = parser::parse_program(code)?;
    eval_program(&stmts, env)
}

/// REPL rendering:
/// - Null => print nothing
/// - Str  => print raw (no quotes), so ANSI works
/// - else => compact colorized pretty-print (or plain if colors disabled)
fn render_for_repl(v: &Value) -> Option<String> {
    let config = get_config();
    match v {
        Value::Null => None,
        Value::Str(s) => Some(s.clone()),
        _ => {
            if config.colors.enabled {
                Some(pp_colored(v))
            } else {
                Some(pp(v))
            }
        }
    }
}

/// Apply a color from the theme to a string
fn colorize(s: &str, hex_color: &str) -> String {
    let config = get_config();
    if config.colors.true_color {
        format!("{}", s.with(parse_hex_color(hex_color)))
    } else {
        // Fallback to basic colors when true_color is disabled
        s.to_string()
    }
}

/// Colorized pretty-print using theme colors from config
fn pp_colored(v: &Value) -> String {
    let colors = get_theme_colors();
    match v {
        Value::Null => colorize("null", &colors.dim),
        Value::Bool(b) => colorize(&b.to_string(), &colors.boolean),
        Value::Int(n) => colorize(&n.to_string(), &colors.number),
        Value::Float(x) => colorize(&x.to_string(), &colors.number),
        Value::Str(s) => colorize(&format!("\"{}\"", s), &colors.string),
        Value::Uri(u) => colorize(u, &colors.uri),
        Value::Array(items) => {
            let mut s = String::new();
            s.push_str(&colorize("[", &colors.punctuation));
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&pp_item_colored(it));
            }
            s.push_str(&colorize("]", &colors.punctuation));
            s
        }
        Value::Record(map) => {
            let mut s = String::new();
            s.push_str(&colorize("{", &colors.punctuation));
            let mut first = true;
            for (k, v) in map {
                if !first {
                    s.push_str(", ");
                }
                first = false;
                s.push_str(&colorize(k, &colors.key));
                s.push_str(": ");
                s.push_str(&pp_item_colored(v));
            }
            s.push_str(&colorize("}", &colors.punctuation));
            s
        }
        Value::Table(t) => colorize(&format!("<Table rows={}>", t.rows.len()), &colors.dim),
        Value::Lambda(_) => colorize("<lambda>", &colors.dim),
        Value::AsyncLambda(_) => colorize("<async lambda>", &colors.dim),
        Value::Future(_) => colorize("<future>", &colors.dim),
        Value::Error(msg) => colorize(&format!("Error: {}", msg), &colors.error),
        Value::Builtin(b) => colorize(&format!("<builtin:{}>", b.name), &colors.dim),
    }
}

fn pp_item_colored(v: &Value) -> String {
    let colors = get_theme_colors();
    match v {
        Value::Null => colorize("null", &colors.dim),
        Value::Bool(b) => colorize(&b.to_string(), &colors.boolean),
        Value::Int(n) => colorize(&n.to_string(), &colors.number),
        Value::Float(x) => colorize(&x.to_string(), &colors.number),
        Value::Str(s) => colorize(&format!("\"{}\"", s), &colors.string),
        Value::Uri(u) => colorize(u, &colors.uri),
        Value::Array(a) => colorize(&format!("[…{}]", a.len()), &colors.punctuation),
        Value::Record(_) => colorize("{…}", &colors.dim),
        Value::Table(t) => colorize(&format!("<Table rows={}>", t.rows.len()), &colors.dim),
        Value::Lambda(_) => colorize("<lambda>", &colors.dim),
        Value::AsyncLambda(_) => colorize("<async lambda>", &colors.dim),
        Value::Future(_) => colorize("<future>", &colors.dim),
        Value::Error(msg) => colorize(&format!("Error: {}", msg), &colors.error),
        Value::Builtin(b) => colorize(&format!("<builtin:{}>", b.name), &colors.dim),
    }
}

// Non-colored versions for when colors are disabled
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
        Value::AsyncLambda(_) => "<async lambda>".into(),
        Value::Future(_) => "<future>".into(),
        Value::Builtin(b) => format!("<builtin:{}>", b.name),
        Value::Error(msg) => format!("Error: {}", msg),
    }
}

#[allow(dead_code)]
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
        Value::AsyncLambda(_) => "<async lambda>".into(),
        Value::Future(_) => "<future>".into(),
        Value::Builtin(b) => format!("<builtin:{}>", b.name),
        Value::Error(msg) => format!("Error: {}", msg),
    }
}
