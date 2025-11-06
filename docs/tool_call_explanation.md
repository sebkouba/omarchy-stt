# How Tool Calling Works: A Complete Walkthrough

## For Junior Developers: Understanding the "Magic"

This document explains how voice commands like **"switch to workspace 3"** trigger shell commands like `hyprctl dispatch workspace 3`. It seems magical, but it's actually a clever orchestration between configuration, LLM tool calling, and parameter substitution.

---

## The Big Picture

```
User speaks → Parakeet transcribes → Groq/Kimi decides + extracts params → Execute shell command → Show notification
```

**Key insight:** The LLM (Kimi K2) does ALL the intelligence work. Our code just:
1. Tells the LLM what tools exist
2. Executes whatever the LLM decides to call
3. Does simple string substitution for parameters

No pattern matching. No regex. No parsing voice commands ourselves.

---

## Example: "Switch to workspace 3"

Let's trace this exact command through the entire system.

---

## Part 1: The Configuration File

**File:** `~/.config/transcribe-rs/tools.json`

```json
{
  "tools": [
    {
      "name": "switch_workspace",
      "description": "Switch to a Hyprland workspace. Call when user says 'workspace 3', 'go to workspace 5', 'switch to workspace 2', etc.",
      "command": "hyprctl",
      "args": ["dispatch", "workspace", "{workspace}"],
      "parameters": {
        "workspace": {
          "type": "integer",
          "description": "Target workspace number (1-10)"
        }
      }
    }
  ]
}
```

### Breaking Down Each Field

| Field | Value | Purpose |
|-------|-------|---------|
| `name` | `"switch_workspace"` | Unique identifier for this tool. The LLM will return this name when it decides to call the tool. |
| `description` | `"Switch to a Hyprland workspace..."` | **CRITICAL!** This tells the LLM WHEN to use this tool. The more explicit you are about trigger phrases, the better. |
| `command` | `"hyprctl"` | The actual shell command to execute. |
| `args` | `["dispatch", "workspace", "{workspace}"]` | Command-line arguments. Notice `{workspace}` - this is a **placeholder** that will be replaced with the actual value. |
| `parameters.workspace` | `{ type: "integer", description: "..." }` | Tells the LLM: "You need to extract a workspace number from what the user said." |

**The crucial understanding:** The `description` field teaches the LLM. The `parameters` section tells the LLM what to extract.

---

## Part 2: Loading the Configuration (Startup)

**File:** `src/tools.rs`, function `load_tools()`

When you run `transcribe start`, this happens:

```rust
// src/groq.rs:133
let tools = crate::tools::load_tools()?;
```

This reads `~/.config/transcribe-rs/tools.json` and deserializes it into Rust structs:

```rust
pub struct ToolConfig {
    pub name: String,              // "switch_workspace"
    pub description: String,       // "Switch to a Hyprland workspace..."
    pub command: String,           // "hyprctl"
    pub args: Vec<String>,         // ["dispatch", "workspace", "{workspace}"]
    pub parameters: HashMap<...>,  // { "workspace": { type: "integer", ... } }
}
```

The `Vec<ToolConfig>` is stored in the `GroqClient` struct and carried around for the entire session.

---

## Part 3: You Speak

**You say:** "Switch to workspace 3"

**Parakeet transcribes:** `"Switch to workspace 3"` (just text, no understanding)

The transcription is stored in `/tmp/ptt_current.wav` and then converted to text. At this point, the system has NO IDEA this is a tool call. It's just text.

---

## Part 4: Converting Tools to Kimi Format

**File:** `src/groq.rs`, function `create_tools_from_config()`

Before sending the request to Groq/Kimi, we need to convert our simple JSON format into the **OpenAI tool calling format** that Kimi expects:

```rust
// src/groq.rs:293
fn create_tools_from_config(&self) -> Vec<Tool> {
    self.tools.iter().map(|tool_config| {
        // Build JSON schema for parameters
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for (param_name, param_schema) in &tool_config.parameters {
            properties.insert(
                param_name.clone(),
                json!({
                    "type": param_schema.param_type,      // "integer"
                    "description": param_schema.description, // "Target workspace number"
                }),
            );
            required.push(param_name.clone()); // ["workspace"]
        }

        Tool {
            tool_type: "function".to_string(),
            function: FunctionDef {
                name: tool_config.name.clone(),
                description: tool_config.description.clone(),
                parameters: json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                }),
            },
        }
    }).collect()
}
```

This produces the **Kimi tool definition**:

```json
{
  "type": "function",
  "function": {
    "name": "switch_workspace",
    "description": "Switch to a Hyprland workspace. Call when user says 'workspace 3'...",
    "parameters": {
      "type": "object",
      "properties": {
        "workspace": {
          "type": "integer",
          "description": "Target workspace number (1-10)"
        }
      },
      "required": ["workspace"]
    }
  }
}
```

**This JSON schema is what teaches the LLM how to call the tool.**

---

## Part 5: Sending to Groq/Kimi

**File:** `src/groq.rs`, function `complete_async()`

We send a request to the Groq API with:

1. **System message:** `"You are Kimi, an AI assistant created by Moonshot AI."` (required by Kimi)
2. **User message:** The prompt from `~/.config/transcribe-rs/prompts/clean.md` + the transcription
3. **Tools array:** All tool definitions (including `switch_workspace`)

```rust
// src/groq.rs:196
let request = ApiRequest {
    model: "moonshotai/kimi-k2-instruct-0905",
    messages: [
        { role: "system", content: "You are Kimi..." },
        { role: "user", content: "Your Task: First, check if user is requesting...\n\nOriginal dictation:\nSwitch to workspace 3" }
    ],
    tools: [
        { type: "function", function: { name: "switch_workspace", ... } },
        { type: "function", function: { name: "turn_leds_on", ... } },
        // ... all other tools
    ],
    tool_choice: "auto"  // Let LLM decide
};
```

**What we're asking Kimi:** "Here's what the user said. Here are the tools you can use. Decide if this matches any tool."

---

## Part 6: Kimi's Decision (The Magic Happens Here)

Kimi K2 reads:
- **User said:** "Switch to workspace 3"
- **Available tool:** `switch_workspace` with description "Call when user says 'workspace 3', 'go to workspace 5'..."
- **Required parameter:** `workspace` (type: integer)

**Kimi thinks:** "This matches the `switch_workspace` tool! The user wants workspace `3`."

**Kimi returns:**

```json
{
  "choices": [{
    "message": {
      "role": "assistant",
      "content": null,
      "tool_calls": [
        {
          "id": "call_abc123",
          "type": "function",
          "function": {
            "name": "switch_workspace",
            "arguments": "{\"workspace\": 3}"
          }
        }
      ]
    },
    "finish_reason": "tool_calls"
  }]
}
```

**Key observations:**
- `finish_reason: "tool_calls"` - signals "I want to call a tool, not return text"
- `function.name: "switch_workspace"` - which tool to call
- `function.arguments: "{\"workspace\": 3}"` - **the LLM extracted the number 3 from the sentence!**

**This is the breakthrough:** The LLM did natural language understanding. It saw "workspace 3" and knew to extract `3` as the parameter.

---

## Part 7: Detecting the Tool Call

**File:** `src/groq.rs`, lines 234-271

Our code checks the response:

```rust
// src/groq.rs:234
if finish_reason.as_deref() == Some("tool_calls") {
    log("Model returned finish_reason='tool_calls' - executing tools");
    tool_was_called = true;  // Important: flag that we executed a tool

    // Get the tool calls from the response
    if let Some(tool_calls) = &choice.message.tool_calls {
        for tool_call in tool_calls {
            let function_name = &tool_call.function.name;        // "switch_workspace"
            let function_args = &tool_call.function.arguments;   // "{\"workspace\": 3}"

            // Execute it!
            let result = self.execute_tool(function_name, function_args).await?;
        }
    }
}
```

---

## Part 8: Executing the Tool

**File:** `src/groq.rs`, function `execute_tool()`

```rust
// src/groq.rs:331
async fn execute_tool(&self, function_name: &str, args: &str) -> Result<ToolResult, Box<dyn Error>> {
    // Find our ToolConfig by name
    let tool_config = self.tools.iter()
        .find(|t| t.name == function_name)  // Find "switch_workspace"
        .ok_or_else(|| format!("Unknown tool: {}", function_name))?;

    // Parse the JSON arguments: "{\"workspace\": 3}" → { "workspace": 3 }
    let params: serde_json::Value = serde_json::from_str(args)?;

    // Delegate to the tools module
    let result = crate::tools::execute_tool(tool_config, &params)?;

    Ok(ToolResult { success: true, message: result })
}
```

Now we're in the generic tools execution code.

---

## Part 9: Parameter Substitution

**File:** `src/tools.rs`, function `execute_cli_tool()`

```rust
// src/tools.rs:68
fn execute_cli_tool(tool: &ToolConfig, params: &serde_json::Value) -> Result<String, Box<dyn Error>> {
    // tool.args = ["dispatch", "workspace", "{workspace}"]
    // params = { "workspace": 3 }

    let mut substituted_args = Vec::new();

    for arg_template in &tool.args {
        let substituted = substitute_params(arg_template, params)?;
        substituted_args.push(substituted);
    }

    // substituted_args = ["dispatch", "workspace", "3"]

    log(&format!("Executing CLI tool: {} {:?}", tool.command, substituted_args));
    // Logs: "Executing CLI tool: hyprctl [\"dispatch\", \"workspace\", \"3\"]"

    let output = Command::new(&tool.command)  // "hyprctl"
        .args(&substituted_args)              // ["dispatch", "workspace", "3"]
        .output()?;

    // This runs: hyprctl dispatch workspace 3

    if output.status.success() {
        Ok(format!("Success: {}", String::from_utf8_lossy(&output.stdout).trim()))
    } else {
        Err(format!("Command failed: {}", String::from_utf8_lossy(&output.stderr)).into())
    }
}
```

### The Substitution Function

**File:** `src/tools.rs`, function `substitute_params()`

```rust
// src/tools.rs:105
fn substitute_params(template: &str, params: &serde_json::Value) -> Result<String, Box<dyn Error>> {
    // template = "{workspace}"
    // params = { "workspace": 3 }

    let mut result = template.to_string();
    let re = regex::Regex::new(r"\{(\w+)\}")?;  // Match {anything}

    for cap in re.captures_iter(template) {
        let param_name = &cap[1];           // "workspace"
        let placeholder = &cap[0];          // "{workspace}"

        let value = params.get(param_name)  // Get the value: 3
            .ok_or(format!("Missing parameter: {}", param_name))?;

        // Convert JSON value to string
        let value_str = match value {
            serde_json::Value::Number(n) => n.to_string(),  // 3 → "3"
            serde_json::Value::String(s) => s.clone(),
            // ... other types
        };

        result = result.replace(placeholder, &value_str);
        // "{workspace}" → "3"
    }

    Ok(result)  // "3"
}
```

**Result:** `["dispatch", "workspace", "{workspace}"]` becomes `["dispatch", "workspace", "3"]`

---

## Part 10: Running the Shell Command

```rust
Command::new("hyprctl")
    .args(&["dispatch", "workspace", "3"])
    .output()?;
```

This is exactly equivalent to running in your terminal:

```bash
hyprctl dispatch workspace 3
```

Hyprland switches to workspace 3. The command succeeds.

---

## Part 11: Returning to the CLI

**File:** `src/bin/cli.rs`, function `handle_stop()`, lines 267-276

```rust
// src/bin/cli.rs:268
if tool_was_called {
    // Show notification only, skip clipboard and paste
    notifications::notify("✅ Tool executed", &text, 3000);
} else {
    // Normal flow: clipboard → paste
    clipboard::copy_to_clipboard(&text);
    paste::paste_from_clipboard();
}
```

**Because `tool_was_called = true`**, the system shows a notification instead of pasting text.

---

## The Complete Flow Summary

```
1. Load tools.json → Vec<ToolConfig>

2. User speaks: "Switch to workspace 3"
   Parakeet transcribes: "Switch to workspace 3"

3. Convert ToolConfig → Kimi tool definitions (JSON schema)

4. Send to Groq API:
   - Messages: [system, user_with_transcription]
   - Tools: [switch_workspace, turn_leds_on, ...]
   - tool_choice: "auto"

5. Kimi decides: "This is switch_workspace"
   Kimi extracts: {"workspace": 3}
   Kimi returns: finish_reason="tool_calls"

6. Rust code detects tool call, parses arguments

7. Find ToolConfig for "switch_workspace"

8. Substitute parameters:
   ["{workspace}"] + {"workspace": 3} → ["3"]

9. Run command:
   hyprctl dispatch workspace 3

10. Show notification, skip paste
```

---

## Why This Design is Brilliant

### What the LLM Does (Hard Part)
- Natural language understanding ("workspace 3" → tool call)
- Parameter extraction (3 from "workspace 3")
- Handling variations ("go to workspace 3", "switch to workspace three")

### What Our Code Does (Easy Part)
- Load JSON config
- Format requests for Kimi API
- Simple string replacement (`{workspace}` → `3`)
- Execute shell commands

**The complexity is outsourced to the LLM.** Our code is almost embarrassingly simple because Kimi does all the hard work.

---

## Why You Thought It Couldn't Work

You probably expected:
- Pattern matching: `if text.contains("workspace")`
- Regex parsing: `/workspace (\d+)/`
- Manual parameter extraction

**None of that exists!** The LLM reads natural language descriptions and JSON schemas, makes intelligent decisions, and extracts parameters. We just execute what it tells us.

---

## Adding New Tools

Now you can see why adding tools is trivial:

```json
{
  "name": "open_file",
  "description": "Open a file in the editor. Call when user says 'open file...', 'edit...'",
  "command": "code",
  "args": ["{filepath}"],
  "parameters": {
    "filepath": {
      "type": "string",
      "description": "Path to the file to open"
    }
  }
}
```

**User says:** "Open file config.json"

**Kimi extracts:** `{"filepath": "config.json"}`

**System runs:** `code config.json`

No code changes needed!

---

## The Role of the System Prompt

**File:** `~/.config/transcribe-rs/prompts/clean.md`

The first lines are critical:

```
Your Task: First, check if the user is requesting an action that matches
one of your available tools (e.g., turning LEDs on/off, controlling lights
or display background). If so, call the appropriate tool function.

If the user is NOT requesting a tool action, then clean the spoken dictation...
```

This tells Kimi: "**Tool calls first, text cleaning second.**" Without this, Kimi might default to just cleaning the text instead of recognizing tool calls.

---

## Debugging Tips

**Check the logs:** `/tmp/ptt_rust_debug.log`

You'll see:
```
[2025-11-06 ...] [groq] Loaded 4 tools from config
[2025-11-06 ...] [groq] Tools count: 4
[2025-11-06 ...] [groq] finish_reason: Some("tool_calls")
[2025-11-06 ...] [groq] Found 1 tool call(s)
[2025-11-06 ...] [groq] Executing tool: switch_workspace with args: {"workspace":3}
[2025-11-06 ...] [tools] Executing CLI tool: hyprctl ["dispatch", "workspace", "3"]
[2025-11-06 ...] [tools] Tool executed successfully: switch_workspace
```

This shows the entire flow in real-time.

---

## Key Takeaways

1. **LLM tool calling is not pattern matching.** It's the LLM reading descriptions and making decisions.

2. **Parameters are extracted by the LLM**, not parsed by regex. Kimi sees "workspace 3" and knows `workspace=3`.

3. **Our code is deliberately simple.** We load config, format API calls, substitute strings, run commands.

4. **The JSON schema teaches the LLM.** Good `description` fields = better tool selection.

5. **No recompilation needed.** Add tools to JSON, restart the daemon, done.

---

## Why It "Just Works"

You thought it couldn't work because you expected traditional programming:
- Parse input
- Extract parameters with regex
- Match patterns

Instead, we use **LLM-as-a-service:**
- Describe tools in natural language
- Let the LLM do all the understanding
- We just execute what it decides

**The LLM is the brain. Our code is the hands.**
