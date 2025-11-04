# Harper Daemon Integration Plan

**Author**: Claude Code
**Date**: 2025-11-04
**Status**: Proposal - Awaiting Approval

## Executive Summary

Move Harper grammar/spell checking from the CLI process into the transcription daemon to eliminate the 300ms dictionary loading overhead on every transcription. This change will reduce typical transcription pipeline time from ~1000ms to ~700ms (30% improvement).

## Problem Statement

### Current Architecture
```
User PTT → CLI (transcribe start/stop)
           ↓
           transcribe-client (sends audio file path)
           ↓
           transcribe-daemon (Parakeet model cached in memory)
           ↓
           transcribe-client returns text
           ↓
           CLI applies Harper (loads dictionaries from scratch: 300ms)
           ↓
           CLI handles clipboard/paste
```

### Performance Bottleneck
- **Harper dictionary loading**: 300ms per transcription
- **Root cause**: Each `transcribe stop` is a new process, no state persists
- **Cache behavior**: Harper caching works (5-20ms after first load), but only within a single process
- **Impact**: 30% of total pipeline time wasted on repeated dictionary loads

### Why This Matters
From real usage logs, a typical transcription takes:
- 600ms: Transcription (daemon, cached model)
- **300ms: Harper dictionary load** ⚠️
- 20ms: Harper linting
- 50ms: Clipboard/paste operations

---

## Architecture Options

### Option A: Integrated Harper in Transcription Daemon (RECOMMENDED)

**Description**: Extend the existing `transcribe-daemon` to handle Harper processing.

**Architecture**:
```
User PTT → CLI (transcribe start/stop)
           ↓
           transcribe-client (sends audio file + processing flags)
           ↓
           transcribe-daemon
             • Parakeet model (cached)
             • Harper dictionaries (cached)
             • Processes: transcribe → Harper → return
           ↓
           CLI handles clipboard/paste only
```

**Pros**:
- ✅ Single daemon to manage
- ✅ Simpler IPC (one round-trip)
- ✅ Harper dictionary loaded once at daemon startup
- ✅ Natural request/response flow
- ✅ Easy to disable Harper (pass flag in request)
- ✅ Less complexity in systemd service management

**Cons**:
- ⚠️ Daemon becomes multi-purpose (transcription + text processing)
- ⚠️ Daemon restart required for user dictionary changes
- ⚠️ Slightly larger memory footprint (~50MB for Harper dictionaries)

**Performance Impact**:
- First transcription: ~800ms (one-time Harper load)
- Subsequent: ~650ms (350ms saved vs current implementation)

---

### Option B: Separate Harper Daemon

**Description**: Create a dedicated `harper-daemon` service alongside `transcribe-daemon`.

**Architecture**:
```
User PTT → CLI (transcribe start/stop)
           ↓
           transcribe-client (sends to transcription daemon)
           ↓
           transcribe-daemon (returns raw text)
           ↓
           harper-client (sends to Harper daemon)
           ↓
           harper-daemon (returns corrected text)
           ↓
           CLI handles clipboard/paste
```

**Pros**:
- ✅ Clean separation of concerns
- ✅ Can restart Harper daemon independently
- ✅ Easier to disable Harper entirely (don't start daemon)
- ✅ Could potentially be used by other applications

**Cons**:
- ❌ Two daemons to manage (systemd services, health checks)
- ❌ Two IPC round-trips (adds latency)
- ❌ More complex error handling (two failure points)
- ❌ More configuration (two socket paths)
- ❌ Overkill for a personal dictation tool

**Performance Impact**:
- Similar to Option A, but +10-20ms for extra IPC overhead

---

## Recommendation: Option A (Integrated)

**Rationale**:
1. **Simplicity**: transcribe-rs is a personal tool, not a distributed system
2. **Performance**: Eliminates IPC overhead, fewer moving parts
3. **User Experience**: Single daemon to start/stop, simpler troubleshooting
4. **Maintenance**: Less code to maintain, fewer potential failure modes

**When Option B makes sense**:
- If Harper needs to be shared across multiple applications
- If memory constraints require separating services
- If Harper updates are frequent and require isolated restarts

For a personal PTT dictation tool, the integrated approach is the clear winner.

---

## Implementation Design (Option A)

### 1. Protocol Changes

**Current Protocol**:
```json
// Request
{"file": "/tmp/ptt_current.wav"}

// Response
{"success": true, "text": "transcribed text"}
```

**New Protocol**:
```json
// Request
{
  "file": "/tmp/ptt_current.wav",
  "harper": {
    "enabled": true,
    "user_dict_path": "/home/user/.config/transcribe-rs/harper_dictionary.txt",
    "dialect": "American",
    "disabled_linters": ["AvoidCurses"],
    "save_corrections": true,
    "corrections_dir": "/home/user/.config/transcribe-rs/harper_corrections"
  },
  "transcription_corrections": {
    "enabled": true,
    "corrections_file": "/home/user/.config/transcribe-rs/transcription_corrections.json"
  }
}

// Response
{
  "success": true,
  "text": "corrected transcribed text",
  "processing": {
    "transcription_ms": 450,
    "harper_ms": 12,
    "corrections_applied": 2
  }
}
```

**Backward Compatibility**: Support both formats. If `harper` field is missing, skip Harper processing.

### 2. Daemon Architecture Changes

**File**: `src/bin/daemon.rs`

```rust
struct DaemonState {
    transcription_engine: ParakeetEngine,
    harper_dict_cache: Arc<FstDictionary>,  // Loaded once at startup
    config: Config,
}

impl DaemonState {
    fn new(config: Config) -> Result<Self, Box<dyn Error>> {
        // Load Parakeet model
        let mut engine = ParakeetEngine::new();
        engine.load_model(...)?;

        // Pre-load Harper dictionary (300ms one-time cost at daemon startup)
        eprintln!("📖 Loading Harper dictionaries...");
        let harper_dict = Arc::new(FstDictionary::curated());
        eprintln!("✓ Harper ready");

        Ok(DaemonState {
            transcription_engine: engine,
            harper_dict_cache: harper_dict,
            config,
        })
    }

    fn process_request(&mut self, request: ProcessRequest) -> ProcessResponse {
        let start = Instant::now();

        // 1. Transcribe audio
        let transcription = self.transcription_engine
            .transcribe_file(&request.file, None)?;
        let transcription_ms = start.elapsed().as_millis();

        let mut text = transcription.text;
        let mut corrections_applied = 0;

        // 2. Apply transcription corrections (phonetic fixes)
        if request.transcription_corrections.enabled {
            text = apply_corrections(&text, &request.transcription_corrections)?;
        }

        // 3. Apply Harper (using cached dictionary)
        let harper_start = Instant::now();
        if request.harper.enabled {
            let session = process_with_harper_cached(
                &text,
                &self.harper_dict_cache,  // Use cached dictionary
                &request.harper,
            )?;
            text = session.corrected_text;
            corrections_applied = session.corrections.len();

            // Save correction session if requested
            if request.harper.save_corrections {
                session.save_to_file(&request.harper.corrections_dir)?;
            }
        }
        let harper_ms = harper_start.elapsed().as_millis();

        ProcessResponse {
            success: true,
            text: Some(text),
            processing: Some(ProcessingMetrics {
                transcription_ms,
                harper_ms,
                corrections_applied,
            }),
            error: None,
        }
    }
}
```

### 3. Harper Module Changes

**File**: `src/harper_processor.rs`

Add new function that accepts pre-loaded dictionary:

```rust
/// Process text with Harper using a pre-loaded curated dictionary
/// This is used by the daemon to avoid reloading dictionaries on each request
pub fn process_with_harper_cached(
    text: &str,
    curated_dict: &Arc<FstDictionary>,
    config: &HarperRequestConfig,
) -> Result<CorrectionSession, Box<dyn Error>> {
    // Load user dictionary (small, fast)
    let user_dict = load_user_dictionary(&config.user_dict_path)?;

    // Merge with pre-loaded curated dictionary
    let mut merged_dict = MergedDictionary::new();
    merged_dict.add_dictionary(curated_dict.clone());  // Already loaded
    merged_dict.add_dictionary(Arc::new(user_dict));

    // Rest of processing logic unchanged...
}
```

### 4. Client Changes

**File**: `src/bin/client.rs`

Update to send full processing request:

```rust
fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::load()?;
    let audio_file = parse_args();

    // Build processing request
    let request = ProcessRequest {
        file: audio_file,
        harper: HarperRequestConfig {
            enabled: config.harper.enabled,
            user_dict_path: config.harper.dictionary_path,
            dialect: config.harper.dialect,
            disabled_linters: config.harper.disabled_linters,
            save_corrections: true,
            corrections_dir: config.harper.corrections_dir,
        },
        transcription_corrections: TranscriptionCorrectionsConfig {
            enabled: config.transcription_corrections.enabled,
            corrections_file: config.transcription_corrections.corrections_file,
        },
    };

    // Send to daemon and print result
    let response = send_request(&config.daemon.socket_path, &request)?;
    println!("{}", response.text.unwrap_or_default());
    Ok(())
}
```

### 5. CLI Changes

**File**: `src/bin/cli.rs`

Simplify `handle_stop()` - remove Harper processing since daemon handles it:

```rust
fn handle_stop(config: &Config) -> Result<(), Box<dyn Error>> {
    // ... recording stop logic ...

    // Transcribe (daemon now handles Harper)
    let transcription = transcribe_file(&audio_file, &config.audio.log_file)?;

    // Add trailing space (still in CLI for simplicity)
    let text = clipboard::add_trailing_space_after_punctuation(&transcription);

    // Clipboard and paste
    clipboard::copy_to_clipboard(&text)?;
    paste::paste_from_clipboard()?;

    Ok(())
}
```

---

## Implementation Phases

### Phase 1: Protocol Foundation (2-3 hours)
**Goal**: Extend protocol without breaking existing functionality

1. Define new request/response structs in `src/lib.rs`:
   - `ProcessRequest` (superset of `TranscribeRequest`)
   - `ProcessResponse` (superset of `TranscribeResponse`)
   - `HarperRequestConfig`
   - `ProcessingMetrics`

2. Update daemon to accept both old and new request formats:
   ```rust
   // Try new format first, fall back to old format
   let request = serde_json::from_str::<ProcessRequest>(&line)
       .or_else(|_| serde_json::from_str::<TranscribeRequest>(&line))?;
   ```

3. Add unit tests for protocol serialization/deserialization

**Success Criteria**: Existing clients continue to work with updated daemon

### Phase 2: Daemon Integration (3-4 hours)
**Goal**: Move Harper processing into daemon

1. Add Harper dependencies to daemon binary (already in workspace)

2. Create `DaemonState` struct to hold cached dictionaries:
   - Load Harper dictionary at startup (log time taken)
   - Handle user dictionary reloading on SIGHUP (optional enhancement)

3. Implement `process_request()` with full pipeline:
   - Transcription
   - Transcription corrections
   - Harper processing (with cached dictionary)
   - Timing metrics

4. Update `handle_client()` to use new architecture

5. Add error handling for Harper failures (should not crash daemon)

**Success Criteria**:
- Daemon starts and loads dictionaries successfully
- First transcription takes ~800ms (one-time Harper load)
- Subsequent transcriptions take ~650ms
- Daemon handles Harper errors gracefully

### Phase 3: Client Update (1-2 hours)
**Goal**: Update client to send full processing requests

1. Update `transcribe-client` to build `ProcessRequest` from config

2. Add command-line flags for testing:
   - `--no-harper`: Skip Harper processing
   - `--timing`: Print processing metrics

3. Update error messages to reflect new failure modes

**Success Criteria**: Client sends full config and receives processed text

### Phase 4: CLI Simplification (1-2 hours)
**Goal**: Remove duplicate Harper code from CLI

1. Remove Harper processing from `src/bin/cli.rs`

2. Remove Harper-related logging (now in daemon)

3. Update performance logging to use metrics from daemon response

4. Keep transcription corrections in CLI initially (move later if needed)

**Success Criteria**:
- CLI is simpler and faster
- No functional regression
- Performance metrics still logged correctly

### Phase 5: Testing & Validation (2-3 hours)
**Goal**: Verify correctness and performance

1. Update integration tests:
   - Test daemon with Harper enabled/disabled
   - Test backward compatibility with old clients
   - Test error scenarios (missing dictionary, etc.)

2. Update performance test to use daemon:
   ```rust
   // First request: 800ms (one-time load)
   assert!(first_ms < 900);

   // Subsequent: 650ms (cached)
   assert!(subsequent_ms < 700);
   ```

3. Manual testing:
   - Restart daemon, measure startup time
   - Run 10 consecutive transcriptions, verify caching
   - Test with Harper disabled
   - Verify correction sessions are saved correctly

4. Update documentation:
   - README: Mention Harper runs in daemon
   - CLAUDE.md: Update architecture diagram
   - Add troubleshooting section for daemon logs

**Success Criteria**:
- All tests pass
- Performance targets met (see below)
- Documentation updated

### Phase 6: Deployment (1 hour)
**Goal**: Roll out to production (user's system)

1. Build release binaries: `cargo build --release`

2. Restart daemon: `systemctl --user restart transcribe-daemon`

3. Monitor logs: `journalctl --user -u transcribe-daemon -f`

4. Run test transcriptions, verify performance

5. Check for any errors or warnings

**Success Criteria**: User's system running updated daemon with Harper integrated

---

## Performance Targets

### Current (Baseline)
- Total: 1000ms
  - Transcription: 600ms (daemon)
  - Harper load: 300ms (CLI, every time)
  - Harper lint: 20ms (CLI)
  - Clipboard: 20ms (CLI)
  - Paste: 60ms (CLI)

### Target (After Integration)
- **Daemon startup**: 3000ms (one-time)
  - Parakeet load: 2500ms
  - Harper load: 300ms (curated dictionary)
  - Socket setup: 200ms

- **First transcription after daemon start**: 700ms
  - Transcription: 600ms
  - Harper (cached): 20ms
  - Clipboard: 20ms
  - Paste: 60ms

- **Subsequent transcriptions**: 700ms (same as first)
  - Harper dictionary cache persists
  - No per-transcription penalty

### Performance Improvement
- **Per-transcription savings**: 300ms → **30% faster**
- **User experience**: Imperceptible difference (<1s is "instant")
- **Daemon startup cost**: Acceptable (one-time, ~3 seconds)

---

## Risk Analysis

### Risk 1: Daemon Memory Usage
**Concern**: Harper dictionaries consume ~50MB RAM

**Mitigation**:
- Acceptable for modern systems (daemon already uses ~200MB)
- Total daemon footprint: ~250MB (trivial on 16GB+ systems)
- Monitor with `ps aux | grep transcribe-daemon`

**Likelihood**: Low
**Impact**: Low

### Risk 2: Daemon Restart Required for Dictionary Changes
**Concern**: User updates harper_dictionary.txt, changes not reflected until restart

**Mitigation**:
- Phase 1: Document that daemon restart is needed
- Phase 2 (future): Implement SIGHUP handler to reload user dictionary
- User dictionary is small (~100 words), loads in <1ms
- Most users rarely change dictionary

**Likelihood**: Medium
**Impact**: Low (easy workaround: restart daemon)

### Risk 3: Harper Errors Crash Daemon
**Concern**: A Harper bug could take down transcription service

**Mitigation**:
- Wrap all Harper calls in `catch_unwind()` or proper error handling
- If Harper fails, return raw transcription (graceful degradation)
- Add daemon health monitoring
- systemd restarts daemon automatically on crash

**Likelihood**: Low (Harper is stable)
**Impact**: Medium (temporary loss of transcription)

### Risk 4: Backward Compatibility
**Concern**: Old clients break with new daemon

**Mitigation**:
- Support both old and new request formats
- Test with old `transcribe-client` binary
- Deployment plan: update daemon first, then clients

**Likelihood**: Low (protocol designed for compatibility)
**Impact**: High (broken user workflow)

### Risk 5: Increased Complexity
**Concern**: Daemon becomes harder to debug

**Mitigation**:
- Add verbose logging for Harper operations
- Include timing metrics in responses
- Add `--debug` flag to daemon for detailed output
- Update troubleshooting docs

**Likelihood**: Medium
**Impact**: Low (manageable with good logging)

---

## Testing Strategy

### Unit Tests
```rust
// src/bin/daemon.rs
#[cfg(test)]
mod tests {
    #[test]
    fn test_protocol_backward_compatibility() {
        // Old format still works
    }

    #[test]
    fn test_harper_caching() {
        // Dictionary loaded once
    }

    #[test]
    fn test_harper_disabled() {
        // Skips Harper when disabled
    }

    #[test]
    fn test_harper_error_handling() {
        // Graceful degradation on Harper failure
    }
}
```

### Integration Tests
```rust
// tests/daemon_integration.rs
#[test]
fn test_full_pipeline_with_harper() {
    // Start daemon, send request, verify corrected text
}

#[test]
fn test_performance_caching() {
    // First request: ~800ms
    // Second request: ~650ms (Harper cached)
}

#[test]
fn test_correction_sessions_saved() {
    // Verify JSON files created in corrections_dir
}
```

### Performance Tests
```bash
# Measure daemon startup
time transcribe-daemon &

# Measure first transcription
time transcribe stop

# Measure subsequent transcriptions (10x)
for i in {1..10}; do
    transcribe start
    sleep 2
    time transcribe stop
done
```

### Manual Testing Checklist
- [ ] Daemon starts successfully with Harper enabled
- [ ] Daemon logs show "Harper ready" message
- [ ] First transcription completes in <800ms
- [ ] Subsequent transcriptions in <700ms
- [ ] Harper corrections are applied (verify with intentional typo)
- [ ] Correction sessions saved to `~/.config/transcribe-rs/harper_corrections/`
- [ ] Daemon restarts cleanly (`systemctl --user restart`)
- [ ] Works with Harper disabled (`harper.enabled = false` in config)
- [ ] Old transcribe-client still works (backward compatibility)
- [ ] Error messages are clear and helpful
- [ ] Memory usage stable (<300MB)

---

## Rollback Plan

If integration causes issues:

1. **Immediate Rollback** (< 5 minutes):
   ```bash
   # Restore previous binary
   cp ~/backups/transcribe-daemon-old ./target/release/transcribe-daemon
   systemctl --user restart transcribe-daemon
   ```

2. **Git Rollback**:
   ```bash
   git revert <commit-hash>
   cargo build --release
   systemctl --user restart transcribe-daemon
   ```

3. **Fallback Mode**: If daemon is broken, use direct Parakeet transcription:
   ```bash
   # Temporary workaround (slow but works)
   # Run transcription in CLI process instead of daemon
   ```

---

## Future Enhancements (Post-Integration)

### 1. Lazy Harper Loading
Load Harper dictionary on first use, not at startup:
- Faster daemon startup (300ms saved)
- First transcription pays the cost
- Overall same performance after first use

### 2. SIGHUP Handler for Dictionary Reload
```bash
# Reload user dictionary without restarting daemon
kill -SIGHUP $(pgrep transcribe-daemon)
```

### 3. Parallel Processing
Process Harper and transcription corrections concurrently:
```rust
let (corrected_text, harper_result) = tokio::join!(
    apply_corrections(text),
    process_with_harper(text)
);
```
Potential savings: 10-20ms (if corrections are slow)

### 4. Harper Correction Learning
Track frequently corrected words and add to user dictionary automatically:
```bash
# After 3 corrections of "LLM" → suggest adding to dictionary
```

### 5. Multi-Threaded Daemon
Handle multiple transcription requests concurrently:
- Useful if multiple users or applications share daemon
- Currently single-threaded (sufficient for personal use)

### 6. WebSocket Protocol
Upgrade from line-delimited JSON to WebSocket for streaming results:
- Stream transcription as it completes
- Stream Harper corrections in real-time
- Better for long audio files

---

## Success Metrics

### Performance
- ✅ Daemon startup: < 4 seconds
- ✅ First transcription: < 800ms
- ✅ Subsequent transcriptions: < 700ms
- ✅ Memory usage: < 300MB

### Reliability
- ✅ Zero crashes in 100 consecutive transcriptions
- ✅ Graceful handling of Harper errors
- ✅ Backward compatible with old clients

### User Experience
- ✅ No perceived latency increase (<1s is "instant")
- ✅ Clear error messages
- ✅ Easy to disable Harper if needed
- ✅ Daemon restarts cleanly

---

## Open Questions

1. **Should transcription_corrections also move to daemon?**
   - Pro: Complete processing in one place
   - Con: Adds more complexity to daemon
   - **Recommendation**: Move in Phase 2, keep initial scope small

2. **Should we support dynamic dictionary reloading?**
   - Pro: No daemon restart needed
   - Con: Adds complexity (SIGHUP handler, file watching)
   - **Recommendation**: Add as future enhancement (Phase 2)

3. **Should we expose processing metrics in CLI output?**
   - Pro: Helps users understand performance
   - Con: Clutters output
   - **Recommendation**: Add `--verbose` flag to show metrics

4. **Should we support multiple dialects simultaneously?**
   - Pro: Users can switch without restarting daemon
   - Con: 4x memory usage (one dictionary per dialect)
   - **Recommendation**: Not needed, single dialect per user is typical

---

## Conclusion

Moving Harper to the daemon is a high-value, medium-complexity change that will deliver a **30% performance improvement** with acceptable trade-offs. The integrated approach (Option A) is recommended for its simplicity and maintainability.

**Estimated Total Effort**: 12-16 hours over 6 phases
**Expected Performance Gain**: 300ms per transcription (30% faster)
**User Impact**: Minimal (daemon restart required, otherwise transparent)

**Recommendation**: Proceed with Option A implementation.

---

## Approval Checklist

Before implementation, confirm:
- [ ] Architecture approach (Option A: Integrated) approved
- [ ] Protocol changes reviewed and acceptable
- [ ] Performance targets are sufficient
- [ ] Risk mitigations are adequate
- [ ] Testing strategy is comprehensive
- [ ] Timeline and effort estimate are reasonable

**Next Steps**: Await user approval to begin Phase 1 implementation.
