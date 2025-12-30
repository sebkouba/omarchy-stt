# Enter Key Capture During Long Recording - Failed Attempts

**Date:** 2025-12-30
**Status:** Incomplete - needs proper implementation

## Goal

During long recording mode, pressing Enter should:
1. Stop current recording and transcribe
2. Paste the transcription
3. Send Enter key to the app (to submit in chat)
4. Immediately start a new recording

This enables continuous dictation in chat apps - speak, hit Enter to send, keep speaking.

## Attempt 1: Non-blocking evdev polling (FAILED)

**Approach:**
- Use `evdev` crate to monitor keyboard for Enter key
- Set device to non-blocking mode via `fcntl`
- Poll with `fetch_events()` in a loop with sleep

**Code pattern:**
```rust
fn spawn_enter_key_monitor(tx: mpsc::Sender<ShortcutEvent>, running: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        // Set non-blocking via fcntl
        let fd = device.as_raw_fd();
        let flags = fcntl(fd, FcntlArg::F_GETFL)?;
        let new_flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
        fcntl(fd, FcntlArg::F_SETFL(new_flags))?;

        loop {
            match device.fetch_events() {
                Ok(events) => { /* process */ }
                Err(e) if e.kind() == WouldBlock => { /* no events */ }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
}
```

**Why it failed:**
- Polling is inefficient and inelegant
- The Enter key events passed through to the app AND triggered our handler = double Enter
- No mechanism to prevent the physical Enter from reaching the app

## Attempt 2: Blocking evdev in thread (PARTIAL SUCCESS)

**Approach:**
- Let `fetch_events()` block naturally (that's its design)
- Thread sleeps until kernel delivers events - efficient

**Code pattern:**
```rust
fn spawn_enter_key_monitor(tx: mpsc::Sender<ShortcutEvent>, running: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        // blocking - waits for kernel events
        while running.load(Ordering::SeqCst) {
            match device.fetch_events() {
                Ok(events) => {
                    for event in events {
                        if let Key::KEY_ENTER = event.kind() {
                            if event.value() == 1 { // press
                                tx.blocking_send(ShortcutEvent::EnterPressed);
                            }
                        }
                    }
                }
            }
        }
    });
}
```

**Result:**
- Enter key detection worked
- But Enter still passed through to the app = double Enter problem

## Attempt 3: Grab device during long recording (CATASTROPHIC FAILURE)

**Approach:**
- Use `device.grab()` to capture the keyboard exclusively
- Only grab when in LongRecording state, ungrab when exiting
- Physical Enter wouldn't reach apps while grabbed

**Code pattern:**
```rust
let grab_enter = Arc::new(AtomicBool::new(false));

// In evdev thread:
if should_grab && !currently_grabbed {
    device.grab()?;  // Capture device exclusively
    currently_grabbed = true;
}

// When entering LongRecording:
grab_enter.store(true, Ordering::SeqCst);

// When exiting LongRecording:
grab_enter.store(false, Ordering::SeqCst);
```

**CRITICAL FAILURE:**
- `device.grab()` grabs the ENTIRE device, not just Enter key
- When we grabbed kanata (keyboard remapper), ALL keyboard input was captured
- User could not type ANYTHING - not even to exit the mode
- Required hard reboot to recover
- **NEVER grab a keyboard device without re-emitting events**

## Correct Approach (NOT YET IMPLEMENTED)

**Solution: Grab + re-emit via uinput**

1. Grab the keyboard device (captures all events)
2. Create a virtual keyboard via uinput
3. For every event that is NOT Enter: re-emit through virtual keyboard
4. For Enter events: consume (don't re-emit), send to our channel

```rust
// Pseudocode for correct implementation
let virtual_kb = UinputDevice::create_from_device(&device)?;

device.grab()?;

for event in device.fetch_events()? {
    if event.key() == Key::KEY_ENTER && event.value() == 1 {
        // Consume Enter - send to our handler
        tx.send(EnterPressed);
    } else {
        // Re-emit everything else to virtual keyboard
        virtual_kb.emit(&[event])?;
    }
}
```

This way:
- Enter is captured exclusively during long recording
- All other keys pass through normally
- No keyboard lockup

## Alternative Approaches

1. **Use Ctrl+Enter via GlobalShortcuts portal**
   - Register `Ctrl+Enter` as a global shortcut
   - Clean, no evdev grabbing needed
   - Slightly different UX

2. **Accept double-Enter, work around it**
   - Don't grab at all
   - Accept that Enter passes through
   - Maybe send backspace before our Enter?
   - Hacky but simple

## Key Learnings

1. **evdev `device.grab()` is all-or-nothing** - it grabs every event from the device
2. **Blocking evdev reads are correct** - don't try to make it non-blocking with polling
3. **Always test keyboard grabbing with a way to recover** - have SSH ready or a hardware kill switch
4. **uinput re-emission is required** for capturing specific keys while passing others through
