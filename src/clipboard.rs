//! Clipboard operations using wl-copy/wl-paste (Wayland)

use log::{debug, error, warn};
use std::error::Error;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::paste;

/// Saved clipboard content for restoration
#[derive(Debug, Clone)]
pub struct SavedClipboard {
    /// The raw bytes from the clipboard (may be text or binary)
    content: Option<Vec<u8>>,
}

impl SavedClipboard {
    /// Save the current clipboard content
    pub fn save() -> Self {
        debug!("Saving current clipboard content via wl-paste");

        let result = Command::new("wl-paste")
            .arg("--no-newline")
            .output();

        match result {
            Ok(output) if output.status.success() => {
                let content = output.stdout;
                debug!("Saved {} bytes from clipboard", content.len());
                SavedClipboard {
                    content: Some(content),
                }
            }
            Ok(output) => {
                // wl-paste returns non-zero if clipboard is empty
                let stderr = String::from_utf8_lossy(&output.stderr);
                debug!("Clipboard appears empty or unavailable: {}", stderr);
                SavedClipboard { content: None }
            }
            Err(e) => {
                warn!("Failed to save clipboard content: {}", e);
                SavedClipboard { content: None }
            }
        }
    }

    /// Restore the saved clipboard content
    pub fn restore(self) -> Result<(), Box<dyn Error>> {
        match self.content {
            Some(content) => {
                debug!("Restoring {} bytes to clipboard via wl-copy", content.len());

                let mut child = Command::new("wl-copy")
                    .stdin(Stdio::piped())
                    .spawn()
                    .map_err(|e| {
                        let err = format!("Failed to start wl-copy for restore: {}", e);
                        error!("{}", err);
                        err
                    })?;

                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(&content).map_err(|e| {
                        let err = format!("Failed to write to wl-copy stdin: {}", e);
                        error!("{}", err);
                        err
                    })?;
                }

                let status = child.wait().map_err(|e| {
                    let err = format!("Failed to wait for wl-copy: {}", e);
                    error!("{}", err);
                    err
                })?;

                if !status.success() {
                    let err = format!("wl-copy restore exited with status: {}", status);
                    error!("{}", err);
                    return Err(err.into());
                }

                debug!("Clipboard content restored successfully");
                Ok(())
            }
            None => {
                debug!("No saved clipboard content to restore (was empty)");
                // Clear the clipboard since it was originally empty
                let status = Command::new("wl-copy")
                    .arg("--clear")
                    .status();

                match status {
                    Ok(s) if s.success() => {
                        debug!("Clipboard cleared (was originally empty)");
                    }
                    _ => {
                        debug!("Could not clear clipboard, leaving as-is");
                    }
                }
                Ok(())
            }
        }
    }

    /// Check if there was content saved
    pub fn has_content(&self) -> bool {
        self.content.is_some()
    }
}

/// Copy text to system clipboard using wl-copy
pub fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn Error>> {
    debug!("Copying {} bytes to clipboard via wl-copy", text.len());

    let mut child = Command::new("wl-copy")
        .arg("--sensitive") // Prevent cliphist from storing transcriptions
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| {
            let err = format!(
                "Failed to start wl-copy: {}\n\n\
                Is wl-clipboard installed?\n\
                  Check with: which wl-copy\n\
                  Install with: sudo pacman -S wl-clipboard\n\n\
                Run system check:\n\
                  transcribe doctor",
                e
            );
            error!(" wl-copy not found: {}", e);
            err
        })?;

    debug!("wl-copy spawned, writing to stdin...");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).map_err(|e| {
            let err = format!("Failed to write to wl-copy stdin: {}", e);
            error!(" {}", err);
            err
        })?;
    }

    debug!("Waiting for wl-copy to complete...");
    let status = child.wait().map_err(|e| {
        let err = format!("Failed to wait for wl-copy: {}", e);
        error!(" {}", err);
        err
    })?;

    if !status.success() {
        let err = format!("wl-copy exited with status: {}", status);
        error!(" {}", err);
        return Err(err.into());
    }

    debug!("wl-copy completed successfully");
    Ok(())
}

/// Add space after sentence-ending punctuation
/// This allows consecutive PTT dictations to flow naturally
pub fn add_trailing_space_after_punctuation(text: &str) -> String {
    if let Some(last_char) = text.chars().last() {
        if matches!(last_char, '.' | '!' | '?') {
            return format!("{} ", text);
        }
    }
    text.to_string()
}

/// Options for the copy/paste workflow
#[derive(Debug, Clone)]
pub struct PasteWorkflowOptions {
    /// Add trailing space after sentence-ending punctuation (.!?)
    pub add_trailing_space: bool,
    /// Automatically paste after copying to clipboard
    pub auto_paste: bool,
    /// Save and restore original clipboard content after pasting
    pub preserve_clipboard: bool,
    /// Delay in milliseconds after paste before restoring clipboard
    /// This gives the target application time to read the clipboard
    pub restore_delay_ms: u64,
}

impl Default for PasteWorkflowOptions {
    fn default() -> Self {
        Self {
            add_trailing_space: true,
            auto_paste: true,
            preserve_clipboard: true,
            restore_delay_ms: 100,
        }
    }
}

/// Result of the paste workflow
#[derive(Debug)]
pub struct PasteWorkflowResult {
    /// Whether the paste was attempted (false if auto_paste disabled)
    pub paste_attempted: bool,
    /// Whether the paste succeeded (None if not attempted)
    pub paste_succeeded: Option<bool>,
    /// Whether clipboard was restored (None if preservation disabled)
    pub clipboard_restored: Option<bool>,
}

/// Complete copy/paste workflow with optional clipboard preservation.
///
/// This is the single source of truth for the copy→paste→restore flow.
/// Both cli.rs and hotkey-daemon.rs should use this function.
///
/// Flow:
/// 1. Optionally save original clipboard content
/// 2. Optionally add trailing space after punctuation
/// 3. Copy text to clipboard
/// 4. Optionally paste (via ydotool)
/// 5. Optionally wait and restore original clipboard
pub fn copy_paste_workflow(
    text: &str,
    options: &PasteWorkflowOptions,
) -> Result<PasteWorkflowResult, Box<dyn Error>> {
    // Step 1: Save original clipboard if preservation is enabled
    let saved_clipboard = if options.preserve_clipboard {
        debug!("Saving original clipboard content...");
        let saved = SavedClipboard::save();
        debug!("Clipboard saved (has_content: {})", saved.has_content());
        Some(saved)
    } else {
        None
    };

    // Step 2: Optionally add trailing space after punctuation
    let text = if options.add_trailing_space {
        add_trailing_space_after_punctuation(text)
    } else {
        text.to_string()
    };

    // Step 3: Copy to clipboard
    debug!("Copying to clipboard...");
    copy_to_clipboard(&text)?;
    debug!("Clipboard copy successful");

    // Step 4: Optionally paste
    let mut result = PasteWorkflowResult {
        paste_attempted: false,
        paste_succeeded: None,
        clipboard_restored: None,
    };

    if options.auto_paste {
        result.paste_attempted = true;
        debug!("Attempting auto-paste...");

        match paste::paste_from_clipboard() {
            Ok(_) => {
                debug!("Paste successful");
                result.paste_succeeded = Some(true);

                // Step 5: Restore clipboard after successful paste
                if let Some(saved) = saved_clipboard {
                    // Wait for the application to read from clipboard
                    // ydotool returns immediately after sending key events,
                    // but the app reads the clipboard asynchronously
                    if options.restore_delay_ms > 0 {
                        debug!(
                            "Waiting {}ms for paste to complete before restoring clipboard...",
                            options.restore_delay_ms
                        );
                        std::thread::sleep(Duration::from_millis(options.restore_delay_ms));
                    }

                    debug!("Restoring original clipboard content...");
                    match saved.restore() {
                        Ok(_) => {
                            debug!("Clipboard restored successfully");
                            result.clipboard_restored = Some(true);
                        }
                        Err(e) => {
                            warn!("Failed to restore clipboard: {}", e);
                            result.clipboard_restored = Some(false);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Paste failed: {}", e);
                result.paste_succeeded = Some(false);
                // Don't restore clipboard if paste failed - user may want to manually paste
            }
        }
    } else {
        debug!("Auto-paste disabled, clipboard only");
        // Don't restore clipboard when auto-paste disabled - user needs clipboard content
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_space_after_period() {
        let input = "Hello world.";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "Hello world. ");
    }

    #[test]
    fn test_add_space_after_question() {
        let input = "How are you?";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "How are you? ");
    }

    #[test]
    fn test_add_space_after_exclamation() {
        let input = "Wow!";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "Wow! ");
    }

    #[test]
    fn test_no_space_after_comma() {
        let input = "Hello world, how are you";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "Hello world, how are you");
    }

    #[test]
    fn test_empty_string() {
        let input = "";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "");
    }

    #[test]
    fn test_clipboard_save_and_restore() {
        // Skip if wl-copy/wl-paste not available (CI environment)
        if std::process::Command::new("which")
            .arg("wl-paste")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            println!("Skipping clipboard test: wl-paste not available");
            return;
        }

        // Set known content in clipboard
        let original_content = "test-clipboard-preservation-content-12345";
        if copy_to_clipboard(original_content).is_err() {
            println!("Skipping clipboard test: cannot write to clipboard (no Wayland session?)");
            return;
        }

        // Save the clipboard
        let saved = SavedClipboard::save();
        assert!(saved.has_content(), "Should have saved content");

        // Overwrite with different content (simulating dictation paste)
        let dictation_text = "This is transcribed text that would normally stay in clipboard";
        copy_to_clipboard(dictation_text).expect("Should copy dictation");

        // Verify clipboard has the dictation
        let after_copy = std::process::Command::new("wl-paste")
            .arg("--no-newline")
            .output()
            .expect("wl-paste should work");
        let after_text = String::from_utf8_lossy(&after_copy.stdout);
        assert_eq!(after_text, dictation_text, "Clipboard should have dictation text");

        // Restore original content
        saved.restore().expect("Restore should succeed");

        // Verify clipboard has original content again
        let restored = std::process::Command::new("wl-paste")
            .arg("--no-newline")
            .output()
            .expect("wl-paste should work");
        let restored_text = String::from_utf8_lossy(&restored.stdout);
        assert_eq!(
            restored_text, original_content,
            "Clipboard should be restored to original content"
        );
    }

    // Note: Clipboard tests share system state (the actual clipboard).
    // Run with `cargo test --lib clipboard -- --test-threads=1` to avoid interference.
    #[test]
    #[ignore] // Ignored by default to avoid interference with other clipboard tests
    fn test_clipboard_save_empty() {
        // Skip if wl-copy/wl-paste not available
        if std::process::Command::new("which")
            .arg("wl-paste")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            println!("Skipping clipboard test: wl-paste not available");
            return;
        }

        // Clear clipboard first
        let _ = std::process::Command::new("wl-copy")
            .arg("--clear")
            .status();

        // Save empty clipboard
        let saved = SavedClipboard::save();
        // Note: has_content() may be false for empty clipboard

        // Put something in clipboard
        if copy_to_clipboard("temporary content for empty test").is_err() {
            println!("Skipping clipboard test: cannot write to clipboard");
            return;
        }

        // Restore (should clear or leave empty)
        let _ = saved.restore();

        // The clipboard should either be empty or cleared
        // We don't assert here because behavior may vary
    }

    #[test]
    fn test_copy_paste_workflow_no_paste() {
        // Test the workflow with auto_paste disabled
        // This tests copy and trailing space handling without requiring ydotool

        // Skip if wl-copy/wl-paste not available
        if std::process::Command::new("which")
            .arg("wl-paste")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            println!("Skipping clipboard test: wl-paste not available");
            return;
        }

        let original_content = "original-workflow-test-content";
        if copy_to_clipboard(original_content).is_err() {
            println!("Skipping clipboard test: cannot write to clipboard");
            return;
        }

        // Run workflow with auto_paste disabled
        let options = PasteWorkflowOptions {
            add_trailing_space: true,
            auto_paste: false,
            preserve_clipboard: true,
            restore_delay_ms: 0,
        };

        let result = copy_paste_workflow("Hello world.", &options)
            .expect("Workflow should succeed");

        // Verify workflow result
        assert!(!result.paste_attempted, "Paste should not be attempted");
        assert!(result.paste_succeeded.is_none(), "Paste result should be None");
        assert!(result.clipboard_restored.is_none(), "Restore not done when paste not attempted");

        // Verify clipboard has the new content (with trailing space)
        let clipboard_content = std::process::Command::new("wl-paste")
            .arg("--no-newline")
            .output()
            .expect("wl-paste should work");
        let text = String::from_utf8_lossy(&clipboard_content.stdout);
        assert_eq!(text, "Hello world. ", "Clipboard should have text with trailing space");
    }

    #[test]
    fn test_copy_paste_workflow_with_preservation() {
        // Test the full workflow including paste and clipboard restoration
        // Requires ydotool to be available

        // Skip if wl-copy/wl-paste not available
        if std::process::Command::new("which")
            .arg("wl-paste")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            println!("Skipping clipboard test: wl-paste not available");
            return;
        }

        // Skip if ydotool not available
        if std::process::Command::new("which")
            .arg("ydotool")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            println!("Skipping clipboard test: ydotool not available");
            return;
        }

        // Set original clipboard content
        let original_content = "original-content-before-dictation-12345";
        if copy_to_clipboard(original_content).is_err() {
            println!("Skipping clipboard test: cannot write to clipboard");
            return;
        }

        // Verify original content is set
        let before = std::process::Command::new("wl-paste")
            .arg("--no-newline")
            .output()
            .expect("wl-paste should work");
        assert_eq!(
            String::from_utf8_lossy(&before.stdout),
            original_content,
            "Original content should be in clipboard"
        );

        // Run the workflow with preservation enabled
        let options = PasteWorkflowOptions {
            add_trailing_space: false,
            auto_paste: true,
            preserve_clipboard: true,
            restore_delay_ms: 100,
        };

        let dictation_text = "This is transcribed dictation text";
        let result = copy_paste_workflow(dictation_text, &options);

        // The paste might fail if ydotool daemon isn't running, that's OK
        // We're mainly testing that when it succeeds, clipboard is restored
        match result {
            Ok(workflow_result) => {
                assert!(workflow_result.paste_attempted, "Paste should be attempted");

                if workflow_result.paste_succeeded == Some(true) {
                    // Paste succeeded - clipboard should be restored
                    assert_eq!(
                        workflow_result.clipboard_restored,
                        Some(true),
                        "Clipboard should be restored after successful paste"
                    );

                    // Verify clipboard is restored to original content
                    let after = std::process::Command::new("wl-paste")
                        .arg("--no-newline")
                        .output()
                        .expect("wl-paste should work");
                    assert_eq!(
                        String::from_utf8_lossy(&after.stdout),
                        original_content,
                        "Clipboard should be restored to original content after paste"
                    );
                } else {
                    println!("Paste failed (ydotool daemon not running?), skipping restoration check");
                }
            }
            Err(e) => {
                println!("Workflow failed (expected if ydotool not running): {}", e);
            }
        }
    }
}
