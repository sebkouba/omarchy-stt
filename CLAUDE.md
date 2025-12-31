# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**transcribe-rs-v2** is a Rust-based push-to-talk dictation system for Linux/Wayland with four architectural layers:

1. **Core Library** (`src/lib.rs`, `src/engines/`, `src/audio.rs`) - Reusable transcription API with trait-based engine abstraction
2. **Recording Daemon** (`src/bin/recording-daemon.rs`) - Continuous audio capture to circular buffer for zero-latency recording
3. **Transcription Daemon/Client** (`src/bin/daemon.rs`, `src/bin/client.rs`) - Long-running service with model loaded for fast transcription
4. **Push-to-Talk CLI** (`src/bin/cli.rs` + support modules) - Desktop integration orchestrating recording, transcription, corrections, and pasting

This is a v2 fork of the original project at `/home/seb/code/cloned/transcribe-rs`. Uses separate socket paths (`/tmp/transcribe-rs-v2.sock` for transcription, `/tmp/transcribe-rs-v2-recording.sock` for recording) to allow simultaneous operation.

## High-Level Architecture

### TranscriptionEngine Trait Pattern

All transcription engines implement a common trait (`src/lib.rs:TranscriptionEngine`) with associated types for parameters:

- **WhisperEngine** (`src/engines/whisper.rs`) - Single GGML model file, Metal/Vulkan acceleration, segment-level timestamps
- **ParakeetEngine** (`src/engines/parakeet/`) - Multi-ONNX directory structure, Int8/FP32 quantization, configurable timestamp granularity (token/word/segment)

This abstraction allows the CLI and daemon to work with any engine through the same interface.

### Push-to-Talk Workflow

```
User Press Hotkey → transcribe start
    ↓
recording::start_recording() connects to recording-daemon socket
    ↓
Daemon returns current buffer index (start_index saved to /tmp/ptt_recording.pid)
    ↓
Recording daemon continuously writes to circular buffer (always recording)
    ↓
User Release Hotkey → transcribe stop
    ↓
recording::stop_recording() sends stop + start_index to daemon
    ↓
Daemon extracts samples from circular buffer, writes /tmp/ptt_current.wav (~5-10ms)
    ↓
Calls transcribe-client via Unix socket (/tmp/transcribe-rs-v2.sock)
    ↓
Transcription daemon (with Parakeet loaded) transcribes and returns text
    ↓
transcription_corrections::apply_corrections() fixes common errors via fuzzy matching
    ↓
Optional: groq::process_with_llm() for LLM post-processing (grammar, formatting, tools)
    ↓
clipboard::add_trailing_space_after_punctuation() adds space after . ! ?
    ↓
clipboard::copy_to_clipboard() uses wl-copy subprocess
    ↓
paste::paste_from_clipboard() uses ydotool to simulate Ctrl+V or Ctrl+Shift+V
    ↓
Text appears in active window
    ↓
Optional: dictation_logger::log() records to CSV for analysis
```

### Daemon Architecture

**Recording Daemon** (`recording-daemon`):
- Continuously records from microphone to circular buffer in RAM
- Buffer size: 2 minutes @ 16kHz mono (~3.7 MB)
- Listens on `/tmp/transcribe-rs-v2-recording.sock`
- Commands: `start` (returns index), `stop` (extracts audio from index to now)
- Zero-latency recording start, ~5-10ms extraction time
- Handles FFmpeg subprocess for audio input

**Transcription Daemon** (`transcribe-daemon`):
- Eliminates 3-4 second model load time by keeping Parakeet in memory
- Loads model at startup from config path
- Listens on Unix socket `/tmp/transcribe-rs-v2.sock`
- Simple line-delimited JSON protocol (request: `{"audio_file": "/path/to/file.wav"}`, response: `{"text": "..."}`)
- Single-threaded request processing (sufficient for personal use)

## Critical Technical Decisions

### Why External Tools Over Rust Libraries

**wl-copy instead of arboard crate:**
- `arboard` had clipboard persistence issues on Wayland
- `wl-copy` is the native Wayland clipboard tool with guaranteed compatibility
- Trade-off: External dependency but reliable behavior

**ydotool for auto-paste:**
- Wayland security prevents apps from sending keyboard events
- `ydotool` is a privileged tool designed for this use case
- Simulates key codes: 29=Ctrl, 42=Shift, 47=V
- Currently Hyprland-specific: uses `hyprctl activewindow -j` to detect terminals and choose Ctrl+Shift+V vs Ctrl+V

**Recording daemon + circular buffer instead of on-demand FFmpeg:**
- Eliminates 100-300ms FFmpeg startup latency
- Zero-latency recording start (already recording to buffer)
- ~5-10ms extraction time (just write buffer slice to WAV)
- FFmpeg still used internally by daemon for audio input
- Trade-off: Additional daemon process but near-instant UX

### Audio Processing Requirements

Strict format enforcement in `audio::read_wav_samples()`:
- Sample Rate: 16 kHz (required by models)
- Channels: Mono (1 channel)
- Bit Depth: 16-bit PCM
- Format: WAV

**Microphone source** configured via environment variable `RECORDING_MICROPHONE` in `recording-daemon` systemd service (defaults to hardcoded value if not set).

### Transcription Corrections System

**Fuzzy pattern matching** (`src/transcription_corrections.rs`):
- Fixes common ASR errors using configurable JSON rules
- Supports two algorithms: Jaro-Winkler (names/prefixes) and Levenshtein (general text)
- Default similarity threshold: 85%
- Case-sensitive/insensitive matching
- Example: "see plus plus" → "C++" with high confidence matching

**Configuration** (`~/.config/transcribe-rs/transcription_corrections.json`):
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
    }
  ]
}
```

### LLM Post-Processing

**Groq API integration** (`src/groq.rs`):
- Optional LLM-based grammar correction and formatting
- Tool calling support for executing commands during dictation
- Model: `moonshotai/kimi-k2-instruct-0905`
- Tools loaded from `~/.config/transcribe-rs/tools.json`
- Iterative execution (up to 5 tool calls per request)
- Use case: "Send this email to John" can trigger email tool

### Punctuation Intelligence

`clipboard::add_trailing_space_after_punctuation()` adds space after `.`, `!`, `?` for natural consecutive dictations:
- "Hello world." → paste → "Another sentence." flows naturally
- No space after commas or other punctuation

### Dictation Logging

**Privacy-conscious logging** (`src/dictation_logger.rs`):
- Disabled by default (opt-in via config)
- Two separate CSV logs: basic transcriptions, LLM corrections
- Tracks: timestamp, raw text, corrected text, duration, context
- Use case: Analyze correction patterns, model accuracy

## Common Development Commands

## Development Workflow (Versioned Builds)

This project uses versioned builds to prevent development from interfering with running daemons.

### Directory Structure
- `builds/staging/` - Latest development build (use for testing)
- `builds/current/` - Symlink to active production version (daemons use this)
- `builds/YYYYMMDD-HHMMSS/` - Timestamped versions for rollback

### Workflow

**During development:**
1. Make code changes
2. Run `./scripts/build.sh` to compile to staging (does NOT affect running daemons)
3. Test manually: `./builds/staging/transcribe-client samples/jfk.wav`

**When ready to go live:**
4. Run `./scripts/promote.sh` to make staging the current version and restart daemons

**If something breaks:**
5. Run `./scripts/rollback.sh` to revert to a previous version

### Key Points
- `cargo build --release` alone does NOT affect running daemons
- Daemons always run from `builds/current/`
- Testing uses `builds/staging/` directly
- `promote.sh` creates a timestamped snapshot and updates the symlink

### Scripts Summary
| Script | Effect |
|--------|--------|
| `./scripts/build.sh` | Compile to staging (safe, no restart) |
| `./scripts/promote.sh` | Make staging live + restart daemons |
| `./scripts/rollback.sh [version]` | Revert to previous (or specified) version |

### Building (Manual)

```bash
# Check without building
cargo check

# Format and lint
cargo fmt
cargo clippy
```

### Testing

```bash
# Run all tests (10 unit tests + 3 integration tests)
cargo test

# Run library tests only
cargo test --lib

# Run specific module tests
cargo test --lib clipboard
cargo test --lib recording

# Run integration tests
cargo test --test whisper
cargo test --test parakeet
cargo test --test openai

# Run examples
cargo run --example transcribe
cargo run --example transcribe-file
```

### Running and Debugging

```bash
# Start both daemons
RECORDING_MICROPHONE="your-device" ./target/release/recording-daemon &
./target/release/transcribe-daemon &

# Or via systemd
systemctl --user start recording-daemon transcribe-daemon
systemctl --user status recording-daemon
systemctl --user status transcribe-daemon
journalctl --user -u recording-daemon -f
journalctl --user -u transcribe-daemon -f

# Manual push-to-talk test
./target/release/transcribe start
# (speak for a few seconds)
./target/release/transcribe stop

# Test transcription of existing file
./target/release/transcribe-client samples/jfk.wav

# View debug logs (timestamped entries with module tags)
tail -f /tmp/ptt_rust_debug.log
./view-ptt-logs.sh
```

## Deployment and Installation

### Prerequisites (Arch Linux)

```bash
sudo pacman -S wl-clipboard ydotool ffmpeg
```

### Installation Steps

1. Build release binaries and set up versioned builds:
   ```bash
   cargo build --release
   mkdir -p builds/staging
   VERSION=$(date +%Y%m%d-%H%M%S)
   mkdir builds/$VERSION
   cp target/release/{transcribe,transcribe-daemon,transcribe-client,recording-daemon,hotkey-daemon} builds/$VERSION/
   ln -s $VERSION builds/current
   ```

2. Download Parakeet model:
   ```bash
   mkdir -p models
   cd models
   wget https://blob.handy.computer/parakeet-v3-int8.tar.gz
   tar -xzf parakeet-v3-int8.tar.gz
   cd ..
   ```

3. Create config: `./target/release/transcribe config init`

4. Install systemd services:
   ```bash
   # Create recording-daemon.service (set RECORDING_MICROPHONE)
   # Create transcribe-daemon.service (set WorkingDirectory)
   # Copy both to ~/.config/systemd/user/
   systemctl --user daemon-reload
   systemctl --user enable recording-daemon transcribe-daemon
   systemctl --user start recording-daemon transcribe-daemon
   ```

5. Configure hotkey daemon OR Hyprland keybindings:

   **Option A: Hotkey daemon (recommended)**
   ```bash
   # Uses XDG Desktop Portal GlobalShortcuts - works across Wayland compositors
   systemctl --user enable hotkey-daemon
   systemctl --user start hotkey-daemon
   ```

   **Option B: Hyprland keybindings** in `~/.config/hypr/hyprland.conf`:
   ```ini
   bind = SUPER SHIFT CTRL ALT, E, exec, /path/to/transcribe-rs-v2/builds/current/transcribe start
   bindr = SUPER SHIFT CTRL ALT, E, exec, /path/to/transcribe-rs-v2/builds/current/transcribe stop
   ```

### File Locations

**Build directories:**
- `builds/staging/` - Development build for testing
- `builds/current/` - Symlink to active version (daemons use this)
- `builds/YYYYMMDD-HHMMSS/` - Timestamped version snapshots

**Temporary files:**
- `/tmp/ptt_current.wav` - Current/last recording
- `/tmp/ptt_recording.pid` - Recording start index (repurposed from PID file)
- `/tmp/ptt_rust_debug.log` - Debug log with timestamps
- `/tmp/transcribe-rs-v2.sock` - Transcription daemon Unix socket
- `/tmp/transcribe-rs-v2-recording.sock` - Recording daemon Unix socket

**Model files:**
- `models/parakeet-tdt-0.6b-v3-int8/` - Parakeet model (default)
- `models/whisper-medium-q4_1.bin` - Whisper model (alternative)

**Configuration files:**
- `~/.config/transcribe-rs/config.toml` - Main configuration
- `~/.config/transcribe-rs/transcription_corrections.json` - Correction rules
- `~/.config/transcribe-rs/tools.json` - LLM tool definitions (optional)
- `~/.config/transcribe-rs/dictation_log.csv` - Basic dictation log (if enabled)
- `~/.config/transcribe-rs/llm_corrections_log.csv` - LLM corrections log (if enabled)

### Monitoring and Debugging

**Debug logging:**
All modules log to `/tmp/ptt_rust_debug.log` with timestamps and module tags:
```
[2025-11-04 12:34:56.789] [recording] ffmpeg started with PID: 12345
[2025-11-04 12:34:58.123] [clipboard] Copied to clipboard: "Hello world"
[2025-11-04 12:34:58.456] [paste] Detected terminal, using Ctrl+Shift+V
```

**Desktop notifications:**
- "🎤 Recording" - Recording started (marked in buffer)
- "⏹️ Processing..." - Recording stopped, transcribing
- "✅ Pasted: [preview]" - Success (shows final corrected text)
- "🔧 Corrections applied: N" - Corrections made to transcription
- "🤖 LLM: [preview]" - LLM post-processing applied
- "🛠️ Tool: [name]" - Tool executed by LLM
- "❌ Error: [message]" - Errors

## Important Patterns and Conventions

### Module Structure

```
src/
├── lib.rs                          # Core types, TranscriptionEngine trait
├── audio.rs                        # WAV reading, format validation
├── config.rs                       # TOML configuration management
├── engines/
│   ├── whisper.rs                 # Whisper engine
│   └── parakeet/
│       ├── engine.rs              # Parakeet implementation
│       ├── model.rs               # ONNX model management
│       └── timestamps.rs          # Timestamp processing
├── remote/
│   └── openai.rs                  # OpenAI API (async)
├── groq.rs                        # Groq LLM client with tool calling
├── transcription_corrections.rs   # Fuzzy pattern matching corrections
├── tools.rs                       # Tool loading and execution for LLM
├── dictation_logger.rs            # CSV logging for dictations
├── circular_buffer.rs             # Ring buffer for continuous recording
├── performance_log.rs             # Performance timing utilities
├── timing.rs                      # Timing helpers
├── prompts.rs                     # LLM system prompts
├── bin/
│   ├── cli.rs                    # Main CLI (transcribe command)
│   ├── daemon.rs                 # Transcription daemon
│   ├── client.rs                 # Transcription client
│   └── recording-daemon.rs       # Recording daemon with circular buffer
└── [CLI support modules]
    ├── clipboard.rs              # wl-copy integration
    ├── paste.rs                  # ydotool integration
    ├── recording.rs              # Recording daemon client
    ├── terminal_detect.rs        # Hyprland window detection
    └── notifications.rs          # Desktop notifications
```

### Error Handling and Logging

- All public APIs return `Result<T, Box<dyn Error>>`
- Each module has a `log()` function that appends to `/tmp/ptt_rust_debug.log`
- Process management includes timeouts (5s) and fallbacks (SIGKILL)
- Engine loading validates model files exist before proceeding

### Performance Characteristics

**Parakeet (Int8 quantized):**
- Model load time: 3-4 seconds (why daemon exists)
- Inference: 5-30x real-time (hardware dependent)
- Memory: Model stays loaded in daemon

**Whisper:**
- Model load time: Faster than Parakeet
- Inference: Slower than Parakeet
- Better multilingual support

### Platform-Specific Code

- `whisper-rs` features: `metal` (macOS), `vulkan` (Linux/Windows) - conditionally compiled
- Terminal detection: Hyprland-specific via `hyprctl activewindow -j` (see `terminal_detect.rs`)
- Microphone source: Environment variable `RECORDING_MICROPHONE` in recording-daemon (defaults to hardcoded value)
- Circular buffer: Lock-free atomic operations for thread-safe concurrent access

## Key Dependencies

- `hound` - WAV file I/O
- `ort` - ONNX Runtime for Parakeet
- `whisper-rs` - Whisper.cpp bindings (with Metal/Vulkan features)
- `async-openai` - OpenAI API client
- `reqwest` - HTTP client for Groq API
- `ureq` - Lightweight HTTP for tool execution
- `clap` 4.5 - CLI argument parsing (derive API)
- `notify-rust` - Desktop notifications
- `nix` - Unix signal handling (SIGINT, SIGKILL)
- `serde`/`serde_json` - Configuration and protocol serialization
- `toml` - Config file parsing
- `chrono` - Timestamp formatting for logs
- `rapidfuzz` - Fuzzy string matching (Jaro-Winkler, Levenshtein)
- `similar` - Text diffing for logging corrections
- `ctrlc` - Signal handling for daemon graceful shutdown
- `dirs` - Cross-platform config directory location
