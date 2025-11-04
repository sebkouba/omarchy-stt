use std::path::PathBuf;
use transcribe_rs::harper_processor::{process_with_harper, Dialect};

fn main() {
    let dict_path = PathBuf::from("/tmp/empty_dict.txt");
    std::fs::write(&dict_path, "").unwrap();

    let text = "This is fucking awesome shit.";
    println!("Original: {}", text);

    match process_with_harper(text, &dict_path, Dialect::American, &["AvoidCurses".to_string()]) {
        Ok(session) => {
            println!("Corrected: {}", session.corrected_text);

            if session.corrected_text.contains('*') {
                println!("❌ FAIL: Still censoring swear words!");
            } else {
                println!("✅ PASS: No censorship!");
            }

            if session.has_corrections() {
                println!("\nCorrections made:");
                for c in &session.corrections {
                    println!("  '{}' → '{}'", c.original, c.replacement);
                }
            }
        }
        Err(e) => println!("Error: {}", e),
    }
}
