//! OCR module for capturing screen context
//!
//! This module provides functionality to capture screenshots of the active window
//! and perform OCR to extract text, which can be used as context for LLM processing.
//!
//! Requires the `ocr` feature flag and system libraries:
//! - leptonica
//! - tesseract
//!
//! Install on Arch Linux:
//! ```bash
//! sudo pacman -S leptonica tesseract tesseract-data-eng grim
//! ```
//!
//! Build with OCR support:
//! ```bash
//! cargo build --release --features ocr
//! ```

use crate::config::OcrConfig;
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// Append a log message to the debug log
fn log(message: &str) {
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/ptt_rust_debug.log")
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        writeln!(file, "[{}] [ocr] {}", timestamp, message).ok();
    }
}

/// Capture a screenshot of the active window using hyprctl and grim
///
/// Returns the path to the saved screenshot
pub fn capture_active_window(config: &OcrConfig) -> Result<String, Box<dyn Error>> {
    log("Capturing active window screenshot...");

    // Get active window geometry from Hyprland
    let hyprctl_output = Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()
        .map_err(|e| format!("Failed to run hyprctl: {}. Is Hyprland running?", e))?;

    if !hyprctl_output.status.success() {
        let stderr = String::from_utf8_lossy(&hyprctl_output.stderr);
        return Err(format!("hyprctl failed: {}", stderr).into());
    }

    let json_str = String::from_utf8_lossy(&hyprctl_output.stdout);
    log(&format!("hyprctl output: {}", json_str));

    // Parse JSON to extract window geometry
    let json: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse hyprctl JSON: {}", e))?;

    // Extract position and size
    let at = json
        .get("at")
        .and_then(|v| v.as_array())
        .ok_or("Missing 'at' field in hyprctl output")?;
    let size = json
        .get("size")
        .and_then(|v| v.as_array())
        .ok_or("Missing 'size' field in hyprctl output")?;

    let x = at.get(0).and_then(|v| v.as_i64()).unwrap_or(0);
    let y = at.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
    let width = size.get(0).and_then(|v| v.as_i64()).unwrap_or(800);
    let height = size.get(1).and_then(|v| v.as_i64()).unwrap_or(600);

    // Format geometry for grim: "x,y widthxheight"
    let geometry = format!("{},{} {}x{}", x, y, width, height);
    log(&format!("Window geometry: {}", geometry));

    // Capture screenshot with grim
    let screenshot_path = &config.screenshot_path;
    let grim_output = Command::new("grim")
        .args(["-g", &geometry, screenshot_path])
        .output()
        .map_err(|e| format!("Failed to run grim: {}. Is grim installed?", e))?;

    if !grim_output.status.success() {
        let stderr = String::from_utf8_lossy(&grim_output.stderr);
        return Err(format!("grim failed: {}", stderr).into());
    }

    log(&format!("Screenshot saved to: {}", screenshot_path));
    Ok(screenshot_path.clone())
}

/// Perform OCR on an image file using leptess (Tesseract bindings)
///
/// Returns the extracted text
#[cfg(feature = "ocr")]
pub fn perform_ocr(image_path: &str, config: &OcrConfig) -> Result<String, Box<dyn Error>> {
    use leptess::LepTess;

    log(&format!("Performing OCR on: {}", image_path));
    let start = Instant::now();

    // Initialize Tesseract with specified language
    // First parameter is datapath (None = use default TESSDATA_PREFIX)
    let mut lt = LepTess::new(None, &config.language).map_err(|e| {
        format!(
            "Failed to initialize Tesseract: {}. Is tesseract-data-{} installed?",
            e, config.language
        )
    })?;

    // Set DPI for better accuracy
    lt.set_source_resolution(config.dpi as i32);

    // Load the image
    lt.set_image(image_path)
        .map_err(|e| format!("Failed to load image '{}': {}", image_path, e))?;

    // Perform OCR
    let text = lt
        .get_utf8_text()
        .map_err(|e| format!("OCR failed: {}", e))?;

    let duration = start.elapsed();
    log(&format!(
        "OCR completed in {:?}, extracted {} chars",
        duration,
        text.len()
    ));

    // Clean up the text (remove excessive whitespace)
    let cleaned = text
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(cleaned)
}

/// Stub function when OCR feature is not enabled
#[cfg(not(feature = "ocr"))]
pub fn perform_ocr(_image_path: &str, _config: &OcrConfig) -> Result<String, Box<dyn Error>> {
    Err("OCR support not compiled. Build with: cargo build --release --features ocr".into())
}

/// Spawn a background OCR worker process
///
/// This function captures a screenshot and spawns a detached process to perform OCR.
/// The result will be written to the configured result_path.
pub fn spawn_ocr_worker(config: &OcrConfig) -> Result<(), Box<dyn Error>> {
    log("Starting async OCR workflow...");

    // First, capture the screenshot (this is fast, ~50ms)
    let screenshot_path = capture_active_window(config)?;

    // Spawn a detached process to perform OCR
    // We use the same binary with a special subcommand
    let exe_path = std::env::current_exe()
        .map_err(|e| format!("Failed to get current executable path: {}", e))?;

    log(&format!("Spawning OCR worker: {:?}", exe_path));

    // Write a flag to indicate OCR is in progress
    let flag_path = "/tmp/ptt_ocr_running.flag";
    fs::write(flag_path, "")?;

    // Spawn the worker process
    let child = Command::new(exe_path)
        .args([
            "ocr-worker",
            &screenshot_path,
            &config.result_path,
            &config.language,
            &config.dpi.to_string(),
        ])
        .spawn()
        .map_err(|e| format!("Failed to spawn OCR worker: {}", e))?;

    log(&format!("OCR worker spawned with PID: {}", child.id()));

    Ok(())
}

/// Perform OCR worker task (called by spawned process)
///
/// This is the entry point for the background OCR worker process.
pub fn run_ocr_worker(
    screenshot_path: &str,
    result_path: &str,
    language: &str,
    dpi: u32,
) -> Result<(), Box<dyn Error>> {
    log(&format!(
        "OCR worker starting: {} -> {}",
        screenshot_path, result_path
    ));

    let config = OcrConfig {
        language: language.to_string(),
        dpi,
        screenshot_path: screenshot_path.to_string(),
        result_path: result_path.to_string(),
    };

    // Perform OCR
    let result = match perform_ocr(screenshot_path, &config) {
        Ok(text) => text,
        Err(e) => {
            log(&format!("OCR worker error: {}", e));
            format!("OCR_ERROR: {}", e)
        }
    };

    // Write result to file
    fs::write(result_path, &result).map_err(|e| format!("Failed to write OCR result: {}", e))?;

    // Remove the running flag
    let flag_path = "/tmp/ptt_ocr_running.flag";
    let _ = fs::remove_file(flag_path);

    log(&format!(
        "OCR worker completed, wrote {} chars to {}",
        result.len(),
        result_path
    ));

    Ok(())
}

/// Wait for OCR result with timeout
///
/// Returns the OCR text if available, or None if timeout or error
pub fn wait_for_ocr_result(
    config: &OcrConfig,
    timeout: Duration,
) -> Result<Option<String>, Box<dyn Error>> {
    let flag_path = "/tmp/ptt_ocr_running.flag";
    let result_path = &config.result_path;

    log(&format!(
        "Waiting for OCR result (timeout: {:?})...",
        timeout
    ));
    let start = Instant::now();

    // Poll for completion
    while start.elapsed() < timeout {
        // Check if the flag file still exists (OCR in progress)
        if !Path::new(flag_path).exists() {
            // OCR is done, check for result
            if Path::new(result_path).exists() {
                let text = fs::read_to_string(result_path)?;

                // Check for error marker
                if text.starts_with("OCR_ERROR:") {
                    let error_msg = text.strip_prefix("OCR_ERROR: ").unwrap_or(&text);
                    log(&format!("OCR returned error: {}", error_msg));
                    return Err(error_msg.to_string().into());
                }

                log(&format!("OCR result retrieved: {} chars", text.len()));
                return Ok(Some(text));
            } else {
                log("OCR flag removed but no result file found");
                return Ok(None);
            }
        }

        // Sleep briefly before polling again
        std::thread::sleep(Duration::from_millis(50));
    }

    log("OCR timeout reached");

    // Timeout - check if result exists anyway
    if Path::new(result_path).exists() {
        let text = fs::read_to_string(result_path)?;
        if !text.starts_with("OCR_ERROR:") {
            log(&format!(
                "OCR result found after timeout: {} chars",
                text.len()
            ));
            return Ok(Some(text));
        }
    }

    Ok(None)
}

/// Clean up OCR temporary files
pub fn cleanup_ocr_files(config: &OcrConfig) {
    let flag_path = "/tmp/ptt_ocr_running.flag";
    let _ = fs::remove_file(flag_path);
    let _ = fs::remove_file(&config.screenshot_path);
    let _ = fs::remove_file(&config.result_path);
    log("OCR temporary files cleaned up");
}

/// Check if Tesseract is available on the system
pub fn check_tesseract_available() -> Result<String, String> {
    let output = Command::new("tesseract")
        .arg("--version")
        .output()
        .map_err(|e| format!("Failed to run tesseract: {}", e))?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout);
        let first_line = version.lines().next().unwrap_or("unknown version");
        Ok(first_line.to_string())
    } else {
        Err("tesseract command failed".to_string())
    }
}

/// Check if grim is available on the system
pub fn check_grim_available() -> Result<String, String> {
    let output = Command::new("grim")
        .arg("-h")
        .output()
        .map_err(|e| format!("Failed to run grim: {}", e))?;

    // grim returns success or shows help
    if output.status.success() || !output.stderr.is_empty() || !output.stdout.is_empty() {
        Ok("grim available".to_string())
    } else {
        Err("grim command failed".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ocr_config_default() {
        let config = OcrConfig::default();
        assert_eq!(config.language, "eng");
        assert_eq!(config.dpi, 300);
    }

    #[test]
    fn test_check_tesseract() {
        // This test will pass if tesseract is installed
        let result = check_tesseract_available();
        println!("Tesseract check: {:?}", result);
    }

    #[test]
    fn test_check_grim() {
        // This test will pass if grim is installed
        let result = check_grim_available();
        println!("Grim check: {:?}", result);
    }
}
