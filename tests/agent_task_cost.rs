//! What the agentic features cost over whole *tasks*, not single calls.
//!
//! Every number this project has quoted so far is a per-call proxy: 2.26× for
//! `@nest`, 26.3× for a handle, 54% classification coverage. None of them says
//! whether an agent gets more done. This file measures a task end to end.
//!
//! # What is and is not being measured
//!
//! There is **no language model in this loop**. What is measured is the
//! mechanical cost of a task under two explicitly-modelled strategies, over a
//! fixed corpus:
//!
//! * **Discover-then-act** — what an agent must do when a builtin's result shape
//!   is unknown: call it, read the result, then write the pipeline it wanted.
//!   Two round-trips, and the full intermediate result crosses into context.
//! * **Compose** — what `x-returns` makes possible: write the whole pipeline
//!   from the declared shape, run it server-side, and receive only the answer.
//!   One round-trip.
//!
//! The strategies are a **model of agent behaviour**, stated here rather than
//! buried: a real agent may explore more (making the gap larger) or already know
//! a builtin (making it smaller). This measures what the shell makes possible,
//! not what a given model does with it. Treating these numbers as evidence about
//! real agents would repeat exactly the mistake this project has been correcting
//! — the README's cache multiplier was a model quoted as a measurement.
//!
//! The corpus deliberately includes tasks the features do **not** help, so the
//! aggregate is not cherry-picked.

use aethershell::value::Value;

fn call(name: &str, args: Vec<Value>) -> Option<Value> {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env).ok()
}

fn tokens(s: &str) -> usize {
    match call("tokens", vec![Value::Str(s.to_string())]) {
        Some(Value::Int(n)) => n as usize,
        Some(Value::Record(m)) => match m.get("tokens") {
            Some(Value::Int(n)) => *n as usize,
            other => panic!("unexpected tokens shape: {other:?}"),
        },
        other => panic!("unexpected tokens result: {other:?}"),
    }
}

/// Tokens a value costs when it crosses into the agent's context.
fn cost(v: &Value) -> usize {
    match aethershell::builtins::render_agent(v, None) {
        Some(text) => tokens(&text),
        None => 0,
    }
}

struct Task {
    name: &'static str,
    /// The call an agent makes.
    builtin: &'static str,
    args: Vec<Value>,
    /// The narrowing it actually wanted, applied to that result.
    narrow: fn(&Value) -> Value,
}

fn take_n(v: &Value, n: usize) -> Value {
    match v {
        Value::Array(items) => Value::Array(items.iter().take(n).cloned().collect()),
        other => other.clone(),
    }
}

fn count_of(v: &Value) -> Value {
    match v {
        Value::Array(items) => Value::Int(items.len() as i64),
        _ => Value::Int(1),
    }
}

fn corpus() -> Vec<Task> {
    vec![
        Task {
            name: "list a directory, keep 5",
            builtin: "ls",
            args: vec![Value::Str("src".into())],
            narrow: |v| take_n(v, 5),
        },
        Task {
            name: "list a directory, count it",
            builtin: "ls",
            args: vec![Value::Str(".".into())],
            narrow: count_of,
        },
        Task {
            name: "large range, keep 3",
            builtin: "range",
            args: vec![Value::Int(1), Value::Int(2000)],
            narrow: |v| take_n(v, 3),
        },
        Task {
            name: "large range, count it",
            builtin: "range",
            args: vec![Value::Int(1), Value::Int(5000)],
            narrow: count_of,
        },
        // Controls: the answer *is* the whole result, so composing saves
        // nothing. Without these the aggregate would flatter the features.
        Task {
            name: "small range, keep all (no saving expected)",
            builtin: "range",
            args: vec![Value::Int(1), Value::Int(6)],
            narrow: |v| v.clone(),
        },
        Task {
            name: "scalar result (no saving possible)",
            builtin: "pwd",
            args: vec![],
            narrow: |v| v.clone(),
        },
    ]
}

struct Measured {
    name: &'static str,
    discover_tokens: usize,
    discover_trips: usize,
    compose_tokens: usize,
    compose_trips: usize,
}

fn measure() -> Vec<Measured> {
    // Handles off for the discover strategy: it is defined by receiving the
    // whole intermediate result, which is what makes it expensive.
    let mut out = Vec::new();
    for t in corpus() {
        let Some(full) = call(t.builtin, t.args.clone()) else {
            continue;
        };
        let answer = (t.narrow)(&full);

        std::env::set_var("AETHER_HANDLE_BYTES", "0");
        let intermediate = cost(&full);
        let final_cost = cost(&answer);
        std::env::remove_var("AETHER_HANDLE_BYTES");

        out.push(Measured {
            name: t.name,
            // Receives the whole result, then the answer it wanted.
            discover_tokens: intermediate + final_cost,
            discover_trips: 2,
            // Writes the pipeline from the declared shape; only the answer
            // crosses back.
            compose_tokens: final_cost,
            compose_trips: 1,
        });
    }
    out
}

#[test]
fn report_task_cost_across_the_corpus() {
    let rows = measure();
    assert!(!rows.is_empty(), "the corpus must actually run");

    println!(
        "\n{:<44} {:>10} {:>10} {:>8}",
        "task", "discover", "compose", "ratio"
    );
    let (mut d_tok, mut c_tok, mut d_trips, mut c_trips) = (0, 0, 0, 0);
    for r in &rows {
        let ratio = if r.compose_tokens == 0 {
            f64::INFINITY
        } else {
            r.discover_tokens as f64 / r.compose_tokens as f64
        };
        println!(
            "{:<44} {:>10} {:>10} {:>7.1}x",
            r.name, r.discover_tokens, r.compose_tokens, ratio
        );
        d_tok += r.discover_tokens;
        c_tok += r.compose_tokens;
        d_trips += r.discover_trips;
        c_trips += r.compose_trips;
    }
    let mut ratios: Vec<f64> = rows
        .iter()
        .filter(|r| r.compose_tokens > 0)
        .map(|r| r.discover_tokens as f64 / r.compose_tokens as f64)
        .collect();
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = ratios[ratios.len() / 2];

    println!(
        "{:<44} {:>10} {:>10} {:>7.1}x",
        "TOTAL (dominated by the largest task)",
        d_tok,
        c_tok,
        d_tok as f64 / c_tok.max(1) as f64
    );
    println!("round-trips: discover {d_trips}, compose {c_trips}");
    println!(
        "per-task ratio: min {:.1}x, median {:.1}x, max {:.1}x",
        ratios[0],
        median,
        ratios[ratios.len() - 1]
    );
    println!(
        "\nDo not quote a single multiplier from this. The saving is entirely\n\
         determined by how much of a result the agent discards, and on this\n\
         corpus that spans three orders of magnitude ({:.0}x to {:.0}x). The\n\
         TOTAL row is arithmetic on sums, so the largest task dominates it; it\n\
         describes this corpus and nothing else.\n\
         Modelled, not observed: no language model is in this loop. See the\n\
         module docs for the two strategies and their assumptions.\n",
        ratios[0],
        ratios[ratios.len() - 1]
    );
}

#[test]
fn composing_never_costs_more_than_discovering() {
    // The directional claim, which must hold for every task including the
    // controls. If composing were ever more expensive, the feature would be a
    // trap on some workloads and the aggregate would be hiding it.
    for r in measure() {
        assert!(
            r.compose_tokens <= r.discover_tokens,
            "{}: composing cost {} vs discovering {}",
            r.name,
            r.compose_tokens,
            r.discover_tokens
        );
        assert!(r.compose_trips <= r.discover_trips, "{}", r.name);
    }
}

#[test]
fn the_corpus_contains_tasks_the_features_do_not_help() {
    // Guards the honesty of the aggregate. A corpus of only large results would
    // report a flattering multiplier that says nothing about real workloads.
    let unhelped = measure()
        .into_iter()
        .filter(|r| r.discover_tokens < r.compose_tokens * 3)
        .count();
    assert!(
        unhelped >= 2,
        "expected several tasks with little or no saving, found {unhelped}"
    );
}

#[test]
fn no_single_multiplier_describes_this_corpus() {
    // The point of this assertion is to stop a headline number being lifted out
    // of the report. If the spread ever collapsed, a single figure might be
    // defensible — until then, quoting one would be the same error as the
    // README's cache multiplier, which was a model presented as a measurement.
    let mut ratios: Vec<f64> = measure()
        .into_iter()
        .filter(|r| r.compose_tokens > 0)
        .map(|r| r.discover_tokens as f64 / r.compose_tokens as f64)
        .collect();
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (lo, hi) = (ratios[0], ratios[ratios.len() - 1]);
    assert!(
        hi / lo > 100.0,
        "the spread collapsed ({lo:.1}x..{hi:.1}x) — revisit whether a single \
         figure is now honest, rather than deleting this test"
    );
}
