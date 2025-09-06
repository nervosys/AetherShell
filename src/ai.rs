//! Provider-agnostic LLMs + Agents (single & multi-agent swarms) for Aether Shell.
//! This version adds:
//! - Model URIs (`openai:gpt-4o-mini`, `ollama:llama3`, `compat:mixtral`, `tgi:mixtral`, `stub`)
//! - Backend registry and per-agent model selection
//! - ToolRegistry with Builtin + (stub) MCP resolver
//! - Agents: run_sync + run_sync_with_model
//! - Swarm framework with Coordinator (RoundRobin + Router stubs)
//! - Multi-modal support for images, audio, and video
//! - Compat: ai::agents::run_sync AND ai::agents::swarm::run_sync are available.

use anyhow::{Result, anyhow};
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{Value as J, json};

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
        Provider::OpenAICompat => Box::new(OpenAiCompatMultiModalBackend),
        _ => Box::new(StubMultiModalBackend),
    }
}

// Multi-modal backend implementations
struct StubMultiModalBackend;
impl MultiModalLlmBackend for StubMultiModalBackend {
    fn chat_multimodal(&self, messages: &[MultiModalMessage]) -> Result<String> {
        let text = messages
            .iter()
            .map(|m| m.to_text())
            .collect::<Vec<_>>()
            .join("\n");
        stub::complete_sync(&text)
    }
}

struct OpenAiMultiModalBackend;
impl MultiModalLlmBackend for OpenAiMultiModalBackend {
    fn chat_multimodal(&self, messages: &[MultiModalMessage]) -> Result<String> {
        let api_key =
            std::env::var("OPENAI_API_KEY").map_err(|_| anyhow!("OPENAI_API_KEY not set"))?;
        let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".into());
        let url = "https://api.openai.com/v1/chat/completions";

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
            "model": model,
            "messages": openai_messages,
            "temperature": 0.2,
            "max_tokens": 1000
        });

        let v: J = Client::new()
            .post(url)
            .header(AUTHORIZATION, format!("Bearer {}", api_key))
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

/// Route by `AETHER_AI` to one of: stub | openai | ollama | openai_compat | tgi
pub fn complete_sync_router(prompt: &str) -> Result<String> {
    match std::env::var("AETHER_AI")
        .unwrap_or_else(|_| "stub".into())
        .as_str()
    {
        "openai" => openai::complete_sync(prompt),
        "ollama" => ollama::complete_sync(prompt),
        "openai_compat" | "compat" => openai_compat::complete_sync(prompt),
        "tgi" => tgi::complete_sync(prompt),
        _ => stub::complete_sync(prompt),
    }
}

// ---------------------- Backends -----------------------

pub mod stub {
    use anyhow::Result;
    pub fn complete_sync(prompt: &str) -> Result<String> {
        let t = prompt.trim();
        let preview = if t.len() > 400 {
            format!("{}…", &t[..400])
        } else {
            t.to_string()
        };
        Ok(format!(
            "[ai:stub]\nsummary: ok\ntext: {}",
            preview.replace('\n', " ")
        ))
    }
}

pub mod openai {
    use super::*;
    pub fn complete_sync(prompt: &str) -> Result<String> {
        let api_key =
            std::env::var("OPENAI_API_KEY").map_err(|_| anyhow!("OPENAI_API_KEY not set"))?;
        let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
        let url = "https://api.openai.com/v1/chat/completions";
        let body = json!({
            "model": model,
            "messages": [
                { "role":"system", "content":"You are a succinct assistant embedded in a shell." },
                { "role":"user",   "content": prompt }
            ],
            "temperature": 0.2
        });
        let v: J = Client::new()
            .post(url)
            .header(AUTHORIZATION, format!("Bearer {}", api_key))
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
        let v: J = Client::new()
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
        let v: J = Client::new()
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
        let r = Client::new()
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
    OpenAICompat,
    Tgi,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    pub provider: Provider,
    pub model: String,
}

/// Parse strings like:
/// - "openai:gpt-4o-mini" / "ollama:llama3" / "compat:mixtral" / "tgi:mixtral" / "stub"
pub fn parse_model_ref(s: &str) -> ModelRef {
    let s = s.trim();
    if let Some((pfx, rest)) = s.split_once(':') {
        let model = rest.trim().to_string();
        let provider = match pfx.trim().to_lowercase().as_str() {
            "openai" => Provider::OpenAI,
            "ollama" => Provider::Ollama,
            "compat" | "openai_compat" => Provider::OpenAICompat,
            "tgi" => Provider::Tgi,
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
    fn chat(&self, messages: &[ChatMessage]) -> Result<String> {
        let last = messages.last().map(|m| m.content.as_str()).unwrap_or("");
        stub::complete_sync(last)
    }
}
struct OpenAiBackend;
impl LlmBackend for OpenAiBackend {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String> {
        let api_key =
            std::env::var("OPENAI_API_KEY").map_err(|_| anyhow!("OPENAI_API_KEY not set"))?;
        let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
        let url = "https://api.openai.com/v1/chat/completions";
        let body = json!({ "model": model, "messages": messages, "temperature": 0.2 });
        let v: J = Client::new()
            .post(url)
            .header(AUTHORIZATION, format!("Bearer {}", api_key))
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
        let v: J = Client::new()
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
struct TgiBackend;
impl LlmBackend for TgiBackend {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String> {
        let base = std::env::var("TGI_URL").unwrap_or_else(|_| "http://localhost:8080".into());
        let url = format!("{}/generate", base.trim_end_matches('/'));
        let body = json!({"inputs": render_prompt(messages), "parameters": {"temperature": 0.2}});
        let v: J = Client::new()
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

fn backend_from_env() -> Box<dyn LlmBackend> {
    backend_from_model(std::env::var("AETHER_MODEL_URI").unwrap_or_else(|_| {
        match std::env::var("AETHER_AI")
            .unwrap_or_else(|_| "stub".into())
            .as_str()
        {
            "openai" => "openai:gpt-4o-mini",
            "ollama" => "ollama:llama3",
            "openai_compat" | "compat" => "compat:mixtral",
            "tgi" => "tgi:mixtral",
            _ => "stub",
        }
        .to_string()
    }))
}
fn backend_from_model(uri: String) -> Box<dyn LlmBackend> {
    let m = parse_model_ref(&uri);
    match m.provider {
        Provider::OpenAI => Box::new(OpenAiBackend),
        Provider::Ollama => Box::new(OllamaBackend),
        Provider::OpenAICompat => Box::new(OpenAiCompatBackend),
        Provider::Tgi => Box::new(TgiBackend),
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

fn parse_agent_command(text: &str) -> (Option<J>, String) {
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
fn display_value(v: &Value) -> String {
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
                        let mut coord = self.coord.take().expect("coord not initialized");
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

// ------------- MCP (Model Context Protocol) stubs -------------
// Compile-safe placeholders; wire to a real MCP server later.
#[allow(dead_code)]
pub mod mcp {
    use super::*;
    #[derive(Debug, Clone)]
    pub struct McpClient {
        pub endpoint: String,
    }
    impl McpClient {
        pub fn new(endpoint: &str) -> Self {
            Self {
                endpoint: endpoint.to_string(),
            }
        }
        pub fn list_tools(&self) -> Result<Vec<String>> {
            // TODO: call MCP server
            Ok(vec![])
        }
        pub fn call_tool(&self, _name: &str, _input: &str) -> Result<String> {
            // TODO: call MCP server tool
            Ok(String::new())
        }
    }

    /// Resolver that exposes MCP tools
    pub struct McpToolResolver {
        client: McpClient,
    }
    impl McpToolResolver {
        pub fn new(endpoint: &str) -> Self {
            Self {
                client: McpClient::new(endpoint),
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
                client: McpClient,
            }
            impl crate::ai::agents::Tool for McpTool {
                fn name(&self) -> &str {
                    &self.name
                }
                fn description(&self) -> &str {
                    "MCP tool"
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
                client: self.client.clone(),
            }))
        }
    }
}
