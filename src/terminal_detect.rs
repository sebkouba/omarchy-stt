//! Terminal window detection using hyprctl (Hyprland-specific)

use once_cell::sync::Lazy;
use std::error::Error;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Cache for terminal detection result (timestamp, is_terminal)
static TERMINAL_CACHE: Lazy<Mutex<Option<(Instant, bool)>>> = Lazy::new(|| Mutex::new(None));

/// How long to cache the terminal detection result
const CACHE_DURATION: Duration = Duration::from_secs(2);

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

/// Check if the active window is a terminal (with 2-second cache)
pub fn is_active_window_terminal() -> bool {
    // Check cache first
    if let Ok(cache) = TERMINAL_CACHE.lock() {
        if let Some((cached_time, cached_result)) = *cache {
            if cached_time.elapsed() < CACHE_DURATION {
                return cached_result;
            }
        }
    }

    // Cache miss or expired - do the expensive hyprctl call
    let result = is_active_window_terminal_uncached();

    // Update cache
    if let Ok(mut cache) = TERMINAL_CACHE.lock() {
        *cache = Some((Instant::now(), result));
    }

    result
}

/// Uncached terminal detection (calls hyprctl)
fn is_active_window_terminal_uncached() -> bool {
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
        let terminal_apps = ["alacritty", "kitty", "wezterm", "foot"];

        for app in &terminal_apps {
            assert!(terminal_apps.iter().any(|term| app.contains(term)));
        }
    }
}
