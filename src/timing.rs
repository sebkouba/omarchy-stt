//! Performance timing utilities for tracking transcription workflow durations

use std::error::Error;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

const TIMESTAMP_FILE: &str = "/tmp/ptt_start_timestamp";

/// Get current timestamp in nanoseconds since UNIX epoch
fn get_current_timestamp_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_nanos()
}

/// Save the start timestamp to file
pub fn save_start_time() -> Result<(), Box<dyn Error>> {
    let timestamp = get_current_timestamp_ns();
    fs::write(TIMESTAMP_FILE, timestamp.to_string())?;
    Ok(())
}

/// Load the start timestamp from file
pub fn load_start_time() -> Result<u128, Box<dyn Error>> {
    let content = fs::read_to_string(TIMESTAMP_FILE)?;
    let timestamp = content.trim().parse::<u128>()?;
    Ok(timestamp)
}

/// Calculate duration in seconds from start timestamp to now
pub fn calculate_duration_from_start() -> Result<f64, Box<dyn Error>> {
    let start_ns = load_start_time()?;
    let current_ns = get_current_timestamp_ns();
    let duration_ns = current_ns - start_ns;
    // Convert nanoseconds to seconds
    Ok(duration_ns as f64 / 1_000_000_000.0)
}

/// Clean up the timestamp file
pub fn cleanup_timestamp_file() {
    let _ = fs::remove_file(TIMESTAMP_FILE);
}

/// Structure to track milestone timestamps throughout the workflow
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub start_ns: u128,
    pub recording_stop_ns: Option<u128>,
    pub transcription_done_ns: Option<u128>,
    pub processing_done_ns: Option<u128>,
    pub clipboard_done_ns: Option<u128>,
    pub paste_done_ns: Option<u128>,
}

impl PerformanceMetrics {
    /// Create new metrics from saved start time
    pub fn from_start_time() -> Result<Self, Box<dyn Error>> {
        let start_ns = load_start_time()?;
        Ok(Self {
            start_ns,
            recording_stop_ns: None,
            transcription_done_ns: None,
            processing_done_ns: None,
            clipboard_done_ns: None,
            paste_done_ns: None,
        })
    }

    /// Mark when recording stopped
    pub fn mark_recording_stop(&mut self) {
        self.recording_stop_ns = Some(get_current_timestamp_ns());
    }

    /// Mark when transcription completed
    pub fn mark_transcription_done(&mut self) {
        self.transcription_done_ns = Some(get_current_timestamp_ns());
    }

    /// Mark when text processing completed
    pub fn mark_processing_done(&mut self) {
        self.processing_done_ns = Some(get_current_timestamp_ns());
    }

    /// Mark when clipboard copy completed
    pub fn mark_clipboard_done(&mut self) {
        self.clipboard_done_ns = Some(get_current_timestamp_ns());
    }

    /// Mark when paste completed
    pub fn mark_paste_done(&mut self) {
        self.paste_done_ns = Some(get_current_timestamp_ns());
    }

    /// Get total duration from start to paste in seconds
    pub fn total_duration_seconds(&self) -> f64 {
        let end_ns = self.paste_done_ns.unwrap_or_else(get_current_timestamp_ns);
        let duration_ns = end_ns - self.start_ns;
        duration_ns as f64 / 1_000_000_000.0
    }

    /// Get recording duration in seconds
    pub fn recording_duration_seconds(&self) -> Option<f64> {
        self.recording_stop_ns.map(|stop| {
            let duration_ns = stop - self.start_ns;
            duration_ns as f64 / 1_000_000_000.0
        })
    }

    /// Get transcription duration in seconds (from recording stop to transcription done)
    pub fn transcription_duration_seconds(&self) -> Option<f64> {
        match (self.recording_stop_ns, self.transcription_done_ns) {
            (Some(start), Some(end)) => {
                let duration_ns = end - start;
                Some(duration_ns as f64 / 1_000_000_000.0)
            }
            _ => None,
        }
    }

    /// Get processing duration in seconds (from transcription done to processing done)
    pub fn processing_duration_seconds(&self) -> Option<f64> {
        match (self.transcription_done_ns, self.processing_done_ns) {
            (Some(start), Some(end)) => {
                let duration_ns = end - start;
                Some(duration_ns as f64 / 1_000_000_000.0)
            }
            _ => None,
        }
    }

    /// Get clipboard duration in seconds (from processing done to clipboard done)
    pub fn clipboard_duration_seconds(&self) -> Option<f64> {
        match (self.processing_done_ns, self.clipboard_done_ns) {
            (Some(start), Some(end)) => {
                let duration_ns = end - start;
                Some(duration_ns as f64 / 1_000_000_000.0)
            }
            _ => None,
        }
    }

    /// Get paste duration in seconds (from clipboard done to paste done)
    pub fn paste_duration_seconds(&self) -> Option<f64> {
        match (self.clipboard_done_ns, self.paste_done_ns) {
            (Some(start), Some(end)) => {
                let duration_ns = end - start;
                Some(duration_ns as f64 / 1_000_000_000.0)
            }
            _ => None,
        }
    }
}

/// Breakdown of timing for each stage of the dictation workflow
#[derive(Debug, Clone)]
pub struct TimingBreakdown {
    /// How long the audio recording was (ms)
    pub recording_ms: u64,
    /// Time to transcribe via daemon (ms)
    pub transcription_ms: u64,
    /// Time for corrections + LLM processing (ms)
    pub processing_ms: u64,
    /// Time for clipboard + paste (ms)
    pub paste_ms: u64,
}

impl TimingBreakdown {
    /// Create a new timing breakdown
    pub fn new(recording_ms: u64, transcription_ms: u64, processing_ms: u64, paste_ms: u64) -> Self {
        Self {
            recording_ms,
            transcription_ms,
            processing_ms,
            paste_ms,
        }
    }

    /// Total end-to-end time (excludes recording since that's user-controlled)
    pub fn processing_total_ms(&self) -> u64 {
        self.transcription_ms + self.processing_ms + self.paste_ms
    }

    /// Total time including recording
    pub fn total_ms(&self) -> u64 {
        self.recording_ms + self.processing_total_ms()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_save_and_load_timestamp() {
        save_start_time().unwrap();
        thread::sleep(Duration::from_millis(10));
        let duration = calculate_duration_from_start().unwrap();
        assert!(duration >= 0.01 && duration < 1.0);
        cleanup_timestamp_file();
    }

    #[test]
    fn test_performance_metrics() {
        save_start_time().unwrap();
        let mut metrics = PerformanceMetrics::from_start_time().unwrap();

        thread::sleep(Duration::from_millis(10));
        metrics.mark_recording_stop();

        thread::sleep(Duration::from_millis(10));
        metrics.mark_transcription_done();

        thread::sleep(Duration::from_millis(10));
        metrics.mark_processing_done();

        thread::sleep(Duration::from_millis(10));
        metrics.mark_clipboard_done();

        thread::sleep(Duration::from_millis(10));
        metrics.mark_paste_done();

        assert!(metrics.total_duration_seconds() >= 0.05);
        assert!(metrics.recording_duration_seconds().is_some());
        assert!(metrics.transcription_duration_seconds().is_some());
        assert!(metrics.processing_duration_seconds().is_some());
        assert!(metrics.clipboard_duration_seconds().is_some());
        assert!(metrics.paste_duration_seconds().is_some());

        cleanup_timestamp_file();
    }
}
