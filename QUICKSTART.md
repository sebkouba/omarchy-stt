# Quick Start Guide - Voice Transcription

Everything is set up and ready to go! Here's how to use it:

## Simple Usage

### Option 1: All-in-One Script (Easiest!)

Record and transcribe your voice in one command:

```bash
./transcribe-voice.sh 10
```

This will:
1. Record for 10 seconds (change the number for different duration)
2. Automatically transcribe what you said
3. Show the result

### Option 2: Step-by-Step

1. **Record your voice:**
   ```bash
   ./record.sh recording.wav 5
   ```
   (Records for 5 seconds - change duration as needed)

2. **Transcribe the recording:**
   ```bash
   cargo run --example simple --release
   ```

## What Just Happened?

- ✓ Rust is installed (v1.90.0)
- ✓ Parakeet AI model downloaded (456MB)
- ✓ Recording script created (`record.sh`)
- ✓ Simple transcription tool built
- ✓ Tested successfully with sample audio!

## Audio Format

Your recordings are automatically formatted correctly:
- 16kHz sample rate
- Mono (1 channel)
- 16-bit PCM WAV format

## Test Files

Try the included samples:
```bash
# Test with Steve Jobs speech
cargo run --example test-sample --release

# Or run the original example
cargo run --example transcribe --release
```

## Tips

- **Longer recordings:** `./transcribe-voice.sh 30` (records for 30 seconds)
- **Check your mic:** Make sure your microphone is working and not muted
- **Quiet environment:** Works best with minimal background noise
- **Clear speech:** Speak clearly and at a normal pace

## Troubleshooting

If transcription is empty:
- Check microphone is connected and working
- Verify PulseAudio is running: `pactl info`
- Test recording: `./record.sh test.wav 3` then play it back
- Check audio levels in your system settings

Enjoy transcribing! 🎤
