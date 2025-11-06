# Lessons Learned: Kimi K2 Tool Calling Implementation

## What We Tried

1. **async-openai library (v0.29.3 → v0.30.1)**
   - Standard OpenAI-compatible client
   - Expected drop-in compatibility with Groq API

2. **Custom reqwest implementation**
   - Raw HTTP requests with manual JSON serialization
   - Custom request/response types

3. **System prompt modifications**
   - Tried adding tool instructions to system prompt
   - Attempted to override user prompt context

4. **Tool description approaches**
   - Generic descriptions: "Turn on the display background LEDs"
   - Explicit descriptions: "Call this tool when user asks to turn on, enable, activate..."

5. **Debug logging**
   - Added comprehensive request/response logging
   - Logged finish_reason, tool_calls, and execution results

## What Worked ✓

### 1. Raw reqwest Instead of async-openai
**Why**: Groq returns `service_tier: "on_demand"` but async-openai only recognizes OpenAI's standard values (`auto`, `default`, `flex`, `scale`)
```
Error: unknown variant `on_demand`, expected one of `scale`, `default`, `flex`, `priority`
```
**Solution**: Custom structs with lenient deserialization that skip unknown fields

### 2. Following Kimi Docs Exactly
**Critical requirements**:
- Check `finish_reason == "tool_calls"` (not just presence of tool_calls field)
- Include `name` field in tool result messages:
  ```rust
  Message {
      role: "tool",
      tool_call_id: Some(tool_call.id),
      name: Some(function_name),  // ← REQUIRED
      content: Some(json_result)
  }
  ```
- Append complete assistant message with tool_calls before sending tool results

### 3. Explicit Tool Descriptions
**Pattern from Kimi docs**:
```
"description": "
    [What it does]

    Call this tool when [specific trigger phrases].
    [What parameters it needs]
"
```

**Our working example**:
```rust
"Turn on the display background LEDs. Call this tool when the user asks
to turn on, enable, activate, switch on, or light up the LEDs, lights,
or display background. This function takes no parameters and will
physically turn on the LED hardware."
```

### 4. Debug Logging
Essential for diagnosing issues:
- Log complete request JSON (shows if tools are included)
- Log finish_reason (shows if model wants to use tools)
- Log tool_calls array (shows what model is trying to call)
- Log tool execution results

## What Didn't Work ✗

### 1. async-openai Library
- **Issue**: Groq-specific response fields break deserialization
- **Lesson**: OpenAI-compatible ≠ fully compatible for all providers

### 2. Vague Tool Descriptions
- **Example**: "Turn on the display background LEDs"
- **Issue**: Model doesn't know WHEN to use the tool
- **Lesson**: Descriptions need explicit trigger conditions

### 3. Relying on System Prompt for Tool Guidance
- **Tried**: Adding "You have tools available, use them when needed" to system prompt
- **Issue**: User message context (e.g., "clean this text") overrides system guidance
- **Lesson**: Tool descriptions are more important than system prompt for tool selection

### 4. Assuming Model Would "Just Know" to Use Tools
- **Reality**: Model autonomously decides based on:
  1. Tool descriptions (WHEN to use them)
  2. User message context
  3. Conversation history
- **Lesson**: If tools aren't being used, model is making a rational decision based on insufficient information

## Key Insights

1. **Service Tier Incompatibility**: Groq's `service_tier` field uses different values than OpenAI, breaking typed clients

2. **Model Autonomy**: Kimi decides whether to use tools - even with `tool_choice: "auto"`, it can choose not to use them

3. **Context Matters**: A prompt saying "only return corrected text" will prevent tool usage even if tools are available

4. **Description Quality**: The quality and specificity of tool descriptions directly impacts usage frequency

5. **Debugging Strategy**:
   - First verify tools are in request → check serialized JSON
   - Then check model response → look at finish_reason
   - Finally examine tool execution → log results

## Implementation Details

### Request Format
```json
{
  "model": "moonshotai/kimi-k2-instruct-0905",
  "messages": [...],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "turn_leds_on",
        "description": "...",
        "parameters": {...}
      }
    }
  ],
  "tool_choice": "auto"
}
```

### Response Format (When Tool Call Requested)
```json
{
  "choices": [{
    "finish_reason": "tool_calls",  // ← KEY INDICATOR
    "message": {
      "role": "assistant",
      "content": "",
      "tool_calls": [{
        "id": "call_xyz",
        "type": "function",
        "function": {
          "name": "turn_leds_on",
          "arguments": "{}"
        }
      }]
    }
  }]
}
```

### Tool Calling Loop Pattern
```rust
while finish_reason.as_deref() == Some("tool_calls") {
    // 1. Send request with tools
    // 2. Check finish_reason
    // 3. If "tool_calls": append assistant message → execute tools → append tool results → loop
    // 4. If "stop": return content
}
```

## Files Modified
- `src/groq.rs` - Complete rewrite (465 lines)
  - Custom request/response types
  - Lenient deserialization
  - Tool calling loop
  - LED API integration
  - Debug logging

- `Cargo.toml` - Dependencies
  - Added `reqwest = "0.12"` for LED HTTP calls
  - Already had `tokio` and `async-openai` (now unused)

## References
- Kimi K2 tool calling docs: `docs/kimi_tool_call.md`
- Groq API docs: https://console.groq.com/docs/api-reference
- Original implementation: `src/groq.rs` (ureq-based, no tool support)
