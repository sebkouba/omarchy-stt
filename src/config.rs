//! Configuration management for transcribe-rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    #[serde(default)]
    pub ocr: OcrConfig,
    #[serde(default)]
    pub watch: WatchConfig,
    #[serde(default)]
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    pub vad: VadConfig,
    #[serde(default)]
    pub transcription: TranscriptionConfig,
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
    /// Restore original clipboard content after pasting (preserve user's clipboard)
    #[serde(default = "default_true")]
    pub prevent_clipboard_pollution: bool,
    /// Add space after sentence-ending punctuation (.!?)
    pub add_space_after_punctuation: bool,
    /// List of terminal application classes (for Ctrl+Shift+V vs Ctrl+V)
    pub terminal_apps: Vec<String>,
}

fn default_true() -> bool {
    true
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
    /// List of prompt names that should have conversation history enabled
    /// Example: ["ask", "chat"] - only these prompts will maintain history
    /// Empty list means no prompts have history (even if conversation_history_enabled is true)
    pub conversation_history_prompts: Vec<String>,
    /// Word that, when spoken alone, clears the conversation history
    /// Matched case-insensitively, ignoring punctuation and extra spaces
    /// Example: "clear" - saying "clear" or "Clear." will reset history
    pub conversation_history_clear_word: String,
    /// How many minutes of history to include
    pub conversation_history_minutes: u32,
    /// Maximum number of turns (user+assistant pairs) to include
    pub conversation_max_turns: usize,
    /// Directory for storing history files
    pub conversation_history_dir: String,
    /// Named tool sets - each set is a list of tool names
    /// Example: {"hyprland": ["switch_workspace", "focus_window"], "smart_home": ["turn_leds_on", "turn_leds_off"]}
    #[serde(default)]
    pub tool_sets: HashMap<String, Vec<String>>,
    /// Map prompt names to tool set names
    /// Example: {"ask": "none", "clean": "smart_home", "control": "hyprland"}
    /// If a prompt is not in this map, it defaults to no tools
    #[serde(default)]
    pub prompt_tool_mapping: HashMap<String, String>,
    /// Enable file chat mode (write Q&A to markdown files instead of clipboard)
    #[serde(default)]
    pub file_chat_enabled: bool,
    /// Directory where file chat markdown files are stored
    #[serde(default)]
    pub file_chat_dir: String,
}

/// OCR configuration for screen context capture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrConfig {
    /// Language code for Tesseract (e.g., "eng", "fra", "deu")
    pub language: String,
    /// DPI for OCR processing (higher = more accurate but slower)
    pub dpi: u32,
    /// Path for temporary screenshot storage
    pub screenshot_path: String,
    /// Path for OCR result storage
    pub result_path: String,
}

/// Watch directory configuration for automatic transcription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    /// Enable directory watching
    pub enabled: bool,
    /// Directory to watch for new audio files
    pub watch_dir: String,
    /// Output directory for transcription text files (defaults to watch_dir if not set)
    #[serde(default)]
    pub output_dir: Option<String>,
    /// Supported audio file extensions (without dot)
    pub extensions: Vec<String>,
    /// Debounce duration in milliseconds (wait for file write to complete)
    pub debounce_ms: u64,
    /// Scan for existing files on startup (default: true)
    #[serde(default = "default_true")]
    pub scan_existing: bool,
}

/// Hotkey daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    /// Modifier keys for all hotkeys (e.g., ["super", "shift", "ctrl", "alt"])
    pub modifiers: Vec<String>,
    /// Threshold in milliseconds to distinguish tap from hold
    pub tap_threshold_ms: u64,
    /// List of hotkey bindings
    pub bindings: Vec<HotkeyBinding>,
}

/// Voice Activity Detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VadConfig {
    /// Enable VAD preprocessing
    pub enabled: bool,
    /// Speech probability threshold (0.0-1.0, higher = stricter)
    pub threshold: f32,
    /// Minimum audio duration in seconds to apply VAD (shorter audio skips VAD)
    pub min_duration_seconds: f32,
    /// Minimum speech duration in milliseconds (filters out very short sounds)
    pub min_speech_duration_ms: i32,
    /// Minimum silence duration in milliseconds (to split segments)
    pub min_silence_duration_ms: i32,
}

/// Transcription provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionConfig {
    /// Transcription provider: "local" (uses transcribe-daemon) or "groq" (uses Groq API)
    pub provider: String,
    /// Groq model to use (whisper-large-v3, whisper-large-v3-turbo, distil-whisper-large-v3-en)
    pub groq_model: String,
    /// Language hint for Groq (ISO-639-1 code like "en", "de"). Empty for auto-detect.
    pub groq_language: String,
}

/// A single hotkey binding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyBinding {
    /// The key for this binding (e.g., "q", "e", "r")
    pub key: String,
    /// Prompt to use for LLM processing (None = raw transcription)
    pub prompt: Option<String>,
    /// Enable OCR screen capture for context
    #[serde(default)]
    pub ocr: bool,
    /// Enable GUI conversation mode
    #[serde(default)]
    pub gui: bool,
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
            prevent_clipboard_pollution: true,
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
            corrections_file: config_dir
                .join("transcription_corrections.json")
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
            enabled: false, // Disabled by default for privacy
            basic_log_enabled: true,
            llm_log_enabled: true,
            basic_log_path: config_dir
                .join("dictation_log.csv")
                .to_string_lossy()
                .to_string(),
            llm_log_path: config_dir
                .join("llm_corrections_log.csv")
                .to_string_lossy()
                .to_string(),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        let mut tool_sets = HashMap::new();
        // Define default tool sets
        tool_sets.insert("none".to_string(), Vec::new());
        // Add more default tool sets as examples (empty for now since no tools defined)
        tool_sets.insert("all".to_string(), Vec::new());

        let mut prompt_tool_mapping = HashMap::new();
        // Default: ask prompt gets no tools (for questions/conversation)
        prompt_tool_mapping.insert("ask".to_string(), "none".to_string());
        // Default: clean prompt gets all tools (for dictation with actions)
        prompt_tool_mapping.insert("clean".to_string(), "all".to_string());

        let config_dir = dirs::config_dir()
            .map(|d| d.join("transcribe-rs"))
            .unwrap_or_else(|| PathBuf::from("/tmp/transcribe-rs"));

        LlmConfig {
            conversation_history_enabled: true,
            conversation_history_prompts: vec!["ask".to_string()], // Default to "ask" prompt only
            conversation_history_clear_word: "clear".to_string(),
            conversation_history_minutes: 5,
            conversation_max_turns: 10,
            conversation_history_dir: "/tmp".to_string(),
            tool_sets,
            prompt_tool_mapping,
            file_chat_enabled: true,
            file_chat_dir: config_dir.join("chats").to_string_lossy().to_string(),
        }
    }
}

impl Default for OcrConfig {
    fn default() -> Self {
        OcrConfig {
            language: "eng".to_string(),
            dpi: 300,
            screenshot_path: "/tmp/ptt_ocr_screenshot.png".to_string(),
            result_path: "/tmp/ptt_ocr_result.txt".to_string(),
        }
    }
}

impl Default for WatchConfig {
    fn default() -> Self {
        let config_dir = dirs::config_dir()
            .map(|d| d.join("transcribe-rs"))
            .unwrap_or_else(|| PathBuf::from("/tmp/transcribe-rs"));

        WatchConfig {
            enabled: false, // Disabled by default
            watch_dir: config_dir.join("watch").to_string_lossy().to_string(),
            output_dir: None, // Defaults to watch_dir
            extensions: vec![
                "wav".to_string(),
                "m4a".to_string(),
                "mp3".to_string(),
                "ogg".to_string(),
                "flac".to_string(),
                "webm".to_string(),
            ],
            debounce_ms: 1000,   // 1 second debounce
            scan_existing: true, // Process existing files on startup
        }
    }
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        HotkeyConfig {
            modifiers: vec![
                "super".to_string(),
                "shift".to_string(),
                "ctrl".to_string(),
                "alt".to_string(),
            ],
            tap_threshold_ms: 700,
            bindings: vec![
                HotkeyBinding {
                    key: "q".to_string(),
                    prompt: Some("clean".to_string()),
                    ocr: false,
                    gui: false,
                },
                HotkeyBinding {
                    key: "e".to_string(),
                    prompt: None, // Raw transcription
                    ocr: false,
                    gui: false,
                },
                HotkeyBinding {
                    key: "w".to_string(),
                    prompt: Some("ask".to_string()),
                    ocr: false,
                    gui: false,
                },
                HotkeyBinding {
                    key: "r".to_string(),
                    prompt: Some("ocr".to_string()),
                    ocr: true,
                    gui: false,
                },
                HotkeyBinding {
                    key: "t".to_string(),
                    prompt: None,
                    ocr: false,
                    gui: true,
                },
            ],
        }
    }
}

impl Default for VadConfig {
    fn default() -> Self {
        VadConfig {
            enabled: true,
            threshold: 0.3,  // Lower = less likely to cut off speech (was 0.5)
            min_duration_seconds: 30.0,
            min_speech_duration_ms: 250,
            min_silence_duration_ms: 300,  // Higher = requires longer silence before splitting (was 100)
        }
    }
}

impl Default for TranscriptionConfig {
    fn default() -> Self {
        TranscriptionConfig {
            provider: "local".to_string(), // Default to local daemon for backwards compatibility
            groq_model: "whisper-large-v3-turbo".to_string(), // Good balance of speed and quality
            groq_language: String::new(), // Empty = auto-detect
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
        let config_dir = dirs::config_dir().ok_or("Could not find config directory")?;
        Ok(config_dir.join("transcribe-rs").join("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watch_config_output_dir_default() {
        let config = WatchConfig::default();
        assert!(config.output_dir.is_none());
    }

    #[test]
    fn test_watch_config_output_dir_from_toml() {
        let toml_str = r#"
            enabled = true
            watch_dir = "/input/audio"
            output_dir = "/output/transcripts"
            extensions = ["mp3", "wav"]
            debounce_ms = 2000
        "#;

        let config: WatchConfig = toml::from_str(toml_str).expect("Failed to parse TOML");

        assert!(config.enabled);
        assert_eq!(config.watch_dir, "/input/audio");
        assert_eq!(config.output_dir, Some("/output/transcripts".to_string()));
        assert_eq!(config.extensions, vec!["mp3", "wav"]);
        assert_eq!(config.debounce_ms, 2000);
    }

    #[test]
    fn test_watch_config_output_dir_omitted() {
        let toml_str = r#"
            enabled = true
            watch_dir = "/input/audio"
            extensions = ["mp3"]
            debounce_ms = 1000
        "#;

        let config: WatchConfig = toml::from_str(toml_str).expect("Failed to parse TOML");

        assert!(config.enabled);
        assert_eq!(config.watch_dir, "/input/audio");
        assert!(config.output_dir.is_none());
    }

    #[test]
    fn test_watch_config_output_dir_serialization() {
        let config = WatchConfig {
            enabled: true,
            watch_dir: "/input".to_string(),
            output_dir: Some("/output".to_string()),
            extensions: vec!["mp3".to_string()],
            debounce_ms: 1000,
            scan_existing: true,
        };

        let toml_str = toml::to_string(&config).expect("Failed to serialize");
        assert!(toml_str.contains("output_dir = \"/output\""));

        // Deserialize back
        let parsed: WatchConfig = toml::from_str(&toml_str).expect("Failed to parse");
        assert_eq!(parsed.output_dir, Some("/output".to_string()));
    }
}
