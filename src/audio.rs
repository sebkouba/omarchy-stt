//! Audio processing utilities for transcription.
//!
//! This module provides functions for reading and processing audio files
//! to prepare them for transcription engines.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Read WAV file samples and convert them to the required format.
///
/// This function reads a WAV file and converts it to the format expected by
/// transcription engines: 16kHz sample rate, 16-bit samples, mono channel.
///
/// # Arguments
///
/// * `wav_path` - Path to the WAV file to read
///
/// # Returns
///
/// Returns a vector of f32 samples normalized to the range [-1.0, 1.0].
///
/// # Errors
///
/// This function will return an error if:
/// - The file cannot be opened or read
/// - The WAV format is incorrect (not 16kHz, 16-bit, mono)
/// - The samples cannot be converted to the expected format
///
/// # Examples
///
/// ```rust,no_run
/// use transcribe_rs::audio::read_wav_samples;
/// use std::path::Path;
///
/// let samples = read_wav_samples(Path::new("audio.wav"))?;
/// println!("Loaded {} samples", samples.len());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Audio Requirements
///
/// The input WAV file must have:
/// - Sample rate: 16,000 Hz
/// - Bit depth: 16 bits per sample
/// - Channels: 1 (mono)
/// - Format: PCM integer samples
pub fn read_wav_samples(wav_path: &Path) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let mut reader = hound::WavReader::open(wav_path)?;
    let spec = reader.spec();

    let expected_spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    if spec.channels != expected_spec.channels {
        return Err(format!(
            "Expected {} channels, found {}",
            expected_spec.channels, spec.channels
        )
        .into());
    }

    if spec.sample_rate != expected_spec.sample_rate {
        return Err(format!(
            "Expected {} Hz sample rate, found {} Hz",
            expected_spec.sample_rate, spec.sample_rate
        )
        .into());
    }

    if spec.bits_per_sample != expected_spec.bits_per_sample {
        return Err(format!(
            "Expected {} bits per sample, found {}",
            expected_spec.bits_per_sample, spec.bits_per_sample
        )
        .into());
    }

    if spec.sample_format != expected_spec.sample_format {
        return Err(format!("Expected Int sample format, found {:?}", spec.sample_format).into());
    }

    let samples: Result<Vec<f32>, _> = reader
        .samples::<i16>()
        .map(|sample| sample.map(|s| s as f32 / i16::MAX as f32))
        .collect();

    Ok(samples?)
}

/// Write i16 PCM samples as a WAV file
///
/// This function creates a properly formatted WAV file from raw i16 PCM samples.
/// The output WAV file will have a 44-byte header followed by the raw sample data.
///
/// # Arguments
///
/// * `samples` - Slice of i16 PCM samples
/// * `sample_rate` - Sample rate in Hz (typically 16000)
/// * `output_path` - Path where the WAV file should be written
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if file writing fails.
///
/// # Examples
///
/// ```rust,no_run
/// use transcribe_rs::audio::write_wav_from_samples;
/// use std::path::Path;
///
/// let samples = vec![0i16; 16000]; // 1 second of silence at 16kHz
/// write_wav_from_samples(&samples, 16000, Path::new("output.wav"))?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn write_wav_from_samples(
    samples: &[i16],
    sample_rate: u32,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = BufWriter::new(File::create(output_path)?);

    // Calculate sizes
    let num_samples = samples.len() as u32;
    let num_channels: u16 = 1; // Mono
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * num_channels as u32 * bits_per_sample as u32 / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_size = num_samples * num_channels as u32 * bits_per_sample as u32 / 8;
    let file_size = 36 + data_size;

    // Write RIFF header (12 bytes)
    writer.write_all(b"RIFF")?;
    writer.write_all(&file_size.to_le_bytes())?;
    writer.write_all(b"WAVE")?;

    // Write fmt chunk (24 bytes)
    writer.write_all(b"fmt ")?;
    writer.write_all(&16u32.to_le_bytes())?; // Chunk size
    writer.write_all(&1u16.to_le_bytes())?; // PCM format
    writer.write_all(&num_channels.to_le_bytes())?;
    writer.write_all(&sample_rate.to_le_bytes())?;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&block_align.to_le_bytes())?;
    writer.write_all(&bits_per_sample.to_le_bytes())?;

    // Write data chunk (8 bytes + data)
    writer.write_all(b"data")?;
    writer.write_all(&data_size.to_le_bytes())?;

    // Write sample data - batch convert then write for performance
    let sample_bytes: Vec<u8> = samples
        .iter()
        .flat_map(|&sample| sample.to_le_bytes())
        .collect();
    writer.write_all(&sample_bytes)?;

    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_write_read_roundtrip() {
        // Create temporary file path
        let temp_path = std::env::temp_dir().join("test_roundtrip.wav");

        // Write samples
        let original_samples: Vec<i16> = vec![100, -200, 300, -400, 500];
        write_wav_from_samples(&original_samples, 16000, &temp_path).unwrap();

        // Read back
        let read_samples = read_wav_samples(&temp_path).unwrap();

        // Convert back to i16 for comparison
        let read_i16: Vec<i16> = read_samples
            .iter()
            .map(|&f| (f * i16::MAX as f32) as i16)
            .collect();

        assert_eq!(original_samples, read_i16);

        // Cleanup
        fs::remove_file(&temp_path).ok();
    }

    #[test]
    fn test_write_silence() {
        let temp_path = std::env::temp_dir().join("test_silence.wav");

        // 1 second of silence at 16kHz
        let samples = vec![0i16; 16000];
        write_wav_from_samples(&samples, 16000, &temp_path).unwrap();

        // Verify file exists and has reasonable size
        let metadata = fs::metadata(&temp_path).unwrap();
        // 44 bytes header + 16000 samples * 2 bytes = 32044 bytes
        assert_eq!(metadata.len(), 32044);

        // Read back and verify
        let read_samples = read_wav_samples(&temp_path).unwrap();
        assert_eq!(read_samples.len(), 16000);
        assert!(read_samples.iter().all(|&s| s == 0.0));

        fs::remove_file(&temp_path).ok();
    }

    #[test]
    fn test_write_sine_wave() {
        let temp_path = std::env::temp_dir().join("test_sine.wav");

        // Generate 440Hz sine wave at 16kHz for 0.5 seconds
        let sample_rate = 16000;
        let duration = 0.5;
        let frequency = 440.0;
        let num_samples = (sample_rate as f32 * duration) as usize;

        let samples: Vec<i16> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                let amplitude = 0.8 * i16::MAX as f32;
                (amplitude * (2.0 * std::f32::consts::PI * frequency * t).sin()) as i16
            })
            .collect();

        write_wav_from_samples(&samples, sample_rate as u32, &temp_path).unwrap();

        // Verify file
        let read_samples = read_wav_samples(&temp_path).unwrap();
        assert_eq!(read_samples.len(), num_samples);

        // Verify it's not silence (should have non-zero values)
        let has_signal = read_samples.iter().any(|&s| s.abs() > 0.1);
        assert!(has_signal);

        fs::remove_file(&temp_path).ok();
    }
}
