//! In-process latency and throughput of the evaluator.
//!
//!   cargo run --release --example perf_probe
//!
//! Startup is measured separately, by timing the `ae` process
//! (`benches/agentic/startup.sh`); this measures what happens after it.
//! Each case runs once to warm up, then `REPS` times; the median is reported.

use aethershell::eval::eval_program;
use aethershell::parser::parse_program;
use std::time::Instant;

const REPS: usize = 7;

fn median_ms(code: &str) -> (f64, String) {
    let stmts = parse_program(code).expect("parse");
    let run = || {
        let mut env = aethershell::modules::env_with_modules();
        let t = Instant::now();
        let v = eval_program(&stmts, &mut env).expect("eval");
        (t.elapsed().as_secs_f64() * 1e3, v)
    };
    let (_, v) = run();
    let mut times: Vec<f64> = (0..REPS).map(|_| run().0).collect();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let shown = match &v {
        aethershell::value::Value::Int(n) => n.to_string(),
        aethershell::value::Value::Float(f) => format!("{f:.3}"),
        other => other.type_name().to_string(),
    };
    (times[REPS / 2], shown)
}

fn main() {
    let cases: &[(&str, &str, usize)] = &[
        // (label, code, operations it performs, for a per-op figure)
        ("builtin call x100k", "arr.range(100000) | map(fn(x) => len([x])) | sum", 100_000),
        ("lambda map x1M", "arr.range(1000000) | map(fn(x) => x * 2) | sum", 1_000_000),
        ("where x1M", "arr.range(1000000) | where(fn(x) => x % 3 == 0) | len", 1_000_000),
        ("implicit .field x100k", "arr.range(100000) | map(fn(i) => {n: i}) | where(.n > 5) | len", 100_000),
        ("string concat x100k", "arr.range(100000) | map(fn(x) => str(x) + \"-\") | len", 100_000),
        ("sort 200k", "arr.range(200000) | map(fn(x) => (x * 7919) % 200003) | sort | len", 200_000),
        ("sql over 50k rows", "arr.range(50000) | map(fn(i) => {n: i, g: i % 10}) | sql(\"select g, count(*) from t group by g\") | len", 50_000),
        ("parse+eval tiny", "1 + 1", 1),
    ];
    println!(
        "{:<24} {:>10} {:>12}   result",
        "case", "median ms", "ns / op"
    );
    for (label, code, ops) in cases {
        let (ms, shown) = median_ms(code);
        println!(
            "{label:<24} {ms:>10.2} {:>12.0}   {shown}",
            ms * 1e6 / *ops as f64
        );
    }
}
