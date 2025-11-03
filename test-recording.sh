#!/bin/bash
# Quick test to verify microphone recording

TEST_FILE="/tmp/test_recording.wav"
MIC_SOURCE="alsa_input.usb-046d_C922_Pro_Stream_Webcam_C4C393EF-02.analog-stereo"

echo "🎤 Recording 3 seconds... SPEAK NOW!"
ffmpeg -f pulse -i "$MIC_SOURCE" -ar 16000 -ac 1 -sample_fmt s16 -y -t 3 "$TEST_FILE" 2>&1 | grep -E "(Duration|size)"

echo ""
echo "📊 File info:"
ls -lh "$TEST_FILE"
file "$TEST_FILE"

echo ""
echo "🔊 Playing back recording..."
ffplay -nodisp -autoexit "$TEST_FILE" 2>/dev/null

echo ""
echo "📝 Transcribing..."
/home/seb/code/cloned/transcribe-rs/target/release/transcribe-client "$TEST_FILE"
