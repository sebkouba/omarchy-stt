#!/usr/bin/env bash
# Test typing text at cursor position using ydotool

TEXT="This is a test transcription result!"

# Set up ydotool socket
export YDOTOOL_SOCKET=/tmp/.ydotool_socket

# Small delay to ensure the target window is ready to receive input
sleep 0.2

# Type the text with minimal delay (2ms between keys instead of default 20ms)
ydotool type --key-delay=1 "$TEXT"
