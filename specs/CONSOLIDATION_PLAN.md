# Transcribe-RS v2: Rust Consolidation Plan

## Context

This is transcribe-rs-v2, a development copy of the working transcribe-rs system. The original (in `../transcribe-rs/`) remains **completely untouched** and is your production system that works perfectly.

### What Already Works (v1)
- Push-to-talk voice dictation triggered by Hyprland keybinding
- Daemon/client architecture (model stays loaded, 3-7x faster)
- Auto-paste to active window (terminal detection)
- Shell scripts handle: recording, transcription, clipboard, paste
- Socket path: `/tmp/transcribe-rs.sock` (v1 production)

### What's Changed in v2
- Socket path: `/tmp/transcribe-rs-v2.sock` (allows both to run simultaneously)
- Fresh build directory (no conflicts)
- All files copied (including models)

**Both versions can run side-by-side for safe testing.**

---

## Goal: Consolidate Shell Scripts → Rust

Replace bash scripts with a unified Rust CLI that provides better:
- Error handling and reporting
- Cross-platform compatibility foundations
- Easier distribution (fewer external dependencies)
- Type safety and maintainability

### Non-Goals
- Don't change the working functionality
- Don't try to handle all window managers yet
- Don't rebuild ffmpeg (keep using it)
- Don't add GUI (stay CLI-focused)

---

## Architecture: New CLI Design

### Current Structure (v1)
```
Hyprland keybindings
  ↓
ptt-test.sh (recording logic)
  ↓ spawns ffmpeg
  ↓ manages PID files
transcribe-to-clipboard.sh
  ↓ calls wl-copy
  ↓ calls ydotool
transcribe-client (Rust)
  ↓ Unix socket
transcribe-daemon (Rust)
```

### Proposed Structure (v2)
```
Hyprland keybindings
  ↓
transcribe record start
transcribe record stop
  ↓ spawns ffmpeg (same as before)
  ↓ Rust state management (no PID files)
  ↓ calls transcribe client internally
  ↓ uses arboard crate for clipboard
  ↓ uses ydotool or native Rust paste

OR

transcribe daemon --with-ptt
  ↓ combines daemon + PTT handling
  ↓ listens for client commands
  ↓ handles recording logic internally
```

### New Binary Structure

**Single binary with subcommands:**

```bash
transcribe                    # Main CLI entry point

# Daemon operations
transcribe daemon             # Start transcription daemon (current behavior)
transcribe daemon --with-ptt  # Daemon + PTT handling (future)

# Client operations
transcribe client <file>      # Send file to daemon (current behavior)
transcribe transcribe <file>  # Direct transcription (no daemon)

# PTT recording operations
transcribe record start       # Start PTT recording
transcribe record stop        # Stop & transcribe & paste

# Utility operations
transcribe config             # Show current config
transcribe setup              # Interactive setup wizard (future)
transcribe models             # Model management (future)
```

---

## Implementation Phases

### Phase 1: Create Unified CLI Binary (CURRENT FOCUS)

**Goal:** Single `transcribe` binary that can do everything the shell scripts do.

**Files to create:**
- `src/bin/cli.rs` - Main CLI entry point with clap for argument parsing
- `src/recording.rs` - Recording logic (ffmpeg spawning, state management)
- `src/clipboard.rs` - Clipboard operations using `arboard` crate
- `src/paste.rs` - Auto-paste logic (ydotool wrapper or native)
- `src/config.rs` - Config file structure (future)

**Minimal working version:**
```bash
# Must work exactly like current system
transcribe record start  # = ptt-test.sh start
transcribe record stop   # = ptt-test.sh stop + transcribe-to-clipboard.sh

# Update Hyprland config to call these instead of shell scripts
```

**Dependencies to add to Cargo.toml:**
```toml
clap = { version = "4.5", features = ["derive"] }
arboard = "3.4"           # Cross-platform clipboard
notify-rust = "4.11"      # Desktop notifications
dirs = "5.0"              # Config directory paths
```

**Keep using external tools for now:**
- ffmpeg (audio capture) - proven, handles all hardware
- ydotool (paste) - proven, works with Wayland

### Phase 2: Config File System

**Goal:** Stop hardcoding paths and settings.

**Config file location:** `~/.config/transcribe-rs/config.toml`

**Config structure:**
```toml
[audio]
microphone = "alsa_input.usb-046d_C922_Pro_Stream_Webcam_C4C393EF-02.analog-stereo"
sample_rate = 16000
recording_path = "/tmp/ptt_current.wav"

[model]
path = "~/.local/share/transcribe-rs/models/parakeet-tdt-0.6b-v3-int8"
engine = "parakeet"  # or "whisper"
quantization = "int8"  # or "fp32"

[daemon]
socket_path = "/tmp/transcribe-rs-v2.sock"

[keybinding]
# Documentation only - user must configure in their WM
hotkey = "Super+Shift+Ctrl+Alt+Q"

[integration]
auto_paste = true
add_space_after_punctuation = true
terminal_apps = ["alacritty", "kitty", "wezterm", "foot"]  # For Ctrl+Shift+V detection
```

**Files to create:**
- `src/config.rs` - Config struct with serde
- `transcribe config` - Show current config
- `transcribe config edit` - Open in $EDITOR

### Phase 3: Replace ydotool (Optional)

**Goal:** Reduce external dependencies while keeping reliability.

**Options:**
1. **Keep ydotool** (safest) - It works, users already have it
2. **Pure Rust** - Use `evdev` crate (requires input group permissions)
3. **Hybrid** - Try Rust, fallback to ydotool if available

**Recommendation:** Start with ydotool wrapper, make it pluggable later.

### Phase 4: Better Integration (Future)

**Goal:** Make it easier to install and configure.

**Features:**
- `transcribe setup` - Interactive setup wizard
  - Auto-detect microphone (pactl on Linux)
  - Download models if missing
  - Generate WM-specific keybinding examples
  - Test recording → transcription → paste flow

- `transcribe models list` - Show available models
- `transcribe models download <name>` - Download from blob.handy.computer
- `transcribe test` - End-to-end test without changing anything

### Phase 5: Cross-Platform Foundations (Future)

**Goal:** Make core work on Windows/Mac.

**What needs to be conditional:**
- Audio capture (ffmpeg is cross-platform, but device names differ)
- Clipboard (arboard handles this)
- Paste (platform-specific, conditional compilation)
- Daemon socket (Unix vs Windows named pipes)

**Not a priority until Linux version is polished.**

---

## Technical Decisions

### Why Keep ffmpeg?
- Battle-tested audio capture
- Handles all hardware quirks
- Cross-platform
- 16kHz mono conversion built-in
- Not worth replacing unless we need finer control

### Why Use arboard Instead of wl-copy?
- Pure Rust (one less dependency)
- Cross-platform (future-proof)
- Handles Wayland, X11, Windows, macOS
- More reliable clipboard lifecycle management

### Why Not Build a Hotkey Listener?
- Every window manager has its own system
- Wayland has no standard global hotkey protocol
- Users expect to configure hotkeys in their WM config
- Building universal hotkey listener = reinventing WM features badly

### State Management
Current: PID files at `/tmp/ptt_recording.pid`
New: In-memory state or atomic files with better locking

**Options:**
1. Keep PID file pattern (works, familiar)
2. Use named lock files with `fs2` crate
3. For daemon mode: internal state (no files)

**Recommendation:** Keep PID files for CLI mode, internal state for daemon mode.

---

## Testing Strategy

### Unit Tests
- Config parsing
- Audio file validation
- State management logic

### Integration Tests
- Record → transcribe → clipboard → paste flow
- Daemon startup/shutdown
- Client communication

### Manual Testing Checklist
```bash
# Test CLI works like shell scripts
transcribe record start
# (speak)
transcribe record stop
# Should paste transcription

# Test daemon mode
transcribe daemon &
transcribe client samples/jfk.wav
# Should print transcription

# Test both daemons can run
# Terminal 1: Original (production)
cd ../transcribe-rs
./start-daemon.sh

# Terminal 2: New version (dev)
cd ../transcribe-rs-v2
cargo run --bin transcribe -- daemon

# Both should work on different sockets
```

---

## Migration Path

### Step 1: Build CLI alongside shell scripts
- Shell scripts still work
- New CLI can be tested independently
- No rush to switch

### Step 2: Test new CLI thoroughly
```bash
# Try new CLI version
transcribe-rs-v2/target/release/transcribe record start
# ...
transcribe-rs-v2/target/release/transcribe record stop
```

### Step 3: Update Hyprland bindings when ready
```ini
# Old (production)
bind = SUPER SHIFT CTRL ALT, Q, exec, /home/seb/code/cloned/transcribe-rs/ptt-test.sh start
bindri = , Q, exec, /home/seb/code/cloned/transcribe-rs/ptt-test.sh stop

# New (when tested)
bind = SUPER SHIFT CTRL ALT, Q, exec, /home/seb/code/cloned/transcribe-rs-v2/target/release/transcribe record start
bindri = , Q, exec, /home/seb/code/cloned/transcribe-rs-v2/target/release/transcribe record stop
```

### Step 4: Run both for a while
- Keep v1 as backup
- Use v2 daily
- Fix any issues discovered

### Step 5: Replace v1 (when confident)
```bash
# Copy v2 over v1 OR update your keybindings permanently
```

---

## File Structure After Implementation

```
transcribe-rs-v2/
├── src/
│   ├── lib.rs                    # Core transcription library (unchanged)
│   ├── engines/                  # Parakeet, Whisper (unchanged)
│   ├── bin/
│   │   ├── cli.rs               # NEW: Main CLI entry point
│   │   ├── daemon.rs            # Current daemon (might integrate with CLI)
│   │   └── client.rs            # Current client (might integrate with CLI)
│   ├── recording.rs             # NEW: Recording logic (ffmpeg wrapper)
│   ├── clipboard.rs             # NEW: Clipboard operations (arboard)
│   ├── paste.rs                 # NEW: Auto-paste logic
│   └── config.rs                # NEW: Config file handling
├── specs/
│   └── CONSOLIDATION_PLAN.md    # This file
├── examples/                     # Existing examples
├── tests/                        # Existing tests
├── models/                       # Model files
├── *.sh                          # Shell scripts (keep as reference, deprecate usage)
└── Cargo.toml                    # Update with new dependencies

```

---

## Next Immediate Steps

1. **Add dependencies to Cargo.toml**
   - clap (CLI argument parsing)
   - arboard (clipboard)
   - notify-rust (notifications)
   - dirs (config paths)

2. **Create src/bin/cli.rs**
   - Argument parsing structure
   - Subcommand dispatch
   - Help messages

3. **Extract recording logic from ptt-test.sh → src/recording.rs**
   - Spawn ffmpeg
   - PID/state management
   - File validation

4. **Extract clipboard logic from transcribe-to-clipboard.sh → src/clipboard.rs**
   - Use arboard instead of wl-copy
   - Punctuation space logic
   - Terminal detection

5. **Test the new CLI**
   - `transcribe record start/stop` should work exactly like shell scripts
   - Both v1 and v2 should work simultaneously

6. **Update Hyprland keybindings** (when confident)
   - Point to new CLI binary

---

## Success Criteria

### Phase 1 Complete When:
- ✅ Single `transcribe` binary built
- ✅ `transcribe record start` works like `ptt-test.sh start`
- ✅ `transcribe record stop` works like `ptt-test.sh stop + transcribe-to-clipboard.sh`
- ✅ Can update Hyprland bindings to use new binary
- ✅ Shell scripts no longer needed (but kept as reference)

### Phase 2 Complete When:
- ✅ Config file system working
- ✅ No hardcoded paths
- ✅ `transcribe config` shows settings
- ✅ Easy to change microphone/model/settings

### Ready for Others When:
- ✅ Install script exists
- ✅ AUR package available
- ✅ Documentation for end users
- ✅ Tested on multiple WMs (Hyprland, Sway, i3)
- ✅ Works on X11 and Wayland

---

## Questions to Resolve

1. **CLI subcommand structure:** Should it be:
   - `transcribe record start/stop` (verb-noun)
   - `transcribe start-record/stop-record` (verb-noun combined)
   - `transcribe ptt start/stop` (explicit PTT naming)

2. **Daemon integration:** Should daemon handle PTT recording or keep separate?
   - Separate: `transcribe record` calls `transcribe client` (modular)
   - Integrated: `transcribe daemon --with-ptt` does everything (simpler)

3. **Config precedence:**
   - Config file only?
   - Config file + CLI args override?
   - Config file + ENV vars + CLI args?

4. **Backward compatibility:**
   - Keep old binaries (`transcribe-daemon`, `transcribe-client`)?
   - Deprecate and point to new CLI?

**Recommendation:** Start with separate binaries, integrate if it makes sense later.

---

## Resources

### Relevant Crates
- `clap` - CLI parsing: https://docs.rs/clap
- `arboard` - Clipboard: https://docs.rs/arboard
- `notify-rust` - Notifications: https://docs.rs/notify-rust
- `serde` - Config serialization (already have)
- `toml` - Config format: https://docs.rs/toml
- `dirs` - Platform dirs: https://docs.rs/dirs
- `evdev` - Linux input (future): https://docs.rs/evdev

### Current Codebase Reference
- Shell script logic: `ptt-test.sh` (recording, ffmpeg, validation)
- Clipboard/paste logic: `transcribe-to-clipboard.sh`
- Terminal detection: line 79-95 in transcribe-to-clipboard.sh
- Timing logic: line 86-108 in ptt-test.sh (critical for file stability)

---

**Last Updated:** 2025-11-04
**Status:** Ready to start Phase 1 - CLI implementation
