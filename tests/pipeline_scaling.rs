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

/// Serialises the timing tests in this file.
///
/// They share one process, and `cargo test` runs them on parallel threads. A
/// ratchet calibrated against *external* load is still wrecked by a sibling
/// timing test hammering the same process -- adding the wide-record check made
/// the other two fail immediately. Serialise with a lock rather than
/// `--test-threads=1`, which only helps whoever remembers to pass it.
/// (empty, small, large) source for one program.
type Sizes = (&'static str, &'static str, &'static str);

static TIMING: std::sync::Mutex<()> = std::sync::Mutex::new(());

const ROUNDS: usize = 9;

/// Baseline and subject measured *in the same rounds*, returning the median
/// ratio of each.
///
/// `growth` already interleaves the two sizes of one program, so a spike cannot
/// land on only one size. It does not interleave the *baseline* with the
/// *subject*: those were two sequential calls, so the baseline could be timed in
/// a quiet window and the subject in a loud one. Under `--jobs 12` that is
/// exactly what happened -- a 6.9x baseline against a 37.8x `map`, on a change
/// that cannot affect scaling at all (`map` calls `call_lambda` per element,
/// which does not go through the argument validator).
///
/// Here the four programs are timed adjacently inside each round and the ratio
/// for that round is computed from those four numbers, so contention inflates
/// both sides together. The median over rounds then discards a round that was
/// contended anyway. A genuinely quadratic subject is ~100x in *every* round, so
/// the median stays ~100x and the ratchet still bites.
fn calibrated(base: Sizes, subject: Sizes) -> (f64, f64) {
    // Every program is timed at three sizes -- empty, small, large -- and the
    // empty case measures the fixed cost of a run (parse, environment setup,
    // printing) that has nothing to do with N.
    //
    // Subtracting that cost matters: `range(0, N) | sum` walks the collection
    // in Rust and is so fast that the fixed cost dominated it, so a 10x step in
    // N measured as 4.3x. A yardstick reading 4.3x for something linear made an
    // honest 21x `map` look quadratic, and three runs in six failed that way.
    //
    // The subtraction is done on the BEST time for each size, not per round.
    // Per-round subtraction was tried and could not work: at 2,000 elements the
    // work is smaller than the run-to-run noise, so the empty case often timed
    // slower than the small one and only one round in nine was usable. The
    // minimum over rounds is the measurement least polluted by whatever else
    // the machine was doing, which is exactly what a subtraction needs.
    let best_of = |src: &str| {
        let mut best = f64::MAX;
        for _ in 0..ROUNDS {
            let t = Instant::now();
            let _ = run(src);
            best = best.min(t.elapsed().as_secs_f64());
        }
        best
    };
    // Paired by size, so the two programs meet each size in the same stretch
    // of time. The load robustness comes from the minimum over rounds, not
    // from the pairing -- each `best_of` is nine consecutive runs of one
    // program, which is not interleaving and should not be described as it.
    let (bz, sz) = (best_of(base.0), best_of(subject.0));
    let (bs, ss) = (best_of(base.1), best_of(subject.1));
    let (bl, sl) = (best_of(base.2), best_of(subject.2));

    let net = |zero: f64, small: f64, large: f64, what: &str| {
        let (ds, dl) = (small - zero, large - zero);
        assert!(
            ds > 0.0 && dl > 0.0,
            "{what}: the timer cannot see past the fixed per-run cost (empty {zero:.5}s, small {small:.5}s, large {large:.5}s), so nothing built on it means anything"
        );
        dl / ds
    };
    (net(bz, bs, bl, "baseline"), net(sz, ss, sl, "subject"))
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
    let _timing = TIMING.lock().unwrap_or_else(|e| e.into_inner());
    let (base, ratio) = calibrated(
        (
            "range(0, 0) | sum",
            "range(0, 2000) | sum",
            "range(0, 20000) | sum",
        ),
        (
            "range(0, 0) | map(fn(x) => x + 1)",
            "range(0, 2000) | map(fn(x) => x + 1)",
            "range(0, 20000) | map(fn(x) => x + 1)",
        ),
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
    let _timing = TIMING.lock().unwrap_or_else(|e| e.into_inner());
    // The defect copied the pipe input once per element, so cost grew with
    // (elements x width). The two ratchets above catch the element half on
    // narrow data; this one carries the width half, by running the same
    // calibrated scaling check over WIDE records.
    //
    // It used to compare wide against narrow at a fixed element count, which
    // is the more direct statement of the defect and could not be measured.
    // Three designs were tried and all failed, for the same reason: cloning a
    // wide record legitimately costs more than cloning an integer, that
    // difference is large and variable, and it sits on top of the signal.
    //
    //   direct comparison, fixed threshold   28.0x healthy / 28.8x broken
    //   build cost subtracted               2.2x-10.6x healthy over five runs
    //   build amortised over five maps      1.5x-14.4x healthy over five runs
    //
    // A healthy spread wider than the gap to the broken case is not a test.
    // Scaling is the formulation that survives: the width confound is present
    // in both sizes and divides out, exactly as load does.
    // Deliberately smaller than the other two. Under the defect this walks
    // N^2 *wide records*, and at 20,000 it did not finish in half an hour --
    // a falsification nobody can afford to run is one nobody runs, so the
    // check that this test still bites would quietly stop happening.
    let (base, ratio) = calibrated(
        ("range(0, 0) | sum", "range(0, 500) | sum", "range(0, 5000) | sum"),
        ("range(0, 0) | map(fn(x) => { id: x, pad: \"................................\" }) | map(fn(r) => 1)", "range(0, 500) | map(fn(x) => { id: x, pad: \"................................\" }) | map(fn(r) => 1)", "range(0, 5000) | map(fn(x) => { id: x, pad: \"................................\" }) | map(fn(r) => 1)"),
    );
    assert!(
        ratio < ceiling(base),
        "mapping a constant over wide records grew {ratio:.1}x from 500 to 5,000 elements, against a closure-free baseline of {base:.1}x on this machine. The closure reads nothing, so anything superlinear is the collection being copied per call."
    );
}

#[test]
fn where_is_held_to_the_same_bound() {
    let _timing = TIMING.lock().unwrap_or_else(|e| e.into_inner());
    let (base, ratio) = calibrated(
        (
            "range(0, 0) | sum",
            "range(0, 2000) | sum",
            "range(0, 20000) | sum",
        ),
        (
            "range(0, 0) | where(fn(x) => x > 0)",
            "range(0, 2000) | where(fn(x) => x > 0)",
            "range(0, 20000) | where(fn(x) => x > 0)",
        ),
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
