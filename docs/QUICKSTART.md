# Quick Start Guide

Get push-to-talk dictation working in 10 minutes.

## Prerequisites

**System Requirements:**
- Linux with Wayland (tested on Arch + Hyprland)
- PulseAudio/PipeWire for audio
- Rust toolchain (`rustup`)

**Install dependencies:**
```bash
# Arch Linux
sudo pacman -S wl-clipboard ydotool ffmpeg

# Ubuntu/Debian
sudo apt install wl-clipboard ydotool ffmpeg libasound2-dev

# Start ydotool daemon (required for auto-paste)
sudo systemctl enable --now ydotool
```

## 1. Build

```bash
git clone https://github.com/YOUR_USERNAME/transcribe-rs-v2
cd transcribe-rs-v2
cargo build --release
```

## 2. Download Model

```bash
mkdir -p models && cd models
wget https://blob.handy.computer/parakeet-v3-int8.tar.gz
tar -xzf parakeet-v3-int8.tar.gz
cd ..
```

## 3. Find Your Microphone

```bash
pactl list sources short
```

Example output:
```
0  alsa_output.pci-0000_00_1f.3.analog-stereo.monitor  module-alsa-card.c  s16le 2ch 44100Hz  SUSPENDED
1  alsa_input.pci-0000_00_1f.3.analog-stereo           module-alsa-card.c  s16le 2ch 44100Hz  SUSPENDED
2  alsa_input.usb-Blue_Yeti-00.analog-stereo           module-alsa-card.c  s16le 2ch 48000Hz  IDLE
```

Pick the `alsa_input.*` device you want to use.

## 4. Create Config

```bash
./target/release/transcribe config init
```

Creates `~/.config/transcribe-rs/config.toml` with sensible defaults.

## 5. Start Daemons

**Quick test (manual):**
```bash
# Terminal 1 - Recording daemon
RECORDING_MICROPHONE="alsa_input.usb-Blue_Yeti-00.analog-stereo" ./target/release/recording-daemon

# Terminal 2 - Transcription daemon
./target/release/transcribe-daemon
```

**Production (systemd):**

Create `~/.config/systemd/user/recording-daemon.service`:
```ini
[Unit]
Description=Transcribe-RS Recording Daemon
After=sound.target

[Service]
Type=simple
# IMPORTANT: Replace with YOUR microphone from step 3
Environment="RECORDING_MICROPHONE=alsa_input.usb-Blue_Yeti-00.analog-stereo"
ExecStart=%h/transcribe-rs-v2/target/release/recording-daemon
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
```

Create `~/.config/systemd/user/transcribe-daemon.service`:
```ini
[Unit]
Description=Transcribe-RS Transcription Daemon
After=network.target

[Service]
Type=simple
WorkingDirectory=%h/transcribe-rs-v2
ExecStart=%h/transcribe-rs-v2/target/release/transcribe-daemon
Restart=on-failure

[Install]
WantedBy=default.target
```

Start them:
```bash
systemctl --user daemon-reload
systemctl --user enable --now recording-daemon transcribe-daemon
```

## 6. Set Up Hotkey (Hyprland)

Add to `~/.config/hypr/hyprland.conf`:
```ini
# Push-to-talk: hold to record, release to transcribe
bind = SUPER SHIFT CTRL ALT, E, exec, ~/transcribe-rs-v2/target/release/transcribe start
bindr = SUPER SHIFT CTRL ALT, E, exec, ~/transcribe-rs-v2/target/release/transcribe stop
```

Reload: `hyprctl reload`

## 7. Test It

1. Press and hold `Super+Shift+Ctrl+Alt+E`
2. Speak: "Hello world, this is a test"
3. Release the key
4. Text appears in your active window!

## Troubleshooting

**Check daemon status:**
```bash
systemctl --user status recording-daemon
systemctl --user status transcribe-daemon
```

**View logs:**
```bash
journalctl --user -u recording-daemon -f
journalctl --user -u transcribe-daemon -f
```

**Test transcription manually:**
```bash
./target/release/transcribe-client samples/jfk.wav
```

**Debug log:**
```bash
tail -f /tmp/ptt_rust_debug.log
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RECORDING_MICROPHONE` | `default` | PulseAudio source name |
| `RECORDING_BUFFER_SIZE` | `120` | Circular buffer seconds |
| `RECORDING_SOCKET_PATH` | `/tmp/transcribe-rs-v2-recording.sock` | Recording daemon socket |

## Next Steps

- See [README.md](../README.md) for full configuration options
- See [docs/DAEMON.md](DAEMON.md) for daemon protocol details
- See [docs/TRANSCRIPTION_CORRECTIONS.md](TRANSCRIPTION_CORRECTIONS.md) for fixing common transcription errors
