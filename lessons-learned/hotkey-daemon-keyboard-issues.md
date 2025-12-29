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

## UNSOLVED: Hotkey Key Leaks When Modifiers Released First

### Problem
In push-to-talk (Recording) mode, if user releases modifier keys (Ctrl+Shift+Alt+Super) before releasing the hotkey key (E), the E key leaks through to the focused application, producing "eeeeeee".

**Status as of 2025-12-29:** This problem is **NOT SOLVED**. Multiple approaches were attempted and documented, but none successfully addressed the root cause.

### Why It's Hard: Per-Device Modifier Tracking

The fundamental issue is that **Wayland compositors track modifier state separately for each input device**:

1. When we grab the kanata keyboard device, the compositor never sees modifier RELEASE events
2. The compositor's view of kanata's modifier state freezes with all modifiers DOWN
3. Sending synthetic releases via ydotool only affects the ydotool virtual device's state
4. When we later send Ctrl+V via ydotool for pasting, the compositor sees:
   - kanata device: Ctrl+Super still DOWN
   - ydotool device: Ctrl+V
   - Combined: Ctrl+Super+V (opens clipboard manager instead of pasting)

**Root cause:** We cannot clear the kanata device's modifier state in the compositor because:
- While grabbed, events don't reach compositor
- After ungrab, keys are already physically released (no events to send)
- ydotool operates on a different device that doesn't affect kanata's state

See `lessons-learned/2025-12-29-wayland-modifier-state-device-isolation.md` for detailed analysis.

### Approaches Attempted

#### 1. Immediate Grab with Synthetic Releases (Failed)

**Idea:** Grab immediately on hotkey press, snapshot modifiers, send synthetic releases after ungrab.

**Implementation (documented but not committed):**
```rust
/// Snapshot of modifiers at grab time
struct GrabbedModifiers {
    ctrl: bool, alt: bool, shift: bool, meta: bool,
}

/// Recording state includes grabbed modifiers
Recording {
    press_time: Instant,
    active: ActiveBinding,
    grabbed_modifiers: GrabbedModifiers,
}
```

**Why it failed:**
- Synthetic releases from ydotool don't clear kanata device's stuck modifiers
- Side effect: Subsequent paste operations become Ctrl+Super+V instead of Ctrl+V
- Result: Clipboard manager opens instead of pasting

#### 2. Grab-on-First-Modifier-Release (Proposed, Not Implemented)

**Idea:** When first modifier is released while hotkey is still held, immediately grab.

**Why we didn't try it:**
- Still faces the same per-device modifier tracking problem
- Added complexity for uncertain benefit
- Would need to track individual modifier states during Recording

### Current Workaround

**User training:** Release all keys simultaneously, or accept occasional "e" leaks.

**State machine protection:** The existing `PendingGrab` state correctly handles tap-to-toggle by waiting for all keys to release before grabbing, which works for long-recording mode entry but doesn't help with push-to-talk modifier leaks.

## Related Files

- `src/bin/hotkey-daemon.rs` - Main daemon with state machine
- `lessons-learned/2025-12-29-wayland-modifier-state-device-isolation.md` - Detailed analysis of the modifier leak problem
- `lessons-learned/evdev-keyboard-grab.md` - Original grab/ungrab lessons

## Key evdev/ydotool Facts

- `device.grab()` gives exclusive access - ALL events from that device go ONLY to your process
- Wayland compositors track modifier state **per input device**
- ydotool creates a separate `/dev/uinput` virtual device - events from it don't affect physical device state
- ydotool key codes: 29=LCtrl, 97=RCtrl, 42=LShift, 54=RShift, 56=LAlt, 100=RAlt, 125=LMeta, 126=RMeta, 28=Enter
- ydotool format: `ydotool key CODE:1` (press), `CODE:0` (release)
- evdev event values: 1=press, 0=release, 2=repeat (always skip repeat for state tracking)

## Testing Checklist

- [x] Enter debounce works - no rapid-fire submits in LongRecording
- [ ] Releasing modifiers before hotkey key doesn't leak characters (UNSOLVED)
- [ ] No sticky modifiers after ungrab (FAILS with immediate grab approach)
- [x] PendingUngrab waits for hotkey key release (not any key)
