use transcribe_rs::groq::GroqClient;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("=== Testing Different Prompt Formats ===\n");

    let client = GroqClient::from_env_file()?;

    // Test 1: Simple prompt (we know this works)
    println!("--- Test 1: Simple prompt ---");
    let simple_prompt = "You are a helpful assistant that controls LED lights. When the user asks you to control LEDs, use the appropriate tool function.";
    match client.complete(simple_prompt, "Turn off the LEDs") {
        Ok(result) => println!("✅ {} (tool_called: {})\n", result.text, result.tool_called),
        Err(e) => println!("❌ Error: {}\n", e),
    }

    // Test 2: With "Original dictation:" format
    println!("--- Test 2: With 'Original dictation:' format ---");
    let formatted_prompt = "You are a helpful assistant that controls LED lights. When the user asks you to control LEDs, use the appropriate tool function.\n\nOriginal dictation:";
    match client.complete(formatted_prompt, "Turn off the LEDs") {
        Ok(result) => println!("✅ {} (tool_called: {})\n", result.text, result.tool_called),
        Err(e) => println!("❌ Error: {}\n", e),
    }

    // Test 3: Slightly longer but still concise
    println!("--- Test 3: Slightly longer prompt ---");
    let medium_prompt = "Check if the user is requesting LED control. If so, call the appropriate tool. Otherwise, just return the text.\n\nOriginal dictation:";
    match client.complete(medium_prompt, "Turn off the LEDs") {
        Ok(result) => println!("✅ {} (tool_called: {})\n", result.text, result.tool_called),
        Err(e) => println!("❌ Error: {}\n", e),
    }

    Ok(())
}
