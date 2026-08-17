//! The NERVOSYS model plane: IronGate, IronWorks, IronVault.
//!
//! Named for the layer it reaches, not for a product. `model-plane` is the
//! IronStack registry's own name for these three components; IronStack itself
//! is the registry that indexes the whole stack, and is a different thing from
//! the three this module talks to.
//!
//! AetherShell is a *frontend*. It does not decide which model serves a
//! request, it does not run inference, and it does not own where weights live.
//! Those three jobs belong to three other projects, and this module is the one
//! place that knows how to reach them:
//!
//! ```text
//!   AetherShell ──▶ IronGate ──▶ IronWorks ──▶ IronVault
//!    (frontend)     (routing)   (inference)   (model store)
//!         └──────────────────────────────────────┘
//!                  model store / conversion
//! ```
//!
//! Two edges, deliberately:
//!
//! * **Inference → IronGate only.** AetherShell never addresses IronWorks
//!   directly. Which backend serves a prompt is a routing decision, and routing
//!   is what the gateway is for; a frontend that reaches past it re-implements
//!   the escalation ladder, the budget ceiling and the circuit breaker badly.
//!   The gateway in turn reaches IronWorks, and IronWorks reaches IronVault —
//!   neither of those hops is AetherShell's business.
//!
//! * **Model store → IronVault directly.** Managing local weights is not
//!   inference traffic, so it does not belong on the gateway path. IronGate is
//!   explicit that it never talks to IronVault: where the weights live is the
//!   inference engine's concern. AetherShell's own model-management surface is
//!   a *sibling* of IronWorks here, not a client of it.
//!
//! ## Why the vault is reached through its CLI rather than its crate
//!
//! `ironvault` declares `rust-version = "1.89"`; AetherShell declares 1.88.
//! Taking the crate as a dependency would raise this project's MSRV for every
//! user, including those who never touch a model file — and it would pull a
//! second AES/Argon2/tokio stack into a shell binary. The `iv` binary exposes
//! the same capabilities with `--format json`, so the CLI is the cheaper edge.
//! Revisit if AetherShell's own MSRV ever reaches 1.89.

use anyhow::{anyhow, Context, Result};
use serde_json::Value as Json;
use std::time::Duration;

// ── IronGate: the routing edge ──────────────────────────────────────────────

/// Where IronGate's OpenAI-compatible API lives.
///
/// The default matches `irongate.example.toml`'s `[server] port = 7700`, plus
/// the `/v1` prefix its chat, models and embeddings routes are mounted under.
pub fn gate_url() -> String {
    std::env::var("IRONGATE_URL").unwrap_or_else(|_| "http://localhost:7700/v1".to_string())
}

/// The server root, for the routes that are *not* under `/v1` — `/health`,
/// `/status`, `/metrics`.
///
/// Derived from `gate_url` rather than configured separately: two environment
/// variables that must agree is a way to have them disagree.
pub fn gate_root() -> String {
    root_of(&gate_url())
}

/// The model name to ask for when the caller does not name one.
///
/// `auto` is IronGate's canonical virtual model: the frontend states the task,
/// the gateway picks the backend. Naming a concrete model here would defeat the
/// routing this integration exists to use.
pub fn gate_model() -> String {
    std::env::var("IRONGATE_MODEL").unwrap_or_else(|_| "auto".to_string())
}

/// Bearer token, when the gateway is configured with `require_auth = true`.
///
/// Absent by default: IronGate ships `require_auth = false` and is normally
/// bound to loopback.
pub fn gate_token() -> Option<String> {
    std::env::var("IRONGATE_API_KEY")
        .or_else(|_| std::env::var("IRONGATE_TOKEN"))
        .ok()
        .filter(|s| !s.is_empty())
}

/// What `GET /health` reports.
#[derive(Debug, Clone)]
pub struct GateHealth {
    /// Number of provider adapters the gateway has registered.
    pub providers: u64,
    /// Virtual model names the gateway will answer to (`auto`, `fast`, ...).
    pub models: Vec<String>,
}

fn probe_client(timeout: Duration) -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .context("building HTTP client")
}

/// Ask the gateway whether it is up, and what it will route.
///
/// Short timeout: this is called on paths where an unreachable gateway must
/// fail fast enough to fall through to something else, not stall the shell.
pub fn gate_health() -> Result<GateHealth> {
    let url = format!("{}/health", gate_root());
    let client = probe_client(Duration::from_secs(2))?;
    let resp = client
        .get(&url)
        .send()
        .with_context(|| format!("IronGate not reachable at {url}"))?
        .error_for_status()?;
    let body: Json = resp.json().context("IronGate /health returned non-JSON")?;

    Ok(GateHealth {
        providers: body["providers"].as_u64().unwrap_or(0),
        models: body["models"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// Per-target circuit state from `GET /status`.
///
/// This is how an operator sees whether the local leg of the stack — the
/// IronWorks target behind the gateway — is actually serving. AetherShell
/// reports it; it does not act on it.
pub fn gate_status() -> Result<Json> {
    let url = format!("{}/status", gate_root());
    let client = probe_client(Duration::from_secs(2))?;
    let resp = client
        .get(&url)
        .send()
        .with_context(|| format!("IronGate not reachable at {url}"))?
        .error_for_status()?;
    resp.json().context("IronGate /status returned non-JSON")
}

/// Is the gateway reachable right now?
pub fn gate_available() -> bool {
    gate_health().is_ok()
}

/// One completion from the gateway, with the routing evidence attached.
#[derive(Debug, Clone)]
pub struct GateCompletion {
    pub text: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    /// The concrete backend the gateway chose, from `x-irongate-target`.
    /// `None` when the header is absent — an older gateway, or a proxy that
    /// stripped it.
    pub target: Option<String>,
    /// Difficulty the classifier assigned, from `x-irongate-difficulty`.
    pub difficulty: Option<String>,
    pub elapsed_ms: f64,
}

/// Send one prompt through the gateway and report what came back *and* where
/// it was routed.
///
/// The routing evidence is the reason this exists rather than another bare
/// OpenAI POST: a caller that cannot see which target served a request cannot
/// tell a local answer from a metered cloud one.
pub fn gate_complete(model: &str, prompt: &str, max_tokens: Option<u32>) -> Result<GateCompletion> {
    let base = gate_url();
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));

    let mut body = serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
    });
    if let Some(n) = max_tokens {
        body["max_tokens"] = serde_json::json!(n);
    }

    let client = crate::security::create_secure_http_client()
        .context("Failed to create secure HTTP client")?;
    let mut req = client.post(&url).json(&body);
    if let Some(token) = gate_token() {
        req = req.header("Authorization", format!("Bearer {token}"));
    }

    let started = std::time::Instant::now();
    let resp = req.send().with_context(|| {
        format!("IronGate not reachable at {base}. Start it, or set IRONGATE_URL.")
    })?;

    let header = |name: &str| {
        resp.headers()
            .get(name)
            .and_then(|h| h.to_str().ok())
            .map(String::from)
    };
    let target = header("x-irongate-target");
    let difficulty = header("x-irongate-difficulty");

    let resp = resp.error_for_status()?;
    let v: Json = resp.json().context("IronGate returned non-JSON")?;
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;

    let text = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow!("IronGate response missing content field"))?
        .to_string();

    Ok(GateCompletion {
        text,
        prompt_tokens: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
        completion_tokens: v["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
        target,
        difficulty,
        elapsed_ms,
    })
}

// ── IronVault: the model store and conversion edge ──────────────────────────

/// The vault CLI to invoke. `iv` since IronVault 5.0 (it was `aim` in the
/// AI Model Vault 4.x line).
pub fn vault_bin() -> String {
    std::env::var("IRONVAULT_BIN").unwrap_or_else(|_| "iv".to_string())
}

/// Run `iv` with the given arguments and return stdout.
///
/// Errors carry the vault's own stderr rather than a generic "command failed":
/// the two failures a caller actually hits are "`iv` is not installed" and
/// "`IRONVAULT_PASSPHRASE` is not set", and only the vault can tell them apart.
pub fn vault_run(args: &[&str]) -> Result<String> {
    let bin = vault_bin();
    let out = std::process::Command::new(&bin)
        .args(args)
        .output()
        .map_err(|e| {
            anyhow!(
                "IronVault CLI '{bin}' could not be run: {e}. \
                 Install it with `cargo install ironvault`, or set IRONVAULT_BIN \
                 to its path."
            )
        })?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!(
            "`{bin} {}` failed: {}",
            args.join(" "),
            if stderr.trim().is_empty() {
                "no error output".to_string()
            } else {
                stderr.trim().to_string()
            }
        ));
    }

    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Is the vault CLI present?
pub fn vault_available() -> bool {
    std::process::Command::new(vault_bin())
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Everything the vault is storing, as the vault reports it.
///
/// `iv list` reads the encrypted inventory, so this needs `IRONVAULT_PASSPHRASE`
/// in the environment — the error surfaces from the vault itself.
pub fn vault_list() -> Result<Json> {
    let stdout = vault_run(&["list", "--format", "json"])?;
    serde_json::from_str(&stdout).context("`iv list --format json` returned non-JSON")
}

/// The conversion paths the vault actually supports.
pub fn vault_conversions() -> Result<Json> {
    let stdout = vault_run(&["list-conversions", "--format", "json"])
        .or_else(|_| vault_run(&["list-conversions"]))?;
    // `list-conversions` predates the `--format` flag on some builds; fall back
    // to reporting the text verbatim rather than failing the call.
    serde_json::from_str(&stdout).or_else(|_| Ok(Json::String(stdout.trim().to_string())))
}

/// Convert a stored model to another format.
///
/// Mirrors `iv convert <name> -t <format> [-q <quant>] [-o <path>] [--validate]`.
pub fn vault_convert(
    name: &str,
    to_format: &str,
    quantization: Option<&str>,
    output: Option<&str>,
    validate: bool,
) -> Result<String> {
    let mut args: Vec<String> = vec![
        "convert".to_string(),
        name.to_string(),
        "--to-format".to_string(),
        to_format.to_string(),
    ];
    if let Some(q) = quantization {
        args.push("--quantization".to_string());
        args.push(q.to_string());
    }
    if let Some(o) = output {
        args.push("--output".to_string());
        args.push(o.to_string());
    }
    if validate {
        args.push("--validate".to_string());
    }

    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    vault_run(&refs)
}

/// The pure half of [`gate_root`], split out so it is testable without the
/// environment.
fn root_of(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    trimmed
        .strip_suffix("/v1")
        .unwrap_or(trimmed)
        .trim_end_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    // These assert the *derivation*, which is pure. Anything requiring a
    // running gateway or an installed vault lives in tests/model_plane.rs, where
    // it can be skipped when the stack is absent.

    #[test]
    fn the_root_is_the_v1_url_without_its_suffix() {
        // Both routes are served by one process, so /health must land on the
        // same host and port as /v1/chat/completions.
        assert_eq!(
            super::root_of("http://localhost:7700/v1"),
            "http://localhost:7700"
        );
    }

    #[test]
    fn a_url_already_at_the_root_is_left_alone() {
        assert_eq!(
            super::root_of("http://gate.internal"),
            "http://gate.internal"
        );
    }

    #[test]
    fn a_trailing_slash_does_not_produce_a_double_slash() {
        // `{root}/health` is built by concatenation, so a trailing slash here
        // becomes `//health`, which some proxies do not normalise.
        assert_eq!(
            super::root_of("http://localhost:7700/v1/"),
            "http://localhost:7700"
        );
    }

    #[test]
    fn a_path_that_merely_contains_v1_is_not_truncated() {
        // Stripping "v1" anywhere rather than at the end would break a gateway
        // mounted under a prefix.
        assert_eq!(
            super::root_of("http://host/v1/gateway"),
            "http://host/v1/gateway"
        );
    }
}
