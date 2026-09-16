//! `map` and `where` must cost time proportional to the work, not to the
//! square of it.
//!
//! Until this test existed, `call_lambda` cloned the environment's pipe input
//! on every invocation. In `xs | map(fn(x) => ...)` the pipe input *is* `xs`,
//! so the whole collection was deep-copied once per element. Measured on Linux
//! with 12.0.2 before the fix:
//!
//! | input                              | before | after |
//! | ---------------------------------- | -----: | ----: |
//! | 500 GitHub issue records, `where`  | 290 ms | 15 ms |
//! | closure *ignoring* its argument    | 246 ms |  9 ms |
//! | `map` over 10,000 integers         | 515 ms |  9 ms |
//! | `map` over 20,000 integers         | 1801 ms| 14 ms |
//!
//! The giveaway was the second row: a closure that never reads its argument
//! cost 237 ms over 500 elements, which can only be the cost of copying
//! something the closure was not using. Doubling N multiplied the time by 3.3
//! to 4.2 -- the signature of an O(N^2) hidden inside an O(N) loop.
//!
//! Nothing in the unit suite could see this. Every correctness test used
//! collections of three or four elements, where a quadratic term is invisible.
//! It took running the shell against 500 real records, next to `jq` doing the
//! same job in 1 ms, for the gap to become a number.

use aethershell::env::Env;
use aethershell::{eval::eval_program, parser::parse_program, value::Value};
use std::time::{Duration, Instant};

fn run(src: &str) -> Value {
    let prog = parse_program(src).unwrap_or_else(|e| panic!("parse {src}: {e:?}"));
    let mut env = Env::new();
    eval_program(&prog, &mut env).unwrap_or_else(|e| panic!("eval {src}: {e}"))
}

/// Wall time of `src`, best of `n` -- the minimum is the measurement least
/// polluted by whatever else the machine was doing.
fn best_of(n: usize, src: &str) -> Duration {
    (0..n)
        .map(|_| {
            let t = Instant::now();
            let _ = run(src);
            t.elapsed()
        })
        .min()
        .expect("at least one run")
}

const ROUNDS: usize = 9;

/// How much slower `src_large` is than `src_small`, measured by interleaving
/// them so a load spike cannot land on only one side.
///
/// The first version of this file compared two independent best-of-5 runs
/// against a fixed threshold, and it failed inside `cargo test --workspace
/// --jobs 12`: 146 test binaries competing for 24 cores turned a 10x ratio
/// into 41x, in a build with no optimisation. The measurement was not wrong
/// about the machine, it was wrong about the code -- which is the definition
/// of a flaky ratchet, and a flaky ratchet gets ignored and then deleted.
fn growth(small: &str, large: &str) -> f64 {
    let mut lo = Duration::MAX;
    let mut hi = Duration::MAX;
    for _ in 0..ROUNDS {
        let a = Instant::now();
        let _ = run(small);
        lo = lo.min(a.elapsed());
        let b = Instant::now();
        let _ = run(large);
        hi = hi.min(b.elapsed());
    }
    hi.as_secs_f64() / lo.as_secs_f64().max(1e-9)
}

/// The same 10x size step through a path with no closures in it.
///
/// `sum` walks the collection once in Rust, so its growth is linear by
/// construction. Load inflates it exactly as it inflates `map`, which is what
/// makes it usable as a yardstick: the assertions below compare `map` against
/// *this machine right now*, not against a number chosen on a quiet one.
fn linear_baseline() -> f64 {
    growth("range(0, 2000) | sum", "range(0, 20000) | sum")
}

/// The ceiling a linear `map` must stay under, given today's baseline.
///
/// Quadratic growth over a 10x step is ~100x. Linear is ~10x. Four times the
/// measured linear baseline sits well clear of the first and well above the
/// second, and the floor of 20 keeps the bound sane if `sum` is so fast that
/// its own measurement is dominated by parse time.
fn ceiling(baseline: f64) -> f64 {
    (4.0 * baseline).max(20.0)
}

fn array_len(v: &Value) -> usize {
    match v {
        Value::Array(xs) => xs.len(),
        other => panic!("expected an array, got {other:?}"),
    }
}

#[test]
fn map_cost_grows_with_n_not_with_n_squared() {
    let base = linear_baseline();
    let ratio = growth(
        "range(0, 2000) | map(fn(x) => x + 1)",
        "range(0, 20000) | map(fn(x) => x + 1)",
    );
    assert!(
        ratio < ceiling(base),
        "map over 20,000 elements took {ratio:.1}x the time of 2,000, while a \
         closure-free walk of the same two sizes took {base:.1}x on this \
         machine. Linear is ~10x; the quadratic version this test exists for \
         measured ~47x."
    );
}

#[test]
fn a_closure_that_ignores_its_argument_does_not_pay_for_the_collection() {
    // The sharpest form of the defect: identical element count, one cheap
    // closure, and the only thing that varies is how much data is sitting in
    // the pipe. If the pipe input is being copied per call, the wide case
    // costs many times the narrow one for no additional work.
    let ratio = growth(
        "range(0, 2000) | map(fn(x) => 1)",
        "range(0, 2000) | map(fn(x) => { id: x, pad: \"................................\" }) \
         | map(fn(r) => 1)",
    );
    // Both sides have the same element count and the same trivial closure;
    // only the width of what sits in the pipe differs, and the wide side pays
    // an honest extra pass to build the records. Anything beyond a small
    // multiple is the collection being copied per call.
    assert!(
        ratio < 25.0,
        "mapping a constant over 2,000 wide records cost {ratio:.1}x the same \
         map over 2,000 integers. The closure reads neither, so the difference \
         is the collection being copied per call."
    );
}

#[test]
fn where_is_held_to_the_same_bound() {
    let base = linear_baseline();
    let ratio = growth(
        "range(0, 2000) | where(fn(x) => x > 0)",
        "range(0, 20000) | where(fn(x) => x > 0)",
    );
    assert!(
        ratio < ceiling(base),
        "where over 20,000 elements took {ratio:.1}x the time of 2,000, against \
         a closure-free baseline of {base:.1}x on this machine"
    );
}

// ── the behaviour the optimisation must not change ──────────────────────

#[test]
fn both_lambda_arities_still_work() {
    // `map` used to discover the arity by calling with two arguments and
    // retrying with one. It now asks the lambda. Both forms must still run,
    // and the index must still be the index.
    assert_eq!(
        run("[10, 20, 30] | map(fn(x) => x * 2)"),
        run("[20, 40, 60]")
    );
    assert_eq!(run("[10, 20, 30] | map(fn(x, i) => i)"), run("[0, 1, 2]"));
    assert_eq!(
        run("[10, 20, 30] | where(fn(x) => x > 15)"),
        run("[20, 30]")
    );
    assert_eq!(
        run("[10, 20, 30] | where(fn(x, i) => i > 0)"),
        run("[20, 30]")
    );
}

#[test]
fn a_lambda_of_the_wrong_arity_is_still_refused() {
    let prog = parse_program("[1, 2, 3] | map(fn(a, b, c) => a)").expect("parse");
    let mut env = Env::new();
    let err = eval_program(&prog, &mut env)
        .expect_err("a three-parameter lambda cannot be a map callback");
    assert!(
        err.to_string().contains("args"),
        "the refusal should name the arity mismatch, got: {err}"
    );
}

#[test]
fn the_pipe_input_survives_a_lambda_call() {
    // The fix takes the pipe input instead of cloning it. If it failed to put
    // it back, a builtin reading the pipe after a lambda would see nothing --
    // which is precisely the leak the original clone was guarding against.
    assert_eq!(run("[1, 2, 3] | map(fn(x) => x + 1) | sum"), Value::Int(9));
    assert_eq!(
        run("[1, 2, 3] | where(fn(x) => x > 1) | map(fn(x) => x * 10) | sum"),
        Value::Int(50)
    );
    // A lambda whose body itself pipes: the inner pipeline must not consume
    // the outer one's input.
    assert_eq!(
        array_len(&run("[[1, 2], [3, 4]] | map(fn(xs) => xs | sum)")),
        2
    );
}

#[test]
fn non_vacuity_the_measured_programs_do_the_work_they_claim() {
    // A ratio test passes trivially if both sides do nothing. Assert the
    // shapes, so a `map` that silently returned its input unchanged, or a
    // `range` that produced an empty array, would be caught here rather than
    // flattering the timings above.
    assert_eq!(
        array_len(&run("range(0, 20000) | map(fn(x) => x + 1)")),
        20000
    );
    assert_eq!(
        array_len(&run("range(0, 20000) | where(fn(x) => x > 0)")),
        19999
    );
    assert_eq!(
        array_len(&run(
            "range(0, 2000) | map(fn(x) => { id: x, pad: \"................................\" })"
        )),
        2000
    );

    // And the timings must be large enough to be a measurement rather than
    // clock granularity.
    let large = best_of(3, "range(0, 20000) | map(fn(x) => x + 1)");
    assert!(
        large > Duration::from_micros(200),
        "the 20,000-element map completed in {large:?}, which is too close to \
         the clock's resolution for the ratio assertions to mean anything"
    );
}
