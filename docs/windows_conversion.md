# Windows 11 Conversion Analysis

**Status:** Feasible with moderate complexity
**Estimated Timeline:** 7-11 weeks (1-2 developers)
**Conversion Difficulty:** MEDIUM-HARD
**Deal Breakers:** NONE

---

## Executive Summary

The transcribe-rs-v2 application can be successfully converted to Windows 11 for enterprise distribution as an MSI installer. The core transcription engines (Parakeet/Whisper) and Harper grammar checker are already cross-platform and will work without modification. The primary conversion effort involves replacing 5 Linux-specific subsystems with Windows equivalents:

1. **Clipboard** (wl-copy → arboard/Windows API)
2. **Keyboard simulation** (ydotool → enigo/SendInput API)
3. **Global hotkeys** (Hyprland → RegisterHotKey API or AutoHotkey)
4. **IPC** (Unix sockets → Named Pipes or TCP localhost)
5. **Daemon management** (systemd → Windows Service or Startup)

**All components have viable Windows equivalents.** No fundamental architectural blockers exist.

---

## Table of Contents

1. [Deal Breakers Assessment](#1-deal-breakers-assessment)
2. [Platform-Specific Dependencies](#2-platform-specific-dependencies)
3. [Component Conversion Analysis](#3-component-conversion-analysis)
4. [Windows Integration Challenges](#4-windows-integration-challenges)
5. [MSI Installer Requirements](#5-msi-installer-requirements)
6. [Enterprise Deployment Risks](#6-enterprise-deployment-risks)
7. [Conversion Roadmap](#7-conversion-roadmap)
8. [Technology Stack](#8-technology-stack)
9. [Cost Estimate](#9-cost-estimate)
10. [Final Recommendations](#10-final-recommendations)

---

## 1. Deal Breakers Assessment

### ✅ NO DEAL BREAKERS FOUND

| Component | Linux Implementation | Windows Equivalent | Status |
|-----------|---------------------|-------------------|--------|
| **Transcription** | Parakeet (ONNX) | Parakeet (ONNX) | ✅ Already cross-platform |
| **Transcription** | Whisper (whisper-rs) | Whisper (whisper-rs + Vulkan) | ✅ Already configured in Cargo.toml |
| **Grammar** | Harper | Harper | ✅ Pure Rust, cross-platform |
| **Clipboard** | wl-copy | arboard / clipboard-win | ✅ Easy replacement |
| **Keyboard** | ydotool | enigo / SendInput API | ⚠️ Possible (AV concerns) |
| **Hotkeys** | Hyprland keybinds | RegisterHotKey / AHK | ⚠️ Moderate complexity |
| **Audio** | FFmpeg + PulseAudio | FFmpeg + DirectShow | ✅ Easy replacement |
| **IPC** | Unix sockets | Named Pipes / TCP | ✅ Easy replacement |
| **Daemon** | systemd | Windows Service / Startup | ✅ Multiple options |
| **Notifications** | notify-rust | notify-rust | ✅ Already supports Windows |

**Conclusion:** All critical components can be ported. Main risks are in enterprise deployment (code signing, antivirus, keyboard hooks).

---

## 2. Platform-Specific Dependencies

### 2.1 Cross-Platform Dependencies (No Changes Needed)

These dependencies already work on Windows:

```toml
# Cargo.toml - These work unchanged
hound = "3.5"                    # WAV I/O
ort = "2.0"                      # ONNX Runtime (Parakeet)
whisper-rs = "0.13.2"            # Whisper (has Windows + Vulkan support)
harper-core = "0.12"             # Grammar checking
serde = "1.0"                    # Serialization
serde_json = "1.0"               # JSON protocol
clap = "4.5"                     # CLI parsing
chrono = "0.4"                   # Timestamps
tokio = "1.0"                    # Async runtime
async-openai = "0.28"            # API client
notify-rust = "4.11"             # Desktop notifications (works on Windows!)
```

**Action Required:** None. These crates are cross-platform.

### 2.2 Linux-Specific Dependencies (Must Replace)

```toml
# Currently used on Linux
nix = "0.29"                     # Unix signals, process management
# std::os::unix::net::UnixStream  # Unix domain sockets (in stdlib)
```

**Windows Replacements:**

```toml
[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.58", features = [
    "Win32_System_Threading",              # Process management
    "Win32_UI_Input_KeyboardAndMouse",     # Hotkeys, keyboard sim
    "Win32_UI_WindowsAndMessaging",        # Window detection
    "Win32_Storage_FileSystem",            # Named pipes
    "Win32_Foundation",                    # Base types
]}
arboard = "3.4"                           # Clipboard
enigo = "0.2"                             # Keyboard simulation
```

---

## 3. Component Conversion Analysis

### 3.1 Clipboard (`src/clipboard.rs`)

#### Current Implementation
```rust
// Linux: Uses external wl-copy command
pub fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn Error>> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()?;
    child.stdin.as_mut().unwrap().write_all(text.as_bytes())?;
    child.wait()?;
    Ok(())
}
```

#### Windows Solution
```rust
use arboard::Clipboard;

pub fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn Error>> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text)?;
    Ok(())
}
```

**Alternative:** Use `windows-rs` directly with `SetClipboardData` API.

**Conversion Difficulty:** 🟢 EASY
**Risk Level:** LOW
**Effort:** 30 minutes

**Notes:**
- The docs mention `arboard` had issues on Wayland, but it's stable on Windows
- `clipboard-win` is another mature option
- No external dependencies needed

---

### 3.2 Keyboard Simulation (`src/paste.rs`)

#### Current Implementation
```rust
// Linux: Uses ydotool to simulate key codes
pub fn paste_from_clipboard() -> Result<(), Box<dyn Error>> {
    let is_terminal = terminal_detect::is_active_window_terminal();

    let mut cmd = Command::new("ydotool");
    cmd.arg("key");

    if is_terminal {
        cmd.args(["29:1", "42:1", "47:1", "47:0", "42:0", "29:0"]);  // Ctrl+Shift+V
    } else {
        cmd.args(["29:1", "47:1", "47:0", "29:0"]);  // Ctrl+V
    }

    cmd.output()?;
    sleep(Duration::from_millis(50));  // Wayland compositor delay
    Ok(())
}
```

#### Windows Solution (Option 1: enigo crate)
```rust
use enigo::{Enigo, Key, KeyboardControllable};

pub fn paste_from_clipboard() -> Result<(), Box<dyn Error>> {
    let mut enigo = Enigo::new();
    let is_terminal = is_active_window_terminal();

    if is_terminal {
        enigo.key_down(Key::Control);
        enigo.key_down(Key::Shift);
        enigo.key_click(Key::Layout('v'));
        enigo.key_up(Key::Shift);
        enigo.key_up(Key::Control);
    } else {
        enigo.key_down(Key::Control);
        enigo.key_click(Key::Layout('v'));
        enigo.key_up(Key::Control);
    }

    Ok(())
}
```

#### Windows Solution (Option 2: Native SendInput API)
```rust
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, VK_CONTROL, VK_SHIFT, VK_V
};

pub fn paste_from_clipboard() -> Result<(), Box<dyn Error>> {
    unsafe {
        // Press Ctrl
        send_key(VK_CONTROL.0, 0);
        // Press V
        send_key(VK_V.0, 0);
        // Release V
        send_key(VK_V.0, KEYEVENTF_KEYUP);
        // Release Ctrl
        send_key(VK_CONTROL.0, KEYEVENTF_KEYUP);
    }
    Ok(())
}
```

**Conversion Difficulty:** 🟡 MEDIUM
**Risk Level:** MEDIUM-HIGH
**Effort:** 2-4 hours

**Critical Risks:**
1. **Antivirus False Positives:** Keyboard simulation can trigger AV/EDR alerts
2. **UAC Elevation:** May require admin rights or user approval
3. **Enterprise Restrictions:** Many enterprises block input simulation tools
4. **SendInput Limitations:** May not work with elevated windows (UAC dialogs)

**Mitigation Strategies:**
- Code signing (reduces AV false positives)
- Make auto-paste opt-in (provide "clipboard only" mode)
- Document for IT departments in deployment guide
- Consider user education about AV whitelisting

---

### 3.3 Terminal Detection (`src/terminal_detect.rs`)

#### Current Implementation
```rust
// Linux: Uses Hyprland compositor command
pub fn is_active_window_terminal() -> bool {
    let output = Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()
        .ok()?;

    let json: Value = serde_json::from_slice(&output.stdout).ok()?;
    let class = json["class"].as_str()?;

    TERMINAL_APPS.contains(&class)
}

const TERMINAL_APPS: &[&str] = &["kitty", "alacritty", "wezterm", ...];
```

#### Windows Solution
```rust
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION
};

pub fn is_active_window_terminal() -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));

        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id)
            .ok()?;

        // Get executable name
        let mut buffer = [0u16; 260];
        let mut size = buffer.len() as u32;
        QueryFullProcessImageNameW(process, 0, &mut buffer, &mut size).ok()?;

        let exe_name = String::from_utf16_lossy(&buffer[..size as usize]);
        let exe_name = std::path::Path::new(&exe_name)
            .file_name()?
            .to_str()?
            .to_lowercase();

        TERMINAL_APPS.contains(&exe_name.as_str())
    }
}

const TERMINAL_APPS: &[&str] = &[
    "windowsterminal.exe",
    "wt.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "conemu.exe",
    "conemu64.exe",
];
```

**Conversion Difficulty:** 🟡 MEDIUM
**Risk Level:** LOW-MEDIUM
**Effort:** 2-3 hours

**Notes:**
- Windows has fewer terminal emulators than Linux (simpler list)
- Windows Terminal is the primary modern terminal
- Process name detection is reliable on Windows

---

### 3.4 Audio Recording (`src/recording.rs`, `src/bin/recording-daemon.rs`)

#### Current Implementation
```rust
// Linux: FFmpeg with PulseAudio/ALSA
let microphone = "alsa_input.usb-046d_C922_Pro_Stream_Webcam...";
let mut child = Command::new("ffmpeg")
    .args([
        "-f", "pulse",
        "-i", microphone,
        "-ar", "16000",
        "-ac", "1",
        "-f", "s16le",
        "pipe:1"
    ])
    .spawn()?;

// Read PCM samples from stdout into circular buffer
```

#### Windows Solution (Option 1: FFmpeg with DirectShow)
```rust
// Windows: FFmpeg with DirectShow input
let microphone = "Microphone (Webcam C922 Pro Stream)";
let mut child = Command::new("ffmpeg")
    .args([
        "-f", "dshow",
        "-i", &format!("audio={}", microphone),
        "-ar", "16000",
        "-ac", "1",
        "-f", "s16le",
        "pipe:1"
    ])
    .spawn()?;

// Same circular buffer logic works unchanged
```

**Microphone Enumeration:**
```bash
# List available audio devices
ffmpeg -list_devices true -f dshow -i dummy
```

#### Windows Solution (Option 2: WASAPI with cpal)
```rust
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Host, StreamConfig
};

pub fn start_recording() -> Result<(), Box<dyn Error>> {
    let host = cpal::default_host();
    let device = host.default_input_device()
        .ok_or("No input device available")?;

    let config = StreamConfig {
        channels: 1,
        sample_rate: cpal::SampleRate(16000),
        buffer_size: cpal::BufferSize::Default,
    };

    let stream = device.build_input_stream(
        &config,
        move |data: &[i16], _: &_| {
            // Write to circular buffer
            buffer.write(data);
        },
        |err| eprintln!("Stream error: {}", err),
        None
    )?;

    stream.play()?;
    Ok(())
}
```

**Conversion Difficulty:**
- **Option 1 (FFmpeg):** 🟢 EASY - 1-2 hours
- **Option 2 (WASAPI/cpal):** 🟡 MEDIUM - 1-2 days

**Risk Level:** LOW
**Recommendation:** Use Option 1 (FFmpeg) for MVP, consider Option 2 for production

**Notes:**
- FFmpeg is already cross-platform and well-tested
- DirectShow is Windows' native audio framework
- WASAPI via cpal gives better control but requires more refactoring
- Microphone selection needs UI or config file (no hardcoded names)

---

### 3.5 Desktop Notifications (`src/notifications.rs`)

#### Current Implementation
```rust
use notify_rust::Notification;

pub fn notify_recording_started() -> Result<(), Box<dyn Error>> {
    Notification::new()
        .summary("🎤 Recording")
        .body("Push-to-talk started")
        .show()?;
    Ok(())
}
```

#### Windows Status

**✅ NO CHANGES NEEDED**

The `notify-rust` crate already supports Windows via Windows Toast notifications. The code works as-is on Windows 10/11.

**Conversion Difficulty:** 🟢 NONE
**Risk Level:** NONE
**Effort:** 0 hours

---

### 3.6 Inter-Process Communication (IPC)

#### Current Implementation
```rust
// Linux: Unix domain sockets
use std::os::unix::net::{UnixStream, UnixListener};

// Server (daemon)
let listener = UnixListener::bind("/tmp/transcribe-rs-v2.sock")?;
for stream in listener.incoming() {
    let stream = stream?;
    handle_client(stream);
}

// Client
let mut stream = UnixStream::connect("/tmp/transcribe-rs-v2.sock")?;
writeln!(stream, "{}", json)?;
```

#### Windows Solution (Option 1: Named Pipes - Native)
```rust
// Server
use std::os::windows::io::AsRawHandle;
use windows::Win32::Storage::FileSystem::{
    CreateNamedPipeW, PIPE_ACCESS_DUPLEX, FILE_FLAG_OVERLAPPED
};

let pipe_name = r"\\.\pipe\transcribe-rs-v2";
let handle = unsafe {
    CreateNamedPipeW(
        pipe_name,
        PIPE_ACCESS_DUPLEX,
        // ... other flags
    )?
};

// Client
let mut stream = std::fs::OpenOptions::new()
    .read(true)
    .write(true)
    .open(r"\\.\pipe\transcribe-rs-v2")?;
```

**Recommendation:** Use `named_pipe` crate for easier API.

#### Windows Solution (Option 2: TCP Localhost - Cross-Platform)
```rust
use std::net::{TcpListener, TcpStream};

// Server
let listener = TcpListener::bind("127.0.0.1:47821")?;
for stream in listener.incoming() {
    let stream = stream?;
    handle_client(stream);
}

// Client
let mut stream = TcpStream::connect("127.0.0.1:47821")?;
writeln!(stream, "{}", json)?;
```

**Comparison:**

| Feature | Named Pipes | TCP Localhost |
|---------|------------|---------------|
| Security | Better (filesystem permissions) | Lower (port binding) |
| Performance | Faster (kernel IPC) | Slightly slower (network stack) |
| Portability | Windows-only | Cross-platform |
| Firewall | No issues | May trigger firewall alerts |
| Complexity | Higher (Windows-specific API) | Lower (std::net) |

**Conversion Difficulty:**
- **Named Pipes:** 🟡 MEDIUM - 4-6 hours
- **TCP Localhost:** 🟢 EASY - 1-2 hours

**Risk Level:** LOW for both
**Recommendation:** TCP localhost for MVP, Named Pipes for production

---

### 3.7 File Paths (`/tmp/` directories)

#### Current Implementation
```rust
const AUDIO_FILE: &str = "/tmp/ptt_current.wav";
const PID_FILE: &str = "/tmp/ptt_recording.pid";
const LOG_FILE: &str = "/tmp/ptt_rust_debug.log";
const SOCKET_PATH: &str = "/tmp/transcribe-rs-v2.sock";
```

#### Windows Solution
```rust
use std::env;

pub fn get_temp_dir() -> PathBuf {
    env::temp_dir()  // Returns C:\Users\<user>\AppData\Local\Temp
}

pub fn audio_file_path() -> PathBuf {
    get_temp_dir().join("ptt_current.wav")
}

pub fn log_file_path() -> PathBuf {
    get_temp_dir().join("ptt_rust_debug.log")
}

// For named pipes (if using them)
pub fn socket_path() -> String {
    r"\\.\pipe\transcribe-rs-v2".to_string()
}
```

**Conversion Difficulty:** 🟢 TRIVIAL
**Risk Level:** NONE
**Effort:** 30 minutes (find-replace + testing)

---

## 4. Windows Integration Challenges

### 4.1 Global Hotkey Registration (NEW COMPONENT)

#### The Problem

On Linux, the window manager (Hyprland) handles global hotkeys via config file:
```ini
bind = SUPER SHIFT CTRL ALT, E, exec, transcribe start
bindr = SUPER SHIFT CTRL ALT, E, exec, transcribe stop
```

Windows has no equivalent built-in mechanism. The application must register system-wide hotkeys programmatically.

#### Challenge: Push-to-Talk (Key Press + Release)

The current design relies on:
- **Press hotkey:** Start recording
- **Release hotkey:** Stop recording and transcribe

Windows `RegisterHotKey` API only detects **key press**, not release. Detecting key release requires low-level keyboard hooks.

#### Solution Options

##### Option 1: Native Windows Hotkeys + Keyboard Hook (Recommended for Production)

**Architecture:**
- Create hidden window for message loop
- Use `RegisterHotKey` for initial press detection
- Use `SetWindowsHookEx` (WH_KEYBOARD_LL) for release detection

```rust
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, SetWindowsHookEx, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, MSG, WM_HOTKEY
};

unsafe extern "system" fn keyboard_proc(
    code: i32,
    w_param: WPARAM,
    l_param: LPARAM
) -> LRESULT {
    if code == HC_ACTION {
        let kb = &*(l_param.0 as *const KBDLLHOOKSTRUCT);
        if kb.vkCode == VK_E.0 as u32 {
            if w_param.0 == WM_KEYDOWN {
                // Start recording
            } else if w_param.0 == WM_KEYUP {
                // Stop recording
            }
        }
    }
    CallNextHookEx(None, code, w_param, l_param)
}

pub fn register_hotkey() -> Result<(), Box<dyn Error>> {
    unsafe {
        // Install low-level keyboard hook
        let hook = SetWindowsHookEx(
            WH_KEYBOARD_LL,
            Some(keyboard_proc),
            None,
            0
        )?;

        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            // Process messages
        }
    }
    Ok(())
}
```

**Pros:**
- Native Windows solution
- No external dependencies
- Full control over hotkey behavior

**Cons:**
- **CRITICAL:** Low-level keyboard hooks trigger antivirus alerts
- Requires background process (message loop)
- Complex implementation (150-200 lines)
- May not work with UAC-elevated windows

##### Option 2: AutoHotkey Companion Script (Recommended for MVP)

**Architecture:**
- Bundle AutoHotkey script with MSI installer
- Script calls `transcribe.exe start` / `stop`
- User runs script at startup

```ahk
; transcribe-hotkey.ahk
#Persistent
#SingleInstance Force

; Ctrl+Shift+Alt+E hotkey
^+!e::
    Run, "C:\Program Files\TranscribeRS\transcribe.exe" start
    KeyWait, e  ; Wait for key release
    Run, "C:\Program Files\TranscribeRS\transcribe.exe" stop
return
```

**Pros:**
- Simple implementation (10 lines of AHK)
- No low-level hooks needed
- Works with elevated windows
- Easy to customize

**Cons:**
- Requires AutoHotkey installed (can bundle EXE version)
- Separate process to manage
- Less integrated feel

##### Option 3: System Tray App with UI Hotkey Configuration

**Architecture:**
- Background system tray application
- User configures hotkey via GUI
- Uses low-level hook for detection

**Pros:**
- Most user-friendly
- Visual feedback in tray icon
- Professional appearance

**Cons:**
- Most complex to implement (1-2 weeks)
- Still has AV concerns (keyboard hooks)
- Requires GUI framework (e.g., tauri, egui)

#### Recommendation by Use Case

| Use Case | Recommendation | Rationale |
|----------|---------------|-----------|
| **MVP / Initial Release** | Option 2 (AutoHotkey) | Fastest to market, lowest risk |
| **Production / v1.0** | Option 3 (System Tray) | Best UX, professional |
| **Power Users** | Option 1 (Native) | Most control, no dependencies |

**Conversion Difficulty:** 🔴 HIGH
**Risk Level:** HIGH (AV concerns)
**Effort:** 1-2 weeks (Option 3), 2-3 days (Option 1), 2-4 hours (Option 2)

---

### 4.2 Daemon Management (Systemd → Windows)

#### Current Implementation

```bash
# Linux: systemd user service
systemctl --user enable transcribe-daemon
systemctl --user start transcribe-daemon
```

Service file:
```ini
[Unit]
Description=Transcription Daemon

[Service]
ExecStart=/path/to/transcribe-daemon
Restart=always

[Install]
WantedBy=default.target
```

#### Windows Solutions

##### Option 1: Windows Service (Enterprise-Ready)

**Pros:**
- Runs in background (no user login needed)
- Auto-start on boot
- Service control manager integration
- Professional for enterprise

**Cons:**
- Requires admin rights to install
- Complex setup process
- User isolation challenges (which user's microphone?)

**Implementation:**
```rust
use windows_service::*;

fn main() -> Result<(), Box<dyn Error>> {
    // Define service
    let service_name = "TranscribeDaemon";
    let service_main = move |_args: Vec<String>| {
        // Service entry point
        run_daemon()
    };

    service_dispatcher::start(service_name, service_main)?;
    Ok(())
}
```

**Not Recommended:** Transcription needs user context (microphone, clipboard). Services run in different session.

##### Option 2: Startup Folder (Recommended)

**Pros:**
- No admin rights required
- Runs in user context (microphone works)
- Simple installation
- Easy for users to disable

**Cons:**
- Only runs when user logs in
- Less "enterprise" feel
- Visible in Task Manager

**Implementation:**
```rust
use std::env;

pub fn install_startup() -> Result<(), Box<dyn Error>> {
    let startup_path = PathBuf::from(env::var("APPDATA")?)
        .join(r"Microsoft\Windows\Start Menu\Programs\Startup")
        .join("transcribe-daemon.lnk");

    // Create shortcut to transcribe-daemon.exe
    create_shortcut(&startup_path, current_exe_path())?;
    Ok(())
}
```

**Recommendation:** This is the right approach for transcribe-rs.

##### Option 3: Task Scheduler (Best of Both Worlds)

**Pros:**
- Runs in user context
- More robust than startup folder
- Can run at login even if startup folder disabled
- No admin required (user-level task)

**Cons:**
- More complex to set up
- Requires XML task definition

**Implementation:**
```rust
use std::process::Command;

pub fn install_task() -> Result<(), Box<dyn Error>> {
    // Create scheduled task via schtasks.exe
    Command::new("schtasks")
        .args([
            "/Create",
            "/TN", "TranscribeDaemon",
            "/TR", r"C:\Program Files\TranscribeRS\transcribe-daemon.exe",
            "/SC", "ONLOGON",
            "/RL", "LIMITED",
        ])
        .output()?;
    Ok(())
}
```

#### Recommendation

**For MVP:** Option 2 (Startup Folder) - simplest, works reliably
**For Production:** Option 3 (Task Scheduler) - more robust, better for enterprise

**Conversion Difficulty:** 🟢 EASY (Option 2), 🟡 MEDIUM (Option 3)
**Risk Level:** LOW
**Effort:** 2-3 hours

---

## 5. MSI Installer Requirements

### 5.1 WiX Toolset Setup

**WiX Toolset v4** is the industry standard for creating Windows installers.

#### Installation
```bash
# Install WiX Toolset
dotnet tool install --global wix

# Verify installation
wix --version
```

#### Project Structure
```
transcribe-rs-v2/
├── Cargo.toml
├── wix/
│   ├── main.wxs              # Main installer definition
│   ├── ui.wxs                # Custom UI dialogs
│   ├── bundle.wxs            # Optional: Bootstrap for runtime dependencies
│   └── License.rtf           # License agreement
└── target/
    └── release/
        ├── transcribe.exe
        ├── transcribe-daemon.exe
        └── transcribe-client.exe
```

### 5.2 MSI Components

#### Minimal WiX Definition (`wix/main.wxs`)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
  <Product
    Id="*"
    Name="TranscribeRS"
    Version="!(bind.FileVersion.transcribe.exe)"
    Manufacturer="Your Company"
    Language="1033">

    <Package
      InstallerVersion="500"
      Compressed="yes"
      InstallScope="perUser"
      Description="AI-Powered Dictation for Windows"
    />

    <MajorUpgrade
      DowngradeErrorMessage="A newer version is already installed."
      AllowSameVersionUpgrades="yes"
    />

    <MediaTemplate EmbedCab="yes" />

    <!-- Installation Directory -->
    <Directory Id="TARGETDIR" Name="SourceDir">
      <Directory Id="LocalAppDataFolder">
        <Directory Id="INSTALLFOLDER" Name="TranscribeRS">

          <!-- Binaries -->
          <Component Id="MainExecutable" Guid="PUT-GUID-HERE">
            <File
              Id="transcribe.exe"
              Source="$(var.ProjectDir)\target\release\transcribe.exe"
              KeyPath="yes"
            />
          </Component>

          <Component Id="DaemonExecutable" Guid="PUT-GUID-HERE">
            <File
              Id="transcribe_daemon.exe"
              Source="$(var.ProjectDir)\target\release\transcribe-daemon.exe"
              KeyPath="yes"
            />
          </Component>

          <Component Id="ClientExecutable" Guid="PUT-GUID-HERE">
            <File
              Id="transcribe_client.exe"
              Source="$(var.ProjectDir)\target\release\transcribe-client.exe"
              KeyPath="yes"
            />
          </Component>

          <!-- Configuration -->
          <Directory Id="ConfigDir" Name="config">
            <Component Id="DefaultConfig" Guid="PUT-GUID-HERE">
              <File
                Source="config.example.toml"
                Name="config.toml"
                KeyPath="yes"
              />
            </Component>
          </Directory>

          <!-- Models (if bundling) -->
          <Directory Id="ModelsDir" Name="models">
            <!-- WARNING: Parakeet model is 200-300MB -->
            <!-- Consider on-demand download instead -->
          </Directory>

        </Directory>
      </Directory>

      <!-- Startup Folder Shortcut -->
      <Directory Id="StartupFolder">
        <Component Id="StartupShortcut" Guid="PUT-GUID-HERE">
          <Shortcut
            Id="DaemonStartup"
            Name="TranscribeRS Daemon"
            Target="[INSTALLFOLDER]transcribe-daemon.exe"
            WorkingDirectory="INSTALLFOLDER"
          />
          <RegistryValue
            Root="HKCU"
            Key="Software\TranscribeRS"
            Name="StartupInstalled"
            Type="integer"
            Value="1"
            KeyPath="yes"
          />
        </Component>
      </Directory>
    </Directory>

    <!-- Feature Definition -->
    <Feature Id="Complete" Level="1">
      <ComponentRef Id="MainExecutable" />
      <ComponentRef Id="DaemonExecutable" />
      <ComponentRef Id="ClientExecutable" />
      <ComponentRef Id="DefaultConfig" />
      <ComponentRef Id="StartupShortcut" />
    </Feature>

    <!-- UI -->
    <UIRef Id="WixUI_InstallDir" />
    <Property Id="WIXUI_INSTALLDIR" Value="INSTALLFOLDER" />

  </Product>
</Wix>
```

#### Build Process
```bash
# Build Rust binaries
cargo build --release

# Build MSI
cd wix
wix build main.wxs -o TranscribeRS-0.1.0.msi

# Sign MSI (REQUIRED for enterprise)
signtool sign /f certificate.pfx /p password /tr http://timestamp.digicert.com TranscribeRS-0.1.0.msi
```

### 5.3 Code Signing (CRITICAL)

#### Why Code Signing is Non-Negotiable

1. **Windows SmartScreen:** Unsigned executables show scary warning ("Windows protected your PC")
2. **Enterprise Policy:** Most enterprises block unsigned software
3. **Antivirus:** Signed binaries have lower false positive rates
4. **User Trust:** Professional software is always signed

#### Obtaining a Certificate

**Option 1: Standard Code Signing Certificate**
- **Providers:** DigiCert, Sectigo, GlobalSign
- **Cost:** $300-500/year
- **Validation:** Organization validation required
- **Delivery:** USB token or file-based (.pfx)
- **Timeline:** 3-7 days

**Option 2: Extended Validation (EV) Certificate**
- **Cost:** $500-800/year
- **Benefit:** Immediate SmartScreen reputation (no warning)
- **Requirement:** Higher validation standards
- **Delivery:** Hardware token only
- **Timeline:** 1-2 weeks

**Recommendation:** Start with Standard, upgrade to EV for enterprise sales.

#### Signing Process
```bash
# Sign all executables
signtool sign /f cert.pfx /p password /tr http://timestamp.digicert.com ^
    target\release\transcribe.exe ^
    target\release\transcribe-daemon.exe ^
    target\release\transcribe-client.exe

# Sign MSI installer
signtool sign /f cert.pfx /p password /tr http://timestamp.digicert.com ^
    wix\TranscribeRS-0.1.0.msi

# Verify signatures
signtool verify /pa TranscribeRS-0.1.0.msi
```

**Timeline Impact:** Budget 1-2 weeks for certificate purchase and validation.

### 5.4 Model Distribution Strategy

**Problem:** Parakeet model is 200-300 MB. Including it in MSI makes installer huge.

**Options:**

1. **Bundle in MSI** (simplest)
   - Pro: Works offline
   - Con: 300MB installer size
   - Con: Slow download/install

2. **Download on First Run** (recommended)
   ```rust
   pub async fn ensure_model_downloaded() -> Result<(), Box<dyn Error>> {
       let model_path = get_model_path();
       if !model_path.exists() {
           download_model_with_progress(&model_path).await?;
       }
       Ok(())
   }
   ```
   - Pro: Small installer (20-30 MB)
   - Pro: Can update model independently
   - Con: Requires internet on first use

3. **Separate Model Pack MSI**
   - Pro: Optional offline installation
   - Con: More complex distribution
   - Con: User confusion (two installers?)

**Recommendation:** Option 2 (download on first run) with fallback to manual download link.

---

## 6. Enterprise Deployment Risks

### 6.1 Security & Compliance

| Risk | Severity | Impact | Mitigation |
|------|----------|---------|------------|
| **Unsigned executables** | 🔴 CRITICAL | Blocked by SmartScreen | Purchase code signing cert immediately |
| **Keyboard simulation** | 🟠 HIGH | EDR alerts, policy blocks | Make opt-in, provide "clipboard only" mode |
| **Low-level keyboard hooks** | 🟠 HIGH | AV false positives | Code signing + submit to AV vendors |
| **Network access (Groq API)** | 🟡 MEDIUM | Firewall blocks | Document, add proxy support |
| **Microphone access** | 🟡 MEDIUM | Privacy concerns | Windows permission dialog, clear UX |
| **Local model inference** | 🟢 LOW | Resource usage | Document requirements (4GB RAM, CPU) |

### 6.2 Installation & Distribution

| Risk | Severity | Impact | Mitigation |
|------|----------|---------|------------|
| **Admin rights required** | 🟡 MEDIUM | User can't install | Support per-user install (LocalAppData) |
| **Large installer size** | 🟡 MEDIUM | Slow downloads | Download models on first run |
| **AV false positives** | 🟠 HIGH | Installation blocked | Code signing + VirusTotal submission |
| **Windows Defender SmartScreen** | 🔴 CRITICAL | "Unknown publisher" warning | EV code signing certificate |
| **Software restriction policies** | 🟡 MEDIUM | Enterprise blocks unknown apps | Provide SHA256 hashes, SBOM |
| **Network installation** | 🟢 LOW | MSI doesn't work over network | Test network install scenarios |

### 6.3 Runtime & Operational

| Risk | Severity | Impact | Mitigation |
|------|----------|---------|------------|
| **Daemon crashes** | 🟡 MEDIUM | Service stops working | Auto-restart on failure |
| **Firewall blocks localhost** | 🟢 LOW | IPC fails | Use named pipes instead of TCP |
| **Process monitoring flags daemon** | 🟡 MEDIUM | EDR quarantines | Document for IT, whitelist guidance |
| **High CPU during inference** | 🟢 LOW | Performance complaints | Document requirements, optimize |
| **High memory usage (300MB)** | 🟢 LOW | Resource constraints | Document requirements |
| **GPU driver compatibility** | 🟡 MEDIUM | Inference fails | Fallback to CPU, clear error messages |
| **Conflicting hotkeys** | 🟡 MEDIUM | Hotkey doesn't work | Configurable hotkey in UI |
| **Model update breaks compatibility** | 🟢 LOW | Inference errors | Version model format, compatibility check |

### 6.4 Support & Maintenance

| Risk | Severity | Impact | Mitigation |
|------|----------|---------|------------|
| **Windows updates break app** | 🟡 MEDIUM | Functionality lost | Automated testing on Windows Insider |
| **User misconfiguration** | 🟡 MEDIUM | Support burden | Robust defaults, `transcribe doctor` command |
| **Difficult troubleshooting** | 🟡 MEDIUM | Support costs | Comprehensive logging, diagnostic tool |
| **Update distribution** | 🟡 MEDIUM | Users on old versions | Auto-update check (optional) |

---

## 7. Conversion Roadmap

### Phase 1: Core Platform Porting (2-3 weeks)

**Objective:** Get basic transcription working on Windows without hotkeys.

#### Week 1: Foundation
- [ ] Create platform abstraction layer (`src/platform/mod.rs`)
- [ ] Add Windows-specific dependencies to `Cargo.toml`
- [ ] Replace `/tmp/` paths with `env::temp_dir()`
- [ ] Implement Windows clipboard (arboard)
- [ ] Implement Windows keyboard simulation (enigo)
- [ ] Test: Manual clipboard copy works

#### Week 2: IPC & Audio
- [ ] Implement TCP localhost IPC (replace Unix sockets)
- [ ] Test: Daemon <-> Client communication on Windows
- [ ] Implement FFmpeg with DirectShow audio capture
- [ ] Test: Audio recording produces valid 16kHz mono WAV
- [ ] Update config loading for Windows paths (`%APPDATA%`)

#### Week 3: Integration Testing
- [ ] Port recording-daemon to Windows
- [ ] Port transcribe-daemon to Windows
- [ ] Test: Full workflow without hotkeys
  - Manually run `transcribe start`
  - Speak
  - Manually run `transcribe stop`
  - Verify text appears in clipboard
- [ ] Fix bugs found during testing
- [ ] Document known issues

**Deliverable:** Windows binaries that work when invoked from command line.

---

### Phase 2: Windows Integration (2-3 weeks)

**Objective:** Add Windows-specific features for usability.

#### Week 4: Hotkey System (MVP)
- [ ] Option 2: Create AutoHotkey script
- [ ] Bundle AHK script with binaries
- [ ] Test: Hotkey triggers start/stop
- [ ] Document hotkey customization

**OR**

- [ ] Option 1: Implement native RegisterHotKey + keyboard hook
- [ ] Create hidden window for message loop
- [ ] Register global hotkey
- [ ] Detect key release via keyboard hook
- [ ] Test: Push-to-talk works

#### Week 5: Daemon Management
- [ ] Implement startup folder installation
- [ ] Add "Install to Startup" command to CLI
- [ ] Add "Uninstall from Startup" command to CLI
- [ ] Test: Daemon auto-starts on login
- [ ] Implement daemon health check

#### Week 6: System Tray (Optional)
- [ ] Create system tray icon (using `tray-icon` crate)
- [ ] Add menu: Start/Stop, Settings, Exit
- [ ] Show recording state in icon
- [ ] Test: User can control via tray

**Deliverable:** Windows application with hotkey support and auto-start.

---

### Phase 3: MSI Installer & Enterprise Prep (2-3 weeks)

**Objective:** Create professional installer for enterprise deployment.

#### Week 7: WiX Installer
- [ ] Install WiX Toolset v4
- [ ] Create `wix/main.wxs` with binary components
- [ ] Add startup shortcut component
- [ ] Add config file handling
- [ ] Test: MSI installs and uninstalls cleanly
- [ ] Test: Repair and upgrade scenarios

#### Week 8: Code Signing & Security
- [ ] **Purchase code signing certificate** (START EARLY!)
- [ ] Set up signing infrastructure
- [ ] Sign all executables
- [ ] Sign MSI installer
- [ ] Test: Signed MSI installs without warnings
- [ ] Submit to VirusTotal
- [ ] Request whitelisting from major AV vendors

#### Week 9: Testing & Documentation
- [ ] Test on clean Windows 11 VM
- [ ] Test on Windows 10
- [ ] Test with Windows Defender
- [ ] Test with enterprise AV (if available)
- [ ] Write admin deployment guide
- [ ] Write user guide
- [ ] Write troubleshooting guide
- [ ] Create video tutorial

**Deliverable:** Signed MSI installer ready for distribution.

---

### Phase 4: Production Hardening (1-2 weeks)

**Objective:** Polish for enterprise sales.

#### Week 10: Advanced Features
- [ ] Replace TCP with Named Pipes (security)
- [ ] Add proxy configuration for Groq API
- [ ] Implement configurable hotkeys
- [ ] Add telemetry opt-in (for diagnostics)
- [ ] Implement auto-update check

#### Week 11: CI/CD & Release
- [ ] Set up GitHub Actions for Windows builds
- [ ] Automate MSI generation
- [ ] Automate signing in CI
- [ ] Create release workflow
- [ ] Generate SHA256 hashes
- [ ] Create SBOM (Software Bill of Materials)

**Deliverable:** Production-ready MSI with automated release process.

---

### Total Timeline: 7-11 weeks

| Phase | Duration | Confidence |
|-------|----------|------------|
| Phase 1: Core Porting | 2-3 weeks | High |
| Phase 2: Windows Integration | 2-3 weeks | Medium-High |
| Phase 3: MSI & Enterprise | 2-3 weeks | Medium (certificate dependent) |
| Phase 4: Production Polish | 1-2 weeks | Medium |
| **Total** | **7-11 weeks** | **Medium-High** |

**Critical Path:** Code signing certificate purchase (1-2 weeks lead time).

---

## 8. Technology Stack

### 8.1 Windows-Specific Dependencies

Add to `Cargo.toml`:

```toml
[target.'cfg(target_os = "windows")'.dependencies]
# Windows API bindings
windows = { version = "0.58", features = [
    "Win32_System_Threading",              # Process management, DLLs
    "Win32_UI_Input_KeyboardAndMouse",     # Hotkeys, SendInput
    "Win32_UI_WindowsAndMessaging",        # Message loop, window detection
    "Win32_Storage_FileSystem",            # Named pipes, file operations
    "Win32_Foundation",                    # Base types (HANDLE, BOOL, etc.)
    "Win32_System_Diagnostics_ToolHelp",   # Process enumeration
]}

# Cross-platform crates with Windows support
arboard = "3.4"                            # Clipboard
enigo = { version = "0.2", features = ["serde"] }  # Keyboard simulation
tray-icon = "0.16"                         # System tray (optional)
windows-service = "0.7"                    # Windows Services (optional)

# Audio (if not using FFmpeg)
cpal = "0.15"                              # Cross-platform audio I/O

# Named pipes (if not using TCP)
named-pipe = "0.4"                         # Easier than raw Windows API

[build-dependencies]
# Embed icon and version info in EXE
winresource = "0.1"
```

### 8.2 Cross-Platform Abstraction

Create `src/platform/mod.rs`:

```rust
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

// Platform-agnostic interface
pub trait ClipboardProvider {
    fn copy(&mut self, text: &str) -> Result<(), Box<dyn Error>>;
}

pub trait KeyboardProvider {
    fn paste(&mut self) -> Result<(), Box<dyn Error>>;
}

pub trait IpcProvider {
    fn connect(path: &str) -> Result<Box<dyn Read + Write>, Box<dyn Error>>;
    fn bind(path: &str) -> Result<Box<dyn Iterator<Item = Box<dyn Read + Write>>>, Box<dyn Error>>;
}

// Factory functions
pub fn get_clipboard() -> Box<dyn ClipboardProvider> {
    #[cfg(target_os = "linux")]
    { Box::new(linux::LinuxClipboard::new()) }

    #[cfg(target_os = "windows")]
    { Box::new(windows::WindowsClipboard::new()) }
}
```

This allows main code to remain platform-agnostic.

### 8.3 Build Configuration

Update `build.rs`:

```rust
fn main() {
    // Embed version info and icon on Windows
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "TranscribeRS");
        res.set("FileDescription", "AI-Powered Dictation");
        res.compile().unwrap();
    }

    // Set build number
    println!("cargo:rustc-env=BUILD_NUMBER={}",
        env::var("BUILD_NUMBER").unwrap_or("0".to_string())
    );
}
```

### 8.4 Development Environment

**Required Tools:**

1. **Rust Toolchain:**
   ```bash
   rustup target add x86_64-pc-windows-msvc
   rustup toolchain install stable-x86_64-pc-windows-msvc
   ```

2. **Visual Studio Build Tools:**
   - Download from Microsoft (free)
   - Needed for whisper-rs compilation
   - Required components: MSVC v143, Windows 11 SDK

3. **WiX Toolset v4:**
   ```bash
   dotnet tool install --global wix
   ```

4. **Code Signing Tools:**
   - Windows SDK (includes signtool.exe)
   - Or Visual Studio

5. **Testing VMs:**
   - Windows 11 Enterprise (90-day eval)
   - Windows 10 (for compatibility testing)

---

## 9. Cost Estimate

### 9.1 One-Time Costs

| Item | Cost Range | Notes |
|------|------------|-------|
| **Code Signing Certificate** | $300-800 | Standard: $300-500, EV: $500-800 (annual) |
| **Development Labor** | $30,000-60,000 | 7-11 weeks @ $100-150/hr (1 developer) |
| **Windows Development VM** | $0 | Free 90-day Windows 11 Enterprise eval |
| **Testing Devices** | $0-1,000 | Use personal devices or buy test laptop |
| **WiX Toolset** | $0 | Free and open source |
| **Visual Studio Build Tools** | $0 | Free |
| **Domain/Hosting** | $50-200 | If not using GitHub Releases |

**Total One-Time:** $30,350-62,000 (dominated by development labor)

### 9.2 Annual Recurring Costs

| Item | Cost/Year | Notes |
|------|-----------|-------|
| **Code Signing Renewal** | $300-800 | Required annually |
| **File Hosting** | $0-100 | Free if using GitHub Releases |
| **Support/Maintenance** | $5,000-20,000 | Bug fixes, Windows updates (1-2 weeks/year) |
| **AV Vendor Submissions** | $0 | Usually free, but time-consuming |

**Total Annual:** $5,300-20,900

### 9.3 Break-Even Analysis for Enterprise Sales

**Assumptions:**
- Initial development cost: $40,000
- Annual costs: $10,000
- License price: $200/user/year

**Break-even:**
- Year 1: 250 licenses ($50,000 revenue)
- Year 2+: 50 licenses/year ($10,000 revenue)

**Note:** Adjust pricing based on market research. Enterprise dictation tools typically range from $200-1,000/user/year.

---

## 10. Final Recommendations

### 10.1 For MVP (6-8 weeks)

**Recommended Approach:**

1. **✅ START IMMEDIATELY:** Purchase code signing certificate (1-2 week lead time)

2. **Phase 1 (Weeks 1-3):** Core Porting
   - Replace clipboard (arboard)
   - Replace keyboard (enigo)
   - Replace IPC (TCP localhost)
   - Replace audio (FFmpeg + DirectShow)
   - Manual testing: `transcribe start/stop` works

3. **Phase 2 (Weeks 4-5):** Windows Integration
   - Bundle AutoHotkey script for hotkeys (MVP approach)
   - Startup folder integration
   - Basic system tray icon (optional)

4. **Phase 3 (Weeks 6-8):** Distribution
   - Create WiX installer
   - Sign all binaries
   - Test on clean Windows VM
   - Write documentation

**MVP Feature Set:**
- ✅ Local transcription (Parakeet/Whisper)
- ✅ Push-to-talk via hotkey (AutoHotkey)
- ✅ Clipboard copy
- ✅ Auto-paste
- ✅ Desktop notifications
- ✅ Auto-start daemon
- ✅ Signed MSI installer
- ❌ System tray UI (v2.0)
- ❌ Hotkey configuration UI (v2.0)
- ❌ Named Pipes (v2.0)

**Timeline:** 6-8 weeks (confident)

---

### 10.2 For Enterprise Production (11-14 weeks)

**Add These Features:**

1. **Security Hardening:**
   - Replace TCP localhost with Named Pipes
   - Add telemetry opt-out
   - Comprehensive audit logging
   - Proxy support for Groq API

2. **Professional UI:**
   - System tray application
   - Settings GUI
   - Hotkey configuration UI
   - Microphone selection UI

3. **Enterprise Features:**
   - MSI transforms for silent install
   - Group Policy support (registry-based config)
   - Centralized logging option
   - License key validation (if applicable)

4. **Documentation:**
   - Admin deployment guide
   - IT security whitepaper
   - Troubleshooting guide
   - API documentation (for integration)

**Timeline:** 11-14 weeks (medium confidence)

---

### 10.3 Critical Success Factors

#### 1. Code Signing (🔴 BLOCKER)

**Action:** Purchase certificate **immediately** before starting development.

**Rationale:** Without code signing:
- Windows SmartScreen will block installation
- Enterprise IT won't approve
- Antivirus false positive rate is 10-20x higher

**Cost:** $300-500 (Standard) or $500-800 (EV)

**Timeline Impact:** 1-2 weeks for certificate validation

---

#### 2. Keyboard Simulation Testing (🟠 HIGH RISK)

**Action:** Test keyboard simulation on locked-down enterprise Windows **early** (Week 2-3).

**Rationale:** If enterprise policies block input simulation, need fallback plan.

**Fallback Options:**
- Clipboard-only mode (no auto-paste)
- Custom paste hotkey instead of global recording hotkey
- Partnership with IT for whitelisting

---

#### 3. AV False Positive Mitigation (🟠 HIGH RISK)

**Action:**
1. Code sign all binaries
2. Submit to VirusTotal immediately after MVP
3. Request whitelisting from major vendors (Windows Defender, Norton, McAfee, Kaspersky)

**Timeline:** Allow 2-4 weeks for AV vendor review

---

#### 4. Early Enterprise Pilot (🟡 MEDIUM RISK)

**Action:** Find 1-2 enterprise pilot customers before full launch.

**Benefits:**
- Real-world testing of deployment process
- Feedback on IT approval requirements
- Validation of security model
- Case studies for marketing

---

### 10.4 Go/No-Go Decision

**Recommendation: ✅ GO**

**Confidence Level: 80%**

#### Reasons to Proceed:

1. **No Technical Blockers:** All Linux components have Windows equivalents
2. **Proven Market:** Enterprise dictation tools exist and succeed (Dragon, Talon)
3. **Core Tech Works:** Parakeet/Whisper already support Windows
4. **Reasonable Timeline:** 7-11 weeks is achievable with 1-2 developers
5. **Manageable Risks:** All risks have mitigation strategies

#### Key Risks (Manageable):

1. **Keyboard Simulation:** May need clipboard-only fallback
2. **Code Signing Cost:** $300-800/year is reasonable for enterprise product
3. **AV False Positives:** Mitigated by signing + vendor submissions
4. **Enterprise Approval:** Mitigated by documentation + pilot customers

#### Unknown Risks (Validate Early):

1. **Enterprise Policy Restrictions:** Test on real enterprise Windows by Week 3
2. **Performance on Windows:** Benchmark Parakeet inference on Windows vs Linux
3. **Audio Driver Compatibility:** Test with various USB microphones
4. **Hotkey Conflicts:** May need configurable hotkeys

---

### 10.5 Decision Tree

```
START
  ↓
Q: Do you have $300-800 for code signing?
  ├─ No → STOP (code signing is mandatory for enterprise)
  └─ Yes → Continue
           ↓
Q: Can you dedicate 7-11 weeks for development?
  ├─ No → Consider hiring contractor
  └─ Yes → Continue
           ↓
Q: Do you have access to enterprise Windows for testing?
  ├─ No → Find pilot customer early (Week 3)
  └─ Yes → Continue
           ↓
Q: Is keyboard simulation a must-have feature?
  ├─ Yes → Test on enterprise Windows early (RISK)
  └─ No → Continue (clipboard-only mode is safe)
           ↓
✅ GO FOR IT!
```

---

### 10.6 Next Steps (Week 1)

**Day 1:**
1. ⚠️ **Purchase code signing certificate** (critical path)
2. Set up Windows 11 development VM
3. Install Visual Studio Build Tools
4. Install WiX Toolset

**Day 2:**
1. Create `src/platform/` module structure
2. Add Windows dependencies to `Cargo.toml`
3. Implement clipboard wrapper (arboard)
4. Test: Copy "Hello" to clipboard on Windows

**Day 3:**
1. Implement keyboard simulation (enigo)
2. Test: Simulate Ctrl+V on Windows
3. Implement file path helpers (`env::temp_dir()`)

**Day 4:**
1. Implement TCP localhost IPC
2. Test: Simple echo server/client on Windows
3. Port client.rs to Windows

**Day 5:**
1. Port daemon.rs to Windows
2. Test: Full daemon <-> client workflow
3. Document findings and blockers

**End of Week 1:** Decision point - are there any unexpected blockers?

---

## Conclusion

Converting transcribe-rs-v2 to Windows 11 is **feasible and recommended**. The architecture is sound, all Linux components have Windows equivalents, and the transcription engines are already cross-platform. The main challenges are:

1. **Code signing** (mandatory, $300-800/year)
2. **Keyboard simulation** (may need fallback)
3. **Global hotkeys** (moderate complexity)

With proper planning and early risk mitigation, a production-ready MSI installer can be delivered in **7-11 weeks** for **$30k-60k** in development costs.

**The biggest unknown is keyboard simulation behavior in locked-down enterprise environments.** Recommend early testing with enterprise pilot customers to validate this critical feature.

**Start immediately with code signing certificate purchase** - it's on the critical path.

---

## Appendix A: Similar Products

**Existing Windows dictation tools for comparison:**

1. **Dragon NaturallySpeaking** ($300-500)
   - Market leader, proprietary models
   - Deep Windows integration
   - Shows market demand exists

2. **Talon Voice** (Free, open source)
   - Code-by-voice tool
   - Uses Dragon or wav2letter models
   - Proves open-source can compete

3. **Vocola** (Free)
   - Voice macro system
   - Works with Dragon
   - Shows extensibility matters

4. **Windows Speech Recognition** (Built-in)
   - Basic, but shows baseline UX
   - Often inadequate → creates market opportunity

**Key Takeaway:** Premium dictation market exists on Windows. Enterprise customers pay $300-1,000/year for quality tools.

---

## Appendix B: Testing Checklist

**Pre-Release Testing Matrix:**

| Test Scenario | Windows 11 | Windows 10 | Notes |
|---------------|-----------|-----------|-------|
| Clean install | ☐ | ☐ | Fresh VM, no dev tools |
| Upgrade install | ☐ | ☐ | Over previous version |
| Uninstall | ☐ | ☐ | No leftovers |
| Repair | ☐ | ☐ | Via MSI |
| Recording 1s audio | ☐ | ☐ | Edge case |
| Recording 30s audio | ☐ | ☐ | Normal case |
| Recording 2m audio | ☐ | ☐ | Buffer limit |
| USB microphone | ☐ | ☐ | Logitech C922 |
| Built-in microphone | ☐ | ☐ | Laptop mic |
| Bluetooth microphone | ☐ | ☐ | Wireless |
| Hotkey conflicts | ☐ | ☐ | With other apps |
| Terminal detection | ☐ | ☐ | PowerShell, cmd, Windows Terminal |
| GUI detection | ☐ | ☐ | Notepad, Word, Chrome |
| Fast repeated dictations | ☐ | ☐ | <1s between recordings |
| Daemon crash recovery | ☐ | ☐ | Kill daemon process |
| Network disconnected | ☐ | ☐ | If using remote models |
| Firewall enabled | ☐ | ☐ | Windows Firewall |
| Windows Defender | ☐ | ☐ | No false positives |
| 3rd-party AV | ☐ | ☐ | Norton, McAfee, Kaspersky |
| Low disk space | ☐ | ☐ | <100MB free |
| Low memory | ☐ | ☐ | 4GB RAM system |
| Sleep/Resume | ☐ | ☐ | Daemon survives |
| User logout/login | ☐ | ☐ | Daemon restarts |
| Windows updates | ☐ | ☐ | Survives reboot |

---

## Appendix C: Resources

**Documentation:**
- Windows API: https://learn.microsoft.com/en-us/windows/win32/
- windows-rs crate: https://github.com/microsoft/windows-rs
- WiX Toolset: https://wixtoolset.org/docs/
- Code Signing: https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool

**Community:**
- Rust Windows Discord: https://discord.gg/rust-lang
- WiX Users Mailing List: https://wixtoolset.org/documentation/
- r/rust_gamedev (Windows dev tips)

**Similar Projects (Study These):**
- Talon Voice: https://talonvoice.com/ (open source, Python + Rust)
- Numen: https://numenvoice.org/ (voice coding)
- Whisper.cpp Windows port: https://github.com/ggerganov/whisper.cpp

**Certificate Providers:**
- DigiCert: https://www.digicert.com/code-signing
- Sectigo: https://sectigo.com/ssl-certificates-tls/code-signing
- GlobalSign: https://www.globalsign.com/en/code-signing-certificate

---

*Document Version: 1.0*
*Last Updated: 2025-11-05*
*Author: Architecture Analysis*
