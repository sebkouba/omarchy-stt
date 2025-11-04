//! Audio recording management using ffmpeg

use crate::config::AudioConfig;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

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

/// Start recording audio with ffmpeg
pub fn start_recording(config: &AudioConfig) -> Result<(), Box<dyn Error>> {
    log("=== Recording start requested ===", &config.log_file);

    // Check if already recording
    if Path::new(&config.recording_pid_file).exists() {
        let pid = fs::read_to_string(&config.recording_pid_file)?;
        log(&format!("WARNING: Already recording (PID: {})", pid.trim()), &config.log_file);
        return Err("Already recording".into());
    }

    // Remove old recording file
    if Path::new(&config.recording_path).exists() {
        let metadata = fs::metadata(&config.recording_path)?;
        fs::remove_file(&config.recording_path)?;
        log(&format!("Removed old recording file ({} bytes)", metadata.len()), &config.log_file);
    }

    // Start ffmpeg in background
    log(&format!("Starting ffmpeg recording to {}", config.recording_path), &config.log_file);
    let child = Command::new("ffmpeg")
        .args([
            "-f", "pulse",
            "-i", &config.microphone,
            "-ar", &config.sample_rate.to_string(),
            "-ac", "1",
            "-sample_fmt", "s16",
            "-y", &config.recording_path,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            log(&format!("ERROR: Failed to spawn ffmpeg: {}", e), &config.log_file);
            format!("Failed to start ffmpeg: {}\n\nIs ffmpeg installed? Check with: which ffmpeg\nInstall with: sudo pacman -S ffmpeg", e)
        })?;

    let pid = child.id();
    fs::write(&config.recording_pid_file, pid.to_string())?;
    log(&format!("ffmpeg started with PID: {}", pid), &config.log_file);

    // Give ffmpeg time to initialize
    thread::sleep(Duration::from_millis(150));
    log("ffmpeg initialization delay complete", &config.log_file);

    // Verify ffmpeg is still running
    if !is_process_running(pid) {
        fs::remove_file(&config.recording_pid_file)?;
        log("ERROR: ffmpeg died immediately after starting!", &config.log_file);

        let err_msg = format!(
            "ffmpeg failed to start recording.\n\n\
            Possible causes:\n\
            1. Microphone not found: '{}'\n\
            2. Microphone in use by another application\n\
            3. PulseAudio not running\n\n\
            List available microphones:\n\
              pactl list sources short\n\n\
            Fix configuration:\n\
              transcribe config show\n\
              nano ~/.config/transcribe-rs/config.toml\n\n\
            Check system:\n\
              transcribe doctor",
            config.microphone
        );

        return Err(err_msg.into());
    }

    log("Recording started successfully", &config.log_file);
    Ok(())
}

/// Stop recording and return the path to the audio file
pub fn stop_recording(config: &AudioConfig) -> Result<PathBuf, Box<dyn Error>> {
    log("=== Recording stop requested ===", &config.log_file);

    // Check if recording
    if !Path::new(&config.recording_pid_file).exists() {
        log("WARNING: Not recording (PID file not found)", &config.log_file);
        return Err("Not recording".into());
    }

    // Get PID and kill ffmpeg
    let pid_str = fs::read_to_string(&config.recording_pid_file)?;
    let pid: u32 = pid_str.trim().parse()?;
    log(&format!("Stopping recording (PID: {})", pid), &config.log_file);

    // Send SIGINT to ffmpeg
    if is_process_running(pid) {
        log("Sending SIGINT to ffmpeg...", &config.log_file);
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            kill(Pid::from_raw(pid as i32), Signal::SIGINT)?;
        }
    } else {
        log("WARNING: Process not found in process table", &config.log_file);
    }

    // Wait for ffmpeg to exit (with timeout)
    log("Waiting for ffmpeg to exit...", &config.log_file);
    let start = Instant::now();
    let max_wait = Duration::from_secs(5);
    let mut poll_count = 0;

    while is_process_running(pid) {
        poll_count += 1;
        if start.elapsed() > max_wait {
            log(&format!("ERROR: ffmpeg did not exit after {:?}, killing forcefully", start.elapsed()), &config.log_file);
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

    log(&format!("ffmpeg exited after {:?} (polled {} times)", start.elapsed(), poll_count), &config.log_file);

    // Give filesystem time to flush
    thread::sleep(Duration::from_millis(50));
    log("Filesystem sync delay complete", &config.log_file);

    // Remove PID file
    fs::remove_file(&config.recording_pid_file)?;
    log("PID file removed", &config.log_file);

    // Verify file stability
    verify_file_stable(config)?;

    // Validate file
    let file_size = fs::metadata(&config.recording_path)?.len();
    log(&format!("Recording file size: {} bytes", file_size), &config.log_file);

    if file_size < 1000 {
        log(&format!("ERROR: Recording file too small ({} bytes)", file_size), &config.log_file);
        return Err("Recording file too small - microphone may be busy".into());
    }

    log("Recording stopped successfully", &config.log_file);
    Ok(PathBuf::from(&config.recording_path))
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
fn verify_file_stable(config: &AudioConfig) -> Result<(), Box<dyn Error>> {
    if !Path::new(&config.recording_path).exists() {
        return Err("Recording file not found".into());
    }

    let size1 = fs::metadata(&config.recording_path)?.len();
    thread::sleep(Duration::from_millis(20));
    let size2 = fs::metadata(&config.recording_path)?.len();

    if size1 != size2 {
        log(&format!("WARNING: File size changed from {} to {} bytes, waiting longer...", size1, size2), &config.log_file);
        thread::sleep(Duration::from_millis(100));
        let size3 = fs::metadata(&config.recording_path)?.len();
        log(&format!("File size after additional wait: {} bytes", size3), &config.log_file);
    } else {
        log(&format!("File size stable at {} bytes", size1), &config.log_file);
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
    fn test_default_config_values() {
        // Verify default config has reasonable values
        let config = AudioConfig::default();
        assert!(!config.recording_pid_file.is_empty());
        assert!(!config.recording_path.is_empty());
        assert!(!config.microphone.is_empty());
        assert!(!config.log_file.is_empty());
        assert_eq!(config.sample_rate, 16000);
    }
}
