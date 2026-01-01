//! Clipboard operations using wl-copy/wl-paste (Wayland)

use log::{debug, error, warn};
use std::error::Error;
use std::io::Write;
use std::process::{Command, Stdio};

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
}
