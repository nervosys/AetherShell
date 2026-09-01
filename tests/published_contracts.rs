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
    //
    // Line endings are normalised too: these files land as CRLF on a Windows
    // checkout, so an assertion written against a bare newline fails for every
    // Windows contributor while passing for whoever wrote it. Same shape as the
    // `ls.path` example that was only true on one machine -- and it caught this
    // very test out within an hour of it being written.
    let s = fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
    s.trim_start_matches('\u{feff}').replace("\r\n", "\n")
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
///
/// Scans for `.route(` and then the next string literal, because the path is
/// not always on the same line. An earlier version matched only
/// `.route("path"` and therefore missed the entire `long_lived` router --
/// `/api/v1/ws` and the SSE streams -- along with every multi-line
/// registration. It reported nine real endpoints as phantom and I deleted
/// their documentation on the strength of it. A source scan that is wrong
/// about what it cannot see is worse than no scan: it is confident.
fn served_routes() -> Vec<String> {
    let src = fs::read_to_string(repo("src/agent_api.rs")).expect("read agent_api.rs");
    let mut out = Vec::new();
    let mut rest = src.as_str();
    while let Some(i) = rest.find(".route(") {
        rest = &rest[i + ".route(".len()..];
        let Some(q) = rest.find('"') else { break };
        // Only accept a quote that opens before any `)` closes the call.
        if rest[..q].contains(')') {
            continue;
        }
        let after = &rest[q + 1..];
        let Some(end) = after.find('"') else { break };
        let raw = &after[..end];
        if !raw.starts_with('/') {
            continue;
        }
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

/// The Homebrew formula names a release tag, and nothing checked it.
///
/// It was pinned at `v10.0.0` while the crate reached 11.0.2 — four releases
/// of drift, so `brew install` built a version nobody was shipping any more.
/// It had drifted before too: an earlier pass found it pinned at `v0.2.0` with
/// a literal `PLACEHOLDER_SHA256`, declaring Apache-2.0 for AGPL code and
/// invoking a subcommand that does not exist, which meant it could not have
/// installed anyone at all.
///
/// A formula is a published contract like the others in this file: it tells
/// someone else how to obtain this software. So it gets the same treatment —
/// checked against the crate version rather than trusted.
#[test]
fn the_homebrew_formula_names_the_current_release() {
    let formula = read("Formula/aethershell.rb");
    let version = env!("CARGO_PKG_VERSION");

    let expected_tag = format!("/v{version}.tar.gz");
    assert!(
        formula.contains(&expected_tag),
        "the formula's url does not point at v{version}; it will build whatever \
         tag it does name, which is how it ended up four releases behind"
    );

    // A real digest, not a placeholder. The earlier version shipped
    // `PLACEHOLDER_SHA256`, which fails the install rather than the review.
    let digest = formula
        .lines()
        .find(|l| l.trim_start().starts_with("sha256 "))
        .expect("formula declares a sha256");
    let hex: String = digest
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_lowercase();
    assert!(
        hex.len() >= 64,
        "the formula's sha256 is not a 64-character digest: {digest}"
    );
}

/// The formula must state the licence the crate actually carries. It said
/// Apache-2.0 for AGPL-3.0-or-later code — permissive versus copyleft, which is
/// not a typo but a materially wrong claim about what a user may do.
#[test]
fn the_homebrew_formula_states_the_real_licence() {
    let formula = read("Formula/aethershell.rb");
    let manifest = read("Cargo.toml");
    let crate_licence = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("license "))
        .and_then(|l| l.split('"').nth(1))
        .expect("Cargo.toml declares a licence")
        .to_string();
    assert!(
        formula.contains(&format!("license \"{crate_licence}\"")),
        "the formula does not declare {crate_licence:?}, which is what the crate \
         carries"
    );
}
