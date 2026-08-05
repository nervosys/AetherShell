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

/// A line is suspect if it interpolates directly inside quotes *and* looks like
/// a shell command.
fn is_suspect(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("*") {
        return false;
    }
    // The two injectable shapes: '{}' for PowerShell single-quoted, "{}" (as
    // \"{}\" in a Rust literal, or bare inside a raw string) for double-quoted
    // PowerShell and AppleScript.
    let has_quoted_placeholder =
        trimmed.contains("'{}'") || trimmed.contains("\\\"{}\\\"") || trimmed.contains("\"{}\"");
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
