# transcribe-rs-v2

**Fast, local push-to-talk dictation for Linux/Wayland/Hyprland**

A Rust-based voice dictation system that transcribes your speech and automatically pastes it into any application. Uses local AI models (Parakeet or Whisper) for privacy and speed - no cloud API required.

Press a hotkey, speak, release, and your text appears instantly in the active window.

## Features

- 🎤 **Push-to-Talk Recording** - Hold a hotkey to record, release to transcribe
- ⚡ **Sub-second Latency** - Daemon keeps model loaded for instant transcription (3-4s cold start eliminated)
- 🔒 **100% Local** - All processing on your machine, no cloud APIs
- 📋 **Auto-Paste** - Transcription automatically typed into active window
- 🖥️ **Smart Terminal Detection** - Uses Ctrl+Shift+V in terminals, Ctrl+V elsewhere
- 🎯 **Accurate** - Powered by Parakeet (NVIDIA NeMo) or Whisper models
- ⚙️ **Configurable** - TOML config for microphone, model, behavior

## Demo

```
User: *presses Super+Shift+Ctrl+Alt+E*
User: "Hello world, this is a test."
User: *releases key*
System: [Records → Transcribes → Pastes "Hello world, this is a test."]
```

## Performance

Using int8 quantized Parakeet model:
- **30x real-time** on M4 Max
- **20x real-time** on Ryzen 5700X
- **5x real-time** on Intel i5-6500

(A 3-second recording transcribes in ~150ms on modern hardware)

---

## Prerequisites

### Operating System & Desktop Environment
- **Linux** (tested on Arch)
- **Wayland** compositor
- **Hyprland** window manager (for terminal detection and keybinding)

⚠️ **Currently Hyprland-specific** - Terminal auto-detection uses `hyprctl`. Other Wayland compositors may work but are untested.

### Required System Packages

```bash
# Arch Linux
sudo pacman -S wl-clipboard ydotool ffmpeg

# Ubuntu/Debian (untested)
sudo apt install wl-clipboard ydotool ffmpeg
```

**What they do:**
- `wl-clipboard` (wl-copy) - Clipboard management on Wayland
- `ydotool` - Keyboard event simulation (requires privileged access)
- `ffmpeg` - Audio recording with PulseAudio

### Build Dependencies

```bash
sudo pacman -S rustup base-devel
rustup default stable
```

---

## Installation

### Quick Install (Recommended)

```bash
# Clone the repository
git clone https://github.com/YOUR_USERNAME/transcribe-rs-v2
cd transcribe-rs-v2

# Run installer (checks dependencies, builds binaries, downloads model)
chmod +x install.sh
./install.sh
```

The installer will:
1. ✓ Check all dependencies are installed
2. ✓ Build release binaries
3. ✓ Download Parakeet model (~400MB)
4. ✓ Create default configuration
5. ✓ Show systemd service setup instructions
6. ✓ Show Hyprland keybinding setup

### Manual Installation

<details>
<summary>Click to expand manual steps</summary>

#### 1. Build the Project

```bash
cargo build --release
```

Binaries created:
- `target/release/transcribe` - Main CLI (start/stop recording)
- `target/release/transcribe-daemon` - Long-running transcription service
- `target/release/transcribe-client` - Direct daemon client

#### 2. Download the Model

```bash
mkdir -p models
cd models
wget https://blob.handy.computer/parakeet-v3-int8.tar.gz
tar -xzf parakeet-v3-int8.tar.gz
cd ..
```

#### 3. Create Configuration

```bash
./target/release/transcribe config init
```

This creates `~/.config/transcribe-rs/config.toml` with defaults.

#### 4. Configure Your Microphone

List available microphones:
```bash
pactl list sources short
```

Edit config if needed:
```bash
nano ~/.config/transcribe-rs/config.toml
```

Change `microphone = "default"` to your specific device if needed.

</details>

---

## Configuration

### Config File Location

`~/.config/transcribe-rs/config.toml`

### Example Configuration

```toml
[audio]
# Microphone source (see "Finding Your Microphone" below)
microphone = "default"
sample_rate = 16000
recording_path = "/tmp/ptt_current.wav"
recording_pid_file = "/tmp/ptt_recording.pid"
log_file = "/tmp/ptt_rust_debug.log"

[model]
# Path to model directory (relative to working directory or absolute)
path = "models/parakeet-tdt-0.6b-v3-int8"
engine = "parakeet"
quantization = "int8"  # or "fp32"

[daemon]
# Unix socket for daemon communication
socket_path = "/tmp/transcribe-rs-v2.sock"

[integration]
# Automatically paste transcription
auto_paste = true
# Add space after sentence-ending punctuation (.!?)
add_space_after_punctuation = true
# Terminal apps (for Ctrl+Shift+V detection)
terminal_apps = [
    "alacritty",
    "kitty",
    "wezterm",
    "foot",
    "terminal",
    "konsole",
    "xterm",
]
```

### Finding Your Microphone

```bash
# List available microphones
pactl list sources short
```

Output example:
```
0  alsa_output.pci-0000_00_1f.3.analog-stereo.monitor ...
1  alsa_input.pci-0000_00_1f.3.analog-stereo ...
2  alsa_input.usb-Logitech_Webcam_C922-02.analog-stereo ...
```

Use the full name (e.g., `alsa_input.usb-Logitech_Webcam_C922-02.analog-stereo`) or device index (`2`), or keep `"default"` to use system default.

### Config Commands

```bash
# Show current configuration
transcribe config show

# Show config file path
transcribe config path

# Create default config file
transcribe config init
```

---

## Usage

### 1. Start the Daemon

The daemon keeps the model loaded in memory for instant transcription.

**Option A: Run manually (for testing)**
```bash
./target/release/transcribe-daemon
```

**Option B: Systemd service (recommended)**

Create `~/.config/systemd/user/transcribe-daemon.service`:
```ini
[Unit]
Description=Transcribe-RS Daemon
After=network.target

[Service]
Type=simple
# IMPORTANT: Set this to your project directory
WorkingDirectory=/home/YOUR_USERNAME/code/transcribe-rs-v2
ExecStart=/home/YOUR_USERNAME/code/transcribe-rs-v2/target/release/transcribe-daemon
Restart=on-failure

[Install]
WantedBy=default.target
```

Enable and start:
```bash
systemctl --user daemon-reload
systemctl --user enable transcribe-daemon
systemctl --user start transcribe-daemon

# Check status
systemctl --user status transcribe-daemon

# View logs
journalctl --user -u transcribe-daemon -f
```

### 2. Set Up Keybindings

Add to `~/.config/hypr/hyprland.conf`:

```ini
# Push-to-Talk Dictation
# Press = Start recording
# Release = Stop recording and transcribe
bind = SUPER SHIFT CTRL ALT, E, exec, /path/to/transcribe-rs-v2/target/release/transcribe start
bindr = SUPER SHIFT CTRL ALT, E, exec, /path/to/transcribe-rs-v2/target/release/transcribe stop
```

**Note:** `bindr` (bind-release) requires Hyprland. Adjust the key combination to your preference.

Reload Hyprland config:
```bash
hyprctl reload
```

### 3. Use It!

1. Press and hold your hotkey (e.g., `Super+Shift+Ctrl+Alt+E`)
2. Speak clearly
3. Release the hotkey
4. Text appears in your active window

---

## Troubleshooting

### Check System Health

```bash
# Check all dependencies and configuration
transcribe doctor
```

### Common Issues

#### "Failed to connect to transcribe daemon"
- **Solution:** Start the daemon: `transcribe-daemon` or `systemctl --user start transcribe-daemon`
- Check daemon status: `systemctl --user status transcribe-daemon`

#### "ffmpeg failed to start" or "Recording file too small"
- **Cause:** Microphone not found or in use
- **Solution:**
  ```bash
  # List microphones
  pactl list sources short

  # Update config
  nano ~/.config/transcribe-rs/config.toml
  # Set: microphone = "YOUR_DEVICE_NAME"
  ```

#### "Model not found"
- **Solution:** Download model (see Installation) or update `model.path` in config
- Use absolute path: `path = "/home/user/models/parakeet-tdt-0.6b-v3-int8"`

#### Paste not working
- **Cause:** `ydotool` not running or lacks permissions
- **Solution:**
  ```bash
  # Check ydotool daemon
  systemctl status ydotool

  # Start if needed
  sudo systemctl enable --now ydotool
  ```

#### Text pastes in wrong format (Ctrl+V vs Ctrl+Shift+V)
- **Cause:** Terminal not detected correctly
- **Solution:** Add your terminal to `integration.terminal_apps` in config

### Debug Logs

```bash
# View detailed debug logs
tail -f /tmp/ptt_rust_debug.log

# Or use provided viewer script
./view-ptt-logs.sh
```

---

## Platform Support & Limitations

### Supported
- ✅ Linux (Arch Linux tested)
- ✅ Wayland compositors (Hyprland tested)
- ✅ PulseAudio microphones

### Limitations
- ❌ **X11 not supported** (uses Wayland-specific tools)
- ❌ **Hyprland required** for terminal detection (`hyprctl` commands)
- ❌ **macOS/Windows not tested** (possible with modifications)

### Known Issues
- Terminal detection is Hyprland-specific (uses `hyprctl activewindow`)
- Other Wayland compositors (Sway, River, etc.) untested but may work with adjustments

---

## Advanced Usage

### Using Different Models

**Whisper Model:**
```toml
[model]
path = "models/whisper-medium-q4_1.bin"
engine = "whisper"
```

Download Whisper models:
```bash
cd models
wget https://blob.handy.computer/whisper-medium-q4_1.bin
```

### Manual Transcription

```bash
# Transcribe a specific file
transcribe-client audio.wav

# Manual recording workflow
transcribe start
# (speak)
transcribe stop
```

### Library Usage

This project can also be used as a Rust library for integrating transcription into other applications:

```rust
use transcribe_rs::{TranscriptionEngine, engines::parakeet::ParakeetEngine};
use std::path::PathBuf;

let mut engine = ParakeetEngine::new();
engine.load_model(&PathBuf::from("models/parakeet-tdt-0.6b-v3-int8"))?;
let result = engine.transcribe_file(&PathBuf::from("audio.wav"), None)?;
println!("{}", result.text);
```

---

## Model Information

### Parakeet (Recommended)

- **Source:** NVIDIA NeMo Parakeet-TDT 0.6B
- **Type:** Transducer-based streaming ASR
- **Size:** ~400MB (int8 quantized)
- **Speed:** 5-30x real-time depending on hardware
- **Download:** https://blob.handy.computer/parakeet-v3-int8.tar.gz
- **HuggingFace:** https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx

### Whisper (Alternative)

- **Source:** OpenAI Whisper
- **Type:** Transformer-based ASR
- **Models:** Multiple sizes (tiny to large)
- **Speed:** Slower than Parakeet but better multilingual support
- **Download:** https://huggingface.co/ggerganov/whisper.cpp

### Audio Requirements

Input must be:
- **Format:** WAV
- **Sample Rate:** 16 kHz
- **Channels:** Mono
- **Bit Depth:** 16-bit PCM

(The recording system handles this automatically)

---

## Contributing

Contributions welcome! Areas for improvement:

- **Multi-compositor support** (Sway, River, KDE Wayland)
- **X11 support** (different clipboard/paste tools)
- **macOS/Windows support**
- **Model management** (download, switch models easily)
- **Language selection** (multilingual model support)
- **Wake word detection** (hands-free activation)

---

## Acknowledgments

- **[istupakov](https://github.com/istupakov/onnx-asr)** - ONNX implementation of Parakeet
- **NVIDIA** - Parakeet model
- **[whisper.cpp](https://github.com/ggerganov/whisper.cpp)** - Whisper implementation
- **Original transcribe-rs** - Base library this project extends

---

## License

MIT License - See LICENSE file for details

---

## FAQ

**Q: Why v2?**
A: Fork of original transcribe-rs library, extended with push-to-talk dictation system and daemon architecture.

**Q: Why not use cloud APIs (OpenAI, Google, etc.)?**
A: Privacy, cost, and latency. Local processing is instant, free, and your voice never leaves your machine.

**Q: Can I use this on X11?**
A: Not currently - uses Wayland-specific tools (wl-clipboard). PRs welcome for X11 support!

**Q: Does it work with Sway/other Wayland compositors?**
A: Probably! But terminal detection (`hyprctl`) needs porting. Main functionality should work.

**Q: How much RAM does it use?**
A: ~1-2GB for daemon with Parakeet model loaded. Recording/transcription is lightweight.

**Q: What's the socket path used for?**
A: The CLI communicates with the daemon via Unix socket (`/tmp/transcribe-rs-v2.sock`). Keeps model loaded between recordings.
