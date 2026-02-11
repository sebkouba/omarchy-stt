# Implementation Plan: Streaming Transcription During Recording

## Goal

Transcribe audio incrementally while the user is still speaking, so that when they release the hotkey, the result is nearly instant. Currently all transcription happens after recording stops (~150ms+ latency). With streaming, most audio is already transcribed by release time.

## Architecture Overview

```
Current:  Press → [record...] → Release → extract → write WAV → transcribe → paste
                                           ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
                                                    all latency here

Proposed: Press → [record + transcribe every 1s...] → Release → final transcribe (delta only) → paste
                                                       ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
                                                        ~10-30ms (last chunk only)
```

Three components need changes. No new binaries or crates required.

---

## Step 1: recording-daemon — Add `read_current` Command

**File:** `src/bin/recording-daemon.rs`

**What:** Add a new socket command that returns the audio accumulated since `start` without stopping the recording. The circular buffer already supports arbitrary range extraction via `extract_range()`.

**Socket protocol:**

```json
// Request
{"command": "read_current", "start_index": 12345}

// Response
{
  "ok": true,
  "samples": 48000,
  "duration_ms": 3000,
  "wav_path": "/tmp/ptt_streaming.wav"
}
```

**Implementation:**

1. Add a `handle_read_current(request, state) -> Value` function alongside existing `handle_start`/`handle_stop`/`handle_cancel`
2. Extract samples from `start_index` to the buffer's current `get_current_index()`
3. Write to a separate WAV path (`/tmp/ptt_streaming.wav`) to avoid clobbering the final recording file
4. Do NOT apply VAD (streaming reads are short, VAD adds latency, and it's only useful for audio > 20s)
5. Do NOT reset recording state — recording continues

**Client-side** (`src/recording.rs`): Add `pub fn read_current(config: &AudioConfig, start_index: usize) -> Result<StreamingReadResult>` that sends the command and returns the WAV path + sample count.

```rust
pub struct StreamingReadResult {
    pub wav_path: PathBuf,
    pub samples: u64,
    pub duration_ms: u64,
}
```

**Risk:** Minimal. Purely additive, no changes to existing commands.

---

## Step 2: transcribe-daemon — Add Request Serialization

**File:** `src/bin/daemon.rs`

**What:** The daemon currently handles one client at a time (sequential accept loop). Streaming will send requests every ~1s while the hotkey-daemon might also send a final request at release. The ONNX session is not thread-safe for concurrent inference.

**Implementation:**

Add a `tokio::sync::Mutex` around the `ParakeetEngine` in `DaemonState`:

```rust
struct DaemonState {
    transcription_engine: tokio::sync::Mutex<ParakeetEngine>,
}
```

Each `handle_client` acquires the lock before calling `transcription_engine.transcribe()`. This ensures streaming requests and the final transcription never overlap. If a streaming request is in-flight when the final request arrives, the final request simply waits for the lock (adds at most one streaming inference latency, which is fine since the final result will be near-identical anyway).

**Alternative:** Use a channel-based request queue. More complex, same result. Mutex is simpler and sufficient.

**Risk:** Low. The daemon already processes requests sequentially; this just makes it explicit and safe under concurrent connections.

---

## Step 3: hotkey-daemon — Add Streaming Loop

**File:** `src/bin/hotkey-daemon.rs`

**What:** When `StartRecording` action fires, spawn a background task that periodically reads the growing buffer, transcribes it, and stores the latest partial result. When `StopAndTranscribe` fires, use the last streaming result as a fast path or run one final transcription on the delta.

### 3a: Streaming State

Add shared state for the streaming loop:

```rust
struct StreamingState {
    latest_text: String,
    latest_word_count: usize,
    start_index: usize,
}

// Shared between streaming task and main event loop
type SharedStreamingState = Arc<tokio::sync::Mutex<StreamingState>>;
```

### 3b: Streaming Loop (spawned on StartRecording)

```rust
async fn run_streaming_loop(
    config: AudioConfig,
    start_index: usize,
    state: SharedStreamingState,
    cancel: CancellationToken,
) {
    let interval = Duration::from_millis(1000); // 1s between transcriptions
    let min_samples = 8000; // 0.5s minimum before first attempt

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(interval) => {}
        }

        // Read current buffer snapshot
        let read_result = match recording::read_current(&config, start_index) {
            Ok(r) => r,
            Err(_) => continue,
        };

        if read_result.samples < min_samples {
            continue;
        }

        // Transcribe via daemon (blocks until mutex available)
        let text = match transcribe_file(&read_result.wav_path) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Update shared state
        let mut s = state.lock().await;
        s.latest_text = text.trim().to_string();
        s.latest_word_count = s.latest_text.split_whitespace().count();
    }
}
```

### 3c: Modified StopAndTranscribe Flow

When `StopAndTranscribe` fires:

1. Cancel the streaming loop (`cancel_token.cancel()`)
2. Call `stop_recording()` as before (extracts final audio, writes WAV)
3. **Fast path check:** If the streaming state's `latest_word_count > 0` and the final audio is only slightly longer than what was last streamed (< 1.5s delta), use the streaming result directly — skip final transcription
4. **Full path:** Otherwise, transcribe the final WAV as before (but this is now rare — only for very short recordings where no streaming pass completed)
5. Continue with corrections, LLM enhancement, and pasting as before

```rust
// Pseudocode for the decision
let streaming = streaming_state.lock().await;
let final_result = stop_recording(&config)?;

let text = if streaming.latest_word_count > 0
    && (final_result.duration_ms - last_streamed_duration_ms) < 1500
{
    // Fast path: streaming result is close enough
    // Optionally: transcribe just the last 1.5s and append
    streaming.latest_text.clone()
} else {
    // Full transcription
    transcribe_file(&final_result.audio_file)?
};
```

**Refinement option:** Instead of the fast path, always run the final transcription but display/use the streaming result immediately while the final one confirms. For a CLI tool without a preview UI, the fast path is simpler and the accuracy difference is negligible.

### 3d: CancellationToken

Use `tokio_util::sync::CancellationToken` (already available via tokio ecosystem). Add to `Cargo.toml`:

```toml
tokio-util = { version = "0.7", features = ["rt"] }
```

**Risk:** Medium. This is the most complex change. Key concern is ensuring the streaming task is always cancelled before `stop_recording` is called (otherwise the streaming loop might try to `read_current` after recording has stopped).

---

## What NOT to Change

- **Circular buffer** (`circular_buffer.rs`): Already has `extract_range()` and `get_current_index()`. No changes needed.
- **VAD** (`vad.rs`): Only applies to final recording (audio > 20s). Streaming reads are short, skip VAD.
- **State machine** (`hotkey_state.rs`): Pure state machine, action-based. Streaming is an implementation detail of how `StartRecording` and `StopAndTranscribe` actions are executed. No new states or transitions needed.
- **Clipboard/pasting** (`clipboard.rs`, `paste.rs`): Unchanged, receives final text as before.
- **LLM enhancement**: Unchanged, runs after final text is determined.

---

## Dependency Changes

```toml
# Add to Cargo.toml [dependencies]
tokio-util = { version = "0.7", features = ["rt"] }
```

No other new dependencies. `tokio::sync::Mutex` is already available from the existing `tokio` dependency.

---

## Testing Strategy

1. **Unit test `read_current`:** Start recording, write known samples, call `read_current`, verify WAV contains expected data
2. **Integration test serialization:** Send two concurrent transcription requests to the daemon, verify both complete without crash
3. **Manual test:** Hold hotkey for 5s, release. Compare latency with and without streaming (should go from ~150ms to ~30ms perceived)
4. **Edge cases:**
   - Very short press (< 0.5s): No streaming pass completes, falls through to full transcription — same as current behavior
   - Very long press (> 2 min): Buffer wraps, `read_current` returns available range — works correctly due to circular buffer design
   - Cancel during recording: Streaming loop cancelled, no transcription attempted — correct

---

## Expected Performance Impact

| Metric | Before | After |
|--------|--------|-------|
| Latency at release (3s recording) | ~150ms | ~30ms (last delta only) |
| Latency at release (10s recording) | ~400ms | ~30ms |
| CPU during recording | ~0% (just buffering) | ~15% (periodic inference) |
| Memory | No change | +one WAV file in /tmp (~100KB) |

The tradeoff is clear: burn some CPU during recording to eliminate virtually all perceived latency at release time.
