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
    LongRecording { active: ActiveBinding },
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
    let shortcut_ids: Vec<String> = binding_map.keys().cloned().collect();
    let descriptions: Vec<String> = shortcut_ids.iter()
        .map(|id| format!("Transcribe hotkey for key {}",
            key_from_shortcut_id(id).unwrap_or_default()))
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
    let mut last_enter_time: Option<Instant> = None;
    let enter_debounce = Duration::from_millis(500);

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
            // IDLE + Activated -> Start recording
            (RecordingState::Idle, ShortcutEvent::Activated { shortcut_id, .. }) => {
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

            // RECORDING + Deactivated (same shortcut) -> Check tap vs hold
            (RecordingState::Recording { press_time, active }, ShortcutEvent::Deactivated { shortcut_id, .. })
                if shortcut_id == &active.shortcut_id =>
            {
                let duration = press_time.elapsed();
                eprintln!("[TRANSITION] Recording: shortcut released after {:?}ms", duration.as_millis());

                if duration < tap_threshold {
                    // Tap detected - enter long recording mode
                    eprintln!("[TRANSITION] Recording -> LongRecording (tap detected)");
                    println!(">>> LONG RECORDING MODE <<<");
                    println!(">>> Press hotkey again to submit+continue or finish <<<");
                    state = RecordingState::LongRecording {
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

            // LONG RECORDING + Activated (same shortcut) -> FINISH (toggle off)
            (RecordingState::LongRecording { active }, ShortcutEvent::Activated { shortcut_id, .. })
                if shortcut_id == &active.shortcut_id =>
            {
                // Same key pressed again - finish recording (toggle behavior)
                eprintln!("[TRANSITION] LongRecording -> Idle (same key pressed, finishing)");
                let config_clone = config.clone();
                let prompt_clone = active.binding.prompt.clone();
                match process_transcription(&config_clone, prompt_clone.as_deref()) {
                    Ok(_) => eprintln!("[OK] Transcription processed"),
                    Err(e) => eprintln!("[ERROR] Transcription failed: {}", e),
                }
                state = RecordingState::Idle;
                println!(">>> LONG RECORDING ENDED <<<");
            }

            // LONG RECORDING + Activated (DIFFERENT shortcut) -> Cancel current, start new with different prompt
            (RecordingState::LongRecording { active: _ }, ShortcutEvent::Activated { shortcut_id, .. }) => {
                // Different key pressed - cancel current recording, start fresh with new binding
                if let Some(new_binding) = binding_map.get(shortcut_id) {
                    eprintln!("[TRANSITION] LongRecording: switching to different key {}", shortcut_id);
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

    if !matches!(state, RecordingState::Idle) {
        eprintln!("[CLEANUP] Cleaning up recording state on exit");
        cancel_recording(&config).ok();
    }

    eprintln!("[INFO] Hotkey daemon stopped");
    Ok(())
}
