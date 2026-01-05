//! Interactive microphone selection for the recording-daemon
//!
//! This utility helps users discover and select available microphones for use
//! with the recording-daemon. It runs `pactl list sources short` to find devices,
//! allows interactive selection, and updates the systemd service configuration.
//!
//! # Usage
//!
//! ```sh
//! list-microphones
//! ```
//!
//! The tool will:
//! 1. List available input devices (microphones)
//! 2. Show the currently configured microphone (if any)
//! 3. Allow you to select a new microphone interactively
//! 4. Update the systemd service file automatically
//! 5. Reload systemd to apply changes

use dialoguer::{theme::ColorfulTheme, Select};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const SERVICE_FILE: &str = "recording-daemon.service";

fn get_service_path() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".config/systemd/user")
        .join(SERVICE_FILE)
}

fn get_available_microphones() -> Result<Vec<(String, String)>, String> {
    let output = Command::new("pactl")
        .args(["list", "sources", "short"])
        .output()
        .map_err(|e| format!("Failed to run 'pactl list sources short': {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "pactl command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut microphones = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[1];
            let state = parts.last().unwrap_or(&"UNKNOWN");

            // Only include input devices, not monitor devices (output loopbacks)
            if !name.contains(".monitor") {
                microphones.push((name.to_string(), state.to_string()));
            }
        }
    }

    Ok(microphones)
}

fn get_current_microphone() -> Option<String> {
    let service_path = get_service_path();
    if !service_path.exists() {
        return None;
    }

    let content = fs::read_to_string(&service_path).ok()?;
    for line in content.lines() {
        if line.contains("RECORDING_MICROPHONE=") {
            // Parse: Environment="RECORDING_MICROPHONE=device-name"
            if let Some(start) = line.find("RECORDING_MICROPHONE=") {
                let after_eq = &line[start + "RECORDING_MICROPHONE=".len()..];
                // Remove trailing quote if present
                let value = after_eq.trim_end_matches('"');
                return Some(value.to_string());
            }
        }
    }
    None
}

fn update_service_file(microphone: &str) -> Result<(), String> {
    let service_path = get_service_path();

    if !service_path.exists() {
        return Err(format!(
            "Service file not found: {}\nPlease create it first.",
            service_path.display()
        ));
    }

    let content = fs::read_to_string(&service_path)
        .map_err(|e| format!("Failed to read service file: {}", e))?;

    let mut new_lines = Vec::new();
    let mut found = false;

    for line in content.lines() {
        if line.contains("RECORDING_MICROPHONE=") {
            new_lines.push(format!(
                "Environment=\"RECORDING_MICROPHONE={}\"",
                microphone
            ));
            found = true;
        } else {
            new_lines.push(line.to_string());
        }
    }

    if !found {
        // Find [Service] section and add after it
        let mut inserted = false;
        new_lines.clear();
        for line in content.lines() {
            new_lines.push(line.to_string());
            if line.trim() == "[Service]" && !inserted {
                new_lines.push(format!(
                    "Environment=\"RECORDING_MICROPHONE={}\"",
                    microphone
                ));
                inserted = true;
            }
        }
        if !inserted {
            return Err("Could not find [Service] section in service file".to_string());
        }
    }

    // Ensure file ends with newline
    let new_content = new_lines.join("\n") + "\n";

    fs::write(&service_path, new_content)
        .map_err(|e| format!("Failed to write service file: {}", e))?;

    Ok(())
}

fn reload_systemd() -> Result<(), String> {
    println!("Reloading systemd user daemon...");

    let output = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output()
        .map_err(|e| format!("Failed to run systemctl daemon-reload: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "systemctl daemon-reload failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

fn restart_recording_daemon() -> Result<(), String> {
    println!("Restarting recording-daemon...");

    let output = Command::new("systemctl")
        .args(["--user", "restart", "recording-daemon"])
        .output()
        .map_err(|e| format!("Failed to restart recording-daemon: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to restart recording-daemon: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

fn main() {
    println!("Microphone Selection for recording-daemon");
    println!("==========================================");
    println!();

    // Get available microphones
    let microphones = match get_available_microphones() {
        Ok(mics) => mics,
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!();
            eprintln!("Make sure PulseAudio/PipeWire is running and pactl is installed.");
            std::process::exit(1);
        }
    };

    if microphones.is_empty() {
        eprintln!("No input devices (microphones) found.");
        eprintln!("Check your audio setup with 'pactl list sources'.");
        std::process::exit(1);
    }

    // Get current configuration
    let current = get_current_microphone();

    // Build selection items
    let items: Vec<String> = microphones
        .iter()
        .map(|(name, state)| {
            let state_indicator = match state.as_str() {
                "RUNNING" => "[ACTIVE]",
                "IDLE" => "[idle]",
                "SUSPENDED" => "[suspended]",
                _ => "",
            };
            let current_marker = if current.as_ref() == Some(name) {
                " ← current"
            } else {
                ""
            };
            format!("{} {}{}", name, state_indicator, current_marker)
        })
        .collect();

    // Find default selection (current microphone or first)
    let default_idx = current
        .as_ref()
        .and_then(|c| microphones.iter().position(|(name, _)| name == c))
        .unwrap_or(0);

    println!("Available microphones:");
    println!();

    // Interactive selection
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select microphone")
        .items(&items)
        .default(default_idx)
        .interact_opt();

    let selected_idx = match selection {
        Ok(Some(idx)) => idx,
        Ok(None) => {
            println!("Selection cancelled.");
            return;
        }
        Err(e) => {
            eprintln!("Error during selection: {}", e);
            std::process::exit(1);
        }
    };

    let (selected_name, _) = &microphones[selected_idx];

    // Check if same as current
    if current.as_ref() == Some(selected_name) {
        println!();
        println!("'{}' is already the configured microphone.", selected_name);
        return;
    }

    println!();
    println!("Selected: {}", selected_name);

    // Update service file
    if let Err(e) = update_service_file(selected_name) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    println!("Updated {}", get_service_path().display());

    // Reload systemd
    if let Err(e) = reload_systemd() {
        eprintln!("Warning: {}", e);
        eprintln!("You may need to run: systemctl --user daemon-reload");
    }

    // Restart the daemon
    if let Err(e) = restart_recording_daemon() {
        eprintln!("Warning: {}", e);
        eprintln!("You may need to run: systemctl --user restart recording-daemon");
    }

    println!();
    println!("Done! The recording-daemon is now using: {}", selected_name);
}
