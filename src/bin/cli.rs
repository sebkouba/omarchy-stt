use clap::{Parser, Subcommand};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use transcribe_rs::{clipboard, notifications, paste, recording};

/// Append a log message to the debug log
fn log(message: &str) {
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/ptt_rust_debug.log")
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        writeln!(file, "[{}] [cli] {}", timestamp, message).ok();
    }
}

#[derive(Parser)]
#[command(name = "transcribe")]
#[command(about = "Audio transcription with push-to-talk support", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start push-to-talk recording
    Start,
    /// Stop recording and transcribe
    Stop,
    /// Start the transcription daemon (same as transcribe-daemon binary)
    Daemon,
    /// Send a file to the transcription daemon (same as transcribe-client binary)
    Client {
        /// Path to audio file
        file: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start => handle_start(),
        Commands::Stop => handle_stop(),
        Commands::Daemon => {
            // Delegate to existing daemon implementation
            eprintln!("To run the daemon, use: transcribe-daemon");
            eprintln!("(Daemon integration will be added in Phase 3)");
            Ok(())
        }
        Commands::Client { file } => {
            // Delegate to existing client implementation
            eprintln!("To transcribe a file, use: transcribe-client {:?}", file);
            eprintln!("(Client integration will be added in Phase 3)");
            Ok(())
        }
    }
}

fn handle_start() -> Result<(), Box<dyn Error>> {
    log("=== HANDLE START ===");

    log("Starting recording...");
    recording::start_recording()?;
    log("Recording started successfully");

    log("Sending notification...");
    if let Err(e) = notifications::notify_recording_started() {
        log(&format!("WARNING: Notification failed: {}", e));
    }

    println!("🎤 Recording started...");
    log("=== HANDLE START COMPLETE ===");
    Ok(())
}

fn handle_stop() -> Result<(), Box<dyn Error>> {
    log("=== HANDLE STOP ===");

    // Stop recording
    log("Stopping recording...");
    let audio_file = recording::stop_recording()?;
    log(&format!("Audio file: {:?}", audio_file));

    log("Sending stop notification...");
    if let Err(e) = notifications::notify_recording_stopped() {
        log(&format!("WARNING: Notification failed: {}", e));
    }
    println!("⏹️  Recording stopped");

    // Transcribe using daemon client
    println!("📝 Transcribing...");
    log("Calling transcribe_file...");
    let transcription = match transcribe_file(&audio_file) {
        Ok(t) => {
            log(&format!("Transcription received: {} chars", t.len()));
            t
        }
        Err(e) => {
            log(&format!("ERROR: Transcription failed: {}", e));
            eprintln!("Transcription error: {}", e);
            return Err(e);
        }
    };

    if transcription.is_empty() {
        log("ERROR: Empty transcription");
        notifications::notify_error("No speech detected").ok();
        return Err("Empty transcription".into());
    }

    log(&format!("Transcription text: '{}'", transcription));

    // Add space after punctuation
    log("Adding trailing space after punctuation...");
    let text = clipboard::add_trailing_space_after_punctuation(&transcription);
    log(&format!("Final text: '{}'", text));

    // Copy to clipboard
    log("Copying to clipboard...");
    match clipboard::copy_to_clipboard(&text) {
        Ok(_) => log("Clipboard copy successful"),
        Err(e) => {
            log(&format!("ERROR: Clipboard copy failed: {}", e));
            eprintln!("Clipboard error: {}", e);
            return Err(e);
        }
    }

    // Create preview
    let preview = if text.len() > 100 {
        format!("{}...", &text[..100])
    } else {
        text.clone()
    };
    log(&format!("Preview: '{}'", preview));

    // Auto-paste if ydotool available
    log("Checking if ydotool is available...");
    if paste::is_ydotool_available() {
        log("ydotool available, attempting paste...");
        match paste::paste_from_clipboard() {
            Ok(_) => {
                log("Paste successful");
                notifications::notify_transcription_pasted(&preview).ok();
            }
            Err(e) => {
                log(&format!("WARNING: Paste failed: {}", e));
                notifications::notify_transcription_copied(&preview).ok();
            }
        }
    } else {
        log("ydotool not available, clipboard only");
        notifications::notify_transcription_copied(&preview).ok();
    }

    println!("Transcription: {}", text);
    log("=== HANDLE STOP COMPLETE ===");
    Ok(())
}

/// Call the transcribe-client binary to transcribe a file
fn transcribe_file(file: &PathBuf) -> Result<String, Box<dyn Error>> {
    // Try to find transcribe-client in the same directory as this binary
    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe.parent().ok_or("Cannot get exe directory")?;
    let client_path = exe_dir.join("transcribe-client");

    log(&format!("Looking for transcribe-client at: {:?}", client_path));

    if !client_path.exists() {
        log(&format!("ERROR: transcribe-client not found at {:?}", client_path));
        return Err(format!(
            "transcribe-client binary not found at {:?}. Make sure the daemon is running.",
            client_path
        ).into());
    }

    log(&format!("Calling transcribe-client with file: {:?}", file));
    let output = Command::new(&client_path)
        .arg(file)
        .output()
        .map_err(|e| {
            log(&format!("ERROR: Failed to execute transcribe-client: {}", e));
            e
        })?;

    log(&format!("transcribe-client exit status: {:?}", output.status));

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        log(&format!("transcribe-client stderr: {}", error));
        return Err(format!("Transcription failed: {}", error).into());
    }

    let text = String::from_utf8(output.stdout)?;
    log(&format!("transcribe-client stdout: '{}'", text));
    Ok(text.trim().to_string())
}
