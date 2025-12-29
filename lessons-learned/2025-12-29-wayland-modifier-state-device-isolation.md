# Wayland Modifier State and Per-Device Input Tracking

## Context

We attempted to fix a critical UX bug in the hotkey-daemon where pressing Ctrl+Alt+Shift+Super+E for push-to-talk dictation would leak the "E" key if the user released the modifier keys before releasing E. This investigation revealed fundamental limitations in how Wayland compositors track modifier state across multiple input devices.

## The Problem

**User Experience:** When using the push-to-talk hotkey (Ctrl+Alt+Shift+Super+E):
1. User presses all five keys together
2. User releases the four modifier keys (Ctrl, Alt, Shift, Super) while still holding E
3. The "E" key leaks through to the focused application
4. Result: "eeeeeeee" appears in the text field

**Why it happens:**
- Initially, the daemon doesn't grab the keyboard (to avoid interfering with normal typing)
- When modifiers are released, the system sees plain "E" keypresses
- These go directly to the focused application

## What We Tried

### Approach 1: Immediate Keyboard Grab on Hotkey Press

**Idea:** As soon as the hotkey is detected, grab the keyboard immediately to capture all subsequent key releases.

**Implementation (documented but not committed):**
```rust
// On hotkey press:
device.grab()?;  // Grab immediately
let grabbed_modifiers = snapshot_current_modifiers();  // Save which were pressed
state = Recording { press_time, active, grabbed_modifiers };
```

**Why it failed:**
- When we grab the keyboard, the compositor sees modifier PRESSES but not their RELEASES
- The grab intercepts the release events - they never reach the compositor
- Result: The compositor thinks modifiers are still held down (sticky modifiers)

**Side effect observed:**
- After ungrabbing, subsequent operations fail
- Example: Pasting via Ctrl+V becomes Ctrl+Super+V (compositor still thinks Super is down)
- This opens the clipboard manager instead of pasting

### Approach 2: Synthetic Modifier Releases via ydotool

**Idea:** After ungrabbing, send synthetic key release events for all modifiers through ydotool to clear the compositor's stuck state.

**Implementation attempt:**
```rust
// After ungrabbing:
device.ungrab()?;

// Send synthetic releases for all modifiers that were grabbed
if grabbed_modifiers.ctrl {
    Command::new("ydotool").args(&["key", "29:0"]).output()?;  // LCtrl release
}
if grabbed_modifiers.shift {
    Command::new("ydotool").args(&["key", "42:0"]).output()?;  // LShift release
}
// ... etc for alt, meta
```

**Why it failed:**
- ydotool creates a **separate virtual input device** (`/dev/uinput`)
- Wayland compositors track modifier state **per input device**
- Sending a release from the ydotool device doesn't affect the kanata device's state

**The fundamental issue:** When we:
1. Grab the kanata keyboard device
2. System never sees modifier releases from kanata
3. Send synthetic releases from ydotool device
4. Later paste with ydotool (Ctrl+V)

The compositor sees:
- **kanata device:** Ctrl+Super still DOWN (never saw releases because we grabbed)
- **ydotool device:** Ctrl+V
- **Combined result:** Ctrl+Super+V (compositor merges modifier state from all devices)

### Approach 3: Release Before Grab, Re-grab After

**Idea:** Wait for all modifiers to be released naturally before grabbing, avoiding the stuck-modifier problem entirely.

**Implementation:**
- Transition to `PendingGrab` state when hotkey is released quickly (tap)
- Only grab keyboard once all keys are released
- This ensures the compositor sees all the releases before we take exclusive control

**Why it doesn't solve the original problem:**
- This approach works for tap-to-toggle (long recording mode)
- But doesn't help with the push-to-talk scenario (modifiers released while E is held)
- The leak happens BEFORE we would grab, during the Recording state

### Approach 4: Grab-on-First-Modifier-Release

**Idea (proposed but not implemented):** Detect when the FIRST modifier is released while the hotkey key is still held, and immediately grab at that moment.

**Flow:**
```
T0: Ctrl+Shift+Alt+Super+E pressed → Recording (not grabbed)
T1: User releases Ctrl (first modifier released, E still held)
    → Detect this condition
    → Grab keyboard immediately
    → Capture subsequent Shift/Alt/Super/E releases
    → Send synthetic releases for ALL modifiers via ydotool
```

**Why we didn't try it:**
- Would require tracking individual modifier key states during Recording
- Still faces the per-device modifier tracking problem
- The synthetic releases from ydotool wouldn't clear kanata's modifier state
- Added complexity for uncertain benefit

## The Root Cause

**Wayland's Per-Device Modifier Tracking:**

Wayland compositors (Hyprland, Sway, etc.) maintain modifier state separately for each input device:

```
Input Device State Table:
┌─────────────────┬──────┬───────┬───────┬───────┐
│ Device          │ Ctrl │ Shift │ Alt   │ Super │
├─────────────────┼──────┼───────┼───────┼───────┤
│ /dev/input/...  │ DOWN │ DOWN  │ DOWN  │ DOWN  │  ← Physical keyboard (kanata)
│ /dev/uinput     │ UP   │ UP    │ UP    │ UP    │  ← ydotool virtual device
└─────────────────┴──────┴───────┴───────┴───────┘

When determining final modifier state, compositor ORs all devices:
Final Ctrl = kanata.ctrl OR ydotool.ctrl = DOWN OR UP = DOWN
```

**Why this design exists:**
- Multiple keyboards can be connected simultaneously
- Each may have different modifier states
- Compositor must correctly handle multi-device scenarios (e.g., laptop keyboard + external keyboard)

**Why we can't work around it:**
1. When we grab kanata device, events don't reach compositor
2. Compositor never sees modifier releases from kanata
3. kanata's modifiers stay in DOWN state indefinitely
4. Sending releases from ydotool only affects ydotool's device state
5. kanata's stuck modifiers persist until user physically presses and releases them again

## Key Technical Insights

### evdev Grab Behavior

```rust
device.grab()?;  // EVIOCGRAB ioctl
```

What happens:
- ALL events from this device are routed exclusively to our process
- Events do NOT go to the compositor
- Compositor's view of device state freezes at grab time
- Ungrabbing doesn't restore state - compositor continues from frozen state

### ydotool Architecture

```bash
ydotool key 29:1  # Ctrl press
ydotool key 29:0  # Ctrl release
```

How it works:
- Creates `/dev/uinput` virtual device
- Injects events as if from a real keyboard
- **Operates on a different device than the one we grabbed**
- Cannot affect the grabbed device's compositor state

### Wayland Input Architecture

Unlike X11 (which has a global keyboard state), Wayland tracks:
- Per-device keyboard state
- Per-device seat assignments
- Per-client grab state

This improves security and multi-device handling but makes synthetic event injection harder.

## Potential Future Solutions

### Option 1: Don't Grab the Real Keyboard

Instead of grabbing the kanata device:
1. Configure kanata to emit a unique key code for the hotkey combo (e.g., F13)
2. Monitor for F13 via evdev
3. When detected, start recording WITHOUT grabbing
4. Accept that modifier+E can leak
5. Rely on user training (release all keys simultaneously)

**Pros:** No sticky modifiers, simple implementation
**Cons:** Doesn't fix the original UX problem

### Option 2: Use a Wayland Protocol Extension

Implement or use a Wayland protocol that allows:
- Requesting "logical grab" (grab hotkey combo, not whole device)
- Compositor cooperation for modifier state clearing
- Requires compositor support (Hyprland, Sway, etc. would need to implement)

**Pros:** Proper solution at the protocol level
**Cons:** Requires upstream changes, not available today

### Option 3: Compositor-Specific Workarounds

Hyprland example:
```bash
hyprctl dispatch exec "pkill -USR1 hotkey-daemon"  # Signal to clear modifiers
```

Custom hyprland plugin to:
- Detect when hotkey-daemon grabs/ungrabs
- Automatically clear modifier state for specific devices

**Pros:** Can work today for specific setups
**Cons:** Not portable, requires per-compositor implementation

### Option 4: Full Virtual Keyboard Approach

Radical redesign:
1. Run kanata to intercept the physical keyboard
2. kanata forwards all events to ydotool virtual device (not compositor)
3. Our daemon monitors kanata's output
4. When hotkey detected, kanata stops forwarding temporarily
5. Daemon processes hotkey, then signals kanata to resume

**Pros:** Complete control over event flow
**Cons:** Complex, high latency, kanata already does similar things

### Option 5: Accept the Limitation, Improve UX

Document the behavior and add affordances:
- Show visual feedback when grab happens
- Display "Release all keys" message
- Auto-clear on timeout (if stuck modifiers detected, send synthetic events periodically)
- Add "emergency ungrab" keybinding (e.g., Escape pressed 3 times)

**Pros:** Pragmatic, works with current architecture
**Cons:** UX is still imperfect

## Design Decisions Made

### Why We Chose Not to Implement the Grab-Immediately Approach

Even though documentation was written describing a "SOLVED" implementation with immediate grab and synthetic releases, we did not commit this code because:

1. **It doesn't actually solve the problem** - per-device tracking means ydotool releases won't clear kanata's state
2. **It introduces a worse problem** - sticky modifiers affect all subsequent operations
3. **User testing would reveal the issue** - clipboard manager opening instead of paste
4. **The solution is incomplete** - needs compositor-level cooperation

### Current State Machine Strategy

The existing `PendingGrab` state handles tap-to-toggle correctly:
- Wait for all keys released before grabbing
- Ensures compositor sees all releases
- Only then take exclusive control

This works because:
- User releases hotkey quickly (tap)
- All modifiers are released naturally
- We grab AFTER compositor has updated state
- Future operations are clean

**The unsolved scenario:** Push-to-talk where modifiers are released early.

## Patterns to Remember

### Per-Device State is Real

When working with input devices in Wayland:
- Assume every input device has independent state
- Compositor merges state across devices
- You cannot "inject" state changes for a device you don't control
- Grabbing freezes compositor's view of that device

### Synthetic Events Create New Devices

Tools like ydotool, xdotool, wtype:
- Create virtual `/dev/uinput` devices
- Events appear to come from a different keyboard
- Don't affect the original device's state in the compositor
- Useful for automation, not for state correction

### Testing Modifier State Issues

To reproduce:
```bash
# Terminal 1: Grab keyboard
sudo evtest --grab /dev/input/by-id/usb-...-event-kbd

# Terminal 2: Type with modifiers
# Press Ctrl, then Ctrl+C to close evtest while Ctrl held

# Terminal 3: Try to paste
# Ctrl+V opens clipboard manager instead (Ctrl still "stuck")
```

To clear stuck modifiers manually:
```bash
# Press and release the stuck key
# OR restart compositor
```

### Signal Handling for Emergency Ungrab

Critical for development:
```rust
let running = Arc::new(AtomicBool::new(true));
let r = running.clone();
ctrlc::set_handler(move || {
    device.ungrab().ok();  // ALWAYS ungrab on Ctrl+C
    r.store(false, Ordering::SeqCst);
})?;
```

Without this, killing the daemon leaves keyboard grabbed (system unusable).

## Open Questions

1. **Can we query compositor modifier state?** Is there a Wayland protocol to read current modifier state and detect stuck keys?

2. **Can we ungrab and immediately re-grab to force state update?** Does a grab/ungrab/grab cycle cause compositor to re-sync device state?

3. **Does libinput have helper functions for this?** Maybe there's a library function to "reset device state" we're not aware of.

4. **Can we send events to the original device?** Is it possible to inject events into `/dev/input/eventX` to simulate releases for kanata device specifically?

5. **Multi-seat scenarios?** How does this behave with multiple seats (multiple users on same system)?

## Related Files

- `/home/seb/code/cloned/transcribe-rs-v2/src/bin/hotkey-daemon.rs` - Main daemon (1030 lines)
- `/home/seb/code/cloned/transcribe-rs-v2/lessons-learned/hotkey-daemon-keyboard-issues.md` - Documents both solved (Enter debounce) and unsolved (modifier leak) issues
- `/home/seb/code/cloned/transcribe-rs-v2/lessons-learned/evdev-keyboard-grab.md` - Original evdev grab/ungrab lessons
- `/home/seb/code/cloned/transcribe-rs-v2/prompts/kb-debounce-release.md` - Original problem statement
- `/home/seb/code/cloned/transcribe-rs-v2/docs/testing-plan.md` - Phased testing strategy for refactoring

## Related Lessons Learned

- `evdev-keyboard-grab.md` - Fundamentals of EVIOCGRAB and event handling
- `2025-12-28-progress-bars-and-systemd-wayland.md` - Wayland environment setup for systemd services

## Git Changes

**No code changes were committed** for the modifier leak fix. The documentation in `hotkey-daemon-keyboard-issues.md` was updated to describe a solution marked as "SOLVED", but this was premature - the described implementation with `GrabbedModifiers` and synthetic releases does not actually solve the root cause.

**Uncommitted changes:**
- Modified: `lessons-learned/hotkey-daemon-keyboard-issues.md` (marked solution as SOLVED)
- Added: `prompts/kb-debounce-release.md` (original problem description)
- Added: `docs/testing-plan.md` (future refactoring plan)

## Recommendations

### Immediate Term

1. **Update documentation** to accurately reflect that the modifier leak problem is **not solved**
2. **Revert the "SOLVED" marking** in `hotkey-daemon-keyboard-issues.md`
3. **Document the root cause** (per-device tracking) in the issues file
4. **Add user documentation** explaining the limitation and workaround (release all keys simultaneously)

### Medium Term

1. **Prototype Option 5** (accept limitation, improve UX):
   - Visual feedback showing which keys are held
   - "Release all keys to continue" message
   - Emergency ungrab on repeated Escape

2. **Research compositor protocols**:
   - Check Hyprland/Sway/wlroots documentation for modifier state APIs
   - Reach out to compositor developers for guidance

3. **Experiment with grab/ungrab cycling**:
   - Test if rapid grab/ungrab/grab sequence forces state re-sync

### Long Term

1. **Consider full virtual keyboard approach** (Option 4) if the UX impact is severe
2. **Contribute to Wayland protocols** if needed for proper hotkey daemon support
3. **Upstream kanata integration** - work with kanata to solve this at the layer-remapping level

## For Next Time

When debugging input-related issues:
- Test with `evtest --grab` to understand compositor behavior
- Use `libinput debug-events` to see all input device events
- Check if compositor has IPC for state queries (e.g., `hyprctl devices`)
- Remember: virtual devices (ydotool) are separate from physical devices
- Always add Ctrl+C handler to ungrab before exiting

When documenting solutions:
- Don't mark as "SOLVED" until code is committed and tested
- Distinguish between "approach attempted" and "approach verified working"
- Include reproduction steps for the bug and verification steps for the fix
