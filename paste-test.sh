#!/bin/bash
# Test pasting text at cursor position

TEXT="This is a test transcription result!"

# Copy to clipboard
echo "$TEXT" | wl-copy

# Small delay to ensure clipboard is ready
sleep 0.1

# Simulate Ctrl+V to paste
ydotool key 29:1 47:1 47:0 29:0

echo "Pasted: $TEXT"
