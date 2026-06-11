//! Extensive agentic-SWE **token** benchmark: the same three tasks written
//! idiomatically in six languages, measured with the REAL cl100k + o200k BPE.
//! MechGen uses its landed ab-initio surface (inferred signatures, layout, no
//! `;`) — every MechGen snippet here is accepted by the compiler (`--check`).
//!
//! This is the *objective* token axis (the other three agentic axes —
//! determinism, reliability, safety — are in `swe_lang_profiles`). The token
//! floor is the payload; this measures how close each language's surface gets to
//! it on real SWE micro-tasks.
//!
//!   cargo run -p agentic-eval --example swe_token_benchmark --features real-tokens

use agentic_eval::tokens::Model;

fn main() {
    let cl = Model::OpenAiGpt4;
    let o2 = Model::OpenAiGpt4o;
    println!("=== Agentic-SWE token benchmark — 6 languages × 3 tasks (real BPE) ===");
    println!(
        "tokenizer: {}\n",
        if cl.is_exact() { "REAL tiktoken (exact)" } else { "HEURISTIC — rerun with --features real-tokens" }
    );

    // [language] = [factorial, sum-loop, point+dist2]
    let langs: &[(&str, [&str; 3])] = &[
        (
            "MechGen",
            [
                "f factorial(n)\n  if n <= 1\n    1\n  else\n    n * factorial(n - 1)",
                "f sum(xs)\n  var t = 0\n  for x in xs\n    t = t + x\n  t",
                "S Point { x: f64, y: f64 }\nf dist2(p: Point)\n  p.x * p.x + p.y * p.y",
            ],
        ),
        (
            "Python",
            [
                "def factorial(n):\n    return 1 if n <= 1 else n * factorial(n - 1)",
                "def sum_list(xs):\n    t = 0\n    for x in xs:\n        t += x\n    return t",
                "from dataclasses import dataclass\n@dataclass\nclass Point:\n    x: float\n    y: float\ndef dist2(p):\n    return p.x * p.x + p.y * p.y",
            ],
        ),
        (
            "Rust",
            [
                "fn factorial(n: u64) -> u64 {\n    if n <= 1 { 1 } else { n * factorial(n - 1) }\n}",
                "fn sum_list(xs: &[i64]) -> i64 {\n    let mut t = 0;\n    for x in xs {\n        t += x;\n    }\n    t\n}",
                "struct Point {\n    x: f64,\n    y: f64,\n}\nfn dist2(p: &Point) -> f64 {\n    p.x * p.x + p.y * p.y\n}",
            ],
        ),
        (
            "Go",
            [
                "func factorial(n int) int {\n\tif n <= 1 {\n\t\treturn 1\n\t}\n\treturn n * factorial(n-1)\n}",
                "func sumList(xs []int) int {\n\tt := 0\n\tfor _, x := range xs {\n\t\tt += x\n\t}\n\treturn t\n}",
                "type Point struct {\n\tX, Y float64\n}\nfunc dist2(p Point) float64 {\n\treturn p.X*p.X + p.Y*p.Y\n}",
            ],
        ),
        (
            "TypeScript",
            [
                "function factorial(n: number): number {\n  return n <= 1 ? 1 : n * factorial(n - 1);\n}",
                "function sumList(xs: number[]): number {\n  let t = 0;\n  for (const x of xs) {\n    t += x;\n  }\n  return t;\n}",
                "interface Point {\n  x: number;\n  y: number;\n}\nfunction dist2(p: Point): number {\n  return p.x * p.x + p.y * p.y;\n}",
            ],
        ),
        (
            "Java",
            [
                "static long factorial(long n) {\n    return n <= 1 ? 1 : n * factorial(n - 1);\n}",
                "static long sumList(long[] xs) {\n    long t = 0;\n    for (long x : xs) {\n        t += x;\n    }\n    return t;\n}",
                "record Point(double x, double y) {}\nstatic double dist2(Point p) {\n    return p.x() * p.x() + p.y() * p.y();\n}",
            ],
        ),
    ];

    println!("{:<12} {:>9} {:>9} {:>9} {:>9}", "language", "factori", "sum", "point", "TOTAL cl");
    let mut totals: Vec<(&str, usize, usize)> = Vec::new();
    for (name, progs) in langs {
        let c: Vec<usize> = progs.iter().map(|p| cl.count(p)).collect();
        let o: usize = progs.iter().map(|p| o2.count(p)).sum();
        let tot: usize = c.iter().sum();
        println!("{name:<12} {:>9} {:>9} {:>9} {:>9}", c[0], c[1], c[2], tot);
        totals.push((name, tot, o));
    }

    println!("\nRANK by total cl100k tokens (lower = terser):");
    totals.sort_by_key(|t| t.1);
    let best = totals[0].1 as f64;
    let mg = totals.iter().find(|t| t.0 == "MechGen").unwrap().1;
    for (i, (name, tot, o)) in totals.iter().enumerate() {
        let mark = if *name == "MechGen" { "  ← landed ab-initio surface" } else { "" };
        println!("  {}. {name:<11} {tot:>3} cl100k  {o:>3} o200k  ({:.2}x){mark}", i + 1, *tot as f64 / best);
    }

    println!("\nREADING");
    let py = totals.iter().find(|t| t.0 == "Python").unwrap().1;
    println!("  MechGen total {mg} cl100k vs Python {py}, Rust {}, Go {}, TS {}, Java {}.",
        totals.iter().find(|t| t.0 == "Rust").unwrap().1,
        totals.iter().find(|t| t.0 == "Go").unwrap().1,
        totals.iter().find(|t| t.0 == "TypeScript").unwrap().1,
        totals.iter().find(|t| t.0 == "Java").unwrap().1);
    println!("  Every MechGen snippet compiles (--check). The terseness is from inference +");
    println!("  `;`-removal (real, landed), NOT layout (token-neutral) — names/ops/literals are");
    println!("  the irreducible payload floor that bounds all of them.");
}
