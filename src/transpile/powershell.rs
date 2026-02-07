//! PowerShell → Aether Shell transpiler (compat mode).
//!
//! This is a pragmatic, line-oriented transpiler for a *useful subset* of PowerShell:
//!   - simple commands (cmdlets), args, and pipelines (`|`)
//!   - single quotes:  'literal string'
//!   - double quotes:  expand $V and ${V} as Aether string interpolations `${V}`
//!   - $VAR as a standalone arg becomes an identifier `VAR`
//!   - simple assignments: $NAME = value  →  let NAME = <expr>
//!   - common cmdlets mapped to Aether builtins
//!
//! Fallback: if a line contains constructs we don't yet handle,
//! we emit:  sh(["pwsh","-c", "<original line>"])
//!
//! Output: Aether source as a String.

use anyhow::Result;

/// Transpile a whole PowerShell script (multi-line) to Aether code.
pub fn transpile_powershell_to_ae(src: &str) -> Result<String> {
    let mut out = String::new();
    out.push_str("// Transpiled from PowerShell → Aether (compat mode)\n");

    let mut in_multiline_string = false;
    let mut multiline_buffer = String::new();

    for raw_line in src.lines() {
        let line = raw_line.trim();

        // Handle here-strings @" ... "@
        if in_multiline_string {
            if line == "\"@" || line == "'@" {
                in_multiline_string = false;
                // Process the multiline string
                out.push_str(&format!("\"{}\"", escape_string(&multiline_buffer)));
                out.push('\n');
                multiline_buffer.clear();
            } else {
                multiline_buffer.push_str(raw_line);
                multiline_buffer.push('\n');
            }
            continue;
        }

        if line.contains("@\"") || line.contains("@'") {
            in_multiline_string = true;
            multiline_buffer.clear();
            continue;
        }

        if line.is_empty() {
            continue;
        }

        // PowerShell comments
        if line.starts_with('#') {
            out.push_str(&format!("// {}\n", &line[1..].trim_start()));
            continue;
        }

        // Block comments <# ... #>
        if line.starts_with("<#") {
            out.push_str(&format!(
                "// {}\n",
                line.trim_start_matches("<#").trim_end_matches("#>").trim()
            ));
            continue;
        }

        // Skip if contains complex constructs that need fallback
        if contains_complex_construct(line) {
            out.push_str(&render_fallback_pwsh(line));
            out.push('\n');
            continue;
        }

        // Try simple assignment: $VAR = value
        if let Some(assign) = parse_ps_assignment(line) {
            out.push_str(&assign);
            out.push('\n');
            continue;
        }

        // Try to parse as a pipeline of commands
        match parse_ps_pipeline(line) {
            Ok(cmds) if !cmds.is_empty() => {
                let aether_line = render_ps_pipeline(&cmds);
                out.push_str(&aether_line);
                out.push('\n');
            }
            _ => {
                // Fallback if we failed to parse robustly
                out.push_str(&render_fallback_pwsh(line));
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
struct PsCommand {
    cmdlet: Token,    // The cmdlet or command name
    args: Vec<Token>, // Arguments (may include -Parameters)
}

#[derive(Debug, Clone)]
enum Token {
    /// Normal / double-quoted token with possible var expansions
    Interp(Vec<Piece>),
    /// Single-quoted string with no expansion
    Single(String),
    /// Parameter like -Name or -Path
    Parameter(String),
    /// Splatted variable @args
    Splat(String),
}

#[derive(Debug, Clone)]
enum Piece {
    Text(String),
    Var(String), // VAR name (no leading '$')
}

/* =============================================================================
Complexity detection
============================================================================= */

fn contains_complex_construct(s: &str) -> bool {
    // Detect PowerShell constructs we don't handle yet
    s.contains("$(")           // Subexpression
        || s.contains("`(")    // Escaped parens
        || s.contains(">>")    // Append redirection
        || s.contains("2>&1")  // Error redirect
        || s.contains("*>")    // All streams redirect
        || s.starts_with("if ")
        || s.starts_with("if(")
        || s.starts_with("foreach ")
        || s.starts_with("foreach(")
        || s.starts_with("for ")
        || s.starts_with("for(")
        || s.starts_with("while ")
        || s.starts_with("while(")
        || s.starts_with("do ")
        || s.starts_with("switch ")
        || s.starts_with("switch(")
        || s.starts_with("try ")
        || s.starts_with("try{")
        || s.starts_with("function ")
        || s.starts_with("class ")
        || s.starts_with("enum ")
        || s.contains(" -join ")
        || s.contains(" -split ")
        || s.contains(" -match ")
        || s.contains(" -replace ")
        || s.contains(" -contains ")
        || s.contains(" -in ")
        || s.contains(" -notin ")
        || s.contains("[")     // Type casts/attributes
        || s.contains("::")    // Static method calls
        || s.contains("..") // Range operator
}

fn render_fallback_pwsh(line: &str) -> String {
    let quoted = json_string_literal(line);
    format!("sh([\"pwsh\", \"-c\", {}]);", quoted)
}

/* =============================================================================
Assignment parsing
============================================================================= */

fn parse_ps_assignment(line: &str) -> Option<String> {
    // Match: $VarName = expression
    let trimmed = line.trim();

    // Must start with $
    if !trimmed.starts_with('$') {
        return None;
    }

    // Find the = sign
    let eq_pos = trimmed.find('=')?;

    // Check for compound assignment operators
    let before_eq = &trimmed[..eq_pos];
    if before_eq.ends_with('+')
        || before_eq.ends_with('-')
        || before_eq.ends_with('*')
        || before_eq.ends_with('/')
    {
        return None; // +=, -=, etc. - fallback
    }

    let var_part = trimmed[1..eq_pos].trim();
    let val_part = trimmed[eq_pos + 1..].trim();

    // Validate variable name
    if !is_ps_ident(var_part) {
        return None;
    }

    // Parse the value
    if val_part.is_empty() {
        return Some(format!("let {} = null;", var_part));
    }

    // Handle different value types
    if val_part.starts_with('"') {
        // Double-quoted string with potential interpolation
        if let Ok(tok) = parse_ps_token(val_part) {
            let expr = render_token_expr(&tok);
            return Some(format!("let {} = {};", var_part, expr));
        }
    } else if val_part.starts_with('\'') {
        // Single-quoted literal string
        if let Ok(tok) = parse_ps_token(val_part) {
            let expr = render_token_expr(&tok);
            return Some(format!("let {} = {};", var_part, expr));
        }
    } else if val_part.starts_with('$') {
        // Variable reference
        if let Some(var) = parse_ps_var_ref(val_part) {
            return Some(format!("let {} = {};", var_part, var));
        }
    } else if val_part.starts_with("@(") {
        // Array literal - simplified handling
        if let Some(end) = val_part.rfind(')') {
            let inner = &val_part[2..end];
            let elements: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            let ae_elements: Vec<String> = elements.iter().map(|e| parse_simple_value(e)).collect();
            return Some(format!("let {} = [{}];", var_part, ae_elements.join(", ")));
        }
    } else if val_part.starts_with("@{") {
        // Hashtable - simplified handling
        return None; // Complex, use fallback
    } else if is_ps_number(val_part) {
        return Some(format!("let {} = {};", var_part, val_part));
    } else if val_part == "$true" || val_part.eq_ignore_ascii_case("$true") {
        return Some(format!("let {} = true;", var_part));
    } else if val_part == "$false" || val_part.eq_ignore_ascii_case("$false") {
        return Some(format!("let {} = false;", var_part));
    } else if val_part == "$null" || val_part.eq_ignore_ascii_case("$null") {
        return Some(format!("let {} = null;", var_part));
    } else {
        // Treat as bare string
        return Some(format!(
            "let {} = \"{}\";",
            var_part,
            escape_string(val_part)
        ));
    }

    None
}

fn parse_simple_value(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') {
        s.to_string()
    } else if s.starts_with('\'') && s.ends_with('\'') {
        format!("\"{}\"", &s[1..s.len() - 1])
    } else if s.starts_with('$') {
        s[1..].to_string()
    } else if is_ps_number(s) {
        s.to_string()
    } else {
        format!("\"{}\"", escape_string(s))
    }
}

/* =============================================================================
Pipeline parsing
============================================================================= */

fn parse_ps_pipeline(s: &str) -> Result<Vec<PsCommand>> {
    let parts = split_ps_pipes(s)?;
    let mut cmds = Vec::new();

    for p in parts {
        let toks = split_ps_words(&p)?;
        if toks.is_empty() {
            continue;
        }
        let cmdlet = toks[0].clone();
        let args = toks[1..].to_vec();
        cmds.push(PsCommand { cmdlet, args });
    }

    Ok(cmds)
}

fn split_ps_pipes(s: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut dq = false;
    let mut sq = false;
    let mut paren_depth: usize = 0;

    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        // Handle escape character
        if ch == '`' && i + 1 < chars.len() {
            cur.push(ch);
            cur.push(chars[i + 1]);
            i += 2;
            continue;
        }

        if ch == '"' && !sq {
            dq = !dq;
            cur.push(ch);
        } else if ch == '\'' && !dq {
            sq = !sq;
            cur.push(ch);
        } else if ch == '(' && !dq && !sq {
            paren_depth += 1;
            cur.push(ch);
        } else if ch == ')' && !dq && !sq {
            paren_depth = paren_depth.saturating_sub(1);
            cur.push(ch);
        } else if ch == '|' && !dq && !sq && paren_depth == 0 {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }

        i += 1;
    }

    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }

    Ok(out)
}

fn split_ps_words(s: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_dq = false;
    let mut in_sq = false;

    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        // Handle escape character (backtick in PowerShell)
        if ch == '`' && i + 1 < chars.len() {
            cur.push(chars[i + 1]);
            i += 2;
            continue;
        }

        if ch == '"' && !in_sq {
            if in_dq {
                // End of double-quoted string
                cur.push(ch);
                in_dq = false;
            } else {
                // Start of double-quoted string
                if !cur.is_empty() {
                    tokens.push(parse_ps_token(&cur)?);
                    cur.clear();
                }
                cur.push(ch);
                in_dq = true;
            }
        } else if ch == '\'' && !in_dq {
            if in_sq {
                // End of single-quoted string
                cur.push(ch);
                in_sq = false;
            } else {
                // Start of single-quoted string
                if !cur.is_empty() {
                    tokens.push(parse_ps_token(&cur)?);
                    cur.clear();
                }
                cur.push(ch);
                in_sq = true;
            }
        } else if ch.is_whitespace() && !in_dq && !in_sq {
            if !cur.is_empty() {
                tokens.push(parse_ps_token(&cur)?);
                cur.clear();
            }
        } else {
            cur.push(ch);
        }

        i += 1;
    }

    if !cur.is_empty() {
        tokens.push(parse_ps_token(&cur)?);
    }

    Ok(tokens)
}

fn parse_ps_token(s: &str) -> Result<Token> {
    let s = s.trim();

    if s.is_empty() {
        return Ok(Token::Single(String::new()));
    }

    // Parameter (starts with -)
    if s.starts_with('-') && s.len() > 1 {
        let param_name = &s[1..];
        if is_ps_ident(param_name) || param_name.chars().all(|c| c.is_alphanumeric()) {
            return Ok(Token::Parameter(param_name.to_string()));
        }
    }

    // Splatted variable
    if s.starts_with('@') && s.len() > 1 {
        let var_name = &s[1..];
        if is_ps_ident(var_name) {
            return Ok(Token::Splat(var_name.to_string()));
        }
    }

    // Single-quoted string
    if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        return Ok(Token::Single(inner.to_string()));
    }

    // Double-quoted string with variable interpolation
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        let pieces = parse_ps_interpolation(inner)?;
        return Ok(Token::Interp(pieces));
    }

    // Bare word or variable
    if s.starts_with('$') {
        let var_name = &s[1..];
        if is_ps_ident(var_name) {
            return Ok(Token::Interp(vec![Piece::Var(var_name.to_string())]));
        }
    }

    // Plain text
    Ok(Token::Interp(vec![Piece::Text(s.to_string())]))
}

fn parse_ps_interpolation(s: &str) -> Result<Vec<Piece>> {
    let mut pieces = Vec::new();
    let mut cur_text = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        // Escape character
        if ch == '`' && i + 1 < chars.len() {
            cur_text.push(chars[i + 1]);
            i += 2;
            continue;
        }

        // Variable interpolation
        if ch == '$' {
            // Save accumulated text
            if !cur_text.is_empty() {
                pieces.push(Piece::Text(cur_text.clone()));
                cur_text.clear();
            }

            // Parse variable name
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                // ${variable} form
                let start = i + 2;
                if let Some(end) = s[start..].find('}') {
                    let var_name = &s[start..start + end];
                    if is_ps_ident(var_name) {
                        pieces.push(Piece::Var(var_name.to_string()));
                        i = start + end + 1;
                        continue;
                    }
                }
            } else {
                // $variable form
                let mut var_name = String::new();
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    var_name.push(chars[j]);
                    j += 1;
                }
                if !var_name.is_empty() && is_ps_ident(&var_name) {
                    pieces.push(Piece::Var(var_name));
                    i = j;
                    continue;
                }
            }

            cur_text.push(ch);
        } else {
            cur_text.push(ch);
        }

        i += 1;
    }

    if !cur_text.is_empty() {
        pieces.push(Piece::Text(cur_text));
    }

    Ok(pieces)
}

/* =============================================================================
Rendering
============================================================================= */

fn render_ps_pipeline(cmds: &[PsCommand]) -> String {
    if cmds.len() == 1 {
        return render_ps_command(&cmds[0]) + ";";
    }

    let parts: Vec<String> = cmds.iter().map(render_ps_command).collect();
    parts.join(" | ") + ";"
}

fn render_ps_command(cmd: &PsCommand) -> String {
    let cmdlet_name = match &cmd.cmdlet {
        Token::Interp(pieces) => {
            if pieces.len() == 1 {
                if let Piece::Text(t) = &pieces[0] {
                    t.clone()
                } else {
                    render_token_expr(&cmd.cmdlet)
                }
            } else {
                render_token_expr(&cmd.cmdlet)
            }
        }
        Token::Single(s) => s.clone(),
        Token::Parameter(p) => format!("-{}", p),
        Token::Splat(s) => format!("@{}", s),
    };

    // Map common PowerShell cmdlets to Aether builtins
    if let Some(mapped) = map_ps_cmdlet(&cmdlet_name) {
        let args_str = render_ps_args(&cmd.args);
        if args_str.is_empty() {
            return mapped.to_string();
        } else {
            return format!("{}({})", mapped, args_str);
        }
    }

    // Generic command - use sh() wrapper
    let mut all_args = vec![json_string_literal(&cmdlet_name)];
    for arg in &cmd.args {
        all_args.push(render_token_as_sh_arg(arg));
    }
    format!("sh([{}])", all_args.join(", "))
}

fn render_ps_args(args: &[Token]) -> String {
    // Convert PowerShell args to Aether function args
    let mut result = Vec::new();
    let mut i = 0;

    while i < args.len() {
        match &args[i] {
            Token::Parameter(param) => {
                // Check if next arg is a value
                if i + 1 < args.len() {
                    match &args[i + 1] {
                        Token::Parameter(_) => {
                            // Switch parameter (no value)
                            result.push(format!("{}: true", param));
                        }
                        _ => {
                            // Parameter with value
                            let value = render_token_expr(&args[i + 1]);
                            result.push(format!("{}: {}", param, value));
                            i += 1;
                        }
                    }
                } else {
                    // Last arg is a switch
                    result.push(format!("{}: true", param));
                }
            }
            _ => {
                result.push(render_token_expr(&args[i]));
            }
        }
        i += 1;
    }

    result.join(", ")
}

fn render_token_expr(tok: &Token) -> String {
    match tok {
        Token::Single(s) => format!("\"{}\"", escape_string(s)),
        Token::Parameter(p) => format!("\"-{}\"", p),
        Token::Splat(s) => s.clone(), // Pass as variable
        Token::Interp(pieces) => {
            if pieces.len() == 1 {
                match &pieces[0] {
                    Piece::Text(t) => format!("\"{}\"", escape_string(t)),
                    Piece::Var(v) => v.clone(),
                }
            } else {
                // Build interpolated string
                let mut parts = Vec::new();
                for piece in pieces {
                    match piece {
                        Piece::Text(t) => parts.push(format!("\"{}\"", escape_string(t))),
                        Piece::Var(v) => parts.push(format!("str({})", v)),
                    }
                }
                format!("({})", parts.join(" + "))
            }
        }
    }
}

fn render_token_as_sh_arg(tok: &Token) -> String {
    match tok {
        Token::Single(s) => json_string_literal(s),
        Token::Parameter(p) => json_string_literal(&format!("-{}", p)),
        Token::Splat(_) => "\"@args\"".to_string(), // Simplified
        Token::Interp(pieces) => {
            if pieces.len() == 1 {
                match &pieces[0] {
                    Piece::Text(t) => json_string_literal(t),
                    Piece::Var(v) => v.clone(), // Variable reference
                }
            } else {
                // Complex interpolation - render as expression
                render_token_expr(tok)
            }
        }
    }
}

/* =============================================================================
Cmdlet mapping
============================================================================= */

fn map_ps_cmdlet(name: &str) -> Option<&'static str> {
    // Map common PowerShell cmdlets to Aether builtins
    // Case-insensitive matching
    match name.to_lowercase().as_str() {
        "write-host" | "write-output" | "echo" => Some("echo"),
        "get-location" | "pwd" => Some("pwd"),
        "set-location" | "cd" => Some("cd"),
        "get-childitem" | "dir" | "ls" | "gci" => Some("ls"),
        "get-content" | "cat" | "type" | "gc" => Some("read_text"),
        "set-content" | "sc" => Some("write_text"),
        "add-content" | "ac" => Some("append_text"),
        "copy-item" | "copy" | "cp" | "cpi" => Some("cp"),
        "move-item" | "move" | "mv" | "mi" => Some("mv"),
        "remove-item" | "del" | "rm" | "ri" | "erase" => Some("rm"),
        "new-item" | "ni" => Some("touch"),
        "test-path" => Some("exists"),
        "get-process" | "ps" | "gps" => Some("proc_list"),
        "stop-process" | "kill" | "spps" => Some("proc_kill"),
        "start-process" | "saps" => Some("proc_spawn"),
        "get-date" => Some("now"),
        "get-random" => Some("random"),
        "measure-object" => Some("len"),
        "select-object" | "select" => Some("select"),
        "where-object" | "where" | "?" => Some("where"),
        "foreach-object" | "foreach" | "%" => Some("map"),
        "sort-object" | "sort" => Some("sort"),
        "group-object" | "group" => Some("group"),
        "convertto-json" => Some("to_json"),
        "convertfrom-json" => Some("from_json"),
        "invoke-webrequest" | "curl" | "wget" | "iwr" => Some("http_get"),
        "invoke-restmethod" | "irm" => Some("http_request"),
        "get-clipboard" | "gcb" => Some("clip_get"),
        "set-clipboard" | "scb" => Some("clip_set"),
        "get-service" | "gsv" => Some("svc_list"),
        "start-service" | "sasv" => Some("svc_start"),
        "stop-service" | "spsv" => Some("svc_stop"),
        "restart-service" => Some("svc_restart"),
        "get-host" => Some("sys_info"),
        "hostname" => Some("sys_hostname"),
        "clear-host" | "cls" | "clear" => Some("clear"),
        "exit" => Some("exit"),
        _ => None,
    }
}

/* =============================================================================
Helper functions
============================================================================= */

fn parse_ps_var_ref(s: &str) -> Option<String> {
    if s.starts_with("${") {
        if let Some(end) = s.find('}') {
            let name = &s[2..end];
            if is_ps_ident(name) {
                return Some(name.to_string());
            }
        }
    } else if let Some(name) = s.strip_prefix('$') {
        let name = name.trim();
        if is_ps_ident(name) {
            return Some(name.to_string());
        }
    }
    None
}

fn is_ps_ident(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
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

fn is_ps_number(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let s = s.trim();

    // Handle negative numbers
    let s = if s.starts_with('-') { &s[1..] } else { s };

    // Integer
    if s.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }

    // Float
    if s.contains('.') {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() == 2 {
            return parts[0].chars().all(|c| c.is_ascii_digit())
                && parts[1].chars().all(|c| c.is_ascii_digit());
        }
    }

    false
}

fn escape_string(s: &str) -> String {
    let mut out = String::new();
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
    out
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
            c if c.is_control() => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/* =============================================================================
Tests
============================================================================= */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_assignment() {
        let result = transpile_powershell_to_ae("$x = 42").unwrap();
        assert!(result.contains("let x = 42;"));
    }

    #[test]
    fn test_string_assignment() {
        let result = transpile_powershell_to_ae("$name = \"hello\"").unwrap();
        assert!(result.contains("let name = \"hello\";"));
    }

    #[test]
    fn test_pipeline() {
        let result =
            transpile_powershell_to_ae("Get-Process | Where-Object { $_.CPU -gt 10 }").unwrap();
        // Should transpile to Aether pipeline or fall back if script blocks not supported
        assert!(
            result.contains("where") || result.contains("sh(") || result.contains("pwsh"),
            "Expected pipeline transpilation or fallback, got: {}",
            result
        );
    }

    #[test]
    fn test_simple_cmdlet() {
        let result = transpile_powershell_to_ae("Get-ChildItem").unwrap();
        assert!(result.contains("ls"));
    }

    #[test]
    fn test_cmdlet_with_path() {
        let result = transpile_powershell_to_ae("Get-Content 'file.txt'").unwrap();
        assert!(result.contains("read_text"));
    }
}
