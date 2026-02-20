//! Candle Backend — Pure Rust inference via HuggingFace Candle
//!
//! Supports GGUF and SafeTensors model formats for text generation and embeddings.
//! Uses candle-transformers for model architectures (LLaMA, Mistral, Phi, etc.)
//! and the HuggingFace tokenizers crate for BPE/SentencePiece tokenization.

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use candle_core::{DType, Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::{
    quantized_llama, quantized_mistral, quantized_phi, quantized_phi3,
};

use super::{EmbeddingResult, GenerationParams, InferenceResult, LocalInferenceBackend};

// ============================================================================
// MODEL ARCHITECTURE DISPATCH
// ============================================================================

/// Supported quantized model architectures loaded from GGUF files
enum QuantizedModel {
    /// Llama family (Llama 2, Llama 3, Code Llama, Mixtral via MoE)
    Llama(quantized_llama::ModelWeights),
    /// Mistral 7B
    Mistral(quantized_mistral::Model),
    /// Phi-2
    Phi2(quantized_phi::ModelWeights),
    /// Phi-3
    Phi3(quantized_phi3::ModelWeights),
}

impl QuantizedModel {
    /// Run a forward pass: token IDs → logits
    fn forward(&mut self, tokens: &Tensor, index_pos: usize) -> Result<Tensor> {
        match self {
            Self::Llama(m) => m.forward(tokens, index_pos),
            Self::Mistral(m) => m.forward(tokens, index_pos),
            Self::Phi2(m) => m.forward(tokens, index_pos),
            Self::Phi3(m) => m.forward(tokens, index_pos),
        }
    }
}

/// A loaded model handle
struct LoadedModel {
    /// Path to the model file
    path: String,
    /// Quantized transformer model (for GGUF)
    quantized: Option<QuantizedModel>,
    /// Embedding weights (for SafeTensors — embedding-only mode)
    embed_weights: Option<Tensor>,
    /// Tokenizer for this model
    tokenizer: tokenizers::Tokenizer,
    /// Device the model is loaded on
    device: Device,
    /// Model config metadata
    config: ModelConfig,
}

/// Minimal model configuration
#[derive(Debug, Clone)]
struct ModelConfig {
    vocab_size: usize,
    hidden_size: usize,
    num_layers: usize,
    num_heads: usize,
    max_seq_len: usize,
    model_type: ModelType,
}

/// Model architecture type — determines which quantized model to instantiate
#[derive(Debug, Clone, PartialEq)]
enum ModelType {
    Llama,
    Mistral,
    Phi2,
    Phi3,
    Unknown(String),
}

impl std::fmt::Display for ModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Llama => write!(f, "llama"),
            Self::Mistral => write!(f, "mistral"),
            Self::Phi2 => write!(f, "phi2"),
            Self::Phi3 => write!(f, "phi3"),
            Self::Unknown(s) => write!(f, "{}", s),
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            vocab_size: 32000,
            hidden_size: 4096,
            num_layers: 32,
            num_heads: 32,
            max_seq_len: 4096,
            model_type: ModelType::Llama,
        }
    }
}

/// Candle-based local inference backend
pub struct CandleBackend {
    models: Arc<Mutex<HashMap<String, LoadedModel>>>,
    device: Device,
}

impl CandleBackend {
    pub fn new() -> Self {
        // Prefer CUDA if available, fall back to CPU
        let device = Device::cuda_if_available(0).unwrap_or(Device::Cpu);
        Self {
            models: Arc::new(Mutex::new(HashMap::new())),
            device,
        }
    }

    /// Detect model architecture from GGUF metadata
    fn detect_model_type_from_gguf(
        content: &candle_core::quantized::gguf_file::Content,
    ) -> ModelType {
        // Check the "general.architecture" GGUF metadata key
        if let Some(arch) = content.metadata.get("general.architecture") {
            let arch_str = format!("{:?}", arch).to_lowercase();
            if arch_str.contains("llama") {
                return ModelType::Llama;
            } else if arch_str.contains("mistral") {
                return ModelType::Mistral;
            } else if arch_str.contains("phi3") {
                return ModelType::Phi3;
            } else if arch_str.contains("phi") {
                return ModelType::Phi2;
            }
        }

        // Fall back to filename-based heuristics
        ModelType::Llama
    }

    /// Detect model type from filename heuristics (for non-GGUF formats)
    fn detect_model_type_from_filename(filename: &str) -> ModelType {
        let lower = filename.to_lowercase();
        if lower.contains("mistral") {
            ModelType::Mistral
        } else if lower.contains("phi-3") || lower.contains("phi3") {
            ModelType::Phi3
        } else if lower.contains("phi-2") || lower.contains("phi2") || lower.contains("phi") {
            ModelType::Phi2
        } else if lower.contains("qwen") {
            // Qwen uses a llama-like architecture in GGUF
            ModelType::Llama
        } else if lower.contains("gemma") {
            ModelType::Llama
        } else {
            ModelType::Llama
        }
    }

    /// Detect model config from a GGUF file's metadata
    fn detect_config_from_gguf(
        content: &candle_core::quantized::gguf_file::Content,
        filename: &str,
    ) -> ModelConfig {
        let model_type = Self::detect_model_type_from_gguf(content);

        let mut config = ModelConfig {
            model_type: model_type.clone(),
            ..Default::default()
        };

        // Try to read numeric metadata from GGUF keys
        let arch_prefix = match &model_type {
            ModelType::Llama => "llama",
            ModelType::Mistral => "mistral",
            ModelType::Phi2 => "phi2",
            ModelType::Phi3 => "phi3",
            ModelType::Unknown(s) => s.as_str(),
        };

        // Helper closure to extract u32 metadata
        let get_u32 = |key: &str| -> Option<usize> {
            content.metadata.get(key).and_then(|v| {
                let s = format!("{:?}", v);
                s.trim_start_matches("U32(")
                    .trim_end_matches(')')
                    .parse::<usize>()
                    .ok()
            })
        };

        if let Some(v) = get_u32(&format!("{}.embedding_length", arch_prefix)) {
            config.hidden_size = v;
        }
        if let Some(v) = get_u32(&format!("{}.block_count", arch_prefix)) {
            config.num_layers = v;
        }
        if let Some(v) = get_u32(&format!("{}.attention.head_count", arch_prefix)) {
            config.num_heads = v;
        }
        if let Some(v) = get_u32(&format!("{}.context_length", arch_prefix)) {
            config.max_seq_len = v;
        }

        // Fallback: filename-based size heuristics
        let lower = filename.to_lowercase();
        if config.hidden_size == 4096 {
            // Only override if we got the default
            if lower.contains("13b") || lower.contains("14b") {
                config.hidden_size = 5120;
                config.num_layers = 40;
                config.num_heads = 40;
            } else if lower.contains("70b") {
                config.hidden_size = 8192;
                config.num_layers = 80;
                config.num_heads = 64;
            } else if lower.contains("1b") || lower.contains("1.5b") {
                config.hidden_size = 2048;
                config.num_layers = 22;
                config.num_heads = 32;
            } else if lower.contains("3b") {
                config.hidden_size = 3200;
                config.num_layers = 26;
                config.num_heads = 32;
            }
        }

        if lower.contains("128k") {
            config.max_seq_len = 131072;
        } else if lower.contains("32k") {
            config.max_seq_len = 32768;
        } else if lower.contains("8k") {
            config.max_seq_len = 8192;
        }

        config
    }

    /// Load a tokenizer — tries to find tokenizer.json next to the model, falls back to default
    fn load_tokenizer(model_path: &str) -> Result<tokenizers::Tokenizer> {
        let path = std::path::Path::new(model_path);

        // Try tokenizer.json in the same directory
        if let Some(dir) = path.parent() {
            let tokenizer_path = dir.join("tokenizer.json");
            if tokenizer_path.exists() {
                return tokenizers::Tokenizer::from_file(&tokenizer_path)
                    .map_err(|e| anyhow!("Failed to load tokenizer: {}", e));
            }
        }

        // Try parent directory (common in HuggingFace layouts)
        if let Some(dir) = path.parent().and_then(|d| d.parent()) {
            let tokenizer_path = dir.join("tokenizer.json");
            if tokenizer_path.exists() {
                return tokenizers::Tokenizer::from_file(&tokenizer_path)
                    .map_err(|e| anyhow!("Failed to load tokenizer: {}", e));
            }
        }

        Err(anyhow!(
            "No tokenizer.json found near {}. Place a tokenizer.json in the same directory.",
            model_path
        ))
    }

    /// Build a MistralConfig from GGUF metadata
    fn mistral_config_from_gguf(
        content: &candle_core::quantized::gguf_file::Content,
    ) -> candle_transformers::models::mistral::Config {
        use candle_nn::Activation;

        // Helper to extract u32 value from GGUF metadata
        let get_u32 = |key: &str, default: usize| -> usize {
            content
                .metadata
                .get(key)
                .and_then(|v| {
                    let s = format!("{:?}", v);
                    s.trim_start_matches("U32(")
                        .trim_end_matches(')')
                        .parse::<usize>()
                        .ok()
                })
                .unwrap_or(default)
        };

        let get_f64 = |key: &str, default: f64| -> f64 {
            content
                .metadata
                .get(key)
                .and_then(|v| {
                    let s = format!("{:?}", v);
                    s.trim_start_matches("F32(")
                        .trim_end_matches(')')
                        .parse::<f64>()
                        .ok()
                })
                .unwrap_or(default)
        };

        let prefix = "mistral";
        let hidden_size = get_u32(&format!("{prefix}.embedding_length"), 4096);
        let num_heads = get_u32(&format!("{prefix}.attention.head_count"), 32);
        let num_kv_heads = get_u32(&format!("{prefix}.attention.head_count_kv"), 8);

        candle_transformers::models::mistral::Config {
            vocab_size: get_u32(&format!("{prefix}.vocab_size"), 32000),
            hidden_size,
            intermediate_size: get_u32(&format!("{prefix}.feed_forward_length"), 14336),
            num_hidden_layers: get_u32(&format!("{prefix}.block_count"), 32),
            num_attention_heads: num_heads,
            head_dim: Some(hidden_size / num_heads),
            num_key_value_heads: num_kv_heads,
            hidden_act: Activation::Silu,
            max_position_embeddings: get_u32(&format!("{prefix}.context_length"), 32768),
            rms_norm_eps: get_f64(&format!("{prefix}.attention.layer_norm_rms_epsilon"), 1e-5),
            rope_theta: get_f64(&format!("{prefix}.rope.freq_base"), 10000.0),
            sliding_window: Some(get_u32(&format!("{prefix}.attention.sliding_window"), 4096)),
            use_flash_attn: false,
        }
    }

    /// Create a LogitsProcessor from GenerationParams
    fn make_logits_processor(params: &GenerationParams) -> LogitsProcessor {
        let seed = params.seed.unwrap_or(42);

        if params.temperature <= 0.0 {
            // Deterministic / greedy decoding
            LogitsProcessor::from_sampling(seed, Sampling::ArgMax)
        } else if params.top_p < 1.0 {
            LogitsProcessor::from_sampling(
                seed,
                Sampling::TopP {
                    p: params.top_p,
                    temperature: params.temperature,
                },
            )
        } else {
            LogitsProcessor::from_sampling(
                seed,
                Sampling::All {
                    temperature: params.temperature,
                },
            )
        }
    }

    /// Apply repetition penalty to logits in-place
    fn apply_repetition_penalty(logits: &mut [f32], generated: &[u32], penalty: f64) {
        if penalty == 1.0 {
            return;
        }
        let penalty = penalty as f32;
        for &token_id in generated {
            if let Some(logit) = logits.get_mut(token_id as usize) {
                if *logit > 0.0 {
                    *logit /= penalty;
                } else {
                    *logit *= penalty;
                }
            }
        }
    }

    /// Autoregressive text generation using a loaded quantized model
    fn generate_with_quantized(
        model: &mut QuantizedModel,
        tokenizer: &tokenizers::Tokenizer,
        device: &Device,
        prompt: &str,
        params: &GenerationParams,
    ) -> Result<InferenceResult> {
        let start = std::time::Instant::now();

        // Tokenize input
        let encoding = tokenizer
            .encode(prompt, true)
            .map_err(|e| anyhow!("Tokenization failed: {}", e))?;
        let prompt_tokens = encoding.get_ids().to_vec();
        let prompt_len = prompt_tokens.len() as u32;

        if prompt_tokens.is_empty() {
            return Err(anyhow!("Empty prompt after tokenization"));
        }

        // Create logits processor for sampling
        let mut logits_processor = Self::make_logits_processor(params);

        // Determine EOS token(s)
        let eos_tokens: Vec<u32> = [
            "</s>",
            "<|endoftext|>",
            "<|end|>",
            "<|eot_id|>",
            "<|im_end|>",
        ]
        .iter()
        .filter_map(|s| tokenizer.token_to_id(s))
        .collect();
        let eos_default = 2u32; // common EOS fallback

        // Prefill: process the entire prompt in one forward pass
        let input = Tensor::new(prompt_tokens.as_slice(), device)?.unsqueeze(0)?;
        let logits = model.forward(&input, 0)?;

        // Extract last-token logits for first generated token
        let logits = logits.squeeze(0)?;
        let logits = if logits.dims().len() == 2 {
            // (seq_len, vocab_size) → take last position
            let seq_len = logits.dim(0)?;
            logits.narrow(0, seq_len - 1, 1)?.squeeze(0)?
        } else {
            logits
        };

        // Sample first token with repetition penalty
        let rep_penalty = params.repetition_penalty;
        let mut generated_tokens: Vec<u32> = Vec::new();

        let first_token = if rep_penalty != 1.0 {
            logits_processor.sample_f(&logits, |logits_slice| {
                Self::apply_repetition_penalty(logits_slice, &prompt_tokens, rep_penalty);
            })?
        } else {
            logits_processor.sample(&logits)?
        };

        // Check for immediate EOS
        if eos_tokens.contains(&first_token) || first_token == eos_default {
            let elapsed = start.elapsed();
            return Ok(InferenceResult {
                text: String::new(),
                prompt_tokens: prompt_len,
                completion_tokens: 0,
                generation_ms: elapsed.as_secs_f64() * 1000.0,
                tokens_per_second: 0.0,
            });
        }
        generated_tokens.push(first_token);

        // Autoregressive decode loop
        let max_tokens = params.max_tokens.min(4096);
        for i in 1..max_tokens {
            let pos = prompt_tokens.len() + i as usize;
            let input = Tensor::new(&[*generated_tokens.last().unwrap()], device)?.unsqueeze(0)?;
            let logits = model.forward(&input, pos)?;

            // Extract logits (handle both 1D and 2D shapes)
            let logits = logits.squeeze(0)?;
            let logits = if logits.dims().len() == 2 {
                logits.narrow(0, logits.dim(0)? - 1, 1)?.squeeze(0)?
            } else {
                logits
            };

            // Sample with repetition penalty
            let all_context: Vec<u32> = prompt_tokens
                .iter()
                .chain(generated_tokens.iter())
                .copied()
                .collect();

            let next_token = if rep_penalty != 1.0 {
                logits_processor.sample_f(&logits, |logits_slice| {
                    Self::apply_repetition_penalty(logits_slice, &all_context, rep_penalty);
                })?
            } else {
                logits_processor.sample(&logits)?
            };

            // Check EOS
            if eos_tokens.contains(&next_token) || next_token == eos_default {
                break;
            }

            generated_tokens.push(next_token);
        }

        let elapsed = start.elapsed();
        let generation_ms = elapsed.as_secs_f64() * 1000.0;
        let completion_tokens = generated_tokens.len() as u32;

        // Decode generated tokens
        let text = tokenizer
            .decode(&generated_tokens, true)
            .unwrap_or_default();

        let tokens_per_second = if generation_ms > 0.0 {
            completion_tokens as f64 / (generation_ms / 1000.0)
        } else {
            0.0
        };

        Ok(InferenceResult {
            text,
            prompt_tokens: prompt_len,
            completion_tokens,
            generation_ms,
            tokens_per_second,
        })
    }
}

impl LocalInferenceBackend for CandleBackend {
    fn name(&self) -> &str {
        "candle"
    }

    fn load_model(&self, path: &str) -> Result<String> {
        let handle = format!(
            "candle:{}",
            std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("model")
        );

        let tokenizer = Self::load_tokenizer(path)?;

        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let filename = std::path::Path::new(path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("");

        let (quantized, embed_weights, config) = match ext {
            "gguf" => {
                // Load GGUF via candle's quantized format support
                let mut file =
                    std::fs::File::open(path).context("Failed to open GGUF model file")?;
                let content = candle_core::quantized::gguf_file::Content::read(
                    &mut std::io::BufReader::new(&mut file),
                )?;

                let config = Self::detect_config_from_gguf(&content, filename);

                // Re-open file (Content::read consumed the reader position)
                let mut file2 = std::fs::File::open(path).context("Failed to re-open GGUF file")?;
                let content2 = candle_core::quantized::gguf_file::Content::read(
                    &mut std::io::BufReader::new(&mut file2),
                )?;

                let quantized_model = match &config.model_type {
                    ModelType::Llama | ModelType::Unknown(_) => {
                        let weights = quantized_llama::ModelWeights::from_gguf(
                            content2,
                            &mut std::io::BufReader::new(&mut file2),
                            &self.device,
                        )?;
                        QuantizedModel::Llama(weights)
                    }
                    ModelType::Mistral => {
                        let mistral_config = Self::mistral_config_from_gguf(&content2);
                        let vb = candle_transformers::quantized_var_builder::VarBuilder::from_gguf(
                            path,
                            &self.device,
                        )?;
                        let model = quantized_mistral::Model::new(&mistral_config, vb)?;
                        QuantizedModel::Mistral(model)
                    }
                    ModelType::Phi2 => {
                        let weights = quantized_phi::ModelWeights::from_gguf(
                            content2,
                            &mut std::io::BufReader::new(&mut file2),
                            &self.device,
                        )?;
                        QuantizedModel::Phi2(weights)
                    }
                    ModelType::Phi3 => {
                        let weights = quantized_phi3::ModelWeights::from_gguf(
                            false, // use_flash_attn
                            content2,
                            &mut std::io::BufReader::new(&mut file2),
                            &self.device,
                        )?;
                        QuantizedModel::Phi3(weights)
                    }
                };

                (Some(quantized_model), None, config)
            }
            "safetensors" => {
                // SafeTensors: load embedding weights for embedding-only mode
                // Full transformer inference from SafeTensors requires model-specific
                // weight key mapping — GGUF is preferred for generation
                let tensors = candle_core::safetensors::load(path, &self.device)?;

                let embed = tensors
                    .get("model.embed_tokens.weight")
                    .or_else(|| tensors.get("transformer.wte.weight"))
                    .cloned();

                let model_type = Self::detect_model_type_from_filename(filename);
                let config = ModelConfig {
                    model_type,
                    ..Default::default()
                };

                (None, embed, config)
            }
            _ => {
                return Err(anyhow!(
                    "Candle backend does not support .{} format. Use .gguf (recommended) or .safetensors",
                    ext
                ));
            }
        };

        let loaded = LoadedModel {
            path: path.to_string(),
            quantized,
            embed_weights,
            tokenizer,
            device: self.device.clone(),
            config,
        };

        let mut models = self.models.lock().unwrap();
        models.insert(handle.clone(), loaded);

        Ok(handle)
    }

    fn unload_model(&self, handle: &str) -> Result<()> {
        let mut models = self.models.lock().unwrap();
        models
            .remove(handle)
            .ok_or_else(|| anyhow!("Model '{}' not loaded", handle))?;
        Ok(())
    }

    fn loaded_models(&self) -> Vec<String> {
        let models = self.models.lock().unwrap();
        models.keys().cloned().collect()
    }

    fn generate(
        &self,
        handle: &str,
        prompt: &str,
        params: &GenerationParams,
    ) -> Result<InferenceResult> {
        let mut models = self.models.lock().unwrap();
        let model = models
            .get_mut(handle)
            .ok_or_else(|| anyhow!("Model '{}' not loaded", handle))?;

        if let Some(ref mut quantized) = model.quantized {
            // Full transformer generation via candle-transformers
            Self::generate_with_quantized(
                quantized,
                &model.tokenizer,
                &model.device,
                prompt,
                params,
            )
        } else {
            // SafeTensors embedding-only mode — no generation capability
            Err(anyhow!(
                "Model loaded from SafeTensors supports embeddings only. \
                 Use a .gguf model for text generation."
            ))
        }
    }

    fn embed(&self, handle: &str, inputs: &[String]) -> Result<EmbeddingResult> {
        let models = self.models.lock().unwrap();
        let model = models
            .get(handle)
            .ok_or_else(|| anyhow!("Model '{}' not loaded", handle))?;

        let mut all_embeddings = Vec::new();
        let mut total_tokens = 0u32;

        // Use embedding weights if available (SafeTensors or GGUF with extracted embeddings)
        let embed_weight = model.embed_weights.as_ref().ok_or_else(|| {
            anyhow!(
                "No embedding weights available for model '{}'. \
                 Embeddings require model.embed_tokens.weight in the model file.",
                handle
            )
        })?;

        for input in inputs {
            let encoding = model
                .tokenizer
                .encode(input.as_str(), true)
                .map_err(|e| anyhow!("Tokenization failed: {}", e))?;
            let token_ids = encoding.get_ids().to_vec();
            total_tokens += token_ids.len() as u32;

            let input_tensor = Tensor::new(token_ids.as_slice(), &model.device)?;
            let embeddings = embed_weight.index_select(&input_tensor, 0)?;
            // Mean pool across sequence dimension
            let mean = embeddings.mean(0)?;
            let embedding: Vec<f32> = mean.to_vec1()?;
            all_embeddings.push(embedding);
        }

        Ok(EmbeddingResult {
            embeddings: all_embeddings,
            total_tokens,
        })
    }

    fn supports_format(&self, extension: &str) -> bool {
        matches!(extension, "safetensors" | "gguf")
    }

    fn estimate_memory_mb(&self, path: &str) -> Result<u64> {
        let metadata = std::fs::metadata(path)?;
        let file_bytes = metadata.len();
        // GGUF quantized models: ~1.2x file size in memory
        // SafeTensors: ~1.0x file size
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let multiplier = if ext == "gguf" { 1.2 } else { 1.0 };
        Ok((file_bytes as f64 * multiplier / (1024.0 * 1024.0)) as u64)
    }
}
