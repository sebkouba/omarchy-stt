# Prompt Implementation Plan

## Overview

Add support for post-processing transcribed text through LLM prompts before pasting. This allows cleaning up dictation, formatting for different contexts (email, slack, etc.), or applying custom transformations.

## Command Interface

### New CLI Syntax

```bash
# Normal transcription (existing behavior)
transcribe start
transcribe stop

# Transcription with prompt post-processing
transcribe start --prompt clean
transcribe stop
```

The `--prompt` flag is passed to `start` and stored in a state file so `stop` knows to apply the prompt.

### State Management

Create `/tmp/ptt_prompt.txt` to communicate prompt name from `start` to `stop`:
- `start --prompt clean` writes "clean" to file
- `start` without flag removes file (or writes empty)
- `stop` reads file to determine if prompt processing needed

## Prompt File Format

### Directory Structure

```
prompts/
├── clean.md
├── email.md
└── slack.md
```

### File Format

Simple markdown files containing just the system prompt text. Example `prompts/clean.md`:

```markdown
Your Task is to Clean this spoken dictation into polished text while preserving the speaker's original meaning and voice. Only return the corrected text.

- Remove filler words (um, uh, like, you know, so, basically)
  Example: "so um I think we should like focus on the API" → "I think we should focus on the API"

- Handle self-corrections (keep only the final intent)
  Example: "We need to deploy on Friday... no wait, actually Thursday" → "We need to deploy on Thursday"

...
```

The entire file content becomes the system prompt.

## Architecture Components

### 1. Prompt Loader (`src/prompts.rs`)

```rust
pub fn load_prompt(name: &str) -> Result<String, Box<dyn Error>> {
    let path = format!("prompts/{}.md", name);
    fs::read_to_string(&path)
        .map_err(|e| format!("Failed to load prompt '{}': {}", name, e).into())
}
```

### 2. Groq API Client (`src/groq.rs`)

```rust
pub struct GroqClient {
    api_key: String,
}

pub struct GroqRequest {
    pub prompt: String,
    pub user_message: String,
}

impl GroqClient {
    pub fn new(api_key: String) -> Self { ... }

    pub fn complete(&self, request: &GroqRequest) -> Result<String, Box<dyn Error>> {
        // POST to https://api.groq.com/openai/v1/chat/completions
        // Payload:
        // {
        //   "messages": [
        //     {"role": "system", "content": "{prompt}"},
        //     {"role": "user", "content": "{user_message}"}
        //   ],
        //   "model": "moonshotai/kimi-k2-instruct-0905",
        //   "temperature": 0.6,
        //   "max_completion_tokens": 4096,
        //   "top_p": 1,
        //   "stream": false
        // }
        //
        // Extract text from response["choices"][0]["message"]["content"]
    }
}
```

### 3. API Key Loading

Read from `./.env` file:
```rust
fn load_groq_api_key() -> Result<String, Box<dyn Error>> {
    let env_content = fs::read_to_string(".env")?;
    for line in env_content.lines() {
        if let Some(key) = line.strip_prefix("GROQ_API_KEY=") {
            return Ok(key.trim().to_string());
        }
    }
    Err("GROQ_API_KEY not found in .env".into())
}
```

## Workflow Integration

### Modified Stop Command Flow

**Current flow:**
```
stop → transcribe via socket → add punctuation spaces → copy to clipboard → paste
```

**New flow with prompt:**
```
stop → read /tmp/ptt_prompt.txt
     ↓
     if prompt name exists:
         ↓
         transcribe via socket → send to Groq API → copy processed text → paste
         ↓
         (on API error: copy original text → paste → show error notification)
     else:
         ↓
         transcribe via socket → add punctuation spaces → copy to clipboard → paste
```

### Error Handling and Fallback

If Groq API call fails:
1. Log error to `/tmp/ptt_rust_debug.log`
2. Copy raw transcription to clipboard
3. Paste raw transcription
4. Show notification: "❌ Groq API failed: [error]. Pasted raw transcription."

This ensures the user always gets their text, even if post-processing fails.

## Implementation Steps

1. **Add CLI flag** (`src/bin/cli.rs`)
   - Add `prompt: Option<String>` field to `Start` variant
   - Write prompt name to `/tmp/ptt_prompt.txt` in start handler

2. **Create prompt loader** (`src/prompts.rs`)
   - Simple file reader for `prompts/{name}.md`
   - Return helpful error if file not found

3. **Create Groq client** (`src/groq.rs`)
   - Struct with API key
   - HTTP POST request with `ureq` or `reqwest` (blocking)
   - JSON serialization/deserialization
   - Extract response text

4. **Modify stop command** (`src/bin/cli.rs`)
   - Read `/tmp/ptt_prompt.txt` after transcription
   - If prompt specified:
     - Load prompt file
     - Load API key from `.env`
     - Call Groq API
     - Use processed text instead of raw transcription
   - On error: use raw transcription + error notification

5. **Create initial prompt** (`prompts/clean.md`)
   - Copy content from `specs/groq_curl.md` lines 22-56

6. **Testing**
   - Test normal flow (no prompt): `transcribe start && sleep 2 && transcribe stop`
   - Test prompt flow: `transcribe start --prompt clean && sleep 2 && transcribe stop`
   - Test error cases: invalid prompt name, missing API key, API failure

## Dependencies to Add

```toml
[dependencies]
# For HTTP requests (choose one)
ureq = { version = "2.9", features = ["json"] }  # Simpler, blocking
# OR
reqwest = { version = "0.11", features = ["blocking", "json"] }  # More features

serde_json = "1.0"  # Already have serde
```

## Future Extensions (Not Implementing Now)

- Custom model/temperature per prompt (add TOML frontmatter to .md files)
- Environment variable fallback for API key
- Multiple API providers (OpenAI, Anthropic, etc.)
- Streaming responses with incremental pasting
- Prompt templates with variable substitution

## Files to Create/Modify

**New files:**
- `src/prompts.rs` - Prompt loader
- `src/groq.rs` - Groq API client
- `prompts/clean.md` - Default cleaning prompt

**Modified files:**
- `src/bin/cli.rs` - Add --prompt flag, modify stop command
- `src/lib.rs` - Export new modules
- `Cargo.toml` - Add HTTP client dependency

**Temporary files:**
- `/tmp/ptt_prompt.txt` - State file for prompt name
