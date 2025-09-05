//! Bash → Aurora Shell transpiler (compat mode).
//!
//! This is a pragmatic, line-oriented transpiler for a *useful subset* of Bash:
//!   - simple commands, args, and pipelines (`|`)
//!   - single quotes:  'no expansion'
//!   - double quotes:  expand $V and ${V} as Aurora string interpolations `${V}`
//!   - $VAR as a standalone arg becomes an identifier `VAR`
//!   - simple assignments: NAME=value  →  let NAME = <expr>
//!
//! Fallback: if a line contains redirections or constructs we don't yet handle,
//! we emit:  sh(["bash","-lc", "<original line>"])
//!
//! Output: Aurora source as a String. The `sh(...)` call is assumed to be a builtin
//! (or small helper) that runs an external command. If you prefer a different name,
//! adjust `render_external_call` below.

use anyhow::{Result, anyhow};

/// Transpile a whole Bash script (multi-line) to Aurora code.
pub fn transpile_bash_to_ae(src: &str) -> Result<String> {
    let mut out = String::new();
    out.push_str("// Transpiled from Bash → Aurora (compat mode)\n");

    for raw_line in src.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            // preserve comments as Aurora comments
            if line.starts_with('#') {
                out.push_str(&format!("// {}\n", &line[1..].trim_start()));
            }
            continue;
        }

        // Quick detect: if there are redirections or unsupported operators,
        // fall back to `bash -lc`.
        if contains_redirection_or_complex(line) {
            out.push_str(&render_fallback_bash(line));
            out.push('\n');
            continue;
        }

        // Simple assignment?
        if let Some(assign) = parse_simple_assignment(line) {
            out.push_str(&assign);
            out.push('\n');
            continue;
        }

        // Try to parse as a pipeline of commands.
        match parse_pipeline(line) {
            Ok(cmds) if !cmds.is_empty() => {
                let aurora_line = render_pipeline(&cmds);
                out.push_str(&aurora_line);
                out.push('\n');
            }
            _ => {
                // Fallback if we failed to parse robustly
                out.push_str(&render_fallback_bash(line));
                out.push('\n');
            }
        }
    }

    Ok(out)
}

/* =============================================================================
Representation
============================================================================= */

#[derive(Debug, Clone)]
struct Command {
    program: Token,   // typically plain word (may include vars if unquoted/double-quoted)
    args: Vec<Token>, // arguments
}

#[derive(Debug, Clone)]
enum Token {
    /// Normal / double-quoted token with possible var expansions
    Interp(Vec<Piece>),
    /// Single-quoted string with no expansion
    Single(String),
}

#[derive(Debug, Clone)]
enum Piece {
    Text(String),
    Var(String), // VAR name (no leading '$')
}

/* =============================================================================
Top-level quick checks and simple forms
============================================================================= */

fn contains_redirection_or_complex(s: &str) -> bool {
    // quick n' dirty: redirect operators or process-substitution or background
    // If you want to support these, map them to Aurora I/O or fallback to `bash -lc`.
    let suspicious = ['>', '<', '&'];
    s.chars().any(|c| suspicious.contains(&c))
        || s.contains("2>")
        || s.contains(">>")
        || s.contains("<<")
        || s.contains("|&")
        || s.contains("<<<")
        || s.contains(">|")
        || s.contains("$(")  // command substitution: not supported yet, fallback
        || s.contains("`") // backticks: not supported yet, fallback
}

fn render_fallback_bash(line: &str) -> String {
    // Escape the line into a single-quoted Aurora string literal safely.
    // We'll build: sh(["bash","-lc","<line>"])
    let quoted = single_quote_literal(line);
    format!("sh([\"bash\",\"-lc\", {}]);", quoted)
}

fn parse_simple_assignment(line: &str) -> Option<String> {
    // Accept NAME=VALUE (no spaces around '='). Ignore export/readonly for now.
    // Examples:
    //   FOO=bar           -> let FOO = "bar";
    //   X=$HOME           -> let X = HOME;
    //   N=123             -> let N = 123;
    // We avoid compound assignments like PATH="$HOME:${PATH}" -> fallback (too complex).
    if line.starts_with("export ") || line.starts_with("readonly ") {
        return None; // skip here so that fallback or command path handles it
    }

    // No spaces allowed on either side for this "simple" matcher.
    if line.split_whitespace().count() != 1 {
        return None;
    }
    if let Some(eq) = line.find('=') {
        let (lhs, rhs) = line.split_at(eq);
        let lhs = lhs.trim();
        if lhs.is_empty() {
            return None;
        }
        if !is_ident(lhs) {
            return None;
        }
        let rhs = &rhs[1..].trim(); // skip '='

        // If RHS contains spaces or operators, it's not "simple"
        if rhs.is_empty() || rhs.contains(' ') || contains_redirection_or_complex(rhs) {
            return None;
        }

        // Decide how to render RHS:
        if rhs.starts_with('"') || rhs.starts_with('\'') {
            // Strip quotes and render appropriately.
            if let Ok(tok) = parse_single_token(rhs) {
                let expr = render_arg_expr(&tok);
                return Some(format!("let {} = {};", lhs, expr));
            } else {
                return None;
            }
        } else if rhs.starts_with('$') {
            if let Some(var) = parse_var_ref(rhs) {
                return Some(format!("let {} = {};", lhs, var));
            } else {
                return None;
            }
        } else if is_int_literal(rhs) {
            return Some(format!("let {} = {};", lhs, rhs));
        } else if is_float_literal(rhs) {
            return Some(format!("let {} = {};", lhs, rhs));
        } else {
            // treat as plain string
            return Some(format!("let {} = {};", lhs, single_quote_literal(rhs)));
        }
    }
    None
}

/* =============================================================================
Parsing a pipeline
============================================================================= */

fn parse_pipeline(s: &str) -> Result<Vec<Command>> {
    let parts = split_unquoted_pipes(s)?;
    let mut cmds = Vec::new();
    for p in parts {
        let toks = split_shell_words(&p)?;
        if toks.is_empty() {
            continue;
        }
        let program = toks[0].clone();
        let args = toks[1..].to_vec();
        cmds.push(Command { program, args });
    }
    Ok(cmds)
}

fn split_unquoted_pipes(s: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut dq = false;
    let mut sq = false;

    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !dq => {
                sq = !sq;
                cur.push(ch);
            }
            '"' if !sq => {
                dq = !dq;
                cur.push(ch);
            }
            '|' if !sq && !dq => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    Ok(out)
}

fn split_shell_words(s: &str) -> Result<Vec<Token>> {
    let mut toks: Vec<Token> = Vec::new();
    let mut cur = Vec::<Piece>::new();
    let mut literal_sq = None::<String>; // collecting for a 'single quoted' string
    let mut dq = false;
    let mut sq = false;
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !dq => {
                if !sq {
                    sq = true;
                    literal_sq = Some(String::new());
                } else {
                    // closing single quote
                    let lit = literal_sq.take().unwrap_or_default();
                    toks.push(Token::Single(lit));
                    sq = false;
                }
            }
            '"' if !sq => {
                dq = !dq;
                if dq {
                    // opening double quote -> nothing special here
                } else {
                    // closing -> flush current Interp pieces as a token if any
                    if !cur.is_empty() {
                        toks.push(Token::Interp(std::mem::take(&mut cur)));
                    }
                }
            }
            c if sq => {
                // inside single quotes, no expansions
                if let Some(ref mut lit) = literal_sq {
                    lit.push(c);
                }
            }
            c if dq => {
                match c {
                    '$' => {
                        if let Some((var, consumed)) = read_var(&mut chars) {
                            cur.push(Piece::Var(var));
                            // consumed handled inside read_var
                            let _ = consumed;
                        } else {
                            cur.push(Piece::Text("$".into()));
                        }
                    }
                    _ => cur.push(Piece::Text(c.to_string())),
                }
            }
            c if c.is_whitespace() => {
                // token boundary (only if we have pieces pending)
                if !cur.is_empty() {
                    toks.push(Token::Interp(std::mem::take(&mut cur)));
                }
            }
            '$' => {
                if let Some((var, consumed)) = read_var(&mut chars) {
                    cur.push(Piece::Var(var));
                    let _ = consumed;
                } else {
                    cur.push(Piece::Text("$".into()));
                }
            }
            _ => {
                cur.push(Piece::Text(ch.to_string()));
            }
        }
    }

    // finalize last pieces if present
    if sq {
        // unterminated single quote: treat everything as literal
        let lit = literal_sq.take().unwrap_or_default();
        toks.push(Token::Single(lit));
    }
    if !cur.is_empty() {
        toks.push(Token::Interp(cur));
    }

    Ok(toks)
}

fn parse_single_token(s: &str) -> Result<Token> {
    // Parse a single token that may be quoted; reuse the splitting and expect exactly one token.
    let toks = split_shell_words(s)?;
    if toks.len() == 1 {
        Ok(toks.into_iter().next().unwrap())
    } else {
        Err(anyhow!("expected single token, got {}", toks.len()))
    }
}

/* =============================================================================
Rendering Aurora code
============================================================================= */

fn render_pipeline(cmds: &[Command]) -> String {
    let mut parts = Vec::new();
    for (i, c) in cmds.iter().enumerate() {
        let is_first = i == 0;
        parts.push(render_command(c, is_first));
    }
    parts.join(" | ")
}

fn render_command(cmd: &Command, _is_first: bool) -> String {
    // If the program can be mapped to an Aurora builtin, render directly; else `sh([...])`.
    let prog_name = token_to_plain_word(&cmd.program);

    if let Some(name) = prog_name.as_deref() {
        if let Some(builtin) = map_builtin(name) {
            let args = cmd
                .args
                .iter()
                .map(render_arg_expr)
                .collect::<Vec<_>>()
                .join(", ");
            return if args.is_empty() {
                format!("{}()", builtin)
            } else {
                format!("{}({})", builtin, args)
            };
        }
    }

    // External: sh(["prog","arg1","arg2", ...])
    render_external_call(cmd)
}

fn render_external_call(cmd: &Command) -> String {
    let mut arr_items = Vec::new();

    // Program token as a string literal if we can't get a simple word;
    // otherwise use its text directly (still a string literal).
    let prog_text = match token_to_plain_word(&cmd.program) {
        Some(s) => s,
        None => token_to_string(&cmd.program),
    };
    arr_items.push(json_string_literal(&prog_text));

    for a in &cmd.args {
        arr_items.push(json_string_literal(&token_to_string(a)));
    }
    format!("sh([{}])", arr_items.join(", "))
}

fn render_arg_expr(tok: &Token) -> String {
    match tok {
        Token::Single(s) => double_quote_with_escapes(s),
        Token::Interp(pcs) => {
            // If it's exactly one Var, render as an identifier (VAR) not a string.
            if pcs.len() == 1 {
                if let Piece::Var(v) = &pcs[0] {
                    if is_ident(v) {
                        return v.clone();
                    }
                }
            }
            // Otherwise, produce an interpolated string: "text${VAR}more"
            let mut s = String::from("\"");
            for p in pcs {
                match p {
                    Piece::Text(t) => s.push_str(&escape_in_double_quotes(t)),
                    Piece::Var(v) => {
                        s.push_str("${");
                        s.push_str(v);
                        s.push('}');
                    }
                }
            }
            s.push('"');
            s
        }
    }
}

/* =============================================================================
Utilities: token → strings
============================================================================= */

/// If the token is a simple plain word w/o expansions and not quoted, return it; else None.
fn token_to_plain_word(tok: &Token) -> Option<String> {
    match tok {
        Token::Single(_) => None,
        Token::Interp(pcs) => {
            // plain word if all pieces are Text and none contain spaces
            if pcs.iter().all(|p| matches!(p, Piece::Text(_))) {
                let s = pcs
                    .iter()
                    .map(|p| match p {
                        Piece::Text(t) => t,
                        _ => unreachable!(),
                    })
                    .cloned()
                    .collect::<String>();
                if !s.is_empty() && !s.chars().any(|c| c.is_whitespace()) {
                    return Some(s);
                }
            }
            None
        }
    }
}

/// Turn a token into a string by evaluating only *static* parts; variables become literal `$VAR`.
fn token_to_string(tok: &Token) -> String {
    match tok {
        Token::Single(s) => s.clone(),
        Token::Interp(pcs) => {
            let mut s = String::new();
            for p in pcs {
                match p {
                    Piece::Text(t) => s.push_str(t),
                    Piece::Var(v) => {
                        s.push('$');
                        s.push_str(v);
                    }
                }
            }
            s
        }
    }
}

/* =============================================================================
Small lex helpers
============================================================================= */

fn read_var(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<(String, usize)> {
    // Supports $NAME and ${NAME}; returns (name, consumed_chars) excluding the leading '$'
    let mut consumed = 0usize;
    if let Some('{') = chars.peek().copied() {
        // ${NAME}
        let _ = chars.next(); // consume '{'
        consumed += 1;
        let mut name = String::new();
        while let Some(&ch) = chars.peek() {
            consumed += 1;
            let _ = chars.next();
            if ch == '}' {
                break;
            }
            name.push(ch);
        }
        if name.is_empty() {
            return None;
        }
        Some((name, consumed))
    } else {
        // $NAME
        let mut name = String::new();
        while let Some(&ch) = chars.peek() {
            if ch == '_' || ch.is_ascii_alphanumeric() {
                name.push(ch);
                let _ = chars.next();
                consumed += 1;
            } else {
                break;
            }
        }
        if name.is_empty() {
            None
        } else {
            Some((name, consumed))
        }
    }
}

fn parse_var_ref(s: &str) -> Option<String> {
    if let Some(rest) = s.strip_prefix("${") {
        if let Some(end) = rest.find('}') {
            let name = &rest[..end];
            if is_ident(name) {
                return Some(name.to_string());
            }
        }
        None
    } else if let Some(name) = s.strip_prefix('$') {
        if is_ident(name) {
            Some(name.to_string())
        } else {
            None
        }
    } else {
        None
    }
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    for ch in chars {
        if !(ch == '_' || ch.is_ascii_alphanumeric()) {
            return false;
        }
    }
    true
}

fn is_int_literal(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn is_float_literal(s: &str) -> bool {
    // very loose
    s.contains('.')
        && s.chars().filter(|c| *c == '.').count() == 1
        && s.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/* =============================================================================
Quoting helpers
============================================================================= */

fn single_quote_literal(s: &str) -> String {
    // Aurora strings are double-quoted; but for array-to-json-ish we need JSON-like "..." too.
    // We'll produce a JSON string literal here for `sh([...])`.
    json_string_literal(s)
}

fn json_string_literal(s: &str) -> String {
    // Simple JSON string escaper for ASCII-oriented input.
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn double_quote_with_escapes(s: &str) -> String {
    // Aurora string literal with minimal escapes, *no* interpolation.
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn escape_in_double_quotes(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

/* =============================================================================
Builtin mapping
============================================================================= */

fn map_builtin(name: &str) -> Option<&'static str> {
    // Extend as Aurora adds more native builtins.
    match name {
        "echo" => Some("echo"),
        "pwd" => Some("pwd"),
        "cd" => Some("cd"),
        // "cat" => Some("read_text"), // optional: would change behavior (returns text)
        _ => None,
    }
}
