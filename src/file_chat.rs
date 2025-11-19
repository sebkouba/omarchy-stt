//! File chat module for writing Q&A conversations to markdown files
//!
//! This module handles writing dictation Q&A pairs to markdown files,
//! creating a persistent chat interface that can be viewed in any markdown viewer.

use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::Path;

/// Append a Q&A exchange to the daily chat file
///
/// Creates a markdown file named YYYY-MM-DD.md in the configured directory,
/// appending timestamped user/assistant exchanges.
pub fn append_to_chat_file(
    output_dir: &str,
    user_text: &str,
    assistant_text: &str,
    log_file: &str,
) -> Result<(), Box<dyn Error>> {
    log("=== FILE CHAT APPEND ===", log_file);

    // Ensure output directory exists
    let dir_path = Path::new(output_dir);
    if !dir_path.exists() {
        log(&format!("Creating chat directory: {}", output_dir), log_file);
        fs::create_dir_all(dir_path)?;
    }

    // Generate filename based on current date
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let file_path = dir_path.join(format!("{}.md", today));

    log(&format!("Chat file: {}", file_path.display()), log_file);

    // Generate timestamp for this entry
    let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();

    // Format the Q&A entry
    let entry = format!(
        "## {}\n\n**User:** {}\n\n**Assistant:** {}\n\n---\n\n",
        timestamp,
        user_text.trim(),
        assistant_text.trim()
    );

    // Append to file (create if doesn't exist)
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)?;

    file.write_all(entry.as_bytes())?;

    log(&format!("Appended {} bytes to chat file", entry.len()), log_file);
    log("=== FILE CHAT APPEND COMPLETE ===", log_file);

    Ok(())
}

/// Append a log message to the debug log
fn log(message: &str, log_file: &str) {
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        writeln!(file, "[{}] [file_chat] {}", timestamp, message).ok();
    }
}
