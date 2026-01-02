# File Watcher Batch Transcription

**Date:** 2026-01-02
**Status:** Partially implemented, disabled pending architectural changes

## Goal

Watch a directory (Dropbox sync folder) for new audio files (MP3, M4A, etc.), transcribe them using the local Parakeet model, and output text files to a separate directory.

## What Was Implemented

### 1. WatchConfig with output_dir option (`src/config.rs`)

```toml
[watch]
enabled = false  # Currently disabled
watch_dir = "/home/seb/Dropbox/Apps/RecUp App"
output_dir = "/home/seb/Documents/Transcriptions"
extensions = ["wav", "m4a", "mp3", "ogg", "flac", "webm"]
debounce_ms = 1000
scan_existing = true  # Process existing files on startup
```

### 2. FileWatcher with startup scan (`src/file_watcher.rs`)

- Uses `notify` crate for filesystem events (inotify on Linux)
- Debouncing to wait for Dropbox sync to complete
- Scans existing files on startup (new `scan_existing` config option)
- Converts non-WAV formats to WAV via FFmpeg
- Splits long audio (>5 min) into chunks due to Parakeet's ~6 min limit
- Moves processed files to `watch_dir/processed/` subdirectory

### 3. Integration in transcription daemon (`src/bin/daemon.rs`)

- `handle_watch_event()` function processes FileReady events
- Writes transcriptions to `output_dir` (creates if needed)
- Archives original files to `processed/` subdirectory

## What Worked

1. **File detection** - notify crate works well, debouncing handles Dropbox sync
2. **Format conversion** - FFmpeg MP3→WAV is instant (4650x realtime)
3. **Chunk splitting** - FFmpeg chunk extraction is instant (24700x realtime)
4. **Separate output directory** - Clean separation of input/output
5. **Startup scan** - Processes existing files when daemon starts
6. **Path with spaces** - Rust's PathBuf handles spaces correctly

## What Didn't Work

### 1. Blocks keyboard dictation (CRITICAL)

The file watcher runs in the transcription daemon's main loop. When processing long files, it blocks the socket handler, preventing keyboard dictation from working.

```
User presses hotkey → recording-daemon captures audio
                    → tries to connect to transcribe-daemon socket
                    → transcribe-daemon is busy transcribing file watcher chunk
                    → keyboard dictation hangs or times out
```

### 2. No VAD applied (MAJOR)

The file watcher path bypasses VAD entirely:

| Path | VAD Applied? | Speed |
|------|--------------|-------|
| Keyboard dictation | Yes (in recording-daemon) | 20-30x realtime |
| File watcher | No | 3-9x realtime |

For long recordings with pauses/silence, this means transcribing 2+ hours of audio when only 30 minutes contains speech.

### 3. Slow transcription under sustained load

- Cold Parakeet: ~9x realtime (34 sec for 5 min audio)
- Hot/throttled Parakeet: ~3.6x realtime (82 sec for 5 min audio)

A 2-hour recording with 26 chunks took 40 minutes to transcribe, during which keyboard dictation was blocked.

### 4. Audio library limitations

The codebase uses `hound` for audio I/O, which only supports WAV. MP3/M4A require FFmpeg conversion. This is fine (FFmpeg is fast) but adds a dependency.

## Architecture Analysis

### Current Flow (Problematic)

```
transcribe-daemon (single process)
├── Socket listener (keyboard dictation)
├── File watcher thread
└── Parakeet model (shared, single-threaded inference)

Problem: File watcher calls transcribe_file() directly,
blocking the socket listener.
```

### Keyboard Dictation Flow (Works Well)

```
recording-daemon
├── Circular buffer (continuous recording)
├── VAD processing (Silero VAD)
└── Writes filtered WAV

transcribe-daemon
├── Receives WAV path via socket
└── Transcribes (short, VAD-filtered audio = fast)
```

## Proposed Solution: Separate watch-daemon

### Option A: Dedicated watch-daemon with own model (~1.8GB extra RAM)

```
watch-daemon (new binary)
├── File watcher
├── Own Parakeet model instance
├── VAD processing
└── Writes to output_dir

transcribe-daemon (unchanged)
├── Socket listener only
└── Keyboard dictation priority
```

**Pros:** True parallelism, no interference
**Cons:** Doubles RAM usage (~3.6GB total for both daemons)

### Option B: watch-daemon queues to transcribe-daemon

```
watch-daemon (new binary)
├── File watcher
├── Converts/splits audio
├── Applies VAD
└── Queues jobs to transcribe-daemon socket

transcribe-daemon (modified)
├── Socket listener
├── Priority queue (keyboard > file jobs)
└── Processes queue when idle
```

**Pros:** Shares model, less RAM
**Cons:** Still blocks during transcription, needs priority queue

### Option C: On-demand subprocess (Recommended for simplicity)

```
File appears in watch_dir
    ↓
Lightweight watcher script detects it
    ↓
Spawns: transcribe-batch <file> (new binary)
    ↓
transcribe-batch:
  - Loads own Parakeet model
  - Applies VAD
  - Transcribes
  - Writes output
  - Exits (frees RAM)
```

**Pros:**
- Only uses RAM when processing
- No interference with keyboard dictation
- Can run with `nice -n 19` for low priority
- Simple to implement

**Cons:**
- Model load time (3-4 sec) per file
- Not suitable for real-time use

## Implementation Plan for Option C

### 1. Create `transcribe-batch` binary

```rust
// src/bin/transcribe-batch.rs
fn main() {
    let input_path = args[1];
    let output_dir = args[2];

    // Load model
    let engine = ParakeetEngine::new();
    engine.load_model(&model_path)?;

    // Load VAD
    let vad = VadManager::new(&vad_config)?;

    // Convert to WAV if needed
    let wav_path = convert_to_wav(&input_path)?;

    // Read and apply VAD
    let samples = audio::read_wav_samples(&wav_path)?;
    let filtered = vad.process_f32(&samples)?;

    // Write filtered audio
    let filtered_path = write_temp_wav(&filtered)?;

    // Transcribe (may need chunking if >6 min after VAD)
    let result = engine.transcribe_file(&filtered_path, None)?;

    // Write output
    let output_path = output_dir.join(format!("{}.txt", stem));
    fs::write(&output_path, &result.text)?;

    // Move original to processed/
    fs::rename(&input_path, processed_dir.join(filename))?;
}
```

### 2. Create watcher script or lightweight daemon

```bash
#!/bin/bash
# watch-transcribe.sh
inotifywait -m -e close_write "$WATCH_DIR" | while read dir event file; do
    if [[ "$file" =~ \.(mp3|m4a|wav|ogg|flac)$ ]]; then
        nice -n 19 transcribe-batch "$dir$file" "$OUTPUT_DIR" &
    fi
done
```

Or as a Rust binary using notify crate (lighter than full daemon).

### 3. Systemd service

```ini
[Unit]
Description=Transcribe file watcher
After=network.target

[Service]
Type=simple
ExecStart=/path/to/watch-transcribe
Restart=always
Nice=19
IOSchedulingClass=idle

[Install]
WantedBy=default.target
```

## Key Files

| File | Purpose |
|------|---------|
| `src/config.rs` | WatchConfig struct with output_dir |
| `src/file_watcher.rs` | FileWatcher, chunk splitting, format conversion |
| `src/bin/daemon.rs` | handle_watch_event() integration (currently disabled) |
| `src/vad.rs` | VadManager using Silero VAD |

## Performance Benchmarks

| Operation | Time | Speed |
|-----------|------|-------|
| FFmpeg MP3→WAV (15 min) | 0.25 sec | 4650x |
| FFmpeg chunk split (5 min) | 0.05 sec | 24700x |
| Parakeet 5-min chunk (cold) | 34 sec | 9x |
| Parakeet 5-min chunk (throttled) | 82 sec | 3.6x |
| VAD processing | ~1 sec/min | Fast |

## Cloud Alternative

For long-form audio, cloud APIs are significantly faster:

| Service | Speed | Cost |
|---------|-------|------|
| Groq Whisper | ~100x realtime | Free tier available |
| Deepgram | ~100x realtime | Pay per minute |
| OpenAI Whisper | ~50x realtime | Pay per minute |

Consider: `transcribe-batch --backend groq` for files over a certain length.

## Testing Commands

```bash
# Test FFmpeg conversion speed
time ffmpeg -y -i input.mp3 -ar 16000 -ac 1 -f wav output.wav

# Test chunk extraction speed
time ffmpeg -y -i input.wav -ss 0 -t 300 -c copy chunk.wav

# Test transcription speed
time ./builds/current/transcribe-client test.wav

# Check daemon logs
journalctl --user -u transcribe-daemon -f

# Manually trigger transcription
./builds/current/transcribe-client /path/to/audio.wav
```

## Current Status

- **File watcher**: Implemented but disabled (`enabled = false`)
- **Keyboard dictation**: Working normally
- **Batch transcription**: Needs separate daemon/process (not yet implemented)
- **VAD for files**: Not implemented in file watcher path

## Next Steps

1. Implement `transcribe-batch` binary with VAD
2. Create lightweight watcher (script or minimal daemon)
3. Add systemd service with nice/ionice for low priority
4. Consider cloud backend option for very long files
5. Re-enable file watcher config once separate process is ready
