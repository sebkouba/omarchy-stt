//! Conversation history management for multi-turn LLM conversations

use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Manages conversation history for multi-turn LLM conversations
pub struct ConversationHistory {
    history_file: PathBuf,
    max_age_seconds: u64,
    max_turns: usize,
}

/// A single entry in the conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: u64,  // Unix timestamp in seconds
    pub role: String,    // "user" or "assistant"
    pub content: String,
    #[serde(default)]
    pub is_first: bool,  // True if this is the first user message with prompt instructions
}

/// Message format compatible with groq.rs Message struct
#[derive(Debug, Clone)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
}

impl ConversationHistory {
    /// Creates a new conversation history manager
    ///
    /// # Arguments
    /// * `prompt_name` - Name of the prompt (e.g., "clean", "email")
    /// * `max_age_minutes` - How many minutes of history to keep
    /// * `max_turns` - Maximum number of turns (user+assistant pairs)
    /// * `history_dir` - Directory to store history files
    pub fn new(prompt_name: &str, max_age_minutes: u32, max_turns: usize, history_dir: &str) -> Self {
        // Handle empty prompt name with fallback
        let safe_prompt_name = if prompt_name.is_empty() { "default" } else { prompt_name };

        let history_file = PathBuf::from(history_dir)
            .join(format!("transcribe-rs-v2-history-{}.jsonl", safe_prompt_name));

        Self {
            history_file,
            max_age_seconds: (max_age_minutes as u64) * 60,
            max_turns,
        }
    }

    /// Load conversation history, filtered by time and turn limit
    ///
    /// Returns a vector of messages in chronological order
    pub fn load_history(&self) -> Result<Vec<HistoryMessage>, Box<dyn Error>> {
        // If file doesn't exist, return empty history
        if !self.history_file.exists() {
            return Ok(Vec::new());
        }

        // Read all entries
        let file = fs::File::open(&self.history_file)?;
        let reader = BufReader::new(file);
        let mut entries: Vec<HistoryEntry> = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<HistoryEntry>(&line) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    eprintln!("Warning: Failed to parse history line: {} - {}", line, e);
                    continue;
                }
            }
        }

        // Filter by time
        let entries = self.filter_by_time(entries);

        // Limit to max turns
        let entries = self.limit_turns(entries);

        // Convert to HistoryMessage format
        let messages = entries
            .into_iter()
            .map(|entry| HistoryMessage {
                role: entry.role,
                content: entry.content,
            })
            .collect();

        Ok(messages)
    }

    /// Append a user message to history
    pub fn append_user(&self, content: &str, is_first: bool) -> Result<(), Box<dyn Error>> {
        let entry = HistoryEntry {
            timestamp: Self::current_timestamp(),
            role: "user".to_string(),
            content: content.to_string(),
            is_first,
        };
        self.append_entry(&entry)
    }

    /// Append an assistant message to history
    pub fn append_assistant(&self, content: &str) -> Result<(), Box<dyn Error>> {
        let entry = HistoryEntry {
            timestamp: Self::current_timestamp(),
            role: "assistant".to_string(),
            content: content.to_string(),
            is_first: false,
        };
        self.append_entry(&entry)
    }

    /// Clear the conversation history (delete the file)
    pub fn clear(&self) -> Result<(), Box<dyn Error>> {
        if self.history_file.exists() {
            fs::remove_file(&self.history_file)?;
        }
        Ok(())
    }

    /// Get current Unix timestamp in seconds
    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
    }

    /// Filter entries by timestamp, keeping only recent messages
    fn filter_by_time(&self, entries: Vec<HistoryEntry>) -> Vec<HistoryEntry> {
        let now = Self::current_timestamp();
        let cutoff = now.saturating_sub(self.max_age_seconds);

        entries
            .into_iter()
            .filter(|entry| entry.timestamp >= cutoff)
            .collect()
    }

    /// Limit to max turns (each turn = user + assistant pair)
    fn limit_turns(&self, entries: Vec<HistoryEntry>) -> Vec<HistoryEntry> {
        // Count complete turns (user + assistant pairs)
        // Keep only the most recent max_turns
        if entries.len() <= self.max_turns * 2 {
            return entries;
        }

        // Take the last max_turns * 2 messages
        let start_index = entries.len() - (self.max_turns * 2);
        entries[start_index..].to_vec()
    }

    /// Append an entry to the JSONL file
    fn append_entry(&self, entry: &HistoryEntry) -> Result<(), Box<dyn Error>> {
        // Prune old entries periodically to prevent unbounded growth
        self.maybe_prune_old_entries()?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.history_file)?;

        let json = serde_json::to_string(entry)?;
        writeln!(file, "{}", json)?;

        Ok(())
    }

    /// Prune old entries from the history file to prevent unbounded growth
    fn maybe_prune_old_entries(&self) -> Result<(), Box<dyn Error>> {
        if !self.history_file.exists() {
            return Ok(());
        }

        // Check file size - prune if > 1MB
        let metadata = fs::metadata(&self.history_file)?;
        if metadata.len() < 1_000_000 {
            return Ok(());
        }

        log("History file exceeded 1MB, pruning old entries");

        // Read all entries
        let file = fs::File::open(&self.history_file)?;
        let reader = BufReader::new(file);
        let mut entries: Vec<HistoryEntry> = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
                entries.push(entry);
            }
        }

        // Filter by time
        let entries = self.filter_by_time(entries);

        // Rewrite file with only recent entries
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&self.history_file)?;

        for entry in entries {
            let json = serde_json::to_string(&entry)?;
            writeln!(file, "{}", json)?;
        }

        log(&format!("Pruned history file: {}", self.history_file.display()));

        Ok(())
    }
}

/// Logs a message to the debug log
fn log(message: &str) {
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/ptt_rust_debug.log")
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        writeln!(file, "[{}] [conversation_history] {}", timestamp, message).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_by_time() {
        let history = ConversationHistory::new("test", 1, 10, "/tmp");

        let now = ConversationHistory::current_timestamp();
        let entries = vec![
            HistoryEntry {
                timestamp: now - 120, // 2 minutes ago (should be filtered)
                role: "user".to_string(),
                content: "old message".to_string(),
                is_first: true,
            },
            HistoryEntry {
                timestamp: now - 30, // 30 seconds ago (should be kept)
                role: "user".to_string(),
                content: "recent message".to_string(),
                is_first: false,
            },
        ];

        let filtered = history.filter_by_time(entries);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].content, "recent message");
    }

    #[test]
    fn test_limit_turns() {
        let history = ConversationHistory::new("test", 5, 2, "/tmp");

        let entries = vec![
            HistoryEntry {
                timestamp: 1,
                role: "user".to_string(),
                content: "msg 1".to_string(),
                is_first: true,
            },
            HistoryEntry {
                timestamp: 2,
                role: "assistant".to_string(),
                content: "response 1".to_string(),
                is_first: false,
            },
            HistoryEntry {
                timestamp: 3,
                role: "user".to_string(),
                content: "msg 2".to_string(),
                is_first: false,
            },
            HistoryEntry {
                timestamp: 4,
                role: "assistant".to_string(),
                content: "response 2".to_string(),
                is_first: false,
            },
            HistoryEntry {
                timestamp: 5,
                role: "user".to_string(),
                content: "msg 3".to_string(),
                is_first: false,
            },
            HistoryEntry {
                timestamp: 6,
                role: "assistant".to_string(),
                content: "response 3".to_string(),
                is_first: false,
            },
        ];

        let limited = history.limit_turns(entries);
        assert_eq!(limited.len(), 4); // 2 turns = 4 messages
        assert_eq!(limited[0].content, "msg 2");
        assert_eq!(limited[3].content, "response 3");
    }

    #[test]
    fn test_append_and_load() {
        let temp_dir = std::env::temp_dir();
        let test_prompt = format!("test-{}", ConversationHistory::current_timestamp());
        let history = ConversationHistory::new(&test_prompt, 5, 10, temp_dir.to_str().unwrap());

        // Clear any existing history
        history.clear().unwrap();

        // Append some messages
        history.append_user("Hello", true).unwrap();
        history.append_assistant("Hi there!").unwrap();
        history.append_user("How are you?", false).unwrap();

        // Load and verify
        let messages = history.load_history().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[2].content, "How are you?");

        // Cleanup
        history.clear().unwrap();
    }

    #[test]
    fn test_empty_prompt_name() {
        let history = ConversationHistory::new("", 5, 10, "/tmp");
        assert!(history.history_file.to_string_lossy().contains("transcribe-rs-v2-history-default.jsonl"));
    }
}
