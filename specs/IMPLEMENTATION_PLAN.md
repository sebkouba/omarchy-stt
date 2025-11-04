# Transcribe-RS v2: Step-by-Step Implementation Plan

## Overview

This plan breaks down the Rust consolidation into small, independently testable steps. Each step builds on the previous one and can be tested without breaking the existing shell script system.

**Key Principle:** Keep the existing bash scripts working until the very end. Test each Rust component independently before integration.

## Implementation Status

**Last Updated:** 2025-11-04

### Phase 1: Foundation (Library Modules) - ✅ COMPLETED
- ✅ **Step 1.1:** Dependencies added to Cargo.toml
- ✅ **Step 1.2:** `src/clipboard.rs` created with 5 passing tests
- ✅ **Step 1.3:** `src/notifications.rs` created with tests
- ✅ **Step 1.4:** `src/terminal_detect.rs` created with tests
- ✅ **Step 1.5:** `src/paste.rs` created with tests
- ✅ **Step 1.6:** `src/recording.rs` created with 2 passing tests
- ✅ **Step 1.7:** All modules exported from `src/lib.rs`
- ✅ All 10 library tests passing (`cargo test --lib`)

### Phase 2: Integration (CLI Binary) - ✅ COMPLETED
- ✅ **Step 2.1:** `src/bin/cli.rs` created with clap subcommands
- ✅ Added `transcribe` binary to Cargo.toml
- ✅ Binary builds successfully (`cargo build --release --bin transcribe`)
- ✅ Help command working (`transcribe --help`)
- ✅ Commands: `start`, `stop`, `daemon`, `client`

### Phase 3: Deployment - 🚧 READY FOR TESTING
- ⏳ **Step 3.1:** Update Hyprland keybindings (manual step)
- ⏳ **Step 3.2:** Create install script
- ⏳ **Step 3.3:** Setup systemd service (optional)

### Phase 4: Configuration - 📋 FUTURE
- ⏳ Config file system (deferred until core is solid)

---

## Current State

```
User Input (Hyprland keybinding)
  ↓
ptt-test.sh start/stop          [Bash - manages recording lifecycle]
  ↓ spawns ffmpeg
  ↓ manages PID file
  ↓ timing/validation
  ↓
transcribe-to-clipboard.sh      [Bash - handles output]
  ↓ calls transcribe-client     [Rust - already done ✅]
  ↓ Unix socket
  ↓ transcribe-daemon           [Rust - already done ✅]
  ↓ wl-copy (clipboard)
  ↓ ydotool (paste)
  ↓ hyprctl + jq (terminal detect)
  ↓ notify-send (notifications)
```

## Target State

```
User Input (Hyprland keybinding)
  ↓
transcribe ptt start/stop       [Rust - everything integrated]
  ↓ spawns ffmpeg (external)
  ↓ Rust state management
  ↓ calls transcribe-client (internal)
  ↓ arboard (clipboard)
  ↓ ydotool wrapper (paste)
  ↓ hyprctl parser (terminal detect)
  ↓ notify-rust (notifications)
```

---

## Phase 1: Foundation (Library Modules)

Build the individual Rust components as library modules that can be tested independently.

### Step 1.1: Add Dependencies

**Goal:** Get all the crates we need into Cargo.toml

**Action:**
```toml
# Add to [dependencies] section in Cargo.toml
clap = { version = "4.5", features = ["derive"] }
arboard = "3.4"
notify-rust = "4.11"
dirs = "5.0"
```

**Test:**
```bash
cargo check
# Should compile without errors
```

**Success Criteria:** ✅ `cargo check` passes

---

### Step 1.2: Clipboard Module

**Goal:** Replace `wl-copy` with pure Rust (arboard crate)

**Create:** `src/clipboard.rs`
```rust
//! Clipboard operations using arboard (cross-platform)

use arboard::Clipboard;
use std::error::Error;

/// Copy text to system clipboard
pub fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn Error>> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text)?;
    Ok(())
}

/// Add space after sentence-ending punctuation
/// This allows consecutive PTT dictations to flow naturally
pub fn add_trailing_space_after_punctuation(text: &str) -> String {
    if let Some(last_char) = text.chars().last() {
        if matches!(last_char, '.' | '!' | '?') {
            return format!("{} ", text);
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_space_after_period() {
        let input = "Hello world.";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "Hello world. ");
    }

    #[test]
    fn test_add_space_after_question() {
        let input = "How are you?";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "How are you? ");
    }

    #[test]
    fn test_no_space_after_comma() {
        let input = "Hello world, how are you";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "Hello world, how are you");
    }

    #[test]
    fn test_empty_string() {
        let input = "";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "");
    }
}
```

**Test:**
```bash
# Run unit tests
cargo test --lib clipboard

# Manual test (requires running in a graphical session)
cargo test --lib clipboard::tests::test_clipboard_round_trip -- --ignored
```

**Success Criteria:**
- ✅ Unit tests pass
- ✅ Can copy text to clipboard (manual test)
- ✅ Punctuation spacing works correctly

---

### Step 1.3: Notification Module

**Goal:** Replace `notify-send` with pure Rust (notify-rust crate)

**Create:** `src/notifications.rs`
```rust
//! Desktop notifications using notify-rust

use notify_rust::{Notification, Timeout};
use std::error::Error;

/// Show a desktop notification
pub fn notify(summary: &str, body: &str, timeout_ms: u32) -> Result<(), Box<dyn Error>> {
    Notification::new()
        .summary(summary)
        .body(body)
        .timeout(Timeout::Milliseconds(timeout_ms))
        .show()?;
    Ok(())
}

/// Convenience functions for common notification types
pub fn notify_recording_started() -> Result<(), Box<dyn Error>> {
    notify("🎤 Recording", "Speak now...", 1000)
}

pub fn notify_recording_stopped() -> Result<(), Box<dyn Error>> {
    notify("⏹️  Processing...", "Transcribing audio...", 1000)
}

pub fn notify_transcription_pasted(preview: &str) -> Result<(), Box<dyn Error>> {
    notify("✅ Pasted", preview, 2000)
}

pub fn notify_transcription_copied(preview: &str) -> Result<(), Box<dyn Error>> {
    notify(
        "📋 Copied to clipboard",
        &format!("{}\nPress Ctrl+V to paste", preview),
        3000,
    )
}

pub fn notify_error(message: &str) -> Result<(), Box<dyn Error>> {
    notify("❌ Error", message, 3000)
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests only verify the functions don't panic
    // Actual notification display requires a graphical session
    #[test]
    #[ignore] // Run with --ignored when you want to see notifications
    fn test_notify_recording_started() {
        notify_recording_started().ok();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    #[test]
    #[ignore]
    fn test_notify_error() {
        notify_error("This is a test error").ok();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
```

**Test:**
```bash
# Unit test (won't show actual notifications)
cargo test --lib notifications

# Manual test (shows actual notifications)
cargo test --lib notifications -- --ignored
```

**Success Criteria:**
- ✅ Notifications compile without errors
- ✅ Can display notifications (manual test)

---

### Step 1.4: Terminal Detection Module

**Goal:** Detect if active window is a terminal (for Ctrl+Shift+V vs Ctrl+V)

**Create:** `src/terminal_detect.rs`
```rust
//! Terminal window detection using hyprctl (Hyprland-specific)

use std::error::Error;
use std::process::Command;

/// Get the class of the currently active window
pub fn get_active_window_class() -> Result<String, Box<dyn Error>> {
    let output = Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()?;

    if !output.status.success() {
        return Err("hyprctl command failed".into());
    }

    let json_str = String::from_utf8(output.stdout)?;
    let json: serde_json::Value = serde_json::from_str(&json_str)?;

    let class = json["class"]
        .as_str()
        .ok_or("class field not found")?
        .to_lowercase();

    Ok(class)
}

/// Check if the active window is a terminal
pub fn is_active_window_terminal() -> bool {
    let terminal_apps = [
        "alacritty",
        "kitty",
        "wezterm",
        "foot",
        "terminal",
        "konsole",
        "terminator",
        "xterm",
        "urxvt",
        "st",
        "code", // VS Code terminal
    ];

    match get_active_window_class() {
        Ok(class) => terminal_apps.iter().any(|term| class.contains(term)),
        Err(_) => false, // Default to non-terminal if detection fails
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires Hyprland to be running
    fn test_get_active_window_class() {
        match get_active_window_class() {
            Ok(class) => {
                println!("Active window class: {}", class);
                assert!(!class.is_empty());
            }
            Err(e) => {
                println!("Could not detect window (not on Hyprland?): {}", e);
            }
        }
    }

    #[test]
    #[ignore]
    fn test_is_terminal() {
        let is_terminal = is_active_window_terminal();
        println!("Is active window a terminal? {}", is_terminal);
        // Just print for manual verification
    }
}
```

**Test:**
```bash
# Unit tests (requires Hyprland)
cargo test --lib terminal_detect -- --ignored
```

**Success Criteria:**
- ✅ Can detect window class
- ✅ Correctly identifies terminals vs non-terminals

---

### Step 1.5: Paste Module

**Goal:** Replace ydotool shell commands with Rust wrapper

**Create:** `src/paste.rs`
```rust
//! Auto-paste functionality using ydotool

use std::error::Error;
use std::process::Command;
use std::thread;
use std::time::Duration;

/// Paste from clipboard using ydotool
/// Automatically detects if active window is a terminal and uses appropriate key combo
pub fn paste_from_clipboard() -> Result<(), Box<dyn Error>> {
    // Set ydotool socket path
    std::env::set_var("YDOTOOL_SOCKET", "/tmp/.ydotool_socket");

    // CRITICAL: Give clipboard time to propagate through Wayland compositor
    thread::sleep(Duration::from_millis(50));

    // Detect if terminal
    let is_terminal = crate::terminal_detect::is_active_window_terminal();

    let exit_status = if is_terminal {
        // Terminal: Use Ctrl+Shift+V
        // Key codes: 29 = Left Ctrl, 42 = Left Shift, 47 = V
        Command::new("ydotool")
            .args(["key", "29:1", "42:1", "47:1", "47:0", "42:0", "29:0"])
            .status()?
    } else {
        // Non-terminal: Use Ctrl+V
        // Key codes: 29 = Left Ctrl, 47 = V
        Command::new("ydotool")
            .args(["key", "29:1", "47:1", "47:0", "29:0"])
            .status()?
    };

    if !exit_status.success() {
        return Err("ydotool command failed".into());
    }

    Ok(())
}

/// Check if ydotool is available
pub fn is_ydotool_available() -> bool {
    Command::new("ydotool")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ydotool_check() {
        let available = is_ydotool_available();
        println!("ydotool available: {}", available);
        // Just print for manual verification
    }

    #[test]
    #[ignore] // Requires ydotool and will actually paste
    fn test_paste() {
        // First, put something in the clipboard manually
        // Then run this test to see if it pastes
        paste_from_clipboard().ok();
        thread::sleep(Duration::from_secs(1));
    }
}
```

**Test:**
```bash
# Check if ydotool is available
cargo test --lib paste::tests::test_ydotool_check

# Manual paste test (dangerous - will actually paste!)
cargo test --lib paste::tests::test_paste -- --ignored
```

**Success Criteria:**
- ✅ Can detect ydotool availability
- ✅ Can execute paste command (manual test)

---

### Step 1.6: Recording Module

**Goal:** Replace bash ffmpeg management with Rust

**Create:** `src/recording.rs`
```rust
//! Audio recording management using ffmpeg

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

const RECORDING_PID_FILE: &str = "/tmp/ptt_recording.pid";
const RECORDING_FILE: &str = "/tmp/ptt_current.wav";
const MIC_SOURCE: &str = "alsa_input.usb-046d_C922_Pro_Stream_Webcam_C4C393EF-02.analog-stereo";
const LOG_FILE: &str = "/tmp/ptt_rust_debug.log";

/// Append a log message to the debug log
fn log(message: &str) {
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE)
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        writeln!(file, "[{}] [recording] {}", timestamp, message).ok();
    }
}

/// Start recording audio with ffmpeg
pub fn start_recording() -> Result<(), Box<dyn Error>> {
    log("=== Recording start requested ===");

    // Check if already recording
    if Path::new(RECORDING_PID_FILE).exists() {
        let pid = fs::read_to_string(RECORDING_PID_FILE)?;
        log(&format!("WARNING: Already recording (PID: {})", pid.trim()));
        return Err("Already recording".into());
    }

    // Remove old recording file
    if Path::new(RECORDING_FILE).exists() {
        let metadata = fs::metadata(RECORDING_FILE)?;
        fs::remove_file(RECORDING_FILE)?;
        log(&format!("Removed old recording file ({} bytes)", metadata.len()));
    }

    // Start ffmpeg in background
    log(&format!("Starting ffmpeg recording to {}", RECORDING_FILE));
    let child = Command::new("ffmpeg")
        .args([
            "-f", "pulse",
            "-i", MIC_SOURCE,
            "-ar", "16000",
            "-ac", "1",
            "-sample_fmt", "s16",
            "-y", RECORDING_FILE,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let pid = child.id();
    fs::write(RECORDING_PID_FILE, pid.to_string())?;
    log(&format!("ffmpeg started with PID: {}", pid));

    // Give ffmpeg time to initialize
    thread::sleep(Duration::from_millis(150));
    log("ffmpeg initialization delay complete");

    // Verify ffmpeg is still running
    if !is_process_running(pid) {
        fs::remove_file(RECORDING_PID_FILE)?;
        log("ERROR: ffmpeg died immediately after starting!");
        return Err("ffmpeg failed to start".into());
    }

    log("Recording started successfully");
    Ok(())
}

/// Stop recording and return the path to the audio file
pub fn stop_recording() -> Result<PathBuf, Box<dyn Error>> {
    log("=== Recording stop requested ===");

    // Check if recording
    if !Path::new(RECORDING_PID_FILE).exists() {
        log("WARNING: Not recording (PID file not found)");
        return Err("Not recording".into());
    }

    // Get PID and kill ffmpeg
    let pid_str = fs::read_to_string(RECORDING_PID_FILE)?;
    let pid: u32 = pid_str.trim().parse()?;
    log(&format!("Stopping recording (PID: {})", pid));

    // Send SIGINT to ffmpeg
    if is_process_running(pid) {
        log("Sending SIGINT to ffmpeg...");
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            kill(Pid::from_raw(pid as i32), Signal::SIGINT)?;
        }
    } else {
        log("WARNING: Process not found in process table");
    }

    // Wait for ffmpeg to exit (with timeout)
    log("Waiting for ffmpeg to exit...");
    let start = Instant::now();
    let max_wait = Duration::from_secs(5);
    let mut poll_count = 0;

    while is_process_running(pid) {
        poll_count += 1;
        if start.elapsed() > max_wait {
            log(&format!("ERROR: ffmpeg did not exit after {:?}, killing forcefully", start.elapsed()));
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                kill(Pid::from_raw(pid as i32), Signal::SIGKILL).ok();
            }
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    log(&format!("ffmpeg exited after {:?} (polled {} times)", start.elapsed(), poll_count));

    // Give filesystem time to flush
    thread::sleep(Duration::from_millis(50));
    log("Filesystem sync delay complete");

    // Remove PID file
    fs::remove_file(RECORDING_PID_FILE)?;
    log("PID file removed");

    // Verify file stability
    verify_file_stable()?;

    // Validate file
    let file_size = fs::metadata(RECORDING_FILE)?.len();
    log(&format!("Recording file size: {} bytes", file_size));

    if file_size < 1000 {
        log(&format!("ERROR: Recording file too small ({} bytes)", file_size));
        return Err("Recording file too small - microphone may be busy".into());
    }

    log("Recording stopped successfully");
    Ok(PathBuf::from(RECORDING_FILE))
}

/// Check if a process is running
fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        kill(Pid::from_raw(pid as i32), Signal::from_c_int(0).unwrap()).is_ok()
    }
    #[cfg(not(unix))]
    {
        // Fallback for non-Unix systems
        false
    }
}

/// Verify file size is stable (not still being written)
fn verify_file_stable() -> Result<(), Box<dyn Error>> {
    if !Path::new(RECORDING_FILE).exists() {
        return Err("Recording file not found".into());
    }

    let size1 = fs::metadata(RECORDING_FILE)?.len();
    thread::sleep(Duration::from_millis(20));
    let size2 = fs::metadata(RECORDING_FILE)?.len();

    if size1 != size2 {
        log(&format!("WARNING: File size changed from {} to {} bytes, waiting longer...", size1, size2));
        thread::sleep(Duration::from_millis(100));
        let size3 = fs::metadata(RECORDING_FILE)?.len();
        log(&format!("File size after additional wait: {} bytes", size3));
    } else {
        log(&format!("File size stable at {} bytes", size1));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires actual hardware and will create files
    fn test_recording_lifecycle() {
        // Start recording
        println!("Starting recording...");
        start_recording().unwrap();
        println!("Recording started, waiting 2 seconds...");

        // Record for 2 seconds
        thread::sleep(Duration::from_secs(2));

        // Stop recording
        println!("Stopping recording...");
        let audio_file = stop_recording().unwrap();
        println!("Recording saved to: {:?}", audio_file);

        // Verify file exists and has content
        assert!(audio_file.exists());
        let metadata = fs::metadata(&audio_file).unwrap();
        assert!(metadata.len() > 1000, "File should have audio data");

        println!("Test passed! File size: {} bytes", metadata.len());
    }

    #[test]
    #[ignore]
    fn test_start_already_recording_error() {
        // Start first recording
        start_recording().ok();

        // Try to start again - should fail
        let result = start_recording();
        assert!(result.is_err());

        // Cleanup
        stop_recording().ok();
    }
}
```

**Additional dependency needed:**
```toml
# Add to Cargo.toml [dependencies]
chrono = "0.4"

[target.'cfg(unix)'.dependencies]
nix = { version = "0.29", features = ["signal"] }
```

**Test:**
```bash
# Add dependencies first
# Edit Cargo.toml to add chrono and nix

# Manual test (will actually record!)
cargo test --lib recording::tests::test_recording_lifecycle -- --ignored --nocapture
```

**Success Criteria:**
- ✅ Can start recording (ffmpeg spawns)
- ✅ Can stop recording (ffmpeg exits cleanly)
- ✅ Audio file is created with valid data
- ✅ Prevents starting recording twice

---

### Step 1.7: Update lib.rs

**Action:** Add module declarations to `src/lib.rs`

```rust
// Add to the top of src/lib.rs (after existing use statements)

pub mod clipboard;
pub mod notifications;
pub mod paste;
pub mod recording;
pub mod terminal_detect;
```

**Test:**
```bash
cargo build --lib
cargo test --lib
```

**Success Criteria:**
- ✅ Library builds without errors
- ✅ All unit tests pass

---

## Phase 2: CLI Binary

Now that we have all the pieces, create a unified CLI that uses them.

### Step 2.1: Create CLI Structure

**Goal:** Create the main CLI entry point with subcommands

**Create:** `src/bin/cli.rs`
```rust
use clap::{Parser, Subcommand};
use std::error::Error;
use std::path::PathBuf;
use transcribe_rs::{clipboard, notifications, paste, recording};

#[derive(Parser)]
#[command(name = "transcribe")]
#[command(about = "Audio transcription with push-to-talk support", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start push-to-talk recording
    Start,
    /// Stop recording and transcribe
    Stop,
    /// Start the transcription daemon (same as transcribe-daemon binary)
    Daemon,
    /// Send a file to the transcription daemon (same as transcribe-client binary)
    Client {
        /// Path to audio file
        file: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start => handle_start(),
        Commands::Stop => handle_stop(),
        Commands::Daemon => {
            // Delegate to existing daemon implementation
            println!("To run the daemon, use: transcribe-daemon");
            println!("(Daemon integration will be added in Phase 3)");
            Ok(())
        }
        Commands::Client { file } => {
            // Delegate to existing client implementation
            println!("To transcribe a file, use: transcribe-client {:?}", file);
            println!("(Client integration will be added in Phase 3)");
            Ok(())
        }
    }
}

fn handle_start() -> Result<(), Box<dyn Error>> {
    recording::start_recording()?;
    notifications::notify_recording_started()?;
    println!("🎤 Recording started...");
    Ok(())
}

fn handle_stop() -> Result<(), Box<dyn Error>> {
    // Stop recording
    let audio_file = recording::stop_recording()?;
    notifications::notify_recording_stopped()?;
    println!("⏹️  Recording stopped");

    // Transcribe using daemon client
    println!("📝 Transcribing...");
    let transcription = transcribe_file(&audio_file)?;

    if transcription.is_empty() {
        notifications::notify_error("No speech detected")?;
        return Err("Empty transcription".into());
    }

    // Add space after punctuation
    let text = clipboard::add_trailing_space_after_punctuation(&transcription);

    // Copy to clipboard
    clipboard::copy_to_clipboard(&text)?;

    // Create preview
    let preview = if text.len() > 100 {
        format!("{}...", &text[..100])
    } else {
        text.clone()
    };

    // Auto-paste if ydotool available
    if paste::is_ydotool_available() {
        paste::paste_from_clipboard()?;
        notifications::notify_transcription_pasted(&preview)?;
    } else {
        notifications::notify_transcription_copied(&preview)?;
    }

    println!("Transcription: {}", text);
    Ok(())
}

/// Call the transcribe-client binary to transcribe a file
fn transcribe_file(file: &PathBuf) -> Result<String, Box<dyn Error>> {
    use std::process::Command;

    let output = Command::new("transcribe-client")
        .arg(file)
        .output()?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Transcription failed: {}", error).into());
    }

    let text = String::from_utf8(output.stdout)?;
    Ok(text.trim().to_string())
}
```

**Update Cargo.toml:**
```toml
# Add new binary
[[bin]]
name = "transcribe"
path = "src/bin/cli.rs"
```

**Test:**
```bash
# Build the CLI
cargo build --release --bin transcribe

# Test help
./target/release/transcribe --help
./target/release/transcribe start --help
./target/release/transcribe stop --help

# Manual full test (requires daemon running and hardware)
# Terminal 1: Start daemon
./target/release/transcribe-daemon

# Terminal 2: Test PTT
./target/release/transcribe start
# (speak for a few seconds)
./target/release/transcribe stop
```

**Success Criteria:**
- ✅ CLI binary compiles
- ✅ Help text displays correctly
- ✅ `transcribe start` starts recording
- ✅ `transcribe stop` stops, transcribes, and pastes
- ✅ Works exactly like the bash scripts

---

### Step 2.2: Rename for Convenience

**Goal:** Make `transcribe ptt start/stop` feel more natural

**Option 1:** Keep `transcribe start/stop` (simpler, what we have)
**Option 2:** Add `transcribe ptt start/stop` (more explicit)
**Option 3:** Add `transcribe record start/stop` (matches plan doc)

**Recommendation:** Start with Option 1 (simplest), can add aliases later.

**Test:** User acceptance - does the command feel natural?

**Success Criteria:**
- ✅ Command is intuitive to use
- ✅ Works reliably

---

## Phase 3: Integration & Polish

### Step 3.1: Update Hyprland Bindings

**Goal:** Switch from bash scripts to Rust CLI

**Action:** Update your Hyprland config:
```ini
# Old (bash scripts)
bind = SUPER SHIFT CTRL ALT, E, exec, /home/seb/code/cloned/transcribe-rs-v2/ptt-test.sh start
bindr = SUPER SHIFT CTRL ALT, E, exec, /home/seb/code/cloned/transcribe-rs-v2/ptt-test.sh stop

# New (Rust CLI)
bind = SUPER SHIFT CTRL ALT, E, exec, /home/seb/code/cloned/transcribe-rs-v2/target/release/transcribe start
bindr = SUPER SHIFT CTRL ALT, E, exec, /home/seb/code/cloned/transcribe-rs-v2/target/release/transcribe stop
```

Reload Hyprland config:
```bash
hyprctl reload
```

**Test:**
- Press hotkey and speak
- Release hotkey
- Verify transcription pastes into active window

**Success Criteria:**
- ✅ Hotkey triggers recording
- ✅ Release stops and pastes
- ✅ Works as well as bash version
- ✅ Can still rollback to bash if needed (just change config back)

---

### Step 3.2: Install Script

**Goal:** Make it easy to install system-wide

**Create:** `install.sh`
```bash
#!/bin/bash
# Install transcribe-rs system-wide

set -e

echo "🔨 Building release binaries..."
cargo build --release

echo "📦 Installing binaries..."
sudo cp target/release/transcribe /usr/local/bin/
sudo cp target/release/transcribe-daemon /usr/local/bin/
sudo cp target/release/transcribe-client /usr/local/bin/

echo "✅ Installation complete!"
echo ""
echo "Next steps:"
echo "1. Start the daemon: transcribe-daemon"
echo "2. Update your keybindings to call 'transcribe start' and 'transcribe stop'"
echo "3. Test with: transcribe start (speak) transcribe stop"
```

**Test:**
```bash
./install.sh
which transcribe  # Should show /usr/local/bin/transcribe
transcribe --help
```

**Success Criteria:**
- ✅ Binaries installed to /usr/local/bin
- ✅ Can call `transcribe` from anywhere

---

### Step 3.3: Systemd Service (Optional)

**Goal:** Auto-start daemon on login

**Update:** `transcribe-daemon.service`
```ini
[Unit]
Description=Transcribe-RS Daemon
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/transcribe-daemon
WorkingDirectory=/home/seb/code/cloned/transcribe-rs-v2
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=default.target
```

**Install:**
```bash
mkdir -p ~/.config/systemd/user/
cp transcribe-daemon.service ~/.config/systemd/user/
systemctl --user enable transcribe-daemon
systemctl --user start transcribe-daemon
systemctl --user status transcribe-daemon
```

**Test:**
```bash
# Check service status
systemctl --user status transcribe-daemon

# Test transcription
echo "test" | transcribe client /tmp/test.wav
```

**Success Criteria:**
- ✅ Daemon starts automatically on login
- ✅ Daemon restarts on failure
- ✅ Can still use manual daemon start for development

---

## Phase 4: Configuration (Future Enhancement)

This can be deferred until the core functionality is solid.

### Step 4.1: Config File Support

**Goal:** Stop hardcoding microphone names and paths

**Create:** `src/config.rs`
```rust
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub audio: AudioConfig,
    pub model: ModelConfig,
    pub daemon: DaemonConfig,
    pub integration: IntegrationConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AudioConfig {
    pub microphone: String,
    pub sample_rate: u32,
    pub recording_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelConfig {
    pub path: String,
    pub engine: String,
    pub quantization: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub socket_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IntegrationConfig {
    pub auto_paste: bool,
    pub add_space_after_punctuation: bool,
    pub terminal_apps: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            audio: AudioConfig {
                microphone: "default".to_string(),
                sample_rate: 16000,
                recording_path: "/tmp/ptt_current.wav".to_string(),
            },
            model: ModelConfig {
                path: "models/parakeet-tdt-0.6b-v3-int8".to_string(),
                engine: "parakeet".to_string(),
                quantization: "int8".to_string(),
            },
            daemon: DaemonConfig {
                socket_path: "/tmp/transcribe-rs-v2.sock".to_string(),
            },
            integration: IntegrationConfig {
                auto_paste: true,
                add_space_after_punctuation: true,
                terminal_apps: vec![
                    "alacritty".to_string(),
                    "kitty".to_string(),
                    "wezterm".to_string(),
                    "foot".to_string(),
                ],
            },
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn Error>> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let config_path = Self::config_path()?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        fs::write(config_path, content)?;
        Ok(())
    }

    fn config_path() -> Result<PathBuf, Box<dyn Error>> {
        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?;
        Ok(config_dir.join("transcribe-rs").join("config.toml"))
    }
}
```

**Add CLI commands:**
```rust
// In src/bin/cli.rs Commands enum
Config {
    #[command(subcommand)]
    config_cmd: ConfigCommands,
},

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Create default config file
    Init,
    /// Edit config in $EDITOR
    Edit,
}
```

**Test:**
```bash
transcribe config init
transcribe config show
transcribe config edit
```

**Success Criteria:**
- ✅ Config file created at ~/.config/transcribe-rs/config.toml
- ✅ Can customize microphone, model path, etc.
- ✅ Changes are respected by all commands

---

## Testing Strategy

### Unit Tests
Each module has its own tests:
```bash
cargo test --lib
```

### Integration Tests
Test the full workflow:
```bash
# Create tests/integration_test.rs
cargo test --test integration_test
```

### Manual Testing Checklist
```bash
# 1. Test recording
transcribe start
# (speak for 2 seconds)
transcribe stop
# ✅ Should paste transcription

# 2. Test daemon mode
transcribe-daemon &
transcribe client samples/jfk.wav
# ✅ Should print transcription

# 3. Test both versions can coexist
cd ../transcribe-rs
./start-daemon.sh  # v1 on /tmp/transcribe-rs.sock

cd ../transcribe-rs-v2
transcribe-daemon &  # v2 on /tmp/transcribe-rs-v2.sock

# ✅ Both should work independently

# 4. Test error cases
transcribe stop  # When not recording
transcribe start  # Twice in a row
# ✅ Should show helpful errors

# 5. Test clipboard and paste
transcribe start
# (speak)
transcribe stop
# ✅ Should paste into terminal with Ctrl+Shift+V
# ✅ Should paste into browser with Ctrl+V
```

---

## Migration Timeline

### Week 1: Foundation
- ✅ Add dependencies
- ✅ Build and test individual modules
- ✅ Verify each works independently

### Week 2: Integration
- ✅ Create CLI binary
- ✅ Test full workflow manually
- ✅ Compare with bash script behavior

### Week 3: Deployment
- ✅ Update Hyprland bindings
- ✅ Run both versions in parallel
- ✅ Fix any issues discovered

### Week 4: Polish
- ✅ Add config file support
- ✅ Create install script
- ✅ Setup systemd service
- ✅ Write documentation

---

## Rollback Plan

At any point, you can rollback to bash scripts:

1. **Before changing keybindings:** Just delete the Rust binary
2. **After changing keybindings:** Change Hyprland config back to `.sh` files
3. **After installing:** `sudo rm /usr/local/bin/transcribe*` and update config

The bash scripts remain untouched and functional throughout this process.

---

## Success Metrics

### Phase 1 Complete When:
- ✅ All library modules compile
- ✅ Unit tests pass
- ✅ Manual tests verify each component works

### Phase 2 Complete When:
- ✅ CLI binary works like bash scripts
- ✅ Can switch keybindings without breaking workflow
- ✅ No regressions in functionality

### Phase 3 Complete When:
- ✅ Using Rust version exclusively
- ✅ Bash scripts no longer needed
- ✅ System is reliable and fast

### Ready for Others When:
- ✅ Config system works
- ✅ Install script tested
- ✅ Documentation complete
- ✅ Tested on fresh system

---

## Open Questions

1. **Microphone detection:** Should we auto-detect available microphones?
   - Pro: Easier setup
   - Con: More complexity
   - **Decision:** Defer to config file - let users specify

2. **Error handling:** How verbose should error messages be?
   - Current: Notifications + stdout
   - Alternative: Logs only?
   - **Decision:** Keep notifications for user feedback, add verbose flag later

3. **Terminal detection:** Only supports Hyprland currently
   - Should we support X11 window managers?
   - **Decision:** Start with Hyprland, make it pluggable later

4. **Command naming:** Should it be `transcribe start/stop` or `transcribe ptt start/stop`?
   - `start/stop`: Shorter, simpler
   - `ptt start/stop`: More explicit
   - **Decision:** Start with `start/stop`, can add aliases

---

## Dependencies Summary

```toml
[dependencies]
# Existing
hound = "3.5.1"
log = "0.4.28"
ndarray = "0.16.1"
ort = { version = "2.0.0-rc.10" }
env_logger = "0.10.0"
regex = "1.11.2"
thiserror = "2.0.16"
once_cell = "1.21.3"
tokio = { version = "1.47.1", features = ["rt-multi-thread"] }
async-openai = { version = "0.29.3" }
async-trait = { version = "0.1.89" }
derive_builder = { version = "0.20.2" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# NEW - Phase 1
clap = { version = "4.5", features = ["derive"] }
arboard = "3.4"
notify-rust = "4.11"
dirs = "5.0"
chrono = "0.4"

# NEW - Phase 4 (config)
toml = "0.8"

[target.'cfg(unix)'.dependencies]
nix = { version = "0.29", features = ["signal"] }
```

---

## Next Immediate Steps

1. **Add dependencies to Cargo.toml**
2. **Create `src/clipboard.rs` and test it**
3. **Create `src/notifications.rs` and test it**
4. **Create `src/terminal_detect.rs` and test it**
5. **Create `src/paste.rs` and test it**
6. **Create `src/recording.rs` and test it**
7. **Update `src/lib.rs` to export modules**
8. **Create `src/bin/cli.rs` and test full integration**
9. **Update Hyprland keybindings**
10. **Use it daily and fix bugs**

---

**Last Updated:** 2025-11-04
**Status:** Ready to implement Phase 1
**Estimated Time:** 1-2 weeks for Phase 1, 1 week for Phase 2
