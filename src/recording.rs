//! Audio recording management using recording daemon
//!
//! This module provides a client interface to the recording daemon,
//! which continuously records audio to a circular buffer for near-instant
//! extraction without FFmpeg spawn/exit overhead.

use crate::config::AudioConfig;
use std::error::Error;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

const DAEMON_SOCKET: &str = "/tmp/transcribe-rs-v2-recording.sock";

/// Append a log message to the debug log
fn log(message: &str, log_file: &str) {
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        writeln!(file, "[{}] [recording] {}", timestamp, message).ok();
    }
}

/// Start recording audio via daemon
pub fn start_recording(config: &AudioConfig) -> Result<(), Box<dyn Error>> {
    log("=== Recording start requested ===", &config.log_file);

    // Check if already recording
    if Path::new(&config.recording_pid_file).exists() {
        let index = fs::read_to_string(&config.recording_pid_file)?;
        log(&format!("WARNING: Already recording (start_index: {})", index.trim()), &config.log_file);
        return Err("Already recording".into());
    }

    // Connect to daemon
    log(&format!("Connecting to daemon at {}", DAEMON_SOCKET), &config.log_file);
    let mut stream = UnixStream::connect(DAEMON_SOCKET).map_err(|e| {
        log(&format!("ERROR: Failed to connect to recording daemon: {}", e), &config.log_file);
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
    log(&format!("Sent request: {}", request), &config.log_file);

    // Read response
    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    log(&format!("Received response: {}", response_line.trim()), &config.log_file);

    let response: serde_json::Value = serde_json::from_str(&response_line)?;

    if !response["ok"].as_bool().unwrap_or(false) {
        let error = response["error"].as_str().unwrap_or("Unknown error");
        log(&format!("ERROR: Daemon returned error: {}", error), &config.log_file);
        return Err(error.into());
    }

    // Save start_index to file (repurposing the PID file)
    let start_index = response["start_index"]
        .as_u64()
        .ok_or("Missing start_index in response")?;
    fs::write(&config.recording_pid_file, start_index.to_string())?;

    log(&format!("Recording started at index: {}", start_index), &config.log_file);
    log("Recording started successfully", &config.log_file);
    Ok(())
}

/// Stop recording and return the path to the audio file
pub fn stop_recording(config: &AudioConfig) -> Result<PathBuf, Box<dyn Error>> {
    log("=== Recording stop requested ===", &config.log_file);

    // Check if recording
    if !Path::new(&config.recording_pid_file).exists() {
        log("WARNING: Not recording (index file not found)", &config.log_file);
        return Err("Not recording".into());
    }

    // Read start_index from file
    let start_index_str = fs::read_to_string(&config.recording_pid_file)?;
    let start_index: u64 = start_index_str.trim().parse()?;
    log(&format!("Stopping recording from index: {}", start_index), &config.log_file);

    // Connect to daemon
    log(&format!("Connecting to daemon at {}", DAEMON_SOCKET), &config.log_file);
    let mut stream = UnixStream::connect(DAEMON_SOCKET).map_err(|e| {
        log(&format!("ERROR: Failed to connect to recording daemon: {}", e), &config.log_file);
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
    log(&format!("Sent request: {}", request), &config.log_file);

    // Read response
    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    log(&format!("Received response: {}", response_line.trim()), &config.log_file);

    let response: serde_json::Value = serde_json::from_str(&response_line)?;

    // Remove state file
    fs::remove_file(&config.recording_pid_file)?;
    log("Index file removed", &config.log_file);

    if !response["ok"].as_bool().unwrap_or(false) {
        let error = response["error"].as_str().unwrap_or("Unknown error");
        log(&format!("ERROR: Daemon returned error: {}", error), &config.log_file);
        return Err(error.into());
    }

    // Extract WAV path and metadata
    let wav_path = response["wav_path"]
        .as_str()
        .ok_or("Missing wav_path in response")?;
    let duration_ms = response["duration_ms"].as_u64().unwrap_or(0);
    let latency_ms = response["latency_ms"].as_u64().unwrap_or(0);
    let samples = response["samples"].as_u64().unwrap_or(0);

    log(&format!(
        "Recording stopped successfully: path={}, duration={:.2}s, latency={}ms, samples={}",
        wav_path,
        duration_ms as f64 / 1000.0,
        latency_ms,
        samples
    ), &config.log_file);

    // Validate file exists
    let file_size = fs::metadata(wav_path)?.len();
    log(&format!("WAV file size: {} bytes", file_size), &config.log_file);

    if file_size < 1000 {
        log(&format!("ERROR: WAV file too small ({} bytes)", file_size), &config.log_file);
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
        println!("Buffer fullness: {:.1}%", response["buffer_fullness"].as_f64().unwrap_or(0.0) * 100.0);
    }
}
