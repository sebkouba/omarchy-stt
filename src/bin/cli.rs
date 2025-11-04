use clap::{Parser, Subcommand};
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use transcribe_rs::{clipboard, notifications, paste, recording};

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
    recording::start_recording()?;
    notifications::notify_recording_started()?;
    println!("🎤 Recording started...");
    Ok(())
}

fn handle_stop() -> Result<(), Box<dyn Error>> {
    // Stop recording
    let audio_file = recording::stop_recording()?;
    notifications::notify_recording_stopped()?;
    println!("⏹️  Recording stopped");

    // Transcribe using daemon client
    println!("📝 Transcribing...");
    let transcription = transcribe_file(&audio_file)?;

    if transcription.is_empty() {
        notifications::notify_error("No speech detected")?;
        return Err("Empty transcription".into());
    }

    // Add space after punctuation
    let text = clipboard::add_trailing_space_after_punctuation(&transcription);

    // Copy to clipboard
    clipboard::copy_to_clipboard(&text)?;

    // Create preview
    let preview = if text.len() > 100 {
        format!("{}...", &text[..100])
    } else {
        text.clone()
    };

    // Auto-paste if ydotool available
    if paste::is_ydotool_available() {
        paste::paste_from_clipboard()?;
        notifications::notify_transcription_pasted(&preview)?;
    } else {
        notifications::notify_transcription_copied(&preview)?;
    }

    println!("Transcription: {}", text);
    Ok(())
}

/// Call the transcribe-client binary to transcribe a file
fn transcribe_file(file: &PathBuf) -> Result<String, Box<dyn Error>> {
    let output = Command::new("transcribe-client")
        .arg(file)
        .output()?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Transcription failed: {}", error).into());
    }

    let text = String::from_utf8(output.stdout)?;
    Ok(text.trim().to_string())
}
