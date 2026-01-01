# Hotkey State Machine Extraction for Testability

**Date:** 2026-01-01
**Branch:** `feat/hotkey-state-machine-extraction`
**Status:** Implementation complete, ready for manual testing and CI integration

## Context

The goal was to build a comprehensive automated testing suite for the push-to-talk transcription system. The main challenge: the hotkey daemon (`src/bin/hotkey-daemon.rs`) had all state machine logic interleaved with side effects (D-Bus, recording daemon, clipboard, hyprctl), making it untestable.

## What Was Done

### 1. Extracted Pure State Machine (`src/hotkey_state.rs`)

Created a new module with:

```rust
// Events
pub enum HotkeyEvent {
    Activated { shortcut_id: String },
    Deactivated { shortcut_id: String },
}

// States
pub enum RecordingState {
    Idle,
    Recording { press_time, shortcut_id, prompt },
    LongRecording { shortcut_id, prompt, entered_at },
    PendingRepaste { shortcut_id, text },
    PendingTranscription { shortcut_id, prompt },
}

// Actions (side effects to execute)
pub enum Action {
    StartRecording,
    StopAndTranscribe { prompt },
    CancelRecording,
    BindLongRecordingKeys,
    UnbindLongRecordingKeys,
    Repaste { text },
    SubmitAndContinue { prompt },
    Notify { title, body },
}
```

The core function is pure:
```rust
impl RecordingState {
    pub fn transition(self, event: HotkeyEvent, ctx: &TransitionContext) -> TransitionResult {
        // Returns new_state + Vec<Action>
    }
}
```

### 2. Added StateMachine Wrapper

Manages internal caches (last_transcription, last_repaste_time) that the transition function needs but shouldn't own:

```rust
pub struct StateMachine {
    state: RecordingState,
    last_transcription: Option<String>,
    last_repaste_time: Option<Instant>,
}

impl StateMachine {
    pub fn handle_event(&mut self, event, binding_map, thresholds) -> Vec<Action>;
    pub fn cache_transcription(&mut self, text: String);
    pub fn record_repaste(&mut self);
}
```

### 3. Refactored hotkey-daemon.rs

The daemon now uses the extracted state machine:

```rust
// Main event loop (simplified)
let mut state_machine = StateMachine::new();

while running {
    let dbus_event = rx.recv().await;
    let hotkey_event: HotkeyEvent = dbus_event.into();

    let actions = state_machine.handle_event(hotkey_event, &binding_map, ...);

    for action in actions {
        execute_action(action, &config, &mut state_machine);
    }
}
```

The `execute_action()` function handles all side effects (start_recording, transcribe, clipboard, hyprctl, etc.).

### 4. Added 17 Unit Tests

All state transitions are now tested:

| Test | What it verifies |
|------|------------------|
| `test_idle_activation_starts_recording` | Basic activation |
| `test_hold_release_transcribes` | Push-to-talk (hold > tap_threshold) |
| `test_tap_enters_long_recording` | Tap < tap_threshold → long mode |
| `test_double_tap_repastes` | Quick double-tap → repaste |
| `test_long_recording_finish` | Tap after 1.5s → transcribe |
| `test_enter_submits_and_continues` | Enter key in long mode |
| `test_escape_cancels` | Escape key cancels |
| `test_switch_prompt_during_long` | Different key switches prompt |
| `test_pending_repaste_on_release` | Key release triggers paste |
| `test_pending_transcription_on_release` | Key release triggers transcription |
| `test_debounce_after_repaste` | Ignores activation within 1s of repaste |
| ... | (and more) |

Run with: `cargo test --lib hotkey_state`

## Files Changed

| File | Change |
|------|--------|
| `src/hotkey_state.rs` | **NEW** - 740 lines, pure state machine + tests |
| `src/lib.rs` | Added `pub mod hotkey_state;` |
| `src/bin/hotkey-daemon.rs` | Refactored to use StateMachine + execute_action() |

## How to Continue

### Immediate Next Steps

1. **Manual testing** - Run `./scripts/promote.sh` and test all hotkey patterns:
   - Push-to-talk (hold key)
   - Long recording (tap, speak, tap)
   - Double-tap repaste
   - Enter to submit+continue
   - Escape to cancel
   - Switch prompt during long recording

2. **Virtual audio testing** - The original goal was E2E testing with audio:
   ```bash
   # Create virtual sink for test audio
   pactl load-module module-null-sink sink_name=test_sink

   # Configure recording-daemon to use it
   RECORDING_MICROPHONE=test_sink.monitor ./recording-daemon

   # Play test audio
   paplay --device=test_sink samples/jfk.wav
   ```

3. **Integration tests** - Create `tests/hotkey_state.rs` for multi-step sequences:
   ```rust
   #[test]
   fn test_full_push_to_talk_sequence() {
       let mut sm = StateMachine::new();
       // Activate, wait, deactivate, verify actions
   }
   ```

### Testing Infrastructure Ideas (from initial analysis)

| Layer | Approach | Status |
|-------|----------|--------|
| Unit | State machine tests | ✅ Done (21 tests) |
| Component | Socket protocol tests | Existing in `tests/recording_daemon.rs` |
| Hotkey logic | D-Bus signal injection | Not yet implemented |
| Audio | Virtual PipeWire sink | Not yet implemented |
| E2E | Log parsing verification | Not yet implemented |

### Key Design Decisions

1. **Action enum over trait abstraction** - Simpler than injecting mock dependencies. Actions are data that can be inspected in tests.

2. **StateMachine owns caches** - `last_transcription` and `last_repaste_time` are managed by the wrapper, not passed through context every time.

3. **Instant in state** - States like `Recording { press_time }` use real `Instant`. Tests work by creating states with artificial timestamps.

4. **Separate D-Bus event type** - `DbusEvent` (with timestamp) → `HotkeyEvent` (without) conversion keeps state machine clean.

## Bug Found: State Pollution from Events

**Issue discovered:** Enter key worked once in long recording mode, then stopped working on subsequent presses.

**Root cause:** When handling Enter in `LongRecording`, the transition incorrectly used the event's `shortcut_id` ("transcribe-enter") instead of preserving the original key ("transcribe-e"):

```rust
// BAD - line 310-311 before fix
shortcut_id: shortcut_id.clone(),  // This was "transcribe-enter"!

// GOOD - after fix
shortcut_id: original_id.clone(),  // Preserves "transcribe-e"
```

**Effect:** After first Enter, state had `shortcut_id: "transcribe-enter"`. Second Enter matched "same key activated" branch, triggering double-tap detection instead of submit+continue.

**Prevention strategy implemented:**

1. **Invariant tests** - Added `test_invariant_*` tests that verify control keys never appear as shortcut_id in recording states. These catch the *class* of bug, not just this instance.

2. **Anti-pattern documentation** - Added "State pollution from events" pattern to CLAUDE.md with code examples.

3. **Test organization** - Tests now categorized as:
   - Invariant tests (properties that must always hold)
   - Regression tests (specific bugs found and fixed)
   - Behavior tests (expected functionality)

**Key insight:** Invariant tests > regression tests > behavior tests for catching future bugs. An invariant test would have caught this bug immediately, even if we hadn't anticipated this specific scenario.

## Gotchas

1. **Timing in tests** - Use `Instant::now() - Duration::from_millis(X)` to create past timestamps. Don't rely on `thread::sleep()` for timing-sensitive tests.

2. **One pre-existing flaky test** - `timing::tests::test_save_and_load_timestamp` occasionally fails due to file system race. Unrelated to this work.

3. **SubmitAndContinue restarts recording** - The action executor calls `start_recording()` after processing, keeping the user in long recording mode.

4. **Control key isolation** - "transcribe-enter" and "transcribe-escape" are control keys for long recording mode, NOT transcription bindings. They should never appear as `shortcut_id` in any recording state.

## Related Files

- `src/bin/hotkey-daemon.rs` - Uses the state machine
- `src/recording.rs` - Recording daemon client (socket protocol)
- `tests/recording_daemon.rs` - Existing socket protocol tests
- `~/.config/transcribe-rs/config.toml` - Hotkey bindings configuration
