use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;

const GROQ_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const MODEL: &str = "moonshotai/kimi-k2-instruct-0905";

/// Groq API client for LLM post-processing
pub struct GroqClient {
    api_key: String,
}

/// Request structure for Groq API
#[derive(Debug, Serialize)]
struct GroqApiRequest {
    messages: Vec<Message>,
    model: String,
    temperature: f32,
    max_completion_tokens: u32,
    top_p: f32,
    stream: bool,
    stop: Option<String>,
}

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: String,
}

/// Response structure from Groq API
#[derive(Debug, Deserialize)]
struct GroqApiResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: String,
}

impl GroqClient {
    /// Creates a new Groq client with the given API key
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }

    /// Creates a new Groq client by loading API key from .env file
    pub fn from_env_file() -> Result<Self, Box<dyn Error>> {
        let api_key = load_groq_api_key()?;
        Ok(Self::new(api_key))
    }

    /// Sends a completion request to Groq API
    ///
    /// # Arguments
    /// * `prompt` - The prompt/instructions from the .md file
    /// * `transcription` - The transcribed text to process
    ///
    /// # Returns
    /// The processed text from the LLM
    pub fn complete(&self, prompt: &str, transcription: &str) -> Result<String, Box<dyn Error>> {
        // Kimi requires this exact system prompt according to the docs
        let system_prompt = "You are Kimi, an AI assistant created by Moonshot AI.";

        // User message is the prompt instructions followed by the transcription
        let user_message = format!("{}\n\nOriginal dictation:\n{}", prompt, transcription);

        let request = GroqApiRequest {
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: user_message,
                },
            ],
            model: MODEL.to_string(),
            temperature: 0.6,
            max_completion_tokens: 4096,
            top_p: 1.0,
            stream: false,
            stop: None,
        };

        let response = ureq::post(GROQ_API_URL)
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .send_json(&request)?;

        let api_response: GroqApiResponse = response.into_json()?;

        api_response
            .choices
            .first()
            .map(|choice| choice.message.content.clone())
            .ok_or_else(|| "No response from Groq API".into())
    }
}

/// Loads the Groq API key from ~/.config/transcribe-rs/.env file
fn load_groq_api_key() -> Result<String, Box<dyn Error>> {
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let env_path = config_dir.join("transcribe-rs").join(".env");

    let env_content = fs::read_to_string(&env_path)
        .map_err(|e| format!("Failed to read .env file at {}: {}\nCreate the file with: echo 'GROQ_API_KEY=your_key_here' > {}", env_path.display(), e, env_path.display()))?;

    for line in env_content.lines() {
        let line = line.trim();
        if let Some(key) = line.strip_prefix("GROQ_API_KEY=") {
            let key = key.trim();
            // Remove quotes if present
            let key = key.trim_matches('"').trim_matches('\'');
            if !key.is_empty() {
                return Ok(key.to_string());
            }
        }
    }

    Err(format!("GROQ_API_KEY not found in .env file at {}", env_path.display()).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires valid API key and network access
    fn test_groq_completion() {
        let client = GroqClient::from_env_file().expect("Failed to load API key");
        let result = client.complete(
            "You are a helpful assistant. Respond with 'test passed'.",
            "say test passed"
        );
        assert!(result.is_ok());
    }
}
