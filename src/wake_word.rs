//! Wake word detection using OpenWakeWord ONNX models
//!
//! This module provides voice-activated wake word detection to trigger dictation
//! start/stop commands instead of requiring keyboard hotkeys.
//!
//! ## Architecture
//!
//! The OpenWakeWord detection pipeline has three stages:
//! 1. **Melspectrogram**: Converts raw audio to mel-frequency spectrogram
//! 2. **Embedding**: Extracts speech features using a shared model
//! 3. **Classification**: Detects specific wake words
//!
//! ## Usage
//!
//! ```rust,no_run
//! use transcribe_rs::wake_word::{WakeWordDetector, WakeWordConfig};
//!
//! let config = WakeWordConfig {
//!     model_path: "models/wake_words".into(),
//!     wake_word: "alexa".to_string(),
//!     threshold: 0.5,
//! };
//!
//! let detector = WakeWordDetector::new(config)?;
//! detector.start_listening(|word| {
//!     println!("Detected: {}", word);
//! })?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ndarray::Array2;
use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Value,
};
use std::collections::VecDeque;
use std::error::Error;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::fs;

/// Configuration for wake word detection
#[derive(Clone, Debug)]
pub struct WakeWordConfig {
    /// Path to directory containing ONNX models
    pub model_path: PathBuf,
    /// Wake word to detect (e.g., "alexa", "hey_jarvis")
    pub wake_word: String,
    /// Detection threshold (0.0-1.0, default 0.5)
    pub threshold: f32,
    /// Sample rate for input audio (default 16000)
    pub sample_rate: u32,
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from("models/wake_words"),
            wake_word: "alexa".to_string(),
            threshold: 0.5,
            sample_rate: 16000,
        }
    }
}

/// Wake word detection engine
pub struct WakeWordDetector {
    config: WakeWordConfig,
    melspec_session: Session,
    embedding_session: Session,
    classifier_session: Session,
}

impl WakeWordDetector {
    /// Create a new wake word detector
    pub fn new(config: WakeWordConfig) -> Result<Self, Box<dyn Error>> {
        log(&format!("Initializing wake word detector for '{}'", config.wake_word), "/tmp/ptt_rust_debug.log");

        // Validate model files exist
        let melspec_path = config.model_path.join("melspectrogram.onnx");
        let embedding_path = config.model_path.join("embedding_model.onnx");
        let classifier_path = config.model_path.join(format!("{}_v0.1.onnx", config.wake_word));

        if !melspec_path.exists() {
            return Err(format!("Melspectrogram model not found: {:?}", melspec_path).into());
        }
        if !embedding_path.exists() {
            return Err(format!("Embedding model not found: {:?}", embedding_path).into());
        }
        if !classifier_path.exists() {
            return Err(format!("Classifier model not found: {:?}", classifier_path).into());
        }

        log("Loading ONNX models...", "/tmp/ptt_rust_debug.log");

        // Load models with optimizations
        let melspec_session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(2)?
            .commit_from_file(melspec_path)?;

        let embedding_session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(2)?
            .commit_from_file(embedding_path)?;

        let classifier_session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(1)?
            .commit_from_file(classifier_path)?;

        log("Models loaded successfully", "/tmp/ptt_rust_debug.log");

        Ok(Self {
            config,
            melspec_session,
            embedding_session,
            classifier_session,
        })
    }

    /// Detect wake word in audio samples
    ///
    /// # Arguments
    ///
    /// * `samples` - Audio samples at 16kHz, mono, normalized to [-1.0, 1.0]
    ///
    /// # Returns
    ///
    /// Confidence score between 0.0 and 1.0
    pub fn detect(&mut self, samples: &[f32]) -> Result<f32, Box<dyn Error>> {
        // OpenWakeWord expects 1280 samples (80ms at 16kHz)
        const FRAME_SIZE: usize = 1280;

        if samples.len() != FRAME_SIZE {
            return Err(format!("Expected {} samples, got {}", FRAME_SIZE, samples.len()).into());
        }

        // Step 1: Compute melspectrogram
        // Input shape: [1, 1280] (batch, samples)
        let audio_array = Array2::from_shape_vec((1, FRAME_SIZE), samples.to_vec())?;
        let audio_value = Value::from_array(audio_array)?;
        let melspec_outputs = self.melspec_session.run(ort::inputs!["input" => &audio_value])?;

        // Step 2: Extract embeddings
        // The melspectrogram output is fed to embedding model
        let melspec = &melspec_outputs["output"];
        let embedding_outputs = self.embedding_session.run(ort::inputs!["input" => melspec])?;

        // Step 3: Classify wake word
        let embeddings = &embedding_outputs["output"];
        let classifier_outputs = self.classifier_session.run(ort::inputs!["input" => embeddings])?;

        // Get confidence score (assuming single output value)
        let scores = classifier_outputs["output"].try_extract_tensor::<f32>()?;
        let confidence = scores.1[0]; // (shape, data) tuple - get first element of data

        Ok(confidence)
    }

    /// Start listening for wake words continuously
    ///
    /// # Arguments
    ///
    /// * `callback` - Function to call when wake word is detected
    pub fn start_listening<F>(&mut self, callback: F) -> Result<(), Box<dyn Error>>
    where
        F: Fn(&str) + Send + 'static,
    {
        log("Starting continuous wake word listening...", "/tmp/ptt_rust_debug.log");

        // Set up audio input
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or("No input device available")?;

        log(&format!("Using audio device: {:?}", device.name()?), "/tmp/ptt_rust_debug.log");

        let config = device.default_input_config()?;
        log(&format!("Audio config: {:?}", config), "/tmp/ptt_rust_debug.log");

        // Buffer for accumulating audio samples (80ms frames = 1280 samples at 16kHz)
        const FRAME_SIZE: usize = 1280;
        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::with_capacity(FRAME_SIZE * 2)));
        let buffer_clone = buffer.clone();

        // Detection parameters
        let threshold = self.config.threshold;
        let wake_word = self.config.wake_word.clone();

        // Build audio stream
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        let mut buf = buffer_clone.lock().unwrap();
                        buf.extend(data.iter().copied());
                    },
                    |err| log(&format!("Audio stream error: {}", err), "/tmp/ptt_rust_debug.log"),
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let mut buf = buffer_clone.lock().unwrap();
                        // Convert i16 to f32 normalized [-1.0, 1.0]
                        buf.extend(data.iter().map(|&s| s as f32 / 32768.0));
                    },
                    |err| log(&format!("Audio stream error: {}", err), "/tmp/ptt_rust_debug.log"),
                    None,
                )?
            }
            cpal::SampleFormat::U16 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        let mut buf = buffer_clone.lock().unwrap();
                        // Convert u16 to f32 normalized [-1.0, 1.0]
                        buf.extend(data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0));
                    },
                    |err| log(&format!("Audio stream error: {}", err), "/tmp/ptt_rust_debug.log"),
                    None,
                )?
            }
            _ => return Err("Unsupported sample format".into()),
        };

        stream.play()?;
        log("Audio stream started", "/tmp/ptt_rust_debug.log");

        // Detection loop
        loop {
            std::thread::sleep(std::time::Duration::from_millis(10));

            let mut samples = Vec::new();
            {
                let mut buf = buffer.lock().unwrap();
                if buf.len() >= FRAME_SIZE {
                    // Extract frame
                    samples = buf.drain(..FRAME_SIZE).collect();
                }
            }

            if !samples.is_empty() {
                match self.detect(&samples) {
                    Ok(confidence) => {
                        if confidence >= threshold {
                            log(&format!("Wake word detected! Confidence: {:.3}", confidence), "/tmp/ptt_rust_debug.log");
                            callback(&wake_word);
                        }
                    }
                    Err(e) => {
                        log(&format!("Detection error: {}", e), "/tmp/ptt_rust_debug.log");
                    }
                }
            }
        }
    }
}

/// Append a log message to the debug log
fn log(message: &str, log_file: &str) {
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        writeln!(file, "[{}] [wake_word] {}", timestamp, message).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = WakeWordConfig::default();
        assert_eq!(config.wake_word, "alexa");
        assert_eq!(config.threshold, 0.5);
        assert_eq!(config.sample_rate, 16000);
    }
}
