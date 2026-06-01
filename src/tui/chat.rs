//! Chat interface components for multi-modal conversations

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use super::agents::{Modality, MultiModalAgent, MultiModalMessage};
use super::app::{ChatMessage, MessageRole};
use super::media::MediaFile;
use crate::ai;

#[derive(Debug, Clone)]
pub struct ChatSession {
    pub id: Uuid,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub participants: Vec<Participant>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub settings: ChatSettings,
}

#[derive(Debug, Clone)]
pub struct Participant {
    pub id: Uuid,
    pub name: String,
    pub participant_type: ParticipantType,
    pub model: Option<String>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParticipantType {
    User,
    Agent,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSettings {
    pub auto_summarize: bool,
    pub context_window_size: usize,
    pub enable_media_analysis: bool,
    pub temperature: f32,
    pub max_tokens: Option<usize>,
    pub system_prompt: Option<String>,
}

impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            auto_summarize: false,
            context_window_size: 4096,
            enable_media_analysis: true,
            temperature: 0.7,
            max_tokens: None,
            system_prompt: None,
        }
    }
}

impl ChatSession {
    pub fn new(title: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            title,
            messages: Vec::new(),
            participants: Vec::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            settings: ChatSettings::default(),
        }
    }

    pub fn add_participant(&mut self, participant: Participant) {
        self.participants.push(participant);
        self.updated_at = chrono::Utc::now();
    }

    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
        self.updated_at = chrono::Utc::now();

        // Auto-summarize if enabled and context window is full
        if self.settings.auto_summarize && self.messages.len() > self.settings.context_window_size {
            let _ = self.summarize_old_messages();
        }
    }

    pub fn process_multimodal_message(
        &mut self,
        content: String,
        media_files: Vec<MediaFile>,
        _sender_id: Uuid,
    ) -> Result<ChatMessage> {
        let mut message = ChatMessage {
            id: Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            role: MessageRole::User,
            content: content.clone(),
            media_attachments: media_files.clone(),
            model: None,
        };

        // If media files are attached and analysis is enabled, process them
        if !media_files.is_empty() && self.settings.enable_media_analysis {
            let mut enhanced_content = content;

            for media in &media_files {
                let analysis = self.analyze_media_file(media)?;
                enhanced_content.push_str(&format!("\n\n[Media Analysis: {}]", analysis));
            }

            message.content = enhanced_content;
        }

        self.add_message(message.clone());
        Ok(message)
    }

    fn analyze_media_file(&self, media: &MediaFile) -> Result<String> {
        match media.media_type {
            super::media::MediaType::Image => {
                // For image analysis, we'd send to a vision-capable model
                let prompt = format!(
                    "Analyze this image file: {}. Describe what you see in detail.",
                    media.path
                );
                ai::complete_sync_router(&prompt)
            }
            super::media::MediaType::Audio => {
                // For audio, we'd transcribe first then analyze
                Ok(format!(
                    "Audio file: {} (duration: {:?}s)",
                    media.path,
                    media.duration.unwrap_or(0.0)
                ))
            }
            super::media::MediaType::Video => {
                // For video, we'd extract keyframes and analyze
                Ok(format!(
                    "Video file: {} (duration: {:?}s)",
                    media.path,
                    media.duration.unwrap_or(0.0)
                ))
            }
            super::media::MediaType::Unknown => Ok(format!("Unknown media type: {}", media.path)),
        }
    }

    fn summarize_old_messages(&mut self) -> Result<()> {
        // Take the first half of messages for summarization
        let cutoff = self.messages.len() / 2;
        let messages_to_summarize = &self.messages[..cutoff];

        // Create summary prompt
        let summary_content = messages_to_summarize
            .iter()
            .map(|msg| format!("{:?}: {}", msg.role, msg.content))
            .collect::<Vec<_>>()
            .join("\n");

        let summary_prompt = format!(
            "Summarize this conversation concisely, preserving key information:\n\n{}",
            summary_content
        );

        let summary = ai::complete_sync_router(&summary_prompt)?;

        // Replace old messages with summary
        let summary_message = ChatMessage {
            id: Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            role: MessageRole::System,
            content: format!("[Conversation Summary] {}", summary),
            media_attachments: Vec::new(),
            model: Some("summarizer".to_string()),
        };

        // Keep recent messages and add summary at the beginning
        self.messages.drain(..cutoff);
        self.messages.insert(0, summary_message);

        Ok(())
    }

    pub fn export_to_markdown(&self) -> String {
        let mut markdown = format!("# {}\n\n", self.title);
        markdown.push_str(&format!(
            "Created: {}\n",
            self.created_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));
        markdown.push_str(&format!(
            "Participants: {}\n\n",
            self.participants
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        ));

        for message in &self.messages {
            let role_emoji = match message.role {
                MessageRole::User => "👤",
                MessageRole::Assistant => "🤖",
                MessageRole::System => "⚙️",
            };

            markdown.push_str(&format!(
                "## {} {} ({})\n\n{}\n\n",
                role_emoji,
                format!("{:?}", message.role),
                message.timestamp.format("%H:%M:%S"),
                message.content
            ));

            if !message.media_attachments.is_empty() {
                markdown.push_str("**Attachments:**\n");
                for media in &message.media_attachments {
                    markdown.push_str(&format!("- {}\n", media.display_info()));
                }
                markdown.push('\n');
            }
        }

        markdown
    }

    pub fn get_context_for_ai(&self, include_media: bool) -> String {
        let mut context = String::new();

        if let Some(system_prompt) = &self.settings.system_prompt {
            context.push_str(&format!("System: {}\n\n", system_prompt));
        }

        // Get recent messages within context window
        let start_idx = if self.messages.len() > self.settings.context_window_size {
            self.messages.len() - self.settings.context_window_size
        } else {
            0
        };

        for message in &self.messages[start_idx..] {
            context.push_str(&format!("{:?}: {}\n", message.role, message.content));

            if include_media && !message.media_attachments.is_empty() {
                context.push_str("Media: ");
                for media in &message.media_attachments {
                    context.push_str(&format!("[{}] ", media.display_info()));
                }
                context.push('\n');
            }
        }

        context
    }
}

/// Manager for multiple chat sessions
pub struct ChatManager {
    pub sessions: HashMap<Uuid, ChatSession>,
    pub active_session_id: Option<Uuid>,
    pub agents: HashMap<Uuid, MultiModalAgent>,
}

impl Default for ChatManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            active_session_id: None,
            agents: HashMap::new(),
        }
    }

    pub fn create_session(&mut self, title: String) -> Uuid {
        let session = ChatSession::new(title);
        let session_id = session.id;
        self.sessions.insert(session_id, session);
        self.active_session_id = Some(session_id);
        session_id
    }

    pub fn get_active_session(&self) -> Option<&ChatSession> {
        self.active_session_id.and_then(|id| self.sessions.get(&id))
    }

    pub fn get_active_session_mut(&mut self) -> Option<&mut ChatSession> {
        self.active_session_id
            .and_then(|id| self.sessions.get_mut(&id))
    }

    pub fn switch_session(&mut self, session_id: Uuid) -> Result<()> {
        if self.sessions.contains_key(&session_id) {
            self.active_session_id = Some(session_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found"))
        }
    }

    pub fn add_agent_to_session(&mut self, session_id: Uuid, agent: MultiModalAgent) -> Result<()> {
        let agent_id = agent.base_agent.id;

        if let Some(session) = self.sessions.get_mut(&session_id) {
            let participant = Participant {
                id: agent_id,
                name: agent.base_agent.name.clone(),
                participant_type: ParticipantType::Agent,
                model: Some(agent.base_agent.model.clone()),
                capabilities: agent.base_agent.tools.clone(),
            };

            session.add_participant(participant);
            self.agents.insert(agent_id, agent);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found"))
        }
    }

    pub fn process_agent_response(
        &mut self,
        session_id: Uuid,
        agent_id: Uuid,
        input: String,
    ) -> Result<String> {
        if let Some(agent) = self.agents.get_mut(&agent_id) {
            let multimodal_message = MultiModalMessage {
                content: input,
                modality: Modality::Text,
                metadata: HashMap::new(),
            };

            let response = agent.process_multimodal_input(multimodal_message)?;

            // Add response to session
            if let Some(session) = self.sessions.get_mut(&session_id) {
                let message = ChatMessage {
                    id: Uuid::new_v4(),
                    timestamp: chrono::Utc::now(),
                    role: MessageRole::Assistant,
                    content: response.clone(),
                    media_attachments: Vec::new(),
                    model: Some(agent.base_agent.model.clone()),
                };

                session.add_message(message);
            }

            Ok(response)
        } else {
            Err(anyhow::anyhow!("Agent not found"))
        }
    }
}
