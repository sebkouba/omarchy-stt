# Hotkey Daemon Keyboard Issues

## Overview

The hotkey-daemon (`src/bin/hotkey-daemon.rs`) uses evdev for keyboard monitoring with EVIOCGRAB for exclusive access during LongRecording mode. Several keyboard-related bugs were discovered and partially fixed.

## Working: Enter Key Debounce in LongRecording

### Problem
When pressing Enter in LongRecording mode to submit+continue, the ydotool-simulated Enter key would arrive after re-grabbing and trigger another submit+continue, causing a cascade of empty transcriptions.

### Root Cause
After processing Enter:
1. We ungrab → send Ctrl+V and Enter via ydotool → start new recording → sleep 100ms → drain events → re-grab
2. ydotool's Enter event sometimes arrives AFTER the drain, getting captured as a new Enter press
3. This triggers another submit+continue with empty transcription

### Fix (Implemented)
Added 500ms debounce for Enter key in LongRecording mode:

```rust
// src/bin/hotkey-daemon.rs lines 721-723
let mut last_enter_submit: Option<Instant> = None;
let enter_debounce = Duration::from_millis(500);

// In the Enter handler (lines 888-894):
if let Some(last) = last_enter_submit {
    if last.elapsed() < enter_debounce {
        eprintln!("[DEBOUNCE] Ignoring Enter ({}ms since last)", last.elapsed().as_millis());
        continue;
    }
}
// ... process Enter ...
last_enter_submit = Some(Instant::now());
```

Logs show `[DEBOUNCE] Ignoring Enter (Xms since last)` when ydotool echo is blocked.

## Unsolved: Hotkey Key Leaks When Modifiers Released First

### Problem
In push-to-talk (Recording) mode, if user releases modifier keys (Ctrl+Shift+Alt+Super) before releasing the hotkey key (E), the E key leaks through to the focused application, producing "eeeeeee".

Example:
1. User presses Ctrl+Shift+Alt+Super+E → enters Recording mode (NOT grabbed)
2. User releases Ctrl, Shift, Alt, Super (while still holding E)
3. E is now just a plain keypress → goes to system → "eeee" appears
4. User releases E → transcription happens, but damage is done

### Why It's Hard

**Approach 1: Grab immediately on Recording entry**
- Problem: System sees modifier PRESSES but not RELEASES (we capture them)
- Result: Sticky modifiers - system thinks Ctrl/Shift/etc are still held
- Side effect: Subsequent paste operations get mangled (Ctrl+Shift+Ctrl+V)

**Approach 2: Send modifier releases via ydotool after ungrab**
- Tried sending release events for all modifiers after ungrabbing
- Still had timing issues and didn't fully resolve sticky state

### Potential Solution (Not Yet Implemented)

A state machine approach that detects modifier release while hotkey is still held:

1. In Recording state, track when modifiers are released
2. When FIRST modifier is released AND hotkey key is still held:
   - Immediately grab the keyboard (prevents E from leaking)
   - Immediately send release events for ALL modifiers via ydotool (clears sticky state)
   - Stay in Recording state (but now grabbed)
3. When hotkey key is released:
   - If grabbed, ungrab
   - Process transcription normally

This requires careful ordering:
```
T0: User presses Ctrl+Shift+Alt+Super+E → Recording (not grabbed)
T1: User releases Ctrl (first modifier release detected)
    → Grab keyboard NOW (Ctrl release goes to us, not system)
    → Send ydotool releases for Ctrl+Shift+Alt+Super (clears system state)
    → Now: keyboard grabbed, system modifier state clean
T2: User releases remaining modifiers → captured by grab, ignored
T3: User releases E → ungrab, process transcription
```

### State Machine Enhancement Needed

Current states in `RecordingState` enum:
- `Idle`
- `Recording { press_time, active }` - NOT grabbed
- `PendingGrab { active }` - waiting for all keys released before grab
- `LongRecording { active }` - GRABBED
- `PendingUngrab { should_transcribe, active }` - waiting for hotkey release

May need to add:
- `RecordingGrabbed { press_time, active }` - Recording but grabbed due to early modifier release

Or handle it within the existing Recording state by tracking `is_grabbed` separately.

## Related Files

- `src/bin/hotkey-daemon.rs` - Main daemon with state machine
- `lessons-learned/evdev-keyboard-grab.md` - Original grab/ungrab lessons

## Key evdev/ydotool Facts

- `device.grab()` gives exclusive access - ALL events from that device go ONLY to your process
- ydotool key codes: 29=LCtrl, 97=RCtrl, 42=LShift, 54=RShift, 56=LAlt, 100=RAlt, 125=LMeta, 126=RMeta, 28=Enter
- ydotool format: `ydotool key CODE:1` (press), `CODE:0` (release)
- evdev event values: 1=press, 0=release, 2=repeat (always skip repeat for state tracking)

## Testing Checklist

- [x] Enter debounce works - no rapid-fire submits in LongRecording
- [ ] Releasing modifiers before hotkey key doesn't leak characters
- [ ] No sticky modifiers after ungrab
- [x] PendingUngrab waits for hotkey key release (not any key)
