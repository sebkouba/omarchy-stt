# Tool Calling Implementation

## Overview

The transcribe-rs-v2 system integrates Kimi K2 tool calling to enable voice commands that trigger physical actions (like controlling LED lights) instead of pasting text. When a tool is called, the system shows a notification rather than pasting the response into the active window.

## Architecture

### High-Level Flow

```
User dictation → Parakeet transcription → Groq/Kimi processing
                                                  ↓
                                    ┌─────────────┴──────────────┐
                                    │                            │
                              Tool call?                    No tool call?
                                    │                            │
                                    ↓                            ↓
                          Execute HTTP API              Clean text formatting
                                    │                            │
                                    ↓                            ↓
                          Show notification only          Paste into window
```

### Key Components

1. **`src/groq.rs`** - Groq API client with tool calling support
2. **`src/bin/cli.rs`** - CLI integration that routes tool calls vs text paste
3. **`~/.config/transcribe-rs/prompts/clean.md`** - System prompt that enables both cleaning and tools

## Implementation Details

### 1. Groq Client (`src/groq.rs`)

**Core Types:**

```rust
pub struct CompletionResult {
    pub text: String,
    pub tool_called: bool,  // NEW: tracks if a tool was executed
}
```

**Tool Definition Structure:**

Tools are currently **hardcoded** in `GroqClient`:
- `create_led_on_tool()` - Returns Tool struct for turning LEDs on
- `create_led_off_tool()` - Returns Tool struct for turning LEDs off

Each tool has:
- `name`: Function identifier (e.g., "turn_leds_on")
- `description`: When to use it (critical for model decision-making)
- `parameters`: JSON schema (currently empty for LED tools)

**Tool Execution:**

The `execute_tool()` method maps function names to implementations:
- `"turn_leds_on"` → `turn_leds_on()` → HTTP POST to LED API
- `"turn_leds_off"` → `turn_leds_off()` → HTTP POST to LED API

**API Endpoint:** Hardcoded as `http://192.168.2.40/json/state` (see `LED_API_URL` constant)

**Tool Calling Loop:**

Located in `complete_async()` method (lines ~191-290):
1. Send request with tools array
2. Check `finish_reason`:
   - `"tool_calls"` → Execute tool, append result, continue loop
   - `"stop"` → Return final text with `tool_called` flag
3. Track `tool_was_called` throughout iterations
4. Return `CompletionResult { text, tool_called }`

### 2. CLI Integration (`src/bin/cli.rs`)

**Processing Flow:**

Located in `handle_stop()` function (~lines 221-327):

```rust
// 1. Process with Groq (if prompt enabled)
match process_with_groq(&transcription, prompt_name, log_file) {
    Ok(result) => {
        tool_was_called = result.tool_called;
        // ...
    }
}

// 2. Branch on tool_called flag
if tool_was_called {
    // Show notification only, skip clipboard and paste
    notifications::notify("✅ Tool executed", &text, 3000);
} else {
    // Normal flow: clipboard → paste
    clipboard::copy_to_clipboard(&text);
    paste::paste_from_clipboard();
}
```

**Key Decision Point:** Line ~268 checks `tool_was_called` to determine behavior

### 3. System Prompt

**Location:** `~/.config/transcribe-rs/prompts/clean.md`

**Critical First Lines:**
```
Your Task: First, check if the user is requesting an action that matches
one of your available tools (e.g., turning LEDs on/off, controlling lights
or display background). If so, call the appropriate tool function.

If the user is NOT requesting a tool action, then clean the spoken dictation...
```

**Why This Matters:**
- Without tool-first instruction, model defaults to text cleaning
- Prompt structure affects Groq's tool call parser (see `docs/lessons-learned/kimi-tool-calling.md`)

## Current Limitations

### 1. Hardcoded Tools

**Problem:** All tools are defined in source code:
- Tool definitions: `src/groq.rs` lines ~293-309
- Tool execution: `src/groq.rs` lines ~312-318
- API endpoints: Hardcoded constants

**To add a coffee machine:**
- Requires code changes in `groq.rs`
- Requires recompilation
- Cannot be configured per-user

### 2. Hardcoded API Endpoint

`LED_API_URL` constant points to specific IP address. No configuration file support.

### 3. Single Tool Set

All clients share the same tools. No per-prompt or per-user tool configuration.

## Configuration Files

**Current:**
- `~/.config/transcribe-rs/.env` - Groq API key only
- `~/.config/transcribe-rs/prompts/*.md` - Text processing prompts

**Not Yet Supported:**
- Tool definitions
- API endpoint configuration
- Tool parameters/schemas

## Key Source References

### Tool Definition & Execution
- `src/groq.rs:293-371` - Tool creation and execution methods
- `src/groq.rs:191-290` - Tool calling loop in `complete_async()`
- `src/groq.rs:38-43` - `CompletionResult` type

### CLI Integration
- `src/bin/cli.rs:221-327` - Tool call vs paste branching logic
- `src/bin/cli.rs:553-566` - `process_with_groq()` wrapper
- `src/bin/cli.rs:267-276` - Tool call notification path

### Documentation
- `docs/kimi_tool_call.md` - Kimi K2 official tool calling guide
- `docs/lessons-learned/kimi-tool-calling.md` - Implementation learnings
- `specs/groq_curl.md` - API examples

## Testing

**Manual test:** `examples/test_tool_calling.rs`
- Simple prompt that should trigger tools
- Verifies `tool_called` flag
- Logs full request/response

**Integration test:** `src/groq.rs:498-518` (currently `#[ignore]`)
- Requires valid API key and LED device
- Tests end-to-end tool calling

## Tool Calling Behavior Notes

### Model Decision Making

The model autonomously decides whether to use tools based on:
1. **Tool descriptions** - Must include explicit trigger phrases
2. **User message context** - "turn off LEDs" vs "clean this text"
3. **System prompt** - Tool-first vs text-first instructions

Example of explicit description (from `src/groq.rs:301`):
```rust
"Turn off the display background LEDs. Call this tool when the user asks
to turn off, disable, deactivate, switch off, or extinguish the LEDs..."
```

### Groq API Quirks

- Sometimes returns raw Kimi format as error: `<|tool_calls_section_begin|>`
- Intermittent parser failures with longer prompts (see lessons-learned doc)
- Works reliably with concise prompts and explicit tool descriptions

## Future Enhancements (Not Implemented)

### Design Decisions from Discussion

**Execution Backends:**
- **CLI commands** (primary): Execute via subprocess (e.g., `hyprctl dispatch workspace 3`)
- **HTTP APIs** (current): Keep existing LED API support
- **Unix sockets**: Future consideration

**Tool Organization:**
- **Per-prompt tool sets**: Different prompts load different tools from separate JSON files
  - Example: `window-control.md` loads `tools/hyprland.json`
  - Example: `smart-home.md` loads `tools/leds.json`
- **Orchestrating prompt option**: Single prompt that routes between simple transcription and tool categories

**Parameter Handling:**
- **LLM extraction**: Model extracts parameters from natural language
  - User says: "switch to workspace 3"
  - LLM extracts: `{workspace: 3}`
  - System substitutes into template: `hyprctl dispatch workspace {workspace}`

### Proposed Configuration Structure

```
~/.config/transcribe-rs/
├── prompts/
│   ├── clean.md              # Text cleaning only
│   ├── window-control.md     # Hyprland tools (per-prompt)
│   ├── smart-home.md         # LED/device tools (per-prompt)
│   └── orchestrator.md       # Routes to tools or cleaning (all-in-one)
└── tools/
    ├── hyprland.json         # Window management via hyprctl
    ├── leds.json             # LED control via HTTP
    └── coffee.json           # Future: coffee machine control
```

### Tool Definition Format (JSON)

```json
{
  "tools": [
    {
      "name": "switch_workspace",
      "description": "Switch to workspace N. Call when user says 'go to workspace 3', 'switch to workspace 5', etc.",
      "backend": "cli",
      "cli": {
        "command": "hyprctl",
        "args": ["dispatch", "workspace", "{workspace}"]
      },
      "parameters": {
        "workspace": {
          "type": "integer",
          "description": "Target workspace number (1-10)"
        }
      }
    },
    {
      "name": "turn_leds_off",
      "description": "Turn off display LEDs. Call when user asks to turn off, disable, deactivate lights.",
      "backend": "http",
      "http": {
        "url": "http://192.168.2.40/json/state",
        "method": "POST",
        "body": {"on": false}
      },
      "parameters": {}
    }
  ]
}
```

### Implementation Strategy

**Option A: Per-Prompt Tools (Recommended Start)**
- Each prompt file declares which tool configs to load
- Simpler model context (fewer tools per request)
- Clear separation of concerns
- Example frontmatter:
  ```markdown
  ---
  tools: ["hyprland"]
  ---
  Control Hyprland windows. When user requests window/workspace actions, use tools...
  ```

**Option B: Orchestrating Prompt (Single Entry Point)**
- One prompt loads ALL tool configs
- First checks if request is tool-related or text transcription
- Routes accordingly
- Pros: Single workflow, no prompt switching
- Cons: Larger context, model may confuse tool categories

**Hybrid Approach (Try First):**
- Start with all tools in one orchestrating prompt
- If context gets too large or model gets confused, split into per-prompt tools
- Can migrate without breaking existing setup

### Required Code Changes

1. **New Module: `src/tools/`**
   - `config.rs` - Load and parse JSON tool definitions
   - `types.rs` - Tool structs (ToolConfig, Backend, Parameters)
   - `executor.rs` - Execute CLI/HTTP tools with parameter substitution
   - `mod.rs` - Public API for tool system

2. **Extend `src/prompts.rs`**
   - Parse YAML frontmatter in prompt files (optional, for per-prompt tools)
   - Extract `tools: [...]` array
   - Load corresponding JSON files from `~/.config/transcribe-rs/tools/`

3. **Refactor `src/groq.rs`**
   - Remove hardcoded tool methods (`create_led_on_tool`, etc.)
   - Add `GroqClient::new_with_tools(api_key, tool_configs: Vec<ToolConfig>)`
   - Generate Kimi tool definitions dynamically from ToolConfig
   - Route `execute_tool()` to generic `tools::executor::execute()`

4. **Update `src/bin/cli.rs`**
   - Load prompt → parse tool list → load tool configs
   - Pass tools to GroqClient constructor
   - Keep existing `tool_called` branching (no changes needed)

### Example: Hyprland Tool in Action

**User says:** "Switch to workspace 3"

**Flow:**
1. Parakeet transcribes: "Switch to workspace 3"
2. Groq/Kimi receives tools array including `switch_workspace`
3. Model decides to call `switch_workspace` with `{workspace: 3}`
4. Executor runs: `hyprctl dispatch workspace 3`
5. CLI shows notification: "✅ Tool executed: Switched to workspace 3"
6. No paste occurs (existing behavior)

### Migration Path

**Phase 1: Extract Current Tools**
- Create `~/.config/transcribe-rs/tools/leds.json`
- Move LED tool definitions from code to JSON
- Implement JSON loader and CLI/HTTP executors
- Test with existing LED tools (no new functionality)

**Phase 2: Add Hyprland Tools**
- Create `tools/hyprland.json` with workspace/window tools
- Test with orchestrating prompt (all tools in one)
- Measure if context size or confusion becomes issue

**Phase 3: Optimize (If Needed)**
- Split into per-prompt tools if orchestrating prompt struggles
- Add tool categories/namespacing
- Implement caching/validation

### Open Questions

1. **Tool discovery**: Should prompts auto-discover all JSON files in `tools/`, or explicitly list them?
2. **Error handling**: What if CLI command fails? Return error to LLM for retry?
3. **Security**: Validate/sanitize parameters before shell execution?
4. **Tool response format**: Should CLI stdout be parsed (JSON/text) or just return success/fail?

See lessons-learned document for implementation pitfalls to avoid.
