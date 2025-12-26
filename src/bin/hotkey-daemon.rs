//! Hotkey daemon for push-to-talk transcription
//!
//! This daemon listens for keyboard events via evdev and provides:
//! - Push-to-talk: Hold hotkey to record, release to transcribe
//! - Tap-to-toggle: Quick tap enters long-recording mode, tap again to finish
//! - Escape to cancel during long-recording mode
//!
//! Uses evdev-shortcut crate for keyboard monitoring, bypassing Wayland compositor
//! limitations and providing reliable key detection regardless of release order.

use evdev_shortcut::{Key, Modifier, Shortcut, ShortcutListener, ShortcutState};
use futures::StreamExt;
use log::{debug, error, info, warn};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::pin::pin;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use transcribe_rs::{
    clipboard, config::Config, eww_widget, notifications, paste, recording,
    transcription_corrections::TranscriptionCorrector,
};

/// State machine for the hotkey daemon
#[derive(Debug, Clone, PartialEq)]
enum RecordingState {
    /// Not recording, waiting for hotkey press
    Idle,
    /// Recording in push-to-talk mode (hotkey held)
    Recording { press_time: Instant },
    /// Long recording mode (hotkey was tapped, waiting for completion)
    LongRecording,
}

/// Parse a modifier string to evdev Modifier
fn parse_modifier(s: &str) -> Option<Modifier> {
    match s.to_lowercase().as_str() {
        "super" | "meta" | "win" | "logo" => Some(Modifier::Meta),
        "shift" => Some(Modifier::Shift),
        "ctrl" | "control" => Some(Modifier::Ctrl),
        "alt" => Some(Modifier::Alt),
        _ => {
            warn!("Unknown modifier: {}", s);
            None
        }
    }
}

/// Parse a key string to evdev Key
fn parse_key(s: &str) -> Option<Key> {
    match s.to_lowercase().as_str() {
        "a" => Some(Key::KeyA),
        "b" => Some(Key::KeyB),
        "c" => Some(Key::KeyC),
        "d" => Some(Key::KeyD),
        "e" => Some(Key::KeyE),
        "f" => Some(Key::KeyF),
        "g" => Some(Key::KeyG),
        "h" => Some(Key::KeyH),
        "i" => Some(Key::KeyI),
        "j" => Some(Key::KeyJ),
        "k" => Some(Key::KeyK),
        "l" => Some(Key::KeyL),
        "m" => Some(Key::KeyM),
        "n" => Some(Key::KeyN),
        "o" => Some(Key::KeyO),
        "p" => Some(Key::KeyP),
        "q" => Some(Key::KeyQ),
        "r" => Some(Key::KeyR),
        "s" => Some(Key::KeyS),
        "t" => Some(Key::KeyT),
        "u" => Some(Key::KeyU),
        "v" => Some(Key::KeyV),
        "w" => Some(Key::KeyW),
        "x" => Some(Key::KeyX),
        "y" => Some(Key::KeyY),
        "z" => Some(Key::KeyZ),
        "1" => Some(Key::Key1),
        "2" => Some(Key::Key2),
        "3" => Some(Key::Key3),
        "4" => Some(Key::Key4),
        "5" => Some(Key::Key5),
        "6" => Some(Key::Key6),
        "7" => Some(Key::Key7),
        "8" => Some(Key::Key8),
        "9" => Some(Key::Key9),
        "0" => Some(Key::Key0),
        "space" => Some(Key::KeySpace),
        "enter" | "return" => Some(Key::KeyEnter),
        "escape" | "esc" => Some(Key::KeyEsc),
        "tab" => Some(Key::KeyTab),
        "backspace" => Some(Key::KeyBackspace),
        "f1" => Some(Key::KeyF1),
        "f2" => Some(Key::KeyF2),
        "f3" => Some(Key::KeyF3),
        "f4" => Some(Key::KeyF4),
        "f5" => Some(Key::KeyF5),
        "f6" => Some(Key::KeyF6),
        "f7" => Some(Key::KeyF7),
        "f8" => Some(Key::KeyF8),
        "f9" => Some(Key::KeyF9),
        "f10" => Some(Key::KeyF10),
        "f11" => Some(Key::KeyF11),
        "f12" => Some(Key::KeyF12),
        _ => {
            warn!("Unknown key: {}", s);
            None
        }
    }
}

/// Find keyboard devices
fn find_keyboard_devices() -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let pattern = "/dev/input/by-id/*-kbd";
    let devices: Vec<PathBuf> = glob::glob(pattern)?
        .filter_map(|entry| entry.ok())
        .collect();

    if devices.is_empty() {
        // Fallback: try to find any event device that looks like a keyboard
        let fallback_pattern = "/dev/input/by-id/*-event-kbd";
        let fallback_devices: Vec<PathBuf> = glob::glob(fallback_pattern)?
            .filter_map(|entry| entry.ok())
            .collect();

        if fallback_devices.is_empty() {
            return Err("No keyboard devices found. Make sure you're in the 'input' group.".into());
        }
        return Ok(fallback_devices);
    }

    Ok(devices)
}

/// Start recording via the recording daemon
fn start_recording(config: &Config) -> Result<(), Box<dyn Error>> {
    info!("Starting recording...");
    recording::start_recording(&config.audio)?;
    notifications::notify_recording_started()?;
    eww_widget::show_recording_widget();
    Ok(())
}

/// Stop recording and get the audio file path
fn stop_recording(config: &Config) -> Result<PathBuf, Box<dyn Error>> {
    info!("Stopping recording...");
    eww_widget::hide_recording_widget();
    notifications::notify_recording_stopped()?;
    let audio_file = recording::stop_recording(&config.audio)?;
    Ok(audio_file)
}

/// Cancel recording without transcribing
fn cancel_recording(config: &Config) -> Result<(), Box<dyn Error>> {
    info!("Cancelling recording...");
    eww_widget::hide_recording_widget();

    // Check if we're actually recording
    if std::path::Path::new(&config.audio.recording_pid_file).exists() {
        // Remove the recording state file to cancel
        fs::remove_file(&config.audio.recording_pid_file)?;
        notifications::notify("🚫 Cancelled", "Recording cancelled", 1500)?;
    }

    Ok(())
}

/// Transcribe audio file via daemon client
fn transcribe_file(audio_file: &PathBuf) -> Result<String, Box<dyn Error>> {
    info!("Transcribing: {:?}", audio_file);

    // Find the transcribe-client binary
    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe.parent().ok_or("Cannot get exe directory")?;
    let client_path = exe_dir.join("transcribe-client");

    if !client_path.exists() {
        return Err(format!(
            "transcribe-client not found at {:?}. Make sure the daemon is running.",
            client_path
        )
        .into());
    }

    let output = Command::new(&client_path)
        .arg(audio_file)
        .output()
        .map_err(|e| format!("Failed to execute transcribe-client: {}", e))?;

    if !output.status.success() {
        let err_output = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Transcription failed: {}", err_output).into());
    }

    let text = String::from_utf8(output.stdout)?;
    Ok(text.trim().to_string())
}

/// Apply transcription corrections
fn apply_corrections(text: &str, config: &Config) -> String {
    if !config.transcription_corrections.enabled {
        return text.to_string();
    }

    let corrections_file = PathBuf::from(&config.transcription_corrections.corrections_file);
    match TranscriptionCorrector::from_file(&corrections_file) {
        Ok(corrector) => {
            let corrected = corrector.correct(text);
            if corrected != text {
                debug!("Applied corrections: '{}' → '{}'", text, corrected);
            }
            corrected
        }
        Err(e) => {
            warn!("Failed to load corrections: {}", e);
            text.to_string()
        }
    }
}

/// Process transcription through LLM if prompt is configured
fn process_with_llm(
    text: &str,
    prompt_name: Option<&str>,
    config: &Config,
) -> Result<String, Box<dyn Error>> {
    let prompt_name = match prompt_name {
        Some(name) => name,
        None => return Ok(text.to_string()),
    };

    use transcribe_rs::{groq, prompts};

    debug!("Processing with LLM prompt: {}", prompt_name);
    let prompt = prompts::load_prompt(prompt_name)?;

    // Create client based on tool mapping
    let client = if let Some(tool_set_name) = config.llm.prompt_tool_mapping.get(prompt_name) {
        if let Some(tool_names) = config.llm.tool_sets.get(tool_set_name) {
            if tool_names.is_empty() {
                groq::GroqClient::from_env_file_no_tools()?
            } else {
                groq::GroqClient::from_env_file_with_tool_set(tool_names.clone())?
            }
        } else {
            groq::GroqClient::from_env_file_no_tools()?
        }
    } else {
        groq::GroqClient::from_env_file_no_tools()?
    };

    let result = client.complete_with_context(&prompt, text, prompt_name, None)?;
    Ok(result.text)
}

/// Copy to clipboard and paste
fn copy_and_paste(text: &str, config: &Config) -> Result<(), Box<dyn Error>> {
    // Add trailing space after punctuation
    let text = if config.integration.add_space_after_punctuation {
        clipboard::add_trailing_space_after_punctuation(text)
    } else {
        text.to_string()
    };

    // Copy to clipboard
    clipboard::copy_to_clipboard(&text)?;

    // Create preview
    let preview = if text.len() > 100 {
        format!("{}...", &text[..100])
    } else {
        text.clone()
    };

    // Auto-paste if enabled
    if config.integration.auto_paste {
        match paste::paste_from_clipboard() {
            Ok(_) => {
                notifications::notify_transcription_pasted(&preview)?;
            }
            Err(e) => {
                warn!("Paste failed: {}", e);
                notifications::notify_transcription_copied(&preview)?;
            }
        }
    } else {
        notifications::notify_transcription_copied(&preview)?;
    }

    Ok(())
}

/// Process the complete transcription workflow
fn process_transcription(config: &Config, prompt_name: Option<&str>) -> Result<(), Box<dyn Error>> {
    // Stop recording and get audio file
    let audio_file = stop_recording(config)?;

    // Transcribe
    let transcription = transcribe_file(&audio_file)?;
    if transcription.is_empty() {
        notifications::notify_error("No speech detected")?;
        return Err("Empty transcription".into());
    }

    info!("Transcription: {}", transcription);

    // Apply corrections
    let corrected = apply_corrections(&transcription, config);

    // Process with LLM if configured
    let final_text = match process_with_llm(&corrected, prompt_name, config) {
        Ok(text) => text,
        Err(e) => {
            warn!("LLM processing failed: {}, using corrected text", e);
            corrected
        }
    };

    // Copy and paste
    copy_and_paste(&final_text, config)?;

    info!("Transcription complete: {}", final_text);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    if let Err(e) = transcribe_rs::logging::init() {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    }

    info!("Starting hotkey daemon...");

    // Load configuration
    let config = Config::load()?;
    info!("Configuration loaded");

    // Parse hotkey from config
    let modifiers: Vec<Modifier> = config
        .hotkey
        .modifiers
        .iter()
        .filter_map(|s| parse_modifier(s))
        .collect();

    let key = parse_key(&config.hotkey.key).ok_or_else(|| {
        format!(
            "Invalid hotkey key: '{}'. Check your config.",
            config.hotkey.key
        )
    })?;

    info!(
        "Hotkey configured: {:?} + {:?}",
        config.hotkey.modifiers, config.hotkey.key
    );

    // Find keyboard devices
    let devices = find_keyboard_devices()?;
    info!("Found {} keyboard device(s)", devices.len());
    for device in &devices {
        info!("  - {:?}", device);
    }

    // Create shortcut listener
    let listener = ShortcutListener::new();

    // Register main hotkey
    let main_shortcut = Shortcut::new(&modifiers, key);
    listener.add(main_shortcut.clone());
    info!("Registered main hotkey");

    // Register Escape key for cancellation (no modifiers)
    let escape_shortcut = Shortcut::new(&[], Key::KeyEsc);
    listener.add(escape_shortcut.clone());
    info!("Registered Escape key for cancellation");

    // Create event stream
    let stream = listener.listen(&devices)?;
    let mut stream = pin!(stream);
    info!("Listening for keyboard events...");

    // Setup signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        info!("Received shutdown signal");
        r.store(false, Ordering::SeqCst);
    })?;

    // State machine
    let mut state = RecordingState::Idle;
    let tap_threshold = std::time::Duration::from_millis(config.hotkey.tap_threshold_ms);
    let prompt_name = config.hotkey.default_prompt.as_deref();

    println!("Hotkey daemon running. Press Ctrl+C to stop.");
    println!(
        "Hotkey: {:?} + {}",
        config.hotkey.modifiers, config.hotkey.key
    );
    println!("Tap threshold: {}ms", config.hotkey.tap_threshold_ms);

    while running.load(Ordering::SeqCst) {
        // Use tokio::select! to handle both events and shutdown
        tokio::select! {
            Some(event) = stream.next() => {
                let is_main_hotkey = event.shortcut == main_shortcut;
                let is_escape = event.shortcut == escape_shortcut;

                debug!("Event: {:?} state={:?}", event, state);

                match (&state, event.state, is_main_hotkey, is_escape) {
                    // IDLE + Hotkey pressed -> Start recording
                    (RecordingState::Idle, ShortcutState::Pressed, true, _) => {
                        info!("Hotkey pressed - starting recording");
                        match start_recording(&config) {
                            Ok(_) => {
                                state = RecordingState::Recording {
                                    press_time: Instant::now(),
                                };
                            }
                            Err(e) => {
                                error!("Failed to start recording: {}", e);
                                notifications::notify_error(&format!("Start failed: {}", e)).ok();
                            }
                        }
                    }

                    // RECORDING + Hotkey released -> Check tap vs hold
                    (RecordingState::Recording { press_time }, ShortcutState::Released, true, _) => {
                        let duration = press_time.elapsed();
                        info!("Hotkey released after {:?}", duration);

                        if duration < tap_threshold {
                            // Tap detected - enter long recording mode
                            info!("Tap detected - entering long recording mode");
                            notifications::notify("🎤 Long Recording", "Tap hotkey to finish, Escape to cancel", 2000).ok();
                            state = RecordingState::LongRecording;
                        } else {
                            // Hold detected - normal push-to-talk
                            info!("Hold detected - processing transcription");
                            match process_transcription(&config, prompt_name) {
                                Ok(_) => info!("Transcription processed successfully"),
                                Err(e) => {
                                    error!("Transcription failed: {}", e);
                                    notifications::notify_error(&format!("Error: {}", e)).ok();
                                }
                            }
                            state = RecordingState::Idle;
                        }
                    }

                    // LONG RECORDING + Hotkey pressed -> Finish recording
                    (RecordingState::LongRecording, ShortcutState::Pressed, true, _) => {
                        info!("Hotkey pressed in long recording mode - finishing");
                        match process_transcription(&config, prompt_name) {
                            Ok(_) => info!("Transcription processed successfully"),
                            Err(e) => {
                                error!("Transcription failed: {}", e);
                                notifications::notify_error(&format!("Error: {}", e)).ok();
                            }
                        }
                        state = RecordingState::Idle;
                    }

                    // LONG RECORDING + Escape pressed -> Cancel
                    (RecordingState::LongRecording, ShortcutState::Pressed, _, true) => {
                        info!("Escape pressed - cancelling recording");
                        match cancel_recording(&config) {
                            Ok(_) => info!("Recording cancelled"),
                            Err(e) => error!("Failed to cancel: {}", e),
                        }
                        state = RecordingState::Idle;
                    }

                    // Ignore other combinations
                    _ => {
                        debug!("Ignoring event in current state");
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Received Ctrl+C, shutting down...");
                break;
            }
        }
    }

    // Cleanup on exit
    if state != RecordingState::Idle {
        warn!("Cleaning up recording state on exit");
        cancel_recording(&config).ok();
    }

    info!("Hotkey daemon stopped");
    Ok(())
}
