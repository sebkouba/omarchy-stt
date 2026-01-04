# Continuous Recording Daemon (Approach 2): Implementation Plan

## Overview

Replace the current FFmpeg spawn-per-recording approach with a **pipe-based continuous recording daemon** that eliminates nearly all stop latency (~420ms savings, from 433ms → ~10ms).

**Current Architecture:**
```
User presses hotkey → transcribe start
  ↓ spawns FFmpeg → records to /tmp/ptt_current.wav
  ↓ 433ms latency: 363ms FFmpeg exit + 50ms fs sync + 20ms stability check
User releases hotkey → transcribe stop
```

**New Architecture:**
```
System startup → recording-daemon starts
  ↓ spawns FFmpeg once with stdout pipe
  ↓ continuously reads PCM samples into RAM circular buffer
  ↓ listens on Unix socket /tmp/transcribe-rs-v2-recording.sock

User presses hotkey → transcribe start
  ↓ send start command → daemon records buffer index

User releases hotkey → transcribe stop
  ↓ send stop command → daemon extracts samples from buffer
  ↓ ~10ms latency: extract + write WAV file
  ↓ returns path to WAV file
```

## Performance Targets

| Metric | Current | Target | Improvement |
|--------|---------|--------|-------------|
| Start latency | ~213ms (FFmpeg spawn + init) | 0ms (already recording) | 100% |
| Stop latency | ~433ms (FFmpeg exit + delays) | ~5-10ms (buffer extract + WAV write) | 97% |
| **Total improvement** | - | - | **~640ms saved per dictation** |

## Phase 1: Circular Buffer Foundation

### 1.1: Create Circular Buffer Module

**File:** `src/circular_buffer.rs`

**Features:**
- Fixed-size ring buffer for i16 PCM samples
- Configurable capacity (default: 2 minutes @ 16kHz = 1,920,000 samples = 3.66 MB)
- Thread-safe read/write with `Arc<Mutex<CircularBuffer>>`
- Wraparound-aware extraction

**API:**
```rust
pub struct CircularBuffer {
    buffer: Vec<i16>,
    capacity: usize,
    write_index: usize,  // Current write position
    total_written: usize, // Total samples written (never wraps)
}

impl CircularBuffer {
    pub fn new(capacity: usize) -> Self;
    pub fn write_samples(&mut self, samples: &[i16]) -> usize;
    pub fn extract_range(&self, start_index: usize, sample_count: usize) -> Vec<i16>;
    pub fn get_current_index(&self) -> usize;
}
```

**Tests:**
- Basic write/read
- Wraparound extraction (start before wrap, end after wrap)
- Overwrite old data when full
- Extract more samples than capacity (should return only what's available)

---

### 1.2: Add WAV File Generation

**File:** `src/audio.rs` (add new function)

**Function:**
```rust
/// Write i16 PCM samples as WAV file with proper header
pub fn write_wav_from_samples(
    samples: &[i16],
    sample_rate: u32,
    output_path: &Path,
) -> Result<(), Box<dyn Error>>
```

**WAV Header Format (44 bytes):**
```
RIFF header (12 bytes):
  - "RIFF" (4 bytes)
  - file_size - 8 (4 bytes, little-endian u32)
  - "WAVE" (4 bytes)

fmt chunk (24 bytes):
  - "fmt " (4 bytes)
  - 16 (4 bytes, chunk size)
  - 1 (2 bytes, PCM format)
  - 1 (2 bytes, mono)
  - sample_rate (4 bytes)
  - byte_rate = sample_rate * 2 (4 bytes)
  - 2 (2 bytes, block align = channels * bits_per_sample / 8)
  - 16 (2 bytes, bits per sample)

data chunk (8 bytes + data):
  - "data" (4 bytes)
  - data_size = samples.len() * 2 (4 bytes)
  - raw samples (samples.len() * 2 bytes)
```

**Tests:**
- Write 1 second of silence, verify with `hound` reader
- Write sine wave, verify format matches FFmpeg-generated WAV
- Compare against existing test WAV files

---

## Phase 2: Recording Daemon Binary

### 2.1: Create Daemon Binary Structure

**File:** `src/bin/recording-daemon.rs`

**Architecture:**
```
main()
  ↓
spawn_ffmpeg_continuous() → Child process
  ↓
spawn_reader_thread() → reads stdout, writes to CircularBuffer
  ↓
spawn_socket_listener() → handles start/stop commands
  ↓
wait_for_signals() → SIGTERM/SIGINT cleanup
```

**FFmpeg Command:**
```bash
ffmpeg -f pulse -i <MICROPHONE> -ar 16000 -ac 1 -f s16le pipe:1
```

**Output:** Raw 16-bit PCM samples to stdout (no WAV header, no file I/O)

---

### 2.2: Reader Thread Implementation

**Responsibilities:**
1. Read from FFmpeg stdout in chunks (8192 bytes = 4096 samples)
2. Convert bytes to i16 samples (little-endian)
3. Write to circular buffer
4. Detect FFmpeg crash (EOF) and restart
5. Log performance metrics (samples/sec, buffer fullness)

**Error Handling:**
- FFmpeg crash → wait 1 second, restart FFmpeg, log warning
- Buffer write failure → log error, continue (should never fail)
- Read timeout → log warning, check if FFmpeg still alive

---

### 2.3: Socket Listener Implementation

**Protocol:** Line-delimited JSON on Unix socket `/tmp/transcribe-rs-v2-recording.sock`

**Commands:**

**Start Recording:**
```json
Request:  {"command": "start"}
Response: {"ok": true, "start_index": 1234567}
```
- Returns current buffer index
- Client saves this to `/tmp/ptt_recording.pid` (repurposed file)

**Stop Recording:**
```json
Request:  {"command": "stop", "start_index": 1234567}
Response: {"ok": true, "wav_path": "/tmp/ptt_current.wav", "duration_ms": 2350}
```
- Calculates `sample_count = current_index - start_index`
- Extracts samples from buffer
- Writes WAV file
- Returns path and duration

**Ping (Health Check):**
```json
Request:  {"command": "ping"}
Response: {"ok": true, "buffer_fullness": 0.32, "uptime_seconds": 3600}
```

**Error Response:**
```json
Response: {"ok": false, "error": "Already recording"}
```

---

### 2.4: Daemon State Management

**State Machine:**
```
IDLE → START → RECORDING → STOP → IDLE
```

**State:**
```rust
struct DaemonState {
    recording_start_index: Option<usize>,
    ffmpeg_process: Option<Child>,
    buffer: Arc<Mutex<CircularBuffer>>,
    start_time: Instant,
}
```

**Concurrency:**
- Single-threaded request processing (no concurrent recordings)
- Mutex-protected buffer for reader thread
- State protected by Mutex

---

## Phase 3: Client Integration

### 3.1: Update Recording Module

**File:** `src/recording.rs`

**Changes:**

**`start_recording()`:**
```rust
pub fn start_recording(config: &AudioConfig) -> Result<(), Box<dyn Error>> {
    // 1. Connect to recording daemon socket
    let mut stream = UnixStream::connect("/tmp/transcribe-rs-v2-recording.sock")?;

    // 2. Send start command
    let request = json!({"command": "start"});
    writeln!(stream, "{}", request)?;

    // 3. Read response
    let response: serde_json::Value = read_json_response(&mut stream)?;

    // 4. Save start_index to PID file (repurpose existing file)
    let start_index = response["start_index"].as_u64().ok_or("Missing start_index")?;
    fs::write(&config.recording_pid_file, start_index.to_string())?;

    log("Recording started via daemon", &config.log_file);
    Ok(())
}
```

**`stop_recording()`:**
```rust
pub fn stop_recording(config: &AudioConfig) -> Result<PathBuf, Box<dyn Error>> {
    // 1. Read start_index from file
    let start_index_str = fs::read_to_string(&config.recording_pid_file)?;
    let start_index: u64 = start_index_str.trim().parse()?;

    // 2. Connect to daemon
    let mut stream = UnixStream::connect("/tmp/transcribe-rs-v2-recording.sock")?;

    // 3. Send stop command
    let request = json!({"command": "stop", "start_index": start_index});
    writeln!(stream, "{}", request)?;

    // 4. Read response
    let response: serde_json::Value = read_json_response(&mut stream)?;
    let wav_path = response["wav_path"].as_str().ok_or("Missing wav_path")?;

    // 5. Remove state file
    fs::remove_file(&config.recording_pid_file)?;

    log(&format!("Recording stopped, saved to {}", wav_path), &config.log_file);
    Ok(PathBuf::from(wav_path))
}
```

**Remove:**
- All FFmpeg spawning/killing logic
- `verify_file_stable()` function (no longer needed)
- 50ms filesystem sync delay
- 20ms stability check
- SIGINT/SIGKILL signal handling

**Keep:**
- Logging infrastructure
- Error handling patterns
- PID file management (now stores buffer index instead of PID)

---

## Phase 4: Deployment & Integration

### 4.1: Systemd Service

**File:** `recording-daemon.service`

```ini
[Unit]
Description=Transcribe-RS Recording Daemon (Continuous FFmpeg)
After=network.target sound.target pulseaudio.service

[Service]
Type=simple
ExecStart=~/transcribe-rs-v2/target/release/recording-daemon
WorkingDirectory=~/transcribe-rs-v2
Restart=always
RestartSec=5s

# Environment
Environment="RECORDING_BUFFER_SIZE=120"  # seconds

[Install]
WantedBy=default.target
```

**Installation:**
```bash
# Build
cargo build --release --bin recording-daemon

# Install service
cp recording-daemon.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable recording-daemon
systemctl --user start recording-daemon

# Check status
systemctl --user status recording-daemon
journalctl --user -u recording-daemon -f
```

---

### 4.2: Dependency Management

**Transcription daemon should wait for recording daemon:**

Update `transcribe-daemon.service`:
```ini
[Unit]
After=recording-daemon.service
Requires=recording-daemon.service
```

---

### 4.3: Update CLI Binary

**File:** `src/bin/cli.rs`

No changes needed! The `recording` module is already used, so changes are transparent.

---

## Phase 5: Error Handling & Edge Cases

### 5.1: Recording Longer Than Buffer

**Problem:** User records for 3 minutes, buffer only holds 2 minutes.

**Solution:**
- When `sample_count > buffer.capacity()`, return only last N minutes
- Log warning: "Recording truncated to last 2 minutes"
- Notify user via desktop notification

---

### 5.2: Daemon Crash Recovery

**Problem:** Recording daemon crashes during recording.

**Detection:**
- Client gets connection refused when trying to stop
- PID file exists but socket is dead

**Solution:**
```rust
// In stop_recording()
if let Err(_) = UnixStream::connect(socket_path) {
    log("ERROR: Recording daemon not running!", &config.log_file);
    fs::remove_file(&config.recording_pid_file).ok();
    return Err("Recording daemon crashed - audio lost".into());
}
```

**User Experience:**
- Desktop notification: "❌ Recording lost - daemon crashed"
- Instructions: `systemctl --user restart recording-daemon`

---

### 5.3: FFmpeg Crash During Recording

**Problem:** FFmpeg dies while daemon is running.

**Solution:**
- Reader thread detects EOF on stdout
- Logs error with FFmpeg stderr output
- Waits 1 second (avoid tight loop)
- Respawns FFmpeg with same arguments
- Logs warning: "FFmpeg restarted after crash"
- Buffer keeps old data, recording can continue

---

### 5.4: Concurrent Start Requests

**Problem:** User presses hotkey twice quickly.

**Solution:**
```rust
// In daemon state
if state.recording_start_index.is_some() {
    return json!({"ok": false, "error": "Already recording"});
}
```

Client sees error response, shows notification.

---

### 5.5: Buffer Wraparound Edge Cases

**Problem:** Start index = 1,900,000, current index = 50,000 (wrapped around).

**Solution:**
```rust
// In extract_range()
let sample_count = if start_index > self.total_written {
    // Start index is from before current buffer data
    // Return only what's available in current buffer
    self.total_written.min(self.capacity)
} else {
    (self.total_written - start_index).min(self.capacity)
};
```

Log warning if data was lost.

---

## Phase 6: Testing Strategy

### 6.1: Unit Tests

**`tests/circular_buffer_test.rs`:**
```rust
#[test]
fn test_basic_write_read()
#[test]
fn test_wraparound_extraction()
#[test]
fn test_overwrite_old_data()
#[test]
fn test_extract_more_than_capacity()
```

**`tests/wav_generation_test.rs`:**
```rust
#[test]
fn test_write_silence()
#[test]
fn test_write_sine_wave()
#[test]
fn test_compare_with_ffmpeg()  // Uses existing test1.wav
```

**`src/audio.rs` (existing tests):**
```rust
#[test]
fn test_read_write_roundtrip()  // Write then read back
```

---

### 6.2: Integration Tests

**`tests/recording_daemon_test.rs`:**
```rust
#[test]
#[ignore]  // Requires daemon running
fn test_start_stop_workflow()

#[test]
#[ignore]
fn test_concurrent_start_error()

#[test]
#[ignore]
fn test_daemon_health_check()

#[test]
fn test_daemon_socket_protocol()  // Mock daemon, test JSON parsing
```

---

### 6.3: Manual Testing Checklist

```bash
# 1. Start daemon
cargo run --release --bin recording-daemon

# 2. In another terminal, test recording
./target/release/transcribe start
sleep 2
./target/release/transcribe stop

# 3. Verify WAV file
ls -lh /tmp/ptt_current.wav
ffprobe /tmp/ptt_current.wav

# 4. Test back-to-back recordings
./target/release/transcribe start && sleep 1 && ./target/release/transcribe stop
./target/release/transcribe start && sleep 1 && ./target/release/transcribe stop
./target/release/transcribe start && sleep 1 && ./target/release/transcribe stop

# 5. Test long recording (buffer overflow)
./target/release/transcribe start
sleep 150  # 2.5 minutes (exceeds 2-minute buffer)
./target/release/transcribe stop
# Should show warning about truncation

# 6. Test daemon crash recovery
pkill recording-daemon
./target/release/transcribe stop  # Should show error
systemctl --user restart recording-daemon
./target/release/transcribe start && sleep 1 && ./target/release/transcribe stop

# 7. Measure latency
time ./target/release/transcribe stop  # Should be <50ms
```

---

### 6.4: Performance Testing

**Measure stop latency:**
```bash
# Add timing logs to recording.rs stop_recording()
log(&format!("Stop latency breakdown:
  Socket connect: {}ms
  Command send: {}ms
  Daemon processing: {}ms
  Response read: {}ms
  Total: {}ms", ...));
```

**Measure buffer performance:**
```bash
# Add metrics to reader thread
log(&format!("Buffer stats:
  Samples written: {}
  Write rate: {} samples/sec
  Buffer fullness: {}%
  FFmpeg uptime: {}s", ...));
```

---

## Dependencies

**New crates needed:**
```toml
# Already have:
# serde, serde_json, chrono, nix

# No new dependencies needed!
```

---

## Cargo.toml Updates

```toml
[[bin]]
name = "recording-daemon"
path = "src/bin/recording-daemon.rs"
```

---

## File Structure

```
src/
├── circular_buffer.rs          # NEW: Ring buffer for PCM samples
├── audio.rs                     # MODIFIED: Add write_wav_from_samples()
├── recording.rs                 # MODIFIED: Use daemon socket instead of FFmpeg
├── bin/
│   ├── recording-daemon.rs     # NEW: Continuous recording daemon
│   ├── cli.rs                  # UNCHANGED: Transparent to changes
│   ├── daemon.rs               # UNCHANGED
│   └── client.rs               # UNCHANGED

tests/
├── circular_buffer_test.rs     # NEW: Buffer tests
├── wav_generation_test.rs      # NEW: WAV writing tests
└── recording_daemon_test.rs    # NEW: Integration tests

specs/
└── CONTINUOUS_RECORDING_DAEMON.md  # This file

recording-daemon.service        # NEW: Systemd service
```

---

## Implementation Order

1. ✅ Write this spec
2. ⏳ Implement `src/circular_buffer.rs` with tests
3. ⏳ Add `write_wav_from_samples()` to `src/audio.rs` with tests
4. ⏳ Create `src/bin/recording-daemon.rs` basic structure
5. ⏳ Implement FFmpeg spawning and reader thread
6. ⏳ Implement socket listener and protocol
7. ⏳ Update `src/recording.rs` to use socket
8. ⏳ Write integration tests
9. ⏳ Create systemd service
10. ⏳ Manual testing and latency measurement
11. ⏳ Documentation updates (CLAUDE.md)
12. ✅ Build release and notify user

---

## Success Criteria

- [ ] Stop latency reduced from ~433ms to <50ms (measured)
- [ ] Start latency is 0ms (instant response)
- [ ] Can record back-to-back without delays
- [ ] Daemon automatically restarts on crash
- [ ] Buffer handles wraparound correctly
- [ ] All unit tests passing
- [ ] All integration tests passing
- [ ] Manual testing checklist complete
- [ ] User can switch back to old system if needed
- [ ] No regressions in transcription quality

---

## Rollback Plan

If something goes wrong:

1. Stop recording daemon: `systemctl --user stop recording-daemon`
2. Revert `src/recording.rs` changes: `git checkout src/recording.rs`
3. Rebuild: `cargo build --release --bin transcribe`
4. Old FFmpeg-based system works again

The old and new systems can coexist during testing by using different socket paths.

---

## Future Optimizations (Not in Scope)

- [ ] Zero-copy buffer extraction using memory mapping
- [ ] Configurable buffer size via CLI argument
- [ ] Multiple simultaneous recordings (unlikely needed)
- [ ] Streaming transcription (start transcribing before recording stops)
- [ ] GPU-accelerated audio processing
- [ ] Compressed buffer storage (unlikely needed with 3.66MB)

---

## Estimated Timeline

- **Phase 1 (Buffer & WAV):** 2-3 hours
- **Phase 2 (Daemon):** 3-4 hours
- **Phase 3 (Integration):** 1-2 hours
- **Phase 4 (Deployment):** 1 hour
- **Phase 5 (Error Handling):** 1-2 hours
- **Phase 6 (Testing):** 2-3 hours

**Total:** 10-15 hours

---

**Status:** Ready to implement
**Created:** 2025-11-04
**Author:** Claude (Sonnet 4.5)
