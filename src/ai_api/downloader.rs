use crate::ai_api::{models::*, storage::ModelStorage};
use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tokio::io::AsyncWriteExt;
use url::Url;

/// Model downloader for Hugging Face and other sources
pub struct ModelDownloader {
    client: Client,
    storage: ModelStorage,
    cache_dir: std::path::PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub model_id: String,
    pub source: ModelSource,
    pub format_preference: Option<ModelFormat>,
    pub quantization: Option<String>,
    pub validate_checksum: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub stage: DownloadStage,
    pub progress: f64, // 0.0 to 1.0
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: Option<u64>,
    pub eta_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DownloadStage {
    Initializing,
    DownloadingMetadata,
    DownloadingModel,
    ValidatingChecksum,
    ExtractingFiles,
    StoringModel,
    Complete,
    Failed(String),
}

/// Hugging Face model info from API
#[derive(Debug, Clone, Deserialize)]
pub struct HuggingFaceModelInfo {
    pub id: String,
    pub sha: String,
    pub downloads: u64,
    pub likes: u64,
    pub tags: Vec<String>,
    pub siblings: Vec<HuggingFaceFile>,
    pub library_name: Option<String>,
    pub pipeline_tag: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HuggingFaceFile {
    pub rfilename: String,
    pub size: Option<u64>,
    pub blob_id: String,
}

impl ModelDownloader {
    pub fn new(storage: ModelStorage) -> Result<Self> {
        let cache_dir = storage.get_cache_dir().to_path_buf();
        fs::create_dir_all(&cache_dir)?;

        // SECURITY FIX (LOW-002): Use secure HTTP client with timeouts
        let client =
            crate::security::create_secure_async_client().unwrap_or_else(|_| Client::new());

        Ok(Self {
            client,
            storage,
            cache_dir,
        })
    }

    /// Download a model from Hugging Face
    pub async fn download_model(&mut self, request: DownloadRequest) -> Result<LocalModelMetadata> {
        match &request.source.origin {
            origin if origin == "huggingface" => self.download_huggingface_model(request).await,
            origin if origin == "url" => self.download_from_url(request).await,
            _ => Err(anyhow::anyhow!(
                "Unsupported model source: {}",
                request.source.origin
            )),
        }
    }

    /// Download model from Hugging Face
    async fn download_huggingface_model(
        &mut self,
        request: DownloadRequest,
    ) -> Result<LocalModelMetadata> {
        let model_id = &request.model_id;

        // Get model info from Hugging Face API
        let model_info = self.fetch_huggingface_model_info(model_id).await?;

        // Determine which files to download based on format preference
        let files_to_download = self.select_files_to_download(&model_info, &request)?;

        if files_to_download.is_empty() {
            return Err(anyhow::anyhow!(
                "No suitable files found for model {}",
                model_id
            ));
        }

        // Create temporary directory for download
        let temp_dir = self
            .cache_dir
            .join(format!("download_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir)?;

        let mut downloaded_files = Vec::new();
        let mut total_size = 0u64;

        // Download all required files
        for file in &files_to_download {
            let file_url = format!(
                "https://huggingface.co/{}/resolve/main/{}",
                model_id, file.rfilename
            );

            let local_path = temp_dir.join(&file.rfilename);

            // Create subdirectories if needed
            if let Some(parent) = local_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let downloaded_size = self.download_file(&file_url, &local_path).await?;
            total_size += downloaded_size;

            downloaded_files.push((file.clone(), local_path));
        }

        // Determine the main model file and format
        let (_main_file, main_path, format) = self.identify_main_model_file(&downloaded_files)?;

        // Read the main model file
        let model_data = fs::read(&main_path)?;

        // Calculate checksum
        let mut hasher = Sha256::new();
        hasher.update(&model_data);
        let checksum = format!("{:x}", hasher.finalize());

        // Create metadata
        let metadata = LocalModelMetadata {
            id: model_id.clone(),
            name: model_id.clone(),
            description: Some(format!("Downloaded from Hugging Face: {}", model_id)),
            version: model_info.sha.clone(),
            format,
            file_path: String::new(), // Will be set by storage
            config_path: self.find_config_file(&downloaded_files),
            tokenizer_path: self.find_tokenizer_file(&downloaded_files),
            size_bytes: total_size,
            sha256: checksum,
            downloaded_at: chrono::Utc::now(),
            last_used: None,
            usage_count: 0,
            capabilities: self.infer_capabilities(&model_info),
            parameters: self.extract_parameters(&model_info),
            source: ModelSource {
                origin: "huggingface".to_string(),
                url: Some(format!("https://huggingface.co/{}", model_id)),
                repository: Some(model_id.clone()),
                commit: Some(model_info.sha),
                license: None, // TODO: Extract from model card
            },
        };

        // Store the model
        self.storage
            .store_model(&model_data, metadata.clone())
            .await?;

        // Copy additional files to storage if needed
        self.store_additional_files(&downloaded_files, &metadata)
            .await?;

        // Clean up temporary directory
        if fs::remove_dir_all(&temp_dir).is_err() {
            // Log warning but don't fail
        }

        Ok(metadata)
    }

    /// Download model from a direct URL
    async fn download_from_url(&mut self, request: DownloadRequest) -> Result<LocalModelMetadata> {
        let url = request
            .source
            .url
            .clone()
            .ok_or_else(|| anyhow::anyhow!("URL required for URL source"))?;

        let temp_path = self
            .cache_dir
            .join(format!("temp_{}", uuid::Uuid::new_v4()));
        let downloaded_size = self.download_file(&url, &temp_path).await?;

        let model_data = fs::read(&temp_path)?;

        // Calculate checksum
        let mut hasher = Sha256::new();
        hasher.update(&model_data);
        let checksum = format!("{:x}", hasher.finalize());

        // Infer format from URL or filename
        let format = self.infer_format_from_url(&url)?;

        let metadata = LocalModelMetadata {
            id: request.model_id.clone(),
            name: request.model_id.clone(),
            description: Some(format!("Downloaded from URL: {}", url)),
            version: "1.0".to_string(),
            format,
            file_path: String::new(),
            config_path: None,
            tokenizer_path: None,
            size_bytes: downloaded_size,
            sha256: checksum,
            downloaded_at: chrono::Utc::now(),
            last_used: None,
            usage_count: 0,
            capabilities: ModelCapabilities {
                chat: true,
                completions: true,
                embeddings: false,
                image_generation: false,
                image_understanding: false,
                audio_generation: false,
                audio_understanding: false,
                video_understanding: false,
                function_calling: true,
                streaming: true,
            },
            parameters: HashMap::new(),
            source: request.source,
        };

        self.storage
            .store_model(&model_data, metadata.clone())
            .await?;

        // Clean up temporary file
        if fs::remove_file(&temp_path).is_err() {
            // Log warning but don't fail
        }

        Ok(metadata)
    }

    /// Fetch model information from Hugging Face API
    async fn fetch_huggingface_model_info(&self, model_id: &str) -> Result<HuggingFaceModelInfo> {
        let url = format!("https://huggingface.co/api/models/{}", model_id);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "ai-model-api/1.0")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to fetch model info: HTTP {}",
                response.status()
            ));
        }

        let model_info: HuggingFaceModelInfo = response.json().await?;
        Ok(model_info)
    }

    /// Download a single file with progress tracking
    async fn download_file(&self, url: &str, local_path: &Path) -> Result<u64> {
        let response = self
            .client
            .get(url)
            .header("User-Agent", "ai-model-api/1.0")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to download file: HTTP {}",
                response.status()
            ));
        }

        let _total_size = response.content_length();
        let mut file = tokio::fs::File::create(local_path).await?;
        let mut downloaded = 0u64;

        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            // TODO: Report progress
        }

        file.flush().await?;
        Ok(downloaded)
    }

    /// Select which files to download based on preferences
    fn select_files_to_download(
        &self,
        model_info: &HuggingFaceModelInfo,
        request: &DownloadRequest,
    ) -> Result<Vec<HuggingFaceFile>> {
        let mut selected_files = Vec::new();

        // Priority order for different formats
        let format_extensions = match &request.format_preference {
            Some(ModelFormat::GGUF) => vec!["gguf"],
            Some(ModelFormat::SafeTensors) => vec!["safetensors"],
            Some(ModelFormat::PyTorch) => vec!["bin", "pt", "pth"],
            Some(ModelFormat::ONNX) => vec!["onnx"],
            _ => vec!["gguf", "safetensors", "bin", "pt", "onnx"], // Default priority
        };

        // Find model files
        for ext in &format_extensions {
            for file in &model_info.siblings {
                if file.rfilename.ends_with(&format!(".{}", ext)) {
                    // Prefer quantized versions if requested
                    if let Some(quant) = &request.quantization {
                        if file.rfilename.contains(quant) {
                            selected_files.push(file.clone());
                            break;
                        }
                    } else {
                        selected_files.push(file.clone());
                        break;
                    }
                }
            }
            if !selected_files.is_empty() {
                break;
            }
        }

        // Always include config and tokenizer files
        for file in &model_info.siblings {
            if file.rfilename == "config.json"
                || file.rfilename == "tokenizer.json"
                || file.rfilename == "tokenizer_config.json"
                || file.rfilename.starts_with("tokenizer")
            {
                selected_files.push(file.clone());
            }
        }

        Ok(selected_files)
    }

    /// Identify the main model file from downloaded files
    fn identify_main_model_file(
        &self,
        files: &[(HuggingFaceFile, std::path::PathBuf)],
    ) -> Result<(HuggingFaceFile, std::path::PathBuf, ModelFormat)> {
        for (file, path) in files {
            if file.rfilename.ends_with(".gguf") {
                return Ok((file.clone(), path.clone(), ModelFormat::GGUF));
            }
        }

        for (file, path) in files {
            if file.rfilename.ends_with(".safetensors") {
                return Ok((file.clone(), path.clone(), ModelFormat::SafeTensors));
            }
        }

        for (file, path) in files {
            if file.rfilename.ends_with(".bin") || file.rfilename.ends_with(".pt") {
                return Ok((file.clone(), path.clone(), ModelFormat::PyTorch));
            }
        }

        Err(anyhow::anyhow!("No suitable model file found"))
    }

    /// Find config file path
    fn find_config_file(&self, files: &[(HuggingFaceFile, std::path::PathBuf)]) -> Option<String> {
        for (file, path) in files {
            if file.rfilename == "config.json" {
                return Some(path.to_string_lossy().to_string());
            }
        }
        None
    }

    /// Find tokenizer file path
    fn find_tokenizer_file(
        &self,
        files: &[(HuggingFaceFile, std::path::PathBuf)],
    ) -> Option<String> {
        for (file, path) in files {
            if file.rfilename == "tokenizer.json" {
                return Some(path.to_string_lossy().to_string());
            }
        }
        None
    }

    /// Store additional files alongside the main model
    async fn store_additional_files(
        &self,
        _files: &[(HuggingFaceFile, std::path::PathBuf)],
        _metadata: &LocalModelMetadata,
    ) -> Result<()> {
        // TODO: Implement storing additional files
        // This would copy config.json, tokenizer files, etc. to the model directory
        Ok(())
    }

    /// Infer model capabilities from Hugging Face metadata
    fn infer_capabilities(&self, model_info: &HuggingFaceModelInfo) -> ModelCapabilities {
        let mut capabilities = ModelCapabilities {
            chat: false,
            completions: true,
            embeddings: false,
            image_generation: false,
            image_understanding: false,
            audio_generation: false,
            audio_understanding: false,
            video_understanding: false,
            function_calling: false,
            streaming: true,
        };

        // Infer from tags and pipeline_tag
        if let Some(pipeline) = &model_info.pipeline_tag {
            match pipeline.as_str() {
                "text-generation" => {
                    capabilities.chat = true;
                    capabilities.completions = true;
                }
                "feature-extraction" | "sentence-similarity" => {
                    capabilities.embeddings = true;
                }
                "text-to-image" => {
                    capabilities.image_generation = true;
                }
                "image-to-text" => {
                    capabilities.image_understanding = true;
                }
                _ => {}
            }
        }

        // Check tags for additional capabilities
        for tag in &model_info.tags {
            match tag.as_str() {
                "conversational" | "chat" => capabilities.chat = true,
                "function-calling" => capabilities.function_calling = true,
                "multimodal" => capabilities.image_understanding = true,
                _ => {}
            }
        }

        capabilities
    }

    /// Extract model parameters from metadata
    fn extract_parameters(
        &self,
        model_info: &HuggingFaceModelInfo,
    ) -> HashMap<String, serde_json::Value> {
        let mut params = HashMap::new();

        params.insert(
            "downloads".to_string(),
            serde_json::Value::Number(model_info.downloads.into()),
        );
        params.insert(
            "likes".to_string(),
            serde_json::Value::Number(model_info.likes.into()),
        );

        if let Some(library) = &model_info.library_name {
            params.insert(
                "library_name".to_string(),
                serde_json::Value::String(library.clone()),
            );
        }

        if let Some(pipeline) = &model_info.pipeline_tag {
            params.insert(
                "pipeline_tag".to_string(),
                serde_json::Value::String(pipeline.clone()),
            );
        }

        params.insert(
            "tags".to_string(),
            serde_json::Value::Array(
                model_info
                    .tags
                    .iter()
                    .map(|t| serde_json::Value::String(t.clone()))
                    .collect(),
            ),
        );

        params
    }

    /// Infer format from URL
    fn infer_format_from_url(&self, url: &str) -> Result<ModelFormat> {
        let url_parsed = Url::parse(url)?;
        let path = url_parsed.path();

        if path.ends_with(".gguf") {
            Ok(ModelFormat::GGUF)
        } else if path.ends_with(".safetensors") {
            Ok(ModelFormat::SafeTensors)
        } else if path.ends_with(".bin") || path.ends_with(".pt") || path.ends_with(".pth") {
            Ok(ModelFormat::PyTorch)
        } else if path.ends_with(".onnx") {
            Ok(ModelFormat::ONNX)
        } else {
            Err(anyhow::anyhow!("Cannot infer format from URL: {}", url))
        }
    }

    /// List available models from various sources
    pub async fn search_models(&self, query: &str, source: &str) -> Result<Vec<ModelSearchResult>> {
        match source {
            "huggingface" => self.search_huggingface_models(query).await,
            _ => Err(anyhow::anyhow!("Unsupported search source: {}", source)),
        }
    }

    /// Search Hugging Face models
    async fn search_huggingface_models(&self, query: &str) -> Result<Vec<ModelSearchResult>> {
        let url = format!(
            "https://huggingface.co/api/models?search={}&limit=20",
            urlencoding::encode(query)
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "ai-model-api/1.0")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Search failed: HTTP {}", response.status()));
        }

        let models: Vec<HuggingFaceModelInfo> = response.json().await?;

        let results = models
            .into_iter()
            .map(|model| ModelSearchResult {
                id: model.id.clone(),
                name: model.id,
                description: format!("Hugging Face model with {} downloads", model.downloads),
                source: "huggingface".to_string(),
                downloads: Some(model.downloads),
                likes: Some(model.likes),
                tags: model.tags,
                library_name: model.library_name,
                pipeline_tag: model.pipeline_tag,
            })
            .collect();

        Ok(results)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSearchResult {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub downloads: Option<u64>,
    pub likes: Option<u64>,
    pub tags: Vec<String>,
    pub library_name: Option<String>,
    pub pipeline_tag: Option<String>,
}
