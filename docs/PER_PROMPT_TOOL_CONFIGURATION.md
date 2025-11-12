# Per-Prompt Tool Configuration

**Status:** Implemented (2025-11-12)

## Overview

The per-prompt tool configuration system allows different prompts to use different sets of tools. This enables separation of concerns: some prompts (like "ask") can be for pure conversation without tool execution, while others (like "clean") can have access to specific tools.

## Problem Solved

**Before:** All prompts received ALL tools from `~/.config/transcribe-rs/tools.json` regardless of use case. This meant:
- Conversational prompts ("ask") could accidentally trigger tool execution
- Large context sent to LLM with tools that weren't relevant
- No way to separate conversation from actions

**After:** Each prompt can specify which tool set it should use:
- "ask" prompt → no tools (pure Q&A)
- "clean" prompt → smart home tools only
- "control" prompt → window management tools only

## Architecture

### Configuration Structure

Two new fields in `[llm]` section of `config.toml`:

1. **`tool_sets`** - Named groups of tools
2. **`prompt_tool_mapping`** - Maps prompt names to tool set names

### Flow

```
User: transcribe stop ask
         ↓
CLI loads config.toml
         ↓
Looks up "ask" in prompt_tool_mapping → "none"
         ↓
Looks up "none" in tool_sets → []
         ↓
Creates GroqClient with empty tool list
         ↓
LLM processes with NO tools available
```

## Configuration Format

### Example config.toml

```toml
[llm]
conversation_history_enabled = true
conversation_history_prompts = ["ask"]
conversation_history_clear_word = "clear"
conversation_history_minutes = 5
conversation_max_turns = 10
conversation_history_dir = "/tmp"

# Define named tool sets (each is a list of tool names from tools.json)
[llm.tool_sets]
none = []
smart_home = ["turn_leds_on", "turn_leds_off"]
hyprland = ["switch_workspace", "focus_window", "move_to_workspace"]
all = ["turn_leds_on", "turn_leds_off", "switch_workspace", "focus_window"]

# Map each prompt name to a tool set
[llm.prompt_tool_mapping]
ask = "none"              # Q&A only, no actions
clean = "smart_home"      # Dictation with smart home control
control = "hyprland"      # Window management
orchestrator = "all"      # Everything
```

### Default Configuration

If you don't add these sections, the system uses these defaults:

```toml
[llm.tool_sets]
none = []
all = []

[llm.prompt_tool_mapping]
ask = "none"
clean = "all"
```

**Note:** The default "all" set is empty until you populate it with tool names.

## Usage

### Basic Usage

1. **Define your tools** in `~/.config/transcribe-rs/tools.json` (as before):
   ```json
   {
     "tools": [
       {
         "name": "turn_leds_on",
         "description": "Turn on the display background LEDs",
         "backend": "http",
         "http": {
           "url": "http://192.168.2.40/json/state",
           "method": "POST",
           "body": "{\"on\": true}"
         },
         "parameters": {}
       },
       {
         "name": "switch_workspace",
         "description": "Switch to a different workspace. Call when user says 'go to workspace N'",
         "backend": "cli",
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

2. **Create tool sets** in `config.toml` by referencing tool names:
   ```toml
   [llm.tool_sets]
   none = []
   lighting = ["turn_leds_on", "turn_leds_off"]
   windows = ["switch_workspace", "focus_window"]
   ```

3. **Map prompts to tool sets**:
   ```toml
   [llm.prompt_tool_mapping]
   ask = "none"
   clean = "lighting"
   wm = "windows"
   ```

4. **Use the prompts**:
   ```bash
   # Pure conversation (no tools)
   transcribe stop ask

   # Dictation with lighting control
   transcribe stop clean

   # Window management commands
   transcribe stop wm
   ```

### Behavior Details

- **Prompt not in mapping:** Defaults to no tools (safe default)
- **Tool set not found:** Falls back to no tools with warning in logs
- **Tool name not in tools.json:** Loads other tools, warns about missing ones in `/tmp/ptt_rust_debug.log`
- **Empty tool set:** Works fine, client gets no tools

### Checking What Tools Are Loaded

Watch the debug log while using different prompts:

```bash
tail -f /tmp/ptt_rust_debug.log
```

You'll see entries like:
```
[2025-11-12 16:20:15.123] [cli] Prompt 'ask' mapped to tool set 'none'
[2025-11-12 16:20:15.124] [cli] Tool set 'none' is empty, creating client with no tools
[2025-11-12 16:20:15.125] [groq] Loaded 0 tools from tool set
```

Or:
```
[2025-11-12 16:22:30.456] [cli] Prompt 'clean' mapped to tool set 'smart_home'
[2025-11-12 16:22:30.457] [cli] Tool set 'smart_home' contains 2 tools, loading them
[2025-11-12 16:22:30.458] [tools] Loaded 2 tools from tool set (requested: 2)
[2025-11-12 16:22:30.459] [groq] Loaded 2 tools from tool set
```

## Migration Guide

### From Previous Version

**No breaking changes!** Your existing setup will work with defaults.

To opt into per-prompt tool configuration:

1. **Identify your tools** - Check `~/.config/transcribe-rs/tools.json`
   ```bash
   cat ~/.config/transcribe-rs/tools.json | jq '.tools[].name'
   ```

2. **Add to config.toml**:
   ```bash
   nano ~/.config/transcribe-rs/config.toml
   ```

3. **Create logical groupings**:
   ```toml
   [llm.tool_sets]
   none = []
   home = ["turn_leds_on", "turn_leds_off", "set_thermostat"]
   desktop = ["switch_workspace", "focus_window", "move_window"]
   all = ["turn_leds_on", "turn_leds_off", "switch_workspace", "focus_window"]
   ```

4. **Map your prompts**:
   ```toml
   [llm.prompt_tool_mapping]
   ask = "none"
   clean = "all"
   smart_home = "home"
   hyprland = "desktop"
   ```

5. **Test each prompt** and verify behavior in debug log

### Creating New Prompts

When adding a new prompt file in `~/.config/transcribe-rs/prompts/`:

1. **Create the prompt** (e.g., `coffee.md`):
   ```markdown
   You are a coffee machine controller. When the user asks to make coffee,
   use the make_coffee tool. Otherwise, just respond conversationally.
   ```

2. **Add coffee tools to tools.json**:
   ```json
   {
     "name": "make_coffee",
     "description": "Brew a cup of coffee",
     ...
   }
   ```

3. **Create tool set in config.toml**:
   ```toml
   [llm.tool_sets]
   coffee = ["make_coffee", "check_water_level"]
   ```

4. **Map prompt to tool set**:
   ```toml
   [llm.prompt_tool_mapping]
   coffee = "coffee"
   ```

5. **Use it**:
   ```bash
   transcribe stop coffee
   # Say: "Make me a cappuccino"
   ```

## Best Practices

### Tool Set Organization

**Recommended approach:**

```toml
[llm.tool_sets]
none = []                                    # No tools
lights = ["turn_leds_on", "turn_leds_off"]  # Single domain
wm = ["switch_workspace", "focus_window"]   # Single domain
home_all = ["turn_leds_on", "...", "..."]   # Multiple domains for power user
```

**Why?**
- Smaller context = faster LLM responses
- Less confusion for model (fewer irrelevant tools)
- Clearer separation of concerns

### Prompt Naming

- **ask** - Pure conversation, no actions
- **clean** - Text cleaning/formatting, optional tools
- **{domain}** - Domain-specific actions (e.g., "lights", "windows", "coffee")
- **orchestrator** - If you want one prompt that can do everything

### Testing

After configuration changes:

1. **Test each prompt** with simple commands
2. **Check logs** for tool loading messages
3. **Verify behavior** - does "ask" avoid tool calls? Does "lights" execute tools?

## Troubleshooting

### Prompt uses wrong tools

**Check:**
1. Is prompt name spelled correctly in `prompt_tool_mapping`?
2. Does tool set name exist in `tool_sets`?
3. Are tool names spelled correctly (match tools.json)?

**Debug:**
```bash
tail -f /tmp/ptt_rust_debug.log
```

### Tools not found warning

**Message in log:**
```
Warning: 2 tools not found in config: ["switch_workspace", "foo"]
```

**Fix:**
- Check spelling in tool set definition
- Verify tools exist in `~/.config/transcribe-rs/tools.json`
- Tool names are case-sensitive

### Prompt not in mapping

**Behavior:** Defaults to no tools (safe fallback)

**To fix:** Add prompt to `prompt_tool_mapping`:
```toml
[llm.prompt_tool_mapping]
my_prompt = "none"  # or appropriate tool set
```

## Implementation Details

### Code Changes

- **src/config.rs** - Added `tool_sets` and `prompt_tool_mapping` to `LlmConfig`
- **src/tools.rs** - Added `load_tool_set()` function for filtering tools
- **src/groq.rs** - Added `from_env_file_with_tool_set()` constructor
- **src/bin/cli.rs** - Modified `process_with_groq()` to lookup and load tool sets

### Backward Compatibility

- Existing configs without new fields use sensible defaults
- All three client constructors still work:
  - `from_env_file()` - loads all tools (backward compatible)
  - `from_env_file_no_tools()` - loads no tools
  - `from_env_file_with_tool_set(names)` - loads specific tools (new)

## Examples

### Example 1: Simple Q&A + Smart Home

**config.toml:**
```toml
[llm.tool_sets]
none = []
lights = ["turn_leds_on", "turn_leds_off"]

[llm.prompt_tool_mapping]
ask = "none"
lights = "lights"
```

**Usage:**
```bash
# Conversation
transcribe stop ask
# Say: "What's the weather like?"
# → Text response only

# Control lights
transcribe stop lights
# Say: "Turn on the lights"
# → Executes turn_leds_on tool
```

### Example 2: Multiple Domains

**config.toml:**
```toml
[llm.tool_sets]
none = []
home = ["turn_leds_on", "turn_leds_off", "set_temp"]
desktop = ["switch_workspace", "launch_app"]
all = ["turn_leds_on", "turn_leds_off", "set_temp", "switch_workspace", "launch_app"]

[llm.prompt_tool_mapping]
ask = "none"
home = "home"
work = "desktop"
power = "all"
```

**Usage:**
```bash
transcribe stop ask    # Conversation only
transcribe stop home   # Smart home control
transcribe stop work   # Desktop automation
transcribe stop power  # Everything
```

### Example 3: Single Orchestrating Prompt

**config.toml:**
```toml
[llm.tool_sets]
everything = [
  "turn_leds_on",
  "turn_leds_off",
  "switch_workspace",
  "launch_app",
  "make_coffee"
]

[llm.prompt_tool_mapping]
clean = "everything"
```

**prompts/clean.md:**
```markdown
You are a helpful assistant. First determine if the user wants:
1. Tool action (lights, windows, coffee) → use appropriate tool
2. Text cleanup/formatting → clean the text
3. Question/conversation → respond naturally

Always be context-aware and use tools when appropriate.
```

**Usage:**
```bash
transcribe stop clean
# Say: "Turn off lights" → Tool execution
# Say: "Fix this text" → Text cleanup
# Say: "What time is it?" → Conversation
```

## Future Enhancements

Potential improvements (not yet implemented):

- **Prompt frontmatter** - Define tools in prompt file itself
- **Tool inheritance** - Tool sets that extend other tool sets
- **Dynamic tool discovery** - Auto-load tools from directory
- **Per-user tool sets** - Different configurations per user
- **Tool categories** - Organize tools into categories for easier management

## See Also

- [Tool Calling Implementation](./tool-calling-implementation.md) - Original tool calling architecture
- [Tools Configuration](../tools/) - Example tool definitions
- [Prompts Guide](../prompts/) - Creating custom prompts
