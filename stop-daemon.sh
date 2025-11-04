#!/bin/bash
# Stop the transcribe v2 daemon (only stops the v2 version, not the original)

PROJECT_DIR="/home/seb/code/cloned/transcribe-rs-v2"
SOCKET_PATH="/tmp/transcribe-rs-v2.sock"
BINARY_PATH="$PROJECT_DIR/target/release/transcribe-daemon"

echo "🛑 Stopping transcribe v2 daemon..."

# Method 1: Find PID via socket path (most reliable)
PID=""
if command -v lsof &> /dev/null; then
    PID=$(lsof -U 2>/dev/null | grep "$SOCKET_PATH" | awk '{print $2}' | head -1)
fi

# Method 2: Fallback to binary path search if lsof didn't work
if [ -z "$PID" ]; then
    PID=$(ps aux | grep "$BINARY_PATH" | grep -v grep | awk '{print $2}' | head -1)
fi

# Check if daemon was found
if [ -z "$PID" ]; then
    echo "✓ No v2 daemon found running"

    # Clean up stale socket if it exists
    if [ -S "$SOCKET_PATH" ]; then
        echo "🧹 Removing stale socket: $SOCKET_PATH"
        rm "$SOCKET_PATH"
    fi
    exit 0
fi

# Validate this is the correct process (safety check)
PROC_PATH=$(readlink -f /proc/$PID/exe 2>/dev/null)
if [ -n "$PROC_PATH" ] && [[ "$PROC_PATH" != *"transcribe-rs-v2"* ]]; then
    echo "⚠️  Warning: Found PID $PID but binary path doesn't match v2"
    echo "   Binary: $PROC_PATH"
    echo "   Expected path to contain: transcribe-rs-v2"
    echo "   Aborting for safety (won't kill potentially wrong process)"
    exit 1
fi

# Kill the daemon with SIGTERM (graceful shutdown)
echo "📍 Found v2 daemon (PID: $PID)"
echo "   Sending SIGTERM for graceful shutdown..."
kill "$PID" 2>/dev/null

# Wait for graceful shutdown (up to 3 seconds)
for i in {1..6}; do
    if ! ps -p "$PID" > /dev/null 2>&1; then
        echo "✓ Daemon stopped gracefully"
        break
    fi
    sleep 0.5
done

# Force kill if still running
if ps -p "$PID" > /dev/null 2>&1; then
    echo "⚠️  Daemon still running, force killing (SIGKILL)..."
    kill -9 "$PID" 2>/dev/null
    sleep 0.5

    if ps -p "$PID" > /dev/null 2>&1; then
        echo "❌ Failed to kill daemon (PID: $PID)"
        exit 1
    fi
    echo "✓ Daemon force killed"
fi

# Clean up socket file
if [ -S "$SOCKET_PATH" ]; then
    echo "🧹 Cleaning up socket: $SOCKET_PATH"
    rm "$SOCKET_PATH"
fi

echo "✅ V2 daemon stopped successfully"
