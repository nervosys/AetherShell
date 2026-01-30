//! Metrics and Monitoring System
//!
//! Comprehensive observability for AetherShell:
//! - Prometheus-compatible metrics export
//! - OpenTelemetry traces and spans
//! - Performance dashboards
//! - Alerting and anomaly detection

use anyhow::{anyhow, Result};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::value::Value;

// ============================================================================
// Metric Types
// ============================================================================

/// Metric type classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetricType {
    /// Monotonically increasing counter
    Counter,
    /// Value that can go up or down
    Gauge,
    /// Distribution of values (quantiles)
    Histogram,
    /// Summary with configurable quantiles
    Summary,
}

/// Labels for metric dimensions
pub type Labels = BTreeMap<String, String>;

/// A single metric data point
#[derive(Debug, Clone)]
pub struct MetricValue {
    pub value: f64,
    pub timestamp: u64,
    pub labels: Labels,
}

/// Counter metric - monotonically increasing
#[derive(Debug)]
pub struct Counter {
    name: String,
    help: String,
    value: AtomicU64,
    labels: Labels,
}

impl Counter {
    pub fn new(name: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            help: help.into(),
            value: AtomicU64::new(0),
            labels: Labels::new(),
        }
    }

    pub fn with_labels(mut self, labels: Labels) -> Self {
        self.labels = labels;
        self
    }

    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_by(&self, v: u64) {
        self.value.fetch_add(v, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// Gauge metric - can increase or decrease
#[derive(Debug)]
pub struct Gauge {
    name: String,
    help: String,
    value: Arc<RwLock<f64>>,
    labels: Labels,
}

impl Gauge {
    pub fn new(name: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            help: help.into(),
            value: Arc::new(RwLock::new(0.0)),
            labels: Labels::new(),
        }
    }

    pub fn with_labels(mut self, labels: Labels) -> Self {
        self.labels = labels;
        self
    }

    pub fn set(&self, v: f64) {
        if let Ok(mut guard) = self.value.write() {
            *guard = v;
        }
    }

    pub fn inc(&self) {
        if let Ok(mut guard) = self.value.write() {
            *guard += 1.0;
        }
    }

    pub fn dec(&self) {
        if let Ok(mut guard) = self.value.write() {
            *guard -= 1.0;
        }
    }

    pub fn get(&self) -> f64 {
        self.value.read().map(|g| *g).unwrap_or(0.0)
    }
}

/// Histogram metric - distribution of values
#[derive(Debug)]
pub struct Histogram {
    name: String,
    help: String,
    buckets: Vec<f64>,
    counts: Arc<RwLock<Vec<u64>>>,
    sum: Arc<RwLock<f64>>,
    count: AtomicU64,
    labels: Labels,
}

impl Histogram {
    pub fn new(name: impl Into<String>, help: impl Into<String>) -> Self {
        // Default buckets for latency (in seconds)
        let buckets = vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ];
        Self::with_buckets(name, help, buckets)
    }

    pub fn with_buckets(
        name: impl Into<String>,
        help: impl Into<String>,
        buckets: Vec<f64>,
    ) -> Self {
        let counts = vec![0u64; buckets.len() + 1]; // +1 for +Inf bucket
        Self {
            name: name.into(),
            help: help.into(),
            buckets,
            counts: Arc::new(RwLock::new(counts)),
            sum: Arc::new(RwLock::new(0.0)),
            count: AtomicU64::new(0),
            labels: Labels::new(),
        }
    }

    pub fn with_labels(mut self, labels: Labels) -> Self {
        self.labels = labels;
        self
    }

    pub fn observe(&self, value: f64) {
        // Update sum
        if let Ok(mut sum) = self.sum.write() {
            *sum += value;
        }

        // Update count
        self.count.fetch_add(1, Ordering::Relaxed);

        // Update bucket counts
        if let Ok(mut counts) = self.counts.write() {
            for (i, bucket) in self.buckets.iter().enumerate() {
                if value <= *bucket {
                    counts[i] += 1;
                }
            }
            // Always increment +Inf bucket
            if let Some(last) = counts.last_mut() {
                *last += 1;
            }
        }
    }

    pub fn get_quantile(&self, q: f64) -> f64 {
        let counts = self.counts.read().ok();
        let total = self.count.load(Ordering::Relaxed);

        if total == 0 || counts.is_none() {
            return 0.0;
        }

        let counts = counts.unwrap();
        let target = (q * total as f64) as u64;

        for (i, count) in counts.iter().enumerate() {
            if *count >= target && i < self.buckets.len() {
                return self.buckets[i];
            }
        }

        self.buckets.last().copied().unwrap_or(0.0)
    }
}

// ============================================================================
// Metrics Registry
// ============================================================================

/// Central registry for all metrics
#[derive(Debug)]
pub struct MetricsRegistry {
    counters: RwLock<HashMap<String, Arc<Counter>>>,
    gauges: RwLock<HashMap<String, Arc<Gauge>>>,
    histograms: RwLock<HashMap<String, Arc<Histogram>>>,
    namespace: String,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::with_namespace("aethershell")
    }

    pub fn with_namespace(namespace: impl Into<String>) -> Self {
        Self {
            counters: RwLock::new(HashMap::new()),
            gauges: RwLock::new(HashMap::new()),
            histograms: RwLock::new(HashMap::new()),
            namespace: namespace.into(),
        }
    }

    fn full_name(&self, name: &str) -> String {
        format!("{}_{}", self.namespace, name)
    }

    pub fn counter(&self, name: &str, help: &str) -> Arc<Counter> {
        let full_name = self.full_name(name);
        let mut counters = self.counters.write().unwrap();

        counters
            .entry(full_name.clone())
            .or_insert_with(|| Arc::new(Counter::new(full_name, help)))
            .clone()
    }

    pub fn gauge(&self, name: &str, help: &str) -> Arc<Gauge> {
        let full_name = self.full_name(name);
        let mut gauges = self.gauges.write().unwrap();

        gauges
            .entry(full_name.clone())
            .or_insert_with(|| Arc::new(Gauge::new(full_name, help)))
            .clone()
    }

    pub fn histogram(&self, name: &str, help: &str) -> Arc<Histogram> {
        let full_name = self.full_name(name);
        let mut histograms = self.histograms.write().unwrap();

        histograms
            .entry(full_name.clone())
            .or_insert_with(|| Arc::new(Histogram::new(full_name, help)))
            .clone()
    }

    /// Export metrics in Prometheus text format
    pub fn export_prometheus(&self) -> String {
        let mut output = String::new();

        // Export counters
        if let Ok(counters) = self.counters.read() {
            for counter in counters.values() {
                output.push_str(&format!(
                    "# HELP {} {}\n# TYPE {} counter\n",
                    counter.name, counter.help, counter.name
                ));
                output.push_str(&format!(
                    "{}{} {}\n",
                    counter.name,
                    format_labels(&counter.labels),
                    counter.get()
                ));
            }
        }

        // Export gauges
        if let Ok(gauges) = self.gauges.read() {
            for gauge in gauges.values() {
                output.push_str(&format!(
                    "# HELP {} {}\n# TYPE {} gauge\n",
                    gauge.name, gauge.help, gauge.name
                ));
                output.push_str(&format!(
                    "{}{} {}\n",
                    gauge.name,
                    format_labels(&gauge.labels),
                    gauge.get()
                ));
            }
        }

        // Export histograms
        if let Ok(histograms) = self.histograms.read() {
            for hist in histograms.values() {
                output.push_str(&format!(
                    "# HELP {} {}\n# TYPE {} histogram\n",
                    hist.name, hist.help, hist.name
                ));

                if let Ok(counts) = hist.counts.read() {
                    for (i, bucket) in hist.buckets.iter().enumerate() {
                        let mut labels = hist.labels.clone();
                        labels.insert("le".to_string(), bucket.to_string());
                        output.push_str(&format!(
                            "{}_bucket{} {}\n",
                            hist.name,
                            format_labels(&labels),
                            counts.get(i).unwrap_or(&0)
                        ));
                    }
                    // +Inf bucket
                    let mut labels = hist.labels.clone();
                    labels.insert("le".to_string(), "+Inf".to_string());
                    output.push_str(&format!(
                        "{}_bucket{} {}\n",
                        hist.name,
                        format_labels(&labels),
                        counts.last().unwrap_or(&0)
                    ));
                }

                if let Ok(sum) = hist.sum.read() {
                    output.push_str(&format!(
                        "{}_sum{} {}\n",
                        hist.name,
                        format_labels(&hist.labels),
                        sum
                    ));
                }

                output.push_str(&format!(
                    "{}_count{} {}\n",
                    hist.name,
                    format_labels(&hist.labels),
                    hist.count.load(Ordering::Relaxed)
                ));
            }
        }

        output
    }

    /// Export metrics as JSON
    pub fn export_json(&self) -> Value {
        let mut metrics: BTreeMap<String, Value> = BTreeMap::new();

        // Export counters
        if let Ok(counters) = self.counters.read() {
            for (name, counter) in counters.iter() {
                let mut m: BTreeMap<String, Value> = BTreeMap::new();
                m.insert("type".to_string(), Value::Str("counter".to_string()));
                m.insert("value".to_string(), Value::Int(counter.get() as i64));
                m.insert("help".to_string(), Value::Str(counter.help.clone()));
                metrics.insert(name.clone(), Value::Record(m));
            }
        }

        // Export gauges
        if let Ok(gauges) = self.gauges.read() {
            for (name, gauge) in gauges.iter() {
                let mut m: BTreeMap<String, Value> = BTreeMap::new();
                m.insert("type".to_string(), Value::Str("gauge".to_string()));
                m.insert("value".to_string(), Value::Float(gauge.get()));
                m.insert("help".to_string(), Value::Str(gauge.help.clone()));
                metrics.insert(name.clone(), Value::Record(m));
            }
        }

        // Export histograms (summary)
        if let Ok(histograms) = self.histograms.read() {
            for (name, hist) in histograms.iter() {
                let mut m: BTreeMap<String, Value> = BTreeMap::new();
                m.insert("type".to_string(), Value::Str("histogram".to_string()));
                m.insert(
                    "count".to_string(),
                    Value::Int(hist.count.load(Ordering::Relaxed) as i64),
                );
                if let Ok(sum) = hist.sum.read() {
                    m.insert("sum".to_string(), Value::Float(*sum));
                }
                m.insert("p50".to_string(), Value::Float(hist.get_quantile(0.5)));
                m.insert("p90".to_string(), Value::Float(hist.get_quantile(0.9)));
                m.insert("p99".to_string(), Value::Float(hist.get_quantile(0.99)));
                m.insert("help".to_string(), Value::Str(hist.help.clone()));
                metrics.insert(name.clone(), Value::Record(m));
            }
        }

        Value::Record(metrics)
    }
}

fn format_labels(labels: &Labels) -> String {
    if labels.is_empty() {
        return String::new();
    }

    let pairs: Vec<String> = labels
        .iter()
        .map(|(k, v)| format!("{}=\"{}\"", k, v.replace('"', "\\\"")))
        .collect();

    format!("{{{}}}", pairs.join(","))
}

// ============================================================================
// OpenTelemetry-style Tracing
// ============================================================================

/// Trace context for distributed tracing
#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub sampled: bool,
}

impl TraceContext {
    pub fn new() -> Self {
        Self {
            trace_id: Uuid::new_v4().to_string().replace('-', ""),
            span_id: generate_span_id(),
            parent_span_id: None,
            sampled: true,
        }
    }

    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: generate_span_id(),
            parent_span_id: Some(self.span_id.clone()),
            sampled: self.sampled,
        }
    }

    /// W3C Trace Context header format
    pub fn to_traceparent(&self) -> String {
        format!(
            "00-{}-{}-{}",
            self.trace_id,
            self.span_id,
            if self.sampled { "01" } else { "00" }
        )
    }

    /// Parse W3C Trace Context header
    pub fn from_traceparent(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 || parts[0] != "00" {
            return None;
        }

        Some(Self {
            trace_id: parts[1].to_string(),
            span_id: parts[2].to_string(),
            parent_span_id: None,
            sampled: parts[3] == "01",
        })
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

fn generate_span_id() -> String {
    let bytes: [u8; 8] = rand::random();
    hex::encode(bytes)
}

/// A span representing a unit of work
#[derive(Debug, Clone)]
pub struct Span {
    pub name: String,
    pub context: TraceContext,
    pub start_time: u64,
    pub end_time: Option<u64>,
    pub attributes: BTreeMap<String, String>,
    pub events: Vec<SpanEvent>,
    pub status: SpanStatus,
}

#[derive(Debug, Clone)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: u64,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanStatus {
    Unset,
    Ok,
    Error(String),
}

impl Span {
    pub fn new(name: impl Into<String>, context: TraceContext) -> Self {
        Self {
            name: name.into(),
            context,
            start_time: current_timestamp_micros(),
            end_time: None,
            attributes: BTreeMap::new(),
            events: Vec::new(),
            status: SpanStatus::Unset,
        }
    }

    pub fn set_attribute(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.attributes.insert(key.into(), value.into());
    }

    pub fn add_event(&mut self, name: impl Into<String>) {
        self.events.push(SpanEvent {
            name: name.into(),
            timestamp: current_timestamp_micros(),
            attributes: BTreeMap::new(),
        });
    }

    pub fn set_status(&mut self, status: SpanStatus) {
        self.status = status;
    }

    pub fn end(&mut self) {
        self.end_time = Some(current_timestamp_micros());
    }

    pub fn duration_micros(&self) -> u64 {
        self.end_time.unwrap_or_else(current_timestamp_micros) - self.start_time
    }
}

fn current_timestamp_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

// ============================================================================
// Trace Exporter
// ============================================================================

/// Span processor for collecting and exporting traces
pub struct SpanProcessor {
    spans: RwLock<Vec<Span>>,
    max_spans: usize,
}

impl Default for SpanProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SpanProcessor {
    pub fn new() -> Self {
        Self {
            spans: RwLock::new(Vec::new()),
            max_spans: 10000,
        }
    }

    pub fn record(&self, span: Span) {
        if let Ok(mut spans) = self.spans.write() {
            if spans.len() >= self.max_spans {
                spans.remove(0);
            }
            spans.push(span);
        }
    }

    pub fn get_spans(&self, limit: usize) -> Vec<Span> {
        self.spans
            .read()
            .map(|s| s.iter().rev().take(limit).cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_trace(&self, trace_id: &str) -> Vec<Span> {
        self.spans
            .read()
            .map(|s| {
                s.iter()
                    .filter(|span| span.context.trace_id == trace_id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Export spans in OTLP JSON format (simplified)
    pub fn export_otlp(&self) -> Value {
        let spans: Vec<Value> = self
            .spans
            .read()
            .map(|s| {
                s.iter()
                    .map(|span| {
                        let mut m: BTreeMap<String, Value> = BTreeMap::new();
                        m.insert("name".to_string(), Value::Str(span.name.clone()));
                        m.insert(
                            "traceId".to_string(),
                            Value::Str(span.context.trace_id.clone()),
                        );
                        m.insert(
                            "spanId".to_string(),
                            Value::Str(span.context.span_id.clone()),
                        );
                        if let Some(ref parent) = span.context.parent_span_id {
                            m.insert("parentSpanId".to_string(), Value::Str(parent.clone()));
                        }
                        m.insert(
                            "startTimeUnixNano".to_string(),
                            Value::Int(span.start_time as i64),
                        );
                        if let Some(end) = span.end_time {
                            m.insert("endTimeUnixNano".to_string(), Value::Int(end as i64));
                        }

                        // Attributes
                        let attrs: Vec<Value> = span
                            .attributes
                            .iter()
                            .map(|(k, v)| {
                                let mut attr: BTreeMap<String, Value> = BTreeMap::new();
                                attr.insert("key".to_string(), Value::Str(k.clone()));
                                let mut val: BTreeMap<String, Value> = BTreeMap::new();
                                val.insert("stringValue".to_string(), Value::Str(v.clone()));
                                attr.insert("value".to_string(), Value::Record(val));
                                Value::Record(attr)
                            })
                            .collect();
                        m.insert("attributes".to_string(), Value::Array(attrs));

                        // Status
                        let status = match &span.status {
                            SpanStatus::Unset => "UNSET",
                            SpanStatus::Ok => "OK",
                            SpanStatus::Error(_) => "ERROR",
                        };
                        m.insert("status".to_string(), Value::Str(status.to_string()));

                        Value::Record(m)
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut result: BTreeMap<String, Value> = BTreeMap::new();
        result.insert("spans".to_string(), Value::Array(spans));
        Value::Record(result)
    }
}

// ============================================================================
// Performance Dashboard
// ============================================================================

/// Real-time performance snapshot
#[derive(Debug, Clone)]
pub struct PerformanceSnapshot {
    pub timestamp: u64,
    pub cpu_usage: f64,
    pub memory_mb: f64,
    pub active_agents: u64,
    pub active_workflows: u64,
    pub requests_per_second: f64,
    pub avg_latency_ms: f64,
    pub error_rate: f64,
}

/// Dashboard for real-time monitoring
pub struct Dashboard {
    snapshots: RwLock<VecDeque<PerformanceSnapshot>>,
    max_history: usize,
    registry: Arc<MetricsRegistry>,
}

impl Dashboard {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        Self {
            snapshots: RwLock::new(VecDeque::new()),
            max_history: 1000,
            registry,
        }
    }

    pub fn record_snapshot(&self, snapshot: PerformanceSnapshot) {
        if let Ok(mut snapshots) = self.snapshots.write() {
            if snapshots.len() >= self.max_history {
                snapshots.pop_front();
            }
            snapshots.push_back(snapshot);
        }
    }

    pub fn get_current(&self) -> Option<PerformanceSnapshot> {
        self.snapshots.read().ok()?.back().cloned()
    }

    pub fn get_history(&self, duration_secs: u64) -> Vec<PerformanceSnapshot> {
        let cutoff = current_timestamp_micros() - (duration_secs * 1_000_000);
        self.snapshots
            .read()
            .map(|s| {
                s.iter()
                    .filter(|snap| snap.timestamp >= cutoff)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Generate dashboard summary
    pub fn summary(&self) -> Value {
        let current = self.get_current();
        let history_1m = self.get_history(60);
        let history_5m = self.get_history(300);

        let mut result: BTreeMap<String, Value> = BTreeMap::new();

        // Current status
        if let Some(curr) = current {
            let mut current_map: BTreeMap<String, Value> = BTreeMap::new();
            current_map.insert("cpu_usage".to_string(), Value::Float(curr.cpu_usage));
            current_map.insert("memory_mb".to_string(), Value::Float(curr.memory_mb));
            current_map.insert(
                "active_agents".to_string(),
                Value::Int(curr.active_agents as i64),
            );
            current_map.insert(
                "active_workflows".to_string(),
                Value::Int(curr.active_workflows as i64),
            );
            current_map.insert("rps".to_string(), Value::Float(curr.requests_per_second));
            current_map.insert("latency_ms".to_string(), Value::Float(curr.avg_latency_ms));
            current_map.insert("error_rate".to_string(), Value::Float(curr.error_rate));
            result.insert("current".to_string(), Value::Record(current_map));
        }

        // 1-minute averages
        if !history_1m.is_empty() {
            let avg_rps: f64 = history_1m
                .iter()
                .map(|s| s.requests_per_second)
                .sum::<f64>()
                / history_1m.len() as f64;
            let avg_latency: f64 =
                history_1m.iter().map(|s| s.avg_latency_ms).sum::<f64>() / history_1m.len() as f64;
            let mut avg_1m: BTreeMap<String, Value> = BTreeMap::new();
            avg_1m.insert("rps".to_string(), Value::Float(avg_rps));
            avg_1m.insert("latency_ms".to_string(), Value::Float(avg_latency));
            result.insert("avg_1m".to_string(), Value::Record(avg_1m));
        }

        // 5-minute averages
        if !history_5m.is_empty() {
            let avg_rps: f64 = history_5m
                .iter()
                .map(|s| s.requests_per_second)
                .sum::<f64>()
                / history_5m.len() as f64;
            let avg_latency: f64 =
                history_5m.iter().map(|s| s.avg_latency_ms).sum::<f64>() / history_5m.len() as f64;
            let mut avg_5m: BTreeMap<String, Value> = BTreeMap::new();
            avg_5m.insert("rps".to_string(), Value::Float(avg_rps));
            avg_5m.insert("latency_ms".to_string(), Value::Float(avg_latency));
            result.insert("avg_5m".to_string(), Value::Record(avg_5m));
        }

        // Include registry metrics
        result.insert("metrics".to_string(), self.registry.export_json());

        Value::Record(result)
    }
}

// ============================================================================
// Alerting System
// ============================================================================

/// Alert severity levels
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Alert rule definition
#[derive(Debug, Clone)]
pub struct AlertRule {
    pub id: String,
    pub name: String,
    pub metric: String,
    pub condition: AlertCondition,
    pub threshold: f64,
    pub duration_secs: u64,
    pub severity: AlertSeverity,
    pub annotations: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub enum AlertCondition {
    GreaterThan,
    LessThan,
    Equal,
    NotEqual,
}

/// Active alert instance
#[derive(Debug, Clone)]
pub struct Alert {
    pub id: String,
    pub rule_id: String,
    pub name: String,
    pub message: String,
    pub severity: AlertSeverity,
    pub fired_at: u64,
    pub resolved_at: Option<u64>,
    pub value: f64,
    pub labels: Labels,
}

/// Alert manager for rule evaluation and notification
pub struct AlertManager {
    rules: RwLock<Vec<AlertRule>>,
    active_alerts: RwLock<HashMap<String, Alert>>,
    alert_history: RwLock<VecDeque<Alert>>,
    max_history: usize,
    event_tx: broadcast::Sender<Alert>,
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertManager {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1000);
        Self {
            rules: RwLock::new(Vec::new()),
            active_alerts: RwLock::new(HashMap::new()),
            alert_history: RwLock::new(VecDeque::new()),
            max_history: 1000,
            event_tx: tx,
        }
    }

    pub fn add_rule(&self, rule: AlertRule) {
        if let Ok(mut rules) = self.rules.write() {
            rules.push(rule);
        }
    }

    pub fn remove_rule(&self, rule_id: &str) {
        if let Ok(mut rules) = self.rules.write() {
            rules.retain(|r| r.id != rule_id);
        }
    }

    pub fn evaluate(&self, metric_name: &str, value: f64, labels: Labels) {
        let rules = self.rules.read().ok();
        if rules.is_none() {
            return;
        }

        for rule in rules.unwrap().iter().filter(|r| r.metric == metric_name) {
            let triggered = match rule.condition {
                AlertCondition::GreaterThan => value > rule.threshold,
                AlertCondition::LessThan => value < rule.threshold,
                AlertCondition::Equal => (value - rule.threshold).abs() < f64::EPSILON,
                AlertCondition::NotEqual => (value - rule.threshold).abs() >= f64::EPSILON,
            };

            let alert_key = format!("{}:{}", rule.id, format_labels(&labels));

            if triggered {
                self.fire_alert(&alert_key, rule, value, labels.clone());
            } else {
                self.resolve_alert(&alert_key);
            }
        }
    }

    fn fire_alert(&self, key: &str, rule: &AlertRule, value: f64, labels: Labels) {
        if let Ok(mut active) = self.active_alerts.write() {
            if active.contains_key(key) {
                return; // Already firing
            }

            let alert = Alert {
                id: Uuid::new_v4().to_string(),
                rule_id: rule.id.clone(),
                name: rule.name.clone(),
                message: format!(
                    "{} is {:?} {} (current: {})",
                    rule.metric, rule.condition, rule.threshold, value
                ),
                severity: rule.severity.clone(),
                fired_at: current_timestamp_micros(),
                resolved_at: None,
                value,
                labels,
            };

            let _ = self.event_tx.send(alert.clone());
            active.insert(key.to_string(), alert);
        }
    }

    fn resolve_alert(&self, key: &str) {
        if let Ok(mut active) = self.active_alerts.write() {
            if let Some(mut alert) = active.remove(key) {
                alert.resolved_at = Some(current_timestamp_micros());

                if let Ok(mut history) = self.alert_history.write() {
                    if history.len() >= self.max_history {
                        history.pop_front();
                    }
                    history.push_back(alert);
                }
            }
        }
    }

    pub fn get_active_alerts(&self) -> Vec<Alert> {
        self.active_alerts
            .read()
            .map(|a| a.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_alert_history(&self, limit: usize) -> Vec<Alert> {
        self.alert_history
            .read()
            .map(|h| h.iter().rev().take(limit).cloned().collect())
            .unwrap_or_default()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Alert> {
        self.event_tx.subscribe()
    }

    /// Export alerts summary
    pub fn summary(&self) -> Value {
        let active = self.get_active_alerts();
        let history = self.get_alert_history(50);

        let mut result: BTreeMap<String, Value> = BTreeMap::new();

        // Active alerts
        let active_alerts: Vec<Value> = active
            .iter()
            .map(|a| {
                let mut m: BTreeMap<String, Value> = BTreeMap::new();
                m.insert("id".to_string(), Value::Str(a.id.clone()));
                m.insert("name".to_string(), Value::Str(a.name.clone()));
                m.insert("message".to_string(), Value::Str(a.message.clone()));
                m.insert(
                    "severity".to_string(),
                    Value::Str(format!("{:?}", a.severity)),
                );
                m.insert("fired_at".to_string(), Value::Int(a.fired_at as i64));
                m.insert("value".to_string(), Value::Float(a.value));
                Value::Record(m)
            })
            .collect();
        result.insert("active".to_string(), Value::Array(active_alerts));

        // Recent history
        let history_alerts: Vec<Value> = history
            .iter()
            .map(|a| {
                let mut m: BTreeMap<String, Value> = BTreeMap::new();
                m.insert("id".to_string(), Value::Str(a.id.clone()));
                m.insert("name".to_string(), Value::Str(a.name.clone()));
                m.insert(
                    "severity".to_string(),
                    Value::Str(format!("{:?}", a.severity)),
                );
                m.insert("fired_at".to_string(), Value::Int(a.fired_at as i64));
                if let Some(resolved) = a.resolved_at {
                    m.insert("resolved_at".to_string(), Value::Int(resolved as i64));
                    m.insert(
                        "duration_ms".to_string(),
                        Value::Int(((resolved - a.fired_at) / 1000) as i64),
                    );
                }
                Value::Record(m)
            })
            .collect();
        result.insert("history".to_string(), Value::Array(history_alerts));

        // Summary stats
        let mut stats: BTreeMap<String, Value> = BTreeMap::new();
        stats.insert("active_count".to_string(), Value::Int(active.len() as i64));
        stats.insert(
            "critical_count".to_string(),
            Value::Int(
                active
                    .iter()
                    .filter(|a| a.severity == AlertSeverity::Critical)
                    .count() as i64,
            ),
        );
        stats.insert(
            "warning_count".to_string(),
            Value::Int(
                active
                    .iter()
                    .filter(|a| a.severity == AlertSeverity::Warning)
                    .count() as i64,
            ),
        );
        result.insert("stats".to_string(), Value::Record(stats));

        Value::Record(result)
    }
}

// ============================================================================
// Observability Manager
// ============================================================================

/// Central observability manager combining all monitoring features
pub struct ObservabilityManager {
    pub registry: Arc<MetricsRegistry>,
    pub spans: Arc<SpanProcessor>,
    pub dashboard: Arc<Dashboard>,
    pub alerts: Arc<AlertManager>,

    // Pre-registered metrics
    pub requests_total: Arc<Counter>,
    pub requests_active: Arc<Gauge>,
    pub request_duration: Arc<Histogram>,
    pub errors_total: Arc<Counter>,
    pub agent_operations: Arc<Counter>,
    pub workflow_operations: Arc<Counter>,
}

impl Default for ObservabilityManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservabilityManager {
    pub fn new() -> Self {
        let registry = Arc::new(MetricsRegistry::new());

        // Pre-register common metrics
        let requests_total = registry.counter("requests_total", "Total number of requests");
        let requests_active = registry.gauge("requests_active", "Currently active requests");
        let request_duration =
            registry.histogram("request_duration_seconds", "Request duration in seconds");
        let errors_total = registry.counter("errors_total", "Total number of errors");
        let agent_operations = registry.counter("agent_operations_total", "Total agent operations");
        let workflow_operations =
            registry.counter("workflow_operations_total", "Total workflow operations");

        let dashboard = Arc::new(Dashboard::new(Arc::clone(&registry)));

        Self {
            registry,
            spans: Arc::new(SpanProcessor::new()),
            dashboard,
            alerts: Arc::new(AlertManager::new()),
            requests_total,
            requests_active,
            request_duration,
            errors_total,
            agent_operations,
            workflow_operations,
        }
    }

    /// Start a new trace
    pub fn start_trace(&self, name: &str) -> Span {
        let ctx = TraceContext::new();
        Span::new(name, ctx)
    }

    /// Start a child span
    pub fn start_span(&self, name: &str, parent: &TraceContext) -> Span {
        Span::new(name, parent.child())
    }

    /// Record a completed span
    pub fn record_span(&self, span: Span) {
        self.spans.record(span);
    }

    /// Record a request
    pub fn record_request(&self, duration_secs: f64, error: bool) {
        self.requests_total.inc();
        self.request_duration.observe(duration_secs);
        if error {
            self.errors_total.inc();
        }

        // Check alert conditions
        let error_rate = if self.requests_total.get() > 0 {
            self.errors_total.get() as f64 / self.requests_total.get() as f64
        } else {
            0.0
        };
        self.alerts
            .evaluate("error_rate", error_rate, Labels::new());
    }

    /// Get complete observability summary
    pub fn summary(&self) -> Value {
        let mut result: BTreeMap<String, Value> = BTreeMap::new();

        // Dashboard summary
        result.insert("dashboard".to_string(), self.dashboard.summary());

        // Alerts
        result.insert("alerts".to_string(), self.alerts.summary());

        // Recent traces
        let recent_spans: Vec<Value> = self
            .spans
            .get_spans(10)
            .into_iter()
            .map(|s| {
                let mut m: BTreeMap<String, Value> = BTreeMap::new();
                m.insert("name".to_string(), Value::Str(s.name.clone()));
                m.insert(
                    "trace_id".to_string(),
                    Value::Str(s.context.trace_id.clone()),
                );
                m.insert(
                    "duration_us".to_string(),
                    Value::Int(s.duration_micros() as i64),
                );
                m.insert("status".to_string(), Value::Str(format!("{:?}", s.status)));
                Value::Record(m)
            })
            .collect();
        result.insert("recent_traces".to_string(), Value::Array(recent_spans));

        Value::Record(result)
    }

    /// Setup default alert rules
    pub fn setup_default_alerts(&self) {
        // High error rate
        self.alerts.add_rule(AlertRule {
            id: "high_error_rate".to_string(),
            name: "High Error Rate".to_string(),
            metric: "error_rate".to_string(),
            condition: AlertCondition::GreaterThan,
            threshold: 0.05,
            duration_secs: 60,
            severity: AlertSeverity::Critical,
            annotations: BTreeMap::new(),
        });

        // High latency
        self.alerts.add_rule(AlertRule {
            id: "high_latency".to_string(),
            name: "High Latency".to_string(),
            metric: "request_latency_p99".to_string(),
            condition: AlertCondition::GreaterThan,
            threshold: 1.0,
            duration_secs: 60,
            severity: AlertSeverity::Warning,
            annotations: BTreeMap::new(),
        });

        // Low throughput
        self.alerts.add_rule(AlertRule {
            id: "low_throughput".to_string(),
            name: "Low Throughput".to_string(),
            metric: "requests_per_second".to_string(),
            condition: AlertCondition::LessThan,
            threshold: 1.0,
            duration_secs: 300,
            severity: AlertSeverity::Warning,
            annotations: BTreeMap::new(),
        });
    }
}

// ============================================================================
// Builtins for Shell Integration
// ============================================================================

/// Create metrics builtins for AetherShell
pub fn create_metrics_builtins() -> BTreeMap<String, Value> {
    let mut builtins: BTreeMap<String, Value> = BTreeMap::new();

    builtins.insert(
        "metrics".to_string(),
        Value::Str("Get metrics summary".to_string()),
    );
    builtins.insert(
        "metrics_prometheus".to_string(),
        Value::Str("Export metrics in Prometheus format".to_string()),
    );
    builtins.insert(
        "trace".to_string(),
        Value::Str("Start a new trace".to_string()),
    );
    builtins.insert(
        "alerts".to_string(),
        Value::Str("Get current alerts".to_string()),
    );
    builtins.insert(
        "dashboard".to_string(),
        Value::Str("Get dashboard summary".to_string()),
    );

    builtins
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let counter = Counter::new("test_counter", "Test counter");
        assert_eq!(counter.get(), 0);

        counter.inc();
        assert_eq!(counter.get(), 1);

        counter.inc_by(5);
        assert_eq!(counter.get(), 6);
    }

    #[test]
    fn test_gauge() {
        let gauge = Gauge::new("test_gauge", "Test gauge");
        assert_eq!(gauge.get(), 0.0);

        gauge.set(10.0);
        assert_eq!(gauge.get(), 10.0);

        gauge.inc();
        assert_eq!(gauge.get(), 11.0);

        gauge.dec();
        assert_eq!(gauge.get(), 10.0);
    }

    #[test]
    fn test_histogram() {
        let hist = Histogram::new("test_hist", "Test histogram");

        hist.observe(0.05);
        hist.observe(0.1);
        hist.observe(0.5);
        hist.observe(1.0);
        hist.observe(5.0);

        assert_eq!(hist.count.load(Ordering::Relaxed), 5);

        let p50 = hist.get_quantile(0.5);
        assert!(p50 > 0.0);
    }

    #[test]
    fn test_registry() {
        let registry = MetricsRegistry::new();

        let counter = registry.counter("test", "Test counter");
        counter.inc();
        counter.inc();

        let gauge = registry.gauge("active", "Active count");
        gauge.set(5.0);

        let prometheus = registry.export_prometheus();
        assert!(prometheus.contains("aethershell_test"));
        assert!(prometheus.contains("aethershell_active"));
    }

    #[test]
    fn test_trace_context() {
        let ctx = TraceContext::new();
        assert!(!ctx.trace_id.is_empty());
        assert!(!ctx.span_id.is_empty());
        assert!(ctx.sampled);

        let child = ctx.child();
        assert_eq!(child.trace_id, ctx.trace_id);
        assert_ne!(child.span_id, ctx.span_id);
        assert_eq!(child.parent_span_id, Some(ctx.span_id.clone()));
    }

    #[test]
    fn test_traceparent_format() {
        let ctx = TraceContext::new();
        let header = ctx.to_traceparent();
        assert!(header.starts_with("00-"));

        let parsed = TraceContext::from_traceparent(&header);
        assert!(parsed.is_some());
        let parsed = parsed.unwrap();
        assert_eq!(parsed.trace_id, ctx.trace_id);
        assert_eq!(parsed.span_id, ctx.span_id);
    }

    #[test]
    fn test_span() {
        let ctx = TraceContext::new();
        let mut span = Span::new("test_operation", ctx);

        span.set_attribute("key", "value");
        span.add_event("checkpoint");
        span.set_status(SpanStatus::Ok);
        span.end();

        assert!(span.end_time.is_some());
        assert!(span.duration_micros() > 0);
        assert_eq!(span.attributes.get("key"), Some(&"value".to_string()));
        assert_eq!(span.events.len(), 1);
    }

    #[test]
    fn test_span_processor() {
        let processor = SpanProcessor::new();

        let ctx = TraceContext::new();
        let mut span = Span::new("test", ctx.clone());
        span.end();
        processor.record(span);

        let spans = processor.get_spans(10);
        assert_eq!(spans.len(), 1);

        let trace_spans = processor.get_trace(&ctx.trace_id);
        assert_eq!(trace_spans.len(), 1);
    }

    #[test]
    fn test_alert_rule() {
        let manager = AlertManager::new();

        manager.add_rule(AlertRule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            metric: "error_rate".to_string(),
            condition: AlertCondition::GreaterThan,
            threshold: 0.1,
            duration_secs: 60,
            severity: AlertSeverity::Critical,
            annotations: BTreeMap::new(),
        });

        // Trigger alert
        manager.evaluate("error_rate", 0.2, Labels::new());
        let active = manager.get_active_alerts();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].severity, AlertSeverity::Critical);

        // Resolve alert
        manager.evaluate("error_rate", 0.05, Labels::new());
        let active = manager.get_active_alerts();
        assert_eq!(active.len(), 0);

        let history = manager.get_alert_history(10);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_alert_conditions() {
        let manager = AlertManager::new();

        // Greater than
        manager.add_rule(AlertRule {
            id: "gt".to_string(),
            name: "GT".to_string(),
            metric: "metric_gt".to_string(),
            condition: AlertCondition::GreaterThan,
            threshold: 50.0,
            duration_secs: 0,
            severity: AlertSeverity::Warning,
            annotations: BTreeMap::new(),
        });

        // Less than
        manager.add_rule(AlertRule {
            id: "lt".to_string(),
            name: "LT".to_string(),
            metric: "metric_lt".to_string(),
            condition: AlertCondition::LessThan,
            threshold: 10.0,
            duration_secs: 0,
            severity: AlertSeverity::Warning,
            annotations: BTreeMap::new(),
        });

        manager.evaluate("metric_gt", 60.0, Labels::new());
        manager.evaluate("metric_lt", 5.0, Labels::new());

        let active = manager.get_active_alerts();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn test_observability_manager() {
        let obs = ObservabilityManager::new();

        // Record some metrics
        obs.requests_total.inc();
        obs.requests_active.set(5.0);
        obs.request_duration.observe(0.1);

        // Start a trace
        let mut span = obs.start_trace("test_operation");
        span.set_attribute("test", "value");
        span.end();
        obs.record_span(span);

        // Get summary
        let summary = obs.summary();
        match summary {
            Value::Record(r) => {
                assert!(r.contains_key("dashboard"));
                assert!(r.contains_key("alerts"));
                assert!(r.contains_key("recent_traces"));
            }
            _ => panic!("Expected Record"),
        }
    }

    #[test]
    fn test_prometheus_export() {
        let registry = MetricsRegistry::with_namespace("test");

        let counter = registry.counter("requests", "Total requests");
        counter.inc_by(100);

        let gauge = registry.gauge("active", "Active connections");
        gauge.set(42.0);

        let output = registry.export_prometheus();
        assert!(output.contains("# TYPE test_requests counter"));
        assert!(output.contains("test_requests 100"));
        assert!(output.contains("# TYPE test_active gauge"));
        assert!(output.contains("test_active 42"));
    }

    #[test]
    fn test_json_export() {
        let registry = MetricsRegistry::new();

        let counter = registry.counter("ops", "Operations");
        counter.inc_by(50);

        let json = registry.export_json();
        match json {
            Value::Record(r) => {
                assert!(r.contains_key("aethershell_ops"));
            }
            _ => panic!("Expected Record"),
        }
    }

    #[test]
    fn test_labels_format() {
        let mut labels = Labels::new();
        labels.insert("method".to_string(), "GET".to_string());
        labels.insert("path".to_string(), "/api".to_string());

        let formatted = format_labels(&labels);
        assert!(formatted.contains("method=\"GET\""));
        assert!(formatted.contains("path=\"/api\""));
    }

    #[test]
    fn test_dashboard() {
        let registry = Arc::new(MetricsRegistry::new());
        let dashboard = Dashboard::new(registry);

        dashboard.record_snapshot(PerformanceSnapshot {
            timestamp: current_timestamp_micros(),
            cpu_usage: 25.0,
            memory_mb: 512.0,
            active_agents: 3,
            active_workflows: 2,
            requests_per_second: 100.0,
            avg_latency_ms: 5.0,
            error_rate: 0.01,
        });

        let current = dashboard.get_current();
        assert!(current.is_some());

        let snap = current.unwrap();
        assert_eq!(snap.cpu_usage, 25.0);
        assert_eq!(snap.active_agents, 3);
    }

    #[test]
    fn test_span_status() {
        let ctx = TraceContext::new();
        let mut span = Span::new("test", ctx);

        assert_eq!(span.status, SpanStatus::Unset);

        span.set_status(SpanStatus::Ok);
        assert_eq!(span.status, SpanStatus::Ok);

        span.set_status(SpanStatus::Error("failed".to_string()));
        assert_eq!(span.status, SpanStatus::Error("failed".to_string()));
    }

    #[test]
    fn test_otlp_export() {
        let processor = SpanProcessor::new();

        let ctx = TraceContext::new();
        let mut span = Span::new("test_op", ctx);
        span.set_attribute("service", "test");
        span.end();
        processor.record(span);

        let otlp = processor.export_otlp();
        match otlp {
            Value::Record(r) => {
                assert!(r.contains_key("spans"));
            }
            _ => panic!("Expected Record"),
        }
    }

    #[test]
    fn test_default_alerts_setup() {
        let obs = ObservabilityManager::new();
        obs.setup_default_alerts();

        // Trigger high error rate
        obs.alerts.evaluate("error_rate", 0.1, Labels::new());

        let active = obs.alerts.get_active_alerts();
        assert!(!active.is_empty());
    }

    #[test]
    fn test_metrics_builtins() {
        let builtins = create_metrics_builtins();
        assert!(builtins.contains_key("metrics"));
        assert!(builtins.contains_key("trace"));
        assert!(builtins.contains_key("alerts"));
        assert!(builtins.contains_key("dashboard"));
    }

    #[test]
    fn test_record_request() {
        let obs = ObservabilityManager::new();

        obs.record_request(0.1, false);
        obs.record_request(0.2, false);
        obs.record_request(0.15, true);

        assert_eq!(obs.requests_total.get(), 3);
        assert_eq!(obs.errors_total.get(), 1);
    }
}
