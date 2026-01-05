# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

transcribe-rs is a Rust library and CLI tool for audio transcription supporting multiple ASR (Automatic Speech Recognition) engines including Whisper and Parakeet (NeMo). The library was extracted from the [Handy](https://github.com/cjpais/handy) project to provide a reusable transcription API for the Rust ecosystem.

**This project now includes:**
- Core transcription library with multiple engine support
- Push-to-talk dictation system with daemon/client architecture
- Native Rust CLI replacing previous bash script implementation
- Desktop integration for Wayland/Hyprland environments

## Repository Context

**This is transcribe-rs-v2** - a fork/continuation of the original transcribe-rs project.

- **Original project**: `$PROJECT_ROOT/../transcribe-rs` (still active and in use)
- **This project (v2)**: `$PROJECT_ROOT` (experimental Rust consolidation)
- **Key difference**: Different socket path (`/tmp/transcribe-rs-v2.sock` vs `/tmp/transcribe-rs.sock`) allows both versions to run simultaneously
- **Purpose**: Consolidate bash scripts into Rust for better reliability, maintainability, and distribution

When working with paths, scripts, or the daemon:
- All paths should reference `transcribe-rs-v2` (not `transcribe-rs`)
- Socket path is `/tmp/transcribe-rs-v2.sock`
- The daemon and client in this directory are independent from the original

## Binaries

The project provides three binaries:

### `transcribe` - Main CLI Interface

The primary command-line interface for push-to-talk dictation.

**Subcommands:**
- `transcribe start` - Start recording audio (push-to-talk)
- `transcribe stop` - Stop recording, transcribe, copy to clipboard, and auto-paste
- `transcribe daemon` - Placeholder (use `transcribe-daemon` instead)
- `transcribe client <file>` - Placeholder (use `transcribe-client` instead)

**Workflow:**
```bash
# Hold hotkey, speak
transcribe start

# Release hotkey
transcribe stop
# → Records audio
# → Sends to daemon for transcription
# → Copies result to clipboard via wl-copy
# → Auto-pastes into active window via ydotool (if available)
```

### `transcribe-daemon` - Long-Running Service

Background daemon that keeps the transcription model loaded in memory for fast repeated transcriptions.

**Features:**
- Loads Parakeet model once at startup (`models/parakeet-tdt-0.6b-v3-int8`)
- Listens on Unix socket: `/tmp/transcribe-rs-v2.sock`
- JSON-based request/response protocol
- Eliminates 3-4 second model load time per transcription
- Single-threaded (processes one request at a time)

**Usage:**
```bash
# Start daemon
transcribe-daemon

# Or via systemd
systemctl --user start transcribe-daemon
```

### `transcribe-client` - Daemon Client

Simple client for sending audio files to the daemon for transcription.

**Usage:**
```bash
transcribe-client /tmp/ptt_current.wav
# Outputs: transcribed text
```

Used internally by the `transcribe` CLI for the stop command.

## Push-to-Talk Usage

### Prerequisites

**System requirements:**
- Wayland compositor (tested on Hyprland)
- PulseAudio for audio capture
- `wl-clipboard` package (for wl-copy)
- `ydotool` package (optional, for auto-paste)
- `ffmpeg` for audio recording

**Install on Arch:**
```bash
sudo pacman -S wl-clipboard ydotool ffmpeg
```

### Setup

1. **Build release binaries:**
```bash
cargo build --release
```

2. **Start the daemon:**
```bash
./target/release/transcribe-daemon
# Or install as systemd service (see transcribe-daemon.service)
```

3. **Configure window manager keybindings:**

For Hyprland (`~/.config/hypr/hyprland.conf`):
```ini
# Push-to-talk: Hold Super+Shift+Ctrl+Alt+E to record
bind = SUPER SHIFT CTRL ALT, E, exec, /path/to/omarchy-stt/target/release/transcribe start
bindr = SUPER SHIFT CTRL ALT, E, exec, /path/to/omarchy-stt/target/release/transcribe stop
```

4. **Test the workflow:**
```bash
# Terminal test (without keybindings)
./target/release/transcribe start
# (speak for a few seconds)
./target/release/transcribe stop
# Should paste transcription into terminal
```

### Debug Logging

All operations are logged to `/tmp/ptt_rust_debug.log` with millisecond timestamps.

**View logs in real-time:**
```bash
tail -f /tmp/ptt_rust_debug.log
```

**Log modules:**
- `[recording]` - FFmpeg recording lifecycle
- `[cli]` - Main CLI operations and workflow
- `[clipboard]` - wl-copy operations
- `[paste]` - ydotool paste operations

## Common Commands

### Building and Testing

```bash
# Build all binaries
cargo build --release

# Build specific binary
cargo build --release --bin transcribe
cargo build --release --bin transcribe-daemon
cargo build --release --bin transcribe-client

# Run all tests (library + new CLI modules)
cargo test

# Run library tests only
cargo test --lib

# Run specific module tests
cargo test --lib clipboard
cargo test --lib recording

# Run doc tests
cargo test --doc

# Check code without building
cargo check

# Format code
cargo fmt

# Lint with Clippy
cargo clippy
```

### Daemon Management

```bash
# Start daemon manually
./target/release/transcribe-daemon

# Start via script
./start-daemon.sh

# Check daemon status (if using systemd)
systemctl --user status transcribe-daemon

# View daemon logs
journalctl --user -u transcribe-daemon -f
```

### CLI Testing

```bash
# Manual push-to-talk test
./target/release/transcribe start
# (speak)
./target/release/transcribe stop

# Test transcription of existing file
./target/release/transcribe-client samples/jfk.wav
```

### Development Setup

Before running examples or tests, download the required models:

```bash
# Create models directory
mkdir models

# Download Parakeet model (recommended for daemon)
cd models
wget https://blob.handy.computer/parakeet-v3-int8.tar.gz
tar -xzf parakeet-v3-int8.tar.gz
rm parakeet-v3-int8.tar.gz
cd ..

# Or download Whisper model (alternative)
cd models
wget https://blob.handy.computer/whisper-medium-q4_1.bin
cd ..
```

## Architecture

### Core Design Pattern

The library follows a **trait-based abstraction pattern** to support multiple transcription engines through a common interface:

1. **TranscriptionEngine trait** (`src/lib.rs`): The core trait that all engines implement, defining:
   - Associated types for `InferenceParams` and `ModelParams`
   - Model lifecycle methods: `load_model()`, `load_model_with_params()`, `unload_model()`
   - Transcription methods: `transcribe_samples()`, `transcribe_file()`

2. **Engine implementations** live in `src/engines/`:
   - **Whisper** (`src/engines/whisper.rs`): Uses whisper-rs bindings with hardware acceleration (Metal on macOS, Vulkan on Windows/Linux)
   - **Parakeet** (`src/engines/parakeet/`): ONNX-based implementation with separate encoder, decoder, and preprocessor models

3. **Remote transcription** (`src/remote/`): Async trait for remote API-based transcription (currently OpenAI)

### Module Structure

#### Core Library Modules

- **`src/lib.rs`**: Core types (`TranscriptionResult`, `TranscriptionSegment`) and the `TranscriptionEngine` trait
- **`src/audio.rs`**: Audio file reading and validation (enforces 16kHz, 16-bit, mono WAV format)
- **`src/engines/`**: Local inference engines
  - `whisper.rs`: Whisper engine using single GGML model files
  - `parakeet/`: Parakeet engine split across multiple files:
    - `engine.rs`: Engine implementation with quantization support (FP32/Int8)
    - `model.rs`: Core ONNX model management (encoder, decoder, preprocessor)
    - `timestamps.rs`: Timestamp processing at token/word/segment granularity
- **`src/remote/`**: Remote API engines (async trait)
  - `openai.rs`: OpenAI Whisper API implementation

#### CLI Support Modules (New)

- **`src/clipboard.rs`**: Clipboard operations using `wl-copy` (Wayland)
  - `copy_to_clipboard()` - Copies text via wl-copy subprocess
  - `add_trailing_space_after_punctuation()` - Adds space after `.!?` for natural consecutive dictations
  - Includes comprehensive debug logging

- **`src/notifications.rs`**: Desktop notifications using notify-rust
  - `notify_recording_started()` - "🎤 Recording" notification
  - `notify_recording_stopped()` - "⏹️ Processing..." notification
  - `notify_transcription_pasted()` - "✅ Pasted" with preview
  - `notify_transcription_copied()` - "📋 Copied to clipboard" with preview
  - `notify_error()` - "❌ Error" notifications

- **`src/paste.rs`**: Auto-paste functionality using ydotool
  - `paste_from_clipboard()` - Simulates Ctrl+V or Ctrl+Shift+V (for terminals)
  - `is_ydotool_available()` - Checks if ydotool is installed
  - Automatically detects terminal windows for correct key combo
  - 50ms delay for Wayland clipboard propagation
  - Requires `/tmp/.ydotool_socket`

- **`src/recording.rs`**: Audio recording management using ffmpeg
  - `start_recording()` - Spawns ffmpeg in background, manages PID file
  - `stop_recording()` - Sends SIGINT, waits for exit, validates file
  - Records from PulseAudio: `alsa_input.usb-046d_C922_Pro_Stream_Webcam_C4C393EF-02.analog-stereo`
  - Output format: 16kHz, mono, 16-bit WAV to `/tmp/ptt_current.wav`
  - PID tracking: `/tmp/ptt_recording.pid`
  - File stability verification before returning

- **`src/terminal_detect.rs`**: Terminal window detection (Hyprland-specific)
  - `get_active_window_class()` - Gets window class via hyprctl JSON API
  - `is_active_window_terminal()` - Detects if active window is a terminal
  - Supports: alacritty, kitty, wezterm, foot, konsole, terminator, xterm, urxvt, st, code

#### Binary Entry Points

- **`src/bin/cli.rs`**: Main CLI with clap argument parsing and subcommands
- **`src/bin/daemon.rs`**: Daemon that loads model and listens on Unix socket
- **`src/bin/client.rs`**: Simple client for daemon communication

### Key Architectural Decisions

#### Clipboard: wl-copy over arboard

**Decision:** Use `wl-copy` command instead of `arboard` Rust library.

**Reason:** The `arboard` crate had clipboard persistence issues on Wayland - the clipboard would be cleared when the process exited. Using `wl-copy` (the native Wayland clipboard tool) ensures the text remains in the clipboard after the CLI exits.

**Trade-off:** Requires `wl-clipboard` package installed, but guaranteed compatibility.

#### Auto-Paste: ydotool for Wayland

**Decision:** Use `ydotool` for simulating keyboard input on Wayland.

**Reason:** Wayland's security model prevents most applications from sending keyboard events. `ydotool` is a privileged tool specifically designed for this use case.

**Implementation:**
- Simulates key press/release sequences using key codes (29=Ctrl, 42=Shift, 47=V)
- Detects terminals to use Ctrl+Shift+V instead of Ctrl+V
- Falls back gracefully to clipboard-only if ydotool is unavailable

#### Terminal Detection: hyprctl

**Decision:** Use `hyprctl activewindow -j` to detect window type.

**Reason:** Needed to distinguish between terminal and non-terminal windows to use the correct paste key combination (Ctrl+Shift+V vs Ctrl+V).

**Limitation:** Hyprland-specific. Future work could add support for other compositors (Sway, etc.).

#### Recording: ffmpeg subprocess

**Decision:** Use ffmpeg as external process rather than Rust audio libraries.

**Reason:**
- FFmpeg handles all hardware quirks and audio driver complexity
- Cross-platform audio capture
- Built-in format conversion (to 16kHz mono)
- Battle-tested reliability

**Implementation:**
- SIGINT for graceful shutdown (ensures valid WAV file)
- SIGKILL fallback after 5-second timeout
- File stability checks before returning path

#### Daemon Architecture: Unix Socket

**Decision:** Client-server model with Unix domain sockets and JSON protocol.

**Benefits:**
- Model loaded once, kept in memory
- Eliminates 3-4 second model load time per transcription
- Simple request/response protocol
- Easy to extend with new features

**Trade-off:** Single-threaded (one request at a time), but sufficient for personal use.

### Model Format Differences

1. **Whisper**: Single `.bin` file (GGML format)
2. **Parakeet**: Directory with multiple ONNX files + vocab.txt

### Quantization Support (Parakeet only)

- FP32: `encoder-model.onnx`, `decoder_joint-model.onnx`
- Int8: `encoder-model.int8.onnx`, `decoder_joint-model.int8.onnx`
- Specified via `ParakeetModelParams::fp32()` or `ParakeetModelParams::int8()`

### Timestamp Granularity (Parakeet only)

- Token-level: Raw token boundaries
- Word-level: Grouped into words using whitespace detection
- Segment-level: Grouped into sentence-like segments
- Configured via `ParakeetInferenceParams.timestamp_granularity`

### Hardware Acceleration

- Whisper uses platform-specific features (Metal/Vulkan) via conditional compilation
- Parakeet uses ONNX Runtime which handles backend optimization

### Audio Requirements

All engines expect audio in this exact format:
- Format: WAV (PCM)
- Sample Rate: 16 kHz
- Channels: Mono (1)
- Bit Depth: 16-bit
- The `audio::read_wav_samples()` function validates and converts to f32 samples normalized to [-1.0, 1.0]

## Testing

### Library Tests

Tests are located in `tests/` directory (not inline with source):
- `tests/whisper.rs`: Whisper engine tests
- `tests/parakeet.rs`: Parakeet engine tests
- `tests/openai.rs`: OpenAI remote API tests

Requirements:
- Model files in `models/` directory
- Sample audio files in `samples/` directory (e.g., `samples/jfk.wav`)

### CLI Module Tests

New unit tests in CLI support modules (run with `cargo test --lib`):

- **clipboard.rs** - 5 tests
  - Punctuation space logic (period, question, exclamation)
  - No space after comma
  - Empty string handling

- **recording.rs** - 2 tests
  - `test_process_running_check()` - PID checking logic
  - `test_constants_are_valid()` - Constants validation

- **terminal_detect.rs** - 1 test
  - `test_terminal_detection_logic()` - Terminal app matching

- **paste.rs** - 1 test
  - `test_ydotool_check()` - Availability check

**Total: 10 unit tests** for CLI modules, all passing.

### Integration Testing

The CLI workflow requires manual testing with real hardware:

**Requirements:**
- Wayland environment with Hyprland
- Working microphone (PulseAudio)
- `wl-clipboard`, `ydotool`, `ffmpeg` installed
- `transcribe-daemon` running

**Test procedure:**
1. Start daemon: `./target/release/transcribe-daemon`
2. Test CLI: `./target/release/transcribe start` → speak → `./target/release/transcribe stop`
3. Verify transcription appears in clipboard and pastes into active window
4. Check debug log: `tail -f /tmp/ptt_rust_debug.log`

## Dependencies

### Core Library Dependencies

- **hound**: WAV file reading
- **ort**: ONNX Runtime bindings (for Parakeet)
- **whisper-rs**: Whisper.cpp bindings with hardware acceleration
- **async-openai**: OpenAI API client
- **ndarray**: N-dimensional arrays for model tensor operations
- **serde**, **serde_json**: JSON serialization for daemon protocol

### CLI Dependencies (New)

- **clap** `4.5` with derive features - Command-line argument parsing
- **notify-rust** `4.11` - Desktop notifications (D-Bus integration)
- **chrono** `0.4` - Timestamp formatting for debug logs
- **dirs** `5.0` - Platform-specific directory paths (for future config)
- **toml** `0.8` - TOML configuration parsing (for future config)
- **nix** `0.29` (Unix only) - Unix signal handling (SIGINT, SIGKILL for process management)

**Note on arboard:** While `arboard 3.4` is listed in dependencies, it is not used in the final implementation. The code uses `wl-copy` subprocess instead for better Wayland compatibility.

### Platform-Specific Features

Whisper-rs features are conditionally enabled based on target OS:
- macOS: Metal acceleration
- Windows/Linux: Vulkan acceleration

## File Locations

### Temporary Files

- `/tmp/ptt_current.wav` - Current/last recording (16kHz mono WAV)
- `/tmp/ptt_recording.pid` - Recording process PID during active recording
- `/tmp/ptt_rust_debug.log` - Debug log with timestamped module-tagged entries
- `/tmp/.ydotool_socket` - ydotool daemon socket
- `/tmp/transcribe-rs-v2.sock` - Transcription daemon Unix socket

### Model Files

- `models/parakeet-tdt-0.6b-v3-int8/` - Parakeet model directory (used by daemon)
- `models/whisper-medium-q4_1.bin` - Whisper model file (alternative)

### Service Files

- `transcribe-daemon.service` - systemd user service unit file
- `start-daemon.sh` - Helper script to start daemon

## Future Enhancements

Potential improvements documented in `/specs/IMPLEMENTATION_PLAN.md`:

1. **Configuration System** (Phase 4)
   - TOML config file at `~/.config/transcribe-rs/config.toml`
   - Configurable microphone source
   - Configurable model path
   - Configurable key bindings documentation

2. **Cross-Platform Support**
   - Make clipboard/paste modules conditional on platform
   - Add X11 support alongside Wayland
   - Windows/macOS compatibility layers

3. **Additional Window Managers**
   - Sway (Wayland) - similar to Hyprland
   - i3 (X11) - different window detection method
   - Generic fallback for unknown compositors

4. **Model Management**
   - `transcribe models list` - Show available models
   - `transcribe models download <name>` - Auto-download from blob.handy.computer
   - Multiple model support with runtime switching

5. **Installation**
   - Install script for system-wide deployment
   - AUR package for Arch Linux
   - AppImage or Flatpak for other distributions
