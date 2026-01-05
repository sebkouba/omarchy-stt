# Contributing to omarchy-stt

Thank you for your interest in contributing to omarchy-stt! This is a side project, so contributions are welcome but not expected.

## Development Setup

1. **Clone the repository**
   ```bash
   git clone https://github.com/yourusername/omarchy-stt.git
   cd omarchy-stt
   ```

2. **Install Rust** (if not already installed)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

3. **Install dependencies**
   ```bash
   # Arch Linux
   sudo pacman -S wl-clipboard ydotool ffmpeg

   # Ubuntu/Debian
   sudo apt install wl-clipboard ydotool ffmpeg

   # Fedora
   sudo dnf install wl-clipboard ydotool ffmpeg
   ```

4. **Download a transcription model** (see README for links)

5. **Build the project**
   ```bash
   ./scripts/build.sh
   ```

## Making Changes

1. **Create a branch** for your changes
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes** with appropriate tests

3. **Build and test**
   ```bash
   ./scripts/build.sh
   cargo test
   cargo clippy
   cargo fmt
   ```

4. **Test manually** if applicable
   ```bash
   ./builds/staging/transcribe-client samples/jfk.wav
   ```

5. **Submit a pull request** with a clear description of your changes

## Code Style

- Follow standard Rust conventions
- Run `cargo fmt` before committing
- Run `cargo clippy` and address warnings
- Add tests for new functionality
- Update documentation if you change behavior

## Testing

Run the test suite:
```bash
cargo test
```

Run specific tests:
```bash
cargo test --lib clipboard
cargo test --test parakeet
```

## Reporting Issues

If you find a bug or have a feature request, please open an issue on GitHub with:
- A clear description of the problem or suggestion
- Steps to reproduce (for bugs)
- Your system information (Linux distro, Wayland compositor)
- Relevant logs from `/tmp/ptt_rust_debug.log`

## Questions?

Feel free to open an issue for questions or discussion. This is a side project, so response times may vary.
