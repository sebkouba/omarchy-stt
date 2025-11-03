use std::env;
use std::path::PathBuf;
use transcribe_rs::{
    engines::parakeet::{ParakeetEngine, ParakeetModelParams},
    TranscriptionEngine,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get file path from command line argument
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <audio_file.wav>", args[0]);
        std::process::exit(1);
    }

    let audio_path = PathBuf::from(&args[1]);

    // Check if file exists
    if !audio_path.exists() {
        eprintln!("Error: File not found: {:?}", audio_path);
        std::process::exit(1);
    }

    // Load model
    let mut engine = ParakeetEngine::new();
    let model_path = PathBuf::from("models/parakeet-tdt-0.6b-v3-int8");
    engine.load_model_with_params(&model_path, ParakeetModelParams::int8())?;

    // Transcribe
    let result = engine.transcribe_file(&audio_path, None)?;

    // Output only the transcription text (no formatting)
    println!("{}", result.text);

    engine.unload_model();
    Ok(())
}
