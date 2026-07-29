//! Zsh → Aether Shell transpiler (compat mode).
//!
//! Handles a useful subset of Zsh:
//!   - Simple commands, args, and pipelines (`|`)
//!   - Single/double quotes with $VAR / ${VAR} expansion
//!   - Simple assignments: NAME=value → let NAME = <expr>
//!   - Array literals: name=(a b c) → let name = ["a", "b", "c"];
//!   - Associative arrays: typeset -A name → let name = {};
//!   - local/typeset/declare variables → let
//!   - export NAME=VALUE → let NAME = VALUE
//!   - setopt/unsetopt/autoload/zparseopts → comments
//!   - Multi-line block accumulation (if/for/while/case/function) → single sh() call
//!   - 100+ command → Aether builtin mappings
//!
//! Fallback: constructs we don't handle emit: sh(["zsh","-c","<code>"])

use anyhow::{anyhow, Result};

/// Transpile a Zsh script (multi-line) to Aether code.
pub fn transpile_zsh_to_ae(src: &str) -> Result<String> {
    let mut out = String::new();
    out.push_str("// Transpiled from Zsh → Aether (compat mode)\n");

    let mut block_depth: i32 = 0;
    let mut block_lines: Vec<String> = Vec::new();

    for raw_line in src.lines() {
        let line = raw_line.trim();

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
                out.push_str(&format!("// {}\n", line[1..].trim_start()));
            }
            continue;
        }

        let openers = count_block_openers(line);
        let closers = count_block_closers(line);

        if block_depth > 0 {
            block_lines.push(raw_line.to_string());
            block_depth += openers - closers;
            if block_depth <= 0 {
                let full_block = block_lines.join("\n");
                out.push_str(&render_fallback_zsh(&full_block));
                out.push('\n');
                block_lines.clear();
                block_depth = 0;
            }
            continue;
        }

        if openers > closers {
            block_depth = openers - closers;
            block_lines.push(raw_line.to_string());
            continue;
        }

        // Zsh-specific: setopt / unsetopt → comment
        if line.starts_with("setopt ") || line.starts_with("unsetopt ") {
            out.push_str(&format!("// [zsh] {}\n", line));
            continue;
        }
        if line.starts_with("autoload ") || line.starts_with("zparseopts ") {
            out.push_str(&format!("// [zsh] {}\n", line));
            continue;
        }
        if line.starts_with("compdef ")
            || line.starts_with("compctl ")
            || line.starts_with("zstyle ")
            || line.starts_with("bindkey ")
        {
            out.push_str(&format!("// [zsh] {}\n", line));
            continue;
        }

        // typeset -A (associative array) → empty record
        if line.starts_with("typeset -A ")
            || line.starts_with("declare -A ")
            || line.starts_with("local -A ")
        {
            let rest = line.split_once(" -A ").map(|x| x.1).unwrap_or("").trim();
            if !rest.is_empty() {
                let name = rest.split('=').next().unwrap_or(rest).trim();
                if is_ident(name) {
                    out.push_str(&format!("let {} = {{}};\n", name));
                    continue;
                }
            }
        }

        // typeset / local / declare (non-flag) → let
        if (line.starts_with("local ")
            || line.starts_with("typeset ")
            || line.starts_with("declare "))
            && !line.contains(" -")
        {
            let rest = line.split_once(' ').map(|x| x.1).unwrap_or("").trim();
            if let Some(eq) = rest.find('=') {
                let name = rest[..eq].trim();
                let val = rest[eq + 1..].trim();
                if is_ident(name) {
                    if val.starts_with('(') && val.ends_with(')') {
                        let inner = &val[1..val.len() - 1];
                        let elems: Vec<String> = inner
                            .split_whitespace()
                            .map(|w| {
                                let w = w.trim_matches(|c: char| c == '\'' || c == '"');
                                if is_int_literal(w) {
                                    w.to_string()
                                } else {
                                    json_string_literal(w)
                                }
                            })
                            .collect();
                        out.push_str(&format!("let {} = [{}];\n", name, elems.join(", ")));
                    } else if is_int_literal(val) {
                        out.push_str(&format!("let {} = {};\n", name, val));
                    } else if is_float_literal(val) {
                        out.push_str(&format!("let {} = {};\n", name, val));
                    } else {
                        let cleaned = val.trim_matches(|c: char| c == '\'' || c == '"');
                        out.push_str(&format!(
                            "let {} = {};\n",
                            name,
                            json_string_literal(cleaned)
                        ));
                    }
                    continue;
                }
            } else if is_ident(rest) {
                out.push_str(&format!("let {} = null;\n", rest));
                continue;
            }
        }

        // export NAME=VALUE → let NAME = VALUE;
        if line.starts_with("export ") {
            let rest = line[7..].trim();
            if let Some(eq) = rest.find('=') {
                let lhs = rest[..eq].trim();
                let rhs = rest[eq + 1..].trim();
                if is_ident(lhs) && !rhs.is_empty() {
                    if is_int_literal(rhs) {
                        out.push_str(&format!("let {} = {};\n", lhs, rhs));
                    } else if rhs.starts_with('$') {
                        if let Some(var) = parse_var_ref(rhs) {
                            out.push_str(&format!("let {} = {};\n", lhs, var));
                        } else {
                            out.push_str(&render_fallback_zsh(line));
                            out.push('\n');
                        }
                    } else {
                        let cleaned = rhs.trim_matches(|c: char| c == '\'' || c == '"');
                        out.push_str(&format!(
                            "let {} = {};\n",
                            lhs,
                            json_string_literal(cleaned)
                        ));
                    }
                    continue;
                }
            }
        }

        if contains_zsh_complex(line) {
            out.push_str(&render_fallback_zsh(line));
            out.push('\n');
            continue;
        }

        if let Some(assign) = parse_simple_assignment(line) {
            out.push_str(&assign);
            out.push('\n');
            continue;
        }

        match parse_pipeline(line) {
            Ok(cmds) if !cmds.is_empty() => {
                out.push_str(&render_pipeline(&cmds));
                out.push('\n');
            }
            _ => {
                out.push_str(&render_fallback_zsh(line));
                out.push('\n');
            }
        }
    }

    if !block_lines.is_empty() {
        let full_block = block_lines.join("\n");
        out.push_str(&render_fallback_zsh(&full_block));
        out.push('\n');
    }

    Ok(out)
}
/* =============================================================================
Block depth tracking
============================================================================= */

fn count_block_openers(line: &str) -> i32 {
    let mut count = 0i32;
    let words: Vec<&str> = line.split_whitespace().collect();
    let first = words.first().copied().unwrap_or("");

    match first {
        "if" | "for" | "while" | "until" | "case" | "select" => count += 1,
        "function" => count += 1,
        _ => {}
    }

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

/* =============================================================================
Representation
============================================================================= */

#[derive(Debug, Clone)]
struct Command {
    program: Token,
    args: Vec<Token>,
}

#[derive(Debug, Clone)]
enum Token {
    Interp(Vec<Piece>),
    Single(String),
}

#[derive(Debug, Clone)]
enum Piece {
    Text(String),
    Var(String),
}

/* =============================================================================
Complexity detection
============================================================================= */

fn contains_zsh_complex(s: &str) -> bool {
    let suspicious = ['>', '<', '&'];
    s.chars().any(|c| suspicious.contains(&c))
        || s.contains("2>")
        || s.contains(">>")
        || s.contains("<<")
        || s.contains("|&")
        || s.contains("<<<")
        || s.contains(">|")
        || s.contains("$(")
        || s.contains("`")
        || s.contains("=~")
}

fn render_fallback_zsh(line: &str) -> String {
    let quoted = json_string_literal(line);
    format!("sh([\"zsh\",\"-c\", {}]);", quoted)
}

/* =============================================================================
Simple assignment
============================================================================= */

fn parse_simple_assignment(line: &str) -> Option<String> {
    if line.starts_with("export ") || line.starts_with("readonly ") {
        return None;
    }
    // Handle array assignment NAME=(a b c) → let NAME = ["a", "b", "c"];
    if let Some(eq) = line.find('=') {
        let rhs_start = &line[eq + 1..];
        if rhs_start.starts_with('(') && line.ends_with(')') {
            let lhs = line[..eq].trim();
            if !lhs.is_empty() && is_ident(lhs) {
                let inner = &rhs_start[1..rhs_start.len() - 1];
                let elems: Vec<String> = inner
                    .split_whitespace()
                    .map(|w| {
                        let w = w.trim_matches(|c: char| c == '\'' || c == '"');
                        if is_int_literal(w) {
                            w.to_string()
                        } else if is_float_literal(w) {
                            w.to_string()
                        } else {
                            json_string_literal(w)
                        }
                    })
                    .collect();
                return Some(format!("let {} = [{}];", lhs, elems.join(", ")));
            }
        }
    }
    if line.split_whitespace().count() != 1 {
        return None;
    }
    let eq = line.find('=')?;
    let (lhs, rhs_with_eq) = line.split_at(eq);
    let lhs = lhs.trim();
    if lhs.is_empty() || !is_ident(lhs) {
        return None;
    }
    let rhs = rhs_with_eq[1..].trim();

    if rhs.is_empty() || rhs.contains(' ') || contains_zsh_complex(rhs) {
        return None;
    }

    if rhs.starts_with('"') || rhs.starts_with('\'') {
        if let Ok(tok) = parse_single_token(rhs) {
            let expr = render_arg_expr(&tok);
            return Some(format!("let {} = {};", lhs, expr));
        }
        None
    } else if rhs.starts_with('$') {
        parse_var_ref(rhs).map(|var| format!("let {} = {};", lhs, var))
    } else if rhs.starts_with('(') && rhs.ends_with(')') {
        let inner = &rhs[1..rhs.len() - 1];
        let elems: Vec<String> = inner
            .split_whitespace()
            .map(|w| {
                let w = w.trim_matches(|c: char| c == '\'' || c == '"');
                if is_int_literal(w) {
                    w.to_string()
                } else if is_float_literal(w) {
                    w.to_string()
                } else {
                    json_string_literal(w)
                }
            })
            .collect();
        Some(format!("let {} = [{}];", lhs, elems.join(", ")))
    } else if is_int_literal(rhs) {
        Some(format!("let {} = {};", lhs, rhs))
    } else if is_float_literal(rhs) {
        Some(format!("let {} = {};", lhs, rhs))
    } else {
        Some(format!("let {} = {};", lhs, json_string_literal(rhs)))
    }
}
/* =============================================================================
Pipeline parsing
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
    for ch in s.chars() {
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
    let mut literal_sq = None::<String>;
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
                    toks.push(Token::Single(literal_sq.take().unwrap_or_default()));
                    sq = false;
                }
            }
            '"' if !sq => {
                dq = !dq;
                if !dq && !cur.is_empty() {
                    toks.push(Token::Interp(std::mem::take(&mut cur)));
                }
            }
            c if sq => {
                if let Some(ref mut lit) = literal_sq {
                    lit.push(c);
                }
            }
            c if dq => {
                if c == '$' {
                    if let Some((var, _)) = read_var(&mut chars) {
                        cur.push(Piece::Var(var));
                    } else {
                        cur.push(Piece::Text("$".into()));
                    }
                } else {
                    cur.push(Piece::Text(c.to_string()));
                }
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    toks.push(Token::Interp(std::mem::take(&mut cur)));
                }
            }
            '$' => {
                if let Some((var, _)) = read_var(&mut chars) {
                    cur.push(Piece::Var(var));
                } else {
                    cur.push(Piece::Text("$".into()));
                }
            }
            _ => {
                cur.push(Piece::Text(ch.to_string()));
            }
        }
    }
    if sq {
        toks.push(Token::Single(literal_sq.take().unwrap_or_default()));
    }
    if !cur.is_empty() {
        toks.push(Token::Interp(cur));
    }
    Ok(toks)
}

fn parse_single_token(s: &str) -> Result<Token> {
    let toks = split_shell_words(s)?;
    if toks.len() == 1 {
        Ok(toks
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Expected single token"))?)
    } else {
        Err(anyhow!("expected single token, got {}", toks.len()))
    }
}

/* =============================================================================
Rendering
============================================================================= */

fn render_pipeline(cmds: &[Command]) -> String {
    cmds.iter()
        .enumerate()
        .map(|(i, c)| render_command(c, i == 0))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_command(cmd: &Command, _is_first: bool) -> String {
    let prog_name = token_to_plain_word(&cmd.program);
    if let Some(name) = prog_name.as_deref() {
        if let Some(builtin) = map_zsh_builtin(name) {
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
    render_external_call(cmd)
}

fn render_external_call(cmd: &Command) -> String {
    let mut arr = Vec::new();
    let prog = token_to_plain_word(&cmd.program).unwrap_or_else(|| token_to_string(&cmd.program));
    arr.push(json_string_literal(&prog));
    for a in &cmd.args {
        arr.push(json_string_literal(&token_to_string(a)));
    }
    format!("sh([{}])", arr.join(", "))
}

fn render_arg_expr(tok: &Token) -> String {
    match tok {
        Token::Single(s) => double_quote_with_escapes(s),
        Token::Interp(pcs) => {
            if pcs.len() == 1 {
                if let Piece::Var(v) = &pcs[0] {
                    if is_ident(v) {
                        return v.clone();
                    }
                }
            }
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
Utilities
============================================================================= */

fn token_to_plain_word(tok: &Token) -> Option<String> {
    match tok {
        Token::Single(_) => None,
        Token::Interp(pcs) => {
            if pcs.iter().all(|p| matches!(p, Piece::Text(_))) {
                let s: String = pcs
                    .iter()
                    .map(|p| match p {
                        Piece::Text(t) => t.as_str(),
                        _ => "",
                    })
                    .collect();
                if !s.is_empty() && !s.chars().any(|c| c.is_whitespace()) {
                    return Some(s);
                }
            }
            None
        }
    }
}

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

fn read_var(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<(String, usize)> {
    let mut consumed = 0usize;
    if let Some('{') = chars.peek().copied() {
        let _ = chars.next();
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
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_int_literal(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn is_float_literal(s: &str) -> bool {
    s.contains('.')
        && s.chars().filter(|c| *c == '.').count() == 1
        && s.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn json_string_literal(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn double_quote_with_escapes(s: &str) -> String {
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

fn map_zsh_builtin(name: &str) -> Option<&'static str> {
    match name {
        "echo" | "printf" => Some("echo"),
        "pwd" => Some("pwd"),
        "cd" | "pushd" | "popd" | "dirs" => Some("cd"),
        "exit" => Some("exit"),
        "clear" => Some("clear"),
        "true" => Some("true"),
        "false" => Some("false"),
        "test" => Some("test"),

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

        "grep" => Some("str.grep"),
        "sed" => Some("str.sed"),
        "awk" => Some("str.awk"),
        "cut" => Some("str.cut"),
        "sort" => Some("sort"),
        "uniq" => Some("arr.unique"),
        "tr" => Some("str.tr"),
        "rev" => Some("str.reverse"),
        "xargs" => Some("map"),

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
        "env" | "printenv" => Some("sys.env"),

        "ps" => Some("proc.list"),
        "kill" => Some("proc.kill"),
        "killall" => Some("proc.killall"),
        "top" | "htop" => Some("monitor.htop"),
        "pgrep" => Some("proc.grep"),
        "pkill" => Some("proc.pkill"),
        "nohup" => Some("proc.spawn"),
        "sleep" => Some("sleep"),
        "wait" => Some("wait"),

        "ping" => Some("net.ping"),
        "curl" => Some("http.get"),
        "wget" => Some("http.download"),
        "dig" | "nslookup" | "host" => Some("net.dns_lookup"),
        "ip" => Some("net.ip"),
        "ifconfig" => Some("net.interfaces"),
        "netstat" | "ss" => Some("net.connections"),
        "traceroute" => Some("net.traceroute"),

        "tar" => Some("archive.tar"),
        "gzip" => Some("archive.gzip"),
        "gunzip" => Some("archive.gunzip"),
        "zip" => Some("archive.zip"),
        "unzip" => Some("archive.unzip"),

        "jq" => Some("json.query"),
        "apt" | "apt-get" | "yum" | "dnf" | "brew" | "pacman" => Some("pkg.install"),
        "docker" => Some("docker.run"),
        "podman" => Some("podman.run"),
        "kubectl" => Some("k8s.exec"),
        "git" => Some("sh"),
        "which" | "type" => Some("sys.which"),
        "man" => Some("help"),
        "history" => Some("history"),
        "alias" => Some("alias"),
        "export" => Some("set_env"),
        "source" | "." => Some("source"),
        "read" => Some("input.readline"),

        // Zsh-specific
        "print" => Some("echo"),
        "whence" => Some("sys.which"),
        "vared" => Some("input.readline"),
        "rehash" => Some("noop"),  // cache rebuild — no Aether equivalent
        "emulate" => Some("noop"), // shell emulation mode — no Aether equivalent

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_command() {
        let r = transpile_zsh_to_ae("echo hello").unwrap();
        assert!(r.contains("echo(\"hello\")"), "got: {}", r);
    }

    #[test]
    fn test_assignment() {
        let r = transpile_zsh_to_ae("FOO=bar").unwrap();
        assert!(r.contains("let FOO ="), "got: {}", r);
    }

    #[test]
    fn test_array_literal() {
        let r = transpile_zsh_to_ae("arr=(a b c)").unwrap();
        assert!(r.contains("let arr = ["), "got: {}", r);
    }

    #[test]
    fn test_setopt_comment() {
        let r = transpile_zsh_to_ae("setopt extended_glob").unwrap();
        assert!(r.contains("// [zsh]"), "got: {}", r);
    }

    #[test]
    fn test_pipeline() {
        let r = transpile_zsh_to_ae("ls | grep foo").unwrap();
        assert!(r.contains("ls()") && r.contains("|"), "got: {}", r);
    }

    #[test]
    fn test_export() {
        let r = transpile_zsh_to_ae("export PATH=/usr/bin").unwrap();
        assert!(r.contains("let PATH ="), "got: {}", r);
    }

    #[test]
    fn test_local_var() {
        let r = transpile_zsh_to_ae("local x=42").unwrap();
        assert!(r.contains("let x = 42;"), "got: {}", r);
    }

    #[test]
    fn test_if_block() {
        let script = "if true; then\n  echo yes\nfi";
        let r = transpile_zsh_to_ae(script).unwrap();
        assert!(r.contains("sh([\"zsh\",\"-c\""), "got: {}", r);
    }

    #[test]
    fn test_comment_preserved() {
        let r = transpile_zsh_to_ae("# hello world").unwrap();
        assert!(r.contains("// hello world"), "got: {}", r);
    }

    #[test]
    fn test_typeset_assoc() {
        let r = transpile_zsh_to_ae("typeset -A mymap").unwrap();
        assert!(r.contains("let mymap = {};"), "got: {}", r);
    }
}
