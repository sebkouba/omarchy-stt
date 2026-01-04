use std::error::Error;
use std::fs;
use std::path::PathBuf;

/// Get the prompts directory path (~/.config/transcribe-rs/prompts)
pub fn prompts_dir() -> Result<PathBuf, Box<dyn Error>> {
    let config_dir = dirs::config_dir().ok_or("Could not find config directory")?;
    Ok(config_dir.join("transcribe-rs").join("prompts"))
}

/// Loads a prompt from ~/.config/transcribe-rs/prompts/ directory
///
/// # Arguments
/// * `name` - Name of the prompt file (without .md extension)
///
/// # Returns
/// The content of the prompt file as a string
///
/// # Example
/// ```ignore
/// let prompt = transcribe_rs::prompts::load_prompt("clean")?;
/// // Loads content from ~/.config/transcribe-rs/prompts/clean.md
/// ```
pub fn load_prompt(name: &str) -> Result<String, Box<dyn Error>> {
    let prompts_dir = prompts_dir()?;
    let path = prompts_dir.join(format!("{}.md", name));

    fs::read_to_string(&path)
        .map_err(|e| {
            format!(
                "Failed to load prompt '{}' from {}: {}\nMake sure the prompt file exists in the prompts directory.",
                name,
                path.display(),
                e
            )
            .into()
        })
}
