//! Auto-paste functionality using ydotool

use std::error::Error;
use std::process::Command;
use std::thread;
use std::time::Duration;

/// Paste from clipboard using ydotool
/// Automatically detects if active window is a terminal and uses appropriate key combo
pub fn paste_from_clipboard() -> Result<(), Box<dyn Error>> {
    // Set ydotool socket path
    std::env::set_var("YDOTOOL_SOCKET", "/tmp/.ydotool_socket");

    // CRITICAL: Give clipboard time to propagate through Wayland compositor
    thread::sleep(Duration::from_millis(50));

    // Detect if terminal
    let is_terminal = crate::terminal_detect::is_active_window_terminal();

    let exit_status = if is_terminal {
        // Terminal: Use Ctrl+Shift+V
        // Key codes: 29 = Left Ctrl, 42 = Left Shift, 47 = V
        Command::new("ydotool")
            .args(["key", "29:1", "42:1", "47:1", "47:0", "42:0", "29:0"])
            .status()?
    } else {
        // Non-terminal: Use Ctrl+V
        // Key codes: 29 = Left Ctrl, 47 = V
        Command::new("ydotool")
            .args(["key", "29:1", "47:1", "47:0", "29:0"])
            .status()?
    };

    if !exit_status.success() {
        return Err("ydotool command failed".into());
    }

    Ok(())
}

/// Check if ydotool is available
pub fn is_ydotool_available() -> bool {
    Command::new("ydotool")
        .arg("--version")
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
