use clap::{Parser, Subcommand};
use log::{debug, error, info, warn};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use transcribe_rs::{
    clipboard,
    config::Config,
    dictation_logger, eww_widget, file_chat,
    gui::{
        is_window_running, recover_orphaned_conversation, ConversationState, ConversationWindow,
    },
    logging, notifications, ocr, paste, performance_log, recording, timing,
};

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
        /// Write Q&A to markdown file instead of clipboard (requires --prompt)
        #[arg(long)]
        file_chat: bool,
        /// Open GUI conversation window for back-and-forth chat
        #[arg(long)]
        gui: bool,
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
    /// Internal: GUI conversation window process (do not call directly)
    #[command(hide = true)]
    GuiWindow,
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

/// Handle the GUI window process (runs the window with file watching)
fn handle_gui_window() -> Result<(), Box<dyn Error>> {
    // This function runs in a separate process, launched by process_gui_conversation
    // It blocks until the window is closed

    eprintln!("[GUI-WINDOW] Starting GUI window process...");

    // Load the conversation state
    let state = if ConversationState::exists() {
        eprintln!("[GUI-WINDOW] Loading conversation state...");
        match ConversationState::load() {
            Ok(s) => {
                eprintln!(
                    "[GUI-WINDOW] Loaded state with {} messages",
                    s.messages.len()
                );
                s
            }
            Err(e) => {
                eprintln!("[GUI-WINDOW] ERROR: Failed to load state: {}", e);
                return Err(e);
            }
        }
    } else {
        eprintln!("[GUI-WINDOW] ERROR: Conversation state file not found");
        return Err("Conversation state file not found".into());
    };

    // Run the window (blocking)
    eprintln!("[GUI-WINDOW] Running window...");
    match ConversationWindow::run(state) {
        Ok(_) => {
            eprintln!("[GUI-WINDOW] Window closed normally");
            Ok(())
        }
        Err(e) => {
            eprintln!("[GUI-WINDOW] ERROR: Window failed: {}", e);
            Err(e)
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging (loads from config file or uses defaults)
    if let Err(e) = logging::init() {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    }

    let cli = Cli::parse();

    match cli.command {
        Commands::Start {
            prompt,
            ocr,
            file_chat,
            gui,
        } => {
            let config = Config::load()?;
            handle_start(&config, prompt, ocr, file_chat, gui)
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
        Commands::OcrWorker {
            screenshot_path,
            result_path,
            language,
            dpi,
        } => {
            // Internal OCR worker process
            let dpi_value: u32 = dpi.parse().unwrap_or(300);
            ocr::run_ocr_worker(&screenshot_path, &result_path, &language, dpi_value)
        }
        Commands::GuiWindow => {
            // Internal GUI window process - runs the conversation window with file watching
            handle_gui_window()
        }
    }
}

fn handle_start(
    config: &Config,
    prompt: Option<String>,
    ocr_enabled: bool,
    file_chat: bool,
    gui_mode: bool,
) -> Result<(), Box<dyn Error>> {
    info!("=== HANDLE START ===");

    // Save prompt name to temp file for stop command
    const PROMPT_STATE_FILE: &str = "/tmp/ptt_prompt.txt";
    const FILE_CHAT_FLAG_FILE: &str = "/tmp/ptt_file_chat.flag";
    const GUI_MODE_FLAG_FILE: &str = "/tmp/ptt_gui_mode.flag";

    // Handle prompt state
    let prompt_was_set = if let Some(ref prompt_name) = prompt {
        debug!("Saving prompt name: {}", prompt_name);
        fs::write(PROMPT_STATE_FILE, prompt_name)?;
        true
    } else {
        // Remove prompt file if no prompt specified
        let _ = fs::remove_file(PROMPT_STATE_FILE);
        false
    };

    // Handle OCR if enabled - spawn async worker
    const OCR_REQUESTED_FILE: &str = "/tmp/ptt_ocr_requested.flag";
    if ocr_enabled {
        debug!("OCR requested, spawning async worker...");
        match ocr::spawn_ocr_worker(&config.ocr) {
            Ok(_) => {
                debug!("OCR worker spawned successfully");
                fs::write(OCR_REQUESTED_FILE, "")?;
            }
            Err(e) => {
                warn!("OCR spawn failed: {}", e);
                // Continue without OCR - don't fail the whole operation
                let _ = fs::remove_file(OCR_REQUESTED_FILE);
            }
        }
    } else {
        // Remove OCR flag if not requested
        let _ = fs::remove_file(OCR_REQUESTED_FILE);
    }

    // Save file-chat flag
    if file_chat {
        debug!("File chat mode enabled");
        fs::write(FILE_CHAT_FLAG_FILE, "1")?;
    } else {
        let _ = fs::remove_file(FILE_CHAT_FLAG_FILE);
    }

    // Save GUI mode flag
    if gui_mode {
        debug!("GUI conversation mode enabled");

        // Check for and recover any orphaned conversations from previous crashes
        if let Err(e) = recover_orphaned_conversation() {
            warn!("Failed to recover orphaned conversation: {}", e);
        }

        fs::write(GUI_MODE_FLAG_FILE, "1")?;

        // GUI mode requires LLM processing, auto-set prompt if not specified
        if !prompt_was_set {
            debug!("GUI mode: auto-setting default prompt 'chat'");
            fs::write(PROMPT_STATE_FILE, "chat")?;
        }
    } else {
        let _ = fs::remove_file(GUI_MODE_FLAG_FILE);
    }

    debug!("Starting recording...");
    recording::start_recording(&config.audio)?;
    debug!("Recording started successfully");

    debug!("Sending notification...");
    if let Err(e) = notifications::notify_recording_started() {
        warn!("Notification failed: {}", e);
    }

    // Show eww recording indicator widget
    eww_widget::show_recording_widget();

    println!("🎤 Recording started...");
    info!("=== HANDLE START COMPLETE ===");
    Ok(())
}

fn handle_stop(config: &Config) -> Result<(), Box<dyn Error>> {
    info!("=== HANDLE STOP ===");

    // Hide eww recording indicator widget immediately
    eww_widget::hide_recording_widget();

    // Save start timestamp for performance tracking (measures from stop command to paste)
    if let Err(e) = timing::save_start_time() {
        warn!("Failed to save start timestamp: {}", e);
    }

    // Initialize performance metrics from saved start time
    let mut metrics = match timing::PerformanceMetrics::from_start_time() {
        Ok(m) => Some(m),
        Err(e) => {
            warn!("Failed to load start timestamp: {}", e);
            None
        }
    };

    // Send notification immediately for instant user feedback
    debug!("Sending stop notification...");
    if let Err(e) = notifications::notify_recording_stopped() {
        warn!("Notification failed: {}", e);
    }
    println!("⏹️  Recording stopped");

    // Stop recording (may take 0.6-1.6s depending on audio length)
    debug!("Stopping recording...");
    let recording_result = recording::stop_recording(&config.audio)?;
    let audio_file = &recording_result.audio_file;
    debug!(
        "Audio file: {:?}, duration: {}ms",
        audio_file, recording_result.duration_ms
    );

    if let Some(ref mut m) = metrics {
        m.mark_recording_stop();
    }

    // Transcribe using daemon client
    println!("📝 Transcribing...");
    debug!("Calling transcribe_file...");
    let transcription = match transcribe_file(audio_file) {
        Ok(t) => {
            debug!("Transcription received: {} chars", t.len());
            t
        }
        Err(e) => {
            error!("Transcription failed: {}", e);

            // Check if this is a daemon connection error
            let error_msg = e.to_string();
            if error_msg.contains("Failed to connect")
                || error_msg.contains("daemon")
                || error_msg.contains("No such file or directory")
            {
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
        error!("Empty transcription");
        notifications::notify_error("No speech detected").ok();
        return Err("Empty transcription".into());
    }

    debug!("Transcription text: '{}'", transcription);

    // Apply transcription corrections (phonetic/acoustic fixes) if enabled
    // Note: This still runs in CLI. Harper processing now happens in daemon.
    let processed_text = if config.transcription_corrections.enabled {
        debug!("Applying transcription corrections...");
        use std::path::PathBuf;
        use transcribe_rs::transcription_corrections::TranscriptionCorrector;

        let corrections_file = PathBuf::from(&config.transcription_corrections.corrections_file);
        match TranscriptionCorrector::from_file(&corrections_file) {
            Ok(corrector) => {
                let corrected = corrector.correct(&transcription);
                if corrected != transcription {
                    debug!(
                        "Applied transcription corrections: '{}' → '{}'",
                        transcription, corrected
                    );
                } else {
                    debug!("No transcription corrections needed");
                }
                corrected
            }
            Err(e) => {
                warn!("Failed to load transcription corrections: {}", e);
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
        debug!("OCR was requested, waiting for result...");
        let _ = fs::remove_file(OCR_REQUESTED_FILE);

        match ocr::wait_for_ocr_result(&config.ocr, std::time::Duration::from_secs(5)) {
            Ok(Some(text)) => {
                debug!("OCR context retrieved: {} chars", text.len());
                // Clean up OCR files
                ocr::cleanup_ocr_files(&config.ocr);
                Some(text)
            }
            Ok(None) => {
                debug!("No OCR result available (timeout or not found)");
                ocr::cleanup_ocr_files(&config.ocr);
                None
            }
            Err(e) => {
                error!("OCR error: {}", e);
                ocr::cleanup_ocr_files(&config.ocr);
                None
            }
        }
    } else {
        None
    };

    // Check for GUI conversation mode FIRST (before other LLM processing)
    const GUI_MODE_FLAG_FILE: &str = "/tmp/ptt_gui_mode.flag";
    const PROMPT_STATE_FILE: &str = "/tmp/ptt_prompt.txt";
    const FILE_CHAT_FLAG_FILE: &str = "/tmp/ptt_file_chat.flag";

    // Check for and recover any orphaned conversations (e.g., from window crashes)
    if let Err(e) = recover_orphaned_conversation() {
        warn!("Failed to recover orphaned conversation: {}", e);
    }

    let gui_mode = std::path::Path::new(GUI_MODE_FLAG_FILE).exists();
    if gui_mode {
        debug!("GUI conversation mode detected");
        let _ = fs::remove_file(GUI_MODE_FLAG_FILE);

        // Get prompt name for LLM processing
        let prompt_name = if let Ok(name) = fs::read_to_string(PROMPT_STATE_FILE) {
            let _ = fs::remove_file(PROMPT_STATE_FILE);
            name.trim().to_string()
        } else {
            "chat".to_string()
        };

        // Process GUI conversation with context
        match process_gui_conversation(&processed_text, &prompt_name, ocr_context.as_deref()) {
            Ok(_) => {
                debug!("GUI conversation processed successfully");
            }
            Err(e) => {
                error!("GUI conversation failed: {}", e);
                notifications::notify_error(&format!("GUI conversation failed: {}", e)).ok();
            }
        }

        // Skip clipboard/paste/file-chat for GUI mode
        if let Some(ref mut m) = metrics {
            m.mark_processing_done();
            m.mark_clipboard_done();
            m.mark_paste_done();
        }

        info!("=== HANDLE STOP COMPLETE (GUI MODE) ===");
        return Ok(());
    }

    // Normal (non-GUI) flow continues below
    let mut llm_processing_triggered = false;
    let mut tool_was_called = false;
    let file_chat_mode = std::path::Path::new(FILE_CHAT_FLAG_FILE).exists();
    if file_chat_mode {
        debug!("File chat mode detected");
        // Clean up flag file
        let _ = fs::remove_file(FILE_CHAT_FLAG_FILE);
    }
    let final_text = if let Ok(prompt_name) = fs::read_to_string(PROMPT_STATE_FILE) {
        let prompt_name = prompt_name.trim();
        if !prompt_name.is_empty() {
            llm_processing_triggered = true;
            debug!("Prompt requested: {}", prompt_name);

            // Check if this is a clear history command
            if is_clear_history_command(&processed_text, config) {
                debug!(
                    "Clear history command detected for prompt '{}'",
                    prompt_name
                );
                match clear_conversation_history(prompt_name, config) {
                    Ok(_) => {
                        info!("Conversation history cleared");
                        notifications::notify(
                            "🗑️ History Cleared",
                            &format!("Conversation history for '{}' has been reset", prompt_name),
                            2000,
                        )
                        .ok();
                        "".to_string() // Return empty string so nothing gets pasted
                    }
                    Err(e) => {
                        error!("Failed to clear history: {}", e);
                        notifications::notify_error(&format!("Failed to clear history: {}", e))
                            .ok();
                        processed_text.clone()
                    }
                }
            } else {
                // Try to process with Groq API
                match process_with_groq(&processed_text, prompt_name, ocr_context.as_deref()) {
                    Ok(result) => {
                        debug!("Groq processing successful: '{}'", result.text);
                        tool_was_called = result.tool_called;
                        if tool_was_called {
                            debug!("Tool was called - will skip paste and show notification only");
                        }
                        result.text
                    }
                    Err(e) => {
                        error!("Groq processing failed: {}", e);
                        notifications::notify_error(&format!(
                            "Groq API failed: {}\nPasted raw transcription.",
                            e
                        ))
                        .ok();
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
        debug!("Adding trailing space after punctuation...");
        clipboard::add_trailing_space_after_punctuation(&final_text)
    } else {
        final_text
    };
    debug!("Final text: '{}'", text);

    // If file-chat mode, write to markdown file instead of clipboard
    if file_chat_mode {
        debug!("File chat mode - writing to markdown file");

        // Check if file chat is enabled in config
        if !config.llm.file_chat_enabled {
            warn!("File chat is disabled in config");
            notifications::notify_error("File chat is disabled in config").ok();
        } else {
            match file_chat::append_to_chat_file(
                &config.llm.file_chat_dir,
                &processed_text, // User's question
                &text,           // LLM's response
            ) {
                Ok(_) => {
                    debug!("Successfully wrote to chat file");
                    let preview = if text.len() > 50 {
                        format!("{}...", &text[..50])
                    } else {
                        text.clone()
                    };
                    notifications::notify("📝 Chat saved", &preview, 3000).ok();
                }
                Err(e) => {
                    error!("Failed to write chat file: {}", e);
                    notifications::notify_error(&format!("Failed to write chat: {}", e)).ok();
                }
            }
        }

        if let Some(ref mut m) = metrics {
            m.mark_clipboard_done();
            m.mark_paste_done();
        }
    } else if tool_was_called {
        // If a tool was called, just show notification and skip paste
        debug!("Tool called - skipping clipboard and paste, showing notification only");
        notifications::notify("✅ Tool executed", &text, 3000).ok();

        if let Some(ref mut m) = metrics {
            m.mark_clipboard_done();
            m.mark_paste_done();
        }
    } else {
        // Normal flow: copy to clipboard, paste, and restore original clipboard

        // Save original clipboard content first
        debug!("Saving original clipboard content...");
        let saved_clipboard = clipboard::SavedClipboard::save();
        debug!(
            "Clipboard saved (has_content: {})",
            saved_clipboard.has_content()
        );

        // Copy transcription to clipboard
        debug!("Copying to clipboard...");
        match clipboard::copy_to_clipboard(&text) {
            Ok(_) => {
                debug!("Clipboard copy successful");
                if let Some(ref mut m) = metrics {
                    m.mark_clipboard_done();
                }
            }
            Err(e) => {
                error!("Clipboard copy failed: {}", e);
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
        debug!("Preview: '{}'", preview);

        // Auto-paste if enabled
        if config.integration.auto_paste {
            debug!("Attempting auto-paste...");
            match paste::paste_from_clipboard() {
                Ok(_) => {
                    debug!("Paste successful");
                    if let Some(ref mut m) = metrics {
                        m.mark_paste_done();
                    }
                    notifications::notify_transcription_pasted(&preview).ok();

                    // Restore original clipboard content after successful paste
                    debug!("Restoring original clipboard content...");
                    if let Err(e) = saved_clipboard.restore() {
                        warn!("Failed to restore clipboard: {}", e);
                    } else {
                        debug!("Clipboard restored successfully");
                    }
                }
                Err(e) => {
                    warn!("Paste failed: {}, clipboard only", e);
                    notifications::notify_transcription_copied(&preview).ok();
                    // Don't restore clipboard if paste failed - user may want to manually paste
                }
            }
        } else {
            debug!("Auto-paste disabled in config, clipboard only");
            // Mark paste as done even if disabled, to track total workflow time
            if let Some(ref mut m) = metrics {
                m.mark_paste_done();
            }
            notifications::notify_transcription_copied(&preview).ok();
            // Don't restore clipboard when auto-paste disabled - user needs clipboard content
        }
    }

    // Log performance metrics
    if let Some(m) = metrics {
        if let Err(e) = performance_log::log_performance_detailed(&m, &text) {
            warn!("Failed to log performance: {}", e);
        } else {
            debug!(
                "Performance logged: {:.3}s total",
                m.total_duration_seconds()
            );
        }

        // Log dictation if enabled
        if config.dictation_logging.enabled {
            let duration = m.total_duration_seconds();

            if llm_processing_triggered && config.dictation_logging.llm_log_enabled {
                // Only log if LLM actually changed the text (after trimming whitespace)
                if processed_text.trim() != text.trim() {
                    // Generate diff showing what changed
                    let diff = dictation_logger::generate_diff(&processed_text, &text);
                    debug!("LLM diff: {}", diff);

                    // Log to LLM corrections log
                    if let Err(e) = dictation_logger::log_llm_correction(
                        &processed_text,
                        &text,
                        &diff,
                        duration,
                        &config.dictation_logging.llm_log_path,
                    ) {
                        warn!("Failed to log LLM correction: {}", e);
                    } else {
                        debug!("Logged to LLM corrections log");
                    }
                } else {
                    debug!("LLM processing triggered but no actual changes made (skipped logging)");
                }
            } else if !llm_processing_triggered && config.dictation_logging.basic_log_enabled {
                // Log to basic dictation log
                if let Err(e) = dictation_logger::log_basic_dictation(
                    &text,
                    duration,
                    &config.dictation_logging.basic_log_path,
                ) {
                    warn!("Failed to log basic dictation: {}", e);
                } else {
                    debug!("Logged to basic dictation log");
                }
            }
        }

        // Clean up timestamp file
        timing::cleanup_timestamp_file();
    }

    println!("Transcription: {}", text);
    info!("=== HANDLE STOP COMPLETE ===");
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
                println!(
                    "⚠️  Config file already exists at: {}",
                    config_path.display()
                );
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
                    println!(
                        "  ℹ️  Using specific microphone: {}",
                        config.audio.microphone
                    );
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
    match Command::new("pactl")
        .args(["list", "sources", "short"])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                let sources = String::from_utf8_lossy(&output.stdout);
                let mic_lines: Vec<&str> = sources
                    .lines()
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
        println!(
            "\nFor help, see: https://github.com/YOUR_USERNAME/transcribe-rs-v2#troubleshooting"
        );
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
            println!(
                "     Install with: sudo pacman -S {}",
                match cmd {
                    "ffmpeg" => "ffmpeg",
                    "wl-copy" => "wl-clipboard",
                    "ydotool" => "ydotool",
                    "pactl" => "pulseaudio",
                    _ => cmd,
                }
            );
            false
        }
    }
}

/// Process transcribed text with Groq API using specified prompt
fn process_with_groq(
    text: &str,
    prompt_name: &str,
    ocr_context: Option<&str>,
) -> Result<transcribe_rs::groq::CompletionResult, Box<dyn Error>> {
    use transcribe_rs::{config::Config, groq, prompts};

    debug!("Loading prompt: {}", prompt_name);
    let prompt = prompts::load_prompt(prompt_name)?;

    // Load config to determine which tools to use for this prompt
    let config = Config::load()?;

    // Look up the tool set for this prompt
    let client = if let Some(tool_set_name) = config.llm.prompt_tool_mapping.get(prompt_name) {
        debug!(
            "Prompt '{}' mapped to tool set '{}'",
            prompt_name, tool_set_name
        );

        // Look up the tool names for this tool set
        if let Some(tool_names) = config.llm.tool_sets.get(tool_set_name) {
            if tool_names.is_empty() {
                debug!(
                    "Tool set '{}' is empty, creating client with no tools",
                    tool_set_name
                );
                groq::GroqClient::from_env_file_no_tools()?
            } else {
                debug!(
                    "Tool set '{}' contains {} tools, loading them",
                    tool_set_name,
                    tool_names.len()
                );
                groq::GroqClient::from_env_file_with_tool_set(tool_names.clone())?
            }
        } else {
            warn!(
                "Tool set '{}' not found in config, using no tools",
                tool_set_name
            );
            groq::GroqClient::from_env_file_no_tools()?
        }
    } else {
        debug!(
            "Prompt '{}' not in tool mapping, using no tools",
            prompt_name
        );
        groq::GroqClient::from_env_file_no_tools()?
    };

    debug!("Sending request to Groq API...");
    let result = client.complete_with_context(&prompt, text, prompt_name, ocr_context)?;

    Ok(result)
}

/// Process a GUI conversation with full context
fn process_gui_conversation(
    user_text: &str,
    prompt_name: &str,
    ocr_context: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    use transcribe_rs::{config::Config, groq, prompts};

    debug!("Processing GUI conversation with context");

    // Load or create conversation state
    let mut state = if ConversationState::exists() {
        debug!("Loading existing GUI conversation state");
        ConversationState::load()?
    } else {
        debug!("Creating new GUI conversation state");
        // Create new conversation with timestamped filename
        let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?
            .join("transcribe-rs")
            .join("conversations");
        fs::create_dir_all(&config_dir)?;
        let conversation_file = config_dir.join(format!("conversation_{}.md", timestamp));
        debug!("New conversation file: {:?}", conversation_file);
        ConversationState::with_prompt(conversation_file, prompt_name.to_string())
    };

    debug!("Loaded conversation with {} messages", state.messages.len());

    // Get conversation context for LLM
    let conversation_context = state.get_context_for_llm();
    debug!(
        "Conversation context: {} message pairs",
        conversation_context.len()
    );

    // Load prompt
    debug!("Loading prompt: {}", prompt_name);
    let prompt = prompts::load_prompt(prompt_name)?;

    // Load config to determine which tools to use for this prompt
    let config = Config::load()?;

    // Look up the tool set for this prompt
    let client = if let Some(tool_set_name) = config.llm.prompt_tool_mapping.get(prompt_name) {
        debug!(
            "Prompt '{}' mapped to tool set '{}'",
            prompt_name, tool_set_name
        );

        // Look up the tool names for this tool set
        if let Some(tool_names) = config.llm.tool_sets.get(tool_set_name) {
            if tool_names.is_empty() {
                debug!("Tool set is empty, creating client with no tools");
                groq::GroqClient::from_env_file_no_tools()?
            } else {
                debug!("Tool set contains {} tools, loading them", tool_names.len());
                groq::GroqClient::from_env_file_with_tool_set(tool_names.clone())?
            }
        } else {
            warn!("Tool set '{}' not found, using no tools", tool_set_name);
            groq::GroqClient::from_env_file_no_tools()?
        }
    } else {
        debug!(
            "Prompt '{}' not in tool mapping, using no tools",
            prompt_name
        );
        groq::GroqClient::from_env_file_no_tools()?
    };

    // Call Groq with conversation context using a special method
    debug!("Calling Groq API with GUI conversation context...");
    let result =
        client.complete_with_history(&prompt, user_text, &conversation_context, ocr_context)?;
    debug!("LLM response: '{}'", result.text);

    // Add new messages to state
    state.add_message("user", user_text.to_string());
    state.add_message("assistant", result.text.clone());

    // Save state
    state.window_open = true;
    state.save()?;
    debug!("Saved updated conversation state");

    // Save to markdown file
    state.save_to_markdown()?;
    debug!("Saved conversation to markdown");

    // Check if GUI window process is already running (using PID file)
    let window_running = is_window_running();

    if window_running {
        debug!("GUI window already running, state file updated (window will auto-reload)");
    } else {
        debug!("Spawning GUI window process...");

        // Get current executable path
        let current_exe = std::env::current_exe()?;

        // Spawn window process (non-blocking)
        Command::new(&current_exe)
            .arg("gui-window") // Hidden command
            .spawn()?;

        debug!("GUI window process spawned successfully");
    }

    Ok(())
}

/// Handle GUI conversation mode - show egui window with conversation
#[allow(dead_code)]
fn handle_gui_conversation(user_text: &str, assistant_text: &str) -> Result<(), Box<dyn Error>> {
    debug!("Handling GUI conversation mode");

    // Load or create conversation state
    let mut state = if ConversationState::exists() {
        debug!("Loading existing conversation state");
        ConversationState::load()?
    } else {
        debug!("Creating new conversation state");
        // Create new conversation with timestamped filename
        let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?
            .join("transcribe-rs")
            .join("conversations");
        fs::create_dir_all(&config_dir)?;
        let conversation_file = config_dir.join(format!("conversation_{}.md", timestamp));
        debug!("Conversation file: {:?}", conversation_file);
        ConversationState::new(conversation_file)
    };

    // Add new messages
    state.add_message("user", user_text.to_string());
    state.add_message("assistant", assistant_text.to_string());

    // Save state for window to read
    state.window_open = true;
    state.save()?;
    debug!("Saved conversation state");

    // Save to markdown file
    state.save_to_markdown()?;
    debug!("Saved conversation to markdown");

    // Launch or update GUI window (blocking call)
    debug!("Launching GUI window...");
    if let Err(e) = ConversationWindow::run(state) {
        error!("GUI window error: {}", e);
        return Err(e);
    }

    debug!("GUI window closed");
    Ok(())
}

/// Call the transcribe-client binary to transcribe a file
fn transcribe_file(file: &PathBuf) -> Result<String, Box<dyn Error>> {
    // Try to find transcribe-client in the same directory as this binary
    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe.parent().ok_or("Cannot get exe directory")?;
    let client_path = exe_dir.join("transcribe-client");

    debug!("Looking for transcribe-client at: {:?}", client_path);

    if !client_path.exists() {
        error!("transcribe-client not found at {:?}", client_path);
        return Err(format!(
            "transcribe-client binary not found at {:?}. Make sure the daemon is running.",
            client_path
        )
        .into());
    }

    debug!("Calling transcribe-client with file: {:?}", file);
    let output = Command::new(&client_path).arg(file).output().map_err(|e| {
        error!("Failed to execute transcribe-client: {}", e);
        e
    })?;

    debug!("transcribe-client exit status: {:?}", output.status);

    if !output.status.success() {
        let err_output = String::from_utf8_lossy(&output.stderr);
        error!("transcribe-client stderr: {}", err_output);
        return Err(format!("Transcription failed: {}", err_output).into());
    }

    let text = String::from_utf8(output.stdout)?;
    debug!("transcribe-client stdout: '{}'", text);
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
fn clear_conversation_history(
    prompt_name: &str,
    config: &transcribe_rs::config::Config,
) -> Result<(), Box<dyn Error>> {
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
