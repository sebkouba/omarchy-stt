//! Dictation logging utilities for tracking transcriptions and LLM corrections

use crate::timing::TimingBreakdown;
use chrono::Local;
use similar::{ChangeTag, TextDiff};
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Escape a string for CSV format
/// Wraps in quotes if necessary and escapes internal quotes
fn csv_escape(s: &str) -> String {
    // Check if escaping is needed
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        // Wrap in quotes and escape internal quotes by doubling them
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Generate a compact inline diff showing changes between two texts
/// Returns a string showing deletions and insertions
pub fn generate_diff(original: &str, corrected: &str) -> String {
    let diff = TextDiff::from_words(original, corrected);
    let mut result = Vec::new();

    for change in diff.iter_all_changes() {
        let value = change.value();
        match change.tag() {
            ChangeTag::Delete => {
                // Show deletions in brackets with minus
                result.push(format!("[-{}]", value.trim()));
            }
            ChangeTag::Insert => {
                // Show insertions in brackets with plus
                result.push(format!("[+{}]", value.trim()));
            }
            ChangeTag::Equal => {
                // Skip unchanged parts for brevity (or include context if needed)
                // For now, we'll skip to keep the diff compact
                continue;
            }
        }
    }

    if result.is_empty() {
        "(no changes)".to_string()
    } else {
        result.join(" ")
    }
}

/// Log a basic dictation (without LLM processing)
///
/// # Arguments
/// * `text` - The transcribed text
/// * `duration_seconds` - Time from hotkey press to paste completion
/// * `log_path` - Path to the CSV log file
///
/// # Format
/// CSV columns: timestamp,text,duration_seconds
pub fn log_basic_dictation(
    text: &str,
    duration_seconds: f64,
    log_path: &str,
) -> Result<(), Box<dyn Error>> {
    let path = Path::new(log_path);

    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Check if file exists to determine if we need to write headers
    let needs_header = !path.exists();

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    // Write header if this is a new file
    if needs_header {
        writeln!(file, "timestamp,text,duration_seconds")?;
    }

    // Write the log entry
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let escaped_text = csv_escape(text);
    writeln!(
        file,
        "{},{},{:.3}",
        timestamp, escaped_text, duration_seconds
    )?;

    Ok(())
}

/// Log a dictation with detailed timing breakdown
///
/// # Arguments
/// * `text` - The transcribed text
/// * `breakdown` - Timing breakdown for each stage
/// * `log_path` - Path to the CSV log file (will use _timed suffix)
///
/// # Format
/// CSV columns: timestamp,text,recording_ms,transcription_ms,processing_ms,paste_ms,total_ms
pub fn log_dictation_with_timing(
    text: &str,
    breakdown: &TimingBreakdown,
    log_path: &str,
) -> Result<(), Box<dyn Error>> {
    // Use a separate file with _timed suffix to not conflict with old format
    let timed_path = log_path.replace(".csv", "_timed.csv");
    let path = Path::new(&timed_path);

    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Check if file exists to determine if we need to write headers
    let needs_header = !path.exists();

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    // Write header if this is a new file
    if needs_header {
        writeln!(
            file,
            "timestamp,text,recording_ms,transcription_ms,processing_ms,paste_ms,total_ms"
        )?;
    }

    // Write the log entry
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let escaped_text = csv_escape(text);
    writeln!(
        file,
        "{},{},{},{},{},{},{}",
        timestamp,
        escaped_text,
        breakdown.recording_ms,
        breakdown.transcription_ms,
        breakdown.processing_ms,
        breakdown.paste_ms,
        breakdown.total_ms()
    )?;

    Ok(())
}

/// Log an LLM correction session
///
/// # Arguments
/// * `original` - The original transcribed text (before LLM processing)
/// * `corrected` - The text after LLM processing
/// * `diff` - A string describing what changed between original and corrected
/// * `duration_seconds` - Time from hotkey press to paste completion
/// * `log_path` - Path to the CSV log file
///
/// # Format
/// CSV columns: timestamp,correction_occurred,original_text,corrected_text,diff,duration_seconds
/// correction_occurred is "Y" if text changed, "N" if unchanged
pub fn log_llm_correction(
    original: &str,
    corrected: &str,
    diff: &str,
    duration_seconds: f64,
    log_path: &str,
) -> Result<(), Box<dyn Error>> {
    let path = Path::new(log_path);

    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Check if file exists to determine if we need to write headers
    let needs_header = !path.exists();

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    // Write header if this is a new file
    if needs_header {
        writeln!(
            file,
            "timestamp,correction_occurred,original_text,corrected_text,diff,duration_seconds"
        )?;
    }

    // Determine if correction occurred
    let correction_occurred = if original != corrected { "Y" } else { "N" };

    // Write the log entry
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let escaped_original = csv_escape(original);
    let escaped_corrected = csv_escape(corrected);
    let escaped_diff = csv_escape(diff);
    writeln!(
        file,
        "{},{},{},{},{},{:.3}",
        timestamp,
        correction_occurred,
        escaped_original,
        escaped_corrected,
        escaped_diff,
        duration_seconds
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_csv_escape_simple() {
        assert_eq!(csv_escape("hello"), "hello");
        assert_eq!(csv_escape("hello world"), "hello world");
    }

    #[test]
    fn test_csv_escape_comma() {
        assert_eq!(csv_escape("hello, world"), "\"hello, world\"");
    }

    #[test]
    fn test_csv_escape_quotes() {
        assert_eq!(csv_escape("hello \"world\""), "\"hello \"\"world\"\"\"");
    }

    #[test]
    fn test_csv_escape_newline() {
        assert_eq!(csv_escape("hello\nworld"), "\"hello\nworld\"");
    }

    #[test]
    fn test_log_basic_dictation() {
        let temp_dir = tempdir().unwrap();
        let log_path = temp_dir.path().join("basic_test.csv");
        let log_path_str = log_path.to_str().unwrap();

        // Log first entry
        log_basic_dictation("Hello world", 2.5, log_path_str).unwrap();

        // Log second entry with special characters
        log_basic_dictation("Test, with \"quotes\"", 3.2, log_path_str).unwrap();

        // Read the file and verify
        let content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(lines.len(), 3); // Header + 2 entries
        assert_eq!(lines[0], "timestamp,text,duration_seconds");
        assert!(lines[1].contains("Hello world"));
        assert!(lines[1].contains("2.500"));
        assert!(lines[2].contains("\"Test, with \"\"quotes\"\"\""));
        assert!(lines[2].contains("3.200"));
    }

    #[test]
    fn test_log_llm_correction() {
        let temp_dir = tempdir().unwrap();
        let log_path = temp_dir.path().join("llm_test.csv");
        let log_path_str = log_path.to_str().unwrap();

        // Log entry with correction
        let diff1 = generate_diff("teh test", "the test");
        log_llm_correction("teh test", "the test", &diff1, 4.1, log_path_str).unwrap();

        // Log entry without correction
        let diff2 = generate_diff("perfect text", "perfect text");
        log_llm_correction("perfect text", "perfect text", &diff2, 3.8, log_path_str).unwrap();

        // Read the file and verify
        let content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(lines.len(), 3); // Header + 2 entries
        assert_eq!(
            lines[0],
            "timestamp,correction_occurred,original_text,corrected_text,diff,duration_seconds"
        );
        assert!(lines[1].contains(",Y,"));
        assert!(lines[1].contains("teh test"));
        assert!(lines[1].contains("the test"));
        assert!(lines[1].contains("4.100"));
        assert!(lines[2].contains(",N,"));
        assert!(lines[2].contains("perfect text"));
        assert!(lines[2].contains("3.800"));
    }

    #[test]
    fn test_generate_diff() {
        // Test word-level diff
        let diff = generate_diff("hello world", "hello there");
        assert!(diff.contains("[-world]"));
        assert!(diff.contains("[+there]"));

        // Test no changes
        let diff = generate_diff("same text", "same text");
        assert_eq!(diff, "(no changes)");

        // Test multiple changes
        let diff = generate_diff("the quick brown fox", "the fast brown dog");
        assert!(diff.contains("[-quick]"));
        assert!(diff.contains("[+fast]"));
        assert!(diff.contains("[-fox]"));
        assert!(diff.contains("[+dog]"));
    }

    #[test]
    fn test_append_behavior() {
        let temp_dir = tempdir().unwrap();
        let log_path = temp_dir.path().join("append_test.csv");
        let log_path_str = log_path.to_str().unwrap();

        // Log multiple entries
        log_basic_dictation("Entry 1", 1.0, log_path_str).unwrap();
        log_basic_dictation("Entry 2", 2.0, log_path_str).unwrap();
        log_basic_dictation("Entry 3", 3.0, log_path_str).unwrap();

        // Verify all entries are present
        let content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(lines.len(), 4); // Header + 3 entries
        assert!(content.contains("Entry 1"));
        assert!(content.contains("Entry 2"));
        assert!(content.contains("Entry 3"));
    }
}
