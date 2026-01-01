#!/bin/bash
#
# End-to-End Audio Test for transcribe-rs-v2
#
# This script tests the full audio pipeline:
# 1. Creates a PipeWire virtual sink
# 2. Starts a test recording daemon using that sink
# 3. Plays test audio, triggers recording, captures transcription
# 4. Verifies transcription accuracy using fuzzy matching
# 5. Cleans up on exit
#
# Requirements: PipeWire, pactl, paplay, socat, Python 3
#

set -e

# =============================================================================
# Configuration
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

TEST_SINK_NAME="transcribe_test_sink"
# Note: Recording daemon uses hardcoded paths, cannot be customized via env
RECORDING_SOCKET="/tmp/transcribe-rs-v2-recording.sock"
TRANSCRIPTION_SOCKET="/tmp/transcribe-rs-v2.sock"
OUTPUT_WAV_PATH="/tmp/ptt_current.wav"

# Expected transcription for JFK sample (normalized for comparison)
EXPECTED_JFK="And so, my fellow Americans, ask not what your country can do for you. Ask what you can do for your country."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# State tracking for cleanup
MODULE_ID=""
RECORDING_DAEMON_PID=""
PRODUCTION_DAEMON_STOPPED=false

# =============================================================================
# Utility Functions
# =============================================================================

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# =============================================================================
# Cleanup Function
# =============================================================================

cleanup() {
    local exit_code=$?
    log_info "Cleaning up..."

    # Stop test recording daemon if running
    if [ -n "$RECORDING_DAEMON_PID" ] && kill -0 "$RECORDING_DAEMON_PID" 2>/dev/null; then
        log_info "Stopping test recording daemon (PID: $RECORDING_DAEMON_PID)"
        kill "$RECORDING_DAEMON_PID" 2>/dev/null || true
        wait "$RECORDING_DAEMON_PID" 2>/dev/null || true
    fi

    # Restart production daemon if we stopped it
    if [ "$PRODUCTION_DAEMON_STOPPED" = true ]; then
        log_info "Restarting production recording-daemon..."
        if systemctl --user start recording-daemon 2>/dev/null; then
            log_success "Production recording-daemon restarted"
        else
            log_warning "Failed to restart production recording-daemon"
            log_warning "Run: systemctl --user start recording-daemon"
        fi
    fi

    # Unload virtual sink module
    if [ -n "$MODULE_ID" ]; then
        log_info "Unloading virtual sink module (ID: $MODULE_ID)"
        pactl unload-module "$MODULE_ID" 2>/dev/null || true
    fi

    if [ $exit_code -eq 0 ]; then
        log_success "Cleanup complete"
    else
        log_warning "Cleanup complete (test failed with exit code $exit_code)"
    fi

    exit $exit_code
}

trap cleanup EXIT INT TERM

# =============================================================================
# Dependency Checks
# =============================================================================

check_dependencies() {
    log_info "Checking dependencies..."

    local missing=()

    # Check for PipeWire tools
    if ! command -v pactl &>/dev/null; then
        missing+=("pactl (pulseaudio-utils or pipewire-pulse)")
    fi

    if ! command -v paplay &>/dev/null; then
        missing+=("paplay (pulseaudio-utils or pipewire-pulse)")
    fi

    # Check for socat (for Unix socket communication)
    if ! command -v socat &>/dev/null; then
        missing+=("socat")
    fi

    # Check for Python 3 (for fuzzy matching)
    if ! command -v python3 &>/dev/null; then
        missing+=("python3")
    fi

    # Check for jq (for JSON parsing)
    if ! command -v jq &>/dev/null; then
        missing+=("jq")
    fi

    # Check for ffprobe (for audio info, optional)
    if ! command -v ffprobe &>/dev/null; then
        log_warning "ffprobe not found, audio duration info will be unavailable"
    fi

    if [ ${#missing[@]} -gt 0 ]; then
        log_error "Missing dependencies:"
        for dep in "${missing[@]}"; do
            echo "  - $dep"
        done
        echo ""
        echo "Install on Arch Linux:"
        echo "  sudo pacman -S pipewire-pulse socat python jq"
        exit 1
    fi

    # Check if PipeWire/PulseAudio is running
    if ! pactl info &>/dev/null; then
        log_error "PipeWire/PulseAudio server is not running"
        exit 1
    fi

    # Check for test audio file
    if [ ! -f "samples/jfk.wav" ]; then
        log_error "Test audio file not found: samples/jfk.wav"
        exit 1
    fi

    # Check for recording daemon binary
    if [ -f "builds/staging/recording-daemon" ]; then
        RECORDING_DAEMON_BIN="builds/staging/recording-daemon"
    elif [ -f "builds/current/recording-daemon" ]; then
        RECORDING_DAEMON_BIN="builds/current/recording-daemon"
    elif [ -f "target/release/recording-daemon" ]; then
        RECORDING_DAEMON_BIN="target/release/recording-daemon"
    else
        log_error "recording-daemon binary not found"
        echo "Run ./scripts/build.sh first"
        exit 1
    fi
    log_info "Using recording daemon: $RECORDING_DAEMON_BIN"

    # Check for transcribe-client binary
    if [ -f "builds/staging/transcribe-client" ]; then
        TRANSCRIBE_CLIENT_BIN="builds/staging/transcribe-client"
    elif [ -f "builds/current/transcribe-client" ]; then
        TRANSCRIBE_CLIENT_BIN="builds/current/transcribe-client"
    elif [ -f "target/release/transcribe-client" ]; then
        TRANSCRIBE_CLIENT_BIN="target/release/transcribe-client"
    else
        log_error "transcribe-client binary not found"
        echo "Run ./scripts/build.sh first"
        exit 1
    fi
    log_info "Using transcribe client: $TRANSCRIBE_CLIENT_BIN"

    log_success "All dependencies satisfied"
}

# =============================================================================
# Virtual Sink Setup
# =============================================================================

setup_virtual_sink() {
    log_info "Creating virtual audio sink: $TEST_SINK_NAME"

    # Check if sink already exists and remove it
    if pactl list short sinks | grep -q "$TEST_SINK_NAME"; then
        log_warning "Virtual sink already exists, removing it first"
        local old_module=$(pactl list short modules | grep "sink_name=$TEST_SINK_NAME" | cut -f1)
        if [ -n "$old_module" ]; then
            pactl unload-module "$old_module" || true
        fi
    fi

    # Create virtual sink
    MODULE_ID=$(pactl load-module module-null-sink sink_name="$TEST_SINK_NAME" sink_properties=device.description="Transcribe_Test_Sink")

    if [ -z "$MODULE_ID" ]; then
        log_error "Failed to create virtual sink"
        exit 1
    fi

    log_success "Virtual sink created (module ID: $MODULE_ID)"
    log_info "Monitor source: ${TEST_SINK_NAME}.monitor"
}

# =============================================================================
# Recording Daemon Management
# =============================================================================

check_and_stop_production_daemon() {
    # Check if production recording daemon is running
    if systemctl --user is-active recording-daemon &>/dev/null; then
        log_warning "Production recording-daemon is running"
        log_info "Stopping it temporarily for E2E test..."

        if systemctl --user stop recording-daemon; then
            PRODUCTION_DAEMON_STOPPED=true
            log_success "Production recording-daemon stopped"
            # Wait for socket to be released
            sleep 1
        else
            log_error "Failed to stop production recording-daemon"
            log_error "Run: systemctl --user stop recording-daemon"
            exit 1
        fi
    fi

    # Also check for any process using the socket
    if [ -S "$RECORDING_SOCKET" ]; then
        log_warning "Recording socket already exists, removing it"
        rm -f "$RECORDING_SOCKET"
        sleep 0.5
    fi
}

start_test_recording_daemon() {
    log_info "Starting test recording daemon..."

    # Check and stop production daemon if running
    check_and_stop_production_daemon

    # Set environment and start daemon with test microphone
    RECORDING_MICROPHONE="${TEST_SINK_NAME}.monitor" \
        "$RECORDING_DAEMON_BIN" &
    RECORDING_DAEMON_PID=$!

    # Wait for socket to appear
    local max_wait=10
    local waited=0
    while [ ! -S "$RECORDING_SOCKET" ] && [ $waited -lt $max_wait ]; do
        sleep 0.5
        waited=$((waited + 1))
        # Check if daemon is still running
        if ! kill -0 "$RECORDING_DAEMON_PID" 2>/dev/null; then
            log_error "Recording daemon exited unexpectedly"
            wait "$RECORDING_DAEMON_PID" 2>/dev/null || true
            exit 1
        fi
    done

    if [ ! -S "$RECORDING_SOCKET" ]; then
        log_error "Recording daemon socket did not appear after ${max_wait}s"
        exit 1
    fi

    log_success "Recording daemon started (PID: $RECORDING_DAEMON_PID)"
}

# =============================================================================
# Socket Communication
# =============================================================================

send_command() {
    local socket="$1"
    local command="$2"

    echo "$command" | socat - UNIX-CONNECT:"$socket"
}

# =============================================================================
# Fuzzy Matching (Python Helper)
# =============================================================================

calculate_similarity() {
    local text1="$1"
    local text2="$2"

    python3 << EOF
import difflib
import sys

# Normalize text for comparison
def normalize(text):
    # Convert to lowercase, remove extra whitespace
    text = ' '.join(text.lower().split())
    # Remove punctuation for comparison
    import string
    text = text.translate(str.maketrans('', '', string.punctuation))
    return text

text1 = """$text1"""
text2 = """$text2"""

norm1 = normalize(text1)
norm2 = normalize(text2)

similarity = difflib.SequenceMatcher(None, norm1, norm2).ratio() * 100
print(f"{similarity:.1f}")
EOF
}

# =============================================================================
# Test Execution
# =============================================================================

run_test() {
    local audio_file="$1"
    local expected_text="$2"
    local test_name="$3"
    local threshold="${4:-85}"

    log_info "Running test: $test_name"
    log_info "Audio file: $audio_file"
    log_info "Similarity threshold: ${threshold}%"

    # Get audio duration
    local duration=$(ffprobe -v quiet -show_entries format=duration -of csv=p=0 "$audio_file" 2>/dev/null || echo "unknown")
    log_info "Audio duration: ${duration}s"

    # Ping daemon first to make sure it's responsive
    log_info "Pinging recording daemon..."
    local ping_response=$(send_command "$RECORDING_SOCKET" '{"command":"ping"}')
    if ! echo "$ping_response" | jq -e '.ok == true' >/dev/null 2>&1; then
        log_error "Daemon ping failed: $ping_response"
        return 1
    fi
    log_success "Daemon is responsive"

    # Start recording
    log_info "Starting recording..."
    local start_response=$(send_command "$RECORDING_SOCKET" '{"command":"start"}')
    if ! echo "$start_response" | jq -e '.ok == true' >/dev/null 2>&1; then
        log_error "Failed to start recording: $start_response"
        return 1
    fi
    local start_index=$(echo "$start_response" | jq -r '.start_index')
    log_success "Recording started (start_index: $start_index)"

    # Play audio into virtual sink
    log_info "Playing audio into virtual sink..."
    paplay --device="$TEST_SINK_NAME" "$audio_file"
    log_success "Audio playback complete"

    # Small delay to ensure all audio is captured
    sleep 0.2

    # Stop recording
    log_info "Stopping recording..."
    local stop_response=$(send_command "$RECORDING_SOCKET" "{\"command\":\"stop\",\"start_index\":$start_index}")
    if ! echo "$stop_response" | jq -e '.ok == true' >/dev/null 2>&1; then
        log_error "Failed to stop recording: $stop_response"
        return 1
    fi

    local wav_path=$(echo "$stop_response" | jq -r '.wav_path')
    local duration_ms=$(echo "$stop_response" | jq -r '.duration_ms')
    local latency_ms=$(echo "$stop_response" | jq -r '.latency_ms')

    log_success "Recording stopped"
    log_info "  WAV path: $wav_path"
    log_info "  Duration: ${duration_ms}ms"
    log_info "  Latency: ${latency_ms}ms"

    # Verify WAV file exists (should be OUTPUT_WAV_PATH since daemon uses hardcoded path)
    if [ ! -f "$wav_path" ]; then
        log_error "WAV file not found: $wav_path"
        return 1
    fi

    # Verify audio was captured (file should have content)
    local wav_size=$(stat -c%s "$wav_path" 2>/dev/null || echo "0")
    if [ "$wav_size" -lt 100 ]; then
        log_error "WAV file is too small (${wav_size} bytes), audio may not have been captured"
        return 1
    fi
    log_info "  WAV size: ${wav_size} bytes"

    # Check if transcription daemon is running
    if [ ! -S "$TRANSCRIPTION_SOCKET" ]; then
        log_warning "Transcription daemon not running, skipping transcription test"
        log_info "Start it with: systemctl --user start transcribe-daemon"
        return 0
    fi

    # Transcribe
    log_info "Transcribing audio..."
    local transcription
    if ! transcription=$("$TRANSCRIBE_CLIENT_BIN" "$wav_path" 2>/dev/null); then
        log_error "Transcription failed"
        return 1
    fi
    log_success "Transcription complete"
    log_info "Result: \"$transcription\""

    # Calculate similarity
    log_info "Calculating similarity..."
    local similarity=$(calculate_similarity "$transcription" "$expected_text")
    log_info "Similarity: ${similarity}%"

    # Compare against threshold
    local passed=$(python3 -c "print('yes' if float('$similarity') >= float('$threshold') else 'no')")

    if [ "$passed" = "yes" ]; then
        log_success "TEST PASSED: ${similarity}% >= ${threshold}% threshold"
        return 0
    else
        log_error "TEST FAILED: ${similarity}% < ${threshold}% threshold"
        log_info "Expected: \"$expected_text\""
        log_info "Got:      \"$transcription\""
        return 1
    fi
}

# =============================================================================
# Main
# =============================================================================

main() {
    echo ""
    echo "=========================================="
    echo "  transcribe-rs-v2 E2E Audio Test"
    echo "=========================================="
    echo ""

    check_dependencies
    echo ""

    setup_virtual_sink
    echo ""

    start_test_recording_daemon
    echo ""

    # Give the daemon a moment to fill the buffer
    log_info "Waiting for buffer to initialize..."
    sleep 2

    echo ""
    echo "------------------------------------------"
    echo "  Running Tests"
    echo "------------------------------------------"
    echo ""

    local tests_passed=0
    local tests_failed=0

    # Test 1: JFK Speech
    if run_test "samples/jfk.wav" "$EXPECTED_JFK" "JFK Speech" 85; then
        tests_passed=$((tests_passed + 1))
    else
        tests_failed=$((tests_failed + 1))
    fi

    echo ""
    echo "=========================================="
    echo "  Test Summary"
    echo "=========================================="
    echo ""
    log_info "Passed: $tests_passed"
    log_info "Failed: $tests_failed"
    echo ""

    if [ $tests_failed -gt 0 ]; then
        log_error "Some tests failed"
        exit 1
    else
        log_success "All tests passed!"
        exit 0
    fi
}

# Run main function
main "$@"
