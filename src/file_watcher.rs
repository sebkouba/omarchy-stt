//! File watcher for automatic transcription of audio files dropped into a directory

use crate::config::WatchConfig;
use log::{debug, error, info, warn};
use notify::{
    event::{AccessKind, AccessMode, CreateKind, ModifyKind},
    Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

/// Events emitted by the file watcher
#[derive(Debug)]
pub enum WatchEvent {
    /// A new audio file is ready to be transcribed (already converted to WAV if needed)
    FileReady {
        /// Path to the WAV file (either original or converted)
        wav_path: PathBuf,
        /// Original file path (for moving to processed directory)
        original_path: PathBuf,
    },
    /// An error occurred
    Error(String),
}

/// File watcher that monitors a directory for new audio files
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

/// State for tracking file modifications (debouncing)
struct FileState {
    last_modified: Instant,
    last_size: u64,
}

impl FileWatcher {
    /// Create a new file watcher
    pub fn new(
        config: WatchConfig,
    ) -> Result<(Self, Receiver<WatchEvent>), Box<dyn std::error::Error>> {
        let (event_tx, event_rx) = mpsc::channel();
        let (internal_tx, internal_rx) = mpsc::channel();

        // Create the watch directory if it doesn't exist
        let watch_dir = PathBuf::from(&config.watch_dir);
        fs::create_dir_all(&watch_dir)?;
        info!("[file_watcher] Watch directory: {}", watch_dir.display());

        // Create processed subdirectory
        let processed_dir = watch_dir.join("processed");
        fs::create_dir_all(&processed_dir)?;

        // Create the notify watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                match &res {
                    Ok(event) => {
                        debug!("[file_watcher] Raw event: {:?}", event);
                    }
                    Err(e) => {
                        error!("[file_watcher] Watcher error: {}", e);
                    }
                }
                if let Ok(event) = res {
                    let _ = internal_tx.send(event);
                }
            },
            NotifyConfig::default(),
        )?;

        // Start watching the directory
        watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;
        info!("[file_watcher] Started watching directory");

        // Spawn the debouncing/processing thread
        std::thread::spawn(move || {
            process_events(internal_rx, event_tx, config);
        });

        let file_watcher = FileWatcher { _watcher: watcher };

        Ok((file_watcher, event_rx))
    }
}

/// Process file system events with debouncing
fn process_events(rx: Receiver<Event>, tx: Sender<WatchEvent>, config: WatchConfig) {
    let mut pending_files: HashMap<PathBuf, FileState> = HashMap::new();
    let debounce_duration = Duration::from_millis(config.debounce_ms);
    let extensions: Vec<String> = config.extensions.iter().map(|e| e.to_lowercase()).collect();
    let watch_dir = PathBuf::from(&config.watch_dir);

    eprintln!(
        "[file_watcher] Event processor started, debounce: {}ms, extensions: {:?}",
        config.debounce_ms, extensions
    );

    // Scan for existing files on startup if enabled
    if config.scan_existing {
        eprintln!("[file_watcher] Scanning for existing files in {}...", watch_dir.display());
        match fs::read_dir(&watch_dir) {
            Ok(entries) => {
                let mut count = 0;
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && is_audio_file(&path, &extensions) && !is_in_processed_dir(&path) {
                        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                        if size > 0 {
                            eprintln!("[file_watcher] Found existing file: {}", path.display());
                            pending_files.insert(
                                path,
                                FileState {
                                    last_modified: Instant::now(),
                                    last_size: size,
                                },
                            );
                            count += 1;
                        }
                    }
                }
                eprintln!("[file_watcher] Found {} existing audio files to process", count);
            }
            Err(e) => {
                eprintln!("[file_watcher] Failed to scan directory: {}", e);
            }
        }
    }

    loop {
        // Check for new events with a timeout
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                debug!("[file_watcher] Received event: {:?}", event.kind);

                // Process create, modify, and close-write events
                match event.kind {
                    EventKind::Create(CreateKind::File)
                    | EventKind::Modify(ModifyKind::Data(_))
                    | EventKind::Modify(ModifyKind::Metadata(_))
                    | EventKind::Modify(ModifyKind::Any)
                    | EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                        for path in event.paths {
                            debug!("[file_watcher] Checking path: {}", path.display());

                            let is_audio = is_audio_file(&path, &extensions);
                            let in_processed = is_in_processed_dir(&path);

                            debug!(
                                "[file_watcher] is_audio={}, in_processed={}",
                                is_audio, in_processed
                            );

                            if is_audio && !in_processed {
                                let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                                debug!(
                                    "[file_watcher] Adding to pending: {} (size: {} bytes)",
                                    path.display(),
                                    size
                                );
                                pending_files.insert(
                                    path,
                                    FileState {
                                        last_modified: Instant::now(),
                                        last_size: size,
                                    },
                                );
                            }
                        }
                    }
                    _ => {
                        debug!("[file_watcher] Ignoring event type: {:?}", event.kind);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Check for files ready to process
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                warn!("[file_watcher] Channel disconnected, stopping");
                break;
            }
        }

        // Check which files are ready (haven't been modified for debounce_duration)
        let now = Instant::now();
        let ready_files: Vec<PathBuf> = pending_files
            .iter()
            .filter_map(|(path, state)| {
                let elapsed = now.duration_since(state.last_modified);
                if elapsed >= debounce_duration {
                    // Double-check the file size hasn't changed
                    let current_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                    if current_size == state.last_size && current_size > 0 {
                        debug!("[file_watcher] File ready (debounced): {}", path.display());
                        Some(path.clone())
                    } else {
                        debug!(
                            "[file_watcher] File size changed, waiting: {} (was {}, now {})",
                            path.display(),
                            state.last_size,
                            current_size
                        );
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Process ready files
        for path in ready_files {
            pending_files.remove(&path);
            info!("[file_watcher] Processing file: {}", path.display());

            // Convert to WAV if needed and send event
            match prepare_audio_file(&path, &config) {
                Ok(wav_path) => {
                    info!(
                        "[file_watcher] File prepared, WAV path: {}",
                        wav_path.display()
                    );
                    let _ = tx.send(WatchEvent::FileReady {
                        wav_path,
                        original_path: path,
                    });
                }
                Err(e) => {
                    error!("[file_watcher] Failed to prepare {}: {}", path.display(), e);
                    let _ = tx.send(WatchEvent::Error(format!(
                        "Failed to prepare {}: {}",
                        path.display(),
                        e
                    )));
                }
            }
        }
    }
}

/// Check if a file has a supported audio extension
fn is_audio_file(path: &Path, extensions: &[String]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| extensions.contains(&e.to_lowercase()))
        .unwrap_or(false)
}

/// Check if a file is in the processed subdirectory
fn is_in_processed_dir(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "processed")
}

/// Maximum chunk duration in seconds (5 minutes to stay safely under the ~6 min ORT limit)
const MAX_CHUNK_SECONDS: u32 = 300;

/// Convert audio file to WAV format if needed, returns path to WAV file
fn prepare_audio_file(
    path: &Path,
    _config: &WatchConfig,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // If already WAV, just return the path
    if extension == "wav" {
        info!("[file_watcher] File is already WAV, no conversion needed");
        return Ok(path.to_path_buf());
    }

    // Convert to WAV using FFmpeg - write to /tmp to avoid triggering more watch events
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let wav_path = PathBuf::from("/tmp").join(format!("transcribe_watch_{}.wav", stem));

    info!(
        "[file_watcher] Converting {} to {}",
        path.display(),
        wav_path.display()
    );

    let output = Command::new("ffmpeg")
        .args([
            "-y", // Overwrite output
            "-i",
            path.to_str().ok_or("Invalid path")?,
            "-ar",
            "16000", // 16kHz sample rate
            "-ac",
            "1", // Mono
            "-f",
            "wav", // WAV format
            wav_path.to_str().ok_or("Invalid output path")?,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("[file_watcher] FFmpeg failed: {}", stderr);
        return Err(format!("FFmpeg conversion failed: {}", stderr).into());
    }

    debug!("[file_watcher] Conversion successful");
    Ok(wav_path)
}

/// Get the duration of a WAV file in seconds
pub fn get_wav_duration(path: &Path) -> Result<f64, Box<dyn std::error::Error>> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            path.to_str().ok_or("Invalid path")?,
        ])
        .output()?;

    if !output.status.success() {
        return Err("ffprobe failed".into());
    }

    let duration_str = String::from_utf8_lossy(&output.stdout);
    let duration: f64 = duration_str.trim().parse()?;
    Ok(duration)
}

/// Split a WAV file into chunks if it exceeds MAX_CHUNK_SECONDS
/// Returns a list of chunk paths (or just the original path if no splitting needed)
pub fn split_audio_if_needed(wav_path: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let duration = get_wav_duration(wav_path)?;

    if duration <= MAX_CHUNK_SECONDS as f64 {
        // No splitting needed
        return Ok(vec![wav_path.to_path_buf()]);
    }

    info!(
        "[file_watcher] Audio is {:.1}s, splitting into {}s chunks",
        duration, MAX_CHUNK_SECONDS
    );

    let stem = wav_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audio");
    let mut chunks = Vec::new();
    let mut start_time = 0u32;
    let mut chunk_num = 0;

    while (start_time as f64) < duration {
        let chunk_path =
            PathBuf::from("/tmp").join(format!("transcribe_chunk_{}_{}.wav", stem, chunk_num));

        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                wav_path.to_str().ok_or("Invalid path")?,
                "-ss",
                &start_time.to_string(),
                "-t",
                &MAX_CHUNK_SECONDS.to_string(),
                "-c",
                "copy",
                chunk_path.to_str().ok_or("Invalid chunk path")?,
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("[file_watcher] FFmpeg chunk split failed: {}", stderr);
            return Err(format!("FFmpeg chunk split failed: {}", stderr).into());
        }

        chunks.push(chunk_path);
        start_time += MAX_CHUNK_SECONDS;
        chunk_num += 1;
    }

    info!("[file_watcher] Split into {} chunks", chunks.len());
    Ok(chunks)
}

/// Clean up temporary chunk files
pub fn cleanup_chunks(chunks: &[PathBuf], original_wav: &Path) {
    for chunk in chunks {
        // Don't delete if it's the original file
        if chunk != original_wav {
            if let Err(e) = fs::remove_file(chunk) {
                debug!(
                    "[file_watcher] Failed to cleanup chunk {}: {}",
                    chunk.display(),
                    e
                );
            }
        }
    }
}

/// Move original file and any converted WAV to processed directory
pub fn move_to_processed(
    original_path: &Path,
    wav_path: &Path,
    watch_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let processed_dir = watch_dir.join("processed");
    fs::create_dir_all(&processed_dir)?;

    // Move original file
    if original_path.exists() {
        let dest = processed_dir.join(original_path.file_name().unwrap_or_default());
        info!(
            "[file_watcher] Moving {} to {}",
            original_path.display(),
            dest.display()
        );
        fs::rename(original_path, dest)?;
    }

    // If WAV is different from original (was converted), delete the temp WAV
    if wav_path != original_path && wav_path.exists() {
        info!("[file_watcher] Removing temp WAV: {}", wav_path.display());
        fs::remove_file(wav_path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_audio_file() {
        let extensions = vec!["wav".to_string(), "m4a".to_string(), "mp3".to_string()];

        assert!(is_audio_file(Path::new("test.wav"), &extensions));
        assert!(is_audio_file(Path::new("test.WAV"), &extensions));
        assert!(is_audio_file(Path::new("test.m4a"), &extensions));
        assert!(is_audio_file(Path::new("test.mp3"), &extensions));
        assert!(!is_audio_file(Path::new("test.txt"), &extensions));
        assert!(!is_audio_file(Path::new("test"), &extensions));
    }

    #[test]
    fn test_is_in_processed_dir() {
        assert!(is_in_processed_dir(Path::new("/foo/processed/bar.wav")));
        assert!(is_in_processed_dir(Path::new("watch/processed/test.m4a")));
        assert!(!is_in_processed_dir(Path::new("/foo/watch/bar.wav")));
        assert!(!is_in_processed_dir(Path::new("test.wav")));
    }
}
