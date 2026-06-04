//! Evaluating **AI frameworks** for agentic AI use.
//!
//! Where [`languages`](crate::languages) profiles the language an agent writes
//! in, this module profiles the *AI framework* an agent builds with — the
//! library it must discover, drive, and debug autonomously. Same four axes,
//! framework-flavored:
//!
//! - **token efficiency** — how many tokens a working model/pipeline costs
//!   (API verbosity, config boilerplate, import surface).
//! - **determinism** — seeded-run reproducibility, version stability, and
//!   whether artifacts (checkpoints, graphs) are byte-stable.
//! - **reliability** — when the agent misuses the API, does it get an early
//!   structured error (shape checks at graph build) or a runtime tensor
//!   explosion three layers deep?
//! - **safety** — does loading/running third-party artifacts execute arbitrary
//!   code (pickle!), and is the compute surface effect-gated?
//!
//! Plus one framework-specific axis the others don't need:
//!
//! - **discoverability** — can an agent learn the surface *from the framework
//!   itself* (machine-readable schemas/ontology, introspectable ops, stable
//!   programmatic docs) instead of scraping prose?
//!
//! Profiles are curated 0.0–1.0 static judgments with `evidence`, like the
//! language profiles — deterministic, serializable, comparable.
//!
//! ```
//! use agentic_eval::frameworks::{profile, rank_frameworks, Framework};
//! let torch = profile(Framework::PyTorch);
//! assert!(torch.evidence.len() >= 3);
//! let ranked = rank_frameworks();
//! assert!(ranked[0].fitness() >= ranked[ranked.len() - 1].fitness());
//! ```

/// AI frameworks with curated agentic profiles.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Framework {
    PyTorch,
    TensorFlow,
    Jax,
    HuggingFaceTransformers,
    OnnxRuntime,
    ScikitLearn,
    Candle,
    Burn,
    /// RecursiveMachineIntelligence (RMI) — the built-in agentic-first framework of
    /// MachineGenetics (MechGen): self-describing ontology + manifest, binary-first
    /// effect-typed compute. Scored on the same axes as everything else.
    FramewerxRmi,
}

impl Framework {
    /// All profiled frameworks, in fixed (deterministic) order.
    pub fn all() -> [Framework; 9] {
        [
            Framework::PyTorch,
            Framework::TensorFlow,
            Framework::Jax,
            Framework::HuggingFaceTransformers,
            Framework::OnnxRuntime,
            Framework::ScikitLearn,
            Framework::Candle,
            Framework::Burn,
            Framework::FramewerxRmi,
        ]
    }

    /// Canonical lowercase name.
    pub fn name(self) -> &'static str {
        match self {
            Framework::PyTorch => "pytorch",
            Framework::TensorFlow => "tensorflow",
            Framework::Jax => "jax",
            Framework::HuggingFaceTransformers => "transformers",
            Framework::OnnxRuntime => "onnxruntime",
            Framework::ScikitLearn => "sklearn",
            Framework::Candle => "candle",
            Framework::Burn => "burn",
            Framework::FramewerxRmi => "rmi",
        }
    }

    /// Parse a (case-insensitive) name; accepts common aliases
    /// (`torch`, `tf`, `hf`, `scikit-learn`, `rmi`, `ort`).
    pub fn from_name(name: &str) -> Option<Framework> {
        match name.to_ascii_lowercase().as_str() {
            "pytorch" | "torch" => Some(Framework::PyTorch),
            "tensorflow" | "tf" => Some(Framework::TensorFlow),
            "jax" => Some(Framework::Jax),
            "transformers" | "hf" | "huggingface" => Some(Framework::HuggingFaceTransformers),
            "onnxruntime" | "onnx" | "ort" => Some(Framework::OnnxRuntime),
            "sklearn" | "scikit-learn" | "scikit" => Some(Framework::ScikitLearn),
            "candle" => Some(Framework::Candle),
            "burn" => Some(Framework::Burn),
            "rmi" | "recursivemachineintelligence" | "framewerx" => Some(Framework::FramewerxRmi),
            _ => None,
        }
    }
}

/// A curated agentic profile of an AI framework: the four shared axes plus
/// framework-specific **discoverability**, with evidence.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct FrameworkProfile {
    /// Which framework this profiles.
    pub framework: Framework,
    /// Token cost of a working model/pipeline (1.0 = very compact).
    pub token_efficiency: f64,
    /// Seeded reproducibility + artifact/version stability.
    pub determinism: f64,
    /// Early, structured failure on API misuse (vs late tensor explosions).
    pub reliability: f64,
    /// Artifact-loading and execution blast radius (pickle ≈ arbitrary code).
    pub safety: f64,
    /// Can an agent learn the surface from the framework itself
    /// (schemas, ontology, introspection) instead of prose docs?
    pub discoverability: f64,
    /// Why: one evidence string per notable factor.
    pub evidence: Vec<&'static str>,
}

impl FrameworkProfile {
    /// Composite agentic fitness: unweighted mean of all five axes.
    pub fn fitness(&self) -> f64 {
        (self.token_efficiency
            + self.determinism
            + self.reliability
            + self.safety
            + self.discoverability)
            / 5.0
    }
}

impl std::fmt::Display for FrameworkProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: fitness {:.2} (tokens {:.2}, determinism {:.2}, reliability {:.2}, safety {:.2}, discoverability {:.2})",
            self.framework.name(),
            self.fitness(),
            self.token_efficiency,
            self.determinism,
            self.reliability,
            self.safety,
            self.discoverability
        )
    }
}

/// The curated profile for `fw` (static, documented judgments — see module docs).
pub fn profile(fw: Framework) -> FrameworkProfile {
    match fw {
        Framework::PyTorch => FrameworkProfile {
            framework: fw,
            token_efficiency: 0.7,
            determinism: 0.5,
            reliability: 0.5,
            safety: 0.3,
            discoverability: 0.5,
            evidence: vec![
                "dominant in LLM training data: agents emit competent PyTorch with few tokens of guidance",
                "eager execution defers shape errors to runtime, mid-forward — late self-correction signal",
                "determinism requires opt-in flags (use_deterministic_algorithms) and still has CUDA caveats",
                "torch.load = pickle = arbitrary code execution on artifact load (weights_only mitigates, not default historically)",
                "good runtime introspection (modules walkable) but no machine-readable surface schema",
            ],
        },
        Framework::TensorFlow => FrameworkProfile {
            framework: fw,
            token_efficiency: 0.5,
            determinism: 0.55,
            reliability: 0.55,
            safety: 0.45,
            discoverability: 0.5,
            evidence: vec![
                "graph mode catches shape errors at build, but the Keras/TF1/TF2 API strata cost agent tokens and confusion",
                "SavedModel is a real schema'd artifact (better than pickle)",
                "version churn between majors broke much trained-data knowledge",
                "op-level determinism is opt-in and incomplete on GPU",
            ],
        },
        Framework::Jax => FrameworkProfile {
            framework: fw,
            token_efficiency: 0.65,
            determinism: 0.85,
            reliability: 0.6,
            safety: 0.55,
            discoverability: 0.45,
            evidence: vec![
                "functional purity + explicit PRNG keys: the most reproducible mainstream choice",
                "jit tracing errors (abstract tracer leaks) are notoriously confusing for agents",
                "compact numpy-like surface, but the ecosystem (flax/optax/orbax) adds standing context",
                "no pickle-by-default artifacts; checkpoint formats are schema'd",
            ],
        },
        Framework::HuggingFaceTransformers => FrameworkProfile {
            framework: fw,
            token_efficiency: 0.85,
            determinism: 0.45,
            reliability: 0.5,
            safety: 0.4,
            discoverability: 0.7,
            evidence: vec![
                "pipeline()/AutoModel: a working LLM in ~3 lines — best token economy of the set",
                "Hub model cards + config.json are machine-readable (good discoverability)",
                "trust_remote_code executes arbitrary hub code; safetensors fixed weights but custom code remains the hole",
                "version pinning matters: behavior drifts across releases; remote artifacts mutate",
            ],
        },
        Framework::OnnxRuntime => FrameworkProfile {
            framework: fw,
            token_efficiency: 0.6,
            determinism: 0.8,
            reliability: 0.7,
            safety: 0.75,
            discoverability: 0.75,
            evidence: vec![
                "ONNX graphs are fully schema'd protobuf: an agent can introspect every op/shape without running",
                "inference-only scope: small, stable API; graph validation catches malformed models at load",
                "no code execution in artifacts (data-only format) — the safest artifact story here",
                "training support is marginal; agents needing training must go elsewhere",
            ],
        },
        Framework::ScikitLearn => FrameworkProfile {
            framework: fw,
            token_efficiency: 0.8,
            determinism: 0.75,
            reliability: 0.7,
            safety: 0.35,
            discoverability: 0.6,
            evidence: vec![
                "fit/predict uniformity: one API shape across ~all estimators (cheap for agents to generalize)",
                "random_state threading gives easy reproducibility",
                "get_params()/set_params() is machine-walkable; estimator tags exist but are semi-private",
                "joblib/pickle persistence = arbitrary code execution on load",
            ],
        },
        Framework::Candle => FrameworkProfile {
            framework: fw,
            token_efficiency: 0.55,
            determinism: 0.8,
            reliability: 0.75,
            safety: 0.7,
            discoverability: 0.4,
            evidence: vec![
                "Rust: compile-time dimension/type errors catch agent mistakes pre-run; cargo reproducibility",
                "safetensors-native (data-only artifacts)",
                "far less training-data representation: agents need more tokens of guidance than for PyTorch",
                "smaller op surface; no machine-readable self-description",
            ],
        },
        Framework::Burn => FrameworkProfile {
            framework: fw,
            token_efficiency: 0.5,
            determinism: 0.8,
            reliability: 0.8,
            safety: 0.7,
            discoverability: 0.45,
            evidence: vec![
                "type-state tensors (rank/dtype in the type) catch shape misuse at compile time — strongest static reliability of the set",
                "backend-generic (wgpu/candle/ndarray) with cargo-locked reproducibility",
                "youngest ecosystem; thin training-data presence costs agent tokens",
                "derive-macro module system is introspectable in-code but lacks a runtime ontology",
            ],
        },
        Framework::FramewerxRmi => FrameworkProfile {
            framework: fw,
            token_efficiency: 0.75,
            determinism: 0.85,
            reliability: 0.8,
            safety: 0.75,
            discoverability: 0.95,
            evidence: vec![
                "designed agentic-first: FrameworkOntology + token-compact manifest()/describe() — the surface is discoverable without prose docs (the property this axis measures, built in)",
                "deterministic ontology/manifest (fixed order), binary-first RMIL artifacts, deterministic formatter: byte-stable for agent caching/diffing",
                "typed Result errors on every Backend op; quantized/half paths fall back to exact F32 rather than failing or silently degrading",
                "effect-typed compute + feature-gated CUDA with driver-checked construction; no pickle-style artifact execution",
                "young framework: minimal training-data presence means agents rely on its self-description (which is the design bet)",
            ],
        },
    }
}

/// Profiles for all frameworks, in [`Framework::all`] order (deterministic).
pub fn profiles() -> Vec<FrameworkProfile> {
    Framework::all().iter().map(|&f| profile(f)).collect()
}

/// All profiles ranked best-first by [`FrameworkProfile::fitness`]
/// (stable order on ties).
pub fn rank_frameworks() -> Vec<FrameworkProfile> {
    let mut v = profiles();
    v.sort_by(|a, b| {
        b.fitness()
            .partial_cmp(&a.fitness())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    v
}

/// Compare two frameworks: positive deltas mean `a` fits agentic use better.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct FrameworkComparison {
    /// First framework (the subject).
    pub a: FrameworkProfile,
    /// Second framework (the baseline).
    pub b: FrameworkProfile,
    /// `a.fitness() - b.fitness()`.
    pub fitness_delta: f64,
    /// Axis name → delta (a − b), in fixed axis order.
    pub axis_deltas: Vec<(&'static str, f64)>,
}

/// Compare framework `a` against baseline `b` across all five axes.
pub fn compare_frameworks(a: Framework, b: Framework) -> FrameworkComparison {
    let pa = profile(a);
    let pb = profile(b);
    let axis_deltas = vec![
        ("tokens", pa.token_efficiency - pb.token_efficiency),
        ("determinism", pa.determinism - pb.determinism),
        ("reliability", pa.reliability - pb.reliability),
        ("safety", pa.safety - pb.safety),
        ("discoverability", pa.discoverability - pb.discoverability),
    ];
    FrameworkComparison {
        fitness_delta: pa.fitness() - pb.fitness(),
        a: pa,
        b: pb,
        axis_deltas,
    }
}

impl std::fmt::Display for FrameworkComparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} vs {}: fitness delta {:+.2}",
            self.a.framework.name(),
            self.b.framework.name(),
            self.fitness_delta
        )?;
        for (axis, d) in &self.axis_deltas {
            writeln!(f, "  {axis}: {d:+.2}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_framework_profiles_with_evidence() {
        for fw in Framework::all() {
            let p = profile(fw);
            assert!(
                p.evidence.len() >= 3,
                "{} needs ≥3 evidence lines",
                fw.name()
            );
            for s in [
                p.token_efficiency,
                p.determinism,
                p.reliability,
                p.safety,
                p.discoverability,
            ] {
                assert!((0.0..=1.0).contains(&s), "{} score out of range", fw.name());
            }
        }
    }

    #[test]
    fn from_name_roundtrip_and_aliases() {
        for fw in Framework::all() {
            assert_eq!(Framework::from_name(fw.name()), Some(fw));
        }
        assert_eq!(Framework::from_name("torch"), Some(Framework::PyTorch));
        assert_eq!(
            Framework::from_name("HF"),
            Some(Framework::HuggingFaceTransformers)
        );
        assert_eq!(Framework::from_name("rmi"), Some(Framework::FramewerxRmi));
        assert_eq!(Framework::from_name("caffe"), None);
    }

    #[test]
    fn ranking_is_deterministic_and_sorted() {
        let r1 = rank_frameworks();
        let r2 = rank_frameworks();
        let n1: Vec<_> = r1.iter().map(|p| p.framework.name()).collect();
        let n2: Vec<_> = r2.iter().map(|p| p.framework.name()).collect();
        assert_eq!(n1, n2);
        for w in r1.windows(2) {
            assert!(w[0].fitness() >= w[1].fitness());
        }
    }

    #[test]
    fn axis_judgments_hold_directionally() {
        let torch = profile(Framework::PyTorch);
        let jax = profile(Framework::Jax);
        let ort = profile(Framework::OnnxRuntime);
        let hf = profile(Framework::HuggingFaceTransformers);
        let burn = profile(Framework::Burn);
        assert!(
            jax.determinism > torch.determinism,
            "explicit PRNG keys beat opt-in flags"
        );
        assert!(ort.safety > torch.safety, "data-only artifacts beat pickle");
        assert!(
            hf.token_efficiency > burn.token_efficiency,
            "pipeline() in 3 lines beats young Rust ecosystem"
        );
        assert!(
            burn.reliability > torch.reliability,
            "type-state tensors catch shape misuse pre-run"
        );
    }

    #[test]
    fn comparison_deltas_are_consistent() {
        let cmp = compare_frameworks(Framework::FramewerxRmi, Framework::PyTorch);
        let sum: f64 = cmp.axis_deltas.iter().map(|(_, d)| d).sum();
        assert!((sum / 5.0 - cmp.fitness_delta).abs() < 1e-9);
        assert!(format!("{cmp}").contains("rmi vs pytorch"));
    }
}
