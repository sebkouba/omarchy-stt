#!/bin/bash
# Transcribe audio file and copy to clipboard

AUDIO_FILE="${1:-/tmp/ptt_current.wav}"
PROJECT_DIR="/home/seb/code/cloned/transcribe-rs"
LOG_FILE="/tmp/ptt_debug.log"

# Logging function with timestamp
log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S.%3N')] [transcribe-clipboard] $*" >> "$LOG_FILE"
}

log "=== TRANSCRIPTION START ==="
log "Audio file: $AUDIO_FILE"

# Check if audio file exists
if [ ! -f "$AUDIO_FILE" ]; then
    log "ERROR: Audio file not found: $AUDIO_FILE"
    notify-send "❌ Error" "No recording found" -t 2000
    exit 1
fi

FILE_SIZE=$(stat -f%z "$AUDIO_FILE" 2>/dev/null || stat -c%s "$AUDIO_FILE" 2>/dev/null)
log "Audio file size: $FILE_SIZE bytes"

# Transcribe using the daemon client (much faster!)
log "Starting transcription with daemon client..."
TRANSCRIBE_START=$(date +%s%3N)
RESULT=$("$PROJECT_DIR/target/release/transcribe-client" "$AUDIO_FILE" 2>&1)
CLIENT_EXIT=$?
TRANSCRIBE_END=$(date +%s%3N)
TRANSCRIBE_TIME=$((TRANSCRIBE_END - TRANSCRIBE_START))

log "Transcription completed in ${TRANSCRIBE_TIME}ms (exit: $CLIENT_EXIT)"
log "Result length: ${#RESULT} chars"

# Check if transcription is empty
if [ -z "$RESULT" ]; then
    log "ERROR: Empty transcription result"
    notify-send "❌ No speech detected" "Try speaking louder or closer to mic" -t 3000
    exit 1
fi

log "Transcription text: '$RESULT'"

# Copy to clipboard
log "Copying to clipboard..."
echo -n "$RESULT" | wl-copy
CLIPBOARD_EXIT=$?
log "Clipboard copy exit status: $CLIPBOARD_EXIT"

# Show notification with preview
PREVIEW=$(echo "$RESULT" | head -c 100)
if [ ${#RESULT} -gt 100 ]; then
    PREVIEW="${PREVIEW}..."
fi

# Auto-paste using ydotool
if command -v ydotool &> /dev/null; then
    log "ydotool found, attempting auto-paste"

    # Set up ydotool socket
    export YDOTOOL_SOCKET=/tmp/.ydotool_socket

    # CRITICAL: Give clipboard time to propagate through Wayland compositor
    # Without this, paste happens before clipboard is ready
    sleep 0.05
    log "Clipboard propagation delay complete"

    # Detect if active window is a terminal
    WINDOW_CLASS=$(hyprctl activewindow -j 2>/dev/null | jq -r '.class' 2>/dev/null | tr '[:upper:]' '[:lower:]')
    log "Active window class: '$WINDOW_CLASS'"

    # Check if it's a terminal (match common terminal emulators)
    if [[ "$WINDOW_CLASS" =~ (alacritty|kitty|wezterm|foot|terminal|konsole|code|terminator|xterm|urxvt|st) ]]; then
        log "Terminal detected, using Ctrl+Shift+V"
        # Terminal: Use Ctrl+Shift+V
        # 29 = Left Ctrl, 42 = Left Shift, 47 = V
        ydotool key 29:1 42:1 47:1 47:0 42:0 29:0
        PASTE_EXIT=$?
    else
        log "Non-terminal window, using Ctrl+V"
        # Non-terminal: Use Ctrl+V
        # 29 = Left Ctrl, 47 = V
        ydotool key 29:1 47:1 47:0 29:0
        PASTE_EXIT=$?
    fi

    log "ydotool paste exit status: $PASTE_EXIT"
    notify-send "✅ Pasted" "$PREVIEW" -t 2000
else
    log "ydotool not found, clipboard only"
    notify-send "📋 Copied to clipboard" "$PREVIEW\nPress Ctrl+V to paste" -t 3000
fi

log "=== TRANSCRIPTION COMPLETE ==="
echo "Transcription: $RESULT"
