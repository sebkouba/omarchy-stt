//! Groq speech-to-text API
//!
//! This module provides transcription via Groq's Whisper API, which is optimized
//! for low latency. Groq offers blazing-fast inference making it ideal for
//! real-time dictation workflows.
//!
//! # Supported Models
//!
//! - `whisper-large-v3` - Highest quality, multilingual
//! - `whisper-large-v3-turbo` - Fast, multilingual
//! - `distil-whisper-large-v3-en` - Fastest, English only
//!
//! # Authentication
//!
//! Requires GROQ_API_KEY in `~/.config/transcribe-rs/.env`:
//! ```text
//! GROQ_API_KEY=gsk_your_key_here
//! ```
//!
//! # Audio Format
//!
//! Accepts WAV files (16kHz, 16-bit, mono recommended for optimal results).
//! Maximum file size: 25 MB.

use async_trait::async_trait;
use log::debug;
use reqwest::multipart;
use serde::Deserialize;
use std::fs;
use std::path::Path;

use crate::{RemoteTranscriptionEngine, TranscriptionResult, TranscriptionSegment};

const GROQ_TRANSCRIPTION_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";

/// Groq transcription engine using Whisper models
pub struct GroqTranscriptionEngine {
    api_key: String,
    http_client: reqwest::Client,
}

/// Available Groq Whisper models
#[derive(Debug, Clone, Default)]
pub enum GroqModel {
    /// Highest quality, multilingual
    WhisperLargeV3,
    /// Fast, multilingual
    #[default]
    WhisperLargeV3Turbo,
    /// Fastest, English only
    DistilWhisperLargeV3En,
}

impl GroqModel {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::WhisperLargeV3 => "whisper-large-v3",
            Self::WhisperLargeV3Turbo => "whisper-large-v3-turbo",
            Self::DistilWhisperLargeV3En => "distil-whisper-large-v3-en",
        }
    }
}

/// Request parameters for Groq transcription
#[derive(Debug, Clone, Default)]
pub struct GroqRequestParams {
    /// Model to use for transcription
    pub model: GroqModel,
    /// Language code in ISO-639-1 format (e.g., "en", "de")
    /// If not set, Groq will auto-detect the language
    pub language: Option<String>,
    /// Prompt to guide the model (useful for domain-specific terms)
    pub prompt: Option<String>,
    /// Temperature for sampling (0.0-1.0)
    pub temperature: Option<f32>,
}

impl GroqRequestParams {
    /// Create params for English-only fast transcription
    pub fn fast_english() -> Self {
        Self {
            model: GroqModel::DistilWhisperLargeV3En,
            language: Some("en".to_string()),
            ..Default::default()
        }
    }

    /// Create params for high-quality multilingual transcription
    pub fn high_quality() -> Self {
        Self {
            model: GroqModel::WhisperLargeV3,
            ..Default::default()
        }
    }

    /// Create params optimized for low latency with good quality
    pub fn balanced() -> Self {
        Self {
            model: GroqModel::WhisperLargeV3Turbo,
            ..Default::default()
        }
    }
}

/// Response from Groq transcription API (verbose JSON format)
#[derive(Debug, Deserialize)]
struct GroqTranscriptionResponse {
    text: String,
    #[serde(default)]
    segments: Option<Vec<GroqSegment>>,
}

/// A segment in the transcription response
#[derive(Debug, Deserialize)]
struct GroqSegment {
    start: f32,
    end: f32,
    text: String,
}

impl GroqTranscriptionEngine {
    /// Create a new Groq transcription engine with the given API key
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http_client: reqwest::Client::new(),
        }
    }

    /// Create a new Groq transcription engine, loading API key from .env file
    pub fn from_env_file() -> Result<Self, Box<dyn std::error::Error>> {
        let api_key = load_groq_api_key()?;
        Ok(Self::new(api_key))
    }

    /// Synchronous transcription for non-async contexts
    pub fn transcribe_file_sync(
        &self,
        wav_path: &Path,
        params: GroqRequestParams,
    ) -> Result<TranscriptionResult, Box<dyn std::error::Error>> {
        // Check if we're already in a tokio runtime
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // We're inside an existing runtime - use block_in_place
            tokio::task::block_in_place(|| {
                handle.block_on(self.transcribe_file(wav_path, params))
            })
        } else {
            // No runtime exists - create a new one
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(self.transcribe_file(wav_path, params))
        }
    }
}

#[async_trait]
impl RemoteTranscriptionEngine for GroqTranscriptionEngine {
    type RequestParams = GroqRequestParams;

    async fn transcribe_file(
        &self,
        wav_path: &Path,
        params: Self::RequestParams,
    ) -> Result<TranscriptionResult, Box<dyn std::error::Error>> {
        debug!(
            "Transcribing {} with Groq model {}",
            wav_path.display(),
            params.model.as_str()
        );

        // Read the audio file
        let file_bytes = fs::read(wav_path)?;
        let file_name = wav_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("audio.wav")
            .to_string();

        // Build multipart form
        let file_part = multipart::Part::bytes(file_bytes)
            .file_name(file_name)
            .mime_str("audio/wav")?;

        let mut form = multipart::Form::new()
            .part("file", file_part)
            .text("model", params.model.as_str().to_string())
            .text("response_format", "verbose_json");

        if let Some(language) = params.language {
            form = form.text("language", language);
        }

        if let Some(prompt) = params.prompt {
            form = form.text("prompt", prompt);
        }

        if let Some(temperature) = params.temperature {
            form = form.text("temperature", temperature.to_string());
        }

        // Send request
        let response = self
            .http_client
            .post(GROQ_TRANSCRIPTION_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Groq API error ({}): {}", status, error_text).into());
        }

        let groq_response: GroqTranscriptionResponse = response.json().await?;

        debug!("Groq transcription: {}", groq_response.text);

        // Convert segments if available
        let segments = groq_response.segments.map(|segs| {
            segs.into_iter()
                .map(|s| TranscriptionSegment {
                    start: s.start,
                    end: s.end,
                    text: s.text,
                })
                .collect()
        });

        Ok(TranscriptionResult {
            text: groq_response.text,
            segments,
        })
    }
}

/// Load Groq API key from ~/.config/transcribe-rs/.env file
fn load_groq_api_key() -> Result<String, Box<dyn std::error::Error>> {
    let config_dir = dirs::config_dir().ok_or("Could not find config directory")?;
    let env_path = config_dir.join("transcribe-rs").join(".env");

    let env_content = fs::read_to_string(&env_path).map_err(|e| {
        format!(
            "Failed to read .env file at {}: {}\n\
             Create the file with: echo 'GROQ_API_KEY=your_key_here' > {}",
            env_path.display(),
            e,
            env_path.display()
        )
    })?;

    for line in env_content.lines() {
        let line = line.trim();
        if let Some(key) = line.strip_prefix("GROQ_API_KEY=") {
            let key = key.trim();
            // Remove quotes if present
            let key = key.trim_matches('"').trim_matches('\'');
            if !key.is_empty() {
                return Ok(key.to_string());
            }
        }
    }

    Err(format!(
        "GROQ_API_KEY not found in .env file at {}",
        env_path.display()
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_names() {
        assert_eq!(GroqModel::WhisperLargeV3.as_str(), "whisper-large-v3");
        assert_eq!(
            GroqModel::WhisperLargeV3Turbo.as_str(),
            "whisper-large-v3-turbo"
        );
        assert_eq!(
            GroqModel::DistilWhisperLargeV3En.as_str(),
            "distil-whisper-large-v3-en"
        );
    }

    #[test]
    fn test_params_presets() {
        let fast = GroqRequestParams::fast_english();
        assert!(matches!(fast.model, GroqModel::DistilWhisperLargeV3En));
        assert_eq!(fast.language, Some("en".to_string()));

        let quality = GroqRequestParams::high_quality();
        assert!(matches!(quality.model, GroqModel::WhisperLargeV3));

        let balanced = GroqRequestParams::balanced();
        assert!(matches!(balanced.model, GroqModel::WhisperLargeV3Turbo));
    }

    #[test]
    #[ignore] // Requires valid API key and network access
    fn test_groq_transcription() {
        let engine = GroqTranscriptionEngine::from_env_file().expect("Failed to load API key");
        let result = engine
            .transcribe_file_sync(
                Path::new("samples/jfk.wav"),
                GroqRequestParams::fast_english(),
            )
            .expect("Transcription failed");
        assert!(!result.text.is_empty());
        println!("Transcription: {}", result.text);
    }
}
