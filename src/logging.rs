//! Logging initialization module with config file support.
//!
//! This module provides configurable logging using log4rs with per-module log level control.
//! Configuration is read from `~/.config/transcribe-rs/logging.yaml`.

use log::LevelFilter;
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Config, Logger, Root},
    encode::pattern::PatternEncoder,
};
use std::path::PathBuf;

/// Default log file path
const DEFAULT_LOG_FILE: &str = "/tmp/ppt_rust_debug.log";

/// Default logging config filename
const LOGGING_CONFIG_FILE: &str = "logging.yaml";

/// Log pattern format: [timestamp] [level] [module] message
const LOG_PATTERN: &str = "[{d(%Y-%m-%d %H:%M:%S%.3f)}] [{l}] [{M}] {m}{n}";

/// Initialize logging from config file or use defaults.
///
/// Attempts to load configuration from `~/.config/transcribe-rs/logging.yaml`.
/// If the config file doesn't exist or fails to load, uses sensible defaults.
///
/// # Returns
///
/// Returns `Ok(())` if logging was initialized successfully.
pub fn init() -> Result<(), Box<dyn std::error::Error>> {
    // Try to load from config file first
    if let Some(config_path) = get_config_path() {
        if config_path.exists() {
            match log4rs::init_file(&config_path, Default::default()) {
                Ok(()) => {
                    log::info!("Logging initialized from config: {:?}", config_path);
                    return Ok(());
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to load logging config from {:?}: {}. Using defaults.",
                        config_path, e
                    );
                }
            }
        }
    }

    // Fall back to programmatic defaults
    init_default()
}

/// Initialize logging with default configuration.
///
/// Creates a file appender writing to `/tmp/ppt_rust_debug.log` with INFO level root
/// and DEBUG level for transcribe_rs modules.
pub fn init_default() -> Result<(), Box<dyn std::error::Error>> {
    let file_appender = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(LOG_PATTERN)))
        .build(DEFAULT_LOG_FILE)?;

    let config = Config::builder()
        .appender(Appender::builder().build("file", Box::new(file_appender)))
        // Set module-specific log levels
        .logger(Logger::builder().build("transcribe_rs::groq", LevelFilter::Debug))
        .logger(Logger::builder().build("transcribe_rs::recording", LevelFilter::Debug))
        .logger(Logger::builder().build("transcribe_rs::clipboard", LevelFilter::Debug))
        .logger(Logger::builder().build("transcribe_rs::paste", LevelFilter::Debug))
        .logger(Logger::builder().build("transcribe_rs::tools", LevelFilter::Debug))
        .logger(Logger::builder().build("transcribe_rs::conversation_history", LevelFilter::Debug))
        .logger(Logger::builder().build("transcribe_rs::engines::parakeet", LevelFilter::Info))
        .logger(Logger::builder().build("recording_daemon", LevelFilter::Debug))
        // Root logger at INFO level
        .build(Root::builder().appender("file").build(LevelFilter::Info))?;

    log4rs::init_config(config)?;
    log::info!("Logging initialized with default configuration");
    Ok(())
}

/// Get the path to the logging configuration file.
///
/// Returns `~/.config/transcribe-rs/logging.yaml` if the config directory can be determined.
pub fn get_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("transcribe-rs").join(LOGGING_CONFIG_FILE))
}

/// Get the default logging configuration as a YAML string.
///
/// This can be used to create an initial config file for users to customize.
pub fn default_config_yaml() -> &'static str {
    r#"# Transcribe-RS Logging Configuration
#
# Log levels (from most to least verbose): trace, debug, info, warn, error, off
#
# Customize per-module log levels in the 'loggers' section below.
# Module names follow the Rust module path (e.g., transcribe_rs::groq)

appenders:
  file:
    kind: file
    path: "/tmp/ppt_rust_debug.log"
    encoder:
      pattern: "[{d(%Y-%m-%d %H:%M:%S%.3f)}] [{l}] [{M}] {m}{n}"

# Root logger - default level for all modules not explicitly configured
root:
  level: info
  appenders:
    - file

# Per-module log level configuration
loggers:
  # CLI and daemon modules
  transcribe_rs::groq:
    level: debug
  transcribe_rs::recording:
    level: debug
  transcribe_rs::clipboard:
    level: debug
  transcribe_rs::paste:
    level: debug
  transcribe_rs::tools:
    level: debug
  transcribe_rs::conversation_history:
    level: debug

  # Engine modules (can be noisy at trace level)
  transcribe_rs::engines::parakeet:
    level: info
  transcribe_rs::engines::parakeet::model:
    level: info

  # Recording daemon (separate binary)
  recording_daemon:
    level: debug

  # CLI binary
  cli:
    level: debug

  # Transcription daemon
  transcribe_daemon:
    level: info
"#
}

/// Create default logging config file if it doesn't exist.
///
/// Writes the default YAML config to `~/.config/transcribe-rs/logging.yaml`.
///
/// # Returns
///
/// Returns `Ok(true)` if file was created, `Ok(false)` if it already exists.
pub fn create_default_config() -> Result<bool, Box<dyn std::error::Error>> {
    let config_path = get_config_path().ok_or("Could not determine config directory")?;

    if config_path.exists() {
        return Ok(false);
    }

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&config_path, default_config_yaml())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_yaml_is_valid() {
        // Just verify the YAML string is not empty
        assert!(!default_config_yaml().is_empty());
        assert!(default_config_yaml().contains("appenders:"));
        assert!(default_config_yaml().contains("loggers:"));
    }

    #[test]
    fn test_get_config_path() {
        let path = get_config_path();
        assert!(path.is_some());
        let path = path.unwrap();
        assert!(path.ends_with("logging.yaml"));
    }
}
