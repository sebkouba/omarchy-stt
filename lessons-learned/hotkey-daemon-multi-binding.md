# Hotkey Daemon: Multi-Binding Support & State Management

**Date:** 2025-12-26
**Feature:** Multiple hotkey bindings with different prompts/modes

## What We Built

Changed the hotkey daemon from a single hotkey to multiple bindings, each with its own behavior:
- `q` - LLM cleanup with tools ("clean" prompt)
- `e` - Raw transcription (no LLM)
- `w` - Q&A mode ("ask" prompt)
- `r` - OCR context mode
- `t` - GUI conversation mode

## Config Changes

### Old Format
```toml
[hotkey]
modifiers = ["super", "shift", "ctrl", "alt"]
key = "e"
tap_threshold_ms = 700
default_prompt = "clean"
```

### New Format
```toml
[hotkey]
modifiers = ["super", "shift", "ctrl", "alt"]
tap_threshold_ms = 700

[[hotkey.bindings]]
key = "q"
prompt = "clean"

[[hotkey.bindings]]
key = "e"
# No prompt = raw transcription

[[hotkey.bindings]]
key = "w"
prompt = "ask"
```

## Pitfalls & Solutions

### 1. Config Deserialization Breaking Other Binaries

**Problem:** Changed `HotkeyConfig` struct broke `transcribe-client` and other binaries that load the config, even though they don't use the hotkey settings.

**Symptom:** `TomlError: missing field 'key'` when transcribing

**Solution:** After changing config structs, rebuild ALL binaries (`cargo build --release`) and restart ALL daemons, not just the one you're working on.

**Prevention:** Config changes affect everything that calls `Config::load()`. Always do a full rebuild.

### 2. Client/Daemon State Desync

**Problem:** Recording daemon's internal state (`recording_start_index: Option<usize>`) could desync from client's state file (`/tmp/ptt_recording.pid`).

**Scenario:**
1. User starts recording → both client and daemon track state
2. Something fails during stop/cancel → client removes its file, daemon keeps its state
3. Next recording attempt → daemon says "Already recording"

**Solution:** Added a `cancel` command to the recording daemon that **unconditionally** resets state:

```rust
// In recording-daemon.rs
fn handle_cancel(state: SharedState) -> Value {
    let mut state_guard = state.lock().unwrap();
    state_guard.recording_start_index = None;  // Force reset
    state_guard.is_recording.store(false, Ordering::Relaxed);
    json!({"ok": true, "was_recording": was_recording})
}
```

```rust
// In recording.rs - client side
pub fn cancel_recording(config: &AudioConfig) -> Result<(), Box<dyn Error>> {
    // Remove local state file first
    fs::remove_file(&config.recording_pid_file).ok();

    // Then tell daemon to reset (unconditionally)
    let request = serde_json::json!({"command": "cancel"});
    // ... send to daemon
}
```

**Key insight:** When dealing with distributed state (client file + daemon memory), provide an "unconditional reset" operation for recovery.

### 3. Import Placement After Context Compression

**Problem:** After Claude Code context compression, imports can get placed in wrong locations (e.g., `use std::collections::HashMap;` placed inside a function body instead of at the top).

**Solution:** Always check imports are at the top of the file after significant edits. Run `cargo check` to catch misplaced imports.

### 4. State Machine Needs to Track Active Binding

**Problem:** With multiple bindings, the state machine needs to know WHICH key started the recording to match the release correctly.

**Solution:** Wrap binding info in the state enum variants:

```rust
struct ActiveBinding {
    key: Key,
    binding: HotkeyBinding,
}

enum RecordingState {
    Idle,
    Recording { press_time: Instant, active: ActiveBinding },
    LongRecording { active: ActiveBinding },
}
```

Then match on the active key:
```rust
(RecordingState::Recording { active, .. }, ShortcutState::Released, Some(_), _)
    if pressed_key == active.key => { ... }
```

### 5. Notifications Can Come From Multiple Sources

**Problem:** After removing notifications from hotkey-daemon, user still saw notifications.

**Investigation:** Grepped for `notify` across codebase - found notifications in:
- `cli.rs`
- `file_watcher.rs`
- `gui/conversation.rs`

**Lesson:** When removing a feature (like notifications), search the entire codebase, not just the file you're working on.

## Testing Checklist

After changes to hotkey/recording system:

1. [ ] `cargo build --release` (full rebuild)
2. [ ] `systemctl --user restart recording-daemon hotkey-daemon transcribe-daemon`
3. [ ] Test normal push-to-talk (hold, speak, release)
4. [ ] Test tap-to-toggle (tap, speak, tap again)
5. [ ] Test cancel (Escape during long recording)
6. [ ] Test rapid successive recordings
7. [ ] Test switching between different hotkeys (q, e, w, etc.)
8. [ ] Check logs: `tail -f /tmp/ppt_rust_debug.log`

## Files Changed

- `src/config.rs` - New `HotkeyBinding` struct, changed `HotkeyConfig`
- `src/bin/hotkey-daemon.rs` - Multi-binding support, state tracking
- `src/bin/recording-daemon.rs` - Added `cancel` command
- `src/recording.rs` - Added `cancel_recording()` function

## Debug Commands

```bash
# Check daemon status
systemctl --user status hotkey-daemon recording-daemon transcribe-daemon

# Watch logs
tail -f /tmp/ppt_rust_debug.log

# Check recording state
cat /tmp/ptt_recording.pid  # Should not exist when idle

# Ping recording daemon
echo '{"command":"ping"}' | nc -U /tmp/transcribe-rs-v2-recording.sock

# Force cancel (if stuck)
echo '{"command":"cancel"}' | nc -U /tmp/transcribe-rs-v2-recording.sock
```
