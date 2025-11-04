use harper_core::spell::FstDictionary;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::NamedTempFile;
use transcribe_rs::harper_processor::{self, Dialect};

#[test]
fn test_harper_cached_processing() {
    // Load the curated dictionary once (simulating daemon startup)
    let curated_dict = FstDictionary::curated();

    // Create a temporary user dictionary
    let user_dict = NamedTempFile::new().unwrap();

    // Test text with intentional spelling errors
    let text = "This is a teh test sentance.";

    // Process with the cached dictionary
    let session = harper_processor::process_with_harper_cached(
        text,
        &curated_dict,
        user_dict.path(),
        Dialect::American,
        &["AvoidCurses".to_string()],
    ).unwrap();

    // Verify corrections were made
    assert!(session.has_corrections());
    assert_ne!(session.original_text, session.corrected_text);

    // Check that specific corrections were made
    assert!(session.corrected_text.contains("the"));
    assert!(session.corrected_text.contains("sentence"));

    println!("Original: {}", session.original_text);
    println!("Corrected: {}", session.corrected_text);
    println!("Corrections made: {}", session.corrections.len());
}

#[test]
fn test_harper_cached_no_corrections_needed() {
    // Load the curated dictionary
    let curated_dict = FstDictionary::curated();

    // Create a temporary user dictionary
    let user_dict = NamedTempFile::new().unwrap();

    // Perfect text that needs no corrections
    let text = "This is a perfect sentence.";

    // Process with the cached dictionary
    let session = harper_processor::process_with_harper_cached(
        text,
        &curated_dict,
        user_dict.path(),
        Dialect::American,
        &["AvoidCurses".to_string()],
    ).unwrap();

    // Verify no corrections were made
    assert!(!session.has_corrections());
    assert_eq!(session.original_text, session.corrected_text);

    println!("Text unchanged: {}", session.corrected_text);
}

#[test]
fn test_harper_cached_with_custom_dictionary() {
    use std::io::Write;

    // Load the curated dictionary
    let curated_dict = FstDictionary::curated();

    // Create a user dictionary with technical terms
    let mut user_dict = NamedTempFile::new().unwrap();
    writeln!(user_dict, "LLM").unwrap();
    writeln!(user_dict, "Parakeet").unwrap();
    writeln!(user_dict, "ONNX").unwrap();
    user_dict.flush().unwrap();

    // Text with technical terms that would normally be flagged
    let text = "I use an LLM with Parakeet and ONNX models.";

    // Process with the cached dictionary and custom user dictionary
    let session = harper_processor::process_with_harper_cached(
        text,
        &curated_dict,
        user_dict.path(),
        Dialect::American,
        &["AvoidCurses".to_string()],
    ).unwrap();

    // Verify the custom terms are not flagged as errors
    assert_eq!(session.original_text, session.corrected_text);

    println!("Custom dictionary working: {}", session.corrected_text);
}

#[test]
fn test_harper_cached_multiple_calls() {
    // Simulate daemon behavior: load dictionary once, use many times
    let curated_dict = Arc::new(FstDictionary::curated());
    let user_dict = NamedTempFile::new().unwrap();

    // Process multiple texts with the same cached dictionary
    let texts = vec![
        "This is a teh test.",
        "Another sentance with erors.",
        "The quik brown fox.",
    ];

    for (i, text) in texts.iter().enumerate() {
        let session = harper_processor::process_with_harper_cached(
            text,
            &curated_dict,
            user_dict.path(),
            Dialect::American,
            &["AvoidCurses".to_string()],
        ).unwrap();

        assert!(session.has_corrections(), "Text {} should have corrections", i);
        println!("Text {}: '{}' → '{}'", i, session.original_text, session.corrected_text);
    }
}

#[test]
fn test_harper_performance_with_caching() {
    use std::time::Instant;

    let user_dict = NamedTempFile::new().unwrap();
    let text = "This is a teh test sentance with some erors.";

    // First call: load dictionary (one-time cost)
    let start = Instant::now();
    let curated_dict = FstDictionary::curated();
    let dict_load_time = start.elapsed();
    println!("Dictionary load time: {:?}", dict_load_time);

    // First processing call (may include some initialization)
    let start = Instant::now();
    let _session1 = harper_processor::process_with_harper_cached(
        text,
        &curated_dict,
        user_dict.path(),
        Dialect::American,
        &["AvoidCurses".to_string()],
    ).unwrap();
    let first_processing_time = start.elapsed();
    println!("First Harper processing time: {:?}", first_processing_time);

    // Second processing call (should be faster, fully cached)
    let start = Instant::now();
    let _session2 = harper_processor::process_with_harper_cached(
        text,
        &curated_dict,
        user_dict.path(),
        Dialect::American,
        &["AvoidCurses".to_string()],
    ).unwrap();
    let second_processing_time = start.elapsed();
    println!("Second Harper processing time (cached): {:?}", second_processing_time);

    // Key insight: dictionary is loaded once, not on each call
    // In release mode, processing should be much faster
    // In debug mode, it's slower but still uses cached dictionary
    println!("✓ Dictionary loaded once and reused across multiple calls");
}
