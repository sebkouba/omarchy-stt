//! Configuration management for transcribe-rs

use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub audio: AudioConfig,
    pub model: ModelConfig,
    pub daemon: DaemonConfig,
    pub integration: IntegrationConfig,
    #[serde(default)]
    pub transcription_corrections: TranscriptionCorrectionsConfig,
    #[serde(default)]
    pub dictation_logging: DictationLoggingConfig,
    #[serde(default)]
    pub llm: LlmConfig,
}

/// Audio recording configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// PulseAudio/ALSA microphone source (use "default" for system default)
    pub microphone: String,
    /// Sample rate in Hz (must be 16000 for current models)
    pub sample_rate: u32,
    /// Path where recordings are saved
    pub recording_path: String,
    /// Path where recording PID is stored
    pub recording_pid_file: String,
    /// Path where debug logs are written
    pub log_file: String,
}

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Path to the model directory or file
    pub path: String,
    /// Engine to use: "parakeet" or "whisper"
    pub engine: String,
    /// Quantization: "int8" or "fp32" (parakeet only)
    pub quantization: String,
}

/// Daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Unix socket path for daemon communication
    pub socket_path: String,
}

/// Desktop integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationConfig {
    /// Automatically paste transcription into active window
    pub auto_paste: bool,
    /// Add space after sentence-ending punctuation (.!?)
    pub add_space_after_punctuation: bool,
    /// List of terminal application classes (for Ctrl+Shift+V vs Ctrl+V)
    pub terminal_apps: Vec<String>,
}

/// Transcription error corrections (phonetic/acoustic fixes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionCorrectionsConfig {
    /// Enable transcription corrections
    pub enabled: bool,
    /// Path to corrections file (JSON with from/to rules)
    pub corrections_file: String,
}

/// Dictation logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictationLoggingConfig {
    /// Master toggle for all dictation logging
    pub enabled: bool,
    /// Enable logging for non-LLM dictations
    pub basic_log_enabled: bool,
    /// Enable logging for LLM-processed dictations
    pub llm_log_enabled: bool,
    /// Path to basic dictation log (CSV format)
    pub basic_log_path: String,
    /// Path to LLM corrections log (CSV format)
    pub llm_log_path: String,
}

/// LLM conversation history configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Enable conversation history for multi-turn conversations
    pub conversation_history_enabled: bool,
    /// How many minutes of history to include
    pub conversation_history_minutes: u32,
    /// Maximum number of turns (user+assistant pairs) to include
    pub conversation_max_turns: usize,
    /// Directory for storing history files
    pub conversation_history_dir: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            audio: AudioConfig::default(),
            model: ModelConfig::default(),
            daemon: DaemonConfig::default(),
            integration: IntegrationConfig::default(),
            transcription_corrections: TranscriptionCorrectionsConfig::default(),
            dictation_logging: DictationLoggingConfig::default(),
            llm: LlmConfig::default(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        AudioConfig {
            microphone: "default".to_string(),
            sample_rate: 16000,
            recording_path: "/tmp/ptt_current.wav".to_string(),
            recording_pid_file: "/tmp/ptt_recording.pid".to_string(),
            log_file: "/tmp/ptt_rust_debug.log".to_string(),
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        ModelConfig {
            path: "models/parakeet-tdt-0.6b-v3-int8".to_string(),
            engine: "parakeet".to_string(),
            quantization: "int8".to_string(),
        }
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        DaemonConfig {
            socket_path: "/tmp/transcribe-rs-v2.sock".to_string(),
        }
    }
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        IntegrationConfig {
            auto_paste: true,
            add_space_after_punctuation: true,
            terminal_apps: vec![
                "alacritty".to_string(),
                "kitty".to_string(),
                "wezterm".to_string(),
                "foot".to_string(),
                "terminal".to_string(),
                "konsole".to_string(),
                "terminator".to_string(),
                "xterm".to_string(),
                "urxvt".to_string(),
                "st".to_string(),
                "code".to_string(),
            ],
        }
    }
}

impl Default for TranscriptionCorrectionsConfig {
    fn default() -> Self {
        let config_dir = dirs::config_dir()
            .map(|d| d.join("transcribe-rs"))
            .unwrap_or_else(|| PathBuf::from("/tmp/transcribe-rs"));

        TranscriptionCorrectionsConfig {
            enabled: true,
            corrections_file: config_dir.join("transcription_corrections.json")
                .to_string_lossy()
                .to_string(),
        }
    }
}

impl Default for DictationLoggingConfig {
    fn default() -> Self {
        let config_dir = dirs::config_dir()
            .map(|d| d.join("transcribe-rs"))
            .unwrap_or_else(|| PathBuf::from("/tmp/transcribe-rs"));

        DictationLoggingConfig {
            enabled: false,  // Disabled by default for privacy
            basic_log_enabled: true,
            llm_log_enabled: true,
            basic_log_path: config_dir.join("dictation_log.csv")
                .to_string_lossy()
                .to_string(),
            llm_log_path: config_dir.join("llm_corrections_log.csv")
                .to_string_lossy()
                .to_string(),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        LlmConfig {
            conversation_history_enabled: true,
            conversation_history_minutes: 5,
            conversation_max_turns: 10,
            conversation_history_dir: "/tmp".to_string(),
        }
    }
}

impl Config {
    /// Load configuration from file, or return defaults if file doesn't exist
    pub fn load() -> Result<Self, Box<dyn Error>> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            // Return default config if file doesn't exist
            Ok(Config::default())
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let config_path = Self::config_path()?;

        // Create config directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        fs::write(config_path, content)?;
        Ok(())
    }

    /// Get the path to the config file
    pub fn config_path() -> Result<PathBuf, Box<dyn Error>> {
        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?;
        Ok(config_dir.join("transcribe-rs").join("config.toml"))
    }
}
