//! Hotkey daemon for push-to-talk transcription using Hyprland GlobalShortcuts portal
//!
//! This daemon uses the xdg-desktop-portal GlobalShortcuts D-Bus interface to register
//! hotkeys with the Hyprland compositor. This approach:
//! - Has the compositor consume hotkey combos completely (no modifier leaks)
//! - Doesn't require evdev grabbing or dealing with per-device modifier state
//! - Provides clean pressed/released events via D-Bus signals
//!
//! User must configure bindings in hyprland.conf:
//!   bind = SUPER+SHIFT+CTRL+ALT, E, global, transcribe:transcribe-e
//!   bind = SUPER+SHIFT+CTRL+ALT, Q, global, transcribe:transcribe-q
//!   etc.

use evdev::Key;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use transcribe_rs::{
    clipboard, config::Config, eww_widget, paste, recording,
    transcription_corrections::TranscriptionCorrector, transcription_timing,
};
use zbus::{proxy, Connection};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use transcribe_rs::config::HotkeyBinding;
use transcribe_rs::recording::RecordingResult;

/// D-Bus proxy for the GlobalShortcuts portal
#[proxy(
    interface = "org.freedesktop.portal.GlobalShortcuts",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait GlobalShortcuts {
    /// Create a new shortcuts session
    fn create_session(
        &self,
        options: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<OwnedObjectPath>;

    /// Bind shortcuts to the session
    fn bind_shortcuts(
        &self,
        session_handle: &ObjectPath<'_>,
        shortcuts: Vec<(&str, HashMap<&str, Value<'_>>)>,
        parent_window: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<OwnedObjectPath>;

    /// Activated signal - shortcut was pressed
    #[zbus(signal)]
    fn activated(
        &self,
        session_handle: ObjectPath<'_>,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;

    /// Deactivated signal - shortcut was released
    #[zbus(signal)]
    fn deactivated(
        &self,
        session_handle: ObjectPath<'_>,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;
}

/// D-Bus proxy for a portal Request
#[proxy(
    interface = "org.freedesktop.portal.Request",
    default_service = "org.freedesktop.portal.Desktop"
)]
trait Request {
    /// Response signal - indicates the request completed
    #[zbus(signal)]
    fn response(
        &self,
        response: u32,
        results: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;

    /// Close the request
    fn close(&self) -> zbus::Result<()>;
}

/// Events from D-Bus signals
#[derive(Debug, Clone)]
enum ShortcutEvent {
    Activated { shortcut_id: String, timestamp: u64 },
    Deactivated { shortcut_id: String, timestamp: u64 },
}

/// Active binding info stored during recording
#[derive(Debug, Clone)]
struct ActiveBinding {
    shortcut_id: String,
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
    /// Long recording mode (tapped hotkey)
    LongRecording {
        active: ActiveBinding,
        entered_at: Instant, // When we entered long recording (for double-tap detection)
    },
    /// Waiting for key release before repasting (double-tap detected)
    PendingRepaste {
        shortcut_id: String,
        text: String,
    },
    /// Waiting for key release before transcribing (long recording finish)
    PendingTranscription {
        shortcut_id: String,
        prompt: Option<String>,
    },
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

/// Check if any modifier keys are currently pressed on any keyboard device
fn are_modifiers_pressed() -> bool {
    let modifier_keys = [
        Key::KEY_LEFTCTRL,
        Key::KEY_RIGHTCTRL,
        Key::KEY_LEFTSHIFT,
        Key::KEY_RIGHTSHIFT,
        Key::KEY_LEFTALT,
        Key::KEY_RIGHTALT,
        Key::KEY_LEFTMETA,  // Super/Win key
        Key::KEY_RIGHTMETA,
    ];

    // Enumerate all input devices
    for (_path, device) in evdev::enumerate() {
        // Only check devices that have keys (keyboards)
        if let Some(supported_keys) = device.supported_keys() {
            // Check if this device supports any modifier keys
            let has_modifiers = modifier_keys.iter().any(|k| supported_keys.contains(*k));
            if !has_modifiers {
                continue;
            }

            // Get current key state
            if let Ok(key_state) = device.get_key_state() {
                for key in &modifier_keys {
                    if key_state.contains(*key) {
                        debug!("Modifier {:?} is pressed on {:?}", key, device.name());
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Wait for all modifier keys to be released before proceeding
fn wait_for_modifiers_released() {
    let timeout = Duration::from_secs(5);
    let poll_interval = Duration::from_millis(20);
    let start = Instant::now();

    while are_modifiers_pressed() {
        if start.elapsed() > timeout {
            warn!("Timeout waiting for modifiers to be released, proceeding anyway");
            break;
        }
        std::thread::sleep(poll_interval);
    }

    debug!("All modifiers released after {}ms", start.elapsed().as_millis());
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
        // Wait for all modifier keys to be released before pasting
        // This prevents Ctrl+V from combining with held modifiers
        debug!("Waiting for modifiers to be released before paste...");
        wait_for_modifiers_released();

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

/// Bind Enter key as a global shortcut via hyprctl (for long recording mode)
fn bind_enter_key() {
    debug!("Binding Enter key as global shortcut");
    match Command::new("hyprctl")
        .args(["keyword", "bind", ",Return,global,:transcribe-enter"])
        .status()
    {
        Ok(status) if status.success() => {
            eprintln!("[HYPRCTL] Bound Enter key as global shortcut");
        }
        Ok(_) => {
            warn!("hyprctl bind command failed");
        }
        Err(e) => {
            warn!("Failed to execute hyprctl: {}", e);
        }
    }
}

/// Unbind Enter key global shortcut via hyprctl
fn unbind_enter_key() {
    debug!("Unbinding Enter key global shortcut");
    match Command::new("hyprctl")
        .args(["keyword", "unbind", ",Return"])
        .status()
    {
        Ok(status) if status.success() => {
            eprintln!("[HYPRCTL] Unbound Enter key");
        }
        Ok(_) => {
            warn!("hyprctl unbind command failed");
        }
        Err(e) => {
            warn!("Failed to execute hyprctl: {}", e);
        }
    }
}

/// Bind Escape key as a global shortcut via hyprctl (for cancelling long recording)
fn bind_escape_key() {
    debug!("Binding Escape key as global shortcut");
    match Command::new("hyprctl")
        .args(["keyword", "bind", ",Escape,global,:transcribe-escape"])
        .status()
    {
        Ok(status) if status.success() => {
            eprintln!("[HYPRCTL] Bound Escape key as global shortcut");
        }
        Ok(_) => {
            warn!("hyprctl bind command failed for Escape");
        }
        Err(e) => {
            warn!("Failed to execute hyprctl: {}", e);
        }
    }
}

/// Unbind Escape key global shortcut via hyprctl
fn unbind_escape_key() {
    debug!("Unbinding Escape key global shortcut");
    match Command::new("hyprctl")
        .args(["keyword", "unbind", ",Escape"])
        .status()
    {
        Ok(status) if status.success() => {
            eprintln!("[HYPRCTL] Unbound Escape key");
        }
        Ok(_) => {
            warn!("hyprctl unbind command failed for Escape");
        }
        Err(e) => {
            warn!("Failed to execute hyprctl: {}", e);
        }
    }
}

/// Bind both Enter and Escape keys for long recording mode
fn bind_long_recording_keys() {
    bind_enter_key();
    bind_escape_key();
}

/// Unbind both Enter and Escape keys when exiting long recording mode
fn unbind_long_recording_keys() {
    unbind_enter_key();
    unbind_escape_key();
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
/// Returns the final transcribed text on success (for repaste feature)
fn process_transcription(config: &Config, prompt_name: Option<&str>) -> Result<String, Box<dyn Error>> {
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
        return Ok(llm_result.text);
    }

    copy_and_paste(&llm_result.text, config)?;

    info!("Transcription complete: {}", llm_result.text);
    Ok(llm_result.text)
}

/// Process transcription and send Enter (for submit+continue flow)
fn process_transcription_and_send_enter(config: &Config, prompt_name: Option<&str>) -> Result<(), Box<dyn Error>> {
    // Process the transcription normally
    process_transcription(config, prompt_name)?;

    // Send Enter key
    send_enter_key()?;

    Ok(())
}

/// Generate shortcut ID from binding key
fn shortcut_id_from_key(key: &str) -> String {
    format!("transcribe-{}", key.to_lowercase())
}

/// Extract key from shortcut ID
fn key_from_shortcut_id(shortcut_id: &str) -> Option<String> {
    shortcut_id.strip_prefix("transcribe-").map(|s| s.to_string())
}

/// Get the sender name for constructing request object paths
fn get_sender_name(connection: &Connection) -> Result<String, Box<dyn Error>> {
    let unique_name = connection.unique_name()
        .ok_or("Connection has no unique name")?;
    // Convert :1.234 to 1_234
    Ok(unique_name.as_str().trim_start_matches(':').replace('.', "_"))
}

/// Response data extracted from signal
struct PortalResponse {
    response: u32,
    results: HashMap<String, OwnedValue>,
}

/// Create a Request proxy and subscribe to Response signal BEFORE making a call
/// Returns a receiver that will get the response
async fn prepare_response_listener(
    connection: &Connection,
    sender: &str,
    token: &str,
) -> Result<(OwnedObjectPath, tokio::sync::oneshot::Receiver<PortalResponse>), Box<dyn Error>> {
    use futures::StreamExt;

    // Predict the request path based on sender and token
    let request_path_str = format!("/org/freedesktop/portal/desktop/request/{}/{}", sender, token);
    let request_path = OwnedObjectPath::try_from(request_path_str)?;

    eprintln!("[DBUS] Pre-subscribing to response on {:?}", request_path);

    let proxy = RequestProxy::builder(connection)
        .path(request_path.clone())?
        .build()
        .await?;

    let mut stream = proxy.receive_response().await?;

    // Create a channel to forward the response
    let (tx, rx) = tokio::sync::oneshot::channel();

    // Spawn a task to wait for the signal and forward it
    tokio::spawn(async move {
        if let Some(signal) = stream.next().await {
            if let Ok(args) = signal.args() {
                let _ = tx.send(PortalResponse {
                    response: args.response,
                    results: args.results.clone(),
                });
            }
        }
    });

    Ok((request_path, rx))
}

/// Wait for a response on an already-subscribed receiver
async fn wait_for_response_on_receiver(
    rx: tokio::sync::oneshot::Receiver<PortalResponse>,
) -> Result<HashMap<String, OwnedValue>, Box<dyn Error>> {
    // Wait for response with timeout
    let response = tokio::time::timeout(Duration::from_secs(30), rx)
        .await
        .map_err(|_| "Timeout waiting for portal response")?
        .map_err(|_| "Response channel closed")?;

    eprintln!("[DBUS] Got response: code={}", response.response);

    match response.response {
        0 => Ok(response.results),
        1 => Err("User cancelled".into()),
        _ => Err(format!("Request failed with code {}", response.response).into()),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    if let Err(e) = transcribe_rs::logging::init() {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    }

    eprintln!("[STARTUP] Starting hotkey daemon with GlobalShortcuts portal...");

    // Load configuration
    let config = Config::load()?;
    info!("Configuration loaded");

    // Build binding map: shortcut_id -> HotkeyBinding
    let mut binding_map: HashMap<String, HotkeyBinding> = HashMap::new();
    println!("Registering {} hotkey binding(s) via GlobalShortcuts portal:", config.hotkey.bindings.len());
    for binding in &config.hotkey.bindings {
        let shortcut_id = shortcut_id_from_key(&binding.key);
        binding_map.insert(shortcut_id.clone(), binding.clone());
        println!(
            "  - {}: prompt={:?}",
            shortcut_id,
            binding.prompt,
        );
    }

    if binding_map.is_empty() {
        return Err("No valid hotkey bindings configured".into());
    }

    // Connect to D-Bus session bus
    eprintln!("[DBUS] Connecting to session bus...");
    let connection = Connection::session().await?;
    let sender = get_sender_name(&connection)?;
    eprintln!("[DBUS] Connected to session bus (sender: {})", sender);

    // Create GlobalShortcuts proxy
    let proxy = GlobalShortcutsProxy::new(&connection).await?;
    eprintln!("[DBUS] Created GlobalShortcuts proxy");

    // Generate unique token for the session request
    let session_token = format!("transcribe_{}", std::process::id());

    // IMPORTANT: Subscribe to response signal BEFORE making the call
    // Otherwise the signal arrives before we're listening and we miss it
    let (_expected_path, response_stream) = prepare_response_listener(
        &connection,
        &sender,
        &session_token,
    ).await?;

    // Create a session - this returns a request handle, not the session
    let mut session_options: HashMap<&str, Value<'_>> = HashMap::new();
    session_options.insert("handle_token", Value::from(session_token.as_str()));
    session_options.insert("session_handle_token", Value::from("transcribe"));

    eprintln!("[DBUS] Creating session (token: {})...", session_token);
    let request_path = proxy.create_session(session_options).await?;
    eprintln!("[DBUS] Got request handle: {:?}", request_path);

    // Wait for the response to get the actual session handle
    let response_results = wait_for_response_on_receiver(response_stream).await?;

    // Extract session_handle from response
    let session_handle_value = response_results.get("session_handle")
        .ok_or("Response missing session_handle")?;
    let session_handle_str: &str = session_handle_value.downcast_ref()
        .map_err(|e| format!("session_handle is not a string: {}", e))?;
    let session_handle = ObjectPath::try_from(session_handle_str)?;
    eprintln!("[DBUS] Session created: {:?}", session_handle);

    // Prepare shortcuts to bind - store descriptions separately to avoid lifetime issues
    let mut shortcut_ids: Vec<String> = binding_map.keys().cloned().collect();
    // Add transcribe-enter and transcribe-escape for dynamic key binding during long recording
    shortcut_ids.push("transcribe-enter".to_string());
    shortcut_ids.push("transcribe-escape".to_string());

    let descriptions: Vec<String> = shortcut_ids.iter()
        .map(|id| {
            if id == "transcribe-enter" {
                "Enter key for submit during long recording".to_string()
            } else if id == "transcribe-escape" {
                "Escape key for cancel during long recording".to_string()
            } else {
                format!("Transcribe hotkey for key {}",
                    key_from_shortcut_id(id).unwrap_or_default())
            }
        })
        .collect();

    let mut shortcuts: Vec<(&str, HashMap<&str, Value<'_>>)> = Vec::new();
    for (shortcut_id, description) in shortcut_ids.iter().zip(descriptions.iter()) {
        let mut shortcut_options: HashMap<&str, Value<'_>> = HashMap::new();
        shortcut_options.insert("description", Value::from(description.as_str()));
        shortcuts.push((shortcut_id.as_str(), shortcut_options));
    }

    // Bind shortcuts - subscribe to response BEFORE calling
    let bind_token = format!("bind_{}", std::process::id());

    let (_bind_expected_path, bind_response_stream) = prepare_response_listener(
        &connection,
        &sender,
        &bind_token,
    ).await?;

    let mut bind_options: HashMap<&str, Value<'_>> = HashMap::new();
    bind_options.insert("handle_token", Value::from(bind_token.as_str()));

    eprintln!("[DBUS] Binding {} shortcuts...", shortcuts.len());
    let _bind_request = proxy.bind_shortcuts(
        &session_handle,
        shortcuts,
        "",
        bind_options,
    ).await?;

    // Wait for bind response
    let _bind_results = wait_for_response_on_receiver(bind_response_stream).await?;
    eprintln!("[DBUS] Shortcuts bound successfully");

    // Setup signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        info!("Received shutdown signal");
        r.store(false, Ordering::SeqCst);
    })?;

    // Create channel for D-Bus events
    let (tx, mut rx) = mpsc::channel::<ShortcutEvent>(100);

    // Subscribe to Activated signal
    let tx_activated = tx.clone();
    let mut activated_stream = proxy.receive_activated().await?;
    let activated_handle = tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(signal) = activated_stream.next().await {
            if let Ok(args) = signal.args() {
                let event = ShortcutEvent::Activated {
                    shortcut_id: args.shortcut_id.to_string(),
                    timestamp: args.timestamp,
                };
                if tx_activated.send(event).await.is_err() {
                    break;
                }
            }
        }
    });

    // Subscribe to Deactivated signal
    let tx_deactivated = tx;
    let mut deactivated_stream = proxy.receive_deactivated().await?;
    let deactivated_handle = tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(signal) = deactivated_stream.next().await {
            if let Ok(args) = signal.args() {
                let event = ShortcutEvent::Deactivated {
                    shortcut_id: args.shortcut_id.to_string(),
                    timestamp: args.timestamp,
                };
                if tx_deactivated.send(event).await.is_err() {
                    break;
                }
            }
        }
    });

    // State machine
    let mut state = RecordingState::Idle;
    let tap_threshold = Duration::from_millis(config.hotkey.tap_threshold_ms);
    let double_tap_window = Duration::from_millis(1500); // Window for double-tap repaste
    let mut last_transcription: Option<String> = None;
    let mut last_repaste_time: Option<Instant> = None;
    let repaste_debounce = Duration::from_millis(1000); // Ignore events shortly after repaste

    println!();
    println!("Hotkey daemon running with GlobalShortcuts portal.");
    println!("Press Ctrl+C to stop.");
    println!();
    println!("IMPORTANT: You must configure bindings in hyprland.conf:");
    println!("  bind = SUPER+SHIFT+CTRL+ALT, E, global, transcribe:transcribe-e");
    println!("  bind = SUPER+SHIFT+CTRL+ALT, Q, global, transcribe:transcribe-q");
    println!("  (etc. for each key in your config)");
    println!();
    println!("Tap threshold: {}ms", config.hotkey.tap_threshold_ms);
    println!();
    println!("Long recording (tap to toggle):");
    println!("  - Tap same key again: FINISH recording");
    println!("  - Tap different key: Cancel and switch to that prompt");
    println!();

    // Main event loop
    while running.load(Ordering::SeqCst) {
        // Use timeout to check running flag periodically
        let event = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;

        let event = match event {
            Ok(Some(e)) => e,
            Ok(None) => {
                // Channel closed
                break;
            }
            Err(_) => {
                // Timeout, check running flag and continue
                continue;
            }
        };

        eprintln!("[EVENT] {:?} | state={:?}", event, state);

        match (&state, &event) {
            // IDLE + Activated -> Start recording (with debounce check for repaste)
            (RecordingState::Idle, ShortcutEvent::Activated { shortcut_id, .. }) => {
                // Check if we should ignore this event due to recent repaste
                if let Some(repaste_time) = last_repaste_time {
                    if repaste_time.elapsed() < repaste_debounce {
                        eprintln!("[DEBOUNCE] Ignoring activation within {}ms of repaste", repaste_time.elapsed().as_millis());
                        continue;
                    }
                }

                if let Some(binding) = binding_map.get(shortcut_id) {
                    eprintln!("[TRANSITION] Idle -> Recording (shortcut {} activated)", shortcut_id);
                    match start_recording(&config) {
                        Ok(_) => {
                            state = RecordingState::Recording {
                                press_time: Instant::now(),
                                active: ActiveBinding {
                                    shortcut_id: shortcut_id.clone(),
                                    binding: binding.clone(),
                                },
                            };
                        }
                        Err(e) => {
                            eprintln!("[ERROR] Failed to start recording: {}", e);
                        }
                    }
                } else {
                    eprintln!("[WARN] Unknown shortcut: {}", shortcut_id);
                }
            }

            // RECORDING + Deactivated (same shortcut) -> Check quick-tap (repaste) vs tap vs hold
            (RecordingState::Recording { press_time, active }, ShortcutEvent::Deactivated { shortcut_id, .. })
                if shortcut_id == &active.shortcut_id =>
            {
                let duration = press_time.elapsed();
                eprintln!("[TRANSITION] Recording: shortcut released after {:?}ms", duration.as_millis());

                if duration < tap_threshold {
                    // Tap detected - enter long recording mode
                    eprintln!("[TRANSITION] Recording -> LongRecording (tap < {}ms)", tap_threshold.as_millis());
                    println!(">>> LONG RECORDING MODE (tap again within 1.5s to repaste) <<<");
                    bind_long_recording_keys(); // Capture Enter and Escape keys during long recording
                    state = RecordingState::LongRecording {
                        active: active.clone(),
                        entered_at: Instant::now(),
                    };
                } else {
                    // Hold detected - normal push-to-talk, process immediately
                    eprintln!("[TRANSITION] Recording -> Idle (hold detected, processing)");
                    let config_clone = config.clone();
                    let prompt_clone = active.binding.prompt.clone();
                    match process_transcription(&config_clone, prompt_clone.as_deref()) {
                        Ok(text) => {
                            last_transcription = Some(text);
                            eprintln!("[OK] Transcription processed and saved");
                        }
                        Err(e) => eprintln!("[ERROR] Transcription failed: {}", e),
                    }
                    state = RecordingState::Idle;
                }
            }

            // LONG RECORDING + Activated (same shortcut) -> Check for double-tap (repaste) or finish
            (RecordingState::LongRecording { active, entered_at }, ShortcutEvent::Activated { shortcut_id, .. })
                if shortcut_id == &active.shortcut_id =>
            {
                let time_in_long_recording = entered_at.elapsed();

                if time_in_long_recording < double_tap_window {
                    // Double-tap detected! Cancel recording and wait for key release before repasting
                    eprintln!("[TRANSITION] LongRecording -> PendingRepaste (double-tap within {}ms)", time_in_long_recording.as_millis());
                    unbind_long_recording_keys(); // Release Enter and Escape keys

                    // Cancel the recording since we're repasting instead
                    if let Err(e) = cancel_recording(&config) {
                        eprintln!("[WARN] Failed to cancel recording: {}", e);
                    }

                    if let Some(ref text) = last_transcription {
                        // Transition to PendingRepaste - will paste when key is released
                        state = RecordingState::PendingRepaste {
                            shortcut_id: shortcut_id.clone(),
                            text: text.clone(),
                        };
                        eprintln!("[PENDING] Waiting for key release before repasting...");
                    } else {
                        eprintln!("[INFO] No previous transcription to repaste");
                        use transcribe_rs::notifications;
                        notifications::notify("No previous transcription", "Record something first", 2000).ok();
                        state = RecordingState::Idle;
                    }
                } else {
                    // Not a double-tap - defer transcription until key release
                    eprintln!("[TRANSITION] LongRecording -> PendingTranscription (waiting for key release)");
                    unbind_long_recording_keys(); // Release Enter and Escape keys
                    state = RecordingState::PendingTranscription {
                        shortcut_id: shortcut_id.clone(),
                        prompt: active.binding.prompt.clone(),
                    };
                }
            }

            // LONG RECORDING + Enter key (transcribe-enter) -> Submit, send Enter, restart recording
            // NOTE: This MUST come BEFORE the "DIFFERENT shortcut" catch-all arm below!
            (RecordingState::LongRecording { active, .. }, ShortcutEvent::Activated { shortcut_id, .. })
                if shortcut_id == "transcribe-enter" =>
            {
                eprintln!("[TRANSITION] LongRecording: Enter pressed - submit and continue");
                println!(">>> ENTER: Submit and continue <<<");

                let prompt_clone = active.binding.prompt.clone();
                let binding_clone = active.clone();

                // Unbind keys BEFORE processing so the synthetic Enter reaches the app
                unbind_long_recording_keys();

                // Process transcription, paste, and send Enter
                match process_transcription_and_send_enter(&config, prompt_clone.as_deref()) {
                    Ok(_) => {
                        eprintln!("[OK] Transcription submitted with Enter");
                    }
                    Err(e) => {
                        eprintln!("[ERROR] Submit failed: {}", e);
                    }
                }

                // Immediately start new recording with the same binding
                match start_recording(&config) {
                    Ok(_) => {
                        eprintln!("[TRANSITION] Restarted recording after Enter");
                        // Rebind Enter and Escape for continued long recording
                        bind_long_recording_keys();
                        state = RecordingState::LongRecording {
                            active: binding_clone,
                            entered_at: Instant::now(),
                        };
                    }
                    Err(e) => {
                        eprintln!("[ERROR] Failed to restart recording: {}", e);
                        // Enter already unbound, stay in Idle
                        state = RecordingState::Idle;
                    }
                }
            }

            // LONG RECORDING + Escape key (transcribe-escape) -> Cancel and return to Idle
            // NOTE: This MUST come BEFORE the "DIFFERENT shortcut" catch-all arm below!
            (RecordingState::LongRecording { .. }, ShortcutEvent::Activated { shortcut_id, .. })
                if shortcut_id == "transcribe-escape" =>
            {
                eprintln!("[TRANSITION] LongRecording: Escape pressed - cancelling");
                println!(">>> ESCAPE: Cancelling recording <<<");

                unbind_long_recording_keys();

                if let Err(e) = cancel_recording(&config) {
                    eprintln!("[WARN] Failed to cancel recording: {}", e);
                }

                use transcribe_rs::notifications;
                notifications::notify("Recording cancelled", "Press hotkey to start again", 2000).ok();

                state = RecordingState::Idle;
            }

            // LONG RECORDING + Activated (DIFFERENT shortcut) -> Cancel current, start new with different prompt
            (RecordingState::LongRecording { active: _, entered_at: _ }, ShortcutEvent::Activated { shortcut_id, .. }) => {
                // Different key pressed - cancel current recording, start fresh with new binding
                if let Some(new_binding) = binding_map.get(shortcut_id) {
                    eprintln!("[TRANSITION] LongRecording: switching to different key {}", shortcut_id);
                    unbind_long_recording_keys(); // Release Enter and Escape keys
                    // Cancel current recording
                    if let Err(e) = cancel_recording(&config) {
                        eprintln!("[WARN] Failed to cancel recording: {}", e);
                    }
                    // Start new recording with the new binding
                    match start_recording(&config) {
                        Ok(_) => {
                            state = RecordingState::Recording {
                                press_time: Instant::now(),
                                active: ActiveBinding {
                                    shortcut_id: shortcut_id.clone(),
                                    binding: new_binding.clone(),
                                },
                            };
                        }
                        Err(e) => {
                            eprintln!("[ERROR] Failed to start new recording: {}", e);
                            state = RecordingState::Idle;
                        }
                    }
                }
            }

            // Ignore deactivated in LongRecording
            (RecordingState::LongRecording { .. }, ShortcutEvent::Deactivated { .. }) => {
                // Ignore release events in long recording mode
            }

            // PENDING REPASTE + Deactivated -> Now safe to paste (keys released)
            (RecordingState::PendingRepaste { shortcut_id: pending_id, text }, ShortcutEvent::Deactivated { shortcut_id, .. })
                if shortcut_id == pending_id =>
            {
                eprintln!("[TRANSITION] PendingRepaste -> Idle (key released, waiting for modifiers)");

                // copy_and_paste will wait for modifiers to be released
                match copy_and_paste(text, &config) {
                    Ok(_) => {
                        use transcribe_rs::notifications;
                        let preview = if text.len() > 50 {
                            format!("{}...", &text[..50])
                        } else {
                            text.clone()
                        };
                        notifications::notify("Repasted", &preview, 2000).ok();
                        eprintln!("[OK] Repaste successful");
                        last_repaste_time = Some(Instant::now());
                    }
                    Err(e) => eprintln!("[ERROR] Repaste failed: {}", e),
                }
                state = RecordingState::Idle;
                println!(">>> DOUBLE-TAP REPASTE <<<");
            }

            // PENDING TRANSCRIPTION + Deactivated -> Now safe to transcribe and paste (keys released)
            (RecordingState::PendingTranscription { shortcut_id: pending_id, prompt }, ShortcutEvent::Deactivated { shortcut_id, .. })
                if shortcut_id == pending_id =>
            {
                eprintln!("[TRANSITION] PendingTranscription -> Idle (key released, processing)");

                // process_transcription -> copy_and_paste will wait for modifiers
                let config_clone = config.clone();
                match process_transcription(&config_clone, prompt.as_deref()) {
                    Ok(text) => {
                        last_transcription = Some(text);
                        eprintln!("[OK] Transcription processed and saved");
                    }
                    Err(e) => eprintln!("[ERROR] Transcription failed: {}", e),
                }
                state = RecordingState::Idle;
                println!(">>> LONG RECORDING ENDED <<<");
            }

            // Ignore other events in PendingRepaste (e.g., different key)
            (RecordingState::PendingRepaste { .. }, _) => {
                eprintln!("[IGNORED] Event while waiting for repaste key release");
            }

            // Ignore other events in PendingTranscription (e.g., different key)
            (RecordingState::PendingTranscription { .. }, _) => {
                eprintln!("[IGNORED] Event while waiting for transcription key release");
            }

            // Ignore other combinations
            _ => {
                eprintln!("[IGNORED] Event in unexpected state");
            }
        }
    }

    // Cleanup
    eprintln!("[CLEANUP] Shutting down...");
    activated_handle.abort();
    deactivated_handle.abort();

    // Unbind Enter and Escape keys if we were in long recording mode
    if matches!(state, RecordingState::LongRecording { .. }) {
        unbind_long_recording_keys();
    }

    if !matches!(state, RecordingState::Idle) {
        eprintln!("[CLEANUP] Cleaning up recording state on exit");
        cancel_recording(&config).ok();
    }

    eprintln!("[INFO] Hotkey daemon stopped");
    Ok(())
}
