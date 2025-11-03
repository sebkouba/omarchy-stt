use std::path::PathBuf;
use transcribe_rs::{
    engines::parakeet::{ParakeetEngine, ParakeetModelParams},
    TranscriptionEngine,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger (shows progress messages)
    env_logger::init();

    println!("🎤 Simple Voice Transcription");
    println!("=============================\n");

    // Create the engine
    let mut engine = ParakeetEngine::new();

    // Load the Parakeet model (Int8 quantized for speed)
    let model_path = PathBuf::from("models/parakeet-tdt-0.6b-v3-int8");
    println!("Loading model from: {:?}", model_path);
    engine.load_model_with_params(&model_path, ParakeetModelParams::int8())?;
    println!("✓ Model loaded!\n");

    // Transcribe the recording
    let audio_path = PathBuf::from("recording.wav");
    println!("Transcribing: {:?}", audio_path);

    let result = engine.transcribe_file(&audio_path, None)?;

    println!("\n📝 Transcription Result:");
    println!("------------------------");
    println!("{}", result.text);
    println!("------------------------\n");

    // Clean up
    engine.unload_model();

    Ok(())
}
