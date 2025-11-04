use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use transcribe_rs::{
    config::Config,
    engines::parakeet::{ParakeetEngine, ParakeetModelParams},
    TranscriptionEngine,
};

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

/// Check if socket file exists and is stale (not accepting connections)
fn is_socket_stale(socket_path: &str) -> bool {
    if !std::path::Path::new(socket_path).exists() {
        return false;
    }

    // Try to connect to the socket
    match UnixStream::connect(socket_path) {
        Ok(_) => {
            // Socket is active, not stale
            false
        }
        Err(_) => {
            // Socket exists but can't connect - it's stale
            true
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("🚀 Starting transcribe-rs daemon...");

    // Load configuration
    eprintln!("📋 Loading configuration...");
    let config = Config::load()?;
    let socket_path = config.daemon.socket_path.clone();
    let model_path = PathBuf::from(&config.model.path);

    eprintln!("   Socket: {}", socket_path);
    eprintln!("   Model: {}", config.model.path);

    // Set up signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    let socket_path_clone = socket_path.clone();

    ctrlc::set_handler(move || {
        eprintln!("\n🛑 Received shutdown signal, cleaning up...");
        r.store(false, Ordering::SeqCst);

        // Remove socket file
        if let Err(e) = fs::remove_file(&socket_path_clone) {
            eprintln!("⚠️  Warning: Failed to remove socket file: {}", e);
        } else {
            eprintln!("✓ Socket file removed");
        }

        std::process::exit(0);
    })?;

    // Check model exists before trying to load
    if !model_path.exists() {
        eprintln!("❌ Model not found: {}", model_path.display());
        eprintln!("\nDownload Parakeet model:");
        eprintln!("  mkdir -p models && cd models");
        eprintln!("  wget https://blob.handy.computer/parakeet-v3-int8.tar.gz");
        eprintln!("  tar -xzf parakeet-v3-int8.tar.gz");
        eprintln!("\nOr update config with correct path:");
        eprintln!("  nano ~/.config/transcribe-rs/config.toml");
        eprintln!("\nRun health check:");
        eprintln!("  transcribe doctor");
        return Err(format!("Model not found: {}", model_path.display()).into());
    }

    // Load model once
    eprintln!("📦 Loading {} model...", config.model.engine);
    let mut engine = ParakeetEngine::new();

    let model_params = match config.model.quantization.as_str() {
        "int8" => ParakeetModelParams::int8(),
        "fp32" => ParakeetModelParams::fp32(),
        _ => {
            eprintln!("⚠️  Unknown quantization: {}, defaulting to int8", config.model.quantization);
            ParakeetModelParams::int8()
        }
    };

    engine.load_model_with_params(&model_path, model_params)
        .map_err(|e| {
            eprintln!("❌ Failed to load model: {}", e);
            eprintln!("\nCheck that model files are complete:");
            eprintln!("  ls -lh {}", model_path.display());
            eprintln!("\nExpected files:");
            eprintln!("  - encoder-model.int8.onnx (or encoder-model.onnx for fp32)");
            eprintln!("  - decoder_joint-model.int8.onnx (or decoder_joint-model.onnx for fp32)");
            eprintln!("  - nemo128.onnx");
            eprintln!("  - vocab.txt");
            e
        })?;
    eprintln!("✅ Model loaded successfully!");

    // Check if socket exists and handle it
    if std::path::Path::new(&socket_path).exists() {
        if is_socket_stale(&socket_path) {
            eprintln!("⚠️  Removing stale socket file...");
            fs::remove_file(&socket_path)?;
        } else {
            return Err(format!(
                "Socket {} already exists and another daemon is running. \
                Stop the other daemon first or use a different socket path.",
                socket_path
            ).into());
        }
    }

    // Create Unix socket
    let listener = UnixListener::bind(&socket_path)?;
    eprintln!("👂 Listening on {}", socket_path);
    eprintln!("Ready to accept transcription requests!");

    // Accept connections
    for stream in listener.incoming() {
        if !running.load(Ordering::SeqCst) {
            break;
        }

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
    eprintln!("🧹 Cleaning up...");
    engine.unload_model();
    let _ = fs::remove_file(&socket_path);

    eprintln!("👋 Daemon stopped");
    Ok(())
}
