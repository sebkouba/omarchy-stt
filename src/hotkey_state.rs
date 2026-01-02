//! Pure state machine logic for hotkey-driven recording.
//!
//! This module contains the core state machine for push-to-talk transcription,
//! separated from side effects for testability. The state machine handles:
//!
//! - Push-to-talk (hold key to record, release to transcribe)
//! - Long recording mode (tap to start, tap again to finish)
//! - Double-tap repaste (quickly tap twice to paste last transcription)
//! - Enter key to submit and continue recording
//! - Escape key to cancel recording
//! - Switching prompts during long recording
//!
//! # Architecture
//!
//! The state machine is pure: given a state and event, it returns a new state
//! and a list of actions to execute. The caller (hotkey-daemon) is responsible
//! for executing the actions.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::config::HotkeyBinding;

/// Events that can trigger state transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// A hotkey was pressed
    Activated { shortcut_id: String },
    /// A hotkey was released
    Deactivated { shortcut_id: String },
}

/// States of the recording state machine.
#[derive(Debug, Clone)]
pub enum RecordingState {
    /// Not recording, waiting for hotkey press
    Idle,

    /// Recording in push-to-talk mode (hotkey held down)
    Recording {
        /// When the key was pressed
        press_time: Instant,
        /// The shortcut that started recording
        shortcut_id: String,
        /// Prompt to use for LLM processing
        prompt: Option<String>,
    },

    /// Long recording mode (key tapped, recording continues)
    LongRecording {
        /// The shortcut that started recording
        shortcut_id: String,
        /// Prompt to use for LLM processing
        prompt: Option<String>,
        /// When we entered long recording mode
        entered_at: Instant,
    },

    /// Waiting for key release before repasting (double-tap detected)
    PendingRepaste {
        /// The key we're waiting to be released
        shortcut_id: String,
        /// The text to repaste
        text: String,
    },

    /// Waiting for key release before transcribing (long recording finish)
    PendingTranscription {
        /// The key we're waiting to be released
        shortcut_id: String,
        /// Prompt to use for LLM processing
        prompt: Option<String>,
    },
}

impl PartialEq for RecordingState {
    fn eq(&self, other: &Self) -> bool {
        // Compare variants without comparing Instant fields (not comparable)
        match (self, other) {
            (RecordingState::Idle, RecordingState::Idle) => true,
            (
                RecordingState::Recording {
                    shortcut_id: a,
                    prompt: pa,
                    ..
                },
                RecordingState::Recording {
                    shortcut_id: b,
                    prompt: pb,
                    ..
                },
            ) => a == b && pa == pb,
            (
                RecordingState::LongRecording {
                    shortcut_id: a,
                    prompt: pa,
                    ..
                },
                RecordingState::LongRecording {
                    shortcut_id: b,
                    prompt: pb,
                    ..
                },
            ) => a == b && pa == pb,
            (
                RecordingState::PendingRepaste {
                    shortcut_id: a,
                    text: ta,
                },
                RecordingState::PendingRepaste {
                    shortcut_id: b,
                    text: tb,
                },
            ) => a == b && ta == tb,
            (
                RecordingState::PendingTranscription {
                    shortcut_id: a,
                    prompt: pa,
                },
                RecordingState::PendingTranscription {
                    shortcut_id: b,
                    prompt: pb,
                },
            ) => a == b && pa == pb,
            _ => false,
        }
    }
}

impl Eq for RecordingState {}

/// Actions to execute as side effects of state transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Start recording audio
    StartRecording,

    /// Stop recording and transcribe with optional LLM prompt
    StopAndTranscribe { prompt: Option<String> },

    /// Cancel recording without transcribing
    CancelRecording,

    /// Bind Enter and Escape keys as global shortcuts (for long recording mode)
    BindLongRecordingKeys,

    /// Unbind Enter and Escape keys
    UnbindLongRecordingKeys,

    /// Repaste the given text to clipboard and paste
    Repaste { text: String },

    /// Submit current recording (transcribe + paste + send Enter) and restart recording
    SubmitAndContinue { prompt: Option<String> },

    /// Submit current recording (transcribe + paste + send Enter) and end dictation (return to Idle)
    SubmitAndEnd { prompt: Option<String> },

    /// Show a notification
    Notify { title: String, body: String },
}

/// Context needed for transition decisions.
///
/// This contains configuration and cached state that affects transitions
/// but is managed externally to the state machine.
pub struct TransitionContext<'a> {
    /// How long a key must be held to be considered a "hold" vs "tap"
    pub tap_threshold: Duration,

    /// Window for double-tap repaste detection
    pub double_tap_window: Duration,

    /// Map of shortcut IDs to their bindings
    pub binding_map: &'a HashMap<String, HotkeyBinding>,

    /// Last successful transcription (for repaste)
    pub last_transcription: Option<&'a str>,

    /// When the last repaste occurred (for debounce)
    pub last_repaste_time: Option<Instant>,

    /// Ignore activations within this duration after a repaste
    pub repaste_debounce: Duration,
}

/// Result of a state transition.
pub struct TransitionResult {
    /// The new state after the transition
    pub new_state: RecordingState,

    /// Actions to execute
    pub actions: Vec<Action>,
}

impl RecordingState {
    /// Compute the next state and required actions given an event.
    ///
    /// This is the core state machine logic, separated from side effects.
    /// The caller should execute the returned actions after updating state.
    ///
    /// The `now` parameter allows tests to control time for deterministic testing.
    /// Production code should pass `Instant::now()`.
    pub fn transition(self, event: HotkeyEvent, ctx: &TransitionContext, now: Instant) -> TransitionResult {
        match (&self, &event) {
            // ─────────────────────────────────────────────────────────────────
            // IDLE state
            // ─────────────────────────────────────────────────────────────────
            (RecordingState::Idle, HotkeyEvent::Activated { shortcut_id }) => {
                // Check debounce after repaste
                if let Some(repaste_time) = ctx.last_repaste_time {
                    if now.duration_since(repaste_time) < ctx.repaste_debounce {
                        return TransitionResult {
                            new_state: RecordingState::Idle,
                            actions: vec![],
                        };
                    }
                }

                // Look up binding
                if let Some(binding) = ctx.binding_map.get(shortcut_id) {
                    TransitionResult {
                        new_state: RecordingState::Recording {
                            press_time: now,
                            shortcut_id: shortcut_id.clone(),
                            prompt: binding.prompt.clone(),
                        },
                        actions: vec![Action::StartRecording],
                    }
                } else {
                    // Unknown shortcut, ignore
                    TransitionResult {
                        new_state: RecordingState::Idle,
                        actions: vec![],
                    }
                }
            }

            // Idle ignores deactivations
            (RecordingState::Idle, HotkeyEvent::Deactivated { .. }) => TransitionResult {
                new_state: RecordingState::Idle,
                actions: vec![],
            },

            // ─────────────────────────────────────────────────────────────────
            // RECORDING state (push-to-talk, key held)
            // ─────────────────────────────────────────────────────────────────

            // Same key released: check tap vs hold
            (
                RecordingState::Recording {
                    press_time,
                    shortcut_id: active_id,
                    prompt,
                },
                HotkeyEvent::Deactivated { shortcut_id },
            ) if shortcut_id == active_id => {
                let duration = now.duration_since(*press_time);

                if duration < ctx.tap_threshold {
                    // Tap: enter long recording mode
                    TransitionResult {
                        new_state: RecordingState::LongRecording {
                            shortcut_id: shortcut_id.clone(),
                            prompt: prompt.clone(),
                            entered_at: now,
                        },
                        actions: vec![Action::BindLongRecordingKeys],
                    }
                } else {
                    // Hold: finish recording and transcribe
                    TransitionResult {
                        new_state: RecordingState::Idle,
                        actions: vec![Action::StopAndTranscribe {
                            prompt: prompt.clone(),
                        }],
                    }
                }
            }

            // Different key released while recording: ignore
            (RecordingState::Recording { .. }, HotkeyEvent::Deactivated { .. }) => {
                TransitionResult {
                    new_state: self,
                    actions: vec![],
                }
            }

            // Key activated while already recording: ignore
            (RecordingState::Recording { .. }, HotkeyEvent::Activated { .. }) => TransitionResult {
                new_state: self,
                actions: vec![],
            },

            // ─────────────────────────────────────────────────────────────────
            // LONG RECORDING state (tap-to-toggle mode)
            // ─────────────────────────────────────────────────────────────────

            // Same key activated: check for double-tap (repaste) or finish
            (
                RecordingState::LongRecording {
                    shortcut_id: active_id,
                    prompt,
                    entered_at,
                },
                HotkeyEvent::Activated { shortcut_id },
            ) if shortcut_id == active_id => {
                let time_in_long_recording = now.duration_since(*entered_at);

                if time_in_long_recording < ctx.double_tap_window {
                    // Double-tap: cancel recording and prepare to repaste
                    let mut actions =
                        vec![Action::UnbindLongRecordingKeys, Action::CancelRecording];

                    if let Some(text) = ctx.last_transcription {
                        TransitionResult {
                            new_state: RecordingState::PendingRepaste {
                                shortcut_id: shortcut_id.clone(),
                                text: text.to_string(),
                            },
                            actions,
                        }
                    } else {
                        actions.push(Action::Notify {
                            title: "No previous transcription".to_string(),
                            body: "Record something first".to_string(),
                        });
                        TransitionResult {
                            new_state: RecordingState::Idle,
                            actions,
                        }
                    }
                } else {
                    // Normal finish: defer transcription until key release
                    TransitionResult {
                        new_state: RecordingState::PendingTranscription {
                            shortcut_id: shortcut_id.clone(),
                            prompt: prompt.clone(),
                        },
                        actions: vec![Action::UnbindLongRecordingKeys],
                    }
                }
            }

            // Enter key: submit and continue
            (
                RecordingState::LongRecording {
                    shortcut_id: original_id,
                    prompt,
                    ..
                },
                HotkeyEvent::Activated { shortcut_id },
            ) if shortcut_id == "transcribe-enter" => {
                // Submit current, restart recording - preserve the original shortcut_id
                TransitionResult {
                    new_state: RecordingState::LongRecording {
                        shortcut_id: original_id.clone(),
                        prompt: prompt.clone(),
                        entered_at: now,
                    },
                    actions: vec![
                        Action::UnbindLongRecordingKeys,
                        Action::SubmitAndContinue {
                            prompt: prompt.clone(),
                        },
                        Action::BindLongRecordingKeys,
                    ],
                }
            }

            // Escape key: cancel recording
            (RecordingState::LongRecording { .. }, HotkeyEvent::Activated { shortcut_id })
                if shortcut_id == "transcribe-escape" =>
            {
                TransitionResult {
                    new_state: RecordingState::Idle,
                    actions: vec![
                        Action::UnbindLongRecordingKeys,
                        Action::CancelRecording,
                        Action::Notify {
                            title: "Recording cancelled".to_string(),
                            body: "Press hotkey to start again".to_string(),
                        },
                    ],
                }
            }

            // Super key: submit and end (don't restart recording)
            (
                RecordingState::LongRecording { prompt, .. },
                HotkeyEvent::Activated { shortcut_id },
            ) if shortcut_id == "transcribe-super" => {
                TransitionResult {
                    new_state: RecordingState::Idle,
                    actions: vec![
                        Action::UnbindLongRecordingKeys,
                        Action::SubmitAndEnd {
                            prompt: prompt.clone(),
                        },
                    ],
                }
            }

            // Different hotkey activated: switch prompt
            (RecordingState::LongRecording { .. }, HotkeyEvent::Activated { shortcut_id }) => {
                if let Some(binding) = ctx.binding_map.get(shortcut_id) {
                    TransitionResult {
                        new_state: RecordingState::Recording {
                            press_time: now,
                            shortcut_id: shortcut_id.clone(),
                            prompt: binding.prompt.clone(),
                        },
                        actions: vec![
                            Action::UnbindLongRecordingKeys,
                            Action::CancelRecording,
                            Action::StartRecording,
                        ],
                    }
                } else {
                    // Unknown shortcut, ignore
                    TransitionResult {
                        new_state: self,
                        actions: vec![],
                    }
                }
            }

            // Deactivations in long recording mode are ignored
            (RecordingState::LongRecording { .. }, HotkeyEvent::Deactivated { .. }) => {
                TransitionResult {
                    new_state: self,
                    actions: vec![],
                }
            }

            // ─────────────────────────────────────────────────────────────────
            // PENDING REPASTE state (waiting for key release before repasting)
            // ─────────────────────────────────────────────────────────────────

            // Key released: now safe to repaste
            (
                RecordingState::PendingRepaste {
                    shortcut_id: pending_id,
                    text,
                },
                HotkeyEvent::Deactivated { shortcut_id },
            ) if shortcut_id == pending_id => TransitionResult {
                new_state: RecordingState::Idle,
                actions: vec![Action::Repaste { text: text.clone() }],
            },

            // Other events in PendingRepaste: ignore
            (RecordingState::PendingRepaste { .. }, _) => TransitionResult {
                new_state: self,
                actions: vec![],
            },

            // ─────────────────────────────────────────────────────────────────
            // PENDING TRANSCRIPTION state (waiting for key release)
            // ─────────────────────────────────────────────────────────────────

            // Key released: now safe to transcribe
            (
                RecordingState::PendingTranscription {
                    shortcut_id: pending_id,
                    prompt,
                },
                HotkeyEvent::Deactivated { shortcut_id },
            ) if shortcut_id == pending_id => TransitionResult {
                new_state: RecordingState::Idle,
                actions: vec![Action::StopAndTranscribe {
                    prompt: prompt.clone(),
                }],
            },

            // Other events in PendingTranscription: ignore
            (RecordingState::PendingTranscription { .. }, _) => TransitionResult {
                new_state: self,
                actions: vec![],
            },
        }
    }
}

/// State machine manager that tracks state and internal caches.
///
/// This provides a higher-level interface that handles cache updates
/// based on executed actions.
pub struct StateMachine {
    state: RecordingState,
    last_transcription: Option<String>,
    last_repaste_time: Option<Instant>,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl StateMachine {
    /// Create a new state machine in the Idle state.
    pub fn new() -> Self {
        Self {
            state: RecordingState::Idle,
            last_transcription: None,
            last_repaste_time: None,
        }
    }

    /// Get the current state.
    pub fn state(&self) -> &RecordingState {
        &self.state
    }

    /// Get the last transcription (for repaste).
    pub fn last_transcription(&self) -> Option<&str> {
        self.last_transcription.as_deref()
    }

    /// Get the last repaste time (for debounce).
    pub fn last_repaste_time(&self) -> Option<Instant> {
        self.last_repaste_time
    }

    /// Cache a successful transcription result.
    pub fn cache_transcription(&mut self, text: String) {
        self.last_transcription = Some(text);
    }

    /// Record that a repaste just occurred (for debounce).
    ///
    /// The `at` parameter allows tests to control time for deterministic testing.
    /// Production code should pass `Instant::now()`.
    pub fn record_repaste(&mut self, at: Instant) {
        self.last_repaste_time = Some(at);
    }

    /// Handle an event and return the actions to execute.
    ///
    /// The caller should execute the returned actions. For certain actions,
    /// the caller should call back:
    /// - After successful transcription: call `cache_transcription(text)`
    /// - After repaste: call `record_repaste(now)`
    ///
    /// The `now` parameter allows tests to control time for deterministic testing.
    /// Production code should pass `Instant::now()`.
    pub fn handle_event(
        &mut self,
        event: HotkeyEvent,
        binding_map: &HashMap<String, HotkeyBinding>,
        tap_threshold: Duration,
        double_tap_window: Duration,
        repaste_debounce: Duration,
        now: Instant,
    ) -> Vec<Action> {
        let ctx = TransitionContext {
            tap_threshold,
            double_tap_window,
            binding_map,
            last_transcription: self.last_transcription.as_deref(),
            last_repaste_time: self.last_repaste_time,
            repaste_debounce,
        };

        let result =
            std::mem::replace(&mut self.state, RecordingState::Idle).transition(event, &ctx, now);

        self.state = result.new_state;
        result.actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_binding_map() -> HashMap<String, HotkeyBinding> {
        let mut map = HashMap::new();
        map.insert(
            "transcribe-e".to_string(),
            HotkeyBinding {
                key: "e".to_string(),
                prompt: None,
                ocr: false,
                gui: false,
            },
        );
        map.insert(
            "transcribe-q".to_string(),
            HotkeyBinding {
                key: "q".to_string(),
                prompt: Some("grammar".to_string()),
                ocr: false,
                gui: false,
            },
        );
        map
    }

    fn make_ctx(binding_map: &HashMap<String, HotkeyBinding>) -> TransitionContext<'_> {
        TransitionContext {
            tap_threshold: Duration::from_millis(200),
            double_tap_window: Duration::from_millis(1500),
            binding_map,
            last_transcription: None,
            last_repaste_time: None,
            repaste_debounce: Duration::from_millis(1000),
        }
    }

    #[test]
    fn test_idle_activation_starts_recording() {
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::Idle;
        let event = HotkeyEvent::Activated {
            shortcut_id: "transcribe-e".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        assert!(matches!(
            result.new_state,
            RecordingState::Recording { shortcut_id, prompt: None, .. } if shortcut_id == "transcribe-e"
        ));
        assert_eq!(result.actions, vec![Action::StartRecording]);
    }

    #[test]
    fn test_idle_activation_with_prompt() {
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::Idle;
        let event = HotkeyEvent::Activated {
            shortcut_id: "transcribe-q".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        assert!(matches!(
            result.new_state,
            RecordingState::Recording { shortcut_id, prompt: Some(p), .. }
                if shortcut_id == "transcribe-q" && p == "grammar"
        ));
    }

    #[test]
    fn test_unknown_shortcut_ignored() {
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::Idle;
        let event = HotkeyEvent::Activated {
            shortcut_id: "transcribe-unknown".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert!(result.actions.is_empty());
    }

    #[test]
    fn test_hold_release_transcribes() {
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        // Simulate a hold: press_time was 500ms ago
        let press_time = Instant::now() - Duration::from_millis(500);
        let state = RecordingState::Recording {
            press_time,
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };

        let event = HotkeyEvent::Deactivated {
            shortcut_id: "transcribe-e".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert_eq!(
            result.actions,
            vec![Action::StopAndTranscribe { prompt: None }]
        );
    }

    #[test]
    fn test_tap_enters_long_recording() {
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        // Simulate a tap: press_time was just now (< tap_threshold)
        let press_time = Instant::now();
        let state = RecordingState::Recording {
            press_time,
            shortcut_id: "transcribe-e".to_string(),
            prompt: Some("grammar".to_string()),
        };

        let event = HotkeyEvent::Deactivated {
            shortcut_id: "transcribe-e".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        assert!(matches!(
            result.new_state,
            RecordingState::LongRecording { shortcut_id, prompt: Some(p), .. }
                if shortcut_id == "transcribe-e" && p == "grammar"
        ));
        assert_eq!(result.actions, vec![Action::BindLongRecordingKeys]);
    }

    #[test]
    fn test_long_recording_finish() {
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        // Entered long recording > 1.5s ago
        let entered_at = Instant::now() - Duration::from_millis(2000);
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at,
        };

        let event = HotkeyEvent::Activated {
            shortcut_id: "transcribe-e".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        assert!(matches!(
            result.new_state,
            RecordingState::PendingTranscription { shortcut_id, prompt: None }
                if shortcut_id == "transcribe-e"
        ));
        assert_eq!(result.actions, vec![Action::UnbindLongRecordingKeys]);
    }

    #[test]
    fn test_double_tap_repastes() {
        let bindings = make_binding_map();
        let mut ctx = make_ctx(&bindings);
        ctx.last_transcription = Some("Hello world");

        // Entered long recording < 1.5s ago (quick double-tap)
        let entered_at = Instant::now() - Duration::from_millis(500);
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at,
        };

        let event = HotkeyEvent::Activated {
            shortcut_id: "transcribe-e".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        assert!(matches!(
            result.new_state,
            RecordingState::PendingRepaste { shortcut_id, text }
                if shortcut_id == "transcribe-e" && text == "Hello world"
        ));
        assert_eq!(
            result.actions,
            vec![Action::UnbindLongRecordingKeys, Action::CancelRecording]
        );
    }

    #[test]
    fn test_double_tap_no_previous_transcription() {
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings); // no last_transcription

        let entered_at = Instant::now() - Duration::from_millis(500);
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at,
        };

        let event = HotkeyEvent::Activated {
            shortcut_id: "transcribe-e".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert_eq!(
            result.actions,
            vec![
                Action::UnbindLongRecordingKeys,
                Action::CancelRecording,
                Action::Notify {
                    title: "No previous transcription".to_string(),
                    body: "Record something first".to_string(),
                }
            ]
        );
    }

    #[test]
    fn test_enter_submits_and_continues() {
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: Some("grammar".to_string()),
            entered_at: Instant::now() - Duration::from_secs(5),
        };

        let event = HotkeyEvent::Activated {
            shortcut_id: "transcribe-enter".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        // Should stay in LongRecording (with new entered_at)
        assert!(matches!(
            result.new_state,
            RecordingState::LongRecording { prompt: Some(p), .. } if p == "grammar"
        ));
        assert_eq!(
            result.actions,
            vec![
                Action::UnbindLongRecordingKeys,
                Action::SubmitAndContinue {
                    prompt: Some("grammar".to_string())
                },
                Action::BindLongRecordingKeys,
            ]
        );
    }

    #[test]
    fn test_escape_cancels() {
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now(),
        };

        let event = HotkeyEvent::Activated {
            shortcut_id: "transcribe-escape".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert_eq!(
            result.actions,
            vec![
                Action::UnbindLongRecordingKeys,
                Action::CancelRecording,
                Action::Notify {
                    title: "Recording cancelled".to_string(),
                    body: "Press hotkey to start again".to_string(),
                }
            ]
        );
    }

    #[test]
    fn test_super_submits_and_ends() {
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: Some("grammar".to_string()),
            entered_at: Instant::now() - Duration::from_secs(5),
        };

        let event = HotkeyEvent::Activated {
            shortcut_id: "transcribe-super".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        // Should return to Idle (not stay in LongRecording like Enter does)
        assert_eq!(result.new_state, RecordingState::Idle);
        assert_eq!(
            result.actions,
            vec![
                Action::UnbindLongRecordingKeys,
                Action::SubmitAndEnd {
                    prompt: Some("grammar".to_string())
                },
            ]
        );
    }

    #[test]
    fn test_switch_prompt_during_long_recording() {
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_secs(5),
        };

        // Press a different key (q with grammar prompt)
        let event = HotkeyEvent::Activated {
            shortcut_id: "transcribe-q".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        assert!(matches!(
            result.new_state,
            RecordingState::Recording { shortcut_id, prompt: Some(p), .. }
                if shortcut_id == "transcribe-q" && p == "grammar"
        ));
        assert_eq!(
            result.actions,
            vec![
                Action::UnbindLongRecordingKeys,
                Action::CancelRecording,
                Action::StartRecording,
            ]
        );
    }

    #[test]
    fn test_pending_repaste_on_release() {
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::PendingRepaste {
            shortcut_id: "transcribe-e".to_string(),
            text: "Hello world".to_string(),
        };

        let event = HotkeyEvent::Deactivated {
            shortcut_id: "transcribe-e".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert_eq!(
            result.actions,
            vec![Action::Repaste {
                text: "Hello world".to_string()
            }]
        );
    }

    #[test]
    fn test_pending_transcription_on_release() {
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::PendingTranscription {
            shortcut_id: "transcribe-e".to_string(),
            prompt: Some("grammar".to_string()),
        };

        let event = HotkeyEvent::Deactivated {
            shortcut_id: "transcribe-e".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert_eq!(
            result.actions,
            vec![Action::StopAndTranscribe {
                prompt: Some("grammar".to_string())
            }]
        );
    }

    #[test]
    fn test_debounce_after_repaste() {
        let bindings = make_binding_map();
        let mut ctx = make_ctx(&bindings);
        // Repaste happened 500ms ago (within 1000ms debounce)
        ctx.last_repaste_time = Some(Instant::now() - Duration::from_millis(500));

        let state = RecordingState::Idle;
        let event = HotkeyEvent::Activated {
            shortcut_id: "transcribe-e".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        // Should be ignored
        assert_eq!(result.new_state, RecordingState::Idle);
        assert!(result.actions.is_empty());
    }

    #[test]
    fn test_no_debounce_after_debounce_period() {
        let bindings = make_binding_map();
        let mut ctx = make_ctx(&bindings);
        // Repaste happened 1500ms ago (outside 1000ms debounce)
        ctx.last_repaste_time = Some(Instant::now() - Duration::from_millis(1500));

        let state = RecordingState::Idle;
        let event = HotkeyEvent::Activated {
            shortcut_id: "transcribe-e".to_string(),
        };

        let result = state.transition(event, &ctx, Instant::now());

        // Should not be debounced
        assert!(matches!(result.new_state, RecordingState::Recording { .. }));
        assert_eq!(result.actions, vec![Action::StartRecording]);
    }

    #[test]
    fn test_state_machine_handle_event() {
        let bindings = make_binding_map();
        let mut sm = StateMachine::new();

        // Activate
        let actions = sm.handle_event(
            HotkeyEvent::Activated {
                shortcut_id: "transcribe-e".to_string(),
            },
            &bindings,
            Duration::from_millis(200),
            Duration::from_millis(1500),
            Duration::from_millis(1000),
            Instant::now(),
        );

        assert_eq!(actions, vec![Action::StartRecording]);
        assert!(matches!(sm.state(), RecordingState::Recording { .. }));
    }

    #[test]
    fn test_state_machine_caching() {
        let mut sm = StateMachine::new();

        assert!(sm.last_transcription().is_none());

        sm.cache_transcription("Test text".to_string());

        assert_eq!(sm.last_transcription(), Some("Test text"));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // INVARIANT TESTS
    // These tests verify properties that must ALWAYS hold, regardless of
    // specific event sequences. They catch classes of bugs rather than
    // individual scenarios.
    // ─────────────────────────────────────────────────────────────────────────

    /// Control keys that should NEVER appear as the shortcut_id in recording states.
    /// These are temporary bindings used during long recording mode, not actual
    /// transcription hotkeys.
    const CONTROL_KEYS: &[&str] = &["transcribe-enter", "transcribe-escape", "transcribe-super"];

    fn is_control_key(id: &str) -> bool {
        CONTROL_KEYS.contains(&id)
    }

    /// Invariant: Recording states should never have a control key as their shortcut_id.
    /// Control keys (Enter, Escape) are for controlling long recording mode, not for
    /// identifying which transcription binding initiated the recording.
    fn assert_no_control_key_pollution(state: &RecordingState) {
        match state {
            RecordingState::Recording { shortcut_id, .. } => {
                assert!(
                    !is_control_key(shortcut_id),
                    "INVARIANT VIOLATION: Recording state has control key '{}' as shortcut_id",
                    shortcut_id
                );
            }
            RecordingState::LongRecording { shortcut_id, .. } => {
                assert!(
                    !is_control_key(shortcut_id),
                    "INVARIANT VIOLATION: LongRecording state has control key '{}' as shortcut_id",
                    shortcut_id
                );
            }
            RecordingState::PendingTranscription { shortcut_id, .. } => {
                assert!(
                    !is_control_key(shortcut_id),
                    "INVARIANT VIOLATION: PendingTranscription state has control key '{}' as shortcut_id",
                    shortcut_id
                );
            }
            // PendingRepaste and Idle don't have this constraint
            _ => {}
        }
    }

    #[test]
    fn test_invariant_enter_does_not_pollute_state() {
        // Property: pressing Enter in LongRecording should never set shortcut_id to "transcribe-enter"
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        // Test with various starting states
        for original_key in ["transcribe-e", "transcribe-q"] {
            let state = RecordingState::LongRecording {
                shortcut_id: original_key.to_string(),
                prompt: None,
                entered_at: Instant::now() - Duration::from_secs(5),
            };

            let result = state.transition(
                HotkeyEvent::Activated {
                    shortcut_id: "transcribe-enter".to_string(),
                },
                &ctx,
                Instant::now(),
            );

            assert_no_control_key_pollution(&result.new_state);
        }
    }

    #[test]
    fn test_invariant_escape_does_not_pollute_state() {
        // Property: pressing Escape returns to Idle (no state to pollute)
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now(),
        };

        let result = state.transition(
            HotkeyEvent::Activated {
                shortcut_id: "transcribe-escape".to_string(),
            },
            &ctx,
            Instant::now(),
        );

        // Escape goes to Idle, but let's still check
        assert_no_control_key_pollution(&result.new_state);
    }

    #[test]
    fn test_invariant_super_does_not_pollute_state() {
        // Property: pressing Super returns to Idle (no state to pollute)
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now(),
        };

        let result = state.transition(
            HotkeyEvent::Activated {
                shortcut_id: "transcribe-super".to_string(),
            },
            &ctx,
            Instant::now(),
        );

        // Super goes to Idle, but let's still check
        assert_no_control_key_pollution(&result.new_state);
    }

    #[test]
    fn test_invariant_no_control_key_in_any_transition_from_long_recording() {
        // Exhaustive test: no transition FROM LongRecording should result in
        // a state with a control key as shortcut_id
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        let all_events = [
            HotkeyEvent::Activated {
                shortcut_id: "transcribe-e".to_string(),
            },
            HotkeyEvent::Activated {
                shortcut_id: "transcribe-q".to_string(),
            },
            HotkeyEvent::Activated {
                shortcut_id: "transcribe-enter".to_string(),
            },
            HotkeyEvent::Activated {
                shortcut_id: "transcribe-escape".to_string(),
            },
            HotkeyEvent::Activated {
                shortcut_id: "transcribe-super".to_string(),
            },
            HotkeyEvent::Activated {
                shortcut_id: "transcribe-unknown".to_string(),
            },
            HotkeyEvent::Deactivated {
                shortcut_id: "transcribe-e".to_string(),
            },
            HotkeyEvent::Deactivated {
                shortcut_id: "transcribe-enter".to_string(),
            },
        ];

        for original_key in ["transcribe-e", "transcribe-q"] {
            for event in &all_events {
                let state = RecordingState::LongRecording {
                    shortcut_id: original_key.to_string(),
                    prompt: Some("test".to_string()),
                    entered_at: Instant::now() - Duration::from_secs(5),
                };

                let result = state.transition(event.clone(), &ctx, Instant::now());
                assert_no_control_key_pollution(&result.new_state);
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // REGRESSION TESTS
    // These document specific bugs that were found and fixed.
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_enter_twice_in_long_recording() {
        // Regression test: pressing Enter twice in long recording mode should work both times.
        // Previously, the first Enter incorrectly set shortcut_id to "transcribe-enter",
        // causing the second Enter to be treated as a double-tap finish instead of submit+continue.
        let bindings = make_binding_map();
        let ctx = make_ctx(&bindings);

        // Start in long recording mode with original key "transcribe-e"
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: Some("grammar".to_string()),
            entered_at: Instant::now() - Duration::from_secs(5),
        };

        // First Enter
        let event = HotkeyEvent::Activated {
            shortcut_id: "transcribe-enter".to_string(),
        };
        let result = state.transition(event, &ctx, Instant::now());

        // Should stay in LongRecording with original shortcut_id preserved
        assert!(
            matches!(
                &result.new_state,
                RecordingState::LongRecording { shortcut_id, prompt: Some(p), .. }
                    if shortcut_id == "transcribe-e" && p == "grammar"
            ),
            "After first Enter, shortcut_id should still be 'transcribe-e', got: {:?}",
            result.new_state
        );

        assert_eq!(
            result.actions,
            vec![
                Action::UnbindLongRecordingKeys,
                Action::SubmitAndContinue {
                    prompt: Some("grammar".to_string())
                },
                Action::BindLongRecordingKeys,
            ]
        );

        // Simulate time passing (more than double-tap window to be safe)
        let state_after_first_enter = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(), // Should be preserved!
            prompt: Some("grammar".to_string()),
            entered_at: Instant::now() - Duration::from_secs(5),
        };

        // Second Enter - should also trigger SubmitAndContinue, NOT double-tap/finish
        let event2 = HotkeyEvent::Activated {
            shortcut_id: "transcribe-enter".to_string(),
        };
        let result2 = state_after_first_enter.transition(event2, &ctx, Instant::now());

        // Should still be in LongRecording and trigger SubmitAndContinue
        assert!(
            matches!(
                &result2.new_state,
                RecordingState::LongRecording { shortcut_id, .. }
                    if shortcut_id == "transcribe-e"
            ),
            "After second Enter, should still be in LongRecording with 'transcribe-e', got: {:?}",
            result2.new_state
        );

        assert!(
            result2.actions.contains(&Action::SubmitAndContinue {
                prompt: Some("grammar".to_string())
            }),
            "Second Enter should trigger SubmitAndContinue, but got: {:?}",
            result2.actions
        );
    }
}
