//! On-demand batch transcription binary with VAD support.
//!
//! Loads the Parakeet model, applies VAD to filter silence, transcribes,
//! writes output, and exits. Designed to be spawned by watch-daemon.
//!
//! Usage: transcribe-batch <input-file> <output-dir>

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use transcribe_rs::{
    audio,
    config::Config,
    engines::parakeet::{ParakeetEngine, ParakeetModelParams},
    file_watcher, logging,
    vad::VadManager,
    TranscriptionEngine,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    if let Err(e) = logging::init() {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    }

    let start_time = Instant::now();

    // Parse arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <input-file> <output-dir>", args[0]);
        std::process::exit(1);
    }

    let input_path = PathBuf::from(&args[1]);
    let output_dir = PathBuf::from(&args[2]);

    eprintln!("transcribe-batch: Processing {}", input_path.display());

    // Validate input exists
    if !input_path.exists() {
        eprintln!("Error: Input file not found: {}", input_path.display());
        std::process::exit(1);
    }

    // Load config for model path and VAD settings
    let config = Config::load()?;
    let model_path = PathBuf::from(&config.model.path);

    // Create output directory if needed
    fs::create_dir_all(&output_dir)?;

    // Step 1: Convert to WAV if needed
    eprintln!("  Converting to WAV...");
    let wav_path = convert_to_wav(&input_path)?;
    let is_temp_wav = wav_path != input_path;

    // Step 2: Get duration and read samples
    let duration = file_watcher::get_wav_duration(&wav_path)?;
    eprintln!("  Audio duration: {:.1}s ({:.1} min)", duration, duration / 60.0);

    let samples = audio::read_wav_samples(&wav_path)?;

    // Step 3: Apply VAD to filter silence
    let vad = VadManager::new(config.vad.clone());
    let (filtered_samples, vad_result) = vad.process(&samples)?;

    let filtered_duration = filtered_samples.len() as f32 / 16000.0;
    if vad_result.vad_applied {
        eprintln!(
            "  VAD: {:.1}s speech from {:.1}s audio ({:.0}% kept)",
            vad_result.speech_duration_seconds,
            vad_result.original_duration_seconds,
            (vad_result.speech_duration_seconds / vad_result.original_duration_seconds) * 100.0
        );
    }

    // Step 4: Write filtered audio to temp file
    let filtered_wav_path = write_temp_wav(&filtered_samples, 16000)?;

    // Step 5: Split into chunks if needed (>5 min after VAD)
    let chunks = if filtered_duration > 300.0 {
        file_watcher::split_audio_if_needed(&filtered_wav_path)?
    } else {
        vec![filtered_wav_path.clone()]
    };

    if chunks.len() > 1 {
        eprintln!("  Split into {} chunks", chunks.len());
    }

    // Step 6: Load model
    eprintln!("  Loading Parakeet model...");
    let model_load_start = Instant::now();
    let mut engine = ParakeetEngine::new();
    let model_params = match config.model.quantization.as_str() {
        "int8" => ParakeetModelParams::int8(),
        "fp32" => ParakeetModelParams::fp32(),
        _ => ParakeetModelParams::int8(),
    };
    engine.load_model_with_params(&model_path, model_params)?;
    eprintln!("  Model loaded in {:.1}s", model_load_start.elapsed().as_secs_f32());

    // Step 7: Transcribe all chunks
    let mut full_text = String::new();
    for (i, chunk_path) in chunks.iter().enumerate() {
        if chunks.len() > 1 {
            eprintln!("  Transcribing chunk {}/{}...", i + 1, chunks.len());
        } else {
            eprintln!("  Transcribing...");
        }

        let chunk_start = Instant::now();
        match engine.transcribe_file(chunk_path, None) {
            Ok(result) => {
                if !full_text.is_empty() && !result.text.is_empty() {
                    full_text.push(' ');
                }
                full_text.push_str(&result.text);
                eprintln!("    Chunk transcribed in {:.1}s", chunk_start.elapsed().as_secs_f32());
            }
            Err(e) => {
                eprintln!("Error transcribing chunk {}: {}", i + 1, e);
                // Cleanup and exit
                cleanup(&chunks, &filtered_wav_path, is_temp_wav, &wav_path);
                engine.unload_model();
                std::process::exit(1);
            }
        }
    }

    // Step 8: Write output text file
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("transcription");
    let output_path = output_dir.join(format!("{}.txt", stem));

    fs::write(&output_path, &full_text)?;
    eprintln!("  Output written: {}", output_path.display());

    // Step 9: Move original to processed/
    let watch_dir = input_path.parent().unwrap_or(Path::new("."));
    if let Err(e) = file_watcher::move_to_processed(&input_path, &wav_path, watch_dir) {
        eprintln!("Warning: Failed to move to processed: {}", e);
    }

    // Step 10: Cleanup
    cleanup(&chunks, &filtered_wav_path, is_temp_wav, &wav_path);
    engine.unload_model();

    let total_time = start_time.elapsed().as_secs_f32();
    let speed = duration as f32 / total_time;
    eprintln!(
        "  Done! {:.1}s processed in {:.1}s ({:.1}x realtime)",
        duration, total_time, speed
    );

    Ok(())
}

/// Convert audio file to WAV format if needed
fn convert_to_wav(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if extension == "wav" {
        return Ok(path.to_path_buf());
    }

    // Use FFmpeg to convert
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let wav_path = PathBuf::from("/tmp").join(format!("batch_convert_{}.wav", stem));

    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            path.to_str().ok_or("Invalid path")?,
            "-ar",
            "16000",
            "-ac",
            "1",
            "-f",
            "wav",
            wav_path.to_str().ok_or("Invalid output path")?,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg conversion failed: {}", stderr).into());
    }

    Ok(wav_path)
}

/// Write f32 samples to a temporary WAV file
fn write_temp_wav(samples: &[f32], sample_rate: u32) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = PathBuf::from("/tmp").join(format!(
        "batch_vad_filtered_{}.wav",
        std::process::id()
    ));

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(&path, spec)?;
    for &sample in samples {
        let sample_i16 = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
        writer.write_sample(sample_i16)?;
    }
    writer.finalize()?;

    Ok(path)
}

/// Cleanup temporary files
fn cleanup(chunks: &[PathBuf], filtered_wav: &Path, is_temp_wav: bool, original_wav: &Path) {
    // Cleanup chunks (if different from filtered WAV)
    file_watcher::cleanup_chunks(chunks, filtered_wav);

    // Cleanup filtered VAD output
    if filtered_wav.exists() && filtered_wav != original_wav {
        let _ = fs::remove_file(filtered_wav);
    }

    // Cleanup converted WAV (if we converted from MP3/M4A)
    if is_temp_wav && original_wav.exists() {
        let _ = fs::remove_file(original_wav);
    }
}
