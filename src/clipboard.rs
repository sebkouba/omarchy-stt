//! Clipboard operations using arboard (cross-platform)

use arboard::Clipboard;
use std::error::Error;

/// Copy text to system clipboard
pub fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn Error>> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text)?;
    Ok(())
}

/// Add space after sentence-ending punctuation
/// This allows consecutive PTT dictations to flow naturally
pub fn add_trailing_space_after_punctuation(text: &str) -> String {
    if let Some(last_char) = text.chars().last() {
        if matches!(last_char, '.' | '!' | '?') {
            return format!("{} ", text);
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_space_after_period() {
        let input = "Hello world.";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "Hello world. ");
    }

    #[test]
    fn test_add_space_after_question() {
        let input = "How are you?";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "How are you? ");
    }

    #[test]
    fn test_add_space_after_exclamation() {
        let input = "Wow!";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "Wow! ");
    }

    #[test]
    fn test_no_space_after_comma() {
        let input = "Hello world, how are you";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "Hello world, how are you");
    }

    #[test]
    fn test_empty_string() {
        let input = "";
        let output = add_trailing_space_after_punctuation(input);
        assert_eq!(output, "");
    }
}
