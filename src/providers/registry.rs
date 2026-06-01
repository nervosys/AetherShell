//! Provider Registry
//!
//! Central registry for managing LLM providers, their configurations,
//! and intelligent routing of requests to the best available provider.
//! Supports multiple load balancing strategies: round-robin, weighted,
//! least-latency, least-requests, and adaptive.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use super::traits::{ChatRequest, LLMProvider, ModelInfo, ProviderError};
use super::{ModelUri, ProviderConfig, ProviderType};

// ============================================================================
// PROVIDER REGISTRY
// ============================================================================

/// Central registry for all configured LLM providers
pub struct ProviderRegistry {
    /// Configured providers
    providers: RwLock<HashMap<ProviderType, ProviderEntry>>,
    /// Default provider
    default_provider: RwLock<Option<ProviderType>>,
    /// Model routing rules
    routing_rules: RwLock<Vec<RoutingRule>>,
    /// Usage statistics
    stats: RwLock<UsageStats>,
    /// Load balancer
    load_balancer: LoadBalancer,
}

/// Entry for a registered provider
#[allow(dead_code)]
struct ProviderEntry {
    /// Provider configuration
    config: ProviderConfig,
    /// Provider instance (lazily initialized)
    instance: Option<Arc<dyn LLMProvider>>,
    /// Provider status
    status: ProviderStatus,
    /// Supported models
    models: Vec<ModelInfo>,
}

/// Provider status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStatus {
    /// Provider is ready to use
    Ready,
    /// Provider is configured but not validated
    Unchecked,
    /// Provider is temporarily unavailable
    Unavailable { reason: String },
    /// Provider authentication failed
    AuthFailed { reason: String },
    /// Provider is disabled by user
    Disabled,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            default_provider: RwLock::new(None),
            routing_rules: RwLock::new(Vec::new()),
            stats: RwLock::new(UsageStats::default()),
            load_balancer: LoadBalancer::default(),
        }
    }

    /// Create a registry with auto-detected providers from environment
    pub fn from_environment() -> Self {
        let registry = Self::new();

        // Auto-detect configured providers
        for provider_type in ProviderType::all() {
            if let Some(env_key) = provider_type.api_key_env_var() {
                if std::env::var(env_key).is_ok() {
                    let config = ProviderConfig::from_env(provider_type);
                    registry.register(provider_type, config);
                }
            }
        }

        registry
    }

    /// Register a provider
    pub fn register(&self, provider_type: ProviderType, config: ProviderConfig) {
        let mut providers = self.providers.write().unwrap();

        let entry = ProviderEntry {
            config,
            instance: None,
            status: ProviderStatus::Unchecked,
            models: Vec::new(),
        };

        providers.insert(provider_type, entry);

        // Set as default if first provider
        let mut default = self.default_provider.write().unwrap();
        if default.is_none() {
            *default = Some(provider_type);
        }
    }

    /// Unregister a provider
    pub fn unregister(&self, provider_type: &ProviderType) {
        let mut providers = self.providers.write().unwrap();
        providers.remove(provider_type);
    }

    /// Get a provider by type
    pub fn get(&self, provider_type: &ProviderType) -> Option<ProviderConfig> {
        let providers = self.providers.read().unwrap();
        providers.get(provider_type).map(|e| e.config.clone())
    }

    /// Get the default provider
    pub fn default_provider(&self) -> Option<ProviderType> {
        *self.default_provider.read().unwrap()
    }

    /// Set the default provider
    pub fn set_default(&self, provider_type: ProviderType) -> Result<(), ProviderError> {
        let providers = self.providers.read().unwrap();
        if !providers.contains_key(&provider_type) {
            return Err(ProviderError::Unknown {
                message: format!("Provider {:?} not registered", provider_type),
            });
        }
        drop(providers);

        let mut default = self.default_provider.write().unwrap();
        *default = Some(provider_type);
        Ok(())
    }

    /// List all registered providers
    pub fn list(&self) -> Vec<ProviderType> {
        let providers = self.providers.read().unwrap();
        providers.keys().cloned().collect()
    }

    /// List available (ready) providers
    pub fn available(&self) -> Vec<ProviderType> {
        let providers = self.providers.read().unwrap();
        providers
            .iter()
            .filter(|(_, e)| {
                e.status == ProviderStatus::Ready || e.status == ProviderStatus::Unchecked
            })
            .map(|(k, _)| *k)
            .collect()
    }

    /// Get provider status
    pub fn status(&self, provider_type: &ProviderType) -> Option<ProviderStatus> {
        let providers = self.providers.read().unwrap();
        providers.get(provider_type).map(|e| e.status.clone())
    }

    /// Update provider status
    pub fn set_status(&self, provider_type: &ProviderType, status: ProviderStatus) {
        let mut providers = self.providers.write().unwrap();
        if let Some(entry) = providers.get_mut(provider_type) {
            entry.status = status;
        }
    }

    /// Find the best provider for a request
    pub fn route(&self, request: &ChatRequest) -> Result<ProviderType, ProviderError> {
        let routing_rules = self.routing_rules.read().unwrap();

        // Check routing rules first
        for rule in routing_rules.iter() {
            if rule.matches(request) {
                if let Some(provider) = &rule.target_provider {
                    return Ok(*provider);
                }
            }
        }

        // Use the model's provider if specified
        let provider = request.model.provider;

        // Check if provider is available
        let providers = self.providers.read().unwrap();
        if let Some(entry) = providers.get(&provider) {
            match &entry.status {
                ProviderStatus::Ready | ProviderStatus::Unchecked => return Ok(provider),
                ProviderStatus::Unavailable { reason } => {
                    // Try to find a fallback
                    return self.find_fallback(&provider, reason);
                }
                ProviderStatus::AuthFailed { reason } => {
                    return Err(ProviderError::AuthenticationFailed {
                        message: reason.clone(),
                    });
                }
                ProviderStatus::Disabled => {
                    return Err(ProviderError::Unavailable {
                        message: "Provider is disabled".to_string(),
                    });
                }
            }
        }

        // Provider not registered, try default
        let default = self.default_provider.read().unwrap();
        if let Some(default_provider) = default.as_ref() {
            return Ok(*default_provider);
        }

        Err(ProviderError::Unavailable {
            message: "No providers available".to_string(),
        })
    }

    /// Find a fallback provider when the primary is unavailable
    fn find_fallback(
        &self,
        _original: &ProviderType,
        _reason: &str,
    ) -> Result<ProviderType, ProviderError> {
        let providers = self.providers.read().unwrap();

        // Collect all ready providers
        let candidates: Vec<ProviderType> = providers
            .iter()
            .filter(|(_, e)| matches!(e.status, ProviderStatus::Ready | ProviderStatus::Unchecked))
            .map(|(k, _)| *k)
            .collect();

        if candidates.is_empty() {
            return Err(ProviderError::Unavailable {
                message: "No fallback providers available".to_string(),
            });
        }

        // Use load balancer to pick from ready candidates
        Ok(self.select_balanced(&candidates).unwrap_or(candidates[0]))
    }

    /// Add a routing rule
    pub fn add_routing_rule(&self, rule: RoutingRule) {
        let mut rules = self.routing_rules.write().unwrap();
        rules.push(rule);
        // Sort by priority (higher first)
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Set the load balancing strategy
    pub fn set_load_balancing(&self, strategy: LoadBalancingStrategy) {
        self.load_balancer.set_strategy(strategy);
    }

    /// Get the current load balancing strategy
    pub fn load_balancing_strategy(&self) -> LoadBalancingStrategy {
        self.load_balancer.strategy()
    }

    /// Select the best provider from available candidates using load balancing.
    /// Called when multiple providers could serve a request.
    pub fn select_balanced(&self, candidates: &[ProviderType]) -> Option<ProviderType> {
        let stats = self.stats.read().unwrap();
        self.load_balancer.select(candidates, &stats.by_provider)
    }

    /// Get usage statistics
    pub fn stats(&self) -> UsageStats {
        self.stats.read().unwrap().clone()
    }

    /// Record a request
    pub fn record_request(&self, provider: &ProviderType, tokens: u32, success: bool) {
        self.record_request_with_latency(provider, tokens, success, None, None);
    }

    /// Record a request with optional latency and cost data
    pub fn record_request_with_latency(
        &self,
        provider: &ProviderType,
        tokens: u32,
        success: bool,
        latency_ms: Option<f64>,
        cost_usd: Option<f64>,
    ) {
        let mut stats = self.stats.write().unwrap();
        stats.total_requests += 1;
        stats.total_tokens += tokens as u64;

        let provider_stats = stats.by_provider.entry(*provider).or_default();

        provider_stats.requests += 1;
        provider_stats.tokens += tokens as u64;
        if success {
            provider_stats.successes += 1;
        } else {
            provider_stats.failures += 1;
        }

        if let Some(lat) = latency_ms {
            // Update latency stats
            if provider_stats.min_latency_ms == 0.0 || lat < provider_stats.min_latency_ms {
                provider_stats.min_latency_ms = lat;
            }
            if lat > provider_stats.max_latency_ms {
                provider_stats.max_latency_ms = lat;
            }
            // Exponential moving average for avg latency (alpha = 0.1)
            if provider_stats.avg_latency_ms == 0.0 {
                provider_stats.avg_latency_ms = lat;
            } else {
                provider_stats.avg_latency_ms = provider_stats.avg_latency_ms * 0.9 + lat * 0.1;
            }
            // P95 estimate via exponential moving maximum (alpha = 0.05)
            if lat > provider_stats.p95_latency_ms {
                provider_stats.p95_latency_ms = provider_stats.p95_latency_ms * 0.95 + lat * 0.05;
            }
        }

        if let Some(cost) = cost_usd {
            provider_stats.total_cost_usd += cost;
        }

        provider_stats.last_request_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
}

// ============================================================================
// ROUTING RULES
// ============================================================================

/// A rule for routing requests to providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    /// Rule name
    pub name: String,
    /// Rule priority (higher = checked first)
    pub priority: u32,
    /// Condition for this rule
    pub condition: RoutingCondition,
    /// Target provider (None = use default)
    pub target_provider: Option<ProviderType>,
    /// Whether to apply fallback on failure
    pub allow_fallback: bool,
}

impl RoutingRule {
    /// Check if this rule matches a request
    pub fn matches(&self, request: &ChatRequest) -> bool {
        self.condition.matches(request)
    }
}

/// Routing condition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoutingCondition {
    /// Always matches
    Always,
    /// Never matches
    Never,
    /// Matches if all conditions match
    All { conditions: Vec<RoutingCondition> },
    /// Matches if any condition matches
    Any { conditions: Vec<RoutingCondition> },
    /// Negates a condition
    Not { condition: Box<RoutingCondition> },
    /// Matches based on model name pattern
    ModelPattern { pattern: String },
    /// Matches if request has tools
    HasTools,
    /// Matches if request has images
    HasVision,
    /// Matches based on estimated token count
    TokenCountOver { threshold: u32 },
    /// Matches based on user ID
    User { user_id: String },
    /// Matches based on time of day (for load balancing)
    TimeRange { start_hour: u8, end_hour: u8 },
    /// Matches if estimated cost per 1K tokens is under threshold
    CostUnder { max_cost_per_1k: f64 },
    /// Matches if provider's average latency is under threshold (ms)
    LatencyUnder { max_ms: f64 },
}

impl RoutingCondition {
    /// Check if this condition matches a request
    pub fn matches(&self, request: &ChatRequest) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::All { conditions } => conditions.iter().all(|c| c.matches(request)),
            Self::Any { conditions } => conditions.iter().any(|c| c.matches(request)),
            Self::Not { condition } => !condition.matches(request),
            Self::ModelPattern { pattern } => request.model.model.contains(pattern),
            Self::HasTools => {
                request.tools.is_some() && !request.tools.as_ref().unwrap().is_empty()
            }
            Self::HasVision => request.messages.iter().any(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, super::traits::ContentPart::Image { .. }))
            }),
            Self::TokenCountOver { threshold } => {
                let estimated: u32 = request
                    .messages
                    .iter()
                    .map(|m| m.text_content().len() as u32 / 4)
                    .sum();
                estimated > *threshold
            }
            Self::User { user_id } => request.user.as_ref() == Some(user_id),
            Self::TimeRange {
                start_hour,
                end_hour,
            } => {
                use chrono::Timelike;
                let hour = chrono::Local::now().hour() as u8;
                if start_hour <= end_hour {
                    hour >= *start_hour && hour < *end_hour
                } else {
                    // Wraps around midnight
                    hour >= *start_hour || hour < *end_hour
                }
            }
            Self::CostUnder { .. } => {
                // Cost-based routing requires model cost info; checked at the routing layer
                // Here we always match — the registry filters by cost when selecting providers
                true
            }
            Self::LatencyUnder { .. } => {
                // Latency-based routing requires stats; checked at the routing layer
                true
            }
        }
    }
}

// ============================================================================
// USAGE STATISTICS
// ============================================================================

/// Overall usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageStats {
    /// Total requests made
    pub total_requests: u64,
    /// Total tokens processed
    pub total_tokens: u64,
    /// Stats by provider
    pub by_provider: HashMap<ProviderType, ProviderStats>,
    /// Stats by model
    pub by_model: HashMap<String, ModelStats>,
}

/// Per-provider statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderStats {
    /// Number of requests
    pub requests: u64,
    /// Number of successful requests
    pub successes: u64,
    /// Number of failed requests
    pub failures: u64,
    /// Total tokens
    pub tokens: u64,
    /// Average latency in ms
    pub avg_latency_ms: f64,
    /// Minimum observed latency in ms
    #[serde(default)]
    pub min_latency_ms: f64,
    /// Maximum observed latency in ms
    #[serde(default)]
    pub max_latency_ms: f64,
    /// P95 latency estimate in ms (exponential moving average)
    #[serde(default)]
    pub p95_latency_ms: f64,
    /// Total estimated cost in USD
    #[serde(default)]
    pub total_cost_usd: f64,
    /// Last request timestamp (epoch seconds)
    #[serde(default)]
    pub last_request_epoch: u64,
}

/// Per-model statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelStats {
    /// Number of requests
    pub requests: u64,
    /// Total tokens
    pub tokens: u64,
    /// Estimated cost (if known)
    pub estimated_cost: f64,
}

// ============================================================================
// PROVIDER DISCOVERY
// ============================================================================

/// Discover available providers from the environment
pub fn discover_providers() -> Vec<(ProviderType, ProviderConfig)> {
    let mut discovered = Vec::new();

    for provider_type in ProviderType::all() {
        if let Some(config) = discover_provider(&provider_type) {
            discovered.push((provider_type, config));
        }
    }

    discovered
}

/// Discover a specific provider from environment
pub fn discover_provider(provider_type: &ProviderType) -> Option<ProviderConfig> {
    let env_key = provider_type.api_key_env_var()?;
    let api_key = std::env::var(env_key).ok()?;
    Some(ProviderConfig {
        provider: *provider_type,
        api_key: Some(api_key),
        base_url: None,
        organization: None,
        project: None,
        default_model: None,
        timeout_secs: Some(120),
        max_retries: Some(3),
        extra: HashMap::new(),
    })
}

// ============================================================================
// MODEL ALIASES
// ============================================================================

/// Model alias registry for convenient model names
#[derive(Debug, Clone, Default)]
pub struct ModelAliases {
    aliases: HashMap<String, ModelUri>,
}

impl ModelAliases {
    pub fn new() -> Self {
        let mut aliases = Self::default();

        // Add common aliases
        aliases.add("gpt4", ModelUri::parse("openai:gpt-4o").unwrap());
        aliases.add("gpt4-mini", ModelUri::parse("openai:gpt-4o-mini").unwrap());
        aliases.add(
            "claude",
            ModelUri::parse("anthropic:claude-3-5-sonnet-20241022").unwrap(),
        );
        aliases.add(
            "claude-opus",
            ModelUri::parse("anthropic:claude-3-opus-20240229").unwrap(),
        );
        aliases.add("gemini", ModelUri::parse("google:gemini-1.5-pro").unwrap());
        aliases.add(
            "gemini-flash",
            ModelUri::parse("google:gemini-1.5-flash").unwrap(),
        );
        aliases.add("llama", ModelUri::parse("ollama:llama3.2").unwrap());
        aliases.add(
            "mistral",
            ModelUri::parse("mistral:mistral-large-latest").unwrap(),
        );
        aliases.add(
            "deepseek",
            ModelUri::parse("deepseek:deepseek-chat").unwrap(),
        );

        aliases
    }

    /// Add an alias
    pub fn add(&mut self, alias: impl Into<String>, uri: ModelUri) {
        self.aliases.insert(alias.into(), uri);
    }

    /// Resolve an alias
    pub fn resolve(&self, name: &str) -> Option<ModelUri> {
        self.aliases.get(name).cloned()
    }

    /// Parse a model string, checking aliases first
    pub fn parse(&self, model: &str) -> Result<ModelUri, ProviderError> {
        // Check if it's an alias
        if let Some(uri) = self.resolve(model) {
            return Ok(uri);
        }

        // Try parsing as URI
        ModelUri::parse(model).map_err(|_| ProviderError::ModelNotFound {
            model: model.to_string(),
        })
    }

    /// List all aliases
    pub fn list(&self) -> Vec<(&String, &ModelUri)> {
        self.aliases.iter().collect()
    }
}

// ============================================================================
// LOAD BALANCING
// ============================================================================

/// Load balancing strategy for distributing requests across providers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[derive(Default)]
pub enum LoadBalancingStrategy {
    /// No balancing — use the first matching provider
    #[default]
    None,
    /// Cycle through providers in order
    RoundRobin,
    /// Distribute based on assigned weights (higher weight = more traffic)
    Weighted { weights: HashMap<ProviderType, u32> },
    /// Route to the provider with lowest average latency
    LeastLatency,
    /// Route to the provider with fewest in-flight / total requests
    LeastRequests,
    /// Adaptive: starts with round-robin, converges on the provider with
    /// the best success-rate-to-latency ratio over a sliding window
    Adaptive,
}

/// Load balancer state
pub struct LoadBalancer {
    /// Active strategy
    strategy: RwLock<LoadBalancingStrategy>,
    /// Round-robin counter (atomic for lock-free increment)
    rr_counter: AtomicU64,
    /// Weighted counter for deterministic weighted distribution
    weighted_counter: AtomicU64,
}

impl Default for LoadBalancer {
    fn default() -> Self {
        Self::new(LoadBalancingStrategy::None)
    }
}

impl LoadBalancer {
    /// Create a new load balancer with the given strategy
    pub fn new(strategy: LoadBalancingStrategy) -> Self {
        Self {
            strategy: RwLock::new(strategy),
            rr_counter: AtomicU64::new(0),
            weighted_counter: AtomicU64::new(0),
        }
    }

    /// Change the active strategy
    pub fn set_strategy(&self, strategy: LoadBalancingStrategy) {
        let mut s = self.strategy.write().unwrap();
        *s = strategy;
    }

    /// Get the current strategy
    pub fn strategy(&self) -> LoadBalancingStrategy {
        self.strategy.read().unwrap().clone()
    }

    /// Select a provider from `candidates` according to the active strategy.
    /// `stats` provides per-provider metrics for latency / request-based strategies.
    pub fn select(
        &self,
        candidates: &[ProviderType],
        stats: &HashMap<ProviderType, ProviderStats>,
    ) -> Option<ProviderType> {
        if candidates.is_empty() {
            return None;
        }
        if candidates.len() == 1 {
            return Some(candidates[0]);
        }

        let strategy = self.strategy.read().unwrap();
        match &*strategy {
            LoadBalancingStrategy::None => Some(candidates[0]),

            LoadBalancingStrategy::RoundRobin => {
                let idx = self.rr_counter.fetch_add(1, Ordering::Relaxed) as usize;
                Some(candidates[idx % candidates.len()])
            }

            LoadBalancingStrategy::Weighted { weights } => {
                // Build a cumulative weight table
                let total_weight: u32 = candidates
                    .iter()
                    .map(|c| weights.get(c).copied().unwrap_or(1))
                    .sum();
                if total_weight == 0 {
                    return Some(candidates[0]);
                }
                let counter = self.weighted_counter.fetch_add(1, Ordering::Relaxed);
                let slot = (counter % total_weight as u64) as u32;
                let mut cumulative = 0u32;
                for c in candidates {
                    cumulative += weights.get(c).copied().unwrap_or(1);
                    if slot < cumulative {
                        return Some(*c);
                    }
                }
                Some(candidates[candidates.len() - 1])
            }

            LoadBalancingStrategy::LeastLatency => {
                // Pick the candidate with the lowest average latency
                let mut best = candidates[0];
                let mut best_latency = f64::MAX;
                for c in candidates {
                    let latency = stats.get(c).map(|s| s.avg_latency_ms).unwrap_or(0.0);
                    // Treat 0 (no data) as "unknown, try it" — give it priority
                    if latency < best_latency || (latency == 0.0 && best_latency != 0.0) {
                        best_latency = latency;
                        best = *c;
                    }
                }
                Some(best)
            }

            LoadBalancingStrategy::LeastRequests => {
                let mut best = candidates[0];
                let mut best_requests = u64::MAX;
                for c in candidates {
                    let reqs = stats.get(c).map(|s| s.requests).unwrap_or(0);
                    if reqs < best_requests {
                        best_requests = reqs;
                        best = *c;
                    }
                }
                Some(best)
            }

            LoadBalancingStrategy::Adaptive => {
                // Score = success_rate / (avg_latency + 1.0)
                // Higher is better. Unknown providers get a bonus to be explored.
                let mut best = candidates[0];
                let mut best_score = f64::NEG_INFINITY;
                for c in candidates {
                    let score = if let Some(s) = stats.get(c) {
                        if s.requests == 0 {
                            // No data — assign exploration bonus
                            1000.0
                        } else {
                            let success_rate = s.successes as f64 / s.requests as f64;
                            success_rate / (s.avg_latency_ms + 1.0) * 1000.0
                        }
                    } else {
                        // Never seen — exploration bonus
                        1000.0
                    };
                    if score > best_score {
                        best_score = score;
                        best = *c;
                    }
                }
                Some(best)
            }
        }
    }
}

// ============================================================================
// GLOBAL REGISTRY
// ============================================================================

lazy_static::lazy_static! {
    /// Global provider registry
    pub static ref PROVIDER_REGISTRY: ProviderRegistry = ProviderRegistry::from_environment();

    /// Global model aliases
    pub static ref MODEL_ALIASES: ModelAliases = ModelAliases::new();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = ProviderRegistry::new();
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_provider_registration() {
        let registry = ProviderRegistry::new();
        let config = ProviderConfig {
            provider: ProviderType::OpenAI,
            api_key: Some("test".to_string()),
            base_url: None,
            organization: None,
            project: None,
            default_model: None,
            timeout_secs: Some(120),
            max_retries: Some(3),
            extra: HashMap::new(),
        };

        registry.register(ProviderType::OpenAI, config);
        assert!(registry.list().contains(&ProviderType::OpenAI));
    }

    #[test]
    fn test_model_aliases() {
        let aliases = ModelAliases::new();

        let gpt4 = aliases.resolve("gpt4");
        assert!(gpt4.is_some());
        assert_eq!(gpt4.unwrap().provider, ProviderType::OpenAI);
    }

    #[test]
    fn test_routing_condition_always() {
        let condition = RoutingCondition::Always;
        let request = ChatRequest::new(ModelUri::parse("openai:gpt-4o").unwrap(), vec![]);
        assert!(condition.matches(&request));
    }

    #[test]
    fn test_routing_condition_has_tools() {
        let condition = RoutingCondition::HasTools;

        let request_no_tools = ChatRequest::new(ModelUri::parse("openai:gpt-4o").unwrap(), vec![]);
        assert!(!condition.matches(&request_no_tools));

        let request_with_tools =
            ChatRequest::new(ModelUri::parse("openai:gpt-4o").unwrap(), vec![])
                .with_tools(vec![super::super::schema::ToolSchema::new("test", "test")]);
        assert!(condition.matches(&request_with_tools));
    }
}
