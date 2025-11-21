//! Recording Daemon - Continuous audio capture with on-demand extraction
//!
//! This daemon continuously records audio from a microphone into a RAM-based
//! circular buffer, allowing near-instant extraction of recorded segments without
//! the overhead of starting/stopping FFmpeg for each recording.
//!
//! ## Performance
//!
//! - Start latency: 0ms (already recording)
//! - Stop latency: ~5-10ms (extract + write WAV)
//! - Memory usage: ~3.7 MB for 2-minute buffer @ 16kHz mono 16-bit

use log::{debug, error, info, warn};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use transcribe_rs::circular_buffer::{CircularBuffer, SharedBuffer};
use transcribe_rs::logging;

const SOCKET_PATH: &str = "/tmp/transcribe-rs-v2-recording.sock";
const SAMPLE_RATE: u32 = 16000; // 16kHz
const BUFFER_DURATION_SECONDS: usize = 120; // 2 minutes
const OUTPUT_WAV_PATH: &str = "/tmp/ptt_current.wav";

/// Daemon state
struct DaemonState {
    recording_start_index: Option<usize>,
    ffmpeg_process: Option<Child>,
    buffer: SharedBuffer,
    start_time: Instant,
}

impl DaemonState {
    fn new(buffer: SharedBuffer) -> Self {
        Self {
            recording_start_index: None,
            ffmpeg_process: None,
            buffer,
            start_time: Instant::now(),
        }
    }
}

type SharedState = Arc<Mutex<DaemonState>>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    if let Err(e) = logging::init() {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    }

    info!("=== Recording daemon starting ===");

    // Get microphone from environment or use default
    let microphone = std::env::var("RECORDING_MICROPHONE").unwrap_or_else(|_| {
        "alsa_input.usb-046d_C922_Pro_Stream_Webcam_C4C393EF-02.analog-stereo".to_string()
    });

    // Get buffer size from environment or use default
    let buffer_seconds = std::env::var("RECORDING_BUFFER_SIZE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(BUFFER_DURATION_SECONDS);

    let buffer_capacity = SAMPLE_RATE as usize * buffer_seconds;

    debug!(
        "Configuration: microphone={}, buffer={}s ({} samples)",
        microphone, buffer_seconds, buffer_capacity
    );

    // Create shared buffer
    let buffer = Arc::new(Mutex::new(CircularBuffer::new(buffer_capacity)));

    // Create daemon state
    let state = Arc::new(Mutex::new(DaemonState::new(Arc::clone(&buffer))));

    // Spawn FFmpeg and reader thread
    spawn_ffmpeg_and_reader(&microphone, Arc::clone(&buffer), Arc::clone(&state))?;

    // Setup signal handlers
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&running);
    ctrlc::set_handler(move || {
        debug!("Received shutdown signal");
        running_clone.store(false, Ordering::SeqCst);
    })?;

    // Start socket listener
    listen_on_socket(Arc::clone(&state), Arc::clone(&running))?;

    // Cleanup
    debug!("Shutting down daemon");
    cleanup_ffmpeg(Arc::clone(&state));
    fs::remove_file(SOCKET_PATH).ok();

    info!("=== Recording daemon stopped ===");
    Ok(())
}

/// Spawn FFmpeg process and start reader thread
fn spawn_ffmpeg_and_reader(
    microphone: &str,
    buffer: SharedBuffer,
    state: SharedState,
) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Spawning FFmpeg process...");

    let child = Command::new("ffmpeg")
        .args([
            "-f",
            "pulse",
            "-i",
            microphone,
            "-ar",
            &SAMPLE_RATE.to_string(),
            "-ac",
            "1", // Mono
            "-f",
            "s16le",  // 16-bit PCM little-endian
            "pipe:1", // Output to stdout
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let pid = child.id();
    debug!("FFmpeg started with PID: {}", pid);

    // Save process handle in state
    {
        let mut state_guard = state.lock().unwrap();
        state_guard.ffmpeg_process = Some(child);
    }

    // Spawn reader thread
    spawn_reader_thread(buffer, state);

    Ok(())
}

/// Spawn thread to continuously read from FFmpeg stdout
fn spawn_reader_thread(buffer: SharedBuffer, state: SharedState) {
    thread::spawn(move || {
        debug!("Reader thread started");

        loop {
            // Get stdout handle from FFmpeg process
            let stdout = {
                let mut state_guard = state.lock().unwrap();
                if let Some(ref mut child) = state_guard.ffmpeg_process {
                    child.stdout.take()
                } else {
                    error!(" No FFmpeg process in state");
                    break;
                }
            };

            if let Some(mut stdout) = stdout {
                debug!("Reading from FFmpeg stdout...");

                let mut chunk = vec![0u8; 16384]; // 8192 samples = 0.5 seconds @ 16kHz
                let mut samples_written = 0;

                loop {
                    match stdout.read(&mut chunk) {
                        Ok(0) => {
                            error!(" FFmpeg stdout closed (EOF)");
                            break; // EOF - FFmpeg crashed
                        }
                        Ok(n) => {
                            // Convert bytes to i16 samples (little-endian)
                            let sample_count = n / 2;
                            let mut samples = Vec::with_capacity(sample_count);

                            for i in (0..n).step_by(2) {
                                if i + 1 < n {
                                    let sample = i16::from_le_bytes([chunk[i], chunk[i + 1]]);
                                    samples.push(sample);
                                }
                            }

                            // Write to buffer
                            {
                                let mut buf = buffer.lock().unwrap();
                                buf.write_samples(&samples);
                            }

                            samples_written += samples.len();
                        }
                        Err(e) => {
                            error!(" Read error from FFmpeg: {}", e);
                            break;
                        }
                    }
                }

                debug!(
                    "FFmpeg stream ended, total samples written: {}",
                    samples_written
                );

                // Put stdout back (needed for cleanup)
                let mut state_guard = state.lock().unwrap();
                if let Some(ref mut child) = state_guard.ffmpeg_process {
                    child.stdout = Some(stdout);
                }
            }

            // FFmpeg crashed - wait and restart
            debug!("Restarting FFmpeg after crash...");
            thread::sleep(Duration::from_secs(1));

            // Respawn FFmpeg
            if let Err(e) = respawn_ffmpeg(&state) {
                error!(" Failed to respawn FFmpeg: {}", e);
                break;
            }
        }

        debug!("Reader thread exiting");
    });
}

/// Respawn FFmpeg after a crash
fn respawn_ffmpeg(state: &SharedState) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Respawning FFmpeg...");

    // Get microphone from environment
    let microphone = std::env::var("RECORDING_MICROPHONE").unwrap_or_else(|_| {
        "alsa_input.usb-046d_C922_Pro_Stream_Webcam_C4C393EF-02.analog-stereo".to_string()
    });

    let child = Command::new("ffmpeg")
        .args([
            "-f",
            "pulse",
            "-i",
            &microphone,
            "-ar",
            &SAMPLE_RATE.to_string(),
            "-ac",
            "1",
            "-f",
            "s16le",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let pid = child.id();
    debug!("FFmpeg respawned with PID: {}", pid);

    let mut state_guard = state.lock().unwrap();
    state_guard.ffmpeg_process = Some(child);

    Ok(())
}

/// Listen on Unix socket for commands
fn listen_on_socket(
    state: SharedState,
    running: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Remove old socket if it exists
    fs::remove_file(SOCKET_PATH).ok();

    let listener = UnixListener::bind(SOCKET_PATH)?;
    debug!("Listening on socket: {}", SOCKET_PATH);

    // Set socket timeout so we can check running flag
    listener.set_nonblocking(true)?;

    while running.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                handle_client(stream, Arc::clone(&state));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No connection available, sleep and retry
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => {
                error!(" Accept failed: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Handle a client connection
fn handle_client(mut stream: UnixStream, state: SharedState) {
    let peer_addr = stream
        .peer_addr()
        .map(|a| format!("{:?}", a))
        .unwrap_or_else(|_| "unknown".to_string());
    debug!("Client connected: {}", peer_addr);

    let reader = BufReader::new(stream.try_clone().unwrap());

    for line in reader.lines() {
        match line {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                debug!("Received: {}", line);

                let response = match serde_json::from_str::<Value>(&line) {
                    Ok(request) => handle_request(request, Arc::clone(&state)),
                    Err(e) => json!({"ok": false, "error": format!("Invalid JSON: {}", e)}),
                };

                debug!("Sending: {}", response);

                if let Err(e) = writeln!(stream, "{}", response) {
                    error!(" Failed to send response: {}", e);
                    break;
                }
            }
            Err(e) => {
                error!(" Failed to read line: {}", e);
                break;
            }
        }
    }

    debug!("Client disconnected");
}

/// Handle a single request
fn handle_request(request: Value, state: SharedState) -> Value {
    let command = request["command"].as_str().unwrap_or("");

    match command {
        "start" => handle_start(state),
        "stop" => handle_stop(request, state),
        "ping" => handle_ping(state),
        _ => json!({"ok": false, "error": format!("Unknown command: {}", command)}),
    }
}

/// Handle start recording command
fn handle_start(state: SharedState) -> Value {
    let mut state_guard = state.lock().unwrap();

    // Check if already recording
    if state_guard.recording_start_index.is_some() {
        return json!({"ok": false, "error": "Already recording"});
    }

    // Get current buffer index
    let start_index = state_guard.buffer.lock().unwrap().get_current_index();

    state_guard.recording_start_index = Some(start_index);

    debug!("Recording started at index: {}", start_index);

    json!({"ok": true, "start_index": start_index})
}

/// Handle stop recording command
fn handle_stop(request: Value, state: SharedState) -> Value {
    let start_time = Instant::now();

    let mut state_guard = state.lock().unwrap();

    // Get start_index from request
    let start_index = match request["start_index"].as_u64() {
        Some(idx) => idx as usize,
        None => {
            return json!({"ok": false, "error": "Missing start_index parameter"});
        }
    };

    // Verify we're recording
    if state_guard.recording_start_index != Some(start_index) {
        return json!({"ok": false, "error": "Not recording or index mismatch"});
    }

    // Get current buffer index and extract samples
    let samples = {
        let buffer_guard = state_guard.buffer.lock().unwrap();
        let end_index = buffer_guard.get_current_index();
        let sample_count = end_index.saturating_sub(start_index);

        debug!(
            "Stopping recording: start={}, end={}, samples={}",
            start_index, end_index, sample_count
        );

        if sample_count == 0 {
            drop(buffer_guard);
            state_guard.recording_start_index = None;
            return json!({"ok": false, "error": "No audio recorded"});
        }

        // Check if recording was too long and got truncated
        let buffer_capacity = buffer_guard.stats().capacity;
        if sample_count > buffer_capacity {
            debug!(
                "WARNING: Recording truncated to last {} seconds",
                BUFFER_DURATION_SECONDS
            );
        }

        // Extract samples from buffer
        buffer_guard.extract_range(start_index, sample_count)
        // buffer_guard dropped here
    };

    if samples.is_empty() {
        state_guard.recording_start_index = None;
        return json!({"ok": false, "error": "Audio data lost (buffer overwritten)"});
    }

    debug!("Extracted {} samples", samples.len());

    // Write WAV file
    match transcribe_rs::audio::write_wav_from_samples(
        &samples,
        SAMPLE_RATE,
        Path::new(OUTPUT_WAV_PATH),
    ) {
        Ok(_) => {
            let duration_ms = (samples.len() as f64 / SAMPLE_RATE as f64 * 1000.0) as u64;
            let latency_ms = start_time.elapsed().as_millis() as u64;

            debug!(
                "WAV file written: {} ({} samples, {:.2}s, latency: {}ms)",
                OUTPUT_WAV_PATH,
                samples.len(),
                duration_ms as f64 / 1000.0,
                latency_ms
            );

            state_guard.recording_start_index = None;

            json!({
                "ok": true,
                "wav_path": OUTPUT_WAV_PATH,
                "duration_ms": duration_ms,
                "latency_ms": latency_ms,
                "samples": samples.len()
            })
        }
        Err(e) => {
            error!(" Failed to write WAV file: {}", e);
            state_guard.recording_start_index = None;
            json!({"ok": false, "error": format!("Failed to write WAV: {}", e)})
        }
    }
}

/// Handle ping command (health check)
fn handle_ping(state: SharedState) -> Value {
    let state_guard = state.lock().unwrap();
    let buffer_guard = state_guard.buffer.lock().unwrap();
    let stats = buffer_guard.stats();

    json!({
        "ok": true,
        "uptime_seconds": state_guard.start_time.elapsed().as_secs(),
        "buffer_fullness": stats.fullness_percent / 100.0,
        "total_written": stats.total_written,
        "is_recording": state_guard.recording_start_index.is_some()
    })
}

/// Cleanup FFmpeg process on shutdown
fn cleanup_ffmpeg(state: SharedState) {
    debug!("Cleaning up FFmpeg process...");

    let mut state_guard = state.lock().unwrap();

    if let Some(mut child) = state_guard.ffmpeg_process.take() {
        debug!("Killing FFmpeg process (PID: {})", child.id());

        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            let pid = Pid::from_raw(child.id() as i32);
            kill(pid, Signal::SIGTERM).ok();
        }

        // Wait up to 2 seconds for graceful exit
        for _ in 0..20 {
            match child.try_wait() {
                Ok(Some(_)) => {
                    debug!("FFmpeg exited gracefully");
                    return;
                }
                Ok(None) => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    error!(" Failed to wait for FFmpeg: {}", e);
                    break;
                }
            }
        }

        // Force kill if still running
        debug!("Force killing FFmpeg");
        child.kill().ok();
        child.wait().ok();
    }
}
