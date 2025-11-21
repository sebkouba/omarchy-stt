use super::state::ConversationState;
use eframe::egui;
use log::{debug, warn};
use notify::{Watcher, RecursiveMode, Event};
use std::error::Error;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::sync::Mutex;
use std::path::PathBuf;
use std::process::Command;
use std::fs;
use std::thread;

const GUI_WINDOW_PID_FILE: &str = "/tmp/transcribe-rs-gui-window.pid";

/// Check if the GUI window process is currently running
pub fn is_window_running() -> bool {
    if let Ok(pid_str) = fs::read_to_string(GUI_WINDOW_PID_FILE) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            // Use kill -0 to check if process exists (doesn't actually send a signal)
            Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        } else {
            false
        }
    } else {
        false
    }
}

/// Recover orphaned conversation state from crashed window
///
/// If a conversation state file exists but no window is running,
/// this saves the conversation to markdown and cleans up the state.
pub fn recover_orphaned_conversation() -> Result<(), Box<dyn Error>> {
    if ConversationState::exists() && !is_window_running() {
        debug!("Found orphaned conversation state, recovering...");

        let state = ConversationState::load()?;
        state.save_to_markdown()?;
        ConversationState::delete()?;

        // Also clean up stale PID file if it exists
        let _ = fs::remove_file(GUI_WINDOW_PID_FILE);

        debug!("Saved conversation to markdown and cleaned up state");

        // Notify user
        if let Err(e) = crate::notifications::notify(
            "💾 Conversation Recovered",
            "Previous conversation saved to file",
            3000
        ) {
            warn!("Failed to send recovery notification: {}", e);
        }
    }
    Ok(())
}

/// Call the LLM with the user message and conversation context
fn call_llm(user_message: &str, conversation_history: &[(String, String)], prompt_name: &str) -> Result<String, String> {
    use crate::{groq, prompts, config::Config};

    // Load config
    let config = Config::load().map_err(|e| format!("Failed to load config: {}", e))?;

    // Load prompt (defaults to "chat" if not specified)
    let prompt = prompts::load_prompt(prompt_name)
        .map_err(|e| format!("Failed to load prompt '{}': {}", prompt_name, e))?;

    // Look up the tool set for this prompt
    let client = if let Some(tool_set_name) = config.llm.prompt_tool_mapping.get(prompt_name) {
        if let Some(tool_names) = config.llm.tool_sets.get(tool_set_name) {
            if tool_names.is_empty() {
                groq::GroqClient::from_env_file_no_tools()
                    .map_err(|e| format!("Failed to create Groq client: {}", e))?
            } else {
                groq::GroqClient::from_env_file_with_tool_set(tool_names.clone())
                    .map_err(|e| format!("Failed to create Groq client: {}", e))?
            }
        } else {
            groq::GroqClient::from_env_file_no_tools()
                .map_err(|e| format!("Failed to create Groq client: {}", e))?
        }
    } else {
        groq::GroqClient::from_env_file_no_tools()
            .map_err(|e| format!("Failed to create Groq client: {}", e))?
    };

    // Call Groq with conversation context
    let result = client.complete_with_history(&prompt, user_message, conversation_history, None)
        .map_err(|e| format!("LLM call failed: {}", e))?;

    Ok(result.text)
}

/// Response from the LLM background thread
struct LlmResponse {
    user_message: String,
    assistant_response: Result<String, String>,
}

pub struct ConversationWindow {
    state: Arc<Mutex<ConversationState>>,
    file_watcher_rx: Receiver<Result<Event, notify::Error>>,
    _watcher: Box<dyn Watcher>,  // Keep watcher alive
    should_close: bool,
    // Text input fields
    input_text: String,
    is_submitting: bool,
    llm_response_rx: Option<Receiver<LlmResponse>>,
    llm_response_tx: Sender<LlmResponse>,
    refocus_in_frames: u8,  // Count down frames until refocus (0 = don't refocus)
}

impl Drop for ConversationWindow {
    fn drop(&mut self) {
        // This runs when the window process exits
        eprintln!("[GUI-WINDOW] Drop called - cleaning up conversation state");

        // Save final conversation to markdown
        let state = self.state.lock().unwrap();
        if let Err(e) = state.save_to_markdown() {
            eprintln!("[GUI-WINDOW] Failed to save conversation on exit: {}", e);
        } else {
            eprintln!("[GUI-WINDOW] Saved conversation to markdown");
        }

        // Delete state file to signal conversation is over
        if let Err(e) = ConversationState::delete() {
            eprintln!("[GUI-WINDOW] Failed to delete state file on exit: {}", e);
        } else {
            eprintln!("[GUI-WINDOW] Deleted state file - conversation ended");
        }

        // Remove PID file
        if let Err(e) = fs::remove_file(GUI_WINDOW_PID_FILE) {
            eprintln!("[GUI-WINDOW] Failed to remove PID file: {}", e);
        } else {
            eprintln!("[GUI-WINDOW] Removed PID file");
        }
    }
}

impl ConversationWindow {
    pub fn new(state: ConversationState) -> Result<Self, Box<dyn Error>> {
        let state_file_path = PathBuf::from("/tmp/transcribe-rs-conversation-state.json");

        // Set up file watcher
        let (tx, rx) = channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;

        // Watch the state file
        watcher.watch(&state_file_path, RecursiveMode::NonRecursive)?;

        // Write PID file to track this window process
        let pid = std::process::id();
        fs::write(GUI_WINDOW_PID_FILE, pid.to_string())?;
        eprintln!("[GUI-WINDOW] Wrote PID file: {} (PID: {})", GUI_WINDOW_PID_FILE, pid);

        // Set up channel for LLM responses
        let (llm_tx, llm_rx) = channel();

        Ok(Self {
            state: Arc::new(Mutex::new(state)),
            file_watcher_rx: rx,
            _watcher: Box::new(watcher),
            should_close: false,
            input_text: String::new(),
            is_submitting: false,
            llm_response_rx: Some(llm_rx),
            llm_response_tx: llm_tx,
            refocus_in_frames: 0,
        })
    }

    /// Submit the current input text to the LLM
    fn submit_message(&mut self) {
        let message = self.input_text.trim().to_string();
        if message.is_empty() {
            return;
        }

        eprintln!("[GUI-WINDOW] Submitting message: {}", message);
        self.is_submitting = true;
        self.input_text.clear();

        // Get conversation context and prompt name
        let (conversation_context, prompt_name): (Vec<(String, String)>, String) = {
            let state = self.state.lock().unwrap();
            (state.get_context_for_llm(), state.prompt_name.clone())
        };

        // Clone what we need for the thread
        let tx = self.llm_response_tx.clone();
        let user_message = message.clone();

        // Spawn thread to call LLM
        thread::spawn(move || {
            eprintln!("[GUI-WINDOW] LLM thread started with prompt: {}", prompt_name);

            let result = call_llm(&user_message, &conversation_context, &prompt_name);

            let _ = tx.send(LlmResponse {
                user_message,
                assistant_response: result,
            });

            eprintln!("[GUI-WINDOW] LLM thread finished");
        });
    }

    pub fn run(state: ConversationState) -> Result<(), Box<dyn Error>> {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([800.0, 600.0])
                .with_resizable(true)
                .with_always_on_top()
                .with_decorations(true)
                .with_title("Conversation"),
            ..Default::default()
        };

        let window = ConversationWindow::new(state)?;

        eframe::run_native(
            "Transcribe Conversation",
            options,
            Box::new(|_cc| Ok(Box::new(window))),
        )
        .map_err(|e| format!("Failed to run eframe: {}", e).into())
    }
}

impl eframe::App for ConversationWindow {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request repaint every 80ms to check for file changes (even when idle)
        ctx.request_repaint_after(std::time::Duration::from_millis(80));

        // Check for LLM responses (non-blocking)
        if let Some(ref rx) = self.llm_response_rx {
            while let Ok(response) = rx.try_recv() {
                eprintln!("[GUI-WINDOW] Received LLM response");
                self.is_submitting = false;

                match response.assistant_response {
                    Ok(assistant_text) => {
                        // Add messages to state
                        let mut state = self.state.lock().unwrap();
                        state.add_message("user", response.user_message);
                        state.add_message("assistant", assistant_text);

                        // Save state
                        if let Err(e) = state.save() {
                            eprintln!("[GUI-WINDOW] Failed to save state: {}", e);
                        }
                        if let Err(e) = state.save_to_markdown() {
                            eprintln!("[GUI-WINDOW] Failed to save markdown: {}", e);
                        }

                        // Schedule refocus (0 = immediate on next check, try without delay)
                        self.refocus_in_frames = 1;
                    }
                    Err(e) => {
                        eprintln!("[GUI-WINDOW] LLM error: {}", e);
                        if let Err(notify_err) = crate::notifications::notify_error(&e) {
                            eprintln!("[GUI-WINDOW] Failed to send error notification: {}", notify_err);
                        }
                    }
                }
            }
        }

        // Check for file system events (non-blocking)
        while let Ok(event) = self.file_watcher_rx.try_recv() {
            match event {
                Ok(event) => {
                    // File was modified, reload state
                    if event.kind.is_modify() {
                        if let Ok(new_state) = ConversationState::load() {
                            let mut state = self.state.lock().unwrap();
                            *state = new_state;
                            // Request repaint to show new messages
                            ctx.request_repaint();
                        }
                    }
                }
                Err(e) => {
                    eprintln!("File watcher error: {}", e);
                }
            }
        }

        // Handle keyboard shortcuts (only Escape when not typing)
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.input_text.is_empty() {
            self.should_close = true;
        }

        // Check for click outside window (close on focus loss) - but not when submitting
        if !self.is_submitting && !ctx.input(|i| i.focused) {
            // Only close if user clicked elsewhere, not on initial focus
            if ctx.input(|i| i.pointer.any_click()) {
                self.should_close = true;
            }
        }

        // Bottom panel for text input
        egui::TopBottomPanel::bottom("input_panel").show(ctx, |ui| {
            ui.add_space(4.0);

            // Instructions
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Click message to copy • Enter to send • Shift+Enter for newline • ESC to close")
                        .size(11.0)
                        .color(egui::Color32::GRAY),
                );
            });

            ui.add_space(4.0);

            // Text input area
            let text_edit_id = egui::Id::new("message_input");

            ui.horizontal(|ui| {
                let available_width = ui.available_width() - 70.0; // Leave space for button

                // Check for Enter key press (without Shift)
                let enter_pressed = ctx.input(|i| {
                    i.key_pressed(egui::Key::Enter) && !i.modifiers.shift
                });

                let text_edit = egui::TextEdit::multiline(&mut self.input_text)
                    .id(text_edit_id)
                    .desired_width(available_width)
                    .desired_rows(2)
                    .hint_text("Type a message...")
                    .interactive(!self.is_submitting);

                let response = ui.add(text_edit);

                // Handle Enter to submit (Shift+Enter adds newline naturally)
                if enter_pressed && response.has_focus() && !self.is_submitting {
                    // Remove the newline that Enter just added
                    if self.input_text.ends_with('\n') {
                        self.input_text.pop();
                    }
                    self.submit_message();
                }

                // Send button
                ui.add_enabled_ui(!self.is_submitting && !self.input_text.trim().is_empty(), |ui| {
                    if ui.button(if self.is_submitting { "..." } else { "Send" }).clicked() {
                        self.submit_message();
                    }
                });
            });

            ui.add_space(4.0);
        });

        // Handle frame-delayed focus request for text input
        if self.refocus_in_frames > 0 {
            self.refocus_in_frames -= 1;
            if self.refocus_in_frames == 0 {
                ctx.memory_mut(|mem| mem.request_focus(egui::Id::new("message_input")));
            }
        }

        // Main content area for messages
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Conversation");
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    let state = self.state.lock().unwrap();

                    if state.messages.is_empty() && !self.is_submitting {
                        ui.vertical_centered(|ui| {
                            ui.add_space(50.0);
                            ui.label(
                                egui::RichText::new("No messages yet")
                                    .size(16.0)
                                    .color(egui::Color32::GRAY),
                            );
                            ui.label(
                                egui::RichText::new("Type below or dictate to begin")
                                    .size(14.0)
                                    .color(egui::Color32::DARK_GRAY),
                            );
                        });
                    } else {
                        for msg in &state.messages {
                            ui.add_space(8.0);

                            let (role_text, role_color) = if msg.role == "user" {
                                ("You", egui::Color32::from_rgb(100, 149, 237))
                            } else {
                                ("Assistant", egui::Color32::from_rgb(144, 238, 144))
                            };

                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(role_text)
                                        .size(14.0)
                                        .strong()
                                        .color(role_color),
                                );
                                ui.label(
                                    egui::RichText::new(
                                        msg.timestamp.format("%H:%M:%S").to_string(),
                                    )
                                    .size(12.0)
                                    .color(egui::Color32::GRAY),
                                );
                            });

                            ui.add_space(4.0);

                            // Message content with word wrap - clickable to copy
                            let content_text = msg.content.clone();
                            let response = ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.add(egui::Label::new(egui::RichText::new(&msg.content).size(14.0))
                                    .sense(egui::Sense::click()))
                            }).inner;

                            // Handle click to copy
                            if response.clicked() {
                                eprintln!("[GUI-WINDOW] Message clicked, copying to clipboard...");

                                // Copy to clipboard
                                if let Err(e) = crate::clipboard::copy_to_clipboard(&content_text) {
                                    eprintln!("[GUI-WINDOW] Failed to copy to clipboard: {}", e);
                                    if let Err(e) = crate::notifications::notify_error(
                                        &format!("Failed to copy: {}", e)
                                    ) {
                                        eprintln!("[GUI-WINDOW] Failed to send error notification: {}", e);
                                    }
                                } else {
                                    eprintln!("[GUI-WINDOW] Copied to clipboard successfully");

                                    // Show notification with preview (truncate if too long)
                                    let preview = if content_text.len() > 100 {
                                        format!("{}...", &content_text[..97])
                                    } else {
                                        content_text.clone()
                                    };

                                    if let Err(e) = crate::notifications::notify_transcription_copied(&preview) {
                                        eprintln!("[GUI-WINDOW] Failed to send notification: {}", e);
                                    }
                                }

                                // Change cursor to indicate clickable
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }

                            // Show hover effect
                            if response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }

                            ui.add_space(8.0);
                            ui.separator();
                        }

                        // Show "Thinking..." indicator when submitting
                        if self.is_submitting {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Assistant")
                                        .size(14.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(144, 238, 144)),
                                );
                            });
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("Thinking...")
                                    .size(14.0)
                                    .color(egui::Color32::GRAY)
                                    .italics(),
                            );
                        }
                    }
                });
        });

        // Close window if requested (Drop trait handles cleanup)
        if self.should_close {
            eprintln!("[GUI-WINDOW] Closing window, Drop will handle cleanup...");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
