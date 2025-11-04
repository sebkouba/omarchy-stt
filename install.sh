#!/bin/bash
#
# Transcribe-RS v2 Installation Script
#
# This script will:
# 1. Check all system dependencies
# 2. Build release binaries
# 3. Download transcription model
# 4. Create default configuration
# 5. Show setup instructions for systemd and keybindings

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Print functions
print_header() {
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}========================================${NC}"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

# Check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Track if all checks pass
ALL_OK=true

# ============================================================================
# STEP 1: Check Dependencies
# ============================================================================
print_header "Step 1: Checking Dependencies"

echo "Checking required system packages..."

# Check Rust
if command_exists rustc && command_exists cargo; then
    RUST_VERSION=$(rustc --version | awk '{print $2}')
    print_success "Rust $RUST_VERSION installed"
else
    print_error "Rust not found"
    echo "  Install with: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    ALL_OK=false
fi

# Check ffmpeg
if command_exists ffmpeg; then
    print_success "ffmpeg installed"
else
    print_error "ffmpeg not found"
    echo "  Install with: sudo pacman -S ffmpeg"
    ALL_OK=false
fi

# Check wl-clipboard
if command_exists wl-copy; then
    print_success "wl-clipboard installed"
else
    print_error "wl-clipboard not found"
    echo "  Install with: sudo pacman -S wl-clipboard"
    ALL_OK=false
fi

# Check ydotool
if command_exists ydotool; then
    print_success "ydotool installed"
else
    print_error "ydotool not found"
    echo "  Install with: sudo pacman -S ydotool"
    ALL_OK=false
fi

# Check pactl (PulseAudio)
if command_exists pactl; then
    print_success "PulseAudio (pactl) installed"
else
    print_warning "pactl not found (optional but recommended)"
    echo "  Install with: sudo pacman -S pulseaudio"
fi

if [ "$ALL_OK" = false ]; then
    echo ""
    print_error "Missing required dependencies. Please install them first."
    exit 1
fi

echo ""

# ============================================================================
# STEP 2: Build Project
# ============================================================================
print_header "Step 2: Building Project"

echo "Building release binaries (this may take a few minutes)..."
if cargo build --release; then
    print_success "Build successful"
    echo ""
    echo "Binaries created:"
    echo "  • target/release/transcribe (CLI)"
    echo "  • target/release/transcribe-daemon (Background service)"
    echo "  • target/release/transcribe-client (Direct daemon client)"
else
    print_error "Build failed"
    exit 1
fi

echo ""

# ============================================================================
# STEP 3: Download Model
# ============================================================================
print_header "Step 3: Downloading Transcription Model"

MODEL_DIR="models"
MODEL_NAME="parakeet-tdt-0.6b-v3-int8"
MODEL_TAR="parakeet-v3-int8.tar.gz"
MODEL_URL="https://blob.handy.computer/parakeet-v3-int8.tar.gz"

if [ -d "$MODEL_DIR/$MODEL_NAME" ]; then
    print_info "Model already exists at $MODEL_DIR/$MODEL_NAME"
    read -p "Re-download model? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        print_info "Skipping model download"
        echo ""
    else
        rm -rf "$MODEL_DIR/$MODEL_NAME"
    fi
fi

if [ ! -d "$MODEL_DIR/$MODEL_NAME" ]; then
    mkdir -p "$MODEL_DIR"
    cd "$MODEL_DIR"

    echo "Downloading Parakeet model (~400MB)..."
    if command_exists wget; then
        wget -c "$MODEL_URL"
    elif command_exists curl; then
        curl -L -C - -O "$MODEL_URL"
    else
        print_error "Neither wget nor curl found"
        echo "Please download manually:"
        echo "  $MODEL_URL"
        echo "Extract to: $MODEL_DIR/"
        cd ..
        echo ""
    fi

    if [ -f "$MODEL_TAR" ]; then
        echo "Extracting model..."
        tar -xzf "$MODEL_TAR"
        rm "$MODEL_TAR"
        cd ..
        print_success "Model downloaded and extracted"
    else
        cd ..
        print_warning "Model download skipped or failed"
        echo "You can download it later manually"
    fi
    echo ""
fi

# ============================================================================
# STEP 4: Create Configuration
# ============================================================================
print_header "Step 4: Creating Configuration"

CONFIG_DIR="$HOME/.config/transcribe-rs"
CONFIG_FILE="$CONFIG_DIR/config.toml"

if [ -f "$CONFIG_FILE" ]; then
    print_info "Config file already exists: $CONFIG_FILE"
    read -p "Overwrite with defaults? (y/N): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        ./target/release/transcribe config init
        print_success "Config file recreated"
    else
        print_info "Keeping existing config"
    fi
else
    ./target/release/transcribe config init
    print_success "Config file created: $CONFIG_FILE"
fi

echo ""

# Get absolute path to project directory
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ============================================================================
# STEP 5: Show Setup Instructions
# ============================================================================
print_header "Step 5: Setup Instructions"

echo "Installation complete! Follow these steps to start using transcribe-rs:"
echo ""

print_info "1. Update config if needed (optional)"
echo "  View config: ./target/release/transcribe config show"
echo "  Edit config: nano $CONFIG_FILE"
echo ""
echo "  To find your microphone:"
echo "    pactl list sources short"
echo ""

print_info "2. Set up systemd service (recommended)"
echo "  Create service file:"
echo "    nano ~/.config/systemd/user/transcribe-daemon.service"
echo ""
echo "  Paste this content:"
echo "  ─────────────────────────────────────────────────────────"
echo "  [Unit]"
echo "  Description=Transcribe-RS Daemon"
echo "  After=network.target"
echo "  "
echo "  [Service]"
echo "  Type=simple"
echo "  WorkingDirectory=$PROJECT_DIR"
echo "  ExecStart=$PROJECT_DIR/target/release/transcribe-daemon"
echo "  Restart=on-failure"
echo "  "
echo "  [Install]"
echo "  WantedBy=default.target"
echo "  ─────────────────────────────────────────────────────────"
echo ""
echo "  Enable and start:"
echo "    systemctl --user daemon-reload"
echo "    systemctl --user enable transcribe-daemon"
echo "    systemctl --user start transcribe-daemon"
echo ""

print_info "3. Set up Hyprland keybinding"
echo "  Add to ~/.config/hypr/hyprland.conf:"
echo "  ─────────────────────────────────────────────────────────"
echo "  # Push-to-Talk Dictation"
echo "  bind = SUPER SHIFT CTRL ALT, E, exec, $PROJECT_DIR/target/release/transcribe start"
echo "  bindr = SUPER SHIFT CTRL ALT, E, exec, $PROJECT_DIR/target/release/transcribe stop"
echo "  ─────────────────────────────────────────────────────────"
echo ""
echo "  Reload Hyprland:"
echo "    hyprctl reload"
echo ""

print_info "4. Test the installation"
echo "  Run system health check:"
echo "    ./target/release/transcribe doctor"
echo ""
echo "  Manual test:"
echo "    ./target/release/transcribe start"
echo "    (speak for a few seconds)"
echo "    ./target/release/transcribe stop"
echo ""

print_success "Installation complete!"
echo ""
echo "For more information, see README.md"
echo "Troubleshooting: https://github.com/YOUR_USERNAME/transcribe-rs-v2#troubleshooting"
