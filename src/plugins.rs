//! Plugin system for AetherShell extensibility
//!
//! This module provides a comprehensive plugin architecture that allows:
//! - Custom AI backends
//! - Custom builtins (native and script-based)
//! - Custom file handlers
//! - Custom transport protocols
//! - Dynamic library loading (.dll/.so/.dylib)
//! - Script-based plugins (.ae files)
//! - Hot-reloading support
//! - Plugin marketplace integration
//!
//! Plugins can be loaded dynamically at runtime or compiled statically.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Instant, SystemTime};

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
    /// Plugin homepage/repository URL
    #[serde(default)]
    pub homepage: Option<String>,
    /// License identifier (SPDX)
    #[serde(default)]
    pub license: Option<String>,
    /// Keywords for discovery
    #[serde(default)]
    pub keywords: Vec<String>,
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

/// Plugin source type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PluginSource {
    /// Built into AetherShell
    Builtin,
    /// Loaded from a TOML manifest
    Manifest(PathBuf),
    /// Loaded from a dynamic library
    DynamicLibrary(PathBuf),
    /// Loaded from an AetherShell script
    Script(PathBuf),
    /// Installed from marketplace
    Marketplace { registry: String, package: String },
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

    /// Get parameter signature for a builtin (optional)
    fn signature(&self, _name: &str) -> Option<BuiltinSignature> {
        None
    }
}

/// Signature for a plugin builtin function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinSignature {
    /// Function name
    pub name: String,
    /// Parameter definitions
    pub params: Vec<BuiltinParam>,
    /// Return type description
    pub returns: String,
    /// Whether it accepts pipeline input
    pub accepts_input: bool,
    /// Example usage
    pub examples: Vec<String>,
}

/// Parameter definition for a builtin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinParam {
    /// Parameter name
    pub name: String,
    /// Type description
    pub type_desc: String,
    /// Whether required
    pub required: bool,
    /// Default value (as string)
    pub default: Option<String>,
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

// ===================== Dynamic Library Plugin Interface =====================

/// FFI-safe function signature for dynamic library plugins
pub type PluginInitFn = unsafe extern "C" fn() -> *mut PluginVTable;

/// FFI-safe function signature for cleanup
pub type PluginDeinitFn = unsafe extern "C" fn(*mut PluginVTable);

/// Virtual function table for dynamic plugins
#[repr(C)]
pub struct PluginVTable {
    /// Get plugin metadata as JSON
    pub get_metadata: extern "C" fn() -> *const std::ffi::c_char,
    /// Get builtin names as JSON array
    pub get_builtin_names: extern "C" fn() -> *const std::ffi::c_char,
    /// Execute a builtin (name, args_json, input_json) -> result_json
    pub execute_builtin: extern "C" fn(
        *const std::ffi::c_char,
        *const std::ffi::c_char,
        *const std::ffi::c_char,
    ) -> *const std::ffi::c_char,
    /// Get help for a builtin
    pub get_help: extern "C" fn(*const std::ffi::c_char) -> *const std::ffi::c_char,
    /// Free a string allocated by the plugin
    pub free_string: extern "C" fn(*const std::ffi::c_char),
}

/// A loaded dynamic library plugin
pub struct DynamicPlugin {
    metadata: PluginMetadata,
    library: libloading::Library,
    vtable: *mut PluginVTable,
    path: PathBuf,
    loaded_at: Instant,
    last_modified: SystemTime,
}

// Safety: DynamicPlugin is Send+Sync because the vtable functions are thread-safe
unsafe impl Send for DynamicPlugin {}
unsafe impl Sync for DynamicPlugin {}

impl DynamicPlugin {
    /// Load a dynamic library plugin
    pub fn load(path: &Path) -> Result<Self> {
        // Get file modification time for hot-reload detection
        let last_modified = std::fs::metadata(path)
            .context("Failed to get plugin file metadata")?
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH);

        // Load the library
        let library =
            unsafe { libloading::Library::new(path).context("Failed to load dynamic library")? };

        // Get the init function
        let init_fn: libloading::Symbol<PluginInitFn> = unsafe {
            library
                .get(b"aether_plugin_init")
                .context("Plugin missing aether_plugin_init function")?
        };

        // Initialize the plugin
        let vtable = unsafe { init_fn() };
        if vtable.is_null() {
            return Err(anyhow::anyhow!("Plugin initialization returned null"));
        }

        // Get metadata
        let metadata_json = unsafe {
            let ptr = ((*vtable).get_metadata)();
            if ptr.is_null() {
                return Err(anyhow::anyhow!("Plugin returned null metadata"));
            }
            let s = std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned();
            ((*vtable).free_string)(ptr);
            s
        };

        let metadata: PluginMetadata =
            serde_json::from_str(&metadata_json).context("Failed to parse plugin metadata")?;

        Ok(Self {
            metadata,
            library,
            vtable,
            path: path.to_path_buf(),
            loaded_at: Instant::now(),
            last_modified,
        })
    }

    /// Check if the plugin file has been modified
    pub fn needs_reload(&self) -> bool {
        if let Ok(meta) = std::fs::metadata(&self.path) {
            if let Ok(modified) = meta.modified() {
                return modified > self.last_modified;
            }
        }
        false
    }

    /// Get builtin names from the plugin
    pub fn builtin_names(&self) -> Vec<String> {
        unsafe {
            let ptr = ((*self.vtable).get_builtin_names)();
            if ptr.is_null() {
                return vec![];
            }
            let s = std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned();
            ((*self.vtable).free_string)(ptr);
            serde_json::from_str(&s).unwrap_or_default()
        }
    }

    /// Execute a builtin
    pub fn execute(&self, name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value> {
        let name_cstr = std::ffi::CString::new(name)?;
        let args_json = serde_json::to_string(&args)?;
        let args_cstr = std::ffi::CString::new(args_json)?;
        let input_json = serde_json::to_string(&input)?;
        let input_cstr = std::ffi::CString::new(input_json)?;

        unsafe {
            let result_ptr = ((*self.vtable).execute_builtin)(
                name_cstr.as_ptr(),
                args_cstr.as_ptr(),
                input_cstr.as_ptr(),
            );

            if result_ptr.is_null() {
                return Err(anyhow::anyhow!("Plugin returned null result"));
            }

            let result_str = std::ffi::CStr::from_ptr(result_ptr)
                .to_string_lossy()
                .into_owned();
            ((*self.vtable).free_string)(result_ptr);

            // Parse result - could be Value or error
            if result_str.starts_with("{\"error\":") {
                let err: serde_json::Value = serde_json::from_str(&result_str)?;
                Err(anyhow::anyhow!(
                    "{}",
                    err["error"].as_str().unwrap_or("Unknown error")
                ))
            } else {
                serde_json::from_str(&result_str).context("Failed to parse plugin result")
            }
        }
    }

    /// Get help for a builtin
    pub fn help(&self, name: &str) -> Option<String> {
        let name_cstr = std::ffi::CString::new(name).ok()?;
        unsafe {
            let ptr = ((*self.vtable).get_help)(name_cstr.as_ptr());
            if ptr.is_null() {
                return None;
            }
            let s = std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned();
            ((*self.vtable).free_string)(ptr);
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
    }
}

impl Drop for DynamicPlugin {
    fn drop(&mut self) {
        // Try to call deinit if available
        if let Ok(deinit_fn) =
            unsafe { self.library.get::<PluginDeinitFn>(b"aether_plugin_deinit") }
        {
            unsafe {
                deinit_fn(self.vtable);
            }
        }
    }
}

// ===================== Script-based Plugin =====================

/// A plugin defined in AetherShell script (.ae file)
#[derive(Debug, Clone)]
pub struct ScriptPlugin {
    metadata: PluginMetadata,
    path: PathBuf,
    /// Map of builtin name -> lambda expression source
    builtins: HashMap<String, String>,
    /// Help text for builtins
    help_text: HashMap<String, String>,
    /// Last modification time
    last_modified: SystemTime,
}

impl ScriptPlugin {
    /// Load a script plugin from a .ae file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).context("Failed to read script plugin")?;

        // Parse the plugin header (special comment format)
        let mut metadata = PluginMetadata {
            id: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string(),
            name: "Script Plugin".to_string(),
            version: "1.0.0".to_string(),
            author: "Unknown".to_string(),
            description: String::new(),
            categories: vec![PluginCategory::Builtin],
            min_aether_version: "0.1.0".to_string(),
            dependencies: vec![],
            homepage: None,
            license: None,
            keywords: vec![],
        };

        let mut builtins = HashMap::new();
        let mut help_text = HashMap::new();

        // Parse plugin metadata from comments
        // Format: #! plugin.id = "my-plugin"
        //         #! plugin.name = "My Plugin"
        //         #! builtin.my_func = "fn(x) => x * 2"
        //         #! help.my_func = "Doubles the input"
        for line in content.lines() {
            let line = line.trim();
            if let Some(directive) = line.strip_prefix("#!") {
                let directive = directive.trim();
                if let Some((key, value)) = directive.split_once('=') {
                    let key = key.trim();
                    let value = value.trim().trim_matches('"');

                    if let Some(field) = key.strip_prefix("plugin.") {
                        match field {
                            "id" => metadata.id = value.to_string(),
                            "name" => metadata.name = value.to_string(),
                            "version" => metadata.version = value.to_string(),
                            "author" => metadata.author = value.to_string(),
                            "description" => metadata.description = value.to_string(),
                            "license" => metadata.license = Some(value.to_string()),
                            "homepage" => metadata.homepage = Some(value.to_string()),
                            _ => {}
                        }
                    } else if let Some(name) = key.strip_prefix("builtin.") {
                        builtins.insert(name.to_string(), value.to_string());
                    } else if let Some(name) = key.strip_prefix("help.") {
                        help_text.insert(name.to_string(), value.to_string());
                    }
                }
            }
        }

        // Also parse actual function definitions: let my_func = fn(x) => x * 2
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("let ") && line.contains("= fn(") {
                if let Some((name_part, body)) =
                    line.strip_prefix("let ").and_then(|s| s.split_once('='))
                {
                    let name = name_part.trim();
                    let body = body.trim();
                    // Export functions that start with uppercase or have export marker
                    if !name.starts_with('_') {
                        builtins.insert(name.to_string(), body.to_string());
                    }
                }
            }
        }

        let last_modified = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        Ok(Self {
            metadata,
            path: path.to_path_buf(),
            builtins,
            help_text,
            last_modified,
        })
    }

    /// Check if the plugin needs reloading
    pub fn needs_reload(&self) -> bool {
        if let Ok(meta) = std::fs::metadata(&self.path) {
            if let Ok(modified) = meta.modified() {
                return modified > self.last_modified;
            }
        }
        false
    }
}

// ===================== Plugin Registry =====================

/// Global plugin registry singleton
static PLUGIN_REGISTRY: OnceLock<Mutex<PluginRegistry>> = OnceLock::new();

/// Get the global plugin registry
pub fn get_plugin_registry() -> &'static Mutex<PluginRegistry> {
    PLUGIN_REGISTRY.get_or_init(|| Mutex::new(PluginRegistry::new()))
}

/// Hot-reload watcher state
static HOT_RELOAD_ENABLED: OnceLock<RwLock<bool>> = OnceLock::new();

/// Enable or disable hot-reloading
pub fn set_hot_reload(enabled: bool) {
    let lock = HOT_RELOAD_ENABLED.get_or_init(|| RwLock::new(false));
    if let Ok(mut guard) = lock.write() {
        *guard = enabled;
    }
}

/// Check if hot-reloading is enabled
pub fn is_hot_reload_enabled() -> bool {
    HOT_RELOAD_ENABLED
        .get()
        .and_then(|lock| lock.read().ok())
        .map(|guard| *guard)
        .unwrap_or(false)
}

/// Central registry for all plugins
pub struct PluginRegistry {
    /// All registered plugins by ID
    plugins: HashMap<String, PluginEntry>,
    /// AI backend plugins indexed by protocol scheme
    ai_backends: HashMap<String, Arc<dyn AIBackendPlugin>>,
    /// Builtin plugins indexed by function name
    builtins: HashMap<String, (String, Arc<dyn BuiltinPlugin>)>, // (plugin_id, plugin)
    /// Dynamic library plugins
    dynamic_plugins: HashMap<String, Arc<Mutex<DynamicPlugin>>>,
    /// Script plugins
    script_plugins: HashMap<String, Arc<RwLock<ScriptPlugin>>>,
    /// File handlers indexed by extension
    file_handlers: HashMap<String, Arc<dyn FileHandlerPlugin>>,
    /// Transport plugins indexed by scheme
    transports: HashMap<String, Arc<dyn TransportPlugin>>,
    /// Plugin load paths
    load_paths: Vec<PathBuf>,
    /// Marketplace registries
    marketplaces: Vec<MarketplaceRegistry>,
    /// Plugin dependency graph
    dependency_graph: HashMap<String, Vec<String>>,
}

/// Entry for a registered plugin
struct PluginEntry {
    metadata: PluginMetadata,
    source: PluginSource,
    enabled: bool,
    load_time: std::time::Instant,
}

/// Marketplace registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceRegistry {
    /// Registry name
    pub name: String,
    /// Registry URL
    pub url: String,
    /// Whether it's enabled
    pub enabled: bool,
}

/// Plugin search result from marketplace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplacePlugin {
    /// Plugin ID
    pub id: String,
    /// Plugin name
    pub name: String,
    /// Latest version
    pub version: String,
    /// Description
    pub description: String,
    /// Author
    pub author: String,
    /// Download count
    pub downloads: u64,
    /// Categories
    pub categories: Vec<String>,
    /// Keywords
    pub keywords: Vec<String>,
    /// Homepage
    pub homepage: Option<String>,
    /// Repository
    pub repository: Option<String>,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new() -> Self {
        let mut registry = Self {
            plugins: HashMap::new(),
            ai_backends: HashMap::new(),
            builtins: HashMap::new(),
            dynamic_plugins: HashMap::new(),
            script_plugins: HashMap::new(),
            file_handlers: HashMap::new(),
            transports: HashMap::new(),
            load_paths: Vec::new(),
            marketplaces: vec![MarketplaceRegistry {
                name: "AetherShell Official".to_string(),
                url: "https://plugins.aethershell.io/api/v1".to_string(),
                enabled: true,
            }],
            dependency_graph: HashMap::new(),
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
                source: PluginSource::Builtin,
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
                source: PluginSource::Builtin,
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
                source: PluginSource::Builtin,
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
                source: PluginSource::Builtin,
                enabled: true,
                load_time: std::time::Instant::now(),
            },
        );

        // Register by scheme
        self.transports.insert(plugin.scheme().to_string(), plugin);

        Ok(())
    }

    // ===================== Dynamic Plugin Loading =====================

    /// Load a dynamic library plugin
    pub fn load_dynamic_plugin(&mut self, path: &Path) -> Result<String> {
        let plugin = DynamicPlugin::load(path)?;
        let plugin_id = plugin.metadata.id.clone();

        // Check for duplicates
        if self.plugins.contains_key(&plugin_id) {
            return Err(anyhow::anyhow!("Plugin {} is already loaded", plugin_id));
        }

        // Register the plugin
        self.plugins.insert(
            plugin_id.clone(),
            PluginEntry {
                metadata: plugin.metadata.clone(),
                source: PluginSource::DynamicLibrary(path.to_path_buf()),
                enabled: true,
                load_time: Instant::now(),
            },
        );

        // Store the dynamic plugin
        let plugin_arc = Arc::new(Mutex::new(plugin));

        // Register builtins from the dynamic plugin
        if let Ok(guard) = plugin_arc.lock() {
            for name in guard.builtin_names() {
                // Store reference to dynamic plugin for builtin calls
                self.dynamic_plugins
                    .insert(name.clone(), plugin_arc.clone());
            }
        }

        self.dynamic_plugins.insert(plugin_id.clone(), plugin_arc);

        Ok(plugin_id)
    }

    /// Load a script plugin
    pub fn load_script_plugin(&mut self, path: &Path) -> Result<String> {
        let plugin = ScriptPlugin::load(path)?;
        let plugin_id = plugin.metadata.id.clone();

        // Check for duplicates
        if self.plugins.contains_key(&plugin_id) {
            return Err(anyhow::anyhow!("Plugin {} is already loaded", plugin_id));
        }

        // Register the plugin
        self.plugins.insert(
            plugin_id.clone(),
            PluginEntry {
                metadata: plugin.metadata.clone(),
                source: PluginSource::Script(path.to_path_buf()),
                enabled: true,
                load_time: Instant::now(),
            },
        );

        // Store the script plugin
        self.script_plugins
            .insert(plugin_id.clone(), Arc::new(RwLock::new(plugin)));

        Ok(plugin_id)
    }

    /// Load all plugins from configured paths
    pub fn load_all_plugins(&mut self) -> Vec<Result<String>> {
        let mut results = Vec::new();
        let paths = self.load_paths.clone();

        for base_path in paths {
            if !base_path.exists() {
                continue;
            }

            // Scan for plugins
            if let Ok(entries) = std::fs::read_dir(&base_path) {
                for entry in entries.flatten() {
                    let path = entry.path();

                    if path.is_file() {
                        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

                        match ext {
                            // Dynamic libraries
                            "dll" | "so" | "dylib" => {
                                results.push(self.load_dynamic_plugin(&path));
                            }
                            // Script plugins
                            "ae" => {
                                results.push(self.load_script_plugin(&path));
                            }
                            // Manifest files
                            "toml"
                                if path
                                    .file_name()
                                    .map(|n| n.to_str().unwrap_or("").ends_with(".plugin.toml"))
                                    .unwrap_or(false) =>
                            {
                                results.push(
                                    load_plugin_from_manifest(path.to_str().unwrap_or("")).map(
                                        |v| {
                                            if let Value::Record(r) = v {
                                                r.get("id")
                                                    .and_then(|v| {
                                                        if let Value::Str(s) = v {
                                                            Some(s.clone())
                                                        } else {
                                                            None
                                                        }
                                                    })
                                                    .unwrap_or_default()
                                            } else {
                                                String::new()
                                            }
                                        },
                                    ),
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        results
    }

    /// Reload a specific plugin
    pub fn reload_plugin(&mut self, plugin_id: &str) -> Result<()> {
        let entry = self.plugins.get(plugin_id).context("Plugin not found")?;

        match &entry.source {
            PluginSource::DynamicLibrary(path) => {
                let path = path.clone();
                self.unload_plugin(plugin_id)?;
                self.load_dynamic_plugin(&path)?;
            }
            PluginSource::Script(path) => {
                let path = path.clone();
                self.unload_plugin(plugin_id)?;
                self.load_script_plugin(&path)?;
            }
            PluginSource::Manifest(path) => {
                let path = path.clone();
                self.unload_plugin(plugin_id)?;
                load_plugin_from_manifest(path.to_str().unwrap_or(""))?;
            }
            PluginSource::Builtin => {
                return Err(anyhow::anyhow!("Cannot reload built-in plugins"));
            }
            PluginSource::Marketplace { .. } => {
                return Err(anyhow::anyhow!("Use marketplace update to reload"));
            }
        }

        Ok(())
    }

    /// Unload a plugin
    pub fn unload_plugin(&mut self, plugin_id: &str) -> Result<()> {
        let entry = self.plugins.get(plugin_id).context("Plugin not found")?;

        // Check if it's a builtin
        if matches!(entry.source, PluginSource::Builtin) {
            return Err(anyhow::anyhow!("Cannot unload built-in plugins"));
        }

        // Remove from various registries
        self.plugins.remove(plugin_id);
        self.builtins.retain(|_, (pid, _)| pid != plugin_id);
        self.dynamic_plugins.remove(plugin_id);
        self.script_plugins.remove(plugin_id);
        self.ai_backends.remove(plugin_id);
        self.file_handlers
            .retain(|_, h| h.metadata().id != plugin_id);
        self.transports.remove(plugin_id);
        self.dependency_graph.remove(plugin_id);

        Ok(())
    }

    /// Check all plugins for hot-reload
    pub fn check_hot_reload(&mut self) -> Vec<String> {
        let mut reloaded = Vec::new();

        if !is_hot_reload_enabled() {
            return reloaded;
        }

        // Collect plugins that need reloading first (to avoid borrow issues)
        let mut to_reload = Vec::new();

        // Check dynamic plugins
        for (plugin_id, plugin) in &self.dynamic_plugins {
            if let Ok(guard) = plugin.lock() {
                if guard.needs_reload() {
                    to_reload.push(plugin_id.clone());
                }
            }
        }

        // Check script plugins
        for (plugin_id, plugin) in &self.script_plugins {
            if let Ok(guard) = plugin.read() {
                if guard.needs_reload() {
                    to_reload.push(plugin_id.clone());
                }
            }
        }

        // Now reload them
        for plugin_id in to_reload {
            if self.reload_plugin(&plugin_id).is_ok() {
                reloaded.push(plugin_id);
            }
        }

        reloaded
    }

    /// Execute a plugin builtin
    pub fn execute_plugin_builtin(
        &self,
        name: &str,
        args: Vec<Value>,
        input: Option<Value>,
    ) -> Option<Result<Value>> {
        // Check dynamic plugins first
        if let Some(plugin) = self.dynamic_plugins.get(name) {
            if let Ok(guard) = plugin.lock() {
                return Some(guard.execute(name, args, input));
            }
        }

        // Check script plugins
        for (_, plugin) in &self.script_plugins {
            if let Ok(guard) = plugin.read() {
                if guard.builtins.contains_key(name) {
                    // Return the lambda source to be evaluated by the caller
                    let lambda_src = guard.builtins.get(name).cloned();
                    if let Some(src) = lambda_src {
                        return Some(Ok(Value::Str(format!("__plugin_lambda__:{}", src))));
                    }
                }
            }
        }

        // Check trait-based builtin plugins
        if let Some((_, plugin)) = self.builtins.get(name) {
            return Some(plugin.execute(name, args, input));
        }

        None
    }

    /// Get help for a plugin builtin
    pub fn get_plugin_builtin_help(&self, name: &str) -> Option<String> {
        // Check dynamic plugins
        for (_, plugin) in &self.dynamic_plugins {
            if let Ok(guard) = plugin.lock() {
                if let Some(help) = guard.help(name) {
                    return Some(help);
                }
            }
        }

        // Check script plugins
        for (_, plugin) in &self.script_plugins {
            if let Ok(guard) = plugin.read() {
                if let Some(help) = guard.help_text.get(name) {
                    return Some(help.clone());
                }
            }
        }

        // Check trait-based plugins
        if let Some((_, plugin)) = self.builtins.get(name) {
            return plugin.help(name);
        }

        None
    }

    /// List all plugin builtins
    pub fn list_plugin_builtins(&self) -> Vec<(String, String)> {
        let mut result = Vec::new();

        // From dynamic plugins
        for (id, plugin) in &self.dynamic_plugins {
            if let Ok(guard) = plugin.lock() {
                for name in guard.builtin_names() {
                    result.push((name, id.clone()));
                }
            }
        }

        // From script plugins
        for (id, plugin) in &self.script_plugins {
            if let Ok(guard) = plugin.read() {
                for name in guard.builtins.keys() {
                    result.push((name.clone(), id.clone()));
                }
            }
        }

        // From trait-based plugins
        for (name, (id, _)) in &self.builtins {
            result.push((name.clone(), id.clone()));
        }

        result
    }

    // ===================== Marketplace Features =====================

    /// Search plugins in marketplace
    #[cfg(feature = "native")]
    pub fn search_marketplace(&self, query: &str) -> Result<Vec<MarketplacePlugin>> {
        let mut results = Vec::new();

        for marketplace in &self.marketplaces {
            if !marketplace.enabled {
                continue;
            }

            let url = format!(
                "{}/search?q={}",
                marketplace.url,
                urlencoding::encode(query)
            );

            // Use blocking HTTP client
            let response =
                reqwest::blocking::get(&url).context("Failed to connect to marketplace")?;

            if response.status().is_success() {
                let plugins: Vec<MarketplacePlugin> = response
                    .json()
                    .context("Failed to parse marketplace response")?;
                results.extend(plugins);
            }
        }

        Ok(results)
    }

    /// Install a plugin from marketplace
    #[cfg(feature = "native")]
    pub fn install_from_marketplace(&mut self, plugin_id: &str) -> Result<String> {
        for marketplace in &self.marketplaces {
            if !marketplace.enabled {
                continue;
            }

            let url = format!("{}/plugins/{}/download", marketplace.url, plugin_id);

            let response = reqwest::blocking::get(&url);

            if let Ok(resp) = response {
                if resp.status().is_success() {
                    // Get the plugin directory
                    let plugin_dir = self
                        .load_paths
                        .first()
                        .context("No plugin directory configured")?;

                    std::fs::create_dir_all(plugin_dir)?;

                    // Save the plugin
                    let plugin_path = plugin_dir.join(format!("{}.plugin.toml", plugin_id));
                    let content = resp.text()?;
                    std::fs::write(&plugin_path, &content)?;

                    // Load it
                    return load_plugin_from_manifest(plugin_path.to_str().unwrap_or(""))
                        .map(|_| plugin_id.to_string());
                }
            }
        }

        Err(anyhow::anyhow!("Plugin not found in any marketplace"))
    }

    /// Add a custom marketplace registry
    pub fn add_marketplace(&mut self, name: String, url: String) {
        self.marketplaces.push(MarketplaceRegistry {
            name,
            url,
            enabled: true,
        });
    }

    /// List configured marketplaces
    pub fn list_marketplaces(&self) -> Vec<MarketplaceRegistry> {
        self.marketplaces.clone()
    }

    // ===================== Existing Methods =====================

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
            homepage: None,
            license: Some("Apache-2.0".to_string()),
            keywords: vec!["json".to_string(), "file".to_string()],
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
            homepage: None,
            license: Some("Apache-2.0".to_string()),
            keywords: vec!["csv".to_string(), "file".to_string(), "table".to_string()],
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
            homepage: None,
            license: Some("Apache-2.0".to_string()),
            keywords: vec!["toml".to_string(), "file".to_string(), "config".to_string()],
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
        Value::Builtin(b) => serde_json::Value::String(format!("<builtin:{}>", b.name)),
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
        Value::Builtin(b) => toml::Value::String(format!("<builtin:{}>", b.name)),
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

    let homepage = plugin_section
        .get("homepage")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let license = plugin_section
        .get("license")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let keywords = plugin_section
        .get("keywords")
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
        homepage,
        license,
        keywords,
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
            source: PluginSource::Manifest(manifest_path.to_path_buf()),
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

// ===================== Additional Plugin Builtins =====================

/// Load a dynamic library plugin
pub fn bi_plugin_load_dynamic(path: &str) -> Result<Value> {
    let mut registry = get_plugin_registry().lock().unwrap();
    let plugin_id = registry.load_dynamic_plugin(Path::new(path))?;

    let mut result = std::collections::BTreeMap::new();
    result.insert("id".to_string(), Value::Str(plugin_id));
    result.insert("status".to_string(), Value::Str("loaded".to_string()));
    result.insert("type".to_string(), Value::Str("dynamic".to_string()));

    Ok(Value::Record(result))
}

/// Load a script plugin
pub fn bi_plugin_load_script(path: &str) -> Result<Value> {
    let mut registry = get_plugin_registry().lock().unwrap();
    let plugin_id = registry.load_script_plugin(Path::new(path))?;

    let mut result = std::collections::BTreeMap::new();
    result.insert("id".to_string(), Value::Str(plugin_id));
    result.insert("status".to_string(), Value::Str("loaded".to_string()));
    result.insert("type".to_string(), Value::Str("script".to_string()));

    Ok(Value::Record(result))
}

/// Load all plugins from configured paths
pub fn bi_plugin_load_all() -> Value {
    let mut registry = get_plugin_registry().lock().unwrap();
    let results = registry.load_all_plugins();

    let loaded: Vec<Value> = results
        .into_iter()
        .filter_map(|r| r.ok())
        .map(Value::Str)
        .collect();

    Value::Array(loaded)
}

/// Reload a plugin
pub fn bi_plugin_reload(plugin_id: &str) -> Result<Value> {
    let mut registry = get_plugin_registry().lock().unwrap();
    registry.reload_plugin(plugin_id)?;

    let mut result = std::collections::BTreeMap::new();
    result.insert("id".to_string(), Value::Str(plugin_id.to_string()));
    result.insert("status".to_string(), Value::Str("reloaded".to_string()));

    Ok(Value::Record(result))
}

/// Unload a plugin
pub fn bi_plugin_unload(plugin_id: &str) -> Result<Value> {
    unload_plugin(plugin_id)
}

/// Enable hot-reloading
pub fn bi_plugin_hot_reload(enable: bool) -> Value {
    set_hot_reload(enable);
    Value::Bool(enable)
}

/// Check for plugin updates (hot-reload)
pub fn bi_plugin_check_updates() -> Value {
    let mut registry = get_plugin_registry().lock().unwrap();
    let reloaded = registry.check_hot_reload();

    Value::Array(reloaded.into_iter().map(Value::Str).collect())
}

/// List plugin builtins
pub fn bi_plugin_builtins() -> Value {
    let registry = get_plugin_registry().lock().unwrap();
    let builtins = registry.list_plugin_builtins();

    let list: Vec<Value> = builtins
        .into_iter()
        .map(|(name, plugin_id)| {
            let mut rec = std::collections::BTreeMap::new();
            rec.insert("name".to_string(), Value::Str(name));
            rec.insert("plugin".to_string(), Value::Str(plugin_id));
            Value::Record(rec)
        })
        .collect();

    Value::Array(list)
}

/// Get help for a plugin builtin
pub fn bi_plugin_builtin_help(name: &str) -> Value {
    let registry = get_plugin_registry().lock().unwrap();
    match registry.get_plugin_builtin_help(name) {
        Some(help) => Value::Str(help),
        None => Value::Null,
    }
}

/// Search marketplace for plugins
#[cfg(feature = "native")]
pub fn bi_marketplace_search(query: &str) -> Result<Value> {
    let registry = get_plugin_registry().lock().unwrap();
    let results = registry.search_marketplace(query)?;

    let list: Vec<Value> = results
        .into_iter()
        .map(|p| {
            let mut rec = std::collections::BTreeMap::new();
            rec.insert("id".to_string(), Value::Str(p.id));
            rec.insert("name".to_string(), Value::Str(p.name));
            rec.insert("version".to_string(), Value::Str(p.version));
            rec.insert("description".to_string(), Value::Str(p.description));
            rec.insert("author".to_string(), Value::Str(p.author));
            rec.insert("downloads".to_string(), Value::Int(p.downloads as i64));
            rec.insert(
                "categories".to_string(),
                Value::Array(p.categories.into_iter().map(Value::Str).collect()),
            );
            Value::Record(rec)
        })
        .collect();

    Ok(Value::Array(list))
}

/// Install plugin from marketplace
#[cfg(feature = "native")]
pub fn bi_marketplace_install(plugin_id: &str) -> Result<Value> {
    let mut registry = get_plugin_registry().lock().unwrap();
    let id = registry.install_from_marketplace(plugin_id)?;

    let mut result = std::collections::BTreeMap::new();
    result.insert("id".to_string(), Value::Str(id));
    result.insert("status".to_string(), Value::Str("installed".to_string()));

    Ok(Value::Record(result))
}

/// List configured marketplaces
pub fn bi_marketplace_list() -> Value {
    let registry = get_plugin_registry().lock().unwrap();
    let marketplaces = registry.list_marketplaces();

    let list: Vec<Value> = marketplaces
        .into_iter()
        .map(|m| {
            let mut rec = std::collections::BTreeMap::new();
            rec.insert("name".to_string(), Value::Str(m.name));
            rec.insert("url".to_string(), Value::Str(m.url));
            rec.insert("enabled".to_string(), Value::Bool(m.enabled));
            Value::Record(rec)
        })
        .collect();

    Value::Array(list)
}

/// Add a custom marketplace
pub fn bi_marketplace_add(name: &str, url: &str) -> Value {
    let mut registry = get_plugin_registry().lock().unwrap();
    registry.add_marketplace(name.to_string(), url.to_string());
    Value::Bool(true)
}

/// Get plugin source information
pub fn bi_plugin_source(plugin_id: &str) -> Value {
    let registry = get_plugin_registry().lock().unwrap();

    if let Some(entry) = registry.plugins.get(plugin_id) {
        let source_str = match &entry.source {
            PluginSource::Builtin => "builtin".to_string(),
            PluginSource::Manifest(p) => format!("manifest:{}", p.display()),
            PluginSource::DynamicLibrary(p) => format!("dynamic:{}", p.display()),
            PluginSource::Script(p) => format!("script:{}", p.display()),
            PluginSource::Marketplace { registry, package } => {
                format!("marketplace:{}:{}", registry, package)
            }
        };

        let mut rec = std::collections::BTreeMap::new();
        rec.insert("id".to_string(), Value::Str(plugin_id.to_string()));
        rec.insert("source".to_string(), Value::Str(source_str));
        rec.insert("enabled".to_string(), Value::Bool(entry.enabled));
        rec.insert(
            "load_time".to_string(),
            Value::Int(entry.load_time.elapsed().as_secs() as i64),
        );

        Value::Record(rec)
    } else {
        Value::Null
    }
}

/// List plugins by source type
pub fn bi_plugins_by_source(source_type: &str) -> Value {
    let registry = get_plugin_registry().lock().unwrap();

    let filtered: Vec<Value> = registry
        .plugins
        .iter()
        .filter(|(_, entry)| {
            let matches = match source_type.to_lowercase().as_str() {
                "builtin" => matches!(entry.source, PluginSource::Builtin),
                "dynamic" | "library" => matches!(entry.source, PluginSource::DynamicLibrary(_)),
                "script" => matches!(entry.source, PluginSource::Script(_)),
                "manifest" => matches!(entry.source, PluginSource::Manifest(_)),
                "marketplace" => matches!(entry.source, PluginSource::Marketplace { .. }),
                _ => true,
            };
            matches
        })
        .map(|(id, entry)| {
            let mut rec = std::collections::BTreeMap::new();
            rec.insert("id".to_string(), Value::Str(id.clone()));
            rec.insert("name".to_string(), Value::Str(entry.metadata.name.clone()));
            rec.insert(
                "version".to_string(),
                Value::Str(entry.metadata.version.clone()),
            );
            rec.insert("enabled".to_string(), Value::Bool(entry.enabled));
            Value::Record(rec)
        })
        .collect();

    Value::Array(filtered)
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

    #[test]
    fn test_plugin_categories() {
        let cats = bi_plugin_categories();
        if let Value::Array(arr) = cats {
            assert!(arr.len() >= 5);
            assert!(arr.contains(&Value::Str("Builtin".to_string())));
            assert!(arr.contains(&Value::Str("FileHandler".to_string())));
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_hot_reload_toggle() {
        // Initially disabled
        assert!(!is_hot_reload_enabled());

        // Enable
        set_hot_reload(true);
        assert!(is_hot_reload_enabled());

        // Disable again
        set_hot_reload(false);
        assert!(!is_hot_reload_enabled());
    }

    #[test]
    fn test_plugin_source() {
        let source = bi_plugin_source("builtin.json");
        if let Value::Record(rec) = source {
            assert_eq!(rec.get("id"), Some(&Value::Str("builtin.json".to_string())));
            assert_eq!(rec.get("source"), Some(&Value::Str("builtin".to_string())));
            assert_eq!(rec.get("enabled"), Some(&Value::Bool(true)));
        } else {
            panic!("Expected record");
        }
    }

    #[test]
    fn test_plugins_by_source() {
        let builtin = bi_plugins_by_source("builtin");
        if let Value::Array(arr) = builtin {
            assert!(!arr.is_empty());
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_marketplace_list() {
        let list = bi_marketplace_list();
        if let Value::Array(arr) = list {
            // Should have at least the official marketplace
            assert!(!arr.is_empty());
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_plugin_builtins() {
        let builtins = bi_plugin_builtins();
        // May be empty if no plugin builtins registered
        assert!(matches!(builtins, Value::Array(_)));
    }

    #[test]
    fn test_script_plugin_parsing() {
        // Create a temporary script plugin content
        let content = r#"
#! plugin.id = "test-plugin"
#! plugin.name = "Test Plugin"
#! plugin.version = "1.0.0"
#! plugin.author = "Test Author"
#! builtin.double = "fn(x) => x * 2"
#! help.double = "Doubles the input value"

let triple = fn(x) => x * 3
        "#;

        // Write to temp file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_plugin.ae");
        std::fs::write(&temp_file, content).unwrap();

        // Load it
        let plugin = ScriptPlugin::load(&temp_file).unwrap();

        assert_eq!(plugin.metadata.id, "test-plugin");
        assert_eq!(plugin.metadata.name, "Test Plugin");
        assert_eq!(plugin.metadata.author, "Test Author");
        assert!(plugin.builtins.contains_key("double"));
        assert!(plugin.builtins.contains_key("triple"));
        assert!(plugin.help_text.contains_key("double"));

        // Cleanup
        std::fs::remove_file(temp_file).ok();
    }

    #[test]
    fn test_plugin_metadata_serialization() {
        let metadata = PluginMetadata {
            id: "test".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "Author".to_string(),
            description: "Desc".to_string(),
            categories: vec![PluginCategory::Builtin],
            min_aether_version: "0.1.0".to_string(),
            dependencies: vec![],
            homepage: Some("https://example.com".to_string()),
            license: Some("MIT".to_string()),
            keywords: vec!["test".to_string()],
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let parsed: PluginMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(metadata.id, parsed.id);
        assert_eq!(metadata.homepage, parsed.homepage);
        assert_eq!(metadata.license, parsed.license);
    }
}
