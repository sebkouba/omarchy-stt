# Logging System

This document describes the configurable logging system implemented using `log4rs`.

## Overview

The logging system provides:
- Per-module log level configuration via YAML config file
- Standard `log` crate macros (`debug!`, `info!`, `warn!`, `error!`)
- File-based logging with timestamps
- Sensible defaults when no config file exists

## Configuration

### Config File Location

```
~/.config/transcribe-rs/logging.yaml
```

### Creating the Config File

You can create a default config file by copying the template below, or the system will use built-in defaults if no file exists.

### Example Configuration

```yaml
# Transcribe-RS Logging Configuration
#
# Log levels (from most to least verbose): trace, debug, info, warn, error, off
#
# Customize per-module log levels in the 'loggers' section below.
# Module names follow the Rust module path (e.g., transcribe_rs::groq)

appenders:
  file:
    kind: file
    path: "/tmp/ppt_rust_debug.log"
    encoder:
      pattern: "[{d(%Y-%m-%d %H:%M:%S%.3f)}] [{l}] [{M}] {m}{n}"

# Root logger - default level for all modules not explicitly configured
root:
  level: info
  appenders:
    - file

# Per-module log level configuration
loggers:
  # CLI and daemon modules
  transcribe_rs::groq:
    level: debug
  transcribe_rs::recording:
    level: debug
  transcribe_rs::clipboard:
    level: debug
  transcribe_rs::paste:
    level: debug
  transcribe_rs::tools:
    level: debug
  transcribe_rs::conversation_history:
    level: debug

  # Engine modules (can be noisy at trace level)
  transcribe_rs::engines::parakeet:
    level: info
  transcribe_rs::engines::parakeet::model:
    level: info

  # Recording daemon (separate binary)
  recording_daemon:
    level: debug

  # CLI binary
  cli:
    level: debug

  # Transcription daemon
  transcribe_daemon:
    level: info
```

## Log Levels

From most to least verbose:

| Level | Use Case |
|-------|----------|
| `trace` | Very detailed debugging (e.g., every inference step) |
| `debug` | Detailed debugging information |
| `info` | General operational information |
| `warn` | Warning conditions |
| `error` | Error conditions |
| `off` | Disable logging for this module |

## Module Names

The module names correspond to Rust module paths:

| Module | Description |
|--------|-------------|
| `transcribe_rs::groq` | Groq LLM API client |
| `transcribe_rs::recording` | Recording daemon client |
| `transcribe_rs::clipboard` | Clipboard operations (wl-copy) |
| `transcribe_rs::paste` | Paste operations (ydotool) |
| `transcribe_rs::tools` | Tool loading and execution |
| `transcribe_rs::conversation_history` | Conversation history management |
| `transcribe_rs::engines::parakeet` | Parakeet transcription engine |
| `transcribe_rs::engines::parakeet::model` | Parakeet model loading |
| `recording_daemon` | Recording daemon binary |
| `cli` | CLI binary (transcribe command) |
| `transcribe_daemon` | Transcription daemon binary |

## Log Output Format

The default log pattern produces output like:

```
[2025-01-15 14:32:45.123] [DEBUG] [transcribe_rs::recording] Connecting to daemon at /tmp/transcribe-rs-v2-recording.sock
[2025-01-15 14:32:45.156] [INFO] [transcribe_rs::recording] Recording started successfully
[2025-01-15 14:32:48.789] [DEBUG] [transcribe_rs::clipboard] Copying 42 bytes to clipboard via wl-copy
[2025-01-15 14:32:48.812] [DEBUG] [transcribe_rs::paste] Using Ctrl+V for non-terminal
```

## Log File Location

Default: `/tmp/ppt_rust_debug.log`

You can change this in the config file by modifying the `path` under `appenders.file`.

## Viewing Logs

```bash
# Follow logs in real-time
tail -f /tmp/ppt_rust_debug.log

# View last 50 lines
tail -50 /tmp/ppt_rust_debug.log

# Search for errors
grep ERROR /tmp/ppt_rust_debug.log

# Filter by module
grep "transcribe_rs::groq" /tmp/ppt_rust_debug.log
```

## Common Configuration Scenarios

### Quiet Mode (Errors Only)

```yaml
root:
  level: error
  appenders:
    - file
```

### Debug Specific Module

```yaml
root:
  level: warn
  appenders:
    - file

loggers:
  transcribe_rs::groq:
    level: debug
```

### Reduce Parakeet Model Noise

```yaml
loggers:
  transcribe_rs::engines::parakeet:
    level: warn
  transcribe_rs::engines::parakeet::model:
    level: warn
```

### Full Trace for Debugging

```yaml
root:
  level: trace
  appenders:
    - file
```

## Programmatic Usage

The logging module can also be used programmatically:

```rust
use transcribe_rs::logging;

fn main() {
    // Initialize from config file or defaults
    if let Err(e) = logging::init() {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    }

    // Use standard log macros
    log::info!("Application started");
    log::debug!("Debug message: {}", some_value);
}
```

### API Functions

| Function | Description |
|----------|-------------|
| `logging::init()` | Initialize from config file or defaults |
| `logging::init_default()` | Initialize with built-in defaults |
| `logging::get_config_path()` | Get path to logging.yaml |
| `logging::default_config_yaml()` | Get default YAML config as string |
| `logging::create_default_config()` | Create default config file |

## Troubleshooting

### Logs Not Appearing

1. Check if the log file exists: `ls -la /tmp/ppt_rust_debug.log`
2. Check file permissions
3. Verify the config file syntax is valid YAML
4. Check the root log level isn't set to `off`

### Config File Not Loading

If you see "Logging initialized with default configuration" in the logs, the config file wasn't found or had errors. Check:

1. File exists at `~/.config/transcribe-rs/logging.yaml`
2. YAML syntax is valid
3. File permissions allow reading

### Too Much Log Output

Set specific modules to higher levels (warn/error) or set the root level higher.

## Migration from Custom Logging

If you're upgrading from a previous version that used custom `log()` functions, no action is needed. The new system is backward compatible with the same log file location.
