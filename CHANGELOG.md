# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-01-06

### Fixed
- **CRITICAL**: Fixed `config.example.toml` missing ~20 required configuration fields
  - Added complete `[transcription_corrections]` section with enabled flag
  - Added complete `[dictation_logging]` section with all logging options
  - Added all missing `[llm]` conversation history fields:
    - `conversation_history_enabled`
    - `conversation_history_prompts`
    - `conversation_history_clear_word`
    - `conversation_history_minutes`
    - `conversation_max_turns`
    - `conversation_history_dir`
    - `file_chat_enabled`
    - `file_chat_dir`
  - Added `[llm.tool_sets]` section for tool definitions
  - Added `[llm.prompt_tool_mapping]` section for prompt-to-tool mappings
  - Added complete `[watch]` section for file watcher configuration
  - Added missing `[hotkey]` fields: `modifiers` and `tap_threshold_ms`
  - Added `[[hotkey.bindings]]` array with examples
  - Services can now start successfully with example config (after setting microphone)
  - All advanced features disabled by default with clear documentation

### Added
- Comprehensive testing documentation in `lessons-learned/2026-01-06-aur-package-testing.md`
- Better inline comments explaining each configuration option

### Context
This release fixes a critical packaging bug that prevented new AUR users from starting
the services. The example config was severely out of date with the binary's config
parser requirements, causing immediate startup failures with cryptic TOML parsing errors.

## [0.1.0] - 2026-01-06

### Added
- Initial AUR binary package release
- Pre-built binaries for x86_64 Linux
- Bundled Parakeet v3 Int8 model (456MB)
- Complete systemd service files for all daemons
- Helper script `list-microphones` for device discovery
- Example configuration files
- Comprehensive documentation (README.md, SETUP.md)

### Features
- Hotkey daemon with XDG Desktop Portal GlobalShortcuts support
- Recording daemon with circular buffer (2-minute buffer, zero-latency recording)
- Transcription daemon with Parakeet TDT model pre-loaded
- LLM post-processing support via Groq API
- Conversation history and file chat modes
- Transcription corrections via fuzzy matching
- Automatic file watcher for batch transcription
- Clipboard preservation during paste operations
- Terminal detection for proper paste behavior
- Desktop notifications for recording/transcription status

[Unreleased]: https://github.com/sebkouba/omarchy-stt/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/sebkouba/omarchy-stt/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/sebkouba/omarchy-stt/releases/tag/v0.1.0
