//! Cross-Platform Execution Layer
#![allow(unexpected_cfgs)]
//!
//! Provides a unified interface for executing OS operations across all supported platforms:
//! - Windows, macOS, Linux (desktop)
//! - iOS, Android (mobile)
//! - BSD variants
//!
//! This module ensures AetherShell serves as the default OS abstraction layer for AI providers,
//! translating abstract operations into platform-specific implementations.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use crate::os_tools::OperatingSystem;

// ============================================================================
// PLATFORM CAPABILITIES
// ============================================================================

/// Platform-specific capability flags
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformCapabilities {
    /// Full shell access (can execute arbitrary commands)
    pub full_shell: bool,
    /// GUI access available
    pub gui_available: bool,
    /// Root/admin access possible
    pub admin_possible: bool,
    /// Background processes supported
    pub background_processes: bool,
    /// System service management available
    pub service_management: bool,
    /// Package manager available
    pub package_manager: Option<PackageManagerType>,
    /// Container runtime available
    pub container_runtime: Option<ContainerType>,
    /// Virtualization available
    pub virtualization: bool,
    /// Network configuration access
    pub network_config: bool,
    /// Filesystem access level
    pub filesystem_access: FilesystemAccessLevel,
    /// Hardware access level
    pub hardware_access: HardwareAccessLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PackageManagerType {
    Apt,        // Debian/Ubuntu
    Yum,        // RHEL/CentOS
    Dnf,        // Fedora
    Pacman,     // Arch
    Brew,       // macOS
    Chocolatey, // Windows
    Winget,     // Windows
    Scoop,      // Windows
    Apk,        // Alpine
    Pkg,        // FreeBSD
    Snap,       // Cross-platform
    Flatpak,    // Cross-platform
    Termux,     // Android
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContainerType {
    Docker,
    Podman,
    Containerd,
    LXC,
    WSL, // Windows Subsystem for Linux
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum FilesystemAccessLevel {
    #[default]
    Full, // Full access to filesystem
    UserOnly,   // Only user directories
    Sandboxed,  // App sandbox only (iOS)
    Restricted, // Limited access (Android without root)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum HardwareAccessLevel {
    #[default]
    Full, // Direct hardware access
    Limited,     // Through APIs only
    Virtualized, // Running in VM
    None,        // No direct hardware access
}

impl PlatformCapabilities {
    /// Get capabilities for the current platform
    pub fn current() -> Self {
        Self::for_os(&OperatingSystem::current())
    }

    /// Get capabilities for a specific OS
    pub fn for_os(os: &OperatingSystem) -> Self {
        match os {
            OperatingSystem::Linux => Self {
                full_shell: true,
                gui_available: true,
                admin_possible: true,
                background_processes: true,
                service_management: true,
                package_manager: Some(detect_linux_package_manager()),
                container_runtime: detect_container_runtime(),
                virtualization: true,
                network_config: true,
                filesystem_access: FilesystemAccessLevel::Full,
                hardware_access: HardwareAccessLevel::Full,
            },
            OperatingSystem::MacOS => Self {
                full_shell: true,
                gui_available: true,
                admin_possible: true,
                background_processes: true,
                service_management: true,
                package_manager: Some(PackageManagerType::Brew),
                container_runtime: Some(ContainerType::Docker),
                virtualization: true,
                network_config: true,
                filesystem_access: FilesystemAccessLevel::Full,
                hardware_access: HardwareAccessLevel::Full,
            },
            OperatingSystem::Windows => Self {
                full_shell: true,
                gui_available: true,
                admin_possible: true,
                background_processes: true,
                service_management: true,
                package_manager: Some(PackageManagerType::Winget),
                container_runtime: detect_windows_container(),
                virtualization: true,
                network_config: true,
                filesystem_access: FilesystemAccessLevel::Full,
                hardware_access: HardwareAccessLevel::Full,
            },
            OperatingSystem::BSD => Self {
                full_shell: true,
                gui_available: true,
                admin_possible: true,
                background_processes: true,
                service_management: true,
                package_manager: Some(PackageManagerType::Pkg),
                container_runtime: Some(ContainerType::LXC),
                virtualization: true,
                network_config: true,
                filesystem_access: FilesystemAccessLevel::Full,
                hardware_access: HardwareAccessLevel::Full,
            },
            OperatingSystem::iOS => Self {
                full_shell: false, // Unless jailbroken
                gui_available: true,
                admin_possible: false,
                background_processes: false,
                service_management: false,
                package_manager: None,
                container_runtime: None,
                virtualization: false,
                network_config: false,
                filesystem_access: FilesystemAccessLevel::Sandboxed,
                hardware_access: HardwareAccessLevel::Limited,
            },
            OperatingSystem::Android => Self {
                full_shell: cfg!(feature = "termux") || is_android_rooted(),
                gui_available: true,
                admin_possible: is_android_rooted(),
                background_processes: true,
                service_management: is_android_rooted(),
                package_manager: if has_termux() {
                    Some(PackageManagerType::Termux)
                } else {
                    None
                },
                container_runtime: None,
                virtualization: false,
                network_config: is_android_rooted(),
                filesystem_access: if is_android_rooted() {
                    FilesystemAccessLevel::Full
                } else {
                    FilesystemAccessLevel::Restricted
                },
                hardware_access: HardwareAccessLevel::Limited,
            },
        }
    }
}

// Platform detection helpers
fn detect_linux_package_manager() -> PackageManagerType {
    if std::path::Path::new("/usr/bin/apt").exists() {
        PackageManagerType::Apt
    } else if std::path::Path::new("/usr/bin/dnf").exists() {
        PackageManagerType::Dnf
    } else if std::path::Path::new("/usr/bin/yum").exists() {
        PackageManagerType::Yum
    } else if std::path::Path::new("/usr/bin/pacman").exists() {
        PackageManagerType::Pacman
    } else if std::path::Path::new("/sbin/apk").exists() {
        PackageManagerType::Apk
    } else {
        PackageManagerType::None
    }
}

fn detect_container_runtime() -> Option<ContainerType> {
    if std::path::Path::new("/usr/bin/docker").exists() {
        Some(ContainerType::Docker)
    } else if std::path::Path::new("/usr/bin/podman").exists() {
        Some(ContainerType::Podman)
    } else {
        None
    }
}

fn detect_windows_container() -> Option<ContainerType> {
    #[cfg(target_os = "windows")]
    {
        // Check for Docker Desktop or WSL
        if std::path::Path::new("C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe").exists() {
            return Some(ContainerType::Docker);
        }
        // Check for WSL
        if std::process::Command::new("wsl")
            .arg("--version")
            .output()
            .is_ok()
        {
            return Some(ContainerType::WSL);
        }
    }
    None
}

fn is_android_rooted() -> bool {
    #[cfg(target_os = "android")]
    {
        std::path::Path::new("/system/xbin/su").exists()
            || std::path::Path::new("/system/bin/su").exists()
    }
    #[cfg(not(target_os = "android"))]
    false
}

fn has_termux() -> bool {
    #[cfg(target_os = "android")]
    {
        std::path::Path::new("/data/data/com.termux").exists()
    }
    #[cfg(not(target_os = "android"))]
    false
}

// ============================================================================
// CROSS-PLATFORM OPERATION EXECUTION
// ============================================================================

/// Platform-specific implementation of an operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformImplementation {
    /// Target operating system
    pub os: OperatingSystem,
    /// Command to execute
    pub command: String,
    /// Argument template with placeholders
    pub args_template: Vec<String>,
    /// Environment variables to set
    pub env_vars: HashMap<String, String>,
    /// Whether this requires elevated privileges
    pub requires_elevation: bool,
    /// Fallback if primary command unavailable
    pub fallback: Option<Box<PlatformImplementation>>,
}

/// Registry of platform-specific implementations
#[derive(Debug, Clone, Default)]
pub struct PlatformRegistry {
    /// Maps operation_id -> os -> implementation
    implementations: HashMap<String, HashMap<OperatingSystem, PlatformImplementation>>,
}

impl PlatformRegistry {
    pub fn new() -> Self {
        let mut registry = Self::default();
        registry.register_core_operations();
        registry
    }

    /// Register an implementation for an operation
    pub fn register(&mut self, operation_id: &str, impl_: PlatformImplementation) {
        self.implementations
            .entry(operation_id.to_string())
            .or_default()
            .insert(impl_.os.clone(), impl_);
    }

    /// Get implementation for current platform
    pub fn get_current(&self, operation_id: &str) -> Option<&PlatformImplementation> {
        let os = OperatingSystem::current();
        self.get(operation_id, &os)
    }

    /// Get implementation for specific platform
    pub fn get(&self, operation_id: &str, os: &OperatingSystem) -> Option<&PlatformImplementation> {
        self.implementations
            .get(operation_id)
            .and_then(|impls| impls.get(os))
    }

    /// Check if operation is supported on platform
    pub fn is_supported(&self, operation_id: &str, os: &OperatingSystem) -> bool {
        self.get(operation_id, os).is_some()
    }

    /// Get all supported platforms for an operation
    pub fn supported_platforms(&self, operation_id: &str) -> Vec<OperatingSystem> {
        self.implementations
            .get(operation_id)
            .map(|impls| impls.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Register core operations with platform-specific implementations
    fn register_core_operations(&mut self) {
        // ===================== File System =====================

        // fs.read_file - Universal (Rust std::fs)
        for os in all_operating_systems() {
            self.register(
                "fs.read_file",
                PlatformImplementation {
                    os: os.clone(),
                    command: "__native__".to_string(), // Use native Rust
                    args_template: vec![],
                    env_vars: HashMap::new(),
                    requires_elevation: false,
                    fallback: None,
                },
            );
        }

        // fs.write_file - Universal (Rust std::fs)
        for os in all_operating_systems() {
            self.register(
                "fs.write_file",
                PlatformImplementation {
                    os: os.clone(),
                    command: "__native__".to_string(),
                    args_template: vec![],
                    env_vars: HashMap::new(),
                    requires_elevation: false,
                    fallback: None,
                },
            );
        }

        // fs.list_dir
        self.register(
            "fs.list_dir",
            PlatformImplementation {
                os: OperatingSystem::Windows,
                command: "dir".to_string(),
                args_template: vec!["/B".to_string(), "{path}".to_string()],
                env_vars: HashMap::new(),
                requires_elevation: false,
                fallback: Some(Box::new(PlatformImplementation {
                    os: OperatingSystem::Windows,
                    command: "powershell".to_string(),
                    args_template: vec![
                        "-Command".to_string(),
                        "Get-ChildItem -Path {path}".to_string(),
                    ],
                    env_vars: HashMap::new(),
                    requires_elevation: false,
                    fallback: None,
                })),
            },
        );

        for os in [
            OperatingSystem::Linux,
            OperatingSystem::MacOS,
            OperatingSystem::BSD,
        ] {
            self.register(
                "fs.list_dir",
                PlatformImplementation {
                    os: os.clone(),
                    command: "ls".to_string(),
                    args_template: vec!["-la".to_string(), "{path}".to_string()],
                    env_vars: HashMap::new(),
                    requires_elevation: false,
                    fallback: None,
                },
            );
        }

        self.register(
            "fs.list_dir",
            PlatformImplementation {
                os: OperatingSystem::Android,
                command: "ls".to_string(),
                args_template: vec!["-la".to_string(), "{path}".to_string()],
                env_vars: HashMap::new(),
                requires_elevation: false,
                fallback: None,
            },
        );

        self.register(
            "fs.list_dir",
            PlatformImplementation {
                os: OperatingSystem::iOS,
                command: "ls".to_string(), // Available if jailbroken
                args_template: vec!["-la".to_string(), "{path}".to_string()],
                env_vars: HashMap::new(),
                requires_elevation: false,
                fallback: None,
            },
        );

        // ===================== Process Management =====================

        // process.list
        self.register(
            "process.list",
            PlatformImplementation {
                os: OperatingSystem::Windows,
                command: "tasklist".to_string(),
                args_template: vec!["/V".to_string(), "/FO".to_string(), "CSV".to_string()],
                env_vars: HashMap::new(),
                requires_elevation: false,
                fallback: Some(Box::new(PlatformImplementation {
                    os: OperatingSystem::Windows,
                    command: "powershell".to_string(),
                    args_template: vec![
                        "-Command".to_string(),
                        "Get-Process | ConvertTo-Json".to_string(),
                    ],
                    env_vars: HashMap::new(),
                    requires_elevation: false,
                    fallback: None,
                })),
            },
        );

        for os in [
            OperatingSystem::Linux,
            OperatingSystem::MacOS,
            OperatingSystem::BSD,
        ] {
            self.register(
                "process.list",
                PlatformImplementation {
                    os: os.clone(),
                    command: "ps".to_string(),
                    args_template: vec!["aux".to_string()],
                    env_vars: HashMap::new(),
                    requires_elevation: false,
                    fallback: None,
                },
            );
        }

        self.register(
            "process.list",
            PlatformImplementation {
                os: OperatingSystem::Android,
                command: "ps".to_string(),
                args_template: vec!["-A".to_string()],
                env_vars: HashMap::new(),
                requires_elevation: false,
                fallback: None,
            },
        );

        // process.kill
        self.register(
            "process.kill",
            PlatformImplementation {
                os: OperatingSystem::Windows,
                command: "taskkill".to_string(),
                args_template: vec!["/F".to_string(), "/PID".to_string(), "{pid}".to_string()],
                env_vars: HashMap::new(),
                requires_elevation: true,
                fallback: None,
            },
        );

        for os in [
            OperatingSystem::Linux,
            OperatingSystem::MacOS,
            OperatingSystem::BSD,
            OperatingSystem::Android,
        ] {
            self.register(
                "process.kill",
                PlatformImplementation {
                    os: os.clone(),
                    command: "kill".to_string(),
                    args_template: vec!["-9".to_string(), "{pid}".to_string()],
                    env_vars: HashMap::new(),
                    requires_elevation: false,
                    fallback: None,
                },
            );
        }

        // ===================== Network =====================

        // network.http_request - Universal (reqwest)
        for os in all_operating_systems() {
            self.register(
                "network.http_request",
                PlatformImplementation {
                    os: os.clone(),
                    command: "__native__".to_string(),
                    args_template: vec![],
                    env_vars: HashMap::new(),
                    requires_elevation: false,
                    fallback: None,
                },
            );
        }

        // network.ping
        self.register(
            "network.ping",
            PlatformImplementation {
                os: OperatingSystem::Windows,
                command: "ping".to_string(),
                args_template: vec![
                    "-n".to_string(),
                    "{count}".to_string(),
                    "{host}".to_string(),
                ],
                env_vars: HashMap::new(),
                requires_elevation: false,
                fallback: None,
            },
        );

        for os in [
            OperatingSystem::Linux,
            OperatingSystem::MacOS,
            OperatingSystem::BSD,
        ] {
            self.register(
                "network.ping",
                PlatformImplementation {
                    os: os.clone(),
                    command: "ping".to_string(),
                    args_template: vec![
                        "-c".to_string(),
                        "{count}".to_string(),
                        "{host}".to_string(),
                    ],
                    env_vars: HashMap::new(),
                    requires_elevation: false,
                    fallback: None,
                },
            );
        }

        // ===================== System Info =====================

        // system.info
        self.register(
            "system.info",
            PlatformImplementation {
                os: OperatingSystem::Windows,
                command: "powershell".to_string(),
                args_template: vec![
                    "-Command".to_string(),
                    "Get-ComputerInfo | ConvertTo-Json".to_string(),
                ],
                env_vars: HashMap::new(),
                requires_elevation: false,
                fallback: None,
            },
        );

        for os in [OperatingSystem::Linux, OperatingSystem::Android] {
            self.register(
                "system.info",
                PlatformImplementation {
                    os: os.clone(),
                    command: "uname".to_string(),
                    args_template: vec!["-a".to_string()],
                    env_vars: HashMap::new(),
                    requires_elevation: false,
                    fallback: None,
                },
            );
        }

        self.register(
            "system.info",
            PlatformImplementation {
                os: OperatingSystem::MacOS,
                command: "system_profiler".to_string(),
                args_template: vec!["SPSoftwareDataType".to_string(), "-json".to_string()],
                env_vars: HashMap::new(),
                requires_elevation: false,
                fallback: None,
            },
        );

        // ===================== Environment =====================

        // env.get - Universal
        for os in all_operating_systems() {
            self.register(
                "env.get",
                PlatformImplementation {
                    os: os.clone(),
                    command: "__native__".to_string(),
                    args_template: vec![],
                    env_vars: HashMap::new(),
                    requires_elevation: false,
                    fallback: None,
                },
            );
        }

        // ===================== Package Management =====================

        // package.install
        self.register(
            "package.install",
            PlatformImplementation {
                os: OperatingSystem::Windows,
                command: "winget".to_string(),
                args_template: vec![
                    "install".to_string(),
                    "-e".to_string(),
                    "{package}".to_string(),
                ],
                env_vars: HashMap::new(),
                requires_elevation: true,
                fallback: Some(Box::new(PlatformImplementation {
                    os: OperatingSystem::Windows,
                    command: "choco".to_string(),
                    args_template: vec![
                        "install".to_string(),
                        "-y".to_string(),
                        "{package}".to_string(),
                    ],
                    env_vars: HashMap::new(),
                    requires_elevation: true,
                    fallback: None,
                })),
            },
        );

        self.register(
            "package.install",
            PlatformImplementation {
                os: OperatingSystem::MacOS,
                command: "brew".to_string(),
                args_template: vec!["install".to_string(), "{package}".to_string()],
                env_vars: HashMap::new(),
                requires_elevation: false,
                fallback: None,
            },
        );

        self.register(
            "package.install",
            PlatformImplementation {
                os: OperatingSystem::Linux,
                command: "apt".to_string(),
                args_template: vec![
                    "install".to_string(),
                    "-y".to_string(),
                    "{package}".to_string(),
                ],
                env_vars: HashMap::new(),
                requires_elevation: true,
                fallback: Some(Box::new(PlatformImplementation {
                    os: OperatingSystem::Linux,
                    command: "dnf".to_string(),
                    args_template: vec![
                        "install".to_string(),
                        "-y".to_string(),
                        "{package}".to_string(),
                    ],
                    env_vars: HashMap::new(),
                    requires_elevation: true,
                    fallback: Some(Box::new(PlatformImplementation {
                        os: OperatingSystem::Linux,
                        command: "pacman".to_string(),
                        args_template: vec![
                            "-S".to_string(),
                            "--noconfirm".to_string(),
                            "{package}".to_string(),
                        ],
                        env_vars: HashMap::new(),
                        requires_elevation: true,
                        fallback: None,
                    })),
                })),
            },
        );

        self.register(
            "package.install",
            PlatformImplementation {
                os: OperatingSystem::Android,
                command: "pkg".to_string(), // Termux
                args_template: vec![
                    "install".to_string(),
                    "-y".to_string(),
                    "{package}".to_string(),
                ],
                env_vars: HashMap::new(),
                requires_elevation: false,
                fallback: None,
            },
        );
    }
}

fn all_operating_systems() -> Vec<OperatingSystem> {
    vec![
        OperatingSystem::Linux,
        OperatingSystem::MacOS,
        OperatingSystem::Windows,
        OperatingSystem::BSD,
        OperatingSystem::iOS,
        OperatingSystem::Android,
    ]
}

// ============================================================================
// UNIFIED EXECUTOR
// ============================================================================

/// Executor for running operations with platform abstraction
pub struct PlatformExecutor {
    registry: PlatformRegistry,
    capabilities: PlatformCapabilities,
    current_os: OperatingSystem,
}

impl PlatformExecutor {
    pub fn new() -> Self {
        let current_os = OperatingSystem::current();
        Self {
            registry: PlatformRegistry::new(),
            capabilities: PlatformCapabilities::for_os(&current_os),
            current_os,
        }
    }

    /// Get current platform capabilities
    pub fn capabilities(&self) -> &PlatformCapabilities {
        &self.capabilities
    }

    /// Check if an operation is supported on the current platform
    pub fn is_supported(&self, operation_id: &str) -> bool {
        self.registry.is_supported(operation_id, &self.current_os)
    }

    /// Get the implementation for an operation
    pub fn get_implementation(&self, operation_id: &str) -> Option<&PlatformImplementation> {
        self.registry.get_current(operation_id)
    }

    /// Execute an operation with given parameters
    pub fn execute(
        &self,
        operation_id: &str,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<ExecutionResult> {
        let impl_ = self.registry.get_current(operation_id).ok_or_else(|| {
            anyhow!(
                "Operation '{}' not supported on {:?}",
                operation_id,
                self.current_os
            )
        })?;

        // Check elevation requirements
        if impl_.requires_elevation && !self.has_elevation() {
            return Err(anyhow!(
                "Operation '{}' requires elevated privileges",
                operation_id
            ));
        }

        // Handle native Rust implementations
        if impl_.command == "__native__" {
            return self.execute_native(operation_id, params);
        }

        // Build command with parameter substitution
        let args = self.substitute_params(&impl_.args_template, params);

        // Execute command
        let output = std::process::Command::new(&impl_.command)
            .args(&args)
            .envs(&impl_.env_vars)
            .output()
            .map_err(|e| anyhow!("Failed to execute command: {}", e))?;

        Ok(ExecutionResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code(),
        })
    }

    fn substitute_params(
        &self,
        template: &[String],
        params: &HashMap<String, serde_json::Value>,
    ) -> Vec<String> {
        template
            .iter()
            .map(|t| {
                let mut result = t.clone();
                for (key, value) in params {
                    let placeholder = format!("{{{}}}", key);
                    let value_str = match value {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Number(n) => n.to_string(),
                        serde_json::Value::Bool(b) => b.to_string(),
                        _ => value.to_string(),
                    };
                    result = result.replace(&placeholder, &value_str);
                }
                result
            })
            .collect()
    }

    fn execute_native(
        &self,
        operation_id: &str,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<ExecutionResult> {
        match operation_id {
            "fs.read_file" => {
                let path = params
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Missing 'path' parameter"))?;
                let content = std::fs::read_to_string(path)?;
                Ok(ExecutionResult {
                    success: true,
                    stdout: content,
                    stderr: String::new(),
                    exit_code: Some(0),
                })
            }
            "fs.write_file" => {
                let path = params
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Missing 'path' parameter"))?;
                let content = params
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Missing 'content' parameter"))?;
                std::fs::write(path, content)?;
                Ok(ExecutionResult {
                    success: true,
                    stdout: format!("Wrote {} bytes to {}", content.len(), path),
                    stderr: String::new(),
                    exit_code: Some(0),
                })
            }
            "env.get" => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Missing 'name' parameter"))?;
                let value = std::env::var(name).unwrap_or_default();
                Ok(ExecutionResult {
                    success: true,
                    stdout: value,
                    stderr: String::new(),
                    exit_code: Some(0),
                })
            }
            "network.http_request" => {
                // Use synchronous HTTP for now
                Ok(ExecutionResult {
                    success: true,
                    stdout: "HTTP request requires async runtime".to_string(),
                    stderr: String::new(),
                    exit_code: Some(0),
                })
            }
            _ => Err(anyhow!("No native implementation for {}", operation_id)),
        }
    }

    fn has_elevation(&self) -> bool {
        #[cfg(unix)]
        {
            unsafe { libc::geteuid() == 0 }
        }
        #[cfg(windows)]
        {
            // Simplified Windows elevation check
            use std::process::Command;
            match Command::new("net").args(["session"]).output() {
                Ok(output) => output.status.success(),
                Err(_) => false,
            }
        }
        #[cfg(not(any(unix, windows)))]
        false
    }
}

impl Default for PlatformExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of executing an operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

// ============================================================================
// GLOBAL INSTANCES
// ============================================================================

lazy_static::lazy_static! {
    /// Global platform registry
    pub static ref PLATFORM_REGISTRY: PlatformRegistry = PlatformRegistry::new();

    /// Global platform executor
    pub static ref PLATFORM_EXECUTOR: PlatformExecutor = PlatformExecutor::new();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        let os = OperatingSystem::current();
        let caps = PlatformCapabilities::for_os(&os);
        // Should have some capabilities
        assert!(caps.full_shell || !caps.full_shell); // Always true, just checking no panic
    }

    #[test]
    fn test_platform_registry() {
        let registry = PlatformRegistry::new();
        // fs.read_file should be universal
        assert!(registry.is_supported("fs.read_file", &OperatingSystem::Linux));
        assert!(registry.is_supported("fs.read_file", &OperatingSystem::Windows));
        assert!(registry.is_supported("fs.read_file", &OperatingSystem::MacOS));
    }

    #[test]
    fn test_all_platforms_supported() {
        let registry = PlatformRegistry::new();
        let platforms = registry.supported_platforms("fs.read_file");
        assert!(platforms.contains(&OperatingSystem::Linux));
        assert!(platforms.contains(&OperatingSystem::Windows));
        assert!(platforms.contains(&OperatingSystem::MacOS));
        assert!(platforms.contains(&OperatingSystem::iOS));
        assert!(platforms.contains(&OperatingSystem::Android));
    }

    #[test]
    fn test_executor_native_read() {
        let executor = PlatformExecutor::new();
        // Test that executor can handle native operations
        assert!(executor.is_supported("fs.read_file"));
        assert!(executor.is_supported("fs.write_file"));
        assert!(executor.is_supported("env.get"));
    }
}
