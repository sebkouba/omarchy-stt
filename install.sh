#!/bin/bash
#
# omarchy-stt Installation Script
#
# This script will:
# 1. Check all system dependencies
# 2. Build release binaries
# 3. Download transcription model
# 4. Help you select your microphone
# 5. Create systemd services for all daemons
# 6. Guide you through hotkey setup
#

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Print functions
print_header() {
    echo -e "${CYAN}════════════════════════════════════════${NC}"
    echo -e "${CYAN} $1${NC}"
    echo -e "${CYAN}════════════════════════════════════════${NC}"
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

command_exists() {
    command -v "$1" >/dev/null 2>&1
}

ALL_OK=true

# Get absolute path to project directory
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo ""
print_header "omarchy-stt Installation"
echo ""
echo "This will set up voice dictation on your system."
echo "Press Ctrl+C at any time to cancel."
echo ""
read -p "Press Enter to continue..."
echo ""

# ============================================================================
# STEP 1: Check Dependencies
# ============================================================================
print_header "Step 1/7: Checking Dependencies"
echo ""

# Check Rust
if command_exists rustc && command_exists cargo; then
    RUST_VERSION=$(rustc --version | awk '{print $2}')
    print_success "Rust $RUST_VERSION"
else
    print_error "Rust not found"
    echo "  Install: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    ALL_OK=false
fi

# Check ffmpeg
if command_exists ffmpeg; then
    print_success "ffmpeg"
else
    print_error "ffmpeg not found"
    echo "  Install: sudo pacman -S ffmpeg (Arch) or sudo apt install ffmpeg (Ubuntu)"
    ALL_OK=false
fi

# Check wl-clipboard
if command_exists wl-copy; then
    print_success "wl-clipboard"
else
    print_error "wl-clipboard not found"
    echo "  Install: sudo pacman -S wl-clipboard (Arch) or sudo apt install wl-clipboard (Ubuntu)"
    ALL_OK=false
fi

# Check ydotool
if command_exists ydotool; then
    print_success "ydotool"

    # Check if ydotool service is running
    if ! sudo systemctl is-active --quiet ydotool 2>/dev/null; then
        print_warning "ydotool service not running"
        echo "  ydotool needs root access for keyboard simulation"
        echo "  Run: sudo systemctl enable --now ydotool"
        read -p "  Enable ydotool now? (Y/n): " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Nn]$ ]]; then
            sudo systemctl enable --now ydotool
            print_success "ydotool service started"
        fi
    else
        print_success "ydotool service running"
    fi
else
    print_error "ydotool not found"
    echo "  Install: sudo pacman -S ydotool (Arch) or sudo apt install ydotool (Ubuntu)"
    echo "  After install, run: sudo systemctl enable --now ydotool"
    ALL_OK=false
fi

# Check pactl (PulseAudio/PipeWire)
if command_exists pactl; then
    print_success "pactl (PulseAudio/PipeWire)"
else
    print_warning "pactl not found (needed for microphone selection)"
    echo "  Install: sudo pacman -S pulseaudio or pipewire-pulse"
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
print_header "Step 2/7: Building Project"
echo ""
echo "This will take 2-5 minutes..."
echo ""

if cargo build --release; then
    print_success "Build complete"
    echo ""
    echo "Binaries created:"
    echo "  • transcribe-daemon (transcription service)"
    echo "  • recording-daemon (audio capture)"
    echo "  • hotkey-daemon (handles push-to-talk)"
    echo "  • list-microphones (microphone selection tool)"
else
    print_error "Build failed"
    exit 1
fi

echo ""

# ============================================================================
# STEP 3: Download Model
# ============================================================================
print_header "Step 3/7: Downloading AI Model"
echo ""

MODEL_DIR="models"
MODEL_NAME="parakeet-tdt-0.6b-v3-int8"
MODEL_TAR="parakeet-v3-int8.tar.gz"
MODEL_URL="https://blob.handy.computer/parakeet-v3-int8.tar.gz"

if [ -d "$MODEL_DIR/$MODEL_NAME" ]; then
    print_success "Model already exists"
else
    echo "Downloading Parakeet model (~400MB)..."
    mkdir -p "$MODEL_DIR"
    cd "$MODEL_DIR"

    if command_exists wget; then
        wget -c "$MODEL_URL" || { print_error "Download failed"; exit 1; }
    elif command_exists curl; then
        curl -L -C - -O "$MODEL_URL" || { print_error "Download failed"; exit 1; }
    else
        print_error "Neither wget nor curl found"
        exit 1
    fi

    echo "Extracting..."
    tar -xzf "$MODEL_TAR"
    rm "$MODEL_TAR"
    cd ..
    print_success "Model downloaded"
fi

echo ""

# ============================================================================
# STEP 4: Select Microphone
# ============================================================================
print_header "Step 4/7: Select Your Microphone"
echo ""

SELECTED_MIC="default"

if command_exists pactl; then
    echo "Available microphones:"
    echo ""

    # Show input devices only
    pactl list sources short | grep -i input | nl -w2 -s'. '

    echo ""
    echo "0. Use system default"
    echo ""
    read -p "Select microphone (0-9, or Enter for default): " MIC_CHOICE

    if [ ! -z "$MIC_CHOICE" ] && [ "$MIC_CHOICE" != "0" ]; then
        SELECTED_MIC=$(pactl list sources short | grep -i input | sed -n "${MIC_CHOICE}p" | awk '{print $2}')
        if [ ! -z "$SELECTED_MIC" ]; then
            print_success "Selected: $SELECTED_MIC"
        else
            print_warning "Invalid selection, using default"
            SELECTED_MIC="default"
        fi
    else
        print_info "Using system default microphone"
    fi
else
    print_warning "pactl not available, using default microphone"
fi

echo ""

# ============================================================================
# STEP 5: Create Configuration
# ============================================================================
print_header "Step 5/7: Creating Configuration"
echo ""

CONFIG_DIR="$HOME/.config/transcribe-rs"
CONFIG_FILE="$CONFIG_DIR/config.toml"

mkdir -p "$CONFIG_DIR"

if [ -f "$CONFIG_FILE" ]; then
    print_info "Config exists: $CONFIG_FILE"
    read -p "Overwrite? (y/N): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        ./target/release/transcribe config init 2>/dev/null || true
        print_success "Config recreated"
    fi
else
    ./target/release/transcribe config init 2>/dev/null || true
    print_success "Config created: $CONFIG_FILE"
fi

echo ""

# ============================================================================
# STEP 6: Create Systemd Services
# ============================================================================
print_header "Step 6/7: Setting Up Background Services"
echo ""

mkdir -p "$HOME/.config/systemd/user/"

# Recording Daemon
print_info "Creating recording-daemon.service..."
cat > "$HOME/.config/systemd/user/recording-daemon.service" <<EOF
[Unit]
Description=omarchy-stt Recording Daemon
After=sound.target

[Service]
Type=simple
Environment="RECORDING_MICROPHONE=$SELECTED_MIC"
Environment="RECORDING_BUFFER_SIZE=120"
ExecStart=$PROJECT_DIR/target/release/recording-daemon
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
EOF

# Transcription Daemon
print_info "Creating transcribe-daemon.service..."
cat > "$HOME/.config/systemd/user/transcribe-daemon.service" <<EOF
[Unit]
Description=omarchy-stt Transcription Daemon
After=network.target

[Service]
Type=simple
WorkingDirectory=$PROJECT_DIR
ExecStart=$PROJECT_DIR/target/release/transcribe-daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

# Hotkey Daemon
print_info "Creating hotkey-daemon.service..."
cat > "$HOME/.config/systemd/user/hotkey-daemon.service" <<EOF
[Unit]
Description=omarchy-stt Hotkey Daemon
After=graphical-session.target
Requires=recording-daemon.service transcribe-daemon.service

[Service]
Type=simple
Environment="WAYLAND_DISPLAY=wayland-1"
Environment="XDG_RUNTIME_DIR=/run/user/%U"
ExecStart=$PROJECT_DIR/target/release/hotkey-daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

print_success "Service files created"
echo ""

# Enable and start services
read -p "Enable and start services now? (Y/n): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Nn]$ ]]; then
    systemctl --user daemon-reload
    systemctl --user enable recording-daemon transcribe-daemon hotkey-daemon
    systemctl --user start recording-daemon transcribe-daemon hotkey-daemon

    sleep 2

    # Check status
    echo ""
    FAILED=0
    for service in recording-daemon transcribe-daemon hotkey-daemon; do
        if systemctl --user is-active --quiet $service; then
            print_success "$service running"
        else
            print_error "$service failed to start"
            FAILED=1
        fi
    done

    if [ $FAILED -eq 1 ]; then
        echo ""
        print_warning "Some services failed. Check logs:"
        echo "  journalctl --user -u recording-daemon -n 20"
        echo "  journalctl --user -u transcribe-daemon -n 20"
        echo "  journalctl --user -u hotkey-daemon -n 20"
    fi
else
    print_info "Services created but not started"
    echo "  To start later:"
    echo "    systemctl --user daemon-reload"
    echo "    systemctl --user start recording-daemon transcribe-daemon hotkey-daemon"
fi

echo ""

# ============================================================================
# STEP 7: Hotkey Setup
# ============================================================================
print_header "Step 7/7: Hotkey Setup"
echo ""

print_info "The hotkey-daemon uses XDG Desktop Portal for global shortcuts."
echo ""
echo "On first use, your compositor will ask for permission to register"
echo "a global shortcut. This is normal - click 'Allow'."
echo ""
echo "The default hotkey will be configured via the portal."
echo ""
echo "Alternative: Manual Hyprland keybinding (if portal doesn't work):"
echo "─────────────────────────────────────────────────────────────────"
echo "Add to ~/.config/hypr/hyprland.conf:"
echo ""
echo "  bind = SUPER SHIFT, SPACE, exec, systemctl --user kill -s SIGUSR1 hotkey-daemon"
echo "  bindr = SUPER SHIFT, SPACE, exec, systemctl --user kill -s SIGUSR2 hotkey-daemon"
echo ""
echo "Then reload: hyprctl reload"
echo "─────────────────────────────────────────────────────────────────"
echo ""

# ============================================================================
# Installation Complete
# ============================================================================
print_header "🎉 Installation Complete!"
echo ""
print_success "omarchy-stt is ready to use!"
echo ""
echo "Quick Test:"
echo "  1. Press your hotkey (will show permission dialog first time)"
echo "  2. Say 'Hello world'"
echo "  3. Release hotkey"
echo "  4. Text should appear in your active window"
echo ""
echo "Manage Services:"
echo "  systemctl --user status recording-daemon"
echo "  systemctl --user status transcribe-daemon"
echo "  systemctl --user status hotkey-daemon"
echo ""
echo "  systemctl --user restart <service>  # Restart a service"
echo "  systemctl --user stop <service>     # Stop a service"
echo ""
echo "View Logs:"
echo "  journalctl --user -u hotkey-daemon -f"
echo "  tail -f /tmp/ptt_rust_debug.log"
echo ""
echo "Next Steps:"
echo "  📖 See FEATURES.md for LLM cleanup, corrections, and tools"
echo "  📁 Edit config: nano $CONFIG_FILE"
echo "  🔧 Troubleshooting: README.md#troubleshooting"
echo ""
print_info "If you like this, give it a ⭐ on GitHub!"
echo ""
