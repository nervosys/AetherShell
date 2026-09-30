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
        ("module calls x100k", "arr.range(100000) | map(fn(x) => math.abs(x)) | sum", 100_000),
        ("nested field x100k", "arr.range(100000) | map(fn(i) => {meta: {n: i}, payload: arr.range(100)}) | where(.meta.n > 5) | len", 100_000),
        ("indexed map x100k", "arr.range(100000) | map(fn(x, i) => x + i) | sum", 100_000),
        ("reduce x100k", "arr.range(100000) | reduce(fn(a, x) => a + x, 0)", 100_000),
        ("indexed reduce x100k", "arr.range(100000) | reduce(fn(a, x, i) => a + x + i, 0)", 100_000),
        ("string concat x100k", "arr.range(100000) | map(fn(x) => str(x) + \"-\") | len", 100_000),
        ("sort 200k", "arr.range(200000) | map(fn(x) => (x * 7919) % 200003) | sort | len", 200_000),
        ("sql over 50k rows", "arr.range(50000) | map(fn(i) => {n: i, g: i % 10}) | sql(\"select g, count(*) from t group by g\") | len", 50_000),
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
    // A warm request includes parsing. Time enough requests to resolve
    // sub-microsecond work; the old "parse+eval tiny" parsed outside its timer.
    for code in ["1 + 1", "math.abs(-42)", "str.upper(\"hello\")"] {
        let mut env = aethershell::modules::env_with_modules();
        let mut samples = Vec::new();
        for _ in 0..REPS {
            let start = Instant::now();
            for _ in 0..10_000 {
                let program = parse_program(std::hint::black_box(code)).expect("parse");
                std::hint::black_box(eval_program(&program, &mut env).expect("eval"));
            }
            samples.push(start.elapsed().as_secs_f64() * 1e9 / 10_000.0);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "warm parse+eval {code:<20} {:>10.0} ns / request",
            samples[REPS / 2]
        );
    }
}
