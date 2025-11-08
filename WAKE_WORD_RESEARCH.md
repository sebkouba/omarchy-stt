# Wake Word Detection Research for transcribe-rs-v2

**Research Date:** 2025-11-08
**Branch:** `claude/research-rust-wake-word-011CUvHvuwAoy5nyN4CsRLor`

## Overview

This document summarizes research into Rust-based wake word detection libraries suitable for adding voice-activated dictation to transcribe-rs-v2. The goal is to replace keyboard-based push-to-talk with voice commands like "start dictation" and "stop dictation".

## Requirements

- **Simplicity:** Easy integration with existing Rust codebase
- **Performance:** Low latency, minimal CPU/memory overhead
- **Dual Detection:** Support for both start and stop wake words
- **Platform:** Linux/Wayland compatibility
- **License:** Open-source preferred

---

## Available Rust Wake Word Detection Crates

### 1. **oww-rs** (Recommended)

**Repository:** Based on [OpenWakeWord](https://github.com/dscripka/openWakeWord)
**Crate:** [oww-rs](https://crates.io/crates/oww-rs) (v0.0.1, October 2025)
**License:** Open-source (Apache-2.0)

#### Strengths

✅ **Pre-trained models available** - No need to record training samples
✅ **High accuracy** - >99% accuracy claimed, <0.5 false positives/hour, <5% false reject rate
✅ **Lightweight** - Single Raspberry Pi 3 core can run 15-20 models simultaneously
✅ **Pure Rust** - Uses ONNX Runtime (`ort` crate) for inference
✅ **Recently updated** - Active development (October 2025)
✅ **Multiple wake words** - Pre-trained models include "alexa", "hey mycroft", "hey jarvis", "hey rhasspy"
✅ **Simple integration** - Minimalistic API for ONNX model inference

#### Technical Details

- **Audio Format:** 16kHz PCM, 80ms frames
- **Inference Engine:** ONNX Runtime (already used in your Parakeet engine!)
- **Models:** Small ONNX files (~few MB each)
- **Dependencies:** `ort` crate (same as Parakeet engine), `cpal` for audio input
- **Architecture:** Melspectrogram → Google speech embedding model → classification layer

#### Performance Characteristics

- **CPU Usage:** Very efficient - multiple models run in real-time on modest hardware
- **Memory:** Lightweight model files
- **Latency:** Processes 80ms audio frames
- **Accuracy:** Comparable to commercial solutions like Picovoice Porcupine

#### Limitations

⚠️ English-only support
⚠️ Not suitable for ultra-low-power microcontrollers
⚠️ Requires ONNX Runtime dependency (but you already have this!)

#### Integration Approach

1. Add `oww-rs` and `cpal` dependencies to `Cargo.toml`
2. Download pre-trained ONNX models for start/stop words
3. Create new module `src/wake_word.rs` for detection logic
4. Run two models simultaneously: one for "start dictation", one for "stop dictation"
5. Replace hotkey triggers in `src/bin/cli.rs` with wake word callbacks

---

### 2. **rustpotter**

**Repository:** [github.com/GiviMAD/rustpotter](https://github.com/GiviMAD/rustpotter)
**License:** Apache-2.0

#### Strengths

✅ **Pure Rust** - Fully native implementation
✅ **Two detection methods** - Wakeword references (DTW) or neural network models
✅ **Cross-platform** - Windows, macOS, Linux binaries
✅ **Customizable** - Train your own wake words

#### Technical Details

- **Audio Format:** Internally converts to PCM 32-bit float, mono, 16kHz
- **Detection Methods:**
  - **Wakeword References:** Dynamic Time Warping, requires 3-8 sample recordings
  - **Wakeword Models:** Neural networks, requires tagged training data
- **Model Sizes:** Tiny (320KB) to Large (3.1MB)
- **Processing:** Generates MFCCs for each 10ms of audio

#### Limitations

⚠️ **Requires manual training** - No pre-trained models, must record samples
⚠️ **More complex setup** - Need to create training data
⚠️ **Less documented** - Smaller community than OpenWakeWord

#### Integration Approach

Would require:
1. Recording 3-8 samples of yourself saying "start dictation"
2. Recording 3-8 samples of yourself saying "stop dictation"
3. Training wakeword reference files
4. Integration similar to oww-rs but with custom models

---

### 3. **pv_porcupine** (Picovoice)

**Crate:** [pv_porcupine](https://crates.io/crates/pv_porcupine)
**License:** Commercial

#### Status

⚠️ **DEPRECATED** - Rust SDKs will no longer be maintained after July 15, 2025
❌ Not recommended for new projects

---

## Comparison Matrix

| Feature | oww-rs | rustpotter | pv_porcupine |
|---------|--------|------------|--------------|
| **Pre-trained Models** | ✅ Yes | ❌ No | ✅ Yes |
| **Active Maintenance** | ✅ Yes (2025) | ✅ Yes | ❌ Deprecated |
| **License** | ✅ Open-source | ✅ Open-source | ❌ Commercial |
| **Rust Native** | ✅ Yes | ✅ Yes | ✅ Yes |
| **Setup Complexity** | ⭐ Simple | ⭐⭐⭐ Complex | ⭐⭐ Medium |
| **Accuracy** | >99% | Good (with training) | Excellent |
| **CPU Efficiency** | ⭐⭐⭐ Excellent | ⭐⭐ Good | ⭐⭐⭐ Excellent |
| **Memory Usage** | ~Few MB/model | 320KB-3.1MB | ~1MB |
| **ONNX Runtime** | ✅ Yes | ❌ No | ❌ No |
| **Customization** | ⭐⭐ Medium | ⭐⭐⭐ High | ⭐⭐⭐ High |

---

## Recommendation

### **Primary Choice: oww-rs (OpenWakeWord)**

**Reasoning:**

1. **Zero-friction setup** - Pre-trained models work out of the box
2. **Shared dependency** - Already using `ort` crate for Parakeet engine
3. **Active development** - Updated October 2025
4. **Proven accuracy** - >99% accuracy, low false positive rate
5. **Efficient** - Can run multiple models simultaneously on modest hardware
6. **Simple API** - Minimalistic Rust wrapper around ONNX inference

### **Alternative Choice: rustpotter**

Consider if you need:
- Custom wake words beyond available pre-trained models
- Complete control over training data
- Pure Rust without ONNX dependency

However, the training overhead makes this less practical for quick implementation.

---

## Implementation Plan

### Phase 1: Proof of Concept (1-2 hours)

1. Add dependencies to `Cargo.toml`:
   ```toml
   oww-rs = "0.0.1"
   cpal = "0.15"  # Audio input
   ```

2. Download pre-trained models:
   ```bash
   mkdir -p models/wake_words
   cd models/wake_words
   # Download alexa.onnx or hey_jarvis.onnx from OpenWakeWord
   wget https://github.com/dscripka/openWakeWord/raw/main/openwakeword/resources/models/alexa.onnx
   ```

3. Create `src/wake_word.rs`:
   - Initialize oww-rs with ONNX models
   - Set up audio capture with `cpal`
   - Process audio frames through models
   - Emit callbacks on detection

4. Test with simple CLI:
   ```bash
   cargo run --example wake_word_test
   # Say "alexa" → should print "Wake word detected!"
   ```

### Phase 2: Integration (2-3 hours)

1. Add mode flag to `src/bin/cli.rs`:
   ```bash
   transcribe listen  # New wake word mode
   transcribe start   # Existing hotkey mode
   ```

2. Replace hotkey logic with wake word detection:
   - Start detection: "hey jarvis" or "alexa" → start recording
   - Stop detection: Could use same word (toggle) or different word

3. Handle audio routing:
   - Wake word detection uses microphone monitoring
   - Once activated, switch to ffmpeg recording

### Phase 3: Production (1-2 hours)

1. Train/select optimal wake words for start/stop
2. Add configuration to `Cargo.toml` or config file
3. Update systemd service for listen mode
4. Update Hyprland config with optional voice-only mode
5. Test thoroughly with various scenarios

**Total Estimated Time:** 4-7 hours

---

## Wake Word Selection Strategy

### Option A: Single Toggle Word
- **Wake word:** "hey jarvis" (or "alexa")
- **Behavior:** First detection → start recording, second detection → stop recording
- **Pro:** Simpler, only one model needed
- **Con:** Less intuitive than explicit start/stop commands

### Option B: Dual Wake Words
- **Start:** "hey jarvis" or "alexa"
- **Stop:** Custom word like "that's all" or "finished"
- **Pro:** More intuitive workflow
- **Con:** Requires training custom stop word or finding suitable pre-trained model

### Option C: Hybrid Mode
- **Start:** Wake word detection ("hey jarvis")
- **Stop:** Silence detection or timeout
- **Pro:** Most natural for dictation
- **Con:** Requires voice activity detection (VAD) implementation

**Recommended:** Start with Option A for simplicity, evolve to Option B if needed.

---

## Technical Considerations for Integration

### Audio Pipeline Changes

**Current (Hotkey):**
```
Hotkey Press → ffmpeg start → record to /tmp/ptt_current.wav → Hotkey Release → ffmpeg stop
```

**Proposed (Wake Word):**
```
Continuous monitoring → Wake word detected → ffmpeg start → record →
Wake word detected (or timeout) → ffmpeg stop
```

### Challenges

1. **Dual Audio Streams**
   - Wake word detection needs continuous microphone access
   - Recording uses ffmpeg subprocess
   - Solution: Use `cpal` for monitoring, ffmpeg for recording (different handles)

2. **Processing Overhead**
   - Running wake word models continuously
   - Solution: oww-rs is efficient enough for background operation

3. **False Positives**
   - Accidental wake word triggers during dictation
   - Solution: Suspend wake word detection during active recording

4. **State Management**
   - Track: MONITORING → RECORDING → PROCESSING → MONITORING
   - Solution: State machine in `src/wake_word.rs`

---

## Alternative: Command-Based Detection

Instead of continuous monitoring, could use:
- Say "start dictation" → transcribe → if matches, begin recording
- Say "stop dictation" → transcribe → if matches, end recording

This would use your existing Parakeet engine but requires:
- Always-on microphone buffering
- Quick transcription to detect commands
- Higher latency than dedicated wake word models

**Not recommended** - Wake word models are optimized for this exact use case.

---

## Conclusion

**oww-rs** (OpenWakeWord) is the fastest and simplest solution for adding wake word functionality to transcribe-rs-v2:

- ✅ Minimal setup with pre-trained models
- ✅ High accuracy and low resource usage
- ✅ Leverages existing ONNX Runtime dependency
- ✅ Active development and community support
- ✅ Can be integrated in ~5-7 hours of development time

The integration would maintain compatibility with your existing hotkey-based workflow while adding an optional voice-activated mode for hands-free dictation.

---

## Next Steps

1. **Experiment with oww-rs** - Build simple detection demo
2. **Test pre-trained models** - Evaluate "alexa", "hey jarvis", etc. in your environment
3. **Design state machine** - Plan monitoring → recording → processing flow
4. **Prototype integration** - Add to CLI with feature flag
5. **User testing** - Validate accuracy and usability in real-world scenarios

---

## Additional Resources

- [OpenWakeWord GitHub](https://github.com/dscripka/openWakeWord) - Main project with docs
- [oww-rs on crates.io](https://crates.io/crates/oww-rs) - Rust implementation
- [Rustpotter GitHub](https://github.com/GiviMAD/rustpotter) - Alternative pure Rust solution
- [ONNX Runtime Rust Bindings](https://crates.io/crates/ort) - Already in your project
