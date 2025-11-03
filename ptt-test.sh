#!/bin/bash
# Push-to-Talk Recording Test
# This script uses a PID file to manage recording state

RECORDING_PID_FILE="/tmp/ptt_recording.pid"
RECORDING_FILE="/tmp/ptt_current.wav"
MIC_SOURCE="alsa_input.usb-046d_C922_Pro_Stream_Webcam_C4C393EF-02.analog-stereo"
LOG_FILE="/tmp/ptt_debug.log"

# Logging function with timestamp
log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S.%3N')] [ptt-test] $*" >> "$LOG_FILE"
}

case "$1" in
    start)
        log "=== KEY PRESS: Recording start requested ==="

        # Check if already recording
        if [ -f "$RECORDING_PID_FILE" ]; then
            EXISTING_PID=$(cat "$RECORDING_PID_FILE")
            log "WARNING: Already recording (PID: $EXISTING_PID)"
            echo "Already recording"
            exit 0
        fi

        log "Starting ffmpeg recording to $RECORDING_FILE"

        # Remove old recording file to avoid any potential file handle issues
        if [ -f "$RECORDING_FILE" ]; then
            OLD_SIZE=$(stat -f%z "$RECORDING_FILE" 2>/dev/null || stat -c%s "$RECORDING_FILE" 2>/dev/null)
            rm -f "$RECORDING_FILE"
            log "Removed old recording file ($OLD_SIZE bytes)"
        fi

        # Start recording in background (log errors for debugging)
        FFMPEG_LOG="/tmp/ffmpeg_error.log"
        ffmpeg -f pulse -i "$MIC_SOURCE" -ar 16000 -ac 1 -sample_fmt s16 -y "$RECORDING_FILE" >/dev/null 2>"$FFMPEG_LOG" &

        # Save the PID
        FFMPEG_PID=$!
        echo $FFMPEG_PID > "$RECORDING_PID_FILE"
        log "ffmpeg started with PID: $FFMPEG_PID"
        log "PID file written: $RECORDING_PID_FILE"

        # Give ffmpeg time to initialize (open mic, write WAV header, start capturing)
        sleep 0.15
        log "ffmpeg initialization delay complete"

        # Verify ffmpeg is still running
        if ! ps -p "$FFMPEG_PID" > /dev/null 2>&1; then
            log "ERROR: ffmpeg died immediately after starting!"
            rm "$RECORDING_PID_FILE"
            notify-send "❌ Recording Failed" "ffmpeg could not start" -t 3000
            exit 1
        fi

        echo "🎤 Recording started..."
        notify-send "🎤 Recording" "Speak now..." -t 1000
        ;;

    stop)
        log "=== KEY RELEASE: Recording stop requested ==="

        # Check if recording
        if [ ! -f "$RECORDING_PID_FILE" ]; then
            log "WARNING: Not recording (PID file not found)"
            echo "Not recording"
            exit 0
        fi

        # Get the PID and kill the recording
        PID=$(cat "$RECORDING_PID_FILE")
        log "Stopping recording (PID: $PID)"

        # Check if process is actually running
        if ps -p "$PID" > /dev/null 2>&1; then
            log "Process $PID is running, sending SIGINT..."
            kill -SIGINT "$PID" 2>/dev/null
            KILL_STATUS=$?
            log "kill command exit status: $KILL_STATUS"
        else
            log "WARNING: Process $PID not found in process table!"
        fi

        # Wait for ffmpeg to actually exit (poll until process is gone)
        log "Waiting for ffmpeg process to exit..."
        WAIT_START=$(date +%s%3N)
        POLL_COUNT=0
        MAX_WAIT=5000  # 5 seconds max

        while ps -p "$PID" > /dev/null 2>&1; do
            POLL_COUNT=$((POLL_COUNT + 1))
            ELAPSED=$(($(date +%s%3N) - WAIT_START))

            if [ "$ELAPSED" -gt "$MAX_WAIT" ]; then
                log "ERROR: ffmpeg did not exit after ${ELAPSED}ms, killing forcefully"
                kill -9 "$PID" 2>/dev/null
                break
            fi

            # Poll every 10ms
            sleep 0.01
        done

        WAIT_END=$(date +%s%3N)
        WAIT_TIME=$((WAIT_END - WAIT_START))
        log "ffmpeg process exited after ${WAIT_TIME}ms (polled $POLL_COUNT times)"

        # Give filesystem a moment to flush any pending writes
        sleep 0.05
        log "Filesystem sync delay complete"

        rm "$RECORDING_PID_FILE"
        log "PID file removed"

        # Verify file stability (ensure size isn't changing)
        if [ -f "$RECORDING_FILE" ]; then
            SIZE1=$(stat -f%z "$RECORDING_FILE" 2>/dev/null || stat -c%s "$RECORDING_FILE" 2>/dev/null)
            sleep 0.02
            SIZE2=$(stat -f%z "$RECORDING_FILE" 2>/dev/null || stat -c%s "$RECORDING_FILE" 2>/dev/null)

            if [ "$SIZE1" != "$SIZE2" ]; then
                log "WARNING: File size changed from $SIZE1 to $SIZE2 bytes, waiting longer..."
                sleep 0.1
                SIZE2=$(stat -f%z "$RECORDING_FILE" 2>/dev/null || stat -c%s "$RECORDING_FILE" 2>/dev/null)
                log "File size after additional wait: $SIZE2 bytes"
            else
                log "File size stable at $SIZE1 bytes"
            fi
        fi

        # Check file size
        if [ -f "$RECORDING_FILE" ]; then
            FILE_SIZE=$(stat -f%z "$RECORDING_FILE" 2>/dev/null || stat -c%s "$RECORDING_FILE" 2>/dev/null)
            log "Recording file size: $FILE_SIZE bytes"

            # Validate file has actual audio data
            if [ "$FILE_SIZE" -lt 1000 ]; then
                log "ERROR: Recording file is empty or too small ($FILE_SIZE bytes)"
                log "ffmpeg may have failed to capture audio from the microphone"

                # Log ffmpeg errors if available
                if [ -f "/tmp/ffmpeg_error.log" ]; then
                    log "ffmpeg stderr output:"
                    tail -20 /tmp/ffmpeg_error.log >> "$LOG_FILE"
                fi

                echo "⏹️  Recording failed"
                notify-send "❌ Recording Failed" "Microphone may be busy or ffmpeg initialization failed" -t 3000
                log "=== STOP COMPLETE (FAILED) ==="
                exit 1
            fi
        else
            log "ERROR: Recording file not found: $RECORDING_FILE"
            notify-send "❌ Error" "Recording file not found" -t 2000
            log "=== STOP COMPLETE (FAILED) ==="
            exit 1
        fi

        echo "⏹️  Recording stopped"
        notify-send "⏹️  Processing..." "Transcribing audio..." -t 1000

        log "Calling transcribe-to-clipboard.sh..."
        # Transcribe and copy to clipboard
        /home/seb/code/cloned/transcribe-rs/transcribe-to-clipboard.sh "$RECORDING_FILE"
        TRANSCRIBE_STATUS=$?
        log "transcribe-to-clipboard.sh exit status: $TRANSCRIBE_STATUS"
        log "=== STOP COMPLETE ==="
        ;;

    *)
        echo "Usage: $0 {start|stop}"
        exit 1
        ;;
esac
