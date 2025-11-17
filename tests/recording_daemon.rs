//! Integration tests for recording daemon
//!
//! These tests require the recording daemon to be running.
//! Start it with: cargo run --release --bin recording-daemon

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;
use std::time::Duration;

const DAEMON_SOCKET: &str = "/tmp/transcribe-rs-v2-recording.sock";

#[test]
#[ignore] // Requires daemon running
fn test_daemon_ping() {
    let mut stream =
        UnixStream::connect(DAEMON_SOCKET).expect("Failed to connect to daemon. Is it running?");

    let request = serde_json::json!({"command": "ping"});
    writeln!(stream, "{}", request).unwrap();

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line).unwrap();

    let response: serde_json::Value = serde_json::from_str(&response_line).unwrap();
    assert_eq!(response["ok"], true);
    println!("✓ Daemon ping successful");
    println!("  Uptime: {}s", response["uptime_seconds"]);
    println!(
        "  Buffer fullness: {:.1}%",
        response["buffer_fullness"].as_f64().unwrap_or(0.0) * 100.0
    );
}

#[test]
#[ignore] // Requires daemon running
fn test_start_stop_recording() {
    // Start recording
    let mut stream =
        UnixStream::connect(DAEMON_SOCKET).expect("Failed to connect to daemon. Is it running?");

    let request = serde_json::json!({"command": "start"});
    writeln!(stream, "{}", request).unwrap();

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line).unwrap();

    let response: serde_json::Value = serde_json::from_str(&response_line).unwrap();
    assert_eq!(response["ok"], true);

    let start_index = response["start_index"].as_u64().unwrap();
    println!("✓ Recording started at index: {}", start_index);

    // Wait 1 second
    thread::sleep(Duration::from_secs(1));

    // Stop recording
    let mut stream = UnixStream::connect(DAEMON_SOCKET).unwrap();
    let request = serde_json::json!({"command": "stop", "start_index": start_index});
    writeln!(stream, "{}", request).unwrap();

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line).unwrap();

    let response: serde_json::Value = serde_json::from_str(&response_line).unwrap();
    assert_eq!(response["ok"], true);

    let wav_path = response["wav_path"].as_str().unwrap();
    let duration_ms = response["duration_ms"].as_u64().unwrap();
    let latency_ms = response["latency_ms"].as_u64().unwrap();

    println!("✓ Recording stopped successfully");
    println!("  WAV path: {}", wav_path);
    println!("  Duration: {:.2}s", duration_ms as f64 / 1000.0);
    println!("  Stop latency: {}ms", latency_ms);

    // Verify WAV file exists
    assert!(Path::new(wav_path).exists());

    // Verify latency is low (should be <50ms)
    assert!(latency_ms < 100, "Stop latency too high: {}ms", latency_ms);
}

#[test]
#[ignore] // Requires daemon running
fn test_concurrent_start_error() {
    // Start first recording
    let mut stream1 =
        UnixStream::connect(DAEMON_SOCKET).expect("Failed to connect to daemon. Is it running?");

    let request = serde_json::json!({"command": "start"});
    writeln!(stream1, "{}", request).unwrap();

    let mut reader1 = BufReader::new(stream1);
    let mut response_line = String::new();
    reader1.read_line(&mut response_line).unwrap();

    let response: serde_json::Value = serde_json::from_str(&response_line).unwrap();
    assert_eq!(response["ok"], true);
    let start_index = response["start_index"].as_u64().unwrap();

    println!("✓ First recording started at index: {}", start_index);

    // Try to start second recording (should fail)
    let mut stream2 = UnixStream::connect(DAEMON_SOCKET).unwrap();
    let request = serde_json::json!({"command": "start"});
    writeln!(stream2, "{}", request).unwrap();

    let mut reader2 = BufReader::new(stream2);
    let mut response_line = String::new();
    reader2.read_line(&mut response_line).unwrap();

    let response: serde_json::Value = serde_json::from_str(&response_line).unwrap();
    assert_eq!(response["ok"], false);
    assert!(response["error"]
        .as_str()
        .unwrap()
        .contains("Already recording"));

    println!("✓ Concurrent recording correctly rejected");

    // Clean up: stop first recording
    let mut stream3 = UnixStream::connect(DAEMON_SOCKET).unwrap();
    let request = serde_json::json!({"command": "stop", "start_index": start_index});
    writeln!(stream3, "{}", request).unwrap();

    let mut reader3 = BufReader::new(stream3);
    let mut response_line = String::new();
    reader3.read_line(&mut response_line).unwrap();

    let response: serde_json::Value = serde_json::from_str(&response_line).unwrap();
    assert_eq!(response["ok"], true);

    println!("✓ Cleanup complete");
}

#[test]
#[ignore] // Requires daemon running
fn test_back_to_back_recordings() {
    println!("Testing back-to-back recordings (3 recordings in rapid succession)...");

    for i in 1..=3 {
        println!("\nRecording {}/3:", i);

        // Start
        let mut stream = UnixStream::connect(DAEMON_SOCKET).unwrap();
        let request = serde_json::json!({"command": "start"});
        writeln!(stream, "{}", request).unwrap();

        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line).unwrap();

        let response: serde_json::Value = serde_json::from_str(&response_line).unwrap();
        assert_eq!(response["ok"], true);
        let start_index = response["start_index"].as_u64().unwrap();
        println!("  Started at index: {}", start_index);

        // Wait 500ms
        thread::sleep(Duration::from_millis(500));

        // Stop
        let mut stream = UnixStream::connect(DAEMON_SOCKET).unwrap();
        let request = serde_json::json!({"command": "stop", "start_index": start_index});
        writeln!(stream, "{}", request).unwrap();

        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line).unwrap();

        let response: serde_json::Value = serde_json::from_str(&response_line).unwrap();
        assert_eq!(response["ok"], true);

        let latency_ms = response["latency_ms"].as_u64().unwrap();
        println!("  Stopped (latency: {}ms)", latency_ms);
    }

    println!("\n✓ All back-to-back recordings successful");
}
