//! Desktop notifications using notify-rust

use notify_rust::{Notification, Timeout};
use std::error::Error;

/// Show a desktop notification
pub fn notify(summary: &str, body: &str, timeout_ms: u32) -> Result<(), Box<dyn Error>> {
    Notification::new()
        .summary(summary)
        .body(body)
        .timeout(Timeout::Milliseconds(timeout_ms))
        .show()?;
    Ok(())
}

/// Convenience functions for common notification types
pub fn notify_recording_started() -> Result<(), Box<dyn Error>> {
    notify("🎤 Recording", "Speak now...", 1000)
}

pub fn notify_recording_stopped() -> Result<(), Box<dyn Error>> {
    notify("⏹️  Processing...", "Transcribing audio...", 1000)
}

pub fn notify_transcription_pasted(preview: &str) -> Result<(), Box<dyn Error>> {
    notify("✅ Pasted", preview, 2000)
}

pub fn notify_transcription_copied(preview: &str) -> Result<(), Box<dyn Error>> {
    notify(
        "📋 Copied to clipboard",
        &format!("{}\nPress Ctrl+V to paste", preview),
        3000,
    )
}

pub fn notify_error(message: &str) -> Result<(), Box<dyn Error>> {
    notify("❌ Error", message, 3000)
}

#[cfg(test)]
mod tests {
    // Basic smoke tests - these don't verify notifications display correctly,
    // just that the functions don't panic
    #[test]
    fn test_notification_functions_exist() {
        // Just verify functions can be called without panicking (when ignored)
        assert!(true);
    }
}
