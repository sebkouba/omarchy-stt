use serde::{Deserialize, Serialize};
use serde_json::json;
use std::error::Error;
use std::fs;
use std::io::Write;
use crate::tools::ToolConfig;

/// Append a log message to the debug log
fn log(message: &str) {
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/ptt_rust_debug.log")
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        writeln!(file, "[{}] [groq] {}", timestamp, message).ok();
    }
}

const GROQ_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const MODEL: &str = "moonshotai/kimi-k2-instruct-0905";
const MAX_TOOL_ITERATIONS: usize = 5;

/// Groq API client for LLM post-processing with tool calling support
pub struct GroqClient {
    api_key: String,
    http_client: reqwest::Client,
    tools: Vec<ToolConfig>,
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub message: String,
}

/// Completion result with tool execution tracking
#[derive(Debug, Clone)]
pub struct CompletionResult {
    pub text: String,
    pub tool_called: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
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
    finish_reason: Option<String>,
}

impl GroqClient {
    /// Creates a new Groq client with the given API key and tools
    pub fn new(api_key: String, tools: Vec<ToolConfig>) -> Self {
        let http_client = reqwest::Client::new();

        Self {
            api_key,
            http_client,
            tools,
        }
    }

    /// Creates a new Groq client by loading API key from .env file and tools from config
    pub fn from_env_file() -> Result<Self, Box<dyn Error>> {
        let api_key = load_groq_api_key()?;
        let tools = crate::tools::load_tools()?;
        log(&format!("Loaded {} tools from config", tools.len()));
        Ok(Self::new(api_key, tools))
    }

    /// Creates a new Groq client with tools disabled
    pub fn from_env_file_no_tools() -> Result<Self, Box<dyn Error>> {
        let api_key = load_groq_api_key()?;
        Ok(Self::new(api_key, Vec::new()))
    }

    /// Sends a completion request to Groq API with tool calling support
    ///
    /// # Arguments
    /// * `prompt` - The prompt/instructions from the .md file
    /// * `transcription` - The transcribed text to process
    /// * `prompt_name` - The name of the prompt (for conversation history tracking)
    ///
    /// # Returns
    /// CompletionResult with the processed text and whether a tool was called
    pub fn complete(&self, prompt: &str, transcription: &str, prompt_name: &str) -> Result<CompletionResult, Box<dyn Error>> {
        self.complete_with_context(prompt, transcription, prompt_name, None)
    }

    /// Sends a completion request to Groq API with optional OCR screen context
    ///
    /// # Arguments
    /// * `prompt` - The prompt/instructions from the .md file
    /// * `transcription` - The transcribed text to process
    /// * `prompt_name` - The name of the prompt (for conversation history tracking)
    /// * `ocr_context` - Optional screen OCR text for additional context
    ///
    /// # Returns
    /// CompletionResult with the processed text and whether a tool was called
    pub fn complete_with_context(&self, prompt: &str, transcription: &str, prompt_name: &str, ocr_context: Option<&str>) -> Result<CompletionResult, Box<dyn Error>> {
        // Use tokio runtime to run async code
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(self.complete_async_with_context(prompt, transcription, prompt_name, ocr_context))
    }

    /// Async version of complete with full tool calling support
    async fn complete_async(&self, prompt: &str, transcription: &str, prompt_name: &str) -> Result<CompletionResult, Box<dyn Error>> {
        self.complete_async_with_context(prompt, transcription, prompt_name, None).await
    }

    /// Async version of complete with OCR context support
    async fn complete_async_with_context(&self, prompt: &str, transcription: &str, prompt_name: &str, ocr_context: Option<&str>) -> Result<CompletionResult, Box<dyn Error>> {
        let config = crate::config::Config::load()?;

        // Build messages with or without history based on config
        let history_enabled = Self::is_history_enabled_for_prompt(&config.llm, prompt_name);
        log(&format!("Conversation history for prompt '{}': {}", prompt_name, if history_enabled { "ENABLED" } else { "DISABLED" }));

        if let Some(ctx) = ocr_context {
            log(&format!("OCR context provided: {} chars", ctx.len()));
        }

        let mut messages = if history_enabled {
            self.build_messages_with_history(prompt, transcription, prompt_name, &config.llm, ocr_context)?
        } else {
            self.build_messages_without_history(prompt, transcription, ocr_context)
        };

        // Convert ToolConfig to API Tool format
        let tools = if !self.tools.is_empty() {
            Some(self.create_tools_from_config())
        } else {
            None
        };

        // Tool calling loop - iterate until finish_reason is not "tool_calls"
        let mut tool_was_called = false;

        for iteration in 0..MAX_TOOL_ITERATIONS {
            let request = ApiRequest {
                model: MODEL.to_string(),
                messages: messages.clone(),
                temperature: 0.3,
                max_completion_tokens: 4096,
                top_p: 1.0,
                tools: tools.clone(),
                tool_choice: if tools.is_some() { Some("auto".to_string()) } else { None },
            };

            // Log the request for debugging
            log(&format!("=== Iteration {} ===", iteration));
            if let Ok(request_json) = serde_json::to_string_pretty(&request) {
                log(&format!("Request JSON:\n{}", request_json));
            }
            log(&format!("Tools count: {}", self.tools.len()));
            log(&format!("Tools in request: {}", if tools.is_some() { "YES" } else { "NO" }));

            let response = self.http_client
                .post(GROQ_API_URL)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", self.api_key))
                .json(&request)
                .send()
                .await?;

            let response_text = response.text().await?;
            log(&format!("Response text: {}", response_text));

            let api_response: ApiResponse = serde_json::from_str(&response_text)
                .map_err(|e| format!("Failed to parse response: {} | Response: {}", e, response_text))?;

            let choice = api_response
                .choices
                .first()
                .ok_or("No response from Groq API")?;

            let finish_reason = choice.finish_reason.clone();
            log(&format!("finish_reason: {:?}", finish_reason));

            // Check finish_reason to see if model wants to call tools
            if finish_reason.as_deref() == Some("tool_calls") {
                log("Model returned finish_reason='tool_calls' - executing tools");

                // Mark that a tool was called
                tool_was_called = true;

                // Model wants to call tools - append the assistant message
                messages.push(choice.message.clone());

                // Execute each tool call
                if let Some(tool_calls) = &choice.message.tool_calls {
                    log(&format!("Found {} tool call(s)", tool_calls.len()));

                    for tool_call in tool_calls {
                        let function_name = &tool_call.function.name;
                        let function_args = &tool_call.function.arguments;

                        log(&format!("Executing tool: {} with args: {}", function_name, function_args));

                        // Execute the tool
                        let result = self.execute_tool(function_name, function_args).await?;
                        log(&format!("Tool result: {:?}", result));

                        // Add tool result to messages with required fields per docs
                        messages.push(Message {
                            role: "tool".to_string(),
                            tool_call_id: Some(tool_call.id.clone()),
                            name: Some(function_name.clone()),
                            content: Some(serde_json::to_string(&result)?),
                            tool_calls: None,
                        });
                    }
                } else {
                    log("WARNING: finish_reason='tool_calls' but no tool_calls in message!");
                }

                // Continue the loop to get the final response
                continue;
            } else {
                log(&format!("Model did not request tool calls. Content: {:?}", choice.message.content));
            }

            // Not a tool call - return the final content
            if let Some(ref content) = choice.message.content {
                // Save to history if enabled for this prompt
                if history_enabled {
                    if let Err(e) = self.save_to_history(prompt, transcription, prompt_name, content, &config.llm) {
                        log(&format!("Warning: Failed to save conversation history: {}", e));
                    }
                }

                return Ok(CompletionResult {
                    text: content.clone(),
                    tool_called: tool_was_called,
                });
            }

            return Err(format!("Unexpected response at iteration {}: finish_reason={:?}, no content", iteration, finish_reason).into());
        }

        Err(format!("Max tool iterations ({}) exceeded", MAX_TOOL_ITERATIONS).into())
    }

    /// Convert ToolConfig to API Tool format
    fn create_tools_from_config(&self) -> Vec<Tool> {
        self.tools
            .iter()
            .map(|tool_config| {
                // Build parameters JSON schema
                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();

                for (param_name, param_schema) in &tool_config.parameters {
                    properties.insert(
                        param_name.clone(),
                        json!({
                            "type": param_schema.param_type,
                            "description": param_schema.description,
                        }),
                    );
                    required.push(param_name.clone());
                }

                let parameters = json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                });

                Tool {
                    tool_type: "function".to_string(),
                    function: FunctionDef {
                        name: tool_config.name.clone(),
                        description: tool_config.description.clone(),
                        parameters,
                    },
                }
            })
            .collect()
    }

    /// Check if conversation history is enabled for a specific prompt
    fn is_history_enabled_for_prompt(llm_config: &crate::config::LlmConfig, prompt_name: &str) -> bool {
        // Must have global toggle enabled AND prompt must be in the list
        if !llm_config.conversation_history_enabled {
            return false;
        }

        // Check if this prompt is in the enabled list
        llm_config.conversation_history_prompts.contains(&prompt_name.to_string())
    }

    /// Executes a tool call using the generic tools module
    async fn execute_tool(&self, function_name: &str, args: &str) -> Result<ToolResult, Box<dyn Error>> {
        // Find the tool config
        let tool_config = self
            .tools
            .iter()
            .find(|t| t.name == function_name)
            .ok_or_else(|| format!("Unknown tool: {}", function_name))?;

        // Parse arguments
        let params: serde_json::Value = if args.is_empty() {
            json!({})
        } else {
            serde_json::from_str(args)?
        };

        log(&format!("Executing tool: {} with params: {:?}", function_name, params));

        // Execute using the tools module
        let result = crate::tools::execute_tool(tool_config, &params)?;

        Ok(ToolResult {
            success: true,
            message: result,
        })
    }

    /// Build messages without conversation history (legacy behavior)
    fn build_messages_without_history(&self, prompt: &str, transcription: &str, ocr_context: Option<&str>) -> Vec<Message> {
        // Build user content with optional OCR context
        let user_content = if let Some(ocr_text) = ocr_context {
            format!(
                "{}\n\nScreen context (OCR of active window):\n{}\n\nUser dictation:\n{}",
                prompt, ocr_text, transcription
            )
        } else {
            format!("{}\n\nOriginal dictation:\n{}", prompt, transcription)
        };

        vec![
            Message {
                role: "system".to_string(),
                content: Some("You are Kimi, an AI assistant created by Moonshot AI.".to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            Message {
                role: "user".to_string(),
                content: Some(user_content),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ]
    }

    /// Build messages with conversation history
    fn build_messages_with_history(&self, prompt: &str, transcription: &str, prompt_name: &str, llm_config: &crate::config::LlmConfig, ocr_context: Option<&str>) -> Result<Vec<Message>, Box<dyn Error>> {
        use crate::conversation_history::ConversationHistory;

        let history = ConversationHistory::new(
            prompt_name,
            llm_config.conversation_history_minutes,
            llm_config.conversation_max_turns,
            &llm_config.conversation_history_dir,
        );

        let history_messages = history.load_history()?;

        log(&format!("Loaded {} history messages for prompt '{}'", history_messages.len(), prompt_name));

        let mut messages = vec![
            Message {
                role: "system".to_string(),
                content: Some("You are Kimi, an AI assistant created by Moonshot AI.".to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }
        ];

        if history_messages.is_empty() {
            // First message in conversation - include prompt instructions
            log("No history found, starting new conversation");

            // Build user content with optional OCR context
            let user_content = if let Some(ocr_text) = ocr_context {
                format!(
                    "{}\n\nScreen context (OCR of active window):\n{}\n\nUser dictation:\n{}",
                    prompt, ocr_text, transcription
                )
            } else {
                format!("{}\n\nOriginal dictation:\n{}", prompt, transcription)
            };

            messages.push(Message {
                role: "user".to_string(),
                content: Some(user_content),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        } else {
            // Continuing conversation - add history then new dictation
            log("Continuing existing conversation");

            // Add all history messages
            for history_msg in history_messages {
                messages.push(Message {
                    role: history_msg.role,
                    content: Some(history_msg.content),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
            }

            // Add new dictation with optional OCR context
            let user_content = if let Some(ocr_text) = ocr_context {
                format!(
                    "Screen context (OCR of active window):\n{}\n\nUser dictation:\n{}",
                    ocr_text, transcription
                )
            } else {
                transcription.to_string()
            };

            messages.push(Message {
                role: "user".to_string(),
                content: Some(user_content),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }

        Ok(messages)
    }

    /// Save conversation turn to history
    fn save_to_history(&self, prompt: &str, transcription: &str, prompt_name: &str, assistant_response: &str, llm_config: &crate::config::LlmConfig) -> Result<(), Box<dyn Error>> {
        use crate::conversation_history::ConversationHistory;

        let history = ConversationHistory::new(
            prompt_name,
            llm_config.conversation_history_minutes,
            llm_config.conversation_max_turns,
            &llm_config.conversation_history_dir,
        );

        // Check if this is a new conversation or continuation
        let history_messages = history.load_history()?;
        let is_first = history_messages.is_empty();

        // Save user message
        if is_first {
            // First message includes prompt instructions
            let full_user_message = format!("{}\n\nOriginal dictation:\n{}", prompt, transcription);
            history.append_user(&full_user_message, true)?;
            log("Saved first user message with prompt instructions to history");
        } else {
            // Subsequent messages are just the raw dictation
            history.append_user(transcription, false)?;
            log("Saved user message to history");
        }

        // Save assistant response
        history.append_assistant(assistant_response)?;
        log("Saved assistant response to history");

        Ok(())
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
        let tools = vec![];
        let client = GroqClient::new("test_key".to_string(), tools);
        assert_eq!(client.tools.len(), 0);

        let tools = vec![];
        let client_no_tools = GroqClient::new("test_key".to_string(), tools);
        assert_eq!(client_no_tools.tools.len(), 0);
    }

    #[test]
    fn test_tool_definitions() {
        use crate::tools::{ToolConfig, ParameterSchema};
        use std::collections::HashMap;

        let mut params = HashMap::new();
        params.insert(
            "test_param".to_string(),
            ParameterSchema {
                param_type: "string".to_string(),
                description: "Test parameter".to_string(),
            },
        );

        let tool_config = ToolConfig {
            name: "test_tool".to_string(),
            description: "Test tool description".to_string(),
            command: "echo".to_string(),
            args: vec!["{test_param}".to_string()],
            backend: "cli".to_string(),
            http: None,
            parameters: params,
        };

        let client = GroqClient::new("test_key".to_string(), vec![tool_config]);
        let tools = client.create_tools_from_config();

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "test_tool");
        assert!(tools[0].function.description.contains("Test tool"));
    }

    #[test]
    #[ignore] // Requires valid API key and network access
    fn test_groq_completion() {
        let client = GroqClient::from_env_file().expect("Failed to load API key");
        let result = client.complete(
            "You are a helpful assistant. Respond with 'test passed'.",
            "say test passed",
            "test"
        );
        assert!(result.is_ok());
        if let Ok(completion) = result {
            assert!(!completion.tool_called);
            assert!(completion.text.contains("test passed"));
        }
    }

    #[test]
    #[ignore] // Requires valid API key, network access, and tools configured
    fn test_tool_calling_integration() {
        let client = GroqClient::from_env_file().expect("Failed to load API key and tools");

        // Test a command that should trigger tool calling (if tools are configured)
        let result = client.complete(
            "When the user asks you to use tools, use the appropriate tool. Always respond confirming the action.",
            "test command",
            "test"
        );

        assert!(result.is_ok());
        if let Ok(completion) = result {
            println!("Response: {}", completion.text);
            println!("Tool called: {}", completion.tool_called);
        }
    }
}
