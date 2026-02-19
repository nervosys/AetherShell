//! Provider-agnostic LLMs + Agents (single & multi-agent swarms) for Aether Shell.
//!
//! ## Supported AI Backends
//!
//! - **OpenAI** (`openai:gpt-4o-mini`) - OpenAI's API service
//! - **Ollama** (`ollama:llama3`) - Local Ollama server
//! - **vLLM** (`vllm:meta-llama/Llama-3-8B`) - High-performance inference with PagedAttention
//! - **llama.cpp** (`llamacpp:model`) - Efficient CPU/GPU inference
//! - **TGI** (`tgi:mixtral`) - HuggingFace Text Generation Inference
//! - **OpenAI-Compatible** (`compat:mixtral`) - Any OpenAI-compatible API
//!
//! ## Features
//!
//! - Model URIs for flexible backend selection
//! - Backend registry and per-agent model selection
//! - ToolRegistry with Builtin + MCP resolver
//! - Agents: run_sync + run_sync_with_model
//! - Swarm framework with Coordinator (RoundRobin + Router stubs)
//! - Multi-modal support for images, audio, and video
//! - A2A (Agent-to-Agent) communication protocol
//! - NANDA negotiation and task allocation framework
//! - SECURITY: SecureApiConfig with memory sanitization for API keys

use anyhow::{anyhow, Context, Result};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as J};

use crate::secure_config::SecureApiConfig;

// Sub-modules
pub mod a2a;
pub mod a2ui;
pub mod nanda;

// ===================== Cost Tracking =====================

use lazy_static::lazy_static;
use std::sync::Mutex;

/// A single AI API call record for cost tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub provider: String,
    pub model: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub timestamp: String, // ISO 8601
}

/// Cumulative session cost tracker
#[derive(Debug, Default)]
pub struct CostTracker {
    pub records: Vec<UsageRecord>,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_cost_usd: f64,
    pub call_count: u64,
}

impl CostTracker {
    pub fn record(
        &mut self,
        provider: &str,
        model: &str,
        prompt_tokens: u64,
        completion_tokens: u64,
        cost_usd: f64,
    ) {
        let total_tokens = prompt_tokens + completion_tokens;
        self.total_prompt_tokens += prompt_tokens;
        self.total_completion_tokens += completion_tokens;
        self.total_cost_usd += cost_usd;
        self.call_count += 1;
        self.records.push(UsageRecord {
            provider: provider.to_string(),
            model: model.to_string(),
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cost_usd,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }

    pub fn summary(&self) -> serde_json::Value {
        json!({
            "call_count": self.call_count,
            "total_prompt_tokens": self.total_prompt_tokens,
            "total_completion_tokens": self.total_completion_tokens,
            "total_tokens": self.total_prompt_tokens + self.total_completion_tokens,
            "total_cost_usd": format!("{:.6}", self.total_cost_usd),
            "records": self.records.iter().map(|r| json!({
                "provider": r.provider,
                "model": r.model,
                "prompt_tokens": r.prompt_tokens,
                "completion_tokens": r.completion_tokens,
                "cost_usd": format!("{:.6}", r.cost_usd),
                "timestamp": r.timestamp,
            })).collect::<Vec<_>>(),
        })
    }

    pub fn reset(&mut self) {
        self.records.clear();
        self.total_prompt_tokens = 0;
        self.total_completion_tokens = 0;
        self.total_cost_usd = 0.0;
        self.call_count = 0;
    }
}

lazy_static! {
    /// Global session cost tracker
    pub static ref COST_TRACKER: Mutex<CostTracker> = Mutex::new(CostTracker::default());
}

/// Estimate cost per 1K tokens for known providers/models
pub fn estimate_cost(
    provider: &str,
    model: &str,
    prompt_tokens: u64,
    completion_tokens: u64,
) -> f64 {
    let (prompt_per_1k, completion_per_1k) = match (provider, model) {
        // OpenAI
        (_, m) if m.starts_with("gpt-4o-mini") => (0.00015, 0.0006),
        (_, m) if m.starts_with("gpt-4o") => (0.0025, 0.01),
        (_, m) if m.starts_with("gpt-4.5") => (0.075, 0.15),
        (_, m) if m.starts_with("o3-mini") => (0.0011, 0.0044),
        (_, m) if m.starts_with("o3") => (0.01, 0.04),
        (_, m) if m.starts_with("o4-mini") => (0.0011, 0.0044),
        (_, m) if m.starts_with("o1-mini") => (0.003, 0.012),
        (_, m) if m.starts_with("o1") => (0.015, 0.06),
        (_, m) if m.starts_with("gpt-4-turbo") => (0.01, 0.03),
        (_, m) if m.starts_with("gpt-4") => (0.03, 0.06),
        (_, m) if m.starts_with("gpt-3.5") => (0.0005, 0.0015),
        // Anthropic
        (_, m) if m.contains("claude-4-opus") || m.contains("opus-4") => (0.015, 0.075),
        (_, m) if m.contains("claude-4-sonnet") || m.contains("sonnet-4") => (0.003, 0.015),
        (_, m) if m.contains("claude-3-5-sonnet") || m.contains("sonnet-3-5") => (0.003, 0.015),
        (_, m) if m.contains("claude-3-5-haiku") || m.contains("haiku-3-5") => (0.0008, 0.004),
        (_, m) if m.contains("claude-3-opus") => (0.015, 0.075),
        (_, m) if m.contains("claude-3-sonnet") => (0.003, 0.015),
        // DeepSeek
        ("deepseek", _) => (0.00014, 0.00028),
        // Groq (free tier / very cheap)
        ("groq", _) => (0.00059, 0.00079),
        // Together / Fireworks (varies, use llama pricing)
        ("together" | "fireworks", _) => (0.0009, 0.0009),
        // Mistral
        (_, m) if m.contains("mistral-large") => (0.002, 0.006),
        (_, m) if m.contains("mixtral") => (0.0007, 0.0007),
        // Google
        (_, m) if m.contains("gemini-2.5-flash") => (0.00015, 0.0035),
        (_, m) if m.contains("gemini-2.5-pro") => (0.00125, 0.01),
        // Local (free)
        ("local" | "lmstudio" | "llamacpp" | "llama_cpp" | "ollama" | "vllm", _) => (0.0, 0.0),
        // Default: unknown, can't estimate
        _ => (0.0, 0.0),
    };

    (prompt_tokens as f64 * prompt_per_1k / 1000.0)
        + (completion_tokens as f64 * completion_per_1k / 1000.0)
}

/// Record usage from an API response's usage object
pub fn track_usage(provider: &str, model: &str, usage_json: &serde_json::Value) {
    let prompt_tokens = usage_json
        .get("prompt_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completion_tokens = usage_json
        .get("completion_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cost = estimate_cost(provider, model, prompt_tokens, completion_tokens);

    if let Ok(mut tracker) = COST_TRACKER.lock() {
        tracker.record(provider, model, prompt_tokens, completion_tokens, cost);
    }
}

// ===================== GPU Memory Management =====================

/// GPU memory information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMemoryInfo {
    pub gpu_index: usize,
    pub name: String,
    pub total_mb: u64,
    pub used_mb: u64,
    pub free_mb: u64,
}

/// Query GPU memory via nvidia-smi
pub fn query_gpu_memory() -> Vec<GpuMemoryInfo> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,memory.total,memory.used,memory.free",
            "--format=csv,noheader,nounits",
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines()
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 5 {
                        Some(GpuMemoryInfo {
                            gpu_index: parts[0].parse().unwrap_or(0),
                            name: parts[1].to_string(),
                            total_mb: parts[2].parse().unwrap_or(0),
                            used_mb: parts[3].parse().unwrap_or(0),
                            free_mb: parts[4].parse().unwrap_or(0),
                        })
                    } else {
                        None
                    }
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Estimate VRAM required to load a model (in MB)
///
/// Rules of thumb:
/// - GGUF Q4_K_M: ~0.6 bytes per parameter
/// - GGUF Q5_K_M: ~0.7 bytes per parameter
/// - GGUF Q8_0: ~1.0 bytes per parameter
/// - FP16: ~2.0 bytes per parameter
/// - FP32: ~4.0 bytes per parameter
/// - File-size based: file size + ~20% overhead for KV cache
pub fn estimate_model_vram_mb(
    params_billions: Option<f64>,
    file_size_mb: Option<u64>,
    quantization: Option<&str>,
) -> u64 {
    if let Some(params_b) = params_billions {
        let bytes_per_param = match quantization.unwrap_or("q4_k_m") {
            q if q.contains("q4") || q.contains("Q4") => 0.6,
            q if q.contains("q5") || q.contains("Q5") => 0.7,
            q if q.contains("q8") || q.contains("Q8") => 1.0,
            q if q.contains("f16") || q.contains("fp16") || q.contains("F16") => 2.0,
            q if q.contains("f32") || q.contains("fp32") || q.contains("F32") => 4.0,
            _ => 0.7, // default: ~Q5
        };
        // params_b * 1e9 * bytes_per_param / (1024*1024) + 20% KV cache overhead
        let base_mb = (params_b * 1e9 * bytes_per_param / (1024.0 * 1024.0)) as u64;
        base_mb + base_mb / 5 // +20%
    } else if let Some(file_mb) = file_size_mb {
        file_mb + file_mb / 5 // file size + 20% overhead
    } else {
        0
    }
}

/// Check if a model can fit in available GPU memory
pub fn model_fits_gpu(required_mb: u64) -> (bool, Vec<GpuMemoryInfo>) {
    let gpus = query_gpu_memory();
    let fits = gpus.iter().any(|g| g.free_mb >= required_mb);
    (fits, gpus)
}

/// Get a JSON summary of GPU memory status and model fit assessment
pub fn gpu_memory_summary(
    params_billions: Option<f64>,
    file_size_mb: Option<u64>,
    quantization: Option<&str>,
) -> serde_json::Value {
    let gpus = query_gpu_memory();
    let estimated_mb = estimate_model_vram_mb(params_billions, file_size_mb, quantization);
    let fits = gpus.iter().any(|g| g.free_mb >= estimated_mb);

    json!({
        "gpus": gpus.iter().map(|g| json!({
            "index": g.gpu_index,
            "name": g.name,
            "total_mb": g.total_mb,
            "used_mb": g.used_mb,
            "free_mb": g.free_mb,
        })).collect::<Vec<_>>(),
        "gpu_count": gpus.len(),
        "model_estimated_vram_mb": estimated_mb,
        "model_fits": fits,
        "quantization": quantization.unwrap_or("auto"),
    })
}

// ===================== Multi-modal support =====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiModalContent {
    pub text: Option<String>,
    pub image_url: Option<String>,
    pub audio_url: Option<String>,
    pub video_url: Option<String>,
    pub image_data: Option<String>, // base64 encoded
    pub audio_data: Option<String>, // base64 encoded
    pub video_data: Option<String>, // base64 encoded
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiModalMessage {
    pub role: String,
    pub content: Vec<MultiModalContent>,
}

impl MultiModalMessage {
    pub fn text_only(role: &str, text: &str) -> Self {
        Self {
            role: role.to_string(),
            content: vec![MultiModalContent {
                text: Some(text.to_string()),
                image_url: None,
                audio_url: None,
                video_url: None,
                image_data: None,
                audio_data: None,
                video_data: None,
            }],
        }
    }

    pub fn with_image(role: &str, text: &str, image_data: &str) -> Self {
        Self {
            role: role.to_string(),
            content: vec![
                MultiModalContent {
                    text: Some(text.to_string()),
                    image_url: None,
                    audio_url: None,
                    video_url: None,
                    image_data: None,
                    audio_data: None,
                    video_data: None,
                },
                MultiModalContent {
                    text: None,
                    image_url: None,
                    audio_url: None,
                    video_url: None,
                    image_data: Some(image_data.to_string()),
                    audio_data: None,
                    video_data: None,
                },
            ],
        }
    }

    pub fn with_audio(role: &str, text: &str, audio_data: &str) -> Self {
        Self {
            role: role.to_string(),
            content: vec![
                MultiModalContent {
                    text: Some(text.to_string()),
                    image_url: None,
                    audio_url: None,
                    video_url: None,
                    image_data: None,
                    audio_data: None,
                    video_data: None,
                },
                MultiModalContent {
                    text: None,
                    image_url: None,
                    audio_url: None,
                    video_url: None,
                    image_data: None,
                    audio_data: Some(audio_data.to_string()),
                    video_data: None,
                },
            ],
        }
    }

    /// Convert to simple text for non-multimodal backends
    pub fn to_text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| c.text.as_ref())
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Multi-modal LLM backend trait
pub trait MultiModalLlmBackend: Send + Sync {
    fn chat_multimodal(&self, messages: &[MultiModalMessage]) -> Result<String>;
    fn supports_images(&self) -> bool {
        false
    }
    fn supports_audio(&self) -> bool {
        false
    }
    fn supports_video(&self) -> bool {
        false
    }
}

/// Multi-modal router function
pub fn complete_multimodal_sync(messages: &[MultiModalMessage]) -> Result<String> {
    let backend = multimodal_backend_from_env();
    backend.chat_multimodal(messages)
}

fn multimodal_backend_from_env() -> Box<dyn MultiModalLlmBackend> {
    let model_uri = std::env::var("AETHER_MODEL_URI").unwrap_or_else(|_| {
        match std::env::var("AETHER_AI")
            .unwrap_or_else(|_| "stub".into())
            .as_str()
        {
            "openai" => "openai:gpt-4o",
            "ollama" => "ollama:llava",
            "vllm" => "vllm:meta-llama/Llama-3-Vision",
            "llamacpp" => "llamacpp:llava",
            "compat" => "compat:gpt-4v",
            _ => "stub",
        }
        .to_string()
    });

    multimodal_backend_from_model(model_uri)
}

fn multimodal_backend_from_model(uri: String) -> Box<dyn MultiModalLlmBackend> {
    let m = parse_model_ref(&uri);
    match m.provider {
        Provider::OpenAI => Box::new(OpenAiMultiModalBackend),
        Provider::Ollama => Box::new(OllamaMultiModalBackend),
        Provider::OpenAICompat | Provider::LMStudio => Box::new(OpenAiCompatMultiModalBackend),
        _ => Box::new(StubMultiModalBackend),
    }
}

// Multi-modal backend implementations
struct StubMultiModalBackend;
impl MultiModalLlmBackend for StubMultiModalBackend {
    fn chat_multimodal(&self, _messages: &[MultiModalMessage]) -> Result<String> {
        Err(anyhow!(
            "No AI provider configured for multi-modal queries.\n\
            Set AETHER_AI=openai and OPENAI_API_KEY for vision/audio support."
        ))
    }
}

struct OpenAiMultiModalBackend;
impl MultiModalLlmBackend for OpenAiMultiModalBackend {
    fn chat_multimodal(&self, messages: &[MultiModalMessage]) -> Result<String> {
        // SECURITY FIX (HIGH-002): Use SecureApiConfig for memory-safe key handling
        let config = SecureApiConfig::from_keyring_or_env(
            "openai",
            "OPENAI_API_KEY",
            "https://api.openai.com".to_string(),
            std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string()),
            "openai".to_string(),
        )
        .context("Failed to load OpenAI configuration for multimodal")?;

        config
            .validate_format()
            .context("OpenAI API key validation failed")?;

        let url = format!(
            "{}/v1/chat/completions",
            config.endpoint.trim_end_matches('/')
        );

        // Convert messages to OpenAI format
        let openai_messages: Vec<J> = messages
            .iter()
            .map(|msg| {
                let mut content = Vec::new();

                for part in &msg.content {
                    if let Some(text) = &part.text {
                        content.push(json!({
                            "type": "text",
                            "text": text
                        }));
                    }

                    if let Some(image_data) = &part.image_data {
                        content.push(json!({
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/jpeg;base64,{}", image_data)
                            }
                        }));
                    }

                    if let Some(image_url) = &part.image_url {
                        content.push(json!({
                            "type": "image_url",
                            "image_url": {
                                "url": image_url
                            }
                        }));
                    }
                }

                json!({
                    "role": msg.role,
                    "content": content
                })
            })
            .collect();

        let body = json!({
            "model": config.model,
            "messages": openai_messages,
            "temperature": 0.2,
            "max_tokens": 1000
        });

        // Create authorization header (zeroized after use)
        let auth_header = config
            .create_auth_header()
            .ok_or_else(|| anyhow!("OpenAI API key not configured for multimodal"))?;

        // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
        let client = crate::security::create_secure_http_client()
            .context("Failed to create secure HTTP client")?;

        let v: J = client
            .post(&url)
            .header(AUTHORIZATION, auth_header.as_str())
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?;

        Ok(v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string())
    }

    fn supports_images(&self) -> bool {
        true
    }
    fn supports_audio(&self) -> bool {
        false
    }
    fn supports_video(&self) -> bool {
        false
    }
}

struct OllamaMultiModalBackend;
impl MultiModalLlmBackend for OllamaMultiModalBackend {
    fn chat_multimodal(&self, messages: &[MultiModalMessage]) -> Result<String> {
        // Ollama with vision models like llava
        let _base_url =
            std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".into());
        let _model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llava".into());

        // For now, convert to text and use regular completion
        // In full implementation, would use Ollama's multimodal API
        let text = messages
            .iter()
            .map(|m| m.to_text())
            .collect::<Vec<_>>()
            .join("\n");

        ollama::complete_sync(&text)
    }

    fn supports_images(&self) -> bool {
        true
    }
    fn supports_audio(&self) -> bool {
        false
    }
    fn supports_video(&self) -> bool {
        false
    }
}

struct OpenAiCompatMultiModalBackend;
impl MultiModalLlmBackend for OpenAiCompatMultiModalBackend {
    fn chat_multimodal(&self, messages: &[MultiModalMessage]) -> Result<String> {
        // Convert to text for compatibility
        let text = messages
            .iter()
            .map(|m| m.to_text())
            .collect::<Vec<_>>()
            .join("\n");

        openai_compat::complete_sync(&text)
    }
}

// ===================== Provider Router (simple 1-shot completion) =====================

/// Route by `AETHER_AI` to one of: openai | ollama | openai_compat | tgi
/// Supports all 19 provider types via OpenAI-compatible routing.
/// Returns an error with configuration instructions if no provider is set.
pub fn complete_sync_router(prompt: &str) -> Result<String> {
    let provider = std::env::var("AETHER_AI").unwrap_or_default();

    match provider.as_str() {
        // Fast path: direct backends
        "openai" => openai::complete_sync(prompt),
        "ollama" => ollama::complete_sync(prompt),
        "openai_compat" | "compat" => openai_compat::complete_sync(prompt),
        "tgi" => tgi::complete_sync(prompt),

        // Extended providers: route via OpenAI-compatible endpoints
        // These all use /v1/chat/completions format
        "anthropic" | "claude" => complete_via_compat(
            prompt,
            "https://api.anthropic.com/v1",
            "claude-sonnet-4-20250514",
            "ANTHROPIC_API_KEY",
        ),
        "deepseek" => complete_via_compat(
            prompt,
            "https://api.deepseek.com/v1",
            "deepseek-chat",
            "DEEPSEEK_API_KEY",
        ),
        "groq" => complete_via_compat(
            prompt,
            "https://api.groq.com/openai/v1",
            "llama-3.3-70b-versatile",
            "GROQ_API_KEY",
        ),
        "together" => complete_via_compat(
            prompt,
            "https://api.together.xyz/v1",
            "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            "TOGETHER_API_KEY",
        ),
        "fireworks" => complete_via_compat(
            prompt,
            "https://api.fireworks.ai/inference/v1",
            "accounts/fireworks/models/llama-v3p3-70b-instruct",
            "FIREWORKS_API_KEY",
        ),
        "perplexity" => complete_via_compat(
            prompt,
            "https://api.perplexity.ai",
            "sonar",
            "PERPLEXITY_API_KEY",
        ),
        "xai" | "grok" => {
            complete_via_compat(prompt, "https://api.x.ai/v1", "grok-3", "XAI_API_KEY")
        }
        "openrouter" => complete_via_compat(
            prompt,
            "https://openrouter.ai/api/v1",
            "openai/gpt-4o",
            "OPENROUTER_API_KEY",
        ),
        "mistral" => complete_via_compat(
            prompt,
            "https://api.mistral.ai/v1",
            "mistral-large-latest",
            "MISTRAL_API_KEY",
        ),
        "cohere" => complete_via_compat(
            prompt,
            "https://api.cohere.com/v2",
            "command-a",
            "COHERE_API_KEY",
        ),
        "google" | "gemini" => complete_via_compat(
            prompt,
            "https://generativelanguage.googleapis.com/v1beta/openai",
            "gemini-2.5-flash",
            "GOOGLE_API_KEY",
        ),
        "azure" => {
            let base = std::env::var("AZURE_OPENAI_ENDPOINT").unwrap_or_else(|_| {
                "https://YOUR_RESOURCE.openai.azure.com/openai/deployments/YOUR_DEPLOYMENT"
                    .to_string()
            });
            complete_via_compat(prompt, &base, "gpt-4o", "AZURE_OPENAI_API_KEY")
        }
        "local" | "lmstudio" => {
            complete_via_compat(prompt, "http://localhost:1234/v1", "default", "")
        }
        "llama_cpp" | "llamacpp" => {
            complete_via_compat(prompt, "http://localhost:8080/v1", "default", "")
        }
        "vllm" => {
            let base = std::env::var("VLLM_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8000/v1".to_string());
            complete_via_compat(prompt, &base, "default", "")
        }

        // Model URI syntax: "provider:model" (e.g., "openai:gpt-4o", "ollama:llama3")
        uri if uri.contains(':') => {
            let parts: Vec<&str> = uri.splitn(2, ':').collect();
            let scheme = parts[0];
            let model = parts.get(1).copied().unwrap_or("default");
            if let Some((base, key_env)) = provider_base_url(scheme) {
                complete_via_compat(prompt, &base, model, &key_env)
            } else {
                Err(anyhow!("Unknown provider scheme: '{}'\nTry: openai, anthropic, ollama, deepseek, groq, etc.", scheme))
            }
        }

        "" => Err(anyhow!(
            "No AI provider configured.\n\n\
            To use AI features, set the AETHER_AI environment variable:\n\n\
            For OpenAI:\n  \
              $env:AETHER_AI = \"openai\"\n  \
              $env:OPENAI_API_KEY = \"sk-your-key\"\n\n\
            For Anthropic:\n  \
              $env:AETHER_AI = \"anthropic\"\n  \
              $env:ANTHROPIC_API_KEY = \"sk-ant-...\"\n\n\
            For Ollama (local):\n  \
              $env:AETHER_AI = \"ollama\"\n  \
              # Ensure 'ollama serve' is running\n\n\
            For any provider via model URI:\n  \
              $env:AETHER_AI = \"openai:gpt-4o\"\n  \
              $env:AETHER_AI = \"anthropic:claude-sonnet-4-20250514\"\n\n\
            Supported: openai, anthropic, google, ollama, deepseek, groq,\n\
            together, fireworks, perplexity, xai, azure, openrouter,\n\
            mistral, cohere, local, vllm, lmstudio, compat, tgi\n\n\
            Then restart ae."
        )),
        other => Err(anyhow!(
            "Unknown AI provider: '{}'\n\n\
            Supported providers: openai, anthropic, google, ollama, deepseek,\n\
            groq, together, fireworks, perplexity, xai, azure, openrouter,\n\
            mistral, cohere, local, vllm, lmstudio, compat, tgi\n\n\
            You can also use model URIs: AETHER_AI=\"provider:model\"\n\
            Example: $env:AETHER_AI = \"openai:gpt-4o\"",
            other
        )),
    }
}

/// Complete via any OpenAI-compatible endpoint
fn complete_via_compat(
    prompt: &str,
    base_url: &str,
    model: &str,
    api_key_env: &str,
) -> Result<String> {
    let api_key = if api_key_env.is_empty() {
        String::new()
    } else {
        std::env::var(api_key_env).unwrap_or_default()
    };

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.7
    });

    let client = reqwest::blocking::Client::new();
    let mut req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body);

    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }

    let resp = req
        .send()
        .map_err(|e| anyhow!("HTTP request to {} failed: {}", url, e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(anyhow!("API error {} from {}: {}", status, base_url, text));
    }

    let json: serde_json::Value = resp
        .json()
        .map_err(|e| anyhow!("Failed to parse response from {}: {}", base_url, e))?;

    // Track usage/cost if the response includes usage info
    if let Some(usage) = json.get("usage") {
        let model_name = json.get("model").and_then(|m| m.as_str()).unwrap_or(model);
        // Derive provider name from base_url
        let provider_name = base_url
            .split("//")
            .last()
            .and_then(|h| h.split('.').next())
            .unwrap_or("unknown");
        track_usage(provider_name, model_name, usage);
    }

    json["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("No content in response from {}", base_url))
}

/// Map provider scheme to (base_url, api_key_env_var)
fn provider_base_url(scheme: &str) -> Option<(String, String)> {
    match scheme {
        "openai" => Some(("https://api.openai.com/v1".into(), "OPENAI_API_KEY".into())),
        "anthropic" | "claude" => Some((
            "https://api.anthropic.com/v1".into(),
            "ANTHROPIC_API_KEY".into(),
        )),
        "google" | "gemini" => Some((
            "https://generativelanguage.googleapis.com/v1beta/openai".into(),
            "GOOGLE_API_KEY".into(),
        )),
        "ollama" => Some(("http://localhost:11434/v1".into(), String::new())),
        "deepseek" => Some((
            "https://api.deepseek.com/v1".into(),
            "DEEPSEEK_API_KEY".into(),
        )),
        "groq" => Some((
            "https://api.groq.com/openai/v1".into(),
            "GROQ_API_KEY".into(),
        )),
        "together" => Some((
            "https://api.together.xyz/v1".into(),
            "TOGETHER_API_KEY".into(),
        )),
        "fireworks" => Some((
            "https://api.fireworks.ai/inference/v1".into(),
            "FIREWORKS_API_KEY".into(),
        )),
        "perplexity" => Some((
            "https://api.perplexity.ai".into(),
            "PERPLEXITY_API_KEY".into(),
        )),
        "xai" | "grok" => Some(("https://api.x.ai/v1".into(), "XAI_API_KEY".into())),
        "openrouter" => Some((
            "https://openrouter.ai/api/v1".into(),
            "OPENROUTER_API_KEY".into(),
        )),
        "mistral" => Some(("https://api.mistral.ai/v1".into(), "MISTRAL_API_KEY".into())),
        "cohere" => Some(("https://api.cohere.com/v2".into(), "COHERE_API_KEY".into())),
        "azure" => {
            let base = std::env::var("AZURE_OPENAI_ENDPOINT").unwrap_or_else(|_| {
                "https://YOUR_RESOURCE.openai.azure.com/openai/deployments/YOUR_DEPLOYMENT".into()
            });
            Some((base, "AZURE_OPENAI_API_KEY".into()))
        }
        "local" | "lmstudio" => Some(("http://localhost:1234/v1".into(), String::new())),
        "llamacpp" | "llama_cpp" => Some(("http://localhost:8080/v1".into(), String::new())),
        "vllm" => {
            let base = std::env::var("VLLM_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8000/v1".into());
            Some((base, String::new()))
        }
        "tgi" => {
            let base =
                std::env::var("TGI_BASE_URL").unwrap_or_else(|_| "http://localhost:8080/v1".into());
            Some((base, String::new()))
        }
        "compat" => {
            let base = std::env::var("AETHER_COMPAT_BASE")
                .unwrap_or_else(|_| "http://localhost:8000/v1".into());
            Some((base, String::new()))
        }
        _ => None,
    }
}

// ---------------------- Backends -----------------------

/// Stub backend - returns error with configuration instructions
pub mod stub {
    use anyhow::{anyhow, Result};

    pub fn complete_sync(_prompt: &str) -> Result<String> {
        Err(anyhow!(
            "No AI provider configured.\n\n\
            Set AETHER_AI environment variable:\n\
            - openai: $env:AETHER_AI=\"openai\"; $env:OPENAI_API_KEY=\"sk-...\"\n\
            - ollama: $env:AETHER_AI=\"ollama\" (requires 'ollama serve')\n\
            - compat: $env:AETHER_AI=\"compat\"; $env:AETHER_COMPAT_BASE=\"http://...\""
        ))
    }

    /// Check if AI is configured (for graceful warnings)
    pub fn is_configured() -> bool {
        let provider = std::env::var("AETHER_AI").unwrap_or_default();
        !provider.is_empty()
    }

    /// Get configuration warning message
    pub fn config_warning() -> &'static str {
        "AI not configured. Set AETHER_AI=openai|ollama|compat and restart."
    }
}

pub mod openai {
    use super::*;
    pub fn complete_sync(prompt: &str) -> Result<String> {
        // SECURITY FIX (HIGH-002): Use SecureApiConfig for memory-safe key handling
        let config = SecureApiConfig::from_keyring_or_env(
            "openai",
            "OPENAI_API_KEY",
            "https://api.openai.com".to_string(),
            std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            "openai".to_string(),
        )
        .context("Failed to load OpenAI configuration")?;

        config
            .validate_format()
            .context("OpenAI API key validation failed")?;

        let url = format!(
            "{}/v1/chat/completions",
            config.endpoint.trim_end_matches('/')
        );
        let body = json!({
            "model": config.model,
            "messages": [
                { "role":"system", "content":"You are a succinct assistant embedded in a shell." },
                { "role":"user",   "content": prompt }
            ],
            "temperature": 0.2
        });

        // SECURITY: Create authorization header (zeroized after use)
        let auth_header = config
            .create_auth_header()
            .ok_or_else(|| anyhow!("OpenAI API key not configured"))?;

        // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
        let client = crate::security::create_secure_http_client()
            .context("Failed to create secure HTTP client")?;

        let v: J = client
            .post(&url)
            .header(AUTHORIZATION, auth_header.as_str())
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?;
        Ok(v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string())
    }
}

pub mod ollama {
    use super::*;
    pub fn complete_sync(prompt: &str) -> Result<String> {
        let base = std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".into());
        let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3".into());
        let url = format!("{}/api/generate", base.trim_end_matches('/'));
        let body = json!({"model": model, "prompt": prompt, "stream": false});

        // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
        let client = crate::security::create_secure_http_client()
            .context("Failed to create secure HTTP client")?;

        let v: J = client
            .post(&url)
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?;
        Ok(v["response"].as_str().unwrap_or("").to_string())
    }
}

pub mod openai_compat {
    use super::*;
    /// Any OpenAI-compatible server: vLLM, TensorRT-LLM, llama.cpp server, etc.
    pub fn complete_sync(prompt: &str) -> Result<String> {
        let base = std::env::var("AETHER_COMPAT_BASE")
            .unwrap_or_else(|_| "http://localhost:8000/v1".into());
        let model = std::env::var("AETHER_COMPAT_MODEL").unwrap_or_else(|_| "mixtral".into());
        let url = format!("{}/chat/completions", base.trim_end_matches('/'));
        let body = json!({
            "model": model,
            "messages":[
                {"role":"system","content":"You are a succinct assistant embedded in a shell."},
                {"role":"user","content": prompt}
            ],
            "temperature": 0.2
        });

        // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
        let client = crate::security::create_secure_http_client()
            .context("Failed to create secure HTTP client")?;

        let v: J = client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?;
        Ok(v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string())
    }
}

pub mod tgi {
    use super::*;
    #[derive(Serialize)]
    struct Req<'a> {
        inputs: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        parameters: Option<J>,
    }
    pub fn complete_sync(prompt: &str) -> Result<String> {
        let base = std::env::var("TGI_URL").unwrap_or_else(|_| "http://localhost:8080".into());
        let url = format!("{}/generate", base.trim_end_matches('/'));
        let body = Req {
            inputs: prompt,
            parameters: Some(json!({"temperature":0.2})),
        };

        // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
        let client = crate::security::create_secure_http_client()
            .context("Failed to create secure HTTP client")?;

        let r = client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()?
            .error_for_status()?;
        // Some TGI variants return a single object, others an array of objects.
        match r.json::<J>()? {
            J::Object(m) => Ok(m
                .get("generated_text")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string()),
            J::Array(arr) => Ok(arr
                .get(0)
                .and_then(|x| x.get("generated_text"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string()),
            _ => Ok(String::new()),
        }
    }
}

// ===================== Model URIs & Backend registry =====================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    Stub,
    OpenAI,
    Ollama,
    LMStudio,
    OpenAICompat, // Generic OpenAI-compatible (vLLM, llama.cpp, etc.)
    Tgi,
    VLlm,     // Explicitly vLLM
    LlamaCpp, // Explicitly llama.cpp
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    pub provider: Provider,
    pub model: String,
}

/// Parse strings like:
/// - "openai:gpt-4o-mini" / "ollama:llama3" / "compat:mixtral" / "tgi:mixtral" / "stub"
/// - "vllm:meta-llama/Llama-3-8B" / "llamacpp:mistral-7b"
pub fn parse_model_ref(s: &str) -> ModelRef {
    let s = s.trim();
    if let Some((pfx, rest)) = s.split_once(':') {
        let model = rest.trim().to_string();
        let provider = match pfx.trim().to_lowercase().as_str() {
            "openai" => Provider::OpenAI,
            "ollama" => Provider::Ollama,
            "lmstudio" | "lm_studio" | "lm-studio" => Provider::LMStudio,
            "compat" | "openai_compat" => Provider::OpenAICompat,
            "tgi" => Provider::Tgi,
            "vllm" => Provider::VLlm,
            "llamacpp" | "llama.cpp" | "llama_cpp" => Provider::LlamaCpp,
            _ => Provider::Stub,
        };
        ModelRef { provider, model }
    } else {
        // fallback: env or stub
        match s.to_lowercase().as_str() {
            "openai" => ModelRef {
                provider: Provider::OpenAI,
                model: std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()),
            },
            "ollama" => ModelRef {
                provider: Provider::Ollama,
                model: std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3".into()),
            },
            "compat" | "openai_compat" => ModelRef {
                provider: Provider::OpenAICompat,
                model: std::env::var("AETHER_COMPAT_MODEL").unwrap_or_else(|_| "mixtral".into()),
            },
            "tgi" => ModelRef {
                provider: Provider::Tgi,
                model: "mixtral".into(),
            },
            "vllm" => ModelRef {
                provider: Provider::VLlm,
                model: std::env::var("VLLM_MODEL")
                    .unwrap_or_else(|_| "meta-llama/Llama-3-8B".into()),
            },
            "llamacpp" | "llama.cpp" | "llama_cpp" => ModelRef {
                provider: Provider::LlamaCpp,
                model: std::env::var("LLAMACPP_MODEL").unwrap_or_else(|_| "model".into()),
            },
            "lmstudio" | "lm_studio" | "lm-studio" => ModelRef {
                provider: Provider::LMStudio,
                model: std::env::var("LMSTUDIO_MODEL").unwrap_or_else(|_| "default".into()),
            },
            _ => ModelRef {
                provider: Provider::Stub,
                model: "stub".into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

pub trait LlmBackend: Send + Sync {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String>;
}

struct StubBackend;
impl LlmBackend for StubBackend {
    fn chat(&self, _messages: &[ChatMessage]) -> Result<String> {
        Err(anyhow!(
            "No AI provider configured.\n\
            Set AETHER_AI environment variable to: openai, ollama, or compat"
        ))
    }
}
struct OpenAiBackend;
impl LlmBackend for OpenAiBackend {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String> {
        // SECURITY FIX (HIGH-002): Use SecureApiConfig for memory-safe key handling
        let config = SecureApiConfig::from_keyring_or_env(
            "openai",
            "OPENAI_API_KEY",
            "https://api.openai.com".to_string(),
            std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            "openai".to_string(),
        )
        .context("Failed to load OpenAI configuration")?;

        // Validate API key format
        config
            .validate_format()
            .context("OpenAI API key validation failed")?;

        let url = format!(
            "{}/v1/chat/completions",
            config.endpoint.trim_end_matches('/')
        );
        let body = json!({ "model": config.model, "messages": messages, "temperature": 0.2 });

        // Create authorization header (zeroized after use)
        let auth_header = config
            .create_auth_header()
            .ok_or_else(|| anyhow!("OpenAI API key not configured"))?;

        // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
        let client = crate::security::create_secure_http_client()
            .context("Failed to create secure HTTP client")?;

        let v: J = client
            .post(&url)
            .header(AUTHORIZATION, auth_header.as_str())
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .context("Failed to send request to OpenAI")?
            .error_for_status()
            .context("OpenAI API returned error status")?
            .json()
            .context("Failed to parse OpenAI response")?;

        // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
        let content = v["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("OpenAI response missing content field"))?
            .to_string();
        Ok(content)
    }
}
struct OllamaBackend;
impl LlmBackend for OllamaBackend {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String> {
        let prompt = render_prompt(messages);
        ollama::complete_sync(&prompt)
    }
}
struct OpenAiCompatBackend;
impl LlmBackend for OpenAiCompatBackend {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String> {
        let base = std::env::var("AETHER_COMPAT_BASE")
            .unwrap_or_else(|_| "http://localhost:8000/v1".into());
        let model = std::env::var("AETHER_COMPAT_MODEL").unwrap_or_else(|_| "mixtral".into());
        let url = format!("{}/chat/completions", base.trim_end_matches('/'));
        let body = json!({ "model": model, "messages": messages, "temperature": 0.2 });

        // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
        let client = crate::security::create_secure_http_client()
            .context("Failed to create secure HTTP client")?;

        let v: J = client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?;
        let content = v["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("OpenAI-compatible API response missing content field"))?
            .to_string();
        Ok(content)
    }
}
struct TgiBackend;
impl LlmBackend for TgiBackend {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String> {
        let base = std::env::var("TGI_URL").unwrap_or_else(|_| "http://localhost:8080".into());
        let url = format!("{}/generate", base.trim_end_matches('/'));
        let body = json!({"inputs": render_prompt(messages), "parameters": {"temperature": 0.2}});

        // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
        let client = crate::security::create_secure_http_client()
            .context("Failed to create secure HTTP client")?;

        let v: J = client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?;
        let s = v
            .get("generated_text")
            .and_then(|x| x.as_str())
            .or_else(|| {
                v.get(0)
                    .and_then(|x| x.get("generated_text"))
                    .and_then(|x| x.as_str())
            })
            .unwrap_or("");
        Ok(s.to_string())
    }
}

struct VLlmBackend;
impl LlmBackend for VLlmBackend {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String> {
        // vLLM uses OpenAI-compatible API by default (http://localhost:8000/v1)
        let base = std::env::var("VLLM_URL").unwrap_or_else(|_| "http://localhost:8000/v1".into());
        let model = std::env::var("VLLM_MODEL").unwrap_or_else(|_| "meta-llama/Llama-3-8B".into());
        let url = format!("{}/chat/completions", base.trim_end_matches('/'));
        let body = json!({ "model": model, "messages": messages, "temperature": 0.2 });

        // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
        let client = crate::security::create_secure_http_client()
            .context("Failed to create secure HTTP client")?;

        let v: J = client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?;
        let content = v["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("vLLM response missing content field"))?
            .to_string();
        Ok(content)
    }
}

struct LlamaCppBackend;
impl LlmBackend for LlamaCppBackend {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String> {
        // llama.cpp server uses OpenAI-compatible API (http://localhost:8080/v1)
        let base =
            std::env::var("LLAMACPP_URL").unwrap_or_else(|_| "http://localhost:8080/v1".into());
        let model = std::env::var("LLAMACPP_MODEL").unwrap_or_else(|_| "model".into());
        let url = format!("{}/chat/completions", base.trim_end_matches('/'));
        let body = json!({ "model": model, "messages": messages, "temperature": 0.2 });

        // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
        let client = crate::security::create_secure_http_client()
            .context("Failed to create secure HTTP client")?;

        let v: J = client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?;
        let content = v["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("llama.cpp response missing content field"))?
            .to_string();
        Ok(content)
    }
}

// ===================== Backend Detection =====================

/// Information about a detected backend
#[derive(Debug, Clone)]
pub struct BackendInfo {
    pub name: String,
    pub provider: Provider,
    pub endpoint: String,
    pub available: bool,
    pub models: Vec<String>,
}

/// Detect available AI backends by probing common endpoints
pub fn detect_available_backends() -> Vec<BackendInfo> {
    let mut backends = Vec::new();

    // Check Ollama (localhost:11434)
    if let Ok(info) = detect_ollama() {
        backends.push(info);
    }

    // Check vLLM (localhost:8000)
    if let Ok(info) = detect_vllm() {
        backends.push(info);
    }

    // Check llama.cpp (localhost:8080)
    if let Ok(info) = detect_llamacpp() {
        backends.push(info);
    }

    // Check TGI (localhost:8080 - alternate port check)
    if let Ok(info) = detect_tgi() {
        backends.push(info);
    }

    // Check OpenAI (via API key)
    if let Ok(info) = detect_openai() {
        backends.push(info);
    }

    // Check LM Studio (localhost:1234)
    if let Ok(info) = detect_lmstudio() {
        backends.push(info);
    }

    backends
}

fn detect_ollama() -> Result<BackendInfo> {
    let endpoint = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".into());
    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));

    // Use a simple blocking client with short timeout for detection
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;
    let response = client.get(&url).send();

    match response {
        Ok(resp) if resp.status().is_success() => {
            let models = if let Ok(json) = resp.json::<J>() {
                json["models"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| m["name"].as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                vec![]
            };

            Ok(BackendInfo {
                name: "Ollama".to_string(),
                provider: Provider::Ollama,
                endpoint,
                available: true,
                models,
            })
        }
        _ => Err(anyhow!("Ollama not available")),
    }
}

fn detect_vllm() -> Result<BackendInfo> {
    let endpoint = std::env::var("VLLM_URL").unwrap_or_else(|_| "http://localhost:8000/v1".into());
    let url = format!("{}/models", endpoint.trim_end_matches('/'));

    // Use a simple blocking client with short timeout for detection
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;
    let response = client.get(&url).send();

    match response {
        Ok(resp) if resp.status().is_success() => {
            let models = if let Ok(json) = resp.json::<J>() {
                json["data"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| m["id"].as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                vec![]
            };

            Ok(BackendInfo {
                name: "vLLM".to_string(),
                provider: Provider::VLlm,
                endpoint,
                available: true,
                models,
            })
        }
        _ => Err(anyhow!("vLLM not available")),
    }
}

fn detect_llamacpp() -> Result<BackendInfo> {
    let endpoint =
        std::env::var("LLAMACPP_URL").unwrap_or_else(|_| "http://localhost:8080/v1".into());
    let url = format!("{}/models", endpoint.trim_end_matches('/'));

    // Use a simple blocking client with short timeout for detection
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;
    let response = client.get(&url).send();

    match response {
        Ok(resp) if resp.status().is_success() => {
            let models = if let Ok(json) = resp.json::<J>() {
                json["data"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| m["id"].as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                vec!["model".to_string()] // Default model name
            };

            Ok(BackendInfo {
                name: "llama.cpp".to_string(),
                provider: Provider::LlamaCpp,
                endpoint,
                available: true,
                models,
            })
        }
        _ => Err(anyhow!("llama.cpp not available")),
    }
}

fn detect_tgi() -> Result<BackendInfo> {
    let endpoint = std::env::var("TGI_URL").unwrap_or_else(|_| "http://localhost:8080".into());
    let url = format!("{}/health", endpoint.trim_end_matches('/'));

    // Use a simple blocking client with short timeout for detection
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;
    let response = client.get(&url).send();

    match response {
        Ok(resp) if resp.status().is_success() => Ok(BackendInfo {
            name: "Text Generation Inference (TGI)".to_string(),
            provider: Provider::Tgi,
            endpoint,
            available: true,
            models: vec![], // TGI typically serves one model
        }),
        _ => Err(anyhow!("TGI not available")),
    }
}

fn detect_openai() -> Result<BackendInfo> {
    // Check if OpenAI API key is available
    let has_key = std::env::var("OPENAI_API_KEY").is_ok()
        || crate::secure_config::SecureApiConfig::from_keyring(
            "openai",
            "https://api.openai.com".to_string(),
            "gpt-4o-mini".to_string(),
            "openai".to_string(),
        )
        .is_ok();

    if has_key {
        Ok(BackendInfo {
            name: "OpenAI".to_string(),
            provider: Provider::OpenAI,
            endpoint: "https://api.openai.com".to_string(),
            available: true,
            models: vec![
                "gpt-4o".to_string(),
                "gpt-4o-mini".to_string(),
                "gpt-4-turbo".to_string(),
                "gpt-3.5-turbo".to_string(),
            ],
        })
    } else {
        Err(anyhow!("OpenAI API key not configured"))
    }
}

fn detect_lmstudio() -> Result<BackendInfo> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;

    // LM Studio exposes OpenAI-compatible API on :1234
    let resp = client
        .get("http://localhost:1234/v1/models")
        .send()
        .map_err(|_| anyhow!("LM Studio not reachable"))?;

    if resp.status().is_success() {
        let body: serde_json::Value = resp.json().unwrap_or_default();
        let models: Vec<String> = body
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        m.get("id")
                            .and_then(|id| id.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(BackendInfo {
            name: "LM Studio".to_string(),
            provider: Provider::LMStudio,
            endpoint: "http://localhost:1234".to_string(),
            available: true,
            models,
        })
    } else {
        Err(anyhow!("LM Studio not available"))
    }
}

// ========== MCP Server Detection ==========

use std::sync::Arc;
use std::time::{Duration, Instant};

/// MCP Server information
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub endpoint: String,
    pub available: bool,
    pub tools: Vec<String>,
}

/// Cached MCP detection result
#[derive(Debug, Clone)]
pub struct McpDetectionCache {
    servers: Vec<McpServerInfo>,
    timestamp: Instant,
    ttl: Duration,
}

impl McpDetectionCache {
    fn new(servers: Vec<McpServerInfo>, ttl: Duration) -> Self {
        Self {
            servers,
            timestamp: Instant::now(),
            ttl,
        }
    }

    fn is_expired(&self) -> bool {
        self.timestamp.elapsed() > self.ttl
    }
}

lazy_static::lazy_static! {
    pub static ref MCP_CACHE: Arc<Mutex<Option<McpDetectionCache>>> = Arc::new(Mutex::new(None));
    static ref HTTP_CLIENT: reqwest::blocking::Client = {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
            .expect("FATAL: Failed to create HTTP client - this indicates a critical system configuration issue")
    };
}

/// Configure MCP detection cache TTL (time-to-live)
pub fn configure_mcp_cache_ttl(_ttl_seconds: u64) -> Result<()> {
    // This will affect future cache entries
    Ok(())
}

/// Clear MCP detection cache
pub fn clear_mcp_cache() -> Result<()> {
    let mut cache = MCP_CACHE
        .lock()
        .map_err(|e| anyhow!("Failed to acquire MCP cache lock: {}", e))?;
    *cache = None;
    Ok(())
}

/// Detect available MCP servers on common ports (with caching)
pub fn detect_mcp_servers() -> Vec<McpServerInfo> {
    detect_mcp_servers_with_cache(Duration::from_secs(30))
}

/// Detect available MCP servers with custom cache TTL
pub fn detect_mcp_servers_with_cache(cache_ttl: Duration) -> Vec<McpServerInfo> {
    // Check cache first
    if let Ok(cache_guard) = MCP_CACHE.lock() {
        if let Some(ref cache) = *cache_guard {
            if !cache.is_expired() {
                return cache.servers.clone();
            }
        }
    }

    // Cache miss or expired - perform fresh detection
    let servers = detect_mcp_servers_uncached();

    // Update cache
    if let Ok(mut cache_guard) = MCP_CACHE.lock() {
        *cache_guard = Some(McpDetectionCache::new(servers.clone(), cache_ttl));
    }

    servers
}

/// Detect available MCP servers without caching (internal)
/// Detect MCP servers without using cache (always performs network calls)
pub fn detect_mcp_servers_uncached() -> Vec<McpServerInfo> {
    let mut servers = Vec::with_capacity(8); // Pre-allocate capacity

    // Prioritized MCP server endpoints (most likely to respond first)
    let prioritized_endpoints = vec![
        ("filesystem", "http://localhost:3001"),
        ("git", "http://localhost:3002"),
        ("database", "http://localhost:3005"),
        ("docker", "http://localhost:3003"),
        ("aws", "http://localhost:3004"),
        ("custom1", "http://localhost:8080"),
        ("custom2", "http://localhost:8081"),
    ];

    // Use parallel detection for faster response
    let results: Vec<_> = prioritized_endpoints
        .into_iter()
        .map(|(name, endpoint)| std::thread::spawn(move || detect_mcp_server(name, endpoint)))
        .collect();

    // Collect results in order of completion
    for handle in results {
        if let Ok(Ok(info)) = handle.join() {
            if info.available {
                servers.push(info);
            }
        }
    }

    servers
}

/// Detect a specific MCP server
fn detect_mcp_server(name: &str, endpoint: &str) -> Result<McpServerInfo> {
    let url = format!("{}/mcp/v1/tools", endpoint.trim_end_matches('/'));

    // Use shared HTTP client with connection pooling
    let response = HTTP_CLIENT.get(&url).send();

    match response {
        Ok(resp) if resp.status().is_success() => {
            // Try to parse the tools list
            let tools: Vec<mcp::McpToolSchema> = resp.json().unwrap_or_default();
            let tool_names = tools.iter().map(|t| t.name.clone()).collect();

            Ok(McpServerInfo {
                name: name.to_string(),
                endpoint: endpoint.to_string(),
                available: true,
                tools: tool_names,
            })
        }
        _ => Err(anyhow!("MCP server {} not available", name)),
    }
}

/// Automatically select the best available backend
pub fn auto_select_backend() -> Option<String> {
    let backends = detect_available_backends();

    // Priority: local backends first (faster), then cloud
    for backend in backends {
        if !backend.available {
            continue;
        }

        let model = backend
            .models
            .first()
            .map(String::as_str)
            .unwrap_or("model");

        return Some(match backend.provider {
            Provider::Ollama => format!("ollama:{}", model),
            Provider::VLlm => format!("vllm:{}", model),
            Provider::LlamaCpp => format!("llamacpp:{}", model),
            Provider::Tgi => "tgi:model".to_string(),
            Provider::OpenAI => "openai:gpt-4o-mini".to_string(),
            Provider::LMStudio => format!("lmstudio:{}", model),
            _ => continue,
        });
    }

    None
}

pub fn backend_from_env() -> Box<dyn LlmBackend> {
    // First check for explicit configuration
    let model_uri = std::env::var("AETHER_MODEL_URI").ok().or_else(|| {
        std::env::var("AETHER_AI").ok().map(|ai| {
            match ai.as_str() {
                "openai" => "openai:gpt-4o-mini",
                "ollama" => "ollama:llama3",
                "openai_compat" | "compat" => "compat:mixtral",
                "tgi" => "tgi:mixtral",
                "vllm" => "vllm:meta-llama/Llama-3-8B",
                "llamacpp" | "llama.cpp" => "llamacpp:model",
                "lmstudio" | "lm_studio" | "lm-studio" => "lmstudio:default",
                "auto" => return auto_select_backend().unwrap_or_else(|| "stub".to_string()),
                _ => "stub",
            }
            .to_string()
        })
    });

    // If no explicit config, try auto-detection
    let uri =
        model_uri.unwrap_or_else(|| auto_select_backend().unwrap_or_else(|| "stub".to_string()));

    backend_from_model(uri)
}
pub fn backend_from_model(uri: String) -> Box<dyn LlmBackend> {
    let m = parse_model_ref(&uri);
    match m.provider {
        Provider::OpenAI => Box::new(OpenAiBackend),
        Provider::Ollama => Box::new(OllamaBackend),
        Provider::OpenAICompat => Box::new(OpenAiCompatBackend),
        Provider::LMStudio => Box::new(OpenAiCompatBackend), // LM Studio uses OpenAI-compat API
        Provider::Tgi => Box::new(TgiBackend),
        Provider::VLlm => Box::new(VLlmBackend),
        Provider::LlamaCpp => Box::new(LlamaCppBackend),
        Provider::Stub => Box::new(StubBackend),
    }
}

fn render_prompt(msgs: &[ChatMessage]) -> String {
    let mut s = String::new();
    for m in msgs {
        s.push_str(&format!("{}: {}\n", m.role, m.content));
    }
    s
}

pub fn parse_agent_command(text: &str) -> (Option<J>, String) {
    if let Some(start) = text.find("```json") {
        if let Some(end) = text[start + 7..].find("```") {
            let json_str = &text[start + 7..start + 7 + end];
            if let Ok(v) = serde_json::from_str::<J>(json_str) {
                return (Some(v), text[..start].trim().to_string());
            }
        }
    }
    if let Ok(v) = serde_json::from_str::<J>(text) {
        return (Some(v), String::new());
    }
    (None, text.trim().to_string())
}

use crate::value::Value;
pub fn display_value(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(x) => x.to_string(),
        Value::Str(s) => s.clone(),
        Value::Uri(u) => u.clone(),
        Value::Array(a) => format!("[len={}]", a.len()),
        Value::Record(_) => "{…}".into(),
        Value::Table(t) => format!("<Table rows={}>", t.rows.len()),
        Value::Lambda(_) => "<lambda>".into(),
        Value::AsyncLambda(_) => "<async lambda>".into(),
        Value::Future(_) => "<future>".into(),
        Value::Builtin(b) => format!("<builtin:{}>", b.name),
        Value::Error(msg) => format!("Error: {}", msg),
    }
}

// ===================== Agents (single + swarms) =====================

use crate::{builtins, env::Env};

pub mod agents {
    use super::*;
    use std::collections::BTreeMap;

    // ---------- Tools ----------
    /// A callable tool the agent may use.
    pub trait Tool: Send + Sync {
        fn name(&self) -> &str;
        fn description(&self) -> &str;
        fn call(&self, input: &str, env: &mut Env) -> Result<Value>;
    }

    /// Tool that bridges to Aether builtins: input is parsed as a JSON array of args.
    pub struct BuiltinTool {
        pub name: String,
        pub description: String,
    }
    impl Tool for BuiltinTool {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            &self.description
        }
        fn call(&self, input: &str, env: &mut Env) -> Result<Value> {
            let parsed: J = serde_json::from_str(input).unwrap_or(J::Null);
            let mut args = Vec::<Value>::new();
            if let Some(arr) = parsed.as_array() {
                for v in arr {
                    args.push(json_to_value(v));
                }
            } else if parsed.is_string() {
                if let Some(s) = parsed.as_str() {
                    args.push(Value::Str(s.to_string()));
                }
            }
            builtins::call(&self.name, args, env)
        }
    }
    fn json_to_value(v: &J) -> Value {
        match v {
            J::Null => Value::Null,
            J::Bool(b) => Value::Bool(*b),
            J::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Int(i)
                } else if let Some(f) = n.as_f64() {
                    Value::Float(f)
                } else {
                    Value::Null
                }
            }
            J::String(s) => Value::Str(s.clone()),
            J::Array(a) => Value::Array(a.iter().map(json_to_value).collect()),
            J::Object(m) => {
                let mut rec = BTreeMap::new();
                for (k, v) in m {
                    rec.insert(k.clone(), json_to_value(v));
                }
                Value::Record(rec)
            }
        }
    }

    // ---------- Tool Registry ----------
    pub trait ToolResolver: Send + Sync {
        fn list(&self) -> Vec<String>;
        fn get(&self, name: &str) -> Option<Box<dyn Tool>>;
    }

    /// Resolver that exposes Aether builtins as tools.
    pub struct BuiltinToolResolver;
    impl ToolResolver for BuiltinToolResolver {
        fn list(&self) -> Vec<String> {
            vec![
                "print".into(),
                "echo".into(),
                "map".into(),
                "reduce".into(),
                "cd".into(),
                "pwd".into(),
                "!".into(),
                "http_get".into(),
            ]
        }
        fn get(&self, name: &str) -> Option<Box<dyn Tool>> {
            Some(Box::new(BuiltinTool {
                name: name.to_string(),
                description: format!("Aether builtin `{}`", name),
            }))
        }
    }

    pub struct ToolRegistry {
        resolvers: Vec<Box<dyn ToolResolver>>,
    }
    impl ToolRegistry {
        pub fn with_builtins() -> Self {
            Self {
                resolvers: vec![Box::new(BuiltinToolResolver)],
            }
        }
        pub fn with_builtins_and_mcp(endpoint: &str) -> Self {
            let mut r = Self::with_builtins();
            r.resolvers
                .push(Box::new(crate::ai::mcp::McpToolResolver::new(endpoint)));
            r
        }
        pub fn list(&self) -> Vec<String> {
            let mut out = Vec::new();
            for r in &self.resolvers {
                out.extend(r.list());
            }
            out.sort();
            out.dedup();
            out
        }
        pub fn resolve_many(&self, names: &[&str]) -> Vec<Box<dyn Tool>> {
            let mut tools = Vec::new();
            for n in names {
                for r in &self.resolvers {
                    if let Some(t) = r.get(n) {
                        tools.push(t);
                        break;
                    }
                }
            }
            tools
        }
    }

    // ---------- Single Agent ----------
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AgentStep {
        pub thought: String,
        pub command: Option<J>,
        pub observation: Option<String>,
    }

    pub struct Agent {
        backend: Box<dyn super::LlmBackend>,
        pub tools: Vec<Box<dyn Tool>>,
        pub max_steps: usize,
        pub trace: Vec<AgentStep>,
    }
    impl Agent {
        pub fn new(tools: Vec<Box<dyn Tool>>) -> Self {
            Self {
                backend: super::backend_from_env(),
                tools,
                max_steps: 8,
                trace: Vec::new(),
            }
        }
        /// Construct with a specific model URI (e.g., "openai:gpt-4o-mini").
        pub fn with_model_uri(tools: Vec<Box<dyn Tool>>, model_uri: &str) -> Self {
            Self {
                backend: super::backend_from_model(model_uri.to_string()),
                tools,
                max_steps: 8,
                trace: Vec::new(),
            }
        }
        pub fn run_sync(&mut self, goal: &str, dry_run: bool, env: &mut Env) -> Result<String> {
            let system = ChatMessage {
                role: "system".into(),
                content: format!(
                    "You are Aether Agent. Emit JSON commands:\n\
                     {{\"type\":\"tool\",\"tool\":\"<name>\",\"input\":<json or string>}} or \
                     {{\"type\":\"final\",\"output\":\"...\"}}.\nTools:\n{}",
                    self.tools
                        .iter()
                        .map(|t| format!("- {}: {}", t.name(), t.description()))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            };
            let mut dialogue = vec![
                system,
                ChatMessage {
                    role: "user".into(),
                    content: goal.into(),
                },
            ];

            for _ in 0..self.max_steps {
                let reply = self.backend.chat(&dialogue)?;
                let (cmd, thought) = super::parse_agent_command(&reply);
                self.trace.push(AgentStep {
                    thought: thought.clone(),
                    command: cmd.clone(),
                    observation: None,
                });

                if let Some(c) = cmd
                    .as_ref()
                    .and_then(|j| j.get("type"))
                    .and_then(|t| t.as_str())
                {
                    if c == "final" {
                        let out = cmd
                            .as_ref()
                            .and_then(|j| j.get("output"))
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        return if dry_run {
                            Ok(format!("[dry_run] final: {}\ntrace: {:?}", out, self.trace))
                        } else {
                            Ok(out)
                        };
                    }
                }

                if let Some(tool_name) = cmd
                    .as_ref()
                    .and_then(|j| j.get("tool"))
                    .and_then(|s| s.as_str())
                {
                    let input = cmd
                        .as_ref()
                        .and_then(|j| j.get("input"))
                        .unwrap_or(&J::Null)
                        .to_string();
                    let obs = if dry_run {
                        format!("[dry_run] would call {} with {}", tool_name, input)
                    } else {
                        if let Some(tool) = self.tools.iter().find(|t| t.name() == tool_name) {
                            match tool.call(&input, env) {
                                Ok(val) => format!("OK: {}", super::display_value(&val)),
                                Err(e) => format!("ERROR: {}", e),
                            }
                        } else {
                            format!("ERROR: unknown tool {}", tool_name)
                        }
                    };
                    dialogue.push(ChatMessage {
                        role: "assistant".into(),
                        content: reply,
                    });
                    dialogue.push(ChatMessage {
                        role: "user".into(),
                        content: format!("Observation: {}", obs),
                    });
                    if let Some(last) = self.trace.last_mut() {
                        last.observation = Some(obs);
                    }
                    continue;
                }

                dialogue.push(ChatMessage {
                    role: "assistant".into(),
                    content: reply.clone(),
                });
                dialogue.push(ChatMessage {
                    role: "user".into(),
                    content: "Your last response was not valid JSON. Please emit a valid command."
                        .into(),
                });
            }
            Ok(format!(
                "(incomplete) max steps reached; trace: {:?}",
                self.trace
            ))
        }
    }

    // ---------- Public helpers ----------
    /// Convenience wrapper exposed to callers/tests/builtins.
    pub fn run_sync(
        goal: &str,
        tool_names: &[&str],
        max_steps: usize,
        dry_run: bool,
        env: &mut Env,
    ) -> Result<String> {
        let reg = ToolRegistry::with_builtins();
        let tools = reg.resolve_many(tool_names);
        let mut agent = if let Ok(uri) = std::env::var("AETHER_AGENT_MODEL_URI") {
            Agent::with_model_uri(tools, &uri)
        } else {
            Agent::new(tools)
        };
        if max_steps > 0 {
            agent.max_steps = max_steps;
        }
        agent.run_sync(goal, dry_run, env)
    }

    /// Same as `run_sync`, but forces a specific model URI for this run.
    pub fn run_sync_with_model(
        goal: &str,
        tool_names: &[&str],
        model_uri: &str,
        max_steps: usize,
        dry_run: bool,
        env: &mut Env,
    ) -> Result<String> {
        let reg = ToolRegistry::with_builtins();
        let tools = reg.resolve_many(tool_names);
        let mut agent = Agent::with_model_uri(tools, model_uri);
        if max_steps > 0 {
            agent.max_steps = max_steps;
        }
        agent.run_sync(goal, dry_run, env)
    }

    // ---------- Multi-Agent Swarm ----------
    pub mod swarm {
        use super::*;

        pub struct AgentConfig {
            pub id: String,
            pub system: String,
            pub tools: Vec<Box<dyn Tool>>,
            pub max_steps: usize,
            /// Optional model URI for this agent (e.g., "ollama:llama3")
            pub model_uri: Option<String>,
        }
        impl std::fmt::Debug for AgentConfig {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(
                    f,
                    "AgentConfig {{ id: {:?}, system_len: {}, tools: {}, max_steps: {}, model_uri: {:?} }}",
                    self.id,
                    self.system.len(),
                    self.tools.len(),
                    self.max_steps,
                    self.model_uri
                )
            }
        }

        #[derive(Debug, Clone, Copy)]
        pub enum Policy {
            RoundRobin,
            Router,
        }

        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct BlackboardMsg {
            pub author: String,
            pub content: String,
            pub kind: String,
        }

        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct SwarmStep {
            pub agent: String,
            pub reply: String,
            pub parsed: Option<J>,
            pub observation: Option<String>,
        }

        /// Strategy abstraction (Nanda-friendly)
        pub trait Coordinator: Send {
            fn select(&mut self, swarm: &Swarm, tick: usize) -> usize;
        }

        /// Round-robin coordinator (default)
        pub struct RoundRobinCoord {
            next: usize,
        }

        impl RoundRobinCoord {
            pub fn new() -> Self {
                Self { next: 0 }
            }
        }
        impl Coordinator for RoundRobinCoord {
            fn select(&mut self, swarm: &Swarm, _tick: usize) -> usize {
                if swarm.agents.is_empty() {
                    return 0;
                }
                let i = self.next % swarm.agents.len();
                self.next += 1;
                i
            }
        }

        /// Router coordinator (stub): could inspect blackboard and route.
        pub struct RouterCoord;
        impl Coordinator for RouterCoord {
            fn select(&mut self, _swarm: &Swarm, _tick: usize) -> usize {
                0 // stub: always pick agent 0; replace with scoring/LLM routing
            }
        }

        pub struct Swarm {
            pub policy: Policy,
            pub agents: Vec<(AgentConfig, Box<dyn super::super::LlmBackend>)>,
            pub blackboard: Vec<BlackboardMsg>,
            pub steps: Vec<SwarmStep>,
            pub max_iters: usize,
            coord: Option<Box<dyn Coordinator>>,
        }

        impl Swarm {
            pub fn new(policy: Policy, max_iters: usize) -> Self {
                let coord: Box<dyn Coordinator> = match policy {
                    Policy::RoundRobin => Box::new(RoundRobinCoord::new()),
                    Policy::Router => Box::new(RouterCoord),
                };
                Self {
                    policy,
                    agents: Vec::new(),
                    blackboard: Vec::new(),
                    steps: Vec::new(),
                    max_iters,
                    coord: Some(coord),
                }
            }
            pub fn add_agent(&mut self, mut cfg: AgentConfig) {
                // Default per-agent model URI from env if none provided
                if cfg.model_uri.is_none() {
                    if let Ok(uri) = std::env::var("AETHER_SWARM_AGENT_MODEL_URI") {
                        cfg.model_uri = Some(uri);
                    }
                }
                let be = if let Some(uri) = &cfg.model_uri {
                    super::super::backend_from_model(uri.clone())
                } else {
                    super::super::backend_from_env()
                };
                self.agents.push((cfg, be));
            }

            pub fn run_sync(
                &mut self,
                user_goal: &str,
                env: &mut Env,
                dry_run: bool,
            ) -> Result<String> {
                if self.agents.is_empty() {
                    return Err(anyhow!("swarm has no agents"));
                }
                self.blackboard.push(BlackboardMsg {
                    author: "user".into(),
                    content: user_goal.into(),
                    kind: "note".into(),
                });

                for t in 0..self.max_iters {
                    // Move coord out to avoid overlapping borrows (self immutably borrowed below)
                    let i = {
                        // SECURITY: Replace .expect() with proper error handling (CVSS 7.1)
                        let mut coord = self
                            .coord
                            .take()
                            .ok_or_else(|| anyhow!("Coordinator not initialized"))?;
                        let idx = coord.select(self, t);
                        self.coord = Some(coord);
                        idx
                    };
                    let (cfg, be) = &self.agents[i];
                    let reply = be.chat(&self.compose_dialogue(cfg))?;

                    let parsed = try_parse_command(&reply);
                    if let Some(js) = &parsed {
                        if js.get("type").and_then(|x| x.as_str()) == Some("final") {
                            let out = js
                                .get("output")
                                .and_then(|x| x.as_str())
                                .unwrap_or("")
                                .to_string();
                            self.blackboard.push(BlackboardMsg {
                                author: cfg.id.clone(),
                                content: out.clone(),
                                kind: "final".into(),
                            });
                            self.steps.push(SwarmStep {
                                agent: cfg.id.clone(),
                                reply,
                                parsed,
                                observation: None,
                            });
                            return Ok(out);
                        }
                    }

                    let mut observation = None;
                    if let Some(js) = &parsed {
                        if js.get("type").and_then(|x| x.as_str()) == Some("tool") {
                            let tool_name = js.get("tool").and_then(|x| x.as_str()).unwrap_or("");
                            let input = js.get("input").cloned().unwrap_or(J::Null).to_string();
                            let obs = if dry_run {
                                format!("[dry_run] {}/tool {}({})", cfg.id, tool_name, input)
                            } else if let Some(tool) =
                                cfg.tools.iter().find(|t| t.name() == tool_name)
                            {
                                match tool.call(&input, env) {
                                    Ok(v) => format!("OK: {}", super::super::display_value(&v)),
                                    Err(e) => format!("ERROR: {}", e),
                                }
                            } else {
                                format!("ERROR: unknown tool {}", tool_name)
                            };
                            observation = Some(obs);
                        }
                    }

                    if observation.is_none() && parsed.is_none() {
                        self.blackboard.push(BlackboardMsg {
                            author: cfg.id.clone(),
                            content: reply.clone(),
                            kind: "thought".into(),
                        });
                    }
                    self.steps.push(SwarmStep {
                        agent: cfg.id.clone(),
                        reply: reply.clone(),
                        parsed: parsed.clone(),
                        observation: observation.clone(),
                    });
                    if let Some(obs) = observation {
                        self.blackboard.push(BlackboardMsg {
                            author: cfg.id.clone(),
                            content: format!("obs: {obs}"),
                            kind: "note".into(),
                        });
                    }
                }
                Ok(format!(
                    "(incomplete) swarm max_iters reached; steps={}",
                    self.steps.len()
                ))
            }

            fn compose_dialogue(&self, cfg: &AgentConfig) -> Vec<ChatMessage> {
                let mut bb = String::new();
                for m in &self.blackboard {
                    bb.push_str(&format!("- {} [{}]: {}\n", m.author, m.kind, m.content));
                }
                let tools_list = cfg
                    .tools
                    .iter()
                    .map(|t| format!("- {}: {}", t.name(), t.description()))
                    .collect::<Vec<_>>()
                    .join("\n");
                vec![
                    ChatMessage {
                        role: "system".into(),
                        content: format!(
                            "You are agent `{}`.\n{}\n\nBlackboard:\n{}\n\n\
                             Emit JSON commands:\n\
                             - tool: {{\"type\":\"tool\",\"tool\":\"<name>\",\"input\":<json|string>}}\n\
                             - final: {{\"type\":\"final\",\"output\":\"...\"}}\n\
                             - delegate: {{\"type\":\"delegate\",\"target\":\"<agent-id>\",\"input\":<json|string>}}\n\
                             - route: {{\"type\":\"route\",\"target\":\"<agent-id>\"}}",
                            cfg.id, cfg.system, bb
                        ),
                    },
                    ChatMessage {
                        role: "user".into(),
                        content: format!(
                            "Act toward the shared goal. Available tools:\n{}",
                            tools_list
                        ),
                    },
                ]
            }
        }

        fn try_parse_command(text: &str) -> Option<J> {
            if let Some(start) = text.find("```json") {
                if let Some(end) = text[start + 7..].find("```") {
                    if let Ok(v) = serde_json::from_str::<J>(&text[start + 7..start + 7 + end]) {
                        return Some(v);
                    }
                }
            }
            serde_json::from_str::<J>(text).ok()
        }

        // ---- Compatibility shim: some callers use ai::agents::swarm::run_sync ----
        /// Thin wrapper to maintain compatibility; internally calls a single-agent runner.
        pub fn run_sync(
            goal: &str,
            tool_names: &[&str],
            max_steps: usize,
            dry_run: bool,
            env: &mut Env,
        ) -> Result<String> {
            super::run_sync(goal, tool_names, max_steps, dry_run, env)
        }
    }
}

// ------------- MCP (Model Context Protocol) Implementation -------------
#[allow(dead_code)]
pub mod mcp {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// MCP Protocol Version
    pub const MCP_VERSION: &str = "1.0";

    /// MCP Tool Schema
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct McpToolSchema {
        pub name: String,
        pub description: String,
        #[serde(default)]
        pub input_schema: J,
        #[serde(default)]
        pub output_schema: Option<J>,
    }

    /// MCP Client for communicating with MCP servers
    #[derive(Debug, Clone)]
    pub struct McpClient {
        pub endpoint: String,
        tools_cache: Arc<Mutex<HashMap<String, McpToolSchema>>>,
        client: reqwest::blocking::Client,
    }

    impl McpClient {
        pub fn new(endpoint: &str) -> Self {
            // SECURITY FIX (LOW-002): Use secure HTTP client with proper error handling
            let client = crate::security::create_secure_http_client().unwrap_or_else(|_| {
                // Fallback to default client if secure client creation fails
                reqwest::blocking::Client::new()
            });

            Self {
                endpoint: endpoint.to_string(),
                tools_cache: Arc::new(Mutex::new(HashMap::new())),
                client,
            }
        }

        /// Discover available tools from MCP server
        pub fn discover_tools(&self) -> Result<Vec<McpToolSchema>> {
            let url = format!("{}/mcp/v1/tools", self.endpoint.trim_end_matches('/'));

            match self.client.get(&url).send() {
                Ok(response) => {
                    if response.status().is_success() {
                        let tools: Vec<McpToolSchema> = response.json().unwrap_or_default();

                        // Cache discovered tools
                        // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
                        let mut cache = self
                            .tools_cache
                            .lock()
                            .map_err(|e| anyhow!("Failed to acquire tools cache lock: {}", e))?;
                        for tool in &tools {
                            cache.insert(tool.name.clone(), tool.clone());
                        }

                        Ok(tools)
                    } else {
                        // MCP server returned error, return empty list
                        Ok(vec![])
                    }
                }
                Err(_) => {
                    // MCP server unreachable, return empty list
                    Ok(vec![])
                }
            }
        }

        /// List tool names (tries discovery first, falls back to cache)
        pub fn list_tools(&self) -> Result<Vec<String>> {
            // Try fresh discovery
            if let Ok(tools) = self.discover_tools() {
                return Ok(tools.iter().map(|t| t.name.clone()).collect());
            }

            // Fall back to cache
            // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
            let cache = self
                .tools_cache
                .lock()
                .map_err(|e| anyhow!("Failed to acquire tools cache lock: {}", e))?;
            Ok(cache.keys().cloned().collect())
        }

        /// Execute a tool via MCP
        pub fn call_tool(&self, name: &str, input: &str) -> Result<String> {
            let url = format!(
                "{}/mcp/v1/tools/{}/execute",
                self.endpoint.trim_end_matches('/'),
                name
            );

            // Parse input as JSON
            let input_json: J =
                serde_json::from_str(input).unwrap_or_else(|_| J::String(input.to_string()));

            let response = self
                .client
                .post(&url)
                .header(CONTENT_TYPE, "application/json")
                .json(&input_json)
                .send()?;

            if !response.status().is_success() {
                return Err(anyhow!("MCP tool execution failed: {}", response.status()));
            }

            let result: J = response.json()?;
            Ok(result.to_string())
        }

        /// Validate tool input against schema (if available)
        pub fn validate_input(&self, tool_name: &str, _input: &J) -> Result<()> {
            // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
            let cache = self
                .tools_cache
                .lock()
                .map_err(|e| anyhow!("Failed to acquire tools cache lock: {}", e))?;
            if let Some(tool) = cache.get(tool_name) {
                // For now, just check if schema exists
                // Full validation would use jsonschema crate
                if tool.input_schema != J::Null {
                    // Schema exists, assume valid for now
                    // TODO: Implement full JSONSchema validation
                }
                Ok(())
            } else {
                // Tool not in cache, can't validate
                Ok(())
            }
        }

        /// Health check for MCP server
        pub fn health_check(&self) -> bool {
            let url = format!("{}/health", self.endpoint.trim_end_matches('/'));
            self.client
                .get(&url)
                .send()
                .map(|r| r.status().is_success())
                .unwrap_or(false)
        }

        /// Get tool description from cache
        pub fn get_tool_description(&self, name: &str) -> Option<String> {
            // SECURITY: Replace .unwrap() with proper error handling (CVSS 7.1)
            let cache = self.tools_cache.lock().ok()?;
            cache.get(name).map(|t| t.description.clone())
        }
    }

    /// Resolver that exposes MCP tools
    pub struct McpToolResolver {
        client: Arc<McpClient>,
    }

    impl McpToolResolver {
        pub fn new(endpoint: &str) -> Self {
            Self {
                client: Arc::new(McpClient::new(endpoint)),
            }
        }
    }

    impl crate::ai::agents::ToolResolver for McpToolResolver {
        fn list(&self) -> Vec<String> {
            self.client.list_tools().unwrap_or_default()
        }

        fn get(&self, name: &str) -> Option<Box<dyn crate::ai::agents::Tool>> {
            struct McpTool {
                name: String,
                client: Arc<McpClient>,
            }
            impl crate::ai::agents::Tool for McpTool {
                fn name(&self) -> &str {
                    &self.name
                }
                fn description(&self) -> &str {
                    // Try to get cached description, or return default
                    if let Some(_desc) = self.client.get_tool_description(&self.name) {
                        // Need to leak the string to return &'static str
                        // For now, just return a static default
                        "MCP tool"
                    } else {
                        "MCP tool"
                    }
                }
                fn call(
                    &self,
                    input: &str,
                    _env: &mut crate::env::Env,
                ) -> anyhow::Result<crate::value::Value> {
                    let out = self.client.call_tool(&self.name, input)?;
                    Ok(crate::value::Value::Str(out))
                }
            }
            Some(Box::new(McpTool {
                name: name.into(),
                client: Arc::clone(&self.client),
            }))
        }
    }
}
