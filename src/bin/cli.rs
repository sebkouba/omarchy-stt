use clap::{Parser, Subcommand};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use transcribe_rs::{clipboard, config::Config, notifications, paste, recording};

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
        Commands::Start => {
            let config = Config::load()?;
            handle_start(&config)
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
    }
}

fn handle_start(config: &Config) -> Result<(), Box<dyn Error>> {
    log("=== HANDLE START ===", &config.audio.log_file);

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

    // Stop recording
    log("Stopping recording...", &config.audio.log_file);
    let audio_file = recording::stop_recording(&config.audio)?;
    log(&format!("Audio file: {:?}", audio_file), &config.audio.log_file);

    log("Sending stop notification...", &config.audio.log_file);
    if let Err(e) = notifications::notify_recording_stopped() {
        log(&format!("WARNING: Notification failed: {}", e), &config.audio.log_file);
    }
    println!("⏹️  Recording stopped");

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

    if transcription.is_empty() {
        log("ERROR: Empty transcription", &config.audio.log_file);
        notifications::notify_error("No speech detected").ok();
        return Err("Empty transcription".into());
    }

    log(&format!("Transcription text: '{}'", transcription), &config.audio.log_file);

    // Process with Harper if enabled
    let (processed_text, _harper_session) = if config.harper.enabled {
        log("Processing with Harper...", &config.audio.log_file);
        use std::path::PathBuf;
        use transcribe_rs::harper_processor::Dialect;

        let dict_path = PathBuf::from(&config.harper.dictionary_path);
        let dialect = match config.harper.dialect.as_str() {
            "British" => Dialect::British,
            "Australian" => Dialect::Australian,
            "Canadian" => Dialect::Canadian,
            _ => Dialect::American,
        };

        match transcribe_rs::harper_processor::process_with_harper(
            &transcription,
            &dict_path,
            dialect,
            &config.harper.disabled_linters,
        ) {
            Ok(session) => {
                if session.has_corrections() {
                    log(&format!("Harper made {} corrections", session.corrections.len()), &config.audio.log_file);

                    // Save correction session
                    let corrections_dir = PathBuf::from(&config.harper.corrections_dir);
                    if let Err(e) = session.save_to_file(&corrections_dir) {
                        log(&format!("WARNING: Failed to save Harper corrections: {}", e), &config.audio.log_file);
                    }
                } else {
                    log("Harper: no corrections needed", &config.audio.log_file);
                }
                (session.corrected_text.clone(), Some(session))
            }
            Err(e) => {
                log(&format!("WARNING: Harper processing failed: {}", e), &config.audio.log_file);
                (transcription.clone(), None)
            }
        }
    } else {
        (transcription.clone(), None)
    };

    // Add space after punctuation
    log("Adding trailing space after punctuation...", &config.audio.log_file);
    let text = if config.integration.add_space_after_punctuation {
        clipboard::add_trailing_space_after_punctuation(&processed_text)
    } else {
        processed_text
    };
    log(&format!("Final text: '{}'", text), &config.audio.log_file);

    // Copy to clipboard
    log("Copying to clipboard...", &config.audio.log_file);
    match clipboard::copy_to_clipboard(&text) {
        Ok(_) => log("Clipboard copy successful", &config.audio.log_file),
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

    // Auto-paste if enabled and ydotool available
    if config.integration.auto_paste {
        log("Checking if ydotool is available...", &config.audio.log_file);
        if paste::is_ydotool_available() {
            log("ydotool available, attempting paste...", &config.audio.log_file);
            match paste::paste_from_clipboard() {
                Ok(_) => {
                    log("Paste successful", &config.audio.log_file);
                    notifications::notify_transcription_pasted(&preview).ok();
                }
                Err(e) => {
                    log(&format!("WARNING: Paste failed: {}", e), &config.audio.log_file);
                    notifications::notify_transcription_copied(&preview).ok();
                }
            }
        } else {
            log("ydotool not available, clipboard only", &config.audio.log_file);
            notifications::notify_transcription_copied(&preview).ok();
        }
    } else {
        log("Auto-paste disabled in config, clipboard only", &config.audio.log_file);
        notifications::notify_transcription_copied(&preview).ok();
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
