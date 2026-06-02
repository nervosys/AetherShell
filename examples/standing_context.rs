//! Size AetherShell's **standing context** — the tokens an agent harness pays per
//! turn to know what tools exist — under three disclosure strategies, with the real
//! cl100k tokenizer:
//!   • manifest   — `ontology_manifest()`: categories + counts + effects only.
//!   • tool specs — every builtin as `{name, description, signature, effect}` (the
//!                  "expose all builtins as MCP tools up front" form).
//!   • full detail— every builtin's full `ontology_describe` (params + examples).
//!
//!   cargo run --example standing_context --features real-tokens
//!
//! Standing context is re-sent every turn, so its cost multiplies by turn count.
//! Progressive disclosure (ship the manifest, expand on demand) is orthogonal to
//! determinism/reliability/safety/legibility — it changes none of the result
//! semantics, only how much of the catalog rides along each turn.

use aethershell::agent_api::{builtin_tool_specs, ontology_describe_json, ontology_manifest_json};
use aethershell::builtins::est_token_count;

fn toks(j: &serde_json::Value) -> usize {
    est_token_count(&serde_json::to_string(j).unwrap_or_default())
}

fn main() {
    let manifest = ontology_manifest_json();
    let specs = builtin_tool_specs();
    let total = specs.len();

    // Full detail = every builtin's complete describe(), as one array.
    let details: Vec<serde_json::Value> = specs
        .iter()
        .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
        .map(ontology_describe_json)
        .collect();

    let manifest_tok = toks(&manifest);
    let specs_tok = toks(&serde_json::Value::Array(specs));
    let detail_tok = toks(&serde_json::Value::Array(details));

    println!("AetherShell standing-context size ({total} builtins, real cl100k BPE)\n");
    println!(
        "  {:<26}{:>9}{:>12}{:>16}",
        "strategy", "tokens", "vs manifest", "per-10-turn cost"
    );
    println!("  {}", "-".repeat(63));
    for (label, t) in [
        ("manifest (compact root)", manifest_tok),
        ("all tool specs (flat)", specs_tok),
        ("full detail (params+ex)", detail_tok),
    ] {
        println!(
            "  {:<26}{:>9}{:>11.1}x{:>16}",
            label,
            t,
            t as f64 / manifest_tok.max(1) as f64,
            t * 10,
        );
    }

    println!(
        "\nProgressive disclosure ships the manifest ({manifest_tok} tok) and expands a\n\
         category/builtin only when used. Loading all tool specs up front instead costs\n\
         {specs_tok} tok *every turn* ({:.0}x the manifest); full detail costs {detail_tok} ({:.0}x).\n\
         Over a 10-turn session that is {} vs {} vs {} tokens of pure catalog overhead.",
        specs_tok as f64 / manifest_tok.max(1) as f64,
        detail_tok as f64 / manifest_tok.max(1) as f64,
        manifest_tok * 10,
        specs_tok * 10,
        detail_tok * 10,
    );
}
