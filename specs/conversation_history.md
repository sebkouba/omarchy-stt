# Conversation History for LLM Dictations

## Problem Statement

Currently, each LLM dictation is processed independently without context from previous dictations. Users should be able to have multi-turn conversations where:

1. User asks "How do I install this tool?"
2. LLM responds with generic instructions
3. User clarifies "I'm on Arch Linux"
4. LLM responds with Arch-specific instructions, referencing the original question

## Current Architecture

Looking at `src/groq.rs:159-181`, the current message structure is:

```rust
messages = [
    System: "You are Kimi, an AI assistant created by Moonshot AI.",
    User: "{prompt instructions}\n\nOriginal dictation:\n{transcription}"
]
```

Each dictation is isolated - no history is maintained between calls.

## Proposed Solution

### Message Flow Structure

**First dictation in a conversation:**
```
System: [Kimi system prompt]
User: [prompt instructions]\n\nOriginal dictation:\n[dictation 1]
Assistant: [response 1]
```

**Subsequent dictations (within time window):**
```
System: [Kimi system prompt]
User: [prompt instructions]\n\nOriginal dictation:\n[dictation 1]
Assistant: [response 1]
User: [dictation 2]  // Just raw dictation, no prompt prefix
Assistant: [response 2]
User: [dictation 3]
```

**Key insight**: The prompt instructions are only included in the first user message. This sets the context/role for the entire conversation. Subsequent messages are just the raw dictations, creating a natural back-and-forth.

### Key Design Decisions

1. **First message includes prompt instructions** - Sets the context/role for the conversation
2. **Subsequent messages are raw dictations** - Natural back-and-forth, no repeated instructions
3. **Time-based filtering** - Only include messages from the last N minutes (configurable)
4. **Per-prompt history** - Each prompt file maintains its own conversation history
5. **Automatic reset** - If time window expires, next dictation starts a new conversation

### History Storage Format

**Location**: `/tmp/transcribe-rs-v2-history-{prompt_name}.jsonl`

**Format**: JSON Lines (one JSON object per line)

```jsonl
{"timestamp": 1699564800, "role": "user", "content": "prompt instructions + dictation 1", "is_first": true}
{"timestamp": 1699564801, "role": "assistant", "content": "response 1"}
{"timestamp": 1699564850, "role": "user", "content": "dictation 2", "is_first": false}
{"timestamp": 1699564851, "role": "assistant", "content": "response 2"}
```

Why JSONL?
- Simple append-only operations
- Easy to parse line-by-line
- Standard format for conversation logs
- No need to rewrite entire file on each append

The `is_first` flag helps us track which message started the conversation (for debugging/logging purposes).

### Configuration Changes

Add to `src/config.rs` in a new `LlmConfig` section:

```toml
[llm]
# Enable conversation history (multi-turn conversations)
conversation_history_enabled = true

# How many minutes of conversation history to include
conversation_history_minutes = 5

# Maximum number of message pairs to include (prevents token overflow)
# Each turn = 1 user message + 1 assistant message
conversation_max_turns = 10

# Directory for history files (defaults to /tmp)
conversation_history_dir = "/tmp"
```

### Implementation Plan

#### 1. Update Config (src/config.rs)

**Add new struct:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Enable conversation history for multi-turn conversations
    pub conversation_history_enabled: bool,
    /// How many minutes of history to include
    pub conversation_history_minutes: u32,
    /// Maximum number of turns (user+assistant pairs) to include
    pub conversation_max_turns: usize,
    /// Directory for storing history files
    pub conversation_history_dir: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        LlmConfig {
            conversation_history_enabled: true,
            conversation_history_minutes: 5,
            conversation_max_turns: 10,
            conversation_history_dir: "/tmp".to_string(),
        }
    }
}
```

**Add to main Config:**
```rust
pub struct Config {
    // ... existing fields ...
    #[serde(default)]
    pub llm: LlmConfig,
}
```

#### 2. Create Conversation History Module (src/conversation_history.rs)

```rust
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
        let history_file = PathBuf::from(history_dir)
            .join(format!("transcribe-rs-v2-history-{}.jsonl", prompt_name));

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
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.history_file)?;

        let json = serde_json::to_string(entry)?;
        writeln!(file, "{}", json)?;

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
    use std::thread;
    use std::time::Duration;

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
}
```

#### 3. Update Groq Client (src/groq.rs)

**Modify the complete method signature:**
```rust
// Old signature:
pub fn complete(&self, prompt: &str, transcription: &str) -> Result<CompletionResult, Box<dyn Error>>

// New signature:
pub fn complete(&self, prompt: &str, transcription: &str, prompt_name: &str) -> Result<CompletionResult, Box<dyn Error>>
```

**Update complete_async to use history:**
```rust
async fn complete_async(&self, prompt: &str, transcription: &str, prompt_name: &str) -> Result<CompletionResult, Box<dyn Error>> {
    let config = crate::config::Config::load()?;

    // Build messages with or without history based on config
    let mut messages = if config.llm.conversation_history_enabled {
        self.build_messages_with_history(prompt, transcription, prompt_name, &config.llm)?
    } else {
        self.build_messages_without_history(prompt, transcription)
    };

    // ... existing tool calling loop ...

    // After getting final response, save to history if enabled
    if config.llm.conversation_history_enabled && let Some(ref final_content) = final_response_content {
        self.save_to_history(prompt, transcription, prompt_name, final_content, &config.llm)?;
    }

    Ok(CompletionResult {
        text: final_content,
        tool_called: tool_was_called,
    })
}
```

**Add new helper methods:**
```rust
/// Build messages without history (legacy behavior)
fn build_messages_without_history(&self, prompt: &str, transcription: &str) -> Vec<Message> {
    vec![
        Message {
            role: "system".to_string(),
            content: Some("You are Kimi, an AI assistant created by Moonshot AI.".to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        Message {
            role: "user".to_string(),
            content: Some(format!("{}\n\nOriginal dictation:\n{}", prompt, transcription)),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ]
}

/// Build messages with conversation history
fn build_messages_with_history(&self, prompt: &str, transcription: &str, prompt_name: &str, llm_config: &crate::config::LlmConfig) -> Result<Vec<Message>, Box<dyn Error>> {
    use crate::conversation_history::ConversationHistory;

    let history = ConversationHistory::new(
        prompt_name,
        llm_config.conversation_history_minutes,
        llm_config.conversation_max_turns,
        &llm_config.conversation_history_dir,
    );

    let history_messages = history.load_history()?;

    log(&format!("Loaded {} history messages for prompt '{}'", history_messages.len(), prompt_name));

    let mut messages = vec![
        Message {
            role: "system".to_string(),
            content: Some("You are Kimi, an AI assistant created by Moonshot AI.".to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    ];

    if history_messages.is_empty() {
        // First message in conversation - include prompt instructions
        log("No history found, starting new conversation");
        messages.push(Message {
            role: "user".to_string(),
            content: Some(format!("{}\n\nOriginal dictation:\n{}", prompt, transcription)),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    } else {
        // Continuing conversation - add history then new dictation
        log("Continuing existing conversation");

        // Add all history messages
        for history_msg in history_messages {
            messages.push(Message {
                role: history_msg.role,
                content: Some(history_msg.content),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }

        // Add new dictation (just the raw transcription, no prompt)
        messages.push(Message {
            role: "user".to_string(),
            content: Some(transcription.to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }

    Ok(messages)
}

/// Save conversation turn to history
fn save_to_history(&self, prompt: &str, transcription: &str, prompt_name: &str, assistant_response: &str, llm_config: &crate::config::LlmConfig) -> Result<(), Box<dyn Error>> {
    use crate::conversation_history::ConversationHistory;

    let history = ConversationHistory::new(
        prompt_name,
        llm_config.conversation_history_minutes,
        llm_config.conversation_max_turns,
        &llm_config.conversation_history_dir,
    );

    // Check if this is a new conversation or continuation
    let history_messages = history.load_history()?;
    let is_first = history_messages.is_empty();

    // Save user message
    if is_first {
        // First message includes prompt instructions
        let full_user_message = format!("{}\n\nOriginal dictation:\n{}", prompt, transcription);
        history.append_user(&full_user_message, true)?;
        log("Saved first user message with prompt instructions to history");
    } else {
        // Subsequent messages are just the raw dictation
        history.append_user(transcription, false)?;
        log("Saved user message to history");
    }

    // Save assistant response
    history.append_assistant(assistant_response)?;
    log("Saved assistant response to history");

    Ok(())
}
```

#### 4. Update CLI Integration (src/bin/cli.rs)

Find where `groq_client.complete()` is called and update it to pass the prompt name:

```rust
// Example location (need to find exact line)
let result = groq_client.complete(&prompt_content, &transcription, &prompt_name)?;
```

The prompt name should be extracted from the command line arguments. If using `--prompt clean`, then `prompt_name = "clean"`.

#### 5. Add to lib.rs

Add the new module to the library:

```rust
pub mod conversation_history;
```

### Edge Cases & Considerations

1. **Empty history file** ✓ - First dictation starts new conversation
2. **Expired history** ✓ - If all messages are older than time window, treated as new conversation
3. **Different prompts** ✓ - Each prompt maintains separate history (e.g., "clean" vs "email")
4. **Token limits** ✓ - `max_turns` prevents overwhelming the context window
5. **Concurrent dictations** - JSONL append is atomic, minimal race condition risk
6. **File cleanup** - History files stay in /tmp, cleaned by system tmpfs
7. **Privacy** ✓ - History files are temporary and can be disabled via config
8. **Prompt-less dictations** - If using a prompt that's not found, history can still work with empty prompt string
9. **Tool calling** - Only the final assistant response is saved, not intermediate tool call steps

### Testing Strategy

1. **Unit tests** (src/conversation_history.rs):
   - ✓ Test timestamp filtering
   - ✓ Test turn limiting
   - ✓ Test JSONL parsing
   - ✓ Test append and load operations

2. **Integration tests** (tests/conversation_history.rs):
   ```rust
   #[test]
   fn test_two_turn_conversation() {
       // Simulate two sequential dictations
       // Verify second includes first in context
   }

   #[test]
   fn test_time_expiration() {
       // Mock old timestamps
       // Verify conversation resets
   }

   #[test]
   fn test_different_prompts() {
       // Use two different prompt names
       // Verify isolation
   }
   ```

3. **Manual testing**:
   - Record two dictations within 5 minutes using `--prompt clean`, verify context in logs
   - Wait 6 minutes, record again, verify new conversation starts
   - Use `--prompt email`, verify separate history
   - Disable in config, verify feature is off

### Migration & Backward Compatibility

- Feature is enabled by default but respects config
- Existing code continues to work (add default value for prompt_name parameter)
- No breaking changes to public APIs (except adding optional parameter)
- Old dictations work without history if config disabled
- History files are per-prompt, isolated from each other

### Future Enhancements (Out of Scope)

1. Manual history management:
   - `transcribe reset-history --prompt clean` - Clear history for a prompt
   - `transcribe show-history --prompt clean` - View conversation history
2. Persistent history (not in /tmp) for long-term context
3. Cross-prompt context sharing (advanced use case)
4. Token-based pruning instead of turn-based
5. Export conversations for analysis
6. Smart conversation detection (semantic similarity to group related dictations)

## Implementation Checklist

- [ ] Update `src/config.rs` - Add `LlmConfig` struct
- [ ] Create `src/conversation_history.rs` - Core history management
- [ ] Update `src/groq.rs` - Integrate history into message building
- [ ] Update `src/bin/cli.rs` - Pass prompt name to Groq client
- [ ] Add to `src/lib.rs` - Export conversation_history module
- [ ] Write unit tests in `src/conversation_history.rs`
- [ ] Write integration test in `tests/conversation_history.rs`
- [ ] Update `config.toml` template with new `[llm]` section
- [ ] Manual testing with real dictations
- [ ] Update CLAUDE.md documentation

## Summary

This design enables multi-turn conversations by:

1. ✅ Maintaining per-prompt conversation history in JSONL format
2. ✅ Time-based filtering to include only recent messages
3. ✅ Preserving the first message's prompt instructions while keeping subsequent messages natural
4. ✅ Zero-latency overhead (append is fast, load happens once per dictation)
5. ✅ Privacy-conscious with temporary files and config toggle
6. ✅ Clear separation of concerns across modules

**Estimated effort**: ~400-500 lines of code across 4 files, approximately 3-4 hours of development + 1-2 hours testing.
