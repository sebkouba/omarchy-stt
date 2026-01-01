---
name: hotkey-state-machine
description: Guidance for developing the hotkey state machine. Use when working on src/hotkey_state.rs, hotkey-daemon.rs, state transitions, or long recording mode.
---

# Hotkey State Machine Development

## Anti-Patterns

### State Pollution from Events

Never use event data where you should preserve existing state data.

```rust
// BAD: Uses event's shortcut_id instead of preserving original
(RecordingState::LongRecording { prompt, .. }, HotkeyEvent::Activated { shortcut_id })
    if shortcut_id == "transcribe-enter" => {
    TransitionResult {
        new_state: RecordingState::LongRecording {
            shortcut_id: shortcut_id.clone(),  // BUG: This is "transcribe-enter"!
            ...
        },
    }
}

// GOOD: Preserve the original shortcut_id
(RecordingState::LongRecording { shortcut_id: original_id, prompt, .. }, HotkeyEvent::Activated { shortcut_id })
    if shortcut_id == "transcribe-enter" => {
    TransitionResult {
        new_state: RecordingState::LongRecording {
            shortcut_id: original_id.clone(),  // Correct: preserves original
            ...
        },
    }
}
```

## Invariants

These must always hold and are enforced by `test_invariant_*` tests:

1. **Control key isolation**: `"transcribe-enter"` and `"transcribe-escape"` must NEVER appear as `shortcut_id` in any recording state. These are control keys, not transcription bindings.

2. **State continuity**: When handling control keys in long recording mode, the original `shortcut_id` and `prompt` must be preserved.

## Test Categories

- **Invariant tests** (`test_invariant_*`): Properties that must always hold
- **Regression tests**: Specific bugs that were found and fixed
- **Behavior tests**: Expected functionality of individual transitions

When adding new transitions, prefer invariant tests that catch the *class* of bug.

## Completion Checklist

Before considering state machine work complete:

- [ ] If new transition touches state fields: added invariant test
- [ ] `cargo test --lib hotkey_state` passes (all 21+ tests)
- [ ] `./scripts/build.sh` succeeds with no warnings
- [ ] New code preserves existing state fields unless change is intentional
