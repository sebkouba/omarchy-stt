//! Audio recording management using recording daemon
//!
//! This module provides a client interface to the recording daemon,
//! which continuously records audio to a circular buffer for near-instant
//! extraction without FFmpeg spawn/exit overhead.

use crate::config::AudioConfig;
use log::{debug, error, info, warn};
use std::error::Error;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

const DAEMON_SOCKET: &str = "/tmp/transcribe-rs-v2-recording.sock";

/// Start recording audio via daemon
pub fn start_recording(config: &AudioConfig) -> Result<(), Box<dyn Error>> {
    info!("=== Recording start requested ===");

    // Check if already recording
    if Path::new(&config.recording_pid_file).exists() {
        let index = fs::read_to_string(&config.recording_pid_file)?;
        warn!("Already recording (start_index: {})", index.trim());
        return Err("Already recording".into());
    }

    // Connect to daemon
    debug!("Connecting to daemon at {}", DAEMON_SOCKET);
    let mut stream = UnixStream::connect(DAEMON_SOCKET).map_err(|e| {
        error!("Failed to connect to recording daemon: {}", e);
        format!(
            "Failed to connect to recording daemon.\n\n\
            Is the daemon running?\n\
            Start with: recording-daemon\n\
            Or: systemctl --user start recording-daemon\n\n\
            Error: {}",
            e
        )
    })?;

    // Send start command
    let request = serde_json::json!({"command": "start"});
    writeln!(stream, "{}", request)?;
    debug!("Sent request: {}", request);

    // Read response
    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    debug!("Received response: {}", response_line.trim());

    let response: serde_json::Value = serde_json::from_str(&response_line)?;

    if !response["ok"].as_bool().unwrap_or(false) {
        let err = response["error"].as_str().unwrap_or("Unknown error");
        error!("Daemon returned error: {}", err);
        return Err(err.into());
    }

    // Save start_index to file (repurposing the PID file)
    let start_index = response["start_index"]
        .as_u64()
        .ok_or("Missing start_index in response")?;
    fs::write(&config.recording_pid_file, start_index.to_string())?;

    debug!("Recording started at index: {}", start_index);
    info!("Recording started successfully");
    Ok(())
}

/// Stop recording and return the path to the audio file
pub fn stop_recording(config: &AudioConfig) -> Result<PathBuf, Box<dyn Error>> {
    info!("=== Recording stop requested ===");

    // Check if recording
    if !Path::new(&config.recording_pid_file).exists() {
        warn!("Not recording (index file not found)");
        return Err("Not recording".into());
    }

    // Read start_index from file
    let start_index_str = fs::read_to_string(&config.recording_pid_file)?;
    let start_index: u64 = start_index_str.trim().parse()?;
    debug!("Stopping recording from index: {}", start_index);

    // Connect to daemon
    debug!("Connecting to daemon at {}", DAEMON_SOCKET);
    let mut stream = UnixStream::connect(DAEMON_SOCKET).map_err(|e| {
        error!("Failed to connect to recording daemon: {}", e);
        // Clean up state file
        fs::remove_file(&config.recording_pid_file).ok();
        format!(
            "Failed to connect to recording daemon - audio lost.\n\n\
            The daemon may have crashed.\n\
            Restart with: systemctl --user restart recording-daemon\n\n\
            Error: {}",
            e
        )
    })?;

    // Send stop command
    let request = serde_json::json!({
        "command": "stop",
        "start_index": start_index
    });
    writeln!(stream, "{}", request)?;
    debug!("Sent request: {}", request);

    // Read response
    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    debug!("Received response: {}", response_line.trim());

    let response: serde_json::Value = serde_json::from_str(&response_line)?;

    // Remove state file
    fs::remove_file(&config.recording_pid_file)?;
    debug!("Index file removed");

    if !response["ok"].as_bool().unwrap_or(false) {
        let err = response["error"].as_str().unwrap_or("Unknown error");
        error!("Daemon returned error: {}", err);
        return Err(err.into());
    }

    // Extract WAV path and metadata
    let wav_path = response["wav_path"]
        .as_str()
        .ok_or("Missing wav_path in response")?;
    let duration_ms = response["duration_ms"].as_u64().unwrap_or(0);
    let latency_ms = response["latency_ms"].as_u64().unwrap_or(0);
    let samples = response["samples"].as_u64().unwrap_or(0);

    info!(
        "Recording stopped successfully: path={}, duration={:.2}s, latency={}ms, samples={}",
        wav_path,
        duration_ms as f64 / 1000.0,
        latency_ms,
        samples
    );

    // Validate file exists
    let file_size = fs::metadata(wav_path)?.len();
    debug!("WAV file size: {} bytes", file_size);

    if file_size < 1000 {
        error!("WAV file too small ({} bytes)", file_size);
        return Err("WAV file too small - no audio data".into());
    }

    Ok(PathBuf::from(wav_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_values() {
        // Verify default config has reasonable values
        let config = AudioConfig::default();
        assert!(!config.recording_pid_file.is_empty());
        assert!(!config.recording_path.is_empty());
        assert!(!config.microphone.is_empty());
        assert!(!config.log_file.is_empty());
        assert_eq!(config.sample_rate, 16000);
    }

    #[test]
    #[ignore] // Requires daemon running
    fn test_daemon_connection() {
        // Just test if we can connect to the daemon
        match UnixStream::connect(DAEMON_SOCKET) {
            Ok(_) => println!("✓ Daemon is running"),
            Err(e) => println!("✗ Daemon not running: {}", e),
        }
    }

    #[test]
    #[ignore] // Requires daemon running
    fn test_ping_daemon() {
        let mut stream = UnixStream::connect(DAEMON_SOCKET).unwrap();
        let request = serde_json::json!({"command": "ping"});
        writeln!(stream, "{}", request).unwrap();

        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line).unwrap();

        let response: serde_json::Value = serde_json::from_str(&response_line).unwrap();
        assert_eq!(response["ok"], true);
        println!("Daemon uptime: {}s", response["uptime_seconds"]);
        println!(
            "Buffer fullness: {:.1}%",
            response["buffer_fullness"].as_f64().unwrap_or(0.0) * 100.0
        );
    }
}
