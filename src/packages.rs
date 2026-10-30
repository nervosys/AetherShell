//! IronShell Package Management
//!
//! This module provides package management capabilities including:
//! - Package registry integration
//! - Dependency resolution
//! - Version management
//!
//! `import` and `export` of `.ish` files are implemented in
//! `crate::module_loader`; package-name imports (`pkg:`) are not wired to it yet.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// The builtin name on errors from this module. `RegistryClient` serves several
/// builtins (the `marketplace_*` family), so the errors name the module instead.
const BUILTIN: &str = "pkg";

/// The hint for an upstream registry failure. The shell's own policy refusals
/// have their own hints; a registry that answers badly gets this one.
#[cfg(not(target_arch = "wasm32"))]
const REGISTRY_HINT: &str =
    "the registry refused the request or could not be reached; check the network and IRONSHELL_REGISTRY_TOKEN, then retry";

/// An upstream registry failure (`E_IO`): the registry, not the shell's
/// policy, failed the request.
#[cfg(not(target_arch = "wasm32"))]
fn registry_refused(message: String) -> anyhow::Error {
    crate::safety::io_fault(BUILTIN, &message, REGISTRY_HINT)
}

/// The registry's answer that a package does not exist (`E_NOT_FOUND`).
///
/// `builtins.rs` (`registry_says_absent`) tells an absent package from an
/// unreachable registry by this code, so `marketplace_install` and
/// `marketplace_info` depend on it being `E_NOT_FOUND`.
#[cfg(not(target_arch = "wasm32"))]
fn registry_absent(name: &str) -> anyhow::Error {
    use crate::safety::{ErrorCode, SafetyError};
    anyhow::Error::new(SafetyError {
        code: ErrorCode::NotFound,
        message: format!("package '{name}' not found in registry"),
        builtin: BUILTIN.to_string(),
        hint: "list the registry's packages with marketplace.search and use a name from it"
            .to_string(),
        approval: None,
        did_you_mean: Vec::new(),
        expected: "an existing package".to_string(),
        got: name.to_string(),
    })
}

/// Publishing or yanking without a registry token (`E_BAD_STATE`): no policy
/// refused it, the environment lacks the credential.
#[cfg(not(target_arch = "wasm32"))]
fn auth_required() -> anyhow::Error {
    use crate::safety::{ErrorCode, SafetyError};
    anyhow::Error::new(SafetyError {
        code: ErrorCode::BadState,
        message: format!("{BUILTIN}: authentication required; no registry token is set"),
        builtin: BUILTIN.to_string(),
        hint: "set IRONSHELL_REGISTRY_TOKEN or IRONSHELL_TOKEN in the environment, then retry"
            .to_string(),
        approval: None,
        did_you_mean: Vec::new(),
        expected: "IRONSHELL_REGISTRY_TOKEN or IRONSHELL_TOKEN".to_string(),
        got: "neither set".to_string(),
    })
}

/// The wasm build has no HTTP client (`E_UNIMPLEMENTED`).
#[cfg(target_arch = "wasm32")]
fn wasm_unavailable() -> anyhow::Error {
    crate::safety::unimplemented(
        BUILTIN,
        "the package registry is not available in a wasm build",
        "use a native build of ish to reach the package registry",
    )
}

/// Package manifest (ironshell.toml)
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
    /// Main entry point (default: "main.ish")
    #[serde(default = "default_main")]
    pub main: String,
}

fn default_main() -> String {
    "main.ish".to_string()
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
            .join("ironshell")
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
            "package '{}' not found. Install with: ish pkg install {}",
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

// ===================== Registry HTTP Client =====================

/// Response types from the packages.nervosys.ai registry API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPackage {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub downloads: i64,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySearchResponse {
    #[serde(default)]
    pub packages: Vec<RegistryPackage>,
    #[serde(default)]
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryListResponse {
    #[serde(default)]
    pub packages: Vec<RegistryPackage>,
    #[serde(default)]
    pub next_token: Option<String>,
}

/// HTTP client for the IronShell package registry at packages.nervosys.ai
#[derive(Debug, Clone)]
pub struct RegistryClient {
    /// Base URL (e.g. "https://packages.nervosys.ai")
    pub base_url: String,
    /// Registry name (e.g. "ironshell")
    pub registry: String,
    /// Local cache directory
    pub cache_dir: PathBuf,
}

impl Default for RegistryClient {
    fn default() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ironshell")
            .join("packages");

        Self {
            base_url: "https://packages.nervosys.ai".to_string(),
            registry: "ironshell".to_string(),
            cache_dir,
        }
    }
}

/// Largest JSON body the registry is read as. A search or a package record is
/// a few kilobytes; a server that sends megabytes is not answering the query.
#[cfg(not(target_arch = "wasm32"))]
const MAX_JSON_BYTES: u64 = 4 * 1024 * 1024;

/// Largest package archive, in either direction.
#[cfg(not(target_arch = "wasm32"))]
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;

/// One path segment: non-empty, not `.` or `..`, and no separator or drive
/// colon, so that joining it onto a directory stays inside that directory.
#[cfg(not(target_arch = "wasm32"))]
fn check_segment(s: &str, what: &str) -> Result<()> {
    let ok = !s.is_empty()
        && s != "."
        && s != ".."
        && s.len() <= 128
        && !s
            .chars()
            .any(|c| matches!(c, '/' | '\\' | ':') || c.is_control());
    if ok {
        Ok(())
    } else {
        Err(crate::safety::bad_arg(
            BUILTIN,
            &format!("a {what} that is one safe path segment"),
            &format!("{s:?}"),
        ))
    }
}

/// Read a response body, refusing one longer than `max` bytes rather than
/// buffering whatever a server chooses to send.
#[cfg(not(target_arch = "wasm32"))]
fn read_body_limited(resp: reqwest::blocking::Response, max: u64) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut buf = Vec::new();
    resp.take(max + 1)
        .read_to_end(&mut buf)
        .context("failed to read registry response")?;
    if buf.len() as u64 > max {
        return Err(crate::safety::budget_exceeded(
            BUILTIN,
            format!("registry response is larger than {max} bytes; refusing it"),
            "the registry sent more than this client accepts; the response was not buffered",
        ));
    }
    Ok(buf)
}

impl RegistryClient {
    /// Build the API base path: {base_url}/{registry}/api/v1
    fn api_url(&self, path: &str) -> String {
        format!("{}/{}/api/v1{}", self.base_url, self.registry, path)
    }

    /// Get the auth token from IRONSHELL_REGISTRY_TOKEN or IRONSHELL_TOKEN env var
    fn auth_token() -> Option<String> {
        std::env::var("IRONSHELL_REGISTRY_TOKEN")
            .or_else(|_| std::env::var("IRONSHELL_TOKEN"))
            .ok()
    }

    /// Search packages on the remote registry
    #[cfg(not(target_arch = "wasm32"))]
    pub fn search(&self, query: &str, category: Option<&str>) -> Result<Vec<RegistryPackage>> {
        let mut url = self.api_url("/search");
        url.push_str(&format!("?q={}", urlencoding::encode(query)));
        if let Some(cat) = category {
            url.push_str(&format!("&category={}", urlencoding::encode(cat)));
        }

        crate::tls::install_provider();
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        let resp = client
            .get(&url)
            .header("User-Agent", "IronShell/1.5.0")
            .send()
            .context("failed to connect to package registry")?;

        if resp.status().is_success() {
            let body: RegistrySearchResponse =
                serde_json::from_slice(&read_body_limited(resp, MAX_JSON_BYTES)?)
                    .context("failed to parse registry search response")?;
            Ok(body.packages)
        } else if resp.status().as_u16() == 404 {
            Ok(vec![])
        } else {
            Err(registry_refused(format!(
                "registry search failed: HTTP {}",
                resp.status()
            )))
        }
    }

    /// List all packages on the remote registry
    #[cfg(not(target_arch = "wasm32"))]
    pub fn list_remote(&self) -> Result<Vec<RegistryPackage>> {
        let url = self.api_url("/packages");

        crate::tls::install_provider();
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        let resp = client
            .get(&url)
            .header("User-Agent", "IronShell/1.5.0")
            .send()
            .context("failed to connect to package registry")?;

        if resp.status().is_success() {
            let body: RegistryListResponse =
                serde_json::from_slice(&read_body_limited(resp, MAX_JSON_BYTES)?)
                    .context("failed to parse registry list response")?;
            Ok(body.packages)
        } else {
            Err(registry_refused(format!(
                "registry list failed: HTTP {}",
                resp.status()
            )))
        }
    }

    /// Get package details from the remote registry
    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_package(&self, name: &str) -> Result<RegistryPackage> {
        let url = self.api_url(&format!("/packages/{}", urlencoding::encode(name)));

        crate::tls::install_provider();
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        let resp = client
            .get(&url)
            .header("User-Agent", "IronShell/1.5.0")
            .send()
            .context("failed to connect to package registry")?;

        if resp.status().is_success() {
            let pkg: RegistryPackage =
                serde_json::from_slice(&read_body_limited(resp, MAX_JSON_BYTES)?)
                    .context("failed to parse registry package response")?;
            Ok(pkg)
        } else if resp.status().as_u16() == 404 {
            Err(registry_absent(name))
        } else {
            Err(registry_refused(format!(
                "registry get failed: HTTP {}",
                resp.status()
            )))
        }
    }

    /// Download a package tarball from the registry and cache it locally
    #[cfg(not(target_arch = "wasm32"))]
    pub fn download_package(&self, name: &str, version: &str) -> Result<PathBuf> {
        // Both become directory names under the cache. A name that is not one
        // path segment would write outside it, and an absolute one would
        // replace the cache directory entirely.
        check_segment(name, "package name")?;
        check_segment(version, "package version")?;
        let url = self.api_url(&format!(
            "/packages/{}/{}/download",
            urlencoding::encode(name),
            urlencoding::encode(version)
        ));

        crate::tls::install_provider();
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;

        let resp = client
            .get(&url)
            .header("User-Agent", "IronShell/1.5.0")
            .send()
            .context("failed to download package from registry")?;

        if !resp.status().is_success() {
            return Err(registry_refused(format!(
                "download failed for {}@{}: HTTP {}",
                name,
                version,
                resp.status()
            )));
        }

        let bytes = read_body_limited(resp, MAX_ARCHIVE_BYTES)?;

        // Create cache directory: {cache_dir}/{name}/{version}/
        let pkg_dir = self.cache_dir.join(name).join(version);
        fs::create_dir_all(&pkg_dir).context("failed to create package cache directory")?;

        // Save tarball
        let tarball_path = pkg_dir.join(format!("{}-{}.tar.gz", name, version));
        fs::write(&tarball_path, &bytes).context("failed to write package archive")?;

        // Extract tarball
        self.extract_tarball(&tarball_path, &pkg_dir)?;

        Ok(pkg_dir)
    }

    /// Extract a .tar.gz archive into a directory.
    ///
    /// Links are refused outright: a symlink in the archive can point anywhere,
    /// and a later entry written through it would land outside `dest`. Every
    /// other entry goes through `unpack_in`, which refuses any path that
    /// escapes `dest`.
    pub fn extract_tarball(&self, tarball: &Path, dest: &Path) -> Result<()> {
        let file = fs::File::open(tarball)?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);
        for entry in archive
            .entries()
            .context("failed to read package archive")?
        {
            let mut entry = entry.context("failed to read package archive entry")?;
            let kind = entry.header().entry_type();
            if kind.is_symlink() || kind.is_hard_link() {
                return Err(crate::safety::policy_deny(
                    BUILTIN,
                    format!(
                        "package archive contains a link entry ({}); refusing to unpack it",
                        entry
                            .path()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default()
                    ),
                    "a link in a package archive can point outside the cache; the archive was not unpacked",
                ));
            }
            // `Ok(false)` is the library declining an entry that would leave
            // `dest`. That is a hostile archive, not a file to skip.
            let written = entry
                .unpack_in(dest)
                .context("failed to extract package archive")?;
            if !written {
                return Err(crate::safety::policy_deny(
                    BUILTIN,
                    "package archive has an entry that would leave the cache directory; refusing it",
                    "the archive names a path outside its own directory; it was not unpacked",
                ));
            }
        }
        Ok(())
    }

    /// Publish a package to the registry (requires auth token)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn publish(&self, tarball_path: &Path, manifest: &PackageManifest) -> Result<()> {
        let token = Self::auth_token().ok_or_else(auth_required)?;

        let url = self.api_url("/packages");
        let size = fs::metadata(tarball_path)
            .context("failed to read package tarball")?
            .len();
        if size > MAX_ARCHIVE_BYTES {
            return Err(crate::safety::bad_arg(
                BUILTIN,
                &format!("a package archive of at most {MAX_ARCHIVE_BYTES} bytes"),
                &format!("{size} bytes"),
            ));
        }
        let tarball_bytes = fs::read(tarball_path).context("failed to read package tarball")?;

        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&tarball_bytes);

        let body = serde_json::json!({
            "name": manifest.package.name,
            "version": manifest.package.version,
            "description": manifest.package.description,
            "authors": manifest.package.authors,
            "license": manifest.package.license,
            "data": encoded,
        });

        crate::tls::install_provider();
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        let resp = client
            .post(&url)
            .header("User-Agent", "IronShell/1.5.0")
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .context("failed to publish package to registry")?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let err_body = resp.text().unwrap_or_default();
            Err(registry_refused(format!(
                "publish failed: HTTP {} — {}",
                status, err_body
            )))
        }
    }

    /// Yank (remove) a package version from the registry (requires auth token)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn yank(&self, name: &str, version: &str) -> Result<()> {
        let token = Self::auth_token().ok_or_else(auth_required)?;

        let url = self.api_url(&format!(
            "/packages/{}/{}",
            urlencoding::encode(name),
            urlencoding::encode(version)
        ));

        crate::tls::install_provider();
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let resp = client
            .delete(&url)
            .header("User-Agent", "IronShell/1.5.0")
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .context("failed to yank package from registry")?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let err_body = resp.text().unwrap_or_default();
            Err(registry_refused(format!(
                "yank failed: HTTP {} — {}",
                status, err_body
            )))
        }
    }

    /// Convert a RegistryPackage to a Value::Record for pipeline consumption
    pub fn package_to_value(pkg: &RegistryPackage) -> crate::value::Value {
        let mut rec = BTreeMap::new();
        rec.insert(
            "name".to_string(),
            crate::value::Value::Str(pkg.name.clone()),
        );
        rec.insert(
            "version".to_string(),
            crate::value::Value::Str(pkg.version.clone()),
        );
        rec.insert(
            "description".to_string(),
            crate::value::Value::Str(pkg.description.clone()),
        );
        rec.insert(
            "author".to_string(),
            crate::value::Value::Str(pkg.author.clone()),
        );
        rec.insert(
            "downloads".to_string(),
            crate::value::Value::Int(pkg.downloads),
        );
        rec.insert(
            "created_at".to_string(),
            crate::value::Value::Str(pkg.created_at.clone()),
        );
        if let Some(ref cat) = pkg.category {
            rec.insert(
                "category".to_string(),
                crate::value::Value::Str(cat.clone()),
            );
        }
        if !pkg.keywords.is_empty() {
            rec.insert(
                "keywords".to_string(),
                crate::value::Value::Array(
                    pkg.keywords
                        .iter()
                        .map(|k| crate::value::Value::Str(k.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(ref lic) = pkg.license {
            rec.insert("license".to_string(), crate::value::Value::Str(lic.clone()));
        }
        crate::value::Value::Record(rec)
    }
}

// Wasm stubs — no HTTP client available
#[cfg(target_arch = "wasm32")]
impl RegistryClient {
    pub fn search(&self, _query: &str, _category: Option<&str>) -> Result<Vec<RegistryPackage>> {
        Err(wasm_unavailable())
    }
    pub fn list_remote(&self) -> Result<Vec<RegistryPackage>> {
        Err(wasm_unavailable())
    }
    pub fn get_package(&self, _name: &str) -> Result<RegistryPackage> {
        Err(wasm_unavailable())
    }
    pub fn download_package(&self, _name: &str, _version: &str) -> Result<PathBuf> {
        Err(wasm_unavailable())
    }
    pub fn publish(&self, _tarball_path: &Path, _manifest: &PackageManifest) -> Result<()> {
        Err(wasm_unavailable())
    }
    pub fn yank(&self, _name: &str, _version: &str) -> Result<()> {
        Err(wasm_unavailable())
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
            let manifest_path = pkg_dir.join(&latest).join("ironshell.toml");
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
                main: "main.ish".to_string(),
            },
            dependencies: HashMap::new(),
        };

        let toml_content = toml::to_string_pretty(&manifest)?;
        fs::write("ironshell.toml", &toml_content)?;

        // Create main.ish if it doesn't exist
        if !Path::new("main.ish").exists() {
            fs::write(
                "main.ish",
                "# IronShell package: {}\n\nprint(\"Hello from {}!\")\n".replace("{}", name),
            )?;
        }

        Ok(Value::Str(format!(
            "Created package '{}' with ironshell.toml",
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

    /// A refused path segment is coded `E_BAD_ARG`, not `E_UNKNOWN`. The check
    /// runs before any request is made, so this touches no network and no cache.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn refused_segment_carries_its_code() {
        let err = RegistryClient::default()
            .download_package("../escape", "1.0.0")
            .unwrap_err();
        let se = err
            .downcast_ref::<crate::safety::SafetyError>()
            .expect("a coded error");
        assert!(matches!(se.code, crate::safety::ErrorCode::BadArg));
        assert_eq!(se.builtin, "pkg");
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
}
