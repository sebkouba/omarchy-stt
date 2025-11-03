#!/bin/bash
# Transcribe audio file and copy to clipboard

AUDIO_FILE="${1:-/tmp/ptt_current.wav}"
PROJECT_DIR="/home/seb/code/cloned/transcribe-rs"

# Check if audio file exists
if [ ! -f "$AUDIO_FILE" ]; then
    notify-send "❌ Error" "No recording found" -t 2000
    exit 1
fi

# Transcribe (output only the text)
cd "$PROJECT_DIR"
RESULT=$(cargo run --example transcribe-file --release "$AUDIO_FILE" 2>/dev/null)

# Check if transcription is empty
if [ -z "$RESULT" ]; then
    notify-send "❌ No speech detected" "Try speaking louder or closer to mic" -t 3000
    exit 1
fi

# Copy to clipboard
echo "$RESULT" | wl-copy

# Show notification with preview
PREVIEW=$(echo "$RESULT" | head -c 100)
if [ ${#RESULT} -gt 100 ]; then
    PREVIEW="${PREVIEW}..."
fi

# Auto-paste with application-specific handling
if command -v wtype &> /dev/null; then
    # Give time for focus to return to original window
    sleep 0.1

    # Get active window class
    ACTIVE_WINDOW=""
    if command -v hyprctl &> /dev/null; then
        ACTIVE_WINDOW=$(hyprctl activewindow -j 2>/dev/null | grep -oP '"class":\s*"\K[^"]+' 2>/dev/null)
    fi

    # Handle different applications
    case "$ACTIVE_WINDOW" in
        *kitty*|*alacritty*|*foot*|*wezterm*|*konsole*|*terminator*|*gnome-terminal*|*xterm*)
            # Terminal: Use Ctrl+Shift+V
            wtype -M ctrl -M shift V -m shift -m ctrl
            ;;
        *code*|*Code*|*VSCodium*|*codium*)
            # VS Code: Type directly to avoid keybinding conflicts
            wtype "$RESULT"
            ;;
        *)
            # Everything else: Standard Ctrl+V
            wtype -M ctrl V -m ctrl
            ;;
    esac

    # Show notification AFTER paste (so it doesn't steal focus)
    sleep 0.1
    notify-send "✅ Pasted" "$PREVIEW" -t 2000
else
    notify-send "📋 Copied to clipboard" "$PREVIEW\nPress Ctrl+V to paste" -t 3000
fi

echo "Transcription: $RESULT"
