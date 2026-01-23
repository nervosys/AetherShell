//! Plugin system for AetherShell extensibility
//!
//! This module provides a plugin architecture that allows:
//! - Custom AI backends
//! - Custom builtins
//! - Custom file handlers
//! - Custom transport protocols
//!
//! Plugins can be loaded dynamically at runtime or compiled statically.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use crate::value::Value;

// ===================== Plugin Metadata =====================

/// Plugin metadata describing a loaded plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Unique identifier for the plugin
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Version string (semver)
    pub version: String,
    /// Plugin author
    pub author: String,
    /// Brief description
    pub description: String,
    /// Categories this plugin provides (ai_backend, builtin, handler, etc.)
    pub categories: Vec<PluginCategory>,
    /// Minimum AetherShell version required
    pub min_aether_version: String,
    /// Plugin dependencies (other plugin IDs)
    pub dependencies: Vec<String>,
}

/// Categories of plugin functionality
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PluginCategory {
    /// AI/LLM backend provider
    AIBackend,
    /// Custom builtin functions
    Builtin,
    /// File format handler
    FileHandler,
    /// Transport protocol
    Transport,
    /// Syntax/language extension
    Syntax,
    /// TUI component
    TUIComponent,
    /// Other/custom category
    Custom(String),
}

// ===================== Plugin Traits =====================

/// Trait for AI backend plugins
pub trait AIBackendPlugin: Send + Sync {
    /// Get plugin metadata
    fn metadata(&self) -> &PluginMetadata;

    /// Check if this backend is available
    fn is_available(&self) -> bool;

    /// Get supported models
    fn supported_models(&self) -> Vec<String>;

    /// Complete a chat request
    fn chat_completion(&self, model: &str, messages: Vec<crate::ai::ChatMessage>)
        -> Result<String>;

    /// Generate embeddings (optional)
    fn embeddings(&self, _model: &str, _input: &str) -> Result<Vec<f32>> {
        Err(anyhow::anyhow!("Embeddings not supported by this backend"))
    }

    /// Get streaming support
    fn supports_streaming(&self) -> bool {
        false
    }
}

/// Trait for builtin function plugins
pub trait BuiltinPlugin: Send + Sync {
    /// Get plugin metadata
    fn metadata(&self) -> &PluginMetadata;

    /// Get the list of builtin names provided
    fn builtin_names(&self) -> Vec<String>;

    /// Execute a builtin function
    fn execute(&self, name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value>;

    /// Get help text for a builtin
    fn help(&self, name: &str) -> Option<String>;
}

/// Trait for file handler plugins
pub trait FileHandlerPlugin: Send + Sync {
    /// Get plugin metadata
    fn metadata(&self) -> &PluginMetadata;

    /// Get supported file extensions
    fn supported_extensions(&self) -> Vec<String>;

    /// Get supported MIME types
    fn supported_mime_types(&self) -> Vec<String>;

    /// Read and parse a file
    fn read(&self, path: &std::path::Path) -> Result<Value>;

    /// Write a value to a file
    fn write(&self, path: &std::path::Path, value: &Value) -> Result<()>;
}

/// Trait for transport protocol plugins
pub trait TransportPlugin: Send + Sync {
    /// Get plugin metadata
    fn metadata(&self) -> &PluginMetadata;

    /// Get the protocol scheme (e.g., "mqtt", "grpc")
    fn scheme(&self) -> &str;

    /// Connect to an endpoint
    fn connect(&self, uri: &str) -> Result<Box<dyn TransportConnection>>;
}

/// A connection from a transport plugin
pub trait TransportConnection: Send + Sync {
    /// Send a message
    fn send(&self, data: &[u8]) -> Result<()>;

    /// Receive a message (blocking)
    fn receive(&self) -> Result<Vec<u8>>;

    /// Close the connection
    fn close(&self) -> Result<()>;

    /// Check if connected
    fn is_connected(&self) -> bool;
}

// ===================== Plugin Registry =====================

/// Global plugin registry singleton
static PLUGIN_REGISTRY: OnceLock<Mutex<PluginRegistry>> = OnceLock::new();

/// Get the global plugin registry
pub fn get_plugin_registry() -> &'static Mutex<PluginRegistry> {
    PLUGIN_REGISTRY.get_or_init(|| Mutex::new(PluginRegistry::new()))
}

/// Central registry for all plugins
pub struct PluginRegistry {
    /// All registered plugins by ID
    plugins: HashMap<String, PluginEntry>,
    /// AI backend plugins indexed by protocol scheme
    ai_backends: HashMap<String, Arc<dyn AIBackendPlugin>>,
    /// Builtin plugins indexed by function name
    builtins: HashMap<String, (String, Arc<dyn BuiltinPlugin>)>, // (plugin_id, plugin)
    /// File handlers indexed by extension
    file_handlers: HashMap<String, Arc<dyn FileHandlerPlugin>>,
    /// Transport plugins indexed by scheme
    transports: HashMap<String, Arc<dyn TransportPlugin>>,
    /// Plugin load paths
    load_paths: Vec<PathBuf>,
}

/// Entry for a registered plugin
struct PluginEntry {
    metadata: PluginMetadata,
    enabled: bool,
    load_time: std::time::Instant,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new() -> Self {
        let mut registry = Self {
            plugins: HashMap::new(),
            ai_backends: HashMap::new(),
            builtins: HashMap::new(),
            file_handlers: HashMap::new(),
            transports: HashMap::new(),
            load_paths: Vec::new(),
        };

        // Add default load paths
        registry.add_default_load_paths();

        // Register built-in plugins
        registry.register_builtin_plugins();

        registry
    }

    /// Add default plugin load paths
    fn add_default_load_paths(&mut self) {
        // User plugin directory
        if let Some(home) = dirs::home_dir() {
            self.load_paths
                .push(home.join(".aethershell").join("plugins"));
        }

        // XDG data directory
        if let Some(data_dir) = dirs::data_dir() {
            self.load_paths
                .push(data_dir.join("aethershell").join("plugins"));
        }

        // System plugin directory (Unix-like)
        #[cfg(unix)]
        self.load_paths
            .push(PathBuf::from("/usr/share/aethershell/plugins"));

        // System plugin directory (Windows)
        #[cfg(windows)]
        if let Some(program_data) = std::env::var_os("ProgramData") {
            self.load_paths.push(
                PathBuf::from(program_data)
                    .join("AetherShell")
                    .join("plugins"),
            );
        }
    }

    /// Register built-in plugins
    fn register_builtin_plugins(&mut self) {
        // Register the built-in file handlers
        let json_handler = Arc::new(JsonFileHandler);
        self.register_file_handler(json_handler).ok();

        let csv_handler = Arc::new(CsvFileHandler);
        self.register_file_handler(csv_handler).ok();

        let toml_handler = Arc::new(TomlFileHandler);
        self.register_file_handler(toml_handler).ok();
    }

    /// Register an AI backend plugin
    pub fn register_ai_backend(&mut self, plugin: Arc<dyn AIBackendPlugin>) -> Result<()> {
        let metadata = plugin.metadata().clone();
        let plugin_id = metadata.id.clone();

        // Register main entry
        self.plugins.insert(
            plugin_id.clone(),
            PluginEntry {
                metadata: metadata.clone(),
                enabled: true,
                load_time: std::time::Instant::now(),
            },
        );

        // Register by scheme/name
        self.ai_backends.insert(plugin_id, plugin);

        Ok(())
    }

    /// Register a builtin plugin
    pub fn register_builtin(&mut self, plugin: Arc<dyn BuiltinPlugin>) -> Result<()> {
        let metadata = plugin.metadata().clone();
        let plugin_id = metadata.id.clone();

        // Register main entry
        self.plugins.insert(
            plugin_id.clone(),
            PluginEntry {
                metadata: metadata.clone(),
                enabled: true,
                load_time: std::time::Instant::now(),
            },
        );

        // Register each builtin name
        for name in plugin.builtin_names() {
            self.builtins
                .insert(name, (plugin_id.clone(), plugin.clone()));
        }

        Ok(())
    }

    /// Register a file handler plugin
    pub fn register_file_handler(&mut self, plugin: Arc<dyn FileHandlerPlugin>) -> Result<()> {
        let metadata = plugin.metadata().clone();
        let plugin_id = metadata.id.clone();

        // Register main entry
        self.plugins.insert(
            plugin_id,
            PluginEntry {
                metadata: metadata.clone(),
                enabled: true,
                load_time: std::time::Instant::now(),
            },
        );

        // Register by extension
        for ext in plugin.supported_extensions() {
            self.file_handlers
                .insert(ext.to_lowercase(), plugin.clone());
        }

        Ok(())
    }

    /// Register a transport plugin
    pub fn register_transport(&mut self, plugin: Arc<dyn TransportPlugin>) -> Result<()> {
        let metadata = plugin.metadata().clone();
        let plugin_id = metadata.id.clone();

        // Register main entry
        self.plugins.insert(
            plugin_id,
            PluginEntry {
                metadata: metadata.clone(),
                enabled: true,
                load_time: std::time::Instant::now(),
            },
        );

        // Register by scheme
        self.transports.insert(plugin.scheme().to_string(), plugin);

        Ok(())
    }

    /// Get an AI backend by name/scheme
    pub fn get_ai_backend(&self, name: &str) -> Option<Arc<dyn AIBackendPlugin>> {
        self.ai_backends.get(name).cloned()
    }

    /// Get a builtin plugin by function name
    pub fn get_builtin(&self, name: &str) -> Option<Arc<dyn BuiltinPlugin>> {
        self.builtins.get(name).map(|(_, p)| p.clone())
    }

    /// Get a file handler by extension
    pub fn get_file_handler(&self, extension: &str) -> Option<Arc<dyn FileHandlerPlugin>> {
        self.file_handlers.get(&extension.to_lowercase()).cloned()
    }

    /// Get a transport by scheme
    pub fn get_transport(&self, scheme: &str) -> Option<Arc<dyn TransportPlugin>> {
        self.transports.get(scheme).cloned()
    }

    /// List all registered plugins
    pub fn list_plugins(&self) -> Vec<PluginMetadata> {
        self.plugins.values().map(|e| e.metadata.clone()).collect()
    }

    /// List plugins by category
    pub fn list_by_category(&self, category: &PluginCategory) -> Vec<PluginMetadata> {
        self.plugins
            .values()
            .filter(|e| e.metadata.categories.contains(category))
            .map(|e| e.metadata.clone())
            .collect()
    }

    /// Enable a plugin
    pub fn enable_plugin(&mut self, plugin_id: &str) -> Result<()> {
        self.plugins
            .get_mut(plugin_id)
            .context("Plugin not found")?
            .enabled = true;
        Ok(())
    }

    /// Disable a plugin
    pub fn disable_plugin(&mut self, plugin_id: &str) -> Result<()> {
        self.plugins
            .get_mut(plugin_id)
            .context("Plugin not found")?
            .enabled = false;
        Ok(())
    }

    /// Check if a plugin is enabled
    pub fn is_enabled(&self, plugin_id: &str) -> bool {
        self.plugins
            .get(plugin_id)
            .map(|e| e.enabled)
            .unwrap_or(false)
    }

    /// Get plugin metadata
    pub fn get_metadata(&self, plugin_id: &str) -> Option<PluginMetadata> {
        self.plugins.get(plugin_id).map(|e| e.metadata.clone())
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ===================== Built-in File Handlers =====================

/// JSON file handler
struct JsonFileHandler;

impl FileHandlerPlugin for JsonFileHandler {
    fn metadata(&self) -> &PluginMetadata {
        static META: OnceLock<PluginMetadata> = OnceLock::new();
        META.get_or_init(|| PluginMetadata {
            id: "builtin.json".to_string(),
            name: "JSON File Handler".to_string(),
            version: "1.0.0".to_string(),
            author: "AetherShell Team".to_string(),
            description: "Native JSON file reading and writing".to_string(),
            categories: vec![PluginCategory::FileHandler],
            min_aether_version: "0.1.0".to_string(),
            dependencies: vec![],
        })
    }

    fn supported_extensions(&self) -> Vec<String> {
        vec!["json".to_string()]
    }

    fn supported_mime_types(&self) -> Vec<String> {
        vec!["application/json".to_string()]
    }

    fn read(&self, path: &std::path::Path) -> Result<Value> {
        let content = std::fs::read_to_string(path)?;
        let json: serde_json::Value = serde_json::from_str(&content)?;
        Ok(json_to_value(json))
    }

    fn write(&self, path: &std::path::Path, value: &Value) -> Result<()> {
        let json = value_to_json(value);
        let content = serde_json::to_string_pretty(&json)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

/// CSV file handler
struct CsvFileHandler;

impl FileHandlerPlugin for CsvFileHandler {
    fn metadata(&self) -> &PluginMetadata {
        static META: OnceLock<PluginMetadata> = OnceLock::new();
        META.get_or_init(|| PluginMetadata {
            id: "builtin.csv".to_string(),
            name: "CSV File Handler".to_string(),
            version: "1.0.0".to_string(),
            author: "AetherShell Team".to_string(),
            description: "Native CSV file reading and writing".to_string(),
            categories: vec![PluginCategory::FileHandler],
            min_aether_version: "0.1.0".to_string(),
            dependencies: vec![],
        })
    }

    fn supported_extensions(&self) -> Vec<String> {
        vec!["csv".to_string()]
    }

    fn supported_mime_types(&self) -> Vec<String> {
        vec!["text/csv".to_string()]
    }

    fn read(&self, path: &std::path::Path) -> Result<Value> {
        let mut reader = csv::Reader::from_path(path)?;
        let headers: Vec<String> = reader.headers()?.iter().map(|s| s.to_string()).collect();

        let mut rows = Vec::new();
        for result in reader.records() {
            let record = result?;
            let mut row = std::collections::BTreeMap::new();
            for (i, field) in record.iter().enumerate() {
                if let Some(header) = headers.get(i) {
                    row.insert(header.clone(), Value::Str(field.to_string()));
                }
            }
            rows.push(Value::Record(row));
        }

        Ok(Value::Array(rows))
    }

    fn write(&self, path: &std::path::Path, value: &Value) -> Result<()> {
        let mut writer = csv::Writer::from_path(path)?;

        if let Value::Array(rows) = value {
            // Get headers from first row
            if let Some(Value::Record(first)) = rows.first() {
                let headers: Vec<&str> = first.keys().map(|s| s.as_str()).collect();
                writer.write_record(&headers)?;

                // Write data
                for row in rows {
                    if let Value::Record(rec) = row {
                        let values: Vec<String> = headers
                            .iter()
                            .map(|h| rec.get(*h).map(|v| v.to_string()).unwrap_or_default())
                            .collect();
                        writer.write_record(&values)?;
                    }
                }
            }
        }

        writer.flush()?;
        Ok(())
    }
}

/// TOML file handler
struct TomlFileHandler;

impl FileHandlerPlugin for TomlFileHandler {
    fn metadata(&self) -> &PluginMetadata {
        static META: OnceLock<PluginMetadata> = OnceLock::new();
        META.get_or_init(|| PluginMetadata {
            id: "builtin.toml".to_string(),
            name: "TOML File Handler".to_string(),
            version: "1.0.0".to_string(),
            author: "AetherShell Team".to_string(),
            description: "Native TOML file reading and writing".to_string(),
            categories: vec![PluginCategory::FileHandler],
            min_aether_version: "0.1.0".to_string(),
            dependencies: vec![],
        })
    }

    fn supported_extensions(&self) -> Vec<String> {
        vec!["toml".to_string()]
    }

    fn supported_mime_types(&self) -> Vec<String> {
        vec!["application/toml".to_string()]
    }

    fn read(&self, path: &std::path::Path) -> Result<Value> {
        let content = std::fs::read_to_string(path)?;
        let toml_value: toml::Value = toml::from_str(&content)?;
        Ok(toml_to_value(toml_value))
    }

    fn write(&self, path: &std::path::Path, value: &Value) -> Result<()> {
        let toml_value = value_to_toml(value);
        let content = toml::to_string_pretty(&toml_value)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

// ===================== Value Conversion Helpers =====================

/// Convert JSON value to AetherShell Value
fn json_to_value(json: serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::Str(s),
        serde_json::Value::Array(arr) => Value::Array(arr.into_iter().map(json_to_value).collect()),
        serde_json::Value::Object(obj) => {
            let map = obj
                .into_iter()
                .map(|(k, v)| (k, json_to_value(v)))
                .collect();
            Value::Record(map)
        }
    }
}

/// Convert AetherShell Value to JSON value
fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::Value::Number((*i).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::Str(s) => serde_json::Value::String(s.clone()),
        Value::Uri(u) => serde_json::Value::String(u.clone()),
        Value::Array(arr) => serde_json::Value::Array(arr.iter().map(value_to_json).collect()),
        Value::Record(rec) => {
            let obj: serde_json::Map<String, serde_json::Value> = rec
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Table(t) => {
            // Convert table to array of records
            let rows: Vec<serde_json::Value> = t
                .rows
                .iter()
                .map(|row| {
                    let obj: serde_json::Map<String, serde_json::Value> = row
                        .iter()
                        .map(|(k, v)| (k.clone(), value_to_json(v)))
                        .collect();
                    serde_json::Value::Object(obj)
                })
                .collect();
            serde_json::Value::Array(rows)
        }
        Value::Lambda(_) => serde_json::Value::String("<lambda>".to_string()),
        Value::AsyncLambda(_) => serde_json::Value::String("<async lambda>".to_string()),
        Value::Future(_) => serde_json::Value::String("<future>".to_string()),
        Value::Error(msg) => serde_json::json!({"error": msg}),
    }
}

/// Convert TOML value to AetherShell Value
fn toml_to_value(toml: toml::Value) -> Value {
    match toml {
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Integer(i) => Value::Int(i),
        toml::Value::Float(f) => Value::Float(f),
        toml::Value::String(s) => Value::Str(s),
        toml::Value::Datetime(dt) => Value::Str(dt.to_string()),
        toml::Value::Array(arr) => Value::Array(arr.into_iter().map(toml_to_value).collect()),
        toml::Value::Table(table) => {
            let map = table
                .into_iter()
                .map(|(k, v)| (k, toml_to_value(v)))
                .collect();
            Value::Record(map)
        }
    }
}

/// Convert AetherShell Value to TOML value
fn value_to_toml(value: &Value) -> toml::Value {
    match value {
        Value::Null => toml::Value::String("null".to_string()),
        Value::Bool(b) => toml::Value::Boolean(*b),
        Value::Int(i) => toml::Value::Integer(*i),
        Value::Float(f) => toml::Value::Float(*f),
        Value::Str(s) => toml::Value::String(s.clone()),
        Value::Uri(u) => toml::Value::String(u.clone()),
        Value::Array(arr) => toml::Value::Array(arr.iter().map(value_to_toml).collect()),
        Value::Record(rec) => {
            let table: toml::map::Map<String, toml::Value> = rec
                .iter()
                .map(|(k, v)| (k.clone(), value_to_toml(v)))
                .collect();
            toml::Value::Table(table)
        }
        Value::Table(t) => {
            // Convert table to array of tables
            let tables: Vec<toml::Value> = t
                .rows
                .iter()
                .map(|row| {
                    let table: toml::map::Map<String, toml::Value> = row
                        .iter()
                        .map(|(k, v)| (k.clone(), value_to_toml(v)))
                        .collect();
                    toml::Value::Table(table)
                })
                .collect();
            toml::Value::Array(tables)
        }
        Value::Lambda(_) => toml::Value::String("<lambda>".to_string()),
        Value::AsyncLambda(_) => toml::Value::String("<async lambda>".to_string()),
        Value::Future(_) => toml::Value::String("<future>".to_string()),
        Value::Error(msg) => toml::Value::String(format!("Error: {}", msg)),
    }
}

// ===================== Plugin Builtins =====================

/// List all registered plugins
pub fn bi_plugins_list() -> Value {
    let registry = get_plugin_registry().lock().unwrap();
    let plugins: Vec<Value> = registry
        .list_plugins()
        .into_iter()
        .map(|meta| {
            let mut rec = std::collections::BTreeMap::new();
            rec.insert("id".to_string(), Value::Str(meta.id));
            rec.insert("name".to_string(), Value::Str(meta.name));
            rec.insert("version".to_string(), Value::Str(meta.version));
            rec.insert("author".to_string(), Value::Str(meta.author));
            rec.insert("description".to_string(), Value::Str(meta.description));
            rec.insert(
                "categories".to_string(),
                Value::Array(
                    meta.categories
                        .into_iter()
                        .map(|c| Value::Str(format!("{:?}", c)))
                        .collect(),
                ),
            );
            Value::Record(rec)
        })
        .collect();

    Value::Array(plugins)
}

/// Get plugin info by ID
pub fn bi_plugin_info(plugin_id: &str) -> Value {
    let registry = get_plugin_registry().lock().unwrap();

    match registry.get_metadata(plugin_id) {
        Some(meta) => {
            let mut rec = std::collections::BTreeMap::new();
            rec.insert("id".to_string(), Value::Str(meta.id));
            rec.insert("name".to_string(), Value::Str(meta.name));
            rec.insert("version".to_string(), Value::Str(meta.version));
            rec.insert("author".to_string(), Value::Str(meta.author));
            rec.insert("description".to_string(), Value::Str(meta.description));
            rec.insert(
                "min_aether_version".to_string(),
                Value::Str(meta.min_aether_version),
            );
            rec.insert(
                "categories".to_string(),
                Value::Array(
                    meta.categories
                        .into_iter()
                        .map(|c| Value::Str(format!("{:?}", c)))
                        .collect(),
                ),
            );
            rec.insert(
                "dependencies".to_string(),
                Value::Array(meta.dependencies.into_iter().map(Value::Str).collect()),
            );
            rec.insert(
                "enabled".to_string(),
                Value::Bool(registry.is_enabled(plugin_id)),
            );
            Value::Record(rec)
        }
        None => Value::Null,
    }
}

/// Enable a plugin
pub fn bi_plugin_enable(plugin_id: &str) -> Result<Value> {
    let mut registry = get_plugin_registry().lock().unwrap();
    registry.enable_plugin(plugin_id)?;
    Ok(Value::Bool(true))
}

/// Disable a plugin
pub fn bi_plugin_disable(plugin_id: &str) -> Result<Value> {
    let mut registry = get_plugin_registry().lock().unwrap();
    registry.disable_plugin(plugin_id)?;
    Ok(Value::Bool(true))
}

/// List all plugin categories
pub fn bi_plugin_categories() -> Value {
    let categories = vec![
        "AIBackend",
        "Builtin",
        "FileHandler",
        "Transport",
        "Syntax",
        "TUIComponent",
    ];
    Value::Array(
        categories
            .into_iter()
            .map(|c| Value::Str(c.to_string()))
            .collect(),
    )
}

/// Load a plugin from a manifest file
///
/// The manifest file is a TOML file with the following structure:
/// ```toml
/// [plugin]
/// id = "my-plugin"
/// name = "My Plugin"
/// version = "1.0.0"
/// author = "Author Name"
/// description = "Plugin description"
/// categories = ["Builtin"]
/// min_aether_version = "0.1.0"
///
/// [builtins]
/// # Define custom builtins
/// my_func = """
/// fn(x) => x * 2
/// """
/// ```
pub fn load_plugin_from_manifest(path: &str) -> Result<Value> {
    let manifest_path = std::path::Path::new(path);

    if !manifest_path.exists() {
        return Err(anyhow::anyhow!("Plugin manifest not found: {}", path));
    }

    let content =
        std::fs::read_to_string(manifest_path).context("Failed to read plugin manifest")?;

    let manifest: toml::Value =
        toml::from_str(&content).context("Failed to parse plugin manifest")?;

    // Extract plugin metadata
    let plugin_section = manifest
        .get("plugin")
        .context("Missing [plugin] section in manifest")?;

    let id = plugin_section
        .get("id")
        .and_then(|v| v.as_str())
        .context("Missing plugin.id")?;

    let name = plugin_section
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(id);

    let version = plugin_section
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("1.0.0");

    let author = plugin_section
        .get("author")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");

    let description = plugin_section
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let min_version = plugin_section
        .get("min_aether_version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.1.0");

    let categories_arr = plugin_section
        .get("categories")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter_map(|s| match s.to_lowercase().as_str() {
                    "aibackend" | "ai_backend" => Some(PluginCategory::AIBackend),
                    "builtin" | "builtins" => Some(PluginCategory::Builtin),
                    "filehandler" | "file_handler" => Some(PluginCategory::FileHandler),
                    "transport" => Some(PluginCategory::Transport),
                    "syntax" => Some(PluginCategory::Syntax),
                    "tuicomponent" | "tui_component" => Some(PluginCategory::TUIComponent),
                    other => Some(PluginCategory::Custom(other.to_string())),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![PluginCategory::Custom("unknown".to_string())]);

    let dependencies = plugin_section
        .get("dependencies")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let metadata = PluginMetadata {
        id: id.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        author: author.to_string(),
        description: description.to_string(),
        categories: categories_arr,
        min_aether_version: min_version.to_string(),
        dependencies,
    };

    // Register the plugin in the registry
    let mut registry = get_plugin_registry().lock().unwrap();

    // Check if already loaded
    if registry.plugins.contains_key(&metadata.id) {
        return Err(anyhow::anyhow!("Plugin {} is already loaded", metadata.id));
    }

    registry.plugins.insert(
        metadata.id.clone(),
        PluginEntry {
            metadata: metadata.clone(),
            enabled: true,
            load_time: std::time::Instant::now(),
        },
    );

    // Return plugin info
    let mut result = std::collections::BTreeMap::new();
    result.insert("id".to_string(), Value::Str(metadata.id));
    result.insert("name".to_string(), Value::Str(metadata.name));
    result.insert("version".to_string(), Value::Str(metadata.version));
    result.insert("status".to_string(), Value::Str("loaded".to_string()));

    Ok(Value::Record(result))
}

/// Unload a plugin by ID
pub fn unload_plugin(plugin_id: &str) -> Result<Value> {
    let mut registry = get_plugin_registry().lock().unwrap();

    // Check if it's a builtin plugin (cannot unload)
    if plugin_id.starts_with("builtin.") {
        return Err(anyhow::anyhow!(
            "Cannot unload built-in plugin: {}",
            plugin_id
        ));
    }

    // Remove from registry
    if registry.plugins.remove(plugin_id).is_some() {
        // Also remove any registered builtins, handlers, etc.
        registry.builtins.retain(|_key, (pid, _)| pid != plugin_id);

        let mut result = std::collections::BTreeMap::new();
        result.insert("id".to_string(), Value::Str(plugin_id.to_string()));
        result.insert("status".to_string(), Value::Str("unloaded".to_string()));
        Ok(Value::Record(result))
    } else {
        Err(anyhow::anyhow!("Plugin not found: {}", plugin_id))
    }
}

// ===================== Tests =====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_registry_creation() {
        let registry = PluginRegistry::new();
        assert!(!registry.plugins.is_empty());
        assert!(registry.file_handlers.contains_key("json"));
        assert!(registry.file_handlers.contains_key("csv"));
        assert!(registry.file_handlers.contains_key("toml"));
    }

    #[test]
    fn test_json_conversion() {
        let json = serde_json::json!({
            "name": "test",
            "count": 42,
            "active": true,
            "items": [1, 2, 3]
        });

        let value = json_to_value(json.clone());
        let back = value_to_json(&value);

        assert_eq!(json, back);
    }

    #[test]
    fn test_plugin_list() {
        let list = bi_plugins_list();
        if let Value::Array(plugins) = list {
            assert!(!plugins.is_empty());
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_plugin_info() {
        let info = bi_plugin_info("builtin.json");
        if let Value::Record(rec) = info {
            assert_eq!(rec.get("id"), Some(&Value::Str("builtin.json".to_string())));
        } else {
            panic!("Expected record");
        }
    }

    #[test]
    fn test_plugin_enable_disable() {
        // Enable should work for existing plugin
        let result = bi_plugin_enable("builtin.json");
        assert!(result.is_ok());

        // Disable should work
        let result = bi_plugin_disable("builtin.json");
        assert!(result.is_ok());

        // Re-enable
        bi_plugin_enable("builtin.json").unwrap();
    }

    #[test]
    fn test_toml_conversion() {
        let toml_str = r#"
            name = "test"
            count = 42
            active = true
            items = [1, 2, 3]
        "#;

        let toml_value: toml::Value = toml::from_str(toml_str).unwrap();
        let value = toml_to_value(toml_value);

        if let Value::Record(rec) = value {
            assert_eq!(rec.get("name"), Some(&Value::Str("test".to_string())));
            assert_eq!(rec.get("count"), Some(&Value::Int(42)));
            assert_eq!(rec.get("active"), Some(&Value::Bool(true)));
        } else {
            panic!("Expected record");
        }
    }
}
