# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**transcribe-rs-v2** is a Rust-based push-to-talk dictation system for Linux/Wayland with three architectural layers:

1. **Core Library** (`src/lib.rs`, `src/engines/`, `src/audio.rs`) - Reusable transcription API with trait-based engine abstraction
2. **Push-to-Talk CLI** (`src/bin/cli.rs` + support modules) - Desktop integration for voice dictation
3. **Daemon/Client** (`src/bin/daemon.rs`, `src/bin/client.rs`) - Long-running service for fast repeated transcriptions

This is a v2 fork of the original project at `/home/seb/code/cloned/transcribe-rs`. The socket path is `/tmp/transcribe-rs-v2.sock` (different from original) to allow simultaneous operation.

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
recording::start_recording() spawns ffmpeg (PID saved to /tmp/ptt_recording.pid)
    ↓
Records to /tmp/ptt_current.wav (16kHz mono WAV)
    ↓
User Release Hotkey → transcribe stop
    ↓
recording::stop_recording() sends SIGINT to ffmpeg, validates file
    ↓
Calls transcribe-client via Unix socket (/tmp/transcribe-rs-v2.sock)
    ↓
Daemon (with Parakeet loaded) transcribes and returns text
    ↓
clipboard::add_trailing_space_after_punctuation() adds space after . ! ?
    ↓
clipboard::copy_to_clipboard() uses wl-copy subprocess
    ↓
paste::paste_from_clipboard() uses ydotool to simulate Ctrl+V or Ctrl+Shift+V
    ↓
Text appears in active window
```

### Daemon Architecture

The daemon eliminates 3-4 second model load time by keeping the Parakeet model in memory:

- Loads model at startup
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

**ffmpeg subprocess for recording:**
- FFmpeg handles all audio driver complexity and hardware quirks
- Cross-platform, format conversion, battle-tested
- SIGINT ensures graceful shutdown for valid WAV files

### Audio Processing Requirements

Strict format enforcement in `audio::read_wav_samples()`:
- Sample Rate: 16 kHz (required by models)
- Channels: Mono (1 channel)
- Bit Depth: 16-bit PCM
- Format: WAV

**Hardcoded microphone source** in `recording.rs`: `alsa_input.usb-046d_C922_Pro_Stream_Webcam_C4C393EF-02.analog-stereo`

### Punctuation Intelligence

`clipboard::add_trailing_space_after_punctuation()` adds space after `.`, `!`, `?` for natural consecutive dictations:
- "Hello world." → paste → "Another sentence." flows naturally
- No space after commas or other punctuation

## Common Development Commands

### Building

```bash
# Build all binaries (transcribe, transcribe-daemon, transcribe-client)
cargo build --release

# Build specific binary
cargo build --release --bin transcribe-daemon

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
# Start daemon
./target/release/transcribe-daemon

# Or via systemd
systemctl --user start transcribe-daemon
systemctl --user status transcribe-daemon
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

1. Build release binaries: `cargo build --release`

2. Download Parakeet model:
   ```bash
   mkdir -p models
   cd models
   wget https://blob.handy.computer/parakeet-v3-int8.tar.gz
   tar -xzf parakeet-v3-int8.tar.gz
   cd ..
   ```

3. Install systemd service:
   ```bash
   # Edit transcribe-daemon.service to match your paths
   cp transcribe-daemon.service ~/.config/systemd/user/
   systemctl --user daemon-reload
   systemctl --user enable transcribe-daemon
   systemctl --user start transcribe-daemon
   ```

4. Configure Hyprland keybindings in `~/.config/hypr/hyprland.conf`:
   ```ini
   bind = SUPER SHIFT CTRL ALT, E, exec, /path/to/transcribe-rs-v2/target/release/transcribe start
   bindr = SUPER SHIFT CTRL ALT, E, exec, /path/to/transcribe-rs-v2/target/release/transcribe stop
   ```

### File Locations

**Temporary files:**
- `/tmp/ptt_current.wav` - Current/last recording
- `/tmp/ptt_recording.pid` - Recording process PID
- `/tmp/ptt_rust_debug.log` - Debug log with timestamps
- `/tmp/transcribe-rs-v2.sock` - Daemon Unix socket

**Model files:**
- `models/parakeet-tdt-0.6b-v3-int8/` - Parakeet model (default)
- `models/whisper-medium-q4_1.bin` - Whisper model (alternative)

### Monitoring and Debugging

**Debug logging:**
All modules log to `/tmp/ptt_rust_debug.log` with timestamps and module tags:
```
[2025-11-04 12:34:56.789] [recording] ffmpeg started with PID: 12345
[2025-11-04 12:34:58.123] [clipboard] Copied to clipboard: "Hello world"
[2025-11-04 12:34:58.456] [paste] Detected terminal, using Ctrl+Shift+V
```

**Desktop notifications:**
- "🎤 Recording" - Recording started
- "⏹️ Processing..." - Recording stopped, transcribing
- "✅ Pasted: [preview]" - Success
- "❌ Error: [message]" - Errors

## Important Patterns and Conventions

### Module Structure

```
src/
├── lib.rs                    # Core types, TranscriptionEngine trait
├── audio.rs                  # WAV reading, format validation
├── engines/
│   ├── whisper.rs           # Whisper engine
│   └── parakeet/
│       ├── engine.rs        # Parakeet implementation
│       ├── model.rs         # ONNX model management
│       └── timestamps.rs    # Timestamp processing
├── remote/openai.rs         # OpenAI API (async)
├── bin/
│   ├── cli.rs               # Main CLI (transcribe command)
│   ├── daemon.rs            # Daemon (transcribe-daemon)
│   └── client.rs            # Client (transcribe-client)
└── [CLI support modules]
    ├── clipboard.rs         # wl-copy integration
    ├── paste.rs             # ydotool integration
    ├── recording.rs         # ffmpeg management
    ├── terminal_detect.rs   # Hyprland window detection
    └── notifications.rs     # Desktop notifications
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
- Microphone source: Hardcoded in `recording.rs` for specific hardware

## Key Dependencies

- `hound` - WAV file I/O
- `ort` - ONNX Runtime for Parakeet
- `whisper-rs` - Whisper.cpp bindings (with Metal/Vulkan features)
- `async-openai` - OpenAI API client
- `clap` 4.5 - CLI argument parsing (derive API)
- `notify-rust` - Desktop notifications
- `nix` - Unix signal handling (SIGINT, SIGKILL)
- `serde`/`serde_json` - Daemon protocol serialization
