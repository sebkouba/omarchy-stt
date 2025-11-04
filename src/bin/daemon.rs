use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use serde::{Deserialize, Serialize};
use transcribe_rs::{
    config::Config,
    engines::parakeet::{ParakeetEngine, ParakeetModelParams},
    harper_processor::{self, Dialect},
    TranscriptionEngine,
};
use harper_core::spell::FstDictionary;

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

// New protocol structs for Harper integration

#[derive(Debug, Deserialize, Clone)]
struct HarperRequestConfig {
    enabled: bool,
    #[serde(default)]
    user_dict_path: String,
    #[serde(default)]
    dialect: String,
    #[serde(default)]
    disabled_linters: Vec<String>,
    #[serde(default)]
    save_corrections: bool,
    #[serde(default)]
    corrections_dir: String,
}

impl Default for HarperRequestConfig {
    fn default() -> Self {
        HarperRequestConfig {
            enabled: false,
            user_dict_path: String::new(),
            dialect: "American".to_string(),
            disabled_linters: vec![],
            save_corrections: false,
            corrections_dir: String::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ProcessingMetrics {
    transcription_ms: u128,
    harper_ms: u128,
    corrections_applied: usize,
}

#[derive(Debug, Deserialize)]
struct ProcessRequest {
    file: String,
    #[serde(default)]
    harper: HarperRequestConfig,
}

#[derive(Debug, Serialize)]
struct ProcessResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    processing: Option<ProcessingMetrics>,
}

// Daemon state that holds cached models and dictionaries
struct DaemonState {
    transcription_engine: ParakeetEngine,
    harper_dict_cache: Arc<FstDictionary>,
}

impl DaemonState {
    fn new(model_path: &Path, model_params: ParakeetModelParams) -> Result<Self, Box<dyn std::error::Error>> {
        // Load Parakeet model
        let mut engine = ParakeetEngine::new();
        engine.load_model_with_params(model_path, model_params)?;

        // Pre-load Harper dictionary (one-time cost at daemon startup)
        eprintln!("📖 Loading Harper dictionaries...");
        let harper_dict = FstDictionary::curated();
        eprintln!("✅ Harper ready");

        Ok(DaemonState {
            transcription_engine: engine,
            harper_dict_cache: harper_dict,
        })
    }

    fn unload_model(&mut self) {
        self.transcription_engine.unload_model();
    }
}

// Parse dialect string to enum
fn parse_dialect(dialect_str: &str) -> Dialect {
    match dialect_str {
        "British" => Dialect::British,
        "Australian" => Dialect::Australian,
        "Canadian" => Dialect::Canadian,
        _ => Dialect::American, // Default to American
    }
}

fn handle_client(stream: UnixStream, state: &mut DaemonState) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let mut line = String::new();

    // Read request
    reader.read_line(&mut line)?;

    // Try new format first, fall back to old format for backward compatibility
    let response = if let Ok(request) = serde_json::from_str::<ProcessRequest>(&line) {
        // New protocol: full processing with Harper
        handle_process_request(request, state)
    } else if let Ok(request) = serde_json::from_str::<TranscribeRequest>(&line) {
        // Old protocol: transcription only (backward compatibility)
        handle_transcribe_request(request, state)
    } else {
        ProcessResponse {
            success: false,
            text: None,
            error: Some("Invalid request format".to_string()),
            processing: None,
        }
    };

    // Send response
    let response_json = serde_json::to_string(&response)?;
    writeln!(writer, "{}", response_json)?;

    Ok(())
}

// Handle old-style transcription request (backward compatibility)
fn handle_transcribe_request(request: TranscribeRequest, state: &mut DaemonState) -> ProcessResponse {
    let audio_path = PathBuf::from(&request.file);

    // Check if file exists
    if !audio_path.exists() {
        return ProcessResponse {
            success: false,
            text: None,
            error: Some(format!("File not found: {}", request.file)),
            processing: None,
        };
    }

    // Transcribe
    match state.transcription_engine.transcribe_file(&audio_path, None) {
        Ok(result) => ProcessResponse {
            success: true,
            text: Some(result.text),
            error: None,
            processing: None,
        },
        Err(e) => ProcessResponse {
            success: false,
            text: None,
            error: Some(format!("Transcription error: {}", e)),
            processing: None,
        },
    }
}

// Handle new-style processing request with full pipeline
fn handle_process_request(request: ProcessRequest, state: &mut DaemonState) -> ProcessResponse {
    let start = Instant::now();
    let audio_path = PathBuf::from(&request.file);

    // Check if file exists
    if !audio_path.exists() {
        return ProcessResponse {
            success: false,
            text: None,
            error: Some(format!("File not found: {}", request.file)),
            processing: None,
        };
    }

    // 1. Transcribe audio
    let transcription = match state.transcription_engine.transcribe_file(&audio_path, None) {
        Ok(result) => result,
        Err(e) => {
            return ProcessResponse {
                success: false,
                text: None,
                error: Some(format!("Transcription error: {}", e)),
                processing: None,
            };
        }
    };
    let transcription_ms = start.elapsed().as_millis();

    let mut text = transcription.text;
    let mut corrections_applied = 0;

    // 2. Apply Harper processing (if enabled)
    let harper_start = Instant::now();
    if request.harper.enabled {
        let user_dict_path = Path::new(&request.harper.user_dict_path);
        let dialect = parse_dialect(&request.harper.dialect);

        match harper_processor::process_with_harper_cached(
            &text,
            &state.harper_dict_cache,
            user_dict_path,
            dialect,
            &request.harper.disabled_linters,
        ) {
            Ok(session) => {
                text = session.corrected_text.clone();
                corrections_applied = session.corrections.len();

                // Log corrections if any were made
                if session.has_corrections() {
                    eprintln!("📝 Harper made {} correction(s)", corrections_applied);
                    for (i, correction) in session.corrections.iter().enumerate() {
                        eprintln!("   {}. '{}' → '{}' ({})",
                            i + 1,
                            correction.original,
                            correction.replacement,
                            correction.lint_kind
                        );
                    }
                }

                // Save correction session if requested
                if request.harper.save_corrections && session.has_corrections() {
                    let corrections_dir = Path::new(&request.harper.corrections_dir);
                    if let Err(e) = session.save_to_file(corrections_dir) {
                        eprintln!("⚠️  Failed to save correction session: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("⚠️  Harper processing failed: {}", e);
                // Graceful degradation: continue with uncorrected text
            }
        }
    }
    let harper_ms = harper_start.elapsed().as_millis();

    ProcessResponse {
        success: true,
        text: Some(text),
        error: None,
        processing: Some(ProcessingMetrics {
            transcription_ms,
            harper_ms,
            corrections_applied,
        }),
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

    // Load model and Harper dictionaries once
    eprintln!("📦 Loading {} model...", config.model.engine);

    let model_params = match config.model.quantization.as_str() {
        "int8" => ParakeetModelParams::int8(),
        "fp32" => ParakeetModelParams::fp32(),
        _ => {
            eprintln!("⚠️  Unknown quantization: {}, defaulting to int8", config.model.quantization);
            ParakeetModelParams::int8()
        }
    };

    let mut state = DaemonState::new(&model_path, model_params)
        .map_err(|e| {
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
                if let Err(e) = handle_client(stream, &mut state) {
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
    state.unload_model();
    let _ = fs::remove_file(&socket_path);

    eprintln!("👋 Daemon stopped");
    Ok(())
}
