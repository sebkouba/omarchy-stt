# omarchy-stt

**Voice-to-text dictation for Linux/Wayland**

Press hotkey → speak → release → text appears in your active window. Fast, accurate, 100% local.

![Demo](docs/demo.gif)
*(Press Super+Shift+Space, say "Hello world", release, text appears)*

---

## What It Does

Talk to your computer. Your words appear wherever your cursor is—terminal, browser, editor, anywhere.

Everything runs locally on your machine. No internet required. No cloud APIs. Your voice never leaves your computer.

---

## Quick Start

**1. Install dependencies:**
```bash
# Arch Linux
sudo pacman -S ffmpeg wl-clipboard ydotool

# Ubuntu/Debian
sudo apt install ffmpeg wl-clipboard ydotool

# Enable ydotool (required for auto-paste)
sudo systemctl enable --now ydotool
```

**2. Run installer:**
```bash
git clone https://github.com/sebkouba/omarchy-stt
cd omarchy-stt
./install.sh
```

The installer will:
- Build the app (~3 min)
- Download AI model (~400MB)
- Help you pick your microphone
- Show you what hotkey to set

**3. Start it:**
```bash
# Start background services
systemctl --user start recording-daemon transcribe-daemon hotkey-daemon

# Test: Press your hotkey, say "Hello world", release
```

**That's it.** You're dictating.

---

## How It Works

```
┌─────────────────────────────────────────────────────────────┐
│  Press Hotkey → Speak → Release Hotkey                     │
│         ↓              ↓              ↓                     │
│  Start Recording   Recording...   Stop & Transcribe        │
│                                          ↓                  │
│                              Text appears in active window  │
└─────────────────────────────────────────────────────────────┘
```

**Three background daemons:**
1. **recording-daemon** - Always recording to RAM buffer (zero-latency start)
2. **transcribe-daemon** - AI model loaded and ready (fast transcription)
3. **hotkey-daemon** - Listens for your hotkey via XDG Desktop Portal

**Speed:**
- Recording start: **0ms** (already buffering in RAM)
- Recording stop: **~5ms** (extract audio from buffer)
- Transcription: **~150ms** for 3 seconds of speech (on modern CPU)

---

## Features

### Core (works out of the box)
- ⚡ **Fast** - Zero-latency recording, near-instant transcription
- 🔒 **Private** - 100% local processing, no cloud
- 🎯 **Accurate** - Powered by Parakeet (NVIDIA NeMo) or Whisper
- 📋 **Auto-paste** - Types text directly into active window
- 🖥️ **Smart** - Detects terminals, uses Ctrl+Shift+V vs Ctrl+V

### Optional (requires config)
- 🤖 **LLM cleanup** - Grammar/formatting via Groq API
- 🔧 **Custom corrections** - Fix common mistakes ("C plus plus" → "C++")
- 🛠️ **Tool calling** - Voice commands that run scripts

**See [FEATURES.md](FEATURES.md) for details.**

---

## Requirements

**Operating System:**
- Linux with Wayland (tested on Arch + Hyprland)
- Other compositors should work but are untested

**Hardware:**
- ~2GB RAM for AI model
- ~500MB disk space
- Any microphone

**System Packages:**
- `ffmpeg` - Audio capture
- `wl-clipboard` (wl-copy) - Clipboard management
- `ydotool` - Keyboard simulation (needs root: `sudo systemctl enable --now ydotool`)

**Install with:**
```bash
# Arch
sudo pacman -S ffmpeg wl-clipboard ydotool

# Debian/Ubuntu
sudo apt install ffmpeg wl-clipboard ydotool
```

---

## Configuration

Config lives at `~/.config/transcribe-rs/config.toml` (created by installer).

**Change microphone:**
```bash
# List available mics
pactl list sources short

# Edit config or systemd service
nano ~/.config/systemd/user/recording-daemon.service
# Set: Environment="RECORDING_MICROPHONE=your-device-name"
systemctl --user restart recording-daemon
```

**Change hotkey:**
The hotkey-daemon uses XDG Desktop Portal. Set your compositor's global shortcut to trigger it.

**Advanced config:** See [docs/](docs/) folder.

---

## Troubleshooting

**Nothing happens when I press hotkey:**
```bash
# Check daemons are running
systemctl --user status recording-daemon transcribe-daemon hotkey-daemon

# Check logs
journalctl --user -u hotkey-daemon -f
```

**"Failed to connect to daemon":**
```bash
# Restart daemons
systemctl --user restart recording-daemon transcribe-daemon hotkey-daemon
```

**Paste not working:**
```bash
# Check ydotool is running with root
sudo systemctl status ydotool

# Start if needed
sudo systemctl enable --now ydotool
```

**Other issues:**
Run `./install.sh` again - it checks all dependencies.

See [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) for more help.

---

## Project Structure

```
omarchy-stt/
├── README.md           ← You are here
├── FEATURES.md         ← Local vs LLM vs Tools modes
├── install.sh          ← Interactive setup script
├── docs/               ← Detailed documentation
│   ├── QUICKSTART.md   ← Hands-on walkthrough
│   ├── DAEMON.md       ← Daemon architecture
│   ├── TROUBLESHOOTING.md
│   └── ...
└── src/                ← Rust source code
```

---

## Performance

**Transcription Speed** (Parakeet int8 quantized):
- M4 Max: **30x real-time**
- Ryzen 5700X: **20x real-time**
- Intel i5-6500: **5x real-time**

*(A 3-second recording transcribes in ~150ms on modern hardware)*

**Latency Breakdown:**
| Stage | Time |
|-------|------|
| Hotkey press → recording starts | 0ms (already buffering) |
| Recording stops → audio extracted | ~5ms |
| Audio extraction → transcription | ~150ms |
| Transcription → paste | ~50ms |
| **Total** | **~200ms** |

---

## FAQ

**Q: Does this work on X11?**
A: Not currently - uses Wayland-specific tools (wl-clipboard). PRs welcome!

**Q: Does it work on Sway/other compositors?**
A: Should work! Main functionality is compositor-agnostic. Terminal detection uses Hyprland's `hyprctl`.

**Q: Why not use cloud APIs (OpenAI, Google, etc.)?**
A: Privacy, cost, latency. Local is instant, free, and your voice stays on your machine.

**Q: Can I use this as a library in my Rust project?**
A: Yes! The core transcription engine is a library. See [docs/LIBRARY.md](docs/LIBRARY.md).

**Q: What's the difference between transcribe-rs and omarchy-stt?**
A: This is a fork/evolution of the original transcribe-rs library with added daemon architecture, hotkey support, and desktop integration.

---

## Contributing

Contributions welcome! Areas for improvement:
- Multi-compositor support (Sway, River, KDE)
- X11 support
- macOS/Windows ports
- Wake word detection
- Language selection UI

See [CONTRIBUTING.md](CONTRIBUTING.md).

---

## Acknowledgments

- **[Ilya Stupakov](https://github.com/cjpais/transcribe-rs)** - Original transcribe-rs library
- **NVIDIA** - Parakeet model
- **[istupakov](https://github.com/istupakov/onnx-asr)** - ONNX Parakeet implementation
- **[whisper.cpp](https://github.com/ggerganov/whisper.cpp)** - Whisper implementation

---

## License

MIT License - See [LICENSE](LICENSE)
