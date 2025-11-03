#!/bin/bash
# All-in-one voice transcription script

DURATION="${1:-10}"

echo "🎤 Voice Transcription Tool"
echo "==========================="
echo ""
echo "Recording for ${DURATION} seconds..."
echo "Start speaking NOW!"
echo ""

# Record audio
./record.sh recording.wav ${DURATION}

echo ""
echo "Processing transcription..."
echo ""

# Transcribe
cargo run --example simple --release

echo ""
echo "Done! ✓"
