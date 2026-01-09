//! Voice Activity Detection (VAD) using Silero VAD model.
//!
//! This module provides speech segment detection to filter out silence from audio
//! before transcription. It's particularly useful for longer dictations (>20s)
//! where removing silence improves transcription speed and quality.

use log::{info, warn};
use silero_vad_rust::load_silero_vad;
use silero_vad_rust::silero_vad::utils_vad::{get_speech_timestamps, VadParameters};
use std::error::Error;

use crate::config::VadConfig;

/// Default threshold for speech detection (0.0-1.0)
/// Lower = less likely to cut off speech
pub const DEFAULT_THRESHOLD: f32 = 0.3;

/// Default minimum speech duration in milliseconds
pub const DEFAULT_MIN_SPEECH_MS: u32 = 250;

/// Default minimum silence duration in milliseconds
/// Higher = requires longer silence before splitting segments
pub const DEFAULT_MIN_SILENCE_MS: u32 = 300;

/// Default audio duration threshold in seconds (only apply VAD for longer audio)
pub const DEFAULT_MIN_DURATION_SECONDS: f32 = 20.0;

/// Sample rate expected by Silero VAD (must be 16kHz)
const SAMPLE_RATE: u32 = 16000;

/// A speech segment with start and end sample indices
#[derive(Debug, Clone)]
pub struct SpeechSegment {
    /// Start sample index
    pub start: usize,
    /// End sample index
    pub end: usize,
}

impl SpeechSegment {
    /// Duration of this segment in seconds
    pub fn duration_seconds(&self) -> f32 {
        (self.end - self.start) as f32 / SAMPLE_RATE as f32
    }
}

/// Result of VAD processing
#[derive(Debug)]
pub struct VadResult {
    /// Speech segments detected
    pub segments: Vec<SpeechSegment>,
    /// Whether VAD was actually applied (false if audio too short)
    pub vad_applied: bool,
    /// Original audio duration in seconds
    pub original_duration_seconds: f32,
    /// Filtered audio duration in seconds (speech only)
    pub speech_duration_seconds: f32,
}

/// Voice Activity Detection manager
///
/// Wraps the Silero VAD model and provides methods to detect and extract
/// speech segments from audio.
pub struct VadManager {
    config: VadConfig,
}

impl VadManager {
    /// Create a new VadManager with the given configuration
    pub fn new(config: VadConfig) -> Self {
        Self { config }
    }

    /// Check if VAD should be applied based on audio duration
    pub fn should_apply_vad(&self, samples: &[f32]) -> bool {
        if !self.config.enabled {
            return false;
        }

        let duration_seconds = samples.len() as f32 / SAMPLE_RATE as f32;
        duration_seconds >= self.config.min_duration_seconds
    }

    /// Process audio samples and extract speech segments
    ///
    /// Returns the filtered samples containing only speech if VAD is enabled
    /// and audio duration exceeds the threshold. Otherwise returns the original
    /// samples unchanged.
    pub fn process(&self, samples: &[f32]) -> Result<(Vec<f32>, VadResult), Box<dyn Error>> {
        let original_duration = samples.len() as f32 / SAMPLE_RATE as f32;

        if !self.should_apply_vad(samples) {
            if self.config.enabled {
                info!(
                    "VAD skipped: {:.1}s < {:.1}s threshold",
                    original_duration, self.config.min_duration_seconds
                );
            }
            return Ok((
                samples.to_vec(),
                VadResult {
                    segments: vec![],
                    vad_applied: false,
                    original_duration_seconds: original_duration,
                    speech_duration_seconds: original_duration,
                },
            ));
        }

        info!(
            "Applying VAD to {:.1}s audio (threshold: {})",
            original_duration, self.config.threshold
        );

        // Load Silero VAD model
        let mut vad = load_silero_vad()?;

        // Configure VAD parameters
        let params = VadParameters {
            threshold: self.config.threshold,
            sampling_rate: SAMPLE_RATE,
            min_speech_duration_ms: self.config.min_speech_duration_ms as u32,
            min_silence_duration_ms: self.config.min_silence_duration_ms as u32,
            speech_pad_ms: 30,     // Add small padding around speech
            return_seconds: false, // Return sample indices
            ..Default::default()
        };

        // Get speech timestamps
        let timestamps = get_speech_timestamps(samples, &mut vad, &params)?;

        if timestamps.is_empty() {
            warn!("VAD found no speech segments, returning original audio");
            return Ok((
                samples.to_vec(),
                VadResult {
                    segments: vec![],
                    vad_applied: true,
                    original_duration_seconds: original_duration,
                    speech_duration_seconds: original_duration,
                },
            ));
        }

        // Convert timestamps to segments
        let segments: Vec<SpeechSegment> = timestamps
            .iter()
            .map(|ts| SpeechSegment {
                start: ts.start as usize,
                end: ts.end as usize,
            })
            .collect();

        // Extract speech audio
        let mut speech_samples = Vec::new();
        for segment in &segments {
            let start = segment.start.min(samples.len());
            let end = segment.end.min(samples.len());
            speech_samples.extend_from_slice(&samples[start..end]);
        }

        let speech_duration = speech_samples.len() as f32 / SAMPLE_RATE as f32;

        info!(
            "VAD: {} segments, {:.1}s speech from {:.1}s audio ({:.0}% kept)",
            segments.len(),
            speech_duration,
            original_duration,
            (speech_duration / original_duration) * 100.0
        );

        Ok((
            speech_samples,
            VadResult {
                segments,
                vad_applied: true,
                original_duration_seconds: original_duration,
                speech_duration_seconds: speech_duration,
            },
        ))
    }

    /// Process i16 samples (converts to f32 internally)
    pub fn process_i16(&self, samples: &[i16]) -> Result<(Vec<i16>, VadResult), Box<dyn Error>> {
        // Convert i16 to f32 (normalized to -1.0..1.0)
        let f32_samples: Vec<f32> = samples.iter().map(|&s| s as f32 / 32768.0).collect();

        let (filtered_f32, result) = self.process(&f32_samples)?;

        // Convert back to i16
        let filtered_i16: Vec<i16> = filtered_f32
            .iter()
            .map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16)
            .collect();

        Ok((filtered_i16, result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(enabled: bool, min_duration: f32) -> VadConfig {
        VadConfig {
            enabled,
            threshold: DEFAULT_THRESHOLD,
            min_duration_seconds: min_duration,
            min_speech_duration_ms: DEFAULT_MIN_SPEECH_MS as i32,
            min_silence_duration_ms: DEFAULT_MIN_SILENCE_MS as i32,
        }
    }

    #[test]
    fn test_should_apply_vad_disabled() {
        let vad = VadManager::new(make_config(false, 20.0));
        let samples = vec![0.0f32; SAMPLE_RATE as usize * 30]; // 30 seconds
        assert!(!vad.should_apply_vad(&samples));
    }

    #[test]
    fn test_should_apply_vad_short_audio() {
        let vad = VadManager::new(make_config(true, 20.0));
        let samples = vec![0.0f32; SAMPLE_RATE as usize * 10]; // 10 seconds
        assert!(!vad.should_apply_vad(&samples));
    }

    #[test]
    fn test_should_apply_vad_long_audio() {
        let vad = VadManager::new(make_config(true, 20.0));
        let samples = vec![0.0f32; SAMPLE_RATE as usize * 30]; // 30 seconds
        assert!(vad.should_apply_vad(&samples));
    }
}
