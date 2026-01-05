# Features Guide

omarchy-stt has three modes: **Local-Only** (default), **LLM Post-Processing** (optional), and **Tool Calling** (advanced). Start simple, add features as needed.

---

## Mode 1: Local-Only (Default)

**What you get out of the box:**

1. Press hotkey
2. Speak
3. Release hotkey
4. Text appears in active window

**How it works:**
```
Your voice → Parakeet AI (on your machine) → Text → Auto-paste
```

**No internet. No API keys. No cloud processing. No configuration needed.**

This is the core feature. Everything else is optional.

### Accuracy

Parakeet is very accurate for:
- Clear speech
- Standard English
- Normal speaking pace

It may struggle with:
- Heavy accents
- Technical jargon
- Mumbling or very fast speech

**Solution:** Use custom corrections (see below) or LLM post-processing.

---

## Mode 2: Custom Corrections (Optional)

Teach the system your specific vocabulary.

### Setup

Create `~/.config/transcribe-rs/transcription_corrections.json`:

```json
{
  "rules": [
    {
      "from": "see plus plus",
      "to": "C++",
      "case_sensitive": false,
      "fuzzy_matching": true,
      "similarity_threshold": 0.85,
      "algorithm": "Levenshtein"
    },
    {
      "from": "react jay ess",
      "to": "React.js"
    },
    {
      "from": "kubernetes",
      "to": "Kubernetes",
      "case_sensitive": true
    }
  ]
}
```

Enable in `~/.config/transcribe-rs/config.toml`:
```toml
[transcription_corrections]
enabled = true
corrections_file = "~/.config/transcribe-rs/transcription_corrections.json"
```

### How It Works

After transcription, before pasting, the system fuzzy-matches common mistakes:

```
"see plus plus" → (fuzzy match, 87% similar) → "C++"
"react jay ess" → (exact match) → "React.js"
```

**Use cases:**
- Programming terms ("type script" → "TypeScript")
- Product names ("open A I" → "OpenAI")
- Your company's jargon
- Proper nouns the AI mishears

See [docs/TRANSCRIPTION_CORRECTIONS.md](docs/TRANSCRIPTION_CORRECTIONS.md) for details.

---

## Mode 3: LLM Post-Processing (Optional)

Send transcriptions through an LLM for cleanup before pasting.

### What It Does

**Before:**
```
"um so like I think we should uh maybe send that email"
```

**After:**
```
"I think we should send that email."
```

The LLM:
- Removes filler words ("um", "uh", "like")
- Fixes grammar
- Adds punctuation
- Optionally applies custom prompts ("make this professional", "translate to Spanish")

### Setup

1. **Get a Groq API key** (free tier available):
   - Go to https://console.groq.com
   - Create account, get API key

2. **Save key:**
   ```bash
   echo "GROQ_API_KEY=your-key-here" > ~/.config/transcribe-rs/.env
   ```

3. **Enable in config** (`~/.config/transcribe-rs/config.toml`):
   ```toml
   [groq]
   enabled = true
   model = "moonshotai/kimi-k2-instruct-0905"  # Or another model
   ```

4. **Restart daemon:**
   ```bash
   systemctl --user restart hotkey-daemon
   ```

### Custom Prompts

Create prompt files in `~/.config/transcribe-rs/prompts/`:

**`clean.md`** (default):
```markdown
Clean up this transcription:
- Remove filler words
- Fix grammar
- Keep the meaning
```

**`professional.md`**:
```markdown
Make this transcription professional and concise.
Remove casual language. Use business tone.
```

**`spanish.md`**:
```markdown
Translate this to Spanish. Use formal language.
```

Then in your hotkey configuration, specify which prompt to use.

### Privacy Note

**LLM mode sends your transcription to Groq's API.** Your voice still never leaves your machine (transcribed locally first), but the *text* goes to the cloud for cleanup.

If privacy is critical, stick to local-only mode.

---

## Mode 4: Tool Calling (Advanced)

Make voice commands trigger actions instead of pasting text.

### What It Does

Instead of pasting text, run scripts/commands:

```
You say: "Turn on my desk lamp"
       ↓
LLM detects intent: "user wants lamp on"
       ↓
Calls tool: turn_leds_on.sh
       ↓
Notification: "🛠️ Tool: turn_leds_on"
       ↓
Your lamp turns on (no text pasted)
```

### Setup

Requires LLM mode enabled.

1. **Create tools config** (`~/.config/transcribe-rs/tools.json`):
   ```json
   {
     "tools": [
       {
         "name": "turn_leds_on",
         "description": "Turn on desk LED lights. Call when user says 'turn on lights', 'lights on', 'enable LEDs'",
         "command": "/home/user/scripts/led-control.sh",
         "args": ["on"],
         "parameters": {}
       },
       {
         "name": "switch_workspace",
         "description": "Switch to a different workspace. Call when user says 'switch to workspace N', 'go to workspace N'",
         "command": "hyprctl",
         "args": ["dispatch", "workspace", "{workspace}"],
         "parameters": {
           "workspace": {
             "type": "string",
             "description": "Workspace number (1-10)"
           }
         }
       }
     ]
   }
   ```

2. **Enable tool calling** in config:
   ```toml
   [groq]
   enabled = true
   enable_tools = true
   tools_file = "~/.config/transcribe-rs/tools.json"
   ```

3. **Restart daemon:**
   ```bash
   systemctl --user restart hotkey-daemon
   ```

### How It Works

1. You speak: "Switch to workspace 3"
2. Transcribed: "Switch to workspace 3"
3. Sent to LLM with tool descriptions
4. LLM responds: `{"tool": "switch_workspace", "params": {"workspace": "3"}}`
5. System runs: `hyprctl dispatch workspace 3`
6. Notification shows: "🛠️ Tool: switch_workspace"
7. **No text pasted**

### Example Tools

See [docs/example-tools.json](docs/example-tools.json) for ideas:
- Control smart home devices
- Switch workspaces
- Take screenshots
- Open applications
- Run git commands
- Send notifications
- Control media playback

### Security Warning

**Tools can run arbitrary commands.** Only add tools you trust. The LLM decides when to call them based on what you say.

---

## Summary: Which Mode Should I Use?

| Mode | When to Use | Privacy | Speed | Accuracy |
|------|-------------|---------|-------|----------|
| **Local-Only** | Default, good enough for most | 🔒 100% local | ⚡ Fastest | ✅ Good |
| **Custom Corrections** | You have specific jargon | 🔒 100% local | ⚡ Fastest | ✅✅ Better |
| **LLM Post-Processing** | Want grammar cleanup | ☁️ Text to API | 🐌 +500ms | ✅✅✅ Best |
| **Tool Calling** | Voice commands for actions | ☁️ Text to API | 🐌 +500ms | ✅✅✅ Best |

**Start with local-only.** Add features only if you need them.

---

## Configuration Reference

Full config file (`~/.config/transcribe-rs/config.toml`):

```toml
[audio]
sample_rate = 16000
recording_path = "/tmp/ptt_current.wav"

[model]
path = "models/parakeet-tdt-0.6b-v3-int8"
engine = "parakeet"
quantization = "int8"

[integration]
auto_paste = true
add_space_after_punctuation = true
terminal_apps = ["alacritty", "kitty", "foot"]

[transcription_corrections]
enabled = false  # Set to true to enable
corrections_file = "~/.config/transcribe-rs/transcription_corrections.json"

[groq]
enabled = false  # Set to true for LLM mode
model = "moonshotai/kimi-k2-instruct-0905"
enable_tools = false  # Set to true for tool calling
tools_file = "~/.config/transcribe-rs/tools.json"
```

---

## Next Steps

- **Local-only mode:** Just use it, you're done
- **Custom corrections:** See [docs/TRANSCRIPTION_CORRECTIONS.md](docs/TRANSCRIPTION_CORRECTIONS.md)
- **LLM mode:** See [docs/LLM_INTEGRATION.md](docs/LLM_INTEGRATION.md)
- **Tool calling:** See [docs/TOOL_CALLING.md](docs/TOOL_CALLING.md)
