//! Lightweight file watcher daemon for automatic batch transcription.
//!
//! Monitors a directory for new audio files and spawns transcribe-batch
//! processes to handle them. Runs with minimal RAM footprint.

use notify::{
    event::{AccessKind, AccessMode, CreateKind, ModifyKind},
    Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use transcribe_rs::{config::Config, logging};

/// State for tracking file modifications (debouncing)
struct FileState {
    last_modified: Instant,
    last_size: u64,
}

/// Active transcription job
struct ActiveJob {
    child: Child,
    file_path: PathBuf,
    started: Instant,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    if let Err(e) = logging::init() {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    }

    eprintln!("watch-daemon: Starting...");

    // Load configuration
    let config = Config::load()?;

    if !config.watch.enabled {
        eprintln!("Warning: File watching is disabled in config (watch.enabled = false)");
        eprintln!("Proceeding anyway for testing...");
    }

    let watch_dir = PathBuf::from(&config.watch.watch_dir);
    let output_dir = config
        .watch
        .output_dir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| watch_dir.clone());
    let extensions: Vec<String> = config.watch.extensions.iter().map(|e| e.to_lowercase()).collect();
    let debounce_duration = Duration::from_millis(config.watch.debounce_ms);
    let scan_existing = config.watch.scan_existing;

    eprintln!("  Watch directory: {}", watch_dir.display());
    eprintln!("  Output directory: {}", output_dir.display());
    eprintln!("  Extensions: {:?}", extensions);
    eprintln!("  Debounce: {}ms", config.watch.debounce_ms);

    // Create directories
    fs::create_dir_all(&watch_dir)?;
    fs::create_dir_all(&output_dir)?;
    fs::create_dir_all(watch_dir.join("processed"))?;

    // Find the transcribe-batch binary
    let batch_binary = find_batch_binary()?;
    eprintln!("  Batch binary: {}", batch_binary.display());

    // Setup signal handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        eprintln!("\nwatch-daemon: Shutting down...");
        r.store(false, Ordering::SeqCst);
    })?;

    // Create notify watcher
    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        NotifyConfig::default(),
    )?;

    watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;
    eprintln!("  Watching for new files...");

    // State tracking
    let mut pending_files: HashMap<PathBuf, FileState> = HashMap::new();
    let mut processing: HashSet<PathBuf> = HashSet::new();
    let mut active_jobs: Vec<ActiveJob> = Vec::new();

    // Scan existing files on startup
    if scan_existing {
        scan_existing_files(&watch_dir, &extensions, &mut pending_files)?;
    }

    eprintln!("watch-daemon: Ready!");

    // Main event loop
    while running.load(Ordering::SeqCst) {
        // Check for new filesystem events
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                handle_fs_event(event, &extensions, &processing, &mut pending_files);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        // Check for files ready to process (debounced)
        let now = Instant::now();
        let ready_files: Vec<PathBuf> = pending_files
            .iter()
            .filter_map(|(path, state)| {
                if now.duration_since(state.last_modified) >= debounce_duration {
                    // Verify size is stable
                    let current_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                    if current_size == state.last_size && current_size > 0 {
                        return Some(path.clone());
                    }
                }
                None
            })
            .collect();

        // Spawn transcription jobs for ready files
        for path in ready_files {
            pending_files.remove(&path);

            if processing.contains(&path) {
                continue;
            }

            eprintln!("watch-daemon: Spawning job for {}", path.display());
            processing.insert(path.clone());

            match spawn_batch_job(&batch_binary, &path, &output_dir) {
                Ok(child) => {
                    active_jobs.push(ActiveJob {
                        child,
                        file_path: path,
                        started: Instant::now(),
                    });
                }
                Err(e) => {
                    eprintln!("watch-daemon: Failed to spawn job: {}", e);
                    processing.remove(&path);
                }
            }
        }

        // Check for completed jobs
        active_jobs.retain_mut(|job| {
            match job.child.try_wait() {
                Ok(Some(status)) => {
                    let elapsed = job.started.elapsed().as_secs_f32();
                    if status.success() {
                        eprintln!(
                            "watch-daemon: Job completed ({:.1}s): {}",
                            elapsed,
                            job.file_path.display()
                        );
                    } else {
                        eprintln!(
                            "watch-daemon: Job failed (exit {}): {}",
                            status.code().unwrap_or(-1),
                            job.file_path.display()
                        );
                    }
                    processing.remove(&job.file_path);
                    false // Remove from active jobs
                }
                Ok(None) => true, // Still running
                Err(e) => {
                    eprintln!("watch-daemon: Error checking job status: {}", e);
                    processing.remove(&job.file_path);
                    false
                }
            }
        });
    }

    // Wait for active jobs to finish (with timeout)
    if !active_jobs.is_empty() {
        eprintln!("watch-daemon: Waiting for {} active jobs...", active_jobs.len());
        for mut job in active_jobs {
            let _ = job.child.wait();
        }
    }

    eprintln!("watch-daemon: Stopped");
    Ok(())
}

/// Find the transcribe-batch binary
fn find_batch_binary() -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Check in same directory as this binary
    let current_exe = env::current_exe()?;
    let exe_dir = current_exe.parent().ok_or("No parent directory")?;

    let batch_path = exe_dir.join("transcribe-batch");
    if batch_path.exists() {
        return Ok(batch_path);
    }

    // Check in builds/current
    let builds_path = PathBuf::from("builds/current/transcribe-batch");
    if builds_path.exists() {
        return Ok(builds_path.canonicalize()?);
    }

    // Check in target/release
    let target_path = PathBuf::from("target/release/transcribe-batch");
    if target_path.exists() {
        return Ok(target_path.canonicalize()?);
    }

    Err("transcribe-batch binary not found".into())
}

/// Scan existing files in the watch directory
fn scan_existing_files(
    watch_dir: &Path,
    extensions: &[String],
    pending_files: &mut HashMap<PathBuf, FileState>,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("watch-daemon: Scanning existing files...");

    let mut count = 0;
    for entry in fs::read_dir(watch_dir)?.flatten() {
        let path = entry.path();
        if path.is_file() && is_audio_file(&path, extensions) && !is_in_processed_dir(&path) {
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if size > 0 {
                eprintln!("  Found: {}", path.display());
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

    eprintln!("watch-daemon: Found {} existing files", count);
    Ok(())
}

/// Handle filesystem events
fn handle_fs_event(
    event: Event,
    extensions: &[String],
    processing: &HashSet<PathBuf>,
    pending_files: &mut HashMap<PathBuf, FileState>,
) {
    match event.kind {
        EventKind::Create(CreateKind::File)
        | EventKind::Modify(ModifyKind::Data(_))
        | EventKind::Modify(ModifyKind::Any)
        | EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
            for path in event.paths {
                if is_audio_file(&path, extensions)
                    && !is_in_processed_dir(&path)
                    && !processing.contains(&path)
                {
                    let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    if size > 0 {
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
        }
        _ => {}
    }
}

/// Spawn a transcribe-batch subprocess
fn spawn_batch_job(
    batch_binary: &Path,
    input_file: &Path,
    output_dir: &Path,
) -> Result<Child, Box<dyn std::error::Error>> {
    let child = Command::new("nice")
        .args(["-n", "19"])
        .arg(batch_binary)
        .arg(input_file)
        .arg(output_dir)
        .spawn()?;

    Ok(child)
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
