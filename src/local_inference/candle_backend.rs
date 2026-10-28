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
use serde_json::Value as JsonValue;

use super::{
    ChatMessage, EmbeddingResult, GenerationParams, InferenceResult, LocalInferenceBackend,
};

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
    /// The model's chat template (Jinja), from the GGUF or tokenizer_config.json
    chat_template: Option<String>,
    /// The BOS token text the template receives as `bos_token`
    bos_token: Option<String>,
    /// The EOS token text the template receives as `eos_token`
    eos_token: Option<String>,
}

/// Model architecture type — determines which quantized model to instantiate
#[derive(Debug, Clone, PartialEq)]
enum ModelType {
    Llama,
    Mistral,
    Phi2,
    Phi3,
}

// ============================================================================
// CHAT TEMPLATES
// ============================================================================

/// The chat template and special tokens a model ships with
struct ChatSettings {
    template: Option<String>,
    bos_token: Option<String>,
    eos_token: Option<String>,
}

/// The chat template string from a `tokenizer_config.json` value. The field is
/// either a string, or an array of `{name, template}` objects (some repos keep a
/// tool-use template beside the default one). The object named "default" is
/// used, else the first.
fn chat_template_from_config(config: &JsonValue) -> Option<String> {
    match config.get("chat_template")? {
        JsonValue::String(s) => Some(s.clone()),
        JsonValue::Array(items) => {
            let named_default = items
                .iter()
                .find(|item| item.get("name").and_then(JsonValue::as_str) == Some("default"));
            named_default
                .or_else(|| items.first())?
                .get("template")?
                .as_str()
                .map(str::to_string)
        }
        _ => None,
    }
}

/// A special token from a `tokenizer_config.json` value. HuggingFace writes it
/// either as a plain string or as an `AddedToken` object with a `content` field.
fn special_token_from_config(config: &JsonValue, key: &str) -> Option<String> {
    let token = config.get(key)?;
    match token {
        JsonValue::String(s) => Some(s.clone()),
        JsonValue::Object(_) => token.get("content")?.as_str().map(str::to_string),
        _ => None,
    }
}

/// Read `tokenizer_config.json` from the model's own directory, as
/// `load_tokenizer` reads `tokenizer.json`. A file that is there but does not
/// parse is an error: its chat template would otherwise vanish without a word.
fn read_tokenizer_config(model_path: &str) -> Result<Option<JsonValue>> {
    let path = match std::path::Path::new(model_path)
        .parent()
        .map(|dir| dir.join("tokenizer_config.json"))
        .filter(|p| p.exists())
    {
        Some(p) => p,
        None => return Ok(None),
    };
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let config = serde_json::from_str(&text)
        .with_context(|| format!("{} is not valid JSON", path.display()))?;
    Ok(Some(config))
}

/// The chat template and special tokens of a model. The GGUF metadata is read
/// first, then the tokenizer_config.json beside the model. The GGUF names its
/// special tokens by vocabulary id, which is mapped to text through the tokenizer.
fn chat_settings(
    content: &candle_core::quantized::gguf_file::Content,
    tokenizer: &tokenizers::Tokenizer,
    config: Option<&JsonValue>,
) -> ChatSettings {
    let gguf_token = |key: &str| {
        content
            .metadata
            .get(key)
            .and_then(|v| v.to_u32().ok())
            .and_then(|id| tokenizer.id_to_token(id))
    };
    let config_token = |key: &str| config.and_then(|c| special_token_from_config(c, key));
    let template = content
        .metadata
        .get("tokenizer.chat_template")
        .and_then(|v| v.to_string().ok())
        .cloned()
        .or_else(|| config.and_then(chat_template_from_config));
    ChatSettings {
        template,
        bos_token: gguf_token("tokenizer.ggml.bos_token_id").or_else(|| config_token("bos_token")),
        eos_token: gguf_token("tokenizer.ggml.eos_token_id").or_else(|| config_token("eos_token")),
    }
}

/// Render a chat template over `messages` the way HuggingFace's
/// `apply_chat_template` does: `messages`, `add_generation_prompt` (true, so the
/// reply header is written), `bos_token` and `eos_token`. A template that calls
/// `raise_exception(message)` fails the render with that message, which is how a
/// template refuses input it cannot format.
fn render_chat_template(
    template: &str,
    messages: &[ChatMessage],
    bos_token: &str,
    eos_token: &str,
) -> std::result::Result<String, minijinja::Error> {
    let mut env = minijinja::Environment::new();
    // The templates are written for Python's Jinja, and call str and dict
    // methods (`.strip()`, `.items()`) that minijinja has only through this.
    env.set_unknown_method_callback(minijinja_contrib::pycompat::unknown_method_callback);
    env.add_function(
        "raise_exception",
        |message: String| -> std::result::Result<String, minijinja::Error> {
            Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                message,
            ))
        },
    );
    env.add_template("chat", template)?;
    let rendered = env.get_template("chat")?.render(minijinja::context! {
        messages => minijinja::Value::from_serialize(messages),
        add_generation_prompt => true,
        bos_token => bos_token,
        eos_token => eos_token,
    })?;
    Ok(rendered)
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

    /// Autoregressive text generation using a loaded quantized model. `prompt`
    /// is the final text. `add_special_tokens` is passed to the tokenizer: a
    /// rendered chat template already writes its own BOS, so it is false there.
    /// `model_eos` is the model's own end token, which stops generation beside
    /// the common ones: Gemma's `<end_of_turn>` is in none of them.
    fn generate_with_quantized(
        model: &mut QuantizedModel,
        tokenizer: &tokenizers::Tokenizer,
        device: &Device,
        prompt: &str,
        add_special_tokens: bool,
        model_eos: Option<&str>,
        params: &GenerationParams,
    ) -> Result<InferenceResult> {
        let start = std::time::Instant::now();

        // Tokenize input
        let encoding = tokenizer
            .encode(prompt, add_special_tokens)
            .map_err(|e| anyhow!("Tokenization failed: {}", e))?;
        let prompt_tokens = encoding.get_ids().to_vec();
        let prompt_len = prompt_tokens.len() as u32;

        if prompt_tokens.is_empty() {
            return Err(anyhow!("Empty prompt after tokenization"));
        }

        // Create logits processor for sampling
        let mut logits_processor = Self::make_logits_processor(params);

        // Determine EOS token(s)
        let mut eos_tokens: Vec<u32> = [
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
        if let Some(id) = model_eos.and_then(|t| tokenizer.token_to_id(t)) {
            if !eos_tokens.contains(&id) {
                eos_tokens.push(id);
            }
        }
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
        let config = read_tokenizer_config(path)?;
        let chat = chat_settings(&content, &tokenizer, config.as_ref());

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
            chat_template: chat.template,
            bos_token: chat.bos_token,
            eos_token: chat.eos_token,
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
            true,
            model.eos_token.as_deref(),
            params,
        )
    }

    fn chat(
        &self,
        handle: &str,
        messages: &[ChatMessage],
        params: &GenerationParams,
    ) -> Result<InferenceResult> {
        let mut models = self.models.lock().unwrap();
        let model = models
            .get_mut(handle)
            .ok_or_else(|| anyhow!("Model '{}' not loaded", handle))?;

        let template = model.chat_template.as_deref().ok_or_else(|| {
            crate::safety::unimplemented(
                "ai_local_generate",
                "chat messages for a model with no chat template",
                "pass a prompt string, formatted for this model",
            )
        })?;
        let prompt = render_chat_template(
            template,
            messages,
            model.bos_token.as_deref().unwrap_or(""),
            model.eos_token.as_deref().unwrap_or(""),
        )
        .map_err(|e| {
            crate::safety::bad_state(
                "ai_local_generate",
                &format!("the model's chat template failed: {e}"),
                "a model whose chat template renders",
            )
        })?;

        // No special tokens are added to the rendered text: the template writes
        // its own BOS, and a second one would reach the model.
        Self::generate_with_quantized(
            &mut model.quantized,
            &model.tokenizer,
            &model.device,
            &prompt,
            false,
            model.eos_token.as_deref(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// ChatML, as Qwen and many other instruct models ship it
    const CHATML: &str = "{% for message in messages %}<|im_start|>{{ message.role }}\n{{ message.content }}<|im_end|>\n{% endfor %}{% if add_generation_prompt %}<|im_start|>assistant\n{% endif %}";

    fn user(content: &str) -> ChatMessage {
        ChatMessage {
            role: "user".to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn chatml_template_renders_one_user_turn_with_generation_prompt() {
        let out = render_chat_template(CHATML, &[user("hi")], "", "").unwrap();
        assert_eq!(
            out,
            "<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n"
        );
    }

    #[test]
    fn python_string_methods_render_as_in_llama3_templates() {
        let template = "{% for m in messages %}{{ m.content | trim }}|{{ m['content'].strip() }}{% endfor %}";
        assert_eq!(
            render_chat_template(template, &[user("  hi  ")], "", "").unwrap(),
            "hi|hi"
        );
    }

    #[test]
    fn tokenizer_config_array_form_picks_the_default_template() {
        let config = json!({"chat_template": [
            {"name": "tool_use", "template": "TOOL"},
            {"name": "default", "template": "DEFAULT"}
        ]});
        assert_eq!(
            chat_template_from_config(&config).as_deref(),
            Some("DEFAULT")
        );

        let no_default = json!({"chat_template": [
            {"name": "tool_use", "template": "TOOL"},
            {"name": "other", "template": "OTHER"}
        ]});
        assert_eq!(
            chat_template_from_config(&no_default).as_deref(),
            Some("TOOL")
        );

        let string_form = json!({"chat_template": "PLAIN"});
        assert_eq!(
            chat_template_from_config(&string_form).as_deref(),
            Some("PLAIN")
        );
        assert_eq!(chat_template_from_config(&json!({})), None);
    }

    #[test]
    fn raise_exception_fails_the_render_with_its_message() {
        let template = "{% if messages[0].role == 'system' %}{{ raise_exception('no system role here') }}{% endif %}ok";
        let system = ChatMessage {
            role: "system".to_string(),
            content: "be brief".to_string(),
        };
        let err = render_chat_template(template, &[system], "", "").unwrap_err();
        assert!(err.to_string().contains("no system role here"), "{err}");

        // The same template renders when the condition is not met
        assert_eq!(
            render_chat_template(template, &[user("hi")], "", "").unwrap(),
            "ok"
        );
    }

    #[test]
    fn special_token_is_read_from_string_or_content_object() {
        let config = json!({
            "bos_token": {"content": "<s>", "lstrip": false},
            "eos_token": "</s>"
        });
        assert_eq!(
            special_token_from_config(&config, "bos_token").as_deref(),
            Some("<s>")
        );
        assert_eq!(
            special_token_from_config(&config, "eos_token").as_deref(),
            Some("</s>")
        );
        assert_eq!(special_token_from_config(&config, "pad_token"), None);
    }
}
