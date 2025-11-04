use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use serde::{Deserialize, Serialize};
use transcribe_rs::config::Config;

#[derive(Debug, Serialize)]
struct TranscribeRequest {
    file: String,
}

#[derive(Debug, Deserialize)]
struct TranscribeResponse {
    success: bool,
    text: Option<String>,
    error: Option<String>,
}

// New protocol structs for Harper integration

#[derive(Debug, Serialize, Clone)]
struct HarperRequestConfig {
    enabled: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    user_dict_path: String,
    dialect: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    disabled_linters: Vec<String>,
    save_corrections: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    corrections_dir: String,
}

impl Default for HarperRequestConfig {
    fn default() -> Self {
        HarperRequestConfig {
            enabled: false,
            user_dict_path: String::new(),
            dialect: "American".to_string(),
            disabled_linters: vec![],
            save_corrections: false,
            corrections_dir: String::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ProcessingMetrics {
    transcription_ms: u128,
    harper_ms: u128,
    corrections_applied: usize,
}

#[derive(Debug, Serialize)]
struct ProcessRequest {
    file: String,
    #[serde(skip_serializing_if = "is_harper_disabled")]
    harper: HarperRequestConfig,
}

#[derive(Debug, Deserialize)]
struct ProcessResponse {
    success: bool,
    text: Option<String>,
    error: Option<String>,
    processing: Option<ProcessingMetrics>,
}

// Helper function for serde skip_serializing_if
fn is_harper_disabled(harper: &HarperRequestConfig) -> bool {
    !harper.enabled
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = Config::load()?;
    let socket_path = &config.daemon.socket_path;

    // Get file path from command line argument
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <audio_file.wav>", args[0]);
        std::process::exit(1);
    }

    let audio_file = &args[1];

    // Connect to daemon
    let stream = UnixStream::connect(socket_path).map_err(|e| {
        eprintln!("❌ Failed to connect to transcribe daemon at {}", socket_path);
        eprintln!("   Is the daemon running? Start it with: transcribe-daemon");
        e
    })?;

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    // Build full processing request with Harper config
    let request = ProcessRequest {
        file: audio_file.to_string(),
        harper: HarperRequestConfig {
            enabled: config.harper.enabled,
            user_dict_path: config.harper.dictionary_path,
            dialect: config.harper.dialect,
            disabled_linters: config.harper.disabled_linters,
            save_corrections: true,
            corrections_dir: config.harper.corrections_dir,
        },
    };
    let request_json = serde_json::to_string(&request)?;
    writeln!(writer, "{}", request_json)?;

    // Read response
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    let response: ProcessResponse = serde_json::from_str(&response_line)?;

    // Handle response
    if response.success {
        if let Some(text) = response.text {
            // Output only the transcription text (for compatibility with existing scripts)
            println!("{}", text);
        }
        Ok(())
    } else {
        if let Some(error) = response.error {
            eprintln!("❌ Error: {}", error);
        }
        std::process::exit(1);
    }
}
