#!/bin/bash
# Push-to-Talk Recording Test
# This script uses a PID file to manage recording state

RECORDING_PID_FILE="/tmp/ptt_recording.pid"
RECORDING_FILE="/tmp/ptt_current.wav"
MIC_SOURCE="alsa_input.usb-046d_C922_Pro_Stream_Webcam_C4C393EF-02.analog-stereo"

case "$1" in
    start)
        # Check if already recording
        if [ -f "$RECORDING_PID_FILE" ]; then
            echo "Already recording"
            exit 0
        fi

        echo "🎤 Recording started..."
        notify-send "🎤 Recording" "Speak now..." -t 1000

        # Start recording in background
        ffmpeg -f pulse -i "$MIC_SOURCE" -ar 16000 -ac 1 -sample_fmt s16 -y "$RECORDING_FILE" &>/dev/null &

        # Save the PID
        echo $! > "$RECORDING_PID_FILE"
        ;;

    stop)
        # Check if recording
        if [ ! -f "$RECORDING_PID_FILE" ]; then
            echo "Not recording"
            exit 0
        fi

        # Get the PID and kill the recording
        PID=$(cat "$RECORDING_PID_FILE")
        kill -SIGINT "$PID" 2>/dev/null

        # Wait for ffmpeg to finalize the file (critical!)
        wait "$PID" 2>/dev/null || sleep 0.3

        rm "$RECORDING_PID_FILE"

        echo "⏹️  Recording stopped"
        notify-send "⏹️  Processing..." "Transcribing audio..." -t 1000

        # Transcribe and copy to clipboard
        /home/seb/code/cloned/transcribe-rs/transcribe-to-clipboard.sh "$RECORDING_FILE"
        ;;

    *)
        echo "Usage: $0 {start|stop}"
        exit 1
        ;;
esac
