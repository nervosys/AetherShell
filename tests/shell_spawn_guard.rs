//! Nothing may hand a caller-controlled string to a shell without gating it.
//!
//! `sh -c` and `cmd /C` take one string and parse it. That is the point of them,
//! and it is why the builtins that legitimately use them — `nohup_run`,
//! `xargs_exec` — call `safety::guard_exec` first: the capability being granted
//! is *arbitrary execution*, so it is classified `Exec` and approved as such.
//!
//! `web_open_url` used the same construction and gated nothing:
//!
//! ```text
//! std::process::Command::new("cmd").args(["/C", "start", &url]).output()?
//! ```
//!
//! It reads as a fixed command with the URL in an argument slot, which is what
//! made it survive review. It is not. Rust quotes an argument only when it
//! contains a space or a quote, and `cmd` splits its command line on `&`, so a
//! URL with neither space nor quote reaches `cmd` verbatim and the text after
//! `&` runs as a second command. Demonstrated against this machine's `cmd.exe`
//! with `http://example.com&echo.>marker.txt`, which created the file; the
//! same payload through `start` created it too.
//!
//! What makes it worse than an ordinary injection is where it sat in the safety
//! model. `effect_of("web_open_url")` is `Network` by the `web_*` prefix rule,
//! `Network` is not in `centrally_enforced()`, and the builtin was not in
//! `SELF_GUARDED` — so in agent mode it was *default-allow*, unmetered, and
//! outside the `AETHER_NET_ALLOW` egress allowlist. A builtin advertised as
//! "open a web page" was an unapproved `Exec`.
//!
//! The fix is structural rather than a filter, because a filter cannot work
//! here: `&` is the query-string separator, so the dangerous character is legal
//! data in exactly the values this builtin exists to take. Refusing it breaks
//! `?a=1&b=2`; allowing it leaves the hole. The Windows branch therefore no
//! longer uses a shell at all. This test keeps the family from regrowing one.

use std::path::PathBuf;

/// Sites that hand a value to a shell and are allowed to, each because the
/// builtin's whole purpose is to run a command and it gates on `Exec` first.
///
/// This list may only shrink. Adding to it asserts that a string reaching a
/// shell parser without an execution gate is correct.
const ALLOWED: &[(&str, &str)] = &[(
    "bi_xargs_exec",
    "runs a caller's command per item; calls guard_exec on the template",
)];

const SOURCES: &[&str] = &[
    "src/builtins.rs",
    "src/safety.rs",
    "src/os_tools.rs",
    "src/security.rs",
];

fn source(rel: &str) -> Option<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(path).ok()
}

/// Is the program on this line one of the parsing shells?
fn is_shell_spawn(line: &str) -> bool {
    ["\"sh\"", "\"bash\"", "\"zsh\"", "\"cmd\"", "\"pwsh\""]
        .iter()
        .any(|p| line.contains(&format!("Command::new({p})")))
        // `nohup_run` picks the shell through a tuple, so the program name is a
        // variable at the spawn site. Catch the tuple instead.
        || line.contains("(\"cmd\", \"/C\")")
}

/// A spawn site: the enclosing function name, plus its body up to the spawn.
fn shell_spawn_sites(src: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let lines: Vec<&str> = src.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with("//") || t.starts_with("*") || !is_shell_spawn(t) {
            continue;
        }
        // The argument list follows within a few lines of the spawn.
        let args: String = lines[i..(i + 6).min(lines.len())].join("\n");
        // Walk back to the enclosing `fn`.
        let (mut name, mut body_start) = ("<unknown fn>".to_string(), 0usize);
        for (j, l) in lines[..=i].iter().enumerate().rev() {
            if l.starts_with("fn ") || l.starts_with("pub fn ") {
                name = l
                    .trim_start_matches("pub ")
                    .trim_start_matches("fn ")
                    .split('(')
                    .next()
                    .unwrap_or("<unknown fn>")
                    .to_string();
                body_start = j;
                break;
            }
        }
        out.push((name, lines[body_start..=i].join("\n"), args));
    }
    out
}

/// Does the shell receive a value, rather than a literal written in the source?
///
/// A literal such as `.args(["-c", "ulimit -a"])` carries no caller input and is
/// not the shape this test is about — not even `"which osx-cpu-temp &&
/// osx-cpu-temp"`, whose `&&` is inside the literal and was the first version of
/// this function's one false positive. A borrowed local (`&url`, `&full_cmd`) or
/// an interpolation is.
fn passes_a_value(args: &str) -> bool {
    let Some(open) = args.find(".args(") else {
        return false;
    };
    let inside = &args[open + ".args(".len()..];
    let inside = inside.split("])").next().unwrap_or(inside);

    let mut in_str = false;
    let mut esc = false;
    let mut depth = 0i32;
    let mut cur = String::new();
    let mut parts = Vec::new();
    for c in inside.chars() {
        if in_str {
            cur.push(c);
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                cur.push(c);
            }
            '(' | '[' | '{' => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth <= 1 => {
                parts.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    parts.push(cur);

    parts.iter().any(|p| {
        let p = p.trim().trim_start_matches(['&', '[']).trim();
        // `&["-c", …]` is a slice borrow, not a value; `&command` is a value.
        p.contains("format!")
            || (!p.starts_with('"')
                && !p.is_empty()
                && p.chars()
                    .next()
                    .is_some_and(|c| c.is_alphabetic() || c == '_'))
    })
}

#[test]
fn every_shell_spawn_that_takes_a_value_gates_on_exec() {
    let mut offenders = Vec::new();
    let mut total = 0usize;

    for file in SOURCES {
        let Some(src) = source(file) else { continue };
        for (name, body, args) in shell_spawn_sites(&src) {
            total += 1;
            if !passes_a_value(&args) {
                continue;
            }
            if body.contains("guard_exec") {
                continue;
            }
            if ALLOWED.iter().any(|(n, _)| *n == name) {
                continue;
            }
            offenders.push(format!("  {file}: {name}"));
        }
    }

    assert!(
        total >= 6,
        "only {total} shell spawn sites found across {SOURCES:?}; the scanner has \
         drifted and this test is checking almost nothing"
    );
    assert!(
        offenders.is_empty(),
        "{} site(s) hand a caller-controlled string to a shell without \
         `safety::guard_exec`.\n\n\
         `sh -c` and `cmd /C` parse their argument: `cmd` splits on `&`, `sh` on \
         `;` and `|`, and Rust quotes an argument only when it contains a space \
         or a quote — so a value with neither reaches the parser verbatim. This \
         is how `web_open_url` shipped as an unapproved `Exec` while being \
         classified `Network`.\n\n\
         Either drop the shell (a fixed program with the value in its own argv \
         slot is not parsed), or gate on `Exec` and add the function to ALLOWED \
         in this file with the reason.\n\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

#[test]
fn the_scanner_sees_the_sites_it_claims_to() {
    // A check on the checker: if the walk breaks, the test above passes by
    // finding nothing, which reads as coverage.
    let src = source("src/builtins.rs").expect("src/builtins.rs is readable");
    let sites = shell_spawn_sites(&src);
    assert!(
        sites.len() >= 6,
        "only {} shell spawn sites parsed in builtins.rs",
        sites.len()
    );
    let names: Vec<&str> = sites.iter().map(|(n, _, _)| n.as_str()).collect();
    // `bi_nohup_run` was here until it was deleted as unreachable.
    // `bi_nohup_run` and `bi_ulimit_info` were here until their families were
    // deleted as unreachable -- the sixth fixture this session found naming
    // something that no longer exists.
    for expected in ["bi_xargs_exec", "bi_hw_sensors"] {
        assert!(
            names.contains(&expected),
            "{expected} should be among the parsed spawn sites, got {names:?}"
        );
    }
}

#[test]
fn the_value_test_distinguishes_a_literal_from_an_interpolation() {
    // The other direction: prove it rejects the shape it is for.
    assert!(passes_a_value(r#".args(["/C", "start", &url])"#));
    assert!(passes_a_value(r#".args(["-c", &full_cmd])"#));
    assert!(passes_a_value(r#".args(["-c", &format!("echo {}", x)])"#));
    assert!(!passes_a_value(r#".args(["-c", "ulimit -a"])"#));
    assert!(!passes_a_value(
        r#".args(["-c", "lspci | grep -iE 'vga|3d|display'"])"#
    ));
    assert!(!passes_a_value(r#".args(["/c", "ver"])"#));
    // The literal that the first version of this function mistook for an
    // interpolation, because it looked for `&` anywhere after the flag.
    assert!(!passes_a_value(
        r#".args(["-c", "which osx-cpu-temp && osx-cpu-temp"])"#
    ));
    // The shell chosen through a variable, as `nohup_run` does.
    assert!(passes_a_value(r#".args([flag, &command])"#));
}

#[test]
fn the_allowlist_has_a_reason_for_every_entry() {
    for (name, reason) in ALLOWED {
        assert!(
            !reason.trim().is_empty(),
            "{name} is allowed without a reason; the reason is the point"
        );
    }
    assert!(
        ALLOWED.len() <= 1,
        "the shell-spawn allowlist has grown to {}; it may only shrink",
        ALLOWED.len()
    );
}

#[test]
fn every_allowlisted_function_still_exists() {
    // An allowlist entry naming a function that no longer exists is a claim about
    // nothing: it reads as a considered exception, and it exempts a name the
    // compiler can no longer check. Three stale `SELF_GUARDED` entries survived
    // exactly that way -- `curl_exec`, `lxc_exec` and `nohup_run` were listed as
    // guarding themselves after their implementations had become unreachable.
    //
    // The list may only shrink, and this is what makes shrinking mandatory when
    // the thing it describes is deleted.
    let src = source("src/builtins.rs").expect("src/builtins.rs is readable");
    let missing: Vec<&str> = ALLOWED
        .iter()
        .map(|(n, _)| *n)
        // Two conventions in play: this file's allowlist holds *builtin* names
        // (`apply`), the option-injection and shell-spawn ratchets hold *function*
        // names (`bi_apply`). The first version of this check tested only one form
        // and reported four live functions as missing. Accept either.
        .filter(|n| !src.contains(&format!("fn {n}(")) && !src.contains(&format!("fn bi_{n}(")))
        .collect();
    assert!(
        missing.is_empty(),
        "these allowlist entries name functions that no longer exist: {missing:?}
         Remove them -- an exception for something that is gone is not an exception."
    );
}
