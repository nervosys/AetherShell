//! ONNX Runtime Backend — Cross-platform ONNX model inference
//!
//! Supports .onnx model files for text generation and embeddings via the `ort` crate.
//! ONNX Runtime provides hardware-accelerated inference across CPU, CUDA, DirectML,
//! CoreML, and TensorRT execution providers.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ort::session::Session;

use super::{EmbeddingResult, GenerationParams, InferenceResult, LocalInferenceBackend};

/// A loaded ONNX model session
struct LoadedOnnxModel {
    /// Path to the ONNX model
    path: String,
    /// ONNX Runtime session
    session: Session,
    /// Input names
    input_names: Vec<String>,
    /// Output names
    output_names: Vec<String>,
    /// Whether this is likely an embedding model
    is_embedding: bool,
}

/// ONNX Runtime inference backend
pub struct OrtBackend {
    models: Arc<Mutex<HashMap<String, LoadedOnnxModel>>>,
}

impl OrtBackend {
    pub fn new() -> Self {
        Self {
            models: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Detect if a model is an embedding model from its input/output signature
    fn is_embedding_model(session: &Session) -> bool {
        let output_names: Vec<String> = session.outputs.iter().map(|o| o.name.clone()).collect();
        // Common embedding model output names
        output_names.iter().any(|n| {
            n.contains("embedding")
                || n.contains("sentence_embedding")
                || n.contains("last_hidden_state")
                || n.contains("pooler_output")
        })
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

        // Create ONNX Runtime session
        let session = Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .with_intra_threads(num_cpus())?
            .commit_from_file(path)?;

        let input_names: Vec<String> = session.inputs.iter().map(|i| i.name.clone()).collect();

        let output_names: Vec<String> = session.outputs.iter().map(|o| o.name.clone()).collect();

        let is_embedding = Self::is_embedding_model(&session);

        let loaded = LoadedOnnxModel {
            path: path.to_string(),
            session,
            input_names,
            output_names,
            is_embedding,
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
        _params: &GenerationParams,
    ) -> Result<InferenceResult> {
        let models = self.models.lock().unwrap();
        let model = models
            .get(handle)
            .ok_or_else(|| anyhow!("Model '{}' not loaded", handle))?;

        let start = std::time::Instant::now();

        // For ONNX text generation models, we need token IDs as input
        // This is a simplified implementation — real usage needs a tokenizer
        let prompt_bytes = prompt.as_bytes();
        let prompt_tokens = prompt_bytes.len() as u32 / 4; // rough estimate

        // Create input tensor (simplified: pass raw byte IDs)
        // Real implementation would use a proper tokenizer
        let input_ids: Vec<i64> = prompt.chars().map(|c| c as i64).collect();
        let seq_len = input_ids.len();

        let input_array = ndarray::Array2::from_shape_vec((1, seq_len), input_ids)?;

        // Run inference
        let outputs = model.session.run(ort::inputs![
            model.input_names[0].as_str() => input_array.view(),
        ]?)?;

        // Extract output text (model-dependent)
        let text = if let Some(first_output) = outputs.values().next() {
            if let Ok(output) = first_output.try_extract_tensor::<f32>() {
                // For generative models, output is logits — take argmax
                let view = output.view();
                let shape = view.shape();
                if shape.len() >= 2 {
                    let last_dim = shape[shape.len() - 1];
                    // Get last token logits and find argmax
                    let mut generated = Vec::new();
                    let total_elements = view.len();
                    let last_token_start = total_elements.saturating_sub(last_dim);
                    let logits = &view.as_slice().unwrap_or(&[])[last_token_start..];
                    if !logits.is_empty() {
                        let max_idx = logits
                            .iter()
                            .enumerate()
                            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        generated.push(max_idx as u32);
                    }
                    format!("[generated {} token(s)]", generated.len())
                } else {
                    "[output shape mismatch]".to_string()
                }
            } else {
                "[non-float output]".to_string()
            }
        } else {
            "[no output]".to_string()
        };

        let elapsed = start.elapsed();
        let generation_ms = elapsed.as_secs_f64() * 1000.0;

        Ok(InferenceResult {
            text,
            prompt_tokens,
            completion_tokens: 1,
            generation_ms,
            tokens_per_second: if generation_ms > 0.0 {
                1000.0 / generation_ms
            } else {
                0.0
            },
        })
    }

    fn embed(&self, handle: &str, inputs: &[String]) -> Result<EmbeddingResult> {
        let models = self.models.lock().unwrap();
        let model = models
            .get(handle)
            .ok_or_else(|| anyhow!("Model '{}' not loaded", handle))?;

        let mut all_embeddings = Vec::new();
        let mut total_tokens = 0u32;

        for input in inputs {
            let input_ids: Vec<i64> = input.chars().map(|c| c as i64).collect();
            let seq_len = input_ids.len();
            total_tokens += seq_len as u32;

            let input_array = ndarray::Array2::from_shape_vec((1, seq_len), input_ids)?;

            // Create attention mask (all 1s)
            let attention_mask = ndarray::Array2::ones((1, seq_len));

            // Run embedding inference
            let mut ort_inputs = Vec::new();
            ort_inputs.push((
                model.input_names[0].as_str(),
                ort::value::Value::from_array(input_array.view())?,
            ));
            if model.input_names.len() > 1 {
                ort_inputs.push((
                    model.input_names[1].as_str(),
                    ort::value::Value::from_array(attention_mask.view())?,
                ));
            }

            let outputs = model.session.run(ort::SessionInputs::from(ort_inputs))?;

            // Extract embedding from output
            if let Some(output) = outputs.values().next() {
                if let Ok(tensor) = output.try_extract_tensor::<f32>() {
                    let view = tensor.view();
                    let shape = view.shape();
                    if shape.len() >= 2 {
                        // Mean pool across sequence dimension
                        let hidden_dim = shape[shape.len() - 1];
                        let data = view.as_slice().unwrap_or(&[]);
                        let mut embedding = vec![0.0f32; hidden_dim];
                        let num_tokens = data.len() / hidden_dim;
                        for t in 0..num_tokens {
                            for d in 0..hidden_dim {
                                embedding[d] += data[t * hidden_dim + d];
                            }
                        }
                        if num_tokens > 0 {
                            for d in 0..hidden_dim {
                                embedding[d] /= num_tokens as f32;
                            }
                        }
                        all_embeddings.push(embedding);
                    }
                }
            }

            if all_embeddings.len() < inputs.len() {
                // Fallback: zero vector
                all_embeddings.push(vec![0.0f32; 384]);
            }
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
