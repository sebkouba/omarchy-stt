//! Hotkey daemon for push-to-talk transcription with keyboard grab support
//!
//! This daemon listens for keyboard events via evdev and provides:
//! - Push-to-talk: Hold hotkey to record, release to transcribe
//! - Tap-to-toggle: Quick tap enters long-recording mode with keyboard grab
//! - Enter during long-recording: Submit current dictation, send Enter, restart recording
//! - Escape to cancel during long-recording mode
//!
//! Uses raw evdev for keyboard monitoring with EVIOCGRAB for exclusive access
//! during long-recording mode, preventing keys from passing through to apps.

use evdev::{Device, InputEventKind, Key};
use log::{debug, error, info, warn};
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use std::collections::HashMap;
use std::error::Error;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use transcribe_rs::{
    clipboard, config::Config, eww_widget, paste, recording,
    transcription_corrections::TranscriptionCorrector, transcription_timing,
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
    /// Recording in push-to-talk mode (hotkey held, NOT grabbed)
    Recording {
        press_time: Instant,
        active: ActiveBinding,
    },
    /// Long recording mode (keyboard GRABBED)
    LongRecording { active: ActiveBinding },
    /// Pending grab (waiting for all keys to release)
    PendingGrab { active: ActiveBinding },
    /// Pending ungrab after finishing (waiting for all keys to release)
    PendingUngrab { should_transcribe: bool, active: ActiveBinding },
}

/// Modifier state tracking
#[derive(Default)]
struct ModifierState {
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
}

impl ModifierState {
    /// Update modifier state. Only call on press (value=1) or release (value=0), not repeat (value=2)
    fn update(&mut self, key: Key, pressed: bool, released: bool) {
        // Only update on actual press/release, not repeat events
        if !pressed && !released {
            return;
        }
        let is_down = pressed;
        match key {
            Key::KEY_LEFTCTRL | Key::KEY_RIGHTCTRL => self.ctrl = is_down,
            Key::KEY_LEFTALT | Key::KEY_RIGHTALT => self.alt = is_down,
            Key::KEY_LEFTSHIFT | Key::KEY_RIGHTSHIFT => self.shift = is_down,
            Key::KEY_LEFTMETA | Key::KEY_RIGHTMETA => self.meta = is_down,
            _ => {}
        }
    }

    fn all_released(&self) -> bool {
        !self.ctrl && !self.alt && !self.shift && !self.meta
    }

    fn matches_config(&self, modifiers: &[String]) -> bool {
        let want_ctrl = modifiers.iter().any(|m| m.eq_ignore_ascii_case("ctrl") || m.eq_ignore_ascii_case("control"));
        let want_alt = modifiers.iter().any(|m| m.eq_ignore_ascii_case("alt"));
        let want_shift = modifiers.iter().any(|m| m.eq_ignore_ascii_case("shift"));
        let want_meta = modifiers.iter().any(|m| {
            m.eq_ignore_ascii_case("super") || m.eq_ignore_ascii_case("meta") || m.eq_ignore_ascii_case("win") || m.eq_ignore_ascii_case("logo")
        });

        self.ctrl == want_ctrl && self.alt == want_alt && self.shift == want_shift && self.meta == want_meta
    }
}

/// Parse a key string to evdev Key
fn parse_key(s: &str) -> Option<Key> {
    match s.to_lowercase().as_str() {
        "a" => Some(Key::KEY_A),
        "b" => Some(Key::KEY_B),
        "c" => Some(Key::KEY_C),
        "d" => Some(Key::KEY_D),
        "e" => Some(Key::KEY_E),
        "f" => Some(Key::KEY_F),
        "g" => Some(Key::KEY_G),
        "h" => Some(Key::KEY_H),
        "i" => Some(Key::KEY_I),
        "j" => Some(Key::KEY_J),
        "k" => Some(Key::KEY_K),
        "l" => Some(Key::KEY_L),
        "m" => Some(Key::KEY_M),
        "n" => Some(Key::KEY_N),
        "o" => Some(Key::KEY_O),
        "p" => Some(Key::KEY_P),
        "q" => Some(Key::KEY_Q),
        "r" => Some(Key::KEY_R),
        "s" => Some(Key::KEY_S),
        "t" => Some(Key::KEY_T),
        "u" => Some(Key::KEY_U),
        "v" => Some(Key::KEY_V),
        "w" => Some(Key::KEY_W),
        "x" => Some(Key::KEY_X),
        "y" => Some(Key::KEY_Y),
        "z" => Some(Key::KEY_Z),
        "1" => Some(Key::KEY_1),
        "2" => Some(Key::KEY_2),
        "3" => Some(Key::KEY_3),
        "4" => Some(Key::KEY_4),
        "5" => Some(Key::KEY_5),
        "6" => Some(Key::KEY_6),
        "7" => Some(Key::KEY_7),
        "8" => Some(Key::KEY_8),
        "9" => Some(Key::KEY_9),
        "0" => Some(Key::KEY_0),
        "space" => Some(Key::KEY_SPACE),
        "enter" | "return" => Some(Key::KEY_ENTER),
        "escape" | "esc" => Some(Key::KEY_ESC),
        "tab" => Some(Key::KEY_TAB),
        "backspace" => Some(Key::KEY_BACKSPACE),
        "f1" => Some(Key::KEY_F1),
        "f2" => Some(Key::KEY_F2),
        "f3" => Some(Key::KEY_F3),
        "f4" => Some(Key::KEY_F4),
        "f5" => Some(Key::KEY_F5),
        "f6" => Some(Key::KEY_F6),
        "f7" => Some(Key::KEY_F7),
        "f8" => Some(Key::KEY_F8),
        "f9" => Some(Key::KEY_F9),
        "f10" => Some(Key::KEY_F10),
        "f11" => Some(Key::KEY_F11),
        "f12" => Some(Key::KEY_F12),
        _ => {
            warn!("Unknown key: {}", s);
            None
        }
    }
}

/// Find all keyboard devices
fn find_all_keyboards() -> Vec<(PathBuf, Device)> {
    let mut keyboards = Vec::new();

    // First check for virtual keyboard remappers
    if let Ok(entries) = std::fs::read_dir("/dev/input") {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.to_string_lossy().contains("event") {
                continue;
            }

            if let Ok(device) = Device::open(&path) {
                let name = device.name().unwrap_or("").to_lowercase();
                // Check for kanata, kmonad, keyd
                if name.contains("kanata") || name.contains("kmonad") || name.contains("keyd") {
                    println!("Found virtual keyboard: {} at {:?}", device.name().unwrap_or("Unknown"), path);
                    keyboards.push((path, device));
                }
            }
        }
    }

    // If we found virtual keyboards, use those
    if !keyboards.is_empty() {
        return keyboards;
    }

    // Fallback: find physical keyboards
    if let Ok(entries) = std::fs::read_dir("/dev/input") {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.to_string_lossy().contains("event") {
                continue;
            }

            if let Ok(device) = Device::open(&path) {
                if let Some(keys) = device.supported_keys() {
                    // Must have Enter and alphabetic keys to be a keyboard
                    if keys.contains(Key::KEY_ENTER) && keys.contains(Key::KEY_A) {
                        keyboards.push((path, device));
                    }
                }
            }
        }
    }

    keyboards
}

/// Check if running interactively (has a tty)
fn is_interactive() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// Auto-select keyboard: prefer virtual keyboard remappers (kanata), otherwise first found
fn auto_select_keyboard(keyboards: &[(PathBuf, Device)]) -> usize {
    // Prefer virtual keyboard remappers (kanata, kmonad, keyd)
    for (i, (_, dev)) in keyboards.iter().enumerate() {
        let name = dev.name().unwrap_or("").to_lowercase();
        if name.contains("kanata") || name.contains("kmonad") || name.contains("keyd") {
            println!("Auto-selected virtual keyboard: {}", dev.name().unwrap_or("Unknown"));
            return i;
        }
    }
    // Otherwise just use the first one
    println!("Auto-selected first keyboard: {}", keyboards[0].1.name().unwrap_or("Unknown"));
    0
}

/// Detect which keyboard to use by waiting for a keypress (interactive mode only)
fn detect_active_keyboard(keyboards: &mut [(PathBuf, Device)]) -> Option<usize> {
    println!("Detecting active keyboard...");
    println!("Press any key to detect which device to use:");
    println!();

    let mut poll_fds: Vec<_> = keyboards
        .iter()
        .map(|(_, dev)| {
            let fd = unsafe { BorrowedFd::borrow_raw(dev.as_raw_fd()) };
            PollFd::new(fd, PollFlags::POLLIN)
        })
        .collect();

    let timeout = PollTimeout::try_from(10000_u16).unwrap();

    loop {
        match poll(&mut poll_fds, timeout) {
            Ok(0) => {
                println!("Timeout waiting for keypress.");
                return None;
            }
            Ok(_) => {
                let mut last_device_with_keypress = None;

                for (i, pfd) in poll_fds.iter().enumerate() {
                    if let Some(revents) = pfd.revents() {
                        if revents.contains(PollFlags::POLLIN) {
                            let name = keyboards[i].1.name().unwrap_or("Unknown").to_string();
                            if let Ok(events) = keyboards[i].1.fetch_events() {
                                for event in events {
                                    if let InputEventKind::Key(key) = event.kind() {
                                        if event.value() == 1 {
                                            println!("  Detected keypress ({:?}) on: {}", key, name);
                                            last_device_with_keypress = Some(i);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Wait for chained devices (physical -> kanata -> etc)
                if last_device_with_keypress.is_some() {
                    std::thread::sleep(Duration::from_millis(100));

                    for i in 0..keyboards.len() {
                        let fd = unsafe { BorrowedFd::borrow_raw(keyboards[i].1.as_raw_fd()) };
                        let mut single_poll = [PollFd::new(fd, PollFlags::POLLIN)];
                        let short_timeout = PollTimeout::try_from(50_u16).unwrap();
                        if let Ok(n) = poll(&mut single_poll, short_timeout) {
                            if n > 0 {
                                let name = keyboards[i].1.name().unwrap_or("Unknown").to_string();
                                if let Ok(events) = keyboards[i].1.fetch_events() {
                                    for event in events {
                                        if let InputEventKind::Key(key) = event.kind() {
                                            if event.value() == 1 {
                                                println!("  Detected keypress ({:?}) on: {}", key, name);
                                                last_device_with_keypress = Some(i);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    println!();
                    return last_device_with_keypress;
                }
            }
            Err(e) => {
                eprintln!("Poll error: {}", e);
                return None;
            }
        }
    }
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
    recording::cancel_recording(&config.audio)?;
    Ok(())
}

/// Transcribe audio file via daemon client
fn transcribe_file(audio_file: &PathBuf) -> Result<String, Box<dyn Error>> {
    info!("Transcribing: {:?}", audio_file);

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

    let estimated_ms = transcription_timing::estimate_api_time(text.len()).unwrap_or(0);
    debug!("Estimated API time: {}ms for {} chars", estimated_ms, text.len());

    let stop_animation = Arc::new(AtomicBool::new(false));
    start_api_progress_animation(estimated_ms, stop_animation.clone());

    let mut api_timer = transcription_timing::ApiTimer::new(text.len());
    api_timer.start();

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

    if let Some(actual_ms) = api_timer.stop_and_log() {
        debug!("Actual API time: {}ms", actual_ms);
    }

    stop_animation.store(true, Ordering::Relaxed);

    Ok(LlmResult { text: result.text, tool_called: result.tool_called })
}

/// Copy to clipboard and paste
fn copy_and_paste(text: &str, config: &Config) -> Result<(), Box<dyn Error>> {
    let text = if config.integration.add_space_after_punctuation {
        clipboard::add_trailing_space_after_punctuation(text)
    } else {
        text.to_string()
    };

    clipboard::copy_to_clipboard(&text)?;

    if config.integration.auto_paste {
        if let Err(e) = paste::paste_from_clipboard() {
            warn!("Paste failed: {}", e);
        }
    }

    Ok(())
}

/// Send Enter key via ydotool
fn send_enter_key() -> Result<(), Box<dyn Error>> {
    debug!("Sending Enter key via ydotool");
    std::env::set_var("YDOTOOL_SOCKET", "/tmp/.ydotool_socket");

    // Key code 28 = Enter
    let status = Command::new("ydotool")
        .args(["key", "28:1", "28:0"])
        .status()
        .map_err(|e| format!("Failed to execute ydotool: {}", e))?;

    if !status.success() {
        return Err("ydotool command failed".into());
    }

    Ok(())
}

/// Run progress animation in background
fn start_progress_animation(estimated_ms: u64, stop_flag: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let start = Instant::now();
        let estimated_duration = Duration::from_millis(estimated_ms);

        eww_widget::show_loading_widget();

        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            let elapsed = start.elapsed();

            if estimated_ms > 0 {
                let progress = ((elapsed.as_millis() as f64 / estimated_ms as f64) * 100.0).min(100.0) as u8;
                eww_widget::set_loading_progress(progress);

                if elapsed >= estimated_duration {
                    let pulse_offset = ((elapsed.as_millis() % 400) as f64 / 400.0 * 20.0) as u8;
                    eww_widget::set_loading_progress(80 + pulse_offset);
                }
            } else {
                let pulse_ms = transcription_timing::FALLBACK_PULSE_MS as u128;
                let cycle_pos = (elapsed.as_millis() % (pulse_ms * 2)) as f64;
                let progress = if cycle_pos < pulse_ms as f64 {
                    (cycle_pos / pulse_ms as f64 * 100.0) as u8
                } else {
                    (100.0 - ((cycle_pos - pulse_ms as f64) / pulse_ms as f64 * 100.0)) as u8
                };
                eww_widget::set_loading_progress(progress);
            }

            std::thread::sleep(Duration::from_millis(30));
        }

        eww_widget::set_loading_progress(100);
        std::thread::sleep(Duration::from_millis(50));
        eww_widget::hide_loading_widget();
    });
}

/// Run API progress animation in background (yellow bar)
fn start_api_progress_animation(estimated_ms: u64, stop_flag: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let start = Instant::now();
        let estimated_duration = Duration::from_millis(estimated_ms);

        eww_widget::show_api_widget();

        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            let elapsed = start.elapsed();

            if estimated_ms > 0 {
                let progress = ((elapsed.as_millis() as f64 / estimated_ms as f64) * 100.0).min(100.0) as u8;
                eww_widget::set_api_progress(progress);

                if elapsed >= estimated_duration {
                    let pulse_offset = ((elapsed.as_millis() % 400) as f64 / 400.0 * 20.0) as u8;
                    eww_widget::set_api_progress(80 + pulse_offset);
                }
            } else {
                let pulse_ms = transcription_timing::FALLBACK_PULSE_MS as u128;
                let cycle_pos = (elapsed.as_millis() % (pulse_ms * 2)) as f64;
                let progress = if cycle_pos < pulse_ms as f64 {
                    (cycle_pos / pulse_ms as f64 * 100.0) as u8
                } else {
                    (100.0 - ((cycle_pos - pulse_ms as f64) / pulse_ms as f64 * 100.0)) as u8
                };
                eww_widget::set_api_progress(progress);
            }

            std::thread::sleep(Duration::from_millis(30));
        }

        eww_widget::set_api_progress(100);
        std::thread::sleep(Duration::from_millis(50));
        eww_widget::hide_api_widget();
    });
}

/// Process the complete transcription workflow
fn process_transcription(config: &Config, prompt_name: Option<&str>) -> Result<(), Box<dyn Error>> {
    let recording_result = stop_recording(config)?;
    let audio_file = &recording_result.audio_file;
    let recording_ms = recording_result.duration_ms;

    debug!("Recording duration: {}ms", recording_ms);

    let estimated_ms = transcription_timing::estimate_transcription_time(recording_ms).unwrap_or(0);
    debug!("Estimated transcription time: {}ms", estimated_ms);

    let stop_animation = Arc::new(AtomicBool::new(false));
    start_progress_animation(estimated_ms, stop_animation.clone());

    let mut timer = transcription_timing::TranscriptionTimer::new(recording_ms);
    timer.start();

    let transcription = transcribe_file(audio_file)?;

    if let Some(actual_ms) = timer.stop_and_log() {
        debug!("Actual transcription time: {}ms", actual_ms);
    }

    stop_animation.store(true, Ordering::Relaxed);

    if transcription.is_empty() {
        return Err("Empty transcription".into());
    }

    info!("Transcription: {}", transcription);

    let corrected = apply_corrections(&transcription, config);

    let llm_result = match process_with_llm(&corrected, prompt_name, config) {
        Ok(result) => result,
        Err(e) => {
            warn!("LLM processing failed: {}, using corrected text", e);
            LlmResult { text: corrected, tool_called: false }
        }
    };

    if llm_result.tool_called {
        info!("Tool was executed, skipping clipboard/paste");
        use transcribe_rs::notifications;
        let preview = if llm_result.text.len() > 100 {
            format!("{}...", &llm_result.text[..100])
        } else {
            llm_result.text.clone()
        };
        notifications::notify("Tool executed", &preview, 3000).ok();
        return Ok(());
    }

    copy_and_paste(&llm_result.text, config)?;

    info!("Transcription complete: {}", llm_result.text);
    Ok(())
}

/// Process transcription and send Enter (for submit+continue flow)
fn process_transcription_and_send_enter(config: &Config, prompt_name: Option<&str>) -> Result<(), Box<dyn Error>> {
    // Process the transcription normally
    process_transcription(config, prompt_name)?;

    // Small delay to ensure paste completes
    std::thread::sleep(Duration::from_millis(50));

    // Send Enter key
    send_enter_key()?;

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    // Disable stdout buffering for systemd journal
    use std::io::Write;
    let _ = std::io::stdout().flush();

    // Initialize logging
    if let Err(e) = transcribe_rs::logging::init() {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    }

    eprintln!("[STARTUP] Starting hotkey daemon with grab support...");

    // Load configuration
    let config = Config::load()?;
    info!("Configuration loaded");

    // Find keyboard devices
    let mut keyboards = find_all_keyboards();

    if keyboards.is_empty() {
        eprintln!("No keyboards found! Make sure you have permission to read /dev/input/event*");
        eprintln!("Try adding your user to the 'input' group.");
        std::process::exit(1);
    }

    println!("Found {} keyboard device(s):", keyboards.len());
    for (path, dev) in &keyboards {
        println!("  - {} at {:?}", dev.name().unwrap_or("Unknown"), path);
    }
    println!();

    // Select which keyboard to use
    let device_idx = if is_interactive() {
        // Interactive mode: wait for keypress to detect
        detect_active_keyboard(&mut keyboards)
            .expect("Could not detect active keyboard")
    } else {
        // Non-interactive (systemd): auto-select
        auto_select_keyboard(&keyboards)
    };

    let (path, _) = keyboards.remove(device_idx);
    let mut device = Device::open(&path)?;

    println!("Using: {} at {:?}", device.name().unwrap_or("Unknown"), path);
    println!();

    // Build binding map
    let mut binding_map: HashMap<Key, HotkeyBinding> = HashMap::new();
    println!("Registering {} hotkey binding(s):", config.hotkey.bindings.len());
    for binding in &config.hotkey.bindings {
        if let Some(key) = parse_key(&binding.key) {
            binding_map.insert(key, binding.clone());
            println!(
                "  - {:?}+{}: prompt={:?}",
                config.hotkey.modifiers,
                binding.key,
                binding.prompt,
            );
        } else {
            eprintln!("Warning: Invalid key '{}' in binding, skipping", binding.key);
        }
    }

    if binding_map.is_empty() {
        return Err("No valid hotkey bindings configured".into());
    }

    // Setup signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        info!("Received shutdown signal");
        r.store(false, Ordering::SeqCst);
    })?;

    // State machine
    let mut state = RecordingState::Idle;
    let mut modifiers = ModifierState::default();
    let mut key_pressed: HashMap<Key, bool> = HashMap::new();
    let tap_threshold = Duration::from_millis(config.hotkey.tap_threshold_ms);
    let mut is_grabbed = false;

    println!();
    println!("Hotkey daemon running. Press Ctrl+C to stop.");
    println!("Tap threshold: {}ms", config.hotkey.tap_threshold_ms);
    println!();
    println!("During long recording:");
    println!("  - Enter: Submit + continue (paste, send Enter, restart recording)");
    println!("  - Escape: Cancel recording");
    println!("  - Hotkey: Finish recording");
    println!();

    eprintln!("[INFO] Entering main event loop, listening on fd={}", device.as_raw_fd());

    while running.load(Ordering::SeqCst) {
        // Use poll with timeout to allow checking running flag
        let fd = unsafe { BorrowedFd::borrow_raw(device.as_raw_fd()) };
        let mut poll_fds = [PollFd::new(fd, PollFlags::POLLIN)];
        let loop_timeout = PollTimeout::try_from(100_u16).unwrap();

        match poll(&mut poll_fds, loop_timeout) {
            Ok(0) => {
                // Timeout - this is normal, just continue checking running flag
                continue;
            }
            Ok(n) => {
                eprintln!("[POLL] Got {} ready fd(s)", n);
                // Got events
                if n > 0 {
                    // Check revents
                    if let Some(revents) = poll_fds[0].revents() {
                        eprintln!("[POLL] revents={:?}", revents);
                        if !revents.contains(PollFlags::POLLIN) {
                            eprintln!("[WARN] Poll returned but no POLLIN");
                            continue;
                        }
                    }
                }
            }
            Err(e) => {
                if e == nix::errno::Errno::EINTR {
                    continue;
                }
                eprintln!("[ERROR] Poll error: {}", e);
                break;
            }
        }

        let events: Vec<_> = match device.fetch_events() {
            Ok(events) => events.collect(),
            Err(e) => {
                eprintln!("[ERROR] Failed to fetch events: {}", e);
                continue;
            }
        };

        if events.is_empty() {
            continue;
        }

        for event in events {
            if let InputEventKind::Key(key) = event.kind() {
                let pressed = event.value() == 1;
                let released = event.value() == 0;
                let repeat = event.value() == 2;

                // Skip repeat events for state tracking
                if repeat {
                    continue;
                }

                // Log every key event for debugging
                let action = if pressed { "PRESS" } else { "RELEASE" };
                eprintln!("[KEY] {:?} {} | state={:?} | grabbed={}", key, action, state, is_grabbed);

                // Update modifier state
                modifiers.update(key, pressed, released);

                // Track all key states for "all released" detection
                if pressed {
                    key_pressed.insert(key, true);
                } else if released {
                    key_pressed.remove(&key);
                }

                let all_keys_released = key_pressed.is_empty() && modifiers.all_released();

                if all_keys_released {
                    eprintln!("[STATE] All keys released detected");
                }

                // Check if this is one of our hotkeys
                let binding_opt = binding_map.get(&key).cloned();
                let is_enter = key == Key::KEY_ENTER;
                let is_escape = key == Key::KEY_ESC;

                match (&state, pressed, released, &binding_opt) {
                    // IDLE + Hotkey pressed with modifiers -> Start recording
                    (RecordingState::Idle, true, _, Some(binding))
                        if modifiers.matches_config(&config.hotkey.modifiers) =>
                    {
                        eprintln!("[TRANSITION] Idle -> Recording (hotkey {:?} pressed)", key);
                        match start_recording(&config) {
                            Ok(_) => {
                                state = RecordingState::Recording {
                                    press_time: Instant::now(),
                                    active: ActiveBinding {
                                        key,
                                        binding: binding.clone(),
                                    },
                                };
                            }
                            Err(e) => {
                                eprintln!("[ERROR] Failed to start recording: {}", e);
                            }
                        }
                    }

                    // RECORDING + Same hotkey released -> Check tap vs hold
                    (RecordingState::Recording { press_time, active }, _, true, _)
                        if key == active.key =>
                    {
                        let duration = press_time.elapsed();
                        eprintln!("[TRANSITION] Recording: hotkey released after {:?}ms", duration.as_millis());

                        if duration < tap_threshold {
                            // Tap detected - enter pending grab state
                            eprintln!("[TRANSITION] Recording -> PendingGrab (tap detected)");
                            state = RecordingState::PendingGrab {
                                active: active.clone(),
                            };
                        } else {
                            // Hold detected - normal push-to-talk, process immediately
                            eprintln!("[TRANSITION] Recording -> Idle (hold detected, processing)");
                            let config_clone = config.clone();
                            let prompt_clone = active.binding.prompt.clone();
                            match process_transcription(&config_clone, prompt_clone.as_deref()) {
                                Ok(_) => eprintln!("[OK] Transcription processed"),
                                Err(e) => eprintln!("[ERROR] Transcription failed: {}", e),
                            }
                            state = RecordingState::Idle;
                        }
                    }

                    // PENDING GRAB + All keys released -> Grab and enter LongRecording
                    (RecordingState::PendingGrab { active }, _, _, _) if all_keys_released => {
                        eprintln!("[TRANSITION] PendingGrab -> LongRecording (grabbing keyboard)");
                        if let Err(e) = device.grab() {
                            eprintln!("[ERROR] Failed to grab keyboard: {}", e);
                            // Fall back to normal recording without grab
                            state = RecordingState::LongRecording {
                                active: active.clone(),
                            };
                        } else {
                            is_grabbed = true;
                            println!(">>> GRABBED - Keyboard captured <<<");
                            println!(">>> Enter=submit+continue, Escape=cancel, Hotkey=finish <<<");
                            state = RecordingState::LongRecording {
                                active: active.clone(),
                            };
                        }
                    }

                    // LONG RECORDING + Enter pressed -> Submit + Continue
                    (RecordingState::LongRecording { active }, true, _, _) if is_enter => {
                        eprintln!("[TRANSITION] LongRecording: Enter pressed - submit + continue");

                        // IMPORTANT: Ungrab before sending keys via ydotool, otherwise
                        // the simulated Ctrl+V and Enter will be captured by our grab!
                        if is_grabbed {
                            eprintln!("[INFO] Temporarily ungrabbing for paste+enter");
                            if let Err(e) = device.ungrab() {
                                eprintln!("[ERROR] Failed to ungrab: {}", e);
                            } else {
                                is_grabbed = false;
                            }
                        }

                        // Process current transcription and send Enter
                        let config_clone = config.clone();
                        let prompt_clone = active.binding.prompt.clone();

                        match process_transcription_and_send_enter(&config_clone, prompt_clone.as_deref()) {
                            Ok(_) => {
                                eprintln!("[OK] Submitted, restarting recording");
                            }
                            Err(e) => {
                                eprintln!("[ERROR] Submit+continue failed: {}", e);
                            }
                        }

                        // Start a new recording segment
                        if let Err(e) = start_recording(&config) {
                            eprintln!("[ERROR] Failed to restart recording: {}", e);
                        }

                        // Small delay to let ydotool events complete
                        std::thread::sleep(Duration::from_millis(100));

                        // Drain any pending events before re-grabbing
                        if let Ok(events) = device.fetch_events() {
                            let count = events.count();
                            if count > 0 {
                                eprintln!("[INFO] Drained {} pending events", count);
                            }
                        }

                        // Re-grab the keyboard for continued long recording
                        eprintln!("[INFO] Re-grabbing keyboard");
                        if let Err(e) = device.grab() {
                            eprintln!("[ERROR] Failed to re-grab: {}", e);
                        } else {
                            is_grabbed = true;
                        }

                        // Stay in LongRecording state with same active binding
                    }

                    // LONG RECORDING + Escape pressed -> Cancel
                    (RecordingState::LongRecording { active }, true, _, _) if is_escape => {
                        eprintln!("[TRANSITION] LongRecording -> PendingUngrab (Escape pressed)");
                        match cancel_recording(&config) {
                            Ok(_) => eprintln!("[OK] Recording cancelled"),
                            Err(e) => eprintln!("[ERROR] Failed to cancel: {}", e),
                        }
                        // Enter pending ungrab without transcription
                        state = RecordingState::PendingUngrab {
                            should_transcribe: false,
                            active: active.clone(),
                        };
                    }

                    // LONG RECORDING + Same hotkey pressed -> Finish (pending ungrab)
                    (RecordingState::LongRecording { active }, true, _, Some(_))
                        if key == active.key =>
                    {
                        eprintln!("[TRANSITION] LongRecording -> PendingUngrab (hotkey pressed to finish)");
                        state = RecordingState::PendingUngrab {
                            should_transcribe: true,
                            active: active.clone(),
                        };
                    }

                    // PENDING UNGRAB: Ungrab immediately, don't wait for all keys released
                    // This prevents getting stuck when user presses keys frantically
                    (RecordingState::PendingUngrab { should_transcribe, active }, _, true, _) => {
                        // On any key release, check if we can ungrab
                        // We ungrab immediately - the "wait for all keys" was causing stuck state
                        eprintln!("[TRANSITION] PendingUngrab -> Idle (ungrabbing keyboard)");
                        if is_grabbed {
                            if let Err(e) = device.ungrab() {
                                eprintln!("[ERROR] Failed to ungrab keyboard: {}", e);
                            } else {
                                is_grabbed = false;
                                println!(">>> RELEASED - Keyboard ungrabbed <<<");
                            }
                        }

                        if *should_transcribe {
                            eprintln!("[INFO] Processing final transcription...");
                            let config_clone = config.clone();
                            let prompt_clone = active.binding.prompt.clone();
                            match process_transcription(&config_clone, prompt_clone.as_deref()) {
                                Ok(_) => eprintln!("[OK] Transcription processed"),
                                Err(e) => eprintln!("[ERROR] Transcription failed: {}", e),
                            }
                        }

                        state = RecordingState::Idle;
                        // Clear key tracking to avoid stale state
                        key_pressed.clear();
                        modifiers = ModifierState::default();
                    }

                    // Log unhandled key events in LongRecording
                    (RecordingState::LongRecording { .. }, true, _, _) => {
                        eprintln!("[IGNORED] Key {:?} pressed in LongRecording (not Enter/Escape/Hotkey)", key);
                    }

                    // Ignore other combinations
                    _ => {}
                }
            }
        }
    }

    // Cleanup on exit
    if is_grabbed {
        eprintln!("[CLEANUP] Ungrabbing keyboard on exit");
        device.ungrab().ok();
    }

    if !matches!(state, RecordingState::Idle) {
        eprintln!("[CLEANUP] Cleaning up recording state on exit");
        cancel_recording(&config).ok();
    }

    eprintln!("[INFO] Hotkey daemon stopped");
    Ok(())
}
