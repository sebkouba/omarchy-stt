use transcribe_rs::groq::GroqClient;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("=== Kimi K2 Tool Calling Test ===\n");

    // Create client with tools enabled
    let client = GroqClient::from_env_file()?;

    // Simple, direct prompt that encourages tool usage
    let test_prompt = "You are a helpful assistant that controls LED lights. \
                       When the user asks you to control LEDs, use the appropriate tool function. \
                       After executing the tool, confirm what you did.";

    // Test cases
    let test_cases = vec![
        "Please turn off the LEDs",
        "Switch off the lights",
        "Turn off my display background LEDs",
        "Lights off please",
    ];

    for (i, test_input) in test_cases.iter().enumerate() {
        println!("\n--- Test Case {} ---", i + 1);
        println!("Input: {}", test_input);
        println!("\nSending request to Kimi K2...");
        println!("(Check /tmp/ptt_rust_debug.log for detailed request/response)\n");

        match client.complete(test_prompt, test_input, "test") {
            Ok(result) => {
                println!("✅ Success!");
                println!("Response: {}", result.text);
                println!("Tool called: {}", result.tool_called);
            }
            Err(e) => {
                println!("❌ Error: {}", e);
            }
        }

        // Add a separator for readability
        println!("\n{}", "=".repeat(60));

        // Only run first test by default, uncomment to run all
        break;
    }

    println!("\n📋 Check /tmp/ptt_rust_debug.log for:");
    println!("   - Full request JSON (with tools array)");
    println!("   - Response from Kimi");
    println!("   - finish_reason value");
    println!("   - Tool execution results");
    println!("\nTo view logs: tail -f /tmp/ptt_rust_debug.log");

    Ok(())
}
