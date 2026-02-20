//! Bash → Aether Shell transpiler (compat mode).
//!
//! This is a pragmatic, line-oriented transpiler for a *useful subset* of Bash:
//!   - simple commands, args, and pipelines (`|`)
//!   - single quotes:  'no expansion'
//!   - double quotes:  expand $V and ${V} as Aether string interpolations `${V}`
//!   - $VAR as a standalone arg becomes an identifier `VAR`
//!   - simple assignments: NAME=value  →  let NAME = <expr>
//!
//! Fallback: if a line contains redirections or constructs we don't yet handle,
//! we emit:  sh(["bash","-lc", "<original line>"])
//!
//! Output: Aether source as a String. The `sh(...)` call is assumed to be a builtin
//! (or small helper) that runs an external command. If you prefer a different name,
//! adjust `render_external_call` below.

use anyhow::{anyhow, Result};

/// Transpile a whole Bash script (multi-line) to Aether code.
/// Transpile a whole Bash script (multi-line) to Aether code.
///
/// Multi-line blocks (if/for/while/until/case/select/function) are accumulated
/// and emitted as a single `sh(["bash","-lc","..."])` call rather than falling
/// back line-by-line.
/// Transpile a whole Bash script (multi-line) to Aether code.
///
/// Multi-line blocks (if/for/while/until/case/select/function) are accumulated
/// and emitted as a single `sh(["bash","-lc","..."])` call rather than falling
/// back line-by-line.
pub fn transpile_bash_to_ae(src: &str) -> Result<String> {
    let mut out = String::new();
    out.push_str("// Transpiled from Bash \u{2192} Aether (compat mode)\n");

    let mut block_depth: i32 = 0;
    let mut block_lines: Vec<String> = Vec::new();

    for raw_line in src.lines() {
        let line = raw_line.trim();

        // Inside a block: accumulate
        if line.is_empty() {
            if block_depth > 0 {
                block_lines.push(raw_line.to_string());
            }
            continue;
        }
        if line.starts_with('#') {
            if block_depth > 0 {
                block_lines.push(raw_line.to_string());
            } else {
                out.push_str(&format!("// {}\n", &line[1..].trim_start()));
            }
            continue;
        }

        let openers = count_block_openers(line);
        let closers = count_block_closers(line);

        // Already inside a block
        if block_depth > 0 {
            block_lines.push(raw_line.to_string());
            block_depth += openers - closers;
            if block_depth <= 0 {
                let full_block = block_lines.join("\n");
                out.push_str(&render_fallback_bash(&full_block));
                out.push('\n');
                block_lines.clear();
                block_depth = 0;
            }
            continue;
        }

        // New block starting
        if openers > closers {
            block_depth = openers - closers;
            block_lines.push(raw_line.to_string());
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
                let aether_line = render_pipeline(&cmds);
                out.push_str(&aether_line);
                out.push('\n');
            }
            _ => {
                out.push_str(&render_fallback_bash(line));
                out.push('\n');
            }
        }
    }

    // Flush any unterminated block
    if !block_lines.is_empty() {
        let full_block = block_lines.join("\n");
        out.push_str(&render_fallback_bash(&full_block));
        out.push('\n');
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
    // If you want to support these, map them to Aether I/O or fallback to `bash -lc`.
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

/* =============================================================================
Block depth tracking
============================================================================= */

fn count_block_openers(line: &str) -> i32 {
    let mut count = 0i32;
    let words: Vec<&str> = line.split_whitespace().collect();
    let first = words.first().copied().unwrap_or("");

    // Control-flow keywords that start blocks
    match first {
        "if" | "for" | "while" | "until" | "case" | "select" => count += 1,
        "function" => count += 1,
        _ => {}
    }

    // name() { ... } style function definitions
    if first != "function" && (line.contains("() {") || line.contains("(){")) {
        count += 1;
    }

    count
}

fn count_block_closers(line: &str) -> i32 {
    let mut count = 0i32;
    let trimmed = line.trim().trim_end_matches(';').trim();
    let first_word = trimmed.split_whitespace().next().unwrap_or("");

    if trimmed == "fi" || trimmed == "done" || trimmed == "esac" || trimmed == "}" {
        count += 1;
    } else if first_word == "fi" || first_word == "done" || first_word == "esac" {
        count += 1;
    }

    count
}

fn render_fallback_bash(line: &str) -> String {
    // Escape the line into a single-quoted Aether string literal safely.
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
        Ok(toks
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Expected single token in bash value"))?)
    } else {
        Err(anyhow!("expected single token, got {}", toks.len()))
    }
}

/* =============================================================================
Rendering Aether code
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
    // If the program can be mapped to an Aether builtin, render directly; else `sh([...])`.
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
    // Aether strings are double-quoted; but for array-to-json-ish we need JSON-like "..." too.
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
    // Aether string literal with minimal escapes, *no* interpolation.
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
    match name {
        // Shell builtins
        "echo" => Some("echo"),
        "printf" => Some("echo"),
        "pwd" => Some("pwd"),
        "cd" => Some("cd"),
        "exit" => Some("exit"),
        "clear" => Some("clear"),
        "true" => Some("true"),
        "false" => Some("false"),
        "test" => Some("test"),

        // File operations
        "ls" => Some("ls"),
        "cat" => Some("file.read"),
        "head" => Some("file.head"),
        "tail" => Some("file.tail"),
        "touch" => Some("file.touch"),
        "mkdir" => Some("file.mkdir"),
        "rmdir" => Some("file.rmdir"),
        "cp" => Some("file.copy"),
        "mv" => Some("file.move"),
        "rm" => Some("file.remove"),
        "ln" => Some("file.link"),
        "chmod" => Some("perm.chmod"),
        "chown" => Some("perm.chown"),
        "stat" => Some("file.stat"),
        "find" => Some("file.find"),
        "basename" => Some("file.basename"),
        "dirname" => Some("file.dirname"),
        "realpath" => Some("file.realpath"),
        "readlink" => Some("file.readlink"),
        "wc" => Some("file.wc"),
        "diff" => Some("file.diff"),
        "tee" => Some("file.tee"),
        "mktemp" => Some("file.mktemp"),

        // Text processing
        "grep" => Some("str.grep"),
        "sed" => Some("str.sed"),
        "awk" => Some("str.awk"),
        "cut" => Some("str.cut"),
        "sort" => Some("sort"),
        "uniq" => Some("arr.unique"),
        "tr" => Some("str.tr"),
        "rev" => Some("str.reverse"),
        "xargs" => Some("map"),

        // System info
        "hostname" => Some("sys.hostname"),
        "uname" => Some("sys.uname"),
        "uptime" => Some("sys.uptime"),
        "whoami" => Some("sys.whoami"),
        "id" => Some("sys.id"),
        "date" => Some("sys.date"),
        "df" => Some("sys.disk_usage"),
        "du" => Some("sys.dir_size"),
        "free" => Some("sys.mem_info"),
        "arch" => Some("platform.arch"),
        "env" => Some("sys.env"),
        "printenv" => Some("sys.env"),

        // Process management
        "ps" => Some("proc.list"),
        "kill" => Some("proc.kill"),
        "killall" => Some("proc.killall"),
        "top" => Some("monitor.htop"),
        "htop" => Some("monitor.htop"),
        "pgrep" => Some("proc.grep"),
        "pkill" => Some("proc.pkill"),
        "nohup" => Some("proc.spawn"),
        "sleep" => Some("sleep"),
        "wait" => Some("wait"),

        // Network
        "ping" => Some("net.ping"),
        "curl" => Some("http.get"),
        "wget" => Some("http.download"),
        "dig" => Some("net.dns_lookup"),
        "nslookup" => Some("net.dns_lookup"),
        "host" => Some("net.dns_lookup"),
        "ip" => Some("net.ip"),
        "ifconfig" => Some("net.interfaces"),
        "netstat" => Some("net.connections"),
        "ss" => Some("net.connections"),
        "traceroute" => Some("net.traceroute"),

        // Archive & compression
        "tar" => Some("archive.tar"),
        "gzip" => Some("archive.gzip"),
        "gunzip" => Some("archive.gunzip"),
        "zip" => Some("archive.zip"),
        "unzip" => Some("archive.unzip"),

        // JSON
        "jq" => Some("json.query"),

        // Package managers (common)
        "apt" | "apt-get" => Some("pkg.install"),
        "yum" | "dnf" => Some("pkg.install"),
        "brew" => Some("pkg.install"),

        // Docker / containers
        "docker" => Some("docker.run"),
        "podman" => Some("podman.run"),
        "kubectl" => Some("k8s.exec"),

        // Version control
        "git" => Some("sh"),

        // Misc
        "which" => Some("sys.which"),
        "type" => Some("sys.which"),
        "man" => Some("help"),
        "history" => Some("history"),
        "alias" => Some("alias"),
        "export" => Some("set_env"),
        "source" | "." => Some("source"),
        "read" => Some("input.readline"),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_echo() {
        let r = transpile_bash_to_ae("echo hello").unwrap();
        assert!(r.contains("echo(\"hello\")"), "got: {}", r);
    }

    #[test]
    fn test_assignment() {
        let r = transpile_bash_to_ae("FOO=bar").unwrap();
        assert!(r.contains("let FOO ="), "got: {}", r);
    }

    #[test]
    fn test_numeric_assignment() {
        let r = transpile_bash_to_ae("x=42").unwrap();
        assert!(r.contains("let x = 42;"), "got: {}", r);
    }

    #[test]
    fn test_pipeline() {
        let r = transpile_bash_to_ae("ls | grep foo").unwrap();
        assert!(r.contains("ls()") && r.contains("|"), "got: {}", r);
    }

    #[test]
    fn test_comment_preserved() {
        let r = transpile_bash_to_ae("# hello world").unwrap();
        assert!(r.contains("// hello world"), "got: {}", r);
    }

    #[test]
    fn test_cat_maps_to_file_read() {
        let r = transpile_bash_to_ae("cat file.txt").unwrap();
        assert!(r.contains("file.read"), "got: {}", r);
    }

    #[test]
    fn test_if_block_fallback() {
        let script = "if true; then\n  echo yes\nfi";
        let r = transpile_bash_to_ae(script).unwrap();
        assert!(r.contains("sh([\"bash\""), "got: {}", r);
    }

    #[test]
    fn test_empty_lines_and_shebang() {
        let script = "#!/bin/bash\n\necho hello";
        let r = transpile_bash_to_ae(script).unwrap();
        assert!(r.contains("echo(\"hello\")"), "got: {}", r);
    }

    #[test]
    fn test_hostname_maps() {
        let r = transpile_bash_to_ae("hostname").unwrap();
        assert!(r.contains("sys.hostname"), "got: {}", r);
    }

    #[test]
    fn test_curl_maps_to_http_get() {
        let r = transpile_bash_to_ae("curl https://example.com").unwrap();
        assert!(r.contains("http.get"), "got: {}", r);
    }

    #[test]
    fn test_for_loop_fallback() {
        let script = "for i in 1 2 3; do\n  echo $i\ndone";
        let r = transpile_bash_to_ae(script).unwrap();
        assert!(r.contains("sh([\"bash\""), "got: {}", r);
    }

    #[test]
    fn test_export_assignment() {
        let r = transpile_bash_to_ae("export FOO=bar").unwrap();
        // export should either map to set_env or be handled as export
        assert!(r.contains("set_env") || r.contains("let FOO"), "got: {}", r);
    }
}
