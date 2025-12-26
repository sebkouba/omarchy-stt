//! eww widget control for recording indicator
//!
//! Controls the eww recording indicator widget that shows audio levels
//! during push-to-talk recording.

use log::{debug, warn};
use std::path::PathBuf;
use std::process::Command;

/// Get the path to the eww config directory
fn get_eww_config_path() -> PathBuf {
    // First try project directory
    let project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("eww");
    if project_path.exists() {
        return project_path;
    }

    // Fallback to ~/.config/transcribe-rs/eww
    if let Some(config_dir) = dirs::config_dir() {
        let config_path = config_dir.join("transcribe-rs").join("eww");
        if config_path.exists() {
            return config_path;
        }
    }

    // Return project path even if it doesn't exist (will fail gracefully)
    project_path
}

/// Open the recording indicator widget
pub fn show_recording_widget() {
    let config_path = get_eww_config_path();
    debug!("Opening eww recording widget from {:?}", config_path);

    match Command::new("eww")
        .args(["open", "recording", "--config", config_path.to_str().unwrap_or(".")])
        .spawn()
    {
        Ok(_) => debug!("Recording widget opened"),
        Err(e) => warn!("Failed to open recording widget: {}", e),
    }
}

/// Close the recording indicator widget
pub fn hide_recording_widget() {
    let config_path = get_eww_config_path();
    debug!("Closing eww recording widget");

    match Command::new("eww")
        .args(["close", "recording", "--config", config_path.to_str().unwrap_or(".")])
        .spawn()
    {
        Ok(_) => debug!("Recording widget closed"),
        Err(e) => warn!("Failed to close recording widget: {}", e),
    }
}
