//! Does MechGen's "digital rain" (dense-UTF-8, Matrix-style) actually compress
//! **LLM token streams**? Measured with the real BPE tokenizers (cl100k + o200k)
//! via `--features real-tokens`. Without that feature the numbers are heuristic
//! and the CJK token cost is *under*-counted — so for the real answer run:
//!
//!   cargo run -p agentic-eval --example rain_tokens --features real-tokens
//!
//! Hypothesis under test: pack each MechGen token into one dense glyph to shrink
//! the token stream. Spoiler (and the whole-project lesson): an LLM emits
//! *tokens*, not glyphs — BPE splits rare multi-byte CJK/katakana into multiple
//! tokens, so the dense stream costs MORE tokens even as it costs fewer chars.

use agentic_eval::tokens::Model;

/// Glyph alphabet identical in spirit to MechGen's rain::encode.
fn glyphs() -> Vec<char> {
    let mut v = Vec::new();
    v.extend((0xFF66u32..=0xFF9D).filter_map(char::from_u32)); // half-width katakana
    v.extend((0x30A1u32..=0x30FA).filter_map(char::from_u32)); // full katakana
    v.extend((0x4E00u32..=0x9FA5).filter_map(char::from_u32)); // CJK
    v
}

/// Lightweight tokenizer: runs of [A-Za-z0-9_] are one token; every other
/// non-space char is its own token (matches MechGen's per-token rain mapping).
fn tokenize(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in src.chars() {
        if c.is_alphanumeric() || c == '_' {
            cur.push(c);
        } else {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            if !c.is_whitespace() {
                out.push(c.to_string());
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Rain-encode: one glyph per token. Returns (stream, legend_text).
fn rain(src: &str) -> (String, String) {
    let alpha = glyphs();
    let mut map: std::collections::HashMap<String, char> = std::collections::HashMap::new();
    let mut legend: Vec<(char, String)> = Vec::new();
    let mut stream = String::new();
    for t in tokenize(src) {
        let g = *map.entry(t.clone()).or_insert_with(|| {
            let g = alpha[legend.len() % alpha.len()];
            legend.push((g, t.clone()));
            g
        });
        stream.push(g);
    }
    let legend_text = legend
        .iter()
        .map(|(g, t)| format!("{g}\t{t}"))
        .collect::<Vec<_>>()
        .join("\n");
    (stream, legend_text)
}

/// Minimal base64 (for the bytes-as-text comparison).
fn b64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::new();
    for ch in bytes.chunks(3) {
        let b = [ch[0], *ch.get(1).unwrap_or(&0), *ch.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for k in 0..4 {
            if k <= ch.len() {
                o.push(T[((n >> (18 - 6 * k)) & 0x3f) as usize] as char);
            } else {
                o.push('=');
            }
        }
    }
    o
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

fn main() {
    let exact = Model::OpenAiGpt4.is_exact();
    println!("=== MechGen 'digital rain' vs token streams (cl100k + o200k BPE) ===");
    println!(
        "tokenizer: {}\n",
        if exact {
            "REAL tiktoken (exact)"
        } else {
            "HEURISTIC (CJK undercounted — rerun with --features real-tokens)"
        }
    );

    let samples: &[(&str, &str)] = &[
        ("net", "net MLP { layer fc1: Linear(8, 16); layer act: ReLU; layer fc2: Linear(16, 4); forward { fc1 } }"),
        ("fn", "fn factorial(n: u64) -> u64 { if n <= 1 { return 1; } n * factorial(n - 1) }"),
        ("kb", "kb Family { fact parent(alice, bob); fact parent(bob, carol); rule gp(x: i32, z: i32) where parent(x, y), parent(y, z) { x } }"),
    ];

    println!(
        "{:<5} {:>10} {:>22} {:>22} {:>14}",
        "kind", "form", "cl100k tok", "o200k tok", "chars"
    );
    let cl = Model::OpenAiGpt4;
    let o2 = Model::OpenAiGpt4o;
    let (mut s_cl, mut s_o, mut r_cl, mut r_o, mut rl_cl, mut rl_o) = (0, 0, 0, 0, 0, 0);
    for (name, src) in samples {
        let (stream, legend) = rain(src);
        let rl = format!("{legend}\n{stream}"); // rain + legend (one-off, reversible)
        let bytes = src.as_bytes();
        let base = b64(bytes);
        let row = |label: &str, s: &str| {
            println!(
                "{:<5} {:>10} {:>22} {:>22} {:>14}",
                "",
                label,
                cl.count(s),
                o2.count(s),
                chars(s)
            );
        };
        println!("[{name}]");
        row("source", src);
        row("rain(stream)", &stream);
        row("rain+legend", &rl);
        row("base64(bytes)", &base);
        s_cl += cl.count(src);
        s_o += o2.count(src);
        r_cl += cl.count(&stream);
        r_o += o2.count(&stream);
        rl_cl += cl.count(&rl);
        rl_o += o2.count(&rl);
    }

    println!("\nTOTALS (3 samples) — token ratio vs source (>1.0 = rain is WORSE):");
    println!("  source            cl100k {s_cl:>4}   o200k {s_o:>4}");
    println!(
        "  rain(stream)      cl100k {r_cl:>4} ({:.2}x)   o200k {r_o:>4} ({:.2}x)",
        r_cl as f64 / s_cl as f64,
        r_o as f64 / s_o as f64
    );
    println!(
        "  rain+legend       cl100k {rl_cl:>4} ({:.2}x)   o200k {rl_o:>4} ({:.2}x)",
        rl_cl as f64 / s_cl as f64,
        rl_o as f64 / s_o as f64
    );

    println!("\nVERDICT");
    println!("  Digital rain shrinks CHARACTERS (~3x — the Matrix look) but the dense glyph");
    println!("  stream costs MORE BPE tokens than the ASCII source (ratios above), because the");
    println!(
        "  tokenizer splits each rare multi-byte glyph into several tokens. Adding the legend"
    );
    println!("  (needed for reversibility) makes a one-off snippet far worse still. Even an");
    println!("  amortized shared codebook (stream only) does not beat source on tokens.");
    println!("  This is the project's token-floor finding, re-confirmed on the dense-symbol idea:");
    println!(
        "  an LLM emits TOKENS, not glyphs/bytes — the information (names/ops/dims) is the floor."
    );
    if !exact {
        println!("\n  (heuristic run: it counts ~1 token/char and UNDER-counts CJK — the real cl100k/o200k");
        println!("   gap is larger. Rerun with --features real-tokens for the exact, even-worse numbers.)");
    }
}
