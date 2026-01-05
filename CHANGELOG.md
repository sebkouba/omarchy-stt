# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial open source release
- XDG Desktop Portal hotkey daemon for cross-compositor compatibility
- Support for Parakeet TDT v3 (Int8 quantized) transcription engine
- Support for Whisper (GGML format) transcription engine
- Recording daemon with circular buffer for zero-latency recording
- Transcription daemon for fast repeated transcriptions
- LLM post-processing via Groq API (grammar correction, tool calling)
- Transcription corrections system with fuzzy pattern matching
- Clipboard preservation during paste operations
- Terminal detection for proper paste keybindings (Ctrl+Shift+V vs Ctrl+V)
- Desktop notifications for dictation workflow feedback
- Privacy-conscious optional dictation logging
- Professional installation script with microphone selection
- Comprehensive documentation and examples

### Features
- Push-to-talk dictation via hotkey daemon
- Near-instant recording start (<10ms latency)
- Fast transcription with model kept in memory
- Customizable transcription corrections
- Optional LLM enhancement with tool calling support
- Automatic clipboard restoration after paste

### Supported Platforms
- Linux (Wayland compositors)
- Tested on Arch Linux with Hyprland

## [0.1.0] - 2025-01-05

Initial release.
