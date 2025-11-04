use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use transcribe_rs::{
    engines::parakeet::{ParakeetEngine, ParakeetModelParams},
    TranscriptionEngine,
};

const SOCKET_PATH: &str = "/tmp/transcribe-rs-v2.sock";

#[derive(Debug, Deserialize)]
struct TranscribeRequest {
    file: String,
}

#[derive(Debug, Serialize)]
struct TranscribeResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn handle_client(stream: UnixStream, engine: &mut ParakeetEngine) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let mut line = String::new();

    // Read request
    reader.read_line(&mut line)?;

    let response = match serde_json::from_str::<TranscribeRequest>(&line) {
        Ok(request) => {
            let audio_path = PathBuf::from(&request.file);

            // Check if file exists
            if !audio_path.exists() {
                TranscribeResponse {
                    success: false,
                    text: None,
                    error: Some(format!("File not found: {}", request.file)),
                }
            } else {
                // Transcribe
                match engine.transcribe_file(&audio_path, None) {
                    Ok(result) => TranscribeResponse {
                        success: true,
                        text: Some(result.text),
                        error: None,
                    },
                    Err(e) => TranscribeResponse {
                        success: false,
                        text: None,
                        error: Some(format!("Transcription error: {}", e)),
                    },
                }
            }
        }
        Err(e) => TranscribeResponse {
            success: false,
            text: None,
            error: Some(format!("Invalid request: {}", e)),
        },
    };

    // Send response
    let response_json = serde_json::to_string(&response)?;
    writeln!(writer, "{}", response_json)?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("🚀 Starting transcribe-rs daemon...");

    // Load model once
    eprintln!("📦 Loading Parakeet model...");
    let mut engine = ParakeetEngine::new();
    let model_path = PathBuf::from("models/parakeet-tdt-0.6b-v3-int8");
    engine.load_model_with_params(&model_path, ParakeetModelParams::int8())?;
    eprintln!("✅ Model loaded successfully!");

    // Remove existing socket if it exists
    let _ = fs::remove_file(SOCKET_PATH);

    // Create Unix socket
    let listener = UnixListener::bind(SOCKET_PATH)?;
    eprintln!("👂 Listening on {}", SOCKET_PATH);
    eprintln!("Ready to accept transcription requests!");

    // Accept connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(e) = handle_client(stream, &mut engine) {
                    eprintln!("⚠️  Error handling client: {}", e);
                }
            }
            Err(e) => {
                eprintln!("⚠️  Connection error: {}", e);
            }
        }
    }

    // Cleanup
    engine.unload_model();
    let _ = fs::remove_file(SOCKET_PATH);

    Ok(())
}
