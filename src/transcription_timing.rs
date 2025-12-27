//! Transcription timing estimation
//!
//! Tracks historical transcription times to estimate how long future
//! transcriptions will take based on recording duration.

use log::{debug, warn};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::Instant;

const TIMING_LOG_FILE: &str = "/tmp/ptt_transcription_timing.log";
const MAX_ENTRIES: usize = 100;

/// A single timing data point
#[derive(Debug, Clone)]
pub struct TimingEntry {
    /// Recording duration in milliseconds
    pub recording_ms: u64,
    /// Transcription duration in milliseconds
    pub transcription_ms: u64,
}

/// Transcription timing estimator
#[derive(Debug)]
pub struct TranscriptionTimer {
    start_time: Option<Instant>,
    recording_duration_ms: u64,
}

impl TranscriptionTimer {
    /// Create a new timer with the recording duration
    pub fn new(recording_duration_ms: u64) -> Self {
        Self {
            start_time: None,
            recording_duration_ms,
        }
    }

    /// Start timing the transcription
    pub fn start(&mut self) {
        self.start_time = Some(Instant::now());
    }

    /// Stop timing and log the result
    /// Returns the transcription duration in milliseconds
    pub fn stop_and_log(&self) -> Option<u64> {
        let start = self.start_time?;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        // Log the timing entry
        if let Err(e) = log_timing_entry(self.recording_duration_ms, elapsed_ms) {
            warn!("Failed to log timing entry: {}", e);
        }

        Some(elapsed_ms)
    }
}

/// Log a timing entry to the history file
fn log_timing_entry(recording_ms: u64, transcription_ms: u64) -> std::io::Result<()> {
    let path = PathBuf::from(TIMING_LOG_FILE);

    // Read existing entries
    let mut entries = read_timing_entries();

    // Add new entry
    entries.push(format!("{},{}", recording_ms, transcription_ms));

    // Keep only the last MAX_ENTRIES
    if entries.len() > MAX_ENTRIES {
        entries = entries.split_off(entries.len() - MAX_ENTRIES);
    }

    // Write back
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;

    for entry in entries {
        writeln!(file, "{}", entry)?;
    }

    debug!("Logged timing: recording={}ms, transcription={}ms", recording_ms, transcription_ms);
    Ok(())
}

/// Read timing entries from the history file
fn read_timing_entries() -> Vec<String> {
    let path = PathBuf::from(TIMING_LOG_FILE);

    match fs::File::open(&path) {
        Ok(file) => {
            BufReader::new(file)
                .lines()
                .filter_map(|l| l.ok())
                .filter(|l| !l.is_empty())
                .collect()
        }
        Err(_) => Vec::new(),
    }
}

/// Parse timing entries
fn parse_timing_entries() -> Vec<TimingEntry> {
    read_timing_entries()
        .iter()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() == 2 {
                let recording_ms = parts[0].parse().ok()?;
                let transcription_ms = parts[1].parse().ok()?;
                Some(TimingEntry { recording_ms, transcription_ms })
            } else {
                None
            }
        })
        .collect()
}

/// Estimate transcription time based on recording duration
/// Returns estimated milliseconds, or None if no historical data
pub fn estimate_transcription_time(recording_ms: u64) -> Option<u64> {
    let entries = parse_timing_entries();

    if entries.is_empty() {
        debug!("No timing history, cannot estimate");
        return None;
    }

    // Calculate average ratio: transcription_ms / recording_ms
    let total_recording: u64 = entries.iter().map(|e| e.recording_ms).sum();
    let total_transcription: u64 = entries.iter().map(|e| e.transcription_ms).sum();

    if total_recording == 0 {
        return None;
    }

    // Ratio is typically around 0.05-0.2 (5-20% of recording time on good hardware)
    let ratio = total_transcription as f64 / total_recording as f64;
    let estimated = (recording_ms as f64 * ratio) as u64;

    // Add a small buffer (20%) to avoid finishing before the bar completes
    let with_buffer = (estimated as f64 * 1.2) as u64;

    // Minimum 200ms, maximum 10s
    let clamped = with_buffer.max(200).min(10000);

    debug!(
        "Estimated transcription time: {}ms (ratio={:.3}, entries={})",
        clamped, ratio, entries.len()
    );

    Some(clamped)
}

/// Get the default fallback duration for pulsing animation
pub const FALLBACK_PULSE_MS: u64 = 500;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_no_history() {
        // When there's no history, should return None
        // This test assumes the log file doesn't exist or is empty in test env
    }
}
