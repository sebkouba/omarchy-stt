use std::path::PathBuf;
use transcribe_rs::{
    engines::parakeet::{ParakeetEngine, ParakeetModelParams},
    TranscriptionEngine,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("🎤 Testing with sample audio");
    println!("============================\n");

    let mut engine = ParakeetEngine::new();

    let model_path = PathBuf::from("models/parakeet-tdt-0.6b-v3-int8");
    println!("Loading model...");
    engine.load_model_with_params(&model_path, ParakeetModelParams::int8())?;
    println!("✓ Model loaded!\n");

    // Test with dots sample
    let audio_path = PathBuf::from("samples/dots.wav");
    println!("Transcribing: {:?}\n", audio_path);

    let result = engine.transcribe_file(&audio_path, None)?;

    println!("📝 Result:");
    println!("------------------------");
    println!("{}", result.text);
    println!("------------------------\n");

    engine.unload_model();

    Ok(())
}
