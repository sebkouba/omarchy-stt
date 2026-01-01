//! eww widget control for recording, loading, and API indicators
//!
//! Controls the eww widgets that show:
//! - Recording: audio levels during push-to-talk (green bar)
//! - Loading: transcription progress (blue bar)
//! - API: API request progress (yellow bar)

use log::{debug, warn};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const PROGRESS_FILE: &str = "/tmp/ptt_transcription_progress";
const API_PROGRESS_FILE: &str = "/tmp/ptt_api_progress";

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

/// Open the recording indicator widget (green audio level bar)
pub fn show_recording_widget() {
    let config_path = get_eww_config_path();
    debug!("Opening eww recording widget from {:?}", config_path);

    match Command::new("eww")
        .args([
            "open",
            "recording",
            "--config",
            config_path.to_str().unwrap_or("."),
        ])
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
        .args([
            "close",
            "recording",
            "--config",
            config_path.to_str().unwrap_or("."),
        ])
        .spawn()
    {
        Ok(_) => debug!("Recording widget closed"),
        Err(e) => warn!("Failed to close recording widget: {}", e),
    }
}

/// Open the loading indicator widget (blue progress bar)
pub fn show_loading_widget() {
    let config_path = get_eww_config_path();
    debug!("Opening eww loading widget from {:?}", config_path);

    // Initialize progress file to 0
    if let Err(e) = fs::write(PROGRESS_FILE, "0") {
        warn!("Failed to initialize progress file: {}", e);
    }

    match Command::new("eww")
        .args([
            "open",
            "loading",
            "--config",
            config_path.to_str().unwrap_or("."),
        ])
        .spawn()
    {
        Ok(_) => debug!("Loading widget opened"),
        Err(e) => warn!("Failed to open loading widget: {}", e),
    }
}

/// Close the loading indicator widget
pub fn hide_loading_widget() {
    let config_path = get_eww_config_path();
    debug!("Closing eww loading widget");

    // Clean up progress file
    fs::remove_file(PROGRESS_FILE).ok();

    match Command::new("eww")
        .args([
            "close",
            "loading",
            "--config",
            config_path.to_str().unwrap_or("."),
        ])
        .spawn()
    {
        Ok(_) => debug!("Loading widget closed"),
        Err(e) => warn!("Failed to close loading widget: {}", e),
    }
}

/// Update the loading progress (0-100)
pub fn set_loading_progress(progress: u8) {
    let progress = progress.min(100);
    if let Err(e) = fs::write(PROGRESS_FILE, progress.to_string()) {
        warn!("Failed to update progress file: {}", e);
    }
}

/// Open the API indicator widget (yellow progress bar)
pub fn show_api_widget() {
    let config_path = get_eww_config_path();
    debug!("Opening eww API widget from {:?}", config_path);

    // Initialize progress file to 0
    if let Err(e) = fs::write(API_PROGRESS_FILE, "0") {
        warn!("Failed to initialize API progress file: {}", e);
    }

    match Command::new("eww")
        .args([
            "open",
            "api",
            "--config",
            config_path.to_str().unwrap_or("."),
        ])
        .spawn()
    {
        Ok(_) => debug!("API widget opened"),
        Err(e) => warn!("Failed to open API widget: {}", e),
    }
}

/// Close the API indicator widget
pub fn hide_api_widget() {
    let config_path = get_eww_config_path();
    debug!("Closing eww API widget");

    // Clean up progress file
    fs::remove_file(API_PROGRESS_FILE).ok();

    match Command::new("eww")
        .args([
            "close",
            "api",
            "--config",
            config_path.to_str().unwrap_or("."),
        ])
        .spawn()
    {
        Ok(_) => debug!("API widget closed"),
        Err(e) => warn!("Failed to close API widget: {}", e),
    }
}

/// Update the API progress (0-100)
pub fn set_api_progress(progress: u8) {
    let progress = progress.min(100);
    if let Err(e) = fs::write(API_PROGRESS_FILE, progress.to_string()) {
        warn!("Failed to update API progress file: {}", e);
    }
}
