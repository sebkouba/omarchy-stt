//! Auto-paste functionality using ydotool

use std::error::Error;
use std::fs;
use std::io::Write;
use std::process::Command;
use std::thread;
use std::time::Duration;

/// Append a log message to the debug log
fn log(message: &str) {
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/ptt_rust_debug.log")
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        writeln!(file, "[{}] [paste] {}", timestamp, message).ok();
    }
}

/// Paste from clipboard using ydotool
/// Automatically detects if active window is a terminal and uses appropriate key combo
pub fn paste_from_clipboard() -> Result<(), Box<dyn Error>> {
    log("Paste operation starting...");

    // Set ydotool socket path
    let socket_path = "/tmp/.ydotool_socket";
    std::env::set_var("YDOTOOL_SOCKET", socket_path);
    log(&format!("Set YDOTOOL_SOCKET={}", socket_path));

    // CRITICAL: Give clipboard time to propagate through Wayland compositor
    log("Waiting 50ms for clipboard to propagate...");
    thread::sleep(Duration::from_millis(50));

    // Detect if terminal
    log("Detecting if active window is terminal...");
    let is_terminal = crate::terminal_detect::is_active_window_terminal();
    log(&format!("Is terminal: {}", is_terminal));

    let exit_status = if is_terminal {
        // Terminal: Use Ctrl+Shift+V
        // Key codes: 29 = Left Ctrl, 42 = Left Shift, 47 = V
        log("Using Ctrl+Shift+V for terminal");
        Command::new("ydotool")
            .args(["key", "29:1", "42:1", "47:1", "47:0", "42:0", "29:0"])
            .status()
            .map_err(|e| {
                log(&format!("ERROR: Failed to execute ydotool: {}", e));
                e
            })?
    } else {
        // Non-terminal: Use Ctrl+V
        // Key codes: 29 = Left Ctrl, 47 = V
        log("Using Ctrl+V for non-terminal");
        Command::new("ydotool")
            .args(["key", "29:1", "47:1", "47:0", "29:0"])
            .status()
            .map_err(|e| {
                log(&format!("ERROR: Failed to execute ydotool: {}", e));
                e
            })?
    };

    log(&format!("ydotool exit status: {:?}", exit_status));

    if !exit_status.success() {
        log("ERROR: ydotool command failed");
        return Err("ydotool command failed".into());
    }

    log("Paste operation completed successfully");
    Ok(())
}

/// Check if ydotool is available
pub fn is_ydotool_available() -> bool {
    // ydotool doesn't support --version, use 'help' instead
    Command::new("ydotool")
        .arg("help")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ydotool_check() {
        let available = is_ydotool_available();
        // Just check that the function runs without panicking
        // The result depends on whether ydotool is installed
        println!("ydotool available: {}", available);
    }
}
