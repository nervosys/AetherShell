//! ONNX Runtime Backend — embeddings from ONNX encoder models
//!
//! Loads .onnx files through the `ort` crate, with the model's own
//! `tokenizer.json` (beside the model, or one directory up as in a HuggingFace
//! `onnx/` layout). Embedding models are supported: sentence-transformers and
//! BERT-style encoders that output `sentence_embedding` or
//! `last_hidden_state`. Text generation is not: decoding needs a model-specific
//! loop with past key-values, so `generate` refuses rather than answering from
//! a single forward pass. GGUF models generate through the candle backend.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ort::session::Session;
use ort::value::Tensor;

use super::{EmbeddingResult, GenerationParams, InferenceResult, LocalInferenceBackend};

/// A loaded ONNX model session
struct LoadedOnnxModel {
    /// ONNX Runtime session
    session: Session,
    /// The model's own tokenizer
    tokenizer: tokenizers::Tokenizer,
    /// Input names, in the graph's order
    input_names: Vec<String>,
    /// Output names, in the graph's order
    output_names: Vec<String>,
}

/// ONNX Runtime inference backend
pub struct OrtBackend {
    models: Arc<Mutex<HashMap<String, LoadedOnnxModel>>>,
}

impl Default for OrtBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl OrtBackend {
    pub fn new() -> Self {
        Self {
            models: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// The tokenizer.json beside the model, or one directory up.
    fn load_tokenizer(model_path: &str) -> Result<tokenizers::Tokenizer> {
        let path = std::path::Path::new(model_path);
        for dir in [path.parent(), path.parent().and_then(|d| d.parent())]
            .into_iter()
            .flatten()
        {
            let candidate = dir.join("tokenizer.json");
            if candidate.exists() {
                return tokenizers::Tokenizer::from_file(&candidate).map_err(|e| {
                    anyhow!("failed to load tokenizer {}: {}", candidate.display(), e)
                });
            }
        }
        Err(anyhow!(
            "no tokenizer.json beside {} or one directory up; an ONNX model's token ids \
             come from its own tokenizer, so it cannot be run without one",
            model_path
        ))
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, LoadedOnnxModel>>> {
        self.models
            .lock()
            .map_err(|_| crate::safety::lock_poisoned("ai_local", "the ONNX model table"))
    }
}

impl LocalInferenceBackend for OrtBackend {
    fn name(&self) -> &str {
        "onnx"
    }

    fn load_model(&self, path: &str) -> Result<String> {
        let handle = format!(
            "onnx:{}",
            std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("model")
        );

        let tokenizer = Self::load_tokenizer(path)?;
        let session = Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .with_intra_threads(num_cpus())?
            .commit_from_file(path)?;

        let input_names: Vec<String> = session
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();
        let output_names: Vec<String> = session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        if !input_names.iter().any(|n| n == "input_ids") {
            return Err(anyhow!(
                "{} has no `input_ids` input (inputs: {}); only encoder models that take \
                 token ids are supported",
                path,
                input_names.join(", ")
            ));
        }

        let loaded = LoadedOnnxModel {
            session,
            tokenizer,
            input_names,
            output_names,
        };
        self.lock()?.insert(handle.clone(), loaded);
        Ok(handle)
    }

    fn unload_model(&self, handle: &str) -> Result<()> {
        self.lock()?
            .remove(handle)
            .ok_or_else(|| anyhow!("Model '{}' not loaded", handle))?;
        Ok(())
    }

    fn loaded_models(&self) -> Vec<String> {
        self.lock()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn generate(
        &self,
        _handle: &str,
        _prompt: &str,
        _params: &GenerationParams,
    ) -> Result<InferenceResult> {
        Err(crate::safety::unimplemented(
            "ai_local_generate",
            "text generation from an ONNX model",
            "load a GGUF model, which the candle backend generates from, or use this \
             ONNX model with ai_local_embed",
        ))
    }

    fn embed(&self, handle: &str, inputs: &[String]) -> Result<EmbeddingResult> {
        let mut models = self.lock()?;
        let model = models
            .get_mut(handle)
            .ok_or_else(|| anyhow!("Model '{}' not loaded", handle))?;

        let mut all_embeddings = Vec::with_capacity(inputs.len());
        let mut total_tokens = 0u32;

        for input in inputs {
            let encoding = model
                .tokenizer
                .encode(input.as_str(), true)
                .map_err(|e| anyhow!("tokenizing input: {}", e))?;
            let ids: Vec<i64> = encoding.get_ids().iter().map(|&i| i as i64).collect();
            let mask: Vec<i64> = encoding
                .get_attention_mask()
                .iter()
                .map(|&m| m as i64)
                .collect();
            let type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&t| t as i64).collect();
            let seq_len = ids.len();
            total_tokens += seq_len as u32;

            // Feed every input the graph declares that this knows how to fill.
            let mut feed = Vec::new();
            for name in &model.input_names {
                let data = match name.as_str() {
                    "input_ids" => ids.clone(),
                    "attention_mask" => mask.clone(),
                    "token_type_ids" => type_ids.clone(),
                    other => {
                        return Err(anyhow!(
                            "the model needs an input `{}` that a tokenizer does not provide",
                            other
                        ))
                    }
                };
                feed.push((
                    std::borrow::Cow::<str>::from(name.clone()),
                    ort::session::SessionInputValue::from(Tensor::from_array((
                        [1usize, seq_len],
                        data,
                    ))?),
                ));
            }
            let outputs = model.session.run(feed)?;

            // A pooled output is used as it is; a hidden state is mean-pooled
            // over the tokens the attention mask keeps.
            let embedding = if let Some(pooled) = outputs.get("sentence_embedding") {
                let (_, data) = pooled.try_extract_tensor::<f32>()?;
                data.to_vec()
            } else {
                let name = model
                    .output_names
                    .iter()
                    .find(|n| n.as_str() == "last_hidden_state")
                    .or(model.output_names.first())
                    .ok_or_else(|| anyhow!("the model has no outputs"))?;
                let (shape, data) = outputs[name.as_str()].try_extract_tensor::<f32>()?;
                if shape.len() != 3 || shape[1] as usize != seq_len {
                    return Err(anyhow!(
                        "output `{}` has shape {:?}, not [1, {}, hidden]; not an encoder \
                         hidden state",
                        name,
                        &shape[..],
                        seq_len
                    ));
                }
                let hidden = shape[2] as usize;
                let mut sum = vec![0.0f32; hidden];
                let mut kept = 0.0f32;
                for (t, &m) in mask.iter().enumerate() {
                    if m == 0 {
                        continue;
                    }
                    kept += 1.0;
                    for (d, s) in sum.iter_mut().enumerate() {
                        *s += data[t * hidden + d];
                    }
                }
                if kept > 0.0 {
                    sum.iter_mut().for_each(|s| *s /= kept);
                }
                sum
            };
            all_embeddings.push(embedding);
        }

        Ok(EmbeddingResult {
            embeddings: all_embeddings,
            total_tokens,
        })
    }

    fn supports_format(&self, extension: &str) -> bool {
        extension == "onnx"
    }

    fn estimate_memory_mb(&self, path: &str) -> Result<u64> {
        let metadata = std::fs::metadata(path)?;
        // ONNX models need ~1.5x file size due to runtime graph optimization
        Ok((metadata.len() as f64 * 1.5 / (1024.0 * 1024.0)) as u64)
    }
}

/// Get the number of CPU cores for threading
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
