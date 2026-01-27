//! Tests for TUI media handling functionality

#[cfg(test)]
mod tui_media_tests {
    use aethershell::tui::media::{
        AudioPlayer, MediaFile, MediaType, get_supported_formats, is_media_file,
    };

    #[test]
    fn test_media_type_detection() {
        // Test image formats
        assert_eq!(
            MediaFile::from_path("test.jpg").unwrap().media_type,
            MediaType::Image
        );
        assert_eq!(
            MediaFile::from_path("test.PNG").unwrap().media_type,
            MediaType::Image
        );
        assert_eq!(
            MediaFile::from_path("image.webp").unwrap().media_type,
            MediaType::Image
        );

        // Test video formats
        assert_eq!(
            MediaFile::from_path("movie.mp4").unwrap().media_type,
            MediaType::Video
        );
        assert_eq!(
            MediaFile::from_path("video.AVI").unwrap().media_type,
            MediaType::Video
        );

        // Test audio formats
        assert_eq!(
            MediaFile::from_path("song.mp3").unwrap().media_type,
            MediaType::Audio
        );
        assert_eq!(
            MediaFile::from_path("audio.WAV").unwrap().media_type,
            MediaType::Audio
        );

        // Test unknown formats
        assert_eq!(
            MediaFile::from_path("document.txt").unwrap().media_type,
            MediaType::Unknown
        );
    }

    #[test]
    fn test_display_info_formatting() {
        let image_file = MediaFile {
            path: "/path/to/image.jpg".to_string(),
            media_type: MediaType::Image,
            size: Some((1920, 1080)),
            duration: None,
            thumbnail: None,
        };

        let info = image_file.display_info();
        assert!(info.contains("🖼️"));
        assert!(info.contains("1920x1080"));
        assert!(info.contains("image.jpg"));

        let video_file = MediaFile {
            path: "/path/to/video.mp4".to_string(),
            media_type: MediaType::Video,
            size: None,
            duration: Some(120.5),
            thumbnail: None,
        };

        let info = video_file.display_info();
        assert!(info.contains("🎬"));
        assert!(info.contains("120.5s"));
        assert!(info.contains("video.mp4"));
    }

    #[test]
    fn test_supported_formats() {
        let formats = get_supported_formats();

        // Check that common formats are supported
        assert!(formats.contains(&"jpg"));
        assert!(formats.contains(&"png"));
        assert!(formats.contains(&"mp4"));
        assert!(formats.contains(&"mp3"));
        assert!(formats.contains(&"wav"));

        // Check case insensitive detection
        assert!(is_media_file("test.JPG"));
        assert!(is_media_file("movie.MP4"));
        assert!(!is_media_file("document.txt"));
    }

    #[test]
    fn test_audio_player_creation() {
        let player = AudioPlayer::new();

        // Test basic operations don't panic
        assert!(player.play("test.mp3").is_ok());
        assert!(player.stop().is_ok());
    }

    #[test]
    fn test_media_file_path_handling() {
        // Test with various path formats
        let paths = vec![
            "image.jpg",
            "./images/photo.png",
            "/absolute/path/video.mp4",
            "C:\\Windows\\audio.wav",
        ];

        for path in paths {
            let result = MediaFile::from_path(path);
            assert!(
                result.is_ok(),
                "Failed to create MediaFile for path: {}",
                path
            );

            let media_file = result.unwrap();
            assert_eq!(media_file.path, path);
        }
    }

    #[test]
    fn test_media_type_case_insensitive() {
        let test_cases = vec![
            ("file.JPG", MediaType::Image),
            ("file.Mp4", MediaType::Video),
            ("file.MP3", MediaType::Audio),
            ("file.TXT", MediaType::Unknown),
        ];

        for (filename, expected_type) in test_cases {
            let media_file = MediaFile::from_path(filename).unwrap();
            assert_eq!(media_file.media_type, expected_type);
        }
    }

    #[test]
    fn test_filename_extraction() {
        let media_file = MediaFile::from_path("/long/path/to/file.jpg").unwrap();
        let info = media_file.display_info();
        assert!(info.contains("file.jpg"));
        assert!(!info.contains("/long/path/to/"));
    }
}
