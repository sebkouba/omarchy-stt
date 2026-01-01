//! Integration tests for the hotkey state machine.
//!
//! These tests verify multi-event sequences and session-level behavior
//! that the unit tests in `src/hotkey_state.rs` don't cover.
//!
//! The tests simulate realistic user sessions by:
//! 1. Sending sequences of events through the state machine
//! 2. Verifying action ordering and correctness
//! 3. Testing cache behavior across sessions
//!
//! All timing is controlled via explicit `Instant` values - no `thread::sleep()`.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use transcribe_rs::config::HotkeyBinding;
use transcribe_rs::hotkey_state::{Action, HotkeyEvent, RecordingState, StateMachine};

// ─────────────────────────────────────────────────────────────────────────────
// TEST HELPERS
// ─────────────────────────────────────────────────────────────────────────────

/// Standard timing configuration for tests
struct TestConfig {
    tap_threshold: Duration,
    double_tap_window: Duration,
    repaste_debounce: Duration,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            tap_threshold: Duration::from_millis(200),
            double_tap_window: Duration::from_millis(1500),
            repaste_debounce: Duration::from_millis(1000),
        }
    }
}

/// Create a standard binding map for testing
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
    map.insert(
        "transcribe-r".to_string(),
        HotkeyBinding {
            key: "r".to_string(),
            prompt: Some("code".to_string()),
            ocr: false,
            gui: false,
        },
    );
    map
}

/// Helper to send an event and get actions with controlled time
fn send_event(
    sm: &mut StateMachine,
    event: HotkeyEvent,
    bindings: &HashMap<String, HotkeyBinding>,
    config: &TestConfig,
    now: Instant,
) -> Vec<Action> {
    sm.handle_event(
        event,
        bindings,
        config.tap_threshold,
        config.double_tap_window,
        config.repaste_debounce,
        now,
    )
}

/// Helper to create an activation event
fn activate(shortcut_id: &str) -> HotkeyEvent {
    HotkeyEvent::Activated {
        shortcut_id: shortcut_id.to_string(),
    }
}

/// Helper to create a deactivation event
fn deactivate(shortcut_id: &str) -> HotkeyEvent {
    HotkeyEvent::Deactivated {
        shortcut_id: shortcut_id.to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// COMPLETE PTT SESSION TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_complete_ptt_hold_session() {
    // Simulate a complete push-to-talk session:
    // 1. Press and hold key for > tap_threshold
    // 2. Release key
    // 3. Verify transcription action
    // 4. Cache result

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Press key at t0
    let actions = send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    assert_eq!(actions, vec![Action::StartRecording]);
    assert!(matches!(sm.state(), RecordingState::Recording { .. }));

    // Release at t0 + 250ms (> tap_threshold of 200ms) - should transcribe
    let t1 = t0 + Duration::from_millis(250);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);
    assert_eq!(actions, vec![Action::StopAndTranscribe { prompt: None }]);
    assert!(matches!(sm.state(), RecordingState::Idle));

    // Simulate caching the transcription result (normally done by daemon after transcription)
    sm.cache_transcription("Hello world from PTT".to_string());
    assert_eq!(sm.last_transcription(), Some("Hello world from PTT"));
}

#[test]
fn test_complete_ptt_tap_to_long_recording_session() {
    // Simulate tap-to-toggle session:
    // 1. Quick tap (< tap_threshold) -> enters long recording
    // 2. Tap again after double_tap_window -> finishes recording

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Quick tap - press at t0
    let actions = send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    assert_eq!(actions, vec![Action::StartRecording]);

    // Quick release at t0 + 50ms (< tap_threshold of 200ms) - enters long recording
    let t1 = t0 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);
    assert_eq!(actions, vec![Action::BindLongRecordingKeys]);
    assert!(matches!(sm.state(), RecordingState::LongRecording { .. }));

    // Tap again at t1 + 1600ms (> double_tap_window of 1500ms) to finish
    let t2 = t1 + Duration::from_millis(1600);
    let actions = send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t2);
    assert_eq!(actions, vec![Action::UnbindLongRecordingKeys]);
    assert!(matches!(
        sm.state(),
        RecordingState::PendingTranscription { .. }
    ));

    // Release key - should now transcribe
    let t3 = t2 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t3);
    assert_eq!(actions, vec![Action::StopAndTranscribe { prompt: None }]);
    assert!(matches!(sm.state(), RecordingState::Idle));
}

#[test]
fn test_ptt_session_with_prompt() {
    // Verify prompts are correctly passed through the session

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Use the 'q' key which has a grammar prompt
    let actions = send_event(&mut sm, activate("transcribe-q"), &bindings, &config, t0);
    assert_eq!(actions, vec![Action::StartRecording]);

    // Verify state has prompt
    if let RecordingState::Recording { prompt, .. } = sm.state() {
        assert_eq!(prompt.as_deref(), Some("grammar"));
    } else {
        panic!("Expected Recording state");
    }

    // Hold and release at t0 + 250ms
    let t1 = t0 + Duration::from_millis(250);
    let actions = send_event(&mut sm, deactivate("transcribe-q"), &bindings, &config, t1);
    assert_eq!(
        actions,
        vec![Action::StopAndTranscribe {
            prompt: Some("grammar".to_string())
        }]
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// MULTIPLE SESSIONS AND CACHE TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_multiple_sessions_update_cache() {
    // Verify that each new session's transcription replaces the previous cache

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Session 1
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(250);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);
    sm.cache_transcription("First transcription".to_string());

    assert_eq!(sm.last_transcription(), Some("First transcription"));

    // Session 2
    let t2 = t1 + Duration::from_millis(100);
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t2);
    let t3 = t2 + Duration::from_millis(250);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t3);
    sm.cache_transcription("Second transcription".to_string());

    // Cache should now have the new transcription
    assert_eq!(sm.last_transcription(), Some("Second transcription"));
}

#[test]
fn test_double_tap_repaste_uses_cached_transcription() {
    // Verify that double-tap repaste uses the most recent cached transcription

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // First session - do a normal PTT and cache the result
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(250);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);
    sm.cache_transcription("Cached text for repaste".to_string());

    // Now do a double-tap
    // First tap - starts recording
    let t2 = t1 + Duration::from_millis(100);
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t2);
    // Quick release - enters long recording (< tap_threshold)
    let t3 = t2 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t3);
    assert_eq!(actions, vec![Action::BindLongRecordingKeys]);

    // Second tap within double_tap_window - should trigger repaste
    let t4 = t3 + Duration::from_millis(500); // < 1500ms double_tap_window
    let actions = send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t4);

    // Should unbind, cancel, and go to PendingRepaste
    assert_eq!(
        actions,
        vec![Action::UnbindLongRecordingKeys, Action::CancelRecording]
    );
    assert!(matches!(
        sm.state(),
        RecordingState::PendingRepaste { text, .. } if text == "Cached text for repaste"
    ));

    // Release key to complete the repaste
    let t5 = t4 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t5);
    assert_eq!(
        actions,
        vec![Action::Repaste {
            text: "Cached text for repaste".to_string()
        }]
    );
    assert!(matches!(sm.state(), RecordingState::Idle));
}

#[test]
fn test_repaste_after_multiple_sessions() {
    // Verify that repaste uses the LATEST cached transcription after multiple sessions

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Session 1
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(250);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);
    sm.cache_transcription("First text".to_string());

    // Session 2
    let t2 = t1 + Duration::from_millis(100);
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t2);
    let t3 = t2 + Duration::from_millis(250);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t3);
    sm.cache_transcription("Second text".to_string());

    // Session 3
    let t4 = t3 + Duration::from_millis(100);
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t4);
    let t5 = t4 + Duration::from_millis(250);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t5);
    sm.cache_transcription("Third text".to_string());

    // Double-tap to repaste should use "Third text"
    let t6 = t5 + Duration::from_millis(100);
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t6);
    let t7 = t6 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t7);
    let t8 = t7 + Duration::from_millis(500); // < double_tap_window
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t8);
    let t9 = t8 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t9);

    assert_eq!(
        actions,
        vec![Action::Repaste {
            text: "Third text".to_string()
        }]
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// SESSION INTERRUPTION TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_escape_cancels_long_recording() {
    // Verify that Escape properly cancels and allows new session

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Enter long recording mode
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);
    assert!(matches!(sm.state(), RecordingState::LongRecording { .. }));

    // Press Escape
    let t2 = t1 + Duration::from_millis(100);
    let actions = send_event(&mut sm, activate("transcribe-escape"), &bindings, &config, t2);
    assert_eq!(
        actions,
        vec![
            Action::UnbindLongRecordingKeys,
            Action::CancelRecording,
            Action::Notify {
                title: "Recording cancelled".to_string(),
                body: "Press hotkey to start again".to_string(),
            }
        ]
    );
    assert!(matches!(sm.state(), RecordingState::Idle));

    // Should be able to start a new session immediately
    let t3 = t2 + Duration::from_millis(50);
    let actions = send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t3);
    assert_eq!(actions, vec![Action::StartRecording]);
}

#[test]
fn test_escape_does_not_affect_cache() {
    // Canceling a recording should NOT clear the previous cache

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Do a successful session first
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(250);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);
    sm.cache_transcription("Preserved text".to_string());

    // Start another session and cancel it
    let t2 = t1 + Duration::from_millis(100);
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t2);
    let t3 = t2 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t3);
    let t4 = t3 + Duration::from_millis(100);
    send_event(&mut sm, activate("transcribe-escape"), &bindings, &config, t4);

    // Previous transcription should still be cached
    assert_eq!(sm.last_transcription(), Some("Preserved text"));

    // Double-tap should repaste the preserved text
    let t5 = t4 + Duration::from_millis(100);
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t5);
    let t6 = t5 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t6);
    let t7 = t6 + Duration::from_millis(500);
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t7);
    let t8 = t7 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t8);

    assert_eq!(
        actions,
        vec![Action::Repaste {
            text: "Preserved text".to_string()
        }]
    );
}

#[test]
fn test_switching_prompts_during_long_recording() {
    // Pressing a different key during long recording should switch prompts

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Enter long recording with 'e' (no prompt)
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);

    // Wait past double_tap_window so it's not treated as double-tap
    let t2 = t1 + Duration::from_millis(1600);

    // Press 'q' (grammar prompt) - should cancel and restart
    let actions = send_event(&mut sm, activate("transcribe-q"), &bindings, &config, t2);
    assert_eq!(
        actions,
        vec![
            Action::UnbindLongRecordingKeys,
            Action::CancelRecording,
            Action::StartRecording,
        ]
    );

    // Should now be recording with q's prompt
    if let RecordingState::Recording {
        shortcut_id,
        prompt,
        ..
    } = sm.state()
    {
        assert_eq!(shortcut_id, "transcribe-q");
        assert_eq!(prompt.as_deref(), Some("grammar"));
    } else {
        panic!("Expected Recording state with q key");
    }

    // Complete this session
    let t3 = t2 + Duration::from_millis(250);
    let actions = send_event(&mut sm, deactivate("transcribe-q"), &bindings, &config, t3);
    assert_eq!(
        actions,
        vec![Action::StopAndTranscribe {
            prompt: Some("grammar".to_string())
        }]
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// ACTION ORDERING AND BIND/UNBIND TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_bind_unbind_are_always_paired() {
    // Verify that BindLongRecordingKeys is always eventually followed by UnbindLongRecordingKeys

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();
    let mut bind_count = 0i32;

    let t0 = Instant::now();

    // Helper to track bind/unbind balance
    let track_actions = |actions: &[Action], count: &mut i32| {
        for action in actions {
            match action {
                Action::BindLongRecordingKeys => *count += 1,
                Action::UnbindLongRecordingKeys => *count -= 1,
                _ => {}
            }
        }
    };

    // Scenario 1: Enter long recording, then finish normally
    let actions = send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    track_actions(&actions, &mut bind_count);

    let t1 = t0 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);
    track_actions(&actions, &mut bind_count);
    assert_eq!(
        bind_count, 1,
        "Should have 1 bind after entering long recording"
    );

    let t2 = t1 + Duration::from_millis(1600);

    let actions = send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t2);
    track_actions(&actions, &mut bind_count);
    assert_eq!(bind_count, 0, "Should be balanced after finish press");

    let t3 = t2 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t3);
    track_actions(&actions, &mut bind_count);

    assert_eq!(
        bind_count, 0,
        "Bind/unbind should be balanced after full session"
    );

    // Scenario 2: Enter long recording, then escape
    let t4 = t3 + Duration::from_millis(100);
    let actions = send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t4);
    track_actions(&actions, &mut bind_count);

    let t5 = t4 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t5);
    track_actions(&actions, &mut bind_count);
    assert_eq!(bind_count, 1);

    let t6 = t5 + Duration::from_millis(100);
    let actions = send_event(&mut sm, activate("transcribe-escape"), &bindings, &config, t6);
    track_actions(&actions, &mut bind_count);
    assert_eq!(bind_count, 0, "Escape should unbind");
}

#[test]
fn test_enter_key_maintains_bind_state() {
    // Pressing Enter in long recording mode should unbind, submit, then rebind

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Enter long recording
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);

    // Wait past double_tap_window
    let t2 = t1 + Duration::from_millis(1600);

    // Press Enter
    let actions = send_event(&mut sm, activate("transcribe-enter"), &bindings, &config, t2);

    // Should unbind, submit, then rebind (in that order)
    assert_eq!(
        actions,
        vec![
            Action::UnbindLongRecordingKeys,
            Action::SubmitAndContinue { prompt: None },
            Action::BindLongRecordingKeys,
        ]
    );

    // Should still be in long recording
    assert!(matches!(sm.state(), RecordingState::LongRecording { .. }));
}

#[test]
fn test_multiple_enter_presses_in_long_recording() {
    // Regression test: multiple Enter presses should all work correctly

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Enter long recording with grammar prompt
    send_event(&mut sm, activate("transcribe-q"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-q"), &bindings, &config, t1);

    // Wait past double_tap_window
    let t2 = t1 + Duration::from_millis(1600);

    // First Enter
    let actions = send_event(&mut sm, activate("transcribe-enter"), &bindings, &config, t2);
    assert!(actions.contains(&Action::SubmitAndContinue {
        prompt: Some("grammar".to_string())
    }));

    // Verify we're still in LongRecording with original shortcut
    if let RecordingState::LongRecording {
        shortcut_id,
        prompt,
        ..
    } = sm.state()
    {
        assert_eq!(shortcut_id, "transcribe-q");
        assert_eq!(prompt.as_deref(), Some("grammar"));
    } else {
        panic!("Should still be in LongRecording");
    }

    // Wait and do second Enter
    let t3 = t2 + Duration::from_millis(1600);
    let actions = send_event(&mut sm, activate("transcribe-enter"), &bindings, &config, t3);
    assert!(actions.contains(&Action::SubmitAndContinue {
        prompt: Some("grammar".to_string())
    }));

    // Still in LongRecording with original shortcut
    if let RecordingState::LongRecording {
        shortcut_id,
        prompt,
        ..
    } = sm.state()
    {
        assert_eq!(shortcut_id, "transcribe-q");
        assert_eq!(prompt.as_deref(), Some("grammar"));
    } else {
        panic!("Should still be in LongRecording after second Enter");
    }

    // Third Enter
    let t4 = t3 + Duration::from_millis(1600);
    let actions = send_event(&mut sm, activate("transcribe-enter"), &bindings, &config, t4);
    assert!(actions.contains(&Action::SubmitAndContinue {
        prompt: Some("grammar".to_string())
    }));
}

// ─────────────────────────────────────────────────────────────────────────────
// DEBOUNCE TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_repaste_debounce_prevents_accidental_activation() {
    // After a repaste, quick activation should be ignored

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Cache a transcription
    sm.cache_transcription("Text to repaste".to_string());

    // Do a double-tap repaste
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);
    let t2 = t1 + Duration::from_millis(500);
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t2);
    let t3 = t2 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t3);

    // Record the repaste at t3
    sm.record_repaste(t3);

    // Immediate activation at t3 + 500ms (< 1000ms debounce) should be debounced
    let t4 = t3 + Duration::from_millis(500);
    let actions = send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t4);
    assert!(actions.is_empty(), "Should be debounced");
    assert!(matches!(sm.state(), RecordingState::Idle));

    // Activation at t3 + 1100ms (> 1000ms debounce) should work
    let t5 = t3 + Duration::from_millis(1100);
    let actions = send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t5);
    assert_eq!(actions, vec![Action::StartRecording]);
}

// ─────────────────────────────────────────────────────────────────────────────
// EDGE CASE TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_unknown_shortcut_during_recording_is_ignored() {
    // Unknown shortcuts should not affect ongoing recording

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Start recording
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    assert!(matches!(sm.state(), RecordingState::Recording { .. }));

    // Unknown shortcut activation - should be ignored
    let t1 = t0 + Duration::from_millis(100);
    let actions = send_event(&mut sm, activate("transcribe-unknown"), &bindings, &config, t1);
    assert!(actions.is_empty());
    assert!(matches!(sm.state(), RecordingState::Recording { .. }));

    // Unknown shortcut deactivation - should be ignored
    let t2 = t1 + Duration::from_millis(50);
    let actions = send_event(
        &mut sm,
        deactivate("transcribe-unknown"),
        &bindings,
        &config,
        t2,
    );
    assert!(actions.is_empty());
    assert!(matches!(sm.state(), RecordingState::Recording { .. }));
}

#[test]
fn test_wrong_key_release_is_ignored() {
    // Releasing a different key than the one that started recording should be ignored

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Start recording with 'e'
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(250);

    // Release 'q' - should be ignored
    let actions = send_event(&mut sm, deactivate("transcribe-q"), &bindings, &config, t1);
    assert!(actions.is_empty());
    assert!(matches!(sm.state(), RecordingState::Recording { .. }));

    // Release 'e' - should work
    let t2 = t1 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t2);
    assert_eq!(actions, vec![Action::StopAndTranscribe { prompt: None }]);
}

#[test]
fn test_pending_repaste_ignores_other_events() {
    // While waiting for key release in PendingRepaste, other events should be ignored

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Cache a transcription and do double-tap to get to PendingRepaste
    sm.cache_transcription("Pending repaste text".to_string());

    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);
    let t2 = t1 + Duration::from_millis(500);
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t2);
    // Now in PendingRepaste

    assert!(matches!(sm.state(), RecordingState::PendingRepaste { .. }));

    // Activating another key should be ignored
    let t3 = t2 + Duration::from_millis(50);
    let actions = send_event(&mut sm, activate("transcribe-q"), &bindings, &config, t3);
    assert!(actions.is_empty());
    assert!(matches!(sm.state(), RecordingState::PendingRepaste { .. }));

    // Releasing wrong key should be ignored
    let t4 = t3 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-q"), &bindings, &config, t4);
    assert!(actions.is_empty());

    // Only releasing the correct key should complete repaste
    let t5 = t4 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t5);
    assert_eq!(
        actions,
        vec![Action::Repaste {
            text: "Pending repaste text".to_string()
        }]
    );
}

#[test]
fn test_pending_transcription_ignores_other_events() {
    // While waiting for key release in PendingTranscription, other events should be ignored

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Enter long recording and tap to finish
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);
    let t2 = t1 + Duration::from_millis(1600);
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t2);
    // Now in PendingTranscription

    assert!(matches!(
        sm.state(),
        RecordingState::PendingTranscription { .. }
    ));

    // Other events should be ignored
    let t3 = t2 + Duration::from_millis(50);
    let actions = send_event(&mut sm, activate("transcribe-q"), &bindings, &config, t3);
    assert!(actions.is_empty());

    let t4 = t3 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-q"), &bindings, &config, t4);
    assert!(actions.is_empty());

    // Only correct key release completes
    let t5 = t4 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t5);
    assert_eq!(actions, vec![Action::StopAndTranscribe { prompt: None }]);
}

#[test]
fn test_idle_ignores_deactivation() {
    // Deactivation while idle should be ignored

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t0);
    assert!(actions.is_empty());
    assert!(matches!(sm.state(), RecordingState::Idle));
}

// ─────────────────────────────────────────────────────────────────────────────
// REALISTIC USER SESSION TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_realistic_session_ptt_then_repaste() {
    // Simulate: User does PTT -> transcribes "hello" -> double-tap to repaste

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // PTT session
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(300);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);
    assert_eq!(actions, vec![Action::StopAndTranscribe { prompt: None }]);

    // Daemon transcribes and caches
    sm.cache_transcription("hello".to_string());

    // User does double-tap to repaste
    let t2 = t1 + Duration::from_millis(100);
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t2);
    let t3 = t2 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t3);
    let t4 = t3 + Duration::from_millis(500);
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t4);
    let t5 = t4 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t5);

    assert_eq!(
        actions,
        vec![Action::Repaste {
            text: "hello".to_string()
        }]
    );
}

#[test]
fn test_realistic_session_long_recording_with_enter() {
    // Simulate: User taps to start -> speaks -> Enter (submit) -> speaks more -> tap to finish

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Tap to start long recording
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);
    assert!(matches!(sm.state(), RecordingState::LongRecording { .. }));

    // User speaks for a while...
    let t2 = t1 + Duration::from_millis(1600);

    // Press Enter to submit first part
    let actions = send_event(&mut sm, activate("transcribe-enter"), &bindings, &config, t2);
    assert!(actions.contains(&Action::SubmitAndContinue { prompt: None }));

    // User speaks some more...
    let t3 = t2 + Duration::from_millis(1600);

    // Tap to finish
    let actions = send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t3);
    assert_eq!(actions, vec![Action::UnbindLongRecordingKeys]);

    let t4 = t3 + Duration::from_millis(50);
    let actions = send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t4);
    assert_eq!(actions, vec![Action::StopAndTranscribe { prompt: None }]);
    assert!(matches!(sm.state(), RecordingState::Idle));
}

#[test]
fn test_realistic_session_mistake_and_cancel() {
    // Simulate: User taps to start -> realizes mistake -> Escape -> new recording

    let bindings = make_binding_map();
    let config = TestConfig::default();
    let mut sm = StateMachine::new();

    let t0 = Instant::now();

    // Tap to start
    send_event(&mut sm, activate("transcribe-e"), &bindings, &config, t0);
    let t1 = t0 + Duration::from_millis(50);
    send_event(&mut sm, deactivate("transcribe-e"), &bindings, &config, t1);

    // Oops, wrong hotkey! Cancel
    let t2 = t1 + Duration::from_millis(100);
    let actions = send_event(&mut sm, activate("transcribe-escape"), &bindings, &config, t2);
    assert!(actions.contains(&Action::CancelRecording));
    assert!(matches!(sm.state(), RecordingState::Idle));

    // Start again with correct prompt
    let t3 = t2 + Duration::from_millis(50);
    let actions = send_event(&mut sm, activate("transcribe-q"), &bindings, &config, t3);
    assert_eq!(actions, vec![Action::StartRecording]);

    if let RecordingState::Recording { prompt, .. } = sm.state() {
        assert_eq!(prompt.as_deref(), Some("grammar"));
    }
}
