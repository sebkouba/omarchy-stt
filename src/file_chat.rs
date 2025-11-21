//! File chat module for writing Q&A conversations to markdown files
//!
//! This module handles writing dictation Q&A pairs to markdown files,
//! creating a persistent chat interface that can be viewed in any markdown viewer.

use log::debug;
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
) -> Result<(), Box<dyn Error>> {
    debug!("=== FILE CHAT APPEND ===");

    // Ensure output directory exists
    let dir_path = Path::new(output_dir);
    if !dir_path.exists() {
        debug!("Creating chat directory: {}", output_dir);
        fs::create_dir_all(dir_path)?;
    }

    // Generate filename based on current date
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let file_path = dir_path.join(format!("{}.md", today));

    debug!("Chat file: {}", file_path.display());

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

    debug!("Appended {} bytes to chat file", entry.len());
    debug!("=== FILE CHAT APPEND COMPLETE ===");

    Ok(())
}
