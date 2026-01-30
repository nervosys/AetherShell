//! Agent Marketplace Registry
//!
//! This module provides a registry for sharing and discovering AetherShell agents.
//! Agents can be published, searched, installed, and versioned.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Agent metadata for the marketplace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    /// Unique agent identifier (namespace/name format)
    pub name: String,
    /// Human-readable display name
    pub display_name: String,
    /// Agent description
    pub description: String,
    /// Semantic version
    pub version: String,
    /// Author information
    pub author: AuthorInfo,
    /// License (SPDX identifier)
    pub license: String,
    /// Repository URL
    pub repository: Option<String>,
    /// Homepage URL
    pub homepage: Option<String>,
    /// Keywords for search
    pub keywords: Vec<String>,
    /// Categories
    pub categories: Vec<String>,
    /// Agent capabilities
    pub capabilities: Vec<String>,
    /// Required AetherShell version
    pub aethershell_version: String,
    /// Dependencies on other agents
    pub dependencies: Vec<AgentDependency>,
    /// Agent definition
    pub agent: AgentDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorInfo {
    pub name: String,
    pub email: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDependency {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// System prompt for the agent
    pub system_prompt: String,
    /// Model to use (optional, uses default if not specified)
    pub model: Option<String>,
    /// Available tools/builtins
    pub tools: Vec<String>,
    /// Tool calling mode
    pub tool_choice: Option<String>,
    /// Temperature for generation
    pub temperature: Option<f32>,
    /// Max tokens
    pub max_tokens: Option<u32>,
}

/// Published agent in the registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedAgent {
    pub manifest: AgentManifest,
    /// Download count
    pub downloads: u64,
    /// Star count
    pub stars: u64,
    /// Fork count
    pub forks: u64,
    /// Verification status
    pub verified: bool,
    /// Published timestamp
    pub published_at: u64,
    /// Last updated timestamp
    pub updated_at: u64,
    /// SHA-256 hash of the agent bundle
    pub checksum: String,
    /// Bundle size in bytes
    pub size: u64,
}

/// Search query for the marketplace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: Option<String>,
    pub category: Option<String>,
    pub keyword: Option<String>,
    pub author: Option<String>,
    pub sort_by: Option<SortBy>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    Downloads,
    Stars,
    Recent,
    Name,
}

/// Search results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub results: Vec<PublishedAgent>,
}

/// Local registry client
pub struct RegistryClient {
    /// Registry URL
    registry_url: String,
    /// Local cache directory
    cache_dir: PathBuf,
    /// Installed agents
    installed: HashMap<String, InstalledAgent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledAgent {
    pub manifest: AgentManifest,
    pub installed_at: u64,
    pub path: PathBuf,
}

impl RegistryClient {
    /// Create a new registry client
    pub fn new(registry_url: Option<String>) -> Result<Self> {
        let registry_url =
            registry_url.unwrap_or_else(|| "https://registry.aethershell.dev".to_string());

        let cache_dir = dirs::cache_dir()
            .ok_or_else(|| anyhow!("Could not determine cache directory"))?
            .join("aethershell")
            .join("agents");

        std::fs::create_dir_all(&cache_dir)?;

        let installed = Self::load_installed(&cache_dir)?;

        Ok(Self {
            registry_url,
            cache_dir,
            installed,
        })
    }

    /// Search for agents in the registry
    pub async fn search(&self, query: SearchQuery) -> Result<SearchResults> {
        let client = reqwest::Client::new();

        let mut url = format!("{}/api/v1/agents/search", self.registry_url);
        let mut params = vec![];

        if let Some(q) = &query.query {
            params.push(format!("q={}", urlencoding::encode(q)));
        }
        if let Some(cat) = &query.category {
            params.push(format!("category={}", urlencoding::encode(cat)));
        }
        if let Some(kw) = &query.keyword {
            params.push(format!("keyword={}", urlencoding::encode(kw)));
        }
        if let Some(author) = &query.author {
            params.push(format!("author={}", urlencoding::encode(author)));
        }
        if let Some(sort) = &query.sort_by {
            params.push(format!("sort={}", serde_json::to_string(sort)?));
        }
        if let Some(page) = query.page {
            params.push(format!("page={}", page));
        }
        if let Some(per_page) = query.per_page {
            params.push(format!("per_page={}", per_page));
        }

        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }

        let response = client.get(&url).send().await?;
        let results: SearchResults = response.json().await?;
        Ok(results)
    }

    /// Get agent details
    pub async fn get(&self, name: &str, version: Option<&str>) -> Result<PublishedAgent> {
        let client = reqwest::Client::new();

        let version_suffix = version.map(|v| format!("/{}", v)).unwrap_or_default();
        let url = format!(
            "{}/api/v1/agents/{}{}",
            self.registry_url, name, version_suffix
        );

        let response = client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("Agent not found: {}", name));
        }

        let agent: PublishedAgent = response.json().await?;
        Ok(agent)
    }

    /// Install an agent
    pub fn install<'a>(&'a mut self, name: &'a str, version: Option<&'a str>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<InstalledAgent>> + Send + 'a>> {
        Box::pin(async move {
            // Check if already installed
            if let Some(installed) = self.installed.get(name) {
                if version.is_none() || version == Some(&installed.manifest.version) {
                    return Ok(installed.clone());
                }
            }

            // Fetch agent from registry
            let agent = self.get(name, version).await?;

            // Install dependencies first
            for dep in &agent.manifest.dependencies {
                self.install(&dep.name, Some(&dep.version)).await?;
            }

            // Download and install
            let client = reqwest::Client::new();
            let url = format!(
                "{}/api/v1/agents/{}/{}/download",
                self.registry_url, name, agent.manifest.version
            );

            let response = client.get(&url).send().await?;
            let bytes = response.bytes().await?;

            // Verify checksum
            let checksum = format!("{:x}", md5::compute(&bytes));
            if checksum != agent.checksum {
                return Err(anyhow!("Checksum mismatch for agent {}", name));
            }

            // Save to local cache
            let agent_dir = self.cache_dir.join(name).join(&agent.manifest.version);
            std::fs::create_dir_all(&agent_dir)?;

            let manifest_path = agent_dir.join("manifest.json");
            std::fs::write(
                &manifest_path,
                serde_json::to_string_pretty(&agent.manifest)?,
            )?;

            let agent_path = agent_dir.join("agent.ae");
            std::fs::write(&agent_path, &bytes)?;

            let installed = InstalledAgent {
                manifest: agent.manifest.clone(),
                installed_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
                path: agent_dir.clone(),
            };

            self.installed.insert(name.to_string(), installed.clone());
            self.save_installed()?;

            Ok(installed)
        })
    }

    /// Uninstall an agent
    pub fn uninstall(&mut self, name: &str) -> Result<()> {
        if let Some(installed) = self.installed.remove(name) {
            std::fs::remove_dir_all(&installed.path)?;
            self.save_installed()?;
            Ok(())
        } else {
            Err(anyhow!("Agent not installed: {}", name))
        }
    }

    /// List installed agents
    pub fn list_installed(&self) -> Vec<&InstalledAgent> {
        self.installed.values().collect()
    }

    /// Publish an agent to the registry
    pub async fn publish(
        &self,
        manifest: &AgentManifest,
        agent_code: &str,
        api_key: &str,
    ) -> Result<PublishedAgent> {
        let client = reqwest::Client::new();

        let url = format!("{}/api/v1/agents/publish", self.registry_url);

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&serde_json::json!({
                "manifest": manifest,
                "code": agent_code,
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error: serde_json::Value = response.json().await?;
            return Err(anyhow!("Failed to publish: {}", error));
        }

        let published: PublishedAgent = response.json().await?;
        Ok(published)
    }

    /// Load installed agents from disk
    fn load_installed(cache_dir: &PathBuf) -> Result<HashMap<String, InstalledAgent>> {
        let index_path = cache_dir.join("installed.json");
        if index_path.exists() {
            let content = std::fs::read_to_string(&index_path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(HashMap::new())
        }
    }

    /// Save installed agents index
    fn save_installed(&self) -> Result<()> {
        let index_path = self.cache_dir.join("installed.json");
        let content = serde_json::to_string_pretty(&self.installed)?;
        std::fs::write(&index_path, content)?;
        Ok(())
    }

    /// Get agent by name (from installed or fetch)
    pub fn get_installed(&self, name: &str) -> Option<&InstalledAgent> {
        self.installed.get(name)
    }

    /// Load agent code
    pub fn load_agent_code(&self, name: &str) -> Result<String> {
        let installed = self
            .get_installed(name)
            .ok_or_else(|| anyhow!("Agent not installed: {}", name))?;

        let code_path = installed.path.join("agent.ae");
        Ok(std::fs::read_to_string(&code_path)?)
    }
}

/// CLI commands for the marketplace
#[derive(Debug, Clone)]
pub enum MarketplaceCommand {
    Search {
        query: String,
    },
    Install {
        name: String,
        version: Option<String>,
    },
    Uninstall {
        name: String,
    },
    List,
    Info {
        name: String,
    },
    Publish {
        path: PathBuf,
    },
    Update {
        name: Option<String>,
    },
}

/// Process marketplace CLI commands
pub async fn process_command(command: MarketplaceCommand) -> Result<String> {
    let mut client = RegistryClient::new(None)?;

    match command {
        MarketplaceCommand::Search { query } => {
            let results = client
                .search(SearchQuery {
                    query: Some(query),
                    ..Default::default()
                })
                .await?;

            let mut output = format!("Found {} agents:\n\n", results.total);
            for agent in results.results {
                output.push_str(&format!(
                    "  {} v{} - {}\n    by {} | ⬇ {} | ⭐ {}\n\n",
                    agent.manifest.name,
                    agent.manifest.version,
                    agent.manifest.description,
                    agent.manifest.author.name,
                    agent.downloads,
                    agent.stars
                ));
            }
            Ok(output)
        }

        MarketplaceCommand::Install { name, version } => {
            let installed = client.install(&name, version.as_deref()).await?;
            Ok(format!(
                "✓ Installed {} v{}\n  Location: {}",
                installed.manifest.name,
                installed.manifest.version,
                installed.path.display()
            ))
        }

        MarketplaceCommand::Uninstall { name } => {
            client.uninstall(&name)?;
            Ok(format!("✓ Uninstalled {}", name))
        }

        MarketplaceCommand::List => {
            let installed = client.list_installed();
            if installed.is_empty() {
                return Ok("No agents installed.".to_string());
            }

            let mut output = format!("Installed agents ({}):\n\n", installed.len());
            for agent in installed {
                output.push_str(&format!(
                    "  {} v{}\n    {}\n\n",
                    agent.manifest.name, agent.manifest.version, agent.manifest.description
                ));
            }
            Ok(output)
        }

        MarketplaceCommand::Info { name } => {
            let agent = client.get(&name, None).await?;
            Ok(format!(
                "{}\n{}\n\nVersion: {}\nAuthor: {} <{}>\nLicense: {}\n\nCapabilities: {}\nKeywords: {}\n\nDownloads: {}\nStars: {}\nVerified: {}",
                agent.manifest.display_name,
                agent.manifest.description,
                agent.manifest.version,
                agent.manifest.author.name,
                agent.manifest.author.email.unwrap_or_default(),
                agent.manifest.license,
                agent.manifest.capabilities.join(", "),
                agent.manifest.keywords.join(", "),
                agent.downloads,
                agent.stars,
                if agent.verified { "✓" } else { "✗" }
            ))
        }

        MarketplaceCommand::Publish { path } => {
            let manifest_path = path.join("manifest.json");
            let code_path = path.join("agent.ae");

            if !manifest_path.exists() {
                return Err(anyhow!("manifest.json not found in {}", path.display()));
            }
            if !code_path.exists() {
                return Err(anyhow!("agent.ae not found in {}", path.display()));
            }

            let manifest: AgentManifest =
                serde_json::from_str(&std::fs::read_to_string(&manifest_path)?)?;
            let code = std::fs::read_to_string(&code_path)?;

            let api_key = std::env::var("AETHERSHELL_API_KEY")
                .map_err(|_| anyhow!("AETHERSHELL_API_KEY environment variable not set"))?;

            let published = client.publish(&manifest, &code, &api_key).await?;
            Ok(format!(
                "✓ Published {} v{}\n  Registry: {}",
                published.manifest.name, published.manifest.version, client.registry_url
            ))
        }

        MarketplaceCommand::Update { name } => {
            let agents_to_update: Vec<_> = if let Some(name) = name {
                vec![name]
            } else {
                client.installed.keys().cloned().collect()
            };

            let mut output = String::new();
            for agent_name in agents_to_update {
                let latest = client.get(&agent_name, None).await?;
                let needs_update = client
                    .get_installed(&agent_name)
                    .map(|installed| latest.manifest.version != installed.manifest.version)
                    .unwrap_or(false);
                let old_version = client
                    .get_installed(&agent_name)
                    .map(|i| i.manifest.version.clone());

                if needs_update {
                    client.install(&agent_name, None).await?;
                    if let Some(old_ver) = old_version {
                        output.push_str(&format!(
                            "✓ Updated {} {} → {}\n",
                            agent_name, old_ver, latest.manifest.version
                        ));
                    }
                } else if client.get_installed(&agent_name).is_some() {
                    output.push_str(&format!("  {} is up to date\n", agent_name));
                }
            }

            if output.is_empty() {
                output = "All agents are up to date.".to_string();
            }
            Ok(output)
        }
    }
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            query: None,
            category: None,
            keyword: None,
            author: None,
            sort_by: Some(SortBy::Downloads),
            page: Some(1),
            per_page: Some(20),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_manifest_serialization() {
        let manifest = AgentManifest {
            name: "nervosys/code-reviewer".to_string(),
            display_name: "Code Reviewer".to_string(),
            description: "Automated code review agent".to_string(),
            version: "1.0.0".to_string(),
            author: AuthorInfo {
                name: "Nervosys".to_string(),
                email: Some("contact@nervosys.ai".to_string()),
                url: Some("https://nervosys.ai".to_string()),
            },
            license: "MIT".to_string(),
            repository: Some("https://github.com/nervosys/code-reviewer".to_string()),
            homepage: None,
            keywords: vec!["code".to_string(), "review".to_string()],
            categories: vec!["development".to_string()],
            capabilities: vec!["code_analysis".to_string(), "suggestions".to_string()],
            aethershell_version: ">=0.2.0".to_string(),
            dependencies: vec![],
            agent: AgentDefinition {
                system_prompt: "You are a code reviewer...".to_string(),
                model: Some("gpt-4o-mini".to_string()),
                tools: vec!["read_file".to_string(), "grep".to_string()],
                tool_choice: None,
                temperature: Some(0.3),
                max_tokens: Some(4096),
            },
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: AgentManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, manifest.name);
    }

    #[test]
    fn test_search_query_default() {
        let query = SearchQuery::default();
        assert!(query.query.is_none());
        assert_eq!(query.page, Some(1));
        assert_eq!(query.per_page, Some(20));
    }
}
