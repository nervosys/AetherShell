//! External Agentic Tools Integration
//!
//! This module provides integration for external packages (like simon) to be
//! used as tools by AI agents through AetherShell's agent API.
//!
//! ## Architecture
//!
//! External tools are discovered and registered from:
//! 1. Installed marketplace packages with tool definitions
//! 2. Local paths specified in configuration
//! 3. System-wide tool registrations
//!
//! Tools are exposed through the same Agent API as built-in tools, allowing
//! AI agents to seamlessly use external capabilities.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::RwLock;
use std::time::{Duration, Instant};

// ============================================================================
// Self-Describing Tool Support (e.g., simon with `ai manifest`)
// ============================================================================

/// Manifest returned by self-describing tools via `tool ai manifest -f json`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfDescribingManifest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub tools: Vec<SelfDescribingTool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfDescribingTool {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub parameters: SelfDescribingParameters,
    #[serde(default)]
    pub example: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SelfDescribingParameters {
    #[serde(rename = "type", default = "default_object_type")]
    pub param_type: String,
    #[serde(default)]
    pub properties: HashMap<String, SelfDescribingProperty>,
    #[serde(default)]
    pub required: Vec<String>,
}

fn default_object_type() -> String {
    "object".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfDescribingProperty {
    #[serde(rename = "type", default)]
    pub prop_type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub default: Option<JsonValue>,
}

// ============================================================================
// External Tool Definitions
// ============================================================================

/// External tool manifest (tool.toml or aethershell.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalToolManifest {
    /// Tool metadata
    pub tool: ToolMetadata,
    /// Tool capabilities/functions
    #[serde(default)]
    pub capabilities: Vec<ToolCapability>,
    /// Agent integration settings
    #[serde(default)]
    pub agent: AgentSettings,
    /// Whether this is a self-describing tool (has `ai manifest` command)
    #[serde(skip)]
    pub self_describing: bool,
    /// Cached self-describing manifest
    #[serde(skip)]
    pub cached_manifest: Option<SelfDescribingManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    /// Tool name (e.g., "simon", "cargo", "docker")
    pub name: String,
    /// Human-readable display name
    #[serde(default)]
    pub display_name: Option<String>,
    /// Tool description
    pub description: String,
    /// Version
    pub version: String,
    /// Author
    #[serde(default)]
    pub author: Option<String>,
    /// License
    #[serde(default)]
    pub license: Option<String>,
    /// Homepage
    #[serde(default)]
    pub homepage: Option<String>,
    /// Repository
    #[serde(default)]
    pub repository: Option<String>,
    /// Executable path (relative to tool root or absolute)
    pub executable: String,
    /// Categories for discovery
    #[serde(default)]
    pub categories: Vec<String>,
    /// Keywords for search
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// A capability/function exposed by the tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCapability {
    /// Capability name (e.g., "cpu_info", "memory_stats")
    pub name: String,
    /// Description for AI agents
    pub description: String,
    /// Command/subcommand to invoke
    pub command: String,
    /// Arguments template (supports placeholders)
    #[serde(default)]
    pub args: Vec<String>,
    /// Parameters schema for AI agents
    #[serde(default)]
    pub parameters: Vec<CapabilityParameter>,
    /// Output format
    #[serde(default = "default_output_format")]
    pub output_format: OutputFormat,
    /// Permission level required
    #[serde(default)]
    pub permission: PermissionLevel,
    /// Example usage
    #[serde(default)]
    pub examples: Vec<CapabilityExample>,
}

fn default_output_format() -> OutputFormat {
    OutputFormat::Json
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityParameter {
    /// Parameter name
    pub name: String,
    /// Description
    pub description: String,
    /// Type (string, number, boolean, array, object)
    #[serde(rename = "type")]
    pub param_type: String,
    /// Whether required
    #[serde(default)]
    pub required: bool,
    /// Default value
    #[serde(default)]
    pub default: Option<JsonValue>,
    /// Enum values (for constrained parameters)
    #[serde(default)]
    pub enum_values: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityExample {
    pub description: String,
    pub args: JsonValue,
    pub expected_output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Json,
    Text,
    Table,
    Csv,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    #[default]
    Read,
    Write,
    Execute,
    Admin,
}

/// Agent-specific settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentSettings {
    /// Whether AI agents can use this tool
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Maximum execution timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Rate limit (calls per minute)
    #[serde(default)]
    pub rate_limit: Option<u32>,
    /// Whether to capture stderr
    #[serde(default = "default_true")]
    pub capture_stderr: bool,
    /// Working directory for execution
    #[serde(default)]
    pub working_dir: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    30
}

// ============================================================================
// External Tool Registry
// ============================================================================

/// Registered external tool instance
#[derive(Debug, Clone)]
pub struct RegisteredTool {
    pub manifest: ExternalToolManifest,
    pub root_path: PathBuf,
    pub executable_path: PathBuf,
}

/// External tool invocation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalToolResult {
    pub tool: String,
    pub capability: String,
    pub success: bool,
    pub output: Option<JsonValue>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub exit_code: Option<i32>,
}

/// Global registry for external tools
pub struct ExternalToolRegistry {
    tools: RwLock<HashMap<String, RegisteredTool>>,
    search_paths: RwLock<Vec<PathBuf>>,
}

impl ExternalToolRegistry {
    /// Create a new external tool registry
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            search_paths: RwLock::new(vec![]),
        }
    }

    /// Add a search path for tool discovery
    pub fn add_search_path(&self, path: PathBuf) {
        if let Ok(mut paths) = self.search_paths.write() {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }

    /// Register a tool from a local path
    pub fn register_from_path(&self, tool_path: &Path) -> Result<String> {
        let manifest = Self::load_manifest(tool_path)?;
        let tool_name = manifest.tool.name.clone();

        let executable_path = if Path::new(&manifest.tool.executable).is_absolute() {
            PathBuf::from(&manifest.tool.executable)
        } else {
            tool_path.join(&manifest.tool.executable)
        };

        // Verify executable exists
        if !executable_path.exists() {
            // Try common binary locations
            let possible_paths = [
                tool_path
                    .join("target/release")
                    .join(&manifest.tool.executable),
                tool_path
                    .join("target/debug")
                    .join(&manifest.tool.executable),
                tool_path.join("bin").join(&manifest.tool.executable),
                #[cfg(windows)]
                tool_path
                    .join("target/release")
                    .join(format!("{}.exe", &manifest.tool.executable)),
                #[cfg(windows)]
                tool_path
                    .join("target/debug")
                    .join(format!("{}.exe", &manifest.tool.executable)),
            ];

            let found_path = possible_paths.iter().find(|p| p.exists());
            if let Some(found) = found_path {
                let registered = RegisteredTool {
                    manifest: manifest.clone(),
                    root_path: tool_path.to_path_buf(),
                    executable_path: found.clone(),
                };

                if let Ok(mut tools) = self.tools.write() {
                    tools.insert(tool_name.clone(), registered);
                }

                return Ok(tool_name);
            }

            return Err(anyhow!(
                "Executable not found: {} (searched in {})",
                manifest.tool.executable,
                tool_path.display()
            ));
        }

        let registered = RegisteredTool {
            manifest,
            root_path: tool_path.to_path_buf(),
            executable_path,
        };

        if let Ok(mut tools) = self.tools.write() {
            tools.insert(tool_name.clone(), registered);
        }

        Ok(tool_name)
    }

    /// Load manifest from tool directory
    fn load_manifest(tool_path: &Path) -> Result<ExternalToolManifest> {
        // Try different manifest file names
        let manifest_files = ["aethershell.toml", "tool.toml", ".aethershell.toml"];

        for filename in manifest_files {
            let manifest_path = tool_path.join(filename);
            if manifest_path.exists() {
                let content = std::fs::read_to_string(&manifest_path)
                    .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
                let manifest: ExternalToolManifest = toml::from_str(&content)
                    .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
                return Ok(manifest);
            }
        }

        // Try to infer from Cargo.toml for Rust projects
        let cargo_path = tool_path.join("Cargo.toml");
        if cargo_path.exists() {
            return Self::infer_from_cargo(&cargo_path, tool_path);
        }

        Err(anyhow!(
            "No tool manifest found in {} (tried: {:?})",
            tool_path.display(),
            manifest_files
        ))
    }

    /// Infer tool manifest from Cargo.toml
    fn infer_from_cargo(cargo_path: &Path, tool_path: &Path) -> Result<ExternalToolManifest> {
        let content = std::fs::read_to_string(cargo_path)?;
        let cargo: toml::Value = toml::from_str(&content)?;

        let package = cargo
            .get("package")
            .ok_or_else(|| anyhow!("No [package] section in Cargo.toml"))?;

        let name = package
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("No name in Cargo.toml"))?;

        let description = package
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("Rust application")
            .to_string();

        let version = package
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.1.0")
            .to_string();

        // Determine executable name
        let executable = if cfg!(windows) {
            format!("{}.exe", name)
        } else {
            name.to_string()
        };

        Ok(ExternalToolManifest {
            tool: ToolMetadata {
                name: name.to_string(),
                display_name: Some(name.to_string()),
                description,
                version,
                author: package.get("authors").and_then(|v| {
                    v.as_array()
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                }),
                license: package
                    .get("license")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                homepage: package
                    .get("homepage")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                repository: package
                    .get("repository")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                executable,
                categories: vec!["utility".to_string()],
                keywords: package
                    .get("keywords")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            capabilities: Self::infer_capabilities(tool_path, name),
            agent: AgentSettings::default(),
            self_describing: false,
            cached_manifest: None,
        })
    }

    /// Infer capabilities from a tool (via --help or common patterns)
    fn infer_capabilities(_tool_path: &Path, name: &str) -> Vec<ToolCapability> {
        let mut capabilities = Vec::new();

        // Default capability: run the tool with arguments
        capabilities.push(ToolCapability {
            name: "run".to_string(),
            description: format!("Execute {} with provided arguments", name),
            command: "".to_string(),
            args: vec![],
            parameters: vec![CapabilityParameter {
                name: "args".to_string(),
                description: "Command line arguments".to_string(),
                param_type: "array".to_string(),
                required: false,
                default: Some(json!([])),
                enum_values: None,
            }],
            output_format: OutputFormat::Text,
            permission: PermissionLevel::Execute,
            examples: vec![],
        });

        // Try to detect subcommands from help output
        // (We'll do this lazily on first use to avoid startup slowdown)

        capabilities
    }

    // ========================================================================
    // Self-Describing Tool Support
    // ========================================================================

    /// Register a self-describing tool (has `ai manifest` command)
    ///
    /// Self-describing tools like simon provide:
    /// - `tool ai manifest -f json` - Returns JSON schema of capabilities
    /// - CLI subcommands with `-f json` for JSON output
    pub fn register_self_describing(
        &self,
        executable_path: impl AsRef<Path>,
        name: Option<&str>,
    ) -> Result<String> {
        let exe_path = executable_path.as_ref();
        if !exe_path.exists() {
            return Err(anyhow!("Executable not found: {}", exe_path.display()));
        }

        // Load manifest from tool
        let manifest = Self::load_self_describing_manifest(exe_path)?;
        let tool_name = name
            .map(|s| s.to_string())
            .unwrap_or_else(|| manifest.name.to_lowercase().replace(' ', "_"));

        // Convert to our format
        let capabilities = Self::convert_self_describing_capabilities(&manifest);

        let external_manifest = ExternalToolManifest {
            tool: ToolMetadata {
                name: tool_name.clone(),
                display_name: Some(manifest.name.clone()),
                description: manifest.description.clone(),
                version: manifest
                    .version
                    .clone()
                    .unwrap_or_else(|| "0.0.0".to_string()),
                author: None,
                license: None,
                homepage: None,
                repository: None,
                executable: exe_path.to_string_lossy().to_string(),
                categories: manifest.capabilities.clone(),
                keywords: vec![],
            },
            capabilities,
            agent: AgentSettings::default(),
            self_describing: true,
            cached_manifest: Some(manifest),
        };

        let root_path = exe_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let registered = RegisteredTool {
            manifest: external_manifest,
            root_path,
            executable_path: exe_path.to_path_buf(),
        };

        if let Ok(mut tools) = self.tools.write() {
            tools.insert(tool_name.clone(), registered);
        }

        Ok(tool_name)
    }

    /// Check if executable supports `ai manifest` command
    pub fn is_self_describing(executable_path: &Path) -> bool {
        if !executable_path.exists() {
            return false;
        }
        Command::new(executable_path)
            .args(["ai", "manifest", "--help"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Load manifest from self-describing tool
    fn load_self_describing_manifest(exe_path: &Path) -> Result<SelfDescribingManifest> {
        let output = Command::new(exe_path)
            .args(["ai", "manifest", "-f", "json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("Failed to run ai manifest on {}", exe_path.display()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("ai manifest failed: {}", stderr.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str(&stdout).with_context(|| "Failed to parse AI manifest JSON")
    }

    /// Convert self-describing tools to ToolCapability format
    fn convert_self_describing_capabilities(
        manifest: &SelfDescribingManifest,
    ) -> Vec<ToolCapability> {
        manifest
            .tools
            .iter()
            .map(|tool| {
                // Map function name to CLI subcommand (e.g., get_cpu_info -> cli cpu)
                let cmd = Self::function_to_cli_command(&tool.name);

                ToolCapability {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    command: cmd,
                    args: vec!["-f".to_string(), "json".to_string()],
                    parameters: tool
                        .parameters
                        .properties
                        .iter()
                        .map(|(name, prop)| CapabilityParameter {
                            name: name.clone(),
                            description: prop.description.clone(),
                            param_type: prop.prop_type.clone(),
                            required: tool.parameters.required.contains(name),
                            default: prop.default.clone(),
                            enum_values: None,
                        })
                        .collect(),
                    output_format: OutputFormat::Json,
                    permission: PermissionLevel::Read,
                    examples: tool
                        .example
                        .iter()
                        .map(|ex| CapabilityExample {
                            description: ex.clone(),
                            args: json!({}),
                            expected_output: None,
                        })
                        .collect(),
                }
            })
            .collect()
    }

    /// Map function names to CLI subcommands
    /// e.g., get_cpu_info -> "cli cpu", get_gpu_status -> "cli gpu"
    fn function_to_cli_command(func_name: &str) -> String {
        // Common patterns: get_X_info, get_X_status, get_X_list -> cli X
        let name = func_name
            .strip_prefix("get_")
            .unwrap_or(func_name)
            .replace("_info", "")
            .replace("_status", "")
            .replace("_list", "")
            .replace("_summary", " all")
            .replace("_details", "");

        if name.contains("system") {
            "cli all".to_string()
        } else if name.contains("gpu") {
            "cli gpu".to_string()
        } else if name.contains("cpu") {
            "cli cpu".to_string()
        } else if name.contains("memory") {
            "cli memory".to_string()
        } else if name.contains("process") {
            "cli processes".to_string()
        } else if name.contains("temperature") || name.contains("temp") {
            "cli temperature".to_string()
        } else if name.contains("power") {
            "cli power".to_string()
        } else {
            format!("cli {}", name.split('_').next().unwrap_or(&name))
        }
    }

    /// Get self-describing manifest for a tool
    pub fn get_self_describing_manifest(&self, tool_name: &str) -> Option<SelfDescribingManifest> {
        self.get_tool(tool_name)
            .and_then(|t| t.manifest.cached_manifest)
    }

    /// Get manifest in specific provider format (OpenAI, Anthropic, etc.)
    pub fn get_manifest_for_provider(
        &self,
        tool_name: &str,
        provider: &str,
    ) -> Result<Option<JsonValue>> {
        let tool = self
            .get_tool(tool_name)
            .ok_or_else(|| anyhow!("Tool not found: {}", tool_name))?;

        if !tool.manifest.self_describing {
            return Ok(None);
        }

        let output = Command::new(&tool.executable_path)
            .args(["ai", "manifest", "-f", provider])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("Failed to get {} manifest", provider))?;

        if !output.status.success() {
            return Err(anyhow!("Manifest command failed"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Some(serde_json::from_str(&stdout)?))
    }

    /// List capabilities for a tool
    pub fn list_capabilities(&self, tool_name: &str) -> Result<Vec<(String, String)>> {
        let tool = self
            .get_tool(tool_name)
            .ok_or_else(|| anyhow!("Tool not found: {}", tool_name))?;

        Ok(tool
            .manifest
            .capabilities
            .iter()
            .map(|c| (c.name.clone(), c.description.clone()))
            .collect())
    }

    /// Get a registered tool
    pub fn get_tool(&self, name: &str) -> Option<RegisteredTool> {
        self.tools.read().ok()?.get(name).cloned()
    }

    /// List all registered tools
    pub fn list_tools(&self) -> Vec<String> {
        self.tools
            .read()
            .map(|tools| tools.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Execute a tool capability
    pub fn execute(
        &self,
        tool_name: &str,
        capability_name: &str,
        args: &JsonValue,
    ) -> Result<ExternalToolResult> {
        let tool = self
            .get_tool(tool_name)
            .ok_or_else(|| anyhow!("Tool not found: {}", tool_name))?;

        let capability = tool
            .manifest
            .capabilities
            .iter()
            .find(|c| c.name == capability_name)
            .ok_or_else(|| anyhow!("Capability not found: {}.{}", tool_name, capability_name))?;

        let start = Instant::now();

        // Build command
        let mut cmd = Command::new(&tool.executable_path);

        // Set working directory
        if let Some(ref wd) = tool.manifest.agent.working_dir {
            cmd.current_dir(wd);
        } else {
            cmd.current_dir(&tool.root_path);
        }

        // Add subcommand if specified
        if !capability.command.is_empty() {
            cmd.arg(&capability.command);
        }

        // Process arguments from the capability template and provided args
        let processed_args = Self::process_args(&capability.args, &capability.parameters, args)?;
        for arg in processed_args {
            cmd.arg(arg);
        }

        // Configure output capture
        cmd.stdout(Stdio::piped());
        if tool.manifest.agent.capture_stderr {
            cmd.stderr(Stdio::piped());
        }

        // Execute with timeout enforcement
        let timeout = Duration::from_secs(tool.manifest.agent.timeout_secs);
        let child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn {}", tool_name))?;

        // Watchdog thread enforces timeout by killing the process
        let child_id = child.id();
        let timed_out = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let timed_out_clone = timed_out.clone();
        let timeout_secs = tool.manifest.agent.timeout_secs;

        let _watchdog = std::thread::spawn(move || {
            std::thread::sleep(timeout);
            timed_out_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            #[cfg(windows)]
            {
                let _ = Command::new("taskkill")
                    .args(["/F", "/PID", &child_id.to_string()])
                    .output();
            }
            #[cfg(unix)]
            {
                unsafe {
                    libc::kill(child_id as i32, libc::SIGKILL);
                }
            }
        });

        let output = child
            .wait_with_output()
            .with_context(|| format!("Failed to execute {}", tool_name))?;

        // Check if watchdog killed it
        if timed_out.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(ExternalToolResult {
                tool: tool_name.to_string(),
                capability: capability_name.to_string(),
                success: false,
                output: None,
                error: Some(format!(
                    "External tool '{}' timed out after {}s",
                    tool_name, timeout_secs
                )),
                duration_ms: timeout.as_millis() as u64,
                exit_code: None,
            });
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        // Parse output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            // Try to parse as JSON if expected
            let parsed_output = match capability.output_format {
                OutputFormat::Json => serde_json::from_str(&stdout).unwrap_or(json!(stdout.trim())),
                _ => json!(stdout.trim()),
            };

            Ok(ExternalToolResult {
                tool: tool_name.to_string(),
                capability: capability_name.to_string(),
                success: true,
                output: Some(parsed_output),
                error: None,
                duration_ms,
                exit_code: output.status.code(),
            })
        } else {
            let error_msg = if !stderr.is_empty() {
                stderr.to_string()
            } else if !stdout.is_empty() {
                stdout.to_string()
            } else {
                format!("Command failed with exit code {:?}", output.status.code())
            };

            Ok(ExternalToolResult {
                tool: tool_name.to_string(),
                capability: capability_name.to_string(),
                success: false,
                output: None,
                error: Some(error_msg),
                duration_ms,
                exit_code: output.status.code(),
            })
        }
    }

    /// Process argument templates with provided values
    fn process_args(
        template_args: &[String],
        parameters: &[CapabilityParameter],
        provided: &JsonValue,
    ) -> Result<Vec<String>> {
        let mut result = Vec::new();

        // If provided is an array, use as positional arguments
        if let Some(arr) = provided.as_array() {
            for item in arr {
                match item {
                    JsonValue::String(s) => result.push(s.clone()),
                    JsonValue::Number(n) => result.push(n.to_string()),
                    JsonValue::Bool(b) => result.push(b.to_string()),
                    _ => result.push(item.to_string()),
                }
            }
            return Ok(result);
        }

        // If provided is an object, match to parameters
        if let Some(obj) = provided.as_object() {
            // First process template args with substitution
            for arg in template_args {
                let processed = Self::substitute_arg(arg, obj);
                result.push(processed);
            }

            // Then add remaining parameters as flags
            for param in parameters {
                if let Some(value) = obj.get(&param.name) {
                    // Skip if already in template
                    if template_args
                        .iter()
                        .any(|a| a.contains(&format!("{{{}}}", param.name)))
                    {
                        continue;
                    }

                    match value {
                        JsonValue::Bool(true) => {
                            result.push(format!("--{}", param.name));
                        }
                        JsonValue::Bool(false) => {}
                        JsonValue::String(s) => {
                            result.push(format!("--{}", param.name));
                            result.push(s.clone());
                        }
                        JsonValue::Number(n) => {
                            result.push(format!("--{}", param.name));
                            result.push(n.to_string());
                        }
                        JsonValue::Array(arr) => {
                            for item in arr {
                                result.push(format!("--{}", param.name));
                                match item {
                                    JsonValue::String(s) => result.push(s.clone()),
                                    _ => result.push(item.to_string()),
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(result)
    }

    /// Substitute {param} placeholders in argument template
    fn substitute_arg(template: &str, values: &serde_json::Map<String, JsonValue>) -> String {
        let mut result = template.to_string();
        for (key, value) in values {
            let placeholder = format!("{{{}}}", key);
            let replacement = match value {
                JsonValue::String(s) => s.clone(),
                JsonValue::Number(n) => n.to_string(),
                JsonValue::Bool(b) => b.to_string(),
                _ => value.to_string(),
            };
            result = result.replace(&placeholder, &replacement);
        }
        result
    }

    /// Discover and register tools from search paths
    pub fn discover(&self) -> Vec<String> {
        let mut discovered = Vec::new();

        let paths: Vec<PathBuf> = self
            .search_paths
            .read()
            .map(|p| p.clone())
            .unwrap_or_default();

        for search_path in paths {
            if !search_path.exists() {
                continue;
            }

            // Check if the search path itself is a tool
            if Self::is_tool_directory(&search_path) {
                if let Ok(name) = self.register_from_path(&search_path) {
                    discovered.push(name);
                }
                continue;
            }

            // Search subdirectories
            if let Ok(entries) = std::fs::read_dir(&search_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() && Self::is_tool_directory(&path) {
                        if let Ok(name) = self.register_from_path(&path) {
                            discovered.push(name);
                        }
                    }
                }
            }
        }

        discovered
    }

    /// Check if a directory contains a tool
    fn is_tool_directory(path: &Path) -> bool {
        let manifest_files = [
            "aethershell.toml",
            "tool.toml",
            ".aethershell.toml",
            "Cargo.toml",
        ];
        manifest_files.iter().any(|f| path.join(f).exists())
    }

    /// Convert tools to OpenAI function calling format
    pub fn to_openai_tools(&self) -> Vec<JsonValue> {
        let tools = self.tools.read();
        let tools = match tools {
            Ok(t) => t,
            Err(_) => return vec![],
        };

        let mut result = Vec::new();
        for (tool_name, tool) in tools.iter() {
            for capability in &tool.manifest.capabilities {
                let full_name = format!("{}_{}", tool_name, capability.name);
                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();

                for param in &capability.parameters {
                    let param_schema = json!({
                        "type": param.param_type,
                        "description": param.description,
                    });
                    properties.insert(param.name.clone(), param_schema);
                    if param.required {
                        required.push(param.name.clone());
                    }
                }

                result.push(json!({
                    "type": "function",
                    "function": {
                        "name": full_name,
                        "description": format!("[{}] {}", tool_name, capability.description),
                        "parameters": {
                            "type": "object",
                            "properties": properties,
                            "required": required,
                        }
                    }
                }));
            }
        }
        result
    }

    /// Convert tools to Anthropic tool format
    pub fn to_anthropic_tools(&self) -> Vec<JsonValue> {
        let tools = self.tools.read();
        let tools = match tools {
            Ok(t) => t,
            Err(_) => return vec![],
        };

        let mut result = Vec::new();
        for (tool_name, tool) in tools.iter() {
            for capability in &tool.manifest.capabilities {
                let full_name = format!("{}_{}", tool_name, capability.name);
                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();

                for param in &capability.parameters {
                    properties.insert(
                        param.name.clone(),
                        json!({
                            "type": param.param_type,
                            "description": param.description,
                        }),
                    );
                    if param.required {
                        required.push(param.name.clone());
                    }
                }

                result.push(json!({
                    "name": full_name,
                    "description": format!("[{}] {}", tool_name, capability.description),
                    "input_schema": {
                        "type": "object",
                        "properties": properties,
                        "required": required,
                    }
                }));
            }
        }
        result
    }
}

impl Default for ExternalToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Global Instance
// ============================================================================

lazy_static::lazy_static! {
    /// Global external tool registry
    pub static ref EXTERNAL_TOOLS: ExternalToolRegistry = {
        let registry = ExternalToolRegistry::new();

        // Add default search paths
        if let Some(home) = dirs::home_dir() {
            registry.add_search_path(home.join(".aethershell").join("tools"));
        }
        if let Some(data) = dirs::data_local_dir() {
            registry.add_search_path(data.join("aethershell").join("tools"));
        }

        registry
    };
}

// ============================================================================
// Agent API Integration
// ============================================================================

/// Execute an external tool call from an agent
pub fn execute_external_tool(tool_call: &str, args: &JsonValue) -> Result<ExternalToolResult> {
    // Parse tool_call as "tool_capability" format
    let parts: Vec<&str> = tool_call.splitn(2, '_').collect();
    if parts.len() != 2 {
        return Err(anyhow!(
            "Invalid tool call format: expected 'tool_capability', got '{}'",
            tool_call
        ));
    }

    let tool_name = parts[0];
    let capability_name = parts[1];

    EXTERNAL_TOOLS.execute(tool_name, capability_name, args)
}

/// Check if a tool call is for an external tool
pub fn is_external_tool(tool_name: &str) -> bool {
    let parts: Vec<&str> = tool_name.splitn(2, '_').collect();
    if parts.len() != 2 {
        return false;
    }
    EXTERNAL_TOOLS.get_tool(parts[0]).is_some()
}

/// Get combined tools (internal + external) for AI providers
pub fn get_all_agent_tools_openai() -> Vec<JsonValue> {
    let mut tools = crate::providers::tools::AETHER_TOOLS.to_openai_functions();
    tools.extend(EXTERNAL_TOOLS.to_openai_tools());
    tools
}

/// Get combined tools for Anthropic
pub fn get_all_agent_tools_anthropic() -> Vec<JsonValue> {
    let mut tools = crate::providers::tools::AETHER_TOOLS.to_anthropic_tools();
    tools.extend(EXTERNAL_TOOLS.to_anthropic_tools());
    tools
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = ExternalToolRegistry::new();
        assert!(registry.list_tools().is_empty());
    }

    #[test]
    fn test_process_args_array() {
        let args = json!(["--verbose", "-n", "5"]);
        let result = ExternalToolRegistry::process_args(&[], &[], &args).unwrap();
        assert_eq!(result, vec!["--verbose", "-n", "5"]);
    }

    #[test]
    fn test_process_args_object() {
        let params = vec![
            CapabilityParameter {
                name: "verbose".to_string(),
                description: "Enable verbose mode".to_string(),
                param_type: "boolean".to_string(),
                required: false,
                default: None,
                enum_values: None,
            },
            CapabilityParameter {
                name: "count".to_string(),
                description: "Number of items".to_string(),
                param_type: "number".to_string(),
                required: false,
                default: None,
                enum_values: None,
            },
        ];
        let args = json!({"verbose": true, "count": 5});
        let result = ExternalToolRegistry::process_args(&[], &params, &args).unwrap();
        assert!(result.contains(&"--verbose".to_string()));
        assert!(result.contains(&"--count".to_string()));
        assert!(result.contains(&"5".to_string()));
    }

    #[test]
    fn test_substitute_arg() {
        let values = serde_json::from_str::<serde_json::Map<String, JsonValue>>(
            r#"{"name": "test", "count": 5}"#,
        )
        .unwrap();

        assert_eq!(
            ExternalToolRegistry::substitute_arg("--name={name}", &values),
            "--name=test"
        );
        assert_eq!(
            ExternalToolRegistry::substitute_arg("-n {count}", &values),
            "-n 5"
        );
    }

    #[test]
    fn test_self_describing_manifest_parse() {
        let json_str = r#"{
            "name": "Test Tool",
            "description": "A test tool",
            "version": "1.0.0",
            "capabilities": ["monitoring"],
            "tools": [
                {
                    "name": "get_info",
                    "description": "Get information",
                    "category": "System",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "verbose": {
                                "type": "boolean",
                                "description": "Verbose output"
                            }
                        },
                        "required": []
                    }
                }
            ]
        }"#;

        let manifest: SelfDescribingManifest = serde_json::from_str(json_str).unwrap();
        assert_eq!(manifest.name, "Test Tool");
        assert_eq!(manifest.tools.len(), 1);
        assert_eq!(manifest.tools[0].name, "get_info");
        assert_eq!(manifest.capabilities, vec!["monitoring"]);
    }

    #[test]
    fn test_function_to_cli_command() {
        assert_eq!(
            ExternalToolRegistry::function_to_cli_command("get_cpu_info"),
            "cli cpu"
        );
        assert_eq!(
            ExternalToolRegistry::function_to_cli_command("get_gpu_status"),
            "cli gpu"
        );
        assert_eq!(
            ExternalToolRegistry::function_to_cli_command("get_system_summary"),
            "cli all"
        );
        assert_eq!(
            ExternalToolRegistry::function_to_cli_command("get_memory_info"),
            "cli memory"
        );
    }
}
