use clap::{Parser, Subcommand};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use transcribe_rs::{clipboard, config::Config, dictation_logger, notifications, ocr, paste, performance_log, recording, timing};

/// Append a log message to the debug log
fn log(message: &str, log_file: &str) {
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        writeln!(file, "[{}] [cli] {}", timestamp, message).ok();
    }
}

#[derive(Parser)]
#[command(name = "transcribe")]
#[command(about = "Audio transcription with push-to-talk support", long_about = None)]
#[command(version = env!("FULL_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start push-to-talk recording
    Start {
        /// Optional prompt name for LLM post-processing (e.g., "clean", "email")
        #[arg(short, long)]
        prompt: Option<String>,
        /// Capture screen OCR for context (requires tesseract)
        #[arg(long)]
        ocr: bool,
    },
    /// Stop recording and transcribe
    Stop,
    /// Configuration management
    Config {
        #[command(subcommand)]
        config_cmd: ConfigCommands,
    },
    /// Check system dependencies and configuration
    Doctor,
    /// Start the transcription daemon (same as transcribe-daemon binary)
    Daemon,
    /// Send a file to the transcription daemon (same as transcribe-client binary)
    Client {
        /// Path to audio file
        file: PathBuf,
    },
    /// Internal: OCR worker process (do not call directly)
    #[command(hide = true)]
    OcrWorker {
        /// Path to screenshot image
        screenshot_path: String,
        /// Path to write OCR result
        result_path: String,
        /// Tesseract language code
        language: String,
        /// OCR DPI setting
        dpi: String,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Initialize default configuration file
    Init,
    /// Show path to configuration file
    Path,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { prompt, ocr } => {
            let config = Config::load()?;
            handle_start(&config, prompt, ocr)
        }
        Commands::Stop => {
            let config = Config::load()?;
            handle_stop(&config)
        }
        Commands::Config { config_cmd } => handle_config(config_cmd),
        Commands::Doctor => handle_doctor(),
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
        Commands::OcrWorker { screenshot_path, result_path, language, dpi } => {
            // Internal OCR worker process
            let dpi_value: u32 = dpi.parse().unwrap_or(300);
            ocr::run_ocr_worker(&screenshot_path, &result_path, &language, dpi_value)
        }
    }
}

fn handle_start(config: &Config, prompt: Option<String>, ocr_enabled: bool) -> Result<(), Box<dyn Error>> {
    log("=== HANDLE START ===", &config.audio.log_file);

    // Save prompt name to temp file for stop command
    const PROMPT_STATE_FILE: &str = "/tmp/ptt_prompt.txt";
    if let Some(prompt_name) = prompt {
        log(&format!("Saving prompt name: {}", prompt_name), &config.audio.log_file);
        fs::write(PROMPT_STATE_FILE, prompt_name)?;
    } else {
        // Remove prompt file if no prompt specified
        let _ = fs::remove_file(PROMPT_STATE_FILE);
    }

    // Handle OCR if enabled - spawn async worker
    const OCR_REQUESTED_FILE: &str = "/tmp/ptt_ocr_requested.flag";
    if ocr_enabled {
        log("OCR requested, spawning async worker...", &config.audio.log_file);
        match ocr::spawn_ocr_worker(&config.ocr) {
            Ok(_) => {
                log("OCR worker spawned successfully", &config.audio.log_file);
                fs::write(OCR_REQUESTED_FILE, "")?;
            }
            Err(e) => {
                log(&format!("WARNING: OCR spawn failed: {}", e), &config.audio.log_file);
                // Continue without OCR - don't fail the whole operation
                let _ = fs::remove_file(OCR_REQUESTED_FILE);
            }
        }
    } else {
        // Remove OCR flag if not requested
        let _ = fs::remove_file(OCR_REQUESTED_FILE);
    }

    log("Starting recording...", &config.audio.log_file);
    recording::start_recording(&config.audio)?;
    log("Recording started successfully", &config.audio.log_file);

    log("Sending notification...", &config.audio.log_file);
    if let Err(e) = notifications::notify_recording_started() {
        log(&format!("WARNING: Notification failed: {}", e), &config.audio.log_file);
    }

    println!("🎤 Recording started...");
    log("=== HANDLE START COMPLETE ===", &config.audio.log_file);
    Ok(())
}

fn handle_stop(config: &Config) -> Result<(), Box<dyn Error>> {
    log("=== HANDLE STOP ===", &config.audio.log_file);

    // Save start timestamp for performance tracking (measures from stop command to paste)
    if let Err(e) = timing::save_start_time() {
        log(&format!("WARNING: Failed to save start timestamp: {}", e), &config.audio.log_file);
    }

    // Initialize performance metrics from saved start time
    let mut metrics = match timing::PerformanceMetrics::from_start_time() {
        Ok(m) => Some(m),
        Err(e) => {
            log(&format!("WARNING: Failed to load start timestamp: {}", e), &config.audio.log_file);
            None
        }
    };

    // Send notification immediately for instant user feedback
    log("Sending stop notification...", &config.audio.log_file);
    if let Err(e) = notifications::notify_recording_stopped() {
        log(&format!("WARNING: Notification failed: {}", e), &config.audio.log_file);
    }
    println!("⏹️  Recording stopped");

    // Stop recording (may take 0.6-1.6s depending on audio length)
    log("Stopping recording...", &config.audio.log_file);
    let audio_file = recording::stop_recording(&config.audio)?;
    log(&format!("Audio file: {:?}", audio_file), &config.audio.log_file);

    if let Some(ref mut m) = metrics {
        m.mark_recording_stop();
    }

    // Transcribe using daemon client
    println!("📝 Transcribing...");
    log("Calling transcribe_file...", &config.audio.log_file);
    let transcription = match transcribe_file(&audio_file, &config.audio.log_file) {
        Ok(t) => {
            log(&format!("Transcription received: {} chars", t.len()), &config.audio.log_file);
            t
        }
        Err(e) => {
            log(&format!("ERROR: Transcription failed: {}", e), &config.audio.log_file);

            // Check if this is a daemon connection error
            let error_msg = e.to_string();
            if error_msg.contains("Failed to connect") || error_msg.contains("daemon") || error_msg.contains("No such file or directory") {
                notifications::notify_error("Daemon not found\nStart with: transcribe-daemon").ok();
            } else {
                notifications::notify_error(&format!("Transcription failed: {}", e)).ok();
            }

            eprintln!("Transcription error: {}", e);
            return Err(e);
        }
    };

    if let Some(ref mut m) = metrics {
        m.mark_transcription_done();
    }

    if transcription.is_empty() {
        log("ERROR: Empty transcription", &config.audio.log_file);
        notifications::notify_error("No speech detected").ok();
        return Err("Empty transcription".into());
    }

    log(&format!("Transcription text: '{}'", transcription), &config.audio.log_file);

    // Apply transcription corrections (phonetic/acoustic fixes) if enabled
    // Note: This still runs in CLI. Harper processing now happens in daemon.
    let processed_text = if config.transcription_corrections.enabled {
        log("Applying transcription corrections...", &config.audio.log_file);
        use std::path::PathBuf;
        use transcribe_rs::transcription_corrections::TranscriptionCorrector;

        let corrections_file = PathBuf::from(&config.transcription_corrections.corrections_file);
        match TranscriptionCorrector::from_file(&corrections_file) {
            Ok(corrector) => {
                let corrected = corrector.correct(&transcription);
                if corrected != transcription {
                    log(&format!("Applied transcription corrections: '{}' → '{}'", transcription, corrected), &config.audio.log_file);
                } else {
                    log("No transcription corrections needed", &config.audio.log_file);
                }
                corrected
            }
            Err(e) => {
                log(&format!("WARNING: Failed to load transcription corrections: {}", e), &config.audio.log_file);
                transcription.clone()
            }
        }
    } else {
        transcription.clone()
    };

    // Harper processing now happens in the daemon (via transcribe-client)
    // This eliminates the 300ms dictionary loading overhead on each transcription

    // Check if OCR was requested and retrieve result
    const OCR_REQUESTED_FILE: &str = "/tmp/ptt_ocr_requested.flag";
    let ocr_context = if std::path::Path::new(OCR_REQUESTED_FILE).exists() {
        log("OCR was requested, waiting for result...", &config.audio.log_file);
        let _ = fs::remove_file(OCR_REQUESTED_FILE);

        match ocr::wait_for_ocr_result(&config.ocr, std::time::Duration::from_secs(5)) {
            Ok(Some(text)) => {
                log(&format!("OCR context retrieved: {} chars", text.len()), &config.audio.log_file);
                // Clean up OCR files
                ocr::cleanup_ocr_files(&config.ocr);
                Some(text)
            }
            Ok(None) => {
                log("No OCR result available (timeout or not found)", &config.audio.log_file);
                ocr::cleanup_ocr_files(&config.ocr);
                None
            }
            Err(e) => {
                log(&format!("OCR error: {}", e), &config.audio.log_file);
                ocr::cleanup_ocr_files(&config.ocr);
                None
            }
        }
    } else {
        None
    };

    // Check if prompt-based post-processing is requested
    const PROMPT_STATE_FILE: &str = "/tmp/ptt_prompt.txt";
    let mut llm_processing_triggered = false;
    let mut tool_was_called = false;
    let final_text = if let Ok(prompt_name) = fs::read_to_string(PROMPT_STATE_FILE) {
        let prompt_name = prompt_name.trim();
        if !prompt_name.is_empty() {
            llm_processing_triggered = true;
            log(&format!("Prompt requested: {}", prompt_name), &config.audio.log_file);

            // Check if this is a clear history command
            if is_clear_history_command(&processed_text, &config) {
                log(&format!("Clear history command detected for prompt '{}'", prompt_name), &config.audio.log_file);
                match clear_conversation_history(prompt_name, &config) {
                    Ok(_) => {
                        log("Conversation history cleared", &config.audio.log_file);
                        notifications::notify("🗑️ History Cleared", &format!("Conversation history for '{}' has been reset", prompt_name), 2000).ok();
                        "".to_string()  // Return empty string so nothing gets pasted
                    }
                    Err(e) => {
                        log(&format!("ERROR: Failed to clear history: {}", e), &config.audio.log_file);
                        notifications::notify_error(&format!("Failed to clear history: {}", e)).ok();
                        processed_text.clone()
                    }
                }
            } else {
                // Try to process with Groq API
                match process_with_groq(&processed_text, prompt_name, ocr_context.as_deref(), &config.audio.log_file) {
                Ok(result) => {
                    log(&format!("Groq processing successful: '{}'", result.text), &config.audio.log_file);
                    tool_was_called = result.tool_called;
                    if tool_was_called {
                        log("Tool was called - will skip paste and show notification only", &config.audio.log_file);
                    }
                    result.text
                }
                Err(e) => {
                    log(&format!("ERROR: Groq processing failed: {}", e), &config.audio.log_file);
                    notifications::notify_error(&format!("Groq API failed: {}\nPasted raw transcription.", e)).ok();
                    processed_text.clone()
                }
                }
            }
        } else {
            processed_text.clone()
        }
    } else {
        processed_text.clone()
    };

    if let Some(ref mut m) = metrics {
        m.mark_processing_done();
    }

    // Add space after punctuation (do this even if tool was called, for logging)

    let text = if config.integration.add_space_after_punctuation {
        log("Adding trailing space after punctuation...", &config.audio.log_file);
        clipboard::add_trailing_space_after_punctuation(&final_text)
    } else {
        final_text
    };
    log(&format!("Final text: '{}'", text), &config.audio.log_file);

    // If a tool was called, just show notification and skip paste
    if tool_was_called {
        log("Tool called - skipping clipboard and paste, showing notification only", &config.audio.log_file);
        notifications::notify("✅ Tool executed", &text, 3000).ok();

        if let Some(ref mut m) = metrics {
            m.mark_clipboard_done();
            m.mark_paste_done();
        }
    } else {
        // Normal flow: copy to clipboard and paste

        // Copy to clipboard
        log("Copying to clipboard...", &config.audio.log_file);
        match clipboard::copy_to_clipboard(&text) {
            Ok(_) => {
                log("Clipboard copy successful", &config.audio.log_file);
                if let Some(ref mut m) = metrics {
                    m.mark_clipboard_done();
                }
            }
            Err(e) => {
                log(&format!("ERROR: Clipboard copy failed: {}", e), &config.audio.log_file);
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
        log(&format!("Preview: '{}'", preview), &config.audio.log_file);

        // Auto-paste if enabled
        if config.integration.auto_paste {
            log("Attempting auto-paste...", &config.audio.log_file);
            match paste::paste_from_clipboard() {
                Ok(_) => {
                    log("Paste successful", &config.audio.log_file);
                    if let Some(ref mut m) = metrics {
                        m.mark_paste_done();
                    }
                    notifications::notify_transcription_pasted(&preview).ok();
                }
                Err(e) => {
                    log(&format!("WARNING: Paste failed: {}, clipboard only", e), &config.audio.log_file);
                    notifications::notify_transcription_copied(&preview).ok();
                }
            }
        } else {
        log("Auto-paste disabled in config, clipboard only", &config.audio.log_file);
        // Mark paste as done even if disabled, to track total workflow time
        if let Some(ref mut m) = metrics {
            m.mark_paste_done();
        }
        notifications::notify_transcription_copied(&preview).ok();
        }
    }

    // Log performance metrics
    if let Some(m) = metrics {
        if let Err(e) = performance_log::log_performance_detailed(&m, &text) {
            log(&format!("WARNING: Failed to log performance: {}", e), &config.audio.log_file);
        } else {
            log(&format!("Performance logged: {:.3}s total", m.total_duration_seconds()), &config.audio.log_file);
        }

        // Log dictation if enabled
        if config.dictation_logging.enabled {
            let duration = m.total_duration_seconds();

            if llm_processing_triggered && config.dictation_logging.llm_log_enabled {
                // Only log if LLM actually changed the text (after trimming whitespace)
                if processed_text.trim() != text.trim() {
                    // Generate diff showing what changed
                    let diff = dictation_logger::generate_diff(&processed_text, &text);
                    log(&format!("LLM diff: {}", diff), &config.audio.log_file);

                    // Log to LLM corrections log
                    if let Err(e) = dictation_logger::log_llm_correction(
                        &processed_text,
                        &text,
                        &diff,
                        duration,
                        &config.dictation_logging.llm_log_path,
                    ) {
                        log(&format!("WARNING: Failed to log LLM correction: {}", e), &config.audio.log_file);
                    } else {
                        log("Logged to LLM corrections log", &config.audio.log_file);
                    }
                } else {
                    log("LLM processing triggered but no actual changes made (skipped logging)", &config.audio.log_file);
                }
            } else if !llm_processing_triggered && config.dictation_logging.basic_log_enabled {
                // Log to basic dictation log
                if let Err(e) = dictation_logger::log_basic_dictation(
                    &text,
                    duration,
                    &config.dictation_logging.basic_log_path,
                ) {
                    log(&format!("WARNING: Failed to log basic dictation: {}", e), &config.audio.log_file);
                } else {
                    log("Logged to basic dictation log", &config.audio.log_file);
                }
            }
        }

        // Clean up timestamp file
        timing::cleanup_timestamp_file();
    }

    println!("Transcription: {}", text);
    log("=== HANDLE STOP COMPLETE ===", &config.audio.log_file);
    Ok(())
}

/// Handle config subcommands
fn handle_config(config_cmd: ConfigCommands) -> Result<(), Box<dyn Error>> {
    match config_cmd {
        ConfigCommands::Show => {
            let config = Config::load()?;
            let config_toml = toml::to_string_pretty(&config)?;
            println!("{}", config_toml);
            Ok(())
        }
        ConfigCommands::Init => {
            let config_path = Config::config_path()?;
            if config_path.exists() {
                println!("⚠️  Config file already exists at: {}", config_path.display());
                println!("Use 'transcribe config show' to view current config");
            } else {
                let config = Config::default();
                config.save()?;
                println!("✅ Created default config at: {}", config_path.display());
                println!("\nEdit with:");
                println!("  $EDITOR {}", config_path.display());
            }
            Ok(())
        }
        ConfigCommands::Path => {
            let config_path = Config::config_path()?;
            println!("{}", config_path.display());
            Ok(())
        }
    }
}

/// Check system dependencies and configuration
fn handle_doctor() -> Result<(), Box<dyn Error>> {
    println!("🔍 Checking transcribe-rs system health...\n");

    let mut all_ok = true;

    // Check dependencies
    println!("📦 Required Dependencies:");
    all_ok &= check_command("ffmpeg", "Audio recording");
    all_ok &= check_command("wl-copy", "Clipboard management");
    all_ok &= check_command("ydotool", "Auto-paste functionality");
    all_ok &= check_command("pactl", "Microphone detection");

    println!();

    // Check OCR dependencies (optional)
    println!("📷 OCR Dependencies (optional, for --ocr flag):");
    let tesseract_ok = check_command("tesseract", "OCR text extraction");
    let grim_ok = check_command("grim", "Screenshot capture");
    if !tesseract_ok || !grim_ok {
        println!("  ℹ️  OCR features require both tesseract and grim");
        println!("     Install with: sudo pacman -S tesseract tesseract-data-eng grim");
    }

    println!();

    // Check configuration
    println!("⚙️  Configuration:");
    let config_path = Config::config_path()?;
    if config_path.exists() {
        println!("  ✅ Config file exists: {}", config_path.display());

        match Config::load() {
            Ok(config) => {
                println!("  ✅ Config file is valid");

                // Check model path
                let model_path = std::path::Path::new(&config.model.path);
                if model_path.exists() {
                    println!("  ✅ Model exists: {}", config.model.path);
                } else {
                    println!("  ❌ Model not found: {}", config.model.path);
                    println!("     Download with:");
                    println!("       mkdir -p models && cd models");
                    println!("       wget https://blob.handy.computer/parakeet-v3-int8.tar.gz");
                    println!("       tar -xzf parakeet-v3-int8.tar.gz");
                    all_ok = false;
                }

                // Check microphone
                if config.audio.microphone != "default" {
                    println!("  ℹ️  Using specific microphone: {}", config.audio.microphone);
                    println!("     Verify with: pactl list sources short");
                }
            }
            Err(e) => {
                println!("  ❌ Config file is invalid: {}", e);
                all_ok = false;
            }
        }
    } else {
        println!("  ❌ Config file not found: {}", config_path.display());
        println!("     Create with: transcribe config init");
        all_ok = false;
    }

    println!();

    // Check daemon
    println!("🔧 Daemon Status:");
    match Config::load() {
        Ok(config) => {
            let socket_path = &config.daemon.socket_path;
            if std::path::Path::new(socket_path).exists() {
                // Try to connect to daemon
                match std::os::unix::net::UnixStream::connect(socket_path) {
                    Ok(_) => {
                        println!("  ✅ Daemon is running");
                    }
                    Err(_) => {
                        println!("  ⚠️  Socket exists but daemon not responding");
                        println!("     Remove stale socket: rm {}", socket_path);
                        println!("     Start daemon: transcribe-daemon");
                    }
                }
            } else {
                println!("  ⚠️  Daemon is not running");
                println!("     Run: systemctl --user start transcribe-daemon");
            }
        }
        Err(_) => {
            println!("  ⚠️  Cannot check daemon (config not loaded)");
        }
    }

    println!();

    // Check microphone sources
    println!("🎤 Available Microphones:");
    match Command::new("pactl").args(["list", "sources", "short"]).output() {
        Ok(output) => {
            if output.status.success() {
                let sources = String::from_utf8_lossy(&output.stdout);
                let mic_lines: Vec<&str> = sources.lines()
                    .filter(|line| line.contains("input"))
                    .collect();

                if mic_lines.is_empty() {
                    println!("  ⚠️  No input sources found");
                } else {
                    for line in mic_lines {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            println!("  • {} (index: {})", parts[1], parts[0]);
                        }
                    }
                }
            }
        }
        Err(_) => {
            println!("  ⚠️  Could not list microphones (pactl failed)");
        }
    }

    println!();

    // Summary
    if all_ok {
        println!("✅ All checks passed! System is ready to use.");
        println!("\nQuick start:");
        println!("  1. Start daemon: transcribe-daemon");
        println!("  2. Set up keybinding in Hyprland config");
        println!("  3. Press hotkey, speak, release");
    } else {
        println!("❌ Some issues found. Please fix them before using transcribe-rs.");
        println!("\nFor help, see: https://github.com/YOUR_USERNAME/transcribe-rs-v2#troubleshooting");
    }

    Ok(())
}

/// Check if a command exists in PATH
fn check_command(cmd: &str, purpose: &str) -> bool {
    match Command::new("which").arg(cmd).output() {
        Ok(output) if output.status.success() => {
            println!("  ✅ {} ({})", cmd, purpose);
            true
        }
        _ => {
            println!("  ❌ {} ({}) - NOT FOUND", cmd, purpose);
            println!("     Install with: sudo pacman -S {}",
                match cmd {
                    "ffmpeg" => "ffmpeg",
                    "wl-copy" => "wl-clipboard",
                    "ydotool" => "ydotool",
                    "pactl" => "pulseaudio",
                    _ => cmd
                }
            );
            false
        }
    }
}

/// Process transcribed text with Groq API using specified prompt
fn process_with_groq(text: &str, prompt_name: &str, ocr_context: Option<&str>, log_file: &str) -> Result<transcribe_rs::groq::CompletionResult, Box<dyn Error>> {
    use transcribe_rs::{groq, prompts};

    log(&format!("Loading prompt: {}", prompt_name), log_file);
    let prompt = prompts::load_prompt(prompt_name)?;

    log("Loading Groq API key from .env", log_file);
    let client = groq::GroqClient::from_env_file()?;

    log("Sending request to Groq API...", log_file);
    let result = client.complete_with_context(&prompt, text, prompt_name, ocr_context)?;

    Ok(result)
}

/// Call the transcribe-client binary to transcribe a file
fn transcribe_file(file: &PathBuf, log_file: &str) -> Result<String, Box<dyn Error>> {
    // Try to find transcribe-client in the same directory as this binary
    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe.parent().ok_or("Cannot get exe directory")?;
    let client_path = exe_dir.join("transcribe-client");

    log(&format!("Looking for transcribe-client at: {:?}", client_path), log_file);

    if !client_path.exists() {
        log(&format!("ERROR: transcribe-client not found at {:?}", client_path), log_file);
        return Err(format!(
            "transcribe-client binary not found at {:?}. Make sure the daemon is running.",
            client_path
        ).into());
    }

    log(&format!("Calling transcribe-client with file: {:?}", file), log_file);
    let output = Command::new(&client_path)
        .arg(file)
        .output()
        .map_err(|e| {
            log(&format!("ERROR: Failed to execute transcribe-client: {}", e), log_file);
            e
        })?;

    log(&format!("transcribe-client exit status: {:?}", output.status), log_file);

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        log(&format!("transcribe-client stderr: {}", error), log_file);
        return Err(format!("Transcription failed: {}", error).into());
    }

    let text = String::from_utf8(output.stdout)?;
    log(&format!("transcribe-client stdout: '{}'", text), log_file);
    Ok(text.trim().to_string())
}

/// Check if the transcribed text matches the clear history command
/// Normalized: lowercase, remove all punctuation and extra spaces
fn is_clear_history_command(text: &str, config: &transcribe_rs::config::Config) -> bool {
    let clear_word = &config.llm.conversation_history_clear_word;

    // Normalize both strings: lowercase, remove punctuation, trim spaces
    let normalize = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };

    let normalized_text = normalize(text);
    let normalized_clear_word = normalize(clear_word);

    normalized_text == normalized_clear_word
}

/// Clear the conversation history for a specific prompt
fn clear_conversation_history(prompt_name: &str, config: &transcribe_rs::config::Config) -> Result<(), Box<dyn Error>> {
    use transcribe_rs::conversation_history::ConversationHistory;

    let history = ConversationHistory::new(
        prompt_name,
        config.llm.conversation_history_minutes,
        config.llm.conversation_max_turns,
        &config.llm.conversation_history_dir,
    );

    history.clear()?;
    Ok(())
}
