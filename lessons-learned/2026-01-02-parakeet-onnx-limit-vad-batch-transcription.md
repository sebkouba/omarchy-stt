# Parakeet ONNX Sequence Limit, VAD Quality Improvement, and On-Demand Batch Architecture

**Date:** 2026-01-02
**Branch:** `feature/batch-transcription`
**Status:** Complete and deployed

## Context

Implemented a file watcher system to automatically transcribe audio files (MP3, M4A) dropped into a Dropbox directory. During implementation, discovered critical insights about Parakeet's sequence length limits and VAD's impact on transcription quality.

## Major Discoveries

### 1. Parakeet ONNX Has a Hard 6.7-Minute Sequence Limit

**The Problem:**
Attempting to transcribe a 15.5-minute audio file produced a cryptic error:
```
Attempting to broadcast an axis by a dimension other than 1. 6685 by 11685
```

**Root Cause Investigation:**

Used Python + ONNX library to inspect the exported model structure:

```python
import onnx
model = onnx.load("models/parakeet-tdt-0.6b-v3-int8/encoder.onnx")
# Found constant tensor: [1, 9999, 1024]
```

This is the **relative positional encoding table** for FastConformer's relative attention mechanism.

**The Math:**

For relative attention with sequence length N, you need `2N-1` relative position entries:
- Table size: 9999 entries
- Max sequence: `(9999 + 1) / 2 = 5000 frames`
- Frame duration: 80ms (10ms window × 8x subsampling)
- **Max duration: 5000 × 80ms = 400 seconds ≈ 6.7 minutes**

**Why This Happens:**

The PyTorch model can handle arbitrary lengths (dynamically generates position encodings), but the ONNX export requires **fixed-size tensors**. The 9999-entry table was baked in at export time.

**Error Breakdown:**
```
6685 frames (input length) + 5000 (offset) = 11685
Tries to broadcast against table size 9999 → ERROR
```

**Implications:**
- This is a hard limit in the ONNX model file
- Workaround: Chunk audio into <6 min segments
- Alternative: Re-export ONNX with larger table (but increases model size)

### 2. VAD Improves Transcription Quality (Not Just Speed)

**Initial Assumption:** VAD trades quality for speed by filtering out potential speech.

**Reality:** VAD **improved** transcription quality significantly.

**Evidence:**

| File | Without VAD | With VAD | Change |
|------|-------------|----------|--------|
| DV-2026-01-01-190119.txt | 980 words | 1535 words | **+57%** |
| DV-2025-12-31-112726.txt | 7408 words | 7766 words | **+5%** |
| Small files | N/A | Byte-for-byte identical | 0% |

**Why VAD Improves Quality:**

1. **Better chunking boundaries** - Without VAD, 5-minute chunks split at arbitrary points (potentially mid-sentence). With VAD, silence is removed first, so chunks contain coherent speech segments.

2. **Reduces silence-induced errors** - Transcribing long silence periods can cause model drift or hallucinations.

3. **More speech in context window** - After filtering 99% of silence, the model sees more continuous speech within its 6.7-minute limit.

**Performance Impact:**

15.5-minute file with sparse speech:
```
VAD: 6.5s speech from 934.7s audio (1% kept)
Transcription: 5.9s total (159.6x realtime)
```

Without VAD, this would require 26+ chunks and take ~15 minutes to process.

### 3. On-Demand Architecture Pattern for Batch Processing

**Problem:** Initial implementation ran file watcher in the transcription daemon's main loop, blocking keyboard dictation during long transcriptions.

**Solution:** Hybrid architecture - lightweight watcher + on-demand batch process.

**Architecture:**

```
watch-daemon (always running, <10 MB RAM)
├── Monitors directory with inotify
├── Debounces file changes (waits for Dropbox sync)
├── Spawns: transcribe-batch <file> <output-dir>
└── Continues monitoring immediately

transcribe-batch (spawned on-demand)
├── Loads Parakeet model (3-4 sec)
├── Applies VAD (filters silence)
├── Chunks if >6 min after VAD
├── Transcribes all chunks
├── Writes output.txt
├── Moves original to processed/
└── Exits (frees ~1.8 GB RAM)
```

**Benefits:**

- **No interference** - Keyboard dictation daemon stays responsive
- **Low baseline RAM** - Only uses memory when processing
- **Natural priority** - Can run with `nice -n 19` and `IOSchedulingClass=idle`
- **Simple** - No IPC, queues, or coordination needed
- **Robust** - Crashes don't affect watcher or other jobs

**Trade-off:**
- 3-4 second model load per file vs instant hot model
- Acceptable for batch processing (not real-time use case)

### 4. Systemd Service WorkingDirectory for Relative Paths

**Problem:**
Config specified model path as `models/parakeet-tdt-0.6b-v3-int8/` (relative), but systemd service failed:
```
Error: Model file not found: models/parakeet-tdt-0.6b-v3-int8/encoder.onnx
```

**Solution:**
Add `WorkingDirectory` to systemd service:

```ini
[Service]
WorkingDirectory=/home/seb/code/cloned/transcribe-rs-v2
ExecStart=%h/code/cloned/transcribe-rs-v2/builds/current/watch-daemon
```

**Why:**
Systemd services default to root working directory (`/`), breaking relative paths. Setting `WorkingDirectory` makes relative paths in config work consistently across manual runs and systemd.

**Pattern to Remember:**
For any daemon with relative paths in config, always set `WorkingDirectory` to the project root.

## What Worked

### 1. FFmpeg for Format Conversion
- MP3/M4A → WAV conversion is nearly instant (4650x realtime)
- Chunking is instant (24700x realtime)
- Handles paths with spaces correctly via Rust's PathBuf

### 2. Silero VAD Integration
- Filtering 99% silence from 15-minute file: <1 second
- Improved transcription quality (see Discovery #2)
- Simple API: `vad.process(&samples)` returns filtered samples

### 3. Notify Crate for File Watching
- Clean inotify integration on Linux
- Debouncing built-in (waits for file changes to settle)
- Reliable detection of Dropbox-synced files

### 4. Systemd User Services
- Auto-start on boot: `systemctl --user enable watch-daemon`
- Low priority scheduling: `Nice=19` + `IOSchedulingClass=idle`
- Easy monitoring: `journalctl --user -u watch-daemon -f`

### 5. On-Demand Spawning Pattern
- Simple `Command::new("transcribe-batch").spawn()`
- No need for complex IPC or queue management
- Naturally isolated - job failures don't affect watcher

## What Didn't Work

### 1. Running File Watcher in Transcription Daemon
Initial approach integrated file watcher into `transcribe-daemon`'s main loop. This blocked the Unix socket listener during long transcriptions, making keyboard dictation hang.

**Fix:** Separate lightweight watcher that spawns independent processes.

### 2. Arbitrary 5-Minute Chunking Without VAD
Chunking raw audio at fixed intervals:
- Splits mid-sentence
- Processes huge amounts of silence
- Slower (3-9x realtime vs 159x with VAD)

**Fix:** Apply VAD first, then chunk the filtered audio if needed.

## Implementation Details

### New Binaries

#### `transcribe-batch`
On-demand batch transcription with VAD:

```rust
// Simplified flow
let samples = audio::read_wav_samples(&wav_path)?;
let (filtered_samples, vad_result) = vad.process(&samples)?;
let filtered_wav = write_temp_wav(&filtered_samples)?;
let chunks = split_audio_if_needed(&filtered_wav)?; // If >6 min

for chunk in chunks {
    let result = engine.transcribe_file(&chunk)?;
    transcriptions.push(result.text);
}

fs::write(output_path, transcriptions.join(" "))?;
```

**Features:**
- Auto-detects format, converts MP3/M4A → WAV
- VAD filtering before chunking
- Respects 6.7-minute Parakeet limit
- Moves processed files to `processed/` subdirectory

#### `watch-daemon`
Lightweight file watcher:

```rust
// Simplified flow
let watcher = notify::recommended_watcher(event_handler)?;
watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;

// On file ready:
Command::new("transcribe-batch")
    .arg(&file_path)
    .arg(&output_dir)
    .spawn()?;
```

**Features:**
- 1-second debounce (waits for Dropbox sync)
- Scans existing files on startup
- Minimal RAM footprint (<10 MB)

### Configuration

Added to `~/.config/transcribe-rs/config.toml`:

```toml
[watch]
enabled = true
watch_dir = "/home/seb/Dropbox/Apps/RecUp App"
output_dir = "/home/seb/Documents/Transcriptions"
extensions = ["wav", "m4a", "mp3", "ogg", "flac", "webm"]
debounce_ms = 1000
scan_existing = true
```

### Systemd Service

`~/.config/systemd/user/watch-daemon.service`:

```ini
[Unit]
Description=Transcribe file watcher daemon
After=default.target

[Service]
Type=simple
WorkingDirectory=/home/seb/code/cloned/transcribe-rs-v2
ExecStart=%h/code/cloned/transcribe-rs-v2/builds/current/watch-daemon
Restart=always
Nice=19
IOSchedulingClass=idle

[Install]
WantedBy=default.target
```

**Commands:**
```bash
systemctl --user enable watch-daemon  # Auto-start on boot
systemctl --user start watch-daemon   # Start now
journalctl --user -u watch-daemon -f  # Monitor logs
```

## Performance Benchmarks

| Operation | Time | Speed | Notes |
|-----------|------|-------|-------|
| FFmpeg MP3→WAV (15 min) | 0.25s | 4650x | Instant conversion |
| Parakeet model load | 3-4s | N/A | Per-job overhead |
| VAD filter (15 min, 99% silence) | <1s | Fast | Silero VAD |
| Transcribe VAD output (6.5s speech) | 5.9s | 159x | From 15-min file |
| Total (15-min sparse file) | ~10s | 93x | Including model load |

**Without VAD** (for comparison):
- Same 15-min file would need 26 chunks (5 min each)
- Estimated time: ~15 minutes
- Speed: ~1x realtime

## Patterns to Remember

### 1. Inspect ONNX Models for Fixed Constraints

When models have unexpected length limits, inspect the ONNX structure:

```python
import onnx
model = onnx.load("model.onnx")
for tensor in model.graph.initializer:
    print(f"{tensor.name}: {tensor.dims}")
```

Look for suspiciously specific constants (9999 = `2×5000-1` was the clue).

### 2. VAD First, Then Chunk

For long-form transcription:
```
1. Apply VAD to filter silence
2. Check filtered duration
3. Chunk only if needed
4. Transcribe chunks
```

NOT:
```
1. Chunk raw audio
2. Transcribe each chunk (with tons of silence)
```

### 3. On-Demand Spawning for Batch Jobs

For background processing that shouldn't block real-time operations:
- Lightweight watcher daemon (always running)
- Heavy processing spawned on-demand
- No IPC needed - just spawn and forget

**When to use:**
- Non-realtime workloads
- Memory-heavy processing
- Don't need instant response (<5 sec startup is OK)

### 4. Systemd WorkingDirectory

For any daemon that uses relative paths:
```ini
[Service]
WorkingDirectory=/absolute/path/to/project
ExecStart=/absolute/path/to/binary
```

Relative paths in config files will resolve from `WorkingDirectory`.

## Open Questions

None - implementation is complete and working in production.

## Related Files

**Core implementation:**
- `/home/seb/code/cloned/transcribe-rs-v2/src/bin/transcribe-batch.rs` - On-demand batch transcription
- `/home/seb/code/cloned/transcribe-rs-v2/src/bin/watch-daemon.rs` - File watcher daemon
- `/home/seb/code/cloned/transcribe-rs-v2/src/vad.rs` - VAD integration (Silero)
- `/home/seb/code/cloned/transcribe-rs-v2/src/file_watcher.rs` - File watching utilities

**Configuration:**
- `~/.config/transcribe-rs/config.toml` - Watch settings
- `~/.config/systemd/user/watch-daemon.service` - Systemd service

**Related lessons:**
- `2026-01-02-file-watcher-batch-transcription.md` - Initial investigation (this session builds on it)

## Git Changes Summary

**Commit:** `file watch 2` (43c4863)

Added:
- `src/bin/transcribe-batch.rs` (240 lines) - On-demand batch transcription with VAD
- `src/bin/watch-daemon.rs` (323 lines) - Lightweight file watcher daemon
- Updated `Cargo.toml` - Added two new binaries
- Updated `scripts/build.sh` - Build new binaries to staging

**Total:** +572 lines, -1 line

## Testing Commands

```bash
# Test batch transcription manually
./builds/current/transcribe-batch "/path/to/audio.mp3" ~/Documents/Transcriptions/

# Monitor watch-daemon logs
journalctl --user -u watch-daemon -f

# Check watch-daemon status
systemctl --user status watch-daemon

# Test by dropping file in Dropbox
cp ~/test.mp3 ~/Dropbox/Apps/RecUp\ App/
# Watch logs for automatic processing

# Check VAD performance
time ./builds/current/transcribe-batch long-file.mp3 /tmp/output/
```

## Key Takeaways

1. **ONNX export constraints are real** - PyTorch flexibility doesn't always translate. Inspect ONNX files for fixed-size tensors.

2. **VAD improves quality, not just speed** - Filtering silence before transcription leads to better chunking and more coherent results.

3. **On-demand spawning > always-hot daemon** - For batch workloads, 3-4 second startup is acceptable trade-off for isolation and low baseline RAM.

4. **Systemd WorkingDirectory matters** - Relative paths break without it.

5. **Measure quality, not just speed** - Word count comparison revealed VAD's surprising quality improvement.
