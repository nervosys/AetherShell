//! The evaluation deadline must actually stop evaluation.
//!
//! Finding 6a closed the Agent API's missing request deadline, but only at the
//! HTTP layer: the connection was freed and the caller got a 408 while the
//! evaluation kept running on a blocking-pool thread, because dropping a
//! `spawn_blocking` handle does not cancel the closure. `safety::enter_deadline`
//! plus a check in `eval_expr` is what closes that.
//!
//! The reason these tests exist rather than a code review: the previous attempt
//! at this — mounting a `TimeoutLayer` — read as obviously correct and did
//! nothing at all, and only an assertion written *before* the fix revealed it.
//! So the claim being tested here is not "a deadline is configured" but "work
//! stops, and it stops near the deadline rather than after finishing".

use aethershell::{env::Env, eval, parser, safety, value::Value};
use std::time::{Duration, Instant};

/// Parse and evaluate `code`, propagating either failure.
fn eval(code: &str) -> anyhow::Result<Value> {
    let stmts = parser::parse_program(code)?;
    let mut env = Env::default();
    eval::eval_program(&stmts, &mut env)
}

/// A workload that is *interpreter-bound*, not allocation-bound.
///
/// The obvious version — one enormous `range` — is the wrong test. `range`
/// materialises its array inside the builtin, and the deadline cannot interrupt
/// a builtin that never returns to the interpreter, so such a test measures
/// allocation and hangs rather than demonstrating anything. (Learned the hard
/// way: `range(0, 40000000)` wedged the suite.)
///
/// Nesting keeps the data small while re-entering `eval_expr` millions of
/// times, which is the thing being tested.
const LONG_RUNNING: &str =
    "range(0, 400000) | map(fn(x) => range(0, 50) | map(fn(y) => y + x) | length()) | length()";

#[test]
fn a_deadline_stops_a_long_evaluation() {
    let started = Instant::now();
    let result = {
        let _guard = safety::enter_deadline(Duration::from_millis(300));
        eval(LONG_RUNNING)
    };
    let elapsed = started.elapsed();

    assert!(
        result.is_err(),
        "evaluation ran to completion despite an expired deadline — the \
         interpreter is not checking it (took {elapsed:?})"
    );
    assert!(
        elapsed < Duration::from_secs(20),
        "evaluation stopped only after finishing its work, not at the \
         deadline — took {elapsed:?}"
    );

    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("time limit"),
        "the error must say the work was cancelled for time, so a caller can \
         tell it apart from a genuine failure; got {msg:?}"
    );
}

/// The far more important half. A deadline that fires on *correct* programs is
/// worse than none, because it turns a working shell into an unreliable one.
#[test]
fn ordinary_evaluation_is_unaffected() {
    let _guard = safety::enter_deadline(Duration::from_secs(60));
    let out = eval("[1, 2, 3] | map(fn(x) => x * 2)").expect("ordinary work must not be cancelled");
    assert!(format!("{out:?}").contains('6'), "got {out:?}");
}

/// With no deadline set — the REPL, scripts, every other test — the check must
/// be inert. This is the default path, so a regression here would be felt
/// everywhere.
#[test]
fn evaluation_without_a_deadline_is_never_interrupted() {
    // Sized to comfortably exceed DEADLINE_CHECK_INTERVAL (1024 steps) so the
    // clock-sampling branch is actually exercised, while staying quick in a
    // debug build — the claim is "the check is inert without a deadline", which
    // scale does not make more true.
    let out = eval("range(0, 20000) | map(fn(x) => x + 1) | length()")
        .expect("with no deadline set, nothing may be cancelled");
    assert!(format!("{out:?}").contains("20000"), "got {out:?}");
}

/// These threads are pooled and reused. A deadline left set would make the next
/// request on that thread fail instantly for no reason, which would be a much
/// worse bug than the one being fixed.
#[test]
fn a_deadline_does_not_leak_to_later_work_on_the_same_thread() {
    {
        let _guard = safety::enter_deadline(Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(20));
        let _ = eval(LONG_RUNNING); // expected to be cancelled
    }

    // Same thread, no deadline in scope: must behave normally.
    eval("[1, 2, 3] | map(fn(x) => x * 2)")
        .expect("a dropped deadline must not poison later work on this thread");
}

// ---------------------------------------------------------------------------
// Finding 13: recursion depth
// ---------------------------------------------------------------------------
//
// These go through `safety::with_eval_stack`, which is not ceremony — it is the
// supported way to evaluate, and running them without it demonstrated why.
//
// `MAX_CALL_DEPTH` is 2000, and a debug frame measured at roughly 30 KB, so the
// limit needs ~60 MB of stack to be reachable. A default test thread has about
// 2 MB, and the first version of `legitimately_deep_recursion_still_works` duly
// overflowed at depth 1500 — the stack lost the race to the limit, which is the
// precise failure the large stack exists to prevent.
//
// **This is a real constraint on the fix, not a test detail.** A library
// consumer calling `eval::eval_program` on an ordinary thread gets the depth
// limit but not the stack, so deep recursion still aborts before the limit can
// refuse it. `with_eval_stack` (or an equivalently large `stack_size`) is
// required, which is why `main` and the Agent API's tokio runtime both use it.

/// Unbounded recursion used to abort the process, which no error handling could
/// catch because a stack overflow does not unwind.
#[test]
fn unbounded_recursion_is_refused_rather_than_fatal() {
    let err = safety::with_eval_stack(|| eval("let f = fn(x) => f(x)\nf(1)"))
        .expect_err("unbounded recursion must not be allowed to run");
    assert!(
        err.to_string().contains("recursion too deep"),
        "it must fail for depth, not incidentally; got {err}"
    );
}

/// The half that keeps this from being a regression dressed as a fix. Before
/// the large stack, depth 40 aborted the process — so a depth limit alone would
/// have had to sit below 40 to fire at all, rejecting ordinary programs.
#[test]
fn legitimately_deep_recursion_still_works() {
    let out = safety::with_eval_stack(|| {
        eval("let f = fn(n) => match n { 0 => 0, _ => f(n - 1) }\nf(1500)")
    })
    .expect("1500 levels is a reasonable program and must not be refused");
    assert!(format!("{out:?}").contains('0'), "got {out:?}");
}
