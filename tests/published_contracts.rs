//! The documents other systems read about this one, checked against reality.
//!
//! Three published contracts describe how to call AetherShell:
//!
//!   * `.well-known/openapi.yaml`  — pointed at by IronStack's registry
//!   * `.well-known/ai-plugin.json` — read by model hosts
//!   * `.well-known/ironstack.json` — the stack registry's manifest contract
//!
//! Nothing checked whether any of them was true, and all three had drifted.
//! `openapi.yaml` documented no authentication and claimed version 1.4.0
//! against a crate at 8.0.0. `ai-plugin.json` declared `"auth": {"type":
//! "none"}` — in the file whose entire job is telling a model how to
//! authenticate. `ironstack verify` reported `ok` throughout, because it
//! compares identity, placement and dependencies, not whether a document tells
//! the truth about how to *call* the thing.
//!
//! Each was fixed by hand. This is what stops them drifting back.

use std::fs;
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn read(rel: &str) -> String {
    // `ai-plugin.json` carries a UTF-8 BOM; strip it rather than trip over it.
    let s = fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
    s.trim_start_matches('\u{feff}').to_string()
}

fn crate_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[test]
fn the_openapi_spec_declares_the_authentication_the_server_enforces() {
    let spec = read(".well-known/openapi.yaml");
    assert!(
        spec.contains("bearerAuth"),
        "openapi.yaml must declare the bearer scheme the server actually requires"
    );
    assert!(
        spec.contains("securitySchemes"),
        "a scheme referenced but never defined documents nothing"
    );
    // The global requirement, not merely a definition sitting unused.
    assert!(
        spec.contains("security:\n  - bearerAuth: []"),
        "bearerAuth must be applied globally, not just defined"
    );
}

#[test]
fn the_openapi_spec_exempts_health_exactly_as_the_server_does() {
    // The server lets `/health` through the auth middleware unchecked. A spec
    // that requires a token there would send liveness probes chasing one.
    let spec = read(".well-known/openapi.yaml");
    let health = spec
        .split("  /health:")
        .nth(1)
        .expect("openapi.yaml should define /health");
    let block: String = health.lines().take(12).collect::<Vec<_>>().join("\n");
    assert!(
        block.contains("security: []"),
        "/health must override the global security requirement, got:\n{block}"
    );
}

#[test]
fn the_plugin_manifest_does_not_claim_the_api_is_open() {
    // `"auth": {"type": "none"}` shipped for months while every route required
    // a bearer token. A model reading it is not merely missing the token — it
    // is told there is nothing to send.
    let manifest = read(".well-known/ai-plugin.json");
    assert!(
        !manifest.contains("\"type\": \"none\""),
        "ai-plugin.json must not declare `auth: none` while the server requires a token"
    );
    assert!(
        manifest.contains("bearer"),
        "ai-plugin.json must name the scheme a model is expected to use"
    );
}

#[test]
fn every_published_contract_agrees_with_the_crate_version() {
    // openapi.yaml sat at 1.4.0 across four major releases. A version that
    // never moves reads exactly like one that is current.
    let v = crate_version();
    let spec = read(".well-known/openapi.yaml");
    assert!(
        spec.contains(&format!("version: {v}")),
        "openapi.yaml should declare version {v}"
    );
}

#[test]
fn the_ironstack_manifest_matches_the_registrys_view_of_us() {
    // `ironstack verify` compares id, product, layer and depends_on. Those are
    // the fields that must not drift; assert them here too so a change is
    // caught in this repo's own CI rather than only when someone runs the
    // registry's tool.
    let manifest = read(".well-known/ironstack.json");
    for expected in [
        "\"id\": \"aethershell\"",
        "\"product\": \"AetherShell\"",
        "\"layer\": \"languages\"",
        "\"irongate\"",
        "\"ironvault\"",
    ] {
        assert!(
            manifest.contains(expected),
            "ironstack.json lost {expected}, which `ironstack verify` compares"
        );
    }
}

#[test]
fn the_contracts_are_wellformed_and_not_merely_present() {
    // The first attempt at fixing openapi.yaml added a *second* top-level
    // `components:` key. It still parsed, the last key won, and
    // `securitySchemes` silently vanished — a document that looked edited and
    // documented nothing.
    let spec = read(".well-known/openapi.yaml");
    let top_level_components = spec
        .lines()
        .filter(|l| l.starts_with("components:"))
        .count();
    assert_eq!(
        top_level_components, 1,
        "duplicate top-level `components:` keys silently discard the earlier one"
    );

    let manifest = read(".well-known/ai-plugin.json");
    serde_json::from_str::<serde_json::Value>(&manifest)
        .expect("ai-plugin.json must be valid JSON");
    let iron = read(".well-known/ironstack.json");
    serde_json::from_str::<serde_json::Value>(&iron).expect("ironstack.json must be valid JSON");
}

// ── Documented endpoints vs endpoints that exist ────────────────────────────

/// Paths declared in `openapi.yaml`, read line-wise rather than via a YAML
/// parser so this test needs no dependency the crate does not already have.
fn documented_paths() -> Vec<String> {
    read(".well-known/openapi.yaml")
        .lines()
        .filter_map(|l| {
            let t = l.strip_prefix("  ")?;
            if t.starts_with('/') && t.ends_with(':') && !t.starts_with("  ") {
                Some(t.trim_end_matches(':').to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Routes the agent API actually registers, normalised to OpenAPI's `{param}`
/// form from axum's `:param`.
fn served_routes() -> Vec<String> {
    let src = fs::read_to_string(repo("src/agent_api.rs")).expect("read agent_api.rs");
    let mut out = Vec::new();
    for (i, _) in src.match_indices(".route(\"") {
        let rest = &src[i + ".route(\"".len()..];
        if let Some(end) = rest.find('"') {
            let raw = &rest[..end];
            let mut path = String::new();
            for seg in raw.split('/') {
                if seg.is_empty() {
                    continue;
                }
                path.push('/');
                if let Some(param) = seg.strip_prefix(':') {
                    path.push('{');
                    path.push_str(param);
                    path.push('}');
                } else {
                    path.push_str(seg);
                }
            }
            out.push(path);
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn the_spec_documents_no_endpoint_that_does_not_exist() {
    // It documented nine that did not: five `marketplace/*`, three
    // `orchestration/*`, and `/api/v1/ws` — whose handler exists in the source
    // and is never routed. An agent generating a client from that spec gets
    // nine methods that 404, which is the same failure as `auth: none` pointed
    // the other way: a contract claiming capability rather than hiding a
    // requirement.
    let served = served_routes();
    let phantom: Vec<String> = documented_paths()
        .into_iter()
        .filter(|p| !served.contains(p))
        .collect();
    assert!(
        phantom.is_empty(),
        "openapi.yaml documents {} endpoint(s) the server does not serve: {:?}\n\
         Either route them or remove them — a spec is a contract, not a roadmap.",
        phantom.len(),
        phantom
    );
}

#[test]
fn the_spec_documents_every_endpoint_that_does_exist() {
    // The other direction. An undocumented route is a capability agents cannot
    // discover, which for a project whose thesis is machine-discoverability is
    // its own kind of bug.
    let documented = documented_paths();
    let missing: Vec<String> = served_routes()
        .into_iter()
        .filter(|p| !documented.contains(p))
        .collect();
    assert!(
        missing.is_empty(),
        "the server serves {} route(s) openapi.yaml does not document: {:?}",
        missing.len(),
        missing
    );
}
