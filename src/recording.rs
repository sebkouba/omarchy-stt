//! Audio recording management using ffmpeg

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

const RECORDING_PID_FILE: &str = "/tmp/ptt_recording.pid";
const RECORDING_FILE: &str = "/tmp/ptt_current.wav";
const MIC_SOURCE: &str = "alsa_input.usb-046d_C922_Pro_Stream_Webcam_C4C393EF-02.analog-stereo";
const LOG_FILE: &str = "/tmp/ptt_rust_debug.log";

/// Append a log message to the debug log
fn log(message: &str) {
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE)
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        writeln!(file, "[{}] [recording] {}", timestamp, message).ok();
    }
}

/// Start recording audio with ffmpeg
pub fn start_recording() -> Result<(), Box<dyn Error>> {
    log("=== Recording start requested ===");

    // Check if already recording
    if Path::new(RECORDING_PID_FILE).exists() {
        let pid = fs::read_to_string(RECORDING_PID_FILE)?;
        log(&format!("WARNING: Already recording (PID: {})", pid.trim()));
        return Err("Already recording".into());
    }

    // Remove old recording file
    if Path::new(RECORDING_FILE).exists() {
        let metadata = fs::metadata(RECORDING_FILE)?;
        fs::remove_file(RECORDING_FILE)?;
        log(&format!("Removed old recording file ({} bytes)", metadata.len()));
    }

    // Start ffmpeg in background
    log(&format!("Starting ffmpeg recording to {}", RECORDING_FILE));
    let child = Command::new("ffmpeg")
        .args([
            "-f", "pulse",
            "-i", MIC_SOURCE,
            "-ar", "16000",
            "-ac", "1",
            "-sample_fmt", "s16",
            "-y", RECORDING_FILE,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let pid = child.id();
    fs::write(RECORDING_PID_FILE, pid.to_string())?;
    log(&format!("ffmpeg started with PID: {}", pid));

    // Give ffmpeg time to initialize
    thread::sleep(Duration::from_millis(150));
    log("ffmpeg initialization delay complete");

    // Verify ffmpeg is still running
    if !is_process_running(pid) {
        fs::remove_file(RECORDING_PID_FILE)?;
        log("ERROR: ffmpeg died immediately after starting!");
        return Err("ffmpeg failed to start".into());
    }

    log("Recording started successfully");
    Ok(())
}

/// Stop recording and return the path to the audio file
pub fn stop_recording() -> Result<PathBuf, Box<dyn Error>> {
    log("=== Recording stop requested ===");

    // Check if recording
    if !Path::new(RECORDING_PID_FILE).exists() {
        log("WARNING: Not recording (PID file not found)");
        return Err("Not recording".into());
    }

    // Get PID and kill ffmpeg
    let pid_str = fs::read_to_string(RECORDING_PID_FILE)?;
    let pid: u32 = pid_str.trim().parse()?;
    log(&format!("Stopping recording (PID: {})", pid));

    // Send SIGINT to ffmpeg
    if is_process_running(pid) {
        log("Sending SIGINT to ffmpeg...");
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            kill(Pid::from_raw(pid as i32), Signal::SIGINT)?;
        }
    } else {
        log("WARNING: Process not found in process table");
    }

    // Wait for ffmpeg to exit (with timeout)
    log("Waiting for ffmpeg to exit...");
    let start = Instant::now();
    let max_wait = Duration::from_secs(5);
    let mut poll_count = 0;

    while is_process_running(pid) {
        poll_count += 1;
        if start.elapsed() > max_wait {
            log(&format!("ERROR: ffmpeg did not exit after {:?}, killing forcefully", start.elapsed()));
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                kill(Pid::from_raw(pid as i32), Signal::SIGKILL).ok();
            }
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    log(&format!("ffmpeg exited after {:?} (polled {} times)", start.elapsed(), poll_count));

    // Give filesystem time to flush
    thread::sleep(Duration::from_millis(50));
    log("Filesystem sync delay complete");

    // Remove PID file
    fs::remove_file(RECORDING_PID_FILE)?;
    log("PID file removed");

    // Verify file stability
    verify_file_stable()?;

    // Validate file
    let file_size = fs::metadata(RECORDING_FILE)?.len();
    log(&format!("Recording file size: {} bytes", file_size));

    if file_size < 1000 {
        log(&format!("ERROR: Recording file too small ({} bytes)", file_size));
        return Err("Recording file too small - microphone may be busy".into());
    }

    log("Recording stopped successfully");
    Ok(PathBuf::from(RECORDING_FILE))
}

/// Check if a process is running
fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        // Send signal 0 to check if process exists without actually sending a signal
        kill(Pid::from_raw(pid as i32), None).is_ok()
    }
    #[cfg(not(unix))]
    {
        // Fallback for non-Unix systems
        false
    }
}

/// Verify file size is stable (not still being written)
fn verify_file_stable() -> Result<(), Box<dyn Error>> {
    if !Path::new(RECORDING_FILE).exists() {
        return Err("Recording file not found".into());
    }

    let size1 = fs::metadata(RECORDING_FILE)?.len();
    thread::sleep(Duration::from_millis(20));
    let size2 = fs::metadata(RECORDING_FILE)?.len();

    if size1 != size2 {
        log(&format!("WARNING: File size changed from {} to {} bytes, waiting longer...", size1, size2));
        thread::sleep(Duration::from_millis(100));
        let size3 = fs::metadata(RECORDING_FILE)?.len();
        log(&format!("File size after additional wait: {} bytes", size3));
    } else {
        log(&format!("File size stable at {} bytes", size1));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_running_check() {
        // Test with our own process (should always return true)
        let our_pid = std::process::id();
        assert!(is_process_running(our_pid));

        // Test with a PID that definitely doesn't exist
        assert!(!is_process_running(9999999));
    }

    #[test]
    fn test_constants_are_valid() {
        // Just verify constants are set
        assert!(!RECORDING_PID_FILE.is_empty());
        assert!(!RECORDING_FILE.is_empty());
        assert!(!MIC_SOURCE.is_empty());
        assert!(!LOG_FILE.is_empty());
    }
}
