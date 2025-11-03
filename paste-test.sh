#!/usr/bin/env bash
# Test pasting text at cursor position using ydotool

TEXT="This is a test transcription result!"

# Set up ydotool socket
export YDOTOOL_SOCKET=/tmp/.ydotool_socket

# Copy to clipboard first
echo -n "$TEXT" | wl-copy

# Small delay to ensure the target window is ready to receive input
sleep 0.2

# Detect if active window is a terminal
WINDOW_CLASS=$(hyprctl activewindow -j | jq -r '.class' | tr '[:upper:]' '[:lower:]')

# Check if it's a terminal (match common terminal emulators)
if [[ "$WINDOW_CLASS" =~ (alacritty|kitty|wezterm|foot|terminal|konsole|gnome-terminal|terminator|xterm|urxvt|st|code) ]]; then
    # Terminal: Use Ctrl+Shift+V
    # 29 = Left Ctrl, 42 = Left Shift, 47 = V
    ydotool key 29:1 42:1 47:1 47:0 42:0 29:0
else
    # Non-terminal: Use Ctrl+V
    # 29 = Left Ctrl, 47 = V
    ydotool key 29:1 47:1 47:0 29:0
fi
