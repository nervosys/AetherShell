//! Media handling for terminal display
//!
//! Supports images, videos, and audio files with terminal-friendly
//! display and processing capabilities.

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose, Engine};
use image::{DynamicImage, ImageFormat};
use std::path::Path;
use viuer::{print_from_file, Config as ViuerConfig};

#[derive(Debug, Clone)]
pub struct MediaFile {
    pub path: String,
    pub media_type: MediaType,
    pub size: Option<(u32, u32)>,  // width, height for images/videos
    pub duration: Option<f64>,     // duration in seconds for audio/video
    pub thumbnail: Option<String>, // base64 encoded thumbnail
}

#[derive(Debug, Clone, PartialEq)]
pub enum MediaType {
    Image,
    Video,
    Audio,
    Unknown,
}

impl MediaFile {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let media_type = detect_media_type(&path_str)?;

        let mut file = MediaFile {
            path: path_str,
            media_type,
            size: None,
            duration: None,
            thumbnail: None,
        };

        // Load metadata based on type
        match file.media_type {
            MediaType::Image => {
                if let Ok(img) = image::open(&file.path) {
                    file.size = Some((img.width(), img.height()));
                    file.thumbnail = generate_thumbnail(&img)?;
                }
            }
            MediaType::Video => {
                // For now, just mark as video - in real implementation
                // you'd use ffmpeg or similar to extract metadata
                file.duration = Some(0.0); // Placeholder
            }
            MediaType::Audio => {
                // Placeholder for audio duration detection
                file.duration = Some(0.0);
            }
            MediaType::Unknown => {}
        }

        Ok(file)
    }

    /// Display the media in the terminal
    pub fn display_in_terminal(&self, max_width: u16, max_height: u16) -> Result<()> {
        match self.media_type {
            MediaType::Image => {
                let conf = ViuerConfig {
                    transparent: false,
                    absolute_offset: false,
                    width: Some(max_width as u32),
                    height: Some(max_height as u32),
                    ..Default::default()
                };

                print_from_file(&self.path, &conf)
                    .map_err(|e| anyhow!("Failed to display image: {}", e))?;
            }
            MediaType::Video => {
                // For videos, we could extract frames or show a placeholder
                println!("🎬 Video: {} (duration: {:?}s)", self.path, self.duration);
            }
            MediaType::Audio => {
                println!("🎵 Audio: {} (duration: {:?}s)", self.path, self.duration);
            }
            MediaType::Unknown => {
                println!("❓ Unknown media type: {}", self.path);
            }
        }
        Ok(())
    }

    /// Get a text representation for UI display
    pub fn display_info(&self) -> String {
        match self.media_type {
            MediaType::Image => {
                if let Some((w, h)) = self.size {
                    format!(
                        "🖼️  {}x{} - {}",
                        w,
                        h,
                        Path::new(&self.path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    )
                } else {
                    format!(
                        "🖼️  {}",
                        Path::new(&self.path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    )
                }
            }
            MediaType::Video => {
                format!(
                    "🎬 {} ({:.1}s)",
                    Path::new(&self.path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                    self.duration.unwrap_or(0.0)
                )
            }
            MediaType::Audio => {
                format!(
                    "🎵 {} ({:.1}s)",
                    Path::new(&self.path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                    self.duration.unwrap_or(0.0)
                )
            }
            MediaType::Unknown => {
                format!(
                    "❓ {}",
                    Path::new(&self.path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )
            }
        }
    }
}

fn detect_media_type(path: &str) -> Result<MediaType> {
    let extension = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        // Images
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tiff" | "svg" => Ok(MediaType::Image),
        // Videos
        "mp4" | "avi" | "mov" | "mkv" | "wmv" | "flv" | "webm" | "m4v" => Ok(MediaType::Video),
        // Audio
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "wma" | "m4a" => Ok(MediaType::Audio),
        _ => Ok(MediaType::Unknown),
    }
}

fn generate_thumbnail(img: &DynamicImage) -> Result<Option<String>> {
    // Create a small thumbnail for quick preview
    let thumbnail = img.thumbnail(64, 64);
    let mut buffer = Vec::new();
    thumbnail
        .write_to(&mut std::io::Cursor::new(&mut buffer), ImageFormat::Png)
        .map_err(|e| anyhow!("Failed to generate thumbnail: {}", e))?;

    Ok(Some(general_purpose::STANDARD.encode(buffer)))
}

/// Media player functionality for audio files
pub struct AudioPlayer {
    // In a full implementation, this would use rodio for playback
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn play(&self, _path: &str) -> Result<()> {
        // Placeholder - would use rodio to play audio
        println!("🎵 Playing audio...");
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        println!("⏹️  Stopped audio");
        Ok(())
    }
}

/// Utility functions for media processing
pub fn get_supported_formats() -> Vec<&'static str> {
    vec![
        // Images
        "jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff", "svg", // Videos
        "mp4", "avi", "mov", "mkv", "wmv", "flv", "webm", "m4v", // Audio
        "mp3", "wav", "flac", "aac", "ogg", "wma", "m4a",
    ]
}

pub fn is_media_file(path: &str) -> bool {
    let extension = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    get_supported_formats().contains(&extension.as_str())
}
