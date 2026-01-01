# egui Indicator Prototype - Lessons Learned

**Date:** 2026-01-01
**Goal:** Replace simple eww progress bar with animated dot matrix (recording) and helix (processing) indicator inspired by ParaDict2

## What We Built

A series of prototype binaries exploring visual indicators:

1. **`indicator-catalogue`** (`src/bin/indicator-catalogue.rs`) - 12 different indicator style explorations
2. **`indicator-catalogue-v2`** (`src/bin/indicator-catalogue-v2.rs`) - 6 variations of matrix + helix combination
3. **`indicator-preview`** (`src/bin/indicator-preview.rs`) - Final design: pill-shaped indicator with:
   - Recording: 5-row dot matrix that fills pill shape, audio-reactive
   - Processing: DNA helix animation constrained to pill boundaries
   - ParaDict2 color scheme (green/blue/yellow/red)
4. **`indicator-daemon`** (`src/bin/indicator-daemon.rs`) - Attempted production replacement for eww

## What Worked

### Visual Design
- **Pill shape** with semicircular ends looks clean and modern
- **Dot matrix** fills the pill shape naturally - center row has more dots, outer rows fewer
- **Helix animation** constrained to pill boundaries creates pleasing effect
- **ParaDict2 color scheme**: Green (recording), Blue (transcribing), Yellow (enhancing), Red (error)
- **200x40px dimensions** match existing eww widget footprint

### Technical Implementation
- **egui/eframe** works well for canvas-based animations
- **File-based state** (`/tmp/ptt_indicator_state`) is simple IPC
- **Audio level from file** (`/tmp/ptt_audio_level`) reuses existing recording-daemon output

### Key Parameters (indicator-preview.rs)
```rust
const INDICATOR_WIDTH: f32 = 200.0;
const INDICATOR_HEIGHT: f32 = 40.0;

// Dot matrix
let dot_radius = 2.8;
let dot_spacing = 7.5;
const ROWS: usize = 5;

// Helix
const NUM_POINTS: usize = 20;
```

## What Didn't Work

### Critical Miscommunication
- **User expected eww enhancement**, I built egui replacement
- Should have clarified upfront: "eww cannot do these animations, we need a different technology"
- Changed architecture without explicit agreement

### egui Window Management on Wayland
- `ViewportCommand::Visible(false)` didn't properly hide window
- Window still showed as grey box even when "hidden"
- Transparent background didn't work as expected
- Window positioning hardcoded, not dynamic to screen size

### Missing Features for Production
- No proper Wayland layer-shell support (window isn't a true overlay)
- No way to position at bottom-center of active monitor
- Mouse passthrough may not work
- Window decorations showing despite `with_decorations(false)`

## Files Created (Kept but Disabled)

```
src/bin/indicator-catalogue.rs    # Style exploration
src/bin/indicator-catalogue-v2.rs # Matrix+helix variations
src/bin/indicator-preview.rs      # Final design preview
src/bin/indicator-daemon.rs       # Failed production attempt
```

These are still in `Cargo.toml` but NOT copied to builds/staging by `scripts/build.sh`.

## Reverted Files

- `src/eww_widget.rs` - Restored to original eww-based implementation

## Next Steps to Complete This Feature

### 1. Proper Wayland Overlay Support
The indicator needs to be a proper Wayland layer-shell surface. Options:
- **gtk4-layer-shell** with gtk4-rs - Most reliable for Wayland overlays
- **smithay-client-toolkit** - Lower level but full control
- **iced** with layer-shell - Alternative to egui

### 2. Fix Window Visibility
Need actual show/hide, not just transparency tricks:
```rust
// Current (broken)
ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));

// Needed: Actually destroy/recreate window, or use proper layer-shell visibility
```

### 3. Dynamic Positioning
Calculate screen dimensions and position at bottom center:
```rust
// Need to query active monitor dimensions
let screen_width = get_active_monitor_width();
let x = (screen_width - INDICATOR_WIDTH) / 2.0;
let y = screen_height - INDICATOR_HEIGHT - 50.0; // 50px from bottom
```

### 4. Integration Testing
Once overlay works:
1. Test with hotkey-daemon state transitions
2. Verify audio-reactive matrix responds to actual voice input
3. Test all state transitions: idle → recording → transcribing → enhancing → idle

### 5. Systemd Service
Create `indicator-daemon.service` similar to other daemons.

## Alternative: Enhance eww Instead

If full replacement is too complex, consider:
- Simpler eww animation (pulsing bar, color changes)
- Multiple eww boxes to simulate dot pattern (static, not animated)
- Accept eww limitations and keep simple progress bar

## Running the Prototypes

```bash
# Preview the final design (standalone, not integrated)
cargo run --bin indicator-preview

# Explore all indicator styles
cargo run --bin indicator-catalogue

# Test matrix+helix variations
cargo run --bin indicator-catalogue-v2
```

## Key Takeaway

**Always clarify technology changes before implementing.** The visual design is solid, but replacing eww with egui is a significant architecture change that needs:
1. Explicit user agreement
2. Proper Wayland integration (layer-shell)
3. Thorough testing before deployment
