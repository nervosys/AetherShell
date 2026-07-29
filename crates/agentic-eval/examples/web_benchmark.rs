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
    print!(
        "{}",
        compare_web_stacks(WebStack::Spine, WebStack::OpenAiApi)
    );

    // ── Evidence behind SPINE's profile ─────────────────────────────────────
    println!("\nwhy SPINE scores where it does:");
    for e in &profile(WebStack::Spine).evidence {
        println!("  - {e}");
    }

    println!(
        "\nReading: SPINE now leads the composite (0.90), edging gRPC (0.83).\n\
         It was always strong on the agent-native axes it was designed for\n\
         (LLM StreamStart/Token/End frames with multiplex-aware StreamCancel +\n\
         mid-stream usage, a CapabilityQuery handshake, inline W3C TraceContext).\n\
         v1.4.0's CBOR wire format plus v1.5.0's byte-string tensor payloads\n\
         bring encoding to parity with protobuf (0.95; 89% smaller embedding\n\
         frames), and per-message Ed25519 signed frames give message-level\n\
         non-repudiation beyond channel mTLS (security 0.95). The inflection is\n\
         three deployable bridges: a runnable MCP stdio server (v1.6.0), the\n\
         OpenAI-compatible gateway, and a production-grade gRPC AgentService\n\
         (v1.8.0, reflection-enabled + real-model-backed in v1.9.0) — reachable\n\
         from the three dominant agent ecosystems with standard client stubs,\n\
         lifting interop 0.15 -> 0.67. Honest\n\
         caveat: interop is still SPINE's weakest axis — the bridges map the\n\
         agentic surface (not SPINE's native binary frames) and SPINE's own\n\
         protocol has ~zero native install base."
    );
}
