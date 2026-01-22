//! AetherShell Package Management
//!
//! This module provides package management capabilities including:
//! - Local module imports from `.ae` files
//! - Package registry integration
//! - Dependency resolution
//! - Version management
//!
//! ## Import Syntax
//! ```aethershell
//! # Import entire module
//! import "path/to/module.ae"
//! import "path/to/module.ae" as mymod
//!
//! # Import specific items
//! import { func1, func2 } from "path/to/module.ae"
//! import { func1 as f1, func2 } from "path/to/module.ae"
//!
//! # Import from package registry
//! import "pkg:math-utils@1.0"
//! import { sin, cos } from "pkg:math-utils"
//! ```

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast::{ImportItem, Stmt};
use crate::env::Env;
use crate::parser::parse_program;
use crate::value::Value;

/// Package manifest (aether.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub package: PackageInfo,
    #[serde(default)]
    pub dependencies: HashMap<String, Dependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    /// Main entry point (default: "main.ae")
    #[serde(default = "default_main")]
    pub main: String,
}

fn default_main() -> String {
    "main.ae".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    /// Simple version string: "1.0"
    Version(String),
    /// Detailed dependency spec
    Detailed(DependencySpec),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencySpec {
    pub version: String,
    #[serde(default)]
    pub git: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub optional: bool,
}

/// Module cache for avoiding re-evaluation
#[derive(Debug, Default)]
pub struct ModuleCache {
    /// Cached module environments by absolute path
    modules: HashMap<PathBuf, Env>,
    /// Modules currently being loaded (for cycle detection)
    loading: Vec<PathBuf>,
}

impl ModuleCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a module is cached
    pub fn get(&self, path: &Path) -> Option<&Env> {
        self.modules.get(path)
    }

    /// Cache a module's environment
    pub fn insert(&mut self, path: PathBuf, env: Env) {
        self.modules.insert(path, env);
    }

    /// Check for import cycles
    pub fn check_cycle(&self, path: &Path) -> Result<()> {
        if self.loading.contains(&path.to_path_buf()) {
            let cycle: Vec<_> = self
                .loading
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            return Err(anyhow!(
                "circular import detected: {} -> {}",
                cycle.join(" -> "),
                path.display()
            ));
        }
        Ok(())
    }

    /// Mark a module as currently loading
    pub fn start_loading(&mut self, path: PathBuf) {
        self.loading.push(path);
    }

    /// Mark a module as finished loading
    pub fn finish_loading(&mut self) {
        self.loading.pop();
    }
}

/// Package registry client
#[derive(Debug, Clone)]
pub struct PackageRegistry {
    /// Registry URL
    pub url: String,
    /// Local cache directory
    pub cache_dir: PathBuf,
}

impl Default for PackageRegistry {
    fn default() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("aethershell")
            .join("packages");

        Self {
            url: "https://packages.nervosys.ai".to_string(),
            cache_dir,
        }
    }
}

impl PackageRegistry {
    /// Resolve a package name to a local path
    pub fn resolve(&self, package: &str) -> Result<PathBuf> {
        // Parse package spec: "name" or "name@version"
        let (name, version) = if let Some(idx) = package.find('@') {
            (&package[..idx], Some(&package[idx + 1..]))
        } else {
            (package, None)
        };

        // Check local cache first
        let pkg_dir = self.cache_dir.join(name);
        if let Some(ver) = version {
            let versioned_dir = pkg_dir.join(ver);
            if versioned_dir.exists() {
                return Ok(versioned_dir);
            }
        } else if pkg_dir.exists() {
            // Find latest version
            if let Some(latest) = self.find_latest_version(&pkg_dir)? {
                return Ok(pkg_dir.join(latest));
            }
        }

        // Package not found locally - would need to fetch from registry
        Err(anyhow!(
            "package '{}' not found. Install with: ae pkg install {}",
            package,
            package
        ))
    }

    /// Find the latest version in a package directory
    fn find_latest_version(&self, pkg_dir: &Path) -> Result<Option<String>> {
        let mut versions: Vec<semver::Version> = Vec::new();

        for entry in fs::read_dir(pkg_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Ok(ver) = semver::Version::parse(&entry.file_name().to_string_lossy()) {
                    versions.push(ver);
                }
            }
        }

        versions.sort();
        Ok(versions.last().map(|v| v.to_string()))
    }

    /// List installed packages
    pub fn list_installed(&self) -> Result<Vec<(String, Vec<String>)>> {
        let mut packages = Vec::new();

        if !self.cache_dir.exists() {
            return Ok(packages);
        }

        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                let mut versions = Vec::new();

                for ver_entry in fs::read_dir(entry.path())? {
                    let ver_entry = ver_entry?;
                    if ver_entry.file_type()?.is_dir() {
                        versions.push(ver_entry.file_name().to_string_lossy().to_string());
                    }
                }

                if !versions.is_empty() {
                    packages.push((name, versions));
                }
            }
        }

        Ok(packages)
    }
}

/// Import resolver
pub struct ImportResolver {
    /// Current working directory for relative imports
    pub cwd: PathBuf,
    /// Module cache
    pub cache: ModuleCache,
    /// Package registry
    pub registry: PackageRegistry,
    /// Search paths for modules
    pub search_paths: Vec<PathBuf>,
}

impl Default for ImportResolver {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            cache: ModuleCache::new(),
            registry: PackageRegistry::default(),
            search_paths: vec![],
        }
    }
}

impl ImportResolver {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            ..Default::default()
        }
    }

    /// Add a search path for modules
    pub fn add_search_path(&mut self, path: PathBuf) {
        if !self.search_paths.contains(&path) {
            self.search_paths.push(path);
        }
    }

    /// Resolve an import source to an absolute file path
    pub fn resolve_path(&self, source: &str) -> Result<PathBuf> {
        // Check for package reference: "pkg:name" or "pkg:name@version"
        if let Some(pkg_spec) = source.strip_prefix("pkg:") {
            return self.registry.resolve(pkg_spec);
        }

        // Relative or absolute path
        let source_path = Path::new(source);

        // If absolute, use directly
        if source_path.is_absolute() {
            if source_path.exists() {
                return Ok(source_path.to_path_buf());
            }
            return Err(anyhow!("module not found: {}", source));
        }

        // Try relative to cwd
        let relative_path = self.cwd.join(source_path);
        if relative_path.exists() {
            return Ok(relative_path.canonicalize()?);
        }

        // Try with .ae extension
        let with_ext = relative_path.with_extension("ae");
        if with_ext.exists() {
            return Ok(with_ext.canonicalize()?);
        }

        // Try search paths
        for search_path in &self.search_paths {
            let try_path = search_path.join(source_path);
            if try_path.exists() {
                return Ok(try_path.canonicalize()?);
            }
            let with_ext = try_path.with_extension("ae");
            if with_ext.exists() {
                return Ok(with_ext.canonicalize()?);
            }
        }

        Err(anyhow!("module not found: {}", source))
    }

    /// Load and evaluate a module, returning its environment
    pub fn load_module(
        &mut self,
        source: &str,
        eval_fn: impl Fn(&[Stmt], &mut Env) -> Result<Value>,
    ) -> Result<Env> {
        let path = self.resolve_path(source)?;

        // Check cache
        if let Some(env) = self.cache.get(&path) {
            return Ok(env.clone());
        }

        // Check for cycles
        self.cache.check_cycle(&path)?;

        // Mark as loading
        self.cache.start_loading(path.clone());

        // Read and parse
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read module: {}", path.display()))?;

        let stmts = parse_program(&content)
            .with_context(|| format!("failed to parse module: {}", path.display()))?;

        // Create module environment and evaluate
        let mut module_env = Env::default();
        eval_fn(&stmts, &mut module_env)?;

        // Finish loading and cache
        self.cache.finish_loading();
        self.cache.insert(path, module_env.clone());

        Ok(module_env)
    }

    /// Process an import statement
    ///
    /// Only public (exported) items from the module are importable.
    /// Private items cannot be accessed from outside the module.
    pub fn process_import(
        &mut self,
        items: &[ImportItem],
        source: &str,
        alias: &Option<String>,
        target_env: &mut Env,
        eval_fn: impl Fn(&[Stmt], &mut Env) -> Result<Value>,
    ) -> Result<()> {
        let module_env = self.load_module(source, eval_fn)?;

        if items.is_empty() {
            // Import entire module
            if let Some(alias_name) = alias {
                // import "path" as name -> Create a record with exported items only
                let exports = module_env.exported_vars();
                target_env.set_var_unchecked(alias_name.clone(), Value::Record(exports));
            } else {
                // import "path" -> Import all exported names into current scope
                let exports = module_env.exported_vars();
                for (name, value) in exports {
                    target_env.set_var_unchecked(name, value);
                }
            }
        } else {
            // Import specific items - must be exported
            for item in items {
                // Check if the item exists in the module
                let value = module_env
                    .get_var(&item.name)
                    .cloned()
                    .ok_or_else(|| anyhow!("'{}' not found in module '{}'", item.name, source))?;

                // Check if the item is exported (public)
                if !module_env.is_exported(&item.name) {
                    return Err(anyhow!(
                        "'{}' is private in module '{}'. Only `pub` items can be imported.",
                        item.name,
                        source
                    ));
                }

                let target_name = item.alias.as_ref().unwrap_or(&item.name);
                target_env.set_var_unchecked(target_name.clone(), value);
            }
        }

        Ok(())
    }
}

/// Parse dependency version requirement
pub fn parse_version_req(req: &str) -> Result<semver::VersionReq> {
    semver::VersionReq::parse(req)
        .map_err(|e| anyhow!("invalid version requirement '{}': {}", req, e))
}

/// Check if a version satisfies a requirement
pub fn version_satisfies(version: &str, requirement: &str) -> Result<bool> {
    let ver = semver::Version::parse(version)
        .map_err(|e| anyhow!("invalid version '{}': {}", version, e))?;
    let req = parse_version_req(requirement)?;
    Ok(req.matches(&ver))
}

/// Builtins for package management
pub mod builtins {
    use super::*;
    use crate::value::Value;

    /// List installed packages
    pub fn pkg_list() -> Result<Value> {
        let registry = PackageRegistry::default();
        let packages = registry.list_installed()?;

        let mut result = Vec::new();
        for (name, versions) in packages {
            let mut pkg_info = BTreeMap::new();
            pkg_info.insert("name".to_string(), Value::Str(name));
            pkg_info.insert(
                "versions".to_string(),
                Value::Array(versions.into_iter().map(Value::Str).collect()),
            );
            result.push(Value::Record(pkg_info));
        }

        Ok(Value::Array(result))
    }

    /// Get package info
    pub fn pkg_info(name: &str) -> Result<Value> {
        let registry = PackageRegistry::default();
        let pkg_dir = registry.cache_dir.join(name);

        if !pkg_dir.exists() {
            return Err(anyhow!("package '{}' not installed", name));
        }

        // Find latest version and read manifest
        if let Some(latest) = registry.find_latest_version(&pkg_dir)? {
            let manifest_path = pkg_dir.join(&latest).join("aether.toml");
            if manifest_path.exists() {
                let content = fs::read_to_string(&manifest_path)?;
                let manifest: PackageManifest = toml::from_str(&content)?;

                let mut info = BTreeMap::new();
                info.insert("name".to_string(), Value::Str(manifest.package.name));
                info.insert("version".to_string(), Value::Str(manifest.package.version));
                info.insert(
                    "description".to_string(),
                    Value::Str(manifest.package.description),
                );
                info.insert(
                    "authors".to_string(),
                    Value::Array(
                        manifest
                            .package
                            .authors
                            .into_iter()
                            .map(Value::Str)
                            .collect(),
                    ),
                );
                if let Some(license) = manifest.package.license {
                    info.insert("license".to_string(), Value::Str(license));
                }
                if let Some(repo) = manifest.package.repository {
                    info.insert("repository".to_string(), Value::Str(repo));
                }

                return Ok(Value::Record(info));
            }
        }

        Err(anyhow!("package manifest not found for '{}'", name))
    }

    /// Get package cache directory
    pub fn pkg_cache_dir() -> Value {
        let registry = PackageRegistry::default();
        Value::Str(registry.cache_dir.display().to_string())
    }

    /// Initialize a new package in the current directory
    pub fn pkg_init(name: &str, description: &str) -> Result<Value> {
        let manifest = PackageManifest {
            package: PackageInfo {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                description: description.to_string(),
                authors: vec![],
                license: Some("MIT".to_string()),
                repository: None,
                main: "main.ae".to_string(),
            },
            dependencies: HashMap::new(),
        };

        let toml_content = toml::to_string_pretty(&manifest)?;
        fs::write("aether.toml", &toml_content)?;

        // Create main.ae if it doesn't exist
        if !Path::new("main.ae").exists() {
            fs::write(
                "main.ae",
                "# AetherShell package: {}\n\nprint(\"Hello from {}!\")\n".replace("{}", name),
            )?;
        }

        Ok(Value::Str(format!(
            "Created package '{}' with aether.toml",
            name
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_satisfies() {
        assert!(version_satisfies("1.0.0", "^1.0").unwrap());
        assert!(version_satisfies("1.2.3", "^1.0").unwrap());
        assert!(!version_satisfies("2.0.0", "^1.0").unwrap());
        assert!(version_satisfies("1.0.0", ">=1.0.0").unwrap());
        assert!(version_satisfies("1.0.0", "1.0.0").unwrap());
    }

    #[test]
    fn test_parse_manifest() {
        let toml = r#"
            [package]
            name = "my-package"
            version = "1.0.0"
            description = "A test package"
            
            [dependencies]
            utils = "^1.0"
        "#;

        let manifest: PackageManifest = toml::from_str(toml).unwrap();
        assert_eq!(manifest.package.name, "my-package");
        assert_eq!(manifest.package.version, "1.0.0");
        assert!(manifest.dependencies.contains_key("utils"));
    }

    #[test]
    fn test_module_cache_cycle_detection() {
        let mut cache = ModuleCache::new();
        let path = PathBuf::from("/test/module.ae");

        cache.start_loading(path.clone());
        assert!(cache.check_cycle(&path).is_err());
        cache.finish_loading();
        assert!(cache.check_cycle(&path).is_ok());
    }
}
