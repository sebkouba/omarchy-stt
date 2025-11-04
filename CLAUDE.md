# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

transcribe-rs is a Rust library for audio transcription supporting multiple ASR (Automatic Speech Recognition) engines including Whisper and Parakeet (NeMo). The library was extracted from the [Handy](https://github.com/cjpais/handy) project to provide a reusable transcription API for the Rust ecosystem.

## Repository Context

**This is transcribe-rs-v2** - a fork/continuation of the original transcribe-rs project for experimental Rust M implementation work.

- **Original project**: `/home/seb/code/cloned/transcribe-rs` (still active and in use)
- **This project (v2)**: `/home/seb/code/cloned/transcribe-rs-v2` (for Rust M implementation experiments)
- **Key difference**: Different socket path (`/tmp/transcribe-rs-v2.sock` vs `/tmp/transcribe-rs.sock`) allows both versions to run simultaneously
- **Purpose**: This separate directory enables continuing development on the Rust M implementation without affecting the stable original version

When working with paths, scripts, or the daemon:
- All paths should reference `transcribe-rs-v2` (not `transcribe-rs`)
- Socket path is `/tmp/transcribe-rs-v2.sock`
- The daemon and client in this directory are independent from the original

## Common Commands

### Building and Testing

```bash
# Build the project
cargo build

# Build with release optimizations
cargo build --release

# Run all tests (requires model files to be present)
cargo test

# Run a specific test
cargo test test_jfk_transcription

# Run doc tests
cargo test --doc

# Run the example (requires models/ directory with models)
cargo run --example transcribe

# Check code without building
cargo check

# Format code
cargo fmt

# Lint with Clippy
cargo clippy
```

### Development Setup

Before running examples or tests, you need to download the required models:

```bash
# Create models directory
mkdir models

# Download Parakeet model (recommended for performance)
cd models
wget https://blob.handy.computer/parakeet-v3-int8.tar.gz
tar -xzf parakeet-v3-int8.tar.gz
rm parakeet-v3-int8.tar.gz
cd ..

# Or download Whisper model (alternative)
cd models
wget https://blob.handy.computer/whisper-medium-q4_1.bin
cd ..
```

## Architecture

### Core Design Pattern

The library follows a **trait-based abstraction pattern** to support multiple transcription engines through a common interface:

1. **TranscriptionEngine trait** (`src/lib.rs:124-202`): The core trait that all engines implement, defining:
   - Associated types for `InferenceParams` and `ModelParams`
   - Model lifecycle methods: `load_model()`, `load_model_with_params()`, `unload_model()`
   - Transcription methods: `transcribe_samples()`, `transcribe_file()`

2. **Engine implementations** live in `src/engines/`:
   - **Whisper** (`src/engines/whisper.rs`): Uses whisper-rs bindings with hardware acceleration (Metal on macOS, Vulkan on Windows/Linux)
   - **Parakeet** (`src/engines/parakeet/`): ONNX-based implementation with separate encoder, decoder, and preprocessor models

3. **Remote transcription** (`src/remote/`): Async trait for remote API-based transcription (currently OpenAI)

### Module Structure

- **`src/lib.rs`**: Core types (`TranscriptionResult`, `TranscriptionSegment`) and the `TranscriptionEngine` trait
- **`src/audio.rs`**: Audio file reading and validation (enforces 16kHz, 16-bit, mono WAV format)
- **`src/engines/`**: Local inference engines
  - `whisper.rs`: Whisper engine using single GGML model files
  - `parakeet/`: Parakeet engine split across multiple files:
    - `engine.rs`: Engine implementation with quantization support (FP32/Int8)
    - `model.rs`: Core ONNX model management (encoder, decoder, preprocessor)
    - `timestamps.rs`: Timestamp processing at token/word/segment granularity
- **`src/remote/`**: Remote API engines (async trait)
  - `openai.rs`: OpenAI Whisper API implementation

### Key Architectural Patterns

1. **Model Format Differences**:
   - Whisper: Single `.bin` file (GGML format)
   - Parakeet: Directory with multiple ONNX files + vocab.txt

2. **Quantization Support** (Parakeet only):
   - FP32: `encoder-model.onnx`, `decoder_joint-model.onnx`
   - Int8: `encoder-model.int8.onnx`, `decoder_joint-model.int8.onnx`
   - Specified via `ParakeetModelParams::fp32()` or `ParakeetModelParams::int8()`

3. **Timestamp Granularity** (Parakeet only):
   - Token-level: Raw token boundaries
   - Word-level: Grouped into words using whitespace detection
   - Segment-level: Grouped into sentence-like segments
   - Configured via `ParakeetInferenceParams.timestamp_granularity`

4. **Hardware Acceleration**:
   - Whisper uses platform-specific features (Metal/Vulkan) via conditional compilation
   - Parakeet uses ONNX Runtime which handles backend optimization

### Audio Requirements

All engines expect audio in this exact format:
- Format: WAV (PCM)
- Sample Rate: 16 kHz
- Channels: Mono (1)
- Bit Depth: 16-bit
- The `audio::read_wav_samples()` function validates and converts to f32 samples normalized to [-1.0, 1.0]

## Testing

Tests are located in `tests/` directory (not inline with source):
- `tests/whisper.rs`: Whisper engine tests
- `tests/parakeet.rs`: Parakeet engine tests
- `tests/openai.rs`: OpenAI remote API tests

Tests require:
- Model files in `models/` directory
- Sample audio files in `samples/` directory (e.g., `samples/jfk.wav`)

## Dependencies

Key dependencies:
- **hound**: WAV file reading
- **ort**: ONNX Runtime bindings (for Parakeet)
- **whisper-rs**: Whisper.cpp bindings with hardware acceleration
- **async-openai**: OpenAI API client
- **ndarray**: N-dimensional arrays for model tensor operations

Platform-specific whisper-rs features are conditionally enabled based on target OS (see `Cargo.toml:23-30`).
