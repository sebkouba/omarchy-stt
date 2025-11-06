use transcribe_rs::groq::GroqClient;
use std::error::Error;
use std::fs;

fn main() -> Result<(), Box<dyn Error>> {
    println!("=== Testing FIXED Prompt ===\n");

    // Load the fixed prompt
    let fixed_prompt = fs::read_to_string("prompts/clean_fixed.md")?;

    println!("Using fixed prompt from: prompts/clean_fixed.md\n");

    // Create client with tools enabled
    let client = GroqClient::from_env_file()?;

    println!("--- Test 1: LED Control (should call tool) ---");
    match client.complete(&fixed_prompt, "Turn off the LEDs please") {
        Ok(result) => {
            println!("✅ Response: {}", result.text);
            println!("Tool called: {}\n", result.tool_called);
        }
        Err(e) => println!("❌ Error: {}\n", e),
    }

    println!("--- Test 2: Regular Dictation (should clean text) ---");
    match client.complete(&fixed_prompt, "um so like I think we should uh focus on the API") {
        Ok(result) => {
            println!("✅ Response: {}", result.text);
            println!("Tool called: {}\n", result.tool_called);
        }
        Err(e) => println!("❌ Error: {}\n", e),
    }

    println!("\n📋 Check results:");
    println!("Run: tail -100 /tmp/ptt_rust_debug.log | grep -E 'finish_reason|Tool result'");

    Ok(())
}
