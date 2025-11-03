#!/bin/bash
# Simple voice recorder that outputs in the format required by transcribe-rs
# Format: 16kHz, mono, 16-bit PCM WAV

OUTPUT_FILE="${1:-recording.wav}"
DURATION="${2:-5}"

echo "Recording for ${DURATION} seconds..."
echo "Press Ctrl+C to stop early"
echo ""
echo "Start speaking now!"

# Record from C922 webcam microphone
# -f pulse: Use PulseAudio (common on Linux)
# -ar 16000: 16kHz sample rate
# -ac 1: Mono (1 channel)
# -sample_fmt s16: 16-bit samples
ffmpeg -f pulse -i alsa_input.usb-046d_C922_Pro_Stream_Webcam_C4C393EF-02.analog-stereo -t ${DURATION} -ar 16000 -ac 1 -sample_fmt s16 -y "${OUTPUT_FILE}" 2>&1 | grep -E "(time=|size=)"

echo ""
echo "Recording saved to: ${OUTPUT_FILE}"
