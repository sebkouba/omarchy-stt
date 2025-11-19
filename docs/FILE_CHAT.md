# File Chat Mode

File chat mode writes LLM Q&A conversations to markdown files instead of pasting to the clipboard. This creates a persistent, human-readable log that can be viewed in any markdown viewer.

## Usage

```bash
# Start with file-chat flag
transcribe start --prompt ask --file-chat

# Stop (writes to markdown instead of pasting)
transcribe stop
```

### Hyprland Keybinding Example

```ini
bind = SUPER SHIFT, F, exec, /path/to/transcribe start --prompt ask --file-chat
bindr = SUPER SHIFT, F, exec, /path/to/transcribe stop
```

## Configuration

Add to `~/.config/transcribe-rs/config.toml` under `[llm]`:

```toml
[llm]
# ... other llm settings ...
file_chat_enabled = true
file_chat_dir = "/home/user/.config/transcribe-rs/chats"
```

## Output Format

Conversations are written to daily files named `YYYY-MM-DD.md`:

```markdown
## 14:32:05

**User:** What is the capital of France?

**Assistant:** Paris

---

## 14:33:20

**User:** And Germany?

**Assistant:** Berlin

---
```

## How Context Works

File chat mode uses the **same conversation history system** as clipboard mode:

- **LLM receives:** Last N turns within the configured time window
  - Controlled by `conversation_max_turns` and `conversation_history_minutes`
  - History stored in `/tmp/conversation_history_{prompt}.json`

- **Markdown file:** Append-only output log
  - Contains all Q&A exchanges for the day
  - Never read back as context for the LLM

The markdown file is purely for human reference. The LLM context comes from the JSON-based history system, not the markdown file.

## Comparison

| Aspect | Clipboard Mode | File Chat Mode |
|--------|---------------|----------------|
| Output destination | Clipboard → paste | Markdown file |
| LLM context | Last N turns (JSON history) | Last N turns (JSON history) |
| Persistence | None (clipboard overwrites) | Daily markdown files |
| Use case | Quick answers in active window | Persistent conversation log |

## Clearing History

The `conversation_history_clear_word` (default: "clear") works the same way - it clears the JSON-based conversation history that gets sent to the LLM. The markdown file is not affected.
