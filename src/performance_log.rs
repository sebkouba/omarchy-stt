//! Performance logging for tracking transcription workflow metrics

use std::error::Error;
use std::fs::OpenOptions;
use std::io::Write;

use crate::timing::PerformanceMetrics;

const PERFORMANCE_LOG_FILE: &str = "/tmp/ptt_performance.log";

/// Get the full build version from environment variable set by build.rs
fn get_build_version() -> &'static str {
    // This env var is set at compile time by build.rs
    env!("FULL_VERSION")
}

/// Log a performance entry to the performance log file
///
/// Format: [YYYY-MM-DD HH:MM:SS] | Build: 0.1.4+1234 | Duration: 2.345s | Text: "transcribed text"
pub fn log_performance(metrics: &PerformanceMetrics, text: &str) -> Result<(), Box<dyn Error>> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PERFORMANCE_LOG_FILE)?;

    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let build_version = get_build_version();
    let duration = metrics.total_duration_seconds();

    // Format: [timestamp] | Build: version | Duration: Xs | Text: "text"
    writeln!(
        file,
        "[{}] | Build: {} | Duration: {:.3}s | Text: \"{}\"",
        timestamp, build_version, duration, text
    )?;

    Ok(())
}

/// Log a performance entry with detailed milestone breakdowns
///
/// This logs the same entry as log_performance, but also includes optional detailed timing breakdowns
pub fn log_performance_detailed(
    metrics: &PerformanceMetrics,
    text: &str,
) -> Result<(), Box<dyn Error>> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PERFORMANCE_LOG_FILE)?;

    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let build_version = get_build_version();
    let total_duration = metrics.total_duration_seconds();

    // Build detailed timing breakdown
    let mut details = Vec::new();

    if let Some(dur) = metrics.recording_duration_seconds() {
        details.push(format!("record:{:.3}s", dur));
    }
    if let Some(dur) = metrics.transcription_duration_seconds() {
        details.push(format!("transcribe:{:.3}s", dur));
    }
    if let Some(dur) = metrics.processing_duration_seconds() {
        details.push(format!("process:{:.3}s", dur));
    }
    if let Some(dur) = metrics.clipboard_duration_seconds() {
        details.push(format!("clipboard:{:.3}s", dur));
    }
    if let Some(dur) = metrics.paste_duration_seconds() {
        details.push(format!("paste:{:.3}s", dur));
    }

    let details_str = if details.is_empty() {
        String::new()
    } else {
        format!(" | Breakdown: {}", details.join(", "))
    };

    // Format: [timestamp] | Build: version | Duration: Xs | Breakdown: ... | Text: "text"
    writeln!(
        file,
        "[{}] | Build: {} | Duration: {:.3}s{} | Text: \"{}\"",
        timestamp, build_version, total_duration, details_str, text
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    /// Create test metrics without relying on shared timestamp file
    fn create_test_metrics() -> PerformanceMetrics {
        let start_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        PerformanceMetrics {
            start_ns,
            recording_stop_ns: None,
            transcription_done_ns: None,
            processing_done_ns: None,
            clipboard_done_ns: None,
            paste_done_ns: None,
        }
    }

    #[test]
    fn test_log_performance() {
        // Create metrics directly without using shared timestamp file
        let mut metrics = create_test_metrics();
        std::thread::sleep(Duration::from_millis(10));
        metrics.mark_paste_done();

        // Log performance
        let result = log_performance(&metrics, "Test transcription");
        assert!(result.is_ok());
    }

    #[test]
    fn test_log_performance_detailed() {
        // Create metrics directly without using shared timestamp file
        let mut metrics = create_test_metrics();

        std::thread::sleep(Duration::from_millis(5));
        metrics.mark_recording_stop();
        std::thread::sleep(Duration::from_millis(5));
        metrics.mark_transcription_done();
        std::thread::sleep(Duration::from_millis(5));
        metrics.mark_processing_done();
        std::thread::sleep(Duration::from_millis(5));
        metrics.mark_clipboard_done();
        std::thread::sleep(Duration::from_millis(5));
        metrics.mark_paste_done();

        // Log detailed performance
        let result = log_performance_detailed(&metrics, "Test transcription with details");
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_version() {
        let version = get_build_version();
        // Version should be in format like "0.1.4+1"
        assert!(version.contains("+"));
    }
}
