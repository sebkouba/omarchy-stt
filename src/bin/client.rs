use serde::{Deserialize, Serialize};
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use transcribe_rs::{config::Config, logging};

#[derive(Debug, Serialize)]
struct TranscribeRequest {
    file: String,
}

#[derive(Debug, Deserialize)]
struct TranscribeResponse {
    success: bool,
    text: Option<String>,
    error: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let _ = logging::init();

    // Load configuration
    let config = Config::load()?;
    let socket_path = &config.daemon.socket_path;

    // Get file path from command line argument
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <audio_file.wav>", args[0]);
        std::process::exit(1);
    }

    let audio_file = &args[1];

    // Connect to daemon
    let stream = UnixStream::connect(socket_path).map_err(|e| {
        eprintln!(
            "❌ Failed to connect to transcribe daemon at {}",
            socket_path
        );
        eprintln!("   Is the daemon running? Start it with: transcribe-daemon");
        e
    })?;

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    // Send transcription request
    let request = TranscribeRequest {
        file: audio_file.to_string(),
    };
    let request_json = serde_json::to_string(&request)?;
    writeln!(writer, "{}", request_json)?;

    // Read response
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    let response: TranscribeResponse = serde_json::from_str(&response_line)?;

    // Handle response
    if response.success {
        if let Some(text) = response.text {
            // Output only the transcription text (for compatibility with existing scripts)
            println!("{}", text);
        }
        Ok(())
    } else {
        if let Some(error) = response.error {
            eprintln!("❌ Error: {}", error);
        }
        std::process::exit(1);
    }
}
