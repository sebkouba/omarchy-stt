#!/bin/bash
set -e

cd "$(dirname "$0")/.."

# Check staging has binaries
if [ ! -f builds/staging/transcribe-daemon ]; then
    echo "Error: No binaries in builds/staging/"
    echo "Run ./scripts/build.sh first"
    exit 1
fi

VERSION=$(date +%Y%m%d-%H%M%S)
echo "Creating version: $VERSION"

mkdir -p "builds/$VERSION"
cp builds/staging/* "builds/$VERSION/"

rm -f builds/current
ln -s "$VERSION" builds/current

echo "Restarting daemons..."
systemctl --user restart transcribe-daemon recording-daemon hotkey-daemon

echo "Done. Now running: $(readlink builds/current)"
systemctl --user is-active transcribe-daemon recording-daemon hotkey-daemon
