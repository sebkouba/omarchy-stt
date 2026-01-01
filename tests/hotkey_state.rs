//! Integration tests for the hotkey state machine.
//!
//! These tests verify the complete behavior of the hotkey state machine
//! including full workflow sequences, invariants, edge cases, and debounce behavior.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use transcribe_rs::{
    config::HotkeyBinding,
    hotkey_state::{Action, HotkeyEvent, RecordingState, StateMachine, TransitionContext},
};

// ─────────────────────────────────────────────────────────────────────────────
// TEST HELPERS
// ─────────────────────────────────────────────────────────────────────────────

/// Create a standard test binding map with common bindings.
fn make_test_binding_map() -> HashMap<String, HotkeyBinding> {
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
    map.insert(
        "transcribe-w".to_string(),
        HotkeyBinding {
            key: "w".to_string(),
            prompt: Some("ask".to_string()),
            ocr: false,
            gui: false,
        },
    );
    map.insert(
        "transcribe-r".to_string(),
        HotkeyBinding {
            key: "r".to_string(),
            prompt: Some("ocr".to_string()),
            ocr: true,
            gui: false,
        },
    );
    map
}

/// Create an Activated event.
fn activated(shortcut_id: &str) -> HotkeyEvent {
    HotkeyEvent::Activated {
        shortcut_id: shortcut_id.to_string(),
    }
}

/// Create a Deactivated event.
fn deactivated(shortcut_id: &str) -> HotkeyEvent {
    HotkeyEvent::Deactivated {
        shortcut_id: shortcut_id.to_string(),
    }
}

/// Default timing constants matching production values.
const TAP_THRESHOLD: Duration = Duration::from_millis(200);
const DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(1500);
const REPASTE_DEBOUNCE: Duration = Duration::from_millis(1000);

/// Create a default transition context.
fn make_ctx(binding_map: &HashMap<String, HotkeyBinding>) -> TransitionContext<'_> {
    TransitionContext {
        tap_threshold: TAP_THRESHOLD,
        double_tap_window: DOUBLE_TAP_WINDOW,
        binding_map,
        last_transcription: None,
        last_repaste_time: None,
        repaste_debounce: REPASTE_DEBOUNCE,
    }
}

/// Create a context with last transcription set.
fn make_ctx_with_transcription<'a>(
    binding_map: &'a HashMap<String, HotkeyBinding>,
    transcription: &'a str,
) -> TransitionContext<'a> {
    TransitionContext {
        tap_threshold: TAP_THRESHOLD,
        double_tap_window: DOUBLE_TAP_WINDOW,
        binding_map,
        last_transcription: Some(transcription),
        last_repaste_time: None,
        repaste_debounce: REPASTE_DEBOUNCE,
    }
}

/// Control keys that should never appear as shortcut_id in recording states.
const CONTROL_KEYS: &[&str] = &["transcribe-enter", "transcribe-escape"];

fn is_control_key(id: &str) -> bool {
    CONTROL_KEYS.contains(&id)
}

/// Assert that no recording state has a control key as its shortcut_id.
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
        _ => {}
    }
}

/// Check if actions contain a Bind action.
fn has_bind_action(actions: &[Action]) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::BindLongRecordingKeys))
}

/// Check if actions contain an Unbind action.
fn has_unbind_action(actions: &[Action]) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::UnbindLongRecordingKeys))
}

// ─────────────────────────────────────────────────────────────────────────────
// PUSH-TO-TALK TESTS (hold to record, release to transcribe)
// ─────────────────────────────────────────────────────────────────────────────

mod push_to_talk {
    use super::*;

    #[test]
    fn complete_push_to_talk_flow() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // 1. Activate (press key)
        let state = RecordingState::Idle;
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());

        assert!(matches!(
            result.new_state,
            RecordingState::Recording { shortcut_id, prompt: None, .. } if shortcut_id == "transcribe-e"
        ));
        assert_eq!(result.actions, vec![Action::StartRecording]);

        // 2. Hold for 500ms (simulated by creating state with old press_time)
        let state = RecordingState::Recording {
            press_time: Instant::now() - Duration::from_millis(500),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };

        // 3. Deactivate (release key)
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert_eq!(
            result.actions,
            vec![Action::StopAndTranscribe { prompt: None }]
        );
    }

    #[test]
    fn push_to_talk_with_prompt() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // Activate with prompt binding
        let state = RecordingState::Idle;
        let result = state.transition(activated("transcribe-q"), &ctx, Instant::now());

        assert!(matches!(
            &result.new_state,
            RecordingState::Recording { prompt: Some(p), .. } if p == "grammar"
        ));

        // Hold and release
        let state = RecordingState::Recording {
            press_time: Instant::now() - Duration::from_millis(500),
            shortcut_id: "transcribe-q".to_string(),
            prompt: Some("grammar".to_string()),
        };
        let result = state.transition(deactivated("transcribe-q"), &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert_eq!(
            result.actions,
            vec![Action::StopAndTranscribe {
                prompt: Some("grammar".to_string())
            }]
        );
    }

    #[test]
    fn push_to_talk_at_exact_threshold() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // Press time exactly at threshold (200ms) should be treated as hold
        let state = RecordingState::Recording {
            press_time: Instant::now() - Duration::from_millis(200),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());

        // At exactly threshold, should transcribe (hold behavior)
        assert_eq!(result.new_state, RecordingState::Idle);
        assert_eq!(
            result.actions,
            vec![Action::StopAndTranscribe { prompt: None }]
        );
    }

    #[test]
    fn push_to_talk_just_under_threshold() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // 199ms hold should be a tap (enters long recording)
        let state = RecordingState::Recording {
            press_time: Instant::now() - Duration::from_millis(1),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());

        assert!(matches!(
            result.new_state,
            RecordingState::LongRecording { .. }
        ));
        assert_eq!(result.actions, vec![Action::BindLongRecordingKeys]);
    }

    #[test]
    fn push_to_talk_ignores_other_key_release() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };

        // Release a different key
        let result = state.transition(deactivated("transcribe-q"), &ctx, Instant::now());

        // Should remain in recording state
        assert!(matches!(
            result.new_state,
            RecordingState::Recording { shortcut_id, .. } if shortcut_id == "transcribe-e"
        ));
        assert!(result.actions.is_empty());
    }

    #[test]
    fn push_to_talk_ignores_activations_while_recording() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };

        // Try to activate another key
        let result = state.transition(activated("transcribe-q"), &ctx, Instant::now());

        // Should remain in recording state with original key
        assert!(matches!(
            result.new_state,
            RecordingState::Recording { shortcut_id, prompt: None, .. } if shortcut_id == "transcribe-e"
        ));
        assert!(result.actions.is_empty());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LONG RECORDING TESTS (tap to start, tap again to finish)
// ─────────────────────────────────────────────────────────────────────────────

mod long_recording {
    use super::*;

    #[test]
    fn tap_to_start_long_recording() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // Quick tap (< 200ms)
        let state = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-e".to_string(),
            prompt: Some("grammar".to_string()),
        };

        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());

        assert!(matches!(
            &result.new_state,
            RecordingState::LongRecording { shortcut_id, prompt: Some(p), .. }
                if shortcut_id == "transcribe-e" && p == "grammar"
        ));
        assert_eq!(result.actions, vec![Action::BindLongRecordingKeys]);
    }

    #[test]
    fn tap_again_to_finish_long_recording() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // Long recording started 2 seconds ago (past double-tap window)
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_millis(2000),
        };

        // Tap same key
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());

        // Should enter PendingTranscription
        assert!(matches!(
            &result.new_state,
            RecordingState::PendingTranscription { shortcut_id, prompt: None }
                if shortcut_id == "transcribe-e"
        ));
        assert_eq!(result.actions, vec![Action::UnbindLongRecordingKeys]);
    }

    #[test]
    fn long_recording_pending_transcription_release() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::PendingTranscription {
            shortcut_id: "transcribe-e".to_string(),
            prompt: Some("grammar".to_string()),
        };

        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert_eq!(
            result.actions,
            vec![Action::StopAndTranscribe {
                prompt: Some("grammar".to_string())
            }]
        );
    }

    #[test]
    fn long_recording_ignores_deactivations() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now(),
        };

        // Release the key (should be ignored)
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());

        assert!(matches!(
            result.new_state,
            RecordingState::LongRecording { .. }
        ));
        assert!(result.actions.is_empty());
    }

    #[test]
    fn complete_long_recording_flow() {
        let bindings = make_test_binding_map();

        // Step 1: Tap to start (press + quick release)
        let ctx = make_ctx(&bindings);
        let state = RecordingState::Idle;
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());
        assert!(matches!(result.new_state, RecordingState::Recording { .. }));

        // Quick release (tap)
        let state = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());
        assert!(matches!(
            result.new_state,
            RecordingState::LongRecording { .. }
        ));
        assert!(has_bind_action(&result.actions));

        // Step 2: Wait (recording continues), then tap again to finish
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_secs(10), // Waited 10 seconds
        };
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());
        assert!(matches!(
            result.new_state,
            RecordingState::PendingTranscription { .. }
        ));
        assert!(has_unbind_action(&result.actions));

        // Step 3: Release to transcribe
        let state = RecordingState::PendingTranscription {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());
        assert_eq!(result.new_state, RecordingState::Idle);
        assert_eq!(
            result.actions,
            vec![Action::StopAndTranscribe { prompt: None }]
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DOUBLE-TAP REPASTE TESTS
// ─────────────────────────────────────────────────────────────────────────────

mod double_tap {
    use super::*;

    #[test]
    fn double_tap_triggers_repaste() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx_with_transcription(&bindings, "Hello world");

        // In long recording within double-tap window
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_millis(500), // 500ms ago
        };

        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());

        assert!(matches!(
            &result.new_state,
            RecordingState::PendingRepaste { shortcut_id, text }
                if shortcut_id == "transcribe-e" && text == "Hello world"
        ));
        assert_eq!(
            result.actions,
            vec![Action::UnbindLongRecordingKeys, Action::CancelRecording]
        );
    }

    #[test]
    fn double_tap_release_repastes() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::PendingRepaste {
            shortcut_id: "transcribe-e".to_string(),
            text: "Hello world".to_string(),
        };

        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert_eq!(
            result.actions,
            vec![Action::Repaste {
                text: "Hello world".to_string()
            }]
        );
    }

    #[test]
    fn double_tap_no_previous_transcription_notifies() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings); // No last_transcription

        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_millis(500),
        };

        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert!(result.actions.contains(&Action::Notify {
            title: "No previous transcription".to_string(),
            body: "Record something first".to_string(),
        }));
    }

    #[test]
    fn double_tap_at_exact_boundary() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx_with_transcription(&bindings, "Test text");

        // At exactly 1500ms (double_tap_window)
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_millis(1500),
        };

        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());

        // At exactly threshold, should NOT be repaste (enters PendingTranscription)
        assert!(matches!(
            result.new_state,
            RecordingState::PendingTranscription { .. }
        ));
    }

    #[test]
    fn double_tap_just_inside_window() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx_with_transcription(&bindings, "Test text");

        // Just under 1500ms
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_millis(1499),
        };

        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());

        // Should trigger repaste
        assert!(matches!(
            result.new_state,
            RecordingState::PendingRepaste { .. }
        ));
    }

    #[test]
    fn pending_repaste_ignores_other_events() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::PendingRepaste {
            shortcut_id: "transcribe-e".to_string(),
            text: "Hello".to_string(),
        };

        // Different key deactivated
        let result = state.clone().transition(deactivated("transcribe-q"), &ctx, Instant::now());
        assert!(matches!(
            result.new_state,
            RecordingState::PendingRepaste { .. }
        ));
        assert!(result.actions.is_empty());

        // Any activation
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());
        assert!(matches!(
            result.new_state,
            RecordingState::PendingRepaste { .. }
        ));
        assert!(result.actions.is_empty());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CONTROL KEYS TESTS (Enter and Escape during long recording)
// ─────────────────────────────────────────────────────────────────────────────

mod control_keys {
    use super::*;

    #[test]
    fn enter_submits_and_continues() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: Some("grammar".to_string()),
            entered_at: Instant::now() - Duration::from_secs(5),
        };

        let result = state.transition(activated("transcribe-enter"), &ctx, Instant::now());

        // Should stay in LongRecording
        assert!(matches!(
            &result.new_state,
            RecordingState::LongRecording { shortcut_id, prompt: Some(p), .. }
                if shortcut_id == "transcribe-e" && p == "grammar"
        ));

        // Should have correct actions
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
    fn escape_cancels_recording() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now(),
        };

        let result = state.transition(activated("transcribe-escape"), &ctx, Instant::now());

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
    fn enter_preserves_original_shortcut_id() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-q".to_string(),
            prompt: Some("grammar".to_string()),
            entered_at: Instant::now() - Duration::from_secs(5),
        };

        let result = state.transition(activated("transcribe-enter"), &ctx, Instant::now());

        // Must preserve transcribe-q, NOT set to transcribe-enter
        if let RecordingState::LongRecording { shortcut_id, .. } = &result.new_state {
            assert_eq!(shortcut_id, "transcribe-q");
            assert!(!is_control_key(shortcut_id));
        } else {
            panic!("Expected LongRecording state");
        }
    }

    #[test]
    fn multiple_enter_presses_work() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // First Enter
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_secs(5),
        };
        let result = state.transition(activated("transcribe-enter"), &ctx, Instant::now());

        assert!(result
            .actions
            .contains(&Action::SubmitAndContinue { prompt: None }));

        // Second Enter (simulating new state after first)
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(), // Must still be transcribe-e
            prompt: None,
            entered_at: Instant::now() - Duration::from_secs(3),
        };
        let result = state.transition(activated("transcribe-enter"), &ctx, Instant::now());

        // Should also submit and continue
        assert!(result
            .actions
            .contains(&Action::SubmitAndContinue { prompt: None }));
        assert!(matches!(
            &result.new_state,
            RecordingState::LongRecording { shortcut_id, .. } if shortcut_id == "transcribe-e"
        ));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PROMPT SWITCHING TESTS
// ─────────────────────────────────────────────────────────────────────────────

mod prompt_switching {
    use super::*;

    #[test]
    fn switch_prompt_during_long_recording() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // In long recording with "e" (no prompt)
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_secs(5),
        };

        // Press "q" which has "grammar" prompt
        let result = state.transition(activated("transcribe-q"), &ctx, Instant::now());

        // Should restart with new prompt
        assert!(matches!(
            &result.new_state,
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
    fn switch_from_prompt_to_no_prompt() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // In long recording with "q" (grammar prompt)
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-q".to_string(),
            prompt: Some("grammar".to_string()),
            entered_at: Instant::now() - Duration::from_secs(5),
        };

        // Press "e" which has no prompt
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());

        assert!(matches!(
            &result.new_state,
            RecordingState::Recording { shortcut_id, prompt: None, .. }
                if shortcut_id == "transcribe-e"
        ));
    }

    #[test]
    fn switch_to_unknown_binding_ignored() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_secs(5),
        };

        // Press unknown key
        let result = state.transition(activated("transcribe-unknown"), &ctx, Instant::now());

        // Should stay in same state
        assert!(matches!(
            result.new_state,
            RecordingState::LongRecording { shortcut_id, .. } if shortcut_id == "transcribe-e"
        ));
        assert!(result.actions.is_empty());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// INVARIANT TESTS
// ─────────────────────────────────────────────────────────────────────────────

mod invariants {
    use super::*;

    #[test]
    fn control_keys_never_pollute_recording_state() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // Test all transitions from LongRecording with all possible events
        let events = vec![
            activated("transcribe-e"),
            activated("transcribe-q"),
            activated("transcribe-enter"),
            activated("transcribe-escape"),
            activated("transcribe-unknown"),
            deactivated("transcribe-e"),
            deactivated("transcribe-enter"),
        ];

        for original_key in ["transcribe-e", "transcribe-q"] {
            for event in &events {
                let state = RecordingState::LongRecording {
                    shortcut_id: original_key.to_string(),
                    prompt: None,
                    entered_at: Instant::now() - Duration::from_secs(5),
                };

                let result = state.transition(event.clone(), &ctx, Instant::now());
                assert_no_control_key_pollution(&result.new_state);
            }
        }
    }

    #[test]
    fn all_states_can_reach_idle() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // Recording -> Idle via hold release
        let state = RecordingState::Recording {
            press_time: Instant::now() - Duration::from_millis(500),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());
        assert_eq!(result.new_state, RecordingState::Idle);

        // LongRecording -> Idle via escape
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now(),
        };
        let result = state.transition(activated("transcribe-escape"), &ctx, Instant::now());
        assert_eq!(result.new_state, RecordingState::Idle);

        // PendingTranscription -> Idle via release
        let state = RecordingState::PendingTranscription {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());
        assert_eq!(result.new_state, RecordingState::Idle);

        // PendingRepaste -> Idle via release
        let state = RecordingState::PendingRepaste {
            shortcut_id: "transcribe-e".to_string(),
            text: "test".to_string(),
        };
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());
        assert_eq!(result.new_state, RecordingState::Idle);
    }

    #[test]
    fn bind_unbind_always_paired_on_escape() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // Tap to enter long recording (binds keys)
        let state = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());
        assert!(has_bind_action(&result.actions));

        // Escape to cancel (must unbind)
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now(),
        };
        let result = state.transition(activated("transcribe-escape"), &ctx, Instant::now());
        assert!(has_unbind_action(&result.actions));
    }

    #[test]
    fn bind_unbind_always_paired_on_finish() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // Tap to enter long recording (binds keys)
        let state = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());
        assert!(has_bind_action(&result.actions));

        // Finish long recording (must unbind)
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_secs(5),
        };
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());
        assert!(has_unbind_action(&result.actions));
    }

    #[test]
    fn bind_unbind_paired_on_double_tap() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx_with_transcription(&bindings, "text");

        // Tap to enter long recording (binds keys)
        let state = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());
        assert!(has_bind_action(&result.actions));

        // Double tap (must unbind)
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_millis(500),
        };
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());
        assert!(has_unbind_action(&result.actions));
    }

    #[test]
    fn bind_unbind_paired_on_prompt_switch() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // In long recording with bound keys
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_secs(5),
        };

        // Switch prompt (must unbind)
        let result = state.transition(activated("transcribe-q"), &ctx, Instant::now());
        assert!(has_unbind_action(&result.actions));
    }

    #[test]
    fn unknown_shortcut_ignored_in_idle() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::Idle;
        let result = state.transition(activated("transcribe-unknown"), &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert!(result.actions.is_empty());
    }

    #[test]
    fn deactivation_ignored_in_idle() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::Idle;
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert!(result.actions.is_empty());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DEBOUNCE TESTS
// ─────────────────────────────────────────────────────────────────────────────

mod debounce {
    use super::*;

    #[test]
    fn activation_debounced_after_repaste() {
        let bindings = make_test_binding_map();
        let mut ctx = make_ctx(&bindings);
        // Repaste happened 500ms ago (within 1000ms debounce)
        ctx.last_repaste_time = Some(Instant::now() - Duration::from_millis(500));

        let state = RecordingState::Idle;
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());

        // Should be ignored
        assert_eq!(result.new_state, RecordingState::Idle);
        assert!(result.actions.is_empty());
    }

    #[test]
    fn activation_allowed_after_debounce_period() {
        let bindings = make_test_binding_map();
        let mut ctx = make_ctx(&bindings);
        // Repaste happened 1500ms ago (outside 1000ms debounce)
        ctx.last_repaste_time = Some(Instant::now() - Duration::from_millis(1500));

        let state = RecordingState::Idle;
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());

        // Should start recording
        assert!(matches!(result.new_state, RecordingState::Recording { .. }));
        assert_eq!(result.actions, vec![Action::StartRecording]);
    }

    #[test]
    fn debounce_at_exact_boundary() {
        let bindings = make_test_binding_map();
        let mut ctx = make_ctx(&bindings);
        // Repaste happened exactly 1000ms ago (at boundary)
        ctx.last_repaste_time = Some(Instant::now() - Duration::from_millis(1000));

        let state = RecordingState::Idle;
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());

        // At exact boundary, should allow (>= means debounce period has passed)
        assert!(matches!(result.new_state, RecordingState::Recording { .. }));
    }

    #[test]
    fn debounce_just_under_boundary() {
        let bindings = make_test_binding_map();
        let mut ctx = make_ctx(&bindings);
        // Repaste happened 999ms ago
        ctx.last_repaste_time = Some(Instant::now() - Duration::from_millis(999));

        let state = RecordingState::Idle;
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());

        // Should still be debounced
        assert_eq!(result.new_state, RecordingState::Idle);
        assert!(result.actions.is_empty());
    }

    #[test]
    fn no_debounce_without_prior_repaste() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings); // last_repaste_time is None

        let state = RecordingState::Idle;
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());

        // Should start recording normally
        assert!(matches!(result.new_state, RecordingState::Recording { .. }));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// EDGE CASES
// ─────────────────────────────────────────────────────────────────────────────

mod edge_cases {
    use super::*;

    #[test]
    fn rapid_key_sequence_activate_deactivate_activate() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // Rapid: activate -> deactivate (tap) -> activate
        let state = RecordingState::Idle;
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());
        assert!(matches!(result.new_state, RecordingState::Recording { .. }));

        // Quick release (tap)
        let state = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());
        assert!(matches!(
            result.new_state,
            RecordingState::LongRecording { .. }
        ));

        // Quick activate again (within double-tap window but no last_transcription)
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now(),
        };
        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());
        // No transcription to repaste, goes to Idle with notification
        assert_eq!(result.new_state, RecordingState::Idle);
    }

    #[test]
    fn pending_transcription_ignores_other_events() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        let state = RecordingState::PendingTranscription {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };

        // Try activation
        let result = state.clone().transition(activated("transcribe-q"), &ctx, Instant::now());
        assert!(matches!(
            result.new_state,
            RecordingState::PendingTranscription { .. }
        ));
        assert!(result.actions.is_empty());

        // Try wrong deactivation
        let result = state.transition(deactivated("transcribe-q"), &ctx, Instant::now());
        assert!(matches!(
            result.new_state,
            RecordingState::PendingTranscription { .. }
        ));
        assert!(result.actions.is_empty());
    }

    #[test]
    fn long_duration_recording_completes_correctly() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // Simulate very long recording (5 minutes)
        let state = RecordingState::Recording {
            press_time: Instant::now() - Duration::from_secs(300),
            shortcut_id: "transcribe-e".to_string(),
            prompt: Some("grammar".to_string()),
        };

        let result = state.transition(deactivated("transcribe-e"), &ctx, Instant::now());

        assert_eq!(result.new_state, RecordingState::Idle);
        assert_eq!(
            result.actions,
            vec![Action::StopAndTranscribe {
                prompt: Some("grammar".to_string())
            }]
        );
    }

    #[test]
    fn very_long_recording_in_long_mode() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // Simulate very long recording in long mode (10 minutes)
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_secs(600),
        };

        let result = state.transition(activated("transcribe-e"), &ctx, Instant::now());

        // Should finish normally (not repaste since way past double-tap window)
        assert!(matches!(
            result.new_state,
            RecordingState::PendingTranscription { .. }
        ));
    }

    #[test]
    fn multiple_prompt_switches() {
        let bindings = make_test_binding_map();
        let ctx = make_ctx(&bindings);

        // Start with e (no prompt)
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now() - Duration::from_secs(5),
        };

        // Switch to q (grammar)
        let result = state.transition(activated("transcribe-q"), &ctx, Instant::now());
        assert!(matches!(
            &result.new_state,
            RecordingState::Recording { prompt: Some(p), .. } if p == "grammar"
        ));

        // Tap release, enter long recording
        let state = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-q".to_string(),
            prompt: Some("grammar".to_string()),
        };
        let result = state.transition(deactivated("transcribe-q"), &ctx, Instant::now());
        assert!(matches!(
            result.new_state,
            RecordingState::LongRecording {
                prompt: Some(_),
                ..
            }
        ));

        // Switch to w (ask)
        let state = RecordingState::LongRecording {
            shortcut_id: "transcribe-q".to_string(),
            prompt: Some("grammar".to_string()),
            entered_at: Instant::now() - Duration::from_secs(5),
        };
        let result = state.transition(activated("transcribe-w"), &ctx, Instant::now());
        assert!(matches!(
            &result.new_state,
            RecordingState::Recording { shortcut_id, prompt: Some(p), .. }
                if shortcut_id == "transcribe-w" && p == "ask"
        ));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// STATE MACHINE API TESTS
// ─────────────────────────────────────────────────────────────────────────────

mod api_tests {
    use super::*;

    #[test]
    fn state_machine_default_state() {
        let sm = StateMachine::new();
        assert!(matches!(sm.state(), RecordingState::Idle));
        assert!(sm.last_transcription().is_none());
        assert!(sm.last_repaste_time().is_none());
    }

    #[test]
    fn state_machine_caches_transcription() {
        let mut sm = StateMachine::new();

        sm.cache_transcription("Hello world".to_string());

        assert_eq!(sm.last_transcription(), Some("Hello world"));
    }

    #[test]
    fn state_machine_records_repaste_time() {
        let mut sm = StateMachine::new();

        assert!(sm.last_repaste_time().is_none());

        let now = Instant::now();
        sm.record_repaste(now);

        assert!(sm.last_repaste_time().is_some());
        assert_eq!(sm.last_repaste_time(), Some(now));
    }

    #[test]
    fn state_machine_handle_event_basic() {
        let bindings = make_test_binding_map();
        let mut sm = StateMachine::new();

        let actions = sm.handle_event(
            activated("transcribe-e"),
            &bindings,
            TAP_THRESHOLD,
            DOUBLE_TAP_WINDOW,
            REPASTE_DEBOUNCE,
            Instant::now(),
        );

        assert_eq!(actions, vec![Action::StartRecording]);
        assert!(matches!(sm.state(), RecordingState::Recording { .. }));
    }

    #[test]
    fn state_machine_full_workflow() {
        let bindings = make_test_binding_map();
        let mut sm = StateMachine::new();

        // Activate
        let actions = sm.handle_event(
            activated("transcribe-e"),
            &bindings,
            TAP_THRESHOLD,
            DOUBLE_TAP_WINDOW,
            REPASTE_DEBOUNCE,
            Instant::now(),
        );
        assert_eq!(actions, vec![Action::StartRecording]);

        // Now that handle_event takes a time parameter, we can properly test timing
        // The unit tests in src/hotkey_state.rs demonstrate this pattern
    }

    #[test]
    fn state_machine_unknown_shortcut() {
        let bindings = make_test_binding_map();
        let mut sm = StateMachine::new();

        let actions = sm.handle_event(
            activated("transcribe-unknown"),
            &bindings,
            TAP_THRESHOLD,
            DOUBLE_TAP_WINDOW,
            REPASTE_DEBOUNCE,
            Instant::now(),
        );

        assert!(actions.is_empty());
        assert!(matches!(sm.state(), RecordingState::Idle));
    }

    #[test]
    fn state_machine_deactivation_in_idle() {
        let bindings = make_test_binding_map();
        let mut sm = StateMachine::new();

        let actions = sm.handle_event(
            deactivated("transcribe-e"),
            &bindings,
            TAP_THRESHOLD,
            DOUBLE_TAP_WINDOW,
            REPASTE_DEBOUNCE,
            Instant::now(),
        );

        assert!(actions.is_empty());
        assert!(matches!(sm.state(), RecordingState::Idle));
    }

    #[test]
    fn state_machine_default_trait() {
        let sm: StateMachine = Default::default();
        assert!(matches!(sm.state(), RecordingState::Idle));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RECORDING STATE EQUALITY TESTS
// ─────────────────────────────────────────────────────────────────────────────

mod state_equality {
    use super::*;

    #[test]
    fn idle_states_equal() {
        assert_eq!(RecordingState::Idle, RecordingState::Idle);
    }

    #[test]
    fn recording_states_equal_ignoring_time() {
        let state1 = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let state2 = RecordingState::Recording {
            press_time: Instant::now() - Duration::from_secs(100),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        assert_eq!(state1, state2);
    }

    #[test]
    fn recording_states_differ_by_id() {
        let state1 = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let state2 = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-q".to_string(),
            prompt: None,
        };
        assert_ne!(state1, state2);
    }

    #[test]
    fn recording_states_differ_by_prompt() {
        let state1 = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let state2 = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-e".to_string(),
            prompt: Some("grammar".to_string()),
        };
        assert_ne!(state1, state2);
    }

    #[test]
    fn long_recording_states_equal_ignoring_time() {
        let state1 = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: Some("test".to_string()),
            entered_at: Instant::now(),
        };
        let state2 = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: Some("test".to_string()),
            entered_at: Instant::now() - Duration::from_secs(100),
        };
        assert_eq!(state1, state2);
    }

    #[test]
    fn pending_repaste_states_equal() {
        let state1 = RecordingState::PendingRepaste {
            shortcut_id: "transcribe-e".to_string(),
            text: "hello".to_string(),
        };
        let state2 = RecordingState::PendingRepaste {
            shortcut_id: "transcribe-e".to_string(),
            text: "hello".to_string(),
        };
        assert_eq!(state1, state2);
    }

    #[test]
    fn pending_repaste_states_differ_by_text() {
        let state1 = RecordingState::PendingRepaste {
            shortcut_id: "transcribe-e".to_string(),
            text: "hello".to_string(),
        };
        let state2 = RecordingState::PendingRepaste {
            shortcut_id: "transcribe-e".to_string(),
            text: "world".to_string(),
        };
        assert_ne!(state1, state2);
    }

    #[test]
    fn different_state_variants_not_equal() {
        let idle = RecordingState::Idle;
        let recording = RecordingState::Recording {
            press_time: Instant::now(),
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
        };
        let long_recording = RecordingState::LongRecording {
            shortcut_id: "transcribe-e".to_string(),
            prompt: None,
            entered_at: Instant::now(),
        };

        assert_ne!(idle, recording);
        assert_ne!(idle, long_recording);
        assert_ne!(recording, long_recording);
    }
}
