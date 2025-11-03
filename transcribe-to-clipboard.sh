#!/bin/bash
# Transcribe audio file and copy to clipboard

AUDIO_FILE="${1:-/tmp/ptt_current.wav}"
PROJECT_DIR="/home/seb/code/cloned/transcribe-rs"

# Check if audio file exists
if [ ! -f "$AUDIO_FILE" ]; then
    notify-send "❌ Error" "No recording found" -t 2000
    exit 1
fi

# Transcribe using the daemon client (much faster!)
RESULT=$("$PROJECT_DIR/target/release/transcribe-client" "$AUDIO_FILE" 2>/dev/null)

# Check if transcription is empty
if [ -z "$RESULT" ]; then
    notify-send "❌ No speech detected" "Try speaking louder or closer to mic" -t 3000
    exit 1
fi

# Copy to clipboard
echo -n "$RESULT" | wl-copy

# Show notification with preview
PREVIEW=$(echo "$RESULT" | head -c 100)
if [ ${#RESULT} -gt 100 ]; then
    PREVIEW="${PREVIEW}..."
fi

# Auto-paste using ydotool
if command -v ydotool &> /dev/null; then
    # Set up ydotool socket
    export YDOTOOL_SOCKET=/tmp/.ydotool_socket

    # Wait for window to be ready and clipboard to be populated
    # sleep 0.2

    # Detect if active window is a terminal
    WINDOW_CLASS=$(hyprctl activewindow -j 2>/dev/null | jq -r '.class' 2>/dev/null | tr '[:upper:]' '[:lower:]')

    # Check if it's a terminal (match common terminal emulators)
    if [[ "$WINDOW_CLASS" =~ (alacritty|kitty|wezterm|foot|terminal|konsole|code|terminator|xterm|urxvt|st) ]]; then
        # Terminal: Use Ctrl+Shift+V
        # 29 = Left Ctrl, 42 = Left Shift, 47 = V
        ydotool key 29:1 42:1 47:1 47:0 42:0 29:0
    else
        # Non-terminal: Use Ctrl+V
        # 29 = Left Ctrl, 47 = V
        ydotool key 29:1 47:1 47:0 29:0
    fi

    notify-send "✅ Pasted" "$PREVIEW" -t 2000
else
    notify-send "📋 Copied to clipboard" "$PREVIEW\nPress Ctrl+V to paste" -t 3000
fi

echo "Transcription: $RESULT"
