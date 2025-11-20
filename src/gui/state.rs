use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::PathBuf;

const STATE_FILE: &str = "/tmp/transcribe-rs-conversation-state.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String, // "user" or "assistant"
    pub content: String,
    pub timestamp: DateTime<Local>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationState {
    pub messages: Vec<Message>,
    pub conversation_file: PathBuf,
    pub window_open: bool,
}

impl ConversationState {
    pub fn new(conversation_file: PathBuf) -> Self {
        Self {
            messages: Vec::new(),
            conversation_file,
            window_open: false,
        }
    }

    pub fn add_message(&mut self, role: &str, content: String) {
        self.messages.push(Message {
            role: role.to_string(),
            content,
            timestamp: Local::now(),
        });
    }

    pub fn load() -> Result<Self, Box<dyn Error>> {
        let content = fs::read_to_string(STATE_FILE)?;
        let state: ConversationState = serde_json::from_str(&content)?;
        Ok(state)
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(STATE_FILE, content)?;
        Ok(())
    }

    pub fn exists() -> bool {
        PathBuf::from(STATE_FILE).exists()
    }

    pub fn delete() -> Result<(), Box<dyn Error>> {
        if Self::exists() {
            fs::remove_file(STATE_FILE)?;
        }
        Ok(())
    }

    pub fn save_to_markdown(&self) -> Result<(), Box<dyn Error>> {
        let mut markdown = String::new();
        markdown.push_str("# Conversation\n\n");
        markdown.push_str(&format!(
            "_Started: {}_\n\n",
            self.messages
                .first()
                .map(|m| m.timestamp.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default()
        ));
        markdown.push_str("---\n\n");

        for msg in &self.messages {
            let role = if msg.role == "user" {
                "**User:**"
            } else {
                "**Assistant:**"
            };
            markdown.push_str(&format!(
                "{} _{}_\n\n{}\n\n",
                role,
                msg.timestamp.format("%H:%M:%S"),
                msg.content
            ));
        }

        // Ensure conversations directory exists
        if let Some(parent) = self.conversation_file.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&self.conversation_file, markdown)?;
        Ok(())
    }

    pub fn get_context_for_llm(&self) -> Vec<(String, String)> {
        self.messages
            .iter()
            .map(|msg| (msg.role.clone(), msg.content.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_conversation_state_creation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("conversation.md");
        let state = ConversationState::new(path.clone());

        assert_eq!(state.messages.len(), 0);
        assert_eq!(state.conversation_file, path);
        assert_eq!(state.window_open, false);
    }

    #[test]
    fn test_add_message() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("conversation.md");
        let mut state = ConversationState::new(path);

        state.add_message("user", "Hello".to_string());
        state.add_message("assistant", "Hi there".to_string());

        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.messages[0].role, "user");
        assert_eq!(state.messages[0].content, "Hello");
        assert_eq!(state.messages[1].role, "assistant");
        assert_eq!(state.messages[1].content, "Hi there");
    }

    #[test]
    fn test_save_to_markdown() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("conversation.md");
        let mut state = ConversationState::new(path.clone());

        state.add_message("user", "Test question".to_string());
        state.add_message("assistant", "Test answer".to_string());

        state.save_to_markdown().unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Conversation"));
        assert!(content.contains("**User:**"));
        assert!(content.contains("Test question"));
        assert!(content.contains("**Assistant:**"));
        assert!(content.contains("Test answer"));
    }
}
