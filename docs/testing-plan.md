# Hotkey Daemon Testing Plan

This document outlines a phased approach to add comprehensive testing to the hotkey-daemon, enabling safe refactoring of the 1000+ line file.

---

## Phase 1: Unit Tests for Existing Pure Logic

**Goal**: Add tests for existing pure functions without any code changes to hotkey-daemon.rs.

**Dependencies to add**:
```toml
[dev-dependencies]
test-case = "3"  # Parameterized tests
```

### 1.1 Test `parse_key()` function

**File**: `tests/hotkey_key_parse.rs`

**Tests to write**:
- All lowercase letter keys (a-z) → `Key::KEY_A` through `Key::KEY_Z`
- All number keys (0-9) → `Key::KEY_0` through `Key::KEY_9`
- Function keys (f1-f12) → `Key::KEY_F1` through `Key::KEY_F12`
- Special keys: space, enter, return, escape, esc, tab, backspace
- Case insensitivity: "A", "a", "ENTER", "Enter", "enter" all work
- Invalid keys return `None`: "invalid", "KEY_A", "", "ctrl"

**Approach**: Use `test-case` for parameterized tests to cover all ~50 key mappings efficiently.

**Blocker**: `parse_key()` is currently private in the binary. We need to either:
- Option A: Move to library (`src/hotkey/key_parse.rs`) and re-export
- Option B: Add `#[cfg(test)]` module in binary with tests inline

**Recommendation**: Option A (move to library) because this enables Phase 2 extraction.

### 1.2 Test `ModifierState` struct

**File**: `tests/hotkey_modifiers.rs`

**Tests for `ModifierState::update()`**:
- Press left ctrl → `ctrl` becomes true
- Press right ctrl → `ctrl` becomes true
- Release left ctrl → `ctrl` becomes false
- Press ctrl, press shift, release ctrl → `ctrl=false, shift=true`
- Repeat events (value=2) are ignored, state unchanged
- All modifier pairs: ctrl, alt, shift, meta (left/right variants)

**Tests for `ModifierState::all_released()`**:
- Default state → true
- Any single modifier pressed → false
- All modifiers pressed → false
- Press then release all → true

**Tests for `ModifierState::matches_config()`**:
- Empty modifiers list, all released → true
- `["ctrl"]` with ctrl pressed → true
- `["ctrl"]` with alt pressed → false
- `["ctrl", "shift"]` with both pressed → true
- `["ctrl", "shift"]` with only ctrl pressed → false
- Case variations: "Ctrl", "CTRL", "control" all work
- Meta aliases: "super", "meta", "win", "logo" all match meta key

**Blocker**: Same as 1.1 - `ModifierState` is private in binary.

### 1.3 Minimal Extraction for Phase 1

To make Phase 1 tests possible, create a small library module:

**Create**: `src/hotkey/mod.rs`
```rust
mod key_parse;
mod modifiers;

pub use key_parse::parse_key;
pub use modifiers::ModifierState;
```

**Create**: `src/hotkey/key_parse.rs`
- Move `parse_key()` function from hotkey-daemon.rs
- Add `pub` visibility

**Create**: `src/hotkey/modifiers.rs`
- Move `ModifierState` struct and impl from hotkey-daemon.rs
- Add `pub` visibility

**Update**: `src/lib.rs`
```rust
pub mod hotkey;
```

**Update**: `src/bin/hotkey-daemon.rs`
- Replace local definitions with `use transcribe_rs::hotkey::{parse_key, ModifierState};`

### 1.4 Expected Outcome

- ~30 unit tests covering key parsing
- ~20 unit tests covering modifier state
- Zero logic changes to hotkey-daemon behavior
- Foundation for Phase 2 extraction

---

## Phase 2: Extract and Test State Machine

**Goal**: Extract the state machine logic into a testable module, enabling unit tests for all state transitions.

**Dependencies to add**:
```toml
[dev-dependencies]
proptest = "1"  # Property-based testing for state machine invariants
```

### 2.1 Define State Machine Types

**Create**: `src/hotkey/state.rs`

```rust
use evdev::Key;
use std::time::{Duration, Instant};

/// Active binding info stored during recording
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveBinding {
    pub key: Key,
    pub prompt: Option<String>,
}

/// Recording state machine
#[derive(Debug, Clone)]
pub enum RecordingState {
    Idle,
    Recording {
        press_time: Instant,
        active: ActiveBinding,
    },
    LongRecording {
        active: ActiveBinding,
    },
    PendingGrab {
        active: ActiveBinding,
    },
    PendingUngrab {
        should_transcribe: bool,
        active: ActiveBinding,
    },
}

/// Input event for state machine (abstracted from evdev)
#[derive(Debug, Clone, PartialEq)]
pub struct KeyEvent {
    pub key: Key,
    pub pressed: bool,
    pub released: bool,
}

/// Output actions from state transitions
#[derive(Debug, Clone, PartialEq)]
pub enum StateAction {
    None,
    StartRecording { binding: ActiveBinding },
    ProcessTranscription { binding: ActiveBinding },
    ProcessAndSendEnter { binding: ActiveBinding },
    CancelRecording,
    GrabKeyboard,
    UngrabKeyboard,
}

/// Result of a state transition
#[derive(Debug)]
pub struct TransitionResult {
    pub new_state: RecordingState,
    pub action: StateAction,
}
```

### 2.2 Extract Transition Logic

**Add to**: `src/hotkey/state.rs`

```rust
impl RecordingState {
    /// Pure function: compute next state and action from current state + event
    pub fn transition(
        &self,
        event: &KeyEvent,
        modifiers_match: bool,
        all_keys_released: bool,
        tap_threshold: Duration,
    ) -> TransitionResult {
        // Move the match statement logic here
        // Return (new_state, action) instead of performing side effects
    }
}
```

Key insight: The current match statement in main() mixes:
1. State transition logic (pure)
2. Side effects (start_recording, device.grab, etc.)

We separate these: `transition()` returns what to do, caller performs actions.

### 2.3 State Machine Unit Tests

**File**: `tests/hotkey_state_machine.rs`

**Transition tests** (one test per match arm):

| Current State | Event | Condition | Expected Next State | Expected Action |
|---------------|-------|-----------|---------------------|-----------------|
| Idle | Hotkey press | modifiers match | Recording | StartRecording |
| Idle | Hotkey press | modifiers don't match | Idle | None |
| Idle | Random key press | - | Idle | None |
| Recording | Same hotkey release | duration < threshold | PendingGrab | None |
| Recording | Same hotkey release | duration >= threshold | Idle | ProcessTranscription |
| Recording | Different key release | - | Recording | None |
| PendingGrab | Any | all_keys_released | LongRecording | GrabKeyboard |
| PendingGrab | Any | keys still held | PendingGrab | None |
| LongRecording | Enter press | - | LongRecording | ProcessAndSendEnter |
| LongRecording | Escape press | - | PendingUngrab(false) | CancelRecording |
| LongRecording | Hotkey press | - | PendingUngrab(true) | None |
| LongRecording | Other key | - | LongRecording | None |
| PendingUngrab | Any key release | - | Idle | UngrabKeyboard + maybe Process |

**Edge case tests**:
- Rapid key presses during PendingGrab
- Multiple hotkeys configured, wrong one released
- Modifier key released during Recording (should continue recording)
- Enter debounce logic (within 500ms of last Enter)

**Property-based tests** (proptest):
- Invariant: From any state, pressing Escape eventually leads to Idle
- Invariant: Idle is reachable from every state via some sequence
- Invariant: No state transition loses the active binding unexpectedly

### 2.4 Update hotkey-daemon.rs

Refactor main loop to use the extracted state machine:

```rust
// Before (current):
match (&state, pressed, released, &binding_opt) {
    (RecordingState::Idle, true, _, Some(binding)) if modifiers.matches_config(...) => {
        start_recording(&config)?;
        state = RecordingState::Recording { ... };
    }
    // ... 12 more arms with mixed logic
}

// After (using extracted module):
let result = state.transition(&event, modifiers_match, all_keys_released, tap_threshold);

match result.action {
    StateAction::StartRecording { binding } => start_recording(&config)?,
    StateAction::ProcessTranscription { binding } => process_transcription(&config, ...)?,
    StateAction::GrabKeyboard => device.grab()?,
    StateAction::UngrabKeyboard => device.ungrab()?,
    StateAction::CancelRecording => cancel_recording(&config)?,
    StateAction::None => {}
}

state = result.new_state;
```

### 2.5 Expected Outcome

- ~25 transition tests (one per state/event combination)
- ~10 edge case tests
- ~5 property-based tests
- State machine logic is now independently verifiable
- Main loop is reduced to ~50 lines of "dispatch actions"

---

## Phase 3: Integration Tests with Mocks

**Goal**: Test the event loop and device interactions using mock implementations.

**Dependencies to add**:
```toml
[dev-dependencies]
mockall = "0.12"
```

### 3.1 Define Device Abstraction

**Create**: `src/hotkey/device.rs`

```rust
use evdev::{Device, Key, InputEvent};
use std::error::Error;

/// Trait abstracting keyboard device operations
#[cfg_attr(test, mockall::automock)]
pub trait KeyboardDevice {
    /// Poll for events with timeout, returns events if available
    fn poll_events(&mut self, timeout_ms: u16) -> Result<Vec<InputEvent>, Box<dyn Error>>;

    /// Grab keyboard for exclusive access
    fn grab(&mut self) -> Result<(), Box<dyn Error>>;

    /// Release keyboard grab
    fn ungrab(&mut self) -> Result<(), Box<dyn Error>>;

    /// Check if currently grabbed
    fn is_grabbed(&self) -> bool;
}

/// Real implementation wrapping evdev::Device
pub struct EvdevKeyboard {
    device: Device,
    grabbed: bool,
}

impl KeyboardDevice for EvdevKeyboard {
    // ... implement using actual evdev calls
}
```

### 3.2 Define Action Executor Abstraction

**Create**: `src/hotkey/executor.rs`

```rust
use std::error::Error;

/// Trait for executing recording/transcription actions
#[cfg_attr(test, mockall::automock)]
pub trait ActionExecutor {
    fn start_recording(&self) -> Result<(), Box<dyn Error>>;
    fn stop_and_transcribe(&self, prompt: Option<&str>) -> Result<String, Box<dyn Error>>;
    fn cancel_recording(&self) -> Result<(), Box<dyn Error>>;
    fn copy_and_paste(&self, text: &str) -> Result<(), Box<dyn Error>>;
    fn send_enter(&self) -> Result<(), Box<dyn Error>>;
}

/// Real implementation using Config and actual system calls
pub struct RealExecutor {
    config: Config,
}

impl ActionExecutor for RealExecutor {
    // ... implement using actual subprocess calls
}
```

### 3.3 Refactor Event Loop

**Update**: `src/hotkey/mod.rs`

```rust
pub struct HotkeyDaemon<D: KeyboardDevice, E: ActionExecutor> {
    device: D,
    executor: E,
    state: RecordingState,
    modifiers: ModifierState,
    config: HotkeyConfig,
}

impl<D: KeyboardDevice, E: ActionExecutor> HotkeyDaemon<D, E> {
    pub fn new(device: D, executor: E, config: HotkeyConfig) -> Self { ... }

    /// Process one iteration of the event loop
    pub fn tick(&mut self) -> Result<bool, Box<dyn Error>> {
        // Returns false when should stop (Ctrl+C)
    }

    /// Run the daemon until stopped
    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        while self.tick()? {}
        Ok(())
    }
}
```

### 3.4 Integration Tests with Mocks

**File**: `tests/hotkey_integration.rs`

```rust
use mockall::predicate::*;
use transcribe_rs::hotkey::{HotkeyDaemon, MockKeyboardDevice, MockActionExecutor};

#[test]
fn test_push_to_talk_flow() {
    let mut mock_device = MockKeyboardDevice::new();
    let mut mock_executor = MockActionExecutor::new();

    // Setup: device returns hotkey press, then release after 1 second
    mock_device.expect_poll_events()
        .times(1)
        .returning(|| Ok(vec![/* Ctrl+D press event */]));
    mock_device.expect_poll_events()
        .times(1)
        .returning(|| Ok(vec![/* Ctrl+D release event after 1s */]));

    // Expectations: start recording, then transcribe
    mock_executor.expect_start_recording()
        .times(1)
        .returning(|| Ok(()));
    mock_executor.expect_stop_and_transcribe()
        .times(1)
        .returning(|_| Ok("hello world".to_string()));
    mock_executor.expect_copy_and_paste()
        .with(eq("hello world"))
        .times(1)
        .returning(|_| Ok(()));

    let mut daemon = HotkeyDaemon::new(mock_device, mock_executor, config);
    daemon.tick().unwrap();  // Process press
    daemon.tick().unwrap();  // Process release
}

#[test]
fn test_tap_to_toggle_flow() {
    // Similar setup but with quick tap (< threshold)
    // Verify: grab is called, stays in LongRecording
}

#[test]
fn test_escape_cancels_recording() {
    // Setup: in LongRecording, Escape pressed
    // Verify: cancel_recording called, ungrab called
}

#[test]
fn test_enter_submits_and_continues() {
    // Setup: in LongRecording, Enter pressed
    // Verify: transcribe + paste + send_enter + restart recording
}
```

### 3.5 Simplified hotkey-daemon.rs

After Phase 3, the binary becomes a thin wrapper:

```rust
fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    transcribe_rs::logging::init()?;

    // Load config
    let config = Config::load()?;

    // Find and select keyboard
    let device = select_keyboard()?;
    let keyboard = EvdevKeyboard::new(device);

    // Create executor with real implementations
    let executor = RealExecutor::new(config.clone());

    // Create and run daemon
    let mut daemon = HotkeyDaemon::new(keyboard, executor, config.hotkey);
    daemon.run()
}
```

**Estimated lines after refactoring**:
- `hotkey-daemon.rs`: ~100 lines (setup + thin main)
- `src/hotkey/state.rs`: ~150 lines
- `src/hotkey/modifiers.rs`: ~50 lines
- `src/hotkey/key_parse.rs`: ~70 lines
- `src/hotkey/device.rs`: ~80 lines
- `src/hotkey/executor.rs`: ~100 lines
- `src/hotkey/daemon.rs`: ~200 lines (event loop)

### 3.6 Expected Outcome

- Full mock coverage of device and executor interactions
- Can test complex scenarios without hardware
- Event loop logic is tested in isolation
- Easy to add new tests for edge cases

---

## Summary

| Phase | New Tests | Code Changes | Dependencies |
|-------|-----------|--------------|--------------|
| 1 | ~50 unit tests | Extract `parse_key`, `ModifierState` to library | `test-case` |
| 2 | ~40 tests | Extract `RecordingState` + transition logic | `proptest` |
| 3 | ~20 integration tests | Abstract `KeyboardDevice`, `ActionExecutor` | `mockall` |

**Total**: ~110 tests covering the entire hotkey daemon logic.

**Risk mitigation**: Each phase is independently valuable. Phase 1 alone provides regression safety for key parsing and modifiers. Phase 2 adds state machine coverage. Phase 3 enables full integration testing.

---

## Appendix: Test Commands

```bash
# Run all tests
cargo test

# Run only hotkey tests
cargo test hotkey

# Run with output (see test names)
cargo test hotkey -- --nocapture

# Run ignored tests (require hardware/daemons)
cargo test -- --ignored

# Run property tests with more cases
PROPTEST_CASES=1000 cargo test state_machine
```
