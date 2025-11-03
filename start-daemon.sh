#!/bin/bash
# Start the transcribe daemon

PROJECT_DIR="/home/seb/code/cloned/transcribe-rs"
DAEMON_BIN="$PROJECT_DIR/target/release/transcribe-daemon"
SOCKET_PATH="/tmp/transcribe-rs.sock"

# Check if daemon is already running
if [ -S "$SOCKET_PATH" ]; then
    echo "⚠️  Daemon appears to be already running (socket exists at $SOCKET_PATH)"
    echo "   If it's stuck, remove the socket with: rm $SOCKET_PATH"
    exit 1
fi

# Change to project directory (models are loaded relative to this)
cd "$PROJECT_DIR"

# Check if binary exists
if [ ! -f "$DAEMON_BIN" ]; then
    echo "❌ Daemon binary not found. Building..."
    cargo build --release --bin transcribe-daemon
fi

# Start daemon
echo "🚀 Starting transcribe daemon..."
exec "$DAEMON_BIN"
