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

# Auto-paste if wtype is available
if command -v wtype &> /dev/null; then
    sleep 0.1
    wtype -M ctrl v -m ctrl
    notify-send "✅ Pasted" "$PREVIEW" -t 2000
else
    notify-send "📋 Copied to clipboard" "$PREVIEW\nPress Ctrl+V to paste" -t 3000
fi

echo "Transcription: $RESULT"
