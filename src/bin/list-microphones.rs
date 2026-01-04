//! List available microphones (PulseAudio sources)
//!
//! This utility helps users discover available microphones for use with the
//! recording-daemon. It runs `pactl list sources short` and displays the
//! results in a user-friendly format.
//!
//! # Usage
//!
//! ```sh
//! list-microphones
//! ```
//!
//! # Setting Your Microphone
//!
//! Once you find your microphone device name, set it in your systemd service
//! or shell environment:
//!
//! ```sh
//! # In ~/.config/systemd/user/recording-daemon.service
//! Environment="RECORDING_MICROPHONE=alsa_input.usb-..."
//!
//! # Or directly in the shell
//! RECORDING_MICROPHONE="alsa_input.usb-..." recording-daemon
//! ```
//!
//! If RECORDING_MICROPHONE is not set, the daemon uses "default" which lets
//! PulseAudio choose the default input device.

use std::process::Command;

fn main() {
    println!("Available microphones (PulseAudio sources):");
    println!("============================================");
    println!();

    // Run pactl list sources short
    let output = match Command::new("pactl")
        .args(["list", "sources", "short"])
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            eprintln!("Error: Failed to run 'pactl list sources short': {}", e);
            eprintln!();
            eprintln!("Make sure PulseAudio is running and pactl is installed.");
            eprintln!("On Arch Linux: pacman -S pulseaudio");
            std::process::exit(1);
        }
    };

    if !output.status.success() {
        eprintln!(
            "Error: pactl command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::process::exit(1);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    if lines.is_empty() {
        println!("No audio sources found.");
        return;
    }

    // Parse and display sources
    // Format: index  name  module  sample_spec  state
    // Example: 0  alsa_output.pci-0000_00_1f.3.analog-stereo.monitor  module-alsa-card.c  s32le 2ch 44100Hz  SUSPENDED
    let mut input_sources = Vec::new();
    let mut monitor_sources = Vec::new();

    for line in lines {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[1];
            let state = parts.last().unwrap_or(&"UNKNOWN");

            // Classify as input or monitor (output loopback)
            if name.contains(".monitor") {
                monitor_sources.push((name, *state));
            } else {
                input_sources.push((name, *state));
            }
        }
    }

    // Display input sources (actual microphones)
    if !input_sources.is_empty() {
        println!("Input devices (microphones):");
        println!("----------------------------");
        for (name, state) in &input_sources {
            let state_indicator = match *state {
                "RUNNING" => "[ACTIVE]",
                "IDLE" => "[idle]",
                "SUSPENDED" => "[suspended]",
                _ => "",
            };
            println!("  {} {}", name, state_indicator);
        }
        println!();
    }

    // Display monitor sources (output loopbacks)
    if !monitor_sources.is_empty() {
        println!("Monitor devices (output loopbacks):");
        println!("-----------------------------------");
        for (name, state) in &monitor_sources {
            let state_indicator = match *state {
                "RUNNING" => "[ACTIVE]",
                "IDLE" => "[idle]",
                "SUSPENDED" => "[suspended]",
                _ => "",
            };
            println!("  {} {}", name, state_indicator);
        }
        println!();
    }

    // Show usage instructions
    println!("Usage:");
    println!("------");
    println!("Set RECORDING_MICROPHONE to one of the device names above.");
    println!();
    println!("In systemd service (~/.config/systemd/user/recording-daemon.service):");
    println!("  Environment=\"RECORDING_MICROPHONE=<device-name>\"");
    println!();
    println!("Or directly in shell:");
    println!("  RECORDING_MICROPHONE=\"<device-name>\" recording-daemon");
    println!();
    println!("Leave unset or use \"default\" for PulseAudio default input.");
}
