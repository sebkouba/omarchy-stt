use std::error::Error;
use std::fs;

/// Loads a prompt from the prompts/ directory
///
/// # Arguments
/// * `name` - Name of the prompt file (without .md extension)
///
/// # Returns
/// The content of the prompt file as a string
///
/// # Example
/// ```no_run
/// let prompt = transcribe_rs::prompts::load_prompt("clean")?;
/// // Loads content from prompts/clean.md
/// ```
pub fn load_prompt(name: &str) -> Result<String, Box<dyn Error>> {
    let path = format!("prompts/{}.md", name);
    fs::read_to_string(&path)
        .map_err(|e| format!("Failed to load prompt '{}': {}", name, e).into())
}
