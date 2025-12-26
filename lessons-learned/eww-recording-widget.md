# eww Recording Widget Implementation

**Date:** 2025-12-26
**Feature:** Real-time audio level indicator during push-to-talk recording

## What We Built

A visual recording indicator using eww (ElKowar's Wacky Widgets) that:
- Appears at bottom center of screen when recording starts
- Shows a green bar that grows/shrinks based on voice amplitude
- Disappears when recording stops

## What Worked

1. **Architecture choice (eww)** - Good fit for Hyprland/Wayland. Lightweight, designed for this exact use case.

2. **Audio level via file polling** - Simple approach: recording-daemon writes level (0-100) to `/tmp/ptt_audio_level`, eww polls every 50ms. No complex IPC needed.

3. **RMS calculation** - Simple and effective for voice level visualization:
   ```rust
   let sum_squares: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
   let rms = (sum_squares / samples.len() as f64).sqrt();
   (rms / 32767.0) as f32  // normalize to 0-1
   ```

4. **AtomicBool for recording state** - Clean way to communicate between socket handler thread and audio reader thread without complex locking.

## What Didn't Work / Gotchas

### eww SCSS is NOT standard CSS

1. **No `@keyframes`** - Animation syntax not supported. Parser chokes on it.
   ```scss
   // BROKEN
   @keyframes pulse {
     0%, 100% { opacity: 1; }
     50% { opacity: 0.5; }
   }
   ```

2. **No `max-width`** - Not a valid GTK CSS property.

3. **No `transition`** - CSS transitions not supported.

4. **No `box-shadow`** - Silently ignored or errors.

**Lesson:** eww uses GTK CSS, not web CSS. Test properties incrementally. When in doubt, use only: `background-color`, `border-radius`, `padding`, `margin`, `min-width`, `min-height`, `color`, `font-size`.

### Empty string handling in yuck

The defpoll can return empty string if file doesn't exist, which breaks arithmetic:
```yuck
; BROKEN - audio_level might be ""
:style "min-width: ${audio_level * 2}px;"

; FIXED - filter with grep, fallback to 0
(defpoll audio_level :interval "50ms"
  `cat /tmp/ptt_audio_level 2>/dev/null | grep -E '^[0-9]+$' || echo 0`)
```

### Debug workflow

```bash
# See errors immediately
eww kill --config /path/to/eww
eww open recording --config /path/to/eww

# After config changes
eww reload --config /path/to/eww

# Check current state
eww debug --config /path/to/eww
```

## File Locations

- Widget config: `eww/eww.yuck`
- Styles: `eww/eww.scss`
- Audio level file: `/tmp/ptt_audio_level` (written by recording-daemon)
- eww daemon socket: `/run/user/1000/eww-server_*`

## CLI Integration

```rust
// In handle_start()
eww_widget::show_recording_widget();

// In handle_stop() - call early for instant feedback
eww_widget::hide_recording_widget();
```

The eww commands are spawned (not waited on) so they don't block the main flow.

## For Next Time

1. **Start minimal** - Begin with simplest possible widget (one box, one color), then add complexity.

2. **Check eww logs immediately** - `eww open` output shows SCSS/yuck parse errors.

3. **GTK CSS reference** - When adding styles, check GTK CSS docs, not web CSS.

4. **Polling interval** - 50ms feels responsive. Could go to 100ms to reduce load if needed.

5. **Consider alternatives** - If eww becomes too limiting, GTK Layer Shell with gtk-rs gives full control but more code.
