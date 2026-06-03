//! A complete, self-describing **ontology** of `agentic-eval` itself.
//!
//! Agentic-first design means a consumer should never have to read prose docs to
//! use the library — it should be able to *discover* every capability through a
//! compact, machine-readable manifest and expand any entry on demand. That is the
//! same progressive-disclosure pattern this crate measures (a cheap root index +
//! `describe(...)` for detail), applied reflexively to the crate's own surface:
//! its four [axes](crate), the [`Effect`] taxonomy with the policy [`Decision`] each
//! effect gets under each [`Mode`], the supported tokenizer [`Model`]s, and the
//! built-in CLI command classifier.
//!
//! The ontology is **deterministic** (built from static data in fixed order, no map
//! iteration), **compact** ([`manifest`] is a few hundred tokens), and
//! **machine-readable** (every type derives `serde::Serialize` under the `serde`
//! feature). Start at [`manifest`], expand with [`describe`], or take the whole
//! structured catalog with [`ontology`].
//!
//! ```
//! // Discover the surface without reading docs:
//! let root = agentic_eval::ontology::manifest();
//! assert!(root.contains("axes:"));
//! // Expand one entry on demand:
//! let safety = agentic_eval::ontology::describe("safety").unwrap();
//! assert!(safety.contains("assess_safety"));
//! let rm = agentic_eval::ontology::describe("destructive").unwrap();
//! assert!(rm.contains("agent=approve")); // the policy decision, machine-checkable
//! ```

use crate::commands::{commands_for, known_command_count};
use crate::safety::{Effect, Mode};
use crate::tokens::Model;

/// The crate's own version (so the ontology never drifts from the build).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One of the four evaluation axes — what it measures and how to invoke it.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct AxisDoc {
    /// Axis name (`tokens` / `determinism` / `reliability` / `safety`).
    pub name: &'static str,
    /// One-line summary of what the axis scores.
    pub summary: &'static str,
    /// The public entry-point functions for this axis.
    pub entry_points: &'static [&'static str],
    /// Whether assessing it needs the program to *run* (a caller closure), vs.
    /// working on text/declared effects alone.
    pub needs_execution: bool,
    /// The report type the axis produces.
    pub output_type: &'static str,
}

/// An effect class with the policy decision it receives under each mode.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct EffectDoc {
    /// Canonical snake_case effect name.
    pub name: &'static str,
    /// One-line description of the effect class.
    pub summary: &'static str,
    /// Whether the class is dangerous (should be gated for an agent).
    pub dangerous: bool,
    /// The decision a human gets (`allow` / `approve` / `deny`).
    pub human_decision: &'static str,
    /// The decision an agent gets under the default policy.
    pub agent_decision: &'static str,
    /// A few CLI commands the built-in classifier maps to this effect.
    pub example_commands: Vec<&'static str>,
}

/// A supported tokenizer/model.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct ModelDoc {
    /// Display name (tokenizer family).
    pub name: &'static str,
    /// Whether this build counts it exactly (a real BPE) vs. an estimate.
    pub exact: bool,
}

/// The complete, structured ontology of the crate.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct Ontology {
    /// Crate name.
    pub crate_name: &'static str,
    /// Crate version (matches the build).
    pub version: &'static str,
    /// One-line description of the crate's purpose.
    pub summary: &'static str,
    /// The four evaluation axes.
    pub axes: Vec<AxisDoc>,
    /// The effect taxonomy with per-mode policy decisions.
    pub effects: Vec<EffectDoc>,
    /// The operating modes the policy distinguishes.
    pub modes: Vec<&'static str>,
    /// The tokenizer models token efficiency can be counted under.
    pub models: Vec<ModelDoc>,
    /// How many CLI commands the built-in classifier recognizes.
    pub known_commands: usize,
}

/// The four evaluation axes, in canonical order.
pub fn axes() -> Vec<AxisDoc> {
    vec![
        AxisDoc {
            name: "tokens",
            summary: "token efficiency: the four cost terms an agent pays — standing \
                      context, input, output, retries — amortized over a session; plus \
                      output scaling (per-item cost) and prompt-cache savings",
            entry_points: &[
                "evaluate",
                "evaluate_with",
                "compare",
                "rank",
                "rank_with",
                "assess_scaling",
                "assess_cache",
                "cacheable_prefix_tokens",
            ],
            needs_execution: false,
            output_type: "AgentCost | ScalingReport | CacheReport",
        },
        AxisDoc {
            name: "determinism",
            summary: "whether a program's output is byte-identical across repeated runs \
                      (so an agent can parse, cache, and diff it)",
            entry_points: &["assess_determinism", "stable_across"],
            needs_execution: true,
            output_type: "DeterminismReport",
        },
        AxisDoc {
            name: "reliability",
            summary: "success rate over representative invocations, whether failures are \
                      structured/actionable rather than dead ends, and graded error \
                      quality (code/message/location/fix)",
            entry_points: &["assess_reliability", "assess_error_quality"],
            needs_execution: true,
            output_type: "ReliabilityReport | ErrorQualityReport",
        },
        AxisDoc {
            name: "safety",
            summary: "the fraction of a program's dangerous blast radius that is gated \
                      (approval/denied) under an agent policy; plus reversibility \
                      (recoverable blast radius) and data-exfiltration exposure",
            entry_points: &[
                "assess_safety",
                "assess_safety_named",
                "assess_safety_script",
                "assess_reversibility",
                "assess_exfiltration",
            ],
            needs_execution: false,
            output_type: "SafetyReport | ReversibilityReport | ExfiltrationReport",
        },
    ]
}

/// The effect taxonomy, each annotated with the policy decision it gets under each
/// mode and a few example commands the built-in classifier maps to it.
pub fn effects() -> Vec<EffectDoc> {
    Effect::all()
        .into_iter()
        .map(|e| EffectDoc {
            name: e.name(),
            summary: e.summary(),
            dangerous: e.is_dangerous(),
            human_decision: e.decision(Mode::Human).name(),
            agent_decision: e.decision(Mode::Agent).name(),
            example_commands: commands_for(e).iter().take(4).copied().collect(),
        })
        .collect()
}

/// The tokenizer models token efficiency can be counted under (exact or estimated).
pub fn models() -> Vec<ModelDoc> {
    Model::all()
        .into_iter()
        .map(|m| ModelDoc {
            name: m.name(),
            exact: m.is_exact(),
        })
        .collect()
}

/// The complete structured ontology of the crate.
pub fn ontology() -> Ontology {
    Ontology {
        crate_name: "agentic-eval",
        version: VERSION,
        summary: "evaluate programs for agentic AI use across four axes — token \
                  efficiency, determinism, reliability, and safety",
        axes: axes(),
        effects: effects(),
        modes: Mode::all().iter().map(|m| m.name()).collect(),
        models: models(),
        known_commands: known_command_count(),
    }
}

/// A compact, deterministic **manifest** — the cheap discovery root. Lists the axes,
/// effect classes, modes, models, and command count, plus how to expand any entry
/// with [`describe`]. A few hundred tokens; the agentic-first entry point.
pub fn manifest() -> String {
    let o = ontology();
    let mut s = String::new();
    s.push_str(&format!("{} {} — {}\n", o.crate_name, o.version, o.summary));
    s.push_str("axes: ");
    s.push_str(&o.axes.iter().map(|a| a.name).collect::<Vec<_>>().join(", "));
    s.push_str(&format!("\neffects({}): ", o.effects.len()));
    s.push_str(
        &o.effects
            .iter()
            .map(|e| e.name)
            .collect::<Vec<_>>()
            .join(" "),
    );
    s.push_str("\nmodes: ");
    s.push_str(&o.modes.join(", "));
    s.push_str("\nmodels: ");
    s.push_str(
        &o.models
            .iter()
            .map(|m| m.name)
            .collect::<Vec<_>>()
            .join(", "),
    );
    s.push_str(&format!(
        "\ncommands: {} classified across {} effect classes",
        o.known_commands,
        o.effects.len()
    ));
    s.push_str("\ndescribe(<axis|effect|model|\"axes\"|\"effects\"|\"models\">) for detail");
    s
}

/// Expand one ontology entry by name (case-insensitive): an axis (`tokens`…), an
/// effect (`destructive`…), a model (`gpt4`/`claude`…), or a section keyword
/// (`axes` / `effects` / `models` / `modes` / `commands`). Returns `None` for an
/// unknown query — the [`manifest`] lists every valid name.
pub fn describe(query: &str) -> Option<String> {
    let q = query.trim().to_ascii_lowercase();
    let o = ontology();

    // Section keywords expand the whole group.
    match q.as_str() {
        "axes" => {
            return Some(
                o.axes
                    .iter()
                    .map(describe_axis)
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
        "effects" => {
            return Some(
                o.effects
                    .iter()
                    .map(describe_effect)
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
        "models" => {
            return Some(
                o.models
                    .iter()
                    .map(|m| format!("{} (exact={})", m.name, m.exact))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
        "modes" => return Some(o.modes.join(", ")),
        "commands" => {
            return Some(format!(
                "{} CLI commands classified; describe an effect (e.g. \"network\") for examples",
                o.known_commands
            ))
        }
        _ => {}
    }

    // A specific axis.
    if let Some(a) = o.axes.iter().find(|a| a.name == q) {
        return Some(describe_axis(a));
    }
    // A specific effect (by canonical name).
    if let Some(e) =
        Effect::from_name(&q).and_then(|e| o.effects.iter().find(|d| d.name == e.name()))
    {
        return Some(describe_effect(e));
    }
    // A specific model (accepts the same aliases as Model::from_name).
    if let Some(m) = Model::from_name(&q) {
        return Some(format!("{} (exact={})", m.name(), m.is_exact()));
    }
    None
}

fn describe_axis(a: &AxisDoc) -> String {
    format!(
        "axis {}: {}\n  output: {}  needs_execution: {}\n  entry_points: {}",
        a.name,
        a.summary,
        a.output_type,
        a.needs_execution,
        a.entry_points.join(", ")
    )
}

fn describe_effect(e: &EffectDoc) -> String {
    format!(
        "effect {}: {}\n  dangerous: {}  human={}  agent={}\n  e.g. {}",
        e.name,
        e.summary,
        e.dangerous,
        e.human_decision,
        e.agent_decision,
        if e.example_commands.is_empty() {
            "(none)".to_string()
        } else {
            e.example_commands.join(", ")
        }
    )
}

impl std::fmt::Display for Ontology {
    /// The manifest followed by every axis and effect expanded — a complete,
    /// human-readable dump of the ontology.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", manifest())?;
        writeln!(f, "\n# axes")?;
        for a in &self.axes {
            writeln!(f, "{}", describe_axis(a))?;
        }
        writeln!(f, "\n# effects")?;
        for e in &self.effects {
            writeln!(f, "{}", describe_effect(e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_compact_and_lists_every_section() {
        let m = manifest();
        assert!(m.contains("agentic-eval"));
        assert!(m.contains(VERSION));
        for axis in ["tokens", "determinism", "reliability", "safety"] {
            assert!(m.contains(axis), "manifest lists axis {axis}: {m}");
        }
        // Every effect name appears.
        for e in Effect::all() {
            assert!(m.contains(e.name()), "manifest lists effect {}", e.name());
        }
        // Compact: a few hundred tokens, not a doc dump.
        assert!(m.len() < 1200, "manifest stays compact ({} bytes)", m.len());
    }

    #[test]
    fn ontology_is_complete_and_consistent() {
        let o = ontology();
        assert_eq!(o.axes.len(), 4);
        assert_eq!(o.effects.len(), 8); // every Effect variant
        assert_eq!(o.modes.len(), 2);
        assert_eq!(o.models.len(), 4);
        assert!(o.known_commands > 100, "classifier ontology is substantial");
        // The effect docs carry the real policy decisions.
        let destructive = o.effects.iter().find(|e| e.name == "destructive").unwrap();
        assert!(destructive.dangerous);
        assert_eq!(destructive.human_decision, "allow");
        assert_eq!(destructive.agent_decision, "approve");
        let privileged = o.effects.iter().find(|e| e.name == "privileged").unwrap();
        assert_eq!(privileged.agent_decision, "deny");
    }

    #[test]
    fn describe_expands_axes_effects_models_and_keywords() {
        assert!(describe("safety").unwrap().contains("assess_safety"));
        assert!(describe("TOKENS").unwrap().contains("AgentCost")); // case-insensitive
        let dest = describe("destructive").unwrap();
        assert!(dest.contains("agent=approve"));
        // Model aliases resolve.
        assert!(describe("gpt4").unwrap().contains("cl100k"));
        // Section keywords expand the whole group.
        assert!(describe("effects").unwrap().contains("privileged"));
        assert!(describe("models").unwrap().contains("heuristic"));
        // Unknown query → None (manifest lists the valid names).
        assert!(describe("does-not-exist").is_none());
    }

    #[test]
    fn manifest_and_describe_are_deterministic() {
        assert_eq!(manifest(), manifest());
        assert_eq!(describe("effects"), describe("effects"));
        // The full Display dump is stable too.
        assert_eq!(ontology().to_string(), ontology().to_string());
    }

    #[test]
    fn version_matches_the_crate() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
        assert!(manifest().contains(env!("CARGO_PKG_VERSION")));
    }
}
