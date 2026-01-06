# Omarchy STT Setup Guide

Complete setup instructions for the omarchy-stt push-to-talk dictation system.

## Table of Contents

1. [Quick Start](#quick-start)
2. [Finding Your Microphone](#finding-your-microphone)
3. [Configuring the System](#configuring-the-system)
4. [Keyboard Shortcuts](#keyboard-shortcuts)
5. [LLM Post-Processing (Optional)](#llm-post-processing-optional)
6. [Troubleshooting](#troubleshooting)

## Quick Start

After installing via pacman/yay, follow these steps:

```bash
# 1. Find your microphone device
list-microphones

# 2. Copy and edit config
mkdir -p ~/.config/transcribe-rs
cp /usr/share/omarchy-stt/config.toml.example ~/.config/transcribe-rs/config.toml

# 3. Edit config (see below for details)
$EDITOR ~/.config/transcribe-rs/config.toml

# 4. Enable and start services
systemctl --user enable --now recording-daemon transcribe-daemon hotkey-daemon

# 5. Test transcription
transcribe-client /path/to/test.wav
```

## Finding Your Microphone

The `list-microphones` utility helps you find your microphone device name:

```bash
$ list-microphones

Available microphones:
1. alsa_input.usb-Blue_Microphones_Yeti_Stereo_Microphone_REV8-00.analog-stereo
2. alsa_input.pci-0000_00_1f.3.analog-stereo
3. alsa_input.usb-046d_C922_Pro_Stream_Webcam-02.analog-stereo

Select microphone [1-3]:
```

Copy the full device name (e.g., `alsa_input.usb-Blue_Microphones...`) to your config.

**Alternative:** Use `pactl list sources | grep Name` to list all audio sources.

## Configuring the System

Edit `~/.config/transcribe-rs/config.toml`:

### Required Settings

```toml
[audio]
# Your microphone device (from list-microphones)
microphone = "alsa_input.usb-YOUR_DEVICE_HERE.analog-stereo"
sample_rate = 16000

[model]
# Pre-configured for installed model
path = "/usr/share/omarchy-stt/models/parakeet-tdt-0.6b-v3-int8"
engine = "parakeet"
quantization = "int8"
```

### Optional Settings

```toml
[integration]
# Automatically paste transcribed text
auto_paste = true

# Restore clipboard after pasting (prevents clipboard pollution)
prevent_clipboard_pollution = true

# Add space after punctuation (.!?) for natural flow
add_space_after_punctuation = true

[llm]
# Enable LLM post-processing (requires GROQ_API_KEY)
enabled = false
model = "moonshotai/kimi-k2-instruct-0905"

[hotkey]
# Default hotkey for dictation (Super+Shift+Space)
# Uses XDG Desktop Portal for global hotkey registration
modifiers = ["Super", "Shift"]
key = "Space"
```

## Keyboard Shortcuts

### Option 1: Hotkey Daemon (Recommended)

The hotkey-daemon uses XDG Desktop Portal to register global shortcuts (Wayland-compatible).

**Default hotkey:** `Super+Shift+Space`

**To customize:** Edit `~/.config/transcribe-rs/config.toml`:

```toml
[hotkey]
modifiers = ["Super", "Control"]  # Change modifiers
key = "D"                          # Change key
```

**Supported modifiers:** Super, Control, Alt, Shift
**Supported keys:** Any letter, number, or special key

**Restart after changes:**
```bash
systemctl --user restart hotkey-daemon
```

### Option 2: Manual Desktop Environment Keybindings

If hotkey-daemon doesn't work with your setup, configure manually:

#### Hyprland

Add to `~/.config/hypr/hyprland.conf`:

```ini
# Note: These commands don't exist in hotkey-daemon mode
# Only use if you're NOT using hotkey-daemon

# Option A: Direct socket communication (advanced)
bind = SUPER_SHIFT, Space, exec, echo "start" | nc -U /tmp/transcribe-rs-v2-recording.sock

# Option B: Use a wrapper script (recommended if available)
# bind = SUPER_SHIFT, Space, exec, /usr/bin/transcribe-toggle
```

**Note:** The current package uses hotkey-daemon, which handles press/release automatically. Manual keybindings are not recommended unless you have a specific reason.

#### GNOME

1. Settings → Keyboard → Keyboard Shortcuts
2. Add Custom Shortcut
3. Name: "Dictation"
4. Command: `transcribe-toggle` (if available)
5. Shortcut: Press your desired key combination

#### KDE Plasma

1. System Settings → Shortcuts → Custom Shortcuts
2. Edit → New → Global Shortcut → Command/URL
3. Trigger: Set your key combination
4. Action: `transcribe-toggle` (if available)

## LLM Post-Processing (Optional)

Enable LLM-based grammar correction and tool calling:

### 1. Get Groq API Key

1. Visit https://console.groq.com
2. Sign up (free tier available)
3. Create an API key

### 2. Configure Environment

Add to `~/.bashrc` or `~/.zshrc`:

```bash
export GROQ_API_KEY="gsk_your_key_here"
```

Reload:
```bash
source ~/.bashrc
```

### 3. Enable in Config

Edit `~/.config/transcribe-rs/config.toml`:

```toml
[llm]
enabled = true
model = "moonshotai/kimi-k2-instruct-0905"  # Fast and accurate
```

### 4. Restart Services

```bash
systemctl --user restart hotkey-daemon
```

### What LLM Processing Does

- **Grammar correction**: Fixes capitalization, punctuation, common errors
- **Formatting**: Formats lists, dates, numbers naturally
- **Tool calling**: Execute commands via voice (requires tools.json setup)

Example: "send email to john subject meeting tomorrow" → Calls email tool

### Custom Tools (Advanced)

Create `~/.config/transcribe-rs/tools.json` to define custom voice commands.

See `/usr/share/omarchy-stt/tools.json.example` for examples (if available).

## Troubleshooting

### Services Won't Start

Check service status:
```bash
systemctl --user status recording-daemon
systemctl --user status transcribe-daemon
systemctl --user status hotkey-daemon
```

View logs:
```bash
journalctl --user -u recording-daemon -f
journalctl --user -u transcribe-daemon -f
journalctl --user -u hotkey-daemon -f
```

### No Audio Recorded

1. Verify microphone device:
   ```bash
   list-microphones
   pactl list sources | grep -A 5 "Name: $(grep microphone ~/.config/transcribe-rs/config.toml)"
   ```

2. Test with direct recording:
   ```bash
   ffmpeg -f pulse -i YOUR_DEVICE -t 5 test.wav
   aplay test.wav
   ```

3. Check recording-daemon logs:
   ```bash
   journalctl --user -u recording-daemon | grep -i error
   ```

### Transcription Not Working

1. Test daemon directly:
   ```bash
   transcribe-client /path/to/test.wav
   ```

2. Check model loaded:
   ```bash
   journalctl --user -u transcribe-daemon | grep "Model loaded"
   ```

3. Verify model path:
   ```bash
   ls -lh /usr/share/omarchy-stt/models/parakeet-tdt-0.6b-v3-int8/
   ```

### Hotkey Not Working

1. Check hotkey-daemon is running:
   ```bash
   systemctl --user status hotkey-daemon
   ```

2. Verify XDG Desktop Portal is available:
   ```bash
   systemctl --user status xdg-desktop-portal
   ```

3. Check hotkey registration:
   ```bash
   journalctl --user -u hotkey-daemon | grep -i "hotkey\|shortcut"
   ```

4. Try changing the hotkey in config.toml

### Text Not Pasting

1. Verify `ydotool` is installed:
   ```bash
   which ydotool
   ```

2. Check if ydotool has permissions:
   ```bash
   ydotool key 1:1 1:0  # Test key press
   ```

3. Check clipboard tool is working:
   ```bash
   echo "test" | wl-copy
   wl-paste
   ```

### LLM Processing Not Working

1. Verify API key is set:
   ```bash
   echo $GROQ_API_KEY
   ```

2. Check config enabled:
   ```bash
   grep "enabled = true" ~/.config/transcribe-rs/config.toml
   ```

3. Test API key manually:
   ```bash
   curl -H "Authorization: Bearer $GROQ_API_KEY" https://api.groq.com/openai/v1/models
   ```

## Performance Tuning

### Reduce Latency

- Use Int8 quantization (default): Fast, good accuracy
- Reduce buffer size in config (advanced)

### Improve Accuracy

- Use FP32 quantization: Slower, better accuracy
  ```toml
  [model]
  quantization = "fp32"
  ```

- Add custom corrections in `~/.config/transcribe-rs/transcription_corrections.json`

### Resource Limits

Services have resource limits (see `/usr/lib/systemd/user/*.service`):

- recording-daemon: 100MB RAM, 10% CPU
- transcribe-daemon: 2GB RAM, 100% CPU
- hotkey-daemon: 50MB RAM, 5% CPU

Adjust if needed by overriding:
```bash
systemctl --user edit transcribe-daemon
```

## Additional Resources

- GitHub: https://github.com/sebkouba/omarchy-stt
- Issues: https://github.com/sebkouba/omarchy-stt/issues
- Example configs: `/usr/share/omarchy-stt/`

## Quick Reference

| Command | Purpose |
|---------|---------|
| `list-microphones` | Find your microphone device |
| `transcribe-client <file.wav>` | Test transcription |
| `systemctl --user status <service>` | Check service status |
| `journalctl --user -u <service> -f` | View live logs |
| `hotkey-daemon --help` | Show hotkey-daemon options |

## Getting Help

If you encounter issues:

1. Check logs: `journalctl --user -u recording-daemon -u transcribe-daemon -u hotkey-daemon`
2. Test each component individually (see troubleshooting)
3. Report bugs: https://github.com/sebkouba/omarchy-stt/issues
