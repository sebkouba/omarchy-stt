//! Transcription and API timing estimation
//!
//! Tracks historical timing data to estimate how long future operations will take:
//! - Transcription: based on recording duration
//! - API calls: based on text length

use log::{debug, warn};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::Instant;

const TIMING_LOG_FILE: &str = "/tmp/ptt_transcription_timing.log";
const API_TIMING_LOG_FILE: &str = "/tmp/ptt_api_timing.log";
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

    debug!(
        "Logged timing: recording={}ms, transcription={}ms",
        recording_ms, transcription_ms
    );
    Ok(())
}

/// Read timing entries from the history file
fn read_timing_entries() -> Vec<String> {
    let path = PathBuf::from(TIMING_LOG_FILE);

    match fs::File::open(&path) {
        Ok(file) => BufReader::new(file)
            .lines()
            .filter_map(|l| l.ok())
            .filter(|l| !l.is_empty())
            .collect(),
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
                Some(TimingEntry {
                    recording_ms,
                    transcription_ms,
                })
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
        clamped,
        ratio,
        entries.len()
    );

    Some(clamped)
}

/// Get the default fallback duration for pulsing animation
pub const FALLBACK_PULSE_MS: u64 = 500;

/// Estimate VAD processing time based on audio duration
/// VAD typically takes 50-100ms per second of audio (Silero model)
pub fn estimate_vad_time(audio_duration_ms: u64) -> u64 {
    // Rough estimate: 80ms per second of audio, minimum 200ms
    let estimate = (audio_duration_ms as f64 * 0.08) as u64;
    estimate.max(200).min(3000) // Cap at 3 seconds
}

/// Estimate total processing time (VAD + transcription)
/// Use this when starting progress before calling stop_recording()
pub fn estimate_total_processing_time(estimated_audio_ms: u64) -> u64 {
    let vad_time = estimate_vad_time(estimated_audio_ms);
    let transcription_time = estimate_transcription_time(estimated_audio_ms).unwrap_or(500);
    vad_time + transcription_time
}

// ============================================================================
// API Timing
// ============================================================================

/// API timing data point (text_length_chars, api_duration_ms)
#[derive(Debug, Clone)]
pub struct ApiTimingEntry {
    /// Input text length in characters
    pub text_length: u64,
    /// API call duration in milliseconds
    pub api_ms: u64,
}

/// API call timer
#[derive(Debug)]
pub struct ApiTimer {
    start_time: Option<Instant>,
    text_length: u64,
}

impl ApiTimer {
    /// Create a new API timer with the input text length
    pub fn new(text_length: usize) -> Self {
        Self {
            start_time: None,
            text_length: text_length as u64,
        }
    }

    /// Start timing the API call
    pub fn start(&mut self) {
        self.start_time = Some(Instant::now());
    }

    /// Stop timing and log the result
    /// Returns the API duration in milliseconds
    pub fn stop_and_log(&self) -> Option<u64> {
        let start = self.start_time?;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        // Log the timing entry
        if let Err(e) = log_api_timing_entry(self.text_length, elapsed_ms) {
            warn!("Failed to log API timing entry: {}", e);
        }

        Some(elapsed_ms)
    }
}

/// Log an API timing entry to the history file
fn log_api_timing_entry(text_length: u64, api_ms: u64) -> std::io::Result<()> {
    let path = PathBuf::from(API_TIMING_LOG_FILE);

    // Read existing entries
    let mut entries = read_api_timing_entries();

    // Add new entry
    entries.push(format!("{},{}", text_length, api_ms));

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

    debug!(
        "Logged API timing: text_length={}, api={}ms",
        text_length, api_ms
    );
    Ok(())
}

/// Read API timing entries from the history file
fn read_api_timing_entries() -> Vec<String> {
    let path = PathBuf::from(API_TIMING_LOG_FILE);

    match fs::File::open(&path) {
        Ok(file) => BufReader::new(file)
            .lines()
            .filter_map(|l| l.ok())
            .filter(|l| !l.is_empty())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Parse API timing entries
fn parse_api_timing_entries() -> Vec<ApiTimingEntry> {
    read_api_timing_entries()
        .iter()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() == 2 {
                let text_length = parts[0].parse().ok()?;
                let api_ms = parts[1].parse().ok()?;
                Some(ApiTimingEntry {
                    text_length,
                    api_ms,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Estimate API call time based on text length
/// Returns estimated milliseconds, or None if no historical data
pub fn estimate_api_time(text_length: usize) -> Option<u64> {
    let entries = parse_api_timing_entries();

    if entries.is_empty() {
        debug!("No API timing history, cannot estimate");
        return None;
    }

    // For API calls, we use a simpler approach: average time per character
    // plus a base overhead
    let total_chars: u64 = entries.iter().map(|e| e.text_length).sum();
    let total_ms: u64 = entries.iter().map(|e| e.api_ms).sum();

    if total_chars == 0 {
        // If no chars but have entries, just use average time
        let avg = total_ms / entries.len() as u64;
        return Some(avg.max(200).min(10000));
    }

    // ms per character ratio
    let ms_per_char = total_ms as f64 / total_chars as f64;
    let estimated = (text_length as f64 * ms_per_char) as u64;

    // Add buffer and clamp
    let with_buffer = (estimated as f64 * 1.2) as u64;
    let clamped = with_buffer.max(300).min(15000);

    debug!(
        "Estimated API time: {}ms (ms_per_char={:.3}, entries={})",
        clamped,
        ms_per_char,
        entries.len()
    );

    Some(clamped)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_estimate_no_history() {
        // When there's no history, should return None
        // This test assumes the log file doesn't exist or is empty in test env
    }
}
