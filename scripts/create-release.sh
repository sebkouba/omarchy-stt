#!/bin/bash
# Script to create a GitHub release tarball for AUR -bin package

set -e

VERSION=${1:-$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')}
RELEASE_NAME="omarchy-stt-${VERSION}-x86_64"
RELEASE_DIR="releases/${RELEASE_NAME}"

echo "Creating release ${VERSION}..."

# Build release binaries
echo "Building release binaries..."
cargo build --release

# Create release directory structure
echo "Creating release directory..."
rm -rf "${RELEASE_DIR}"
mkdir -p "${RELEASE_DIR}"/{systemd,config}

# Copy binaries (core daemons and utilities)
echo "Copying binaries..."
cp target/release/hotkey-daemon \
   target/release/transcribe-daemon \
   target/release/transcribe-client \
   target/release/recording-daemon \
   target/release/list-microphones \
   "${RELEASE_DIR}/"

# Copy systemd services
echo "Copying systemd services..."
cp packaging/systemd/*.service "${RELEASE_DIR}/systemd/"

# Copy example configs
echo "Copying example configs..."
if [ -f "config.example.toml" ]; then
    cp config.example.toml "${RELEASE_DIR}/config/config.toml.example"
else
    echo "Warning: config.example.toml not found, skipping"
fi

if [ -f "transcription_corrections.example.json" ]; then
    cp transcription_corrections.example.json "${RELEASE_DIR}/config/transcription_corrections.json.example"
else
    echo "Warning: transcription_corrections.example.json not found, skipping"
fi

# Copy docs
echo "Copying documentation..."
cp README.md LICENSE "${RELEASE_DIR}/"
cp packaging/SETUP.md "${RELEASE_DIR}/" 2>/dev/null || echo "Warning: SETUP.md not found"

# Create tarball
echo "Creating tarball..."
cd releases
tar -czf "${RELEASE_NAME}.tar.gz" "${RELEASE_NAME}"
cd ..

# Calculate checksum
echo "Calculating SHA256 checksum..."
sha256sum "releases/${RELEASE_NAME}.tar.gz" | tee "releases/${RELEASE_NAME}.tar.gz.sha256"

echo ""
echo "✅ Release tarball created: releases/${RELEASE_NAME}.tar.gz"
echo ""
echo "Next steps:"
echo "1. Create a GitHub release for v${VERSION}"
echo "2. Upload releases/${RELEASE_NAME}.tar.gz to the release"
echo "3. Update PKGBUILD with new version and SHA256 checksum:"
cat "releases/${RELEASE_NAME}.tar.gz.sha256"
echo ""
echo "4. Test the PKGBUILD: cd packaging && makepkg -si"
echo "5. Submit to AUR: git clone ssh://aur@aur.archlinux.org/omarchy-stt-bin.git"
echo ""
