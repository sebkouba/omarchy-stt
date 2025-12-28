# evdev Keyboard Grab for Long-Running Dictation

## Problem

When implementing "Enter to submit and continue" during long-running dictation:
1. Need to capture the Enter key during dictation so it doesn't pass through to the app
2. Need to send simulated keys (Ctrl+V for paste, Enter for submit) via ydotool
3. These two requirements conflict when using EVIOCGRAB

## Key Insight: Grabbed Keyboards Capture ALL Events

When you call `device.grab()` on an evdev device:
- You get **exclusive access** to that keyboard
- ALL key events from that device go ONLY to your process
- This includes events from OTHER processes like ydotool that use the same virtual device

### The Bug

```
User presses Enter during LongRecording (grabbed)
  → Daemon captures Enter, triggers submit+continue
  → Daemon calls ydotool to send Ctrl+V (paste)
  → ydotool sends key events to virtual keyboard
  → BUT: Daemon has grabbed the keyboard!
  → Daemon captures Ctrl+V as "[IGNORED] Key"
  → Daemon calls ydotool to send Enter
  → Daemon captures Enter again!
  → Triggers ANOTHER submit+continue with empty transcription
```

Logs showed this clearly:
```
[KEY] KEY_ENTER PRESS | state=LongRecording | grabbed=true
[TRANSITION] LongRecording: Enter pressed - submit + continue
[KEY] KEY_LEFTCTRL PRESS | state=LongRecording | grabbed=true
[IGNORED] Key KEY_LEFTCTRL pressed in LongRecording
[KEY] KEY_V PRESS | state=LongRecording | grabbed=true
[IGNORED] Key KEY_V pressed in LongRecording
[KEY] KEY_ENTER PRESS | state=LongRecording | grabbed=true
[TRANSITION] LongRecording: Enter pressed - submit + continue  <-- DOUBLE TRIGGER!
[ERROR] Submit+continue failed: Empty transcription
```

## Solution: Ungrab Before Sending Keys

```rust
// LONG RECORDING + Enter pressed -> Submit + Continue
(RecordingState::LongRecording { active }, true, _, _) if is_enter => {
    // IMPORTANT: Ungrab before sending keys via ydotool
    if is_grabbed {
        device.ungrab()?;
        is_grabbed = false;
    }

    // Process transcription, paste, send Enter
    process_transcription_and_send_enter(&config, prompt)?;

    // Start new recording segment
    start_recording(&config)?;

    // Delay to let ydotool events complete
    std::thread::sleep(Duration::from_millis(100));

    // Drain any pending events before re-grabbing
    if let Ok(events) = device.fetch_events() {
        let _ = events.count(); // consume events
    }

    // Re-grab for continued long recording
    device.grab()?;
    is_grabbed = true;
}
```

## Other Lessons Learned

### 1. Modifier Key Tracking with Repeat Events

evdev sends three event values:
- `value=1`: Key pressed
- `value=0`: Key released
- `value=2`: Key repeat (held down)

Initially, modifier tracking broke because repeat events were updating state:
```rust
// BAD: This sets modifier to false on repeat events
modifiers.update(key, pressed); // pressed=false when value=2

// GOOD: Ignore repeat events entirely
if event.value() == 2 { continue; }
modifiers.update(key, pressed, released);
```

### 2. PendingGrab/PendingUngrab States

**PendingGrab:** Wait for ALL keys to be released before grabbing:
- Avoids "sticky modifier" issues where system thinks Shift is still held
- Use `all_keys_released = key_pressed.is_empty() && modifiers.all_released()`

**PendingUngrab:** Ungrab on FIRST key release, NOT all keys released:
- If you wait for all keys, user can get stuck by pressing more keys
- Clear key_pressed and modifiers after ungrab to avoid stale state

**Bug found:** User got stuck in PendingUngrab for 17 seconds because:
1. State was PendingUngrab, waiting for all_keys_released
2. User frantically pressed hotkey, Escape, Enter trying to escape
3. Each new keypress added to key_pressed, resetting the condition
4. Fix: Ungrab on first key release, not all keys released

State machine:
```
Idle → [hotkey press] → Recording
Recording → [hotkey release < 700ms] → PendingGrab
PendingGrab → [all keys released] → LongRecording (grab)
LongRecording → [Enter] → (ungrab, process, regrab, stay in LongRecording)
LongRecording → [hotkey/Escape] → PendingUngrab
PendingUngrab → [any key release] → Idle (ungrab immediately)
```

### 3. Auto-Select Virtual Keyboards for Systemd

When running as a systemd service (non-interactive), can't wait for keypress to detect keyboard. Auto-select logic:
```rust
if is_interactive() {
    detect_active_keyboard(&mut keyboards) // wait for keypress
} else {
    auto_select_keyboard(&keyboards) // prefer kanata/kmonad/keyd
}
```

### 4. Use eprintln for Debug Logs in Daemons

`println!` may be buffered and not show up in journalctl. Use `eprintln!` for immediate, unbuffered output that reliably appears in logs.

## Dependencies

```toml
evdev = "0.12"  # Raw evdev access with grab() support
nix = { version = "0.29", features = ["signal", "poll"] }  # poll() for non-blocking
```

Note: `evdev-shortcut` crate doesn't expose grab() - need raw `evdev` crate.

## Testing Checklist

1. [ ] Tap hotkey → enters LongRecording with grab
2. [ ] Keys don't pass through while grabbed
3. [ ] Enter → transcribes, pastes, sends Enter, continues recording
4. [ ] Escape → cancels, ungrabs
5. [ ] Hotkey again → finishes, transcribes, ungrabs
6. [ ] No double-trigger on Enter (check logs for single transition)
7. [ ] ydotool Ctrl+V and Enter reach the target app
