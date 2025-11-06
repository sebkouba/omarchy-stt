use transcribe_rs::groq::GroqClient;
use std::error::Error;
use std::fs;

fn main() -> Result<(), Box<dyn Error>> {
    println!("=== Testing with Real clean.md Prompt ===\n");

    // Load the actual clean.md prompt
    let prompt_path = dirs::config_dir()
        .ok_or("Could not find config directory")?
        .join("transcribe-rs/prompts/clean.md");

    let real_prompt = fs::read_to_string(&prompt_path)?;

    println!("Using prompt from: {}\n", prompt_path.display());

    // Create client with tools enabled
    let client = GroqClient::from_env_file()?;

    // Test cases that should trigger LED tools
    let test_cases = vec![
        "Turn off the LEDs please",
        "Switch on my lights",
        "Please turn off my display background LEDs",
    ];

    for (i, test_input) in test_cases.iter().enumerate() {
        println!("\n--- Test Case {} ---", i + 1);
        println!("Input: {}", test_input);
        println!("\nSending request...");

        match client.complete(&real_prompt, test_input) {
            Ok(result) => {
                println!("✅ Response: {}", result.text);
                println!("Tool called: {}", result.tool_called);
            }
            Err(e) => {
                println!("❌ Error: {}", e);
            }
        }

        println!("\n{}", "=".repeat(60));

        // Only run first test by default
        break;
    }

    println!("\n📋 Check /tmp/ptt_rust_debug.log to see:");
    println!("   - Did finish_reason == 'tool_calls'?");
    println!("   - Or did it just return cleaned text?");
    println!("\nRun: tail -50 /tmp/ptt_rust_debug.log | grep -A 2 'finish_reason'");

    Ok(())
}
