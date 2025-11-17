use std::path::PathBuf;
use std::time::{Duration, Instant};
use transcribe_rs::{clipboard, config::Config, transcription_corrections::TranscriptionCorrector};

/// Performance test that simulates the full transcription pipeline
/// Tests the path from audio file to clipboard (excluding the actual paste operation)
#[test]
fn test_transcription_pipeline_performance() {
    let test_files = vec!["tests/test1.wav", "tests/test2.wav", "tests/test3.wav"];

    println!("\n=== TRANSCRIPTION PIPELINE PERFORMANCE TEST ===\n");

    for test_file in test_files {
        println!("Testing: {}", test_file);
        let result = measure_pipeline_performance(test_file);

        match result {
            Ok(metrics) => {
                println!("  Total time: {:.0}ms", metrics.total_ms);
                println!("    - Transcription: {:.0}ms", metrics.transcription_ms);
                println!("    - Corrections:   {:.0}ms", metrics.corrections_ms);
                println!("    - Clipboard:     {:.0}ms", metrics.clipboard_ms);
                println!("  Text length: {} chars", metrics.text_length);
                println!("  Final text: {}\n", metrics.final_text);
            }
            Err(e) => {
                println!("  ERROR: {}\n", e);
                panic!("Pipeline test failed for {}: {}", test_file, e);
            }
        }
    }
}

#[derive(Debug)]
struct PipelineMetrics {
    total_ms: f64,
    transcription_ms: f64,
    corrections_ms: f64,
    clipboard_ms: f64,
    text_length: usize,
    final_text: String,
}

fn measure_pipeline_performance(
    audio_file: &str,
) -> Result<PipelineMetrics, Box<dyn std::error::Error>> {
    let start_total = Instant::now();

    // Load config
    let config = Config::load()?;

    // 1. Transcription via daemon client
    let start_transcribe = Instant::now();
    let transcription = call_transcribe_client(audio_file)?;
    let transcription_ms = start_transcribe.elapsed().as_secs_f64() * 1000.0;

    if transcription.is_empty() {
        return Err("Empty transcription".into());
    }

    // 2. Apply transcription corrections (phonetic/acoustic fixes)
    let start_corrections = Instant::now();
    let corrected_transcription = if config.transcription_corrections.enabled {
        let corrections_file = PathBuf::from(&config.transcription_corrections.corrections_file);
        match TranscriptionCorrector::from_file(&corrections_file) {
            Ok(corrector) => corrector.correct(&transcription),
            Err(_) => transcription.clone(),
        }
    } else {
        transcription.clone()
    };
    let corrections_ms = start_corrections.elapsed().as_secs_f64() * 1000.0;

    // 3. Add trailing space after punctuation
    let text = if config.integration.add_space_after_punctuation {
        clipboard::add_trailing_space_after_punctuation(&corrected_transcription)
    } else {
        corrected_transcription
    };

    // 4. Skip actual clipboard copy in test (wl-copy hangs in test environment)
    // In real usage this takes ~20-30ms but wl-copy stays alive serving the clipboard
    let clipboard_ms = 0.0;

    let total_ms = start_total.elapsed().as_secs_f64() * 1000.0;

    Ok(PipelineMetrics {
        total_ms,
        transcription_ms,
        corrections_ms,
        clipboard_ms,
        text_length: text.len(),
        final_text: text,
    })
}

fn call_transcribe_client(file: &str) -> Result<String, Box<dyn std::error::Error>> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::thread;

    let client_path = PathBuf::from("./target/release/transcribe-client");

    if !client_path.exists() {
        return Err(format!(
            "transcribe-client not found at {:?}. Run: cargo build --release",
            client_path
        )
        .into());
    }

    let mut child = Command::new(&client_path)
        .arg(file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    // Read output with timeout
    let (tx, rx) = std::sync::mpsc::channel();
    let tx_clone = tx.clone();

    // Read stdout in separate thread
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut output = String::new();
        for line in reader.lines() {
            if let Ok(line) = line {
                output.push_str(&line);
                output.push('\n');
            }
        }
        tx.send(("stdout", output)).ok();
    });

    // Read stderr in separate thread
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut output = String::new();
        for line in reader.lines() {
            if let Ok(line) = line {
                output.push_str(&line);
                output.push('\n');
            }
        }
        tx_clone.send(("stderr", output)).ok();
    });

    // Wait for process with timeout
    let timeout = Duration::from_secs(10);
    let start = Instant::now();

    loop {
        match child.try_wait()? {
            Some(status) => {
                // Collect output
                let mut stdout_result = String::new();
                let mut stderr_result = String::new();

                while let Ok((kind, output)) = rx.try_recv() {
                    match kind {
                        "stdout" => stdout_result = output,
                        "stderr" => stderr_result = output,
                        _ => {}
                    }
                }

                if !status.success() {
                    return Err(format!("Transcription failed: {}", stderr_result).into());
                }

                return Ok(stdout_result.trim().to_string());
            }
            None => {
                if start.elapsed() > timeout {
                    child.kill()?;
                    return Err("Transcription client timed out after 10 seconds".into());
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}
