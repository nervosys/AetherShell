use crate::ai_api::models::*;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// XDG Base Directory compliant storage for AI models
///
/// Directory structure:
/// ~/.local/share/ai-models/
/// ├── models/                 # Model files
/// │   ├── gguf/              # GGUF format models
/// │   ├── safetensors/       # SafeTensors format models
/// │   ├── pytorch/           # PyTorch format models
/// │   └── onnx/              # ONNX format models
/// ├── metadata/              # Model metadata files
/// ├── cache/                 # Temporary files and downloads
/// └── index.json             # Global model index
///
/// ~/.config/ai-models/
/// ├── config.toml            # Global configuration
/// ├── providers.toml         # Provider configurations
/// └── aliases.toml           # Model aliases and shortcuts
pub struct ModelStorage {
    data_dir: PathBuf,
    config_dir: PathBuf,
    cache_dir: PathBuf,
    index: ModelIndex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelIndex {
    pub version: String,
    pub last_updated: DateTime<Utc>,
    pub models: HashMap<String, LocalModelMetadata>,
    pub aliases: HashMap<String, String>,
    pub total_size: u64,
}

impl ModelStorage {
    pub fn new(config: &StorageConfig) -> Result<Self> {
        let data_dir = if let Some(custom_path) = &config.custom_data_dir {
            custom_path.clone()
        } else {
            Self::get_xdg_data_dir()?
        };

        let config_dir = if let Some(custom_path) = &config.custom_config_dir {
            custom_path.clone()
        } else {
            Self::get_xdg_config_dir()?
        };

        let cache_dir = data_dir.join("cache");

        // Create necessary directories
        fs::create_dir_all(&data_dir)?;
        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&cache_dir)?;
        fs::create_dir_all(data_dir.join("models"))?;
        fs::create_dir_all(data_dir.join("models/gguf"))?;
        fs::create_dir_all(data_dir.join("models/safetensors"))?;
        fs::create_dir_all(data_dir.join("models/pytorch"))?;
        fs::create_dir_all(data_dir.join("models/onnx"))?;
        fs::create_dir_all(data_dir.join("metadata"))?;

        let mut storage = Self {
            data_dir,
            config_dir,
            cache_dir,
            index: ModelIndex {
                version: "1.0".to_string(),
                last_updated: Utc::now(),
                models: HashMap::new(),
                aliases: HashMap::new(),
                total_size: 0,
            },
        };

        storage.load_index()?;
        Ok(storage)
    }

    /// Get XDG data directory for AI models
    fn get_xdg_data_dir() -> Result<PathBuf> {
        if let Ok(xdg_data_home) = std::env::var("XDG_DATA_HOME") {
            Ok(PathBuf::from(xdg_data_home).join("ai-models"))
        } else if let Ok(home) = std::env::var("HOME") {
            Ok(PathBuf::from(home).join(".local/share/ai-models"))
        } else {
            // Windows fallback
            if let Ok(appdata) = std::env::var("APPDATA") {
                Ok(PathBuf::from(appdata).join("ai-models"))
            } else {
                Err(anyhow::anyhow!("Cannot determine data directory"))
            }
        }
    }

    /// Get XDG config directory for AI models
    fn get_xdg_config_dir() -> Result<PathBuf> {
        if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
            Ok(PathBuf::from(xdg_config_home).join("ai-models"))
        } else if let Ok(home) = std::env::var("HOME") {
            Ok(PathBuf::from(home).join(".config/ai-models"))
        } else {
            // Windows fallback
            if let Ok(appdata) = std::env::var("APPDATA") {
                Ok(PathBuf::from(appdata).join("ai-models"))
            } else {
                Err(anyhow::anyhow!("Cannot determine config directory"))
            }
        }
    }

    /// Load the model index from disk
    fn load_index(&mut self) -> Result<()> {
        let index_path = self.data_dir.join("index.json");
        if index_path.exists() {
            let content = fs::read_to_string(&index_path)?;
            self.index = serde_json::from_str(&content)?;
        }
        Ok(())
    }

    /// Save the model index to disk
    fn save_index(&self) -> Result<()> {
        let index_path = self.data_dir.join("index.json");
        let content = serde_json::to_string_pretty(&self.index)?;
        fs::write(&index_path, content)?;
        Ok(())
    }

    /// List all locally stored models
    pub fn list_local_models(&self) -> Result<Vec<ModelInfo>> {
        let mut models = Vec::new();

        for (id, metadata) in &self.index.models {
            let model_info = ModelInfo {
                id: id.clone(),
                object: "model".to_string(),
                created: metadata.downloaded_at.timestamp(),
                owned_by: "local".to_string(),
                provider: "local".to_string(),
                context_length: metadata
                    .parameters
                    .get("context_length")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32),
                max_output: metadata
                    .parameters
                    .get("max_output")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32),
                per_request_limits: None,
                pricing: None, // Local models are free
                capabilities: metadata.capabilities.clone(),
                local_path: Some(metadata.file_path.clone()),
                format: metadata.format.clone(),
                size_bytes: Some(metadata.size_bytes),
                metadata: metadata.parameters.clone(),
            };
            models.push(model_info);
        }

        Ok(models)
    }

    /// Store a model file and metadata
    pub async fn store_model(
        &mut self,
        model_data: &[u8],
        metadata: LocalModelMetadata,
    ) -> Result<()> {
        // Determine storage path based on format
        let format_dir = match metadata.format {
            ModelFormat::GGUF => "gguf",
            ModelFormat::SafeTensors => "safetensors",
            ModelFormat::PyTorch => "pytorch",
            ModelFormat::ONNX => "onnx",
            _ => "other",
        };

        let model_dir = self.data_dir.join("models").join(format_dir);
        fs::create_dir_all(&model_dir)?;

        // Generate filename from model ID
        let filename = format!(
            "{}.{}",
            metadata.id.replace('/', "_"),
            self.get_file_extension(&metadata.format)
        );
        let file_path = model_dir.join(&filename);

        // Write model file
        fs::write(&file_path, model_data)?;

        // Calculate and verify checksum
        let mut hasher = Sha256::new();
        hasher.update(model_data);
        let calculated_hash = format!("{:x}", hasher.finalize());

        if calculated_hash != metadata.sha256 {
            fs::remove_file(&file_path)?;
            return Err(anyhow::anyhow!(
                "Checksum mismatch: expected {}, got {}",
                metadata.sha256,
                calculated_hash
            ));
        }

        // Update metadata with actual file path
        let mut updated_metadata = metadata;
        updated_metadata.file_path = file_path.to_string_lossy().to_string();
        updated_metadata.size_bytes = model_data.len() as u64;

        // Store metadata file
        let metadata_path = self
            .data_dir
            .join("metadata")
            .join(format!("{}.json", updated_metadata.id.replace('/', "_")));
        let metadata_content = serde_json::to_string_pretty(&updated_metadata)?;
        fs::write(&metadata_path, metadata_content)?;

        // Update index
        self.index
            .models
            .insert(updated_metadata.id.clone(), updated_metadata);
        self.index.last_updated = Utc::now();
        self.index.total_size = self.index.models.values().map(|m| m.size_bytes).sum();
        self.save_index()?;

        Ok(())
    }

    /// Remove a model from storage
    pub fn remove_model(&mut self, model_id: &str) -> Result<()> {
        if let Some(metadata) = self.index.models.remove(model_id) {
            // Remove model file
            if let Ok(_) = fs::remove_file(&metadata.file_path) {
                // File removed successfully
            }

            // Remove metadata file
            let metadata_path = self
                .data_dir
                .join("metadata")
                .join(format!("{}.json", model_id.replace('/', "_")));
            if let Ok(_) = fs::remove_file(&metadata_path) {
                // Metadata file removed successfully
            }

            // Remove any config files
            if let Some(config_path) = &metadata.config_path {
                if let Ok(_) = fs::remove_file(config_path) {
                    // Config file removed successfully
                }
            }

            // Remove tokenizer files
            if let Some(tokenizer_path) = &metadata.tokenizer_path {
                if let Ok(_) = fs::remove_file(tokenizer_path) {
                    // Tokenizer file removed successfully
                }
            }

            // Update index
            self.index.last_updated = Utc::now();
            self.index.total_size = self.index.models.values().map(|m| m.size_bytes).sum();
            self.save_index()?;

            Ok(())
        } else {
            Err(anyhow::anyhow!("Model {} not found", model_id))
        }
    }

    /// Get model metadata
    pub fn get_model_metadata(&self, model_id: &str) -> Option<&LocalModelMetadata> {
        self.index.models.get(model_id)
    }

    /// Update model usage statistics
    pub fn update_model_usage(&mut self, model_id: &str) -> Result<()> {
        if let Some(metadata) = self.index.models.get_mut(model_id) {
            metadata.last_used = Some(Utc::now());
            metadata.usage_count += 1;
            self.save_index()?;
        }
        Ok(())
    }

    /// Get storage statistics
    pub fn get_storage_stats(&self) -> StorageStats {
        let model_count = self.index.models.len();
        let total_size = self.index.total_size;

        let mut format_breakdown = HashMap::new();
        for metadata in self.index.models.values() {
            *format_breakdown.entry(metadata.format.clone()).or_insert(0) += 1;
        }

        StorageStats {
            model_count,
            total_size,
            format_breakdown,
            cache_size: self.get_cache_size().unwrap_or(0),
            data_dir: self.data_dir.clone(),
            config_dir: self.config_dir.clone(),
        }
    }

    /// Clean up cache directory
    pub fn cleanup_cache(&self, max_age_days: u64) -> Result<u64> {
        let cache_dir = &self.cache_dir;
        let cutoff_time = Utc::now() - chrono::Duration::days(max_age_days as i64);
        let mut cleaned_size = 0u64;

        if cache_dir.exists() {
            for entry in fs::read_dir(cache_dir)? {
                let entry = entry?;
                let metadata = entry.metadata()?;

                if let Ok(modified) = metadata.modified() {
                    let modified_time: DateTime<Utc> = modified.into();
                    if modified_time < cutoff_time {
                        if metadata.is_file() {
                            cleaned_size += metadata.len();
                            fs::remove_file(entry.path())?;
                        }
                    }
                }
            }
        }

        Ok(cleaned_size)
    }

    /// Add model alias
    pub fn add_alias(&mut self, alias: String, model_id: String) -> Result<()> {
        self.index.aliases.insert(alias, model_id);
        self.save_index()
    }

    /// Remove model alias
    pub fn remove_alias(&mut self, alias: &str) -> Result<()> {
        self.index.aliases.remove(alias);
        self.save_index()
    }

    /// Resolve model ID from alias
    pub fn resolve_alias(&self, id_or_alias: &str) -> String {
        self.index
            .aliases
            .get(id_or_alias)
            .cloned()
            .unwrap_or_else(|| id_or_alias.to_string())
    }

    /// Get cache directory path
    pub fn get_cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    fn get_file_extension(&self, format: &ModelFormat) -> &str {
        match format {
            ModelFormat::GGUF => "gguf",
            ModelFormat::SafeTensors => "safetensors",
            ModelFormat::PyTorch => "pt",
            ModelFormat::ONNX => "onnx",
            ModelFormat::TensorFlow => "pb",
            _ => "bin",
        }
    }

    fn get_cache_size(&self) -> Result<u64> {
        let mut total_size = 0u64;

        if self.cache_dir.exists() {
            for entry in fs::read_dir(&self.cache_dir)? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                if metadata.is_file() {
                    total_size += metadata.len();
                }
            }
        }

        Ok(total_size)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub custom_data_dir: Option<PathBuf>,
    pub custom_config_dir: Option<PathBuf>,
    pub max_cache_size_gb: Option<u64>,
    pub auto_cleanup_days: Option<u64>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            custom_data_dir: None,
            custom_config_dir: None,
            max_cache_size_gb: Some(10), // 10GB default cache limit
            auto_cleanup_days: Some(30), // Clean up cache files older than 30 days
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub model_count: usize,
    pub total_size: u64,
    pub format_breakdown: HashMap<ModelFormat, usize>,
    pub cache_size: u64,
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
}

impl StorageStats {
    pub fn total_size_human(&self) -> String {
        human_bytes(self.total_size)
    }

    pub fn cache_size_human(&self) -> String {
        human_bytes(self.cache_size)
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}
