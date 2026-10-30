//! A builtin that never reads pipeline input refuses it.
//!
//! 373 declared builtins took no subject and dropped `input` unread: their
//! dispatch row discarded it (`|args, _, _|`) or the function they call named
//! it `_input`. So `"x" | platform_arch()` answered as if nothing had been
//! piped, at exit 0. `signature::validate` now refuses piped input to every
//! name in `signature::TAKES_NO_INPUT`, before the body runs.
//!
//! The list is checked against the source here, so a new builtin that drops its
//! input has to join it, and one that starts reading its input has to leave.

use aethershell::builtins::{call_with_input, BUILTIN_LOOKUP};
use aethershell::env::Env;
use aethershell::signature::{signature_of, TAKES_NO_INPUT};
use aethershell::value::Value;
use std::collections::BTreeSet;

fn source() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builtins.rs"),
    )
    .expect("src/builtins.rs is readable")
}

/// Whether `bi_name` declares its pipeline-input parameter unused: every
/// `Option<Value>` parameter's name begins with `_`.
fn function_drops_input(src: &str, function: &str) -> bool {
    let head = format!("\nfn {function}(");
    let Some(at) = src.find(&head) else {
        return false;
    };
    let params = &src[at + head.len()..];
    let params = &params[..params.find(')').unwrap_or(0)];
    let inputs: Vec<&str> = params
        .split(',')
        .filter(|p| p.contains("Option<Value>"))
        .map(|p| p.split(':').next().unwrap_or("").trim())
        .collect();
    !inputs.is_empty() && inputs.iter().all(|n| n.starts_with('_'))
}

/// Whether `text` names the variable `input` as a whole word (not
/// `bi_input_read` or `inputs`).
fn mentions_input(text: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.match_indices("input").any(|(i, _)| {
        !text[..i].chars().next_back().is_some_and(word)
            && !text[i + 5..].chars().next().is_some_and(word)
    })
}

/// For each `BUILTIN_DISPATCH` row, in order: whether it drops its input.
/// Rows are one line each, as the other dispatch scanners also assume.
fn rows_dropping_input(src: &str) -> Vec<bool> {
    let lines: Vec<&str> = src.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.contains("BUILTIN_DISPATCH") && l.contains("&["))
        .expect("BUILTIN_DISPATCH must be findable");
    let mut rows = Vec::new();
    for line in &lines[start + 1..] {
        let t = line.trim();
        if t == "];" {
            break;
        }
        let t = t.strip_prefix("move ").unwrap_or(t);
        if !t.starts_with('|') {
            continue;
        }
        let Some((params, body)) = t[1..].split_once('|') else {
            rows.push(false);
            continue;
        };
        let second = params.split(',').nth(1).map(str::trim).unwrap_or("_");
        // The row ignores its input, or passes it to a function that does.
        let drops = second.starts_with('_')
            || !mentions_input(body)
            || body
                .split_once("bi_")
                .and_then(|(_, rest)| rest.split_once('('))
                .is_some_and(|(f, _)| function_drops_input(src, &format!("bi_{f}")));
        rows.push(drops);
    }
    rows
}

#[test]
fn the_list_is_sorted_and_unique() {
    assert!(
        TAKES_NO_INPUT.windows(2).all(|w| w[0] < w[1]),
        "TAKES_NO_INPUT must be sorted and unique; validate binary-searches it"
    );
}

#[test]
fn the_list_matches_the_source() {
    let src = source();
    let rows = rows_dropping_input(&src);
    let mut derived = BTreeSet::new();
    for (name, idx) in BUILTIN_LOOKUP.iter() {
        let Some(sig) = signature_of(name) else {
            continue;
        };
        if sig.subject.is_some() || !rows.get(*idx).copied().unwrap_or(false) {
            continue;
        }
        derived.insert(sig.name);
    }
    let listed: BTreeSet<&str> = TAKES_NO_INPUT.iter().copied().collect();
    let unlisted: Vec<&&str> = derived.difference(&listed).collect();
    let stale: Vec<&&str> = listed.difference(&derived).collect();
    assert!(
        unlisted.is_empty() && stale.is_empty(),
        "signature::TAKES_NO_INPUT is out of step with src/builtins.rs.\n\
         drops its input but not listed (a piped value would be ignored): {unlisted:?}\n\
         listed but reads its input (a piped value would be refused): {stale:?}"
    );
    assert!(
        derived.len() > 300,
        "only {} found; the scan has drifted",
        derived.len()
    );
}

#[test]
fn every_listed_builtin_refuses_a_piped_value_before_it_runs() {
    // Refused in `validate`, before the body. Only builtins with a required
    // parameter are called: should the refusal ever go, `validate` still stops
    // these at the missing argument, so no body runs. (A run with the check
    // removed and every name called made db_kv_store() create a database in
    // the working directory.) The rest are held by the source check above.
    let mut accepted = Vec::new();
    let mut called = 0;
    for name in TAKES_NO_INPUT {
        let needs_an_argument =
            signature_of(name).is_some_and(|s| s.params.iter().any(|p| p.required));
        if !needs_an_argument {
            continue;
        }
        called += 1;
        let got = call_with_input(name, vec![], Some(Value::Str("x".into())), &mut Env::new());
        match got {
            Err(e)
                if e.to_string().contains("E_BAD_ARG")
                    && e.to_string().contains("no pipeline input") => {}
            other => accepted.push(format!("{name}: {other:?}")),
        }
    }
    assert!(accepted.is_empty(), "not refused:\n{}", accepted.join("\n"));
    assert!(
        called > 100,
        "only {called} were called; the filter has eaten the check"
    );
}

#[test]
fn a_listed_builtin_refuses_and_a_reader_still_reads() {
    let got = call_with_input(
        "platform_arch",
        vec![],
        Some(Value::Str("x".into())),
        &mut Env::new(),
    );
    assert!(format!("{got:?}").contains("no pipeline input"), "{got:?}");
    let echoed = call_with_input(
        "echo",
        vec![],
        Some(Value::Str("kept".into())),
        &mut Env::new(),
    )
    .expect("echo reads its input");
    assert!(format!("{echoed:?}").contains("kept"), "{echoed:?}");
}
