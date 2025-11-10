# Transcribe Daemon

A long-running daemon for fast audio transcription. The daemon loads the model once and keeps it in memory, allowing for much faster transcription requests.

## Architecture

- **transcribe-daemon**: Background service that loads the Parakeet model and listens on Unix socket
- **transcribe-client**: CLI client that sends audio files to the daemon for transcription
- **Protocol**: JSON over Unix domain socket at `/tmp/transcribe-rs-v2.sock`

## Benefits

- **10-100x faster**: No model loading overhead on each request
- **Memory efficient**: Model loaded once and reused
- **No network exposure**: Uses Unix sockets (local only)
- **Simple**: Easy to integrate with scripts

## Usage

### 1. Start the Daemon

```bash
# Terminal 1: Start the daemon (keeps running)
./start-daemon.sh

# Or build and run manually:
cargo build --release --bin transcribe-daemon
./target/release/transcribe-daemon
```

The daemon will:
- Load the Parakeet model (takes 2-5 seconds on first start)
- Create Unix socket at `/tmp/transcribe-rs-v2.sock`
- Wait for transcription requests

### 2. Send Transcription Requests

```bash
# Terminal 2: Use the client to transcribe
./target/release/transcribe-client /path/to/audio.wav
```

The client will:
- Connect to the daemon
- Send the audio file path
- Print the transcription result

### 3. Integration with Scripts

The `transcribe-to-clipboard.sh` script has been updated to use the daemon client:

```bash
# Before (slow - loads model every time):
cargo run --example transcribe-file --release "$AUDIO_FILE"

# After (fast - reuses loaded model):
./target/release/transcribe-client "$AUDIO_FILE"
```

## Running as a Background Service

### Option 1: Simple Background Process

```bash
# Start in background
./start-daemon.sh &

# Or with nohup to keep running after logout
nohup ./start-daemon.sh > /tmp/transcribe-daemon.log 2>&1 &
```

### Option 2: Systemd Service (Recommended)

Create `/etc/systemd/user/transcribe-daemon.service`:

```ini
[Unit]
Description=Transcribe-rs Daemon
After=network.target

[Service]
Type=simple
WorkingDirectory=/home/seb/code/cloned/transcribe-rs-v2
ExecStart=/home/seb/code/cloned/transcribe-rs-v2/target/release/transcribe-daemon
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=default.target
```

Enable and start:

```bash
# Enable on boot
systemctl --user enable transcribe-daemon

# Start now
systemctl --user start transcribe-daemon

# Check status
systemctl --user status transcribe-daemon

# View logs
journalctl --user -u transcribe-daemon -f
```

## Troubleshooting

### Daemon won't start - socket already exists

```bash
rm /tmp/transcribe-rs-v2.sock
./start-daemon.sh
```

### Client can't connect

```bash
# Check if daemon is running
ps aux | grep transcribe-daemon

# Check if socket exists
ls -l /tmp/transcribe-rs-v2.sock
```

### Check daemon logs

If running with systemd:
```bash
journalctl --user -u transcribe-daemon -f
```

If running in terminal, check the terminal output.

## Protocol Details

The daemon uses newline-delimited JSON over Unix sockets:

**Request:**
```json
{"file": "/path/to/audio.wav"}
```

**Response (success):**
```json
{"success": true, "text": "transcribed text here"}
```

**Response (error):**
```json
{"success": false, "error": "error message here"}
```

## Performance Comparison

- **Old method** (transcribe-file): ~3-7 seconds per transcription
  - Model loading: 2-5 seconds
  - Transcription: 1-2 seconds

- **New method** (daemon): ~1-2 seconds per transcription
  - Model loading: 0 seconds (already loaded)
  - Transcription: 1-2 seconds
  - **3-7x faster!**
