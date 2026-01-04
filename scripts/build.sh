#!/bin/bash
set -e

cd "$(dirname "$0")/.."

echo "Building release binaries..."
cargo build --release

echo "Copying to staging..."
mkdir -p builds/staging
cp target/release/{transcribe-daemon,transcribe-client,recording-daemon,hotkey-daemon,transcribe-batch,watch-daemon} builds/staging/

echo "Done. Binaries in builds/staging/"
echo "Test with: ./builds/staging/transcribe-client samples/jfk.wav"
echo "Promote with: ./scripts/promote.sh"
