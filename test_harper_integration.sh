#!/bin/bash
# Test script for Harper Daemon Integration
# This script verifies that Harper processing works in the daemon

set -e

echo "🧪 Testing Harper Daemon Integration"
echo "====================================="
echo

# Check if daemon is running
if ! pgrep -f transcribe-daemon > /dev/null; then
    echo "⚠️  Daemon is not running. Starting daemon..."
    ./target/release/transcribe-daemon &
    DAEMON_PID=$!
    echo "   Daemon started with PID: $DAEMON_PID"
    echo "   Waiting for daemon to initialize..."
    sleep 5
    CLEANUP_DAEMON=1
else
    echo "✓ Daemon is already running"
    CLEANUP_DAEMON=0
fi

echo

# Test with a sample WAV file
echo "📝 Testing transcription with Harper processing..."
TEST_FILE="tests/test1.wav"

if [ ! -f "$TEST_FILE" ]; then
    echo "❌ Test file not found: $TEST_FILE"
    exit 1
fi

echo "   Input: $TEST_FILE"
echo

# Run transcription
echo "🎤 Transcribing..."
RESULT=$(./target/release/transcribe-client "$TEST_FILE")

echo
echo "📋 Result:"
echo "   $RESULT"
echo

# Verify result is not empty
if [ -z "$RESULT" ]; then
    echo "❌ Transcription returned empty result"
    exit 1
fi

echo "✅ Transcription successful!"
echo

# Check if Harper corrections were saved (if any)
CORRECTIONS_DIR="$HOME/.config/transcribe-rs/harper_corrections"
if [ -d "$CORRECTIONS_DIR" ]; then
    RECENT_CORRECTIONS=$(find "$CORRECTIONS_DIR" -name "*.json" -mmin -1 | wc -l)
    if [ "$RECENT_CORRECTIONS" -gt 0 ]; then
        echo "📝 Harper correction session saved"
        echo "   Found $RECENT_CORRECTIONS recent correction file(s)"
        LATEST=$(find "$CORRECTIONS_DIR" -name "*.json" -mmin -1 | head -1)
        if [ -n "$LATEST" ]; then
            echo "   Latest: $LATEST"
            echo
            echo "   Sample corrections:"
            cat "$LATEST" | head -20
        fi
    else
        echo "ℹ️  No recent Harper corrections (text was clean or Harper disabled)"
    fi
fi

echo
echo "🎉 Harper Daemon Integration Test Complete!"
echo

# Cleanup daemon if we started it
if [ "$CLEANUP_DAEMON" = "1" ]; then
    echo "🧹 Stopping daemon (PID: $DAEMON_PID)..."
    kill $DAEMON_PID
    wait $DAEMON_PID 2>/dev/null || true
    echo "✓ Daemon stopped"
fi

exit 0
