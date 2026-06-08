//! Benchmark **web stacks / wire protocols** for agentic AI use — the
//! protocol an agent actually has to speak when it calls another service.
//!
//! Ranks SPINE against the OpenAI API, Anthropic API, MCP, gRPC, plain
//! HTTP+JSON, and GraphQL on five agent-native axes (streaming,
//! tool-discoverability, encoding-efficiency, interop, security-primitives),
//! then shows the SPINE-vs-OpenAI head-to-head and the evidence.
//!
//! Run: `cargo run -p agentic-eval --example web_benchmark`

use agentic_eval::web::{compare_web_stacks, profile, rank_web_stacks, WebStack};

fn main() {
    println!("agentic-eval — web stacks / wire protocols for agentic AI use");
    println!("axes: streaming, tool-discoverability, encoding, interop, security\n");

    // ── Ranked benchmark (best-first by composite agentic fitness) ───────────
    println!(
        "{:<15} {:>7}   {:>9} {:>5} {:>8} {:>7} {:>8}",
        "stack", "fitness", "streaming", "tools", "encoding", "interop", "security"
    );
    for p in rank_web_stacks() {
        println!(
            "{:<15} {:>7.2}   {:>9.2} {:>5.2} {:>8.2} {:>7.2} {:>8.2}",
            p.stack.name(),
            p.fitness(),
            p.streaming,
            p.tool_discoverability,
            p.encoding_efficiency,
            p.interop,
            p.security_primitives,
        );
    }

    // ── Head-to-head: SPINE vs the OpenAI API (the dominant baseline) ───────
    println!("\nhead-to-head (positive = SPINE fits agentic use better):");
    print!("{}", compare_web_stacks(WebStack::Spine, WebStack::OpenAiApi));

    // ── Evidence behind SPINE's profile ─────────────────────────────────────
    println!("\nwhy SPINE scores where it does:");
    for e in &profile(WebStack::Spine).evidence {
        println!("  - {e}");
    }

    println!(
        "\nReading: SPINE leads on the agent-native axes it was designed for\n\
         (LLM-native StreamStart/Token/End frames including encoded latents, a\n\
         CapabilityQuery handshake, inline W3C TraceContext, and a\n\
         secure-by-default auth contract as of v1.3.0). As of v1.4.0 its CBOR\n\
         binary wire format moved encoding from a weakness (0.65) to near-top-\n\
         tier (0.92) — 86% smaller embedding frames, 60% smaller capability\n\
         ads — sitting just behind protobuf. gRPC still leads the composite on\n\
         raw protobuf density + mTLS-class security + broad interop. The OpenAI\n\
         API wins interop by network effect — every SDK already speaks it.\n\
         SPINE's remaining gap is interop; the gateway's OpenAI-compatible\n\
         /v1/chat/completions, /v1/embeddings, and /v1/agentic/{{capabilities,\n\
         codecs}} routes are the migration bridge."
    );
}
