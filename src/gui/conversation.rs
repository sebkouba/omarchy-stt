use super::state::ConversationState;
use eframe::egui;
use notify::{Watcher, RecursiveMode, Event};
use std::error::Error;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::sync::Mutex;
use std::path::PathBuf;
use std::process::Command;
use std::fs;

const STATE_FILE_PATH: &str = "/tmp/transcribe-rs-conversation-state.json";
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
pub fn recover_orphaned_conversation(log_file: &str) -> Result<(), Box<dyn Error>> {
    if ConversationState::exists() && !is_window_running() {
        eprintln!("[RECOVERY] Found orphaned conversation state, recovering...");

        // Append to log file
        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)
        {
            use std::io::Write;
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(file, "[{}] [recovery] Found orphaned conversation, recovering", timestamp);
        }

        let state = ConversationState::load()?;
        state.save_to_markdown()?;
        ConversationState::delete()?;

        // Also clean up stale PID file if it exists
        let _ = fs::remove_file(GUI_WINDOW_PID_FILE);

        eprintln!("[RECOVERY] Saved conversation to markdown and cleaned up state");

        // Notify user
        if let Err(e) = crate::notifications::notify(
            "💾 Conversation Recovered",
            "Previous conversation saved to file",
            3000
        ) {
            eprintln!("[RECOVERY] Failed to send notification: {}", e);
        }
    }
    Ok(())
}

pub struct ConversationWindow {
    state: Arc<Mutex<ConversationState>>,
    state_file_path: PathBuf,
    file_watcher_rx: Receiver<Result<Event, notify::Error>>,
    _watcher: Box<dyn Watcher>,  // Keep watcher alive
    should_close: bool,
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

        Ok(Self {
            state: Arc::new(Mutex::new(state)),
            state_file_path,
            file_watcher_rx: rx,
            _watcher: Box::new(watcher),
            should_close: false,
        })
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

        // Handle keyboard shortcuts
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.should_close = true;
        }

        // Check for click outside window (close on focus loss)
        if !ctx.input(|i| i.focused) {
            // Only close if user clicked elsewhere, not on initial focus
            if ctx.input(|i| i.pointer.any_click()) {
                self.should_close = true;
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Conversation");
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    let state = self.state.lock().unwrap();

                    if state.messages.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(50.0);
                            ui.label(
                                egui::RichText::new("No messages yet")
                                    .size(16.0)
                                    .color(egui::Color32::GRAY),
                            );
                            ui.label(
                                egui::RichText::new("Start dictating to begin the conversation")
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

                            // Message content with word wrap
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.label(egui::RichText::new(&msg.content).size(14.0));
                            });

                            ui.add_space(8.0);
                            ui.separator();
                        }
                    }
                });

            ui.add_space(8.0);

            // Footer with instructions
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Press ESC or click outside to close • Window updates automatically")
                        .size(12.0)
                        .color(egui::Color32::GRAY),
                );
            });
        });

        // Close window if requested (Drop trait handles cleanup)
        if self.should_close {
            eprintln!("[GUI-WINDOW] Closing window, Drop will handle cleanup...");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
