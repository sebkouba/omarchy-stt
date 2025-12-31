#!/bin/bash
set -e

cd "$(dirname "$0")/.."

CURRENT=$(readlink builds/current)
echo "Current version: $CURRENT"

# List versions (excluding current, staging)
VERSIONS=$(ls -d builds/2* 2>/dev/null | sort -r | grep -v "^builds/$CURRENT$" || true)

if [ -z "$VERSIONS" ]; then
    echo "No previous versions to rollback to"
    exit 1
fi

# Use argument or default to most recent non-current version
if [ -n "$1" ]; then
    TARGET="$1"
else
    TARGET=$(echo "$VERSIONS" | head -1 | sed 's|builds/||')
fi

if [ ! -d "builds/$TARGET" ]; then
    echo "Error: Version $TARGET not found"
    echo "Available versions:"
    ls -d builds/2* | sed 's|builds/||'
    exit 1
fi

echo "Rolling back to: $TARGET"

rm -f builds/current
ln -s "$TARGET" builds/current

echo "Restarting daemons..."
systemctl --user restart transcribe-daemon recording-daemon hotkey-daemon

echo "Done. Now running: $(readlink builds/current)"
