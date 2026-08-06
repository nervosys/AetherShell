//! A structural guard against reintroducing findings 10a and 10c.
//!
//! Those findings were fixed by routing every interpolated value through
//! `safety::ps_quote` or `safety::applescript_quote`. Nothing stopped the *next*
//! contributor — or the next refactor — from writing `format!("Start-Service
//! '{}'", name)` again, which is how the defect got in twice: escaping was
//! inconsistent rather than absent, so the pattern looked handled on review.
//!
//! This scans the source for the shape of the bug rather than relying on anyone
//! noticing it. It is a lint, so it is heuristic by nature: it looks for a
//! quoted `{}` placeholder on a line that also looks like a PowerShell or
//! AppleScript command. False positives are fixed by using the helper (which is
//! what you wanted anyway) or, if genuinely not a shell string, by adding the
//! line's distinguishing text to `ALLOWED`.
//!
//! Why a source scan and not a type: the correct fix is a newtype that only the
//! quoting helpers can construct, so a raw `String` cannot reach a command
//! builder at all. That is a larger refactor across ~117 PowerShell sites. This
//! holds the line until then, and fails loudly if it slips.

use std::path::PathBuf;

/// Substrings that mark a line as *not* shell-command construction, even though
/// it contains a quoted placeholder. These are overwhelmingly error messages.
const ALLOWED: &[&str] = &[
    "anyhow!",
    "arg_err",
    "bad_arg",
    "E_",
    "unsupported operator",
    "unknown ",
    "invalid ",
    "no such ",
    "does not exist",
    "not provided",
    "requires ",
    "Cannot convert",
    "failed to",
    "outside the workspace",
    "no backend found",
    "cannot be",
    "expected ",
    "[SECURITY]",
];

/// Tokens that indicate the line is building a PowerShell or AppleScript
/// command, where an unquoted interpolation is a command-injection vector.
const SHELL_MARKERS: &[&str] = &[
    "Get-",
    "Set-",
    "New-",
    "Remove-",
    "Start-",
    "Stop-",
    "Restart-",
    "Add-",
    "Invoke-",
    "Compress-",
    "Expand-",
    "Where-Object",
    "Select-Object",
    "ConvertTo-",
    "ConvertFrom-",
    "Read-Host",
    "Write-Output",
    "FindWindow",
    "System.",
    "display notification",
    "display dialog",
    "do shell script",
    "$_.",
    "$env:",
];

fn source(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Does a `{}` sit *inside* a single-quoted PowerShell string on this line,
/// rather than filling one exactly?
///
/// The first version of this lint matched only the exact shapes `'{}'` and
/// `"{}"`. That blind spot was not theoretical: it let four live injections
/// through — `-like '*{}*'` in `net.ip_addresses` and `net.adapters`,
/// `-ArgumentList '/C {}'` in `timeout`, and `-like '*{}*'` in `log.search` —
/// because each *embeds* the placeholder in a quoted string instead of being
/// one. A value containing `'` escapes the string in every one of them.
///
/// Restricted to single quotes on purpose. A `"` on these lines is usually the
/// Rust literal's own delimiter, so pairing across it would flag the correct
/// `-Id {}` numeric form; `'` is unambiguously PowerShell's.
fn placeholder_embedded_in_single_quotes(line: &str) -> bool {
    let mut search_from = 0;
    while let Some(rel) = line[search_from..].find("{}") {
        let at = search_from + rel;
        let left = line[..at].rfind('\'');
        let right = line[at + 2..].find('\'').map(|r| at + 2 + r);
        if let (Some(l), Some(r)) = (left, right) {
            let before = &line[l + 1..at];
            let after = &line[at + 2..r];
            // Crossing a comma or a Rust string boundary means these two quotes
            // are not a matched pair around this placeholder.
            let crosses = |s: &str| s.contains(',') || s.contains('"') || s.contains('{');
            if !crosses(before) && !crosses(after) {
                return true;
            }
        }
        search_from = at + 2;
    }
    false
}

/// A line is suspect if it interpolates directly inside quotes *and* looks like
/// a shell command.
fn is_suspect(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("*") {
        return false;
    }
    // The injectable shapes: '{}' for PowerShell single-quoted, "{}" (as
    // \"{}\" in a Rust literal, or bare inside a raw string) for double-quoted
    // PowerShell and AppleScript, and a placeholder embedded in a single-quoted
    // string such as '*{}*'.
    let has_quoted_placeholder = trimmed.contains("'{}'")
        || trimmed.contains("\\\"{}\\\"")
        || trimmed.contains("\"{}\"")
        || placeholder_embedded_in_single_quotes(trimmed);
    if !has_quoted_placeholder {
        return false;
    }
    if ALLOWED.iter().any(|a| trimmed.contains(a)) {
        return false;
    }
    SHELL_MARKERS.iter().any(|m| trimmed.contains(m))
}

#[test]
fn no_shell_command_interpolates_a_value_without_quoting_it() {
    let mut offenders = Vec::new();

    for file in ["src/builtins.rs", "src/safety.rs", "src/os_tools.rs"] {
        for (i, line) in source(file).lines().enumerate() {
            if is_suspect(line) {
                offenders.push(format!("  {}:{}\n    {}", file, i + 1, line.trim()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "shell command(s) interpolate a value directly into a quoted literal — a value \
         containing a quote (PowerShell/AppleScript) or `$(…)` (double-quoted PowerShell) \
         executes.\n\nUse `safety::ps_quote(&v)` or `safety::applescript_quote(&v)` and \
         interpolate with `{{}}` rather than `'{{}}'`, so the helper supplies the quotes.\n\n\
         If the line is genuinely not a shell string, add a distinguishing substring to \
         ALLOWED in this test.\n\n{}",
        offenders.join("\n")
    );
}

/// Walk backwards from `line_idx` to the nearest enclosing macro opener and
/// return its name.
///
/// Crude but sufficient: these are all `format!(`/`ps_script!(` immediately
/// followed by the template, so the nearest preceding opener is the enclosing
/// one. A `None` means the text was not inside any of the three.
fn enclosing_macro(lines: &[&str], line_idx: usize) -> Option<&'static str> {
    for l in lines[..=line_idx].iter().rev().take(40) {
        if l.contains("ps_script!(") {
            return Some("ps_script!");
        }
        if l.contains("applescript!(") {
            return Some("applescript!");
        }
        if l.contains("format!(") {
            return Some("format!");
        }
    }
    None
}

/// A PowerShell command built with a value must go through `ps_script!`, which
/// type-checks its arguments, not through bare `format!`, which accepts
/// anything `Display`.
///
/// This is the check that makes the macro effectively mandatory. It is the
/// text-level approximation of a dataflow question ("does this string reach
/// `Command::new("powershell")`?") that a scan cannot answer properly — so it
/// errs toward flagging, and `ALLOWED` absorbs the false positives.
///
/// Worth stating plainly: `format!` here is not *itself* a vulnerability. It is
/// the absence of the type check that caught `vm.create`'s unquoted
/// `-MemoryStartupBytes {}` when three manual passes and the quoted-placeholder
/// lint had all missed it.
#[test]
fn powershell_commands_with_values_use_the_checked_macro() {
    let mut offenders = Vec::new();

    for file in ["src/builtins.rs", "src/os_tools.rs"] {
        let text = source(file);
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if t.starts_with("//") || t.starts_with("*") {
                continue;
            }
            // A shell command with an interpolation in it.
            if !t.contains("{}") || !SHELL_MARKERS.iter().any(|m| t.contains(m)) {
                continue;
            }
            if ALLOWED.iter().any(|a| t.contains(a)) {
                continue;
            }
            if enclosing_macro(&lines, i) == Some("format!") {
                offenders.push(format!("  {}:{}\n    {}", file, i + 1, t));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "PowerShell/AppleScript command(s) built with `format!`, which accepts any \
         `Display` value — including a caller-supplied `String`.\n\nUse \
         `crate::ps_script!` (or `crate::applescript!`), which accepts only \
         pre-escaped literals, integers, and `&'static str`, so an unescaped value \
         is a compile error.\n\nFor a value that must be interpolated *unquoted* \
         (a size or a port), validate it with `safety::ps_bare_number`.\n\n{}",
        offenders.join("\n")
    );
}

/// The guard must actually fire — a lint that cannot fail is worse than none,
/// because it reads as coverage.
#[test]
fn the_guard_detects_the_shape_it_is_meant_to_catch() {
    assert!(
        is_suspect(r#"            .args(["-Command", &format!("Start-Service '{}'", name)])"#),
        "the exact pre-fix shape of finding 10a must be flagged"
    );
    assert!(
        is_suspect(r#"$bytes = [System.Text.Encoding]::UTF8.GetBytes("{}")"#),
        "the pre-fix shape of finding 10c must be flagged"
    );
    assert!(
        is_suspect(r#""display notification \"{}\" with title \"{}\"","#),
        "the AppleScript shape must be flagged"
    );

    // The embedded shapes, which the exact-match version of this lint could not
    // see. All four were live injections found only by the macro check below.
    assert!(
        is_suspect(
            r#"format!("Get-NetIPAddress | Where-Object {{ $_.InterfaceAlias -like '*{}*' }}", iface)"#
        ),
        "a placeholder embedded in a single-quoted -like pattern must be flagged"
    );
    assert!(
        is_suspect(
            r#"format!("$p = Start-Process -FilePath cmd -ArgumentList '/C {}' -PassThru", command)"#
        ),
        "a placeholder embedded in a single-quoted -ArgumentList must be flagged"
    );

    // But an unquoted numeric interpolation is the correct form and must stay
    // clear, or the lint would push callers back toward quoting numbers.
    assert!(!is_suspect(
        r#"            &format!("Get-Process -Id {} -ErrorAction SilentlyContinue", pid),"#
    ));
    assert!(!is_suspect(
        r#"        let cmd = format!("(Get-Process -Id {}).PriorityClass", pid);"#
    ));

    // And must not fire on the fixed form, or it would block the correct fix.
    assert!(!is_suspect(
        r#"            .args(["-Command", &format!("Start-Service {}", crate::safety::ps_quote(&name))])"#
    ));
    assert!(!is_suspect(
        r#"$bytes = [System.Text.Encoding]::UTF8.GetBytes({})"#
    ));
    // Nor on ordinary error messages.
    assert!(!is_suspect(
        r#"return Err(anyhow!("sess_eval: no such session '{}'", id));"#
    ));
}
