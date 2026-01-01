use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;
use transcribe_rs::{
    config::Config,
    engines::parakeet::{ParakeetEngine, ParakeetModelParams},
    file_watcher::{self, FileWatcher, WatchEvent},
    logging, TranscriptionEngine,
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

// Daemon state that holds the transcription engine
struct DaemonState {
    transcription_engine: ParakeetEngine,
}

impl DaemonState {
    fn new(
        model_path: &Path,
        model_params: ParakeetModelParams,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Load Parakeet model
        let mut engine = ParakeetEngine::new();
        engine.load_model_with_params(model_path, model_params)?;

        Ok(DaemonState {
            transcription_engine: engine,
        })
    }

    fn unload_model(&mut self) {
        self.transcription_engine.unload_model();
    }
}

fn handle_client(
    stream: UnixStream,
    state: &mut DaemonState,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let mut line = String::new();

    // Read request
    reader.read_line(&mut line)?;

    // Parse request
    let response = if let Ok(request) = serde_json::from_str::<TranscribeRequest>(&line) {
        handle_transcribe_request(request, state)
    } else {
        TranscribeResponse {
            success: false,
            text: None,
            error: Some("Invalid request format".to_string()),
        }
    };

    // Send response
    let response_json = serde_json::to_string(&response)?;
    writeln!(writer, "{}", response_json)?;

    Ok(())
}

// Handle transcription request
fn handle_transcribe_request(
    request: TranscribeRequest,
    state: &mut DaemonState,
) -> TranscribeResponse {
    let audio_path = PathBuf::from(&request.file);

    // Check if file exists
    if !audio_path.exists() {
        return TranscribeResponse {
            success: false,
            text: None,
            error: Some(format!("File not found: {}", request.file)),
        };
    }

    // Transcribe
    match state
        .transcription_engine
        .transcribe_file(&audio_path, None)
    {
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

/// Handle a file watcher event - transcribe and save to text file
fn handle_watch_event(event: WatchEvent, state: &mut DaemonState, watch_dir: &Path) {
    match event {
        WatchEvent::FileReady {
            wav_path,
            original_path,
        } => {
            eprintln!("📁 Processing watched file: {}", original_path.display());

            // Split into chunks if needed (for long audio files)
            let chunks = match file_watcher::split_audio_if_needed(&wav_path) {
                Ok(chunks) => chunks,
                Err(e) => {
                    eprintln!("❌ Failed to check/split audio: {}", e);
                    return;
                }
            };

            // Transcribe all chunks and concatenate results
            let mut full_text = String::new();
            let mut success = true;

            for (i, chunk_path) in chunks.iter().enumerate() {
                if chunks.len() > 1 {
                    eprintln!("🔄 Transcribing chunk {}/{}", i + 1, chunks.len());
                }

                match state.transcription_engine.transcribe_file(chunk_path, None) {
                    Ok(result) => {
                        if !full_text.is_empty() && !result.text.is_empty() {
                            full_text.push(' ');
                        }
                        full_text.push_str(&result.text);
                    }
                    Err(e) => {
                        eprintln!("❌ Transcription failed for chunk {}: {}", i + 1, e);
                        success = false;
                        break;
                    }
                }
            }

            // Clean up chunk files
            file_watcher::cleanup_chunks(&chunks, &wav_path);

            if success && !full_text.is_empty() {
                // Create the text file path (same name as original, .txt extension)
                let text_filename = original_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("transcription");
                let text_path = watch_dir.join(format!("{}.txt", text_filename));

                // Write transcription to text file
                match fs::write(&text_path, &full_text) {
                    Ok(_) => {
                        eprintln!("✅ Transcription saved: {}", text_path.display());

                        // Move original (and temp WAV if different) to processed
                        if let Err(e) =
                            file_watcher::move_to_processed(&original_path, &wav_path, watch_dir)
                        {
                            eprintln!("⚠️  Failed to move to processed: {}", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to write transcription: {}", e);
                    }
                }
            } else if full_text.is_empty() {
                eprintln!("⚠️  No transcription text generated");
            }
        }
        WatchEvent::Error(e) => {
            eprintln!("⚠️  File watcher error: {}", e);
        }
    }
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
    // Initialize logging
    if let Err(e) = logging::init() {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    }

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

    // Load model
    eprintln!("📦 Loading {} model...", config.model.engine);

    let model_params = match config.model.quantization.as_str() {
        "int8" => ParakeetModelParams::int8(),
        "fp32" => ParakeetModelParams::fp32(),
        _ => {
            eprintln!(
                "⚠️  Unknown quantization: {}, defaulting to int8",
                config.model.quantization
            );
            ParakeetModelParams::int8()
        }
    };

    let mut state = DaemonState::new(&model_path, model_params).map_err(|e| {
        eprintln!("❌ Failed to initialize daemon: {}", e);
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
            )
            .into());
        }
    }

    // Create Unix socket
    let listener = UnixListener::bind(&socket_path)?;
    eprintln!("👂 Listening on {}", socket_path);

    // Set up file watcher if enabled
    let watch_receiver: Option<Receiver<WatchEvent>>;
    let watch_dir: Option<PathBuf>;
    let _file_watcher: Option<FileWatcher>;

    if config.watch.enabled {
        let watch_path = PathBuf::from(&config.watch.watch_dir);
        eprintln!("👁️  Watching directory: {}", watch_path.display());

        match FileWatcher::new(config.watch.clone()) {
            Ok((watcher, receiver)) => {
                _file_watcher = Some(watcher);
                watch_receiver = Some(receiver);
                watch_dir = Some(watch_path);
            }
            Err(e) => {
                eprintln!("⚠️  Failed to start file watcher: {}", e);
                eprintln!("   Directory watching disabled.");
                _file_watcher = None;
                watch_receiver = None;
                watch_dir = None;
            }
        }
    } else {
        _file_watcher = None;
        watch_receiver = None;
        watch_dir = None;
    }

    eprintln!("Ready to accept transcription requests!");

    // Set socket to non-blocking for polling
    listener.set_nonblocking(true)?;

    // Main event loop
    while running.load(Ordering::SeqCst) {
        // Check for socket connections
        match listener.accept() {
            Ok((stream, _)) => {
                // Set stream back to blocking for the handler
                stream.set_nonblocking(false)?;
                if let Err(e) = handle_client(stream, &mut state) {
                    eprintln!("⚠️  Error handling client: {}", e);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No pending connections, continue
            }
            Err(e) => {
                eprintln!("⚠️  Connection error: {}", e);
            }
        }

        // Check for file watcher events
        if let (Some(ref rx), Some(ref dir)) = (&watch_receiver, &watch_dir) {
            // Non-blocking receive
            while let Ok(event) = rx.try_recv() {
                handle_watch_event(event, &mut state, dir);
            }
        }

        // Small sleep to avoid busy-waiting
        std::thread::sleep(Duration::from_millis(10));
    }

    // Cleanup
    eprintln!("🧹 Cleaning up...");
    state.unload_model();
    let _ = fs::remove_file(&socket_path);

    eprintln!("👋 Daemon stopped");
    Ok(())
}
