//! Clipboard operations using wl-copy (Wayland)

use std::error::Error;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

/// Append a log message to the debug log
fn log(message: &str) {
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/ptt_rust_debug.log")
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        writeln!(file, "[{}] [clipboard] {}", timestamp, message).ok();
    }
}

/// Copy text to system clipboard using wl-copy
pub fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn Error>> {
    log(&format!("Copying {} bytes to clipboard via wl-copy", text.len()));

    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| {
            let err = format!("Failed to spawn wl-copy: {}", e);
            log(&format!("ERROR: {}", err));
            err
        })?;

    log("wl-copy spawned, writing to stdin...");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())
            .map_err(|e| {
                let err = format!("Failed to write to wl-copy stdin: {}", e);
                log(&format!("ERROR: {}", err));
                err
            })?;
    }

    log("Waiting for wl-copy to complete...");
    let status = child.wait()
        .map_err(|e| {
            let err = format!("Failed to wait for wl-copy: {}", e);
            log(&format!("ERROR: {}", err));
            err
        })?;

    if !status.success() {
        let err = format!("wl-copy exited with status: {}", status);
        log(&format!("ERROR: {}", err));
        return Err(err.into());
    }

    log("wl-copy completed successfully");
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
}
