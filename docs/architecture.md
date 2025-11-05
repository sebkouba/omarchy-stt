# Architecture Documentation

## System Overview

**transcribe-rs-v2** is a high-performance push-to-talk dictation system for Linux/Wayland that uses a multi-process architecture to achieve near-instant recording startup and fast transcription.

### The Four Binaries

The system consists of four separate compiled programs:

1. **`transcribe`** (CLI) - User-facing command-line interface
2. **`recording-daemon`** - Always-running audio capture service
3. **`transcribe-daemon`** - Always-running AI transcription service
4. **`transcribe-client`** - Bridge utility for daemon communication

---

## Architecture Design

### Two-Daemon Architecture

The system uses **two independent daemons** that run continuously as systemd services:

```
┌─────────────────────────────────────────────────────────────────┐
│                         USER INTERACTION                        │
│                                                                 │
│  Press Hotkey (SUPER+SHIFT+CTRL+ALT+E)                         │
│         ↓                                                       │
│  Hyprland executes: /path/to/transcribe start                  │
│         ↓                                                       │
│  Release Hotkey                                                 │
│         ↓                                                       │
│  Hyprland executes: /path/to/transcribe stop                   │
└─────────────────────────────────────────────────────────────────┘
                         ↓
┌─────────────────────────────────────────────────────────────────┐
│                      CLI ORCHESTRATION                          │
│                                                                 │
│  transcribe (binary)                                            │
│  • Parses start/stop commands                                  │
│  • Manages user notifications                                  │
│  • Handles clipboard operations                                │
│  • Coordinates workflow                                         │
└─────────────────────────────────────────────────────────────────┘
           ↓                              ↓
┌──────────────────────────┐    ┌──────────────────────────┐
│   RECORDING DAEMON       │    │  TRANSCRIPTION DAEMON    │
│                          │    │                          │
│  recording-daemon        │    │  transcribe-daemon       │
│  • 24/7 FFmpeg process   │    │  • Pre-loaded Parakeet   │
│  • Circular RAM buffer   │    │  • Pre-loaded Harper     │
│  • Instant start/stop    │    │  • Fast inference        │
│  • WAV generation        │    │  • Grammar checking      │
│                          │    │                          │
│  Socket:                 │    │  Socket:                 │
│  /tmp/transcribe-rs-v2-  │    │  /tmp/transcribe-rs-v2.  │
│  recording.sock          │    │  sock                    │
└──────────────────────────┘    └──────────────────────────┘
           ↓                              ↑
           └──────────[WAV file]──────────┘
                                          │
                                [transcribe-client]
```

### Why Two Daemons?

**Separation of Concerns:**
- **Recording** = Low CPU, continuous operation, hardware interaction
- **Transcription** = High CPU burst, AI inference, memory-intensive

**Performance Benefits:**
- **Recording daemon**: Eliminates FFmpeg startup lag (0-10ms vs 100-300ms)
- **Transcription daemon**: Eliminates model loading (instant vs 3-4 seconds)
- **Combined**: Sub-second end-to-end latency

**Reliability Benefits:**
- **Isolated failures**: If transcription crashes, recording still works
- **Independent restarts**: Update one daemon without affecting the other
- **Auto-recovery**: Recording daemon auto-restarts FFmpeg on crash

---

## Component Deep Dive

### 1. CLI Binary (`transcribe`)

**Location:** `src/bin/cli.rs`
**Purpose:** User interface and workflow orchestration

**Commands:**
```bash
transcribe start   # Start recording
transcribe stop    # Stop and transcribe
transcribe config  # Manage configuration
transcribe doctor  # System health check
```

**Key Responsibilities:**
- Parse command-line arguments via `clap`
- Coordinate recording start/stop via socket communication
- Orchestrate the transcription pipeline
- Apply post-processing (punctuation spacing, transcription corrections)
- Manage clipboard and auto-paste functionality
- Display desktop notifications
- Performance metrics tracking

**Workflow on `transcribe stop`:**
1. Connect to recording-daemon socket
2. Send stop command with recording start index
3. Receive WAV file path from daemon
4. Spawn `transcribe-client` subprocess with WAV path
5. Capture transcription text from stdout
6. Apply transcription corrections (phonetic fixes)
7. Add space after punctuation (`.`, `!`, `?`)
8. Copy to clipboard via `wl-copy`
9. Auto-paste via `ydotool` (Ctrl+V or Ctrl+Shift+V)
10. Show notification with preview

**Why spawn transcribe-client?**
- Clean separation: CLI doesn't need daemon protocol code
- Simpler process model: subprocess handles socket communication
- Reusable: client can be called standalone for scripting
- Better error handling: subprocess failures are isolated

### 2. Recording Daemon (`recording-daemon`)

**Location:** `src/bin/recording-daemon.rs`
**Socket:** `/tmp/transcribe-rs-v2-recording.sock`
**Service:** `~/.config/systemd/user/recording-daemon.service`

**Purpose:** Zero-latency audio capture via continuous FFmpeg process

**How It Works:**

1. **Startup:**
   - Spawns FFmpeg subprocess recording from microphone
   - Creates 120-second circular buffer in RAM (~3.7 MB)
   - FFmpeg continuously writes 16-bit PCM samples to buffer
   - Creates Unix socket and listens for commands

2. **On `start` command:**
   - Returns current buffer write position (index)
   - No audio processing needed (already recording!)
   - Latency: 0-10ms

3. **On `stop` command:**
   - Receives start index from client
   - Extracts audio samples from circular buffer
   - Generates WAV file at `/tmp/ptt_current.wav`
   - Returns file path and metadata
   - Latency: 5-15ms

4. **Auto-Recovery:**
   - Monitors FFmpeg process health
   - Auto-restarts on crash
   - Preserves buffer continuity

**Protocol (JSON over Unix socket):**

Request:
```json
{"command": "start"}
```

Response:
```json
{"ok": true, "start_index": 123456}
```

Request:
```json
{"command": "stop", "start_index": 123456}
```

Response:
```json
{
  "ok": true,
  "wav_path": "/tmp/ptt_current.wav",
  "duration_ms": 2500,
  "samples": 40000
}
```

**Key Features:**
- Buffer overflow protection (wraps at 120 seconds)
- Validates audio segment extraction
- Health check via `ping` command
- Debug logging to `/tmp/ptt_rust_debug.log`

### 3. Transcription Daemon (`transcribe-daemon`)

**Location:** `src/bin/daemon.rs`
**Socket:** `/tmp/transcribe-rs-v2.sock`
**Service:** `~/.config/systemd/user/transcribe-daemon.service`

**Purpose:** Fast AI transcription via pre-loaded models

**How It Works:**

1. **Startup (3-4 seconds):**
   - Loads Parakeet ONNX model into memory
   - Loads Harper grammar dictionary
   - Creates Unix socket and listens

2. **On transcription request:**
   - Receives WAV file path via socket
   - Reads audio samples from file
   - Runs Parakeet inference (already loaded!)
   - Applies Harper grammar corrections (already loaded!)
   - Returns text and metrics
   - Latency: Proportional to audio length (5-30x real-time)

3. **Single-Threaded Processing:**
   - One request at a time (sufficient for personal use)
   - Simple, reliable, no threading complexity

**Protocol (JSON over Unix socket):**

Request (new format with Harper config):
```json
{
  "file": "/tmp/ptt_current.wav",
  "harper": {
    "enabled": true,
    "dialect": "American",
    "user_dict_path": "/path/to/dict.txt",
    "disabled_linters": ["Readability"],
    "save_corrections": true,
    "corrections_dir": "/tmp/corrections"
  }
}
```

Response:
```json
{
  "success": true,
  "text": "Hello world.",
  "processing": {
    "transcription_ms": 1234,
    "harper_ms": 56,
    "corrections_applied": 2
  }
}
```

**Backward Compatibility:**
Old request format (without Harper config) still supported:
```json
{"file": "/tmp/ptt_current.wav"}
```

**Key Features:**
- Model persistence across requests (why daemon exists!)
- Harper integration eliminates 300ms dictionary loading
- Processing metrics for performance tracking
- Error handling with detailed messages

### 4. Transcribe Client (`transcribe-client`)

**Location:** `src/bin/client.rs`
**Purpose:** Bridge between CLI and transcription daemon

**What It Does:**

This is a **simple wrapper program** that:
1. Takes a WAV file path as command-line argument
2. Loads config to get Harper settings
3. Connects to transcribe-daemon Unix socket
4. Sends JSON request with file path and Harper config
5. Receives JSON response with transcribed text
6. Prints text to stdout (for CLI to capture)
7. Exits

**Example Usage:**

```bash
# Standalone usage
transcribe-client /tmp/ptt_current.wav
# Output: "Hello world."

# Called by CLI (internal)
output = Command::new("transcribe-client").arg(file).output()
text = String::from_utf8(output.stdout)
```

**Why Does This Exist?**

You might wonder: "Why not just call the daemon directly from the CLI?"

**Excellent question!** Here are the reasons:

1. **Clean Separation of Concerns:**
   - CLI handles: recording, clipboard, paste, notifications, UX
   - Client handles: daemon socket protocol, JSON serialization
   - Each binary has a single, focused responsibility

2. **Reusability:**
   - Can be called standalone for scripting: `transcribe-client audio.wav`
   - Shell scripts can use it without needing CLI overhead
   - Other tools can integrate transcription easily

3. **Simpler Process Model:**
   - CLI spawns client as subprocess
   - Client's stdout = transcription text (simple interface)
   - No need to link daemon protocol into CLI binary

4. **Better Error Isolation:**
   - If client crashes, CLI can catch and handle it
   - Subprocess failures don't take down CLI
   - Clean exit codes for error handling

5. **Development Flexibility:**
   - Can update client protocol without rebuilding CLI
   - Can test daemon communication independently
   - Easier debugging (run client standalone)

**Analogy:**
Think of it like a **telephone operator**:
- You (CLI) want to talk to someone (daemon)
- You call the operator (client) with a message (WAV file)
- Operator connects and relays your message
- Operator returns the response
- You don't need to know the internal phone system

**Code Flow:**

```
CLI (cli.rs:470-506)
  ↓
  Find transcribe-client in same directory
  ↓
  Command::new("transcribe-client").arg(wav_file).output()
  ↓
transcribe-client (client.rs:74-133)
  ↓
  Load config
  ↓
  Connect to /tmp/transcribe-rs-v2.sock
  ↓
  Send JSON request with Harper config
  ↓
  Receive JSON response
  ↓
  Print text to stdout
  ↓
CLI
  ↓
  Capture stdout
  ↓
  Process text (corrections, spacing)
  ↓
  Clipboard + paste
```

---

## Complete Request Flow

### Press Hotkey (Start Recording)

```
Time   | Component            | Action
-------|---------------------|----------------------------------------
0ms    | User                | Presses hotkey
1ms    | Hyprland            | Executes: transcribe start
2ms    | CLI                 | Parses "start" command
3ms    | CLI                 | Connects to recording-daemon socket
4ms    | CLI                 | Sends: {"command": "start"}
5ms    | recording-daemon    | Returns: {"ok": true, "start_index": 123456}
6ms    | CLI                 | Saves index to /tmp/ptt_recording.pid
7ms    | CLI                 | Shows notification: "🎤 Recording"
8ms    | CLI                 | Exits
-------|---------------------|----------------------------------------
Total: ~8ms (instant!)
```

**Note:** FFmpeg was already running! No process spawn needed.

### Release Hotkey (Stop & Transcribe)

```
Time    | Component            | Action
--------|---------------------|----------------------------------------
0ms     | User                | Releases hotkey
1ms     | Hyprland            | Executes: transcribe stop
2ms     | CLI                 | Parses "stop" command
3ms     | CLI                 | Reads start_index from /tmp/ptt_recording.pid
4ms     | CLI                 | Connects to recording-daemon socket
5ms     | CLI                 | Sends: {"command": "stop", "start_index": 123456}
10ms    | recording-daemon    | Extracts samples from circular buffer
15ms    | recording-daemon    | Writes /tmp/ptt_current.wav
20ms    | recording-daemon    | Returns: {"ok": true, "wav_path": "..."}
25ms    | CLI                 | Shows notification: "⏹️ Processing..."
30ms    | CLI                 | Spawns: transcribe-client /tmp/ptt_current.wav
35ms    | transcribe-client   | Loads config
40ms    | transcribe-client   | Connects to transcribe-daemon socket
45ms    | transcribe-client   | Sends: {"file": "...", "harper": {...}}
50ms    | transcribe-daemon   | Reads WAV file (already loaded!)
500ms   | transcribe-daemon   | Parakeet inference (model already loaded!)
550ms   | transcribe-daemon   | Harper corrections (dict already loaded!)
555ms   | transcribe-daemon   | Returns: {"success": true, "text": "Hello world."}
560ms   | transcribe-client   | Prints to stdout, exits
565ms   | CLI                 | Captures stdout: "Hello world."
570ms   | CLI                 | Applies transcription corrections
575ms   | CLI                 | Adds space after punctuation: "Hello world. "
580ms   | CLI                 | Copies to clipboard via wl-copy
590ms   | CLI                 | Detects active window (terminal vs GUI)
595ms   | CLI                 | Auto-pastes via ydotool (Ctrl+V or Ctrl+Shift+V)
600ms   | CLI                 | Shows notification: "✅ Pasted: Hello world."
605ms   | CLI                 | Logs performance metrics
610ms   | CLI                 | Exits
--------|---------------------|----------------------------------------
Total: ~610ms (for ~500ms of inference)
```

**Performance Breakdown:**
- Recording overhead: 20ms
- Daemon communication: 30ms
- Parakeet inference: 450ms (depends on audio length)
- Harper corrections: 50ms (no dictionary loading!)
- Post-processing: 25ms
- Clipboard + paste: 15ms

**Without daemons:** Add 3-4 seconds for model loading + 200ms for FFmpeg startup!

---

## Inter-Process Communication

### Communication Mechanisms

1. **Unix Domain Sockets:**
   - Faster than TCP/IP (no network stack)
   - Filesystem-based permissions
   - Reliable stream protocol

2. **JSON Protocol:**
   - Human-readable for debugging
   - Simple serialization/deserialization
   - Backward-compatible (optional fields)

3. **Subprocess Spawning:**
   - CLI spawns transcribe-client
   - Simple stdout capture
   - Clean exit codes

### File-Based State

**Temporary Files:**
- `/tmp/ptt_current.wav` - Current/last recording (16kHz mono 16-bit PCM)
- `/tmp/ptt_recording.pid` - Recording start index (repurposed from PID file)
- `/tmp/ptt_start_time.txt` - Performance tracking timestamp
- `/tmp/ptt_rust_debug.log` - Unified debug log with timestamps
- `/tmp/transcribe-rs-v2.sock` - Transcription daemon socket
- `/tmp/transcribe-rs-v2-recording.sock` - Recording daemon socket

**Persistent Files:**
- `~/.config/transcribe-rs/config.toml` - User configuration
- `models/parakeet-tdt-0.6b-v3-int8/` - AI model files (ONNX)
- `~/.config/transcribe-rs/corrections/` - Harper correction logs

---

## Performance Characteristics

### Latency Numbers

| Operation                    | Latency      | Notes                              |
|------------------------------|--------------|-----------------------------------|
| Recording start              | 0-10ms       | FFmpeg already running            |
| Recording stop               | 5-15ms       | Buffer extraction + WAV write     |
| Daemon connection            | 1-5ms        | Unix socket                       |
| Parakeet inference           | 5-30x RT     | Hardware dependent                |
| Harper corrections           | 50-100ms     | Dictionary already loaded         |
| Clipboard copy               | 5-10ms       | wl-copy subprocess                |
| Auto-paste                   | 10-20ms      | ydotool key simulation            |
| **Total (3s audio)**         | **~800ms**   | Without daemons: **~4800ms**      |

**RT = Real-time** (3 seconds of audio = 150-900ms inference)

### Memory Usage

| Component            | RAM Usage    | Notes                              |
|----------------------|--------------|-----------------------------------|
| recording-daemon     | ~10 MB       | Includes 3.7 MB circular buffer   |
| transcribe-daemon    | ~300 MB      | Parakeet + Harper in memory       |
| transcribe (CLI)     | ~5 MB        | Short-lived, exits after paste    |
| transcribe-client    | ~3 MB        | Short-lived, exits after response |
| **Total**            | **~310 MB**  | Continuous overhead               |

### CPU Usage

| Component            | CPU (Idle)   | CPU (Active)  | Notes                    |
|----------------------|--------------|---------------|--------------------------|
| recording-daemon     | 5-10%        | 10-15%        | Continuous FFmpeg        |
| transcribe-daemon    | 0%           | 200-400%      | Burst during inference   |
| transcribe (CLI)     | -            | 5-10%         | Short-lived              |

---

## Configuration

### Config File Location

`~/.config/transcribe-rs/config.toml`

### Key Settings

```toml
[audio]
microphone = "default"  # or specific PulseAudio source
log_file = "/tmp/ptt_rust_debug.log"

[model]
model_type = "parakeet"
path = "models/parakeet-tdt-0.6b-v3-int8"

[daemon]
socket_path = "/tmp/transcribe-rs-v2.sock"

[recording_daemon]
socket_path = "/tmp/transcribe-rs-v2-recording.sock"
buffer_seconds = 120

[harper]
enabled = true
dialect = "American"
dictionary_path = ""  # Empty = use defaults

[integration]
auto_paste = true
add_space_after_punctuation = true

[transcription_corrections]
enabled = true
corrections_file = "transcription_corrections.txt"
```

---

## Deployment

### Installation Steps

1. **Build all binaries:**
   ```bash
   cargo build --release
   ```
   This creates four binaries in `target/release/`:
   - `transcribe`
   - `recording-daemon`
   - `transcribe-daemon`
   - `transcribe-client`

2. **Install systemd services:**
   ```bash
   # Recording daemon
   cp recording-daemon.service ~/.config/systemd/user/
   systemctl --user daemon-reload
   systemctl --user enable recording-daemon
   systemctl --user start recording-daemon

   # Transcription daemon
   cp transcribe-daemon.service ~/.config/systemd/user/
   systemctl --user daemon-reload
   systemctl --user enable transcribe-daemon
   systemctl --user start transcribe-daemon
   ```

3. **Configure Hyprland keybinding:**
   ```ini
   # ~/.config/hypr/hyprland.conf
   bind = SUPER SHIFT CTRL ALT, E, exec, /home/seb/code/cloned/transcribe-rs-v2/target/release/transcribe start
   bindr = SUPER SHIFT CTRL ALT, E, exec, /home/seb/code/cloned/transcribe-rs-v2/target/release/transcribe stop
   ```

4. **Initialize configuration:**
   ```bash
   ./target/release/transcribe config init
   ```

5. **Check system health:**
   ```bash
   ./target/release/transcribe doctor
   ```

### Service Management

```bash
# Check daemon status
systemctl --user status recording-daemon
systemctl --user status transcribe-daemon

# View logs
journalctl --user -u recording-daemon -f
journalctl --user -u transcribe-daemon -f

# Restart daemons
systemctl --user restart recording-daemon
systemctl --user restart transcribe-daemon
```

---

## Debugging

### Debug Logging

All components log to `/tmp/ptt_rust_debug.log` with timestamps:

```
[2025-11-05 12:34:56.789] [cli] === HANDLE START ===
[2025-11-05 12:34:56.790] [recording] Connected to recording daemon
[2025-11-05 12:34:56.791] [recording] Sent start command
[2025-11-05 12:34:56.792] [recording] Start index: 123456
[2025-11-05 12:34:58.123] [cli] === HANDLE STOP ===
[2025-11-05 12:34:58.124] [recording] Sent stop command
[2025-11-05 12:34:58.135] [recording] WAV file: /tmp/ptt_current.wav
[2025-11-05 12:34:58.140] [cli] Calling transcribe_file...
[2025-11-05 12:34:58.145] [cli] transcribe-client stdout: 'Hello world'
```

### Testing Components Individually

```bash
# Test recording daemon
echo '{"command":"ping"}' | nc -U /tmp/transcribe-rs-v2-recording.sock

# Test transcription daemon
echo '{"file":"/tmp/ptt_current.wav"}' | nc -U /tmp/transcribe-rs-v2.sock

# Test transcribe-client standalone
./target/release/transcribe-client samples/jfk.wav

# Test CLI workflow
./target/release/transcribe start
# (speak)
./target/release/transcribe stop
```

### Common Issues

**Problem:** Daemon not responding
**Solution:** Check socket exists, restart daemon

**Problem:** Empty transcription
**Solution:** Check audio file has valid samples, check microphone

**Problem:** Slow transcription
**Solution:** Check daemon loaded model (should be instant second time)

**Problem:** Auto-paste not working
**Solution:** Check ydotool running, verify terminal detection

---

## Design Principles

### Why This Architecture?

1. **Performance First:**
   - Sub-second end-to-end latency
   - No cold-start penalties
   - Minimal overhead per transcription

2. **Separation of Concerns:**
   - Audio capture ≠ AI inference
   - Each daemon has single responsibility
   - Clean interfaces between components

3. **Reliability:**
   - Isolated failure domains
   - Auto-recovery mechanisms
   - Graceful degradation

4. **Developer Experience:**
   - Simple protocols (JSON + sockets)
   - Standalone component testing
   - Clear debug logging

5. **User Experience:**
   - Instant feedback (notifications)
   - Transparent operation
   - No manual model management

### Alternative Architectures Considered

**Single Daemon (Recording + Transcription):**
- ❌ Mixing concerns (audio + AI)
- ❌ Complex failure modes
- ❌ Harder to debug
- ✅ Fewer processes

**No Daemons (CLI does everything):**
- ❌ 3-4 second model load every time
- ❌ FFmpeg startup lag every time
- ❌ Terrible UX
- ✅ Simpler architecture

**Embedded Client (no separate binary):**
- ❌ CLI needs daemon protocol code
- ❌ Less reusable
- ✅ One fewer binary
- ✅ Slightly simpler

**Verdict:** Two-daemon + separate client is optimal for performance and maintainability.

---

## Future Improvements

### Potential Enhancements

1. **Multi-Client Support:**
   - Currently single-threaded transcription
   - Could add request queue for concurrent users

2. **Hot Model Reloading:**
   - Switch models without restarting daemon
   - Useful for testing different models

3. **Streaming Transcription:**
   - Start transcription before recording finishes
   - Lower perceived latency

4. **Remote Transcription:**
   - Network socket support (TCP/TLS)
   - Transcribe on powerful remote machine

5. **Model Caching:**
   - Keep multiple models in RAM
   - Switch between languages quickly

6. **Buffer Persistence:**
   - Save buffer to disk on shutdown
   - Recover recordings after crash

### Non-Goals

- **Multi-platform support** - Linux/Wayland specific
- **GUI** - CLI/daemon only
- **Cloud transcription** - Local only for privacy
- **Real-time streaming** - Push-to-talk paradigm

---

## Summary

The transcribe-rs-v2 architecture achieves **sub-second dictation latency** through:

1. **Recording daemon** - Eliminates FFmpeg startup lag
2. **Transcription daemon** - Eliminates model loading lag
3. **Separate client** - Clean interface for daemon communication
4. **CLI orchestrator** - User-facing workflow management

Each component has a **single, focused responsibility** and communicates via **simple protocols** (Unix sockets + JSON).

The result: **instant push-to-talk dictation** that feels like native OS functionality.
