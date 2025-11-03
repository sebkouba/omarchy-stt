#!/bin/bash
# Simplest approach - clipboard only

TEXT="This is a test transcription result!"

# Copy to clipboard
echo "$TEXT" | wl-copy

# Show notification
notify-send "📋 Copied to clipboard" "$TEXT" -t 2000

echo "Text copied to clipboard - press Ctrl+V to paste"
