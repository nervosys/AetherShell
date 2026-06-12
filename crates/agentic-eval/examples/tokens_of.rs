//! Real-BPE token count of the files passed as arguments. Zero assumptions: it
//! counts the exact bytes of each file with the real cl100k + o200k tiktoken
//! (when built `--features real-tokens`), so token figures are tied to the same
//! source that was compiled and executed elsewhere — not a paraphrased snippet.
//!
//!   cargo run -p agentic-eval --example tokens_of --features real-tokens -- FILE...

use agentic_eval::tokens::Model;
use std::fs;

fn main() {
    let cl = Model::OpenAiGpt4;
    let o2 = Model::OpenAiGpt4o;
    println!(
        "tokenizer exact: cl100k={} o200k={}",
        cl.is_exact(),
        o2.is_exact()
    );
    println!("{:>7} {:>7}   file", "cl100k", "o200k");
    for path in std::env::args().skip(1) {
        match fs::read_to_string(&path) {
            Ok(s) => println!("{:>7} {:>7}   {}", cl.count(&s), o2.count(&s), path),
            Err(e) => println!("    ERR     ERR   {path}: {e}"),
        }
    }
}
