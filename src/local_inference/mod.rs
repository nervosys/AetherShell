//! Local Inference Backends
//!
//! Provides native Rust inference via Candle (HuggingFace) and ONNX Runtime,
//! replacing external llama.cpp server delegation with in-process inference.
//!
//! Both backends are behind optional feature flags:
//! - `candle` — Pure Rust GPU/CPU inference via candle-core/candle-transformers
//! - `onnx` — ONNX Runtime inference via the `ort` crate
//!
//! When neither feature is enabled, inference falls back to llama.cpp HTTP delegation.

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[cfg(feature = "candle")]
pub mod candle_backend;

#[cfg(feature = "onnx")]
pub mod ort_backend;

// ============================================================================
// BACKEND TRAIT
// ============================================================================

/// Result of a text generation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResult {
    /// Generated text
    pub text: String,
    /// Number of prompt tokens consumed
    pub prompt_tokens: u32,
    /// Number of completion tokens generated
    pub completion_tokens: u32,
    /// Generation time in milliseconds
    pub generation_ms: f64,
    /// Tokens per second
    pub tokens_per_second: f64,
}

/// Result of an embedding request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResult {
    /// Embedding vectors (one per input)
    pub embeddings: Vec<Vec<f32>>,
    /// Total tokens processed
    pub total_tokens: u32,
}

/// Generation parameters
#[derive(Debug, Clone)]
pub struct GenerationParams {
    /// Maximum tokens to generate
    pub max_tokens: u32,
    /// Temperature (0.0 = deterministic, 1.0 = creative)
    pub temperature: f64,
    /// Top-p nucleus sampling
    pub top_p: f64,
    /// Repetition penalty (1.0 = none)
    pub repetition_penalty: f64,
    /// Random seed (None = random)
    pub seed: Option<u64>,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            max_tokens: 2048,
            temperature: 0.7,
            top_p: 0.9,
            repetition_penalty: 1.1,
            seed: None,
        }
    }
}

/// Trait for local inference backends (Candle, ONNX, etc.)
pub trait LocalInferenceBackend: Send + Sync {
    /// Human-readable backend name
    fn name(&self) -> &str;

    /// Load a model from a file path. Returns an opaque handle ID.
    fn load_model(&self, path: &str) -> Result<String>;

    /// Unload a previously loaded model
    fn unload_model(&self, handle: &str) -> Result<()>;

    /// List currently loaded model handles
    fn loaded_models(&self) -> Vec<String>;

    /// Generate text from a prompt
    fn generate(
        &self,
        handle: &str,
        prompt: &str,
        params: &GenerationParams,
    ) -> Result<InferenceResult>;

    /// Generate embeddings for input texts
    fn embed(&self, handle: &str, inputs: &[String]) -> Result<EmbeddingResult>;

    /// Check if a model format is supported by this backend
    fn supports_format(&self, extension: &str) -> bool;

    /// Estimate VRAM/RAM usage for a model file (in MB)
    fn estimate_memory_mb(&self, path: &str) -> Result<u64>;
}

// ============================================================================
// BACKEND REGISTRY
// ============================================================================

/// Every inference backend compiled in. Each is one instance for the whole
/// process: a model that `ai_local_load` puts in a backend's table has to be
/// there for the `ai_local_generate`, `ai_local_embed` and `ai_local_unload`
/// calls that name its handle. (They had been built fresh per call, so every
/// loaded model was dropped as the load returned.)
pub fn available_backends() -> Vec<&'static dyn LocalInferenceBackend> {
    #[allow(unused_mut)]
    let mut backends: Vec<&'static dyn LocalInferenceBackend> = Vec::new();

    #[cfg(feature = "candle")]
    {
        static CANDLE: std::sync::OnceLock<candle_backend::CandleBackend> =
            std::sync::OnceLock::new();
        backends.push(CANDLE.get_or_init(candle_backend::CandleBackend::new));
    }

    #[cfg(feature = "onnx")]
    {
        static ORT: std::sync::OnceLock<ort_backend::OrtBackend> = std::sync::OnceLock::new();
        backends.push(ORT.get_or_init(ort_backend::OrtBackend::new));
    }

    backends
}

/// Find the best backend for a given model file
pub fn backend_for_model(path: &str) -> Option<&'static dyn LocalInferenceBackend> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    available_backends()
        .into_iter()
        .find(|backend| backend.supports_format(&ext))
}

/// Check if any local inference backend is available
pub fn has_local_backend() -> bool {
    cfg!(feature = "candle") || cfg!(feature = "onnx")
}

/// List available backend names
pub fn backend_names() -> Vec<&'static str> {
    #[allow(unused_mut)]
    let mut names = Vec::new();
    #[cfg(feature = "candle")]
    names.push("candle");
    #[cfg(feature = "onnx")]
    names.push("onnx");
    names
}

#[cfg(all(test, any(feature = "candle", feature = "onnx")))]
mod tests {
    #[test]
    fn every_call_sees_the_same_backends() {
        // A model loaded through one call must be there for the next.
        let a = super::available_backends();
        let b = super::available_backends();
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert!(std::ptr::addr_eq(*x, *y), "{} was built anew", x.name());
        }
    }
}
