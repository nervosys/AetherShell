//! Candle Backend — Pure Rust inference via HuggingFace Candle
//!
//! Supports GGUF model files for text generation. SafeTensors files and
//! embeddings are refused until they can be done honestly (see `load_model`
//! and `embed`). Uses candle-transformers for model architectures (LLaMA,
//! Mistral, Phi, etc.) and the HuggingFace tokenizers crate for BPE/SentencePiece
//! tokenization.

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use candle_core::{Device, Tensor};
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
        Ok(match self {
            Self::Llama(m) => m.forward(tokens, index_pos),
            Self::Mistral(m) => m.forward(tokens, index_pos),
            Self::Phi2(m) => m.forward(tokens, index_pos),
            Self::Phi3(m) => m.forward(tokens, index_pos),
        }?)
    }
}

/// A loaded model handle
struct LoadedModel {
    /// Quantized transformer model
    quantized: QuantizedModel,
    /// Tokenizer for this model
    tokenizer: tokenizers::Tokenizer,
    /// Device the model is loaded on
    device: Device,
}

/// Model architecture type — determines which quantized model to instantiate
#[derive(Debug, Clone, PartialEq)]
enum ModelType {
    Llama,
    Mistral,
    Phi2,
    Phi3,
}

/// Candle-based local inference backend
pub struct CandleBackend {
    models: Arc<Mutex<HashMap<String, LoadedModel>>>,
    device: Device,
}

impl Default for CandleBackend {
    fn default() -> Self {
        Self::new()
    }
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

    /// Detect the model architecture from the GGUF "general.architecture" key.
    /// Any architecture other than the four supported ones is refused: running it
    /// through a guessed loader would produce wrong output without an error.
    fn detect_model_type_from_gguf(
        content: &candle_core::quantized::gguf_file::Content,
    ) -> Result<ModelType> {
        let arch = content
            .metadata
            .get("general.architecture")
            .and_then(|v| v.to_string().ok())
            .map(String::as_str)
            .unwrap_or("none");
        match arch {
            "llama" => Ok(ModelType::Llama),
            "mistral" => Ok(ModelType::Mistral),
            "phi2" => Ok(ModelType::Phi2),
            "phi3" => Ok(ModelType::Phi3),
            _ => Err(crate::safety::unimplemented(
                "ai_local_load",
                &format!("GGUF architecture `{arch}`"),
                "load a llama, mistral, phi2 or phi3 GGUF model",
            )),
        }
    }

    /// Load `tokenizer.json` from the model's own directory. No other directory
    /// is searched: in a shared models folder a parent's tokenizer may belong to
    /// a different model.
    fn load_tokenizer(model_path: &str) -> Result<tokenizers::Tokenizer> {
        let tokenizer_path = std::path::Path::new(model_path)
            .parent()
            .map(|dir| dir.join("tokenizer.json"))
            .filter(|p| p.exists());

        match tokenizer_path {
            Some(p) => tokenizers::Tokenizer::from_file(&p)
                .map_err(|e| anyhow!("Failed to load tokenizer: {}", e)),
            None => Err(anyhow!(
                "No tokenizer.json found for {}. A tokenizer.json is needed in the same directory as the model.",
                model_path
            )),
        }
    }

    /// Build a Mistral config from GGUF metadata. The required keys must be
    /// present: a guessed size would run the file as a different model.
    fn mistral_config_from_gguf(
        content: &candle_core::quantized::gguf_file::Content,
    ) -> Result<candle_transformers::models::mistral::Config> {
        use candle_nn::Activation;

        // Helper to extract u32 value from GGUF metadata
        let get_u32 = |key: &str| -> Option<usize> {
            content.metadata.get(key).and_then(|v| {
                let s = format!("{:?}", v);
                s.trim_start_matches("U32(")
                    .trim_end_matches(')')
                    .parse::<usize>()
                    .ok()
            })
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

        let require = |key: &str| -> Result<usize> {
            get_u32(key).ok_or_else(|| {
                crate::safety::arg_err(format!(
                    "ai_local_load: the GGUF file has no `{key}`; it cannot be run as mistral"
                ))
            })
        };

        let prefix = "mistral";
        let hidden_size = require(&format!("{prefix}.embedding_length"))?;
        let num_heads = require(&format!("{prefix}.attention.head_count"))?;
        let num_kv_heads = require(&format!("{prefix}.attention.head_count_kv"))?;
        // Both are divisors (head_dim, and the query/key-value head ratio)
        if num_heads == 0 || num_kv_heads == 0 {
            return Err(crate::safety::arg_err(
                "ai_local_load: the GGUF file has a zero attention head count; it cannot be run as mistral",
            ));
        }

        // The optional keys below keep the Mistral 7B values when absent
        Ok(candle_transformers::models::mistral::Config {
            vocab_size: require(&format!("{prefix}.vocab_size"))?,
            hidden_size,
            intermediate_size: require(&format!("{prefix}.feed_forward_length"))?,
            num_hidden_layers: require(&format!("{prefix}.block_count"))?,
            num_attention_heads: num_heads,
            head_dim: Some(hidden_size / num_heads),
            num_key_value_heads: num_kv_heads,
            hidden_act: Activation::Silu,
            max_position_embeddings: get_u32(&format!("{prefix}.context_length")).unwrap_or(32768),
            rms_norm_eps: get_f64(&format!("{prefix}.attention.layer_norm_rms_epsilon"), 1e-5),
            rope_theta: get_f64(&format!("{prefix}.rope.freq_base"), 10000.0),
            sliding_window: Some(
                get_u32(&format!("{prefix}.attention.sliding_window")).unwrap_or(4096),
            ),
            use_flash_attn: false,
        })
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
            "<|end_of_text|>",
        ]
        .iter()
        .filter_map(|s| tokenizer.token_to_id(s))
        .collect();
        // Id 2 is EOS in the Llama 2 / Mistral vocabularies, but an ordinary
        // token in others, so it stops generation only when the tokenizer
        // names no end token of its own.
        let eos_tokens = if eos_tokens.is_empty() {
            vec![2u32]
        } else {
            eos_tokens
        };

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
        if eos_tokens.contains(&first_token) {
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
        // Refused, not clamped: a caller asking for more would otherwise get
        // a shorter answer that looks complete.
        const MAX_TOKENS: u32 = 4096;
        if params.max_tokens > MAX_TOKENS {
            return Err(crate::safety::arg_err(format!(
                "ai_local_generate: max_tokens {} is over the local limit of {MAX_TOKENS}",
                params.max_tokens
            )));
        }
        let max_tokens = params.max_tokens;
        for i in 1..max_tokens {
            // The token fed here was sampled at position prompt_len + i - 1:
            // the first one follows the prompt directly.
            let pos = prompt_tokens.len() + i as usize - 1;
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
            if eos_tokens.contains(&next_token) {
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

        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        // A safetensors file would give only the input-embedding table, which is
        // not a usable model: refuse it before anything is read.
        if ext == "safetensors" {
            return Err(crate::safety::unimplemented(
                "ai_local_load",
                "running a safetensors model",
                "load a GGUF model to generate text, or an ONNX model to embed",
            ));
        }
        if ext != "gguf" {
            return Err(anyhow!(
                "Candle backend does not support .{} format. Use .gguf",
                ext
            ));
        }

        let tokenizer = Self::load_tokenizer(path)?;

        // Load GGUF via candle's quantized format support
        let mut file = std::fs::File::open(path).context("Failed to open GGUF model file")?;
        let content = candle_core::quantized::gguf_file::Content::read(
            &mut std::io::BufReader::new(&mut file),
        )?;
        let model_type = Self::detect_model_type_from_gguf(&content)?;

        // Re-open file (Content::read consumed the reader position)
        let mut file2 = std::fs::File::open(path).context("Failed to re-open GGUF file")?;
        let content2 = candle_core::quantized::gguf_file::Content::read(
            &mut std::io::BufReader::new(&mut file2),
        )?;

        let quantized = match model_type {
            ModelType::Llama => {
                let weights = quantized_llama::ModelWeights::from_gguf(
                    content2,
                    &mut std::io::BufReader::new(&mut file2),
                    &self.device,
                )?;
                QuantizedModel::Llama(weights)
            }
            ModelType::Mistral => {
                let mistral_config = Self::mistral_config_from_gguf(&content2)?;
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

        let loaded = LoadedModel {
            quantized,
            tokenizer,
            device: self.device.clone(),
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

        Self::generate_with_quantized(
            &mut model.quantized,
            &model.tokenizer,
            &model.device,
            prompt,
            params,
        )
    }

    fn embed(&self, handle: &str, _inputs: &[String]) -> Result<EmbeddingResult> {
        let models = self.models.lock().unwrap();
        if !models.contains_key(handle) {
            return Err(anyhow!("Model '{}' not loaded", handle));
        }
        // Averaging a GGUF model's input-embedding rows is not a model embedding,
        // so embeddings are refused rather than guessed.
        Err(crate::safety::unimplemented(
            "ai_local_embed",
            "embeddings from a GGUF model",
            "load an ONNX embedding model (with its tokenizer.json) and call ai_local_embed on that",
        ))
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
