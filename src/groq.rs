use serde::{Deserialize, Serialize};
use serde_json::json;
use std::error::Error;
use std::fs;

const GROQ_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const MODEL: &str = "moonshotai/kimi-k2-instruct-0905";
const LED_API_URL: &str = "http://192.168.2.40/json/state";
const MAX_TOOL_ITERATIONS: usize = 5;

/// Groq API client for LLM post-processing with tool calling support
pub struct GroqClient {
    api_key: String,
    http_client: reqwest::Client,
    enable_tools: bool,
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub message: String,
}

/// Chat message for API requests
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Message {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// Tool call structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: FunctionCall,
}

/// Function call details
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FunctionCall {
    name: String,
    arguments: String,
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Tool {
    #[serde(rename = "type")]
    tool_type: String,
    function: FunctionDef,
}

/// Function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// API request structure
#[derive(Debug, Serialize)]
struct ApiRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
    max_completion_tokens: u32,
    top_p: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

/// API response structure - lenient deserialization
#[derive(Debug, Deserialize)]
struct ApiResponse {
    choices: Vec<Choice>,
}

/// Choice structure
#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

impl GroqClient {
    /// Creates a new Groq client with the given API key
    pub fn new(api_key: String, enable_tools: bool) -> Self {
        let http_client = reqwest::Client::new();

        Self {
            api_key,
            http_client,
            enable_tools
        }
    }

    /// Creates a new Groq client by loading API key from .env file
    pub fn from_env_file() -> Result<Self, Box<dyn Error>> {
        let api_key = load_groq_api_key()?;
        Ok(Self::new(api_key, true))
    }

    /// Creates a new Groq client with tools disabled
    pub fn from_env_file_no_tools() -> Result<Self, Box<dyn Error>> {
        let api_key = load_groq_api_key()?;
        Ok(Self::new(api_key, false))
    }

    /// Sends a completion request to Groq API with tool calling support
    ///
    /// # Arguments
    /// * `prompt` - The prompt/instructions from the .md file
    /// * `transcription` - The transcribed text to process
    ///
    /// # Returns
    /// The processed text from the LLM
    pub fn complete(&self, prompt: &str, transcription: &str) -> Result<String, Box<dyn Error>> {
        // Use tokio runtime to run async code
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(self.complete_async(prompt, transcription))
    }

    /// Async version of complete with full tool calling support
    async fn complete_async(&self, prompt: &str, transcription: &str) -> Result<String, Box<dyn Error>> {
        // Kimi requires this exact system prompt according to the docs
        let system_prompt = "You are Kimi, an AI assistant created by Moonshot AI.";

        // User message is the prompt instructions followed by the transcription
        let user_message = format!("{}\n\nOriginal dictation:\n{}", prompt, transcription);

        let mut messages: Vec<Message> = vec![
            Message {
                role: "system".to_string(),
                content: Some(system_prompt.to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: "user".to_string(),
                content: Some(user_message),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        // Define tools if enabled
        let tools = if self.enable_tools {
            Some(vec![
                self.create_led_on_tool(),
                self.create_led_off_tool(),
            ])
        } else {
            None
        };

        // Tool calling loop - iterate until we get a final text response
        for _iteration in 0..MAX_TOOL_ITERATIONS {
            let request = ApiRequest {
                model: MODEL.to_string(),
                messages: messages.clone(),
                temperature: 0.6,
                max_completion_tokens: 4096,
                top_p: 1.0,
                tools: tools.clone(),
                tool_choice: if tools.is_some() { Some("auto".to_string()) } else { None },
            };

            let response = self.http_client
                .post(GROQ_API_URL)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", self.api_key))
                .json(&request)
                .send()
                .await?;

            let response_text = response.text().await?;
            let api_response: ApiResponse = serde_json::from_str(&response_text)
                .map_err(|e| format!("Failed to parse response: {} | Response: {}", e, response_text))?;

            let choice = api_response
                .choices
                .first()
                .ok_or("No response from Groq API")?;

            // Check if the model wants to call tools
            if let Some(tool_calls) = &choice.message.tool_calls {
                if tool_calls.is_empty() {
                    // No tool calls, just return the text
                    if let Some(ref content) = choice.message.content {
                        return Ok(content.clone());
                    }
                    return Err("No content in response".into());
                }

                // Add the assistant's message with tool calls to history
                messages.push(Message {
                    role: "assistant".to_string(),
                    content: choice.message.content.clone(),
                    tool_calls: Some(tool_calls.clone()),
                    tool_call_id: None,
                });

                // Execute each tool call
                for tool_call in tool_calls {
                    let function_name = &tool_call.function.name;
                    let function_args = &tool_call.function.arguments;

                    // Execute the tool
                    let result = self.execute_tool(function_name, function_args).await?;

                    // Add tool result to messages
                    messages.push(Message {
                        role: "tool".to_string(),
                        content: Some(serde_json::to_string(&result)?),
                        tool_calls: None,
                        tool_call_id: Some(tool_call.id.clone()),
                    });
                }

                // Continue the loop to get the final response
                continue;
            }

            // No tool calls, return the content
            if let Some(ref content) = choice.message.content {
                return Ok(content.clone());
            }

            return Err("Unexpected response format".into());
        }

        Err(format!("Max tool iterations ({}) exceeded", MAX_TOOL_ITERATIONS).into())
    }

    /// Creates the LED on tool definition
    fn create_led_on_tool(&self) -> Tool {
        Tool {
            tool_type: "function".to_string(),
            function: FunctionDef {
                name: "turn_leds_on".to_string(),
                description: "Turn on the display background LEDs".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
        }
    }

    /// Creates the LED off tool definition
    fn create_led_off_tool(&self) -> Tool {
        Tool {
            tool_type: "function".to_string(),
            function: FunctionDef {
                name: "turn_leds_off".to_string(),
                description: "Turn off the display background LEDs".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
        }
    }

    /// Executes a tool call
    async fn execute_tool(&self, function_name: &str, _args: &str) -> Result<ToolResult, Box<dyn Error>> {
        match function_name {
            "turn_leds_on" => self.turn_leds_on().await,
            "turn_leds_off" => self.turn_leds_off().await,
            _ => Err(format!("Unknown tool: {}", function_name).into()),
        }
    }

    /// Turns on the LEDs by calling the HTTP API
    async fn turn_leds_on(&self) -> Result<ToolResult, Box<dyn Error>> {
        let payload = json!({
            "on": true,
            "v": true
        });

        let response = self.http_client
            .post(LED_API_URL)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(ToolResult {
                success: true,
                message: "LEDs turned on successfully".to_string(),
            })
        } else {
            Ok(ToolResult {
                success: false,
                message: format!("Failed to turn on LEDs: HTTP {}", response.status()),
            })
        }
    }

    /// Turns off the LEDs by calling the HTTP API
    async fn turn_leds_off(&self) -> Result<ToolResult, Box<dyn Error>> {
        let payload = json!({
            "on": false
        });

        let response = self.http_client
            .post(LED_API_URL)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(ToolResult {
                success: true,
                message: "LEDs turned off successfully".to_string(),
            })
        } else {
            Ok(ToolResult {
                success: false,
                message: format!("Failed to turn off LEDs: HTTP {}", response.status()),
            })
        }
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
    fn test_load_api_key_format() {
        // Test that load_groq_api_key function signature is correct
        let _ = load_groq_api_key();
    }

    #[test]
    fn test_tool_result_serialization() {
        let result = ToolResult {
            success: true,
            message: "Test message".to_string(),
        };

        let json = serde_json::to_string(&result).expect("Failed to serialize");
        assert!(json.contains("success"));
        assert!(json.contains("Test message"));

        let deserialized: ToolResult = serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(deserialized.success, true);
        assert_eq!(deserialized.message, "Test message");
    }

    #[test]
    fn test_client_creation() {
        let client = GroqClient::new("test_key".to_string(), true);
        assert!(client.enable_tools);

        let client_no_tools = GroqClient::new("test_key".to_string(), false);
        assert!(!client_no_tools.enable_tools);
    }

    #[test]
    fn test_tool_definitions() {
        let client = GroqClient::new("test_key".to_string(), true);

        let led_on_tool = client.create_led_on_tool();
        assert_eq!(led_on_tool.function.name, "turn_leds_on");
        assert!(led_on_tool.function.description.contains("LED"));

        let led_off_tool = client.create_led_off_tool();
        assert_eq!(led_off_tool.function.name, "turn_leds_off");
        assert!(led_off_tool.function.description.contains("LED"));
    }

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

    #[tokio::test]
    #[ignore] // Requires LED device on network
    async fn test_led_control() {
        let client = GroqClient::new("test_key".to_string(), true);

        // Test turning LEDs on
        let result = client.turn_leds_on().await;
        assert!(result.is_ok());
        if let Ok(tool_result) = result {
            println!("LED ON result: {:?}", tool_result);
        }

        // Wait a bit
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Test turning LEDs off
        let result = client.turn_leds_off().await;
        assert!(result.is_ok());
        if let Ok(tool_result) = result {
            println!("LED OFF result: {:?}", tool_result);
        }
    }

    #[test]
    #[ignore] // Requires valid API key, network access, and LED device
    fn test_tool_calling_integration() {
        let client = GroqClient::from_env_file().expect("Failed to load API key");

        // Test a command that should trigger tool calling
        let result = client.complete(
            "When the user asks you to control LEDs, use the appropriate tool. Always respond confirming the action.",
            "turn on my LEDs"
        );

        assert!(result.is_ok());
        if let Ok(response) = result {
            println!("Response: {}", response);
            // The response should confirm the action
            assert!(response.to_lowercase().contains("led") || response.to_lowercase().contains("light"));
        }
    }
}
