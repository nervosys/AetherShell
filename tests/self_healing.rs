//! Tests for the self-healing pillar (docs/AGENTIC_FIRST_DESIGN.md §9, §5.2).
//!
//! The design claimed a self-correcting loop "falls out of" structured errors.
//! That inference only holds if failures actually carry codes, suggestions are
//! real, repair context is cheap, and a failed attempt leaves no debris. These
//! tests assert each of those four things directly.

use aethershell::value::Value;
use std::sync::Mutex;

// The transaction journal, AETHER_WORKSPACE and AETHER_MODE are process-global.
static LOCK: Mutex<()> = Mutex::new(());

fn try_call(name: &str, args: Vec<Value>) -> Result<Value, anyhow::Error> {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env)
}

fn call(name: &str, args: Vec<Value>) -> Value {
    try_call(name, args).expect("builtin call")
}

fn s(x: &str) -> Value {
    Value::Str(x.to_string())
}

/// The structured record an agent's `catch` block would bind.
fn caught(name: &str, args: Vec<Value>) -> Value {
    let e = try_call(name, args).expect_err("expected failure");
    let se = e
        .downcast_ref::<aethershell::safety::SafetyError>()
        .unwrap_or_else(|| panic!("failure carried no structured error: {e}"));
    Value::from_json(&se.to_json())
}

fn field<'a>(v: &'a Value, path: &[&str]) -> &'a Value {
    let mut cur = v;
    for key in path {
        cur = match cur {
            Value::Record(m) => m
                .get(*key)
                .unwrap_or_else(|| panic!("missing field {key} in {cur:?}")),
            other => panic!("not a record at {key}: {other:?}"),
        };
    }
    cur
}

fn reset() {
    std::env::remove_var("AETHER_MODE");
    let mut env = aethershell::env::Env::new();
    for _ in 0..16 {
        if aethershell::builtins::call("tx_rollback", vec![], &mut env).is_err() {
            break;
        }
    }
}

// ── 1. Every failure is branchable ──────────────────────────────────────────

/// The boundary net: a builtin that fails with bare `anyhow` prose still reaches
/// the caller with a stable code. Before this, ~879 `bail!`/`anyhow!` sites in
/// `builtins.rs` produced unbranchable strings.
#[test]
fn an_uncoded_failure_still_arrives_with_a_code() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    // `cat` on a missing path fails deep inside path validation with plain prose.
    let err = caught("cat", vec![s("no_such_file_ae_self_healing.txt")]);
    assert_eq!(field(&err, &["error", "code"]), &s("E_UNKNOWN"));
    // An unidentified fault must NOT invite a retry — retrying spends budget
    // without changing anything.
    assert_eq!(
        field(&err, &["error", "retryable"]),
        &Value::Bool(false),
        "an uncoded failure must not be advertised as retryable"
    );
    // The original message is preserved verbatim, not replaced by a generic one.
    match field(&err, &["error", "message"]) {
        Value::Str(m) => assert!(!m.is_empty(), "original message was discarded"),
        other => panic!("message not a string: {other:?}"),
    }
}

/// A specific code must never be downgraded to `E_UNKNOWN` by the net.
#[test]
fn a_specific_code_passes_through_the_net_unchanged() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let err = caught("pick", vec![Value::Int(1)]);
    assert_eq!(field(&err, &["error", "code"]), &s("E_BAD_ARG"));
    assert_eq!(field(&err, &["error", "retryable"]), &Value::Bool(true));
}

// ── 2. Suggestions are real, or absent ──────────────────────────────────────

#[test]
fn a_misspelled_builtin_suggests_the_real_name() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    for (typo, want) in [("piick", "pick"), ("digets", "digest"), ("aeconn", "aecon")] {
        let err = caught(typo, vec![Value::Int(1)]);
        assert_eq!(field(&err, &["error", "code"]), &s("E_UNKNOWN_BUILTIN"));
        // This is the one lookup failure worth retrying — the agent has a name.
        assert_eq!(field(&err, &["error", "retryable"]), &Value::Bool(true));
        match field(&err, &["error", "did_you_mean"]) {
            Value::Array(a) => assert!(a.contains(&s(want)), "{typo}: expected {want} among {a:?}"),
            other => panic!("did_you_mean not an array: {other:?}"),
        }
    }
}

/// The old hand-written suggester fell back to `"ls, cat, grep"` when nothing
/// matched — a confident wrong answer, which costs a retrying agent more than
/// silence does. Nothing close must yield nothing at all.
#[test]
fn nothing_close_yields_no_suggestion_rather_than_a_guess() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let err = caught("zzqqxwv_not_remotely_a_builtin", vec![]);
    assert_eq!(field(&err, &["error", "code"]), &s("E_UNKNOWN_BUILTIN"));
    match &err {
        Value::Record(m) => match m.get("error") {
            Some(Value::Record(inner)) => assert!(
                inner.get("did_you_mean").is_none(),
                "invented a suggestion for gibberish: {inner:?}"
            ),
            other => panic!("bad error shape: {other:?}"),
        },
        other => panic!("not a record: {other:?}"),
    }
}

/// Every suggested name must actually be callable. A suggestion that does not
/// exist sends the agent into a second failure.
#[test]
fn every_suggestion_names_a_builtin_that_exists() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    for typo in [
        "piick",
        "digets",
        "tokns",
        "ontology_manifst",
        "aecon_decod",
    ] {
        for cand in aethershell::builtins::did_you_mean(typo) {
            assert!(
                aethershell::builtins::BUILTIN_LOOKUP.contains_key(cand.as_str()),
                "suggested '{cand}' for '{typo}', which is not a builtin"
            );
        }
    }
}

/// Same typo, same suggestions, every time — a repair loop that replays a fix
/// must not see the ordering shift under it.
#[test]
fn suggestions_are_deterministic() {
    let first = aethershell::builtins::did_you_mean("tokns");
    for _ in 0..5 {
        assert_eq!(aethershell::builtins::did_you_mean("tokns"), first);
    }
}

// ── 3. Repair context is cheap ──────────────────────────────────────────────

/// `diagnose` is progressive disclosure applied to failure. The saving tracks how
/// richly a builtin is documented — it is largest exactly where a full dump would
/// hurt most, and small for builtins whose definition is already thin. So the
/// contract asserted here is: **never more expensive than the full describe**, and
/// **under half** of it for a richly-documented builtin.
#[test]
fn diagnose_costs_less_than_a_full_ontology_describe() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let cost = |v: Value| match call("tokens", vec![v]) {
        Value::Int(n) => n,
        other => panic!("tokens returned {other:?}"),
    };
    let measure = |name: &str| {
        let err = caught(name, vec![]);
        let d = cost(call("diagnose", vec![err]));
        let f = cost(call("ontology_describe", vec![s(name)]));
        (d, f)
    };

    for name in ["http_get", "map", "grep", "sort"] {
        let (d, f) = measure(name);
        assert!(
            d <= f,
            "{name}: diagnose ({d} tokens) must never cost more than \
             ontology_describe ({f})"
        );
    }

    // `map` carries a full definition — params, examples, prose — and is where the
    // disclosure actually pays.
    let (d, f) = measure("map");
    assert!(
        d * 2 < f,
        "map: diagnose ({d} tokens) should be under half of ontology_describe ({f})"
    );

    // …and it must still carry what a repair actually needs.
    let diag = call("diagnose", vec![caught("http_get", vec![])]);
    assert_eq!(field(&diag, &["code"]), &s("E_BAD_ARG"));
    assert_eq!(field(&diag, &["builtin"]), &s("http_get"));
    match field(&diag, &["signature"]) {
        Value::Str(sig) => assert!(!sig.is_empty(), "no signature to repair against"),
        other => panic!("signature not a string: {other:?}"),
    }
    assert!(matches!(field(&diag, &["effect"]), Value::Str(_)));
}

/// A failure with no structured code must be reported as such rather than have
/// a code guessed from its prose.
#[test]
fn diagnose_reports_an_unstructured_failure_honestly() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let diag = call("diagnose", vec![s("something went wrong")]);
    assert_eq!(field(&diag, &["code"]), &s("E_UNKNOWN"));
    assert_eq!(field(&diag, &["retryable"]), &Value::Bool(false));
}

/// The suggestions survive into the repair context — that is the whole path an
/// agent walks: catch → diagnose → corrected call.
#[test]
fn diagnose_carries_suggestions_through_from_the_error() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let err = caught("digets", vec![Value::Int(1)]);
    let diag = call("diagnose", vec![err]);
    match field(&diag, &["did_you_mean"]) {
        Value::Array(a) => assert!(a.contains(&s("digest")), "lost suggestions: {a:?}"),
        other => panic!("did_you_mean not an array: {other:?}"),
    }
}

// ── 4. A failed attempt leaves no debris ────────────────────────────────────

/// The point of `try_repair`: retrying is only sound if attempt N+1 starts from
/// the same state attempt N did. A partial batch that fails must be undone.
#[test]
fn a_failed_attempt_is_rolled_back_before_the_error_is_returned() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let w = std::env::temp_dir().join(format!("ae_repair_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);

    let good = w.join("kept.txt");
    std::fs::write(&good, "original").unwrap();
    let made = w.join("made.txt");

    // A batch that mutates twice and then fails on a call that cannot succeed.
    let code = format!(
        "file_write({:?}, \"clobbered\")\nfile_write({:?}, \"new\")\nno_such_builtin_here()",
        good.to_string_lossy().replace('\\', "/"),
        made.to_string_lossy().replace('\\', "/"),
    );
    let out = call("try_repair", vec![s(&code)]);

    assert_eq!(field(&out, &["ok"]), &Value::Bool(false));
    assert_eq!(
        field(&out, &["error", "code"]),
        &s("E_UNKNOWN_BUILTIN"),
        "the structured error must survive the rollback"
    );
    assert_eq!(
        field(&out, &["retryable"]),
        &Value::Bool(true),
        "a misspelled name is exactly the case a repair loop should retry"
    );

    // The debris is gone: the overwrite reverted, the created file removed.
    assert_eq!(
        std::fs::read_to_string(&good).unwrap(),
        "original",
        "a failed attempt left a clobbered file behind"
    );
    assert!(
        !made.exists(),
        "a failed attempt left a created file behind"
    );

    std::env::remove_var("AETHER_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
}

#[test]
fn a_successful_attempt_keeps_its_work() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let w = std::env::temp_dir().join(format!("ae_repair_ok_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);

    let f = w.join("out.txt");
    let code = format!(
        "file_write({:?}, \"written\")",
        f.to_string_lossy().replace('\\', "/")
    );
    let out = call("try_repair", vec![s(&code)]);
    assert_eq!(field(&out, &["ok"]), &Value::Bool(true));
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "written");

    std::env::remove_var("AETHER_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
}

/// An enclosing transaction must keep its earlier work: `try_repair` rolls back
/// only what its own attempt recorded.
#[test]
fn a_failed_attempt_does_not_discard_an_enclosing_transactions_work() {
    let _l = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let w = std::env::temp_dir().join(format!("ae_repair_tx_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&w);
    std::fs::create_dir_all(&w).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &w);

    let earlier = w.join("earlier.txt");
    call("tx_begin", vec![]);
    call(
        "file_write",
        vec![s(&earlier.to_string_lossy()), s("earlier")],
    );

    let code = "no_such_builtin_here()".to_string();
    let out = call("try_repair", vec![s(&code)]);
    assert_eq!(field(&out, &["ok"]), &Value::Bool(false));

    // The enclosing transaction is still open and still holds its own write.
    assert_eq!(
        std::fs::read_to_string(&earlier).unwrap(),
        "earlier",
        "try_repair discarded the enclosing transaction's work"
    );
    call("tx_commit", vec![]);
    assert_eq!(std::fs::read_to_string(&earlier).unwrap(), "earlier");

    std::env::remove_var("AETHER_WORKSPACE");
    let _ = std::fs::remove_dir_all(&w);
}
