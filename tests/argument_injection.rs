//! A fixed program name is not enough: the *arguments* must not be able to
//! turn a benign-looking builtin into arbitrary execution.
//!
//! Four defects, all found on 2026-08-04, each by a *different* method — which
//! is the most useful thing in this file:
//!
//! 1. **PowerShell injection, single-quoted (CWE-78)** — found by reading.
//!    `format!("Start-Service '{}'", name)`: a value containing `'` closes the
//!    literal and the rest executes. Proved with a service name of
//!    `x'; New-Item -ItemType File -Path '<tmp>' -Force; '`.
//!
//! 2. **Option injection (CWE-88)** — found by reading. `tar -cvf out.tar
//!    <files>` with a "file" named `--use-compress-program=sh -c '…'` runs it;
//!    Info-ZIP's `-TT` likewise.
//!
//! 3. **PowerShell injection, double-quoted (CWE-78)** — found by *testing an
//!    assertion* that (1) had made from reading and got wrong. A double-quoted
//!    string expands `$`, so `$(cmd)` runs with no quote in the payload at all.
//!
//! 4. **Unquoted numeric interpolation** — found by the `ps_script!` type
//!    check, and findable no other way here: `-MemoryStartupBytes {}` and
//!    `-LocalPort {}` take caller strings *bare*, so neither reading (which had
//!    missed them twice) nor the source lint (which looks for quoted
//!    placeholders) could see them.
//!
//! The defence is layered accordingly: correct escapers, a source lint for the
//! textual shape, and types that make an unescaped value a compile error. Each
//! layer caught something the others could not.

use aethershell::safety::{
    applescript_quote, ps_bare_number, ps_join, ps_quote, reject_option_like,
    reject_sqlite_dot_command,
};

/// The reason single-quoting is the fix rather than escaping `"`.
///
/// A *double*-quoted PowerShell string expands `$`, so `$(command)` runs even
/// with no quote character anywhere in the payload. Several sites escaped only
/// `"` (as `` `" ``) and were therefore still injectable; this was demonstrated
/// by `base64_encode("$(New-Item …)")` creating the file. Moving those sites to
/// a single-quoted literal removes expansion entirely.
#[test]
fn ps_quote_defeats_subexpression_payloads_that_quote_escaping_missed() {
    // No apostrophes here, so the only transformation under test is that `$`
    // passes through untouched — it needs no escaping once the literal is
    // single-quoted, which is the whole point.
    let payload = "$(New-Item -ItemType File -Path C:\\tmp\\pwned -Force)";
    let quoted = ps_quote(payload).to_string();

    assert_eq!(quoted, format!("'{payload}'"));
    assert!(
        !quoted.starts_with('"'),
        "a double-quoted literal would re-enable $() expansion"
    );

    // And when the payload *does* carry apostrophes, they are doubled so it
    // still cannot break out.
    let with_quotes = "$(Get-Content 'C:\\secret')";
    assert_eq!(
        ps_quote(with_quotes).to_string(),
        "'$(Get-Content ''C:\\secret'')'",
        "apostrophes must be doubled while `$` stays inert"
    );
}

/// AppleScript literals escape with a backslash, so the backslash itself must be
/// escaped first — otherwise escaping the quote is undone by the payload.
#[test]
fn applescript_quote_escapes_backslash_before_quote() {
    assert_eq!(applescript_quote("plain").to_string(), "\"plain\"");

    // `\"` would close the literal if the backslash were not doubled first.
    assert_eq!(applescript_quote(r#"a\"#).to_string(), r#""a\\""#);

    let attack = r#"" & (do shell script "touch /tmp/pwned") & ""#;
    let quoted = applescript_quote(attack).to_string();
    assert!(quoted.starts_with('"') && quoted.ends_with('"'));
    // No bare quote may remain in the interior.
    let interior = &quoted[1..quoted.len() - 1];
    let mut chars = interior.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            chars.next();
        } else {
            assert_ne!(c, '"', "an unescaped quote closes the AppleScript literal");
        }
    }
}

/// The escaping rule itself. PowerShell closes a single-quoted string on `'`
/// and escapes one by doubling it; nothing else is special in that context.
#[test]
fn ps_quote_neutralizes_the_quote_that_ends_the_string() {
    assert_eq!(ps_quote("plain").to_string(), "'plain'");

    // The exact payload that was demonstrated to execute.
    let attack = "x'; New-Item -ItemType File -Path 'C:\\tmp\\pwned' -Force; '";
    let quoted = ps_quote(attack).to_string();

    assert!(quoted.starts_with('\'') && quoted.ends_with('\''));

    // Every quote in the interior must be doubled, so the literal cannot be
    // terminated early. Strip the delimiters, then check the interior has no
    // odd-length run of quotes.
    let interior = &quoted[1..quoted.len() - 1];
    for run in interior.split(|c| c != '\'').filter(|r| !r.is_empty()) {
        assert!(
            run.len() % 2 == 0,
            "an odd run of quotes escapes the literal: {run:?}"
        );
    }
}

/// Nothing should be altered other than quotes — the value still has to be the
/// path the caller asked for.
#[test]
fn ps_quote_leaves_everything_else_alone() {
    for s in [
        r"C:\Program Files\thing.txt",
        "$env:PATH",
        "back`tick",
        "semi;colon",
        "a b c",
    ] {
        assert_eq!(
            ps_quote(s).to_string(),
            format!("'{s}'"),
            "only the quote character needs escaping in a single-quoted literal"
        );
    }
}

/// `sqlite3` accepts its own dot-commands where SQL is expected, and `.system`
/// and `.shell` run programs. Verified before the fix:
/// `sqlite3 db ".system cmd /c echo … > file"` created the file — so
/// `db.sqlite_query` was arbitrary execution with no `Effect::Exec` gate.
#[test]
fn sqlite_dot_commands_are_refused_where_sql_is_expected() {
    for payload in [
        ".system cmd /c calc",
        ".shell /bin/sh",
        // Leading whitespace must not smuggle one past the check.
        "   .system id",
        "\t.shell id",
        "\n.system id",
    ] {
        assert!(
            reject_sqlite_dot_command("db_sqlite_query", payload).is_err(),
            "{payload:?} must be refused — .system and .shell run programs"
        );
    }
}

/// Ordinary SQL must still work, including statements that merely mention a dot.
#[test]
fn ordinary_sql_still_passes() {
    for sql in [
        "SELECT * FROM t",
        "SELECT t.a FROM t",
        "  INSERT INTO t VALUES (1)",
        "UPDATE t SET x = 1.5",
        "SELECT '.system' FROM t",
    ] {
        assert!(
            reject_sqlite_dot_command("db_sqlite_query", sql).is_ok(),
            "{sql:?} is SQL and must be allowed"
        );
    }
}

/// The escapers return a type, not a `String`, so the *type* records that
/// quoting happened.
///
/// A `String` reaching a command builder proves nothing — findings 10a, 10c and
/// 10d were all raw strings reaching a `format!` that read as fine, three
/// separate times. `PsLiteral` and `AppleScriptLiteral` have a private field, so
/// they cannot be built except through the escaper.
///
/// This test documents the guarantee; the compiler enforces it. The negative
/// cases are listed as comments below rather than as code, because they do not
/// compile — which is precisely the property being claimed.
#[test]
fn quoted_literals_are_a_distinct_type_that_only_the_escapers_produce() {
    // Renders through Display, so existing `format!("… {}", ps_quote(&v))` call
    // sites are unaffected.
    let rendered = format!("Start-Service {}", ps_quote("my service"));
    assert_eq!(rendered, "Start-Service 'my service'");

    let apple = format!("display dialog {}", applescript_quote("hi"));
    assert_eq!(apple, "display dialog \"hi\"");

    // Deliberately absent from the API, and each would defeat the guarantee:
    //
    //   let _: PsLiteral = PsLiteral(evil);            // private field
    //   let _: PsLiteral = evil.into();                // no From<String>
    //   let _: &str = &*ps_quote("x");                 // no Deref<Target=str>
    //
    // If any of those start compiling, the newtype has become decorative.
}

/// Values that must be interpolated *unquoted* are validated instead.
///
/// `-MemoryStartupBytes '4GB'` is not `-MemoryStartupBytes 4GB` — the latter is
/// a PowerShell numeric literal — so `ps_quote` is unavailable for these, and
/// they were going in bare. The source lint cannot catch that either: it looks
/// for *quoted* placeholders. `ps_script!`'s type check is the layer that found
/// them (`vm.create` memory/disk, `firewall.allow` port).
#[test]
fn bare_numeric_interpolations_are_validated_not_quoted() {
    for good in ["4GB", "512MB", "1.5TB", "8080", "0", "20gb"] {
        let v = ps_bare_number("vm_create", good)
            .unwrap_or_else(|e| panic!("{good:?} should be accepted: {e}"));
        assert_eq!(v.to_string(), good.trim(), "the value must pass through");
    }

    for bad in [
        "4GB; calc",
        "8080 && id",
        "$(id)",
        "1GB'",
        "",
        "GB",
        "1.2.3",
        "4 GB; rm -rf /",
    ] {
        assert!(
            ps_bare_number("vm_create", bad).is_err(),
            "{bad:?} reaches a command unquoted and must be refused"
        );
    }
}

/// `ps_join` keeps a list of escaped values in the type, rather than dropping to
/// `String` and losing the evidence that escaping happened.
#[test]
fn ps_join_preserves_escaping_across_a_list() {
    let joined = ps_join(
        ["a b".to_string(), "c'd".to_string()]
            .iter()
            .map(|s| ps_quote(s)),
        ",",
    );
    assert_eq!(joined.to_string(), "'a b','c''d'");
}

/// Option-like positional arguments are refused before the tool ever sees them.
#[test]
fn option_like_paths_are_refused() {
    for payload in [
        "--use-compress-program=sh -c 'id'",
        "--to-command=sh -c 'id'",
        "-TTsh -c 'id'",
        "-I/bin/sh",
        "--exclude=x",
    ] {
        let r = reject_option_like("tar_create", &[payload.to_string()]);
        assert!(
            r.is_err(),
            "{payload:?} must be refused — several archivers execute it"
        );
    }
}

/// The check must not break ordinary paths, or callers will route around it.
#[test]
fn ordinary_paths_are_accepted() {
    let ok: Vec<String> = [
        "file.txt",
        "./file.txt",
        "dir/file.txt",
        r"C:\Users\x\file.txt",
        "/abs/path",
        // The documented escape hatch for a file genuinely named "-weird".
        "./-weird",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    assert!(
        reject_option_like("tar_create", &ok).is_ok(),
        "normal paths must pass, including the './-name' workaround"
    );
}
