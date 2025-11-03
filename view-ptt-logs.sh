#!/bin/bash
# Helper script to view PTT debug logs

LOG_FILE="/tmp/ptt_debug.log"

if [ ! -f "$LOG_FILE" ]; then
    echo "No log file found at $LOG_FILE"
    echo "Try using PTT first to generate logs."
    exit 1
fi

case "${1:-tail}" in
    tail)
        echo "Following PTT logs (Ctrl+C to stop)..."
        tail -f "$LOG_FILE"
        ;;
    cat)
        cat "$LOG_FILE"
        ;;
    clear)
        > "$LOG_FILE"
        echo "Log file cleared"
        ;;
    last)
        echo "=== Last PTT session ==="
        tac "$LOG_FILE" | awk '/=== STOP COMPLETE ===/,/=== KEY PRESS: Recording start requested ===/' | tac
        ;;
    stats)
        echo "=== PTT Log Statistics ==="
        echo "Total sessions: $(grep -c "KEY PRESS: Recording start" "$LOG_FILE")"
        echo "Successful transcriptions: $(grep -c "TRANSCRIPTION COMPLETE" "$LOG_FILE")"
        echo "Errors: $(grep -c "ERROR" "$LOG_FILE")"
        echo "Warnings: $(grep -c "WARNING" "$LOG_FILE")"
        echo ""
        echo "Average transcription time:"
        grep "Transcription completed in" "$LOG_FILE" | awk '{print $8}' | sed 's/ms//' | awk '{sum+=$1; count++} END {if(count>0) print sum/count "ms"}'
        ;;
    *)
        echo "Usage: $0 [tail|cat|clear|last|stats]"
        echo ""
        echo "  tail  - Follow logs in real-time (default)"
        echo "  cat   - Show all logs"
        echo "  clear - Clear the log file"
        echo "  last  - Show only the last PTT session"
        echo "  stats - Show statistics"
        exit 1
        ;;
esac
