# Progress Bar System and Systemd Wayland Environment

## Context

Built a multi-stage visual feedback system for the dictation app using eww widgets, showing separate progress bars for different phases (recording, transcription, API calls). Also debugged why the app stopped working after machine restart - the systemd service was missing Wayland environment variables needed for eww to connect to GTK.

## What Worked

### 1. Systemd Wayland Environment Fix

**Problem:** After machine restart, hotkey-daemon wouldn't show eww widgets (recording/progress bars). No errors in logs, but widgets simply didn't appear.

**Root Cause:** Systemd user services don't inherit Wayland session environment variables by default. The eww commands were running but couldn't connect to the GTK display server.

**Solution:** Add environment variables to the systemd service file:

```ini
# In ~/.config/systemd/user/hotkey-daemon.service
[Service]
# Wayland/GTK environment for eww speech indicator
Environment=WAYLAND_DISPLAY=wayland-1
Environment=XDG_RUNTIME_DIR=/run/user/1000
Environment=DISPLAY=:1
```

**Key Insight:** Even though the service runs in the user session (not system-wide), it still needs explicit environment variables for GUI tools. This applies to any systemd service that needs to interact with Wayland/X11 displays.

**Testing:** After adding the variables, run `systemctl --user daemon-reload && systemctl --user restart hotkey-daemon`

### 2. Three-Stage Progress Bar System

Built a color-coded visual feedback system with three distinct widgets:

- **Green bar:** Voice recording (real-time audio levels)
- **Blue bar:** Local transcription (Parakeet)
- **Yellow bar:** API requests (Groq LLM)

Each bar is a separate eww window shown/hidden at the appropriate time, preventing conflicts and providing clear visual distinction.

### 3. Historical Timing-Based Progress Estimation

**Architecture:**
- Two separate timing log files: `/tmp/ptt_transcription_timing.log` and `/tmp/ptt_api_timing.log`
- Each logs last 100 operations as CSV: `input_metric,duration_ms`
- Transcription: metric is recording duration (longer recordings take longer to transcribe)
- API: metric is text length (more text takes longer to process)

**Estimation Algorithm:**

```rust
// Transcription: ratio-based (transcription_time / recording_time)
let total_recording: u64 = entries.iter().map(|e| e.recording_ms).sum();
let total_transcription: u64 = entries.iter().map(|e| e.transcription_ms).sum();
let ratio = total_transcription as f64 / total_recording as f64;
let estimated = (recording_ms as f64 * ratio * 1.2) as u64; // 20% buffer

// API: per-character rate
let ms_per_char = total_ms as f64 / total_chars as f64;
let estimated = (text_length as f64 * ms_per_char * 1.2) as u64; // 20% buffer
```

**Why it works:**
- Parakeet transcription time scales linearly with audio duration
- API response time scales roughly linearly with token count (approximated by char count)
- 20% buffer prevents bar from finishing before operation completes
- Clamping (200ms-10s for transcription, 300ms-15s for API) handles edge cases

### 4. Background Thread Animation Pattern

Each progress bar runs in a separate background thread that:
1. Shows the widget
2. Updates progress via file writes (`/tmp/ptt_transcription_progress` or `/tmp/ptt_api_progress`)
3. Polls file at 30ms intervals via eww defpoll
4. Uses atomic flag for cancellation
5. Hides widget when done

```rust
fn start_loading_progress_animation(estimated_ms: u64, stop_flag: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        eww_widget::show_loading_widget();

        loop {
            if stop_flag.load(Ordering::Relaxed) { break; }

            let progress = calculate_progress(elapsed, estimated_ms);
            eww_widget::set_loading_progress(progress);

            std::thread::sleep(Duration::from_millis(30));
        }

        eww_widget::set_loading_progress(100);
        eww_widget::hide_loading_widget();
    });
}
```

**Key detail:** Stop flag is checked every iteration, allowing instant cancellation on completion.

### 5. Pulse Fallback for No History

When no historical data exists, use a pulsing animation instead of progress:

```rust
// Pulse mode: 0→100→0 over 1000ms
let pulse_ms = 500;
let cycle_pos = elapsed.as_millis() % (pulse_ms * 2);
let progress = if cycle_pos < pulse_ms {
    (cycle_pos as f64 / pulse_ms as f64 * 100.0) as u8
} else {
    (100.0 - ((cycle_pos - pulse_ms as f64) / pulse_ms as f64 * 100.0)) as u8
};
```

After reaching estimated duration, switch to pulse at 80-100% to indicate operation is still running but taking longer than expected.

### 6. RecordingResult Structure

Changed `stop_recording()` to return a struct with timing data instead of just the transcription text:

```rust
pub struct RecordingResult {
    pub text: String,
    pub duration_ms: u64,
}
```

This allows the progress bar system to use actual recording duration for estimation without separate timing logic in the daemon.

## What Didn't Work / Gotchas

### 1. File-Based IPC vs Direct Widget Control

**Tried:** Initially considered using `eww update` commands to set progress directly.

**Problem:** `eww update` requires defining variables in the config, and polling is more natural for continuous updates.

**Chose:** File-based polling. Simple, reliable, and eww is already designed for this pattern. 30ms polling interval is imperceptible and low overhead.

### 2. Single Progress Variable for Multiple Widgets

**Tried:** Reusing the same progress file for both transcription and API widgets.

**Problem:** Widgets overlap in time if transcription finishes and immediately triggers API call. Wrong widget shows wrong data.

**Fixed:** Separate files (`/tmp/ptt_transcription_progress` and `/tmp/ptt_api_progress`) for each widget type.

### 3. Git Repository Service File vs Deployed Service File

**Issue:** The repository version of `systemd/hotkey-daemon.service` doesn't include Wayland environment variables, but the deployed version in `~/.config/systemd/user/` does.

**Why:** Environment variables like `XDG_RUNTIME_DIR=/run/user/1000` are system-specific. User ID 1000 may differ on other machines.

**Solution:** Keep the repository version generic and add a note in CLAUDE.md or installation docs about adding environment variables during deployment.

### 4. Progress Animation Timing Edge Cases

**Issue:** If operation completes in <100ms, the progress bar may show and hide so fast it's jarring.

**Mitigation:** Minimum duration clamps (200ms for transcription, 300ms for API) ensure the bar is visible long enough to be meaningful.

**Better approach:** Could add a minimum display time (e.g., keep widget visible for at least 300ms even if operation finishes faster).

## Key Decisions

### Separate Widgets Instead of Mode Switching

Could have built one widget that changes color/mode based on the current operation. Chose separate widgets because:
- Cleaner code separation (each widget is independent)
- No race conditions when switching between modes
- Easier to debug (can see which widget file exists in `/tmp/`)
- Future flexibility (could show multiple at once if needed)

### File Polling vs Socket/IPC

For progress updates, chose file polling over more complex IPC:
- **Pros:** Simple, no protocol needed, easy to debug (`cat /tmp/ptt_transcription_progress`)
- **Cons:** Slightly higher overhead (stat calls every 30ms)
- **Verdict:** Simplicity wins for this use case. 30ms polling is negligible CPU usage.

### Timer Calibration Logs in /tmp/

Keep timing logs in `/tmp/` (ephemeral) rather than `~/.config/`:
- Progress bars are nice-to-have UX, not critical functionality
- If logs get corrupted, they just disappear on reboot and rebuild
- Avoids polluting user config directory with machine-specific performance data
- Last 100 entries are sufficient for good estimation

## Patterns to Remember

### Systemd User Service Environment Variables

When systemd user services need GUI access:
```ini
[Service]
Environment=WAYLAND_DISPLAY=wayland-1
Environment=XDG_RUNTIME_DIR=/run/user/1000
Environment=DISPLAY=:1
```

Check your actual values:
```bash
echo $WAYLAND_DISPLAY  # usually wayland-0 or wayland-1
echo $XDG_RUNTIME_DIR  # /run/user/$(id -u)
echo $DISPLAY          # usually :0 or :1
```

### Progress Estimation Pattern

For operations with predictable scaling:
1. Log `(input_metric, duration)` pairs
2. Calculate ratio or per-unit cost from historical data
3. Add 20% buffer to avoid finishing early
4. Clamp to reasonable min/max bounds
5. Fall back to pulse animation when no history

### Background Progress Animation

```rust
let stop_flag = Arc::new(AtomicBool::new(false));
let flag_clone = stop_flag.clone();

// Start animation thread
start_animation(estimated_ms, flag_clone);

// ... do work ...

// Stop animation when done
stop_flag.store(true, Ordering::Relaxed);
```

## Open Questions

1. **Cross-machine portability:** Should we auto-detect Wayland environment variables at runtime and write them to the service file during installation?

2. **Progress accuracy:** Could we improve estimation by using weighted averages (recent entries weighted more heavily)?

3. **Minimum display time:** Should progress bars enforce a minimum visible duration to prevent jarring flashes?

4. **Multiple simultaneous operations:** What if user triggers new recording while API call is still running? Currently undefined behavior.

## Related Files

### Modified in this session:
- `/home/seb/.config/systemd/user/hotkey-daemon.service` - Added Wayland environment variables
- `/home/seb/code/cloned/transcribe-rs-v2/eww/eww.yuck` - Added loading and api widgets
- `/home/seb/code/cloned/transcribe-rs-v2/eww/eww.scss` - Added `.loading-bar` and `.api-bar` styles
- `/home/seb/code/cloned/transcribe-rs-v2/src/eww_widget.rs` - Added show/hide/set_progress for loading and API widgets
- `/home/seb/code/cloned/transcribe-rs-v2/src/transcription_timing.rs` - New module for timing estimation
- `/home/seb/code/cloned/transcribe-rs-v2/src/recording.rs` - Changed return type to RecordingResult
- `/home/seb/code/cloned/transcribe-rs-v2/src/bin/hotkey-daemon.rs` - Wired in progress animations
- `/home/seb/code/cloned/transcribe-rs-v2/src/bin/cli.rs` - Updated to use RecordingResult

### Related lessons:
- `lessons-learned/eww-recording-widget.md` - Original green bar implementation
- `lessons-learned/evdev-keyboard-grab.md` - Hotkey daemon fundamentals

## Git Changes Summary

Two main commits:
1. `bee7c53` - "transcription progress" - Added blue progress bar for local Parakeet transcription with timing estimation
2. `b6933ad` - "Add yellow API progress bar for LLM requests" - Added yellow bar for Groq API calls with separate timing calibration

Both commits added ~300 lines of new code, primarily in `transcription_timing.rs` (new module) and progress animation logic in `hotkey-daemon.rs`.

## For Next Time

1. **Document Wayland environment setup** in installation instructions or README
2. **Consider auto-detection script** that reads current env vars and updates service file
3. **Add integration test** for progress estimation (mock timing logs, verify estimation accuracy)
4. **Profile polling overhead** - is 30ms too aggressive? Could we use 50ms or 100ms?
5. **Consider minimum display time** for progress bars to prevent flashing
