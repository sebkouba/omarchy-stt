use std::path::PathBuf;
use transcribe_rs::harper_processor::{process_with_harper, Dialect};

fn main() {
    println!("=== Harper Integration Test ===\n");

    // Create test dictionary
    let config_dir = dirs::config_dir()
        .expect("Could not find config directory")
        .join("transcribe-rs");

    std::fs::create_dir_all(&config_dir).expect("Failed to create config dir");

    let dict_path = config_dir.join("harper_dictionary.txt");
    std::fs::write(&dict_path, "LLM\nParakeet\nAutomattic\n")
        .expect("Failed to write dictionary");

    println!("✅ Created test dictionary at: {}\n", dict_path.display());

    // Test 1: Basic spelling error
    println!("Test 1: Basic spelling error");
    let text1 = "This is a teh test of the Harper intergration.";
    println!("  Original: {}", text1);

    match process_with_harper(text1, &dict_path, Dialect::American, &["AvoidCurses".to_string()]) {
        Ok(session) => {
            println!("  Corrected: {}", session.corrected_text);
            if session.has_corrections() {
                println!("  ✅ Made {} corrections:", session.corrections.len());
                for correction in &session.corrections {
                    println!("    - '{}' → '{}' ({})",
                        correction.original,
                        correction.replacement,
                        correction.lint_kind
                    );
                }
            } else {
                println!("  ℹ️  No corrections needed");
            }
        }
        Err(e) => println!("  ❌ Error: {}", e),
    }

    println!();

    // Test 2: Custom dictionary words
    println!("Test 2: Custom dictionary words (should NOT be corrected)");
    let text2 = "I use an LLM with Parakeet from Automattic.";
    println!("  Original: {}", text2);

    match process_with_harper(text2, &dict_path, Dialect::American, &["AvoidCurses".to_string()]) {
        Ok(session) => {
            println!("  Corrected: {}", session.corrected_text);
            if session.has_corrections() {
                println!("  ⚠️  Unexpected corrections:");
                for correction in &session.corrections {
                    println!("    - '{}' → '{}'",
                        correction.original,
                        correction.replacement
                    );
                }
            } else {
                println!("  ✅ No corrections (custom words recognized)");
            }
        }
        Err(e) => println!("  ❌ Error: {}", e),
    }

    println!();

    // Test 3: Grammar corrections
    println!("Test 3: Grammar corrections");
    let text3 = "the the quick brown fox.";
    println!("  Original: {}", text3);

    match process_with_harper(text3, &dict_path, Dialect::American, &["AvoidCurses".to_string()]) {
        Ok(session) => {
            println!("  Corrected: {}", session.corrected_text);
            if session.has_corrections() {
                println!("  ✅ Made {} corrections:", session.corrections.len());
                for correction in &session.corrections {
                    println!("    - '{}' → '{}' ({}): {}",
                        correction.original,
                        correction.replacement,
                        correction.lint_kind,
                        correction.message
                    );
                }
            }
        }
        Err(e) => println!("  ❌ Error: {}", e),
    }

    println!("\n=== Test Complete ===");
}
