# Resilient PTT Keybinding Implementation

## Problem
The current PTT (push-to-talk) keybinding uses `bind` for start and `bindr` for stop:
```ini
bind = SUPER SHIFT CTRL ALT, Q, exec, /path/to/omarchy-stt/ptt-test.sh start
bindr = SUPER SHIFT CTRL ALT, Q, exec, /path/to/omarchy-stt/ptt-test.sh stop
```

This fails when modifiers are released before the Q key - the stop command doesn't trigger.

## Solution
Use the `i` flag (ignore mods) on the release bind to make it trigger regardless of modifier state.

## Implementation

### 1. Update Hyprland bindings
Edit `~/.config/hypr/bindings.conf`:

```ini
# Start recording when SUPER+SHIFT+CTRL+ALT+Q is pressed
bind = SUPER SHIFT CTRL ALT, Q, exec, /path/to/omarchy-stt/ptt-test.sh start

# Stop recording when Q is released (regardless of modifiers)
bindri = , Q, exec, /path/to/omarchy-stt/ptt-test.sh stop
```

### 2. Make the script idempotent
The script needs to handle being called multiple times (since Q release happens during normal typing too).

Update `ptt-test.sh` to check recording state:

```bash
#!/bin/bash

RECORDING_STATE="/tmp/transcribe-recording.pid"

case "$1" in
    start)
        # Only start if not already recording
        if [ ! -f "$RECORDING_STATE" ]; then
            # Start your recording process here
            # ... existing start logic ...

            # Mark as recording
            echo $$ > "$RECORDING_STATE"
        fi
        ;;
    stop)
        # Only stop if actually recording
        if [ -f "$RECORDING_STATE" ]; then
            # Stop your recording process here
            # ... existing stop logic ...

            # Remove state file
            rm "$RECORDING_STATE"
        fi
        # If not recording, do nothing (this is fine!)
        ;;
esac
```

### 3. Reload Hyprland config
```bash
hyprctl reload
```

## How It Works
- Pressing SUPER+SHIFT+CTRL+ALT+Q triggers `start`
- Releasing Q (with or without modifiers still held) triggers `stop`
- Typing 'q' normally also triggers `stop`, but the script checks the state file and does nothing if not recording
- No performance impact - file existence checks are microsecond-level operations

## Performance Notes
- File existence check (`[ -f "$RECORDING_STATE" ]`) is extremely fast
- Q isn't typed frequently enough to cause any noticeable overhead
- Shell invocation overhead is negligible for such a simple script
- This pattern is common and well-tested for toggle/PTT bindings

## Alternative Approaches (not recommended)
If you want to avoid the Q-release-during-typing issue entirely:
- Use a less common key like `;`, `'`, or `[`
- Use function keys F13-F24 (if keyboard supports)
- Use a mouse button (loses left-hand-only operation)

The current solution is preferred for ergonomics and reliability.
