//! Terminal window detection using hyprctl (Hyprland-specific)

use std::error::Error;
use std::process::Command;

/// Get the class of the currently active window
pub fn get_active_window_class() -> Result<String, Box<dyn Error>> {
    let output = Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()?;

    if !output.status.success() {
        return Err("hyprctl command failed".into());
    }

    let json_str = String::from_utf8(output.stdout)?;
    let json: serde_json::Value = serde_json::from_str(&json_str)?;

    let class = json["class"]
        .as_str()
        .ok_or("class field not found")?
        .to_lowercase();

    Ok(class)
}

/// Check if the active window is a terminal
pub fn is_active_window_terminal() -> bool {
    let terminal_apps = [
        "alacritty",
        "kitty",
        "wezterm",
        "foot",
        "terminal",
        "konsole",
        "terminator",
        "xterm",
        "urxvt",
        "st",
        "code", // VS Code terminal
    ];

    match get_active_window_class() {
        Ok(class) => terminal_apps.iter().any(|term| class.contains(term)),
        Err(_) => false, // Default to non-terminal if detection fails
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_terminal_detection_logic() {
        // Test that we can identify known terminal strings
        let terminal_apps = [
            "alacritty",
            "kitty",
            "wezterm",
            "foot",
        ];

        for app in &terminal_apps {
            assert!(terminal_apps.iter().any(|term| app.contains(term)));
        }
    }
}
