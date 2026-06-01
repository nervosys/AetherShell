//! Tests for multimodal AI functionality

#[cfg(test)]
mod multimodal_ai_tests {
    use aethershell::ai::{
        complete_multimodal_sync, MultiModalContent, MultiModalLlmBackend, MultiModalMessage,
    };

    #[test]
    fn test_multimodal_content_creation() {
        let content = MultiModalContent {
            text: Some("Describe this image".to_string()),
            image_url: Some("https://example.com/image.jpg".to_string()),
            audio_url: None,
            video_url: None,
            image_data: None,
            audio_data: None,
            video_data: None,
        };

        assert_eq!(content.text, Some("Describe this image".to_string()));
        assert_eq!(
            content.image_url,
            Some("https://example.com/image.jpg".to_string())
        );
        assert_eq!(content.audio_url, None);
    }

    #[test]
    fn test_multimodal_message_text_only() {
        let message = MultiModalMessage::text_only("user", "Hello, AI!");

        assert_eq!(message.role, "user");
        assert_eq!(message.content.len(), 1);
        assert_eq!(message.content[0].text, Some("Hello, AI!".to_string()));
        assert_eq!(message.content[0].image_data, None);
    }

    #[test]
    fn test_multimodal_message_with_image() {
        let image_data = "base64_encoded_image_data";
        let message = MultiModalMessage::with_image("user", "What's in this image?", image_data);

        assert_eq!(message.role, "user");
        assert_eq!(message.content.len(), 2);

        // First content part should be text
        assert_eq!(
            message.content[0].text,
            Some("What's in this image?".to_string())
        );
        assert_eq!(message.content[0].image_data, None);

        // Second content part should be image
        assert_eq!(message.content[1].text, None);
        assert_eq!(message.content[1].image_data, Some(image_data.to_string()));
    }

    #[test]
    fn test_multimodal_message_with_audio() {
        let audio_data = "base64_encoded_audio_data";
        let message = MultiModalMessage::with_audio("user", "Transcribe this audio", audio_data);

        assert_eq!(message.role, "user");
        assert_eq!(message.content.len(), 2);

        // First content part should be text
        assert_eq!(
            message.content[0].text,
            Some("Transcribe this audio".to_string())
        );
        assert_eq!(message.content[0].audio_data, None);

        // Second content part should be audio
        assert_eq!(message.content[1].text, None);
        assert_eq!(message.content[1].audio_data, Some(audio_data.to_string()));
    }

    #[test]
    fn test_multimodal_message_to_text() {
        let message = MultiModalMessage {
            role: "user".to_string(),
            content: vec![
                MultiModalContent {
                    text: Some("First part".to_string()),
                    image_url: None,
                    audio_url: None,
                    video_url: None,
                    image_data: None,
                    audio_data: None,
                    video_data: None,
                },
                MultiModalContent {
                    text: Some("Second part".to_string()),
                    image_url: None,
                    audio_url: None,
                    video_url: None,
                    image_data: None,
                    audio_data: None,
                    video_data: None,
                },
                MultiModalContent {
                    text: None,
                    image_url: None,
                    audio_url: None,
                    video_url: None,
                    image_data: Some("image_data".to_string()),
                    audio_data: None,
                    video_data: None,
                },
            ],
        };

        let text = message.to_text();
        assert_eq!(text, "First part Second part");
    }

    #[test]
    fn test_multimodal_sync_completion() {
        // Test with stub backend (should work without external dependencies)
        let messages = vec![MultiModalMessage::text_only(
            "user",
            "Hello, multimodal AI!",
        )];

        // This should use the stub backend and return a response
        let result = complete_multimodal_sync(&messages);

        // If AETHER_AI is not set, the result may be an error
        if std::env::var("AETHER_AI").is_err() {
            // Skip assertion if AI not configured
            return;
        }

        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(!response.is_empty());
        assert!(response.contains("[ai:stub]"));
    }

    #[test]
    fn test_complex_multimodal_conversation() {
        let messages = [
            MultiModalMessage::text_only("system", "You are a helpful multimodal assistant."),
            MultiModalMessage::with_image(
                "user",
                "What do you see in this image?",
                "fake_image_data",
            ),
            MultiModalMessage::text_only("assistant", "I can see various elements in the image."),
            MultiModalMessage::with_audio(
                "user",
                "Can you also analyze this audio?",
                "fake_audio_data",
            ),
        ];

        // Test that all messages are properly formed
        assert_eq!(messages.len(), 4);

        // System message
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content.len(), 1);

        // User message with image
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[1].content.len(), 2);
        assert_eq!(
            messages[1].content[1].image_data,
            Some("fake_image_data".to_string())
        );

        // Assistant response
        assert_eq!(messages[2].role, "assistant");
        assert_eq!(messages[2].content.len(), 1);

        // User message with audio
        assert_eq!(messages[3].role, "user");
        assert_eq!(messages[3].content.len(), 2);
        assert_eq!(
            messages[3].content[1].audio_data,
            Some("fake_audio_data".to_string())
        );
    }

    #[test]
    fn test_empty_multimodal_message() {
        let message = MultiModalMessage {
            role: "user".to_string(),
            content: vec![],
        };

        assert_eq!(message.role, "user");
        assert!(message.content.is_empty());
        assert_eq!(message.to_text(), "");
    }

    #[test]
    fn test_mixed_content_types() {
        let content = vec![
            MultiModalContent {
                text: Some("Look at this image and listen to this audio:".to_string()),
                image_url: None,
                audio_url: None,
                video_url: None,
                image_data: None,
                audio_data: None,
                video_data: None,
            },
            MultiModalContent {
                text: None,
                image_url: Some("https://example.com/image.jpg".to_string()),
                audio_url: None,
                video_url: None,
                image_data: None,
                audio_data: None,
                video_data: None,
            },
            MultiModalContent {
                text: None,
                image_url: None,
                audio_url: Some("https://example.com/audio.mp3".to_string()),
                video_url: None,
                image_data: None,
                audio_data: None,
                video_data: None,
            },
        ];

        let message = MultiModalMessage {
            role: "user".to_string(),
            content,
        };

        assert_eq!(message.content.len(), 3);
        assert_eq!(
            message.content[0].text,
            Some("Look at this image and listen to this audio:".to_string())
        );
        assert_eq!(
            message.content[1].image_url,
            Some("https://example.com/image.jpg".to_string())
        );
        assert_eq!(
            message.content[2].audio_url,
            Some("https://example.com/audio.mp3".to_string())
        );

        let text_only = message.to_text();
        assert_eq!(text_only, "Look at this image and listen to this audio:");
    }

    // Test stub backend capabilities
    struct TestMultiModalBackend;

    impl MultiModalLlmBackend for TestMultiModalBackend {
        fn chat_multimodal(&self, messages: &[MultiModalMessage]) -> anyhow::Result<String> {
            let text_content = messages
                .iter()
                .map(|m| m.to_text())
                .collect::<Vec<_>>()
                .join(" ");
            Ok(format!("Test response to: {}", text_content))
        }

        fn supports_images(&self) -> bool {
            true
        }
        fn supports_audio(&self) -> bool {
            false
        }
        fn supports_video(&self) -> bool {
            false
        }
    }

    #[test]
    fn test_custom_multimodal_backend() {
        let backend = TestMultiModalBackend;

        assert!(backend.supports_images());
        assert!(!backend.supports_audio());
        assert!(!backend.supports_video());

        let messages = vec![MultiModalMessage::text_only("user", "Test message")];

        let result = backend.chat_multimodal(&messages);
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.contains("Test response to: Test message"));
    }
}
