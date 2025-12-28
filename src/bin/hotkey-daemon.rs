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
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::pin::pin;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use transcribe_rs::{
    clipboard, config::Config, eww_widget, paste, recording,
    transcription_corrections::TranscriptionCorrector,
    transcription_timing,
};

use transcribe_rs::config::HotkeyBinding;
use transcribe_rs::recording::RecordingResult;

/// Active binding info stored during recording
#[derive(Debug, Clone)]
struct ActiveBinding {
    key: Key,
    binding: HotkeyBinding,
}

/// State machine for the hotkey daemon
#[derive(Debug, Clone)]
enum RecordingState {
    /// Not recording, waiting for hotkey press
    Idle,
    /// Recording in push-to-talk mode (hotkey held)
    Recording {
        press_time: Instant,
        active: ActiveBinding,
    },
    /// Long recording mode (hotkey was tapped, waiting for completion)
    LongRecording { active: ActiveBinding },
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
/// Checks for virtual keyboards (kanata, kmonad) first, then falls back to physical keyboards
fn find_keyboard_devices() -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut devices: Vec<PathBuf> = Vec::new();

    // First, check for virtual keyboard remappers (kanata, kmonad, etc.)
    // These sit between physical keyboard and applications, so we need to listen to them
    for entry in std::fs::read_dir("/dev/input")? {
        let entry = entry?;
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("event") {
                // Read the device name from sysfs
                let sysfs_name = format!("/sys/class/input/{}/device/name", name);
                if let Ok(device_name) = std::fs::read_to_string(&sysfs_name) {
                    let device_name = device_name.trim().to_lowercase();
                    // Check for known virtual keyboard remappers
                    if device_name.contains("kanata")
                        || device_name.contains("kmonad")
                        || device_name.contains("keyd")
                    {
                        println!("Found virtual keyboard remapper: {} at {:?}", device_name, path);
                        devices.push(path);
                    }
                }
            }
        }
    }

    // If we found virtual keyboards, use those (they intercept physical keyboard events)
    if !devices.is_empty() {
        return Ok(devices);
    }

    // Fallback: try physical keyboards via by-id symlinks
    let pattern = "/dev/input/by-id/*-kbd";
    let physical_devices: Vec<PathBuf> = glob::glob(pattern)?
        .filter_map(|entry| entry.ok())
        .collect();

    if physical_devices.is_empty() {
        return Err("No keyboard devices found. Make sure you're in the 'input' group.".into());
    }

    Ok(physical_devices)
}

/// Start recording via the recording daemon
fn start_recording(config: &Config) -> Result<(), Box<dyn Error>> {
    info!("Starting recording...");
    recording::start_recording(&config.audio)?;
    eww_widget::show_recording_widget();
    Ok(())
}

/// Stop recording and get the audio file with timing info
fn stop_recording(config: &Config) -> Result<RecordingResult, Box<dyn Error>> {
    info!("Stopping recording...");
    eww_widget::hide_recording_widget();
    let result = recording::stop_recording(&config.audio)?;
    Ok(result)
}

/// Cancel recording without transcribing
fn cancel_recording(config: &Config) -> Result<(), Box<dyn Error>> {
    info!("Cancelling recording...");
    eww_widget::hide_recording_widget();

    // Use the cancel command which unconditionally resets daemon state
    recording::cancel_recording(&config.audio)?;

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

/// Result from LLM processing
struct LlmResult {
    text: String,
    tool_called: bool,
}

/// Process transcription through LLM if prompt is configured
fn process_with_llm(
    text: &str,
    prompt_name: Option<&str>,
    config: &Config,
) -> Result<LlmResult, Box<dyn Error>> {
    let prompt_name = match prompt_name {
        Some(name) => name,
        None => return Ok(LlmResult { text: text.to_string(), tool_called: false }),
    };

    use transcribe_rs::{groq, prompts};

    debug!("Processing with LLM prompt: {}", prompt_name);
    let prompt = prompts::load_prompt(prompt_name)?;

    // Estimate API time based on text length
    let estimated_ms = transcription_timing::estimate_api_time(text.len()).unwrap_or(0);
    debug!("Estimated API time: {}ms for {} chars", estimated_ms, text.len());

    // Start API progress animation in background
    let stop_animation = Arc::new(AtomicBool::new(false));
    start_api_progress_animation(estimated_ms, stop_animation.clone());

    // Start API timing for calibration
    let mut api_timer = transcription_timing::ApiTimer::new(text.len());
    api_timer.start();

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

    // Log API timing for future estimation
    if let Some(actual_ms) = api_timer.stop_and_log() {
        debug!("Actual API time: {}ms", actual_ms);
    }

    // Stop API progress animation
    stop_animation.store(true, Ordering::Relaxed);

    Ok(LlmResult { text: result.text, tool_called: result.tool_called })
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

    // Auto-paste if enabled
    if config.integration.auto_paste {
        if let Err(e) = paste::paste_from_clipboard() {
            warn!("Paste failed: {}", e);
        }
    }

    Ok(())
}

/// Run progress animation in background
/// Returns a handle to stop the animation
fn start_progress_animation(estimated_ms: u64, stop_flag: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let start = Instant::now();
        let estimated_duration = std::time::Duration::from_millis(estimated_ms);

        // Show loading widget
        eww_widget::show_loading_widget();

        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            let elapsed = start.elapsed();

            if estimated_ms > 0 {
                // Progress based on estimation
                let progress = ((elapsed.as_millis() as f64 / estimated_ms as f64) * 100.0).min(100.0) as u8;
                eww_widget::set_loading_progress(progress);

                if elapsed >= estimated_duration {
                    // If we exceed estimate, pulse between 80-100
                    let pulse_offset = ((elapsed.as_millis() % 400) as f64 / 400.0 * 20.0) as u8;
                    eww_widget::set_loading_progress(80 + pulse_offset);
                }
            } else {
                // Fallback pulsing animation (no historical data)
                let pulse_ms = transcription_timing::FALLBACK_PULSE_MS as u128;
                let cycle_pos = (elapsed.as_millis() % (pulse_ms * 2)) as f64;
                let progress = if cycle_pos < pulse_ms as f64 {
                    (cycle_pos / pulse_ms as f64 * 100.0) as u8
                } else {
                    (100.0 - ((cycle_pos - pulse_ms as f64) / pulse_ms as f64 * 100.0)) as u8
                };
                eww_widget::set_loading_progress(progress);
            }

            std::thread::sleep(std::time::Duration::from_millis(30));
        }

        // Set to 100% briefly before closing
        eww_widget::set_loading_progress(100);
        std::thread::sleep(std::time::Duration::from_millis(50));
        eww_widget::hide_loading_widget();
    });
}

/// Run API progress animation in background (yellow bar)
fn start_api_progress_animation(estimated_ms: u64, stop_flag: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let start = Instant::now();
        let estimated_duration = std::time::Duration::from_millis(estimated_ms);

        // Show API widget
        eww_widget::show_api_widget();

        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            let elapsed = start.elapsed();

            if estimated_ms > 0 {
                // Progress based on estimation
                let progress = ((elapsed.as_millis() as f64 / estimated_ms as f64) * 100.0).min(100.0) as u8;
                eww_widget::set_api_progress(progress);

                if elapsed >= estimated_duration {
                    // If we exceed estimate, pulse between 80-100
                    let pulse_offset = ((elapsed.as_millis() % 400) as f64 / 400.0 * 20.0) as u8;
                    eww_widget::set_api_progress(80 + pulse_offset);
                }
            } else {
                // Fallback pulsing animation (no historical data)
                let pulse_ms = transcription_timing::FALLBACK_PULSE_MS as u128;
                let cycle_pos = (elapsed.as_millis() % (pulse_ms * 2)) as f64;
                let progress = if cycle_pos < pulse_ms as f64 {
                    (cycle_pos / pulse_ms as f64 * 100.0) as u8
                } else {
                    (100.0 - ((cycle_pos - pulse_ms as f64) / pulse_ms as f64 * 100.0)) as u8
                };
                eww_widget::set_api_progress(progress);
            }

            std::thread::sleep(std::time::Duration::from_millis(30));
        }

        // Set to 100% briefly before closing
        eww_widget::set_api_progress(100);
        std::thread::sleep(std::time::Duration::from_millis(50));
        eww_widget::hide_api_widget();
    });
}

/// Process the complete transcription workflow
fn process_transcription(config: &Config, prompt_name: Option<&str>) -> Result<(), Box<dyn Error>> {
    // Stop recording and get audio file with timing info
    let recording_result = stop_recording(config)?;
    let audio_file = &recording_result.audio_file;
    let recording_ms = recording_result.duration_ms;

    debug!("Recording duration: {}ms", recording_ms);

    // Estimate transcription time based on historical data
    let estimated_ms = transcription_timing::estimate_transcription_time(recording_ms).unwrap_or(0);
    debug!("Estimated transcription time: {}ms", estimated_ms);

    // Start progress animation in background (non-blocking)
    let stop_animation = Arc::new(AtomicBool::new(false));
    start_progress_animation(estimated_ms, stop_animation.clone());

    // Start timing for calibration
    let mut timer = transcription_timing::TranscriptionTimer::new(recording_ms);
    timer.start();

    // Transcribe
    let transcription = transcribe_file(audio_file)?;

    // Log timing for future estimation calibration
    if let Some(actual_ms) = timer.stop_and_log() {
        debug!("Actual transcription time: {}ms", actual_ms);
    }

    // Stop progress animation
    stop_animation.store(true, Ordering::Relaxed);

    if transcription.is_empty() {
        return Err("Empty transcription".into());
    }

    info!("Transcription: {}", transcription);

    // Apply corrections
    let corrected = apply_corrections(&transcription, config);

    // Process with LLM if configured
    let llm_result = match process_with_llm(&corrected, prompt_name, config) {
        Ok(result) => result,
        Err(e) => {
            warn!("LLM processing failed: {}, using corrected text", e);
            LlmResult { text: corrected, tool_called: false }
        }
    };

    // Skip clipboard/paste if a tool was called - the tool effect is the action
    if llm_result.tool_called {
        info!("Tool was executed, skipping clipboard/paste");
        // Show notification with the LLM response confirming the action
        use transcribe_rs::notifications;
        let preview = if llm_result.text.len() > 100 {
            format!("{}...", &llm_result.text[..100])
        } else {
            llm_result.text.clone()
        };
        notifications::notify("Tool executed", &preview, 3000).ok();
        return Ok(());
    }

    // Copy and paste
    copy_and_paste(&llm_result.text, config)?;

    info!("Transcription complete: {}", llm_result.text);
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

    // Parse modifiers from config
    let modifiers: Vec<Modifier> = config
        .hotkey
        .modifiers
        .iter()
        .filter_map(|s| parse_modifier(s))
        .collect();

    // Find keyboard devices
    let devices = find_keyboard_devices()?;
    println!("Found {} keyboard device(s):", devices.len());
    for device in &devices {
        println!("  - {:?}", device);
    }

    // Create shortcut listener
    let listener = ShortcutListener::new();

    // Register all hotkey bindings and build a lookup map
    let mut binding_map: HashMap<Key, HotkeyBinding> = HashMap::new();

    println!("Registering {} hotkey binding(s):", config.hotkey.bindings.len());
    for binding in &config.hotkey.bindings {
        if let Some(key) = parse_key(&binding.key) {
            let shortcut = Shortcut::new(&modifiers, key);
            listener.add(shortcut);
            binding_map.insert(key, binding.clone());
            println!(
                "  - {:?}+{}: prompt={:?}, ocr={}, gui={}",
                config.hotkey.modifiers,
                binding.key,
                binding.prompt,
                binding.ocr,
                binding.gui
            );
        } else {
            eprintln!("Warning: Invalid key '{}' in binding, skipping", binding.key);
        }
    }

    if binding_map.is_empty() {
        return Err("No valid hotkey bindings configured".into());
    }

    // Register Escape key for cancellation (no modifiers)
    let escape_key = Key::KeyEsc;
    let escape_shortcut = Shortcut::new(&[], escape_key);
    listener.add(escape_shortcut);
    println!("Registered Escape key for cancellation");

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

    println!("Hotkey daemon running. Press Ctrl+C to stop.");
    println!("Tap threshold: {}ms", config.hotkey.tap_threshold_ms);

    while running.load(Ordering::SeqCst) {
        match stream.next().await {
            Some(event) => {
                // Check if this is an escape key event
                let is_escape = event.shortcut.key == escape_key;

                // Check if this is one of our registered hotkeys
                let pressed_key = event.shortcut.key;
                let binding_opt = binding_map.get(&pressed_key).cloned();

                println!("DEBUG Event: key={:?} state={:?} binding={:?}", pressed_key, event.state, binding_opt.is_some());

                match (&state, event.state, binding_opt, is_escape) {
                    // IDLE + Hotkey pressed -> Start recording
                    (RecordingState::Idle, ShortcutState::Pressed, Some(binding), _) => {
                        info!("Hotkey {:?} pressed - starting recording", pressed_key);
                        match start_recording(&config) {
                            Ok(_) => {
                                state = RecordingState::Recording {
                                    press_time: Instant::now(),
                                    active: ActiveBinding {
                                        key: pressed_key,
                                        binding,
                                    },
                                };
                            }
                            Err(e) => {
                                error!("Failed to start recording: {}", e);
                            }
                        }
                    }

                    // RECORDING + Same hotkey released -> Check tap vs hold
                    (RecordingState::Recording { press_time, active }, ShortcutState::Released, Some(_), _)
                        if pressed_key == active.key =>
                    {
                        let duration = press_time.elapsed();
                        info!("Hotkey released after {:?}", duration);

                        if duration < tap_threshold {
                            // Tap detected - enter long recording mode
                            info!("Tap detected - entering long recording mode");
                            state = RecordingState::LongRecording {
                                active: active.clone(),
                            };
                        } else {
                            // Hold detected - normal push-to-talk
                            info!("Hold detected - processing transcription");
                            let config_clone = config.clone();
                            let prompt_clone = active.binding.prompt.clone();
                            let result = tokio::task::spawn_blocking(move || {
                                process_transcription(&config_clone, prompt_clone.as_deref())
                                    .map_err(|e| e.to_string())
                            }).await;
                            match result {
                                Ok(Ok(_)) => info!("Transcription processed successfully"),
                                Ok(Err(e)) => error!("Transcription failed: {}", e),
                                Err(e) => error!("Task panicked: {}", e),
                            }
                            state = RecordingState::Idle;
                        }
                    }

                    // LONG RECORDING + Same hotkey pressed -> Finish recording
                    (RecordingState::LongRecording { active }, ShortcutState::Pressed, Some(_), _)
                        if pressed_key == active.key =>
                    {
                        info!("Long recording finished - processing");
                        let config_clone = config.clone();
                        let prompt_clone = active.binding.prompt.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            process_transcription(&config_clone, prompt_clone.as_deref())
                                .map_err(|e| e.to_string())
                        }).await;
                        match result {
                            Ok(Ok(_)) => info!("Transcription processed successfully"),
                            Ok(Err(e)) => error!("Transcription failed: {}", e),
                            Err(e) => error!("Task panicked: {}", e),
                        }
                        state = RecordingState::Idle;
                    }

                    // LONG RECORDING + Escape pressed -> Cancel
                    (RecordingState::LongRecording { .. }, ShortcutState::Pressed, _, true) => {
                        info!("Escape pressed - cancelling recording");
                        match cancel_recording(&config) {
                            Ok(_) => info!("Recording cancelled"),
                            Err(e) => error!("Failed to cancel: {}", e),
                        }
                        state = RecordingState::Idle;
                    }

                    // Ignore other combinations
                    _ => {}
                }
            }
            None => {
                info!("Event stream ended");
                break;
            }
        }
    }

    // Cleanup on exit
    if !matches!(state, RecordingState::Idle) {
        warn!("Cleaning up recording state on exit");
        cancel_recording(&config).ok();
    }

    info!("Hotkey daemon stopped");
    Ok(())
}
