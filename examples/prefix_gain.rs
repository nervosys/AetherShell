//! Measure the token gain from AECON's `@prefix` common-prefix factoring on
//! path-heavy listings — the realistic case where every row shares a deep leading
//! path. Compares, with the real cl100k tokenizer:
//!   • AECON *with* @prefix (current encoder)
//!   • AECON *without* @prefix (the pre-lever bare-TSV baseline, full paths per row)
//!   • PowerShell `ConvertTo-Json -Compress` for the same rows
//!
//!   cargo run --example prefix_gain --features real-tokens
//!
//! @prefix is lossless (round-trip tested) and deterministic (char-based gate, no
//! tokenizer/float), and applies only in agent mode — human legibility is untouched.

use aethershell::builtins::{est_token_count, render_agent};
use aethershell::value::Value;
use std::collections::BTreeMap;

const PREFIX: &str = "/home/user/project/node_modules/.cache/babel-loader/";

fn rows(n: usize) -> Vec<(String, i64)> {
    (0..n)
        .map(|i| (format!("{PREFIX}{i}.json"), 1000 + i as i64 * 7))
        .collect()
}

fn as_array(data: &[(String, i64)]) -> Value {
    Value::Array(
        data.iter()
            .map(|(p, s)| {
                let mut m = BTreeMap::new();
                m.insert("path".to_string(), Value::Str(p.clone()));
                m.insert("size".to_string(), Value::Int(*s));
                Value::Record(m)
            })
            .collect(),
    )
}

/// The pre-@prefix baseline: bare TSV header + one full-path row each.
fn aecon_no_prefix(data: &[(String, i64)]) -> String {
    let mut s = String::from("path\tsize");
    for (p, sz) in data {
        s.push_str(&format!("\n{p}\t{sz}"));
    }
    s
}

/// PowerShell `ConvertTo-Json -Compress` for the same rows.
fn pwsh_json(data: &[(String, i64)]) -> String {
    let mut s = String::from("[");
    for (i, (p, sz)) in data.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(r#"{{"path":"{p}","size":{sz}}}"#));
    }
    s.push(']');
    s
}

fn main() {
    println!("AECON @prefix gain on a path-heavy listing (real cl100k BPE)");
    println!("shared prefix: {PREFIX:?} ({} chars)\n", PREFIX.len());
    println!(
        "  {:>5} | {:>10} {:>10} {:>7} | {:>9} {:>7}",
        "rows", "no-@prefix", "@prefix", "save", "pwsh -c", "vs @pfx"
    );
    for n in [5usize, 25, 100] {
        let data = rows(n);
        let with = render_agent(&as_array(&data), None).expect("aecon");
        let without = aecon_no_prefix(&data);
        let json = pwsh_json(&data);

        let t_with = est_token_count(&with);
        let t_without = est_token_count(&without);
        let t_json = est_token_count(&json);
        let save = 100.0 * (1.0 - t_with as f64 / t_without.max(1) as f64);
        println!(
            "  {:>5} | {:>10} {:>10} {:>6.1}% | {:>9} {:>6.2}x",
            n,
            t_without,
            t_with,
            save,
            t_json,
            t_json as f64 / t_with.max(1) as f64,
        );
    }
    // Sanity: confirm the @prefix line is actually present in the rendered output.
    let sample = render_agent(&as_array(&rows(25)), None).unwrap();
    let first_line = sample
        .lines()
        .find(|l| l.starts_with("@prefix"))
        .unwrap_or("(none)");
    println!("\nfactored line: {first_line}");
}
