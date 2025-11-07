# Integration Example

How to integrate the claude_worktree tool into your dictation app.

## Step 1: Copy Tool Definition

Copy one of the JSON tool definitions to your dictation app's tools directory:

```bash
# Option A: Simple (single project)
cp tools/claude_worktree_simple.json ~/path/to/your/dictation-config/tools/claude_worktree.json

# Option B: Multi-project
cp tools/claude_worktree_multi.json ~/path/to/your/dictation-config/tools/claude_worktree.json
```

## Step 2: Verify Paths

Make sure all paths in the JSON are absolute and correct:

```json
{
  "command": "/home/seb/code/cloned/transcribe-rs-v2/tools/claude_worktree_wrapper.sh",
  "args": ["/home/seb/code/cloned/transcribe-rs-v2", ...]
}
```

## Step 3: Test Manually

Before trying with voice, test the command manually:

```bash
# Test without launching terminal (uses --no-terminal flag)
/home/seb/code/cloned/transcribe-rs-v2/tools/test_tool.sh
```

## Step 4: Reload Your Dictation App

Restart or reload your dictation app to pick up the new tool.

## Step 5: Try Voice Commands

Examples of what to say:

### Simple Commands (for simple tool)
- **"create worktree for adding LED status"**
  - branch_name: "add-led-status"
  - task_description: "Add LED status indicator"

- **"new branch for fixing audio clipping bug"**
  - branch_name: "fix-audio-clipping-bug"
  - task_description: "Fix audio clipping bug"

- **"claude work on improving timestamp accuracy"**
  - branch_name: "improve-timestamp-accuracy"
  - task_description: "Improve timestamp accuracy"

### With Workspace (if your LLM extracts it)
- **"create worktree for adding logging on workspace 3"**
  - branch_name: "add-logging"
  - task_description: "Add logging functionality"
  - workspace: "3"

### Multi-Project Commands (for multi-project tool)
- **"create worktree in transcribe for adding feature X"**
  - project_path: "/home/seb/code/cloned/transcribe-rs-v2"

- **"new branch in transcribe v1 for fixing bug Y"**
  - project_path: "/home/seb/code/cloned/transcribe-rs"

## How the LLM Should Process This

Your LLM tool calling system should:

1. **Detect the intent** from keywords like:
   - "create worktree"
   - "new branch"
   - "claude work on"
   - "start working on"

2. **Extract parameters:**
   - **Branch name:** Convert task to kebab-case
     - "adding LED status" → "add-led-status"
     - "fixing audio bug" → "fix-audio-bug"
   - **Task description:** Use the user's words
     - "adding LED status" → "Add LED status indicator that shows when recording is active"
   - **Workspace:** Extract numbers if mentioned
     - "on workspace 3" → "3"
   - **Project:** Extract project name if mentioned (multi-project only)
     - "in transcribe v1" → "/home/seb/code/cloned/transcribe-rs"

3. **Call the tool** with these parameters:
   ```json
   {
     "name": "claude_worktree",
     "parameters": {
       "branch_name": "add-led-status",
       "task_description": "Add LED status indicator that shows when recording is active",
       "workspace": ""
     }
   }
   ```

## Expected Behavior

When successful:
1. Terminal window opens on specified workspace (or current)
2. Shows task description briefly
3. Launches Claude Code
4. Claude Code reads `.claude-task.md` and starts working
5. Original terminal shows: "Worktree created: /path/to/worktree"

## Troubleshooting Integration

### Tool Not Found
```bash
# Check if the tool definition is in the right place
ls -la ~/path/to/dictation-config/tools/

# Make sure wrapper script is executable
ls -la /home/seb/code/cloned/transcribe-rs-v2/tools/claude_worktree_wrapper.sh
```

### Command Fails
```bash
# Test the wrapper directly
/home/seb/code/cloned/transcribe-rs-v2/tools/claude_worktree_wrapper.sh \
    /home/seb/code/cloned/transcribe-rs-v2 \
    "test-branch" \
    "Test task description" \
    --no-terminal

# Check logs in your dictation app
# The error output will tell you what went wrong
```

### LLM Not Generating Good Branch Names
Update the parameter description in the JSON to be more specific:

```json
"branch_name": {
  "type": "string",
  "description": "Git branch name in kebab-case. Examples: 'add-led-status' (not 'adding-led-status'), 'fix-audio-bug' (not 'fix audio bug'), 'update-readme' (not 'update-README'). Use imperative form: add, fix, update, remove, etc."
}
```

### Workspace Not Working
Make sure you're on Hyprland and have `hyprctl` available:

```bash
# Test workspace switching
hyprctl dispatch workspace 3
```

If you're on a different window manager, you'll need to modify `claude-worktree.sh` lines 100-109.

## Advanced: Custom Task Templates

You can modify the wrapper to create custom `.claude-task.md` templates based on task type.

For example, for bug fixes:

```markdown
# Task

Fix the audio clipping issue in recording.rs

# Context

This is a bug fix. Make sure to:
- [ ] Write a test that reproduces the bug
- [ ] Fix the bug
- [ ] Verify the test passes
- [ ] Update documentation if needed

# Instructions

Please complete the task above and commit with a descriptive message.
```

To implement this, modify the section in `claude-worktree.sh` starting at line 67.

## Integration with Other Tools

You might want to add companion tools:

### 1. List Worktrees
```json
{
  "name": "list_worktrees",
  "description": "Show all active Claude Code worktrees",
  "command": "git",
  "args": ["-C", "/home/seb/code/cloned/transcribe-rs-v2", "worktree", "list"]
}
```

### 2. Remove Worktree
```json
{
  "name": "remove_worktree",
  "description": "Clean up a finished worktree",
  "command": "git",
  "args": ["-C", "/home/seb/code/cloned/transcribe-rs-v2", "worktree", "remove", "{path}"],
  "parameters": {
    "path": {
      "type": "string",
      "description": "Path to the worktree to remove"
    }
  }
}
```

### 3. Switch to Worktree Window
```json
{
  "name": "focus_worktree",
  "description": "Switch focus to an active Claude Code worktree window",
  "command": "hyprctl",
  "args": ["dispatch", "focuswindow", "claude-code"],
  "parameters": {}
}
```

## What Happens Behind the Scenes

```
User speaks: "create worktree for adding LED support"
    ↓
Dictation app transcribes: "create worktree for adding LED support"
    ↓
LLM processes text, detects "claude_worktree" tool should be called
    ↓
LLM generates parameters:
    {
      "branch_name": "add-led-support",
      "task_description": "Add LED support",
      "workspace": ""
    }
    ↓
Dictation app calls:
    /home/.../claude_worktree_wrapper.sh \
        /home/seb/code/cloned/transcribe-rs-v2 \
        "add-led-support" \
        "Add LED support" \
        --workspace ""
    ↓
Wrapper script:
    - Validates project path exists
    - Changes to project directory
    - Finds claude-worktree.sh
    - Calls it with processed arguments
    ↓
claude-worktree.sh:
    - Creates worktree directory
    - Creates git branch
    - Writes .claude-task.md
    - Opens new terminal window
    - Launches claude-code
    ↓
Claude Code:
    - Reads .claude-task.md
    - Starts implementing the feature
    - User continues working in main environment
```

## Success!

You can now continue dictating in your main window while Claude Code works on the feature in a separate worktree!
